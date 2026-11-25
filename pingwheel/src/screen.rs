use crate::client::send_channel_update;
use crate::config::{
    ClientConfig, MAX_CHANNEL_LENGTH, MAX_CORRECTION_PERIOD, MAX_PING_DISTANCE, MAX_PING_DURATION,
    MOD_ID, client_config, persist_client_config, update_client_config,
};
use mod_api::text::TextStyle;
use mod_api::{
    Button, ClientHost, DrawContext, ScreenView, ScreenWidget, ScreenWidgets, Text, TextField,
};
use std::sync::atomic::{AtomicBool, Ordering};

const WHITE: u32 = 0xFFFF_FFFF;
const GRAY: u32 = 0xFFA0_A0A0;
const SLIDER_BG: u32 = 0xFF20_2020;
const SLIDER_BORDER: u32 = 0xFF80_8080;
const SLIDER_BORDER_HOVER: u32 = 0xFFFF_FFFF;
const SLIDER_HANDLE: u32 = 0xFFC6_C6C6;
const SLIDER_HANDLE_HOVER: u32 = 0xFFFF_FFFF;

static CHANNEL_DIRTY: AtomicBool = AtomicBool::new(false);

pub fn is_our_screen(screen: &ScreenView<'_>) -> bool {
    screen.mod_screen == Some(MOD_ID)
}

