//! Bridging Mod configuration model, TOML persistence, and Options > Mods settings.

use mod_api::{ClientHost, Config, ConfigKind, Registrar, SettingSpec};

pub const MOD_ID: &str = "bridgingmod";
pub const CONFIG_FILE_NAME: &str = "bridgingmod.toml";

// Options > Mods setting keys.
pub const SET_ENABLE_BRIDGING: &str = "enable_bridging_assist";
pub const SET_MIN_BRIDGE_DISTANCE: &str = "min_bridge_distance";
pub const SET_ONLY_WHEN_CROUCHED: &str = "only_bridge_when_crouched";
pub const SET_SUPPORTED_AXES: &str = "supported_bridge_axes";
pub const SET_SUPPORTED_AXES_CROUCHED: &str = "supported_bridge_axes_when_crouched";
pub const SET_DELAY_POST_BRIDGING: &str = "delay_post_bridging";

pub const SET_SHOW_CROSSHAIR: &str = "show_crosshair";
pub const SET_SHOW_OUTLINE: &str = "show_outline";
pub const SET_SHOW_NON_BRIDGING_OUTLINE: &str = "show_outline_even_when_not_bridging";
pub const SET_NON_BRIDGE_RESPECTS_CROUCH: &str = "non_bridge_respects_crouch_rules";
pub const SET_OUTLINE_COLOR: &str = "outline_color";
pub const SET_OUTLINE_ALPHA: &str = "outline_alpha";

pub const SET_SKIP_TORCH_BRIDGING: &str = "skip_torch_bridging";
pub const SET_ENABLE_SLAB_ASSIST: &str = "enable_slab_assist";
pub const SET_ENABLE_NON_SOLID_REPLACE: &str = "enable_non_solid_replace";
pub const SET_SNAP_STRENGTH: &str = "bridging_snap_strength";
pub const SET_ADJACENCY: &str = "bridging_adjacency";
pub const SET_PERSPECTIVE_LOCK: &str = "perspective_lock";

pub const SET_SHOW_DEBUG_HIGHLIGHT: &str = "show_debug_highlight";
pub const SET_SHOW_NON_BRIDGING_DEBUG_HIGHLIGHT: &str = "show_non_bridging_debug_highlight";
pub const SET_SHOW_DEBUG_TRACE: &str = "show_debug_trace";

/// Which axes reach-around bridging can place on (`PlacementAxisMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacementAxisMode {
    Horizontal,
    Vertical,
    Both,
}

impl PlacementAxisMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
            Self::Both => "both",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "horizontal" => Some(Self::Horizontal),
            "vertical" => Some(Self::Vertical),
            "both" => Some(Self::Both),
            _ => None,
        }
    }

    pub const fn from_choice(index: u32) -> Self {
        match index {
            0 => Self::Horizontal,
            1 => Self::Vertical,
            _ => Self::Both,
        }
    }

    pub const fn to_choice(self) -> u32 {
        match self {
            Self::Horizontal => 0,
            Self::Vertical => 1,
            Self::Both => 2,
        }
    }
}

/// Optional override for bridging axes while crouching (`PlacementAxisModeOverride`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacementAxisModeOverride {
    Horizontal,
    Vertical,
    Both,
    Fallback,
}

impl PlacementAxisModeOverride {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
            Self::Both => "both",
            Self::Fallback => "fallback",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "horizontal" => Some(Self::Horizontal),
            "vertical" => Some(Self::Vertical),
            "both" => Some(Self::Both),
            "fallback" => Some(Self::Fallback),
            _ => None,
        }
    }

    pub const fn resolve(self, base: PlacementAxisMode) -> PlacementAxisMode {
        match self {
            Self::Horizontal => PlacementAxisMode::Horizontal,
            Self::Vertical => PlacementAxisMode::Vertical,
            Self::Both => PlacementAxisMode::Both,
            Self::Fallback => base,
        }
    }

    pub const fn from_choice(index: u32) -> Self {
        match index {
            0 => Self::Horizontal,
            1 => Self::Vertical,
            2 => Self::Both,
            _ => Self::Fallback,
        }
    }

    pub const fn to_choice(self) -> u32 {
        match self {
            Self::Horizontal => 0,
            Self::Vertical => 1,
            Self::Both => 2,
            Self::Fallback => 3,
        }
    }
}

