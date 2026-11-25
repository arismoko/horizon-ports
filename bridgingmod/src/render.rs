//! World block-outline rendering and crosshair directional indicator.

use crate::config::BridgingConfig;
use crate::math::Direction;
use mod_api::client::hud_layout::Blend;
use mod_api::{ClientHost, DrawContext, HudMode, WorldDepth, WorldDrawContext, WorldLine};

/// Size in GUI pixels of the directional crosshair indicator sprites (`CrosshairRenderingMixin.ICON_SIZE`).
const ICON_SIZE: i32 = 32;

/// Default screen-space line width in pixels for block outlines.
const OUTLINE_LINE_WIDTH: f32 = 2.0;

/// F3 debug trace color (`Render.cubeTrace`: 0x16333333).
const DEBUG_TRACE_COLOR: u32 = 0x1633_3333;

/// Snapshot of targeting state needed during rendering.
#[derive(Clone, Debug, Default)]
pub struct RenderSnapshot {
    /// Active reach-around bridging target `(placement_pos, support_direction)`.
    pub bridging_target: Option<([i32; 3], Direction)>,
    /// Non-bridging placement spot `(hit_pos + hit_face)` when aiming at a normal block
    /// while holding a placeable item and not intersecting the player's bounding box.
    pub non_bridging_target: Option<[i32; 3]>,
    /// Whether the local player is currently crouching.
    pub is_crouching: bool,
    /// Voxels checked along the view path (populated when F3 debug trace is enabled).
    pub debug_trace: Vec<[i32; 3]>,
}

/// Build the 12 wireframe edges of a unit cube at `pos` (`[x, y, z]..[x+1, y+1, z+1]`).
pub fn cube_outline_lines(pos: [i32; 3], argb: u32, width: f32) -> [WorldLine; 12] {
    let x0 = f64::from(pos[0]);
    let y0 = f64::from(pos[1]);
    let z0 = f64::from(pos[2]);
    let x1 = x0 + 1.0;
    let y1 = y0 + 1.0;
    let z1 = z0 + 1.0;

    let l =
        |from: [f64; 3], to: [f64; 3]| WorldLine::new(from, to, argb, width, WorldDepth::Tested);

    [
        // Bottom face
        l([x0, y0, z0], [x1, y0, z0]),
        l([x1, y0, z0], [x1, y0, z1]),
        l([x1, y0, z1], [x0, y0, z1]),
        l([x0, y0, z1], [x0, y0, z0]),
        // Top face
        l([x0, y1, z0], [x1, y1, z0]),
        l([x1, y1, z0], [x1, y1, z1]),
        l([x1, y1, z1], [x0, y1, z1]),
        l([x0, y1, z1], [x0, y1, z0]),
        // Vertical pillars
        l([x0, y0, z0], [x0, y1, z0]),
        l([x1, y0, z0], [x1, y1, z0]),
        l([x1, y0, z1], [x1, y1, z1]),
        l([x0, y0, z1], [x0, y1, z1]),
    ]
}

/// Render bridging outline, optional non-bridging outline, and optional F3 debug trace
/// (`OutlineRendererMixin.renderTracedViewPath`).
pub fn render_world_outlines(
    host: &dyn ClientHost,
    draw: &mut dyn WorldDrawContext,
    snapshot: &RenderSnapshot,
    config: &BridgingConfig,
) {
    let in_debug_menu = host.debug_overlay_visible();

    let bridging_active = config.enable_bridging_assist
        && (!config.only_bridge_when_crouched || snapshot.is_crouching);

    let should_render_outline = if in_debug_menu {
        config.show_debug_highlight
    } else {
        config.show_outline
    };
    let outline_enabled = should_render_outline && bridging_active;

    let should_render_non_bridge = if in_debug_menu {
        config.show_non_bridging_debug_highlight
    } else {
        config.show_outline_even_when_not_bridging
    };
    let non_bridge_enabled =
        should_render_non_bridge && (bridging_active || !config.non_bridge_respects_crouch_rules);

    if !(outline_enabled || non_bridge_enabled || (in_debug_menu && config.show_debug_trace)) {
        return;
    }

    if in_debug_menu && config.show_debug_trace {
        for &pos in &snapshot.debug_trace {
            let lines = cube_outline_lines(pos, DEBUG_TRACE_COLOR, OUTLINE_LINE_WIDTH);
            if draw.lines(&lines).is_err() {
                break;
            }
        }
    }

    let outline_argb = config.outline_argb();
    if outline_argb >> 24 == 0 {
        return;
    }

    if outline_enabled && let Some((pos, _)) = snapshot.bridging_target {
        let lines = cube_outline_lines(pos, outline_argb, OUTLINE_LINE_WIDTH);
        let _ = draw.lines(&lines);
    }

    if non_bridge_enabled && let Some(pos) = snapshot.non_bridging_target {
        let lines = cube_outline_lines(pos, outline_argb, OUTLINE_LINE_WIDTH);
        let _ = draw.lines(&lines);
    }
}

/// Render the directional crosshair placement indicator (`CrosshairRenderingMixin.renderPlacementAssistMarker`).
pub fn render_crosshair_indicator(
    host: &dyn ClientHost,
    draw: &mut dyn DrawContext,
    snapshot: &RenderSnapshot,
    config: &BridgingConfig,
) {
    if !config.show_crosshair {
        return;
    }
    if host.hud_mode() != HudMode::AllVisible || host.observed_screen_id().is_some() {
        return;
    }
    let bridging_active = config.enable_bridging_assist
        && (!config.only_bridge_when_crouched || snapshot.is_crouching);
    if !bridging_active {
        return;
    }
    let Some((_, direction)) = snapshot.bridging_target else {
        return;
    };

    let alignment = direction.indicator_alignment();
    let [gui_w, gui_h] = draw.size();
    let w = gui_w.floor() as i32;
    let h = gui_h.floor() as i32;

    let x = (w - ICON_SIZE + 1) / 2;
    let mut y = (h - ICON_SIZE + 1) / 2;
    if host.debug_overlay_visible() {
        y += 15;
    }

    let sprite = alignment.sprite_name();
    let _ = draw.draw_gui(&mut |canvas| {
        canvas.blit_sprite(
            sprite,
            [x as f32, y as f32, ICON_SIZE as f32, ICON_SIZE as f32],
            None,
            0xFFFF_FFFF,
            Blend::Crosshair,
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_outline_has_twelve_unit_edges() {
        let lines = cube_outline_lines([1, 64, -2], 0x6600_0000, 2.0);
        assert_eq!(lines.len(), 12);
        for line in &lines {
            let dx = line.to[0] - line.from[0];
            let dy = line.to[1] - line.from[1];
            let dz = line.to[2] - line.from[2];
            let len = (dx * dx + dy * dy + dz * dz).sqrt();
            assert!((len - 1.0).abs() < 1e-9);
            assert_eq!(line.color, 0x6600_0000);
            assert_eq!(line.depth, WorldDepth::Tested);
        }
    }
}
