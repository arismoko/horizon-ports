//! Dynamic FPS configuration model, TOML persistence, and the registered settings.

use mod_api::{
    ClientHost, ConfigKind, ConfigSpec, Registrar, SettingKey, SettingSpec, SettingValue,
    SoundSource,
};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

pub const MOD_ID: &str = "dynamicfps";
pub const CONFIG_FILE_NAME: &str = "dynamicfps.toml";

/// Value representing no frame rate limit (vanilla's unlimited slider position).
pub const NO_FRAME_RATE_LIMIT: i32 = 260;

/// Transition speed at or above which volume changes apply immediately.
pub const VOLUME_TRANSITION_IMMEDIATE: f32 = 10.0;

// Options > Mods setting keys.
pub const SET_ENABLED: &str = "enabled";
pub const SET_IGNORE_INITIAL_CLICK: &str = "ignore_initial_click";
pub const SET_IDLE_TIME: &str = "idle_time";
pub const SET_IDLE_CONDITION: &str = "idle_condition";
pub const SET_VOLUME_UP: &str = "volume_transition_up";
pub const SET_VOLUME_DOWN: &str = "volume_transition_down";

pub const SET_UNFOCUSED_FPS: &str = "unfocused_fps";
pub const SET_UNFOCUSED_VOLUME_MASTER: &str = "unfocused_volume_master";
pub const SET_UNFOCUSED_SHOW_TOASTS: &str = "unfocused_show_toasts";

pub const SET_INVISIBLE_FPS: &str = "invisible_fps";
pub const SET_INVISIBLE_VOLUME_MASTER: &str = "invisible_volume_master";
pub const SET_INVISIBLE_SHOW_TOASTS: &str = "invisible_show_toasts";

pub const SET_HOVERED_FPS: &str = "hovered_fps";
pub const SET_HOVERED_VOLUME_MASTER: &str = "hovered_volume_master";
pub const SET_HOVERED_SHOW_TOASTS: &str = "hovered_show_toasts";

pub const SET_ABANDONED_FPS: &str = "abandoned_fps";
pub const SET_ABANDONED_VOLUME_MASTER: &str = "abandoned_volume_master";
pub const SET_ABANDONED_SHOW_TOASTS: &str = "abandoned_show_toasts";

pub const SET_UNPLUGGED_FPS: &str = "unplugged_fps";

pub const SET_BATTERY_ENABLED: &str = "battery_enabled";
pub const SET_BATTERY_SWITCH_STATES: &str = "battery_switch_states";
pub const SET_BATTERY_NOTIFICATIONS: &str = "battery_notifications";
pub const SET_BATTERY_CRITICAL_LEVEL: &str = "battery_critical_level";
pub const SET_BATTERY_INDICATOR_CONDITION: &str = "battery_indicator_condition";
pub const SET_BATTERY_INDICATOR_DEBUG: &str = "battery_indicator_debug";
pub const SET_BATTERY_INDICATOR_PLACEMENT: &str = "battery_indicator_placement";
pub const SET_MOCK_BATTERY_DATA: &str = "mock_battery_data";

/// Client window power state, matching Dynamic FPS's `PowerState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PowerState {
    Focused,
    Hovered,
    Unfocused,
    Invisible,
    Unplugged,
    Abandoned,
}

impl PowerState {
    pub const fn key(self) -> &'static str {
        match self {
            Self::Focused => "focused",
            Self::Hovered => "hovered",
            Self::Unfocused => "unfocused",
            Self::Invisible => "invisible",
            Self::Unplugged => "unplugged",
            Self::Abandoned => "abandoned",
        }
    }
}

/// When to ignore an accidental mouse click right after regaining window focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IgnoreInitialClick {
    Disabled,
    InWorld,
    Constant,
}

impl IgnoreInitialClick {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::InWorld => "in_world",
            Self::Constant => "constant",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "disabled" => Some(Self::Disabled),
            "in_world" => Some(Self::InWorld),
            "constant" => Some(Self::Constant),
            _ => None,
        }
    }

    pub const fn from_choice(index: u32) -> Self {
        match index {
            1 => Self::InWorld,
            2 => Self::Constant,
            _ => Self::Disabled,
        }
    }

    pub const fn to_choice(self) -> u32 {
        match self {
            Self::Disabled => 0,
            Self::InWorld => 1,
            Self::Constant => 2,
        }
    }
}

/// Condition under which Dynamic FPS activates the Idle (`Abandoned`) power state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdleCondition {
    None,
    Vanilla,
    OnBattery,
}

impl IdleCondition {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Vanilla => "vanilla",
            Self::OnBattery => "on_battery",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "vanilla" => Some(Self::Vanilla),
            "on_battery" => Some(Self::OnBattery),
            _ => None,
        }
    }

    pub const fn from_choice(index: u32) -> Self {
        match index {
            0 => Self::None,
            2 => Self::OnBattery,
            _ => Self::Vanilla,
        }
    }

    pub const fn to_choice(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Vanilla => 1,
            Self::OnBattery => 2,
        }
    }
}

/// Graphics preset stored per power state (runtime application awaits sunrise#5553).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphicsState {
    Default,
    Reduced,
    Minimal,
}

impl GraphicsState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Reduced => "reduced",
            Self::Minimal => "minimal",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "default" => Some(Self::Default),
            "reduced" => Some(Self::Reduced),
            "minimal" => Some(Self::Minimal),
            _ => None,
        }
    }
}

/// Condition under which the HUD battery indicator is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatteryIndicatorCondition {
    Disabled,
    Draining,
    Critical,
    Constant,
}

impl BatteryIndicatorCondition {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Draining => "draining",
            Self::Critical => "critical",
            Self::Constant => "constant",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "disabled" => Some(Self::Disabled),
            "draining" => Some(Self::Draining),
            "critical" => Some(Self::Critical),
            "constant" => Some(Self::Constant),
            _ => None,
        }
    }

    pub const fn from_choice(index: u32) -> Self {
        match index {
            0 => Self::Disabled,
            1 => Self::Draining,
            3 => Self::Constant,
            _ => Self::Critical,
        }
    }

    pub const fn to_choice(self) -> u32 {
        match self {
            Self::Disabled => 0,
            Self::Draining => 1,
            Self::Critical => 2,
            Self::Constant => 3,
        }
    }
}

