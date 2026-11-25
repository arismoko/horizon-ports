//! Weather particle spawner (sky, surface, block-face FX) and snow/sandstorm weather sounds,
//! matching `ParticleSpawner.java` and `ParticleRain.doAdditionalWeatherSounds`.

use crate::config::{
    EntryFilter, ParticlePreset, ParticleRainConfig, ParticleRenderType, ParticleStyle,
    Precipitation, SpawnPos, TintType,
};
use crate::particles::{
    ParticleSharedState, StreakOptions, WeatherParticleOptions, sprite_frame_index,
};
use crate::shrub::ShrubManager;
use crate::wind::get_wind;
use mod_api::{
    BiomeProperties, BlockShapeKind, BlockState, ClientHost, ColorResolver, Heightmap,
    HostBlockStates, LevelRef, ParticleKey, ParticleOptions, ParticleType, SoundPlay, SoundSource,
};
use std::collections::HashMap;
use std::f32::consts::TAU;

/// Vanilla Overworld default cloud height (used when `spawn_height_limit == 0`).
pub const DEFAULT_CLOUD_HEIGHT: i32 = 192;
const DEFAULT_SEA_LEVEL: i32 = 63;

/// Registered particle handles resolved during `Hooks::register`.
#[derive(Clone, Copy)]
pub struct ParticleHandles {
    pub weather_translucent: ParticleType<WeatherParticleOptions>,
    pub weather_opaque: ParticleType<WeatherParticleOptions>,
    pub streak: ParticleType<StreakOptions>,
    pub vanilla_rain: ParticleKey,
    pub vanilla_smoke: ParticleKey,
    pub vanilla_dripping_water: ParticleKey,
}

/// Fast deterministic XorShift64 RNG (no external crate dependency).
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Default for Rng {
    fn default() -> Self {
        Self(0x9E37_79B9_7F4A_7C15)
    }
}

impl Rng {
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    pub fn next_f32(&mut self) -> f32 {
        ((self.next_u64() >> 40) as f32) / ((1u32 << 24) as f32)
    }

    pub fn next_f64(&mut self) -> f64 {
        ((self.next_u64() >> 11) as f64) / ((1u64 << 53) as f64)
    }

    pub fn next_usize(&mut self, bound: usize) -> usize {
        if bound <= 1 {
            return 0;
        }
        (self.next_u64() as usize) % bound
    }

    pub fn next_i32_bound(&mut self, bound: i32) -> i32 {
        if bound <= 1 {
            return 0;
        }
        (self.next_u64() % (bound as u64)) as i32
    }

    /// Triangular distribution centered on `mode` with half-width `deviation` (`RandomSource.triangle`).
    pub fn triangle(&mut self, mode: f64, deviation: f64) -> f64 {
        mode + deviation * (self.next_f64() - self.next_f64())
    }
}

/// Mutable per-client spawner state matching `ParticleSpawner.java`.
#[derive(Default)]
pub struct ParticleSpawnerState {
    pub rng: Rng,
    pub after_weather_ticks_left: i32,
    pub was_raining: bool,
    pub spawn_attempts_until_block_fx_idle: i32,
    pub ticks_until_surface_fx_idle: i32,
    pub ticks_until_sky_fx_idle: i32,
    pub rain_sound_time: i32,
    height_cache: HashMap<(i32, i32), i32>,
    last_cache_tick: i64,
}

impl ParticleSpawnerState {
    pub fn reset(&mut self) {
        self.after_weather_ticks_left = 0;
        self.was_raining = false;
        self.spawn_attempts_until_block_fx_idle = 0;
        self.ticks_until_surface_fx_idle = 0;
        self.ticks_until_sky_fx_idle = 0;
        self.rain_sound_time = 0;
        self.height_cache.clear();
    }

    /// Compute sky particle density for the current tick (`ParticleSpawner.getSkyParticleDensity`).
    pub fn sky_particle_density(
        &self,
        config: &ParticleRainConfig,
        thunder_level: f32,
        speed: f32,
    ) -> i32 {
        let base = config.perf.particle_amount / 25;
        if self.ticks_until_sky_fx_idle <= 0 {
            4
        } else {
            let storm = (base as f32) * config.perf.particle_storm_density;
            let lerped = (base as f32) + thunder_level.clamp(0.0, 1.0) * (storm - (base as f32));
            (lerped * (1.0 + speed * 2.0)) as i32
        }
    }

