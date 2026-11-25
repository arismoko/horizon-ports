//! Custom weather particle providers (`particlerain:weather`, `particlerain:weather_opaque`,
//! and `particlerain:streak`) implementing Sunrise's `ParticleProvider` trait.

use crate::config::{
    FadeType, ParticlePreset, ParticleRainConfig, ParticleRenderType, RotationType, SpawnPos,
    WeatherCondition,
};
use crate::wind::{
    WindOptions, get_wind, horizontal_quad_rotation, pack_argb, pitch_angle_to_camera,
    quat_from_axis_angle, quat_mul, turn_backface_flipways, velocity_quad_rotation,
    vertical_quad_rotation,
};
use mod_api::{
    BlockKey, Compound, ModParticle, ParticleBatch, ParticleBlend, ParticleExtractView,
    ParticleLevelView, ParticleMapCodec, ParticleProvider, ParticleQuad, ParticleStreamCodec,
    QuadParticleRenderState, RandomSource, Wire,
};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
use std::sync::{Arc, Mutex};

/// Ordered sprite table matching `assets/particlerain/particles/weather.json`
/// and `weather_opaque.json`.
pub const WEATHER_SPRITES: &[&str] = &[
    "particlerain:rain_0",
    "particlerain:rain_1",
    "particlerain:rain_2",
    "particlerain:rain_3",
    "particlerain:rain_4",
    "particlerain:rain_5",
    "particlerain:rain_6",
    "particlerain:rain_7",
    "particlerain:rain_8",
    "particlerain:rain_9",
    "particlerain:rain_10",
    "particlerain:rain_11",
    "particlerain:rain_12",
    "particlerain:rain_13",
    "particlerain:rain_14",
    "particlerain:rain_15",
    "particlerain:heavy_rain_0",
    "particlerain:heavy_rain_1",
    "particlerain:heavy_rain_2",
    "particlerain:heavy_rain_3",
    "particlerain:snow_0",
    "particlerain:snow_1",
    "particlerain:snow_2",
    "particlerain:snow_3",
    "particlerain:snow_4",
    "particlerain:snow_5",
    "particlerain:snow_6",
    "particlerain:snow_7",
    "particlerain:snow_8",
    "particlerain:snow_9",
    "particlerain:snow_10",
    "particlerain:snow_11",
    "particlerain:snow_12",
    "particlerain:snow_13",
    "particlerain:snow_14",
    "particlerain:snow_15",
    "particlerain:heavy_snow_0",
    "particlerain:heavy_snow_1",
    "particlerain:heavy_snow_2",
    "particlerain:heavy_snow_3",
    "particlerain:ripple_0",
    "particlerain:ripple_1",
    "particlerain:ripple_2",
    "particlerain:ripple_3",
    "particlerain:ripple_4",
    "particlerain:ripple_5",
    "particlerain:ripple_6",
    "particlerain:ripple_7",
    "minecraft:generic_0",
    "minecraft:generic_1",
    "minecraft:generic_2",
    "particlerain:fog_translucent",
    "particlerain:horizontal_fog",
    "particlerain:fog_dithered",
    "particlerain:new_custom_particle",
];

/// Look up a sprite's frame index in `WEATHER_SPRITES`.
pub fn sprite_frame_index(name: &str) -> u32 {
    WEATHER_SPRITES
        .iter()
        .position(|&s| s == name)
        .unwrap_or(WEATHER_SPRITES.len() - 1) as u32
}

/// Typed spawn options for `particlerain:weather` and `particlerain:weather_opaque`.
#[derive(Clone, Debug, PartialEq, Wire)]
pub struct WeatherParticleOptions {
    pub preset_index: u16,
    pub frame: u32,
    pub rgb: [f32; 3],
    pub size: f32,
    pub lifetime: i32,
}

impl WeatherParticleOptions {
    pub fn map_codec() -> ParticleMapCodec<Self> {
        ParticleMapCodec::new(
            |_, _| {
                Ok(Self {
                    preset_index: 0,
                    frame: 0,
                    rgb: [1.0, 1.0, 1.0],
                    size: 0.5,
                    lifetime: 100,
                })
            },
            |_, _| Ok(Compound::new()),
        )
    }

