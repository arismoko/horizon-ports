//! Client-side anvil screen (`MenuScreen`), formattable rename widget (`FormattableEditBox`),
//! and formatting code reference tooltip (`FormattingGuideWidget`).

use crate::config::{ServerConfig, synced_server_config};
use crate::formatting::{
    ALL_FORMATTING_CODES, component_to_string, editable_preview_component, filter_text,
    is_allowed_chat_char, visible_length,
};
use crate::payloads::{RenameItemPayload, channels};
use crate::server::MENU_ID;
use mod_api::client::hud_layout::Blend;
use mod_api::client::widgets::ScreenWidget;
use mod_api::text::TextStyle;
use mod_api::{
    ClientHost, DrawContext, GameMode, GuiDraw, Identifier, ItemStack, MenuScreen, MenuScreenFrame,
    MenuScreenLayout, MenuScreenOpen, ScreenInitPostEvent, ScreenSlot, Text,
};
use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

const COLOR_WHITE: u32 = 0xFFFF_FFFF;
const COLOR_LABEL: u32 = 0xFF40_4040;
const COLOR_YELLOW: u32 = 0xFFFF_FF55;
const COLOR_COST_OK: u32 = 0xFF80_FF20;
const COLOR_COST_BAD: u32 = 0xFFFF_6060;
const COLOR_COST_BG: u32 = 0x4F00_0000;
const COLOR_SELECT_BG: u32 = 0x8000_00FF;
const COLOR_CURSOR: u32 = 0xFFD0_D0D0;

const MAX_VISIBLE_NAME_LEN: usize = 50;

static ACTIVE_WINDOW: AtomicI32 = AtomicI32::new(-1);
static CLIENT_CAN_PICKUP: AtomicBool = AtomicBool::new(false);

pub fn reset_client_state() {
    ACTIVE_WINDOW.store(-1, Ordering::Relaxed);
    CLIENT_CAN_PICKUP.store(false, Ordering::Relaxed);
}

pub fn client_can_pickup() -> bool {
    CLIENT_CAN_PICKUP.load(Ordering::Relaxed)
}

/// Builds the custom `easyanvils:anvil` `MenuScreen` when the server opens it.
pub fn build_anvil_screen(_host: &dyn ClientHost, open: MenuScreenOpen<'_>) -> Option<MenuScreen> {
    let window = open.window;
    ACTIVE_WINDOW.store(window, Ordering::Relaxed);
    CLIENT_CAN_PICKUP.store(false, Ordering::Relaxed);

    #[allow(clippy::needless_update)]
    let slots = vec![
        Some(ScreenSlot {
            position: [27.0, 47.0],
            ..Default::default()
        }),
        Some(ScreenSlot {
            position: [76.0, 47.0],
            ..Default::default()
        }),
        Some(ScreenSlot {
            position: [134.0, 47.0],
            ..Default::default()
        }),
    ];

    let layout = MenuScreenLayout {
        width: 176.0,
        height: 166.0,
        slots,
        inventory_x: 8.0,
        inventory_y: 84.0,
        own_background: true,
        own_labels: true,
        ..Default::default()
    };

    Some(MenuScreen::new(layout).with_draw(move |host, frame, ctx| {
        draw_anvil_screen(host, frame, ctx);
    }))
}

/// Mounts `AnvilNameField` and (when enabled) `FormattingGuideWidget` on `easyanvils:anvil`.
pub fn on_screen_init_post(host: &dyn ClientHost, event: &mut ScreenInitPostEvent<'_>) {
    if event.screen.menu != Some(MENU_ID) {
        return;
    }
    let window = ACTIVE_WINDOW.load(Ordering::Relaxed);
    let left = event.screen.rect[0];
    let top = event.screen.rect[1];
    let width = event.screen.rect[2];

    let mut field = AnvilNameField::new(window, [left + 62.0, top + 24.0, 103.0, 12.0]);
    field.sync_with_slot0(host);
    let edit_id = event.widgets.add_widget(Box::new(field));
    let _ = event.widgets.set_initial_focus(edit_id);

    if synced_server_config().renaming_supports_formatting {
        let q_width = (host.text_width(&Text::literal("?")).max(6) * 2) as f32;
        let guide =
            FormattingGuideWidget::new([left + width - 7.0 - q_width, top + 8.0, q_width, 9.0]);
        let _ = event.widgets.add_widget(Box::new(guide));
    }
}

