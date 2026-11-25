//! The sectioned settings screen the Mods screen's Config button opens (the original's Style,
//! Position and Warning tabs). One mounted widget draws the tabs, the rows and the Done button
//! and writes every change through the same settings Options > Mods uses.

use crate::config::{self, Kind, MOD_ID, Row, Tab};
use mod_api::client::widgets::ScreenWidget;
use mod_api::{ClientHost, DrawContext, SettingValue, Text};

const PANEL_W: f32 = 320.0;
const TAB_H: f32 = 20.0;
const TAB_GAP: f32 = 6.0;
const ROW_STRIDE: f32 = 20.0;
const ROW_H: f32 = 18.0;
const CONTROL_W: f32 = 140.0;
const MARGIN: f32 = 10.0;
const FOOTER_H: f32 = 28.0;
const DONE_W: f32 = 100.0;
const DONE_H: f32 = 20.0;
/// Widest a tooltip line grows, in GUI pixels.
const TOOLTIP_W: f32 = 200.0;
const LINE_HEIGHT: f32 = 9.0;

const WHITE: u32 = 0xFFFF_FFFF;
const PANEL_FILL: u32 = 0xE010_1010;
const PANEL_EDGE: u32 = 0xFF00_0000;
const BUTTON_FILL: u32 = 0xFF3A_3A3A;
const BUTTON_HOVER: u32 = 0xFF55_5555;
const TAB_SELECTED: u32 = 0xFF6A_6A6A;
const SLIDER_FILL: u32 = 0xFF3F_7F3F;

/// Where the panel's parts sit for a window `size`, in GUI pixels. `rows` is the most rows any
/// tab has, so the panel does not change height between tabs.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Layout {
    origin: [f32; 2],
    height: f32,
}

impl Layout {
    fn new(size: [f32; 2], rows: usize) -> Self {
        let height = TAB_H + TAB_GAP + rows as f32 * ROW_STRIDE + FOOTER_H;
        Self {
            origin: [
                ((size[0] - PANEL_W) / 2.0).floor().max(0.0),
                ((size[1] - height) / 2.0).floor().max(0.0),
            ],
            height,
        }
    }

    fn panel(&self) -> [f32; 4] {
        [self.origin[0], self.origin[1], PANEL_W, self.height]
    }

    fn tab(&self, index: usize, count: usize) -> [f32; 4] {
        let width = PANEL_W / count.max(1) as f32;
        [
            self.origin[0] + index as f32 * width,
            self.origin[1],
            width,
            TAB_H,
        ]
    }

    fn row(&self, index: usize) -> [f32; 4] {
        [
            self.origin[0] + MARGIN,
            self.origin[1] + TAB_H + TAB_GAP + index as f32 * ROW_STRIDE,
            PANEL_W - 2.0 * MARGIN,
            ROW_H,
        ]
    }

    fn control(&self, index: usize) -> [f32; 4] {
        let row = self.row(index);
        [row[0] + row[2] - CONTROL_W, row[1], CONTROL_W, ROW_H]
    }

    fn done(&self) -> [f32; 4] {
        [
            self.origin[0] + (PANEL_W - DONE_W) / 2.0,
            self.origin[1] + self.height - DONE_H - 4.0,
            DONE_W,
            DONE_H,
        ]
    }
}

fn contains(rect: [f32; 4], point: [f32; 2]) -> bool {
    point[0] >= rect[0]
        && point[0] < rect[0] + rect[2]
        && point[1] >= rect[1]
        && point[1] < rect[1] + rect[3]
}

/// The value a slider over `control` stands for with the pointer at `x`, snapped to `step`.
fn slider_value(control: [f32; 4], x: f32, min: f64, max: f64, step: f64) -> f64 {
    let t = if control[2] > 0.0 && x.is_finite() {
        f64::from(((x - control[0]) / control[2]).clamp(0.0, 1.0))
    } else {
        0.0
    };
    let raw = min + t * (max - min);
    let snapped = if step > 0.0 {
        min + ((raw - min) / step).round() * step
    } else {
        raw
    };
    ((snapped.clamp(min, max)) * 1e6).round() / 1e6
}

/// Break `text` into lines at most `max` wide by `width`, at spaces.
fn wrap(text: &str, max: f32, width: &dyn Fn(&str) -> f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if width(&format!("{line} {word}")) <= max => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines
}

/// `text` centred in `rect`, vertically too.
fn centered(draw: &mut dyn DrawContext, text: &str, rect: [f32; 4]) {
    let x = rect[0] + ((rect[2] - draw.text_width(text)) / 2.0).floor();
    draw.text(
        text,
        [x, rect[1] + (rect[3] - LINE_HEIGHT) / 2.0],
        WHITE,
        true,
    );
}

fn tr(host: &dyn ClientHost, key: &str) -> String {
    host.translate(key).unwrap_or_else(|| key.to_string())
}