    pub fn stream_codec() -> ParticleStreamCodec<Self> {
        ParticleStreamCodec::wire()
    }
}

/// Typed spawn options for `particlerain:streak`.
#[derive(Clone, Debug, PartialEq, Wire)]
pub struct StreakOptions {
    /// Horizontal direction index: 0 = South, 1 = West, 2 = North, 3 = East (matching `Direction.get2DDataValue`).
    pub dir_2d: u8,
    pub rgb: [f32; 3],
    pub opacity: f32,
    pub size: f32,
}

impl StreakOptions {
    pub fn map_codec() -> ParticleMapCodec<Self> {
        ParticleMapCodec::new(
            |_, _| {
                Ok(Self {
                    dir_2d: 0,
                    rgb: [0.2, 0.3, 1.0],
                    opacity: 0.9,
                    size: 0.5,
                })
            },
            |_, _| Ok(Compound::new()),
        )
    }

    pub fn stream_codec() -> ParticleStreamCodec<Self> {
        ParticleStreamCodec::wire()
    }
}

/// Per-preset runtime snapshot read by `WeatherParticleProvider::tick` and `extract`.
#[derive(Clone, Debug)]
pub struct PresetSnapshot {
    pub weather: WeatherCondition,
    pub spawn_pos: SpawnPos,
    pub wind_strength: f32,
    pub storm_wind_strength: f32,
    pub gravity: f32,
    pub rotation_amount: f32,
    pub bounciness: f32,
    pub opacity: f32,
    pub fade_type: FadeType,
    pub size: f32,
    pub distance_scaling: f32,
    pub render_type: ParticleRenderType,
    pub animate_sprites: bool,
    pub sprite_frames: Vec<u32>,
    pub rotation_type: RotationType,
}

impl PresetSnapshot {
    pub fn from_preset(p: &ParticlePreset) -> Self {
        let sprite_frames = if p.sprite_locations.is_empty() {
            vec![sprite_frame_index("particlerain:new_custom_particle")]
        } else {
            p.sprite_locations
                .iter()
                .map(|s| sprite_frame_index(s))
                .collect()
        };
        Self {
            weather: p.weather,
            spawn_pos: p.spawn_pos,
            wind_strength: p.wind_strength,
            storm_wind_strength: p.storm_wind_strength,
            gravity: p.gravity,
            rotation_amount: p.rotation_amount,
            bounciness: p.bounciness,
            opacity: p.opacity,
            fade_type: p.fade_type,
            size: p.size,
            distance_scaling: p.distance_scaling,
            render_type: p.render_type,
            animate_sprites: p.animate_sprites,
            sprite_frames,
            rotation_type: p.rotation_type,
        }
    }
}

/// Shared state bridged between `client_tick_post` (which has `ClientHost`) and
/// `ParticleProvider` (which ticks inside the render crate with `ParticleLevelView`).
#[derive(Clone, Debug)]
pub struct ParticleSharedState {
    pub game_time: i64,
    pub rain_level: f32,
    pub thunder_level: f32,
    pub camera_pos: [f64; 3],
    pub camera_vel: [f64; 3],
    pub fov: f32,
    pub wind: WindOptions,
    pub particle_distance: i32,
    pub surface_range: i32,
    pub presets: Vec<PresetSnapshot>,
    pub air_keys: Vec<BlockKey>,
    pub fluid_keys: Vec<BlockKey>,
    pub tracked_translucent: usize,
    pub tracked_opaque: usize,
    pub tracked_streaks: usize,
    pub pending_drips: Vec<[f64; 3]>,
}

