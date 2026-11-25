#![deny(unsafe_code)]

//! Better Ping Display for Sunrise: shows each player's numeric ping (with latency-based colour)
//! in the Tab player list.

mod color;
mod config;
mod tab_list;

use crate::config::{
    BetterPingDisplayConfig, CONFIG_FILE_NAME, MOD_ID, SettingKeys, config_spec, register_settings,
};
use crate::tab_list::{
    ObjectiveSnap, TabList, TabListState, TextMetrics, build_narration, is_visible, paint,
};
use mod_api::{
    ClientHost, ClientTickPostEvent, ConfigEvent, ConfigSnapshot, Hooks, Host, HostSide, HudMode,
    Narration, RegisterGuiLayersEvent, Registrar, RegistrationContext, SettingChangedEvent, Text,
    VanillaKeyMappingRef,
};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, OnceLock};

/// The handles of this mod's registered settings, kept at registration.
static SETTING_KEYS: OnceLock<SettingKeys> = OnceLock::new();

const PLAYERLIST_KEY: &str = "key.playerlist";

struct Runtime {
    config: BetterPingDisplayConfig,
    settings_synced: bool,
    playerlist_key: Option<VanillaKeyMappingRef>,
    tab_state: TabListState,
    pending_narration: Option<Text>,
}

impl Runtime {
    fn new() -> Self {
        Self {
            config: BetterPingDisplayConfig::default(),
            settings_synced: false,
            playerlist_key: None,
            tab_state: TabListState::default(),
            pending_narration: None,
        }
    }

    fn persist_config_file(&self, host: &dyn Host) {
        let path = host.config_dir().join(CONFIG_FILE_NAME);
        let _ = std::fs::write(path, self.config.to_toml());
    }

    fn playerlist_key_down(&mut self, host: &dyn ClientHost) -> bool {
        if let Some(mapping) = self.playerlist_key {
            match mapping.is_down(host) {
                Ok(down) => return down,
                Err(_) => self.playerlist_key = None,
            }
        }
        let Ok(mapping) = host.vanilla_key_mapping(PLAYERLIST_KEY) else {
            return false;
        };
        self.playerlist_key = Some(mapping);
        mapping.is_down(host).unwrap_or(false)
    }
}

static RUNTIME: LazyLock<Mutex<Runtime>> = LazyLock::new(|| Mutex::new(Runtime::new()));

fn lock_runtime() -> MutexGuard<'static, Runtime> {
    RUNTIME.lock().unwrap_or_else(|e| e.into_inner())
}

struct HostMetrics<'a>(&'a dyn ClientHost);

impl TextMetrics for HostMetrics<'_> {
    fn text_width(&self, text: &Text) -> i32 {
        self.0.text_width(text)
    }

    fn str_width(&self, text: &str) -> i32 {
        self.0.text_width(&Text::literal(text))
    }

    fn text_split(&self, text: &Text, max_width: i32) -> Vec<Text> {
        self.0.text_split(text, max_width)
    }
}

#[derive(Default)]
struct BetterPingDisplay;

#[mod_api::sunrise_mod]
impl Hooks for BetterPingDisplay {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        if let Some(keys) = register_settings(r) {
            let _ = SETTING_KEYS.set(keys);
        }
        let listener = |event: ConfigEvent<'_>, snapshot: &ConfigSnapshot| -> Result<(), String> {
            let parsed = BetterPingDisplayConfig::parse_toml(&snapshot.text);
            match event {
                ConfigEvent::Loading(_) => {
                    lock_runtime().config = parsed;
                }
                ConfigEvent::Reloading(host) => {
                    {
                        let mut rt = lock_runtime();
                        rt.config = parsed.clone();
                        rt.settings_synced = true;
                    }
                    // Written with the lock released: a changed value notifies `setting_changed`.
                    if let HostSide::Client(client) = host
                        && let Some(keys) = SETTING_KEYS.get()
                    {
                        parsed.write_to_host_settings(client, *keys);
                    }
                }
                _ => {}
            }
            Ok(())
        };
        // A duplicate filename is refused by the game; the first registration stays active.
        let _ = r.register_config(config_spec(), Some(Arc::new(listener)));
    }

    fn setting_changed(&self, host: &dyn Host, event: &mut SettingChangedEvent<'_>) {
        if event.mod_id != MOD_ID {
            return;
        }
        let mut rt = lock_runtime();
        if rt.config.apply_single_setting(event.key, event.value) {
            rt.persist_config_file(host);
        }
    }

    fn register_gui_layers(
        &self,
        _ctx: &RegistrationContext,
        event: &mut RegisterGuiLayersEvent<'_>,
    ) {
        event.layers.wrap_layer(
            "minecraft:tab_list",
            Box::new(|ctx| {
                if !ctx.host().on(MOD_ID) {
                    ctx.draw_previous();
                    return;
                }

                let mut screen_width = 0;
                ctx.with_draw(&mut |draw| {
                    screen_width = draw.size()[0] as i32;
                });

                let draws = {
                    let host = ctx.host();
                    let mut rt = lock_runtime();

                    let Some(tab_list) = host
                        .client_session()
                        .client_tab_list()
                        .ok()
                        .flatten()
                        .map(TabList::copy_from)
                    else {
                        rt.tab_state.set_visible(false);
                        return;
                    };

                    let objective = host
                        .client_scoreboard()
                        .ok()
                        .flatten()
                        .and_then(ObjectiveSnap::copy_list_slot);

                    let local_server = host
                        .client_session()
                        .client_server()
                        .is_some_and(|server| server.integrated_local);

                    let shown = is_visible(
                        host.hud_mode() == HudMode::AllVisible,
                        host.playing(),
                        rt.playerlist_key_down(host),
                        local_server,
                        tab_list.entries.len(),
                        objective.is_some(),
                    );

                    if rt.tab_state.set_visible(shown) {
                        rt.pending_narration = Some(build_narration(&tab_list.entries));
                    }

                    if !shown {
                        return;
                    }

                    let gui_ticks = i64::from(host.gui_ticks());
                    let config = rt.config.clone();
                    let metrics = HostMetrics(host);
                    rt.tab_state.build_draws(
                        &tab_list,
                        objective.as_ref(),
                        screen_width,
                        gui_ticks,
                        &config,
                        &metrics,
                    )
                };

                if !draws.is_empty() {
                    ctx.with_draw(&mut |draw| {
                        let _ = draw.draw_gui(&mut |canvas| paint(canvas, &draws));
                    });
                }
            }),
        );
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPostEvent) {
        if !host.on(MOD_ID) {
            return;
        }
        let unsynced = {
            let mut rt = lock_runtime();
            let first = !rt.settings_synced;
            rt.settings_synced = true;
            first.then(|| rt.config.clone())
        };
        if let (Some(config), Some(keys)) = (unsynced, SETTING_KEYS.get()) {
            // Written with the lock released: a changed value notifies `setting_changed`.
            config.write_to_host_settings(host, *keys);
        }
        let narration = lock_runtime().pending_narration.take();
        if let Some(text) = narration {
            let _ = host.narrate(&text, Narration::Now);
        }
    }
}