pub fn on_screen_init_post(
    host: &dyn ClientHost,
    screen: &ScreenView<'_>,
    widgets: &mut dyn ScreenWidgets,
) {
    if !is_our_screen(screen) {
        return;
    }

    let cfg = client_config();
    let [sx, sy, sw, sh] = screen.rect;
    let cx = sx + sw * 0.5;
    let col_w = 150.0_f32;
    let row_h = 20.0_f32;
    let gap_x = 10.0_f32;
    let gap_y = 24.0_f32;
    let left_x = cx - col_w - gap_x * 0.5;
    let right_x = cx + gap_x * 0.5;
    let top_y = sy + 36.0;

    // Row 0: Ping Volume | Ping Duration
    widgets.add_widget(Box::new(IntSlider::new(
        [left_x, top_y, col_w, row_h],
        0,
        100,
        1,
        cfg.ping_volume,
        format_volume_label,
        |host, value| {
            let updated = update_client_config(|c| c.ping_volume = value);
            updated.write_to_host_settings(host);
            persist_client_config(host, &updated);
        },
    )));
    widgets.add_widget(Box::new(IntSlider::new(
        [right_x, top_y, col_w, row_h],
        1,
        MAX_PING_DURATION,
        1,
        cfg.ping_duration,
        format_duration_label,
        |host, value| {
            let updated = update_client_config(|c| c.ping_duration = value);
            updated.write_to_host_settings(host);
            persist_client_config(host, &updated);
        },
    )));

    // Row 1: Ping Distance | Correction Period
    widgets.add_widget(Box::new(IntSlider::new(
        [left_x, top_y + gap_y, col_w, row_h],
        0,
        MAX_PING_DISTANCE,
        16,
        cfg.ping_distance,
        format_distance_label,
        |host, value| {
            let updated = update_client_config(|c| c.ping_distance = value);
            updated.write_to_host_settings(host);
            persist_client_config(host, &updated);
        },
    )));
    widgets.add_widget(Box::new(IntSlider::new(
        [right_x, top_y + gap_y, col_w, row_h],
        1,
        (MAX_CORRECTION_PERIOD * 10.0) as i32,
        1,
        (cfg.correction_period * 10.0).round() as i32,
        format_correction_label,
        |host, tenths| {
            let updated = update_client_config(|c| c.correction_period = tenths as f32 / 10.0);
            updated.write_to_host_settings(host);
            persist_client_config(host, &updated);
        },
    )));

    // Row 2: Item Icons | Direction Indicator
    widgets.add_button(Button::new(
        [left_x, top_y + gap_y * 2.0, col_w, row_h],
        format_bool_option(
            "settings.pingwheel.item_icon_visible",
            cfg.item_icon_visible,
        ),
        |state, host| {
            let updated = update_client_config(|c| c.item_icon_visible = !c.item_icon_visible);
            updated.write_to_host_settings(host);
            persist_client_config(host, &updated);
            state.label = format_bool_option(
                "settings.pingwheel.item_icon_visible",
                updated.item_icon_visible,
            );
        },
    ));
    widgets.add_button(Button::new(
        [right_x, top_y + gap_y * 2.0, col_w, row_h],
        format_bool_option(
            "settings.pingwheel.direction_indicator_visible",
            cfg.direction_indicator_visible,
        ),
        |state, host| {
            let updated = update_client_config(|c| {
                c.direction_indicator_visible = !c.direction_indicator_visible
            });
            updated.write_to_host_settings(host);
            persist_client_config(host, &updated);
            state.label = format_bool_option(
                "settings.pingwheel.direction_indicator_visible",
                updated.direction_indicator_visible,
            );
        },
    ));

    // Row 3: Player Info | Team Color
    widgets.add_button(Button::new(
        [left_x, top_y + gap_y * 3.0, col_w, row_h],
        format_enum_option(
            "settings.pingwheel.player_info_mode",
            cfg.player_info_mode.as_str(),
        ),
        |state, host| {
            let updated = update_client_config(|c| c.player_info_mode = c.player_info_mode.next());
            updated.write_to_host_settings(host);
            persist_client_config(host, &updated);
            state.label = format_enum_option(
                "settings.pingwheel.player_info_mode",
                updated.player_info_mode.as_str(),
            );
        },
    ));
    widgets.add_button(Button::new(
        [right_x, top_y + gap_y * 3.0, col_w, row_h],
        format_enum_option(
            "settings.pingwheel.team_color_mode",
            cfg.team_color_mode.as_str(),
        ),
        |state, host| {
            let updated = update_client_config(|c| c.team_color_mode = c.team_color_mode.next());
            updated.write_to_host_settings(host);
            persist_client_config(host, &updated);
            state.label = format_enum_option(
                "settings.pingwheel.team_color_mode",
                updated.team_color_mode.as_str(),
            );
        },
    ));

    // Row 4: Ping Size
    widgets.add_widget(Box::new(IntSlider::new(
        [left_x, top_y + gap_y * 4.0, col_w, row_h],
        40,
        300,
        10,
        cfg.ping_size,
        format_size_label,
        |host, value| {
            let updated = update_client_config(|c| c.ping_size = value);
            updated.write_to_host_settings(host);
            persist_client_config(host, &updated);
        },
    )));

    // Channel text field
    let channel_y = top_y + gap_y * 5.0 + 16.0;
    let server_addr = host
        .client_session()
        .client_server()
        .and_then(|s| s.address)
        .map(str::to_owned);
    let current_channel = cfg.active_channel(server_addr.as_deref());
    let mut hint_style = TextStyle::default();
    hint_style.color = Some("dark_gray".to_owned());
    hint_style.italic = Some(true);
    let hint = Text::translate("value.pingwheel.global", Vec::new()).with_style(hint_style);

    widgets.add_text_field(
        TextField::new([cx - 100.0, channel_y, 200.0, 20.0], MAX_CHANNEL_LENGTH)
            .value(current_channel)
            .hint(hint)
            .on_change(|text, host| {
                let addr = host
                    .client_session()
                    .client_server()
                    .and_then(|s| s.address)
                    .map(str::to_owned);
                let updated = update_client_config(|c| {
                    c.set_active_channel(addr.as_deref(), text);
                });
                persist_client_config(host, &updated);
                CHANNEL_DIRTY.store(true, Ordering::Relaxed);
            })
            .on_submit(|_, host| {
                send_channel_update(host);
                CHANNEL_DIRTY.store(false, Ordering::Relaxed);
            }),
    );

    // Done button at bottom
    let done_y = (sy + sh - 32.0).max(channel_y + 30.0);
    widgets.add_button(Button::new(
        [cx - 100.0, done_y, 200.0, 20.0],
        Text::translate("gui.done", Vec::new()),
        |_, host| {
            if CHANNEL_DIRTY.swap(false, Ordering::Relaxed) {
                send_channel_update(host);
            }
            host.mod_screen_close(MOD_ID);
        },
    ));
}