/// Screen corner for the HUD battery indicator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatteryIndicatorPlacement {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl BatteryIndicatorPlacement {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TopLeft => "top_left",
            Self::TopRight => "top_right",
            Self::BottomLeft => "bottom_left",
            Self::BottomRight => "bottom_right",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "top_left" => Some(Self::TopLeft),
            "top_right" => Some(Self::TopRight),
            "bottom_left" => Some(Self::BottomLeft),
            "bottom_right" => Some(Self::BottomRight),
            _ => None,
        }
    }

    pub const fn from_choice(index: u32) -> Self {
        match index {
            1 => Self::TopRight,
            2 => Self::BottomLeft,
            3 => Self::BottomRight,
            _ => Self::TopLeft,
        }
    }

    pub const fn to_choice(self) -> u32 {
        match self {
            Self::TopLeft => 0,
            Self::TopRight => 1,
            Self::BottomLeft => 2,
            Self::BottomRight => 3,
        }
    }

    /// Corner coordinates in GUI pixels, matching Dynamic FPS's `BatteryIndicatorPlacement`.
    pub fn origin(self, gui_width: f32, gui_height: f32) -> [f32; 2] {
        match self {
            Self::TopLeft => [4.0, 4.0],
            Self::TopRight => [(gui_width - 47.0).max(4.0), 4.0],
            Self::BottomLeft => [4.0, (gui_height - 20.0).max(4.0)],
            Self::BottomRight => [(gui_width - 47.0).max(4.0), (gui_height - 20.0).max(4.0)],
        }
    }
}

/// Configuration for one non-focused power state.
#[derive(Clone, Debug, PartialEq)]
pub struct StateConfig {
    /// Target frame rate in FPS (`0..=260`; `260` means unlimited).
    pub frame_rate_target: i32,
    /// Per-state VSync preference (stored for config parity; awaits sunrise#5553).
    pub enable_vsync: bool,
    /// Volume multipliers indexed by `SoundSource::index()` (`0.0..=1.0`).
    pub volume_multipliers: [f32; 11],
    /// Per-state graphics preset (stored for config parity; awaits sunrise#5553).
    pub graphics_state: GraphicsState,
    /// Whether to allow toast notifications while in this state.
    pub show_toasts: bool,
}

impl StateConfig {
    pub const ACTIVE: Self = Self {
        frame_rate_target: NO_FRAME_RATE_LIMIT,
        enable_vsync: false,
        volume_multipliers: [1.0; 11],
        graphics_state: GraphicsState::Default,
        show_toasts: true,
    };

    /// Raw volume multiplier configured for `source`.
    pub fn raw_volume_multiplier(&self, source: SoundSource) -> f32 {
        self.volume_multipliers[source.index()]
    }

    /// Effective volume multiplier for `source`: `0.0` whenever `Master` is `0.0`.
    pub fn volume_multiplier(&self, source: SoundSource) -> f32 {
        if self.volume_multipliers[SoundSource::Master.index()] <= 0.0 {
            0.0
        } else {
            self.volume_multipliers[source.index()]
        }
    }

    pub fn set_volume_multiplier(&mut self, source: SoundSource, value: f32) {
        if value.is_finite() {
            self.volume_multipliers[source.index()] = value.clamp(0.0, 1.0);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdleConfig {
    /// Timeout in seconds (`0` disables idle detection).
    pub timeout_seconds: u32,
    pub condition: IdleCondition,
}

impl Default for IdleConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: 300,
            condition: IdleCondition::Vanilla,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VolumeTransitionConfig {
    /// Volume increase per second (`>= 10.0` means immediate).
    pub up: f32,
    /// Volume decrease per second (`>= 10.0` means immediate).
    pub down: f32,
}

impl Default for VolumeTransitionConfig {
    fn default() -> Self {
        Self { up: 1.0, down: 0.5 }
    }
}

impl VolumeTransitionConfig {
    pub fn is_active(self) -> bool {
        self.up < VOLUME_TRANSITION_IMMEDIATE || self.down < VOLUME_TRANSITION_IMMEDIATE
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatteryTrackerConfig {
    pub enabled: bool,
    pub critical_level: u8,
    pub switch_states: bool,
    pub notifications: bool,
    pub show_when_debug: bool,
    pub display_condition: BatteryIndicatorCondition,
    pub display_placement: BatteryIndicatorPlacement,
    pub mock_battery_data: bool,
}

impl Default for BatteryTrackerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            critical_level: 10,
            switch_states: false,
            notifications: true,
            show_when_debug: false,
            display_condition: BatteryIndicatorCondition::Critical,
            display_placement: BatteryIndicatorPlacement::TopLeft,
            mock_battery_data: false,
        }
    }
}

/// Complete Dynamic FPS client configuration, matching `default_config.json` defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct DynamicFpsConfig {
    pub enabled: bool,
    pub uncap_menu_frame_rate: bool,
    pub ignore_initial_click: IgnoreInitialClick,
    pub idle: IdleConfig,
    pub volume_transition: VolumeTransitionConfig,
    pub battery: BatteryTrackerConfig,
    pub hovered: StateConfig,
    pub unfocused: StateConfig,
    pub invisible: StateConfig,
    pub unplugged: StateConfig,
    pub abandoned: StateConfig,
}

impl Default for DynamicFpsConfig {
    fn default() -> Self {
        let mut unfocused_vols = [1.0; 11];
        unfocused_vols[SoundSource::Master.index()] = 0.25;

        let mut invisible_vols = [1.0; 11];
        invisible_vols[SoundSource::Master.index()] = 0.0;

        Self {
            enabled: true,
            uncap_menu_frame_rate: false,
            ignore_initial_click: IgnoreInitialClick::Disabled,
            idle: IdleConfig::default(),
            volume_transition: VolumeTransitionConfig::default(),
            battery: BatteryTrackerConfig::default(),
            hovered: StateConfig {
                frame_rate_target: 60,
                enable_vsync: false,
                volume_multipliers: [1.0; 11],
                graphics_state: GraphicsState::Default,
                show_toasts: true,
            },
            unfocused: StateConfig {
                frame_rate_target: 1,
                enable_vsync: false,
                volume_multipliers: unfocused_vols,
                graphics_state: GraphicsState::Default,
                show_toasts: false,
            },
            invisible: StateConfig {
                frame_rate_target: 0,
                enable_vsync: false,
                volume_multipliers: invisible_vols,
                graphics_state: GraphicsState::Default,
                show_toasts: false,
            },
            unplugged: StateConfig {
                frame_rate_target: NO_FRAME_RATE_LIMIT,
                enable_vsync: true,
                volume_multipliers: [1.0; 11],
                graphics_state: GraphicsState::Default,
                show_toasts: true,
            },
            abandoned: StateConfig {
                frame_rate_target: 10,
                enable_vsync: false,
                volume_multipliers: [1.0; 11],
                graphics_state: GraphicsState::Default,
                show_toasts: false,
            },
        }
    }
}

impl DynamicFpsConfig {
    pub fn state(&self, state: PowerState) -> &StateConfig {
        match state {
            PowerState::Focused => &StateConfig::ACTIVE,
            PowerState::Hovered => &self.hovered,
            PowerState::Unfocused => &self.unfocused,
            PowerState::Invisible => &self.invisible,
            PowerState::Unplugged => &self.unplugged,
            PowerState::Abandoned => &self.abandoned,
        }
    }

