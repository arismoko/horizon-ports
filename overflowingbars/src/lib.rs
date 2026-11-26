#![deny(unsafe_code)]

//! Overflowing Bars for Sunrise: health, armour and armour toughness stack into coloured rows
//! instead of piling up vertically, each with an optional row count.

mod config;
mod logic;

use crate::config::{Config, IconRow, register_settings};
use crate::logic::{
    ARMOR_LAYER_TINT, HealthTracker, HeartInput, Palette, SpriteOp, TOUGHNESS_LAYER_TINT,
    TOUGHNESS_TINT, WHITE,
};
use mod_api::client::hud_layout::Blend;
use mod_api::{
    ClientHost, CustomizeChatOverlayEvent, DrawContext, GameMode, GuiCanvas, GuiDrawError,
    GuiLayerPlacement, GuiLayerWrapContext, GuiSpriteRegion, Hooks, HudMode,
    RegisterGuiLayersEvent, Registrar, RegistrationContext,
};
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

const LAYER_HEALTH: &str = "minecraft:player_health";
const LAYER_ARMOR: &str = "minecraft:armor_level";
const LAYER_VEHICLE: &str = "minecraft:vehicle_health";
const LAYER_TOUGHNESS_LEFT: &str = "overflowingbars:toughness_level/left";
const LAYER_TOUGHNESS_RIGHT: &str = "overflowingbars:toughness_level/right";

/// Vanilla's first status row sits this far above the bottom edge.
const BASE_HEIGHT: i32 = 39;
const BLACK: u32 = 0xFF00_0000;

/// What the bars read of the local player. On a client only the local player is readable.
#[derive(Clone, Copy, Debug)]
struct Snapshot {
    health: f32,
    max_health: f32,
    absorption: f32,
    armor: i32,
    toughness: i32,
    regenerating: bool,
    poisoned: bool,
    withered: bool,
    /// Whether the world is hardcore (`LevelRef::is_hardcore`), which swaps vanilla's hearts.
    hardcore: bool,
    /// The height of vanilla's right-hand status column as the current layer starts
    /// (`ClientHost::gui_status_heights`), only inside the HUD pass.
    status_right: Option<i32>,
    /// The local player's exact damage cooldown (`EntityRef::damage_cooldown_time`).
    damage_cooldown: Option<i32>,
    tick: i64,
    millis: u64,
}

fn read_snapshot(host: &dyn ClientHost) -> Option<Snapshot> {
    let player = host.local_player()?;
    let living = host.living(player.entity())?;
    let effect = |id: &str| {
        host.effect_key(id)
            .is_some_and(|key| living.potion_affected(host, key))
    };
    let floor = |id: &str| living.attribute_value(host, id).unwrap_or(0.0).floor();
    Some(Snapshot {
        health: living.health(host)?,
        max_health: living.max_health(host).unwrap_or(20.0),
        absorption: living.absorption(host).unwrap_or(0.0),
        armor: floor("minecraft:armor").clamp(0.0, 1e6) as i32,
        toughness: floor("minecraft:armor_toughness").clamp(0.0, 1e6) as i32,
        regenerating: effect("minecraft:regeneration"),
        poisoned: effect("minecraft:poison"),
        withered: effect("minecraft:wither"),
        hardcore: host
            .level(host.dimension())
            .and_then(|level| level.is_hardcore(host))
            .unwrap_or(false),
        status_right: host.gui_status_heights().map(|[_, right]| right),
        damage_cooldown: player.entity().damage_cooldown_time(host),
        tick: i64::from(host.gui_ticks()),
        millis: host.millis(),
    })
}

/// Whether the bars draw at all: a visible HUD and a player who can be hurt
/// (`MultiPlayerGameMode.canHurtPlayer`).
fn bars_visible(host: &dyn ClientHost) -> bool {
    host.hud_mode() == HudMode::AllVisible
        && host.local_player().is_some_and(|player| {
            matches!(
                player.game_mode(host),
                Some(GameMode::Survival | GameMode::Adventure)
            )
        })
}

fn tracker() -> MutexGuard<'static, HealthTracker> {
    static TRACKER: LazyLock<Mutex<HealthTracker>> = LazyLock::new(Mutex::default);
    TRACKER.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Heights of the three bars, in GUI pixels above the bottom edge, as the status-bar stack
/// puts them: each bar starts where the one below ended.
struct Heights {
    health: i32,
    armor: i32,
    toughness_left: i32,
    toughness_right: i32,
    /// The left status column once the health bar is drawn, and once the armor bar is.
    left_after_health: i32,
    left_after_armor: i32,
}

