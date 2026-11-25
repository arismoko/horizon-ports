//! Custom falling-leaf and snowflake `ParticleProvider` implementation matching
//! Falling Leaves' `FallingLeafParticle` physics (wind drag, rain weight, spin
//! acceleration, water floating/buoyancy/flow, lava burning, ground settling, and
//! 16-tick alpha fade-out).

use crate::wind::{TAU, WindSnapshot};
use mod_api::{
    BlockKey, BlockState, Compound, LightLayer, ModParticle, ParticleBatch, ParticleFacing,
    ParticleLevelView, ParticleMapCodec, ParticleProvider, ParticleStreamCodec, RandomSource, Tag,
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

impl LeafParticleKind {
    pub const fn frame_count(self) -> u32 {
        match self {
            Self::Regular => 5,
            Self::RegularVanilla => 12,
            Self::Conifer => 3,
            Self::Cherry => 12,
            Self::Snow => 1,
        }
    }
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
                    Some(Tag::Float(v)) if v.is_finite() => *v,
                    Some(Tag::Double(v)) if v.is_finite() => *v as f32,
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
                    ("r".into(), Tag::Float(value.r)),
                    ("g".into(), Tag::Float(value.g)),
                    ("b".into(), Tag::Float(value.b)),
                    ("size".into(), Tag::Float(value.size)),
                    ("lifespan".into(), Tag::Int(value.lifespan)),
                ]))
            },
        )
    }

    pub fn stream_codec() -> ParticleStreamCodec<Self> {
        ParticleStreamCodec::wire()
    }
}

/// Cached block keys and fluid state ordinals so `ParticleProvider::tick` can inspect
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

const FLAG_IN_WATER: u32 = 1 << 0;
const FLAG_ON_GROUND: u32 = 1 << 1;
const FLAG_STUCK_IN_GROUND: u32 = 1 << 2;

/// Encode `(max_rotate_time, rotate_time, flags)` into an exact finite `f64` integer
/// (19 bits total, always well within `f64`'s 53-bit exact integer mantissa).
pub fn encode_counters(max_rotate_time: u32, rotate_time: u32, flags: u32) -> f64 {
    let packed = ((max_rotate_time & 0xFF) << 11) | ((rotate_time & 0xFF) << 3) | (flags & 0x7);
    f64::from(packed)
}

pub fn decode_counters(raw: f64) -> (u32, u32, u32) {
    let packed = if raw.is_finite() && raw >= 0.0 {
        raw as u32
    } else {
        0
    };
    let max_rotate_time = ((packed >> 11) & 0xFF).max(1);
    let rotate_time = (packed >> 3) & 0xFF;
    let flags = packed & 0x7;
    (max_rotate_time, rotate_time, flags)
}

/// `ParticleProvider` for our custom falling-leaf and snowflake particle types.
pub struct LeafParticleProvider {
    pub kind: LeafParticleKind,
    pub shared: std::sync::Arc<ParticleSharedState>,
}

impl ParticleProvider<LeafParticleData> for LeafParticleProvider {
    fn create_particle(
        &self,
        options: &LeafParticleData,
        _level: &dyn ParticleLevelView,
        _position: [f64; 3],
        _aux: [f64; 3],
        random: &mut dyn RandomSource,
        particle: &mut ModParticle,
    ) -> Option<()> {
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

        let frames = self.kind.frame_count().max(1);
        particle.velocity = [0.0, 0.0, 0.0];
        particle.lifetime = options.lifespan.clamp(20, 1200);
        particle.rgba = [
            options.r.clamp(0.0, 1.0),
            options.g.clamp(0.0, 1.0),
            options.b.clamp(0.0, 1.0),
            1.0,
        ];
        particle.size = size.clamp(0.005, 1.0);
        particle.frame = random.next_int_bound(frames as i32).max(0) as u32;
        particle.facing = ParticleFacing::Camera;
        particle.roll = random.next_float() * TAU;
        particle.state = [
            f64::from(gravity),
            f64::from(wind_coeff),
            f64::from(max_rotate_speed),
            encode_counters(max_rotate_time, 0, 0),
        ];

        Some(())
    }

