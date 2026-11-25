#![deny(unsafe_code)]

//! Particle Rain for Sunrise: replaces weather effects with particle rain, snow,
//! sandstorms, splashes, ripples, rain streaks, tumbling shrubs, and fog.

mod config;
mod particles;
mod png;
mod shrub;
mod spawner;
mod sprites;
mod wind;

use crate::config::{
    CONFIG_FILE_NAME, MOD_ID, ParticleRainConfig, register_mod_configs, register_settings,
};
use crate::particles::{
    ParticleSharedState, StreakOptions, StreakParticleProvider, WeatherParticleOptions,
    WeatherParticleProvider,
};
use crate::shrub::{ShrubManager, ShrubTickContext};
use crate::spawner::{
    DEFAULT_CLOUD_HEIGHT, ParticleHandles, ParticleSpawnerState, precipitation_at,
};
use crate::sprites::generate_weather_sprites;
use crate::wind::y_level_wind_multiplier;
use mod_api::{
    ClientHost, ClientLevelUnloadEvent, ClientReloadHost, ClientResourcesReloadedEvent,
    ClientSetupContext, ClientSetupEvent, ClientTickPostEvent, Command, CommandEvent,
    ConfigContext, ConfigKind, ConfigLoadingEvent, ConfigPath, ConfigReloadingEvent, DebugEntry,
    DebugGroup, Flow, Heightmap, Hooks, HostSide, Identifier, LoopContext, ParticleBlend,
    ParticleGroup, ParticleOptions, PlaySoundEvent, RegisterClientCommandsEvent,
    RegisterConfigsEvent, RegisterDebugEntriesEvent, RegisterParticleGroupsEvent,
    RegisterParticleProvidersEvent, RegisterSpriteSourcesEvent, Registrar, RegistrationContext,
    RenderLevelStage, RenderLevelStageEvent, SettingChangedEvent, Text,
};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

static RAIN_VOLUME_BITS: AtomicU32 = AtomicU32::new(0.2f32.to_bits());
static SPRITE_CONFIG: Mutex<(bool, u32, bool)> = Mutex::new((true, 16, true));
static DEBUG_SNAPSHOT: Mutex<DebugSnapshot> = Mutex::new(DebugSnapshot::empty());

/// Common biome tags checked for `/particlerain biometags` and the F3 biome-tags debug entry.
const INSPECTED_BIOME_TAGS: &[&str] = &[
    "c:in_overworld",
    "c:is_wet",
    "c:is_spooky",
    "c:climate_wet",
    "c:is_windswept",
    "c:is_desert",
    "c:is_snowy",
    "c:is_cold",
    "c:is_hot",
    "minecraft:is_overworld",
    "minecraft:is_forest",
    "minecraft:is_taiga",
    "minecraft:is_jungle",
    "minecraft:is_mountain",
    "minecraft:is_ocean",
    "minecraft:is_river",
    "minecraft:is_beach",
    "minecraft:is_badlands",
    "minecraft:is_savanna",
];

#[derive(Clone, Debug)]
struct DebugSnapshot {
    tracked_particles: usize,
    particle_amount: i32,
    after_weather_ticks_left: i32,
    spawn_attempts_until_block_fx_idle: i32,
    ticks_until_sky_fx_idle: i32,
    ticks_until_surface_fx_idle: i32,
    is_raining: bool,
    precipitation: &'static str,
    wind_multiplier: f32,
    cloud_height: i32,
    surface_height: i32,
    default_surface_height: i32,
    speed: f32,
    sky_particle_density: i32,
    rain_level: f32,
    thunder_level: f32,
}

impl DebugSnapshot {
    const fn empty() -> Self {
        Self {
            tracked_particles: 0,
            particle_amount: 1500,
            after_weather_ticks_left: 0,
            spawn_attempts_until_block_fx_idle: 0,
            ticks_until_sky_fx_idle: 0,
            ticks_until_surface_fx_idle: 0,
            is_raining: false,
            precipitation: "NONE",
            wind_multiplier: 0.0,
            cloud_height: DEFAULT_CLOUD_HEIGHT,
            surface_height: 0,
            default_surface_height: 0,
            speed: 0.0,
            sky_particle_density: 0,
            rain_level: 0.0,
            thunder_level: 0.0,
        }
    }

