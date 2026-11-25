//! Wind simulation and 3D quaternion / quad orientation math matching Particle Rain.

use std::f32::consts::{FRAC_PI_2, PI};

/// Wind parameters matching `ConfigData.WindOptions`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindOptions {
    pub strength: f32,
    pub gust_frequency: f32,
    pub modulation_speed: f32,
    pub y_level_adjustment: bool,
}

impl Default for WindOptions {
    fn default() -> Self {
        Self {
            strength: 0.03,
            gust_frequency: 0.15,
            modulation_speed: 0.1,
            y_level_adjustment: true,
        }
    }
}

/// Height-based wind scaling (`ParticleRain.yLevelWindMultiplier`).
/// Transitions smoothly from 0 at Y <= 50 to 1 at Y >= 90.
pub fn y_level_wind_multiplier(y: f64) -> f32 {
    const TRANSITION_START: f64 = 50.0;
    const TRANSITION_DISTANCE: f64 = 40.0;
    ((y - TRANSITION_START) / TRANSITION_DISTANCE).clamp(0.0, 1.0) as f32
}

fn one_wind_please(pos: f32, frequency: f32, time: f32, max: f32) -> f32 {
    -((pos * frequency + time).sin() * (pos * frequency * 0.5 + time * 0.5).sin() * max + max)
}

/// Compute horizontal wind vector `[wx, 0.0, wz]` at `(x, y, z)` for `game_time`.
/// Matches `ParticleRain.getWind` (when `y_level_adjustment` is false, uses multiplier 1.0
/// so wind stays active everywhere, fixing the original's `0` bug when disabled).
pub fn get_wind(x: f64, y: f64, z: f64, game_time: i64, opts: &WindOptions) -> [f32; 3] {
    let shift = (game_time as f32) * opts.modulation_speed;
    let multiplier = if opts.y_level_adjustment {
        y_level_wind_multiplier(y)
    } else {
        1.0
    };
    [
        one_wind_please(x as f32, opts.gust_frequency, shift, opts.strength) * multiplier,
        0.0,
        one_wind_please(z as f32, opts.gust_frequency, shift, opts.strength) * multiplier,
    ]
}

/// Pack normalized `[r, g, b, a]` (0..=1) into `0xAARRGGBB`.
pub fn pack_argb(rgba: [f32; 4]) -> u32 {
    let ch = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    (ch(rgba[3]) << 24) | (ch(rgba[0]) << 16) | (ch(rgba[1]) << 8) | ch(rgba[2])
}

/// XYZW quaternion multiplication (`a * b`).
pub fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

/// Construct XYZW unit quaternion from axis (need not be normalized) and angle in radians.
pub fn quat_from_axis_angle(axis: [f32; 3], angle: f32) -> [f32; 4] {
    let len_sq = axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2];
    if !len_sq.is_finite() || len_sq <= 1.0e-20 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let inv = len_sq.sqrt().recip();
    let half = angle * 0.5;
    let (s, c) = half.sin_cos();
    [axis[0] * inv * s, axis[1] * inv * s, axis[2] * inv * s, c]
}

/// Rotate vector `v` by unit XYZW quaternion `q`.
pub fn quat_rotate_vec3(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let qv = [q[0], q[1], q[2]];
    let t = [
        2.0 * (qv[1] * v[2] - qv[2] * v[1]),
        2.0 * (qv[2] * v[0] - qv[0] * v[2]),
        2.0 * (qv[0] * v[1] - qv[1] * v[0]),
    ];
    [
        v[0] + q[3] * t[0] + (qv[1] * t[2] - qv[2] * t[1]),
        v[1] + q[3] * t[1] + (qv[2] * t[0] - qv[0] * t[2]),
        v[2] + q[3] * t[2] + (qv[0] * t[1] - qv[1] * t[0]),
    ]
}

/// Flip quad around its local Y axis if its +Z normal points away from the camera.
/// Matches `WeatherParticle.turnBackfaceFlipways`.
pub fn turn_backface_flipways(q: [f32; 4], camera_offset: [f32; 3]) -> [f32; 4] {
    let normal = quat_rotate_vec3(q, [0.0, 0.0, 1.0]);
    let dot =
        normal[0] * camera_offset[0] + normal[1] * camera_offset[1] + normal[2] * camera_offset[2];
    if dot > 0.0 {
        quat_mul(q, quat_from_axis_angle([0.0, 1.0, 0.0], PI))
    } else {
        q
    }
}

/// Angle between velocity vector and +Y (`CustomParticle.velocityAngle`).
pub fn velocity_angle(velocity: [f32; 3]) -> f32 {
    let len_sq = velocity[0] * velocity[0] + velocity[1] * velocity[1] + velocity[2] * velocity[2];
    let ny = if len_sq.is_finite() && len_sq > 1.0e-20 {
        velocity[1] / len_sq.sqrt()
    } else {
        -1.0
    };
    ny.clamp(-1.0, 1.0).acos()
}

