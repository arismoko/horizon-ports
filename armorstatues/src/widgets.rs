//! Custom mounted `ScreenWidget` implementations for the Armor Statues tabbed menu screen.
use mod_api::client::widgets::{EditKey, ScreenWidget, TextEdit};
use mod_api::{ClientHost, DrawContext};

use crate::client::{
    SharedState, apply_alignment, apply_pose_now, clear_name_focus, click_sound, confirm_active,
    draw_button_bg, draw_centered_str, draw_checkbox_bg, draw_slider_track, flush_and_send_edit,
    flush_dirty, get_clamp_rotations, get_clipboard, get_first_pose_index,
    get_move_increment_index, lock, send_edit, send_vt_trigger, set_clamp_rotations, set_clipboard,
    set_confirm, set_first_pose_index, set_move_increment_index, switch_tab, tr,
};
use crate::commands::{
    VT_CHECK_TARGET, VT_LOCK, VT_SWAP_MAINHAND_HEAD, VT_SWAP_MAINHAND_OFFHAND, VT_TOOL_RACK,
    VT_UNLOCK,
};
use crate::model::{
    Alignment, DEFAULT_POSE, DEFAULT_SCALE, DEGREES_SNAP_INTERVAL, EditMessage, MAX_SCALE,
    MIN_SCALE, MOVE_INCREMENTS, MUTATORS, POSES, SQUARE_SLIDER_KEY_STEP, StyleOption, SyncMode,
    Tab, format_four_decimals, from_log_scale, from_wrapped_degrees, mirror_pose, random_pose,
    round_sixteenth, snap_value, to_log_scale, to_wrapped_degrees,
};

pub const PANEL_W: f32 = 210.0;
pub const PANEL_H: f32 = 188.0;
pub const TAB_W: f32 = 36.0;
pub const TAB_H: f32 = 26.0;
pub const TAB_STRIDE: f32 = 27.0;

pub const CONFIRM_IMG_TICKS: u32 = 20;
pub const CONFIRM_TXT_TICKS: u32 = 30;

/// Confirmation IDs for buttons that show a tick / "Aligned!" state.
pub const CONFIRM_RANDOMIZE: u32 = 1;
pub const CONFIRM_RESET: u32 = 2;
pub const CONFIRM_MIRROR: u32 = 3;
pub const CONFIRM_COPY: u32 = 4;
pub const CONFIRM_PASTE: u32 = 5;
pub const CONFIRM_SCALE_RESET: u32 = 6;
pub const CONFIRM_CENTERED: u32 = 7;
pub const CONFIRM_CORNERED: u32 = 8;
pub const CONFIRM_ALIGN_BASE: u32 = 10;
pub const CONFIRM_VT_BASE: u32 = 20;

pub fn tabs_start_y(count: usize) -> f32 {
    let n = count as f32;
    let total = n * TAB_H + (n - 1.0);
    ((PANEL_H - total) / 2.0).floor()
}

pub fn tab_rect(origin: [f32; 2], count: usize, index: usize) -> [f32; 4] {
    [
        origin[0] - 32.0,
        origin[1] + tabs_start_y(count) + TAB_STRIDE * index as f32,
        32.0,
        TAB_H,
    ]
}

pub fn hovered_tab(origin: [f32; 2], tabs: &[Tab], mouse: [f32; 2]) -> Option<Tab> {
    for (i, &tab) in tabs.iter().enumerate() {
        let [x, y, w, h] = tab_rect(origin, tabs.len(), i);
        if mouse[0] > x && mouse[0] <= x + w && mouse[1] > y && mouse[1] <= y + h {
            return Some(tab);
        }
    }
    None
}

pub fn position_row_y(origin: [f32; 2], row: usize) -> f32 {
    let total = 6.0 * 22.0 + 5.0 * 7.0;
    let start_y = ((PANEL_H - total) / 2.0).floor();
    origin[1] + start_y + row as f32 * 29.0
}

pub fn alignment_row_y(origin: [f32; 2], row: usize) -> f32 {
    let total = 5.0 * 22.0 + 4.0 * 7.0;
    let start_y = ((PANEL_H - total) / 2.0).floor() + 7.0;
    origin[1] + start_y + row as f32 * 29.0
}

pub fn style_checkbox_y(origin: [f32; 2], creative: bool, index: usize) -> f32 {
    let count = if creative { 7.0 } else { 6.0 };
    let start_y = ((PANEL_H - count * 20.0 - (count - 1.0) * 2.0) / 2.0).floor();
    origin[1] + start_y + index as f32 * 22.0
}

pub fn position_row_visible(focus_row: Option<usize>, row: usize) -> bool {
    match focus_row {
        None => true,
        Some(active) if active == row => true,
        // `PositionControlsWidget.alwaysVisible`: visible unless a slider row (0 or 1) is focused.
        Some(active) if row == 2 => active >= 2,
        // `PositionComponentWidget.alwaysVisible`: X/Y/Z stay visible when Move By (2) is focused.
        Some(2) if row >= 3 => true,
        _ => false,
    }
}

/// Clickable tab bar on the left edge of the panel.
pub struct TabBarWidget {
    pub state: SharedState,
}

impl ScreenWidget for TabBarWidget {
    fn bounds(&self) -> [f32; 4] {
        let s = lock(&self.state);
        let n = s.tabs().len();
        let top = s.origin[1] + tabs_start_y(n);
        let h = n as f32 * TAB_STRIDE;
        [s.origin[0] - 32.0, top, 32.0, h]
    }

    fn visible(&self) -> bool {
        lock(&self.state).focus_row.is_none()
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
        let target = {
            let s = lock(&self.state);
            if s.focus_row.is_some() {
                return false;
            }
            hovered_tab(s.origin, s.tabs(), mouse)
        };
        if let Some(tab) = target {
            let mut s = lock(&self.state);
            if s.tab != tab {
                switch_tab(host, &mut s, tab, true);
                return true;
            }
        }
        false
    }
}

/// Top-right `×` close button (`Equipment`, `Style`, `Alignments`).
pub struct CloseButtonWidget {
    pub state: SharedState,
}

impl ScreenWidget for CloseButtonWidget {
    fn bounds(&self) -> [f32; 4] {
        let s = lock(&self.state);
        [s.origin[0] + PANEL_W - 23.0, s.origin[1] + 8.0, 15.0, 15.0]
    }

    fn visible(&self) -> bool {
        let s = lock(&self.state);
        s.focus_row.is_none() && s.tab.has_close_button()
    }

    fn render(
        &mut self,
        _host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        _mouse: [f32; 2],
        hovered: bool,
        focused: bool,
        _partial_tick: f32,
    ) {
        let r = self.bounds();
        draw_button_bg(draw, r, true, hovered || focused);
        let color = if hovered || focused {
            0xFFFFFFA0
        } else {
            0xFFFFFFFF
        };
        draw_centered_str(draw, "×", r[0] + r[2] * 0.5, r[1] + 3.5, color, true);
    }

    fn mouse_clicked(
        &mut self,
        host: &mut dyn ClientHost,
        _mouse: [f32; 2],
        button: i32,
        _modifiers: u32,
        _double_click: bool,
    ) -> bool {
        if button != 0 {
            return false;
        }
        {
            let mut s = lock(&self.state);
            flush_dirty(host, &mut s);
        }
        click_sound(host);
        host.close_screen();
        true
    }
}

/// Vanilla Tweaks credits `?` button at `(6, 6)` on `Poses` and `Alignments`.
pub struct CreditsButtonWidget {
    pub state: SharedState,
}