    pub fn state_mut(&mut self, state: PowerState) -> Option<&mut StateConfig> {
        match state {
            PowerState::Focused => None,
            PowerState::Hovered => Some(&mut self.hovered),
            PowerState::Unfocused => Some(&mut self.unfocused),
            PowerState::Invisible => Some(&mut self.invisible),
            PowerState::Unplugged => Some(&mut self.unplugged),
            PowerState::Abandoned => Some(&mut self.abandoned),
        }
    }

    /// Serialize to human-readable TOML for `config/dynamicfps.toml`.
    pub fn to_toml(&self) -> String {
        let mut out = String::with_capacity(2048);
        out.push_str("# Dynamic FPS client configuration\n");
        out.push_str("# Changes saved to this file are hot-reloaded automatically.\n\n");

        out.push_str("[general]\n");
        out.push_str(&format!("enabled = {}\n", self.enabled));
        out.push_str("# uncap_menu_frame_rate requires sunrise-org/sunrise#5550\n");
        out.push_str(&format!(
            "uncap_menu_frame_rate = {}\n",
            self.uncap_menu_frame_rate
        ));
        out.push_str("# \"disabled\", \"in_world\", or \"constant\"\n");
        out.push_str(&format!(
            "ignore_initial_click = \"{}\"\n\n",
            self.ignore_initial_click.as_str()
        ));

        out.push_str("[idle]\n");
        out.push_str("# Seconds without input before activating Idle state (0 disables)\n");
        out.push_str(&format!("timeout = {}\n", self.idle.timeout_seconds));
        out.push_str("# \"none\", \"vanilla\", or \"on_battery\"\n");
        out.push_str(&format!(
            "condition = \"{}\"\n\n",
            self.idle.condition.as_str()
        ));

        out.push_str("[volume_transition]\n");
        out.push_str("# Volume change per second (10.0 = immediate)\n");
        out.push_str(&format!("up = {:.2}\n", self.volume_transition.up));
        out.push_str(&format!("down = {:.2}\n\n", self.volume_transition.down));

        out.push_str("[battery]\n");
        out.push_str(&format!("enabled = {}\n", self.battery.enabled));
        out.push_str(&format!(
            "critical_level = {}\n",
            self.battery.critical_level
        ));
        out.push_str(&format!("switch_states = {}\n", self.battery.switch_states));
        out.push_str(&format!("notifications = {}\n", self.battery.notifications));
        out.push_str(&format!(
            "show_when_debug = {}\n",
            self.battery.show_when_debug
        ));
        out.push_str("# \"disabled\", \"draining\", \"critical\", or \"constant\"\n");
        out.push_str(&format!(
            "display_condition = \"{}\"\n",
            self.battery.display_condition.as_str()
        ));
        out.push_str("# \"top_left\", \"top_right\", \"bottom_left\", or \"bottom_right\"\n");
        out.push_str(&format!(
            "display_placement = \"{}\"\n",
            self.battery.display_placement.as_str()
        ));
        out.push_str(&format!(
            "mock_battery_data = {}\n\n",
            self.battery.mock_battery_data
        ));

        write_state_toml(&mut out, "hovered", &self.hovered, true);
        write_state_toml(&mut out, "unfocused", &self.unfocused, true);
        write_state_toml(&mut out, "invisible", &self.invisible, true);
        write_state_toml(&mut out, "unplugged", &self.unplugged, false);
        write_state_toml(&mut out, "abandoned", &self.abandoned, true);
        out
    }

