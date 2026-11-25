//! Bridging Mod's own tabbed config screen, opened by the Mods screen's Config button
//! (`Registrar::register_config_screen`). One tab per original YACL category; every row edits a
//! registered setting through its owner-bound key, so the screen, Options > Mods and
//! `config/bridgingmod.toml` stay in step.

use crate::config::{MOD_ID, RegisteredSetting, SettingKeys, TAB_LABELS};
use mod_api::client::widgets::{Button, ScreenWidget, ScreenWidgets};
use mod_api::{ClientHost, DrawContext, SettingKind, SettingSpec, SettingValue, Text};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

const COLUMN_WIDTH: f32 = 300.0;
const ROW_HEIGHT: f32 = 20.0;
const COLOR_ROW_HEIGHT: f32 = 38.0;
const ROW_GAP: f32 = 4.0;
const TAB_BAR_Y: f32 = 24.0;
const CONTENT_TOP: f32 = 54.0;
const SWATCH_WIDTH: f32 = 20.0;

const BORDER: u32 = 0xFF00_0000;
const BORDER_HIGHLIGHT: u32 = 0xFFFF_FFFF;
const FILL: u32 = 0xFF30_3030;
const FILL_HOVER: u32 = 0xFF3C_3C3C;
const FILL_SELECTED: u32 = 0xFF50_5050;
const SLIDER_FILL: u32 = 0xFF4A_6FB5;
const TEXT: u32 = 0xFFFF_FFFF;

fn translated(host: &dyn ClientHost, key: &str) -> String {
    host.translate(key).unwrap_or_else(|| key.to_owned())
}

fn contains(rect: [f32; 4], point: [f32; 2]) -> bool {
    point[0] >= rect[0]
        && point[0] < rect[0] + rect[2]
        && point[1] >= rect[1]
        && point[1] < rect[1] + rect[3]
}

fn draw_frame(draw: &mut dyn DrawContext, rect: [f32; 4], fill: u32, highlighted: bool) {
    let [x, y, w, h] = rect;
    draw.fill(
        rect,
        if highlighted {
            BORDER_HIGHLIGHT
        } else {
            BORDER
        },
    );
    draw.fill([x + 1.0, y + 1.0, w - 2.0, h - 2.0], fill);
}

fn draw_centered(draw: &mut dyn DrawContext, text: &str, rect: [f32; 4]) {
    let [x, y, w, h] = rect;
    let width = draw.text_width(text);
    draw.text(
        text,
        [
            x + ((w - width) / 2.0).floor(),
            y + ((h - 8.0) / 2.0).floor(),
        ],
        TEXT,
        true,
    );
}

/// The fraction of `rect`'s inner width that `mouse_x` points at, from 0 to 1.
fn fraction_at(rect: [f32; 4], mouse_x: f32) -> f64 {
    f64::from(((mouse_x - rect[0] - 1.0) / (rect[2] - 2.0).max(1.0)).clamp(0.0, 1.0))
}

fn snap(min: f64, max: f64, step: f64, raw: f64) -> f64 {
    let snapped = if step.is_finite() && step > 0.0 {
        min + ((raw - min) / step).round() * step
    } else {
        raw
    };
    snapped.clamp(min, max)
}

fn format_range(value: f64, step: f64) -> String {
    if step >= 1.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}

fn row_height(kind: &SettingKind) -> f32 {
    match kind {
        SettingKind::Color { .. } => COLOR_ROW_HEIGHT,
        _ => ROW_HEIGHT,
    }
}

/// The tab strip: one segment per `TAB_LABELS` entry.
struct TabBar {
    rect: [f32; 4],
    tab: Arc<AtomicUsize>,
}

impl TabBar {
    fn segment(&self, index: usize) -> [f32; 4] {
        let width = self.rect[2] / TAB_LABELS.len() as f32;
        [
            self.rect[0] + width * index as f32,
            self.rect[1],
            width,
            self.rect[3],
        ]
    }
}

impl ScreenWidget for TabBar {
    fn bounds(&self) -> [f32; 4] {
        self.rect
    }

    fn render(
        &mut self,
        host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        mouse: [f32; 2],
        _hovered: bool,
        focused: bool,
        _partial_tick: f32,
    ) {
        let selected = self.tab.load(Ordering::Relaxed);
        for (index, label) in TAB_LABELS.iter().enumerate() {
            let rect = self.segment(index);
            let fill = if index == selected {
                FILL_SELECTED
            } else if contains(rect, mouse) {
                FILL_HOVER
            } else {
                FILL
            };
            draw_frame(draw, rect, fill, index == selected && focused);
            draw_centered(draw, &translated(host, label), rect);
        }
    }