fn heights(cfg: &Config, snap: &Snapshot) -> Heights {
    let two_rows = logic::two_health_rows(snap.max_health, snap.absorption);
    let health_used = if cfg.health.allow_layers {
        (if two_rows { 20 } else { 10 }) + cfg.health.row_shift * logic::ROW_HEIGHT
    } else {
        let absorption = snap.absorption.ceil().clamp(0.0, 1e6) as i32;
        logic::vanilla_health_height(snap.max_health.max(snap.health), absorption)
    };
    let armor_used = if snap.armor > 0 {
        10 + if cfg.armor.allow_layers {
            cfg.armor.row_shift * logic::ROW_HEIGHT
        } else {
            0
        }
    } else {
        0
    };
    let health = BASE_HEIGHT + cfg.health.row_shift * logic::ROW_HEIGHT;
    let armor_base = BASE_HEIGHT + health_used;
    let armor = armor_base + cfg.armor.row_shift * logic::ROW_HEIGHT;
    Heights {
        left_after_health: armor_base,
        left_after_armor: armor_base + armor_used,
        health,
        armor,
        toughness_left: armor_base + armor_used + cfg.toughness.row_shift * logic::ROW_HEIGHT,
        // The bar sits above whatever vanilla's right-hand column holds: the food row, or a
        // mount's hearts, as far as the layer's slot has it.
        toughness_right: snap.status_right.unwrap_or(BASE_HEIGHT + 10)
            + cfg.toughness.row_shift * logic::ROW_HEIGHT,
    }
}

/// Publish the status columns' heights after a bar replaced vanilla's row, as assigning
/// NeoForge's `Hud.leftHeight` / `rightHeight` does, so the rows drawn after it (food, air, other
/// mods' bars) stack above ours. `None` keeps a column as it is. Refused outside the HUD pass.
fn publish_status_heights(host: &dyn ClientHost, left: Option<i32>, right: Option<i32>) {
    let Some([current_left, current_right]) = host.gui_status_heights() else {
        return;
    };
    let _ =
        host.set_gui_status_heights([left.unwrap_or(current_left), right.unwrap_or(current_right)]);
}

/// One primitive of a bar, drawn together with the rest of its bar.
enum Op {
    Sprite(SpriteOp),
    Text {
        text: String,
        x: i32,
        y: i32,
        color: u32,
    },
    Fill {
        rect: [i32; 4],
        color: u32,
    },
}

fn sprite_draws(ops: Vec<SpriteOp>, out: &mut Vec<Op>) {
    out.extend(ops.into_iter().map(Op::Sprite));
}

/// Draw a bar's primitives in order; a refused one leaves the others as they were.
fn draw_ops(draw: &mut dyn DrawContext, ops: &[Op]) {
    let _ = draw.draw_gui(&mut |canvas: &mut dyn GuiCanvas| {
        for op in ops {
            let _: Result<(), GuiDrawError> = match op {
                Op::Sprite(op) => canvas.blit_sprite(
                    &op.sprite,
                    [op.x as f32, op.y as f32, op.w as f32, op.h as f32],
                    op.region.map(|[sw, sh, u, v]| GuiSpriteRegion {
                        size: [sw, sh],
                        offset: [u, v],
                    }),
                    op.color,
                    Blend::Textured,
                ),
                Op::Text { text, x, y, color } => canvas.text(
                    &mod_api::Text::literal(text.clone()),
                    [*x as f32, *y as f32],
                    *color,
                    false,
                ),
                Op::Fill {
                    rect: [x0, y0, x1, y1],
                    color,
                } => canvas.fill(
                    [*x0 as f32, *y0 as f32, (x1 - x0) as f32, (y1 - y0) as f32],
                    *color,
                ),
            };
        }
        Ok(())
    });
}