/// The setting's live value, the registered default while the host does not answer.
fn current(host: &dyn ClientHost, row: &Row) -> SettingValue {
    let stored = config::key(row.name).and_then(|key| host.setting(key).ok());
    match (row.kind, stored) {
        (Kind::Toggle { .. }, Some(SettingValue::Toggle(on))) => SettingValue::Toggle(on),
        (Kind::Choice { .. }, Some(SettingValue::Choice(index))) => SettingValue::Choice(index),
        (Kind::Range { .. }, Some(SettingValue::Range(value))) => SettingValue::Range(value),
        (Kind::Toggle { default }, _) => SettingValue::Toggle(default),
        (Kind::Choice { default, .. }, _) => SettingValue::Choice(default as usize),
        (Kind::Range { default, .. }, _) => SettingValue::Range(default),
    }
}

/// The text on a row's control.
fn control_text(host: &dyn ClientHost, row: &Row, value: &SettingValue) -> String {
    match (row.kind, value) {
        (Kind::Toggle { .. }, SettingValue::Toggle(on)) => {
            tr(host, if *on { "options.on" } else { "options.off" })
        }
        (Kind::Choice { options, .. }, SettingValue::Choice(index)) => options
            .get(*index)
            .map_or_else(String::new, |key| tr(host, key)),
        (Kind::Range { step, .. }, SettingValue::Range(number)) => {
            if step >= 1.0 {
                format!("{number:.0}")
            } else {
                format!("{number:.2}")
            }
        }
        _ => String::new(),
    }
}

/// The mounted settings screen.
pub struct ConfigScreen {
    size: [f32; 2],
    tab: Tab,
    /// The row of the current tab a slider drag is on.
    dragging: Option<usize>,
    rows: Vec<Row>,
}

impl ConfigScreen {
    pub fn new(size: [f32; 2]) -> Self {
        Self {
            size,
            tab: Tab::Style,
            dragging: None,
            rows: config::rows(),
        }
    }

    fn layout(&self) -> Layout {
        let tallest = Tab::ALL
            .iter()
            .map(|tab| self.rows.iter().filter(|row| row.tab == *tab).count())
            .max()
            .unwrap_or(0);
        Layout::new(self.size, tallest)
    }

    fn visible(&self) -> Vec<&Row> {
        self.rows.iter().filter(|row| row.tab == self.tab).collect()
    }

    fn set_slider(&self, host: &mut dyn ClientHost, row: &Row, x: f32) {
        if let Kind::Range { min, max, step, .. } = row.kind {
            let index = self.visible().iter().position(|r| r.name == row.name);
            if let Some(index) = index {
                let value = slider_value(self.layout().control(index), x, min, max, step);
                config::write(host, row.name, SettingValue::Range(value));
            }
        }
    }
}

