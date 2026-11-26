#![deny(unsafe_code)]

//! Dynamic FPS for Sunrise: reduces frame rate and audio volume while the game window
//! is unfocused, minimized, idle, or on battery.

mod battery;
mod config;
mod hud;
mod screen;
mod state;

use crate::battery::{BatteryMonitor, BatteryNotification, BatterySnapshot};
use crate::config::{
    CONFIG_FILE_NAME, DynamicFpsConfig, MOD_ID, PowerState, SYNCING, config_spec,
    register_settings, sync_settings_to_host,
};
use crate::hud::{ensure_toast_icons_registered, render_hud, toast_icon_id};
use crate::state::{
    AtomicVolumeTable, ClickIgnoreTracker, HostEffects, IdleTracker, PowerStateInput,
    SmoothVolumeTracker, desired_effects, limit_for, resolve_power_state,
};
use mod_api::{
    ClientHost, ClientSetupContext, ClientSetupEvent, ClientStoppingEvent, ClientTickPreEvent,
    ConfigEvent, ConfigSnapshot, Flow, FramerateLimitEvent, Hooks, Host, HostSide, Identifier,
    InputKeyEvent, KeyConflictContext, KeyRegistration, ModKeyMappingRef, ModToast,
    MouseButtonPreEvent, MouseScrollingEvent, PlaySoundEvent, Registrar, RegistrationContext,
    RenderFramePreEvent, RenderGuiPostEvent, ScreenInitPostEvent, ScreenOpening,
    ScreenOpeningEvent, SettingChangedEvent, SoundSource, Text, ToastIcon,
};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, MutexGuard};

const KEY_TOGGLE_FORCED: &str = "toggle_forced";
const KEY_TOGGLE_DISABLED: &str = "toggle_disabled";

struct Runtime {
    config: DynamicFpsConfig,
    settings_synced: bool,
    state: PowerState,
    has_rendered_first_frame: bool,
    focused: bool,
    minimized: bool,
    hovered: bool,
    occluded: bool,
    applied: HostEffects,
    disabled_by_user: bool,
    forcing_low_fps: bool,
    idle: IdleTracker,
    click_ignore: ClickIgnoreTracker,
    smooth_volume: SmoothVolumeTracker,
    battery: BatteryMonitor,
    pending_battery_toast: Option<(BatteryNotification, u8)>,
    toast_icons_registered: bool,
    key_toggle_forced: Option<ModKeyMappingRef>,
    key_toggle_disabled: Option<ModKeyMappingRef>,
}

impl Runtime {
    fn new() -> Self {
        Self {
            config: DynamicFpsConfig::default(),
            settings_synced: false,
            state: PowerState::Focused,
            has_rendered_first_frame: true,
            focused: true,
            minimized: false,
            hovered: false,
            occluded: false,
            applied: HostEffects::neutral(),
            disabled_by_user: false,
            forcing_low_fps: false,
            idle: IdleTracker::default(),
            click_ignore: ClickIgnoreTracker::default(),
            smooth_volume: SmoothVolumeTracker::default(),
            battery: BatteryMonitor::new(),
            pending_battery_toast: None,
            toast_icons_registered: false,
            key_toggle_forced: None,
            key_toggle_disabled: None,
        }
    }

    fn update_state(&mut self, now_ms: u64) -> PowerState {
        let battery = self.battery.snapshot();
        let idle = self.idle.is_idle(now_ms, &self.config, battery);
        let next = resolve_power_state(PowerStateInput {
            mod_enabled: self.config.enabled,
            disabled_by_user: self.disabled_by_user,
            forcing_low_fps: self.forcing_low_fps,
            focused: self.focused,
            hovered: self.hovered,
            // A fully hidden window counts as minimized (`Invisible`).
            minimized: self.minimized || self.occluded,
            idle,
            battery,
            battery_enabled: self.config.battery.enabled,
            battery_switch_states: self.config.battery.switch_states,
        });
        if next != self.state {
            self.state = next;
            self.has_rendered_first_frame = false;
            self.smooth_volume.on_state_change(&self.config, next);
            VOLUME_TABLE.store(self.smooth_volume.snapshot());
        }
        next
    }

    fn observe_window(&mut self, host: &dyn ClientHost) {
        self.hovered = host.window_hovered();
        self.occluded = host.window_occluded();
    }

