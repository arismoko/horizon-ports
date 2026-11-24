//! HUD rendering (status banners & battery indicator) and original 16x16 procedural
//! battery toast textures.

use crate::battery::{BatteryNotification, BatterySnapshot, indicator_condition_met};
use crate::config::{DynamicFpsConfig, MOD_ID};
use mod_api::{ClientHost, DrawContext, HudMode, Identifier, Text};

pub const TOAST_ICON_CHARGING: &str = "battery_toast_charging";
pub const TOAST_ICON_DRAINING: &str = "battery_toast_draining";
pub const TOAST_ICON_CRITICAL: &str = "battery_toast_critical";

pub fn toast_icon_id(notification: BatteryNotification) -> Identifier {
    let path = match notification {
        BatteryNotification::Charging => TOAST_ICON_CHARGING,
        BatteryNotification::Discharging => TOAST_ICON_DRAINING,
        BatteryNotification::Critical => TOAST_ICON_CRITICAL,
    };
    Identifier::from_parts(MOD_ID, path).expect("static valid identifier")
}

/// Draw Dynamic FPS HUD elements (`HudInfoRenderer.renderInfo`):
/// - Battery icon + percentage in the configured corner
/// - Centered status banner when disabled or forcing reduced FPS via keybind
pub fn render_hud(
    host: &dyn ClientHost,
    draw: &mut dyn DrawContext,
    config: &DynamicFpsConfig,
    battery: BatterySnapshot,
    disabled_by_user: bool,
    forcing_low_fps: bool,
) {
    if host.hud_mode() != HudMode::AllVisible || host.observed_screen_id().is_some() {
        return;
    }

    let [gui_w, gui_h] = draw.size();

    if config.battery.enabled
        && (config.battery.show_when_debug || !host.debug_overlay_visible())
        && indicator_condition_met(
            config.battery.display_condition,
            battery,
            config.battery.critical_level,
        )
    {
        let [x, y] = config.battery.display_placement.origin(gui_w, gui_h);
        draw_battery_icon(
            draw,
            x,
            y,
            battery.charge,
            battery.state.is_charging(),
            config.battery.critical_level,
        );
        let pct = format!("{}%", battery.charge);
        draw.text(&pct, [x + 20.0, y + 4.0], 0xFFFF_FFFF, true);
    }

    let banner_key = if disabled_by_user {
        Some(("gui.dynamicfps.hud.disabled", "Dynamic FPS Disabled"))
    } else if forcing_low_fps {
        Some((
            "gui.dynamicfps.hud.reducing",
            "Dynamic FPS: Forcing Reduced FPS",
        ))
    } else {
        None
    };

    if let Some((key, fallback)) = banner_key {
        let label = host.translate(key).unwrap_or_else(|| fallback.to_owned());
        let width = host.text_width(&Text::literal(&label)) as f32;
        let x = ((gui_w - width) * 0.5).floor();
        draw.text(&label, [x, 32.0], 0xFFFF_FFFF, true);
    }
}

/// Draw an original 16x16 procedural battery icon at `(x, y)` in GUI pixels.
fn draw_battery_icon(
    draw: &mut dyn DrawContext,
    x: f32,
    y: f32,
    charge: u8,
    charging: bool,
    critical_level: u8,
) {
    let outline = 0xFF1E_1E24;
    let frame = 0xFFD8_DCE2;
    let empty_bg = 0xCC10_1216;

    // Outer dark shadow & crisp frame (13x10 body + 2x4 positive terminal on the right).
    draw.fill([x + 1.0, y + 3.0, 12.0, 10.0], outline);
    draw.fill([x + 13.0, y + 5.0, 2.0, 6.0], outline);
    draw.fill([x + 2.0, y + 4.0, 10.0, 8.0], frame);
    draw.fill([x + 13.0, y + 6.0, 1.0, 4.0], frame);
    draw.fill([x + 3.0, y + 5.0, 8.0, 6.0], empty_bg);

    // Charge fill bar (0..=8 px wide).
    let fill_color = if charging {
        0xFF4C_D964
    } else if charge <= critical_level {
        0xFFE5_3935
    } else if charge <= 35 {
        0xFFFF_B300
    } else {
        0xFF55_DD66
    };
    let fill_w = ((f32::from(charge.min(100)) / 100.0) * 8.0).round();
    if fill_w > 0.0 {
        draw.fill([x + 3.0, y + 5.0, fill_w, 6.0], fill_color);
    }

    // Small golden lightning bolt accent when charging.
    if charging {
        let bolt = 0xFFFF_EA55;
        draw.fill([x + 7.0, y + 4.0, 2.0, 2.0], bolt);
        draw.fill([x + 6.0, y + 6.0, 4.0, 2.0], bolt);
        draw.fill([x + 6.0, y + 8.0, 2.0, 3.0], bolt);
    }
}

