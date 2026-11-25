#![deny(unsafe_code)]

//! InvMove for Sunrise: walk, jump, sneak and sprint while an inventory or other screen is open,
//! with per-screen choices and an optional hidden background tint.

mod config;
mod logic;

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use crate::config::{
    CONFIG_FILE_NAME, FileConfig, MOD_ID, MOVEMENT_KEYS, Options, register_mod_configs,
    register_settings, screen_background_key, screen_movement_key,
};
use crate::logic::{
    Class, SneakInputs, SneakState, classify, classify_background_name, is_pause_screen,
    is_selection_key, move_vector, sneak_when_allowed, sneak_when_disallowed, toggle_movement,
};
use mod_api::{
    ClientHost, ClientTickPreEvent, ConfigContext, ConfigKind, ConfigLoadingEvent, ConfigPath,
    ConfigReloadingEvent, Flow, Hooks, HostSide, KeyConflictContext, KeyRegistration,
    MenuBackgroundEvent, ModKeyMappingRef, MovementInputUpdateEvent, RegisterConfigsEvent,
    Registrar, RegistrationContext, ScreenCharTypedPreEvent, ScreenClosingEvent,
    ScreenInitPostEvent, ScreenKeyPressedPreEvent, ScreenMouseButtonPressedPreEvent,
    ScreenRegionKind, ScreenRenderPostEvent, ScreenTransparentBackgroundEvent, ScreenView,
    VanillaKeyMappingRef,
};

const KEY_TOGGLE_MOVE: &str = "toggle_move";
const UNBOUND: &str = "key.keyboard.unknown";

/// The screen open now, as far as the game tells.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Screen {
    id: String,
    menu: Option<String>,
    mod_screen: Option<String>,
}

impl Screen {
    fn of(view: &ScreenView<'_>) -> Self {
        Self {
            id: view.id.to_owned(),
            menu: view.menu.map(str::to_owned),
            mod_screen: view.mod_screen.map(str::to_owned),
        }
    }

    fn class(&self) -> Class {
        classify(&self.id, self.menu.as_deref(), self.mod_screen.as_deref())
    }

    /// What a text box typed into is remembered by.
    fn typing_key(&self) -> String {
        format!("{}|{}", self.id, self.menu.as_deref().unwrap_or(""))
    }
}

#[derive(Default)]
struct Runtime {
    /// The last screen the screen hooks reported (menu and mod owner are not otherwise readable).
    tracked: Option<Screen>,
    recipe_book_visible: bool,
    /// The screen a text box last took typed characters in; a mouse press clears it.
    typing_in: Option<String>,
    sneak: SneakState,
    sprint_latched: bool,
    sprint_prev_down: bool,
    toggle_prev_down: bool,
    file: FileConfig,
    file_dirty: bool,
    toggle_key: Option<ModKeyMappingRef>,
    mappings: HashMap<String, VanillaKeyMappingRef>,
    /// Extra mappings this mod holds down inside a screen.
    held_extras: Vec<VanillaKeyMappingRef>,
}

impl Runtime {
    fn mapping(&mut self, host: &dyn ClientHost, name: &str) -> Option<VanillaKeyMappingRef> {
        if let Some(handle) = self.mappings.get(name) {
            return Some(*handle);
        }
        let handle = host.vanilla_key_mapping(name).ok()?;
        self.mappings.insert(name.to_owned(), handle);
        Some(handle)
    }

    /// Whether the key bound to the vanilla mapping is physically down.
    fn pressed(&mut self, host: &dyn ClientHost, name: &str) -> bool {
        let Some(handle) = self.mapping(host, name) else {
            return false;
        };
        match handle.physical_down(host) {
            Ok(down) => down,
            Err(_) => {
                self.mappings.remove(name);
                false
            }
        }
    }

    /// Whether the player marked the movement key as one InvMove may read.
    fn key_allowed(host: &dyn ClientHost, mapping: &str) -> bool {
        MOVEMENT_KEYS
            .iter()
            .find(|(name, _)| *name == mapping)
            .and_then(|(_, key)| host.setting_bool(key))
            .unwrap_or(true)
    }

    fn toggle_key_down(&mut self, host: &dyn ClientHost) -> bool {
        if self.toggle_key.is_none() {
            self.toggle_key = host.mod_key_mapping(MOD_ID, KEY_TOGGLE_MOVE).ok();
        }
        let Some(key) = self.toggle_key else {
            return false;
        };
        match key.binding(host) {
            Ok((name, _)) if name != UNBOUND => host.is_key_down(name),
            Ok(_) => false,
            Err(_) => {
                self.toggle_key = None;
                false
            }
        }
    }

