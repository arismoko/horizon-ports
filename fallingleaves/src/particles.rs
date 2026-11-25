//! Custom falling-leaf and snowflake `ParticleProvider` implementation matching
//! Falling Leaves' `FallingLeafParticle` physics (wind drag, rain weight, spin
//! acceleration, water floating/buoyancy/flow, lava burning, ground settling, and
//! 16-tick alpha fade-out).

use crate::wind::{TAU, WindSnapshot};
use mod_api::{
    BlockKey, BlockState, Compound, CustomParticle, LightLayer, ParticleFacing, ParticleLevelView,
    ParticleMapCodec, ParticleProvider, ParticleSpawn, ParticleStreamCodec, ParticleTickView, Tag,
    Wire,
};
use std::collections::HashMap;
use std::sync::Mutex;

pub const FADE_DURATION: i32 = 16;
pub const WATER_FRICTION: f64 = 0.075;
const MAX_QUEUED_LAVA_SPLASHES: usize = 64;

/// Which visual and physics preset a particle provider runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeafParticleKind {
    Regular,
    RegularVanilla,
    Conifer,
    Cherry,
    Snow,
}

/// Typed options carried by our registered particle types.
#[derive(Clone, Debug, PartialEq, Wire)]
pub struct LeafParticleData {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub size: f32,
    pub lifespan: i32,
}

impl Default for LeafParticleData {
    fn default() -> Self {
        Self {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            size: 0.1,
            lifespan: 200,
        }
    }
}

impl LeafParticleData {
    pub fn map_codec() -> ParticleMapCodec<Self> {
        ParticleMapCodec::new(
            |fields, _| {
                let read_f32 = |k: &str, default: f32| match fields.get(&k.into()) {
                    Some(Tag::Float(bits)) => {
                        let v = f32::from_bits(*bits);
                        if v.is_finite() { v } else { default }
                    }
                    Some(Tag::Double(bits)) => {
                        let v = f64::from_bits(*bits) as f32;
                        if v.is_finite() { v } else { default }
                    }
                    _ => default,
                };
                let lifespan = match fields.get(&"lifespan".into()) {
                    Some(Tag::Int(v)) => (*v).clamp(20, 1200),
                    _ => 200,
                };
                Ok(Self {
                    r: read_f32("r", 1.0).clamp(0.0, 1.0),
                    g: read_f32("g", 1.0).clamp(0.0, 1.0),
                    b: read_f32("b", 1.0).clamp(0.0, 1.0),
                    size: read_f32("size", 0.1).clamp(0.01, 1.0),
                    lifespan,
                })
            },
            |value, _| {
                Ok(Compound::from([
                    ("r".into(), Tag::Float(value.r.to_bits())),
                    ("g".into(), Tag::Float(value.g.to_bits())),
                    ("b".into(), Tag::Float(value.b.to_bits())),
                    ("size".into(), Tag::Float(value.size.to_bits())),
                    ("lifespan".into(), Tag::Int(value.lifespan)),
                ]))
            },
        )
    }

    pub fn stream_codec() -> ParticleStreamCodec<Self> {
        ParticleStreamCodec::wire()
    }

    /// Replaces non-finite colour and size values (the wire codec decodes raw `f32`s, and
    /// `f32::clamp` passes NaN through) with their defaults.
    pub fn finite_or_default(&self) -> Self {
        let default = Self::default();
        let finite = |v: f32, fallback: f32| if v.is_finite() { v } else { fallback };
        Self {
            r: finite(self.r, default.r),
            g: finite(self.g, default.g),
            b: finite(self.b, default.b),
            size: finite(self.size, default.size),
            lifespan: self.lifespan,
        }
    }
}