pub fn on_screen_render_post(
    host: &dyn ClientHost,
    draw: &mut dyn DrawContext,
    screen: &ScreenView<'_>,
) {
    if !is_our_screen(screen) {
        return;
    }

    let [sx, sy, sw, _] = screen.rect;
    let cx = sx + sw * 0.5;

    let title = host
        .translate("settings.pingwheel.title")
        .unwrap_or_else(|| "Ping Wheel Settings".to_owned());
    let title_w = draw.text_width(&title);
    draw.text(&title, [cx - title_w * 0.5, sy + 14.0], WHITE, true);

    let channel_y = sy + 36.0 + 24.0 * 5.0 + 16.0;
    let channel_label = host
        .translate("settings.pingwheel.channel")
        .unwrap_or_else(|| "Ping Channel".to_owned());
    draw.text(&channel_label, [cx - 100.0, channel_y - 12.0], GRAY, false);

    // Show channel tooltip when hovering over the channel box.
    let [mx, my] = screen.mouse;
    if mx >= cx - 100.0 && mx <= cx + 100.0 && my >= channel_y && my <= channel_y + 20.0 {
        let tip = Text::translate("settings.pingwheel.channel.tooltip", Vec::new());
        let lines = host.text_split(&tip, 200);
        let _ = draw.gui_tooltip(&lines, [mx, my]);
    }
}

pub fn on_screen_closing(host: &dyn ClientHost, screen: &ScreenView<'_>) {
    if !is_our_screen(screen) {
        return;
    }
    let cfg: ClientConfig = client_config();
    persist_client_config(host, &cfg);
}

pub fn take_channel_dirty() -> bool {
    CHANNEL_DIRTY.swap(false, Ordering::Relaxed)
}

fn format_volume_label(host: &dyn ClientHost, value: i32) -> String {
    let val_str = if value == 0 {
        host.translate("options.off")
            .unwrap_or_else(|| "OFF".to_owned())
    } else {
        format!("{value}%")
    };
    format_setting_pair(
        host,
        "settings.pingwheel.ping_volume",
        "Ping Volume",
        &val_str,
    )
}

fn format_duration_label(host: &dyn ClientHost, value: i32) -> String {
    let val_str = if value >= MAX_PING_DURATION {
        host.translate("value.pingwheel.infinite")
            .unwrap_or_else(|| "∞".to_owned())
    } else {
        format!("{value}s")
    };
    format_setting_pair(
        host,
        "settings.pingwheel.ping_duration",
        "Ping Duration",
        &val_str,
    )
}

fn format_distance_label(host: &dyn ClientHost, value: i32) -> String {
    let val_str = if value == 0 {
        host.translate("value.pingwheel.hidden")
            .unwrap_or_else(|| "Hidden".to_owned())
    } else if value >= MAX_PING_DISTANCE {
        host.translate("value.pingwheel.infinite")
            .unwrap_or_else(|| "∞".to_owned())
    } else {
        format!("{value}m")
    };
    format_setting_pair(
        host,
        "settings.pingwheel.ping_distance",
        "Ping Distance",
        &val_str,
    )
}

fn format_correction_label(host: &dyn ClientHost, tenths: i32) -> String {
    let secs = tenths as f32 / 10.0;
    let val_str = if secs >= MAX_CORRECTION_PERIOD {
        host.translate("value.pingwheel.infinite")
            .unwrap_or_else(|| "∞".to_owned())
    } else {
        format!("{secs:.1}s")
    };
    format_setting_pair(
        host,
        "settings.pingwheel.correction_period",
        "Correction Period",
        &val_str,
    )
}

fn format_size_label(host: &dyn ClientHost, value: i32) -> String {
    let val_str = format!("{value}%");
    format_setting_pair(host, "settings.pingwheel.ping_size", "Ping Size", &val_str)
}

fn format_setting_pair(
    host: &dyn ClientHost,
    key: &str,
    fallback_label: &str,
    value: &str,
) -> String {
    match host.translate(key) {
        Some(template) if template.contains("%s") => template.replacen("%s", value, 1),
        Some(label) => format!("{label}: {value}"),
        None => format!("{fallback_label}: {value}"),
    }
}

fn format_bool_option(key: &str, value: bool) -> Text {
    let state_key = if value { "options.on" } else { "options.off" };
    Text::translate(key, Vec::new())
        .append(Text::literal(": "))
        .append(Text::translate(state_key, Vec::new()))
}