impl Default for ParticleSharedState {
    fn default() -> Self {
        let cfg = ParticleRainConfig::default();
        Self {
            game_time: 0,
            rain_level: 0.0,
            thunder_level: 0.0,
            camera_pos: [0.0; 3],
            camera_vel: [0.0; 3],
            fov: 70.0,
            wind: cfg.wind,
            particle_distance: cfg.perf.particle_distance,
            surface_range: cfg.perf.surface_range,
            presets: cfg
                .particles
                .iter()
                .map(PresetSnapshot::from_preset)
                .collect(),
            air_keys: Vec::new(),
            fluid_keys: Vec::new(),
            tracked_translucent: 0,
            tracked_opaque: 0,
            tracked_streaks: 0,
            pending_drips: Vec::new(),
        }
    }
}

impl ParticleSharedState {
    pub fn sync_config(&mut self, cfg: &ParticleRainConfig) {
        self.wind = cfg.wind;
        self.particle_distance = cfg.perf.particle_distance;
        self.surface_range = cfg.perf.surface_range;
        self.presets = cfg
            .particles
            .iter()
            .map(PresetSnapshot::from_preset)
            .collect();
    }

    pub fn total_tracked(&self) -> usize {
        self.tracked_translucent + self.tracked_opaque + self.tracked_streaks
    }

    fn is_non_solid(&self, key: BlockKey) -> bool {
        self.air_keys.contains(&key)
    }

    fn is_fluid(&self, key: BlockKey) -> bool {
        self.fluid_keys.contains(&key)
    }
}

/// Particle provider for custom weather particles (`CustomParticle.java`).
///
/// `ModParticle::state` stores four clean finite numbers per particle:
/// - `state[0]`: `preset_index as f64`
/// - `state[1]`: `rotation_variation as f64`
/// - `state[2]`: collision animation progress (`2.0` = not colliding, `0.0..=1.0` = collapsing)
/// - `state[3]`: `edge_bounces as f64`
pub struct WeatherParticleProvider {
    shared: Arc<Mutex<ParticleSharedState>>,
    blend: ParticleBlend,
}

impl WeatherParticleProvider {
    pub fn new(shared: Arc<Mutex<ParticleSharedState>>, blend: ParticleBlend) -> Self {
        Self { shared, blend }
    }
}

impl ParticleProvider<WeatherParticleOptions> for WeatherParticleProvider {
    fn create_particle(
        &self,
        options: &WeatherParticleOptions,
        _level: &dyn ParticleLevelView,
        position: [f64; 3],
        _aux: [f64; 3],
        random: &mut dyn RandomSource,
        particle: &mut ModParticle,
    ) -> Option<()> {
        let guard = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        let preset = guard.presets.get(options.preset_index as usize)?;

        let is_thundering = guard.thunder_level > 0.9;
        let wind_mul = if is_thundering {
            preset.storm_wind_strength
        } else {
            preset.wind_strength
        };
        let spawn_wind = if wind_mul != 0.0 {
            let w = get_wind(
                position[0],
                position[1],
                position[2],
                guard.game_time,
                &guard.wind,
            );
            [
                (w[0] * wind_mul * 10.0) as f64,
                0.0,
                (w[2] * wind_mul * 10.0) as f64,
            ]
        } else {
            [0.0; 3]
        };

        let yd = if preset.spawn_pos == SpawnPos::Sky {
            -(preset.gravity as f64)
        } else {
            preset.bounciness as f64
        };

        particle.position = position;
        particle.previous = position;
        particle.velocity = [spawn_wind[0], yd, spawn_wind[2]];
        particle.lifetime = options.lifetime.max(1);
        particle.frame = options.frame;
        particle.rgba = [options.rgb[0], options.rgb[1], options.rgb[2], 0.0];
        particle.size = if preset.distance_scaling != 0.0 {
            let dist = distance_3d(position, guard.camera_pos);
            distance_size(options.size, dist, preset.distance_scaling, guard.fov)
        } else {
            options.size
        };

        if preset.rotation_type == RotationType::Horizontal {
            particle.roll = FRAC_PI_2 * (random.next_int_bound(4) as f32);
        } else {
            particle.roll = 0.0;
        }

        let rot_var = preset.rotation_amount * ((random.next_float() - 0.5) * 2.0);
        particle.state = [
            options.preset_index as f64,
            rot_var as f64,
            2.0, // not colliding
            0.0, // edge_bounces
        ];
        Some(())
    }

