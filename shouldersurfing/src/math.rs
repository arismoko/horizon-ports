//! Vector and angle helpers. Angles are degrees, as in the game.

pub type V3 = [f64; 3];

pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn scale(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

pub fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn length(a: V3) -> f64 {
    dot(a, a).sqrt()
}

/// `Camera.forwards`: yaw 0 looks south (+z), pitch 90 looks down.
pub fn forward(yaw: f32, pitch: f32) -> V3 {
    let (sy, cy) = f64::from(yaw).to_radians().sin_cos();
    let (sp, cp) = f64::from(pitch).to_radians().sin_cos();
    [-sy * cp, -sp, cy * cp]
}

/// The `(yaw, pitch)` in degrees that look along `dir`, or None for a zero or non-finite one.
pub fn look_angles(dir: V3) -> Option<(f32, f32)> {
    let horizontal = dir[0].hypot(dir[2]);
    if !(horizontal.is_finite() && dir[1].is_finite()) || horizontal.max(dir[1].abs()) < 1e-9 {
        return None;
    }
    let yaw = (-dir[0]).atan2(dir[2]).to_degrees() as f32;
    let pitch = (-dir[1]).atan2(horizontal).to_degrees() as f32;
    Some((yaw, pitch))
}

/// `Camera.left`.
pub fn left(yaw: f32) -> V3 {
    let (sy, cy) = f64::from(yaw).to_radians().sin_cos();
    [cy, 0.0, sy]
}

/// `Camera.up`.
pub fn up(yaw: f32, pitch: f32) -> V3 {
    let (sy, cy) = f64::from(yaw).to_radians().sin_cos();
    let (sp, cp) = f64::from(pitch).to_radians().sin_cos();
    [-sy * sp, cp, cy * sp]
}

/// Where the camera sits relative to the eye for an offset `[x, y, z]`: `x` to the left, `y` up,
/// `z` backwards (`ShoulderSurfingCamera.calcCameraOffset`).
pub fn camera_displacement(yaw: f32, pitch: f32, offset: V3) -> V3 {
    let l = scale(left(yaw), offset[0]);
    let u = scale(up(yaw, pitch), offset[1]);
    let f = scale(forward(yaw, pitch), -offset[2]);
    add(add(l, u), f)
}

/// `Mth.wrapDegrees`: into `-180..180`.
pub fn wrap_degrees(angle: f32) -> f32 {
    if !angle.is_finite() {
        return 0.0;
    }
    let mut a = angle % 360.0;
    if a >= 180.0 {
        a -= 360.0;
    }
    if a < -180.0 {
        a += 360.0;
    }
    a
}

/// `Mth.approachDegrees`: moves `from` towards `to` along the short way by at most `step`.
pub fn approach_degrees(from: f32, to: f32, step: f32) -> f32 {
    let diff = wrap_degrees(to - from);
    let step = step.abs();
    from + diff.clamp(-step, step)
}

#[cfg(test)]
/// The yaw that looks along the horizontal direction `(dx, dz)`.
pub fn yaw_of(dx: f64, dz: f64) -> f32 {
    (-dx).atan2(dz).to_degrees() as f32
}

pub fn lerp(t: f64, a: f64, b: f64) -> f64 {
    a + (b - a) * t
}

/// Rotates a movement input `[left, forward]` from the camera's frame into the player's frame
/// (`d` = camera yaw minus player yaw, degrees).
pub fn rotate_input(input: [f32; 2], d: f32) -> [f32; 2] {
    let (s, c) = d.to_radians().sin_cos();
    [-input[1] * s + input[0] * c, input[1] * c + input[0] * s]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: V3, b: V3) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
    }

    #[test]
    fn basis_is_orthonormal() {
        for (y, p) in [(0.0, 0.0), (37.0, -20.0), (-120.0, 60.0), (200.0, 89.0)] {
            let (f, l, u) = (forward(y, p), left(y), up(y, p));
            assert!((length(f) - 1.0).abs() < 1e-9);
            assert!(dot(f, l).abs() < 1e-9 && dot(f, u).abs() < 1e-9 && dot(l, u).abs() < 1e-9);
        }
        assert!(near(forward(0.0, 0.0), [0.0, 0.0, 1.0]));
        assert!(near(left(0.0), [1.0, 0.0, 0.0]));
        assert!(near(forward(0.0, 90.0), [0.0, -1.0, 0.0]));
    }

    #[test]
    fn displacement_follows_offset_signs() {
        // Looking south, a negative x offset puts the camera on the player's right (west).
        let d = camera_displacement(0.0, 0.0, [-0.75, 0.0, 4.0]);
        assert!(near(d, [-0.75, 0.0, -4.0]));
        let d = camera_displacement(0.0, 0.0, [0.0, 1.0, 0.0]);
        assert!(near(d, [0.0, 1.0, 0.0]));
    }

    #[test]
    fn wrapping_and_approach() {
        assert_eq!(wrap_degrees(190.0), -170.0);
        assert_eq!(wrap_degrees(-190.0), 170.0);
        assert_eq!(wrap_degrees(f32::NAN), 0.0);
        assert_eq!(approach_degrees(170.0, -170.0, 5.0), 175.0);
        assert_eq!(approach_degrees(0.0, 3.0, 5.0), 3.0);
        assert_eq!(approach_degrees(0.0, -30.0, 5.0), -5.0);
    }

    #[test]
    fn yaw_of_matches_forward() {
        for y in [-170.0_f32, -90.0, 0.0, 45.0, 135.0] {
            let f = forward(y, 0.0);
            assert!((wrap_degrees(yaw_of(f[0], f[2]) - y)).abs() < 1e-3);
        }
    }

    #[test]
    fn input_rotation_keeps_the_world_direction() {
        // The camera yaw is 90 degrees above the player's: pressing forward must walk where the
        // camera looks, which is to the player's right.
        let r = rotate_input([0.0, 1.0], 90.0);
        assert!((r[0] + 1.0).abs() < 1e-6 && r[1].abs() < 1e-6);
        let r = rotate_input([0.0, 1.0], 0.0);
        assert_eq!(r, [0.0, 1.0]);
        let r = rotate_input([1.0, 0.0], 90.0);
        assert!((r[1] - 1.0).abs() < 1e-6 && r[0].abs() < 1e-6);
    }

    #[test]
    fn look_angles_invert_forward() {
        let (yaw, pitch) = look_angles(forward(37.0, -20.0)).unwrap();
        assert!((yaw - 37.0).abs() < 1e-3 && (pitch + 20.0).abs() < 1e-3);
        assert!(look_angles([0.0; 3]).is_none());
        assert!(look_angles([f64::NAN, 0.0, 1.0]).is_none());
    }
}
