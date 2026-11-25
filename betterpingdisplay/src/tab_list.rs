//! Player list (Tab overlay) layout with Better Ping Display's widened slots and numeric ping readout.
//!
//! Mirrors vanilla's `PlayerTabOverlay` geometry, header/footer bands, multi-column wrapping,
//! player faces, team-formatted names, spectator styling, and `list` scoreboard objectives
//! (integer scores and animated blinking hearts), while applying Better Ping Display's
//! `+45` px slot width expansion and numeric ping rendering.

use crate::config::BetterPingDisplayConfig;
use mod_api::client::hud_layout::Blend;
use mod_api::text::TextStyle;
use mod_api::{
    ClientObjective, ClientScore, ClientTabList, ClientTabListEntry, GameMode, GuiDraw,
    NumberFormat, ObjectiveRenderType, Text,
};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

/// `PlayerTabOverlay.MAX_ROWS_PER_COL`.
pub const MAX_ROWS_PER_COL: i32 = 20;
/// Vanilla reserve for the ping icon in `PlayerTabOverlay.render`.
pub const VANILLA_PING_RESERVE: i32 = 13;
/// Extra slot width added by Better Ping Display (`PlayerListHudMixin.PLAYER_SLOT_EXTRA_WIDTH`).
pub const PLAYER_SLOT_EXTRA_WIDTH: i32 = 45;
/// Right-edge offset for ping text when bars are shown (`CustomPlayerListHud.PING_TEXT_RENDER_OFFSET`).
pub const PING_TEXT_RENDER_OFFSET: i32 = -13;
/// Width reclaimed when vanilla ping bars are hidden (`CustomPlayerListHud.PING_BARS_WIDTH`).
pub const PING_BARS_WIDTH: i32 = 11;

const WHITE: u32 = 0xFFFF_FFFF;
const SPECTATOR_COLOR: u32 = 0x90FF_FFFF;
const BAND_BACKGROUND: u32 = 0x8000_0000;
/// Vanilla's `Options.getBackgroundColor(553648127)` with chat-only background enabled (Sunrise's default).
const ROW_BACKGROUND: u32 = 0x20FF_FFFF;

/// Minimal text measurement & wrapping interface so layout is unit-testable without a live client.
pub trait TextMetrics {
    fn text_width(&self, text: &Text) -> i32;
    fn str_width(&self, text: &str) -> i32;
    fn text_split(&self, text: &Text, max_width: i32) -> Vec<Text>;
}

/// Vanilla's `Hud.extractTabList` visibility rule.
pub fn is_visible(
    hud_shown: bool,
    playing: bool,
    key_down: bool,
    local_server: bool,
    listed: usize,
    has_objective: bool,
) -> bool {
    hud_shown && playing && key_down && !(local_server && listed <= 1 && !has_objective)
}

/// Vanilla's `PlayerTabOverlay.extractPingIcon` sprite selection.
pub const fn ping_sprite(latency: i32) -> &'static str {
    match latency {
        i32::MIN..=-1 => "icon/ping_unknown",
        0..150 => "icon/ping_5",
        150..300 => "icon/ping_4",
        300..600 => "icon/ping_3",
        600..1000 => "icon/ping_2",
        _ => "icon/ping_1",
    }
}

/// Vanilla's `PlayerTabOverlay.setVisible(true)` narration (`multiplayer.player.list.narration`).
pub fn build_narration(entries: &[ClientTabListEntry]) -> Text {
    let mut names = Text::default();
    for (idx, entry) in entries.iter().enumerate() {
        if idx > 0 {
            names = names.append(Text::literal(", "));
        }
        names = names.append(entry.display.clone());
    }
    Text::translate("multiplayer.player.list.narration", vec![names])
}

/// Per-player animated heart state (`PlayerTabOverlay.HealthState`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HealthState {
    last_value: i32,
    displayed_value: i32,
    last_update_tick: i64,
    blink_until_tick: i64,
}