    fn tick(&self, particles: &mut ParticleBatch, level: &dyn ParticleLevelView) {
        let snapshot = {
            let mut guard = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            match self.blend {
                ParticleBlend::Opaque => guard.tracked_opaque = particles.len(),
                ParticleBlend::Translucent => guard.tracked_translucent = particles.len(),
            }
            guard.clone()
        };

        let is_thundering = snapshot.thunder_level > 0.9;
        for p in particles.iter_mut() {
            let preset_idx = p.state[0] as usize;
            let Some(preset) = snapshot.presets.get(preset_idx) else {
                p.removed = true;
                continue;
            };

            // Apply gravity & wind
            p.velocity[1] -= 0.04 * (preset.gravity as f64);
            let wind_mul = if is_thundering {
                preset.storm_wind_strength
            } else {
                preset.wind_strength
            };
            if wind_mul != 0.0 {
                let w = get_wind(
                    p.position[0],
                    p.position[1],
                    p.position[2],
                    snapshot.game_time,
                    &snapshot.wind,
                );
                p.velocity[0] += (w[0] * wind_mul) as f64;
                p.velocity[2] += (w[2] * wind_mul) as f64;
            }

            // Move particle (no vanilla physics drag override)
            p.position[0] += p.velocity[0];
            p.position[1] += p.velocity[1];
            p.position[2] += p.velocity[2];
            p.velocity[0] *= 0.98;
            p.velocity[1] *= 0.98;
            p.velocity[2] *= 0.98;

            let dist = distance_3d(p.position, snapshot.camera_pos);
            let render_dist = match preset.spawn_pos {
                SpawnPos::WorldSurface => snapshot.surface_range as f32,
                _ => snapshot.particle_distance as f32,
            };
            if dist > render_dist + 0.5 {
                p.removed = true;
                continue;
            }

            let speed = (p.velocity[0] * p.velocity[0]
                + p.velocity[1] * p.velocity[1]
                + p.velocity[2] * p.velocity[2])
                .sqrt() as f32;

            // Fading
            p.rgba[3] = compute_alpha(
                p.age,
                p.lifetime,
                dist,
                render_dist,
                preset,
                p.position,
                snapshot.camera_pos,
                snapshot.rain_level,
                snapshot.thunder_level,
            );

            // Distance scaling
            let colliding = p.state[2] <= 1.0;
            if preset.distance_scaling != 0.0 && !colliding {
                p.size = distance_size(preset.size, dist, preset.distance_scaling, snapshot.fov);
            }

            // Roll rotation
            if preset.rotation_amount != 0.0 {
                p.roll += (p.state[1] as f32) * speed;
            }

            // Animated sprites (e.g. ripple_0..7)
            if preset.animate_sprites && preset.sprite_frames.len() > 1 {
                let len = preset.sprite_frames.len() - 1;
                let idx = ((p.age - 1).max(0) as usize) % len;
                p.frame = preset.sprite_frames[idx];
            }

            // Block collision check
            let bx = p.position[0].floor() as i32;
            let by = p.position[1].floor() as i32;
            let bz = p.position[2].floor() as i32;
            let state_here = level.block_state([bx, by, bz]);
            let key_here = state_here.block();
            if !snapshot.is_non_solid(key_here) && !colliding {
                if !snapshot.is_fluid(key_here) && preset.bounciness > 0.0 {
                    let bounce = (preset.bounciness * speed * 6.0) as f64;
                    let prev_by = p.previous[1].floor() as i32;
                    if prev_by > by {
                        // Hit top of block
                        p.velocity[1] = p.velocity[1].abs() + bounce;
                        p.position[1] = (by + 1) as f64 + 0.01;
                    } else {
                        // Hit side of block
                        let bounces = p.state[3] as i32;
                        if bounces >= 3 {
                            p.state[2] = 1.0;
                        } else {
                            p.state[3] = (bounces + 1) as f64;
                            p.velocity[1] += 0.5 * bounce;
                            p.velocity[0] = -p.velocity[0] * 0.5;
                            p.velocity[2] = -p.velocity[2] * 0.5;
                        }
                    }
                } else {
                    p.state[2] = 1.0;
                }
            }

            // Collision shrink/collapse animation
            if p.state[2] <= 1.0 {
                p.state[2] -= speed.max(0.05) as f64;
                if preset.rotation_type != RotationType::RelativeVelocity {
                    p.size = (p.size - speed).max(0.0);
                }
                if p.state[2] <= 0.0 {
                    p.removed = true;
                }
            }
        }
    }