fn client_menu_slots(host: &dyn ClientHost) -> [Option<ItemStack>; 3] {
    let Some(player) = host.local_player() else {
        return [None, None, None];
    };
    let Some(menu) = player.container_menu(host) else {
        return [None, None, None];
    };
    [
        menu.slot(host, 0).flatten(),
        menu.slot(host, 1).flatten(),
        menu.slot(host, 2).flatten(),
    ]
}

fn client_player_state(host: &dyn ClientHost) -> (bool, i32) {
    let Some(player) = host.local_player() else {
        return (false, 0);
    };
    let creative = player.game_mode(host) == Some(GameMode::Creative);
    let level = player.experience(host).map_or(0, |e| e.level);
    (creative, level)
}

fn is_too_expensive(cost: i32, creative: bool, cfg: &ServerConfig) -> bool {
    !creative && (cost == -1 || (cfg.too_expensive_limit != -1 && cost >= cfg.too_expensive_limit))
}

fn draw_anvil_screen(host: &dyn ClientHost, frame: MenuScreenFrame<'_>, ctx: &mut dyn DrawContext) {
    let ox = frame.origin[0].round() as i32;
    let oy = frame.origin[1].round() as i32;
    let slots = client_menu_slots(host);
    let has_left = slots[0].is_some();
    let has_right = slots[1].is_some();
    let has_output = slots[2].is_some();

    let mut draws = vec![
        GuiDraw::Blit {
            texture: Cow::Borrowed("gui/container/anvil"),
            blend: Blend::Textured,
            x: ox,
            y: oy,
            w: 176,
            h: 166,
            uv: [0.0, 0.0],
            src: [176, 166],
            tex: [256, 256],
            color: COLOR_WHITE,
        },
        GuiDraw::Sprite {
            sprite: Cow::Borrowed(if has_left {
                "minecraft:container/anvil/text_field"
            } else {
                "minecraft:container/anvil/text_field_disabled"
            }),
            x: ox + 59,
            y: oy + 20,
            w: 110,
            h: 16,
            region: None,
            color: COLOR_WHITE,
            blend: Blend::Textured,
        },
    ];

    if (has_left || has_right) && !has_output {
        draws.push(GuiDraw::Sprite {
            sprite: Cow::Borrowed("minecraft:container/anvil/error"),
            x: ox + 99,
            y: oy + 45,
            w: 28,
            h: 21,
            region: None,
            color: COLOR_WHITE,
            blend: Blend::Textured,
        });
    }

    let _ = ctx.gui_paint(&draws);

    // Title ("Repair & Name") at (60, 8) and "Inventory" at (8, 72)
    let title_str = host
        .translate("container.repair")
        .unwrap_or_else(|| "Repair & Name".to_owned());
    let inv_str = host
        .translate("container.inventory")
        .unwrap_or_else(|| "Inventory".to_owned());
    ctx.text(
        &title_str,
        [(ox + 60) as f32, (oy + 8) as f32],
        COLOR_LABEL,
        false,
    );
    ctx.text(
        &inv_str,
        [(ox + 8) as f32, (oy + 72) as f32],
        COLOR_LABEL,
        false,
    );

    // Enchantment cost / Too Expensive! label
    let cost = frame.data.first().copied().unwrap_or(0);
    let (creative, exp_level) = client_player_state(host);
    let cfg = synced_server_config();
    let may_pickup = has_output && cost >= 0 && (creative || exp_level >= cost);
    CLIENT_CAN_PICKUP.store(may_pickup, Ordering::Relaxed);

    if cost != 0 {
        let mut color = COLOR_COST_OK;
        let label: Option<String> = if is_too_expensive(cost, creative, &cfg) {
            color = COLOR_COST_BAD;
            Some(
                host.translate("container.repair.expensive")
                    .unwrap_or_else(|| "Too Expensive!".to_owned()),
            )
        } else if !has_output {
            None
        } else {
            if !may_pickup {
                color = COLOR_COST_BAD;
            }
            let fmt = host
                .translate("container.repair.cost")
                .unwrap_or_else(|| "Enchantment Cost: %1$s".to_owned());
            Some(
                fmt.replace("%1$s", &cost.to_string())
                    .replace("%s", &cost.to_string()),
            )
        };

        if let Some(text) = label {
            let text_w = host.text_width(&Text::literal(&text)) as f32;
            let k = (ox + 176 - 8) as f32 - text_w - 2.0;
            ctx.fill(
                [
                    k - 2.0,
                    (oy + 67) as f32,
                    (ox + 176 - 8) as f32 - (k - 2.0),
                    12.0,
                ],
                COLOR_COST_BG,
            );
            ctx.text(&text, [k, (oy + 69) as f32], color, true);
        }
    }
}

