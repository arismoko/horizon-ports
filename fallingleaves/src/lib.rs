#![deny(unsafe_code)]

//! Falling Leaves for Sunrise: adds wind-driven falling leaf and snowflake particles
//! from leaf blocks, with smooth-noise wind, water floating, lava burning, ground
//! settling, hit-block leaf drops, and decaying leaf bursts.

mod config;
mod leaf_util;
mod particles;
mod wind;

use crate::config::{
    CONFIG_FILE_NAME, FallingLeavesConfig, MOD_ID, ParticleImplementation,
    is_vanilla_untinted_leaves, register_mod_configs, register_settings,
};
use crate::leaf_util::{
    ParticleHandles, binomial_half, chebyshev_distance, is_decaying_leaf, modified_spawn_chance,
    spawn_leaf_particles, spawn_snow_particles,
};
use crate::particles::{
    LeafParticleData, LeafParticleKind, LeafParticleProvider, ParticleSharedState,
};
use crate::wind::{Wind, WindRng, WindState};
use mod_api::{
    AddHitEffectsEvent, AnimateTickEvent, BlockPart, BlockState, ClientBlockChangedEvent,
    ClientHost, ClientLevelLoadEvent, ClientTickPostEvent, ConfigContext, ConfigKind,
    ConfigLoadingEvent, ConfigPath, ConfigReloadingEvent, DebugEntry, DebugGroup, Flow, Hooks,
    HostSide, LeavesDecayEvent, LevelHost, ParticleOptions, RandomSource, RegisterConfigsEvent,
    RegisterDebugEntriesEvent, RegisterParticleProvidersEvent, Registrar, RegistrationContext,
    SettingChangedEvent,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

const MAX_PENDING_DECAYS: usize = 128;

/// Global handles and shared state initialized during `Hooks::register`.
static HANDLES: OnceLock<ParticleHandles> = OnceLock::new();
static PARTICLE_SHARED: OnceLock<Arc<ParticleSharedState>> = OnceLock::new();
static RUNTIME: OnceLock<Mutex<Runtime>> = OnceLock::new();
static PENDING_DECAYS: OnceLock<Mutex<Vec<([i32; 3], BlockState)>>> = OnceLock::new();
static CLIENT_ACTIVE: AtomicBool = AtomicBool::new(false);

fn particle_shared() -> Arc<ParticleSharedState> {
    PARTICLE_SHARED
        .get_or_init(|| Arc::new(ParticleSharedState::new()))
        .clone()
}

fn runtime_mutex() -> &'static Mutex<Runtime> {
    RUNTIME.get_or_init(|| Mutex::new(Runtime::new()))
}

fn lock_runtime() -> MutexGuard<'static, Runtime> {
    runtime_mutex().lock().unwrap_or_else(|e| e.into_inner())
}

fn pending_decays() -> &'static Mutex<Vec<([i32; 3], BlockState)>> {
    PENDING_DECAYS.get_or_init(|| Mutex::new(Vec::new()))
}

fn push_pending_decay(pos: [i32; 3], state: BlockState) {
    if let Ok(mut queue) = pending_decays().lock()
        && queue.len() < MAX_PENDING_DECAYS
    {
        queue.push((pos, state));
    }
}

fn drain_pending_decays() -> Vec<([i32; 3], BlockState)> {
    match pending_decays().lock() {
        Ok(mut queue) => std::mem::take(&mut *queue),
        Err(e) => std::mem::take(&mut *e.into_inner()),
    }
}

/// Minimal `RandomSource` backed by `WindRng` for client-tick spawns.
struct LocalRandom(WindRng);

impl RandomSource for LocalRandom {
    fn next_int(&mut self) -> i32 {
        (self.0.next_u64() >> 32) as i32
    }

    fn next_int_bound(&mut self, bound: i32) -> i32 {
        if bound <= 0 {
            return 0;
        }
        self.0.next_usize(bound as usize) as i32
    }

    fn next_long(&mut self) -> i64 {
        self.0.next_u64() as i64
    }

