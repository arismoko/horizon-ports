use mod_api::{Camera, TeamColor};

pub const WHITE: u32 = 0xFFFF_FFFF;

/// Projected 2D screen position and clip-space W depth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenPos {
    pub x: f32,
    pub y: f32,
    pub depth: f32,
}

impl ScreenPos {
    pub fn is_behind_camera(self) -> bool {
        self.depth <= 0.0
    }

    pub fn distance_to(self, point: [f32; 2]) -> f32 {
        if self.is_behind_camera() {
            return f32::MAX;
        }
        let dx = point[0] - self.x;
        let dy = point[1] - self.y;
        (dx * dx + dy * dy).sqrt()
    }

    pub fn is_in_bounds(self, top_left: [f32; 2], bottom_right: [f32; 2]) -> bool {
        self.x >= top_left[0]
            && self.x <= bottom_right[0]
            && self.y >= top_left[1]
            && self.y <= bottom_right[1]
    }
}

/// Multiplies a column-major 4x4 matrix by a 4-component vector.
pub fn mul_mat4_vec4(m: &[f32; 16], v: [f32; 4]) -> [f32; 4] {
    [
        m[0] * v[0] + m[4] * v[1] + m[8] * v[2] + m[12] * v[3],
        m[1] * v[0] + m[5] * v[1] + m[9] * v[2] + m[13] * v[3],
        m[2] * v[0] + m[6] * v[1] + m[10] * v[2] + m[14] * v[3],
        m[3] * v[0] + m[7] * v[1] + m[11] * v[2] + m[15] * v[3],
    ]
}

/// Projects a world-space point into GUI screen coordinates, preserving `depth <= 0`
/// for points behind the camera so off-screen direction indicators remain accurate.
pub fn world_to_screen(world_pos: [f64; 3], camera: &Camera, screen_size: [f32; 2]) -> ScreenPos {
    let rel = [
        (world_pos[0] - camera.position[0]) as f32,
        (world_pos[1] - camera.position[1]) as f32,
        (world_pos[2] - camera.position[2]) as f32,
        1.0,
    ];
    let view = mul_mat4_vec4(&camera.view, rel);
    let mut clip = mul_mat4_vec4(&camera.projection, view);
    let depth = clip[3];
    if depth != 0.0 && depth.is_finite() {
        clip[0] /= depth;
        clip[1] /= depth;
    }
    ScreenPos {
        x: screen_size[0] * (0.5 + clip[0] * 0.5),
        y: screen_size[1] * (0.5 - clip[1] * 0.5),
        depth,
    }
}

/// Computes the unit look vector from vanilla yaw and pitch angles (in degrees).
pub fn view_direction(yaw_deg: f32, pitch_deg: f32) -> [f64; 3] {
    let pitch = f64::from(pitch_deg).to_radians();
    let yaw = f64::from(-yaw_deg).to_radians();
    let cos_yaw = yaw.cos();
    let sin_yaw = yaw.sin();
    let cos_pitch = pitch.cos();
    let sin_pitch = pitch.sin();
    [sin_yaw * cos_pitch, -sin_pitch, cos_yaw * cos_pitch]
}