impl ScreenWidget for CreditsButtonWidget {
    fn bounds(&self) -> [f32; 4] {
        let s = lock(&self.state);
        [s.origin[0] + 6.0, s.origin[1] + 6.0, 20.0, 20.0]
    }

    fn visible(&self) -> bool {
        let s = lock(&self.state);
        s.focus_row.is_none() && matches!(s.tab, Tab::Poses | Tab::Alignments | Tab::VanillaTweaks)
    }

    fn render(
        &mut self,
        _host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        _mouse: [f32; 2],
        hovered: bool,
        focused: bool,
        _partial_tick: f32,
    ) {
        let r = self.bounds();
        draw_button_bg(draw, r, true, hovered || focused);
        let color = if hovered || focused {
            0xFFFFFFA0
        } else {
            0xFFFFFFFF
        };
        draw_centered_str(draw, "?", r[0] + r[2] * 0.5, r[1] + 6.0, color, true);
    }

    fn mouse_clicked(
        &mut self,
        host: &mut dyn ClientHost,
        _mouse: [f32; 2],
        button: i32,
        _modifiers: u32,
        _double_click: bool,
    ) -> bool {
        if button != 0 {
            return false;
        }
        click_sound(host);
        true
    }
}

/// Rotations tab center icon buttons (Lock, Tip `?`, Randomize, Reset, Mirror, Copy, Paste).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RotAction {
    Lock,
    Tip,
    Randomize,
    Reset,
    Mirror,
    Copy,
    Paste,
}

pub struct RotActionButton {
    pub state: SharedState,
    pub action: RotAction,
}

impl RotActionButton {
    fn confirm_id(&self) -> Option<u32> {
        match self.action {
            RotAction::Randomize => Some(CONFIRM_RANDOMIZE),
            RotAction::Reset => Some(CONFIRM_RESET),
            RotAction::Mirror => Some(CONFIRM_MIRROR),
            RotAction::Copy => Some(CONFIRM_COPY),
            RotAction::Paste => Some(CONFIRM_PASTE),
            _ => None,
        }
    }
}

impl ScreenWidget for RotActionButton {
    fn bounds(&self) -> [f32; 4] {
        let o = lock(&self.state).origin;
        match self.action {
            RotAction::Lock => [o[0] + 83.0, o[1] + 10.0, 20.0, 20.0],
            RotAction::Tip => [o[0] + 107.0, o[1] + 10.0, 20.0, 20.0],
            RotAction::Randomize => [o[0] + 83.0, o[1] + 34.0, 20.0, 20.0],
            RotAction::Reset => [o[0] + 107.0, o[1] + 34.0, 20.0, 20.0],
            RotAction::Mirror => [o[0] + 83.0, o[1] + 134.0, 44.0, 20.0],
            RotAction::Copy => [o[0] + 83.0, o[1] + 158.0, 20.0, 20.0],
            RotAction::Paste => [o[0] + 107.0, o[1] + 158.0, 20.0, 20.0],
        }
    }

    fn visible(&self) -> bool {
        lock(&self.state).tab == Tab::Rotations
    }

    fn active(&self) -> bool {
        match self.action {
            RotAction::Tip => false,
            RotAction::Paste => get_clipboard().is_some(),
            _ => true,
        }
    }

    fn render(
        &mut self,
        _host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        _mouse: [f32; 2],
        hovered: bool,
        focused: bool,
        _partial_tick: f32,
    ) {
        let r = self.bounds();
        let active = match self.action {
            RotAction::Tip => true,
            _ => self.active(),
        };
        let confirmed = self
            .confirm_id()
            .is_some_and(|id| confirm_active(&lock(&self.state), id));
        draw_button_bg(draw, r, active, active && (hovered || focused));

        let label = if confirmed {
            "✓"
        } else {
            match self.action {
                RotAction::Lock => {
                    if get_clamp_rotations() {
                        "L"
                    } else {
                        "U"
                    }
                }
                RotAction::Tip => "?",
                RotAction::Randomize => "R",
                RotAction::Reset => "↺",
                RotAction::Mirror => "<->",
                RotAction::Copy => "C",
                RotAction::Paste => "P",
            }
        };
        let color = if !active {
            0xFFA0A0A0
        } else if confirmed {
            0xFF55FF55
        } else if hovered || focused {
            0xFFFFFFA0
        } else {
            0xFFFFFFFF
        };
        draw_centered_str(draw, label, r[0] + r[2] * 0.5, r[1] + 6.0, color, true);
    }

    fn mouse_clicked(
        &mut self,
        host: &mut dyn ClientHost,
        _mouse: [f32; 2],
        button: i32,
        _modifiers: u32,
        _double_click: bool,
    ) -> bool {
        if button != 0 || self.action == RotAction::Tip || !self.active() {
            return false;
        }
        click_sound(host);
        let mut s = lock(&self.state);
        match self.action {
            RotAction::Lock => {
                set_clamp_rotations(!get_clamp_rotations());
            }
            RotAction::Randomize => {
                let pose = random_pose(get_clamp_rotations());
                apply_pose_now(host, &mut s, pose, false);
                set_confirm(&mut s, CONFIRM_RANDOMIZE, CONFIRM_IMG_TICKS);
            }
            RotAction::Reset => {
                apply_pose_now(host, &mut s, DEFAULT_POSE, false);
                set_confirm(&mut s, CONFIRM_RESET, CONFIRM_IMG_TICKS);
            }
            RotAction::Mirror => {
                let pose = mirror_pose(&s.stand.pose);
                apply_pose_now(host, &mut s, pose, true);
                set_confirm(&mut s, CONFIRM_MIRROR, CONFIRM_IMG_TICKS);
            }
            RotAction::Copy => {
                set_clipboard(s.stand.pose);
                set_confirm(&mut s, CONFIRM_COPY, CONFIRM_IMG_TICKS);
            }
            RotAction::Paste => {
                if let Some(pose) = get_clipboard() {
                    apply_pose_now(host, &mut s, pose, false);
                    set_confirm(&mut s, CONFIRM_PASTE, CONFIRM_IMG_TICKS);
                }
            }
            RotAction::Tip => {}
        }
        true
    }
}

/// 2D square slider (horizontal = Y inverted, vertical = X) on the Rotations tab.
pub struct SquareSliderWidget {
    pub state: SharedState,
    pub mutator_idx: usize,
    dragging: bool,
    key_edit: bool,
}

impl SquareSliderWidget {
    pub fn new(state: SharedState, mutator_idx: usize) -> Self {
        Self {
            state,
            mutator_idx,
            dragging: false,
            key_edit: false,
        }
    }

    fn update_from_mouse(&mut self, host: &dyn ClientHost, mouse: [f32; 2]) {
        let r = self.bounds();
        let mods = host.modifiers();
        let lock_h = mods.alt();
        let lock_v = mods.shift();
        let m = MUTATORS[self.mutator_idx];
        let clamp = get_clamp_rotations();
        let mut s = lock(&self.state);
        let mut changed = false;
        if !lock_h {
            let nh = ((f64::from(mouse[0]) - f64::from(r[0] + 8.0)) / f64::from(r[2] - 15.0))
                .clamp(0.0, 1.0);
            m.set_normalized_axis(&mut s.stand.pose, 1, nh, clamp);
            changed = true;
        }
        if !lock_v {
            let nv = ((f64::from(mouse[1]) - f64::from(r[1] + 8.0)) / f64::from(r[3] - 15.0))
                .clamp(0.0, 1.0);
            m.set_normalized_axis(&mut s.stand.pose, 0, nv, clamp);
            changed = true;
        }
        if changed {
            s.dirty_pose = true;
        }
    }
}