    fn format_lines(&self) -> Vec<String> {
        vec![
            format!(
                "Tracked particles: {}/{}",
                self.tracked_particles, self.particle_amount
            ),
            format!(
                "after Weather Ticks Left: {}",
                self.after_weather_ticks_left
            ),
            format!(
                "spawn Attempts Until Block FX Idle: {}",
                self.spawn_attempts_until_block_fx_idle
            ),
            format!("ticks Until Sky FX Idle: {}", self.ticks_until_sky_fx_idle),
            format!(
                "ticks Until Surface FX Idle: {}",
                self.ticks_until_surface_fx_idle
            ),
            format!("is Raining: {}", self.is_raining),
            format!("Biome Precipitation: {}", self.precipitation),
            format!("Wind multiplier: {:.3}", self.wind_multiplier),
            format!("cloud height: {}", self.cloud_height),
            format!("surface height: {}", self.surface_height),
            format!("default surface height: {}", self.default_surface_height),
            format!("speed: {:.3}", self.speed),
            format!("sky particle density: {}", self.sky_particle_density),
            format!("Rain level: {:.3}", self.rain_level),
            format!("Thunder level: {:.3}", self.thunder_level),
        ]
    }
}

struct Runtime {
    config: ParticleRainConfig,
    settings_synced: bool,
    block_keys_initialized: bool,
    spawner: ParticleSpawnerState,
    shrubs: ShrubManager,
}

impl Runtime {
    fn new() -> Self {
        Self {
            config: ParticleRainConfig::default(),
            settings_synced: false,
            block_keys_initialized: false,
            spawner: ParticleSpawnerState::default(),
            shrubs: ShrubManager::default(),
        }
    }

    fn publish_config(&self, shared: &Arc<Mutex<ParticleSharedState>>) {
        RAIN_VOLUME_BITS.store(self.config.sound.rain_volume.to_bits(), Ordering::Relaxed);
        {
            let mut sc = SPRITE_CONFIG.lock().unwrap_or_else(|e| e.into_inner());
            *sc = (
                self.config.compat.water_tint,
                self.config.compat.ripple_res,
                self.config.compat.ripple_use_pack_res,
            );
        }
        let mut guard = shared.lock().unwrap_or_else(|e| e.into_inner());
        guard.sync_config(&self.config);
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

    fn ensure_block_keys(
        &mut self,
        host: &dyn ClientHost,
        shared: &Arc<Mutex<ParticleSharedState>>,
    ) {
        if self.block_keys_initialized {
            return;
        }
        self.block_keys_initialized = true;
        let mut air_keys = Vec::new();
        for name in ["minecraft:air", "minecraft:cave_air", "minecraft:void_air"] {
            if let Some(state) = host.block_state_id(name) {
                air_keys.push(state.block());
            }
        }
        let mut fluid_keys = Vec::new();
        for name in ["minecraft:water", "minecraft:lava"] {
            if let Some(state) = host.block_state_id(name) {
                fluid_keys.push(state.block());
            }
        }
        let mut guard = shared.lock().unwrap_or_else(|e| e.into_inner());
        guard.air_keys = air_keys;
        guard.fluid_keys = fluid_keys;
    }
}

struct ParticleRainMod {
    runtime: Mutex<Runtime>,
    shared: Arc<Mutex<ParticleSharedState>>,
    handles: Mutex<Option<ParticleHandles>>,
}

impl Default for ParticleRainMod {
    fn default() -> Self {
        Self {
            runtime: Mutex::new(Runtime::new()),
            shared: Arc::new(Mutex::new(ParticleSharedState::default())),
            handles: Mutex::new(None),
        }
    }
}

impl ParticleRainMod {
    fn lock(&self) -> MutexGuard<'_, Runtime> {
        self.runtime.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn format_biome_tags(host: &dyn ClientHost) -> Vec<String> {
    let Some(level) = host.level(host.dimension()) else {
        return Vec::new();
    };
    let Some(pos) = host
        .local_player()
        .and_then(|p| p.entity().position(host))
        .map(|p| {
            [
                p[0].floor() as i32,
                p[1].floor() as i32,
                p[2].floor() as i32,
            ]
        })
    else {
        return Vec::new();
    };
    let Some(biome) = level.client_biome(host, pos) else {
        return Vec::new();
    };
    let mut lines = vec![format!("{biome} has the following tags:")];
    for &tag in INSPECTED_BIOME_TAGS {
        if level.biome_in_tag(host, &biome, tag) == Some(true) {
            lines.push(format!("#{tag}"));
        }
    }
    lines
}

#[mod_api::sunrise_mod]
impl Hooks for ParticleRainMod {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        register_settings(r);

