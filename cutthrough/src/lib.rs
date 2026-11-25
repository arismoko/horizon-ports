#![deny(unsafe_code)]

//! Cut Through for Sunrise: the crosshair picks a mob through blocks that have no
//! collision shape (tall grass, flowers) instead of stopping at the plant's outline.

mod config;
mod ray;

use crate::config::{
    CONFIG_FILE_NAME, CutThroughConfig, MOD_ID, register_mod_configs, register_settings,
};
use mod_api::{
    ClientHost, ClientPickEvent, ClientTickPostEvent, ConfigContext, ConfigKind,
    ConfigLoadingEvent, ConfigPath, ConfigReloadingEvent, CrosshairTarget, Flow, Hooks, HostSide,
    InteractionAttempt, InteractionKey, InteractionKeyMappingTriggeredEvent, RegisterConfigsEvent,
    Registrar, RegistrationContext, SettingChangedEvent,
};
use std::sync::{Mutex, MutexGuard};

struct Runtime {
    config: CutThroughConfig,
    settings_synced: bool,
    client_tick: u64,
    last_entity_attack: Option<u64>,
}

impl Runtime {
    fn new() -> Self {
        Self {
            config: CutThroughConfig::default(),
            settings_synced: false,
            client_tick: 0,
            last_entity_attack: None,
        }
    }

    fn sync_initial_settings(&mut self, host: &dyn ClientHost) {
        if self.settings_synced {
            return;
        }
        self.settings_synced = true;
        // `config/cutthrough-client.toml` is the source of truth; mirror it into the
        // Options > Mods page.
        self.config.write_to_host_settings(host);
    }

    fn persist_config_file(&self, host: &dyn ClientHost) {
        if let Ok(path) = ConfigPath::new(CONFIG_FILE_NAME) {
            let _ = host
                .config_files()
                .write(&path, self.config.to_toml().as_bytes());
        }
    }
}

struct CutThrough {
    runtime: Mutex<Runtime>,
}

impl Default for CutThrough {
    fn default() -> Self {
        Self {
            runtime: Mutex::new(Runtime::new()),
        }
    }
}

impl CutThrough {
    fn lock(&self) -> MutexGuard<'_, Runtime> {
        self.runtime.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[mod_api::sunrise_mod]
impl Hooks for CutThrough {
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
        self.lock().config = CutThroughConfig::parse_toml(&event.config.text);
    }

    fn config_reloading(&self, host: HostSide<'_>, event: &mut ConfigReloadingEvent<'_>) {
        if event.config.mod_id != MOD_ID || event.config.kind != ConfigKind::Client {
            return;
        }
        let mut rt = self.lock();
        rt.config = CutThroughConfig::parse_toml(&event.config.text);
        rt.settings_synced = true;
        if let HostSide::Client(client) = host {
            rt.config.write_to_host_settings(client);
        }
    }

    fn setting_changed(&self, host: &mut dyn ClientHost, event: &mut SettingChangedEvent<'_>) {
        if event.mod_id != MOD_ID {
            return;
        }
        let mut rt = self.lock();
        if rt.config.apply_single_setting(host, event.key) {
            rt.persist_config_file(host);
        }
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPostEvent) {
        if !host.on(MOD_ID) {
            return;
        }
        let mut rt = self.lock();
        rt.sync_initial_settings(host);
        rt.client_tick = rt.client_tick.saturating_add(1);
    }

    /// Vanilla stops the pick ray at a block's outline, so a mob behind tall grass cannot
    /// be targeted. When the nearest entity is reached before any collision shape, it wins
    /// over a block the vanilla pick found nearer (`ClipContext.Block.COLLIDER`).
    fn client_pick(&self, host: &mut dyn ClientHost, event: &mut ClientPickEvent<'_>) {
        if !host.on(MOD_ID) {
            return;
        }
        if matches!(event.original, Some(CrosshairTarget::Entity { .. })) {
            return;
        }
        let Some(CrosshairTarget::Entity { point, .. }) = event.entity_candidate else {
            return;
        };
        let origin = event.origin;
        let distance = ((point[0] - origin[0]).powi(2)
            + (point[1] - origin[1]).powi(2)
            + (point[2] - origin[2]).powi(2))
        .sqrt();
        let collider = ray::collider_distance(origin, point, |cell| host.block_collision(cell));
        if ray::entity_wins(distance, collider) {
            *event.result = event.entity_candidate;
        }
    }

    /// Hitting a mob through a plant must not mine the plant a moment later: vanilla's
    /// mining delay is not reset by an entity attack, so the held attack would break the
    /// block as soon as the mob is gone.
    fn interaction_key_mapping_triggered(
        &self,
        host: &mut dyn ClientHost,
        event: &mut InteractionKeyMappingTriggeredEvent,
    ) -> Flow {
        if !host.on(MOD_ID) || event.key != InteractionKey::Attack {
            return Flow::Continue;
        }
        let target = host.crosshair_target();
        let mut rt = self.lock();
        if !rt.config.target_alive_only {
            return Flow::Continue;
        }
        match target {
            Some(CrosshairTarget::Entity { .. }) => {
                rt.last_entity_attack = Some(rt.client_tick);
                Flow::Continue
            }
            Some(CrosshairTarget::Block { .. })
                if event.attempt == InteractionAttempt::Held
                    && ray::mining_suspended(rt.client_tick, rt.last_entity_attack) =>
            {
                event.swing_hand = false;
                Flow::Cancel
            }
            _ => Flow::Continue,
        }
    }
}