/// Custom single-line rename box supporting `§` formatting codes when enabled
/// (`FormattableEditBox`).
struct AnvilNameField {
    window: i32,
    bounds: [f32; 4],
    value: String,
    /// Cursor position in Unicode scalar characters (`0..=chars().count()`).
    cursor: usize,
    /// Selection anchor in Unicode scalar characters.
    anchor: usize,
    editable: bool,
    focused: bool,
    focus_millis: u64,
    last_slot0: Option<ItemStack>,
    initialized: bool,
}

impl AnvilNameField {
    fn new(window: i32, bounds: [f32; 4]) -> Self {
        Self {
            window,
            bounds,
            value: String::new(),
            cursor: 0,
            anchor: 0,
            editable: false,
            focused: true,
            focus_millis: 0,
            last_slot0: None,
            initialized: false,
        }
    }

    fn char_len(&self) -> usize {
        self.value.chars().count()
    }

    fn byte_index(&self, char_pos: usize) -> usize {
        self.value
            .char_indices()
            .nth(char_pos)
            .map_or(self.value.len(), |(b, _)| b)
    }

    fn selection_range(&self) -> (usize, usize) {
        if self.cursor <= self.anchor {
            (self.cursor, self.anchor)
        } else {
            (self.anchor, self.cursor)
        }
    }

    fn selected_text(&self) -> &str {
        let (a, b) = self.selection_range();
        let ba = self.byte_index(a);
        let bb = self.byte_index(b);
        &self.value[ba..bb]
    }

    fn delete_selection(&mut self) -> bool {
        let (a, b) = self.selection_range();
        if a == b {
            return false;
        }
        let ba = self.byte_index(a);
        let bb = self.byte_index(b);
        self.value.replace_range(ba..bb, "");
        self.cursor = a;
        self.anchor = a;
        true
    }

    fn insert_text(&mut self, text: &str, allow_formatting: bool) -> bool {
        let filtered = filter_text(text, allow_formatting);
        if filtered.is_empty() {
            return false;
        }
        let (a, b) = self.selection_range();
        let ba = self.byte_index(a);
        let bb = self.byte_index(b);

        let mut candidate = String::with_capacity(self.value.len() + filtered.len());
        candidate.push_str(&self.value[..ba]);
        candidate.push_str(&filtered);
        candidate.push_str(&self.value[bb..]);

        if visible_length(&candidate, allow_formatting) > MAX_VISIBLE_NAME_LEN {
            // Try adding character-by-character up to the 50 visible-char limit.
            let mut partial = String::new();
            for ch in filtered.chars() {
                let mut probe = String::with_capacity(self.value.len() + partial.len() + 4);
                probe.push_str(&self.value[..ba]);
                probe.push_str(&partial);
                probe.push(ch);
                probe.push_str(&self.value[bb..]);
                if visible_length(&probe, allow_formatting) > MAX_VISIBLE_NAME_LEN {
                    break;
                }
                partial.push(ch);
            }
            if partial.is_empty() {
                return false;
            }
            let added_chars = partial.chars().count();
            self.value.replace_range(ba..bb, &partial);
            self.cursor = a + added_chars;
            self.anchor = self.cursor;
            return true;
        }

        let added_chars = filtered.chars().count();
        self.value = candidate;
        self.cursor = a + added_chars;
        self.anchor = self.cursor;
        true
    }