impl HealthState {
    fn new(value: i32) -> Self {
        Self {
            last_value: value,
            displayed_value: value,
            last_update_tick: 0,
            blink_until_tick: 0,
        }
    }

    fn update(&mut self, value: i32, tick: i64) {
        if value != self.last_value {
            let duration = if value < self.last_value { 20 } else { 10 };
            self.blink_until_tick = tick.saturating_add(duration);
            self.last_value = value;
            self.last_update_tick = tick;
        }
        if tick.saturating_sub(self.last_update_tick) > 20 {
            self.displayed_value = value;
        }
    }

    fn is_blinking(&self, tick: i64) -> bool {
        self.blink_until_tick > tick && (self.blink_until_tick - tick) % 6 >= 3
    }
}

/// Stateful helper that tracks Tab list visibility transitions and animated heart states.
#[derive(Debug, Default)]
pub struct TabListState {
    visible: bool,
    health: HashMap<u128, HealthState>,
}

impl TabListState {
    /// Update visibility, clearing cached heart animation states on transitions.
    /// Returns `true` when the list just became visible (matching `PlayerTabOverlay.setVisible`).
    pub fn set_visible(&mut self, visible: bool) -> bool {
        if self.visible != visible {
            self.health.clear();
            self.visible = visible;
            return visible;
        }
        false
    }

