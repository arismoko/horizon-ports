//! Dynamic FPS's own config screen, opened by the Mods screen's Config button.
//!
//! One custom widget draws a paged panel (`<` / `>` in the header or the Left / Right keys
//! turn the page) and edits the registered settings through `ClientHost::set_setting`:
//! toggles flip, choices cycle (right click goes back), ranges have `-` / `+` ends, a slider
//! (click, drag or scroll) and fine steps on scroll. Every write reaches `setting_changed`,
//! which updates the live config and saves `config/dynamicfps.toml`.

use crate::config::{MOD_ID, RowDef, RowKind, TABS, setting_key};
use mod_api::client::widgets::{Button, ScreenWidget};
use mod_api::{ClientHost, DrawContext, ScreenInitPostEvent, SettingKey, SettingValue, Text};
use std::sync::atomic::{AtomicUsize, Ordering};

const HEADER_H: f32 = 22.0;
const ROW_H: f32 = 22.0;
const FOOTER_H: f32 = 18.0;
/// Rows the tallest page has, so every page shares one panel size.
const PAGE_ROWS: usize = 6;
const CONTROL_W: f32 = 130.0;
const STEP_BUTTON_W: f32 = 16.0;
const PADDING: f32 = 4.0;

const COLOR_TEXT: u32 = 0xFFFF_FFFF;
const COLOR_HINT: u32 = 0xFFAA_AAAA;
const COLOR_BACKDROP: u32 = 0xB000_0000;
const COLOR_HEADER: u32 = 0xFF26_2626;
const COLOR_HEADER_HOVER: u32 = 0xFF3A_3A3A;
const COLOR_ROW_HOVER: u32 = 0x30FF_FFFF;
const COLOR_BORDER: u32 = 0xFF00_0000;
const COLOR_CONTROL: u32 = 0xFF3C_3C3C;
const COLOR_CONTROL_HOVER: u32 = 0xFF50_5050;
const COLOR_SLIDER: u32 = 0xFF3A_7BD5;

/// The page open now; kept across the screen's rebuilds on resize.
static SELECTED_PAGE: AtomicUsize = AtomicUsize::new(0);

fn selected_page() -> usize {
    SELECTED_PAGE.load(Ordering::Relaxed) % TABS.len()
}

fn turn_page(forward: bool) {
    let pages = TABS.len();
    let next = if forward {
        (selected_page() + 1) % pages
    } else {
        (selected_page() + pages - 1) % pages
    };
    SELECTED_PAGE.store(next, Ordering::Relaxed);
}

fn contains(rect: [f32; 4], point: [f32; 2]) -> bool {
    point[0] >= rect[0]
        && point[0] < rect[0] + rect[2]
        && point[1] >= rect[1]
        && point[1] < rect[1] + rect[3]
}

fn translated(host: &dyn ClientHost, key: &str) -> String {
    host.translate(key).unwrap_or_else(|| key.to_owned())
}

fn text_width(host: &dyn ClientHost, text: &str) -> f32 {
    host.text_width(&Text::literal(text)) as f32
}

/// `text` cut with `...` so it is no wider than `max` GUI pixels.
fn fit_text(host: &dyn ClientHost, text: &str, max: f32) -> String {
    if text_width(host, text) <= max {
        return text.to_owned();
    }
    let mut cut = text.to_owned();
    while cut.pop().is_some() {
        let shown = format!("{}...", cut.trim_end());
        if text_width(host, &shown) <= max {
            return shown;
        }
    }
    String::new()
}

fn centered(host: &dyn ClientHost, draw: &mut dyn DrawContext, text: &str, area: [f32; 4]) {
    let x = (area[0] + (area[2] - text_width(host, text)) * 0.5).floor();
    let y = (area[1] + (area[3] - 8.0) * 0.5).floor();
    draw.text(text, [x, y], COLOR_TEXT, true);
}

/// A range value snapped to its step and bounds.
fn snapped(value: f64, min: f64, max: f64, step: f64) -> f64 {
    let steps = ((value - min) / step).round();
    let snapped = (min + steps * step).clamp(min, max);
    (snapped * 1000.0).round() / 1000.0
}

fn range_text(value: f64, step: f64) -> String {
    if step < 1.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.0}")
    }
}

struct PanelRow {
    def: &'static RowDef,
    key: SettingKey,
}