/// Distance between two 3D points.
pub fn distance_3d(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let dz = b[2] - a[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// Squared distance between two 3D points.
pub fn distance_sqr_3d(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let dz = b[2] - a[2];
    dx * dx + dy * dy + dz * dz
}

/// Intersects a ray at `angle` (radians) from the center of `[left_top, right_bottom]`
/// with the rectangle's perimeter.
pub fn calculate_angle_rect_intersection(
    angle: f32,
    left_top: [f32; 2],
    right_bottom: [f32; 2],
) -> [f32; 2] {
    let width = (right_bottom[0] - left_top[0]).max(1.0);
    let height = (right_bottom[1] - left_top[1]).max(1.0);
    let dx = f64::from(angle).cos();
    let dy = f64::from(angle).sin();
    let norm_x = dx / f64::from(width);
    let norm_y = dy / f64::from(height);
    let cx = f64::from(width) * 0.5;
    let cy = f64::from(height) * 0.5;

    if norm_x.abs() < norm_y.abs() {
        if norm_y < 0.0 {
            let t = -cy / dy;
            let x = cx + t * dx;
            return [x as f32 + left_top[0], left_top[1]];
        }
        let t = cy / dy;
        let x = cx + t * dx;
        return [x as f32 + left_top[0], right_bottom[1]];
    }

    if norm_x < 0.0 {
        let t = -cx / dx;
        let y = cy + t * dy;
        return [left_top[0], y as f32 + left_top[1]];
    }

    let t = cx / dx;
    let y = cy + t * dy;
    [right_bottom[0], y as f32 + left_top[1]]
}

/// Computes the directional sound position matching `DirectionalSoundInstance`:
/// maps distances up to 64 blocks linearly onto 0..14 blocks from the player along
/// the direction toward the ping.
pub fn directional_sound_pos(player_pos: [f64; 3], ping_pos: [f64; 3]) -> [f64; 3] {
    let dx = ping_pos[0] - player_pos[0];
    let dy = ping_pos[1] - player_pos[1];
    let dz = ping_pos[2] - player_pos[2];
    let len = (dx * dx + dy * dy + dz * dz).sqrt();
    if len <= 1e-6 || !len.is_finite() {
        return player_pos;
    }
    let mapped = (len.min(64.0) / 64.0) * 14.0;
    let scale = mapped / len;
    [
        player_pos[0] + dx * scale,
        player_pos[1] + dy * scale,
        player_pos[2] + dz * scale,
    ]
}

/// Distance-based ping icon/label scale factor matching `PingView.calculateScale`.
pub fn calculate_ping_scale(distance: f64, ping_size_percent: i32) -> f32 {
    let d = distance.max(1e-6);
    let base = (2.0 / d.powf(0.3)).max(1.0);
    let ping_size = ping_size_percent as f32 / 100.0;
    (base as f32) * 0.5 * ping_size
}

/// Converts a vanilla `TeamColor` to packed `0xAARRGGBB` (matching `ChatFormatting`).
pub fn team_color_to_argb(color: TeamColor) -> u32 {
    let rgb = match color {
        TeamColor::Black => 0x00_0000,
        TeamColor::DarkBlue => 0x00_00AA,
        TeamColor::DarkGreen => 0x00_AA00,
        TeamColor::DarkAqua => 0x00_AAAA,
        TeamColor::DarkRed => 0xAA_0000,
        TeamColor::DarkPurple => 0xAA_00AA,
        TeamColor::Gold => 0xFF_AA00,
        TeamColor::Gray => 0xAA_AAAA,
        TeamColor::DarkGray => 0x55_5555,
        TeamColor::Blue => 0x55_55FF,
        TeamColor::Green => 0x55_FF55,
        TeamColor::Aqua => 0x55_FFFF,
        TeamColor::Red => 0xFF_5555,
        TeamColor::LightPurple => 0xFF_55FF,
        TeamColor::Yellow => 0xFF_FF55,
        TeamColor::White => 0xFF_FFFF,
        _ => 0xFF_FFFF,
    };
    0xFF00_0000 | rgb
}

/// Formats a distance in meters with comma thousands separators and one decimal place,
/// matching Java's `"%,.1f"`.
pub fn format_distance(distance: f64) -> String {
    let d = if distance.is_finite() && distance >= 0.0 {
        distance
    } else {
        0.0
    };
    let raw = format!("{d:.1}");
    let Some((int_part, frac_part)) = raw.split_once('.') else {
        return raw;
    };
    let mut grouped = String::with_capacity(int_part.len() + int_part.len() / 3 + 2);
    let len = int_part.len();
    for (idx, ch) in int_part.chars().enumerate() {
        if idx > 0 && (len - idx).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    grouped.push('.');
    grouped.push_str(frac_part);
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI};

    #[test]
    fn angle_rect_intersection_cardinal_directions() {
        let tl = [10.0, 20.0];
        let br = [210.0, 120.0];

        let right = calculate_angle_rect_intersection(0.0, tl, br);
        assert!((right[0] - 210.0).abs() < 1e-3);
        assert!((right[1] - 70.0).abs() < 1e-3);

        let bottom = calculate_angle_rect_intersection(FRAC_PI_2, tl, br);
        assert!((bottom[0] - 110.0).abs() < 1e-3);
        assert!((bottom[1] - 120.0).abs() < 1e-3);

        let left = calculate_angle_rect_intersection(PI, tl, br);
        assert!((left[0] - 10.0).abs() < 1e-3);
        assert!((left[1] - 70.0).abs() < 1e-3);

        let top = calculate_angle_rect_intersection(-FRAC_PI_2, tl, br);
        assert!((top[0] - 110.0).abs() < 1e-3);
        assert!((top[1] - 20.0).abs() < 1e-3);
    }

    #[test]
    fn directional_sound_clamps_distance_at_64() {
        let p = [0.0, 64.0, 0.0];
        let near = directional_sound_pos(p, [32.0, 64.0, 0.0]);
        assert!((near[0] - 7.0).abs() < 1e-6);
        assert!((near[1] - 64.0).abs() < 1e-6);

        let far = directional_sound_pos(p, [500.0, 64.0, 0.0]);
        assert!((far[0] - 14.0).abs() < 1e-6);

        let same = directional_sound_pos(p, p);
        assert_eq!(same, p);
    }

    #[test]
    fn ping_scale_shrinks_with_distance_and_clamps_at_half() {
        let close = calculate_ping_scale(1.0, 100);
        let mid = calculate_ping_scale(10.0, 100);
        let far = calculate_ping_scale(500.0, 100);
        assert!((close - 1.0).abs() < 1e-4);
        assert!(mid < close);
        assert!((far - 0.5).abs() < 1e-4);
        assert!((calculate_ping_scale(500.0, 200) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn view_direction_matches_vanilla_axes() {
        let south = view_direction(0.0, 0.0);
        assert!(south[0].abs() < 1e-6 && south[1].abs() < 1e-6 && (south[2] - 1.0).abs() < 1e-6);

        let west = view_direction(90.0, 0.0);
        assert!((west[0] + 1.0).abs() < 1e-6 && west[1].abs() < 1e-6 && west[2].abs() < 1e-6);

        let up = view_direction(0.0, -90.0);
        assert!(up[0].abs() < 1e-6 && (up[1] - 1.0).abs() < 1e-6 && up[2].abs() < 1e-6);
    }

    #[test]
    fn format_distance_adds_thousands_commas() {
        assert_eq!(format_distance(0.0), "0.0");
        assert_eq!(format_distance(12.34), "12.3");
        assert_eq!(format_distance(1234.56), "1,234.6");
    }
}
