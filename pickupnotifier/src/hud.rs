//! HUD layer rendering and procedural 11-tier experience orb sprite sheet.

use crate::config::{ClientConfig, EntryBackground, MOD_ID};
use crate::entries::{
    DisplayEntry, ELEMENT_HEIGHT, EntryKind, ITEM_STACK_POP_TIME, TEXT_ITEM_MARGIN,
    covered_area_alpha, shorten_value, vertical_move_offset, xp_orb_color, xp_texture_index,
};
use mod_api::client::hud_layout::PoseOp;
use mod_api::{ClientHost, DrawContext, GuiDraw, HudMode, Identifier, Text};

pub const XP_ORB_TEXTURE_PATH: &str = "xp_orb_sheet";
pub const XP_ORB_SPRITE_ID: &str = "pickupnotifier:xp_orb_sheet";

pub fn ensure_xp_orb_texture(host: &mut dyn ClientHost) -> bool {
    let Ok(id) = Identifier::from_parts(MOD_ID, XP_ORB_TEXTURE_PATH) else {
        return false;
    };
    let rgba = generate_xp_orb_sheet_rgba();
    host.create_dynamic_texture(&id, 64, 64, &rgba).is_ok()
}

/// Generates an original 64x64 RGBA sprite sheet containing 11 glowing experience orb tiers
/// arranged in a 4x3 grid of 16x16 cells (matching vanilla's 0..=10 orb icon indices).
pub fn generate_xp_orb_sheet_rgba() -> Vec<u8> {
    let mut buf = vec![0u8; 64 * 64 * 4];
    for tier in 0u32..11 {
        let cell_x = (tier % 4) * 16;
        let cell_y = (tier / 4) * 16;
        // Radius grows smoothly from 2.6 px (tier 0) to 7.2 px (tier 10).
        let radius = 2.6 + (tier as f32) * 0.46;
        for py in 0u32..16 {
            for px in 0u32..16 {
                let dx = px as f32 + 0.5 - 8.0;
                let dy = py as f32 + 0.5 - 8.0;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist <= radius {
                    let t = (dist / radius).clamp(0.0, 1.0);
                    // Bright white-tinted core with soft translucent halo so tinting pops.
                    let alpha = if t < 0.65 {
                        255.0
                    } else {
                        ((1.0 - (t - 0.65) / 0.35).clamp(0.0, 1.0)) * 230.0
                    };
                    let highlight =
                        if (dx + radius * 0.25).hypot(dy + radius * 0.25) < radius * 0.45 {
                            255u8
                        } else {
                            (255.0 - t * 48.0).clamp(180.0, 255.0) as u8
                        };
                    let idx = (((cell_y + py) * 64 + (cell_x + px)) * 4) as usize;
                    buf[idx] = highlight;
                    buf[idx + 1] = highlight;
                    buf[idx + 2] = highlight;
                    buf[idx + 3] = alpha.round() as u8;
                }
            }
        }
    }
    buf
}

fn argb_white(alpha: f32) -> u32 {
    let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
    (a << 24) | 0x00FF_FFFF
}