impl ScreenWidget for ConfigScreen {
    fn bounds(&self) -> [f32; 4] {
        [0.0, 0.0, self.size[0], self.size[1]]
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
        let size = draw.size();
        if size[0] > 0.0 && size[1] > 0.0 {
            self.size = size;
        }
        let layout = self.layout();
        let panel = layout.panel();
        draw.fill(
            [
                panel[0] - 1.0,
                panel[1] - 1.0,
                panel[2] + 2.0,
                panel[3] + 2.0,
            ],
            PANEL_EDGE,
        );
        draw.fill(panel, PANEL_FILL);

        for (i, tab) in Tab::ALL.iter().enumerate() {
            let rect = layout.tab(i, Tab::ALL.len());
            let fill = if *tab == self.tab {
                TAB_SELECTED
            } else if contains(rect, mouse) {
                BUTTON_HOVER
            } else {
                BUTTON_FILL
            };
            draw.fill([rect[0], rect[1], rect[2] - 1.0, rect[3]], fill);
            centered(draw, &tr(host, tab.label()), rect);
        }

        let mut tooltip = None;
        for (i, row) in self.visible().into_iter().enumerate() {
            let rect = layout.row(i);
            let control = layout.control(i);
            let value = current(host, row);
            let hover = contains(control, mouse);
            draw.fill(
                control,
                if hover || self.dragging == Some(i) {
                    BUTTON_HOVER
                } else {
                    BUTTON_FILL
                },
            );
            if let (Kind::Range { min, max, .. }, SettingValue::Range(number)) = (row.kind, &value)
                && max > min
            {
                let t = ((number - min) / (max - min)).clamp(0.0, 1.0) as f32;
                draw.fill(
                    [control[0], control[1], control[2] * t, control[3]],
                    SLIDER_FILL,
                );
            }
            centered(draw, &control_text(host, row, &value), control);
            draw.text(
                &tr(host, row.label),
                [rect[0], rect[1] + (rect[3] - LINE_HEIGHT) / 2.0],
                WHITE,
                true,
            );
            if tooltip.is_none() && contains(rect, mouse) {
                tooltip = row.tooltip;
            }
        }

        let done = layout.done();
        draw.fill(
            done,
            if contains(done, mouse) {
                BUTTON_HOVER
            } else {
                BUTTON_FILL
            },
        );
        centered(draw, &tr(host, "gui.done"), done);

        if let Some(key) = tooltip {
            let width = |text: &str| draw.text_width(text);
            let lines: Vec<Text> = wrap(&tr(host, key), TOOLTIP_W, &width)
                .into_iter()
                .map(Text::literal)
                .collect();
            if !lines.is_empty() {
                let _ = draw.gui_tooltip(&lines, mouse);
            }
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
        let layout = self.layout();
        if contains(layout.done(), mouse) {
            if button == 0 {
                host.mod_screen_close(MOD_ID);
            }
            return true;
        }
        for (i, tab) in Tab::ALL.iter().enumerate() {
            if contains(layout.tab(i, Tab::ALL.len()), mouse) {
                if button == 0 {
                    self.tab = *tab;
                    self.dragging = None;
                }
                return true;
            }
        }
        let rows: Vec<Row> = self.visible().into_iter().copied().collect();
        for (i, row) in rows.iter().enumerate() {
            if !contains(layout.control(i), mouse) {
                continue;
            }
            match (row.kind, current(host, row)) {
                (Kind::Toggle { .. }, SettingValue::Toggle(on)) if button == 0 => {
                    config::write(host, row.name, SettingValue::Toggle(!on));
                }
                (Kind::Choice { options, .. }, SettingValue::Choice(index)) => {
                    let count = options.len().max(1);
                    let next = if button == 0 {
                        (index + 1) % count
                    } else {
                        (index + count - 1) % count
                    };
                    config::write(host, row.name, SettingValue::Choice(next));
                }
                (Kind::Range { .. }, _) if button == 0 => {
                    self.dragging = Some(i);
                    self.set_slider(host, row, mouse[0]);
                }
                _ => {}
            }
            return true;
        }
        false
    }

    fn mouse_dragged(
        &mut self,
        host: &mut dyn ClientHost,
        mouse: [f32; 2],
        _button: i32,
        _delta: [f32; 2],
        _modifiers: u32,
    ) -> bool {
        let Some(index) = self.dragging else {
            return false;
        };
        let row = self.visible().get(index).copied().copied();
        if let Some(row) = row {
            self.set_slider(host, &row, mouse[0]);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_panel_is_centred_and_never_off_screen() {
        let layout = Layout::new([400.0, 300.0], 9);
        assert_eq!(layout.origin[0], 40.0);
        let tiny = Layout::new([100.0, 100.0], 9);
        assert_eq!(tiny.origin, [0.0, 0.0]);
    }

    #[test]
    fn tabs_split_the_width_and_rows_stack() {
        let layout = Layout::new([400.0, 300.0], 9);
        let (a, b) = (layout.tab(0, 3), layout.tab(1, 3));
        assert_eq!(a[0] + a[2], b[0]);
        assert_eq!(layout.row(1)[1] - layout.row(0)[1], ROW_STRIDE);
        let control = layout.control(0);
        assert_eq!(control[0] + control[2], layout.row(0)[0] + layout.row(0)[2]);
        assert!(layout.row(8)[1] + ROW_H < layout.done()[1]);
    }

    #[test]
    fn hit_tests_are_half_open() {
        let rect = [10.0, 10.0, 5.0, 5.0];
        assert!(contains(rect, [10.0, 10.0]));
        assert!(!contains(rect, [15.0, 12.0]));
    }

    #[test]
    fn sliders_snap_clamp_and_ignore_bad_input() {
        let control = [100.0, 0.0, 100.0, 18.0];
        assert_eq!(slider_value(control, 150.0, -10.0, 10.0, 1.0), 0.0);
        assert_eq!(slider_value(control, 0.0, -10.0, 10.0, 1.0), -10.0);
        assert_eq!(slider_value(control, 999.0, -10.0, 10.0, 1.0), 10.0);
        assert_eq!(slider_value(control, 130.0, 0.0, 1.0, 0.01), 0.3);
        assert_eq!(slider_value(control, f32::NAN, 5.0, 9.0, 1.0), 5.0);
        assert_eq!(slider_value([0.0, 0.0, 0.0, 0.0], 1.0, 5.0, 9.0, 1.0), 5.0);
    }

    #[test]
    fn tooltips_wrap_at_spaces() {
        let width = |text: &str| text.len() as f32;
        assert_eq!(wrap("aa bb cc", 5.0, &width), ["aa bb", "cc"]);
        assert_eq!(wrap("toolongword x", 3.0, &width), ["toolongword", "x"]);
        assert!(wrap("   ", 5.0, &width).is_empty());
    }
}
