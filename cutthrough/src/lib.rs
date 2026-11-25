#![deny(unsafe_code)]

//! Cut Through for Sunrise: the crosshair picks a mob through blocks that have no
//! collision shape (tall grass, flowers) instead of stopping at the plant's outline.

mod config;
mod ray;

use crate::config::{CONFIG_FILE_NAME, CutThroughConfig, MOD_ID, config_spec, register_settings};
use mod_api::{
    ClientHost, ClientPickEvent, ClientTickPostEvent, ConfigEvent, ConfigListener, ConfigSnapshot,
    CrosshairTarget, Flow, Hooks, Host, HostSide, InteractionAttempt, InteractionKey,
    InteractionKeyMappingTriggeredEvent, Registrar, RegistrationContext, SettingChangedEvent,
    SettingKey,
};
use std::ops::ControlFlow;
use std::sync::{Arc, Mutex, MutexGuard};

struct Runtime {
    config: CutThroughConfig,
    setting: Option<SettingKey>,
    settings_synced: bool,
    client_tick: u64,
    last_entity_attack: Option<u64>,
}

impl Runtime {
    fn new() -> Self {
        Self {
            config: CutThroughConfig::default(),
            setting: None,
            settings_synced: false,
            client_tick: 0,
            last_entity_attack: None,
        }
    }
}

/// Writes the config file text into `config/`; a failed write only loses persistence.
fn persist_config_file(host: &dyn Host, text: &str) {
    let path = host.config_dir().join(CONFIG_FILE_NAME);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, text);
}

struct CutThrough {
    runtime: Arc<Mutex<Runtime>>,
}

impl Default for CutThrough {
    fn default() -> Self {
        Self {
            runtime: Arc::new(Mutex::new(Runtime::new())),
        }
    }
}

fn lock(runtime: &Mutex<Runtime>) -> MutexGuard<'_, Runtime> {
    runtime.lock().unwrap_or_else(|e| e.into_inner())
}

impl CutThrough {
    fn lock(&self) -> MutexGuard<'_, Runtime> {
        lock(&self.runtime)
    }
}

#[mod_api::sunrise_mod]
impl Hooks for CutThrough {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        self.lock().setting = register_settings(r);
        let runtime = Arc::clone(&self.runtime);
        let listener = move |event: ConfigEvent<'_>, snapshot: &ConfigSnapshot| {
            match event {
                ConfigEvent::Loading(_) => {
                    lock(&runtime).config = CutThroughConfig::parse_toml(&snapshot.text);
                }
                ConfigEvent::Reloading(host) => {
                    let (config, setting) = {
                        let mut rt = lock(&runtime);
                        rt.config = CutThroughConfig::parse_toml(&snapshot.text);
                        rt.settings_synced = true;
                        (rt.config.clone(), rt.setting)
                    };
                    // The write notifies `setting_changed`, so the lock is not held here.
                    if let (HostSide::Client(client), Some(key)) = (host, setting) {
                        config.write_to_host_settings(client, key);
                    }
                }
                _ => {}
            }
            Ok(())
        };
        let listener: Arc<ConfigListener> = Arc::new(listener);
        if let Err(err) = r.register_config(config_spec(), Some(listener)) {
            eprintln!("cutthrough: config registration failed: {err}");
        }
    }

    fn setting_changed(&self, host: &dyn Host, event: &mut SettingChangedEvent<'_>) {
        let text = {
            let mut rt = self.lock();
            if !rt.config.apply_single_setting(event.key, event.value) {
                return;
            }
            rt.config.to_toml()
        };
        persist_config_file(host, &text);
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPostEvent) {
        if !host.on(MOD_ID) {
            return;
        }
        let pending = {
            let mut rt = self.lock();
            rt.client_tick = rt.client_tick.saturating_add(1);
            if rt.settings_synced {
                None
            } else {
                rt.settings_synced = true;
                rt.setting.map(|key| (rt.config.clone(), key))
            }
        };
        // `config/cutthrough-client.toml` is the source of truth; mirror it into the
        // Options > Mods page. The write notifies `setting_changed`, so no lock is held.
        if let Some((config, key)) = pending {
            config.write_to_host_settings(host, key);
        }
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
        let read: &dyn Host = &*host;
        let Some(level) = host
            .local_player()
            .and_then(|player| player.entity().level(read))
        else {
            return;
        };
        let collider = ray::collider_distance(origin, point, |cell| {
            let mut boxes = Vec::new();
            let _ = level.visit_block_collision(read, cell, &mut |bounds| {
                boxes.push(bounds);
                ControlFlow::Continue(())
            });
            boxes
        });
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