/// Renders all active pick-up entries onto `draw` in GUI pixels.
pub fn render_entries(
    host: &dyn ClientHost,
    draw: &mut dyn DrawContext,
    entries: &[DisplayEntry],
    config: &ClientConfig,
    partial_tick: f32,
    xp_texture_ready: bool,
) {
    if entries.is_empty() || host.hud_mode() != HudMode::AllVisible {
        return;
    }

    let [gui_w, gui_h] = draw.size();
    let scale = config.display.scale();
    if scale <= 0.0 || !scale.is_finite() {
        return;
    }

    let screen_width = (gui_w / scale) as i32;
    let screen_height = (gui_h / scale) as i32;
    let anchor = config.display.screen_corner;

    let ordered: Vec<&DisplayEntry> = if anchor.is_top() {
        entries.iter().collect()
    } else {
        entries.iter().rev().collect()
    };

    let widths: Vec<i32> = ordered
        .iter()
        .map(|entry| entry.entry_width(host, config))
        .collect();
    let max_width = widths.iter().copied().max().unwrap_or(0);
    let total_height = ELEMENT_HEIGHT * ordered.len() as i32;

    let offset_x = (config.display.offset_x as f32 / scale) as i32;
    let offset_y = (config.display.offset_y as f32 / scale) as i32;

    let orig_x = anchor.pos_x(screen_width, max_width, offset_x);
    let orig_y = anchor.pos_y(screen_height, total_height, offset_y);

    let move_time = config.behavior.effective_move_time();
    let move_y = if config.behavior.move_out.moves_vertical(anchor) {
        vertical_move_offset(&ordered, partial_tick, move_time)
    } else {
        0
    };

    let pos_x = orig_x;
    let pos_y = anchor.pos_y(screen_height, total_height, offset_y - move_y);

    let moves_h = config.behavior.move_out.moves_horizontal(anchor);
    let moves_v = config.behavior.move_out.moves_vertical(anchor);
    let bg_opacity_setting = host.gui_options().text_background_opacity;

    let mut element_y = pos_y;
    for (entry, &element_width) in ordered.iter().zip(widths.iter()) {
        let mut element_x = pos_x + anchor.pos_x(max_width, element_width, 0);
        if moves_h {
            let rel = entry.relative_remaining_ticks(partial_tick, move_time);
            element_x +=
                (max_width as f64 * (1.0 - f64::from(rel)) * anchor.normal_x() as f64) as i32;
        }

        let alpha = if !config.behavior.fade_out {
            1.0
        } else if moves_h || moves_v {
            covered_area_alpha(
                element_x,
                element_y,
                element_width,
                orig_x,
                orig_y,
                max_width,
                total_height,
            )
        } else {
            entry.relative_remaining_ticks(partial_tick, move_time)
        };

        if alpha > 0.01 {
            render_single_entry(
                host,
                draw,
                entry,
                config,
                element_x,
                element_y,
                element_width,
                alpha,
                partial_tick,
                bg_opacity_setting,
                xp_texture_ready,
            );
        }

        element_y += ELEMENT_HEIGHT;
    }
}