    fn sync_with_slot0(&mut self, host: &dyn ClientHost) {
        let slot0 = client_menu_slots(host)[0].clone();
        if self.initialized && slot0 == self.last_slot0 {
            return;
        }
        self.initialized = true;
        self.last_slot0 = slot0.clone();

        let allow_formatting = synced_server_config().renaming_supports_formatting;
        if let Some(stack) = slot0 {
            self.value = host
                .hover_name(&stack)
                .map(|t| component_to_string(host, &t, allow_formatting))
                .unwrap_or_default();
            self.editable = true;
            let len = self.char_len();
            self.cursor = len;
            self.anchor = len;
        } else {
            self.value.clear();
            self.editable = false;
            self.cursor = 0;
            self.anchor = 0;
        }
    }

    fn notify_server(&self, host: &mut dyn ClientHost) {
        let Some(slot0) = client_menu_slots(&*host)[0].clone() else {
            return;
        };
        let mut to_send = self.value.clone();
        let has_custom_name = slot0.custom_name(&*host).ok().flatten().is_some();
        if !has_custom_name
            && let Ok(hover) = host.hover_name(&slot0)
            && to_send == host.text_string(&hover)
        {
            to_send.clear();
        }
        if let Some(ch) = channels() {
            let _ = ch.rename_item.send(
                host,
                &RenameItemPayload {
                    window: self.window,
                    name: to_send,
                },
            );
        }
    }
}

impl ScreenWidget for AnvilNameField {
    fn bounds(&self) -> [f32; 4] {
        self.bounds
    }