    fn extract(
        &self,
        particle: &ModParticle,
        view: &ParticleExtractView,
        output: &mut dyn QuadParticleRenderState,
    ) {
        let (rotation_type, cam_vel) = {
            let guard = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            let preset_idx = particle.state[0] as usize;
            let rot = guard
                .presets
                .get(preset_idx)
                .map_or(RotationType::CopyCamera, |p| p.rotation_type);
            (rot, guard.camera_vel)
        };

        let pt = view.partial_tick as f64;
        let wx = particle.previous[0] + (particle.position[0] - particle.previous[0]) * pt;
        let wy = particle.previous[1] + (particle.position[1] - particle.previous[1]) * pt;
        let wz = particle.previous[2] + (particle.position[2] - particle.previous[2]) * pt;
        let center = [
            (wx - view.camera[0]) as f32,
            (wy - view.camera[1]) as f32,
            (wz - view.camera[2]) as f32,
        ];
        let roll = view.previous_roll + (particle.roll - view.previous_roll) * view.partial_tick;

        let rotation = match rotation_type {
            RotationType::CopyCamera => {
                if roll != 0.0 {
                    quat_mul(view.rotation, quat_from_axis_angle([0.0, 0.0, 1.0], roll))
                } else {
                    view.rotation
                }
            }
            RotationType::RelativeVelocity => {
                let rel = [
                    (particle.velocity[0] - cam_vel[0]) as f32,
                    (particle.velocity[1] - cam_vel[1]) as f32,
                    (particle.velocity[2] - cam_vel[2]) as f32,
                ];
                velocity_quad_rotation(rel, center)
            }
            RotationType::WorldVelocity => {
                let vel = [
                    particle.velocity[0] as f32,
                    particle.velocity[1] as f32,
                    particle.velocity[2] as f32,
                ];
                velocity_quad_rotation(vel, center)
            }
            RotationType::Vertical => vertical_quad_rotation(roll, center),
            RotationType::Horizontal => horizontal_quad_rotation(roll, center),
        };

        let mut quad = ParticleQuad::default();
        quad.layer = self.blend;
        quad.center = center;
        quad.rotation = rotation;
        quad.half_size = particle.size;
        quad.frame = particle.frame;
        quad.color = pack_argb(particle.rgba);
        quad.light_coords = view.light_coords;
        output.add(quad);
    }
}

/// Particle provider for `particlerain:streak` (`StreakParticle.java`).
///
/// `ModParticle::state`:
/// - `state[0]`: `dir_2d` (0=South, 1=West, 2=North, 3=East)
/// - `state[1]`: current gravity (randomized every 10 ticks)
/// - `state[2]`: `1.0` if fading out (`doCollisionAnim`), `0.0` otherwise
pub struct StreakParticleProvider {
    shared: Arc<Mutex<ParticleSharedState>>,
}

impl StreakParticleProvider {
    pub fn new(shared: Arc<Mutex<ParticleSharedState>>) -> Self {
        Self { shared }
    }
}

impl ParticleProvider<StreakOptions> for StreakParticleProvider {
    fn create_particle(
        &self,
        options: &StreakOptions,
        _level: &dyn ParticleLevelView,
        position: [f64; 3],
        _aux: [f64; 3],
        _random: &mut dyn RandomSource,
        particle: &mut ModParticle,
    ) -> Option<()> {
        let dir = (options.dir_2d % 4) as f32;
        particle.position = position;
        particle.previous = position;
        particle.velocity = [0.0, -0.1, 0.0];
        particle.rgba = [
            options.rgb[0],
            options.rgb[1],
            options.rgb[2],
            options.opacity,
        ];
        particle.size = options.size;
        particle.lifetime = 200;
        particle.frame = 0;
        particle.roll = dir * FRAC_PI_2;
        particle.state = [dir as f64, 0.0, 0.0, 0.0];
        Some(())
    }