/// The row count next to a bar, from the bar's edge `(x, y)`; `left` puts it left of the edge.
fn row_count(
    draw: &dyn DrawContext,
    cfg: &Config,
    value: i32,
    max_row: i32,
    left: bool,
    (x, y): (i32, i32),
    out: &mut Vec<Op>,
) {
    let Some(number) =
        logic::row_count_number(value, max_row, cfg.count_always, cfg.count_full_rows_only)
    else {
        return;
    };
    let color = logic::dye_argb(cfg.count_color);
    if cfg.count_force_font {
        let mut text = number.to_string();
        if cfg.count_x {
            text.push('x');
        }
        let px = if left {
            x - draw.text_width(&text).ceil() as i32
        } else {
            x + 5
        };
        let text = |dx: i32, dy: i32, color: u32| Op::Text {
            text: text.clone(),
            x: px + dx,
            y: y + 1 + dy,
            color,
        };
        for (dx, dy) in [
            (-1, -1),
            (0, -1),
            (1, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
        ] {
            out.push(text(dx, dy, BLACK));
        }
        out.push(text(0, 0, color));
        return;
    }
    for (glyph, gx, gy) in logic::row_count_glyphs(number, cfg.count_x, left, x, y) {
        for (dx, dy, tint) in [
            (-1, 0, BLACK),
            (1, 0, BLACK),
            (0, -1, BLACK),
            (0, 1, BLACK),
            (0, 0, color),
        ] {
            for [x0, y0, x1, y1] in logic::glyph_fills(glyph, gx + dx, gy + dy) {
                out.push(Op::Fill {
                    rect: [x0, y0, x1, y1],
                    color: tint,
                });
            }
        }
    }
}

fn paint_health(draw: &mut dyn DrawContext, cfg: &Config, snap: &Snapshot) {
    let size = draw.size();
    let (width, height) = (size[0] as i32, size[1] as i32);
    let x = width / 2 - 91;
    let y = height - heights(cfg, snap).health;
    let current = snap.health.ceil().clamp(-1e6, 1e6) as i32;
    let frame = tracker().update(current, snap.tick, snap.millis, snap.damage_cooldown);
    let max_health = snap
        .max_health
        .max(frame.display as f32)
        .max(current as f32);
    let absorption = snap.absorption.ceil().clamp(0.0, 1e6) as i32;
    let input = HeartInput {
        max_health,
        frame,
        absorption,
        regen_index: logic::regen_index(snap.regenerating, snap.tick, max_health),
        base: logic::base_heart_style(snap.poisoned, snap.withered),
        inverse: cfg.health.inverse_coloring,
        colorize_first_row: cfg.health.colorize_first_row,
        hardcore: snap.hardcore,
    };
    let mut ops = Vec::new();
    sprite_draws(logic::plan_hearts(&input, x, y, snap.tick), &mut ops);
    if cfg.health.allow_count {
        row_count(&*draw, cfg, current, 20, true, (x - 2, y), &mut ops);
        let max_absorption = (20 - (20.min(current) as f32 / 2.0).ceil() as i32) * 2;
        row_count(
            &*draw,
            cfg,
            absorption,
            max_absorption,
            true,
            (x - 2, y - 10),
            &mut ops,
        );
    }
    draw_ops(draw, &ops);
}

fn paint_armor(draw: &mut dyn DrawContext, cfg: &Config, snap: &Snapshot) {
    let size = draw.size();
    let x = size[0] as i32 / 2 - 91;
    let y = size[1] as i32 - heights(cfg, snap).armor;
    let row: &IconRow = &cfg.armor;
    let palette = Palette::new(WHITE, ARMOR_LAYER_TINT, row.inverse_coloring);
    let last_row = logic::last_row_points(snap.armor, row.colorize_first_row, false);
    let mut ops = Vec::new();
    sprite_draws(
        logic::plan_armor(x, y, snap.armor, palette, false, last_row, row.skip_empty),
        &mut ops,
    );
    if row.allow_count {
        row_count(&*draw, cfg, snap.armor, 20, true, (x - 2, y), &mut ops);
    }
    draw_ops(draw, &ops);
}

fn paint_toughness(draw: &mut dyn DrawContext, cfg: &Config, snap: &Snapshot, left: bool) {
    if snap.toughness <= 0 {
        return;
    }
    let size = draw.size();
    let h = heights(cfg, snap);
    let x = size[0] as i32 / 2 + if left { -91 } else { 91 };
    let y = size[1] as i32
        - if left {
            h.toughness_left
        } else {
            h.toughness_right
        };
    let row = &cfg.toughness;
    // With layers off the bar is a plain single row, as vanilla's armour bar.
    let match_vanilla = !row.allow_layers;
    let palette = Palette::new(
        TOUGHNESS_TINT,
        TOUGHNESS_LAYER_TINT,
        row.inverse_coloring && !match_vanilla,
    );
    let last_row = logic::last_row_points(snap.toughness, row.colorize_first_row, match_vanilla);
    let mut ops = Vec::new();
    sprite_draws(
        logic::plan_armor(
            x,
            y,
            snap.toughness,
            palette,
            !left,
            last_row,
            !match_vanilla && row.skip_empty,
        ),
        &mut ops,
    );
    if row.allow_count && !match_vanilla {
        row_count(&*draw, cfg, snap.toughness, 20, left, (x - 2, y), &mut ops);
    }
    draw_ops(draw, &ops);
}

/// Wrap a vanilla bar: with its layers on, draw the stacked bar instead; off, vanilla's own.
fn stacked(
    ctx: &mut dyn GuiLayerWrapContext,
    on: fn(&Config) -> bool,
    paint: fn(&mut dyn DrawContext, &Config, &Snapshot),
    published_left: fn(&Heights) -> i32,
) {
    let cfg = Config::read(ctx.host());
    if !on(&cfg) {
        ctx.draw_previous();
        return;
    }
    let host = ctx.host();
    if !bars_visible(host) {
        return;
    }
    let Some(snap) = read_snapshot(host) else {
        return;
    };
    ctx.with_draw(&mut |draw| paint(draw, &cfg, &snap));
    publish_status_heights(
        ctx.host(),
        Some(published_left(&heights(&cfg, &snap))),
        None,
    );
}

#[derive(Default)]
struct OverflowingBars;

#[mod_api::sunrise_mod]
impl Hooks for OverflowingBars {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        register_settings(r);
    }

    fn register_gui_layers(
        &self,
        _cx: &RegistrationContext,
        event: &mut RegisterGuiLayersEvent<'_>,
    ) {
        let layers = &mut event.layers;
        layers.wrap_layer(
            LAYER_HEALTH,
            Box::new(|ctx| {
                stacked(
                    ctx,
                    |c| c.health.allow_layers,
                    paint_health,
                    |h| h.left_after_health,
                )
            }),
        );
        layers.wrap_layer(
            LAYER_ARMOR,
            Box::new(|ctx| {
                stacked(
                    ctx,
                    |c| c.armor.allow_layers,
                    paint_armor,
                    |h| h.left_after_armor,
                )
            }),
        );
        for (id, anchor, left) in [
            (LAYER_TOUGHNESS_LEFT, LAYER_ARMOR, true),
            (LAYER_TOUGHNESS_RIGHT, LAYER_VEHICLE, false),
        ] {
            layers.register(
                id.into(),
                GuiLayerPlacement::Above(anchor.into()),
                Box::new(move |host, draw| {
                    let cfg = Config::read(host);
                    if !cfg.toughness_bar || cfg.toughness_left != left || !bars_visible(host) {
                        return;
                    }
                    if let Some(snap) = read_snapshot(host) {
                        paint_toughness(draw, &cfg, &snap, left);
                        if snap.toughness > 0 {
                            let h = heights(&cfg, &snap);
                            if left {
                                publish_status_heights(host, Some(h.toughness_left + 10), None);
                            } else {
                                publish_status_heights(host, None, Some(h.toughness_right + 10));
                            }
                        }
                    }
                }),
            );
        }
    }

    fn customize_chat_overlay(
        &self,
        host: &mut dyn ClientHost,
        event: &mut CustomizeChatOverlayEvent,
    ) {
        let host: &dyn ClientHost = &*host;
        let cfg = Config::read(host);
        if !cfg.move_chat_above_armor {
            return;
        }
        let Some(snap) = read_snapshot(host) else {
            return;
        };
        let offset = logic::chat_offset(
            logic::two_health_rows(snap.max_health, snap.absorption),
            snap.armor > 0,
            cfg.toughness_bar && snap.toughness > 0,
            cfg.toughness_left,
        );
        event.y = event.y.saturating_sub(offset);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(max_health: f32, absorption: f32, armor: i32) -> Snapshot {
        Snapshot {
            health: max_health,
            max_health,
            absorption,
            armor,
            toughness: 0,
            regenerating: false,
            poisoned: false,
            withered: false,
            hardcore: false,
            status_right: Some(49),
            damage_cooldown: None,
            tick: 0,
            millis: 0,
        }
    }

    #[test]
    fn published_columns_stack_the_rows_the_bars_take() {
        let cfg = Config::default();
        let one = heights(&cfg, &snapshot(20.0, 0.0, 10));
        assert_eq!(one.left_after_health, 49);
        assert_eq!(one.left_after_armor, 59);
        assert_eq!(one.toughness_right, 49);
        // Absorption makes a second health row, and no armor adds nothing.
        let two = heights(&cfg, &snapshot(20.0, 4.0, 0));
        assert_eq!(two.left_after_health, 59);
        assert_eq!(two.left_after_armor, 59);
    }

    #[test]
    fn a_row_shift_moves_the_published_height() {
        let mut cfg = Config::default();
        cfg.armor.row_shift = 1;
        let h = heights(&cfg, &snapshot(20.0, 0.0, 10));
        assert_eq!(h.left_after_armor, 69);
        assert_eq!(h.armor, 49 + logic::ROW_HEIGHT);
    }
}