    /// Parse `config/dynamicfps.toml`, clamping invalid values to safe bounds without panicking.
    pub fn parse_toml(source: &str) -> Self {
        let mut cfg = Self::default();
        let mut section = String::new();

        for raw_line in source.lines() {
            let line = strip_toml_comment(raw_line).trim();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix('[') {
                if let Some(name) = rest.strip_suffix(']') {
                    section = name.trim().to_ascii_lowercase();
                }
                continue;
            }
            let Some((raw_key, raw_val)) = line.split_once('=') else {
                continue;
            };
            let key = raw_key.trim().to_ascii_lowercase();
            let val = raw_val.trim();

            match section.as_str() {
                "" | "general" => match key.as_str() {
                    "enabled" => {
                        if let Some(v) = parse_bool(val) {
                            cfg.enabled = v;
                        }
                    }
                    "uncap_menu_frame_rate" => {
                        if let Some(v) = parse_bool(val) {
                            cfg.uncap_menu_frame_rate = v;
                        }
                    }
                    "ignore_initial_click" => {
                        if let Some(v) = IgnoreInitialClick::parse(unquote(val)) {
                            cfg.ignore_initial_click = v;
                        }
                    }
                    _ => {}
                },
                "idle" => match key.as_str() {
                    "timeout" => {
                        if let Some(v) = parse_i32(val) {
                            cfg.idle.timeout_seconds = v.clamp(0, 86_400) as u32;
                        }
                    }
                    "condition" => {
                        if let Some(v) = IdleCondition::parse(unquote(val)) {
                            cfg.idle.condition = v;
                        }
                    }
                    _ => {}
                },
                "volume_transition" => match key.as_str() {
                    "up" => {
                        if let Some(v) = parse_volume_speed(val) {
                            cfg.volume_transition.up = v;
                        }
                    }
                    "down" => {
                        if let Some(v) = parse_volume_speed(val) {
                            cfg.volume_transition.down = v;
                        }
                    }
                    _ => {}
                },
                "battery" => match key.as_str() {
                    "enabled" => {
                        if let Some(v) = parse_bool(val) {
                            cfg.battery.enabled = v;
                        }
                    }
                    "critical_level" => {
                        if let Some(v) = parse_i32(val) {
                            cfg.battery.critical_level = v.clamp(1, 50) as u8;
                        }
                    }
                    "switch_states" => {
                        if let Some(v) = parse_bool(val) {
                            cfg.battery.switch_states = v;
                        }
                    }
                    "notifications" => {
                        if let Some(v) = parse_bool(val) {
                            cfg.battery.notifications = v;
                        }
                    }
                    "show_when_debug" => {
                        if let Some(v) = parse_bool(val) {
                            cfg.battery.show_when_debug = v;
                        }
                    }
                    "display_condition" => {
                        if let Some(v) = BatteryIndicatorCondition::parse(unquote(val)) {
                            cfg.battery.display_condition = v;
                        }
                    }
                    "display_placement" => {
                        if let Some(v) = BatteryIndicatorPlacement::parse(unquote(val)) {
                            cfg.battery.display_placement = v;
                        }
                    }
                    "mock_battery_data" => {
                        if let Some(v) = parse_bool(val) {
                            cfg.battery.mock_battery_data = v;
                        }
                    }
                    _ => {}
                },
                other => {
                    if let Some(state_rest) = other.strip_prefix("states.") {
                        if let Some(state_name) = state_rest.strip_suffix(".volume_multipliers") {
                            if let (Some(state), Some(source), Some(mult)) = (
                                parse_state_name(state_name),
                                parse_sound_source(&key),
                                parse_f32(val),
                            ) && let Some(sc) = cfg.state_mut(state)
                            {
                                sc.set_volume_multiplier(source, mult);
                            }
                        } else if let Some(state) = parse_state_name(state_rest)
                            && let Some(sc) = cfg.state_mut(state)
                        {
                            apply_state_field(sc, &key, val);
                        }
                    }
                }
            }
        }

        cfg
    }

    /// Apply one registered setting's new value onto `self`. Returns whether `key` named a
    /// setting of ours and `value` had the right kind.
    pub fn apply_single_setting(&mut self, key: &str, value: &SettingValue) -> bool {
        let applied = match key {
            SET_ENABLED => toggle_value(value).map(|v| self.enabled = v),
            SET_IGNORE_INITIAL_CLICK => choice_value(value)
                .map(|v| self.ignore_initial_click = IgnoreInitialClick::from_choice(v)),
            SET_IDLE_TIME => number_value(value).map(|v| {
                self.idle.timeout_seconds = (v.round().clamp(0.0, 30.0) as u32) * 60;
            }),
            SET_IDLE_CONDITION => {
                choice_value(value).map(|v| self.idle.condition = IdleCondition::from_choice(v))
            }
            SET_VOLUME_UP => number_value(value).map(|v| {
                self.volume_transition.up = (v as f32).clamp(0.05, VOLUME_TRANSITION_IMMEDIATE);
            }),
            SET_VOLUME_DOWN => number_value(value).map(|v| {
                self.volume_transition.down = (v as f32).clamp(0.05, VOLUME_TRANSITION_IMMEDIATE);
            }),
            SET_UNFOCUSED_FPS => number_value(value).map(|v| apply_fps(&mut self.unfocused, v)),
            SET_UNFOCUSED_VOLUME_MASTER => {
                number_value(value).map(|v| apply_master_volume(&mut self.unfocused, v))
            }
            SET_UNFOCUSED_SHOW_TOASTS => {
                toggle_value(value).map(|v| self.unfocused.show_toasts = v)
            }
            SET_INVISIBLE_FPS => number_value(value).map(|v| apply_fps(&mut self.invisible, v)),
            SET_INVISIBLE_VOLUME_MASTER => {
                number_value(value).map(|v| apply_master_volume(&mut self.invisible, v))
            }
            SET_INVISIBLE_SHOW_TOASTS => {
                toggle_value(value).map(|v| self.invisible.show_toasts = v)
            }
            SET_HOVERED_FPS => number_value(value).map(|v| apply_fps(&mut self.hovered, v)),
            SET_HOVERED_VOLUME_MASTER => {
                number_value(value).map(|v| apply_master_volume(&mut self.hovered, v))
            }
            SET_HOVERED_SHOW_TOASTS => toggle_value(value).map(|v| self.hovered.show_toasts = v),
            SET_ABANDONED_FPS => number_value(value).map(|v| apply_fps(&mut self.abandoned, v)),
            SET_ABANDONED_VOLUME_MASTER => {
                number_value(value).map(|v| apply_master_volume(&mut self.abandoned, v))
            }
            SET_ABANDONED_SHOW_TOASTS => {
                toggle_value(value).map(|v| self.abandoned.show_toasts = v)
            }
            SET_UNPLUGGED_FPS => number_value(value).map(|v| apply_fps(&mut self.unplugged, v)),
            SET_BATTERY_ENABLED => toggle_value(value).map(|v| self.battery.enabled = v),
            SET_BATTERY_SWITCH_STATES => {
                toggle_value(value).map(|v| self.battery.switch_states = v)
            }
            SET_BATTERY_NOTIFICATIONS => {
                toggle_value(value).map(|v| self.battery.notifications = v)
            }
            SET_BATTERY_CRITICAL_LEVEL => number_value(value)
                .map(|v| self.battery.critical_level = v.round().clamp(1.0, 50.0) as u8),
            SET_BATTERY_INDICATOR_CONDITION => choice_value(value).map(|v| {
                self.battery.display_condition = BatteryIndicatorCondition::from_choice(v)
            }),
            SET_BATTERY_INDICATOR_DEBUG => {
                toggle_value(value).map(|v| self.battery.show_when_debug = v)
            }
            SET_BATTERY_INDICATOR_PLACEMENT => choice_value(value).map(|v| {
                self.battery.display_placement = BatteryIndicatorPlacement::from_choice(v)
            }),
            SET_MOCK_BATTERY_DATA => {
                toggle_value(value).map(|v| self.battery.mock_battery_data = v)
            }
            _ => None,
        };
        applied.is_some()
    }