impl ScreenWidget for SquareSliderWidget {
    fn bounds(&self) -> [f32; 4] {
        let o = lock(&self.state).origin;
        let col = (self.mutator_idx % 2) as f32;
        let row = (self.mutator_idx / 2) as f32;
        [
            o[0] + 23.0 + col * 110.0,
            o[1] + 7.0 + row * 60.0,
            54.0,
            54.0,
        ]
    }

    fn visible(&self) -> bool {
        lock(&self.state).tab == Tab::Rotations
    }

    fn active(&self) -> bool {
        let m = MUTATORS[self.mutator_idx];
        let s = lock(&self.state);
        !m.is_arm || s.stand.show_arms || s.mode != SyncMode::Network
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
        let r = self.bounds();
        let active = self.active();
        let m = MUTATORS[self.mutator_idx];
        let clamp = get_clamp_rotations();
        let (nh, nv) = {
            let s = lock(&self.state);
            (
                m.normalized_axis(&s.stand.pose, 1, clamp),
                m.normalized_axis(&s.stand.pose, 0, clamp),
            )
        };
        let handle = 13.0_f32;
        let sx = (nh as f32) * (r[2] - handle - 2.0);
        let sy = (nv as f32) * (r[3] - handle - 2.0);

        let mods = host.modifiers();
        let lock_h = mods.alt();
        let lock_v = mods.shift();
        if !active || !(hovered || focused) || (!lock_h && !lock_v) {
            draw_slider_track(draw, r);
        } else if lock_h && lock_v {
            draw_slider_track(draw, [r[0] + sx, r[1] + sy, handle + 2.0, handle + 2.0]);
        } else if lock_h {
            draw_slider_track(draw, [r[0] + sx, r[1], handle + 2.0, r[3]]);
        } else {
            draw_slider_track(draw, [r[0], r[1] + sy, r[2], handle + 2.0]);
        }
        draw_button_bg(
            draw,
            [r[0] + 1.0 + sx, r[1] + 1.0 + sy, handle, handle],
            active,
            active && (hovered || self.key_edit),
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
        if button != 0 || !self.active() {
            return false;
        }
        self.dragging = true;
        self.update_from_mouse(host, mouse);
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
        if button != 0 || !self.dragging || !self.active() {
            return false;
        }
        self.update_from_mouse(host, mouse);
        true
    }

    fn mouse_released(
        &mut self,
        host: &mut dyn ClientHost,
        _mouse: [f32; 2],
        button: i32,
        _modifiers: u32,
    ) -> bool {
        if button != 0 || !self.dragging {
            return false;
        }
        self.dragging = false;
        let mut s = lock(&self.state);
        flush_dirty(host, &mut s);
        true
    }

    fn key_pressed(
        &mut self,
        host: &mut dyn ClientHost,
        key: &str,
        _repeat: bool,
        _modifiers: u32,
    ) -> bool {
        if !self.active() {
            return false;
        }
        if matches!(
            key,
            "key.keyboard.enter" | "key.keyboard.keypad.enter" | "key.keyboard.space"
        ) {
            self.key_edit = !self.key_edit;
            return true;
        }
        if !self.key_edit {
            return false;
        }
        let m = MUTATORS[self.mutator_idx];
        let clamp = get_clamp_rotations();
        let mut s = lock(&self.state);
        let handled = match key {
            "key.keyboard.left" => {
                let v = (m.normalized_axis(&s.stand.pose, 1, clamp) - SQUARE_SLIDER_KEY_STEP)
                    .clamp(0.0, 1.0);
                m.set_normalized_axis(&mut s.stand.pose, 1, v, clamp);
                true
            }
            "key.keyboard.right" => {
                let v = (m.normalized_axis(&s.stand.pose, 1, clamp) + SQUARE_SLIDER_KEY_STEP)
                    .clamp(0.0, 1.0);
                m.set_normalized_axis(&mut s.stand.pose, 1, v, clamp);
                true
            }
            "key.keyboard.up" => {
                let v = (m.normalized_axis(&s.stand.pose, 0, clamp) - SQUARE_SLIDER_KEY_STEP)
                    .clamp(0.0, 1.0);
                m.set_normalized_axis(&mut s.stand.pose, 0, v, clamp);
                true
            }
            "key.keyboard.down" => {
                let v = (m.normalized_axis(&s.stand.pose, 0, clamp) + SQUARE_SLIDER_KEY_STEP)
                    .clamp(0.0, 1.0);
                m.set_normalized_axis(&mut s.stand.pose, 0, v, clamp);
                true
            }
            _ => false,
        };
        if handled {
            let pose = s.stand.pose;
            apply_pose_now(host, &mut s, pose, false);
        }
        handled
    }

    fn set_focused(&mut self, focused: bool) {
        if !focused {
            self.key_edit = false;
        }
    }
}

/// Vertical 1D Z-axis slider beside each square slider on the Rotations tab.
pub struct ZSliderWidget {
    pub state: SharedState,
    pub mutator_idx: usize,
    dragging: bool,
    key_edit: bool,
}

impl ZSliderWidget {
    pub fn new(state: SharedState, mutator_idx: usize) -> Self {
        Self {
            state,
            mutator_idx,
            dragging: false,
            key_edit: false,
        }
    }

    fn update_from_mouse(&mut self, mouse: [f32; 2]) {
        let r = self.bounds();
        let nv = ((f64::from(mouse[1]) - f64::from(r[1] + 8.0)) / f64::from(r[3] - 15.0))
            .clamp(0.0, 1.0);
        let m = MUTATORS[self.mutator_idx];
        let clamp = get_clamp_rotations();
        let mut s = lock(&self.state);
        m.set_normalized_axis(&mut s.stand.pose, 2, nv, clamp);
        s.dirty_pose = true;
    }
}

impl ScreenWidget for ZSliderWidget {
    fn bounds(&self) -> [f32; 4] {
        let o = lock(&self.state).origin;
        let col = (self.mutator_idx % 2) as f32;
        let row = (self.mutator_idx / 2) as f32;
        [
            o[0] + 6.0 + col * 183.0,
            o[1] + 7.0 + row * 60.0,
            15.0,
            54.0,
        ]
    }

    fn visible(&self) -> bool {
        lock(&self.state).tab == Tab::Rotations
    }

    fn active(&self) -> bool {
        let m = MUTATORS[self.mutator_idx];
        let s = lock(&self.state);
        !m.is_arm || s.stand.show_arms || s.mode != SyncMode::Network
    }

    fn render(
        &mut self,
        _host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        _mouse: [f32; 2],
        hovered: bool,
        _focused: bool,
        _partial_tick: f32,
    ) {
        let r = self.bounds();
        let active = self.active();
        let m = MUTATORS[self.mutator_idx];
        let nv = m.normalized_axis(&lock(&self.state).stand.pose, 2, get_clamp_rotations());
        let handle = 13.0_f32;
        let sy = (nv as f32) * (r[3] - handle - 2.0);
        draw_slider_track(draw, r);
        draw_button_bg(
            draw,
            [r[0] + 1.0, r[1] + 1.0 + sy, handle, handle],
            active,
            active && (hovered || self.key_edit),
        );
    }