    fn poll_keybinds(&mut self, host: &mut dyn ClientHost, now_ms: u64) {
        if self.key_toggle_forced.is_none() {
            self.key_toggle_forced = host.mod_key_mapping(MOD_ID, KEY_TOGGLE_FORCED).ok();
        }
        if self.key_toggle_disabled.is_none() {
            self.key_toggle_disabled = host.mod_key_mapping(MOD_ID, KEY_TOGGLE_DISABLED).ok();
        }

        if let Some(key) = self.key_toggle_forced {
            while key.consume_click(host).unwrap_or(false) {
                self.forcing_low_fps = !self.forcing_low_fps;
                self.idle.on_activity(now_ms);
                self.update_state(now_ms);
            }
        }

        if let Some(key) = self.key_toggle_disabled {
            while key.consume_click(host).unwrap_or(false) {
                self.disabled_by_user = !self.disabled_by_user;
                self.idle.on_activity(now_ms);
                self.update_state(now_ms);
            }
        }
    }

    fn prepare_battery_toast(&mut self, host: &mut dyn ClientHost) -> Option<ModToast> {
        if let Some(notif) = self.battery.take_pending_notification() {
            self.pending_battery_toast = Some(notif);
        }

        if !self.config.state(self.state).show_toasts {
            return None;
        }
        let (kind, _) = self.pending_battery_toast.take()?;

        if !self.toast_icons_registered {
            self.toast_icons_registered = ensure_toast_icons_registered(host);
        }

        // Refresh charge percentage at display time, matching BatteryToast.onFirstRender.
        let current_charge = self.battery.snapshot().charge;
        let title_key = match kind {
            BatteryNotification::Charging => "toast.dynamicfps.battery_charging",
            BatteryNotification::Discharging => "toast.dynamicfps.battery_draining",
            BatteryNotification::Critical => "toast.dynamicfps.battery_critical",
        };
        let mut toast = ModToast::new(Text::translate(title_key, vec![]));
        toast.description = Some(Text::translate(
            "toast.dynamicfps.battery_charge",
            vec![Text::literal(current_charge.to_string())],
        ));
        if self.toast_icons_registered {
            toast.icon = Some(ToastIcon::Sprite(toast_icon_id(kind)));
        }
        if let Ok(token) = Identifier::from_parts(MOD_ID, "battery") {
            toast.kind = Some(token);
        }
        Some(toast)
    }
}

static VOLUME_TABLE: AtomicVolumeTable = AtomicVolumeTable::new();

struct DynamicFps {
    runtime: Arc<Mutex<Runtime>>,
}

impl Default for DynamicFps {
    fn default() -> Self {
        Self {
            runtime: Arc::new(Mutex::new(Runtime::new())),
        }
    }
}

impl DynamicFps {
    fn lock(&self) -> MutexGuard<'_, Runtime> {
        lock_runtime(&self.runtime)
    }
}

fn lock_runtime(runtime: &Mutex<Runtime>) -> MutexGuard<'_, Runtime> {
    runtime.lock().unwrap_or_else(|e| e.into_inner())
}

/// Save `config/dynamicfps.toml` when its text differs from `text`.
fn persist_config_file(host: &dyn Host, text: &str) {
    let path = host.config_dir().join(CONFIG_FILE_NAME);
    if std::fs::read_to_string(&path).is_ok_and(|current| current == text) {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, text);
}

/// Take the file's text as the live config, as the loader delivers it at load and on reload.
fn adopt_config_text(runtime: &Mutex<Runtime>, text: &str) -> DynamicFpsConfig {
    let parsed = DynamicFpsConfig::parse_toml(text);
    let mut guard = lock_runtime(runtime);
    let rt = &mut *guard;
    rt.config = parsed;
    rt.battery.configure(rt.config.battery);
    rt.smooth_volume.on_state_change(&rt.config, rt.state);
    VOLUME_TABLE.store(rt.smooth_volume.snapshot());
    rt.config.clone()
}