fn format_enum_option(key: &str, variant: &str) -> Text {
    Text::translate(key, Vec::new())
        .append(Text::literal(": "))
        .append(Text::translate(
            format!("value.pingwheel.{variant}"),
            Vec::new(),
        ))
}

struct IntSlider {
    rect: [f32; 4],
    min: i32,
    max: i32,
    step: i32,
    value: i32,
    dragging: bool,
    format_label: fn(&dyn ClientHost, i32) -> String,
    on_change: fn(&mut dyn ClientHost, i32),
}

impl IntSlider {
    fn new(
        rect: [f32; 4],
        min: i32,
        max: i32,
        step: i32,
        value: i32,
        format_label: fn(&dyn ClientHost, i32) -> String,
        on_change: fn(&mut dyn ClientHost, i32),
    ) -> Self {
        Self {
            rect,
            min,
            max,
            step: step.max(1),
            value: value.clamp(min, max),
            dragging: false,
            format_label,
            on_change,
        }
    }

    fn ratio(&self) -> f32 {
        let span = (self.max - self.min).max(1) as f32;
        ((self.value - self.min) as f32 / span).clamp(0.0, 1.0)
    }

    fn set_from_mouse(&mut self, host: &mut dyn ClientHost, mouse_x: f32) {
        let [x, _, w, _] = self.rect;
        let usable = (w - 8.0).max(1.0);
        let t = ((mouse_x - (x + 4.0)) / usable).clamp(0.0, 1.0);
        let raw = self.min as f32 + t * (self.max - self.min) as f32;
        let steps = ((raw - self.min as f32) / self.step as f32).round() as i32;
        let snapped = (self.min + steps * self.step).clamp(self.min, self.max);
        if snapped != self.value {
            self.value = snapped;
            (self.on_change)(host, snapped);
        }
    }
}

impl ScreenWidget for IntSlider {
    fn bounds(&self) -> [f32; 4] {
        self.rect
    }

    fn render(
        &mut self,
        host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        _mouse: [f32; 2],
        hovered: bool,
        focused: bool,
        _partial_tick: f32,
    ) {
        let [x, y, w, h] = self.rect;
        let active = hovered || focused || self.dragging;
        let border = if active {
            SLIDER_BORDER_HOVER
        } else {
            SLIDER_BORDER
        };
        draw.fill([x, y, w, h], border);
        draw.fill([x + 1.0, y + 1.0, w - 2.0, h - 2.0], SLIDER_BG);

        let handle_x = x + self.ratio() * (w - 8.0);
        let handle_col = if active {
            SLIDER_HANDLE_HOVER
        } else {
            SLIDER_HANDLE
        };
        draw.fill([handle_x, y, 8.0, h], handle_col);

        let label = (self.format_label)(host, self.value);
        let tw = draw.text_width(&label);
        draw.text(
            &label,
            [x + (w - tw) * 0.5, y + (h - 8.0) * 0.5],
            WHITE,
            true,
        );
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
        self.dragging = true;
        self.set_from_mouse(host, mouse[0]);
        true
    }

    fn mouse_dragged(
        &mut self,
        host: &mut dyn ClientHost,
        mouse: [f32; 2],
        button: i32,
        _delta: [f32; 2],
        _modifiers: u32,
    ) -> bool {
        if button != 0 || !self.dragging {
            return false;
        }
        self.set_from_mouse(host, mouse[0]);
        true
    }

    fn mouse_released(
        &mut self,
        _host: &mut dyn ClientHost,
        _mouse: [f32; 2],
        button: i32,
        _modifiers: u32,
    ) -> bool {
        if button == 0 && self.dragging {
            self.dragging = false;
            return true;
        }
        false
    }

    fn key_pressed(
        &mut self,
        host: &mut dyn ClientHost,
        key: &str,
        _repeat: bool,
        _modifiers: u32,
    ) -> bool {
        let delta = match key {
            "key.keyboard.left" => -self.step,
            "key.keyboard.right" => self.step,
            _ => return false,
        };
        let next = (self.value + delta).clamp(self.min, self.max);
        if next != self.value {
            self.value = next;
            (self.on_change)(host, next);
        }
        true
    }
}