    fn mouse_clicked(
        &mut self,
        _host: &mut dyn ClientHost,
        mouse: [f32; 2],
        button: i32,
        _modifiers: u32,
        _double_click: bool,
    ) -> bool {
        if button != 0 || !self.active() {
            return false;
        }
        self.dragging = true;
        self.update_from_mouse(mouse);
        true
    }

    fn mouse_dragged(
        &mut self,
        _host: &mut dyn ClientHost,
        mouse: [f32; 2],
        button: i32,
        _delta: [f32; 2],
        _modifiers: u32,
    ) -> bool {
        if button != 0 || !self.dragging || !self.active() {
            return false;
        }
        self.update_from_mouse(mouse);
        true
    }

    fn mouse_released(
        &mut self,
        host: &mut dyn ClientHost,
        _mouse: [f32; 2],
        button: i32,
        _modifiers: u32,
    ) -> bool {
        if button != 0 || !self.dragging {
            return false;
        }
        self.dragging = false;
        let mut s = lock(&self.state);
        flush_dirty(host, &mut s);
        true
    }

    fn key_pressed(
        &mut self,
        host: &mut dyn ClientHost,
        key: &str,
        _repeat: bool,
        _modifiers: u32,
    ) -> bool {
        if !self.active() {
            return false;
        }
        if matches!(
            key,
            "key.keyboard.enter" | "key.keyboard.keypad.enter" | "key.keyboard.space"
        ) {
            self.key_edit = !self.key_edit;
            return true;
        }
        if !self.key_edit {
            return false;
        }
        let m = MUTATORS[self.mutator_idx];
        let clamp = get_clamp_rotations();
        let mut s = lock(&self.state);
        let delta = match key {
            "key.keyboard.up" => -SQUARE_SLIDER_KEY_STEP,
            "key.keyboard.down" => SQUARE_SLIDER_KEY_STEP,
            _ => return false,
        };
        let v = (m.normalized_axis(&s.stand.pose, 2, clamp) + delta).clamp(0.0, 1.0);
        m.set_normalized_axis(&mut s.stand.pose, 2, v, clamp);
        let pose = s.stand.pose;
        apply_pose_now(host, &mut s, pose, false);
        true
    }

    fn set_focused(&mut self, focused: bool) {
        if !focused {
            self.key_edit = false;
        }
    }
}

/// Poses tab pagination arrow buttons (`prev = true` left, `prev = false` right).
pub struct PosePageButton {
    pub state: SharedState,
    pub prev: bool,
}

impl ScreenWidget for PosePageButton {
    fn bounds(&self) -> [f32; 4] {
        let o = lock(&self.state).origin;
        let x = if self.prev { 17.0 } else { 49.0 };
        [o[0] + x, o[1] + 153.0, 20.0, 20.0]
    }

    fn visible(&self) -> bool {
        lock(&self.state).tab == Tab::Poses
    }

    fn active(&self) -> bool {
        let idx = get_first_pose_index();
        if self.prev {
            idx >= 4
        } else {
            idx + 4 < POSES.len()
        }
    }

    fn render(
        &mut self,
        _host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        _mouse: [f32; 2],
        hovered: bool,
        focused: bool,
        _partial_tick: f32,
    ) {
        let r = self.bounds();
        let active = self.active();
        draw_button_bg(draw, r, active, active && (hovered || focused));
        let label = if self.prev { "<" } else { ">" };
        let color = if !active {
            0xFFA0A0A0
        } else if hovered || focused {
            0xFFFFFFA0
        } else {
            0xFFFFFFFF
        };
        draw_centered_str(draw, label, r[0] + r[2] * 0.5, r[1] + 6.0, color, true);
    }

    fn mouse_clicked(
        &mut self,
        host: &mut dyn ClientHost,
        _mouse: [f32; 2],
        button: i32,
        _modifiers: u32,
        _double_click: bool,
    ) -> bool {
        if button != 0 || !self.active() {
            return false;
        }
        let idx = get_first_pose_index();
        let next = if self.prev {
            idx.saturating_sub(4)
        } else {
            idx + 4
        };
        if next < POSES.len() {
            set_first_pose_index(next);
            click_sound(host);
            return true;
        }
        false
    }
}

/// One of the 4 pose preset cards (`60x82`) on the Poses tab.
pub struct PoseCardButton {
    pub state: SharedState,
    pub card_idx: usize,
}

impl ScreenWidget for PoseCardButton {
    fn bounds(&self) -> [f32; 4] {
        let o = lock(&self.state).origin;
        let col = (self.card_idx % 2) as f32;
        let row = (self.card_idx / 2) as f32;
        [
            o[0] + 83.0 + col * 62.0,
            o[1] + 9.0 + row * 88.0,
            60.0,
            82.0,
        ]
    }

    fn visible(&self) -> bool {
        lock(&self.state).tab == Tab::Poses
            && (get_first_pose_index() + self.card_idx) < POSES.len()
    }

    fn render(
        &mut self,
        _host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        _mouse: [f32; 2],
        hovered: bool,
        focused: bool,
        _partial_tick: f32,
    ) {
        let r = self.bounds();
        // Subtle highlight border when hovered/focused; the entity preview itself is drawn by MenuScreen::with_draw.
        if hovered || focused {
            draw.fill([r[0], r[1], r[2], 1.0], 0xFFFFFFFF);
            draw.fill([r[0], r[1] + r[3] - 1.0, r[2], 1.0], 0xFFFFFFFF);
            draw.fill([r[0], r[1], 1.0, r[3]], 0xFFFFFFFF);
            draw.fill([r[0] + r[2] - 1.0, r[1], 1.0, r[3]], 0xFFFFFFFF);
        }
    }

    fn mouse_clicked(
        &mut self,
        host: &mut dyn ClientHost,
        _mouse: [f32; 2],
        button: i32,
        _modifiers: u32,
        _double_click: bool,
    ) -> bool {
        if button != 0 {
            return false;
        }
        let pose_idx = get_first_pose_index() + self.card_idx;
        if let Some(preset) = POSES.get(pose_idx) {
            let mut s = lock(&self.state);
            apply_pose_now(host, &mut s, preset.pose, false);
            click_sound(host);
            return true;
        }
        false
    }
}

/// Borderless custom name EditBox on the Style tab (`(16, 32, 73, 9)`, max 50 UTF-16 units).
pub struct NameBoxWidget {
    pub state: SharedState,
}

impl ScreenWidget for NameBoxWidget {
    fn bounds(&self) -> [f32; 4] {
        let o = lock(&self.state).origin;
        [o[0] + 14.0, o[1] + 30.0, 76.0, 12.0]
    }

    fn visible(&self) -> bool {
        lock(&self.state).tab == Tab::Style
    }

    fn captures_input(&self) -> bool {
        let s = lock(&self.state);
        s.tab == Tab::Style && s.name_focused
    }