/// Cached block keys and fluid state ordinals so `CustomParticle::tick` can inspect
/// water, lava, and air without a `Host`.
#[derive(Clone, Debug, Default)]
pub struct FluidBlockCache {
    pub air_keys: Vec<BlockKey>,
    pub water_key: Option<BlockKey>,
    pub lava_key: Option<BlockKey>,
    /// Maps `BlockState::ordinal()` for `minecraft:water` to its vanilla level (0..=15).
    pub water_levels: HashMap<u16, u8>,
    /// Maps `BlockState::ordinal()` for `minecraft:lava` to its vanilla level (0..=15).
    pub lava_levels: HashMap<u16, u8>,
}

impl FluidBlockCache {
    pub fn is_air(&self, block: BlockKey) -> bool {
        self.air_keys.contains(&block)
    }

    pub fn water_level(&self, state: BlockState) -> Option<u8> {
        if self.water_key == Some(state.block()) {
            Some(
                self.water_levels
                    .get(&state.ordinal())
                    .copied()
                    .unwrap_or(0),
            )
        } else {
            None
        }
    }

    pub fn lava_level(&self, state: BlockState) -> Option<u8> {
        if self.lava_key == Some(state.block()) {
            Some(self.lava_levels.get(&state.ordinal()).copied().unwrap_or(0))
        } else {
            None
        }
    }
}

/// Shared state read by all `LeafParticleProvider` instances during particle ticks.
pub struct ParticleSharedState {
    pub wind: WindSnapshot,
    pub blocks: Mutex<FluidBlockCache>,
    pub lava_splashes: Mutex<Vec<[f64; 3]>>,
}

impl Default for ParticleSharedState {
    fn default() -> Self {
        Self::new()
    }
}

impl ParticleSharedState {
    pub fn new() -> Self {
        Self {
            wind: WindSnapshot::new(),
            blocks: Mutex::new(FluidBlockCache::default()),
            lava_splashes: Mutex::new(Vec::new()),
        }
    }

    pub fn push_lava_splash(&self, pos: [f64; 3]) {
        if let Ok(mut queue) = self.lava_splashes.lock()
            && queue.len() < MAX_QUEUED_LAVA_SPLASHES
        {
            queue.push(pos);
        }
    }

    pub fn drain_lava_splashes(&self) -> Vec<[f64; 3]> {
        match self.lava_splashes.lock() {
            Ok(mut queue) => std::mem::take(&mut *queue),
            Err(e) => std::mem::take(&mut *e.into_inner()),
        }
    }
}

/// Fluid height in blocks (0..=1) for a vanilla fluid `level` property (0 = source,
/// 1..=7 = flowing, 8..=15 = falling).
pub fn fluid_height_for_level(level: u8, same_fluid_above: bool) -> f64 {
    if same_fluid_above {
        return 1.0;
    }
    let amount = if level >= 8 {
        8
    } else {
        8u8.saturating_sub(level)
    };
    f64::from(amount) / 9.0
}

/// `ParticleProvider` for our custom falling-leaf and snowflake particle types, bound to one
/// loaded sprite set.
pub struct LeafParticleProvider {
    pub kind: LeafParticleKind,
    pub shared: std::sync::Arc<ParticleSharedState>,
    pub frames: usize,
}

/// One live falling leaf or snowflake; its fields are its per-particle state.
struct LeafParticle {
    shared: std::sync::Arc<ParticleSharedState>,
    gravity: f64,
    wind_coeff: f64,
    max_rotate_speed: f32,
    max_rotate_time: u32,
    rotate_time: u32,
    in_water: bool,
    on_ground: bool,
    stuck_in_ground: bool,
}