#[allow(clippy::too_many_arguments)]
fn render_single_entry(
    host: &dyn ClientHost,
    draw: &mut dyn DrawContext,
    entry: &DisplayEntry,
    config: &ClientConfig,
    pos_x: i32,
    pos_y: i32,
    entry_width: i32,
    alpha: f32,
    partial_tick: f32,
    bg_opacity_setting: f32,
    xp_texture_ready: bool,
) {
    let scale = config.display.scale();
    let mirror = config.display.screen_corner.is_right();
    let with_sprite = config.display.draw_sprite;
    let text_start_x = if mirror || !with_sprite {
        pos_x
    } else {
        pos_x + 16 + TEXT_ITEM_MARGIN
    };

    // 1. Entry background
    match config.display.entry_background {
        EntryBackground::None => {}
        EntryBackground::Chat => {
            let bg_alpha = (bg_opacity_setting * alpha).clamp(0.0, 1.0);
            let bg_color = ((bg_alpha * 255.0).round() as u32) << 24;
            let height = 16 + i32::from(config.display.display_amount.is_sprite());
            draw.fill(
                [
                    (pos_x - 3) as f32 * scale,
                    pos_y as f32 * scale,
                    (entry_width + 8) as f32 * scale,
                    height as f32 * scale,
                ],
                bg_color,
            );
        }
        EntryBackground::Tooltip => {
            let x0 = (pos_x - 12) as f32 * scale;
            let y0 = (pos_y + 3 - 12) as f32 * scale;
            let w = (entry_width + 24) as f32 * scale;
            let h = (9 + 24) as f32 * scale;
            let tint = argb_white(alpha);
            draw.gui_nine_slice("tooltip/background", [x0, y0, w, h], tint);
            draw.gui_nine_slice("tooltip/frame", [x0, y0, w, h], tint);
        }
    }

    // 2. Entry text (scaled with GuiDraw::Push(Scale) + GuiDraw::Text)
    let text = entry.build_text(config);
    let text_width = host.text_width(&text);
    if text_width > 0 {
        let _ = draw.gui_paint(&[
            GuiDraw::Push(vec![PoseOp::Scale(scale, scale)]),
            GuiDraw::Text {
                text,
                x: text_start_x,
                y: pos_y + 4,
                color: argb_white(alpha),
                shadow: true,
            },
            GuiDraw::Pop,
        ]);
    }

    // 3. Entry sprite (item icon or experience orb)
    if with_sprite {
        let sprite_x = if mirror {
            pos_x + text_width + if text_width == 0 { 0 } else { TEXT_ITEM_MARGIN }
        } else {
            pos_x
        };

        match &entry.kind {
            EntryKind::Item {
                stack, pop_time, ..
            } => {
                // DrawContext::item_stack does not yet support partial alpha tinting (gap: sunrise-org/sunrise#5594).
                if alpha > 0.05 {
                    let (draw_x, draw_y, item_size) = if *pop_time > 0 {
                        let pt = (*pop_time as f32 - partial_tick).max(0.0);
                        let pop_scale = 1.0 + pt / (ITEM_STACK_POP_TIME as f32);
                        let size = 16.0 * scale * ((pop_scale + 1.0) * 0.5).clamp(1.0, 1.35);
                        let x = (sprite_x as f32 + 8.0) * scale - size * 0.5;
                        let y = (pos_y as f32 + 12.0) * scale - size * 0.75;
                        (x, y, size)
                    } else {
                        (sprite_x as f32 * scale, pos_y as f32 * scale, 16.0 * scale)
                    };
                    let _ = draw.item_stack(stack, [draw_x, draw_y], item_size);
                }

                if config.display.display_amount.is_sprite()
                    && (entry.amount > 1 || config.display.display_single_count)
                {
                    let label = shorten_value(entry.amount);
                    let label_w = host.text_width(&Text::literal(&label)).max(1) as f32;
                    let count_scale = (16.0 / label_w).min(1.0);
                    let total_scale = scale * count_scale;
                    let cx = (sprite_x as f32 + 17.0) * scale - label_w * total_scale;
                    let cy = (pos_y as f32 + 18.0) * scale - 9.0 * total_scale;
                    draw.text_scaled(&label, [cx, cy], total_scale, argb_white(alpha), true);
                }
            }
            EntryKind::Experience { tick_count, .. } => {
                if xp_texture_ready {
                    let tier = xp_texture_index(entry.amount);
                    let tx = (tier % 4) * 16;
                    let ty = (tier / 4) * 16;
                    let tint = xp_orb_color(*tick_count as f32 / 2.0, alpha);
                    let _ = draw.sprite(
                        XP_ORB_SPRITE_ID,
                        [tx as f32, ty as f32, 16.0, 16.0],
                        [
                            sprite_x as f32 * scale,
                            pos_y as f32 * scale,
                            16.0 * scale,
                            16.0 * scale,
                        ],
                        Some(tint),
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xp_orb_sheet_has_expected_size_and_nonempty_tiers() {
        let rgba = generate_xp_orb_sheet_rgba();
        assert_eq!(rgba.len(), 64 * 64 * 4);
        // Every tier 0..=10 has an opaque center pixel at (cell_x + 8, cell_y + 8).
        for tier in 0u32..11 {
            let cx = (tier % 4) * 16 + 8;
            let cy = (tier / 4) * 16 + 8;
            let alpha = rgba[((cy * 64 + cx) * 4 + 3) as usize];
            assert_eq!(alpha, 255, "tier {tier} center should be opaque");
        }
    }

    #[test]
    fn argb_white_packs_alpha_channel() {
        assert_eq!(argb_white(1.0), 0xFFFF_FFFF);
        assert_eq!(argb_white(0.0), 0x00FF_FFFF);
        assert_eq!(argb_white(0.5), 0x80FF_FFFF);
    }
}