        let weather_translucent = r.register_particle_type(
            "particlerain:weather",
            false,
            WeatherParticleOptions::map_codec(),
            WeatherParticleOptions::stream_codec(),
        );
        let weather_opaque = r.register_particle_type(
            "particlerain:weather_opaque",
            false,
            WeatherParticleOptions::map_codec(),
            WeatherParticleOptions::stream_codec(),
        );
        let streak = r.register_particle_type(
            "particlerain:streak",
            false,
            StreakOptions::map_codec(),
            StreakOptions::stream_codec(),
        );
        let vanilla_rain = r.particle_key("minecraft:rain");
        let vanilla_smoke = r.particle_key("minecraft:smoke");
        let vanilla_dripping_water = r.particle_key("minecraft:dripping_water");

        *self.handles.lock().unwrap_or_else(|e| e.into_inner()) = Some(ParticleHandles {
            weather_translucent,
            weather_opaque,
            streak,
            vanilla_rain,
            vanilla_smoke,
            vanilla_dripping_water,
        });
    }

    fn register_configs(&self, _ctx: &dyn ConfigContext, event: &mut RegisterConfigsEvent<'_>) {
        register_mod_configs(event.configs);
    }

    fn config_loading(&self, _ctx: &dyn ConfigContext, event: &mut ConfigLoadingEvent<'_>) {
        if event.config.mod_id != MOD_ID || event.config.kind != ConfigKind::Client {
            return;
        }
        let parsed = ParticleRainConfig::parse_toml(&event.config.text);
        let mut rt = self.lock();
        rt.config = parsed;
        rt.publish_config(&self.shared);
    }

    fn config_reloading(&self, host: HostSide<'_>, event: &mut ConfigReloadingEvent<'_>) {
        if event.config.mod_id != MOD_ID || event.config.kind != ConfigKind::Client {
            return;
        }
        let parsed = ParticleRainConfig::parse_toml(&event.config.text);
        let mut rt = self.lock();
        rt.config = parsed;
        rt.publish_config(&self.shared);
        if let HostSide::Client(client) = host {
            rt.config.write_to_host_settings(&*client);
        }
    }

    fn setting_changed(&self, host: &mut dyn ClientHost, event: &mut SettingChangedEvent<'_>) {
        if event.mod_id != MOD_ID {
            return;
        }
        let clear = {
            let mut rt = self.lock();
            let clear = rt.config.apply_setting_change(&*host, event.key);
            rt.publish_config(&self.shared);
            rt.persist_config_file(&*host);
            clear
        };
        if clear {
            let _ = host.clear_particles();
        }
    }

    fn register_sprite_sources(
        &self,
        _ctx: &RegistrationContext,
        event: &mut RegisterSpriteSourcesEvent<'_>,
    ) {
        if let Ok(id) = Identifier::parse("particlerain:weather_sprites") {
            event.register(id, |input| {
                let (water_tint, ripple_res, ripple_use_pack_res) =
                    *SPRITE_CONFIG.lock().unwrap_or_else(|e| e.into_inner());
                Ok(generate_weather_sprites(
                    input.resources,
                    water_tint,
                    ripple_res,
                    ripple_use_pack_res,
                ))
            });
        }
    }

    fn register_particle_groups(
        &self,
        _ctx: &RegistrationContext,
        event: &mut RegisterParticleGroupsEvent<'_>,
    ) {
        if let Ok(id) = Identifier::parse("particlerain:opaque") {
            event.register(id, || ParticleGroup {
                blend: ParticleBlend::Opaque,
                particles: vec!["particlerain:weather_opaque".into()],
            });
        }
        if let Ok(id) = Identifier::parse("particlerain:translucent") {
            event.register(id, || ParticleGroup {
                blend: ParticleBlend::Translucent,
                particles: vec!["particlerain:weather".into(), "particlerain:streak".into()],
            });
        }
    }