    /// This config's values as registered settings, in the units the settings use.
    pub fn setting_values(&self) -> Vec<(&'static str, SettingValue)> {
        let percent = |state: &StateConfig| {
            SettingValue::Range(f64::from(
                (state.raw_volume_multiplier(SoundSource::Master) * 100.0).round(),
            ))
        };
        let fps = |state: &StateConfig| SettingValue::Range(f64::from(state.frame_rate_target));
        vec![
            (SET_ENABLED, SettingValue::Toggle(self.enabled)),
            (
                SET_IGNORE_INITIAL_CLICK,
                SettingValue::Choice(self.ignore_initial_click.to_choice() as usize),
            ),
            (
                SET_IDLE_TIME,
                SettingValue::Range(f64::from((self.idle.timeout_seconds / 60).min(30))),
            ),
            (
                SET_IDLE_CONDITION,
                SettingValue::Choice(self.idle.condition.to_choice() as usize),
            ),
            (
                SET_VOLUME_UP,
                SettingValue::Range(f64::from(self.volume_transition.up)),
            ),
            (
                SET_VOLUME_DOWN,
                SettingValue::Range(f64::from(self.volume_transition.down)),
            ),
            (SET_UNFOCUSED_FPS, fps(&self.unfocused)),
            (SET_UNFOCUSED_VOLUME_MASTER, percent(&self.unfocused)),
            (
                SET_UNFOCUSED_SHOW_TOASTS,
                SettingValue::Toggle(self.unfocused.show_toasts),
            ),
            (SET_INVISIBLE_FPS, fps(&self.invisible)),
            (SET_INVISIBLE_VOLUME_MASTER, percent(&self.invisible)),
            (
                SET_INVISIBLE_SHOW_TOASTS,
                SettingValue::Toggle(self.invisible.show_toasts),
            ),
            (SET_HOVERED_FPS, fps(&self.hovered)),
            (SET_HOVERED_VOLUME_MASTER, percent(&self.hovered)),
            (
                SET_HOVERED_SHOW_TOASTS,
                SettingValue::Toggle(self.hovered.show_toasts),
            ),
            (SET_ABANDONED_FPS, fps(&self.abandoned)),
            (SET_ABANDONED_VOLUME_MASTER, percent(&self.abandoned)),
            (
                SET_ABANDONED_SHOW_TOASTS,
                SettingValue::Toggle(self.abandoned.show_toasts),
            ),
            (SET_UNPLUGGED_FPS, fps(&self.unplugged)),
            (
                SET_BATTERY_ENABLED,
                SettingValue::Toggle(self.battery.enabled),
            ),
            (
                SET_BATTERY_SWITCH_STATES,
                SettingValue::Toggle(self.battery.switch_states),
            ),
            (
                SET_BATTERY_NOTIFICATIONS,
                SettingValue::Toggle(self.battery.notifications),
            ),
            (
                SET_BATTERY_CRITICAL_LEVEL,
                SettingValue::Range(f64::from(self.battery.critical_level)),
            ),
            (
                SET_BATTERY_INDICATOR_CONDITION,
                SettingValue::Choice(self.battery.display_condition.to_choice() as usize),
            ),
            (
                SET_BATTERY_INDICATOR_DEBUG,
                SettingValue::Toggle(self.battery.show_when_debug),
            ),
            (
                SET_BATTERY_INDICATOR_PLACEMENT,
                SettingValue::Choice(self.battery.display_placement.to_choice() as usize),
            ),
            (
                SET_MOCK_BATTERY_DATA,
                SettingValue::Toggle(self.battery.mock_battery_data),
            ),
        ]
    }

    /// Push this config's values into the registered settings store so the config screen
    /// reflects edits made to `config/dynamicfps.toml`. Every write notifies
    /// `setting_changed` synchronously, so callers hold no lock and `SYNCING` is set.
    pub fn write_to_host_settings(&self, host: &mut dyn ClientHost) {
        for (name, value) in self.setting_values() {
            if let Some(key) = setting_key(name) {
                let _ = host.set_setting(key, value);
            }
        }
    }
}

fn toggle_value(value: &SettingValue) -> Option<bool> {
    match value {
        SettingValue::Toggle(v) => Some(*v),
        _ => None,
    }
}

fn choice_value(value: &SettingValue) -> Option<u32> {
    match value {
        SettingValue::Choice(v) => Some(u32::try_from(*v).unwrap_or(u32::MAX)),
        _ => None,
    }
}

fn number_value(value: &SettingValue) -> Option<f64> {
    match value {
        SettingValue::Range(v) if v.is_finite() => Some(*v),
        _ => None,
    }
}

/// Set while this mod mirrors the config file into the settings store, so the resulting
/// `setting_changed` notices are not applied back or written to the file again.
pub static SYNCING: AtomicBool = AtomicBool::new(false);

static SETTING_KEYS: Mutex<Vec<(&'static str, SettingKey)>> = Mutex::new(Vec::new());

/// The key `register_setting` issued for `name`, once `register_settings` ran.
pub fn setting_key(name: &str) -> Option<SettingKey> {
    let keys = SETTING_KEYS.lock().unwrap_or_else(|e| e.into_inner());
    keys.iter()
        .find(|(known, _)| *known == name)
        .map(|(_, key)| *key)
}

/// Mirror `config` into the settings store without notifying ourselves.
pub fn sync_settings_to_host(config: &DynamicFpsConfig, host: &mut dyn ClientHost) {
    SYNCING.store(true, Ordering::SeqCst);
    config.write_to_host_settings(host);
    SYNCING.store(false, Ordering::SeqCst);
}

/// The client config file `config/dynamicfps.toml`.
pub fn config_spec() -> ConfigSpec {
    let default_toml = DynamicFpsConfig::default().to_toml();
    ConfigSpec::new(ConfigKind::Client, default_toml).file(CONFIG_FILE_NAME)
}

/// What a row of the config screen edits.
#[derive(Clone, Copy, Debug)]
pub enum RowKind {
    Toggle {
        default: bool,
    },
    Choice {
        options: &'static [&'static str],
        default: usize,
    },
    Range {
        min: f64,
        max: f64,
        step: f64,
        default: f64,
    },
}

/// One registered setting, as the config screen's row and the settings registry see it.
#[derive(Clone, Copy, Debug)]
pub struct RowDef {
    pub key: &'static str,
    /// Translation key of the row's label; `<label>_tooltip` is its description.
    pub label: &'static str,
    pub kind: RowKind,
}

/// One page of the config screen.
#[derive(Clone, Copy, Debug)]
pub struct TabDef {
    /// Translation key of the page title.
    pub title: &'static str,
    pub rows: &'static [RowDef],
}

const fn toggle_row(key: &'static str, label: &'static str, default: bool) -> RowDef {
    RowDef {
        key,
        label,
        kind: RowKind::Toggle { default },
    }
}

const fn choice_row(
    key: &'static str,
    label: &'static str,
    options: &'static [&'static str],
    default: usize,
) -> RowDef {
    RowDef {
        key,
        label,
        kind: RowKind::Choice { options, default },
    }
}