    fn captures_input(&self) -> bool {
        self.editable && self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn render(
        &mut self,
        host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        _mouse: [f32; 2],
        _hovered: bool,
        focused: bool,
        _partial_tick: f32,
    ) {
        self.sync_with_slot0(host);
        if focused && !self.focused {
            self.focus_millis = host.millis();
        }
        self.focused = focused;

        let allow_formatting = synced_server_config().renaming_supports_formatting;
        let [x, y, w, h] = self.bounds;
        let default_font = Identifier::parse("minecraft:default").ok();

        let cursor_byte = self.byte_index(self.cursor);
        let prefix_text = editable_preview_component(&self.value[..cursor_byte], allow_formatting);
        let cursor_px = host.text_width(&prefix_text) as f32;
        let scroll_x = (cursor_px - (w - 4.0)).max(0.0);

        let clipped = draw.push_clip([x, y - 1.0, w, h + 2.0]).is_ok();

        // Selection highlight
        let (sel_a, sel_b) = self.selection_range();
        if sel_a != sel_b {
            let a_px = host.text_width(&editable_preview_component(
                &self.value[..self.byte_index(sel_a)],
                allow_formatting,
            )) as f32;
            let b_px = host.text_width(&editable_preview_component(
                &self.value[..self.byte_index(sel_b)],
                allow_formatting,
            )) as f32;
            draw.fill(
                [x + a_px - scroll_x, y - 1.0, (b_px - a_px).max(1.0), 10.0],
                COLOR_SELECT_BG,
            );
        }

        // Styled text preview (keeps `§x` codes visible for editing)
        let preview = editable_preview_component(&self.value, allow_formatting);
        if let Some(font) = &default_font {
            let _ = draw.text_with_font(font, &preview, [x - scroll_x, y], COLOR_WHITE, true);
        } else {
            draw.text(&self.value, [x - scroll_x, y], COLOR_WHITE, true);
        }

        // Blinking caret when focused and editable
        if self.focused && self.editable {
            let blink_on =
                ((host.millis().saturating_sub(self.focus_millis)) / 300).is_multiple_of(2);
            if blink_on {
                let cx = x + cursor_px - scroll_x;
                if self.cursor < self.char_len() {
                    draw.fill([cx, y - 1.0, 1.0, 10.0], COLOR_CURSOR);
                } else {
                    draw.text("_", [cx, y], COLOR_WHITE, true);
                }
            }
        }

        if clipped {
            let _ = draw.pop_clip();
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
        if button != 0 {
            return false;
        }
        let [x, y, w, h] = self.bounds;
        if mouse[0] < x || mouse[0] > x + w || mouse[1] < y - 4.0 || mouse[1] > y + h {
            return false;
        }
        self.focused = true;
        self.focus_millis = host.millis();

        let allow_formatting = synced_server_config().renaming_supports_formatting;
        let click_rel = (mouse[0] - x).max(0.0);
        let mut best_pos = 0usize;
        let mut best_dist = click_rel;
        for (idx, (byte_idx, ch)) in self.value.char_indices().enumerate() {
            let end_byte = byte_idx + ch.len_utf8();
            let px = host.text_width(&editable_preview_component(
                &self.value[..end_byte],
                allow_formatting,
            )) as f32;
            let dist = (px - click_rel).abs();
            if dist <= best_dist {
                best_dist = dist;
                best_pos = idx + 1;
            }
        }
        self.cursor = best_pos;
        self.anchor = best_pos;
        true
    }

    fn char_typed(&mut self, host: &mut dyn ClientHost, c: char, _modifiers: u32) -> bool {
        if !self.focused || !self.editable {
            return false;
        }
        let allow_formatting = synced_server_config().renaming_supports_formatting;
        if !is_allowed_chat_char(c, allow_formatting) {
            return false;
        }
        let mut s = [0u8; 4];
        let slice = c.encode_utf8(&mut s);
        if self.insert_text(slice, allow_formatting) {
            self.focus_millis = host.millis();
            self.notify_server(host);
        }
        true
    }

    fn key_pressed(
        &mut self,
        host: &mut dyn ClientHost,
        key: &str,
        _repeat: bool,
        modifiers: u32,
    ) -> bool {
        if !self.focused {
            return false;
        }
        let shift = (modifiers & 1) != 0;
        let ctrl = (modifiers & 2) != 0;
        let allow_formatting = synced_server_config().renaming_supports_formatting;

        if ctrl {
            match key {
                "key.keyboard.a" => {
                    self.anchor = 0;
                    self.cursor = self.char_len();
                    return true;
                }
                "key.keyboard.c" => {
                    let sel = self.selected_text().to_owned();
                    if !sel.is_empty() {
                        let _ = host.set_clipboard_text(&sel);
                    }
                    return true;
                }
                "key.keyboard.x" => {
                    let sel = self.selected_text().to_owned();
                    if !sel.is_empty() {
                        let _ = host.set_clipboard_text(&sel);
                    }
                    if self.editable && self.delete_selection() {
                        self.notify_server(host);
                    }
                    return true;
                }
                "key.keyboard.v" => {
                    if self.editable
                        && let Some(clip) = host.clipboard_text()
                        && self.insert_text(&clip, allow_formatting)
                    {
                        self.focus_millis = host.millis();
                        self.notify_server(host);
                    }
                    return true;
                }
                _ => {}
            }
        }

        match key {
            "key.keyboard.left" => {
                if !shift && self.cursor != self.anchor {
                    let (a, _) = self.selection_range();
                    self.cursor = a;
                    self.anchor = a;
                } else if self.cursor > 0 {
                    self.cursor -= 1;
                    if !shift {
                        self.anchor = self.cursor;
                    }
                }
                self.focus_millis = host.millis();
                true
            }
            "key.keyboard.right" => {
                let len = self.char_len();
                if !shift && self.cursor != self.anchor {
                    let (_, b) = self.selection_range();
                    self.cursor = b;
                    self.anchor = b;
                } else if self.cursor < len {
                    self.cursor += 1;
                    if !shift {
                        self.anchor = self.cursor;
                    }
                }
                self.focus_millis = host.millis();
                true
            }
            "key.keyboard.home" => {
                self.cursor = 0;
                if !shift {
                    self.anchor = 0;
                }
                self.focus_millis = host.millis();
                true
            }
            "key.keyboard.end" => {
                self.cursor = self.char_len();
                if !shift {
                    self.anchor = self.cursor;
                }
                self.focus_millis = host.millis();
                true
            }
            "key.keyboard.backspace" => {
                if !self.editable {
                    return true;
                }
                let changed = if self.cursor != self.anchor {
                    self.delete_selection()
                } else if self.cursor > 0 {
                    let start = self.byte_index(self.cursor - 1);
                    let end = self.byte_index(self.cursor);
                    self.value.replace_range(start..end, "");
                    self.cursor -= 1;
                    self.anchor = self.cursor;
                    true
                } else {
                    false
                };
                if changed {
                    self.focus_millis = host.millis();
                    self.notify_server(host);
                }
                true
            }
            "key.keyboard.delete" => {
                if !self.editable {
                    return true;
                }
                let changed = if self.cursor != self.anchor {
                    self.delete_selection()
                } else if self.cursor < self.char_len() {
                    let start = self.byte_index(self.cursor);
                    let end = self.byte_index(self.cursor + 1);
                    self.value.replace_range(start..end, "");
                    true
                } else {
                    false
                };
                if changed {
                    self.focus_millis = host.millis();
                    self.notify_server(host);
                }
                true
            }
            "key.keyboard.escape" => false,
            _ => self.editable,
        }
    }
}

/// Top-right `?` widget displaying all 22 formatting codes in a tooltip on hover
/// (`FormattingGuideWidget`).
struct FormattingGuideWidget {
    bounds: [f32; 4],
}

impl FormattingGuideWidget {
    fn new(bounds: [f32; 4]) -> Self {
        Self { bounds }
    }
}

impl ScreenWidget for FormattingGuideWidget {
    fn bounds(&self) -> [f32; 4] {
        self.bounds
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
        let [x, y, w, h] = self.bounds;
        let active = hovered || focused;
        let q_w = host.text_width(&Text::literal("?")) as f32;
        let px = x + ((w - q_w) * 0.5).floor();
        let py = y + ((h - 9.0) * 0.5).floor();
        draw.text(
            "?",
            [px, py],
            if active { COLOR_YELLOW } else { COLOR_LABEL },
            false,
        );

        if active {
            let mut lines = Vec::with_capacity(ALL_FORMATTING_CODES.len());
            for fc in ALL_FORMATTING_CODES {
                let name_key = format!("gui.easyanvils.chat.formatting.{}", fc.name);
                let name_text = host
                    .translate(&name_key)
                    .unwrap_or_else(|| fc.name.to_owned());
                let mut name_style = TextStyle::default();
                // Black and obfuscated stay plain so they remain readable on the dark tooltip
                if fc.code != '0' && fc.code != 'k' {
                    if let Some(color_name) = fc.color {
                        name_style.color = Some(color_name.to_owned());
                    }
                    match fc.code {
                        'l' => name_style.bold = Some(true),
                        'm' => name_style.strikethrough = Some(true),
                        'n' => name_style.underlined = Some(true),
                        'o' => name_style.italic = Some(true),
                        _ => {}
                    }
                }
                let code_prefix = format!("§{} - ", fc.code);
                let line = Text::literal(code_prefix)
                    .append(Text::literal(name_text).with_style(name_style));
                lines.push(line);
            }
            let _ = draw.gui_tooltip(&lines, mouse);
        }
    }
}
