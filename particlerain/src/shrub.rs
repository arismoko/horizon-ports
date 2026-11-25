//! Tumbling 3D block-model weather particles (`shrub` / `ParticleStyle::BlockModel`),
//! simulated per tick and drawn in `render_level_stage` via `WorldDrawContext::draw_model_instances`.

use crate::config::ParticlePreset;
use crate::wind::{WindOptions, get_wind};
use mod_api::{
    ClientHost, LevelRef, LightCoords, ModelInstance, ModelRef, Overlay, RenderAssets,
    WorldDrawContext,
};
use std::collections::HashMap;

const MAX_SHRUBS: usize = 64;

#[derive(Clone, Debug)]
struct LiveShrub {
    block_id: String,
    pos: [f64; 3],
    prev_pos: [f64; 3],
    vel: [f64; 3],
    roll: f32,
    prev_roll: f32,
    rotation_amount: f32,
    gravity: f32,
    bounciness: f32,
    wind_strength: f32,
    storm_wind_strength: f32,
    size: f32,
    age: i32,
    lifetime: i32,
    edge_bounces: u8,
}

#[derive(Default)]
pub struct ShrubManager {
    shrubs: Vec<LiveShrub>,
    models: HashMap<String, Option<ModelRef>>,
}

impl ShrubManager {
    pub fn clear(&mut self) {
        self.shrubs.clear();
    }

    pub fn invalidate_models(&mut self) {
        self.models.clear();
    }

    pub fn count(&self) -> usize {
        self.shrubs.len()
    }

    pub fn spawn(
        &mut self,
        pos: [f64; 3],
        preset: &ParticlePreset,
        game_time: i64,
        thunder_level: f32,
        wind_opts: &WindOptions,
    ) {
        if self.shrubs.len() >= MAX_SHRUBS {
            return;
        }
        let wind_mul = if thunder_level > 0.9 {
            preset.storm_wind_strength
        } else {
            preset.wind_strength
        };
        let w = get_wind(pos[0], pos[1], pos[2], game_time, wind_opts);
        self.shrubs.push(LiveShrub {
            block_id: preset.block_id.clone(),
            pos,
            prev_pos: pos,
            vel: [
                (w[0] * wind_mul * 10.0) as f64,
                preset.bounciness as f64,
                (w[2] * wind_mul * 10.0) as f64,
            ],
            roll: 0.0,
            prev_roll: 0.0,
            rotation_amount: preset.rotation_amount,
            gravity: preset.gravity,
            bounciness: preset.bounciness,
            wind_strength: preset.wind_strength,
            storm_wind_strength: preset.storm_wind_strength,
            size: preset.size,
            age: 0,
            lifetime: preset.lifetime.min(600),
            edge_bounces: 0,
        });
    }

    pub fn tick(
        &mut self,
        host: &dyn ClientHost,
        level: LevelRef,
        camera_pos: [f64; 3],
        surface_range: i32,
        game_time: i64,
        thunder_level: f32,
        wind_opts: &WindOptions,
    ) {
        let max_dist_sq = ((surface_range as f64) + 0.5).powi(2);
        let is_thundering = thunder_level > 0.9;

        self.shrubs.retain_mut(|s| {
            s.age += 1;
            if s.age > s.lifetime {
                return false;
            }
            s.prev_pos = s.pos;
            s.prev_roll = s.roll;

            s.vel[1] -= 0.04 * (s.gravity as f64);
            let wind_mul = if is_thundering {
                s.storm_wind_strength
            } else {
                s.wind_strength
            };
            let w = get_wind(s.pos[0], s.pos[1], s.pos[2], game_time, wind_opts);
            s.vel[0] += (w[0] * wind_mul) as f64;
            s.vel[2] += (w[2] * wind_mul) as f64;

            s.pos[0] += s.vel[0];
            s.pos[1] += s.vel[1];
            s.pos[2] += s.vel[2];
            s.vel[0] *= 0.98;
            s.vel[1] *= 0.98;
            s.vel[2] *= 0.98;

            let speed = (s.vel[0] * s.vel[0] + s.vel[1] * s.vel[1] + s.vel[2] * s.vel[2]).sqrt();
            s.roll += s.rotation_amount * (speed as f32);

            let dx = s.pos[0] - camera_pos[0];
            let dy = s.pos[1] - camera_pos[1];
            let dz = s.pos[2] - camera_pos[2];
            if dx * dx + dy * dy + dz * dz > max_dist_sq {
                return false;
            }

            let bp = [
                s.pos[0].floor() as i32,
                s.pos[1].floor() as i32,
                s.pos[2].floor() as i32,
            ];
            if let Some(state) = level.block_state(host, bp) {
                let boxes = level
                    .block_shape(host, state, bp, mod_api::BlockShapeKind::Collision)
                    .unwrap_or_default();
                if !boxes.is_empty() {
                    let bounce = (s.bounciness as f64) * speed * 6.0;
                    if s.prev_pos[1].floor() as i32 > bp[1] {
                        s.vel[1] = s.vel[1].abs() + bounce;
                        s.pos[1] = (bp[1] + 1) as f64 + 0.01;
                    } else if s.edge_bounces < 3 {
                        s.edge_bounces += 1;
                        s.vel[1] += 0.5 * bounce;
                        s.vel[0] = -s.vel[0] * 0.5;
                        s.vel[2] = -s.vel[2] * 0.5;
                    } else {
                        return false;
                    }
                }
            }
            true
        });
    }