/// How Bresenham voxel traversal fills diagonal gaps (`BridgingAdjacency`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BridgingAdjacency {
    Disabled,
    Faces,
    Edges,
    Corners,
}

impl BridgingAdjacency {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Faces => "faces",
            Self::Edges => "edges",
            Self::Corners => "corners",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "disabled" => Some(Self::Disabled),
            "faces" => Some(Self::Faces),
            "edges" => Some(Self::Edges),
            "corners" => Some(Self::Corners),
            _ => None,
        }
    }

    pub const fn from_choice(index: u32) -> Self {
        match index {
            0 => Self::Disabled,
            1 => Self::Faces,
            2 => Self::Edges,
            _ => Self::Corners,
        }
    }

    pub const fn to_choice(self) -> u32 {
        match self {
            Self::Disabled => 0,
            Self::Faces => 1,
            Self::Edges => 2,
            Self::Corners => 3,
        }
    }
}

/// Which camera perspective supplies the view ray (`SourcePerspective`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourcePerspective {
    LetBridgingModDecide,
    CopyTogglePerspective,
    AlwaysEyeline,
}

impl SourcePerspective {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LetBridgingModDecide => "let_bridging_mod_decide",
            Self::CopyTogglePerspective => "copy_toggle_perspective",
            Self::AlwaysEyeline => "always_eyeline",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "let_bridging_mod_decide" => Some(Self::LetBridgingModDecide),
            "copy_toggle_perspective" => Some(Self::CopyTogglePerspective),
            "always_eyeline" => Some(Self::AlwaysEyeline),
            _ => None,
        }
    }

    pub const fn from_choice(index: u32) -> Self {
        match index {
            1 => Self::CopyTogglePerspective,
            2 => Self::AlwaysEyeline,
            _ => Self::LetBridgingModDecide,
        }
    }

    pub const fn to_choice(self) -> u32 {
        match self {
            Self::LetBridgingModDecide => 0,
            Self::CopyTogglePerspective => 1,
            Self::AlwaysEyeline => 2,
        }
    }
}

/// Complete client configuration for Bridging Mod, matching `BridgingConfig`.
#[derive(Clone, Debug, PartialEq)]
pub struct BridgingConfig {
    // Features
    pub enable_bridging_assist: bool,
    pub min_bridge_distance: f32,
    pub only_bridge_when_crouched: bool,
    pub supported_bridge_axes: PlacementAxisMode,
    pub supported_bridge_axes_when_crouched: PlacementAxisModeOverride,
    pub delay_post_bridging: u32,

    // Visuals
    pub show_crosshair: bool,
    pub show_outline: bool,
    pub show_outline_even_when_not_bridging: bool,
    pub non_bridge_respects_crouch_rules: bool,
    pub outline_color_rgb: u32,
    pub outline_alpha: f32,

    // Fixes
    pub skip_torch_bridging: bool,
    pub enable_slab_assist: bool,
    pub enable_non_solid_replace: bool,
    pub bridging_snap_strength: f32,
    pub bridging_adjacency: BridgingAdjacency,
    pub perspective_lock: SourcePerspective,

    // Debug
    pub show_debug_highlight: bool,
    pub show_non_bridging_debug_highlight: bool,
    pub show_debug_trace: bool,
}

impl Default for BridgingConfig {
    fn default() -> Self {
        Self {
            enable_bridging_assist: true,
            min_bridge_distance: 20.0,
            only_bridge_when_crouched: false,
            supported_bridge_axes: PlacementAxisMode::Both,
            supported_bridge_axes_when_crouched: PlacementAxisModeOverride::Fallback,
            delay_post_bridging: 4,

            show_crosshair: true,
            show_outline: true,
            show_outline_even_when_not_bridging: false,
            non_bridge_respects_crouch_rules: true,
            outline_color_rgb: 0x00_0000,
            outline_alpha: 0.4,

            skip_torch_bridging: true,
            enable_slab_assist: true,
            enable_non_solid_replace: true,
            bridging_snap_strength: 1.0,
            bridging_adjacency: BridgingAdjacency::Corners,
            perspective_lock: SourcePerspective::LetBridgingModDecide,

            show_debug_highlight: true,
            show_non_bridging_debug_highlight: false,
            show_debug_trace: false,
        }
    }
}