    fn current_screen(&self, host: &dyn ClientHost) -> Option<Screen> {
        if host.playing() {
            return None;
        }
        let observed = host.observed_screen_id().map(|screen| screen.id);
        Some(match (observed, &self.tracked) {
            (Some(id), Some(tracked)) if tracked.id == id => tracked.clone(),
            (Some(id), _) => Screen {
                mod_screen: (id == "mod")
                    .then(|| host.mod_screen().map(str::to_owned))
                    .flatten(),
                id,
                menu: None,
            },
            (None, _) => Screen {
                id: "unknown".to_owned(),
                ..Screen::default()
            },
        })
    }

    /// The choice for a screen InvMove was not told about, recorded on first sight.
    fn unrecognized(&mut self, movement: bool, key: &str, default: bool) -> bool {
        let map = if movement {
            &mut self.file.unrecognized_movement
        } else {
            &mut self.file.unrecognized_background
        };
        if let Some(value) = map.get(key) {
            return *value;
        }
        map.insert(key.to_owned(), default);
        self.file_dirty = true;
        default
    }

    /// `InvMove.allowMovementInScreen`.
    fn allow_movement(&mut self, host: &dyn ClientHost, opts: &Options, screen: &Screen) -> bool {
        if host.local_player().is_none() || !opts.enabled || !opts.movement || host.is_paused() {
            return false;
        }
        if !opts.recipe_book && self.recipe_book_visible {
            return false;
        }
        if opts.text_field_disables
            && opts.recipe_book
            && self.typing_in.as_deref() == Some(screen.typing_key().as_str())
        {
            return false;
        }
        match screen.class() {
            Class::Known(key) => host.setting_bool(&screen_movement_key(key)).unwrap_or(true),
            Class::Fixed | Class::BookEdit => false,
            Class::GameMode => true,
            Class::Unrecognized(key) => self.unrecognized(true, &key, opts.unrecognized_movement),
        }
    }

    /// `InvMove.shouldDisableScreenBackground`, for a screen already classified.
    fn hide_background(
        &mut self,
        host: &dyn ClientHost,
        opts: &Options,
        screen_id: &str,
        class: Class,
    ) -> bool {
        if host.local_player().is_none() || !opts.enabled || !opts.background {
            return false;
        }
        if is_pause_screen(screen_id) && !opts.hide_on_pause.allows_hiding(host.is_paused()) {
            return false;
        }
        match class {
            Class::Known(key) => host
                .setting_bool(&screen_background_key(key))
                .unwrap_or(true),
            Class::BookEdit => host
                .setting_bool(&screen_background_key("book"))
                .unwrap_or(true),
            Class::Fixed => false,
            Class::GameMode => {
                self.unrecognized(false, "game_mode_switcher", opts.unrecognized_background)
            }
            Class::Unrecognized(key) => {
                self.unrecognized(false, &key, opts.unrecognized_background)
            }
        }
    }

    fn take_file_write(&mut self) -> Option<String> {
        std::mem::take(&mut self.file_dirty).then(|| self.file.to_toml())
    }
}

fn write_file(host: &dyn ClientHost, text: Option<String>) {
    let Some(text) = text else {
        return;
    };
    if let Ok(path) = ConfigPath::new(CONFIG_FILE_NAME) {
        let _ = host.config_files().write(&path, text.as_bytes());
    }
}

#[derive(Default)]
struct InvMove {
    runtime: Mutex<Runtime>,
}

impl InvMove {
    fn lock(&self) -> MutexGuard<'_, Runtime> {
        self.runtime.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[mod_api::sunrise_mod]
impl Hooks for InvMove {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        register_settings(r);
        r.register_key_mapping(
            KeyRegistration::new(KEY_TOGGLE_MOVE, UNBOUND)
                .label("keybind.invmove.toggleMove")
                .category("key.category.invmove.main")
                .context(KeyConflictContext::Universal),
        );
    }

    fn register_configs(&self, _ctx: &dyn ConfigContext, event: &mut RegisterConfigsEvent<'_>) {
        register_mod_configs(event.configs);
    }

    fn config_loading(&self, _ctx: &dyn ConfigContext, event: &mut ConfigLoadingEvent<'_>) {
        if event.config.mod_id != MOD_ID || event.config.kind != ConfigKind::Client {
            return;
        }
        self.lock().file = FileConfig::parse(&event.config.text);
    }

    fn config_reloading(&self, _host: HostSide<'_>, event: &mut ConfigReloadingEvent<'_>) {
        if event.config.mod_id != MOD_ID || event.config.kind != ConfigKind::Client {
            return;
        }
        self.lock().file = FileConfig::parse(&event.config.text);
    }

