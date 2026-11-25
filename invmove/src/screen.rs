//! InvMove's own config screen, opened by the Mods screen's Config button
//! (`SettingsRegistrar::register_config_screen`): tabs for the categories, a paged list of rows
//! per tab, and the lists the config file holds (extra keys, unrecognized screens).

use std::sync::{Arc, Mutex};

use mod_api::client::widgets::{Button, ScreenWidget, ScreenWidgets, TextField};
use mod_api::{ClientHost, DrawContext, SettingKey, SettingValue, Text};

use crate::config::{Keys, MOD_ID, MOVEMENT_KEYS, write_config_file};
use crate::{Runtime, lock};

/// The tab names' translation keys, in order.
const TABS: [&str; 7] = [
    "config.invmove.tab.general",
    "config.invmove.tab.movement",
    "config.invmove.tab.keys",
    "config.invmove.tab.background",
    "config.invmove.tab.screens_movement",
    "config.invmove.tab.screens_background",
    "config.invmove.tab.unrecognized",
];

const TAB_KEYS: usize = 2;
const TAB_UNRECOGNIZED: usize = 6;

const TOP: f32 = 30.0;
const ROW_H: f32 = 20.0;
const GAP: f32 = 4.0;
/// Space under the rows for the extra-key field and the Done button.
const FOOT: f32 = 58.0;
const PAGER_H: f32 = 20.0;
const PAGER_BUTTON_W: f32 = 60.0;

const TEXT: u32 = 0xFFFF_FFFF;
const MUTED: u32 = 0xFFA0_A0A0;
const BORDER: u32 = 0xFF00_0000;
const FILL: u32 = 0xFF3C_3C3C;
const FILL_HOVER: u32 = 0xFF5A_5A5A;
const FILL_SELECTED: u32 = 0xFF3A_6EA5;

/// Which tab and page the screen shows; reset each time Config opens it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Ui {
    tab: usize,
    page: usize,
}

/// One row of a tab.
enum Row {
    /// A registered setting: a toggle, or a choice that cycles.
    Setting(SettingKey),
    /// A screen the config file lists as unrecognized, with its movement or background choice.
    Listed { movement: bool, name: String },
    /// An extra key from the config file; pressing the row removes it.
    Extra(String),
}

fn rows_of(tab: usize, rt: &Runtime, keys: &Keys) -> Vec<Row> {
    let settings = |list: &[SettingKey]| list.iter().copied().map(Row::Setting).collect();
    match tab {
        0 => settings(&[keys.enable]),
        1 => settings(&[
            keys.movement,
            keys.sneak,
            keys.sneak_disallowed,
            keys.sneak_flying,
            keys.dismount,
            keys.text_field,
            keys.recipe_book,
            keys.unrecognized_movement,
        ]),
        TAB_KEYS => {
            let mut rows: Vec<Row> = keys
                .allowed
                .iter()
                .map(|(_, key)| Row::Setting(*key))
                .collect();
            rows.extend(rt.file.extra_keys.iter().cloned().map(Row::Extra));
            rows
        }
        3 => settings(&[
            keys.background,
            keys.hide_on_pause,
            keys.unrecognized_background,
        ]),
        4 => keys
            .screen_movement
            .iter()
            .map(|(_, key)| Row::Setting(*key))
            .collect(),
        5 => keys
            .screen_background
            .iter()
            .map(|(_, key)| Row::Setting(*key))
            .collect(),
        TAB_UNRECOGNIZED => {
            let listed = |movement: bool, map: &std::collections::BTreeMap<String, bool>| {
                map.keys()
                    .map(|name| Row::Listed {
                        movement,
                        name: name.clone(),
                    })
                    .collect::<Vec<_>>()
            };
            let mut rows = listed(true, &rt.file.unrecognized_movement);
            rows.extend(listed(false, &rt.file.unrecognized_background));
            rows
        }
        _ => Vec::new(),
    }
}

fn page_count(rows: usize, per_page: usize) -> usize {
    rows.div_ceil(per_page).max(1)
}

fn translate(host: &dyn ClientHost, key: &str) -> String {
    host.translate(key).unwrap_or_else(|| key.to_owned())
}

fn on_off(host: &dyn ClientHost, on: bool) -> String {
    if on {
        translate(host, "options.on")
    } else {
        translate(host, "options.off")
    }
}