    fn tick(&self, particles: &mut ParticleBatch, level: &dyn ParticleLevelView) {
        let air_keys = {
            let mut guard = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            guard.tracked_streaks = particles.len();
            guard.air_keys.clone()
        };
        let is_air = |k: BlockKey| air_keys.contains(&k);
        let mut new_drips = Vec::new();

        for (idx, p) in particles.iter_mut().enumerate() {
            let dir_2d = (p.state[0] as u8) % 4;
            let mut gravity = p.state[1];
            let mut fading = p.state[2] > 0.5;

            if p.age % 10 == 0 {
                // Deterministic pseudo-random per streak tick
                let hash = ((p.age as u64).wrapping_mul(2654435761)
                    ^ (idx as u64).wrapping_mul(2246822519))
                    >> 16;
                if (hash & 1) == 0 {
                    gravity = ((hash % 100) as f64) / 1000.0;
                } else {
                    gravity = 0.0;
                }
                p.state[1] = gravity;
            }

            if !fading {
                p.velocity[1] -= 0.04 * gravity;
                p.position[1] += p.velocity[1];
                p.velocity[1] *= 0.98;

                // Check ground below
                let below_pos = [
                    p.position[0].floor() as i32,
                    (p.position[1] - 0.05).floor() as i32,
                    p.position[2].floor() as i32,
                ];
                if !is_air(level.block_state(below_pos).block()) {
                    fading = true;
                }

                // Check block behind streak (opposite of face direction)
                let (ox, oz) = match dir_2d {
                    0 => (0.0, -0.06), // South -> behind is North (-Z)
                    1 => (0.06, 0.0),  // West -> behind is East (+X)
                    2 => (0.0, 0.06),  // North -> behind is South (+Z)
                    _ => (-0.06, 0.0), // East -> behind is West (-X)
                };
                let behind_pos = [
                    (p.position[0] + ox).floor() as i32,
                    p.position[1].floor() as i32,
                    (p.position[2] + oz).floor() as i32,
                ];
                if is_air(level.block_state(behind_pos).block()) && !fading {
                    new_drips.push([p.position[0], p.position[1] - 0.05, p.position[2]]);
                    fading = true;
                }
            }

            if fading {
                p.state[2] = 1.0;
                p.velocity[1] = 0.0;
                p.rgba[3] -= 0.1;
                if p.rgba[3] <= 0.0 {
                    p.removed = true;
                }
            }
        }

        if !new_drips.is_empty() {
            let mut guard = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            if guard.pending_drips.len() < 128 {
                guard.pending_drips.extend(new_drips);
            }
        }
    }

    fn extract(
        &self,
        particle: &ModParticle,
        view: &ParticleExtractView,
        output: &mut dyn QuadParticleRenderState,
    ) {
        let pt = view.partial_tick as f64;
        let wx = particle.previous[0] + (particle.position[0] - particle.previous[0]) * pt;
        let wy = particle.previous[1] + (particle.position[1] - particle.previous[1]) * pt;
        let wz = particle.previous[2] + (particle.position[2] - particle.previous[2]) * pt;
        let center = [
            (wx - view.camera[0]) as f32,
            (wy - view.camera[1]) as f32 + particle.size,
            (wz - view.camera[2]) as f32,
        ];
        let q = quat_from_axis_angle([0.0, 1.0, 0.0], particle.roll);
        let rotation = turn_backface_flipways(q, center);

        let mut quad = ParticleQuad::default();
        quad.layer = ParticleBlend::Translucent;
        quad.center = center;
        quad.rotation = rotation;
        quad.half_size = particle.size;
        quad.frame = 0;
        quad.color = pack_argb(particle.rgba);
        quad.light_coords = view.light_coords;
        output.add(quad);
    }
}

