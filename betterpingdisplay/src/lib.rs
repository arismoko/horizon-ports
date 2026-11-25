#![deny(unsafe_code)]

//! Better Ping Display for Sunrise: shows each player's numeric ping (with latency-based colour)
//! in the Tab player list.

mod color;
mod config;
mod tab_list;

use crate::config::{
    BetterPingDisplayConfig, CONFIG_FILE_NAME, MOD_ID, register_mod_configs, register_settings,
};
use crate::tab_list::{TabListState, TextMetrics, build_narration, is_visible};
use mod_api::{
    ClientHost, ClientTickPostEvent, ConfigContext, ConfigKind, ConfigLoadingEvent, ConfigPath,
    ConfigReloadingEvent, DisplaySlot, Hooks, HostSide, HudMode, Narration, RegisterConfigsEvent,
    RegisterGuiLayersEvent, Registrar, RegistrationContext, SettingChangedEvent, Text,
    VanillaKeyMappingRef,
};
use std::sync::{LazyLock, Mutex, MutexGuard};

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

    fn sync_initial_settings(&mut self, host: &dyn ClientHost) {
        if self.settings_synced {
            return;
        }
        self.settings_synced = true;
        self.config.write_to_host_settings(host);
    }

    fn persist_config_file(&self, host: &dyn ClientHost) {
        if let Ok(path) = ConfigPath::new(CONFIG_FILE_NAME) {
            let _ = host
                .config_files()
                .write(&path, self.config.to_toml().as_bytes());
        }
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
        register_settings(r);
    }

    fn register_configs(&self, _ctx: &dyn ConfigContext, event: &mut RegisterConfigsEvent<'_>) {
        register_mod_configs(event.configs);
    }

    fn config_loading(&self, _ctx: &dyn ConfigContext, event: &mut ConfigLoadingEvent<'_>) {
        if event.config.mod_id != MOD_ID || event.config.kind != ConfigKind::Client {
            return;
        }
        let parsed = BetterPingDisplayConfig::parse_toml(&event.config.text);
        let mut rt = lock_runtime();
        rt.config = parsed;
    }

    fn config_reloading(&self, host: HostSide<'_>, event: &mut ConfigReloadingEvent<'_>) {
        if event.config.mod_id != MOD_ID || event.config.kind != ConfigKind::Client {
            return;
        }
        let parsed = BetterPingDisplayConfig::parse_toml(&event.config.text);
        let mut rt = lock_runtime();
        rt.config = parsed;
        rt.settings_synced = true;
        if let HostSide::Client(client) = host {
            rt.config.write_to_host_settings(client);
        }
    }

    fn setting_changed(&self, host: &mut dyn ClientHost, event: &mut SettingChangedEvent<'_>) {
        if event.mod_id != MOD_ID {
            return;
        }
        let mut rt = lock_runtime();
        let changed = rt.config.apply_single_setting(host, event.key);
        rt.sync_initial_settings(host);
        if changed {
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
                    rt.sync_initial_settings(host);

                    let Some(tab_list) = host.client_session().client_tab_list() else {
                        rt.tab_state.set_visible(false);
                        return;
                    };

                    let scoreboard = host.client_scoreboard().ok().flatten();
                    let objective = scoreboard
                        .as_ref()
                        .and_then(|board| board.in_slot(DisplaySlot::List));

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
                        objective,
                        screen_width,
                        gui_ticks,
                        &config,
                        &metrics,
                    )
                };

                if !draws.is_empty() {
                    ctx.with_draw(&mut |draw| {
                        let _ = draw.gui_paint(&draws);
                    });
                }
            }),
        );
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPostEvent) {
        if !host.on(MOD_ID) {
            return;
        }
        let narration = lock_runtime().pending_narration.take();
        if let Some(text) = narration {
            let _ = host.narrate(&text, Narration::Now);
        }
    }
}