/// The text a row shows.
fn describe(host: &dyn ClientHost, keys: &Keys, rt: &Runtime, row: &Row) -> String {
    match row {
        Row::Setting(key) => {
            let Some(meta) = keys.meta.get(key) else {
                return String::new();
            };
            let label = translate(host, &meta.label);
            let value = match &meta.options {
                Some(options) => {
                    let index = host.setting_choice(*key).map_or(0, |index| index as usize);
                    options
                        .get(index)
                        .map_or_else(String::new, |option| translate(host, option))
                }
                None => on_off(host, host.setting_bool(*key).unwrap_or(true)),
            };
            format!("{label}: {value}")
        }
        Row::Listed { movement, name } => {
            let (label, map) = if *movement {
                (
                    "config.invmove.list.movement",
                    &rt.file.unrecognized_movement,
                )
            } else {
                (
                    "config.invmove.list.background",
                    &rt.file.unrecognized_background,
                )
            };
            let on = map.get(name).copied().unwrap_or(true);
            format!("{} {name}: {}", translate(host, label), on_off(host, on))
        }
        Row::Extra(name) => format!("{} {name}", translate(host, "config.invmove.list.remove")),
    }
}

/// What pressing a row does; `backwards` (right click) cycles a choice the other way.
fn activate(host: &mut dyn ClientHost, keys: &Keys, rt: &mut Runtime, row: &Row, backwards: bool) {
    match row {
        Row::Setting(key) => {
            let options = keys
                .meta
                .get(key)
                .and_then(|meta| meta.options.as_ref())
                .map_or(0, Vec::len);
            if options > 0 {
                let current = host.setting_choice(*key).map_or(0, |index| index as usize);
                let next = if backwards {
                    (current + options - 1) % options
                } else {
                    (current + 1) % options
                };
                let _ = host.set_setting(*key, SettingValue::Choice(next));
            } else {
                let on = host.setting_bool(*key).unwrap_or(true);
                let _ = host.set_setting(*key, SettingValue::Toggle(!on));
            }
        }
        Row::Listed { movement, name } => {
            let map = if *movement {
                &mut rt.file.unrecognized_movement
            } else {
                &mut rt.file.unrecognized_background
            };
            if let Some(on) = map.get_mut(name) {
                *on = !*on;
                rt.file_dirty = true;
            }
        }
        Row::Extra(name) => {
            rt.file.extra_keys.retain(|key| key != name);
            rt.file_dirty = true;
        }
    }
}

/// Adds a vanilla key mapping the player typed to the extra keys.
fn add_extra_key(shared: &Mutex<Runtime>, host: &mut dyn ClientHost, text: &str) {
    let name = text.trim();
    if name.is_empty()
        || host.vanilla_key_mapping(name).is_err()
        || MOVEMENT_KEYS.iter().any(|(mapping, _)| *mapping == name)
    {
        return;
    }
    let write = {
        let mut rt = lock(shared);
        if rt.file.extra_keys.iter().any(|key| key == name) {
            return;
        }
        rt.file.extra_keys.push(name.to_owned());
        rt.file_dirty = true;
        rt.take_file_write()
    };
    if let Some(text) = write {
        write_config_file(host, &text);
    }
}

/// Where everything sits for a window size, in GUI pixels.
#[derive(Clone, Copy, Debug)]
struct Layout {
    size: [f32; 2],
    left: f32,
    side_w: f32,
    content_x: f32,
    content_w: f32,
    per_page: usize,
}

impl Layout {
    fn of(size: [f32; 2]) -> Self {
        let total = (size[0] - 16.0).clamp(200.0, 460.0);
        let left = ((size[0] - total) / 2.0).max(0.0).floor();
        let side_w = (total * 0.3).floor().min(120.0);
        let rows_h = size[1] - FOOT - TOP - PAGER_H - GAP;
        let per_page = ((rows_h + GAP) / (ROW_H + GAP)).floor().max(1.0) as usize;
        Self {
            size,
            left,
            side_w,
            content_x: left + side_w + 6.0,
            content_w: total - side_w - 6.0,
            per_page,
        }
    }

    fn tab_rect(&self, index: usize) -> [f32; 4] {
        [
            self.left,
            TOP + index as f32 * (ROW_H + GAP),
            self.side_w,
            ROW_H,
        ]
    }

    fn row_rect(&self, index: usize) -> [f32; 4] {
        [
            self.content_x,
            TOP + index as f32 * (ROW_H + GAP),
            self.content_w,
            ROW_H,
        ]
    }

    fn pager_y(&self) -> f32 {
        self.size[1] - FOOT - PAGER_H
    }