    fn mouse_clicked(
        &mut self,
        _host: &mut dyn ClientHost,
        mouse: [f32; 2],
        button: i32,
        _modifiers: u32,
        _double_click: bool,
    ) -> bool {
        if button != 0 {
            return false;
        }
        match (0..TAB_LABELS.len()).find(|index| contains(self.segment(*index), mouse)) {
            Some(index) => {
                self.tab.store(index, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    fn key_pressed(
        &mut self,
        _host: &mut dyn ClientHost,
        key: &str,
        _repeat: bool,
        _modifiers: u32,
    ) -> bool {
        let current = self.tab.load(Ordering::Relaxed);
        let next = match key {
            "key.keyboard.left" => current.saturating_sub(1),
            "key.keyboard.right" => (current + 1).min(TAB_LABELS.len() - 1),
            _ => return false,
        };
        self.tab.store(next, Ordering::Relaxed);
        true
    }
}

/// One setting row, shown only while its tab is selected: a cycling button for toggles and
/// choices, a slider for ranges, and a three-channel colour picker with a swatch for colours.
struct SettingRow {
    rect: [f32; 4],
    tab_index: usize,
    tab: Arc<AtomicUsize>,
    setting: RegisteredSetting,
    /// The slider (or colour channel) a held mouse button is dragging.
    dragging: Option<usize>,
}

impl SettingRow {
    fn spec(&self) -> &SettingSpec {
        &self.setting.spec
    }

    /// The colour row's `[x, y, w, h]` for channel `index` (0 red, 1 green, 2 blue).
    fn channel_rect(&self, index: usize) -> [f32; 4] {
        let [x, y, w, h] = self.rect;
        let track = w - SWATCH_WIDTH - ROW_GAP;
        let width = (track - 2.0 * ROW_GAP) / 3.0;
        [
            x + (width + ROW_GAP) * index as f32,
            y + h - ROW_HEIGHT,
            width,
            ROW_HEIGHT,
        ]
    }

    fn channel_shift(index: usize) -> u32 {
        16 - 8 * index as u32
    }

    fn cycle(&self, host: &mut dyn ClientHost, backwards: bool) {
        let Ok(current) = host.setting(self.setting.key) else {
            return;
        };
        let next = match (&self.spec().kind, current) {
            (SettingKind::Toggle { .. }, SettingValue::Toggle(on)) => SettingValue::Toggle(!on),
            (SettingKind::Choice { options, .. }, SettingValue::Choice(index))
                if !options.is_empty() =>
            {
                let count = options.len();
                let index = index % count;
                SettingValue::Choice(if backwards {
                    (index + count - 1) % count
                } else {
                    (index + 1) % count
                })
            }
            _ => return,
        };
        let _ = host.set_setting(self.setting.key, next);
    }

    fn drag_to(&self, host: &mut dyn ClientHost, mouse_x: f32) {
        match (&self.spec().kind, self.dragging) {
            (SettingKind::Range { min, max, step, .. }, Some(_)) => {
                let raw = min + fraction_at(self.rect, mouse_x) * (max - min);
                let _ = host.set_setting(
                    self.setting.key,
                    SettingValue::Range(snap(*min, *max, *step, raw)),
                );
            }
            (SettingKind::Color { .. }, Some(channel)) if channel < 3 => {
                let Ok(rgb) = host.setting_color(self.setting.key) else {
                    return;
                };
                let shift = Self::channel_shift(channel);
                let level =
                    (fraction_at(self.channel_rect(channel), mouse_x) * 255.0).round() as u32;
                let next = (rgb & !(0xFF << shift)) | (level << shift);
                let _ = host.set_setting(self.setting.key, SettingValue::Color(next & 0x00FF_FFFF));
            }
            _ => {}
        }
    }

    fn render_color(
        &self,
        host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        rgb: u32,
        hovered: bool,
    ) {
        let [x, y, w, _] = self.rect;
        let label = format!(
            "{}: #{:06x}",
            translated(host, &self.spec().label),
            rgb & 0x00FF_FFFF
        );
        draw.text(&label, [x + 2.0, y + 2.0], TEXT, true);
        for (channel, tint) in [0xFFCC_3333_u32, 0xFF33_CC33, 0xFF33_66CC]
            .into_iter()
            .enumerate()
        {
            let rect = self.channel_rect(channel);
            draw_frame(draw, rect, if hovered { FILL_HOVER } else { FILL }, false);
            let level = f64::from((rgb >> Self::channel_shift(channel)) & 0xFF) / 255.0;
            draw.fill(
                [
                    rect[0] + 1.0,
                    rect[1] + 1.0,
                    (rect[2] - 2.0) * level as f32,
                    rect[3] - 2.0,
                ],
                tint,
            );
        }
        let swatch = [
            x + w - SWATCH_WIDTH,
            self.rect[1] + self.rect[3] - ROW_HEIGHT,
            SWATCH_WIDTH,
            ROW_HEIGHT,
        ];
        draw_frame(draw, swatch, 0xFF00_0000 | (rgb & 0x00FF_FFFF), false);
    }

    fn render_button(
        &self,
        host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        value: SettingValue,
        highlighted: bool,
        hovered: bool,
    ) {
        draw_frame(
            draw,
            self.rect,
            if hovered { FILL_HOVER } else { FILL },
            highlighted,
        );
        let shown = match (&self.spec().kind, value) {
            (SettingKind::Toggle { .. }, SettingValue::Toggle(on)) => {
                if on {
                    host.translate("options.on")
                        .unwrap_or_else(|| "ON".to_owned())
                } else {
                    host.translate("options.off")
                        .unwrap_or_else(|| "OFF".to_owned())
                }
            }
            (SettingKind::Choice { options, .. }, SettingValue::Choice(index)) => options
                .get(index)
                .map(|option| translated(host, option))
                .unwrap_or_default(),
            (SettingKind::Range { min, max, step, .. }, SettingValue::Range(number)) => {
                let [x, y, w, h] = self.rect;
                let fraction = ((number - min) / (max - min)).clamp(0.0, 1.0) as f32;
                draw.fill(
                    [x + 1.0, y + 1.0, (w - 2.0) * fraction, h - 2.0],
                    SLIDER_FILL,
                );
                format_range(number, *step)
            }
            _ => String::new(),
        };
        let label = format!("{}: {shown}", translated(host, &self.spec().label));
        draw_centered(draw, &label, self.rect);
    }
}

impl ScreenWidget for SettingRow {
    fn bounds(&self) -> [f32; 4] {
        self.rect
    }

    fn visible(&self) -> bool {
        self.tab.load(Ordering::Relaxed) == self.tab_index
    }

    fn render(
        &mut self,
        host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        mouse: [f32; 2],
        hovered: bool,
        focused: bool,
        _partial_tick: f32,
    ) {
        if let Ok(value) = host.setting(self.setting.key) {
            match (&self.spec().kind, value) {
                (SettingKind::Color { .. }, SettingValue::Color(rgb)) => {
                    self.render_color(host, draw, rgb, hovered);
                }
                _ => self.render_button(host, draw, value, hovered || focused, hovered),
            }
        }
        if hovered && let Some(tooltip) = &self.spec().tooltip {
            let _ = draw.gui_tooltip(&[Text::translate(tooltip.clone(), Vec::new())], mouse);
        }
    }

    fn mouse_clicked(
        &mut self,
        host: &mut dyn ClientHost,
        mouse: [f32; 2],
        button: i32,
        _modifiers: u32,
        _double_click: bool,
    ) -> bool {
        match self.spec().kind {
            SettingKind::Range { .. } if button == 0 => {
                self.dragging = Some(0);
                self.drag_to(host, mouse[0]);
                true
            }
            SettingKind::Color { .. } if button == 0 => {
                let Some(channel) = (0..3).find(|index| contains(self.channel_rect(*index), mouse))
                else {
                    return false;
                };
                self.dragging = Some(channel);
                self.drag_to(host, mouse[0]);
                true
            }
            SettingKind::Toggle { .. } | SettingKind::Choice { .. }
                if button == 0 || button == 1 =>
            {
                self.cycle(host, button == 1);
                true
            }
            _ => false,
        }
    }

    fn mouse_dragged(
        &mut self,
        host: &mut dyn ClientHost,
        mouse: [f32; 2],
        button: i32,
        _delta: [f32; 2],
        _modifiers: u32,
    ) -> bool {
        if button != 0 || self.dragging.is_none() {
            return false;
        }
        self.drag_to(host, mouse[0]);
        true
    }

    fn mouse_released(
        &mut self,
        _host: &mut dyn ClientHost,
        _mouse: [f32; 2],
        button: i32,
        _modifiers: u32,
    ) -> bool {
        button == 0 && self.dragging.take().is_some()
    }

    fn key_pressed(
        &mut self,
        host: &mut dyn ClientHost,
        key: &str,
        _repeat: bool,
        _modifiers: u32,
    ) -> bool {
        match (&self.spec().kind, key) {
            (
                SettingKind::Toggle { .. } | SettingKind::Choice { .. },
                "key.keyboard.enter" | "key.keyboard.space",
            ) => {
                self.cycle(host, false);
                true
            }
            (
                SettingKind::Range { min, max, step, .. },
                "key.keyboard.left" | "key.keyboard.right",
            ) => {
                let Ok(number) = host.setting_f64(self.setting.key) else {
                    return false;
                };
                let direction = if key == "key.keyboard.left" {
                    -1.0
                } else {
                    1.0
                };
                let _ = host.set_setting(
                    self.setting.key,
                    SettingValue::Range(snap(*min, *max, *step, number + direction * step)),
                );
                true
            }
            _ => false,
        }
    }
}

/// Mount the tab strip, every setting row and the Done button on the open config screen.
pub fn mount(
    widgets: &mut dyn ScreenWidgets,
    screen_rect: [f32; 4],
    tab: &Arc<AtomicUsize>,
    settings: &SettingKeys,
) {
    let [_, _, width, height] = screen_rect;
    let x = (width / 2.0 - COLUMN_WIDTH / 2.0).floor();

    widgets.add_widget(Box::new(TabBar {
        rect: [x, TAB_BAR_Y, COLUMN_WIDTH, ROW_HEIGHT],
        tab: Arc::clone(tab),
    }));

    let mut next_y = [CONTENT_TOP; TAB_LABELS.len()];
    for setting in settings.iter() {
        let Some(y) = next_y.get_mut(setting.tab) else {
            continue;
        };
        let row = row_height(&setting.spec.kind);
        widgets.add_widget(Box::new(SettingRow {
            rect: [x, *y, COLUMN_WIDTH, row],
            tab_index: setting.tab,
            tab: Arc::clone(tab),
            setting: setting.clone(),
            dragging: None,
        }));
        *y += row + ROW_GAP;
    }

    widgets.add_button(Button::new(
        [
            (width / 2.0 - 100.0).floor(),
            height - 28.0,
            200.0,
            ROW_HEIGHT,
        ],
        Text::translate("gui.done", Vec::new()),
        |_state, host| {
            host.mod_screen_close(MOD_ID);
        },
    ));
}

/// Draw the screen title above the tab strip.
pub fn render_title(host: &dyn ClientHost, draw: &mut dyn DrawContext) {
    let [width, _] = draw.size();
    let title = translated(host, "config.bridgingmod.title");
    let text_width = draw.text_width(&title);
    draw.text(
        &title,
        [((width - text_width) / 2.0).floor(), 10.0],
        TEXT,
        true,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_rounds_to_the_step_and_stays_in_range() {
        assert_eq!(snap(0.0, 20.0, 1.0, 4.4), 4.0);
        assert_eq!(snap(0.0, 20.0, 1.0, 99.0), 20.0);
        assert_eq!(snap(0.0, 1.0, 0.05, -3.0), 0.0);
        assert_eq!(snap(0.0, 1.0, 0.0, 0.3), 0.3);
    }

    #[test]
    fn fraction_follows_the_pointer_inside_the_track() {
        let rect = [10.0, 0.0, 102.0, 20.0];
        assert_eq!(fraction_at(rect, 0.0), 0.0);
        assert_eq!(fraction_at(rect, 61.0), 0.5);
        assert_eq!(fraction_at(rect, 500.0), 1.0);
    }

    #[test]
    fn ranges_show_whole_numbers_only_for_whole_steps() {
        assert_eq!(format_range(4.0, 1.0), "4");
        assert_eq!(format_range(0.4, 0.05), "0.40");
    }

    #[test]
    fn colour_channels_pack_red_green_blue() {
        assert_eq!(SettingRow::channel_shift(0), 16);
        assert_eq!(SettingRow::channel_shift(1), 8);
        assert_eq!(SettingRow::channel_shift(2), 0);
    }
}