struct ConfigPanel {
    rect: [f32; 4],
    /// One list per [`TABS`] page, rows whose setting did not register left out.
    pages: Vec<Vec<PanelRow>>,
    /// The range row being dragged.
    dragging: Option<usize>,
}

impl ConfigPanel {
    fn new(rect: [f32; 4]) -> Self {
        let pages = TABS
            .iter()
            .map(|tab| {
                tab.rows
                    .iter()
                    .filter_map(|def| setting_key(def.key).map(|key| PanelRow { def, key }))
                    .collect()
            })
            .collect();
        Self {
            rect,
            pages,
            dragging: None,
        }
    }

    fn rows(&self) -> &[PanelRow] {
        self.pages
            .get(selected_page())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn header_rect(&self) -> [f32; 4] {
        [self.rect[0], self.rect[1], self.rect[2], HEADER_H]
    }

    fn row_rect(&self, index: usize) -> [f32; 4] {
        [
            self.rect[0],
            self.rect[1] + HEADER_H + PADDING + index as f32 * ROW_H,
            self.rect[2],
            ROW_H,
        ]
    }

    fn control_rect(&self, index: usize) -> [f32; 4] {
        let row = self.row_rect(index);
        [
            row[0] + row[2] - CONTROL_W,
            row[1] + 1.0,
            CONTROL_W,
            ROW_H - 2.0,
        ]
    }

    fn row_at(&self, point: [f32; 2]) -> Option<usize> {
        (0..self.rows().len()).find(|&index| contains(self.row_rect(index), point))
    }

    fn slider_rect(&self, index: usize) -> [f32; 4] {
        let control = self.control_rect(index);
        [
            control[0] + STEP_BUTTON_W,
            control[1],
            control[2] - 2.0 * STEP_BUTTON_W,
            control[3],
        ]
    }

    fn set_range(host: &mut dyn ClientHost, row: &PanelRow, value: f64) {
        if let RowKind::Range { min, max, step, .. } = row.def.kind
            && value.is_finite()
        {
            let _ = host.set_setting(row.key, SettingValue::Range(snapped(value, min, max, step)));
        }
    }

    fn drag_to(&self, host: &mut dyn ClientHost, index: usize, x: f32) {
        let Some(row) = self.rows().get(index) else {
            return;
        };
        if let RowKind::Range { min, max, .. } = row.def.kind {
            let slider = self.slider_rect(index);
            if slider[2] > 0.0 {
                let fraction = f64::from(((x - slider[0]) / slider[2]).clamp(0.0, 1.0));
                Self::set_range(host, row, min + fraction * (max - min));
            }
        }
    }

    /// Move a row by one step (`forward` or back): flips a toggle, cycles a choice, nudges a range.
    fn nudge(host: &mut dyn ClientHost, row: &PanelRow, forward: bool) {
        match row.def.kind {
            RowKind::Toggle { default } => {
                let current = host.setting_bool(row.key).unwrap_or(default);
                let _ = host.set_setting(row.key, SettingValue::Toggle(!current));
            }
            RowKind::Choice { options, default } => {
                let count = options.len();
                if count == 0 {
                    return;
                }
                let current = host
                    .setting_choice(row.key)
                    .map(|index| index as usize)
                    .unwrap_or(default);
                let next = if forward {
                    (current + 1) % count
                } else {
                    (current + count - 1) % count
                };
                let _ = host.set_setting(row.key, SettingValue::Choice(next));
            }
            RowKind::Range { step, default, .. } => {
                let current = host.setting_f64(row.key).unwrap_or(default);
                let delta = if forward { step } else { -step };
                Self::set_range(host, row, current + delta);
            }
        }
    }

    fn draw_control(
        &self,
        host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        index: usize,
        row: &PanelRow,
        mouse: [f32; 2],
    ) {
        let control = self.control_rect(index);
        let tint = |rect: [f32; 4]| {
            if contains(rect, mouse) {
                COLOR_CONTROL_HOVER
            } else {
                COLOR_CONTROL
            }
        };
        let inner = |rect: [f32; 4]| [rect[0] + 1.0, rect[1] + 1.0, rect[2] - 2.0, rect[3] - 2.0];
        match row.def.kind {
            RowKind::Toggle { default } => {
                let on = host.setting_bool(row.key).unwrap_or(default);
                draw.fill(control, COLOR_BORDER);
                draw.fill(inner(control), tint(control));
                let label = translated(host, if on { "options.on" } else { "options.off" });
                centered(host, draw, &label, control);
            }
            RowKind::Choice { options, default } => {
                let selected = host
                    .setting_choice(row.key)
                    .map(|choice| choice as usize)
                    .unwrap_or(default);
                draw.fill(control, COLOR_BORDER);
                draw.fill(inner(control), tint(control));
                if let Some(option) = options.get(selected) {
                    let label = fit_text(host, &translated(host, option), control[2] - 8.0);
                    centered(host, draw, &label, control);
                }
            }
            RowKind::Range {
                min,
                max,
                step,
                default,
            } => {
                let minus = [control[0], control[1], STEP_BUTTON_W, control[3]];
                let plus = [
                    control[0] + control[2] - STEP_BUTTON_W,
                    control[1],
                    STEP_BUTTON_W,
                    control[3],
                ];
                let slider = self.slider_rect(index);
                for (rect, label) in [(minus, "-"), (plus, "+")] {
                    draw.fill(rect, COLOR_BORDER);
                    draw.fill(inner(rect), tint(rect));
                    centered(host, draw, label, rect);
                }
                let value = host.setting_f64(row.key).unwrap_or(default);
                let span = max - min;
                let fraction = if span > 0.0 {
                    ((value - min) / span).clamp(0.0, 1.0) as f32
                } else {
                    0.0
                };
                draw.fill(slider, COLOR_BORDER);
                draw.fill(inner(slider), tint(slider));
                let filled = ((slider[2] - 2.0) * fraction).floor();
                if filled > 0.0 {
                    draw.fill(
                        [slider[0] + 1.0, slider[1] + 1.0, filled, slider[3] - 2.0],
                        COLOR_SLIDER,
                    );
                }
                centered(host, draw, &range_text(value, step), slider);
            }
        }
    }
}

impl ScreenWidget for ConfigPanel {
    fn bounds(&self) -> [f32; 4] {
        self.rect
    }