    pub fn resolve_models(&mut self, host: &dyn ClientHost) {
        let assets: &dyn RenderAssets = host.render_assets();
        let needed: Vec<String> = self
            .shrubs
            .iter()
            .map(|s| s.block_id.clone())
            .filter(|id| !self.models.contains_key(id))
            .collect();
        for id in needed {
            let model = host
                .block_state_id(&id)
                .and_then(|state| assets.block_state_model(state).ok());
            self.models.insert(id, model);
        }
    }

    pub fn render(&self, partial_tick: f32, draw: &mut dyn WorldDrawContext) {
        if self.shrubs.is_empty() {
            return;
        }
        let mut by_block: HashMap<&str, Vec<ModelInstance>> = HashMap::new();
        let pt = partial_tick as f64;
        for s in &self.shrubs {
            let origin = [
                s.prev_pos[0] + (s.pos[0] - s.prev_pos[0]) * pt,
                s.prev_pos[1] + (s.pos[1] - s.prev_pos[1]) * pt,
                s.prev_pos[2] + (s.pos[2] - s.prev_pos[2]) * pt,
            ];
            let roll = s.prev_roll + (s.roll - s.prev_roll) * partial_tick;
            let yaw = (-s.vel[0]).atan2(s.vel[2]) as f32;
            let transform = shrub_transform(yaw, roll, s.size);
            let instance = ModelInstance::new(
                origin,
                transform,
                0xFFFF_FFFF,
                LightCoords::FULL_BRIGHT,
                Overlay::NONE,
            );
            by_block.entry(&s.block_id).or_default().push(instance);
        }

        for (block_id, instances) in by_block {
            if let Some(Some(model)) = self.models.get(block_id) {
                let _ = draw.draw_model_instances(*model, &instances);
            }
        }
    }
}

/// Build row-major 3x4 affine matrix matching `BlockDisplayParticle.updateDisplay`:
/// Y-yaw * (translation(wobble) * X-roll * scale(size)).
pub fn shrub_transform(yaw: f32, roll: f32, size: f32) -> [f32; 12] {
    let (sy, cy) = yaw.sin_cos();
    let (sr, cr) = roll.sin_cos();

    // Local offset matching BlockDisplayParticle's rolling wobble, centered on unit block (-0.5)
    let ty = -(roll + 0.8).cos() * 0.7;
    let tz = -(roll + 5.5).cos() * 0.7;

    // R_y(yaw) * R_x(roll) * scale(size)
    let m00 = cy * size;
    let m01 = sy * sr * size;
    let m02 = sy * cr * size;
    let m10 = 0.0;
    let m11 = cr * size;
    let m12 = -sr * size;
    let m20 = -sy * size;
    let m21 = cy * sr * size;
    let m22 = cy * cr * size;

    let tx = sy * tz;
    let rty = ty;
    let rtz = cy * tz;

    [
        m00, m01, m02, tx, // row 0
        m10, m11, m12, rty, // row 1
        m20, m21, m22, rtz, // row 2
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shrub_transform_is_finite_and_scaled() {
        let m = shrub_transform(0.0, 0.0, 1.0);
        assert!(m.iter().all(|v| v.is_finite()));
        assert!((m[0] - 1.0).abs() < 1e-5);
        assert!((m[5] - 1.0).abs() < 1e-5);
        assert!((m[10] - 1.0).abs() < 1e-5);
    }
}