    fn register_particle_providers(
        &self,
        _ctx: &RegistrationContext,
        event: &mut RegisterParticleProvidersEvent<'_>,
    ) {
        let Some(handles) = *self.handles.lock().unwrap_or_else(|e| e.into_inner()) else {
            return;
        };
        event.providers.register_sprite_set(
            handles.weather_translucent,
            Arc::new(WeatherParticleProvider::new(
                Arc::clone(&self.shared),
                ParticleBlend::Translucent,
            )),
        );
        event.providers.register_sprite_set(
            handles.weather_opaque,
            Arc::new(WeatherParticleProvider::new(
                Arc::clone(&self.shared),
                ParticleBlend::Opaque,
            )),
        );
        event.providers.register_sprite_set(
            handles.streak,
            Arc::new(StreakParticleProvider::new(Arc::clone(&self.shared))),
        );
    }

    fn register_debug_entries(
        &self,
        _ctx: &RegistrationContext,
        event: &mut RegisterDebugEntriesEvent<'_>,
    ) {
        event.entries.push(DebugEntry::text(
            format!("{MOD_ID}:debug"),
            DebugGroup::Position,
            |_host, lines| {
                let snap = DEBUG_SNAPSHOT
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                lines.extend(snap.format_lines());
            },
        ));
        event.entries.push(DebugEntry::text(
            format!("{MOD_ID}:biome_tags"),
            DebugGroup::Position,
            |host, lines| {
                lines.extend(format_biome_tags(host));
            },
        ));
    }

    fn register_client_commands(
        &self,
        _host: &mut dyn ClientHost,
        event: &mut RegisterClientCommandsEvent<'_>,
    ) {
        event.commands.push(Command::new("particlerain"));
        event.commands.push(Command::new("particlerain debug"));
        event.commands.push(Command::new("particlerain biometags"));
    }

    fn command(&self, host: &mut dyn ClientHost, event: &mut CommandEvent<'_>) -> Flow {
        let cmd = event.command.trim();
        if cmd == "particlerain" {
            let _ = host.chat_line(Text::literal(
                "Particle Rain settings are in Options -> Mods -> Particle Rain and config/particlerain.toml. Subcommands: /particlerain debug, /particlerain biometags",
            ));
            return Flow::Cancel;
        }
        if cmd == "particlerain debug" {
            let lines = DEBUG_SNAPSHOT
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .format_lines();
            for line in lines {
                let _ = host.chat_line(Text::literal(line));
            }
            return Flow::Cancel;
        }
        if cmd == "particlerain biometags" {
            let lines = format_biome_tags(&*host);
            for line in lines {
                let _ = host.chat_line(Text::literal(line));
            }
            return Flow::Cancel;
        }
        Flow::Continue
    }

    fn client_setup(&self, _ctx: &mut dyn ClientSetupContext, _event: &mut ClientSetupEvent) {
        let rt = self.lock();
        rt.publish_config(&self.shared);
    }

    fn client_resources_reloaded(
        &self,
        _ctx: &dyn ClientReloadHost,
        _event: &mut ClientResourcesReloadedEvent,
    ) {
        let mut rt = self.lock();
        rt.shrubs.invalidate_models();
    }

    fn client_level_unload(
        &self,
        _host: &mut dyn ClientHost,
        _event: &mut ClientLevelUnloadEvent<'_>,
    ) {
        let mut rt = self.lock();
        rt.spawner.reset();
        rt.shrubs.clear();
        let mut shared = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        shared.tracked_translucent = 0;
        shared.tracked_opaque = 0;
        shared.tracked_streaks = 0;
        shared.pending_drips.clear();
    }