const fn range_row(
    key: &'static str,
    label: &'static str,
    min: f64,
    max: f64,
    step: f64,
    default: f64,
) -> RowDef {
    RowDef {
        key,
        label,
        kind: RowKind::Range {
            min,
            max,
            step,
            default,
        },
    }
}

/// Every setting, grouped into the config screen's pages.
pub const TABS: &[TabDef] = &[
    TabDef {
        title: "config.dynamicfps.tab.general",
        rows: &[
            toggle_row(SET_ENABLED, "config.dynamicfps.enabled", true),
            choice_row(
                SET_IGNORE_INITIAL_CLICK,
                "config.dynamicfps.ignore_initial_click",
                &[
                    "config.dynamicfps.ignore_initial_click_disabled",
                    "config.dynamicfps.ignore_initial_click_in_world",
                    "config.dynamicfps.ignore_initial_click_constant",
                ],
                0,
            ),
            range_row(
                SET_IDLE_TIME,
                "config.dynamicfps.idle_time",
                0.0,
                30.0,
                1.0,
                5.0,
            ),
            choice_row(
                SET_IDLE_CONDITION,
                "config.dynamicfps.idle_condition",
                &[
                    "config.dynamicfps.idle_condition_none",
                    "config.dynamicfps.idle_condition_vanilla",
                    "config.dynamicfps.idle_condition_on_battery",
                ],
                1,
            ),
            range_row(
                SET_VOLUME_UP,
                "config.dynamicfps.volume_transition_up",
                0.1,
                10.0,
                0.1,
                1.0,
            ),
            range_row(
                SET_VOLUME_DOWN,
                "config.dynamicfps.volume_transition_down",
                0.1,
                10.0,
                0.1,
                0.5,
            ),
        ],
    },
    TabDef {
        title: "config.dynamicfps.tab.unfocused",
        rows: &[
            range_row(
                SET_UNFOCUSED_FPS,
                "config.dynamicfps.unfocused_fps",
                0.0,
                260.0,
                1.0,
                1.0,
            ),
            range_row(
                SET_UNFOCUSED_VOLUME_MASTER,
                "config.dynamicfps.unfocused_volume_master",
                0.0,
                100.0,
                5.0,
                25.0,
            ),
            toggle_row(
                SET_UNFOCUSED_SHOW_TOASTS,
                "config.dynamicfps.unfocused_show_toasts",
                false,
            ),
        ],
    },
    TabDef {
        title: "config.dynamicfps.tab.invisible",
        rows: &[
            range_row(
                SET_INVISIBLE_FPS,
                "config.dynamicfps.invisible_fps",
                0.0,
                260.0,
                1.0,
                0.0,
            ),
            range_row(
                SET_INVISIBLE_VOLUME_MASTER,
                "config.dynamicfps.invisible_volume_master",
                0.0,
                100.0,
                5.0,
                0.0,
            ),
            toggle_row(
                SET_INVISIBLE_SHOW_TOASTS,
                "config.dynamicfps.invisible_show_toasts",
                false,
            ),
        ],
    },
    TabDef {
        title: "config.dynamicfps.tab.abandoned",
        rows: &[
            range_row(
                SET_ABANDONED_FPS,
                "config.dynamicfps.abandoned_fps",
                0.0,
                260.0,
                1.0,
                10.0,
            ),
            range_row(
                SET_ABANDONED_VOLUME_MASTER,
                "config.dynamicfps.abandoned_volume_master",
                0.0,
                100.0,
                5.0,
                100.0,
            ),
            toggle_row(
                SET_ABANDONED_SHOW_TOASTS,
                "config.dynamicfps.abandoned_show_toasts",
                false,
            ),
        ],
    },
    TabDef {
        title: "config.dynamicfps.tab.hovered",
        rows: &[
            range_row(
                SET_HOVERED_FPS,
                "config.dynamicfps.hovered_fps",
                0.0,
                260.0,
                1.0,
                60.0,
            ),
            range_row(
                SET_HOVERED_VOLUME_MASTER,
                "config.dynamicfps.hovered_volume_master",
                0.0,
                100.0,
                5.0,
                100.0,
            ),
            toggle_row(
                SET_HOVERED_SHOW_TOASTS,
                "config.dynamicfps.hovered_show_toasts",
                true,
            ),
        ],
    },
    TabDef {
        title: "config.dynamicfps.tab.battery",
        rows: &[
            range_row(
                SET_UNPLUGGED_FPS,
                "config.dynamicfps.unplugged_fps",
                0.0,
                260.0,
                1.0,
                260.0,
            ),
            toggle_row(
                SET_BATTERY_ENABLED,
                "config.dynamicfps.battery_tracker",
                false,
            ),
            toggle_row(
                SET_BATTERY_SWITCH_STATES,
                "config.dynamicfps.battery_tracker_switch_states",
                false,
            ),
            toggle_row(
                SET_BATTERY_NOTIFICATIONS,
                "config.dynamicfps.battery_tracker_notifications",
                true,
            ),
            range_row(
                SET_BATTERY_CRITICAL_LEVEL,
                "config.dynamicfps.battery_critical_level",
                1.0,
                50.0,
                1.0,
                10.0,
            ),
        ],
    },
    TabDef {
        title: "config.dynamicfps.tab.battery_hud",
        rows: &[
            choice_row(
                SET_BATTERY_INDICATOR_CONDITION,
                "config.dynamicfps.battery_indicator_condition",
                &[
                    "config.dynamicfps.battery_indicator_condition_disabled",
                    "config.dynamicfps.battery_indicator_condition_draining",
                    "config.dynamicfps.battery_indicator_condition_critical",
                    "config.dynamicfps.battery_indicator_condition_constant",
                ],
                2,
            ),
            toggle_row(
                SET_BATTERY_INDICATOR_DEBUG,
                "config.dynamicfps.battery_indicator_debug",
                false,
            ),
            choice_row(
                SET_BATTERY_INDICATOR_PLACEMENT,
                "config.dynamicfps.battery_indicator_placement",
                &[
                    "config.dynamicfps.battery_indicator_placement_top_left",
                    "config.dynamicfps.battery_indicator_placement_top_right",
                    "config.dynamicfps.battery_indicator_placement_bottom_left",
                    "config.dynamicfps.battery_indicator_placement_bottom_right",
                ],
                0,
            ),
            toggle_row(
                SET_MOCK_BATTERY_DATA,
                "config.dynamicfps.mock_battery_data",
                false,
            ),
        ],
    },
];