    fn tick(&self, particles: &mut ParticleBatch, level: &dyn ParticleLevelView) {
        let (wind_x, wind_z, is_raining) = self.shared.wind.load();
        let wind_x = f64::from(wind_x);
        let wind_z = f64::from(wind_z);

        let block_cache = self
            .shared
            .blocks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();

        for p in particles {
            // Fade-out animation over the last FADE_DURATION ticks.
            if p.age >= p.lifetime + 1 - FADE_DURATION {
                p.rgba[3] = (p.rgba[3] - 1.0 / FADE_DURATION as f32).max(0.0);
            }
            if p.age >= p.lifetime {
                p.removed = true;
                continue;
            }

            let gravity = p.state[0];
            let wind_coeff = p.state[1];
            let max_rotate_speed = p.state[2] as f32;
            let (max_rotate_time, mut rotate_time, mut flags) = decode_counters(p.state[3]);

            let mut in_water = (flags & FLAG_IN_WATER) != 0;
            let mut on_ground = (flags & FLAG_ON_GROUND) != 0;
            let mut stuck_in_ground = (flags & FLAG_STUCK_IN_GROUND) != 0;

            let bx = p.position[0].floor() as i32;
            let by = p.position[1].floor() as i32;
            let bz = p.position[2].floor() as i32;
            let block_pos = [bx, by, bz];

            let cell_state = level.block_state(block_pos);

            // Burn on contact with lava.
            if let Some(lava_lvl) = block_cache.lava_level(cell_state) {
                let above_is_lava = block_cache
                    .lava_level(level.block_state([bx, by + 1, bz]))
                    .is_some();
                let lava_y = f64::from(by) + fluid_height_for_level(lava_lvl, above_is_lava);
                if lava_y >= p.position[1] {
                    self.shared.push_lava_splash(p.position);
                    p.removed = true;
                    continue;
                }
            }

            // Apply gravity + weight of rain under open sky.
            let sky_light = level.brightness(LightLayer::Sky, block_pos);
            let rain_weight = if is_raining && sky_light == 15 {
                0.04
            } else {
                0.0
            };
            p.velocity[1] -= 0.04 * (gravity + rain_weight);

            // Water floating, buoyancy, flow push, and water friction.
            if let Some(water_lvl) = block_cache.water_level(cell_state) {
                let above_is_water = block_cache
                    .water_level(level.block_state([bx, by + 1, bz]))
                    .is_some();
                let water_y = f64::from(by) + fluid_height_for_level(water_lvl, above_is_water);
                if water_y >= p.position[1] - 0.1 {
                    if !in_water {
                        in_water = true;
                        if (water_y - p.position[1]).abs() < 0.2 {
                            p.position[1] = water_y;
                        }
                        p.velocity[1] *= 0.1;
                        p.velocity[0] *= 0.5;
                        p.velocity[2] *= 0.5;
                        rotate_time = 0;
                    } else {
                        let depth = (water_y + 0.1 - p.position[1]).max(0.0);
                        p.velocity[1] += depth * wind_coeff / 30.0;
                    }

                    // Flowing water pushes leaves along the local water slope.
                    if water_lvl != 0 {
                        let (flow_x, flow_z) =
                            estimate_water_flow(level, &block_cache, bx, by, bz, water_lvl);
                        let push_x = flow_x * 0.4;
                        let push_z = flow_z * 0.4;
                        p.velocity[0] += (push_x - p.velocity[0]) * wind_coeff / 60.0;
                        p.velocity[2] += (push_z - p.velocity[2]) * wind_coeff / 60.0;
                    }

                    p.velocity[0] *= 1.0 - WATER_FRICTION;
                    p.velocity[1] *= 1.0 - WATER_FRICTION;
                    p.velocity[2] *= 1.0 - WATER_FRICTION;
                }
            } else {
                in_water = false;
                if !on_ground {
                    rotate_time = (rotate_time + 1).min(max_rotate_time);
                    p.roll += (rotate_time as f32 / max_rotate_time as f32) * max_rotate_speed;
                } else {
                    rotate_time = 0;
                }

                // Approach wind velocity smoothly.
                p.velocity[0] += (wind_x - p.velocity[0]) * wind_coeff / 60.0;
                p.velocity[2] += (wind_z - p.velocity[2]) * wind_coeff / 60.0;
            }

            // Move & collide against solid blocks.
            let old_dx = p.velocity[0];
            let old_dy = p.velocity[1];
            let old_dz = p.velocity[2];

            if !stuck_in_ground && (old_dx != 0.0 || old_dy != 0.0 || old_dz != 0.0) {
                let (dx, dy, dz) =
                    collide_step(level, &block_cache, p.position, old_dx, old_dy, old_dz);

                if (old_dx - dx).abs() > 1e-9 {
                    p.velocity[0] = 0.0;
                }
                if (old_dz - dz).abs() > 1e-9 {
                    p.velocity[2] = 0.0;
                }

                on_ground = (old_dy - dy).abs() > 1e-9 && old_dy < 0.0;
                if !on_ground {
                    stuck_in_ground = false;
                } else if dy.abs() < 1e-5 {
                    stuck_in_ground = true;
                    p.velocity = [0.0, 0.0, 0.0];
                }

                if !stuck_in_ground {
                    p.position[0] += dx;
                    p.position[1] += dy;
                    p.position[2] += dz;
                }
            } else if stuck_in_ground {
                // Re-check whether the supporting block underneath was removed.
                let below = [
                    p.position[0].floor() as i32,
                    (p.position[1] - 0.05).floor() as i32,
                    p.position[2].floor() as i32,
                ];
                if !is_solid_cell(level.block_state(below), &block_cache) {
                    stuck_in_ground = false;
                    on_ground = false;
                } else {
                    p.velocity = [0.0, 0.0, 0.0];
                }
            }

            flags = (if in_water { FLAG_IN_WATER } else { 0 })
                | (if on_ground { FLAG_ON_GROUND } else { 0 })
                | (if stuck_in_ground {
                    FLAG_STUCK_IN_GROUND
                } else {
                    0
                });
            p.state[3] = encode_counters(max_rotate_time, rotate_time, flags);
        }
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
    fn counters_round_trip_and_stay_finite() {
        for max_t in [60u32, 100, 140] {
            for cur_t in [0u32, 50, 140] {
                for flags in 0u32..8 {
                    let encoded = encode_counters(max_t, cur_t, flags);
                    assert!(encoded.is_finite());
                    assert_eq!(decode_counters(encoded), (max_t, cur_t, flags));
                }
            }
        }
    }

    #[test]
    fn fluid_height_matches_vanilla_levels() {
        assert!((fluid_height_for_level(0, false) - 8.0 / 9.0).abs() < 1e-9);
        assert!((fluid_height_for_level(7, false) - 1.0 / 9.0).abs() < 1e-9);
        assert!((fluid_height_for_level(8, false) - 8.0 / 9.0).abs() < 1e-9);
        assert_eq!(fluid_height_for_level(3, true), 1.0);
    }
}