/// Register our three original 16x16 battery toast icons in the client's dynamic texture pool.
pub fn ensure_toast_icons_registered(host: &mut dyn ClientHost) -> bool {
    let icons = [
        (
            BatteryNotification::Charging,
            generate_toast_icon_rgba(BatteryNotification::Charging),
        ),
        (
            BatteryNotification::Discharging,
            generate_toast_icon_rgba(BatteryNotification::Discharging),
        ),
        (
            BatteryNotification::Critical,
            generate_toast_icon_rgba(BatteryNotification::Critical),
        ),
    ];
    let mut ok = true;
    for (kind, rgba) in icons {
        let id = toast_icon_id(kind);
        if host.create_dynamic_texture(&id, 16, 16, &rgba).is_err() {
            ok = false;
        }
    }
    ok
}

/// Generate a crisp original 16x16 RGBA battery icon for a toast notification.
pub fn generate_toast_icon_rgba(kind: BatteryNotification) -> Vec<u8> {
    let mut buf = vec![0u8; 16 * 16 * 4];
    let mut fill = |x0: usize, y0: usize, w: usize, h: usize, rgba: [u8; 4]| {
        for y in y0..(y0 + h).min(16) {
            for x in x0..(x0 + w).min(16) {
                let idx = (y * 16 + x) * 4;
                buf[idx..idx + 4].copy_from_slice(&rgba);
            }
        }
    };

    let outline = [0x1E, 0x1E, 0x24, 0xFF];
    let frame = [0xE2, 0xE6, 0xEC, 0xFF];
    let bg = [0x14, 0x16, 0x1C, 0xFF];

    // Vertical battery silhouette (10x13 body + 4x2 top terminal).
    fill(6, 1, 4, 2, outline);
    fill(7, 2, 2, 1, frame);
    fill(3, 3, 10, 12, outline);
    fill(4, 4, 8, 10, frame);
    fill(5, 5, 6, 8, bg);

    match kind {
        BatteryNotification::Charging => {
            fill(5, 6, 6, 7, [0x4C, 0xD9, 0x64, 0xFF]);
            // Lightning bolt
            let bolt = [0xFF, 0xEA, 0x55, 0xFF];
            fill(8, 5, 2, 3, bolt);
            fill(6, 8, 4, 2, bolt);
            fill(6, 10, 2, 3, bolt);
        }
        BatteryNotification::Discharging => {
            fill(5, 8, 6, 5, [0xFF, 0xB3, 0x00, 0xFF]);
        }
        BatteryNotification::Critical => {
            fill(5, 11, 6, 2, [0xE5, 0x39, 0x35, 0xFF]);
            // Exclamation mark
            let mark = [0xFF, 0x52, 0x52, 0xFF];
            fill(7, 5, 2, 4, mark);
            fill(7, 10, 2, 1, mark);
        }
    }

    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_icon_rgba_buffers_have_exact_16x16_size() {
        for kind in [
            BatteryNotification::Charging,
            BatteryNotification::Discharging,
            BatteryNotification::Critical,
        ] {
            let rgba = generate_toast_icon_rgba(kind);
            assert_eq!(rgba.len(), 16 * 16 * 4);
            assert!(rgba.iter().any(|&b| b != 0));
        }
    }
}