    /// Main per-tick entry point called from `client_tick_post`.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        host: &mut dyn ClientHost,
        level: LevelRef,
        camera_pos: [f64; 3],
        camera_speed: f32,
        game_time: i64,
        rain_level: f32,
        thunder_level: f32,
        config: &ParticleRainConfig,
        handles: &ParticleHandles,
        shared: &ParticleSharedState,
        shrubs: &mut ShrubManager,
    ) {
        let is_raining = rain_level > 0.2;
        if is_raining != self.was_raining {
            self.was_raining = is_raining;
            self.after_weather_ticks_left = if is_raining {
                0
            } else {
                self.rng.next_i32_bound(6000)
            };
        }

        if game_time != self.last_cache_tick {
            if game_time % 80 == 0 {
                self.height_cache.clear();
            }
            self.last_cache_tick = game_time;
        }

        // Weather sounds (snow & sandstorm) run whenever it's raining, regardless of particle cap.
        if rain_level > 0.0 {
            self.tick_weather_sounds(host, level, camera_pos, rain_level, config);
        }

        let tracked = shared.total_tracked() + shrubs.count();
        if (tracked as i32) < config.perf.particle_amount {
            self.tick_sky_fx(
                host,
                level,
                camera_pos,
                camera_speed,
                game_time,
                rain_level,
                thunder_level,
                config,
                handles,
                shrubs,
            );
            self.tick_surface_fx(
                host,
                level,
                camera_pos,
                game_time,
                rain_level,
                thunder_level,
                config,
                handles,
                shrubs,
            );
            self.tick_block_fx(
                host,
                level,
                camera_pos,
                game_time,
                rain_level,
                thunder_level,
                config,
                handles,
                shrubs,
            );
        }

        if self.after_weather_ticks_left > 0 {
            self.after_weather_ticks_left -= 1;
        }
    }

    /// Cached surface height respecting `compat.weather_ignore_blocks` (`ParticleSpawner.getHeight`).
    pub fn get_height(
        &mut self,
        host: &dyn ClientHost,
        level: LevelRef,
        x: i32,
        z: i32,
        config: &ParticleRainConfig,
    ) -> Option<i32> {
        if config.compat.weather_ignore_blocks.entries.is_empty() {
            return host
                .client_surface_heights(&[(x, z)], Heightmap::MotionBlocking)
                .ok()
                .and_then(|v| v.into_iter().next().flatten());
        }

        if let Some(&cached) = self.height_cache.get(&(x, z)) {
            return Some(cached);
        }

        let (min_y, height) = level.height(host)?;
        let max_y = min_y + height;
        let mut y = host
            .client_surface_heights(&[(x, z)], Heightmap::MotionBlocking)
            .ok()
            .and_then(|v| v.into_iter().next().flatten())?;
        if y == 0 || y == -1 {
            y = max_y;
        }

        while y > min_y {
            let pos = [x, y, z];
            let Some(state) = level.block_state(host, pos) else {
                break;
            };
            let no_collision = level
                .block_shape(host, state, pos, BlockShapeKind::Collision)
                .is_none_or(|b| b.is_empty());
            let non_fluid = level.fluid_height(host, pos).unwrap_or(0.0) <= 0.0;
            if non_fluid && (no_collision || is_ignored(host, state, config)) {
                y -= 1;
            } else {
                break;
            }
        }

        let resolved = y + 1;
        self.height_cache.insert((x, z), resolved);
        Some(resolved)
    }

    #[allow(clippy::too_many_arguments)]
    fn tick_sky_fx(
        &mut self,
        host: &mut dyn ClientHost,
        level: LevelRef,
        camera_pos: [f64; 3],
        speed: f32,
        game_time: i64,
        rain_level: f32,
        thunder_level: f32,
        config: &ParticleRainConfig,
        handles: &ParticleHandles,
        shrubs: &mut ShrubManager,
    ) {
        let density = self.sky_particle_density(config, thunder_level, speed);
        if self.ticks_until_sky_fx_idle > 0 {
            self.ticks_until_sky_fx_idle -= 1;
        }
        let dist = config.perf.particle_distance as f32;

        for _ in 0..density.max(0) {
            let height = if speed < 0.8 {
                let a = self.rng.next_f32();
                let b = self.rng.next_f32();
                let h = 1.0 - (a * a - b * b).abs();
                h * (0.4 + 0.6)
            } else {
                self.rng.next_f32()
            };
            let theta = TAU * self.rng.next_f32();
            let phi = (2.0 * height - 1.0).clamp(-1.0, 1.0).acos();
            let x = (dist * phi.sin() * theta.cos()) as f64 + camera_pos[0];
            let mut y = (dist * phi.cos()) as f64 + camera_pos[1];
            let z = (dist * phi.sin() * theta.sin()) as f64 + camera_pos[2];

            let bx = x.floor() as i32;
            let mut by = y.floor() as i32;
            let bz = z.floor() as i32;

            if config.compat.do_spawn_height_limit {
                let cloud_h = if config.compat.spawn_height_limit == 0 {
                    DEFAULT_CLOUD_HEIGHT
                } else {
                    config.compat.spawn_height_limit
                };
                if cloud_h != 0 && by > cloud_h {
                    y = cloud_h as f64;
                    by = cloud_h;
                }
            }

            let Some(heightmap_y) = self.get_height(&*host, level, bx, bz, config) else {
                continue;
            };
            if heightmap_y >= by {
                continue;
            }
            let heightmap_pos = [bx, heightmap_y - 1, bz];
            let Some(ground_state) = level.block_state(&*host, heightmap_pos) else {
                continue;
            };
            let spawn_block_pos = [bx, by, bz];
            let Some(biome) = level.client_biome(&*host, spawn_block_pos) else {
                continue;
            };
            let temp_pos = if config.compat.use_heightmap_temp {
                heightmap_pos
            } else {
                spawn_block_pos
            };
            let precip = precipitation_at(&*host, level, &biome, temp_pos);
            let spawn_chance = self.rng.next_f32();

            for (idx, preset) in config.particles.iter().enumerate() {
                if preset.enabled
                    && preset.spawn_pos == SpawnPos::Sky
                    && preset.density > spawn_chance
                    && preset.weather.is_current(
                        rain_level,
                        thunder_level,
                        self.after_weather_ticks_left,
                    )
                    && preset.precipitation.contains(&precip)
                    && matches_biome_list(&*host, level, &biome, &preset.biome_list)
                    && matches_block_list(&*host, ground_state, &preset.block_list)
                {
                    self.spawn_preset(
                        host,
                        level,
                        idx,
                        preset,
                        [x, y, z],
                        spawn_block_pos,
                        ground_state,
                        &biome,
                        rain_level,
                        thunder_level,
                        game_time,
                        config,
                        handles,
                        shrubs,
                    );
                    self.ticks_until_sky_fx_idle = 100;
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn tick_surface_fx(
        &mut self,
        host: &mut dyn ClientHost,
        level: LevelRef,
        camera_pos: [f64; 3],
        game_time: i64,
        rain_level: f32,
        thunder_level: f32,
        config: &ParticleRainConfig,
        handles: &ParticleHandles,
        shrubs: &mut ShrubManager,
    ) {
        let density = if self.ticks_until_surface_fx_idle <= 0 {
            1
        } else {
            self.ticks_until_surface_fx_idle -= 1;
            config.perf.particle_amount / 200
        };
        let range = config.perf.surface_range as f64;

        for _ in 0..density.max(0) {
            let x = self.rng.triangle(camera_pos[0], range);
            let z = self.rng.triangle(camera_pos[2], range);
            let bx = x.floor() as i32;
            let bz = z.floor() as i32;
            let Some(hy) = self.get_height(&*host, level, bx, bz, config) else {
                continue;
            };
            let heightmap_pos = [bx, hy - 1, bz];
            let Some(ground_state) = level.block_state(&*host, heightmap_pos) else {
                continue;
            };
            let Some(biome) = level.client_biome(&*host, heightmap_pos) else {
                continue;
            };
            let precip = precipitation_at(&*host, level, &biome, heightmap_pos);
            let spawn_chance = self.rng.next_f32();
            let y = (hy as f64) + self.rng.next_f64() + 0.1;

            for (idx, preset) in config.particles.iter().enumerate() {
                if preset.enabled
                    && preset.spawn_pos == SpawnPos::WorldSurface
                    && preset.density > spawn_chance
                    && preset.weather.is_current(
                        rain_level,
                        thunder_level,
                        self.after_weather_ticks_left,
                    )
                    && preset.precipitation.contains(&precip)
                    && matches_biome_list(&*host, level, &biome, &preset.biome_list)
                    && matches_block_list(&*host, ground_state, &preset.block_list)
                {
                    self.spawn_preset(
                        host,
                        level,
                        idx,
                        preset,
                        [x, y, z],
                        heightmap_pos,
                        ground_state,
                        &biome,
                        rain_level,
                        thunder_level,
                        game_time,
                        config,
                        handles,
                        shrubs,
                    );
                    self.ticks_until_surface_fx_idle = 100;
                }
            }
        }
    }

    /// Block-surface FX sampler mirroring `ClientLevel.doAnimateTick` -> `ParticleSpawner.tickBlockFX`.
    #[allow(clippy::too_many_arguments)]
    fn tick_block_fx(
        &mut self,
        host: &mut dyn ClientHost,
        level: LevelRef,
        camera_pos: [f64; 3],
        game_time: i64,
        rain_level: f32,
        thunder_level: f32,
        config: &ParticleRainConfig,
        handles: &ParticleHandles,
        shrubs: &mut ShrubManager,
    ) {
        // Sample 64 random positions per tick across ranges 16 and 32 (idle-throttled like original).
        let samples = if self.spawn_attempts_until_block_fx_idle <= 0 {
            8
        } else {
            64
        };
        let cx = camera_pos[0].floor() as i32;
        let cy = camera_pos[1].floor() as i32;
        let cz = camera_pos[2].floor() as i32;

        for s in 0..samples {
            if self.spawn_attempts_until_block_fx_idle > 0 {
                self.spawn_attempts_until_block_fx_idle -= 1;
            }
            let range = if s % 2 == 0 { 16 } else { 32 };
            let sx = cx + self.rng.next_i32_bound(range) - self.rng.next_i32_bound(range);
            let sy = cy + self.rng.next_i32_bound(range) - self.rng.next_i32_bound(range);
            let sz = cz + self.rng.next_i32_bound(range) - self.rng.next_i32_bound(range);
            let source_pos = [sx, sy, sz];

            let Some(source_state) = level.block_state(&*host, source_pos) else {
                continue;
            };
            let source_has_collision = level
                .block_shape(&*host, source_state, source_pos, BlockShapeKind::Collision)
                .is_some_and(|b| !b.is_empty());
            if source_has_collision && !is_ignored(&*host, source_state, config) {
                continue;
            }
            let Some(biome) = level.client_biome(&*host, source_pos) else {
                continue;
            };
            let precip = precipitation_at(&*host, level, &biome, source_pos);

            for (idx, preset) in config.particles.iter().enumerate() {
                if !preset.enabled
                    || !preset.weather.is_current(
                        rain_level,
                        thunder_level,
                        self.after_weather_ticks_left,
                    )
                {
                    continue;
                }
                // Horizontal dir_2d: 0=South(+Z), 1=West(-X), 2=North(-Z), 3=East(+X)
                let (dir_2d, opposite_offset, is_top, is_bottom) = match preset.spawn_pos {
                    SpawnPos::BlockTop => (0u8, [0, -1, 0], true, false),
                    SpawnPos::BlockBottom => (0u8, [0, 1, 0], false, true),
                    SpawnPos::BlockSides => {
                        let d = self.rng.next_i32_bound(4) as u8;
                        let off = match d {
                            0 => [0, 0, -1], // South face -> block is North
                            1 => [1, 0, 0],  // West face -> block is East
                            2 => [0, 0, 1],  // North face -> block is South
                            _ => [-1, 0, 0], // East face -> block is West
                        };
                        (d, off, false, false)
                    }
                    _ => continue,
                };

                if !preset.precipitation.contains(&precip)
                    || preset.density <= self.rng.next_f32()
                    || !matches_biome_list(&*host, level, &biome, &preset.biome_list)
                {
                    continue;
                }

                let adj_pos = [
                    sx + opposite_offset[0],
                    sy + opposite_offset[1],
                    sz + opposite_offset[2],
                ];
                let Some(adj_state) = level.block_state(&*host, adj_pos) else {
                    continue;
                };
                let adj_boxes = level
                    .block_shape(&*host, adj_state, adj_pos, BlockShapeKind::Collision)
                    .unwrap_or_default();
                let adj_fluid_h = level.fluid_height(&*host, adj_pos).unwrap_or(0.0);
                if (adj_boxes.is_empty() && adj_fluid_h <= 0.0)
                    || is_ignored(&*host, source_state, config)
                {
                    continue;
                }
                if !matches_block_list(&*host, adj_state, &preset.block_list)
                    && !matches_block_list(&*host, source_state, &preset.block_list)
                {
                    continue;
                }
                if preset.needs_sky_access {
                    let Some(hy) = self.get_height(&*host, level, sx, sz, config) else {
                        continue;
                    };
                    if sy < hy {
                        continue;
                    }
                }

                let p1 = self.rng.next_f64();
                let p2 = self.rng.next_f64();
                let spawn_xyz = if is_top {
                    let max_y = adj_boxes
                        .iter()
                        .map(|b| b[4])
                        .fold(adj_fluid_h as f64, f64::max);
                    if max_y <= 0.0 {
                        continue;
                    }
                    [
                        adj_pos[0] as f64 + p2,
                        adj_pos[1] as f64 + max_y + 0.01,
                        adj_pos[2] as f64 + p1,
                    ]
                } else if is_bottom {
                    let min_y = adj_boxes.iter().map(|b| b[1]).fold(1.0f64, f64::min);
                    [
                        adj_pos[0] as f64 + p2,
                        adj_pos[1] as f64 + min_y - 0.01,
                        adj_pos[2] as f64 + p1,
                    ]
                } else {
                    match dir_2d {
                        0 => [
                            adj_pos[0] as f64 + p2,
                            adj_pos[1] as f64 + p1,
                            adj_pos[2] as f64 + 1.01,
                        ],
                        1 => [
                            adj_pos[0] as f64 - 0.01,
                            adj_pos[1] as f64 + p1,
                            adj_pos[2] as f64 + p2,
                        ],
                        2 => [
                            adj_pos[0] as f64 + p2,
                            adj_pos[1] as f64 + p1,
                            adj_pos[2] as f64 - 0.01,
                        ],
                        _ => [
                            adj_pos[0] as f64 + 1.01,
                            adj_pos[1] as f64 + p1,
                            adj_pos[2] as f64 + p2,
                        ],
                    }
                };

                if preset.style == ParticleStyle::Registered
                    && preset.registered_particle_id == "particlerain:streak"
                {
                    let rgb = if config.compat.water_tint {
                        compute_water_tint(&*host, level, adj_pos, &biome, config.compat.tint_mix)
                    } else {
                        [0.2, 0.3, 1.0]
                    };
                    let opts = handles.streak.options(StreakOptions {
                        dir_2d,
                        rgb,
                        opacity: config.streak.opacity,
                        size: config.streak.size,
                    });
                    host.add_particle(&opts, spawn_xyz, [0.0; 3]);
                } else {
                    self.spawn_preset(
                        host,
                        level,
                        idx,
                        preset,
                        spawn_xyz,
                        adj_pos,
                        adj_state,
                        &biome,
                        rain_level,
                        thunder_level,
                        game_time,
                        config,
                        handles,
                        shrubs,
                    );
                }
                self.spawn_attempts_until_block_fx_idle = 10000;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_preset(
        &mut self,
        host: &mut dyn ClientHost,
        level: LevelRef,
        preset_index: usize,
        preset: &ParticlePreset,
        pos: [f64; 3],
        block_pos: [i32; 3],
        ground_state: BlockState,
        biome: &str,
        rain_level: f32,
        thunder_level: f32,
        game_time: i64,
        config: &ParticleRainConfig,
        handles: &ParticleHandles,
        shrubs: &mut ShrubManager,
    ) {
        match preset.style {
            ParticleStyle::Custom => {
                let frame = if preset.sprite_locations.is_empty() {
                    sprite_frame_index("particlerain:new_custom_particle")
                } else {
                    let pick = self.rng.next_usize(preset.sprite_locations.len());
                    sprite_frame_index(&preset.sprite_locations[pick])
                };
                let rgb = compute_tint(
                    &*host,
                    level,
                    block_pos,
                    ground_state,
                    biome,
                    preset,
                    rain_level,
                    config,
                    &mut self.rng,
                );
                let opts_val = WeatherParticleOptions {
                    preset_index: preset_index as u16,
                    frame,
                    rgb,
                    size: preset.size,
                    lifetime: preset.lifetime,
                };
                let opts = match preset.render_type {
                    ParticleRenderType::Opaque => handles.weather_opaque.options(opts_val),
                    ParticleRenderType::Translucent | ParticleRenderType::Blended => {
                        handles.weather_translucent.options(opts_val)
                    }
                };
                host.add_particle(&opts, pos, [0.0; 3]);
            }
            ParticleStyle::Registered => {
                let key = match preset.registered_particle_id.as_str() {
                    "minecraft:rain" => Some(handles.vanilla_rain),
                    "minecraft:smoke" => Some(handles.vanilla_smoke),
                    _ => None,
                };
                if let Some(key) = key {
                    let wind_mul = if thunder_level > 0.9 {
                        preset.storm_wind_strength
                    } else {
                        preset.wind_strength
                    };
                    let vel = if wind_mul != 0.0 {
                        let w = get_wind(pos[0], pos[1], pos[2], game_time, &config.wind);
                        [(w[0] * wind_mul) as f64, 0.0, (w[2] * wind_mul) as f64]
                    } else {
                        [0.0; 3]
                    };
                    host.add_particle(&ParticleOptions::simple(key), pos, vel);
                }
            }
            ParticleStyle::BlockModel => {
                shrubs.spawn(pos, preset, game_time, thunder_level, &config.wind);
            }
        }
    }

    /// Play snow and sandstorm weather ambient sounds matching `ParticleRain.doAdditionalWeatherSounds`.
    fn tick_weather_sounds(
        &mut self,
        host: &dyn ClientHost,
        level: LevelRef,
        camera_pos: [f64; 3],
        rain_level: f32,
        config: &ParticleRainConfig,
    ) {
        if config.sound.snow_volume <= 0.0 && config.sound.wind_volume <= 0.0 {
            return;
        }
        let cam_bx = camera_pos[0].floor() as i32;
        let cam_by = camera_pos[1].floor() as i32;
        let cam_bz = camera_pos[2].floor() as i32;

        // Sample random column within 10 blocks, matching vanilla's tickRainParticles.
        let mut chosen: Option<[i32; 3]> = None;
        let attempts = ((100.0 * rain_level * rain_level) as i32).clamp(1, 100);
        let mut count = 0i32;
        for _ in 0..attempts.min(16) {
            let rx = cam_bx + self.rng.next_i32_bound(21) - 10;
            let rz = cam_bz + self.rng.next_i32_bound(21) - 10;
            let Some(ry) = self.get_height(host, level, rx, rz, config) else {
                continue;
            };
            if ry > cam_by - 10 && ry <= cam_by + 10 {
                count += 1;
                if self.rng.next_i32_bound(count) == 0 {
                    chosen = Some([rx, ry, rz]);
                }
            }
        }

        let Some(rain_pos) = chosen else {
            return;
        };
        if self.rng.next_i32_bound(3) >= self.rain_sound_time {
            self.rain_sound_time += 1;
            return;
        }
        self.rain_sound_time = 0;

        if config.compat.do_spawn_height_limit {
            let cloud_h = if config.compat.spawn_height_limit == 0 {
                DEFAULT_CLOUD_HEIGHT
            } else {
                config.compat.spawn_height_limit
            };
            if rain_pos[1] > cloud_h {
                return;
            }
        }

        let cam_surface = self
            .get_height(host, level, cam_bx, cam_bz, config)
            .unwrap_or(cam_by);
        let above = rain_pos[1] > cam_by + 1 && cam_surface > cam_by;
        let Some(biome) = level.client_biome(host, rain_pos) else {
            return;
        };
        let precip = precipitation_at(host, level, &biome, rain_pos);
        let pos_f64 = [
            rain_pos[0] as f64 + 0.5,
            rain_pos[1] as f64 + 0.5,
            rain_pos[2] as f64 + 0.5,
        ];

        if precip == Precipitation::Snow && config.sound.snow_volume > 0.0 {
            let sound_id = if above {
                "particlerain:weather.snow.above"
            } else {
                "particlerain:weather.snow"
            };
            let pitch = if above { 0.5 } else { 1.0 };
            let play = SoundPlay::at(sound_id, SoundSource::Weather, pos_f64)
                .with_volume(config.sound.snow_volume)
                .with_pitch(pitch);
            let _ = host.play_local_sound(&play, 0);
        } else if precip == Precipitation::None
            && config.sound.wind_volume > 0.0
            && level.biome_in_tag(host, &biome, "c:in_overworld") == Some(true)
            && level
                .biome_properties(host, &biome)
                .is_some_and(|p| p.temperature > 0.25)
        {
            let sound_id = if above {
                "particlerain:weather.sandstorm.above"
            } else {
                "particlerain:weather.sandstorm"
            };
            let pitch = if above { 0.5 } else { 1.0 };
            let play = SoundPlay::at(sound_id, SoundSource::Weather, pos_f64)
                .with_volume(config.sound.wind_volume)
                .with_pitch(pitch);
            let _ = host.play_local_sound(&play, 0);
        }
    }
}

/// Vanilla height-adjusted precipitation check (`Biome.getPrecipitationAt(pos, seaLevel)`).
pub fn precipitation_from_properties(
    props: BiomeProperties,
    y: i32,
    sea_level: i32,
) -> Precipitation {
    if !props.has_precipitation {
        return Precipitation::None;
    }
    let adjusted_temp = if y > sea_level + 17 {
        props.temperature - ((y - (sea_level + 17)) as f32) * 0.00125
    } else {
        props.temperature
    };
    if adjusted_temp < 0.15 {
        Precipitation::Snow
    } else {
        Precipitation::Rain
    }
}

pub fn precipitation_at(
    host: &dyn ClientHost,
    level: LevelRef,
    biome: &str,
    pos: [i32; 3],
) -> Precipitation {
    let Some(props) = level.biome_properties(host, biome) else {
        return Precipitation::None;
    };
    precipitation_from_properties(props, pos[1], DEFAULT_SEA_LEVEL)
}

pub fn is_ignored(host: &dyn ClientHost, state: BlockState, config: &ParticleRainConfig) -> bool {
    matches_block_list(host, state, &config.compat.weather_ignore_blocks)
}

pub fn matches_block_list(host: &dyn ClientHost, state: BlockState, filter: &EntryFilter) -> bool {
    let key = state.block();
    let name = key.id(host).unwrap_or("");
    filter.matches(
        |id| id == name,
        |tag| host.block_in_tag(key, tag) == Some(true),
    )
}

pub fn matches_biome_list(
    host: &dyn ClientHost,
    level: LevelRef,
    biome: &str,
    filter: &EntryFilter,
) -> bool {
    filter.matches(
        |id| id == biome,
        |tag| level.biome_in_tag(host, biome, tag) == Some(true),
    )
}

fn unpack_rgb(hex: u32) -> [f32; 3] {
    [
        (((hex >> 16) & 0xFF) as f32) / 255.0,
        (((hex >> 8) & 0xFF) as f32) / 255.0,
        ((hex & 0xFF) as f32) / 255.0,
    ]
}

fn compute_water_tint(
    host: &dyn ClientHost,
    level: LevelRef,
    pos: [i32; 3],
    biome: &str,
    tint_mix: f32,
) -> [f32; 3] {
    let water = host
        .biome_tint(pos, ColorResolver::Water)
        .ok()
        .or_else(|| level.biome_effects(host, biome).map(|e| e.water))
        .unwrap_or(0x3F76E4);
    let fog = level
        .biome_effects(host, biome)
        .and_then(|e| e.fog)
        .unwrap_or(0xC0D8FF);
    let w = unpack_rgb(water);
    let f = unpack_rgb(fog);
    let t = tint_mix.clamp(0.0, 1.0);
    [
        w[0] + (f[0] - w[0]) * t,
        w[1] + (f[1] - w[1]) * t,
        w[2] + (f[2] - w[2]) * t,
    ]
}

#[allow(clippy::too_many_arguments)]
fn compute_tint(
    host: &dyn ClientHost,
    level: LevelRef,
    pos: [i32; 3],
    ground_state: BlockState,
    biome: &str,
    preset: &ParticlePreset,
    rain_level: f32,
    config: &ParticleRainConfig,
    rng: &mut Rng,
) -> [f32; 3] {
    match preset.tint_type {
        TintType::Water => {
            if !config.compat.water_tint && (preset.id == "rain" || preset.id == "heavy_rain") {
                [1.0, 1.0, 1.0]
            } else {
                compute_water_tint(host, level, pos, biome, config.compat.tint_mix)
            }
        }
        TintType::Fog => {
            let fog = level
                .biome_effects(host, biome)
                .and_then(|e| e.fog)
                .unwrap_or(0xC0D8FF);
            let rgb = unpack_rgb(fog);
            let mul = 0.7 + (1.0 - rain_level.clamp(0.0, 1.0)) * 0.2;
            [rgb[0] * mul, rgb[1] * mul, rgb[2] * mul]
        }
        TintType::Map => {
            let states = host.block_states();
            let rgba = states
                .map_color(ground_state)
                .and_then(|id| states.map_color_rgba(id.saturating_mul(4).saturating_add(2)))
                .unwrap_or([219, 211, 160, 255]);
            let variance = rng.next_f32() * 0.2 + 0.8;
            [
                (rgba[0] as f32 / 255.0) * variance,
                (rgba[1] as f32 / 255.0) * variance,
                (rgba[2] as f32 / 255.0) * variance,
            ]
        }
        TintType::Custom => [
            preset.custom_tint[0] as f32 / 255.0,
            preset.custom_tint[1] as f32 / 255.0,
            preset.custom_tint[2] as f32 / 255.0,
        ],
        TintType::None => [1.0, 1.0, 1.0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precipitation_switches_to_snow_at_high_altitude() {
        let hills = BiomeProperties {
            temperature: 0.2,
            downfall: 0.3,
            has_precipitation: true,
            frozen: false,
        };
        assert_eq!(
            precipitation_from_properties(hills, 64, 63),
            Precipitation::Rain
        );
        assert_eq!(
            precipitation_from_properties(hills, 130, 63),
            Precipitation::Snow
        );
    }

    #[test]
    fn desert_has_no_precipitation() {
        let desert = BiomeProperties {
            temperature: 2.0,
            downfall: 0.0,
            has_precipitation: false,
            frozen: false,
        };
        assert_eq!(
            precipitation_from_properties(desert, 64, 63),
            Precipitation::None
        );
    }

    #[test]
    fn sky_density_scales_with_storm_and_speed() {
        let mut spawner = ParticleSpawnerState::default();
        let cfg = ParticleRainConfig::default();
        assert_eq!(spawner.sky_particle_density(&cfg, 0.0, 0.0), 4); // idle
        spawner.ticks_until_sky_fx_idle = 50;
        assert_eq!(spawner.sky_particle_density(&cfg, 0.0, 0.0), 60);
        assert_eq!(spawner.sky_particle_density(&cfg, 1.0, 0.0), 90);
        assert_eq!(spawner.sky_particle_density(&cfg, 0.0, 0.5), 120);
    }
}