    fn play_sound(&self, _ctx: &dyn LoopContext, event: &mut PlaySoundEvent<'_>) {
        let Some(sound) = event.sound.as_mut() else {
            return;
        };
        if sound.identifier == "minecraft:weather.rain"
            || sound.identifier == "minecraft:weather.rain.above"
        {
            let vol = f32::from_bits(RAIN_VOLUME_BITS.load(Ordering::Relaxed));
            if vol <= 0.0 {
                *event.sound = None;
            } else {
                sound.volume = vol;
            }
        }
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPostEvent) {
        if !host.on(MOD_ID) || host.is_paused() {
            return;
        }
        let Some(handles) = *self.handles.lock().unwrap_or_else(|e| e.into_inner()) else {
            return;
        };
        let Some(level) = host.level(host.dimension()) else {
            return;
        };
        let Some(camera) = host.camera() else {
            return;
        };
        let (rain_level, thunder_level) = level.weather(&*host).unwrap_or((0.0, 0.0));
        let game_time = host.clock().map_or(0, |c| c.game_time);
        let fov = host.graphics_options().fov;
        let cam_vel = host
            .camera_entity()
            .and_then(|e| e.velocity(&*host))
            .unwrap_or([0.0; 3]);
        let speed = (cam_vel[0] * cam_vel[0] + cam_vel[1] * cam_vel[1] + cam_vel[2] * cam_vel[2])
            .sqrt() as f32;

        // Flush any water drips emitted by StreakParticleProvider.
        let pending_drips: Vec<[f64; 3]> = {
            let mut shared = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            shared.game_time = game_time;
            shared.rain_level = rain_level;
            shared.thunder_level = thunder_level;
            shared.camera_pos = camera.position;
            shared.camera_vel = cam_vel;
            shared.fov = fov;
            std::mem::take(&mut shared.pending_drips)
        };
        for drip_pos in pending_drips {
            host.add_particle(
                &ParticleOptions::simple(handles.vanilla_dripping_water),
                drip_pos,
                [0.0; 3],
            );
        }

        let shared_snap = self
            .shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let mut rt = self.lock();
        rt.sync_initial_settings(&*host);
        rt.ensure_block_keys(&*host, &self.shared);

        let config_snap = rt.config.clone();
        let Runtime {
            spawner, shrubs, ..
        } = &mut *rt;
        spawner.tick(
            host,
            level,
            camera.position,
            speed,
            game_time,
            rain_level,
            thunder_level,
            &config_snap,
            &handles,
            &shared_snap,
            shrubs,
        );
        shrubs.tick(
            &*host,
            &ShrubTickContext {
                level,
                camera_pos: camera.position,
                surface_range: config_snap.perf.surface_range,
                game_time,
                thunder_level,
                wind_opts: &config_snap.wind,
            },
        );
        shrubs.resolve_models(&*host);

        // Update F3 debug snapshot.
        let player_block = host
            .local_player()
            .and_then(|p| p.entity().position(&*host))
            .map(|p| {
                [
                    p[0].floor() as i32,
                    p[1].floor() as i32,
                    p[2].floor() as i32,
                ]
            })
            .unwrap_or([
                camera.position[0].floor() as i32,
                camera.position[1].floor() as i32,
                camera.position[2].floor() as i32,
            ]);
        let precip = level
            .client_biome(&*host, player_block)
            .map_or("NONE", |b| {
                precipitation_at(&*host, level, &b, player_block).as_str()
            });
        let surface_h = spawner
            .get_height(
                &*host,
                level,
                player_block[0],
                player_block[2],
                &config_snap,
            )
            .unwrap_or(0);
        let default_surface_h = host
            .client_surface_heights(
                &[(player_block[0], player_block[2])],
                Heightmap::MotionBlocking,
            )
            .ok()
            .and_then(|v| v.into_iter().next().flatten())
            .unwrap_or(0);
        let cloud_h = if config_snap.compat.spawn_height_limit == 0 {
            DEFAULT_CLOUD_HEIGHT
        } else {
            config_snap.compat.spawn_height_limit
        };

        let mut dbg = DEBUG_SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner());
        *dbg = DebugSnapshot {
            tracked_particles: shared_snap.total_tracked() + shrubs.count(),
            particle_amount: config_snap.perf.particle_amount,
            after_weather_ticks_left: spawner.after_weather_ticks_left,
            spawn_attempts_until_block_fx_idle: spawner.spawn_attempts_until_block_fx_idle,
            ticks_until_sky_fx_idle: spawner.ticks_until_sky_fx_idle,
            ticks_until_surface_fx_idle: spawner.ticks_until_surface_fx_idle,
            is_raining: rain_level > 0.2,
            precipitation: precip,
            wind_multiplier: y_level_wind_multiplier(player_block[1] as f64),
            cloud_height: cloud_h,
            surface_height: surface_h,
            default_surface_height: default_surface_h,
            speed,
            sky_particle_density: spawner.sky_particle_density(&config_snap, thunder_level, speed),
            rain_level,
            thunder_level,
        };
    }

    fn render_level_stage(&self, host: &dyn ClientHost, event: &mut RenderLevelStageEvent<'_>) {
        if event.stage != RenderLevelStage::AfterOpaqueBlocks || !host.on(MOD_ID) {
            return;
        }
        let rt = self.lock();
        rt.shrubs.render(event.view.partial_tick, event.draw);
    }
}