/// Register every row of [`TABS`] as a setting and remember the keys issued.
pub fn register_settings(r: &mut dyn Registrar) {
    let mut issued = Vec::new();
    for row in TABS.iter().flat_map(|tab| tab.rows) {
        let tooltip = format!("{}_tooltip", row.label);
        let spec = match row.kind {
            RowKind::Toggle { default } => SettingSpec::toggle(row.key, row.label, default),
            RowKind::Choice { options, default } => {
                SettingSpec::choice(row.key, row.label, options.iter().copied(), default)
            }
            RowKind::Range {
                min,
                max,
                step,
                default,
            } => SettingSpec::range(row.key, row.label, min, max, step, default),
        };
        if let Ok(key) = r.register_setting(spec.tooltip(tooltip)) {
            issued.push((row.key, key));
        }
    }
    *SETTING_KEYS.lock().unwrap_or_else(|e| e.into_inner()) = issued;
}

pub const fn sound_source_name(source: SoundSource) -> &'static str {
    match source {
        SoundSource::Master => "master",
        SoundSource::Music => "music",
        SoundSource::Records => "records",
        SoundSource::Weather => "weather",
        SoundSource::Blocks => "blocks",
        SoundSource::Hostile => "hostile",
        SoundSource::Neutral => "neutral",
        SoundSource::Players => "players",
        SoundSource::Ambient => "ambient",
        SoundSource::Voice => "voice",
        SoundSource::Ui => "ui",
        _ => "master",
    }
}

pub fn parse_sound_source(name: &str) -> Option<SoundSource> {
    match name.trim().to_ascii_lowercase().as_str() {
        "master" => Some(SoundSource::Master),
        "music" => Some(SoundSource::Music),
        "records" => Some(SoundSource::Records),
        "weather" => Some(SoundSource::Weather),
        "blocks" => Some(SoundSource::Blocks),
        "hostile" => Some(SoundSource::Hostile),
        "neutral" => Some(SoundSource::Neutral),
        "players" => Some(SoundSource::Players),
        "ambient" => Some(SoundSource::Ambient),
        "voice" => Some(SoundSource::Voice),
        "ui" => Some(SoundSource::Ui),
        _ => None,
    }
}

fn write_state_toml(out: &mut String, name: &str, state: &StateConfig, full: bool) {
    out.push_str(&format!("[states.{name}]\n"));
    let fps_value = if state.frame_rate_target >= NO_FRAME_RATE_LIMIT {
        -1
    } else {
        state.frame_rate_target
    };
    out.push_str(&format!("frame_rate_target = {fps_value}\n"));
    out.push_str(&format!("enable_vsync = {}\n", state.enable_vsync));
    if full {
        out.push_str(&format!(
            "graphics_state = \"{}\"\n",
            state.graphics_state.as_str()
        ));
        out.push_str(&format!("show_toasts = {}\n", state.show_toasts));
        out.push_str(&format!("\n[states.{name}.volume_multipliers]\n"));
        for source in SoundSource::ALL {
            let key = sound_source_name(source);
            let mult = state.raw_volume_multiplier(source);
            out.push_str(&format!("{key} = {mult:.2}\n"));
        }
    }
    out.push('\n');
}

fn apply_fps(sc: &mut StateConfig, value: f64) {
    sc.frame_rate_target = value.round().clamp(0.0, NO_FRAME_RATE_LIMIT as f64) as i32;
}

fn apply_master_volume(sc: &mut StateConfig, percent: f64) {
    sc.set_volume_multiplier(
        SoundSource::Master,
        (percent as f32 / 100.0).clamp(0.0, 1.0),
    );
}

fn apply_state_field(sc: &mut StateConfig, key: &str, val: &str) {
    match key {
        "frame_rate_target" => {
            if let Some(v) = parse_i32(val) {
                sc.frame_rate_target = if !(0..NO_FRAME_RATE_LIMIT).contains(&v) {
                    NO_FRAME_RATE_LIMIT
                } else {
                    v
                };
            }
        }
        "enable_vsync" => {
            if let Some(v) = parse_bool(val) {
                sc.enable_vsync = v;
            }
        }
        "graphics_state" => {
            if let Some(v) = GraphicsState::parse(unquote(val)) {
                sc.graphics_state = v;
            }
        }
        "show_toasts" => {
            if let Some(v) = parse_bool(val) {
                sc.show_toasts = v;
            }
        }
        _ => {}
    }
}

fn parse_state_name(name: &str) -> Option<PowerState> {
    match name {
        "hovered" => Some(PowerState::Hovered),
        "unfocused" => Some(PowerState::Unfocused),
        "invisible" => Some(PowerState::Invisible),
        "unplugged" => Some(PowerState::Unplugged),
        "abandoned" => Some(PowerState::Abandoned),
        _ => None,
    }
}

fn strip_toml_comment(line: &str) -> &str {
    let mut in_quotes = false;
    let mut escaped = false;
    for (idx, ch) in line.char_indices() {
        if in_quotes {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_quotes = false;
            }
        } else if ch == '"' {
            in_quotes = true;
        } else if ch == '#' {
            return &line[..idx];
        }
    }
    line
}

fn unquote(raw: &str) -> &str {
    let trimmed = raw.trim();
    trimmed
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| {
            trimmed
                .strip_prefix('\'')
                .and_then(|s| s.strip_suffix('\''))
        })
        .unwrap_or(trimmed)
}