impl BridgingConfig {
    /// Packed `0xAARRGGBB` outline colour.
    pub fn outline_argb(&self) -> u32 {
        let a = (self.outline_alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
        (a << 24) | (self.outline_color_rgb & 0x00FF_FFFF)
    }

    /// Resolve effective perspective lock, matching `BridgingMod.getCompatibleSourcePerspective`.
    pub fn effective_perspective(&self, freelook_loaded: bool) -> SourcePerspective {
        match self.perspective_lock {
            SourcePerspective::LetBridgingModDecide => {
                if freelook_loaded {
                    SourcePerspective::AlwaysEyeline
                } else {
                    SourcePerspective::CopyTogglePerspective
                }
            }
            other => other,
        }
    }

    /// Serialize to TOML for `config/bridgingmod.toml`.
    pub fn to_toml(&self) -> String {
        format!(
            "# Bridging Mod client configuration\n\
\n\
[feature]\n\
enable_bridging_assist = {}\n\
min_bridge_distance = {:.2}\n\
only_bridge_when_crouched = {}\n\
supported_bridge_axes = \"{}\"\n\
supported_bridge_axes_when_crouched = \"{}\"\n\
delay_post_bridging = {}\n\
\n\
[vfx]\n\
show_crosshair = {}\n\
show_outline = {}\n\
show_outline_even_when_not_bridging = {}\n\
non_bridge_respects_crouch_rules = {}\n\
outline_color = \"#{:06x}\"\n\
outline_alpha = {:.2}\n\
\n\
[fixes]\n\
skip_torch_bridging = {}\n\
enable_slab_assist = {}\n\
enable_non_solid_replace = {}\n\
bridging_snap_strength = {:.2}\n\
bridging_adjacency = \"{}\"\n\
perspective_lock = \"{}\"\n\
\n\
[debug]\n\
show_debug_highlight = {}\n\
show_non_bridging_debug_highlight = {}\n\
show_debug_trace = {}\n",
            self.enable_bridging_assist,
            self.min_bridge_distance,
            self.only_bridge_when_crouched,
            self.supported_bridge_axes.as_str(),
            self.supported_bridge_axes_when_crouched.as_str(),
            self.delay_post_bridging,
            self.show_crosshair,
            self.show_outline,
            self.show_outline_even_when_not_bridging,
            self.non_bridge_respects_crouch_rules,
            self.outline_color_rgb & 0x00FF_FFFF,
            self.outline_alpha,
            self.skip_torch_bridging,
            self.enable_slab_assist,
            self.enable_non_solid_replace,
            self.bridging_snap_strength,
            self.bridging_adjacency.as_str(),
            self.perspective_lock.as_str(),
            self.show_debug_highlight,
            self.show_non_bridging_debug_highlight,
            self.show_debug_trace,
        )
    }

    /// Parse `config/bridgingmod.toml`; malformed lines fall back to defaults without panicking.
    pub fn parse_toml(text: &str) -> Self {
        let mut cfg = Self::default();
        let mut section = String::new();

        for raw_line in text.lines() {
            let line = strip_comment(raw_line).trim();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix('[')
                && let Some(name) = rest.strip_suffix(']')
            {
                section = name.trim().to_ascii_lowercase();
                continue;
            }
            let Some((raw_key, raw_val)) = line.split_once('=') else {
                continue;
            };
            let key = raw_key.trim().to_ascii_lowercase();
            let val = raw_val.trim();

            match section.as_str() {
                "" | "feature" => match key.as_str() {
                    "enable_bridging_assist" => {
                        if let Some(v) = parse_bool(val) {
                            cfg.enable_bridging_assist = v;
                        }
                    }
                    "min_bridge_distance" => {
                        if let Some(v) = parse_f32(val) {
                            cfg.min_bridge_distance = v.clamp(0.0, 100.0);
                        }
                    }
                    "only_bridge_when_crouched" => {
                        if let Some(v) = parse_bool(val) {
                            cfg.only_bridge_when_crouched = v;
                        }
                    }
                    "supported_bridge_axes" => {
                        if let Some(v) = PlacementAxisMode::parse(unquote(val)) {
                            cfg.supported_bridge_axes = v;
                        }
                    }
                    "supported_bridge_axes_when_crouched" => {
                        if let Some(v) = PlacementAxisModeOverride::parse(unquote(val)) {
                            cfg.supported_bridge_axes_when_crouched = v;
                        }
                    }
                    "delay_post_bridging" => {
                        if let Some(v) = parse_u32(val) {
                            cfg.delay_post_bridging = v.min(20);
                        }
                    }
                    _ => {}
                },
                "vfx" => match key.as_str() {
                    "show_crosshair" => {
                        if let Some(v) = parse_bool(val) {
                            cfg.show_crosshair = v;
                        }
                    }
                    "show_outline" => {
                        if let Some(v) = parse_bool(val) {
                            cfg.show_outline = v;
                        }
                    }
                    "show_outline_even_when_not_bridging" => {
                        if let Some(v) = parse_bool(val) {
                            cfg.show_outline_even_when_not_bridging = v;
                        }
                    }
                    "non_bridge_respects_crouch_rules" => {
                        if let Some(v) = parse_bool(val) {
                            cfg.non_bridge_respects_crouch_rules = v;
                        }
                    }
                    "outline_color" => {
                        if let Some(rgb) = parse_hex_rgb(unquote(val)) {
                            cfg.outline_color_rgb = rgb;
                        }
                    }
                    "outline_alpha" => {
                        if let Some(v) = parse_f32(val) {
                            cfg.outline_alpha = v.clamp(0.0, 1.0);
                        }
                    }
                    _ => {}
                },
                "fixes" => match key.as_str() {
                    "skip_torch_bridging" => {
                        if let Some(v) = parse_bool(val) {
                            cfg.skip_torch_bridging = v;
                        }
                    }
                    "enable_slab_assist" => {
                        if let Some(v) = parse_bool(val) {
                            cfg.enable_slab_assist = v;
                        }
                    }
                    "enable_non_solid_replace" => {
                        if let Some(v) = parse_bool(val) {
                            cfg.enable_non_solid_replace = v;
                        }
                    }
                    "bridging_snap_strength" => {
                        if let Some(v) = parse_f32(val) {
                            cfg.bridging_snap_strength = v.clamp(0.0, 1.0);
                        }
                    }
                    "bridging_adjacency" => {
                        if let Some(v) = BridgingAdjacency::parse(unquote(val)) {
                            cfg.bridging_adjacency = v;
                        }
                    }
                    "perspective_lock" => {
                        if let Some(v) = SourcePerspective::parse(unquote(val)) {
                            cfg.perspective_lock = v;
                        }
                    }
                    _ => {}
                },
                "debug" => match key.as_str() {
                    "show_debug_highlight" => {
                        if let Some(v) = parse_bool(val) {
                            cfg.show_debug_highlight = v;
                        }
                    }
                    "show_non_bridging_debug_highlight" => {
                        if let Some(v) = parse_bool(val) {
                            cfg.show_non_bridging_debug_highlight = v;
                        }
                    }
                    "show_debug_trace" => {
                        if let Some(v) = parse_bool(val) {
                            cfg.show_debug_trace = v;
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        cfg
    }

    /// Apply a single Options > Mods `setting_changed` key onto `self`.
    pub fn apply_single_setting(&mut self, host: &dyn ClientHost, key: &str) -> bool {
        match key {
            SET_ENABLE_BRIDGING => {
                if let Some(v) = host.setting_bool(key) {
                    self.enable_bridging_assist = v;
                    return true;
                }
            }
            SET_MIN_BRIDGE_DISTANCE => {
                if let Some(v) = host.setting_f64(key)
                    && v.is_finite()
                {
                    self.min_bridge_distance = (v as f32).clamp(0.0, 100.0);
                    return true;
                }
            }
            SET_ONLY_WHEN_CROUCHED => {
                if let Some(v) = host.setting_bool(key) {
                    self.only_bridge_when_crouched = v;
                    return true;
                }
            }
            SET_SUPPORTED_AXES => {
                if let Some(v) = host.setting_choice(key) {
                    self.supported_bridge_axes = PlacementAxisMode::from_choice(v);
                    return true;
                }
            }
            SET_SUPPORTED_AXES_CROUCHED => {
                if let Some(v) = host.setting_choice(key) {
                    self.supported_bridge_axes_when_crouched =
                        PlacementAxisModeOverride::from_choice(v);
                    return true;
                }
            }
            SET_DELAY_POST_BRIDGING => {
                if let Some(v) = host.setting_f64(key)
                    && v.is_finite()
                {
                    self.delay_post_bridging = v.round().clamp(0.0, 20.0) as u32;
                    return true;
                }
            }
            SET_SHOW_CROSSHAIR => {
                if let Some(v) = host.setting_bool(key) {
                    self.show_crosshair = v;
                    return true;
                }
            }
            SET_SHOW_OUTLINE => {
                if let Some(v) = host.setting_bool(key) {
                    self.show_outline = v;
                    return true;
                }
            }
            SET_SHOW_NON_BRIDGING_OUTLINE => {
                if let Some(v) = host.setting_bool(key) {
                    self.show_outline_even_when_not_bridging = v;
                    return true;
                }
            }
            SET_NON_BRIDGE_RESPECTS_CROUCH => {
                if let Some(v) = host.setting_bool(key) {
                    self.non_bridge_respects_crouch_rules = v;
                    return true;
                }
            }
            SET_OUTLINE_COLOR => {
                if let Some(v) = host.setting_color(key) {
                    self.outline_color_rgb = v & 0x00FF_FFFF;
                    return true;
                }
            }
            SET_OUTLINE_ALPHA => {
                if let Some(v) = host.setting_f64(key)
                    && v.is_finite()
                {
                    self.outline_alpha = (v as f32).clamp(0.0, 1.0);
                    return true;
                }
            }
            SET_SKIP_TORCH_BRIDGING => {
                if let Some(v) = host.setting_bool(key) {
                    self.skip_torch_bridging = v;
                    return true;
                }
            }
            SET_ENABLE_SLAB_ASSIST => {
                if let Some(v) = host.setting_bool(key) {
                    self.enable_slab_assist = v;
                    return true;
                }
            }
            SET_ENABLE_NON_SOLID_REPLACE => {
                if let Some(v) = host.setting_bool(key) {
                    self.enable_non_solid_replace = v;
                    return true;
                }
            }
            SET_SNAP_STRENGTH => {
                if let Some(v) = host.setting_f64(key)
                    && v.is_finite()
                {
                    self.bridging_snap_strength = (v as f32).clamp(0.0, 1.0);
                    return true;
                }
            }
            SET_ADJACENCY => {
                if let Some(v) = host.setting_choice(key) {
                    self.bridging_adjacency = BridgingAdjacency::from_choice(v);
                    return true;
                }
            }
            SET_PERSPECTIVE_LOCK => {
                if let Some(v) = host.setting_choice(key) {
                    self.perspective_lock = SourcePerspective::from_choice(v);
                    return true;
                }
            }
            SET_SHOW_DEBUG_HIGHLIGHT => {
                if let Some(v) = host.setting_bool(key) {
                    self.show_debug_highlight = v;
                    return true;
                }
            }
            SET_SHOW_NON_BRIDGING_DEBUG_HIGHLIGHT => {
                if let Some(v) = host.setting_bool(key) {
                    self.show_non_bridging_debug_highlight = v;
                    return true;
                }
            }
            SET_SHOW_DEBUG_TRACE => {
                if let Some(v) = host.setting_bool(key) {
                    self.show_debug_trace = v;
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    /// Mirror all config fields into Sunrise's Options > Mods settings store.
    pub fn write_to_host_settings(&self, host: &dyn ClientHost) {
        let set_bool = |k: &str, v: bool| {
            host.set_setting(MOD_ID, k, if v { "true" } else { "false" });
        };
        let set_u32 = |k: &str, v: u32| {
            host.set_setting(MOD_ID, k, &v.to_string());
        };
        let set_f64 = |k: &str, v: f64| {
            host.set_setting(MOD_ID, k, &v.to_string());
        };
        let set_color = |k: &str, rgb: u32| {
            let masked = rgb & 0x00FF_FFFF;
            host.set_setting(MOD_ID, k, &format!("#{masked:06x}"));
        };

        set_bool(SET_ENABLE_BRIDGING, self.enable_bridging_assist);
        set_f64(SET_MIN_BRIDGE_DISTANCE, f64::from(self.min_bridge_distance));
        set_bool(SET_ONLY_WHEN_CROUCHED, self.only_bridge_when_crouched);
        set_u32(SET_SUPPORTED_AXES, self.supported_bridge_axes.to_choice());
        set_u32(
            SET_SUPPORTED_AXES_CROUCHED,
            self.supported_bridge_axes_when_crouched.to_choice(),
        );
        set_f64(SET_DELAY_POST_BRIDGING, f64::from(self.delay_post_bridging));

        set_bool(SET_SHOW_CROSSHAIR, self.show_crosshair);
        set_bool(SET_SHOW_OUTLINE, self.show_outline);
        set_bool(
            SET_SHOW_NON_BRIDGING_OUTLINE,
            self.show_outline_even_when_not_bridging,
        );
        set_bool(
            SET_NON_BRIDGE_RESPECTS_CROUCH,
            self.non_bridge_respects_crouch_rules,
        );
        set_color(SET_OUTLINE_COLOR, self.outline_color_rgb);
        set_f64(SET_OUTLINE_ALPHA, f64::from(self.outline_alpha));

        set_bool(SET_SKIP_TORCH_BRIDGING, self.skip_torch_bridging);
        set_bool(SET_ENABLE_SLAB_ASSIST, self.enable_slab_assist);
        set_bool(SET_ENABLE_NON_SOLID_REPLACE, self.enable_non_solid_replace);
        set_f64(SET_SNAP_STRENGTH, f64::from(self.bridging_snap_strength));
        set_u32(SET_ADJACENCY, self.bridging_adjacency.to_choice());
        set_u32(SET_PERSPECTIVE_LOCK, self.perspective_lock.to_choice());

        set_bool(SET_SHOW_DEBUG_HIGHLIGHT, self.show_debug_highlight);
        set_bool(
            SET_SHOW_NON_BRIDGING_DEBUG_HIGHLIGHT,
            self.show_non_bridging_debug_highlight,
        );
        set_bool(SET_SHOW_DEBUG_TRACE, self.show_debug_trace);
    }
}

/// Register the client config file `config/bridgingmod.toml`.
pub fn register_mod_configs(configs: &mut Vec<Config>) {
    let default_toml = BridgingConfig::default().to_toml();
    configs.push(Config::new(MOD_ID, ConfigKind::Client, &default_toml).file(CONFIG_FILE_NAME));
}

/// Register all in-game settings on Sunrise's Options > Mods > Bridging Mod page.
pub fn register_settings(r: &mut dyn Registrar) {
    let specs = [
        // Features
        SettingSpec::toggle(
            SET_ENABLE_BRIDGING,
            "config.bridgingmod.option.enableBridgingAssist",
            true,
        )
        .tooltip("config.bridgingmod.option.enableBridgingAssist.description.0"),
        SettingSpec::range(
            SET_MIN_BRIDGE_DISTANCE,
            "config.bridgingmod.option.minBridgeDistance",
            0.0,
            100.0,
            0.5,
            20.0,
        )
        .tooltip("config.bridgingmod.option.minBridgeDistance.description.0"),
        SettingSpec::toggle(
            SET_ONLY_WHEN_CROUCHED,
            "config.bridgingmod.option.onlyBridgeWhenCrouched",
            false,
        )
        .tooltip("config.bridgingmod.option.onlyBridgeWhenCrouched.description.0"),
        SettingSpec::choice(
            SET_SUPPORTED_AXES,
            "config.bridgingmod.option.supportedBridgeAxes",
            [
                "enum.bridgingmod.placement_axis.horizontal",
                "enum.bridgingmod.placement_axis.vertical",
                "enum.bridgingmod.placement_axis.both",
            ],
            2,
        )
        .tooltip("config.bridgingmod.option.supportedBridgeAxes.description.0"),
        SettingSpec::choice(
            SET_SUPPORTED_AXES_CROUCHED,
            "config.bridgingmod.option.supportedBridgeAxesWhenCrouched",
            [
                "enum.bridgingmod.placement_axis.horizontal",
                "enum.bridgingmod.placement_axis.vertical",
                "enum.bridgingmod.placement_axis.both",
                "enum.bridgingmod.placement_axis.fallback",
            ],
            3,
        )
        .tooltip("config.bridgingmod.option.supportedBridgeAxesWhenCrouched.description.0"),
        SettingSpec::range(
            SET_DELAY_POST_BRIDGING,
            "config.bridgingmod.option.delayPostBridging",
            0.0,
            20.0,
            1.0,
            4.0,
        )
        .tooltip("config.bridgingmod.option.delayPostBridging.description.0"),
        // Visuals
        SettingSpec::toggle(
            SET_SHOW_CROSSHAIR,
            "config.bridgingmod.option.showCrosshair",
            true,
        )
        .tooltip("config.bridgingmod.option.showCrosshair.description.0"),
        SettingSpec::toggle(
            SET_SHOW_OUTLINE,
            "config.bridgingmod.option.showOutline",
            true,
        )
        .tooltip("config.bridgingmod.option.showOutline.description.0"),
        SettingSpec::toggle(
            SET_SHOW_NON_BRIDGING_OUTLINE,
            "config.bridgingmod.option.showOutlineEvenWhenNotBridging",
            false,
        )
        .tooltip("config.bridgingmod.option.showOutlineEvenWhenNotBridging.description.0"),
        SettingSpec::toggle(
            SET_NON_BRIDGE_RESPECTS_CROUCH,
            "config.bridgingmod.option.nonBridgeRespectsCrouchRules",
            true,
        )
        .tooltip("config.bridgingmod.option.nonBridgeRespectsCrouchRules.description.0"),
        SettingSpec::color(
            SET_OUTLINE_COLOR,
            "config.bridgingmod.option.outlineColour",
            0x00_0000,
        )
        .tooltip("config.bridgingmod.option.outlineColour.description.0"),
        SettingSpec::range(
            SET_OUTLINE_ALPHA,
            "config.bridgingmod.option.outlineAlpha",
            0.0,
            1.0,
            0.05,
            0.4,
        )
        .tooltip("config.bridgingmod.option.outlineAlpha.description.0"),
        // Fixes
        SettingSpec::toggle(
            SET_SKIP_TORCH_BRIDGING,
            "config.bridgingmod.option.skipTorchBridging",
            true,
        )
        .tooltip("config.bridgingmod.option.skipTorchBridging.description.0"),
        SettingSpec::toggle(
            SET_ENABLE_SLAB_ASSIST,
            "config.bridgingmod.option.enableSlabAssist",
            true,
        )
        .tooltip("config.bridgingmod.option.enableSlabAssist.description.0"),
        SettingSpec::toggle(
            SET_ENABLE_NON_SOLID_REPLACE,
            "config.bridgingmod.option.enableNonSolidReplace",
            true,
        )
        .tooltip("config.bridgingmod.option.enableNonSolidReplace.description.0"),
        SettingSpec::range(
            SET_SNAP_STRENGTH,
            "config.bridgingmod.option.bridgingSnapStrength",
            0.0,
            1.0,
            0.05,
            1.0,
        )
        .tooltip("config.bridgingmod.option.bridgingSnapStrength.description.0"),
        SettingSpec::choice(
            SET_ADJACENCY,
            "config.bridgingmod.option.bridgingAdjacency",
            [
                "enum.bridgingmod.bridging_adjacency.disabled",
                "enum.bridgingmod.bridging_adjacency.faces",
                "enum.bridgingmod.bridging_adjacency.edges",
                "enum.bridgingmod.bridging_adjacency.corners",
            ],
            3,
        )
        .tooltip("config.bridgingmod.option.bridgingAdjacency.description.0"),
        SettingSpec::choice(
            SET_PERSPECTIVE_LOCK,
            "config.bridgingmod.option.perspectiveLock",
            [
                "enum.bridgingmod.source_perspective.let_bridging_mod_decide",
                "enum.bridgingmod.source_perspective.copy_toggle_perspective",
                "enum.bridgingmod.source_perspective.always_eyeline",
            ],
            0,
        )
        .tooltip("config.bridgingmod.option.perspectiveLock.description.0"),
        // Debug
        SettingSpec::toggle(
            SET_SHOW_DEBUG_HIGHLIGHT,
            "config.bridgingmod.option.showDebugHighlight",
            true,
        )
        .tooltip("config.bridgingmod.option.showDebugHighlight.description.0"),
        SettingSpec::toggle(
            SET_SHOW_NON_BRIDGING_DEBUG_HIGHLIGHT,
            "config.bridgingmod.option.showNonBridgingDebugHighlight",
            false,
        )
        .tooltip("config.bridgingmod.option.showNonBridgingDebugHighlight.description.0"),
        SettingSpec::toggle(
            SET_SHOW_DEBUG_TRACE,
            "config.bridgingmod.option.showDebugTrace",
            false,
        )
        .tooltip("config.bridgingmod.option.showDebugTrace.description.0"),
    ];

    for spec in specs {
        let _ = r.register_setting(spec);
    }
}

fn strip_comment(line: &str) -> &str {
    let mut in_quotes = false;
    for (idx, ch) in line.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '#' if !in_quotes => return &line[..idx],
            _ => {}
        }
    }
    line
}

fn unquote(raw: &str) -> &str {
    let trimmed = raw.trim();
    trimmed
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(trimmed)
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn parse_u32(raw: &str) -> Option<u32> {
    raw.trim().parse::<u32>().ok()
}

fn parse_f32(raw: &str) -> Option<f32> {
    raw.trim().parse::<f32>().ok().filter(|v| v.is_finite())
}

fn parse_hex_rgb(raw: &str) -> Option<u32> {
    let hex = raw.trim().strip_prefix('#').unwrap_or(raw.trim());
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_round_trip_preserves_custom_values() {
        let custom = BridgingConfig {
            enable_bridging_assist: false,
            min_bridge_distance: 35.5,
            only_bridge_when_crouched: true,
            supported_bridge_axes: PlacementAxisMode::Horizontal,
            supported_bridge_axes_when_crouched: PlacementAxisModeOverride::Vertical,
            delay_post_bridging: 8,
            show_crosshair: false,
            show_outline: false,
            show_outline_even_when_not_bridging: true,
            non_bridge_respects_crouch_rules: false,
            outline_color_rgb: 0x33_99FF,
            outline_alpha: 0.75,
            skip_torch_bridging: false,
            enable_slab_assist: false,
            enable_non_solid_replace: false,
            bridging_snap_strength: 0.8,
            bridging_adjacency: BridgingAdjacency::Edges,
            perspective_lock: SourcePerspective::AlwaysEyeline,
            show_debug_highlight: false,
            show_non_bridging_debug_highlight: true,
            show_debug_trace: true,
        };
        let parsed = BridgingConfig::parse_toml(&custom.to_toml());
        assert_eq!(parsed, custom);
    }

    #[test]
    fn garbage_toml_falls_back_to_defaults_without_panicking() {
        let cfg = BridgingConfig::parse_toml(
            "[feature]\nmin_bridge_distance = NaN\ndelay_post_bridging = -5\n[vfx]\noutline_color = \"zzzz\"\n[broken\n= 123\n",
        );
        assert_eq!(cfg, BridgingConfig::default());
    }

    #[test]
    fn outline_argb_packs_alpha_and_rgb() {
        let cfg = BridgingConfig {
            outline_color_rgb: 0x11_2233,
            outline_alpha: 0.4,
            ..BridgingConfig::default()
        };
        assert_eq!(cfg.outline_argb(), 0x6611_2233);
    }

    #[test]
    fn register_mod_configs_targets_single_owned_client_file() {
        let mut configs = Vec::new();
        register_mod_configs(&mut configs);
        assert_eq!(configs.len(), 1);
        let registered = &configs[0];
        assert_eq!(registered.mod_id, MOD_ID);
        assert_eq!(registered.kind, ConfigKind::Client);
        assert_eq!(registered.file, CONFIG_FILE_NAME);
        assert!(mod_api::ConfigPath::new(&registered.file).is_ok_and(|path| path.owned_by(MOD_ID)));
    }
}