/// Horizontal axis perpendicular to velocity (`CustomParticle.velocityAxis`).
pub fn velocity_axis(velocity: [f32; 3]) -> [f32; 3] {
    let axis = [-velocity[2], 0.0, velocity[0]];
    let len_sq = axis[0] * axis[0] + axis[2] * axis[2];
    if len_sq.is_finite() && len_sq > 1.0e-20 {
        let inv = len_sq.sqrt().recip();
        [axis[0] * inv, 0.0, axis[2] * inv]
    } else {
        [1.0, 0.0, 0.0]
    }
}

/// Build the velocity-aligned billboard quaternion matching `CustomParticle.renderVelocityQuad`.
pub fn velocity_quad_rotation(delta_motion: [f32; 3], camera_offset: [f32; 3]) -> [f32; 4] {
    let angle = velocity_angle(delta_motion);
    let axis = velocity_axis(delta_motion);
    let q_tilt = quat_from_axis_angle(axis, -angle);
    let q_inv = quat_from_axis_angle(axis, angle);
    let transformed = quat_rotate_vec3(q_inv, camera_offset);
    let yaw = transformed[0].atan2(transformed[2]) + PI;
    quat_mul(q_tilt, quat_from_axis_angle([0.0, 1.0, 0.0], yaw))
}

/// Build horizontal quad quaternion matching `CustomParticle.renderHorizontalQuad`.
pub fn horizontal_quad_rotation(roll: f32, camera_offset: [f32; 3]) -> [f32; 4] {
    let mut q = quat_from_axis_angle([-1.0, 0.0, 0.0], FRAC_PI_2);
    if roll != 0.0 {
        q = quat_mul(q, quat_from_axis_angle([0.0, 0.0, 1.0], roll));
    }
    turn_backface_flipways(q, camera_offset)
}

/// Build vertical quad quaternion matching `CustomParticle.renderVerticalQuad`.
pub fn vertical_quad_rotation(roll: f32, camera_offset: [f32; 3]) -> [f32; 4] {
    let yaw = camera_offset[0].atan2(camera_offset[2]);
    let mut q = quat_from_axis_angle([0.0, 1.0, 0.0], yaw);
    if roll != 0.0 {
        q = quat_mul(q, quat_from_axis_angle([0.0, 0.0, 1.0], roll));
    }
    turn_backface_flipways(q, camera_offset)
}

/// Absolute pitch angle (radians) from the camera to a world point (`CustomParticle.getPitchAngleToCamera`).
pub fn pitch_angle_to_camera(pos: [f64; 3], cam: [f64; 3]) -> f32 {
    let ox = (pos[0] - cam[0]) as f32;
    let oy = (pos[1] - cam[1]) as f32;
    let oz = (pos[2] - cam[2]) as f32;
    let horiz = (ox * ox + oz * oz).sqrt();
    oy.atan2(horiz).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn y_level_multiplier_clamps_correctly() {
        assert_eq!(y_level_wind_multiplier(0.0), 0.0);
        assert_eq!(y_level_wind_multiplier(50.0), 0.0);
        assert!((y_level_wind_multiplier(70.0) - 0.5).abs() < 1e-6);
        assert_eq!(y_level_wind_multiplier(90.0), 1.0);
        assert_eq!(y_level_wind_multiplier(200.0), 1.0);
    }

    #[test]
    fn wind_is_zero_underground_when_y_adjustment_enabled() {
        let opts = WindOptions::default();
        let w = get_wind(10.0, 40.0, -5.0, 100, &opts);
        assert_eq!(w, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn wind_is_nonzero_above_ground() {
        let opts = WindOptions::default();
        let w = get_wind(10.0, 100.0, -5.0, 100, &opts);
        assert!(w[0].is_finite() && w[2].is_finite());
        assert_eq!(w[1], 0.0);
        assert!(w[0] <= 0.0 && w[2] <= 0.0);
    }

    #[test]
    fn velocity_angle_straight_down_is_pi() {
        let angle = velocity_angle([0.0, -1.0, 0.0]);
        assert!((angle - PI).abs() < 1e-5);
    }

    #[test]
    fn backface_flip_faces_camera() {
        let q = [0.0, 0.0, 0.0, 1.0]; // +Z normal
        let flipped = turn_backface_flipways(q, [0.0, 0.0, 5.0]);
        let normal = quat_rotate_vec3(flipped, [0.0, 0.0, 1.0]);
        assert!(normal[2] < -0.99);
    }

    #[test]
    fn pack_argb_clamps_and_packs() {
        assert_eq!(pack_argb([1.0, 0.5, 0.0, 1.0]), 0xFFFF_8000);
        assert_eq!(pack_argb([-1.0, 2.0, 0.0, 0.0]), 0x0000_FF00);
    }
}