    fn set_focused(&mut self, focused: bool) {
        lock(&self.state).name_focused = focused;
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
        let r = self.bounds();
        let (value, cursor, highlight, is_focused) = {
            let mut s = lock(&self.state);
            s.name_focused = focused;
            (
                s.name_edit.value.clone(),
                s.name_edit.cursor,
                s.name_edit.highlight,
                focused,
            )
        };
        let tx = r[0] + 2.0;
        let ty = r[1] + 2.0;
        let inner_w = r[2] - 4.0;
        let clip = [tx, r[1] + 1.0, inner_w, r[3] - 2.0];
        let _ = draw.with_scissor(clip, &mut |draw| {
            if value.is_empty() && !is_focused {
                let hint = tr(host, "screen.statuemenus.style.hint", "Custom Name");
                draw.text(&hint, [tx, ty], 0xFF808080, false);
            } else {
                // Scroll horizontally so cursor stays inside box.
                let before_cursor: String = value.chars().take(cursor).collect();
                let cursor_px = draw.text_width(&before_cursor);
                let scroll = (cursor_px - (inner_w - 4.0)).max(0.0);
                if cursor != highlight {
                    let (a, b) = (cursor.min(highlight), cursor.max(highlight));
                    let sa: String = value.chars().take(a).collect();
                    let sb: String = value.chars().take(b).collect();
                    let xa = tx + draw.text_width(&sa) - scroll;
                    let xb = tx + draw.text_width(&sb) - scroll;
                    draw.fill([xa, ty - 1.0, (xb - xa).max(1.0), 10.0], 0x803366CC);
                }
                draw.text(&value, [tx - scroll, ty], 0xFFFFFFFF, true);
                if is_focused && (host.millis() / 300).is_multiple_of(2) {
                    let cx = tx + cursor_px - scroll;
                    draw.fill([cx, ty - 1.0, 1.0, 10.0], 0xFFD0D0D0);
                }
            }
        });
    }

    fn mouse_clicked(
        &mut self,
        _host: &mut dyn ClientHost,
        _mouse: [f32; 2],
        button: i32,
        _modifiers: u32,
        _double_click: bool,
    ) -> bool {
        if button != 0 {
            return false;
        }
        let mut s = lock(&self.state);
        s.name_focused = true;
        s.name_edit.move_cursor_to_end(false);
        true
    }

    fn key_pressed(
        &mut self,
        host: &mut dyn ClientHost,
        key: &str,
        _repeat: bool,
        modifiers: u32,
    ) -> bool {
        let mut s = lock(&self.state);
        if !s.name_focused {
            return false;
        }
        let shift = modifiers & 1 != 0;
        let ctrl = modifiers & 2 != 0;
        let before = s.name_edit.value.clone();
        if ctrl && key == "key.keyboard.c" {
            let sel = s.name_edit.selected_text();
            if !sel.is_empty() {
                let _ = host.set_clipboard_text(&sel);
            }
            return true;
        }
        if ctrl && key == "key.keyboard.x" {
            let sel = s.name_edit.selected_text();
            if !sel.is_empty() {
                let _ = host.set_clipboard_text(&sel);
                s.name_edit.insert_text("");
                s.name_dirty_ticks = 20;
            }
            return true;
        }
        if ctrl && key == "key.keyboard.v" {
            if let Some(clip) = host.clipboard_text() {
                s.name_edit.insert_text(&clip);
                if s.name_edit.value != before {
                    s.name_dirty_ticks = 20;
                }
            }
            return true;
        }
        let edit_key = match key {
            "key.keyboard.backspace" => Some(EditKey::Backspace),
            "key.keyboard.delete" => Some(EditKey::Delete),
            "key.keyboard.left" => Some(EditKey::Left),
            "key.keyboard.right" => Some(EditKey::Right),
            "key.keyboard.home" => Some(EditKey::Home),
            "key.keyboard.end" => Some(EditKey::End),
            "key.keyboard.a" if ctrl => Some(EditKey::SelectAll),
            "key.keyboard.enter" | "key.keyboard.keypad.enter" => {
                s.name_dirty_ticks = 0;
                flush_dirty(host, &mut s);
                return true;
            }
            _ => None,
        };
        if let Some(ek) = edit_key {
            s.name_edit.key(ek, ctrl, shift);
            if s.name_edit.value != before {
                s.name_dirty_ticks = 20;
            }
            return true;
        }
        if key == "key.keyboard.escape" {
            s.name_dirty_ticks = 0;
            flush_dirty(host, &mut s);
            return false;
        }
        // Consume non-Escape keys while focused so inventory hotkeys don't fire.
        true
    }

    fn char_typed(&mut self, _host: &mut dyn ClientHost, c: char, _modifiers: u32) -> bool {
        let mut s = lock(&self.state);
        if !s.name_focused {
            return false;
        }
        let before = s.name_edit.value.clone();
        let mut buf = [0u8; 4];
        s.name_edit.insert_text(c.encode_utf8(&mut buf));
        if s.name_edit.value != before {
            s.name_dirty_ticks = 20;
        }
        true
    }
}

/// Style tab checkbox button (`(96, startY + i * 22, 102, 20)`).
pub struct StyleCheckboxWidget {
    pub state: SharedState,
    pub index: usize,
    pub option: StyleOption,
}

impl ScreenWidget for StyleCheckboxWidget {
    fn bounds(&self) -> [f32; 4] {
        let s = lock(&self.state);
        [
            s.origin[0] + 96.0,
            style_checkbox_y(s.origin, s.stand.creative, self.index),
            102.0,
            20.0,
        ]
    }

    fn visible(&self) -> bool {
        let s = lock(&self.state);
        s.tab == Tab::Style && (!self.option.creative_only() || s.stand.creative)
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
        let r = self.bounds();
        let ticked = self.option.get(&lock(&self.state).stand);
        draw_checkbox_bg(
            draw,
            [r[0] + 2.0, r[1] + 2.0, 16.0, 16.0],
            ticked,
            hovered || focused,
        );
        let label = tr(host, self.option.translation_key(), "");
        let color = if hovered || focused {
            0xFFFFFFA0
        } else {
            0xFFFFFFFF
        };
        draw.text(&label, [r[0] + 26.0, r[1] + 6.0], color, true);
    }

    fn mouse_clicked(
        &mut self,
        host: &mut dyn ClientHost,
        _mouse: [f32; 2],
        button: i32,
        _modifiers: u32,
        _double_click: bool,
    ) -> bool {
        if button != 0 {
            return false;
        }
        let mut s = lock(&self.state);
        clear_name_focus(&mut s);
        let prev = s.stand.clone();
        let next = !self.option.get(&s.stand);
        self.option.set(&mut s.stand, next);
        if !flush_and_send_edit(
            host,
            &mut s,
            EditMessage::Style {
                option: self.option,
                value: next,
            },
        ) {
            s.stand = prev;
        }
        click_sound(host);
        true
    }
}

/// Horizontal slider on the Position tab (`Scale` row 0, `Rotation` row 1).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PosSliderKind {
    Scale,
    Rotation,
}

pub struct PositionSliderWidget {
    pub state: SharedState,
    pub kind: PosSliderKind,
    dragging: bool,
    key_edit: bool,
}

impl PositionSliderWidget {
    pub fn new(state: SharedState, kind: PosSliderKind) -> Self {
        Self {
            state,
            kind,
            dragging: false,
            key_edit: false,
        }
    }

    fn row(&self) -> usize {
        match self.kind {
            PosSliderKind::Scale => 0,
            PosSliderKind::Rotation => 1,
        }
    }

    fn normalized(&self) -> f64 {
        let s = lock(&self.state);
        match self.kind {
            PosSliderKind::Scale => from_log_scale(f64::from(s.stand.scale)).clamp(0.0, 1.0),
            PosSliderKind::Rotation => from_wrapped_degrees(f64::from(s.stand.yaw)).clamp(0.0, 1.0),
        }
    }