impl ParticleProvider<LeafParticleData> for LeafParticleProvider {
    fn create_particle(
        &self,
        spawn: &mut ParticleSpawn<'_, LeafParticleData>,
        particle: &mut dyn ParticleTickView,
    ) -> Option<Box<dyn CustomParticle>> {
        let random = &mut *spawn.random;
        let (gravity, wind_coeff) = match self.kind {
            LeafParticleKind::Cherry => (
                0.0175 + random.next_float() * 0.0050,
                0.12 + random.next_float() * 0.8,
            ),
            LeafParticleKind::Snow => (
                0.0125 + random.next_float() * 0.0125,
                0.1 + random.next_float() * 0.1,
            ),
            LeafParticleKind::Regular
            | LeafParticleKind::RegularVanilla
            | LeafParticleKind::Conifer => (
                0.08 + random.next_float() * 0.04,
                0.6 + random.next_float() * 0.4,
            ),
        };

        let options = spawn.options.finite_or_default();
        let max_rotate_time = (3 + random.next_int_bound(5)) as u32 * 20;
        let sign = if random.next_boolean() {
            -1.0f32
        } else {
            1.0f32
        };
        let max_rotate_speed = match self.kind {
            LeafParticleKind::Cherry | LeafParticleKind::Snow => {
                sign * (0.1 + 0.4 * random.next_float()) * TAU / 20.0
            }
            _ => sign * (0.1 + 2.4 * random.next_float()) * TAU / 20.0,
        };

        let mut size = if self.kind == LeafParticleKind::Cherry {
            options.size / 2.0
        } else {
            options.size
        };
        if random.next_boolean() {
            size *= 1.5;
        }

        let frames = self.frames.max(1);
        let frame = random
            .next_int_bound(frames.min(i32::MAX as usize) as i32)
            .max(0) as usize;
        let roll = random.next_float() * TAU;

        // The instance moves itself, so the base tick's motion and collision stay off.
        particle.set_has_physics(false);
        particle.set_velocity([0.0, 0.0, 0.0]);
        particle.set_lifetime(options.lifespan.clamp(20, 1200));
        particle.set_rgba([
            options.r.clamp(0.0, 1.0),
            options.g.clamp(0.0, 1.0),
            options.b.clamp(0.0, 1.0),
            1.0,
        ]);
        particle.set_size(size.clamp(0.005, 1.0));
        particle.set_sprite_index(frame);
        particle.set_facing(ParticleFacing::Camera);
        particle.set_roll(roll);

        Some(Box::new(LeafParticle {
            shared: self.shared.clone(),
            gravity: f64::from(gravity),
            wind_coeff: f64::from(wind_coeff),
            max_rotate_speed,
            max_rotate_time: max_rotate_time.max(1),
            rotate_time: 0,
            in_water: false,
            on_ground: false,
            stuck_in_ground: false,
        }))
    }
}