    /// `InvMove.onInputUpdate`: after the game sampled the keys (all zero behind a screen), put
    /// back the ones InvMove allows.
    fn movement_input_update(
        &self,
        host: &dyn ClientHost,
        event: &mut MovementInputUpdateEvent<'_>,
    ) {
        if !host.on(MOD_ID) {
            return;
        }
        let Some(player) = host.local_player() else {
            return;
        };
        let opts = Options::read(host);
        let write;
        {
            let mut rt = self.lock();
            let input = &mut *event.input;
            let shift_phys =
                Runtime::key_allowed(host, "key.sneak") && rt.pressed(host, "key.sneak");
            let sprint_phys =
                Runtime::key_allowed(host, "key.sprint") && rt.pressed(host, "key.sprint");
            let toggle_down = rt.toggle_key_down(host);
            let toggle_pressed = toggle_down && !rt.toggle_prev_down;
            rt.toggle_prev_down = toggle_down;

            let Some(screen) = rt.current_screen(host) else {
                // Nothing open: the game's own keys stand; remember what screens maintain.
                rt.tracked = None;
                rt.typing_in = None;
                rt.recipe_book_visible = false;
                rt.sneak.was_sneaking = input.key_presses.sneak;
                rt.sneak.latched = input.key_presses.sneak;
                rt.sneak.prev_down = shift_phys;
                rt.sprint_latched = input.key_presses.sprint;
                rt.sprint_prev_down = sprint_phys;
                if toggle_pressed {
                    let (enabled, _) = toggle_movement(false, false, opts.movement, || false);
                    host.set_setting(MOD_ID, config::MOVEMENT_ENABLE, bool_text(enabled));
                }
                return;
            };

            let mut can_move = rt.allow_movement(host, &opts, &screen);
            if toggle_pressed {
                let (enabled, now) = toggle_movement(true, can_move, opts.movement, || {
                    let mut with_movement = opts;
                    with_movement.movement = true;
                    rt.allow_movement(host, &with_movement, &screen)
                });
                if enabled != opts.movement {
                    host.set_setting(MOD_ID, config::MOVEMENT_ENABLE, bool_text(enabled));
                }
                can_move = now;
            }

            let inputs = SneakInputs {
                toggle_crouch: host.input_options().toggle_crouch,
                passenger: player.entity().is_passenger(host).unwrap_or(false),
                flying: player.abilities(host).is_some_and(|a| a.flying),
                shift_down: shift_phys,
            };
            if can_move {
                let key = |rt: &mut Runtime, mapping: &str| {
                    Runtime::key_allowed(host, mapping) && rt.pressed(host, mapping)
                };
                let forward = key(&mut *rt, "key.forward");
                let backward = key(&mut *rt, "key.back");
                let left = key(&mut *rt, "key.left");
                let right = key(&mut *rt, "key.right");
                let jump = key(&mut *rt, "key.jump");
                let sneak = sneak_when_allowed(opts.sneak, inputs, &mut rt.sneak);
                let sprint = if host.input_options().toggle_sprint {
                    if sprint_phys && !rt.sprint_prev_down {
                        rt.sprint_latched = !rt.sprint_latched;
                    }
                    rt.sprint_latched
                } else {
                    sprint_phys
                };
                rt.sprint_prev_down = sprint_phys;
                input.move_vector = move_vector(forward, backward, left, right);
                input.key_presses.forward = forward;
                input.key_presses.backward = backward;
                input.key_presses.left = left;
                input.key_presses.right = right;
                input.key_presses.jump = jump;
                input.key_presses.sneak = sneak;
                input.key_presses.sprint = sprint;
            } else {
                rt.sprint_prev_down = sprint_phys;
                if opts.enabled
                    && let Some(sneak) = sneak_when_disallowed(opts.sneak, inputs, &mut rt.sneak)
                {
                    input.key_presses.sneak = sneak;
                }
            }
            write = rt.take_file_write();
        }
        write_file(host, write);
    }

    /// Extra keys the config file lists: their held state follows the physical key while a
    /// screen allows movement.
    fn client_tick_pre(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPreEvent) {
        if !host.on(MOD_ID) {
            return;
        }
        let opts = Options::read(host);
        let mut wanted: Vec<(VanillaKeyMappingRef, bool)> = Vec::new();
        let write;
        {
            let mut rt = self.lock();
            let host: &dyn ClientHost = &*host;
            let active = match rt.current_screen(host) {
                Some(screen) => rt.allow_movement(host, &opts, &screen),
                None => false,
            };
            let names = rt.file.extra_keys.clone();
            for name in names {
                let Some(handle) = rt.mapping(host, &name) else {
                    continue;
                };
                let down = active && handle.physical_down(host).unwrap_or(false);
                wanted.push((handle, down));
            }
            for held in std::mem::take(&mut rt.held_extras) {
                if !wanted.iter().any(|(handle, down)| *handle == held && *down) {
                    wanted.push((held, false));
                }
            }
            rt.held_extras = wanted
                .iter()
                .filter(|(_, down)| *down)
                .map(|(handle, _)| *handle)
                .collect();
            write = rt.take_file_write();
        }
        write_file(&*host, write);
        for (handle, down) in wanted {
            let _ = handle.set_down(host, down);
        }
    }