/// Bring the host's session options, toast hold, sound scales and F3 text in line with the
/// current power state, calling only what changed since the last pass.
fn apply_host_effects(runtime: &Mutex<Runtime>, host: &mut dyn ClientHost) {
    let (desired, previous) = {
        let rt = lock_runtime(runtime);
        let desired = desired_effects(rt.state, &rt.config, rt.smooth_volume.snapshot());
        (desired, rt.applied.clone())
    };
    if desired == previous {
        return;
    }
    let mut done = desired.clone();
    if desired.vsync != previous.vsync {
        host.set_session_vsync(desired.vsync);
    }
    if desired.clouds != previous.clouds {
        host.set_session_clouds(desired.clouds);
    }
    if desired.view_bobbing != previous.view_bobbing {
        host.set_session_view_bobbing(desired.view_bobbing);
    }
    if desired.toasts_held != previous.toasts_held && !host.set_toasts_held(desired.toasts_held) {
        done.toasts_held = previous.toasts_held;
    }
    for source in SoundSource::ALL {
        let idx = source.index();
        if desired.sound_scales[idx] != previous.sound_scales[idx] {
            host.set_sound_source_scale(source, desired.sound_scales[idx]);
        }
    }
    if desired.sounds_paused != previous.sounds_paused {
        host.set_sounds_paused(desired.sounds_paused);
    }
    if desired.debug_text != previous.debug_text
        && !host.set_debug_fps_text(desired.debug_text.as_deref())
    {
        done.debug_text = previous.debug_text;
    }
    lock_runtime(runtime).applied = done;
}

#[mod_api::sunrise_mod]
impl Hooks for DynamicFps {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        register_settings(r);