    /// Build the complete sequence of `GuiDraw` calls for the player list with numeric ping.
    pub fn build_draws(
        &mut self,
        tab_list: &ClientTabList,
        objective: Option<&ClientObjective>,
        screen_width: i32,
        gui_ticks: i64,
        config: &BetterPingDisplayConfig,
        metrics: &dyn TextMetrics,
    ) -> Vec<GuiDraw> {
        let entries = &tab_list.entries;
        if entries.is_empty() {
            return Vec::new();
        }

        let spacer = metrics.str_width(" ");
        let mut max_name = 0;
        let mut max_score = 0;

        let score_by_holder: HashMap<&str, &ClientScore> = objective
            .map(|obj| {
                obj.scores
                    .iter()
                    .map(|score| (score.holder.as_str(), score))
                    .collect()
            })
            .unwrap_or_default();

        let mut row_scores = Vec::with_capacity(entries.len());
        for entry in entries {
            max_name = max_name.max(metrics.text_width(&entry.display));
            let mut score_val = 0;
            let mut formatted = None;
            let mut score_width = 0;

            if let Some(obj) = objective {
                let score_info = score_by_holder.get(entry.name.as_str()).copied();
                if let Some(s) = score_info {
                    score_val = s.value;
                }
                if obj.render_type != ObjectiveRenderType::Hearts {
                    if let Some(s) = score_info {
                        let text = format_score(s, obj);
                        score_width = metrics.text_width(&text);
                        formatted = Some(text);
                    }
                    let w = if score_width > 0 {
                        spacer + score_width
                    } else {
                        0
                    };
                    max_score = max_score.max(w);
                }
            }

            row_scores.push((score_val, formatted, score_width));
        }

        if !self.health.is_empty() {
            let displayed: HashSet<u128> = entries.iter().map(|e| e.uuid).collect();
            self.health.retain(|id, _| displayed.contains(id));
        }

        let slots = entries.len() as i32;
        let mut rows = slots;
        let mut cols = 1;
        while rows > MAX_ROWS_PER_COL {
            cols += 1;
            rows = (slots + cols - 1) / cols;
        }

        let width_for_score = match objective {
            Some(obj) if obj.render_type == ObjectiveRenderType::Hearts => 90,
            Some(_) => max_score,
            None => 0,
        };

        let head = 9;
        let ping_reserve = VANILLA_PING_RESERVE + PLAYER_SLOT_EXTRA_WIDTH;
        let available = (screen_width - 50).max(cols);
        let slot_width =
            (cols * (head + max_name + width_for_score + ping_reserve)).min(available) / cols;
        let xxo = screen_width / 2 - (slot_width * cols + (cols - 1) * 5) / 2;
        let mut yyo = 10;
        let mut max_line = slot_width * cols + (cols - 1) * 5;

        let wrap_width = (screen_width - 50).max(1);
        let header_lines = tab_list
            .header
            .as_ref()
            .map(|h| metrics.text_split(h, wrap_width));
        let footer_lines = tab_list
            .footer
            .as_ref()
            .map(|f| metrics.text_split(f, wrap_width));

        for line in header_lines.iter().chain(footer_lines.iter()).flatten() {
            max_line = max_line.max(metrics.text_width(line));
        }

        let mut out = Vec::new();
        let band = |y0: i32, y1: i32| GuiDraw::Fill {
            x0: screen_width / 2 - max_line / 2 - 1,
            y0,
            x1: screen_width / 2 + max_line / 2 + 1,
            y1,
            color: BAND_BACKGROUND,
        };

        if let Some(lines) = &header_lines {
            out.push(band(yyo - 1, yyo + lines.len() as i32 * 9));
            for line in lines {
                let w = metrics.text_width(line);
                out.push(GuiDraw::Text {
                    text: line.clone(),
                    x: screen_width / 2 - w / 2,
                    y: yyo,
                    color: WHITE,
                    shadow: true,
                });
                yyo += 9;
            }
            yyo += 1;
        }

        out.push(band(yyo - 1, yyo + rows * 9));

        for (n, (entry, (score_val, formatted, score_width))) in
            entries.iter().zip(&row_scores).enumerate()
        {
            let n = n as i32;
            let (col, row) = (n / rows, n % rows);
            let slot_left = xxo + col * slot_width + col * 5;
            let yo = yyo + row * 9;

            out.push(GuiDraw::Fill {
                x0: slot_left,
                y0: yo,
                x1: slot_left + slot_width,
                y1: yo + 8,
                color: ROW_BACKGROUND,
            });

            out.push(GuiDraw::PlayerFace {
                uuid: entry.uuid,
                x: slot_left,
                y: yo,
                size: 8,
                hat: true,
                flip: is_upside_down(&entry.name),
                color: WHITE,
            });

            let xo = slot_left + head;
            let spectator = entry.game_mode == GameMode::Spectator;
            out.push(GuiDraw::Text {
                text: entry.display.clone(),
                x: xo,
                y: yo,
                color: if spectator { SPECTATOR_COLOR } else { WHITE },
                shadow: true,
            });

            if let Some(obj) = objective.filter(|_| !spectator) {
                let left = xo + max_name + 1;
                let right = left + width_for_score;
                if right - left > 5 {
                    if obj.render_type == ObjectiveRenderType::Hearts {
                        self.push_hearts(
                            yo, left, right, entry.uuid, *score_val, gui_ticks, metrics, &mut out,
                        );
                    } else if let Some(text) = formatted {
                        out.push(GuiDraw::Text {
                            text: text.clone(),
                            x: right - *score_width,
                            y: yo,
                            color: WHITE,
                            shadow: true,
                        });
                    }
                }
            }

            // Better Ping Display numeric ping readout + optional vanilla ping bars.
            let ping_text = config.format_latency(entry.latency);
            let ping_width = metrics.str_width(&ping_text);
            let ping_color = config.text_color_for(entry.latency);
            let mut ping_x = slot_left + slot_width - ping_width + PING_TEXT_RENDER_OFFSET;
            if !config.render_ping_bars {
                ping_x += PING_BARS_WIDTH;
            }
            out.push(GuiDraw::Text {
                text: Text::literal(ping_text),
                x: ping_x,
                y: yo,
                color: ping_color,
                shadow: true,
            });

            if config.render_ping_bars {
                out.push(sprite(
                    ping_sprite(entry.latency),
                    slot_left + slot_width - 11,
                    yo,
                    10,
                    8,
                ));
            }
        }

        if let Some(lines) = &footer_lines {
            yyo += rows * 9 + 1;
            out.push(band(yyo - 1, yyo + lines.len() as i32 * 9));
            for line in lines {
                let w = metrics.text_width(line);
                out.push(GuiDraw::Text {
                    text: line.clone(),
                    x: screen_width / 2 - w / 2,
                    y: yyo,
                    color: WHITE,
                    shadow: true,
                });
                yyo += 9;
            }
        }

        out
    }