    fn render(
        &mut self,
        host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        mouse: [f32; 2],
        _hovered: bool,
        _focused: bool,
        _partial_tick: f32,
    ) {
        let [x, y, w, h] = self.rect;
        draw.fill([x - 6.0, y - 22.0, w + 12.0, h + 28.0], COLOR_BACKDROP);
        centered(
            host,
            draw,
            &translated(host, "gui.dynamicfps.config.title"),
            [x, y - 20.0, w, 14.0],
        );

        let page = selected_page();
        let Some(tab) = TABS.get(page) else {
            return;
        };
        let header = self.header_rect();
        draw.fill(
            header,
            if contains(header, mouse) {
                COLOR_HEADER_HOVER
            } else {
                COLOR_HEADER
            },
        );
        draw.text("<", [x + 8.0, y + 7.0], COLOR_TEXT, true);
        draw.text(">", [x + w - 14.0, y + 7.0], COLOR_TEXT, true);
        let title = format!(
            "{} ({}/{})",
            translated(host, tab.title),
            page + 1,
            TABS.len()
        );
        centered(host, draw, &title, header);

        let label_width = w - CONTROL_W - 2.0 * PADDING - 6.0;
        let mut hovered_row = None;
        for (index, row) in self.rows().iter().enumerate() {
            let rect = self.row_rect(index);
            if contains(rect, mouse) {
                hovered_row = Some(row);
                draw.fill(rect, COLOR_ROW_HOVER);
            }
            let label = fit_text(host, &translated(host, row.def.label), label_width);
            draw.text(
                &label,
                [x + PADDING, (rect[1] + (ROW_H - 8.0) * 0.5).floor()],
                COLOR_TEXT,
                true,
            );
            self.draw_control(host, draw, index, row, mouse);
        }

        if let Some(row) = hovered_row {
            let hint = translated(host, &format!("{}_tooltip", row.def.label));
            let hint = fit_text(host, &hint, w - 2.0 * PADDING);
            let hint_y = y + HEADER_H + PADDING + PAGE_ROWS as f32 * ROW_H + 4.0;
            draw.text(&hint, [x + PADDING, hint_y], COLOR_HINT, false);
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
        if button != 0 && button != 1 {
            return false;
        }
        if contains(self.header_rect(), mouse) {
            turn_page(mouse[0] >= self.rect[0] + self.rect[2] * 0.5);
            return true;
        }
        let Some(index) = self.row_at(mouse) else {
            return false;
        };
        let Some(row) = self.rows().get(index) else {
            return false;
        };
        match row.def.kind {
            RowKind::Range { .. } => {
                let control = self.control_rect(index);
                if !contains(control, mouse) {
                    return true;
                }
                if mouse[0] < control[0] + STEP_BUTTON_W {
                    Self::nudge(host, row, false);
                } else if mouse[0] >= control[0] + control[2] - STEP_BUTTON_W {
                    Self::nudge(host, row, true);
                } else if button == 0 {
                    self.dragging = Some(index);
                    self.drag_to(host, index, mouse[0]);
                }
            }
            _ => {
                if contains(self.control_rect(index), mouse) {
                    Self::nudge(host, row, button == 0);
                }
            }
        }
        true
    }

    fn mouse_released(
        &mut self,
        _host: &mut dyn ClientHost,
        _mouse: [f32; 2],
        _button: i32,
        _modifiers: u32,
    ) -> bool {
        self.dragging.take().is_some()
    }

    fn mouse_dragged(
        &mut self,
        host: &mut dyn ClientHost,
        mouse: [f32; 2],
        button: i32,
        _delta: [f32; 2],
        _modifiers: u32,
    ) -> bool {
        match self.dragging {
            Some(index) if button == 0 => {
                self.drag_to(host, index, mouse[0]);
                true
            }
            _ => false,
        }
    }

    fn mouse_scrolled(
        &mut self,
        host: &mut dyn ClientHost,
        mouse: [f32; 2],
        scroll: [f64; 2],
        _modifiers: u32,
    ) -> bool {
        let forward = scroll[1] > 0.0;
        if scroll[1] == 0.0 || scroll[1].is_nan() {
            return false;
        }
        if contains(self.header_rect(), mouse) {
            turn_page(!forward);
            return true;
        }
        let Some(row) = self.row_at(mouse).and_then(|index| self.rows().get(index)) else {
            return false;
        };
        Self::nudge(host, row, forward);
        true
    }

    fn key_pressed(
        &mut self,
        _host: &mut dyn ClientHost,
        key: &str,
        _repeat: bool,
        _modifiers: u32,
    ) -> bool {
        match key {
            "key.keyboard.left" => {
                turn_page(false);
                true
            }
            "key.keyboard.right" => {
                turn_page(true);
                true
            }
            _ => false,
        }
    }
}

/// Mount the panel and its Done button on Dynamic FPS's own screen.
pub fn mount(event: &mut ScreenInitPostEvent<'_>) {
    let [_, _, screen_w, screen_h] = event.screen.rect;
    let width = (screen_w - 40.0).clamp(240.0, 380.0);
    let height = HEADER_H + PADDING + PAGE_ROWS as f32 * ROW_H + FOOTER_H;
    let x = ((screen_w - width) * 0.5).floor();
    let y = ((screen_h - height - 30.0) * 0.5).max(28.0).floor();

    event
        .widgets
        .add_widget(Box::new(ConfigPanel::new([x, y, width, height])));
    event.widgets.add_button(Button::new(
        [
            (screen_w * 0.5 - 60.0).floor(),
            y + height + 8.0,
            120.0,
            20.0,
        ],
        Text::translate("gui.done", vec![]),
        |_state, host| {
            host.mod_screen_close(MOD_ID);
        },
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapped_values_stay_on_the_step_grid_and_in_bounds() {
        assert_eq!(snapped(0.34, 0.1, 10.0, 0.1), 0.3);
        assert_eq!(snapped(99.0, 0.0, 10.0, 1.0), 10.0);
        assert_eq!(snapped(-5.0, 0.0, 100.0, 5.0), 0.0);
        assert_eq!(snapped(27.0, 0.0, 100.0, 5.0), 25.0);
    }

    #[test]
    fn range_text_shows_a_decimal_only_for_fractional_steps() {
        assert_eq!(range_text(0.5, 0.1), "0.5");
        assert_eq!(range_text(60.0, 1.0), "60");
    }

    #[test]
    fn contains_is_half_open() {
        let rect = [10.0, 10.0, 5.0, 5.0];
        assert!(contains(rect, [10.0, 10.0]));
        assert!(!contains(rect, [15.0, 12.0]));
    }

    #[test]
    fn the_panel_fits_the_tallest_page() {
        assert!(TABS.iter().all(|tab| tab.rows.len() <= PAGE_ROWS));
    }
}