fn parse_bool(raw: &str) -> Option<bool> {
    match unquote(raw).to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn parse_i32(raw: &str) -> Option<i32> {
    unquote(raw).parse::<i32>().ok()
}

fn parse_f32(raw: &str) -> Option<f32> {
    let v = unquote(raw).parse::<f32>().ok()?;
    v.is_finite().then_some(v)
}

fn parse_volume_speed(raw: &str) -> Option<f32> {
    let v = parse_f32(raw)?;
    if v < 0.0 {
        Some(VOLUME_TRANSITION_IMMEDIATE)
    } else {
        Some(v.clamp(0.05, VOLUME_TRANSITION_IMMEDIATE))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_toml_round_trips_cleanly() {
        let original = DynamicFpsConfig::default();
        let toml = original.to_toml();
        let parsed = DynamicFpsConfig::parse_toml(&toml);
        assert_eq!(parsed, original);
    }

    #[test]
    fn parse_toml_reads_per_source_volume_multipliers_and_clamps_bad_input() {
        let input = r#"
[general]
enabled = false
ignore_initial_click = "in_world"

[idle]
timeout = 120
condition = "on_battery"

[volume_transition]
up = -1.0
down = 0.25

[battery]
enabled = true
critical_level = 99
switch_states = true
notifications = false
show_when_debug = true
display_condition = "draining"
display_placement = "bottom_right"
mock_battery_data = true

[states.unfocused]
frame_rate_target = 15
show_toasts = true

[states.unfocused.volume_multipliers]
master = 0.5
music = 0.0
ambient = 2.5
weather = -0.3
bogus = 0.2

[states.invisible]
frame_rate_target = -1
"#;
        let cfg = DynamicFpsConfig::parse_toml(input);
        assert!(!cfg.enabled);
        assert_eq!(cfg.ignore_initial_click, IgnoreInitialClick::InWorld);
        assert_eq!(cfg.idle.timeout_seconds, 120);
        assert_eq!(cfg.idle.condition, IdleCondition::OnBattery);
        assert_eq!(cfg.volume_transition.up, VOLUME_TRANSITION_IMMEDIATE);
        assert!((cfg.volume_transition.down - 0.25).abs() < 1e-6);
        assert!(cfg.battery.enabled);
        assert_eq!(cfg.battery.critical_level, 50);
        assert!(cfg.battery.switch_states);
        assert!(!cfg.battery.notifications);
        assert!(cfg.battery.show_when_debug);
        assert_eq!(
            cfg.battery.display_condition,
            BatteryIndicatorCondition::Draining
        );
        assert_eq!(
            cfg.battery.display_placement,
            BatteryIndicatorPlacement::BottomRight
        );
        assert!(cfg.battery.mock_battery_data);
        assert_eq!(cfg.unfocused.frame_rate_target, 15);
        assert!(cfg.unfocused.show_toasts);
        assert!((cfg.unfocused.volume_multiplier(SoundSource::Master) - 0.5).abs() < 1e-6);
        assert_eq!(cfg.unfocused.volume_multiplier(SoundSource::Music), 0.0);
        assert_eq!(cfg.unfocused.volume_multiplier(SoundSource::Ambient), 1.0);
        assert_eq!(cfg.unfocused.volume_multiplier(SoundSource::Weather), 0.0);
        assert_eq!(cfg.invisible.frame_rate_target, NO_FRAME_RATE_LIMIT);
    }

    #[test]
    fn master_zero_silences_all_sources() {
        let cfg = DynamicFpsConfig::default();
        assert_eq!(
            cfg.invisible.raw_volume_multiplier(SoundSource::Master),
            0.0
        );
        assert_eq!(cfg.invisible.raw_volume_multiplier(SoundSource::Music), 1.0);
        assert_eq!(cfg.invisible.volume_multiplier(SoundSource::Music), 0.0);
    }

    #[test]
    fn non_minute_and_long_idle_timeouts_round_trip_without_truncation() {
        for timeout_seconds in [45_u32, 90, 3600] {
            let mut original = DynamicFpsConfig::default();
            original.idle.timeout_seconds = timeout_seconds;
            let toml = original.to_toml();
            let parsed = DynamicFpsConfig::parse_toml(&toml);
            assert_eq!(parsed.idle.timeout_seconds, timeout_seconds);
        }
    }

    #[test]
    fn garbage_toml_never_panics_and_falls_back_to_defaults() {
        let cfg = DynamicFpsConfig::parse_toml(
            "[states.unfocused]\nframe_rate_target = NaN\n[broken\n= 123\nvolume = inf\n",
        );
        assert_eq!(cfg, DynamicFpsConfig::default());
    }

    #[test]
    fn config_file_name_is_a_plain_relative_file() {
        let path = std::path::Path::new(CONFIG_FILE_NAME);
        assert!(path.is_relative());
        assert_eq!(path.components().count(), 1);
        let _ = config_spec();
    }

    #[test]
    fn setting_values_round_trip_through_apply_single_setting() {
        let mut original = DynamicFpsConfig::default();
        original.unfocused.frame_rate_target = 17;
        original.battery.critical_level = 20;
        original.idle.timeout_seconds = 600;
        let mut applied = DynamicFpsConfig::default();
        for (key, value) in original.setting_values() {
            assert!(applied.apply_single_setting(key, &value), "{key}");
        }
        assert_eq!(applied.unfocused.frame_rate_target, 17);
        assert_eq!(applied.battery.critical_level, 20);
        assert_eq!(applied.idle.timeout_seconds, 600);
    }

    #[test]
    fn every_row_has_a_setting_value_and_every_value_a_row() {
        let rows: Vec<&str> = TABS
            .iter()
            .flat_map(|tab| tab.rows)
            .map(|row| row.key)
            .collect();
        let values = DynamicFpsConfig::default().setting_values();
        assert_eq!(rows.len(), values.len());
        for (key, _) in values {
            assert!(rows.contains(&key), "{key}");
        }
    }

    #[test]
    fn apply_single_setting_refuses_wrong_kinds_and_non_finite_numbers() {
        let mut cfg = DynamicFpsConfig::default();
        assert!(!cfg.apply_single_setting(SET_ENABLED, &SettingValue::Range(1.0)));
        assert!(!cfg.apply_single_setting(SET_UNFOCUSED_FPS, &SettingValue::Range(f64::NAN)));
        assert!(!cfg.apply_single_setting("missing", &SettingValue::Toggle(true)));
        assert_eq!(cfg, DynamicFpsConfig::default());
    }
}