    fn apply_normalized(&mut self, norm: f64, send_now: bool, host: &mut dyn ClientHost) {
        let norm = norm.clamp(0.0, 1.0);
        let norm = match self.kind {
            PosSliderKind::Scale => norm,
            PosSliderKind::Rotation => snap_value(norm, DEGREES_SNAP_INTERVAL),
        };
        let mut s = lock(&self.state);
        match self.kind {
            PosSliderKind::Scale => {
                s.stand.scale = to_log_scale(norm).clamp(MIN_SCALE, MAX_SCALE);
                s.dirty_scale = true;
            }
            PosSliderKind::Rotation => {
                s.stand.yaw = to_wrapped_degrees(norm);
                s.dirty_rotation = true;
            }
        }
        if send_now {
            flush_dirty(host, &mut s);
        }
    }

    fn update_from_mouse(&mut self, host: &mut dyn ClientHost, mouse: [f32; 2]) {
        let r = self.bounds();
        let norm = (f64::from(mouse[0]) - f64::from(r[0] + 4.0)) / f64::from(r[2] - 8.0);
        self.apply_normalized(norm, false, host);
    }
}

impl ScreenWidget for PositionSliderWidget {
    fn bounds(&self) -> [f32; 4] {
        let o = lock(&self.state).origin;
        [
            o[0] + 8.0 + 76.0,
            position_row_y(o, self.row()) + 1.0,
            90.0,
            20.0,
        ]
    }

    fn visible(&self) -> bool {
        let s = lock(&self.state);
        s.tab == Tab::Position && position_row_visible(s.focus_row, self.row())
    }

    fn render(
        &mut self,
        _host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        _mouse: [f32; 2],
        hovered: bool,
        _focused: bool,
        _partial_tick: f32,
    ) {
        let r = self.bounds();
        let norm = self.normalized() as f32;
        draw_button_bg(draw, r, false, false);
        let handle_x = r[0] + norm * (r[2] - 8.0);
        draw_button_bg(
            draw,
            [handle_x, r[1], 8.0, r[3]],
            true,
            hovered || self.key_edit,
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
        self.update_from_mouse(host, mouse);
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
        self.update_from_mouse(host, mouse);
        true
    }

    fn mouse_released(
        &mut self,
        host: &mut dyn ClientHost,
        _mouse: [f32; 2],
        button: i32,
        _modifiers: u32,
    ) -> bool {
        if button != 0 || !self.dragging {
            return false;
        }
        self.dragging = false;
        let mut s = lock(&self.state);
        flush_dirty(host, &mut s);
        true
    }

    fn key_pressed(
        &mut self,
        host: &mut dyn ClientHost,
        key: &str,
        _repeat: bool,
        _modifiers: u32,
    ) -> bool {
        if matches!(
            key,
            "key.keyboard.enter" | "key.keyboard.keypad.enter" | "key.keyboard.space"
        ) {
            self.key_edit = !self.key_edit;
            return true;
        }
        if !self.key_edit {
            return false;
        }
        let step = 1.0 / 90.0;
        let cur = self.normalized();
        match key {
            "key.keyboard.left" => {
                self.apply_normalized(cur - step, true, host);
                true
            }
            "key.keyboard.right" => {
                self.apply_normalized(cur + step, true, host);
                true
            }
            _ => false,
        }
    }

    fn set_focused(&mut self, focused: bool) {
        if !focused {
            self.key_edit = false;
        }
    }
}

/// Focus magnifier button (`+174`) on each Position row (hidden on row 0 while Alt is held).
pub struct FocusRowButton {
    pub state: SharedState,
    pub row: usize,
    pub alt_hides: bool,
}

impl ScreenWidget for FocusRowButton {
    fn bounds(&self) -> [f32; 4] {
        let o = lock(&self.state).origin;
        [
            o[0] + 8.0 + 174.0,
            position_row_y(o, self.row) + 1.0,
            20.0,
            20.0,
        ]
    }

    fn visible(&self) -> bool {
        let s = lock(&self.state);
        s.tab == Tab::Position
            && position_row_visible(s.focus_row, self.row)
            && !(self.alt_hides && s.alt_held)
    }

    fn render(
        &mut self,
        _host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        _mouse: [f32; 2],
        hovered: bool,
        focused: bool,
        _partial_tick: f32,
    ) {
        let r = self.bounds();
        let active_focus = lock(&self.state).focus_row == Some(self.row);
        draw_button_bg(draw, r, true, hovered || focused || active_focus);
        let color = if active_focus {
            0xFF55FFFF
        } else if hovered || focused {
            0xFFFFFFA0
        } else {
            0xFFFFFFFF
        };
        draw_centered_str(draw, "O", r[0] + r[2] * 0.5, r[1] + 6.0, color, true);
    }

    fn mouse_clicked(
        &mut self,
        host: &mut dyn ClientHost,
        _mouse: [f32; 2],
        button: i32,
        _modifiers: u32,
        _double_click: bool,
    ) -> bool {
        if button != 0 {
            return false;
        }
        let mut s = lock(&self.state);
        s.focus_row = if s.focus_row == Some(self.row) {
            None
        } else {
            Some(self.row)
        };
        click_sound(host);
        true
    }
}

/// Scale reset button on Position row 0, shown while Alt is held (replaces Focus button).
pub struct ScaleResetButton {
    pub state: SharedState,
}

impl ScreenWidget for ScaleResetButton {
    fn bounds(&self) -> [f32; 4] {
        let o = lock(&self.state).origin;
        [o[0] + 8.0 + 174.0, position_row_y(o, 0) + 1.0, 20.0, 20.0]
    }

    fn visible(&self) -> bool {
        let s = lock(&self.state);
        s.tab == Tab::Position && position_row_visible(s.focus_row, 0) && s.alt_held
    }

    fn render(
        &mut self,
        _host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        _mouse: [f32; 2],
        hovered: bool,
        focused: bool,
        _partial_tick: f32,
    ) {
        let r = self.bounds();
        let confirmed = confirm_active(&lock(&self.state), CONFIRM_SCALE_RESET);
        draw_button_bg(draw, r, true, hovered || focused);
        let label = if confirmed { "✓" } else { "↺" };
        let color = if confirmed {
            0xFF55FF55
        } else if hovered || focused {
            0xFFFFFFA0
        } else {
            0xFFFFFFFF
        };
        draw_centered_str(draw, label, r[0] + r[2] * 0.5, r[1] + 6.0, color, true);
    }

    fn mouse_clicked(
        &mut self,
        host: &mut dyn ClientHost,
        _mouse: [f32; 2],
        button: i32,
        _modifiers: u32,
        _double_click: bool,
    ) -> bool {
        if button != 0 {
            return false;
        }
        let mut s = lock(&self.state);
        s.stand.scale = DEFAULT_SCALE;
        s.dirty_scale = false;
        send_edit(host, &mut s, &EditMessage::Scale(DEFAULT_SCALE));
        set_confirm(&mut s, CONFIRM_SCALE_RESET, CONFIRM_IMG_TICKS);
        click_sound(host);
        true
    }
}

/// Move By increment button (`1`, `4`, `8`, `16` pixels) on Position row 2.
pub struct IncrementButton {
    pub state: SharedState,
    pub index: usize,
}

impl ScreenWidget for IncrementButton {
    fn bounds(&self) -> [f32; 4] {
        let o = lock(&self.state).origin;
        let extra = if self.index > 1 { 1.0 } else { 0.0 };
        [
            o[0] + 8.0 + 76.0 + self.index as f32 * 24.0 + extra,
            position_row_y(o, 2) + 1.0,
            20.0,
            20.0,
        ]
    }

    fn visible(&self) -> bool {
        let s = lock(&self.state);
        s.tab == Tab::Position && position_row_visible(s.focus_row, 2)
    }