    fn screen_init_post(&self, host: &dyn ClientHost, event: &mut ScreenInitPostEvent<'_>) {
        if !host.on(MOD_ID) {
            return;
        }
        self.lock().tracked = Some(Screen::of(event.screen));
    }

    fn screen_render_post(&self, host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
        if !host.on(MOD_ID) {
            return;
        }
        let mut rt = self.lock();
        rt.tracked = Some(Screen::of(event.screen));
        rt.recipe_book_visible = event
            .screen
            .regions
            .iter()
            .any(|region| region.kind == ScreenRegionKind::RecipeBook);
    }

    fn screen_closing(&self, host: &dyn ClientHost, _event: &mut ScreenClosingEvent<'_>) {
        if !host.on(MOD_ID) {
            return;
        }
        let mut rt = self.lock();
        rt.tracked = None;
        rt.typing_in = None;
        rt.recipe_book_visible = false;
    }

    /// A text box took a typed character: it has the keyboard, so movement keys are text.
    fn screen_char_typed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenCharTypedPreEvent<'_>,
    ) -> Flow {
        if host.on(MOD_ID) && event.field.is_some() {
            self.lock().typing_in = Some(Screen::of(event.screen).typing_key());
        }
        Flow::Continue
    }

    /// A click may move the focus off the text box.
    fn screen_mouse_button_pressed_pre(
        &self,
        host: &dyn ClientHost,
        _event: &mut ScreenMouseButtonPressedPreEvent<'_>,
    ) -> Flow {
        if host.on(MOD_ID) {
            self.lock().typing_in = None;
        }
        Flow::Continue
    }

    /// `ButtonJumpKeyFixMixin`: a jump key bound to space or Enter must not also click the
    /// focused button while the player can jump in the screen.
    fn screen_key_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenKeyPressedPreEvent<'_>,
    ) -> Flow {
        if !host.on(MOD_ID) || !is_selection_key(event.key) {
            return Flow::Continue;
        }
        let opts = Options::read(host);
        if !opts.enabled || !opts.movement || !Runtime::key_allowed(host, "key.jump") {
            return Flow::Continue;
        }
        if event.screen.menu == Some("minecraft:merchant") {
            return Flow::Continue;
        }
        let cancel;
        let write;
        {
            let mut rt = self.lock();
            let bound = rt
                .mapping(host, "key.jump")
                .and_then(|jump| jump.binding(host).ok().map(|name| name == event.key))
                .unwrap_or(false);
            cancel = bound && rt.allow_movement(host, &opts, &Screen::of(event.screen));
            write = rt.take_file_write();
        }
        write_file(host, write);
        if cancel { Flow::Cancel } else { Flow::Continue }
    }

    /// `BackgroundMixin`: drop the dark tint behind screens that may be seen through.
    fn screen_transparent_background(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenTransparentBackgroundEvent<'_>,
    ) -> Flow {
        if !host.on(MOD_ID) {
            return Flow::Continue;
        }
        let opts = Options::read(host);
        let hide;
        let write;
        {
            let mut rt = self.lock();
            let (id, class) = match rt.current_screen(host) {
                Some(screen) if screen.id != "unknown" => (screen.id.clone(), screen.class()),
                _ => (
                    event.screen.to_owned(),
                    classify_background_name(event.screen),
                ),
            };
            hide = rt.hide_background(host, &opts, &id, class);
            write = rt.take_file_write();
        }
        write_file(host, write);
        if hide { Flow::Cancel } else { Flow::Continue }
    }

    /// The blur behind pause-style screens is their background too.
    fn menu_background(&self, host: &dyn ClientHost, event: &mut MenuBackgroundEvent<'_>) {
        if !host.on(MOD_ID) || event.radius.is_none() {
            return;
        }
        let opts = Options::read(host);
        let hide;
        let write;
        {
            let mut rt = self.lock();
            let Some(screen) = rt.current_screen(host) else {
                return;
            };
            if !is_pause_screen(&screen.id) {
                return;
            }
            hide = rt.hide_background(host, &opts, &screen.id, screen.class());
            write = rt.take_file_write();
        }
        write_file(host, write);
        if hide {
            *event.radius = None;
        }
    }
}

fn bool_text(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}