pub fn distance_3d(a: [f64; 3], b: [f64; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt() as f32
}

/// Distance-based quad scaling (`CustomParticle.getDistanceSize`).
pub fn distance_size(base_size: f32, distance: f32, distance_scaling: f32, fov: f32) -> f32 {
    let size = base_size + base_size * distance * distance_scaling;
    if (distance_scaling - 1.0).abs() < 1e-6 {
        let max_fov = 70.0f32;
        size * (fov.min(max_fov) / max_fov)
    } else {
        size
    }
}

#[allow(clippy::too_many_arguments)]
pub fn compute_alpha(
    age: i32,
    lifetime: i32,
    distance: f32,
    render_distance: f32,
    preset: &PresetSnapshot,
    pos: [f64; 3],
    cam: [f64; 3],
    rain_level: f32,
    thunder_level: f32,
) -> f32 {
    let half_life = (lifetime / 2).max(1) as f32;
    let mut alpha = match preset.fade_type {
        FadeType::InAndOut => {
            let factor = if (age as f32) < half_life {
                (age as f32) / half_life
            } else {
                ((lifetime - age).max(0) as f32) / half_life
            };
            factor * preset.opacity
        }
        FadeType::Out => (((lifetime - age).max(0) as f32) / half_life) * preset.opacity,
        FadeType::Distance => {
            let falloff = 3.0f32;
            let falloff_dist = (render_distance / falloff).max(0.001);
            let t = (((distance + falloff_dist) / falloff_dist) - falloff).clamp(0.0, 1.0);
            preset.opacity * (1.0 - t)
        }
    };

    match preset.rotation_type {
        RotationType::Vertical => {
            let angle = pitch_angle_to_camera(pos, cam);
            let pitch_alpha = 1.0 - (angle.min(FRAC_PI_4) / FRAC_PI_4);
            alpha = alpha.min(pitch_alpha);
        }
        RotationType::Horizontal => {
            let angle = pitch_angle_to_camera(pos, cam);
            let pitch_alpha = angle.min(0.4) / 0.4;
            alpha = alpha.min(pitch_alpha);
        }
        _ => {}
    }

    alpha *= preset.weather.weather_level(rain_level, thunder_level);
    alpha.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::default_presets;

    #[test]
    fn sprite_index_lookup_matches_weather_json() {
        assert_eq!(sprite_frame_index("particlerain:rain_0"), 0);
        assert_eq!(sprite_frame_index("particlerain:heavy_rain_0"), 16);
        assert_eq!(sprite_frame_index("particlerain:snow_0"), 20);
        assert_eq!(sprite_frame_index("particlerain:ripple_0"), 40);
        assert_eq!(sprite_frame_index("minecraft:generic_0"), 48);
        assert_eq!(sprite_frame_index("particlerain:fog_translucent"), 51);
    }

    #[test]
    fn distance_size_scales_with_fov_at_unity() {
        let s = distance_size(0.4, 10.0, 1.0, 70.0);
        assert!((s - 4.4).abs() < 1e-4);
        let zoomed = distance_size(0.4, 10.0, 1.0, 35.0);
        assert!((zoomed - 2.2).abs() < 1e-4);
    }

    #[test]
    fn fade_in_and_out_peaks_at_half_life() {
        let presets = default_presets();
        let fog = PresetSnapshot::from_preset(
            presets.iter().find(|p| p.id == "fog").expect("fog preset"),
        );
        let mid = compute_alpha(
            125,
            250,
            5.0,
            100.0,
            &fog,
            [0.0, 64.0, 0.0],
            [0.0, 74.0, 0.0],
            1.0,
            0.0,
        );
        let start = compute_alpha(
            0,
            250,
            5.0,
            100.0,
            &fog,
            [0.0, 64.0, 0.0],
            [0.0, 74.0, 0.0],
            1.0,
            0.0,
        );
        assert!(mid > start);
    }
}
