//! HUD layer rendering and procedural 11-tier experience orb sprite sheet.

use crate::config::{ClientConfig, EntryBackground};
use crate::entries::{
    DisplayEntry, ELEMENT_HEIGHT, EntryKind, ITEM_STACK_POP_TIME, ItemEntry, TEXT_ITEM_MARGIN,
    covered_area_alpha, shorten_value, vertical_move_offset, xp_orb_color, xp_texture_index,
};
use mod_api::client::hud_layout::PoseOp;
use mod_api::{
    ClientHost, DrawContext, GuiBlitRegion, GuiCanvas, GuiDrawError, HudMode, Identifier, Text,
};
use std::sync::LazyLock;

/// Vanilla's experience orb sheet: a 64x64 grid of 16x16 icons, so resource packs apply.
const XP_ORB_TEXTURE: &str = "textures/entity/experience/experience_orb.png";

static XP_ORB_TEXTURE_ID: LazyLock<Option<Identifier>> =
    LazyLock::new(|| Identifier::from_parts("minecraft", XP_ORB_TEXTURE).ok());

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

    // 2. Entry text and 3. entry sprite (item icon or experience orb), one scoped group.
    let text = entry.build_text(config);
    let text_width = host.text_width(&text);
    let sprite_x = if mirror {
        pos_x + text_width + if text_width == 0 { 0 } else { TEXT_ITEM_MARGIN }
    } else {
        pos_x
    };
    let count_label = match &entry.kind {
        EntryKind::Item(_)
            if with_sprite
                && config.display.display_amount.is_sprite()
                && (entry.amount > 1 || config.display.display_single_count) =>
        {
            let label = shorten_value(entry.amount);
            let label_w = host.text_width(&Text::literal(&label)).max(1) as f32;
            Some((Text::literal(label), label_w))
        }
        _ => None,
    };
    let _ = draw.draw_gui(&mut |canvas| {
        canvas.scope(&[PoseOp::Scale(scale, scale)], None, &mut |c| {
            if text_width > 0 {
                let _ = c.text(
                    &text,
                    [text_start_x as f32, (pos_y + 4) as f32],
                    argb_white(alpha),
                    true,
                );
            }
            if with_sprite {
                match &entry.kind {
                    EntryKind::Item(item) => {
                        // Item models take no alpha tint, so the icon stays opaque until the
                        // entry is gone (gap: sunrise-org/sunrise#5594).
                        if alpha > 0.05 {
                            let _ = draw_item(c, item, sprite_x, pos_y, partial_tick);
                        }
                        if let Some((label, label_w)) = &count_label {
                            let count_scale = (16.0 / label_w).min(1.0);
                            let cx = (sprite_x as f32 + 17.0) - label_w * count_scale;
                            let cy = (pos_y as f32 + 18.0) - 9.0 * count_scale;
                            let _ = c.scope(
                                &[
                                    PoseOp::Translate(cx, cy),
                                    PoseOp::Scale(count_scale, count_scale),
                                ],
                                None,
                                &mut |label_canvas| {
                                    label_canvas.text(label, [0.0, 0.0], argb_white(alpha), true)
                                },
                            );
                        }
                    }
                    EntryKind::Experience { tick_count, .. } => {
                        if let Some(id) = XP_ORB_TEXTURE_ID.as_ref() {
                            let tier = xp_texture_index(entry.amount);
                            let tint = xp_orb_color(*tick_count as f32 / 2.0, alpha);
                            let _ = c.texture(
                                id,
                                [sprite_x as f32, pos_y as f32, 16.0, 16.0],
                                GuiBlitRegion::new(
                                    [((tier % 4) * 16) as f32, ((tier / 4) * 16) as f32],
                                    [16, 16],
                                    [64, 64],
                                ),
                                tint,
                            );
                        }
                    }
                }
            }
            Ok(())
        })
    });
}

/// Draws an item icon with vanilla's hotbar pop-in squash (`Gui.renderSlot`), then its
/// decorations (bar, cooldown) outside the squeeze. The count is drawn by the caller.
fn draw_item(
    canvas: &mut dyn GuiCanvas,
    item: &ItemEntry,
    x: i32,
    y: i32,
    partial_tick: f32,
) -> Result<(), GuiDrawError> {
    let at = [x as f32, y as f32];
    let pop = item.pop_time as f32 - partial_tick;
    if item.pop_time > 0 && pop > 0.0 {
        let grow = 1.0 + pop / (ITEM_STACK_POP_TIME as f32);
        let (px, py) = (x as f32 + 8.0, y as f32 + 12.0);
        canvas.scope(
            &[
                PoseOp::Translate(px, py),
                PoseOp::Scale(1.0 / grow, (grow + 1.0) / 2.0),
                PoseOp::Translate(-px, -py),
            ],
            None,
            &mut |model| model.item(&item.stack, at, 0),
        )?;
    } else {
        canvas.item(&item.stack, at, 0)?;
    }
    canvas.item_decorations(&item.stack, at, Some(""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argb_white_packs_alpha_channel() {
        assert_eq!(argb_white(1.0), 0xFFFF_FFFF);
        assert_eq!(argb_white(0.0), 0x00FF_FFFF);
        assert_eq!(argb_white(0.5), 0x80FF_FFFF);
    }
}