impl CustomParticle for LeafParticle {
    fn tick(&mut self, particle: &mut dyn ParticleTickView, level: &dyn ParticleLevelView) {
        let (wind_x, wind_z, is_raining) = self.shared.wind.load();
        let wind_x = f64::from(wind_x);
        let wind_z = f64::from(wind_z);

        let block_cache = self
            .shared
            .blocks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();

        // Fade-out animation over the last FADE_DURATION ticks.
        if particle.age() >= particle.lifetime() + 1 - FADE_DURATION {
            let mut rgba = particle.rgba();
            rgba[3] = (rgba[3] - 1.0 / FADE_DURATION as f32).max(0.0);
            particle.set_rgba(rgba);
        }
        if !particle.advance_age() {
            return;
        }

        let gravity = self.gravity;
        let wind_coeff = self.wind_coeff;
        let max_rotate_speed = self.max_rotate_speed;
        let max_rotate_time = self.max_rotate_time;
        let mut rotate_time = self.rotate_time;
        let mut in_water = self.in_water;
        let mut on_ground = self.on_ground;
        let mut stuck_in_ground = self.stuck_in_ground;

        let mut position = particle.position();
        let mut velocity = particle.velocity();
        let mut roll = particle.roll();

        let bx = position[0].floor() as i32;
        let by = position[1].floor() as i32;
        let bz = position[2].floor() as i32;
        let block_pos = [bx, by, bz];

        let cell_state = level.block_state(block_pos);

        // Burn on contact with lava.
        if let Some(lava_lvl) = block_cache.lava_level(cell_state) {
            let above_is_lava = block_cache
                .lava_level(level.block_state([bx, by + 1, bz]))
                .is_some();
            let lava_y = f64::from(by) + fluid_height_for_level(lava_lvl, above_is_lava);
            if lava_y >= position[1] {
                self.shared.push_lava_splash(position);
                particle.remove();
                return;
            }
        }

        // Apply gravity + weight of rain under open sky.
        let sky_light = level.brightness(LightLayer::Sky, block_pos);
        let rain_weight = if is_raining && sky_light == 15 {
            0.04
        } else {
            0.0
        };
        velocity[1] -= 0.04 * (gravity + rain_weight);

        // Water floating, buoyancy, flow push, and water friction.
        if let Some(water_lvl) = block_cache.water_level(cell_state) {
            let above_is_water = block_cache
                .water_level(level.block_state([bx, by + 1, bz]))
                .is_some();
            let water_y = f64::from(by) + fluid_height_for_level(water_lvl, above_is_water);
            if water_y >= position[1] - 0.1 {
                if !in_water {
                    in_water = true;
                    if (water_y - position[1]).abs() < 0.2 {
                        position[1] = water_y;
                    }
                    velocity[1] *= 0.1;
                    velocity[0] *= 0.5;
                    velocity[2] *= 0.5;
                    rotate_time = 0;
                } else {
                    let depth = (water_y + 0.1 - position[1]).max(0.0);
                    velocity[1] += depth * wind_coeff / 30.0;
                }

                // Flowing water pushes leaves along the local water slope.
                if water_lvl != 0 {
                    let (flow_x, flow_z) =
                        estimate_water_flow(level, &block_cache, bx, by, bz, water_lvl);
                    let push_x = flow_x * 0.4;
                    let push_z = flow_z * 0.4;
                    velocity[0] += (push_x - velocity[0]) * wind_coeff / 60.0;
                    velocity[2] += (push_z - velocity[2]) * wind_coeff / 60.0;
                }

                velocity[0] *= 1.0 - WATER_FRICTION;
                velocity[1] *= 1.0 - WATER_FRICTION;
                velocity[2] *= 1.0 - WATER_FRICTION;
            }
        } else {
            in_water = false;
            if !on_ground {
                rotate_time = (rotate_time + 1).min(max_rotate_time);
                roll += (rotate_time as f32 / max_rotate_time as f32) * max_rotate_speed;
            } else {
                rotate_time = 0;
            }

            // Approach wind velocity smoothly.
            velocity[0] += (wind_x - velocity[0]) * wind_coeff / 60.0;
            velocity[2] += (wind_z - velocity[2]) * wind_coeff / 60.0;
        }

        // Move & collide against solid blocks.
        let old_dx = velocity[0];
        let old_dy = velocity[1];
        let old_dz = velocity[2];

        if !stuck_in_ground && (old_dx != 0.0 || old_dy != 0.0 || old_dz != 0.0) {
            let (dx, dy, dz) = collide_step(level, &block_cache, position, old_dx, old_dy, old_dz);

            if (old_dx - dx).abs() > 1e-9 {
                velocity[0] = 0.0;
            }
            if (old_dz - dz).abs() > 1e-9 {
                velocity[2] = 0.0;
            }

            on_ground = (old_dy - dy).abs() > 1e-9 && old_dy < 0.0;
            if !on_ground {
                stuck_in_ground = false;
            } else if dy.abs() < 1e-5 {
                stuck_in_ground = true;
                velocity = [0.0, 0.0, 0.0];
            }

            if !stuck_in_ground {
                position[0] += dx;
                position[1] += dy;
                position[2] += dz;
            }
        } else if stuck_in_ground {
            // Re-check whether the supporting block underneath was removed.
            let below = [
                position[0].floor() as i32,
                (position[1] - 0.05).floor() as i32,
                position[2].floor() as i32,
            ];
            if !is_solid_cell(level.block_state(below), &block_cache) {
                stuck_in_ground = false;
                on_ground = false;
            } else {
                velocity = [0.0, 0.0, 0.0];
            }
        }

        self.rotate_time = rotate_time;
        self.in_water = in_water;
        self.on_ground = on_ground;
        self.stuck_in_ground = stuck_in_ground;

        particle.set_roll(roll);
        particle.set_velocity(velocity);
        particle.set_position(position);
    }
}