    fn next_boolean(&mut self) -> bool {
        (self.0.next_u64() & 1) != 0
    }

    fn next_float(&mut self) -> f32 {
        self.0.next_f32()
    }

    fn next_double(&mut self) -> f64 {
        ((self.0.next_u64() >> 11) as f64) * (1.0 / ((1u64 << 53) as f64))
    }

    fn next_gaussian(&mut self) -> f64 {
        let u1 = self.next_double().max(1e-12);
        let u2 = self.next_double();
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }
}

struct Runtime {
    config: FallingLeavesConfig,
    settings_synced: bool,
    fluid_cache_ready: bool,
    dimension: String,
    wind: Wind,
    tick_counter: u64,
    local_rng: LocalRandom,
    last_hit_pos: Option<([i32; 3], u64)>,
}

impl Runtime {
    fn new() -> Self {
        Self {
            config: FallingLeavesConfig::default(),
            settings_synced: false,
            fluid_cache_ready: false,
            dimension: "minecraft:overworld".to_owned(),
            wind: Wind::new(0xFA11_1EAF),
            tick_counter: 0,
            local_rng: LocalRandom(WindRng::new(0x5EED_1EAF)),
            last_hit_pos: None,
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

    fn ensure_fluid_cache(&mut self, host: &dyn ClientHost) {
        if self.fluid_cache_ready {
            return;
        }
        let shared = particle_shared();
        let mut cache = shared.blocks.lock().unwrap_or_else(|e| e.into_inner());
        cache.water_levels.clear();
        cache.lava_levels.clear();

        for level in 0u8..=15 {
            if let Some(state) = host.block_state_id(&format!("minecraft:water[level={level}]")) {
                cache.water_levels.insert(state.ordinal(), level);
            }
            if let Some(state) = host.block_state_id(&format!("minecraft:lava[level={level}]")) {
                cache.lava_levels.insert(state.ordinal(), level);
            }
        }
        self.fluid_cache_ready = true;
    }

    fn debug_snapshot(&self) -> (WindState, f32, f32, String) {
        (
            self.wind.state,
            self.wind.wind_x,
            self.wind.wind_z,
            self.dimension.clone(),
        )
    }
}

/// `BlockPart` attached to `#minecraft:leaves` for ambient leaf/snowflake ticks and block-hit drops.
pub struct LeafBlockPart;

#[mod_api::sunrise_part]
impl BlockPart for LeafBlockPart {
    fn animate_tick(&self, host: &mut dyn ClientHost, event: &mut AnimateTickEvent<'_>) {
        if !host.on(MOD_ID) {
            return;
        }
        let Some(handles) = HANDLES.get().copied() else {
            return;
        };

        let config = lock_runtime().config.clone();
        if !config.enabled {
            return;
        }

        let pos = [event.pos.x, event.pos.y, event.pos.z];
        let Some(block_id) = event.state.block().id(&*host).map(str::to_owned) else {
            return;
        };
        let serialized = host.block_state_string(event.state).unwrap_or_default();
        let player_block_pos = host
            .local_player()
            .and_then(|p| p.entity().position(&*host))
            .map(|p| {
                [
                    p[0].floor() as i32,
                    p[1].floor() as i32,
                    p[2].floor() as i32,
                ]
            });

        let settings = config.settings_for(&block_id);
        let implementation = settings.effective_implementation(&config);

        // When using the Vanilla implementation on a vanilla leaf block that already
        // spawns particles in Sunrise's built-in animateTick, avoid doubling at 1.0x rate;
        // only add extra spawns if the configured chance exceeds vanilla's default.
        let spawn_chance =
            modified_spawn_chance(&config, player_block_pos, pos, &serialized, true, &settings);

        let effective_chance = if implementation == ParticleImplementation::Vanilla
            && vanilla_block_already_animates(&block_id)
        {
            (spawn_chance - vanilla_default_chance(&block_id)).max(0.0)
        } else {
            spawn_chance
        };

        if effective_chance > 0.0 && event.random.next_float() < effective_chance {
            spawn_leaf_particles(
                host,
                &handles,
                &config,
                event.level,
                pos,
                event.state,
                1,
                false,
                &settings,
                event.random,
            );
        }

        // Snowflakes spawn independently of leaf settings (subject to minimal spawn radius).
        let snow_chance = config.snowflake_spawn_chance();
        let inside_radius = config.starting_spawn_radius > 0
            && player_block_pos
                .is_some_and(|pp| chebyshev_distance(pp, pos) < config.starting_spawn_radius);
        if !inside_radius && snow_chance > 0.0 && event.random.next_float() < snow_chance {
            spawn_snow_particles(
                host,
                &handles,
                &config,
                event.level,
                pos,
                1,
                false,
                event.random,
            );
        }
    }

    fn add_hit_effects(
        &self,
        host: &mut dyn ClientHost,
        event: &mut AddHitEffectsEvent<'_>,
    ) -> Flow {
        if !host.on(MOD_ID) {
            return Flow::Continue;
        }
        let Some(handles) = HANDLES.get().copied() else {
            return Flow::Continue;
        };

        let pos = [event.pos.x, event.pos.y, event.pos.z];
        let (config, allow_drop) = {
            let mut rt = lock_runtime();
            let now = rt.tick_counter;
            // Throttle continuous hold-to-mine crack ticks so hitting a leaf block
            // drops a crisp burst like Fabric's AttackBlockCallback.
            let allow = match rt.last_hit_pos {
                Some((prev_pos, prev_tick))
                    if prev_pos == pos && now.saturating_sub(prev_tick) < 4 =>
                {
                    false
                }
                _ => {
                    rt.last_hit_pos = Some((pos, now));
                    true
                }
            };
            (rt.config.clone(), allow)
        };

        if !allow_drop || !config.enabled || !config.leaves_on_block_hit {
            return Flow::Continue;
        }

        let Some(block_id) = event.state.block().id(&*host).map(str::to_owned) else {
            return Flow::Continue;
        };
        let settings = config.settings_for(&block_id);

        if settings.spawn_breaking_leaves {
            let count = binomial_half(3, event.random);
            spawn_leaf_particles(
                host,
                &handles,
                &config,
                event.level,
                pos,
                event.state,
                count,
                false,
                &settings,
                event.random,
            );
        }

        if config.snowflake_spawn_chance() > 0.0 {
            let snow_count = binomial_half(6, event.random);
            spawn_snow_particles(
                host,
                &handles,
                &config,
                event.level,
                pos,
                snow_count,
                false,
                event.random,
            );
        }

        Flow::Continue
    }
}

fn vanilla_block_already_animates(block_id: &str) -> bool {
    is_vanilla_untinted_leaves(block_id)
        || matches!(
            block_id,
            "minecraft:oak_leaves"
                | "minecraft:birch_leaves"
                | "minecraft:jungle_leaves"
                | "minecraft:acacia_leaves"
                | "minecraft:dark_oak_leaves"
                | "minecraft:mangrove_leaves"
                | "minecraft:azalea_leaves"
                | "minecraft:flowering_azalea_leaves"
        )
}

fn vanilla_default_chance(block_id: &str) -> f32 {
    match block_id {
        "minecraft:cherry_leaves" => 0.1,
        "minecraft:pale_oak_leaves" => 0.02,
        _ => 0.01,
    }
}

#[derive(Default)]
struct FallingLeaves;

#[mod_api::sunrise_mod]
impl Hooks for FallingLeaves {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        let falling_leaf = r.register_particle_type::<LeafParticleData>(
            "fallingleaves:falling_leaf",
            true,
            LeafParticleData::map_codec(),
            LeafParticleData::stream_codec(),
        );
        let falling_leaf_vanilla = r.register_particle_type::<LeafParticleData>(
            "fallingleaves:falling_leaf_vanilla",
            true,
            LeafParticleData::map_codec(),
            LeafParticleData::stream_codec(),
        );
        let falling_leaf_conifer = r.register_particle_type::<LeafParticleData>(
            "fallingleaves:falling_leaf_conifer",
            true,
            LeafParticleData::map_codec(),
            LeafParticleData::stream_codec(),
        );
        let falling_cherry = r.register_particle_type::<LeafParticleData>(
            "fallingleaves:falling_cherry",
            true,
            LeafParticleData::map_codec(),
            LeafParticleData::stream_codec(),
        );
        let falling_snow = r.register_particle_type::<LeafParticleData>(
            "fallingleaves:falling_snow",
            true,
            LeafParticleData::map_codec(),
            LeafParticleData::stream_codec(),
        );

        let handles = ParticleHandles {
            falling_leaf,
            falling_leaf_vanilla,
            falling_leaf_conifer,
            falling_cherry,
            falling_snow,
            vanilla_lava: r.particle_key("minecraft:lava"),
            vanilla_tinted_leaves: r.particle_key("minecraft:tinted_leaves"),
            vanilla_cherry_leaves: r.particle_key("minecraft:cherry_leaves"),
            vanilla_pale_oak_leaves: r.particle_key("minecraft:pale_oak_leaves"),
            vanilla_red_poplar_leaves: r.particle_key("minecraft:red_poplar_leaves"),
            vanilla_orange_poplar_leaves: r.particle_key("minecraft:orange_poplar_leaves"),
            vanilla_yellow_poplar_leaves: r.particle_key("minecraft:yellow_poplar_leaves"),
        };
        let _ = HANDLES.set(handles);

        // Cache air/water/lava BlockKeys for ParticleProvider::tick.
        let shared = particle_shared();
        {
            let mut cache = shared.blocks.lock().unwrap_or_else(|e| e.into_inner());
            cache.air_keys = vec![
                r.block_key("minecraft:air"),
                r.block_key("minecraft:cave_air"),
                r.block_key("minecraft:void_air"),
            ];
            cache.water_key = Some(r.block_key("minecraft:water"));
            cache.lava_key = Some(r.block_key("minecraft:lava"));
            cache.water_levels = HashMap::new();
            cache.lava_levels = HashMap::new();
        }

        // Attach animate_tick and add_hit_effects to every block in #minecraft:leaves.
        r.block_part_tag("minecraft:leaves", LeafBlockPart);

        register_settings(r);
    }

    fn register_particle_providers(
        &self,
        _ctx: &RegistrationContext,
        event: &mut RegisterParticleProvidersEvent<'_>,
    ) {
        let Some(handles) = HANDLES.get().copied() else {
            return;
        };
        let shared = particle_shared();

        event.providers.register_sprite_set(
            handles.falling_leaf,
            Arc::new(LeafParticleProvider {
                kind: LeafParticleKind::Regular,
                shared: shared.clone(),
            }),
        );
        event.providers.register_sprite_set(
            handles.falling_leaf_vanilla,
            Arc::new(LeafParticleProvider {
                kind: LeafParticleKind::RegularVanilla,
                shared: shared.clone(),
            }),
        );
        event.providers.register_sprite_set(
            handles.falling_leaf_conifer,
            Arc::new(LeafParticleProvider {
                kind: LeafParticleKind::Conifer,
                shared: shared.clone(),
            }),
        );
        event.providers.register_sprite_set(
            handles.falling_cherry,
            Arc::new(LeafParticleProvider {
                kind: LeafParticleKind::Cherry,
                shared: shared.clone(),
            }),
        );
        event.providers.register_sprite_set(
            handles.falling_snow,
            Arc::new(LeafParticleProvider {
                kind: LeafParticleKind::Snow,
                shared,
            }),
        );
    }

    fn register_configs(&self, _ctx: &dyn ConfigContext, event: &mut RegisterConfigsEvent<'_>) {
        register_mod_configs(event.configs);
    }

    fn config_loading(&self, _ctx: &dyn ConfigContext, event: &mut ConfigLoadingEvent<'_>) {
        if event.config.mod_id != MOD_ID || event.config.kind != ConfigKind::Client {
            return;
        }
        let parsed = FallingLeavesConfig::parse_toml(&event.config.text);
        lock_runtime().config = parsed;
    }

    fn config_reloading(&self, host: HostSide<'_>, event: &mut ConfigReloadingEvent<'_>) {
        if event.config.mod_id != MOD_ID || event.config.kind != ConfigKind::Client {
            return;
        }
        let parsed = FallingLeavesConfig::parse_toml(&event.config.text);
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

    fn register_debug_entries(
        &self,
        _ctx: &RegistrationContext,
        event: &mut RegisterDebugEntriesEvent<'_>,
    ) {
        event.entries.push(
            DebugEntry::text(
                format!("{MOD_ID}:wind"),
                DebugGroup::Position,
                |_host, lines| {
                    let (state, wx, wz, dim) = lock_runtime().debug_snapshot();
                    lines.push(format!(
                        "Falling Leaves Wind: {} ({:.3}, {:.3}) [{}]",
                        state.as_str(),
                        wx,
                        wz,
                        dim
                    ));
                },
            )
            .with_reduced_debug_info(true),
        );
    }

    fn client_level_load(&self, host: &mut dyn ClientHost, event: &mut ClientLevelLoadEvent<'_>) {
        CLIENT_ACTIVE.store(true, Ordering::Relaxed);
        let seed = host.millis() ^ 0xFA11_1EAF;
        let mut rt = lock_runtime();
        rt.dimension = event.dimension.to_owned();
        rt.wind.reset(seed);
        rt.ensure_fluid_cache(&*host);
        particle_shared().wind.store(0.0, 0.0, false);
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPostEvent) {
        if !host.on(MOD_ID) {
            return;
        }
        CLIENT_ACTIVE.store(true, Ordering::Relaxed);

        let Some(handles) = HANDLES.get().copied() else {
            return;
        };

        let player_and_level = host
            .local_player()
            .and_then(|p| Some((p.entity().position(&*host)?, p.level(&*host)?)));

        let (config, paused) = {
            let mut rt = lock_runtime();
            rt.sync_initial_settings(&*host);
            rt.ensure_fluid_cache(&*host);
            rt.tick_counter = rt.tick_counter.wrapping_add(1);
            let paused = host.is_paused();

            if !paused
                && rt.config.enabled
                && let Some((_, level)) = player_and_level
            {
                let (rain, thunder) = level.weather(&*host).unwrap_or((0.0, 0.0));
                let is_raining = rain > 0.2;
                let is_thundering = thunder > 0.9;
                let dim = rt.dimension.clone();
                let wind_enabled = rt.config.wind_enabled;
                let windless = rt.config.windless_dimensions.clone();
                rt.wind
                    .tick(&dim, is_raining, is_thundering, wind_enabled, &windless);
                particle_shared()
                    .wind
                    .store(rt.wind.wind_x, rt.wind.wind_z, is_raining);
            }
            (rt.config.clone(), paused)
        };

        // Drain lava splash particles queued by ParticleProvider::tick.
        for pos in particle_shared().drain_lava_splashes() {
            let opts = ParticleOptions::simple(handles.vanilla_lava);
            let _ = host.add_particle(&opts, pos, [0.0, 0.0, 0.0]);
        }

        if paused || !config.enabled {
            let _ = drain_pending_decays();
            return;
        }

        let Some((player_pos, level)) = player_and_level else {
            let _ = drain_pending_decays();
            return;
        };

        // Spawn bursts for decaying leaf blocks (from leaves_decay / client_block_changed).
        let decays = drain_pending_decays();
        if !decays.is_empty() && config.max_decay_leaves > 0 {
            let mut rng = LocalRandom(WindRng::new(host.millis() ^ 0xDEC4_7EAF));
            for (pos, state) in decays {
                let Some(block_id) = state.block().id(&*host).map(str::to_owned) else {
                    continue;
                };
                let settings = config.settings_for(&block_id);
                if settings.spawn_breaking_leaves {
                    let count = binomial_half(config.max_decay_leaves, &mut rng);
                    spawn_leaf_particles(
                        host, &handles, &config, level, pos, state, count, true, &settings,
                        &mut rng,
                    );
                }
                if config.snowflake_spawn_chance() > 0.0 {
                    let snow_count = binomial_half(2 * config.max_decay_leaves, &mut rng);
                    spawn_snow_particles(
                        host, &handles, &config, level, pos, snow_count, true, &mut rng,
                    );
                }
            }
        }

        // Custom user-configured `leaf_spawners` (non-#minecraft:leaves blocks such as
        // `minecraft:bamboo[leaves=large]`): sample random blocks within 16 and 32 blocks
        // matching ClientLevel.animateTick when the list is non-empty.
        if !config.parsed_spawners.is_empty() {
            let center = [
                player_pos[0].floor() as i32,
                player_pos[1].floor() as i32,
                player_pos[2].floor() as i32,
            ];
            let mut rng = {
                let mut rt = lock_runtime();
                let seed = rt.local_rng.0.next_u64();
                LocalRandom(WindRng::new(seed))
            };
            for radius in [16i32, 32i32] {
                for _ in 0..667 {
                    let pos = [
                        center[0] + rng.next_int_bound(radius) - rng.next_int_bound(radius),
                        center[1] + rng.next_int_bound(radius) - rng.next_int_bound(radius),
                        center[2] + rng.next_int_bound(radius) - rng.next_int_bound(radius),
                    ];
                    let Some(state) = level.block_state(&*host, pos) else {
                        continue;
                    };
                    let Some(block_id) = state.block().id(&*host).map(str::to_owned) else {
                        continue;
                    };
                    if !config
                        .parsed_spawners
                        .iter()
                        .any(|r| r.block_id == block_id)
                    {
                        continue;
                    }
                    let serialized = host.block_state_string(state).unwrap_or_default();
                    if !config
                        .parsed_spawners
                        .iter()
                        .any(|r| r.matches_serialized(&block_id, &serialized))
                    {
                        continue;
                    }

                    let snow_chance = config.snowflake_spawn_chance();
                    let inside_radius = config.starting_spawn_radius > 0
                        && chebyshev_distance(center, pos) < config.starting_spawn_radius;
                    if !inside_radius && snow_chance > 0.0 && rng.next_float() < snow_chance {
                        spawn_snow_particles(
                            host, &handles, &config, level, pos, 1, false, &mut rng,
                        );
                    }

                    let settings = config.settings_for(&block_id);
                    let chance = modified_spawn_chance(
                        &config,
                        Some(center),
                        pos,
                        &serialized,
                        false,
                        &settings,
                    );
                    if chance > 0.0 && rng.next_float() < chance {
                        spawn_leaf_particles(
                            host, &handles, &config, level, pos, state, 1, false, &settings,
                            &mut rng,
                        );
                    }
                }
            }
        }
    }

    fn leaves_decay(&self, host: &dyn LevelHost, event: &mut LeavesDecayEvent) -> Flow {
        if !CLIENT_ACTIVE.load(Ordering::Relaxed) {
            return Flow::Continue;
        }
        let pos = [event.pos.x, event.pos.y, event.pos.z];
        if let Some(state) = host.block_state(pos) {
            push_pending_decay(pos, state);
        }
        Flow::Continue
    }

    fn client_block_changed(&self, host: &dyn ClientHost, event: &mut ClientBlockChangedEvent<'_>) {
        if !host.on(MOD_ID) || event.predicted {
            return;
        }
        let new_id = event.new.block().id(host).unwrap_or("");
        if !matches!(
            new_id,
            "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
        ) {
            return;
        }
        if !host
            .block_in_tag(event.old.block(), "minecraft:leaves")
            .unwrap_or(false)
        {
            return;
        }
        let serialized = host.block_state_string(event.old).unwrap_or_default();
        if is_decaying_leaf(&serialized) {
            push_pending_decay([event.pos.x, event.pos.y, event.pos.z], event.old);
        }
    }
}