        let runtime = Arc::clone(&self.runtime);
        let listener =
            move |event: ConfigEvent<'_>, snapshot: &ConfigSnapshot| -> Result<(), String> {
                match event {
                    ConfigEvent::Loading(_) => {
                        adopt_config_text(&runtime, &snapshot.text);
                    }
                    ConfigEvent::Reloading(side) => {
                        let config = adopt_config_text(&runtime, &snapshot.text);
                        lock_runtime(&runtime).settings_synced = true;
                        if let HostSide::Client(client) = side {
                            sync_settings_to_host(&config, client);
                            let now_ms = client.millis();
                            lock_runtime(&runtime).update_state(now_ms);
                        }
                    }
                    _ => {}
                }
                Ok(())
            };
        // A refused registration leaves the defaults and the settings store in charge.
        let _ = r.register_config(config_spec(), Some(Arc::new(listener)));
        // The Mods screen's Config button opens Dynamic FPS's own paged screen.
        let _ = r.register_config_screen(Arc::new(|_host: &dyn ClientHost| true));

        r.register_key_mapping(
            KeyRegistration::new(KEY_TOGGLE_FORCED, "key.keyboard.unknown")
                .label("key.dynamicfps.toggle_forced")
                .category("key.categories.misc")
                .context(KeyConflictContext::Universal),
        );
        r.register_key_mapping(
            KeyRegistration::new(KEY_TOGGLE_DISABLED, "key.keyboard.unknown")
                .label("key.dynamicfps.toggle_disabled")
                .category("key.categories.misc")
                .context(KeyConflictContext::Universal),
        );
    }

    fn setting_changed(&self, host: &dyn Host, event: &mut SettingChangedEvent<'_>) {
        if event.mod_id != MOD_ID || SYNCING.load(Ordering::SeqCst) {
            return;
        }
        let text = {
            let mut guard = self.lock();
            let rt = &mut *guard;
            if !rt.config.apply_single_setting(event.key, &event.value) {
                return;
            }
            rt.battery.configure(rt.config.battery);
            rt.config.to_toml()
        };
        persist_config_file(host, &text);
    }

    fn screen_init_post(&self, _host: &dyn ClientHost, event: &mut ScreenInitPostEvent<'_>) {
        if event.screen.mod_screen == Some(MOD_ID) {
            screen::mount(event);
        }
    }

    fn client_setup(&self, _ctx: &mut dyn ClientSetupContext, _event: &mut ClientSetupEvent) {}

    fn client_stopping(&self, _host: &mut dyn ClientHost, _event: &mut ClientStoppingEvent) {
        self.lock().battery.stop();
    }

    fn framerate_limit(&self, host: &dyn ClientHost, event: &mut FramerateLimitEvent) {
        if !host.on(MOD_ID) {
            return;
        }
        let now_ms = host.millis();
        let mut rt = self.lock();
        rt.idle.ensure_initialized(now_ms);

        if rt.click_ignore.observe_focus(event.focused, now_ms) {
            rt.idle.on_activity(now_ms);
        }
        rt.focused = event.focused;
        rt.minimized = event.minimized;
        rt.observe_window(host);

        let state = rt.update_state(now_ms);

        event.limit = limit_for(event.limit, state, &rt.config, rt.has_rendered_first_frame);
    }

    fn render_frame_pre(&self, host: &mut dyn ClientHost, _event: &mut RenderFramePreEvent) {
        if !host.on(MOD_ID) {
            return;
        }
        let now_ms = host.millis();
        let focused = host.window_focused();
        let mut rt = self.lock();
        if rt.click_ignore.observe_focus(focused, now_ms) {
            rt.idle.on_activity(now_ms);
        }
        rt.focused = focused;
        rt.observe_window(&*host);
        rt.has_rendered_first_frame = true;
        drop(rt);
        apply_host_effects(&self.runtime, host);
    }

    fn client_tick_pre(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPreEvent) {
        if !host.on(MOD_ID) {
            return;
        }
        let now_ms = host.millis();
        let focused = host.window_focused();
        let cursor = host.cursor();
        let camera_pose = host.camera().map(|c| (c.position, c.yaw, c.pitch));

        // `config/dynamicfps.toml` is the single source of truth; mirror it into the settings
        // store once, without holding the lock (each write notifies `setting_changed`).
        let initial = {
            let mut rt = self.lock();
            if rt.settings_synced {
                None
            } else {
                rt.settings_synced = true;
                Some(rt.config.clone())
            }
        };
        if let Some(config) = initial {
            sync_settings_to_host(&config, host);
        }

        let mut guard = self.lock();
        let rt = &mut *guard;

        if rt.click_ignore.observe_focus(focused, now_ms) {
            rt.idle.on_activity(now_ms);
        }
        rt.focused = focused;
        rt.observe_window(&*host);
        rt.idle.observe_motion(now_ms, cursor, camera_pose);
        rt.poll_keybinds(host, now_ms);

        let state = rt.update_state(now_ms);
        rt.smooth_volume.tick(&rt.config, state);
        VOLUME_TABLE.store(rt.smooth_volume.snapshot());

        let battery_toast = rt.prepare_battery_toast(host);
        drop(guard);

        apply_host_effects(&self.runtime, host);
        if let Some(toast) = battery_toast {
            let _ = host.add_toast(toast);
        }
    }

    fn play_sound(&self, _host: &mut dyn ClientHost, event: &mut PlaySoundEvent<'_>) {
        // Live volume is the host's source scale (`apply_host_effects`); only a sound that
        // would start at 0% is dropped.
        let Some(sound) = event.sound.as_ref() else {
            return;
        };
        if event.starting && VOLUME_TABLE.effective_factor(sound.source) <= 0.0 {
            *event.sound = None;
        }
    }

    fn mouse_button_pre(
        &self,
        host: &mut dyn ClientHost,
        _event: &mut MouseButtonPreEvent,
    ) -> Flow {
        if !host.on(MOD_ID) {
            return Flow::Continue;
        }
        let now_ms = host.millis();
        let focused = host.window_focused();
        let screen_open = host.observed_screen_id().is_some();

        let mut rt = self.lock();
        rt.click_ignore.observe_focus(focused, now_ms);
        rt.focused = focused;
        rt.idle.on_activity(now_ms);

        if rt
            .click_ignore
            .should_ignore_click(now_ms, rt.config.ignore_initial_click, screen_open)
        {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    fn mouse_scrolling(&self, host: &mut dyn ClientHost, _event: &mut MouseScrollingEvent) -> Flow {
        if host.on(MOD_ID) {
            let now_ms = host.millis();
            self.lock().idle.on_activity(now_ms);
        }
        Flow::Continue
    }

    fn input_key(&self, host: &mut dyn ClientHost, _event: &mut InputKeyEvent<'_>) {
        if host.on(MOD_ID) {
            let now_ms = host.millis();
            self.lock().idle.on_activity(now_ms);
        }
    }

    fn screen_opening(
        &self,
        host: &dyn ClientHost,
        _event: &mut ScreenOpeningEvent<'_>,
    ) -> Option<ScreenOpening> {
        if host.on(MOD_ID) {
            let now_ms = host.millis();
            self.lock().idle.on_activity(now_ms);
        }
        None
    }

    fn render_gui_post(&self, host: &dyn ClientHost, event: &mut RenderGuiPostEvent<'_>) {
        if !host.on(MOD_ID) {
            return;
        }
        let (config, battery, disabled_by_user, forcing_low_fps) = {
            let rt = self.lock();
            (
                rt.config.clone(),
                rt.battery.snapshot(),
                rt.disabled_by_user,
                rt.forcing_low_fps,
            )
        };
        let battery: BatterySnapshot = battery;
        render_hud(
            host,
            event.draw,
            &config,
            battery,
            disabled_by_user,
            forcing_low_fps,
        );
    }
}