fn is_solid_cell(state: BlockState, cache: &FluidBlockCache) -> bool {
    let block = state.block();
    !cache.is_air(block) && cache.water_key != Some(block) && cache.lava_key != Some(block)
}

fn collide_step(
    level: &dyn ParticleLevelView,
    cache: &FluidBlockCache,
    pos: [f64; 3],
    dx: f64,
    dy: f64,
    dz: f64,
) -> (f64, f64, f64) {
    let mut cur = pos;

    // Y axis first (matches Entity.collideBoundingBox order).
    let mut out_dy = dy;
    let next_y = cur[1] + dy;
    let cell_y = [
        cur[0].floor() as i32,
        next_y.floor() as i32,
        cur[2].floor() as i32,
    ];
    if is_solid_cell(level.block_state(cell_y), cache) {
        if dy < 0.0 {
            let top = f64::from(cell_y[1] + 1);
            out_dy = (top - cur[1]).min(0.0);
        } else if dy > 0.0 {
            let bottom = f64::from(cell_y[1]);
            out_dy = (bottom - cur[1]).max(0.0);
        }
    }
    cur[1] += out_dy;

    // X axis.
    let mut out_dx = dx;
    let next_x = cur[0] + dx;
    let cell_x = [
        next_x.floor() as i32,
        cur[1].floor() as i32,
        cur[2].floor() as i32,
    ];
    if is_solid_cell(level.block_state(cell_x), cache) {
        out_dx = 0.0;
    }
    cur[0] += out_dx;

    // Z axis.
    let mut out_dz = dz;
    let next_z = cur[2] + dz;
    let cell_z = [
        cur[0].floor() as i32,
        cur[1].floor() as i32,
        next_z.floor() as i32,
    ];
    if is_solid_cell(level.block_state(cell_z), cache) {
        out_dz = 0.0;
    }

    (out_dx, out_dy, out_dz)
}

fn estimate_water_flow(
    level: &dyn ParticleLevelView,
    cache: &FluidBlockCache,
    bx: i32,
    by: i32,
    bz: i32,
    self_level: u8,
) -> (f64, f64) {
    let self_h = fluid_height_for_level(self_level, false);
    let mut fx = 0.0;
    let mut fz = 0.0;
    for (ox, oz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        if let Some(n_lvl) = cache.water_level(level.block_state([bx + ox, by, bz + oz])) {
            let nh = fluid_height_for_level(n_lvl, false);
            let diff = self_h - nh;
            fx += f64::from(ox) * diff;
            fz += f64::from(oz) * diff;
        }
    }
    let len = (fx * fx + fz * fz).sqrt();
    if len > 1e-6 {
        (fx / len, fz / len)
    } else {
        (0.0, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_finite_options_fall_back_to_defaults() {
        let bad = LeafParticleData {
            r: f32::NAN,
            g: f32::INFINITY,
            b: f32::NEG_INFINITY,
            size: f32::NAN,
            lifespan: 80,
        };
        let fixed = bad.finite_or_default();
        assert_eq!(
            fixed,
            LeafParticleData {
                lifespan: 80,
                ..LeafParticleData::default()
            }
        );
    }

    #[test]
    fn finite_options_are_unchanged() {
        let ok = LeafParticleData {
            r: 0.2,
            g: 0.4,
            b: 0.6,
            size: 0.3,
            lifespan: 100,
        };
        assert_eq!(ok.finite_or_default(), ok);
    }

    #[test]
    fn fluid_height_matches_vanilla_levels() {
        assert!((fluid_height_for_level(0, false) - 8.0 / 9.0).abs() < 1e-9);
        assert!((fluid_height_for_level(7, false) - 1.0 / 9.0).abs() < 1e-9);
        assert!((fluid_height_for_level(8, false) - 8.0 / 9.0).abs() < 1e-9);
        assert_eq!(fluid_height_for_level(3, true), 1.0);
    }
}