    #[allow(clippy::too_many_arguments)]
    fn push_hearts(
        &mut self,
        yo: i32,
        left: i32,
        right: i32,
        id: u128,
        score: i32,
        tick: i64,
        metrics: &dyn TextMetrics,
        out: &mut Vec<GuiDraw>,
    ) {
        let health = self
            .health
            .entry(id)
            .or_insert_with(|| HealthState::new(score));
        health.update(score, tick);
        let displayed = health.displayed_value;
        let blink = health.is_blinking(tick);

        let max_hp = score.max(displayed);
        if max_hp <= 0 {
            return;
        }
        let full_hearts = max_hp.div_ceil(2);
        let to_render = max_hp.max(20) / 2;

        let per_heart = ((right - left - 4) as f32 / to_render as f32)
            .min(9.0)
            .floor() as i32;
        if per_heart <= 3 {
            let pct = (score as f32 / 20.0).clamp(0.0, 1.0);
            let color = (((1.0 - pct) * 255.0) as u32) << 16 | ((pct * 255.0) as u32) << 8;
            let hp_num = format_half_int(score);
            let hp = Text::translate("multiplayer.player.list.hp", vec![Text::literal(&hp_num)]);
            let text = if right - metrics.text_width(&hp) >= left {
                hp
            } else {
                Text::literal(hp_num)
            };
            let w = metrics.text_width(&text);
            out.push(GuiDraw::Text {
                text,
                x: (right + left - w) / 2,
                y: yo,
                color: color | 0xFF00_0000,
                shadow: true,
            });
            return;
        }

        let container = if blink {
            "hud/heart/container_blinking"
        } else {
            "hud/heart/container"
        };
        let heart = |name: &'static str, n: i32| sprite(name, left + n * per_heart, yo, 9, 9);
        for n in full_hearts..to_render {
            out.push(heart(container, n));
        }
        for n in 0..full_hearts {
            out.push(heart(container, n));
            if blink {
                if n * 2 + 1 < displayed {
                    out.push(heart("hud/heart/full_blinking", n));
                }
                if n * 2 + 1 == displayed {
                    out.push(heart("hud/heart/half_blinking", n));
                }
            }
            if n * 2 + 1 < score {
                out.push(heart(
                    if n >= 10 {
                        "hud/heart/absorbing_full_blinking"
                    } else {
                        "hud/heart/full"
                    },
                    n,
                ));
            }
            if n * 2 + 1 == score {
                out.push(heart(
                    if n >= 10 {
                        "hud/heart/absorbing_half_blinking"
                    } else {
                        "hud/heart/half"
                    },
                    n,
                ));
            }
        }
    }
}

fn sprite(name: &'static str, x: i32, y: i32, w: i32, h: i32) -> GuiDraw {
    GuiDraw::Sprite {
        sprite: Cow::Borrowed(name),
        x,
        y,
        w,
        h,
        region: None,
        color: WHITE,
        blend: Blend::Textured,
    }
}

fn is_upside_down(name: &str) -> bool {
    name == "Dinnerbone" || name == "Grumm"
}

fn format_score(score: &ClientScore, objective: &ClientObjective) -> Text {
    match score
        .number_format
        .as_ref()
        .or(objective.number_format.as_ref())
    {
        Some(NumberFormat::Blank) => Text::default(),
        Some(NumberFormat::Fixed(text)) => text.clone(),
        Some(NumberFormat::Styled(style)) => {
            Text::literal(score.value.to_string()).with_style(style.clone())
        }
        _ => {
            let mut yellow = TextStyle::default();
            yellow.color = Some("yellow".to_owned());
            Text::literal(score.value.to_string()).with_style(yellow)
        }
    }
}

