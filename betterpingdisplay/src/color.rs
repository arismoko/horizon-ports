//! Latency-based RGB colour interpolation matching Better Ping Display's `PingColors` and `ColorUtil`.

pub const PING_START: i32 = 0;
pub const PING_MID: i32 = 150;
pub const PING_END: i32 = 300;

pub const ALPHA_MASK: u32 = 0xFF00_0000;

pub const COLOR_GREY: u32 = 0x535353;
pub const COLOR_START: u32 = 0x00E676;
pub const COLOR_MID: u32 = 0xD6CD30;
pub const COLOR_END: u32 = 0xE53935;

/// Extract the red channel (`0..=255`) from a `0xRRGGBB` or `0xAARRGGBB` colour.
pub const fn get_red(color: u32) -> u8 {
    ((color >> 16) & 0xFF) as u8
}

/// Extract the green channel (`0..=255`) from a `0xRRGGBB` or `0xAARRGGBB` colour.
pub const fn get_green(color: u32) -> u8 {
    ((color >> 8) & 0xFF) as u8
}

/// Extract the blue channel (`0..=255`) from a `0xRRGGBB` or `0xAARRGGBB` colour.
pub const fn get_blue(color: u32) -> u8 {
    (color & 0xFF) as u8
}

/// Linearly interpolate between two `0xRRGGBB` colours by `offset` (clamped to `0.0..=1.0`).
///
/// Uses Java `Math.round(float)` semantics (`floor(x + 0.5)`) on each channel so the result
/// matches the original mod bit-for-bit.
pub fn interpolate(color_start: u32, color_end: u32, offset: f32) -> u32 {
    let offset = if offset.is_finite() {
        offset.clamp(0.0, 1.0)
    } else {
        0.0
    };

    let r0 = i32::from(get_red(color_start));
    let g0 = i32::from(get_green(color_start));
    let b0 = i32::from(get_blue(color_start));

    let red_diff = i32::from(get_red(color_end)) - r0;
    let green_diff = i32::from(get_green(color_end)) - g0;
    let blue_diff = i32::from(get_blue(color_end)) - b0;

    let new_red = java_round_u8(r0 as f32 + (red_diff as f32 * offset));
    let new_green = java_round_u8(g0 as f32 + (green_diff as f32 * offset));
    let new_blue = java_round_u8(b0 as f32 + (blue_diff as f32 * offset));

    (u32::from(new_red) << 16) | (u32::from(new_green) << 8) | u32::from(new_blue)
}

fn java_round_u8(value: f32) -> u8 {
    (value + 0.5).floor().clamp(0.0, 255.0) as u8
}

fn compute_offset(start: i32, end: i32, value: i32) -> f32 {
    let offset = (value - start) as f32 / (end - start) as f32;
    offset.clamp(0.0, 1.0)
}

/// Compute the `0xRRGGBB` auto-colour for `ping` in milliseconds.
pub fn ping_color_rgb(ping: i32) -> u32 {
    if ping < PING_START {
        return COLOR_GREY;
    }

    if ping < PING_MID {
        return interpolate(
            COLOR_START,
            COLOR_MID,
            compute_offset(PING_START, PING_MID, ping),
        );
    }

    interpolate(
        COLOR_MID,
        COLOR_END,
        compute_offset(PING_MID, PING_END, ping.min(PING_END)),
    )
}

/// Compute the opaque ARGB (`0xFFRRGGBB`) auto-colour for `ping` in milliseconds.
pub fn ping_color(ping: i32) -> u32 {
    ping_color_rgb(ping) | ALPHA_MASK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_components_matches_original() {
        let color = 0xD6C130;
        assert_eq!(get_red(color), 0xD6);
        assert_eq!(get_green(color), 0xC1);
        assert_eq!(get_blue(color), 0x30);
    }

    #[test]
    fn interpolation_endpoints_and_midpoint() {
        assert_eq!(interpolate(COLOR_START, COLOR_MID, 0.0), COLOR_START);
        let mid = interpolate(COLOR_START, COLOR_MID, 0.5);
        assert_eq!(get_red(mid), 0x6B);
        assert_eq!(get_green(mid), 0xDA);
        assert_eq!(get_blue(mid), 0x53);
        assert_eq!(interpolate(COLOR_START, COLOR_MID, 1.0), COLOR_MID);
    }

    #[test]
    fn interpolation_clamps_out_of_range_and_non_finite_without_panicking() {
        assert_eq!(interpolate(COLOR_START, COLOR_MID, -0.5), COLOR_START);
        assert_eq!(interpolate(COLOR_START, COLOR_MID, 1.5), COLOR_MID);
        assert_eq!(interpolate(COLOR_START, COLOR_MID, f32::NAN), COLOR_START);
        assert_eq!(
            interpolate(COLOR_START, COLOR_MID, f32::INFINITY),
            COLOR_START
        );
    }

    #[test]
    fn ping_color_thresholds() {
        assert_eq!(ping_color(-1), COLOR_GREY | ALPHA_MASK);
        assert_eq!(ping_color(i32::MIN), COLOR_GREY | ALPHA_MASK);
        assert_eq!(ping_color(0), COLOR_START | ALPHA_MASK);
        assert_eq!(ping_color(150), COLOR_MID | ALPHA_MASK);
        assert_eq!(ping_color(300), COLOR_END | ALPHA_MASK);
        assert_eq!(ping_color(9999), COLOR_END | ALPHA_MASK);
    }
}