    fn active(&self) -> bool {
        get_move_increment_index() != self.index
    }

    fn render(
        &mut self,
        _host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        _mouse: [f32; 2],
        hovered: bool,
        focused: bool,
        _partial_tick: f32,
    ) {
        let r = self.bounds();
        let active = self.active();
        draw_button_bg(draw, r, active, active && (hovered || focused));
        let px = (MOVE_INCREMENTS[self.index] * 16.0).round() as i32;
        let label = px.to_string();
        let color = if !active {
            0xFFA0A0A0
        } else if hovered || focused {
            0xFFFFFFA0
        } else {
            0xFFFFFFFF
        };
        draw_centered_str(draw, &label, r[0] + r[2] * 0.5, r[1] + 6.0, color, true);
    }

    fn mouse_clicked(
        &mut self,
        host: &mut dyn ClientHost,
        _mouse: [f32; 2],
        button: i32,
        _modifiers: u32,
        _double_click: bool,
    ) -> bool {
        if button != 0 || !self.active() {
            return false;
        }
        set_move_increment_index(self.index);
        click_sound(host);
        true
    }
}

/// Read-only coordinate display box (`66x22` at `+77`) on Position rows 3..6.
pub struct PositionReadoutWidget {
    pub state: SharedState,
    pub axis: usize,
}

impl ScreenWidget for PositionReadoutWidget {
    fn bounds(&self) -> [f32; 4] {
        let o = lock(&self.state).origin;
        [
            o[0] + 8.0 + 77.0,
            position_row_y(o, 3 + self.axis),
            66.0,
            22.0,
        ]
    }

    fn visible(&self) -> bool {
        let s = lock(&self.state);
        s.tab == Tab::Position && position_row_visible(s.focus_row, 3 + self.axis)
    }

    fn active(&self) -> bool {
        false
    }

    fn render(
        &mut self,
        _host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        _mouse: [f32; 2],
        _hovered: bool,
        _focused: bool,
        _partial_tick: f32,
    ) {
        let r = self.bounds();
        draw.gui_nine_slice("widget/text_field", r, 0xFFFFFFFF);
        let val = round_sixteenth(lock(&self.state).stand.pos[self.axis]);
        let txt = format_four_decimals(val);
        draw.text(&txt, [r[0] + 4.0, r[1] + 7.0], 0xFFE0E0E0, true);
    }
}

/// Up / Down nudge button (`20x10` at `+149`) on Position rows 3..6.
pub struct PositionNudgeButton {
    pub state: SharedState,
    pub axis: usize,
    pub up: bool,
}

impl ScreenWidget for PositionNudgeButton {
    fn bounds(&self) -> [f32; 4] {
        let o = lock(&self.state).origin;
        let y_off = if self.up { 1.0 } else { 11.0 };
        [
            o[0] + 8.0 + 149.0,
            position_row_y(o, 3 + self.axis) + y_off,
            20.0,
            10.0,
        ]
    }

    fn visible(&self) -> bool {
        let s = lock(&self.state);
        s.tab == Tab::Position && position_row_visible(s.focus_row, 3 + self.axis)
    }

    fn render(
        &mut self,
        _host: &dyn ClientHost,
        draw: &mut dyn DrawContext,
        _mouse: [f32; 2],
        hovered: bool,
        focused: bool,
        _partial_tick: f32,
    ) {
        let r = self.bounds();
        draw_button_bg(draw, r, true, hovered || focused);
        let sym = if self.up { "▲" } else { "▼" };
        let color = if hovered || focused {
            0xFFFFFFA0
        } else {
            0xFFFFFFFF
        };
        draw_centered_str(draw, sym, r[0] + r[2] * 0.5, r[1] + 1.0, color, false);
    }

    fn mouse_clicked(
        &mut self,
        host: &mut dyn ClientHost,
        _mouse: [f32; 2],
        button: i32,
        _modifiers: u32,
        _double_click: bool,
    ) -> bool {
        if button != 0 {
            return false;
        }
        let inc = MOVE_INCREMENTS[get_move_increment_index()];
        let delta = if self.up { inc } else { -inc };
        let mut s = lock(&self.state);
        let current = round_sixteenth(s.stand.pos[self.axis]);
        let next = round_sixteenth(current + delta);
        if (next - current).abs() > 1e-9 {
            s.stand.pos[self.axis] = next;
            let pos = s.stand.pos;
            send_edit(host, &mut s, &EditMessage::Position(pos));
        }
        click_sound(host);
        true
    }
}

/// Alignments tab buttons (Centered, Cornered, and the 4 surface alignments).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AlignAction {
    Centered,
    Cornered,
    Surface(Alignment),
}

pub struct AlignmentButton {
    pub state: SharedState,
    pub action: AlignAction,
}

impl AlignmentButton {
    fn confirm_id(&self) -> u32 {
        match self.action {
            AlignAction::Centered => CONFIRM_CENTERED,
            AlignAction::Cornered => CONFIRM_CORNERED,
            AlignAction::Surface(a) => {
                CONFIRM_ALIGN_BASE + Alignment::ALL.iter().position(|&x| x == a).unwrap_or(0) as u32
            }
        }
    }

    fn label_key(&self) -> &'static str {
        match self.action {
            AlignAction::Centered => "screen.statuemenus.position.centered",
            AlignAction::Cornered => "screen.statuemenus.position.cornered",
            AlignAction::Surface(a) => a.translation_key(),
        }
    }
}

impl ScreenWidget for AlignmentButton {
    fn bounds(&self) -> [f32; 4] {
        let o = lock(&self.state).origin;
        match self.action {
            AlignAction::Centered => [o[0] + 8.0, alignment_row_y(o, 0) + 1.0, 94.0, 20.0],
            AlignAction::Cornered => [o[0] + 8.0 + 100.0, alignment_row_y(o, 0) + 1.0, 94.0, 20.0],
            AlignAction::Surface(a) => {
                let idx = Alignment::ALL.iter().position(|&x| x == a).unwrap_or(0);
                [o[0] + 8.0, alignment_row_y(o, 1 + idx) + 1.0, 194.0, 20.0]
            }
        }
    }

    fn visible(&self) -> bool {
        lock(&self.state).tab == Tab::Alignments
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
        let r = self.bounds();
        let confirmed = confirm_active(&lock(&self.state), self.confirm_id());
        draw_button_bg(draw, r, true, hovered || focused);
        let label = if confirmed {
            tr(host, "screen.statuemenus.position.aligned", "Aligned!")
        } else {
            tr(host, self.label_key(), "")
        };
        let color = if confirmed {
            0xFF55FF55
        } else if hovered || focused {
            0xFFFFFFA0
        } else {
            0xFFFFFFFF
        };
        draw_centered_str(draw, &label, r[0] + r[2] * 0.5, r[1] + 6.0, color, true);
    }

    fn mouse_clicked(
        &mut self,
        host: &mut dyn ClientHost,
        _mouse: [f32; 2],
        button: i32,
        _modifiers: u32,
        _double_click: bool,
    ) -> bool {
        if button != 0 {
            return false;
        }
        let mut s = lock(&self.state);
        match self.action {
            AlignAction::Centered => {
                let pos = [
                    s.stand.pos[0].floor() + 0.5,
                    s.stand.pos[1].floor(),
                    s.stand.pos[2].floor() + 0.5,
                ];
                s.stand.pos = pos;
                send_edit(host, &mut s, &EditMessage::Position(pos));
            }
            AlignAction::Cornered => {
                let pos = [
                    s.stand.pos[0].floor(),
                    s.stand.pos[1].floor(),
                    s.stand.pos[2].floor(),
                ];
                s.stand.pos = pos;
                send_edit(host, &mut s, &EditMessage::Position(pos));
            }
            AlignAction::Surface(a) => {
                apply_alignment(host, &mut s, a);
            }
        }
        set_confirm(&mut s, self.confirm_id(), CONFIRM_TXT_TICKS);
        click_sound(host);
        true
    }
}