fn format_half_int(score: i32) -> String {
    if score % 2 == 0 {
        format!("{}.0", score / 2)
    } else {
        format!("{:.1}", score as f32 / 2.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{ALPHA_MASK, COLOR_END, COLOR_GREY, COLOR_START};

    struct FixedMetrics;

    impl TextMetrics for FixedMetrics {
        fn text_width(&self, text: &Text) -> i32 {
            match &text.content {
                mod_api::text::TextContent::Literal(s) => self.str_width(s),
                _ => 24,
            }
        }

        fn str_width(&self, text: &str) -> i32 {
            text.chars().count() as i32 * 6
        }

        fn text_split(&self, text: &Text, _max_width: i32) -> Vec<Text> {
            vec![text.clone()]
        }
    }

    fn sample_entry(uuid: u128, name: &str, latency: i32, mode: GameMode) -> ClientTabListEntry {
        ClientTabListEntry::new(
            name.to_owned(),
            uuid,
            mode,
            latency,
            Text::literal(name.to_owned()),
        )
    }

    #[test]
    fn visibility_rules_match_vanilla() {
        assert!(!is_visible(false, true, true, false, 2, false));
        assert!(!is_visible(true, false, true, false, 2, false));
        assert!(!is_visible(true, true, false, false, 2, false));
        assert!(!is_visible(true, true, true, true, 1, false));
        assert!(is_visible(true, true, true, true, 1, true));
        assert!(is_visible(true, true, true, true, 2, false));
        assert!(is_visible(true, true, true, false, 1, false));
    }

    #[test]
    fn ping_text_position_and_bars_toggle() {
        let list = ClientTabList::new(
            vec![sample_entry(1, "Alice", 42, GameMode::Survival)],
            None,
            None,
        );
        let mut state = TabListState::default();

        // Bars off (default): ping text shifted right by PING_BARS_WIDTH (11), no ping sprite.
        let cfg_no_bars = BetterPingDisplayConfig::default();
        let draws_no_bars = state.build_draws(&list, None, 400, 0, &cfg_no_bars, &FixedMetrics);
        let ping_no_bars = draws_no_bars
            .iter()
            .find_map(|d| match d {
                GuiDraw::Text { text, x, color, .. } if *text == Text::literal("42ms") => {
                    Some((*x, *color))
                }
                _ => None,
            })
            .expect("ping text should be drawn");
        let has_ping_sprite_off = draws_no_bars.iter().any(
            |d| matches!(d, GuiDraw::Sprite { sprite, .. } if sprite.starts_with("icon/ping_")),
        );
        assert!(!has_ping_sprite_off);

        // Bars on: ping text shifted left by 11 px and ping_5 sprite drawn.
        let cfg_with_bars = BetterPingDisplayConfig {
            render_ping_bars: true,
            ..BetterPingDisplayConfig::default()
        };
        let draws_with_bars = state.build_draws(&list, None, 400, 0, &cfg_with_bars, &FixedMetrics);
        let ping_with_bars = draws_with_bars
            .iter()
            .find_map(|d| match d {
                GuiDraw::Text { text, x, color, .. } if *text == Text::literal("42ms") => {
                    Some((*x, *color))
                }
                _ => None,
            })
            .expect("ping text should be drawn with bars");
        assert_eq!(ping_no_bars.0 - ping_with_bars.0, PING_BARS_WIDTH);
        assert_eq!(ping_no_bars.1, ping_with_bars.1);
        let has_ping_sprite_on = draws_with_bars
            .iter()
            .any(|d| matches!(d, GuiDraw::Sprite { sprite, .. } if sprite == "icon/ping_5"));
        assert!(has_ping_sprite_on);
    }

    #[test]
    fn ping_colors_and_spectator_alpha() {
        let list = ClientTabList::new(
            vec![
                sample_entry(1, "Low", 0, GameMode::Survival),
                sample_entry(2, "High", 500, GameMode::Spectator),
                sample_entry(3, "Unknown", -1, GameMode::Survival),
            ],
            Some(Text::literal("Server Header")),
            Some(Text::literal("Server Footer")),
        );
        let mut state = TabListState::default();
        let cfg = BetterPingDisplayConfig::default();
        let draws = state.build_draws(&list, None, 400, 0, &cfg, &FixedMetrics);

        let color_of = |label: &str| {
            draws
                .iter()
                .find_map(|d| match d {
                    GuiDraw::Text { text, color, .. } if *text == Text::literal(label) => {
                        Some(*color)
                    }
                    _ => None,
                })
                .unwrap()
        };

        assert_eq!(color_of("0ms"), COLOR_START | ALPHA_MASK);
        assert_eq!(color_of("500ms"), COLOR_END | ALPHA_MASK);
        assert_eq!(color_of("-1ms"), COLOR_GREY | ALPHA_MASK);
        assert_eq!(color_of("High"), SPECTATOR_COLOR);
        assert_eq!(color_of("Server Header"), WHITE);
        assert_eq!(color_of("Server Footer"), WHITE);
    }

    #[test]
    fn tiny_screen_width_never_panics() {
        let list = ClientTabList::new(
            vec![sample_entry(1, "Alice", 25, GameMode::Survival)],
            None,
            None,
        );
        let mut state = TabListState::default();
        let cfg = BetterPingDisplayConfig::default();
        let draws = state.build_draws(&list, None, 0, 0, &cfg, &FixedMetrics);
        assert!(!draws.is_empty());
    }

    #[test]
    fn hearts_extreme_scores_match_vanilla() {
        let count_heart_sprites = |draws: &[GuiDraw]| {
            draws
                .iter()
                .filter(|d| {
                    matches!(
                        d,
                        GuiDraw::Sprite { sprite, .. } if sprite.starts_with("hud/heart/")
                    )
                })
                .count()
        };

        let mut state = TabListState::default();
        let mut draws = Vec::new();

        // i32::MIN: non-positive health draws no hearts and no HP text (no overflow or OOM).
        state.push_hearts(10, 0, 90, 1, i32::MIN, 0, &FixedMetrics, &mut draws);
        assert_eq!(count_heart_sprites(&draws), 0);
        assert!(draws.is_empty());

        // i32::MAX: per_heart = floor(86.0 / (i32::MAX / 2)) = 0 <= 3, so vanilla switches to
        // compact HP text and draws 0 heart sprites.
        draws.clear();
        state.push_hearts(10, 0, 90, 2, i32::MAX, 0, &FixedMetrics, &mut draws);
        assert_eq!(count_heart_sprites(&draws), 0);
        assert_eq!(draws.len(), 1);
        assert!(matches!(
            &draws[0],
            GuiDraw::Text {
                color: 0xFF00_FF00,
                ..
            }
        ));

        // Dropping from positive health to i32::MIN stays safe while displayed health blinks.
        draws.clear();
        state.push_hearts(10, 0, 90, 3, 20, 0, &FixedMetrics, &mut draws);
        assert_eq!(count_heart_sprites(&draws), 20);
        draws.clear();
        state.push_hearts(10, 0, 90, 3, i32::MIN, 1, &FixedMetrics, &mut draws);
        assert_eq!(count_heart_sprites(&draws), 10);
        draws.clear();
        state.push_hearts(10, 0, 90, 3, i32::MIN, 4, &FixedMetrics, &mut draws);
        assert_eq!(count_heart_sprites(&draws), 20);

        // Dropping from high health (>42) to i32::MIN enters compact HP text safely.
        draws.clear();
        state.push_hearts(10, 0, 90, 4, 100, 0, &FixedMetrics, &mut draws);
        draws.clear();
        state.push_hearts(10, 0, 90, 4, i32::MIN, 1, &FixedMetrics, &mut draws);
        assert_eq!(count_heart_sprites(&draws), 0);
        assert_eq!(draws.len(), 1);
        assert!(matches!(
            &draws[0],
            GuiDraw::Text {
                color: 0xFFFF_0000,
                ..
            }
        ));
    }
}