    fn prev_rect(&self) -> [f32; 4] {
        [self.content_x, self.pager_y(), PAGER_BUTTON_W, PAGER_H]
    }

    fn next_rect(&self) -> [f32; 4] {
        [
            self.content_x + self.content_w - PAGER_BUTTON_W,
            self.pager_y(),
            PAGER_BUTTON_W,
            PAGER_H,
        ]
    }

    /// Everything above the footer, which holds the mounted field and button.
    fn panel_bounds(&self) -> [f32; 4] {
        [0.0, 0.0, self.size[0], self.size[1] - FOOT]
    }
}

fn hit(rect: [f32; 4], point: [f32; 2]) -> bool {
    point[0] >= rect[0]
        && point[0] < rect[0] + rect[2]
        && point[1] >= rect[1]
        && point[1] < rect[1] + rect[3]
}

/// Shortens `text` until it fits `max_width`.
fn fit(draw: &dyn DrawContext, text: &str, max_width: f32) -> String {
    let mut text = text.to_owned();
    if draw.text_width(&text) <= max_width {
        return text;
    }
    while text.pop().is_some() {
        let shown = format!("{text}..");
        if draw.text_width(&shown) <= max_width {
            return shown;
        }
    }
    text
}

fn draw_button(
    draw: &mut dyn DrawContext,
    rect: [f32; 4],
    label: &str,
    hovered: bool,
    selected: bool,
) {
    let fill = if selected {
        FILL_SELECTED
    } else if hovered {
        FILL_HOVER
    } else {
        FILL
    };
    draw.fill(rect, BORDER);
    draw.fill(
        [rect[0] + 1.0, rect[1] + 1.0, rect[2] - 2.0, rect[3] - 2.0],
        fill,
    );
    let label = fit(draw, label, rect[2] - 6.0);
    let width = draw.text_width(&label);
    draw.text(
        &label,
        [
            (rect[0] + (rect[2] - width) / 2.0).floor(),
            (rect[1] + (rect[3] - 8.0) / 2.0).floor(),
        ],
        TEXT,
        true,
    );
}

/// The tabs, rows and pager, drawn and hit-tested as one widget since which rows exist changes
/// with the tab and page.
struct Panel {
    shared: Arc<Mutex<Runtime>>,
    keys: Arc<Keys>,
    layout: Layout,
}