/// Vanilla Tweaks tab buttons (`ArmorStandVanillaTweaksScreen`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VanillaTweaksAction {
    CheckTarget,
    Lock,
    Unlock,
    ToolRack,
    SwapHands,
    SwapHead,
}

impl VanillaTweaksAction {
    pub const ALL: [Self; 6] = [
        Self::CheckTarget,
        Self::Lock,
        Self::Unlock,
        Self::ToolRack,
        Self::SwapHands,
        Self::SwapHead,
    ];

    pub fn trigger_id(self) -> i32 {
        match self {
            Self::CheckTarget => VT_CHECK_TARGET,
            Self::Lock => VT_LOCK,
            Self::Unlock => VT_UNLOCK,
            Self::ToolRack => VT_TOOL_RACK,
            Self::SwapHands => VT_SWAP_MAINHAND_OFFHAND,
            Self::SwapHead => VT_SWAP_MAINHAND_HEAD,
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::CheckTarget => "screen.armorstatues.vanilla_tweaks.check_target",
            Self::Lock => "screen.armorstatues.vanilla_tweaks.lock",
            Self::Unlock => "screen.armorstatues.vanilla_tweaks.unlock",
            Self::ToolRack => "screen.armorstatues.vanilla_tweaks.tool_rack",
            Self::SwapHands => "screen.armorstatues.vanilla_tweaks.swap_hands",
            Self::SwapHead => "screen.armorstatues.vanilla_tweaks.swap_head",
        }
    }

    pub fn description_key(self) -> &'static str {
        match self {
            Self::CheckTarget => "screen.armorstatues.vanilla_tweaks.check_target.description",
            Self::Lock => "screen.armorstatues.vanilla_tweaks.lock.description",
            Self::Unlock => "screen.armorstatues.vanilla_tweaks.unlock.description",
            Self::ToolRack => "screen.armorstatues.vanilla_tweaks.tool_rack.description",
            Self::SwapHands => "screen.armorstatues.vanilla_tweaks.swap_hands.description",
            Self::SwapHead => "screen.armorstatues.vanilla_tweaks.swap_head.description",
        }
    }

    pub fn rect(self, o: [f32; 2]) -> [f32; 4] {
        match self {
            Self::CheckTarget => [o[0] + 8.0, alignment_row_y(o, 0) + 1.0, 194.0, 20.0],
            Self::Lock => [o[0] + 8.0, alignment_row_y(o, 1) + 1.0, 94.0, 20.0],
            Self::Unlock => [o[0] + 8.0 + 100.0, alignment_row_y(o, 1) + 1.0, 94.0, 20.0],
            Self::ToolRack => [o[0] + 8.0, alignment_row_y(o, 2) + 1.0, 194.0, 20.0],
            Self::SwapHands => [o[0] + 8.0, alignment_row_y(o, 3) + 1.0, 194.0, 20.0],
            Self::SwapHead => [o[0] + 8.0, alignment_row_y(o, 4) + 1.0, 194.0, 20.0],
        }
    }
}

pub struct VanillaTweaksButton {
    pub state: SharedState,
    pub action: VanillaTweaksAction,
}

impl VanillaTweaksButton {
    fn confirm_id(&self) -> u32 {
        CONFIRM_VT_BASE
            + VanillaTweaksAction::ALL
                .iter()
                .position(|&a| a == self.action)
                .unwrap_or(0) as u32
    }
}

impl ScreenWidget for VanillaTweaksButton {
    fn bounds(&self) -> [f32; 4] {
        self.action.rect(lock(&self.state).origin)
    }

    fn visible(&self) -> bool {
        let s = lock(&self.state);
        s.mode == SyncMode::VanillaTweaks && s.tab == Tab::VanillaTweaks
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
        let r = self.bounds();
        let confirmed = confirm_active(&lock(&self.state), self.confirm_id());
        draw_button_bg(draw, r, true, hovered || focused);
        let label = if confirmed {
            tr(
                host,
                "screen.armorstatues.vanilla_tweaks.trigger_sent",
                "Sent!",
            )
        } else {
            tr(host, self.action.label_key(), "")
        };
        let color = if confirmed {
            0xFF55FF55
        } else if hovered || focused {
            0xFFFFFFA0
        } else {
            0xFFFFFFFF
        };
        draw_centered_str(draw, &label, r[0] + r[2] * 0.5, r[1] + 6.0, color, true);
    }

    fn mouse_clicked(
        &mut self,
        host: &mut dyn ClientHost,
        _mouse: [f32; 2],
        button: i32,
        _modifiers: u32,
        _double_click: bool,
    ) -> bool {
        if button != 0 {
            return false;
        }
        let close_after = self.action == VanillaTweaksAction::CheckTarget;
        {
            let mut s = lock(&self.state);
            send_vt_trigger(host, &s, self.action.trigger_id());
            set_confirm(&mut s, self.confirm_id(), CONFIRM_TXT_TICKS);
        }
        click_sound(host);
        if close_after {
            host.close_screen();
        }
        true
    }
}

pub fn make_name_edit(initial: &str) -> TextEdit {
    let mut edit = TextEdit::new(crate::model::MAX_NAME_LEN);
    edit.set_value(initial);
    edit
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::slot_position;

    #[test]
    fn focus_row_visibility_matches_statuemenus_rules() {
        // No focus: all 6 rows visible.
        for row in 0..6 {
            assert!(position_row_visible(None, row));
        }
        // Focusing Scale (0) or Rotation (1) shows only that slider row.
        for active in [0, 1] {
            for row in 0..6 {
                assert_eq!(position_row_visible(Some(active), row), row == active);
            }
        }
        // Focusing Move By (2) keeps X/Y/Z (3..6) visible too.
        assert!(!position_row_visible(Some(2), 0));
        assert!(!position_row_visible(Some(2), 1));
        for row in 2..6 {
            assert!(position_row_visible(Some(2), row));
        }
        // Focusing X/Y/Z (3, 4, or 5) keeps Move By (2) and that axis row visible.
        for active in 3..6 {
            assert!(!position_row_visible(Some(active), 0));
            assert!(!position_row_visible(Some(active), 1));
            assert!(position_row_visible(Some(active), 2));
            for row in 3..6 {
                assert_eq!(position_row_visible(Some(active), row), row == active);
            }
        }
    }

    #[test]
    fn all_42_slots_sit_within_panel_bounds() {
        for slot in 0..42 {
            let [x, y] = slot_position(slot);
            assert!(x >= 1.0 && x + 16.0 < PANEL_W, "slot {slot} x={x}");
            assert!(y >= 1.0 && y + 16.0 < PANEL_H, "slot {slot} y={y}");
        }
        assert_eq!(slot_position(0), [58.0, 20.0]);
        assert_eq!(slot_position(4), [136.0, 56.0]);
        assert_eq!(slot_position(5), [136.0, 74.0]);
        assert_eq!(slot_position(6), [25.0, 96.0]);
        assert_eq!(slot_position(33), [25.0, 154.0]);
    }
}