impl ScreenWidget for Panel {
    fn bounds(&self) -> [f32; 4] {
        self.layout.panel_bounds()
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
        self.layout = Layout::of(draw.size());
        let layout = self.layout;
        let rt = lock(&self.shared);
        let ui = rt.ui;

        let title = translate(host, "config.invmove.screen.title");
        let title_width = draw.text_width(&title);
        draw.text(
            &title,
            [(layout.size[0] - title_width) / 2.0, 10.0],
            TEXT,
            true,
        );

        for (index, key) in TABS.iter().enumerate() {
            let rect = layout.tab_rect(index);
            draw_button(
                draw,
                rect,
                &translate(host, key),
                hit(rect, mouse),
                index == ui.tab,
            );
        }

        let rows = rows_of(ui.tab, &rt, &self.keys);
        let pages = page_count(rows.len(), layout.per_page);
        let page = ui.page.min(pages - 1);
        let mut tooltip = None;
        for (slot, row) in rows
            .iter()
            .skip(page * layout.per_page)
            .take(layout.per_page)
            .enumerate()
        {
            let rect = layout.row_rect(slot);
            let hovered = hit(rect, mouse);
            draw_button(
                draw,
                rect,
                &describe(host, &self.keys, &rt, row),
                hovered,
                false,
            );
            if hovered && let Row::Setting(key) = row {
                tooltip = self
                    .keys
                    .meta
                    .get(key)
                    .and_then(|meta| meta.tooltip.clone());
            }
        }
        if rows.is_empty() {
            let note = fit(
                draw,
                &translate(host, "config.invmove.list.empty"),
                layout.content_w,
            );
            draw.text(&note, [layout.content_x, TOP + 6.0], MUTED, false);
        }
        if pages > 1 {
            for (rect, key) in [
                (layout.prev_rect(), "config.invmove.page.previous"),
                (layout.next_rect(), "config.invmove.page.next"),
            ] {
                draw_button(draw, rect, &translate(host, key), hit(rect, mouse), false);
            }
            let label = format!("{}/{}", page + 1, pages);
            let width = draw.text_width(&label);
            draw.text(
                &label,
                [
                    (layout.content_x + (layout.content_w - width) / 2.0).floor(),
                    layout.pager_y() + 6.0,
                ],
                MUTED,
                false,
            );
        }
        if let Some(tooltip) = tooltip {
            let _ = draw.gui_tooltip(&[Text::translate(tooltip, vec![])], mouse);
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
        let layout = self.layout;
        let write = {
            let mut rt = lock(&self.shared);
            if let Some(tab) = (0..TABS.len()).find(|tab| hit(layout.tab_rect(*tab), mouse)) {
                rt.ui = Ui { tab, page: 0 };
                return true;
            }
            let rows = rows_of(rt.ui.tab, &rt, &self.keys);
            let pages = page_count(rows.len(), layout.per_page);
            let page = rt.ui.page.min(pages - 1);
            if pages > 1 {
                if hit(layout.prev_rect(), mouse) {
                    rt.ui.page = page.saturating_sub(1);
                    return true;
                }
                if hit(layout.next_rect(), mouse) {
                    rt.ui.page = (page + 1).min(pages - 1);
                    return true;
                }
            }
            let Some(row) = (0..layout.per_page)
                .find(|slot| hit(layout.row_rect(*slot), mouse))
                .and_then(|slot| rows.get(page * layout.per_page + slot))
            else {
                return false;
            };
            activate(host, &self.keys, &mut rt, row, button == 1);
            rt.take_file_write()
        };
        if let Some(text) = write {
            write_config_file(host, &text);
        }
        true
    }

    fn mouse_scrolled(
        &mut self,
        _host: &mut dyn ClientHost,
        mouse: [f32; 2],
        scroll: [f64; 2],
        _modifiers: u32,
    ) -> bool {
        let layout = self.layout;
        if mouse[0] < layout.content_x || scroll[1] == 0.0 {
            return false;
        }
        let mut rt = lock(&self.shared);
        let rows = rows_of(rt.ui.tab, &rt, &self.keys);
        let pages = page_count(rows.len(), layout.per_page);
        let page = rt.ui.page.min(pages - 1);
        rt.ui.page = if scroll[1] > 0.0 {
            page.saturating_sub(1)
        } else {
            (page + 1).min(pages - 1)
        };
        true
    }
}

/// Mounts the screen's widgets during `screen_init_post`; `rect` is the screen's own, the window.
pub fn mount(
    widgets: &mut dyn ScreenWidgets,
    rect: [f32; 4],
    shared: &Arc<Mutex<Runtime>>,
    keys: &Arc<Keys>,
) {
    let size = [rect[2], rect[3]];
    let middle = (size[0] / 2.0 - 100.0).floor();
    widgets.add_widget(Box::new(Panel {
        shared: Arc::clone(shared),
        keys: Arc::clone(keys),
        layout: Layout::of(size),
    }));
    let extra = Arc::clone(shared);
    widgets.add_text_field(
        TextField::new([middle, size[1] - 52.0, 200.0, 20.0], 64)
            .hint(Text::translate("config.invmove.extra_key.hint", vec![]))
            .on_submit(move |text, host| add_extra_key(&extra, host, text)),
    );
    widgets.add_button(Button::new(
        [middle, size[1] - 28.0, 200.0, 20.0],
        Text::translate("gui.done", vec![]),
        |_state, host| host.mod_screen_close(MOD_ID),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_cover_every_row() {
        assert_eq!(page_count(0, 5), 1);
        assert_eq!(page_count(5, 5), 1);
        assert_eq!(page_count(6, 5), 2);
        assert_eq!(page_count(23, 8), 3);
    }

    #[test]
    fn the_layout_keeps_one_row_per_page_on_a_tiny_window() {
        let layout = Layout::of([100.0, 50.0]);
        assert_eq!(layout.per_page, 1);
        assert!(layout.content_w > 0.0);
    }

    #[test]
    fn the_rows_fit_above_the_pager() {
        for height in [180.0, 270.0, 540.0] {
            let layout = Layout::of([480.0, height]);
            let last = layout.row_rect(layout.per_page - 1);
            assert!(last[1] + last[3] <= layout.pager_y(), "{height}");
        }
    }

    #[test]
    fn hits_include_the_top_left_edge_only() {
        let rect = [10.0, 10.0, 20.0, 20.0];
        assert!(hit(rect, [10.0, 10.0]));
        assert!(hit(rect, [29.9, 29.9]));
        assert!(!hit(rect, [30.0, 15.0]));
        assert!(!hit(rect, [9.9, 15.0]));
    }
}
