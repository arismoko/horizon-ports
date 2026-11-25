use mod_api::{ClientHost, Config, ConfigKind, ConfigPath, Registrar, ServerHost, SettingSpec};
use std::collections::BTreeMap;
use std::sync::{PoisonError, RwLock};

pub const MOD_ID: &str = "pingwheel";
pub const CLIENT_CONFIG_FILE: &str = "pingwheel-client.toml";
pub const SERVER_CONFIG_FILE: &str = "pingwheel-server.toml";

pub const TPS: i64 = 20;
pub const MAX_PING_DURATION: i32 = 60;
pub const MAX_PING_DISTANCE: i32 = 2048;
/// Maximum raycast distance in blocks, matching `ClientConfig.raycastDistance` (1024) in
/// Ping-Wheel (`6447533`).
pub const MAX_RAYCAST_DISTANCE: i32 = 1024;
pub const MAX_CORRECTION_PERIOD: f32 = 5.0;
pub const MAX_CHANNEL_LENGTH: usize = 128;

pub const SETTING_PING_VOLUME: &str = "ping_volume";
pub const SETTING_PING_DURATION: &str = "ping_duration";
pub const SETTING_PING_DISTANCE: &str = "ping_distance";
pub const SETTING_CORRECTION_PERIOD: &str = "correction_period";
pub const SETTING_ITEM_ICON_VISIBLE: &str = "item_icon_visible";
pub const SETTING_DIRECTION_INDICATOR_VISIBLE: &str = "direction_indicator_visible";
pub const SETTING_PLAYER_INFO_MODE: &str = "player_info_mode";
pub const SETTING_TEAM_COLOR_MODE: &str = "team_color_mode";
pub const SETTING_PING_SIZE: &str = "ping_size";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelMode {
    Auto,
    Disabled,
    Global,
    TeamOnly,
}

impl ChannelMode {
    pub const ALL: [Self; 4] = [Self::Auto, Self::Disabled, Self::Global, Self::TeamOnly];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Disabled => "disabled",
            Self::Global => "global",
            Self::TeamOnly => "team_only",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "disabled" => Some(Self::Disabled),
            "global" => Some(Self::Global),
            "team_only" => Some(Self::TeamOnly),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerInfoMode {
    Hold,
    Disabled,
    Always,
    Compact,
}

impl PlayerInfoMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hold => "hold",
            Self::Disabled => "disabled",
            Self::Always => "always",
            Self::Compact => "compact",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Hold => Self::Disabled,
            Self::Disabled => Self::Always,
            Self::Always => Self::Compact,
            Self::Compact => Self::Hold,
        }
    }

    pub fn to_index(self) -> usize {
        match self {
            Self::Hold => 0,
            Self::Disabled => 1,
            Self::Always => 2,
            Self::Compact => 3,
        }
    }

    pub fn from_index(idx: u32) -> Self {
        match idx {
            0 => Self::Hold,
            1 => Self::Disabled,
            2 => Self::Always,
            _ => Self::Compact,
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "hold" => Some(Self::Hold),
            "disabled" => Some(Self::Disabled),
            "always" => Some(Self::Always),
            "compact" => Some(Self::Compact),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeamColorMode {
    Full,
    Disabled,
    PingOnly,
    LabelsOnly,
}

impl TeamColorMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Disabled => "disabled",
            Self::PingOnly => "ping_only",
            Self::LabelsOnly => "labels_only",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Full => Self::Disabled,
            Self::Disabled => Self::PingOnly,
            Self::PingOnly => Self::LabelsOnly,
            Self::LabelsOnly => Self::Full,
        }
    }

    pub fn to_index(self) -> usize {
        match self {
            Self::Full => 0,
            Self::Disabled => 1,
            Self::PingOnly => 2,
            Self::LabelsOnly => 3,
        }
    }

    pub fn from_index(idx: u32) -> Self {
        match idx {
            0 => Self::Full,
            1 => Self::Disabled,
            2 => Self::PingOnly,
            _ => Self::LabelsOnly,
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "full" => Some(Self::Full),
            "disabled" => Some(Self::Disabled),
            "ping_only" => Some(Self::PingOnly),
            "labels_only" => Some(Self::LabelsOnly),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClientConfig {
    pub ping_volume: i32,
    pub ping_duration: i32,
    pub ping_distance: i32,
    pub correction_period: f32,
    pub item_icon_visible: bool,
    pub direction_indicator_visible: bool,
    pub player_info_mode: PlayerInfoMode,
    pub team_color_mode: TeamColorMode,
    pub ping_size: i32,
    pub channel: String,
    pub server_channels: BTreeMap<String, String>,
    // Hidden from settings UI, editable in config file.
    pub remove_radius: i32,
    pub raycast_distance: i32,
    pub safe_zone_left: i32,
    pub safe_zone_right: i32,
    pub safe_zone_top: i32,
    pub safe_zone_bottom: i32,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            ping_volume: 100,
            ping_duration: 7,
            ping_distance: 2048,
            correction_period: 1.0,
            item_icon_visible: true,
            direction_indicator_visible: true,
            player_info_mode: PlayerInfoMode::Hold,
            team_color_mode: TeamColorMode::Full,
            ping_size: 100,
            channel: String::new(),
            server_channels: BTreeMap::new(),
            remove_radius: 10,
            raycast_distance: MAX_RAYCAST_DISTANCE,
            safe_zone_left: 5,
            safe_zone_right: 5,
            safe_zone_top: 5,
            safe_zone_bottom: 60,
        }
    }
}

impl ClientConfig {
    pub fn validate(&mut self) {
        self.ping_volume = self.ping_volume.clamp(0, 100);
        self.ping_duration = self.ping_duration.clamp(1, MAX_PING_DURATION);
        self.ping_distance = self.ping_distance.clamp(0, MAX_PING_DISTANCE);
        self.correction_period = if self.correction_period.is_finite() {
            self.correction_period.clamp(0.1, MAX_CORRECTION_PERIOD)
        } else {
            1.0
        };
        self.ping_size = self.ping_size.clamp(40, 300);
        self.remove_radius = self.remove_radius.max(0);
        self.raycast_distance = self.raycast_distance.clamp(1, MAX_RAYCAST_DISTANCE);
        self.safe_zone_left = self.safe_zone_left.max(0);
        self.safe_zone_right = self.safe_zone_right.max(0);
        self.safe_zone_top = self.safe_zone_top.max(0);
        self.safe_zone_bottom = self.safe_zone_bottom.max(0);
        self.channel = truncate_channel(&self.channel);
        for value in self.server_channels.values_mut() {
            *value = truncate_channel(value);
        }
    }

    pub fn active_channel(&self, server_address: Option<&str>) -> String {
        if let Some(addr) = server_address {
            return self.server_channels.get(addr).cloned().unwrap_or_default();
        }
        self.channel.clone()
    }

    pub fn set_active_channel(&mut self, server_address: Option<&str>, new_channel: &str) {
        let trimmed = truncate_channel(new_channel);
        if let Some(addr) = server_address {
            self.server_channels.insert(addr.to_owned(), trimmed);
        } else {
            self.channel = trimmed;
        }
    }

    pub fn default_toml() -> String {
        Self::default().to_toml()
    }

    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        out.push_str("# Ping Wheel client configuration\n");
        out.push_str(&format!("ping_volume = {}\n", self.ping_volume));
        out.push_str(&format!("ping_duration = {}\n", self.ping_duration));
        out.push_str(&format!("ping_distance = {}\n", self.ping_distance));
        out.push_str(&format!(
            "correction_period = {:.1}\n",
            self.correction_period
        ));
        out.push_str(&format!("item_icon_visible = {}\n", self.item_icon_visible));
        out.push_str(&format!(
            "direction_indicator_visible = {}\n",
            self.direction_indicator_visible
        ));
        out.push_str(&format!(
            "player_info_mode = \"{}\"\n",
            self.player_info_mode.as_str()
        ));
        out.push_str(&format!(
            "team_color_mode = \"{}\"\n",
            self.team_color_mode.as_str()
        ));
        out.push_str(&format!("ping_size = {}\n", self.ping_size));
        out.push_str(&format!(
            "channel = \"{}\"\n",
            escape_toml_string(&self.channel)
        ));
        out.push_str("\n# Advanced / hidden options\n");
        out.push_str(&format!("remove_radius = {}\n", self.remove_radius));
        out.push_str(&format!("raycast_distance = {}\n", self.raycast_distance));
        out.push_str(&format!("safe_zone_left = {}\n", self.safe_zone_left));
        out.push_str(&format!("safe_zone_right = {}\n", self.safe_zone_right));
        out.push_str(&format!("safe_zone_top = {}\n", self.safe_zone_top));
        out.push_str(&format!("safe_zone_bottom = {}\n", self.safe_zone_bottom));
        if !self.server_channels.is_empty() {
            out.push_str("\n[server_channels]\n");
            for (addr, ch) in &self.server_channels {
                out.push_str(&format!(
                    "\"{}\" = \"{}\"\n",
                    escape_toml_string(addr),
                    escape_toml_string(ch)
                ));
            }
        }
        out
    }

    pub fn parse_toml(source: &str) -> Self {
        let mut cfg = Self::default();
        let mut section = "";
        for raw_line in source.lines() {
            let line = strip_toml_comment(raw_line).trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1].trim();
                continue;
            }
            let Some((raw_key, raw_val)) = split_toml_kv(line) else {
                continue;
            };
            let key = unquote_toml_string(raw_key.trim());
            let val = raw_val.trim();
            if section == "server_channels" {
                cfg.server_channels.insert(key, unquote_toml_string(val));
                continue;
            }
            match key.as_str() {
                "ping_volume" => {
                    if let Ok(v) = val.parse::<i32>() {
                        cfg.ping_volume = v;
                    }
                }
                "ping_duration" => {
                    if let Ok(v) = val.parse::<i32>() {
                        cfg.ping_duration = v;
                    }
                }
                "ping_distance" => {
                    if let Ok(v) = val.parse::<i32>() {
                        cfg.ping_distance = v;
                    }
                }
                "correction_period" => {
                    if let Ok(v) = val.parse::<f32>() {
                        cfg.correction_period = v;
                    }
                }
                "item_icon_visible" => {
                    if let Ok(v) = val.parse::<bool>() {
                        cfg.item_icon_visible = v;
                    }
                }
                "direction_indicator_visible" => {
                    if let Ok(v) = val.parse::<bool>() {
                        cfg.direction_indicator_visible = v;
                    }
                }
                "player_info_mode" => {
                    if let Some(v) = PlayerInfoMode::parse(&unquote_toml_string(val)) {
                        cfg.player_info_mode = v;
                    }
                }
                "team_color_mode" => {
                    if let Some(v) = TeamColorMode::parse(&unquote_toml_string(val)) {
                        cfg.team_color_mode = v;
                    }
                }
                "ping_size" => {
                    if let Ok(v) = val.parse::<i32>() {
                        cfg.ping_size = v;
                    }
                }
                "channel" => {
                    cfg.channel = unquote_toml_string(val);
                }
                "remove_radius" => {
                    if let Ok(v) = val.parse::<i32>() {
                        cfg.remove_radius = v;
                    }
                }
                "raycast_distance" => {
                    if let Ok(v) = val.parse::<i32>() {
                        cfg.raycast_distance = v;
                    }
                }
                "safe_zone_left" => {
                    if let Ok(v) = val.parse::<i32>() {
                        cfg.safe_zone_left = v;
                    }
                }
                "safe_zone_right" => {
                    if let Ok(v) = val.parse::<i32>() {
                        cfg.safe_zone_right = v;
                    }
                }
                "safe_zone_top" => {
                    if let Ok(v) = val.parse::<i32>() {
                        cfg.safe_zone_top = v;
                    }
                }
                "safe_zone_bottom" => {
                    if let Ok(v) = val.parse::<i32>() {
                        cfg.safe_zone_bottom = v;
                    }
                }
                _ => {}
            }
        }
        cfg.validate();
        cfg
    }

    pub fn write_to_host_settings(&self, host: &dyn ClientHost) {
        host.set_setting(MOD_ID, SETTING_PING_VOLUME, &self.ping_volume.to_string());
        host.set_setting(
            MOD_ID,
            SETTING_PING_DURATION,
            &self.ping_duration.to_string(),
        );
        host.set_setting(
            MOD_ID,
            SETTING_PING_DISTANCE,
            &self.ping_distance.to_string(),
        );
        host.set_setting(
            MOD_ID,
            SETTING_CORRECTION_PERIOD,
            &format!("{:.1}", self.correction_period),
        );
        host.set_setting(
            MOD_ID,
            SETTING_ITEM_ICON_VISIBLE,
            if self.item_icon_visible {
                "true"
            } else {
                "false"
            },
        );
        host.set_setting(
            MOD_ID,
            SETTING_DIRECTION_INDICATOR_VISIBLE,
            if self.direction_indicator_visible {
                "true"
            } else {
                "false"
            },
        );
        host.set_setting(
            MOD_ID,
            SETTING_PLAYER_INFO_MODE,
            &self.player_info_mode.to_index().to_string(),
        );
        host.set_setting(
            MOD_ID,
            SETTING_TEAM_COLOR_MODE,
            &self.team_color_mode.to_index().to_string(),
        );
        host.set_setting(MOD_ID, SETTING_PING_SIZE, &self.ping_size.to_string());
    }

    pub fn apply_setting_from_host(&mut self, host: &dyn ClientHost, key: &str) {
        match key {
            SETTING_PING_VOLUME => {
                if let Some(v) = host.setting_f64(SETTING_PING_VOLUME) {
                    self.ping_volume = v.round() as i32;
                }
            }
            SETTING_PING_DURATION => {
                if let Some(v) = host.setting_f64(SETTING_PING_DURATION) {
                    self.ping_duration = v.round() as i32;
                }
            }
            SETTING_PING_DISTANCE => {
                if let Some(v) = host.setting_f64(SETTING_PING_DISTANCE) {
                    self.ping_distance = v.round() as i32;
                }
            }
            SETTING_CORRECTION_PERIOD => {
                if let Some(v) = host.setting_f64(SETTING_CORRECTION_PERIOD) {
                    self.correction_period = (v * 10.0).round() as f32 / 10.0;
                }
            }
            SETTING_ITEM_ICON_VISIBLE => {
                if let Some(v) = host.setting_bool(SETTING_ITEM_ICON_VISIBLE) {
                    self.item_icon_visible = v;
                }
            }
            SETTING_DIRECTION_INDICATOR_VISIBLE => {
                if let Some(v) = host.setting_bool(SETTING_DIRECTION_INDICATOR_VISIBLE) {
                    self.direction_indicator_visible = v;
                }
            }
            SETTING_PLAYER_INFO_MODE => {
                if let Some(v) = host.setting_choice(SETTING_PLAYER_INFO_MODE) {
                    self.player_info_mode = PlayerInfoMode::from_index(v);
                }
            }
            SETTING_TEAM_COLOR_MODE => {
                if let Some(v) = host.setting_choice(SETTING_TEAM_COLOR_MODE) {
                    self.team_color_mode = TeamColorMode::from_index(v);
                }
            }
            SETTING_PING_SIZE => {
                if let Some(v) = host.setting_f64(SETTING_PING_SIZE) {
                    self.ping_size = v.round() as i32;
                }
            }
            _ => {}
        }
        self.validate();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerConfig {
    pub default_channel_mode: ChannelMode,
    pub player_tracking_enabled: bool,
    pub ms_to_regenerate: i32,
    pub rate_limit: i32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            default_channel_mode: ChannelMode::Auto,
            player_tracking_enabled: true,
            ms_to_regenerate: 1000,
            rate_limit: 5,
        }
    }
}

impl ServerConfig {
    pub fn validate(&mut self) {
        if self.ms_to_regenerate < 0 {
            self.ms_to_regenerate = 1000;
        }
        if self.rate_limit < 0 {
            self.rate_limit = 0;
        }
    }

    pub fn default_toml() -> &'static str {
        r#"# Ping Wheel server configuration
# Default channel mode: "auto", "disabled", "global", or "team_only".
default_channel_mode = "auto"
# Allow pings to track players as moving entities.
player_tracking_enabled = true
# Milliseconds to regenerate one ping usage.
ms_to_regenerate = 1000
# Maximum burst of pings before rate limiting (0 = disabled).
rate_limit = 5
"#
    }

    pub fn to_toml(&self) -> String {
        format!(
            "# Ping Wheel server configuration\n\
             default_channel_mode = \"{}\"\n\
             player_tracking_enabled = {}\n\
             ms_to_regenerate = {}\n\
             rate_limit = {}\n",
            self.default_channel_mode.as_str(),
            self.player_tracking_enabled,
            self.ms_to_regenerate,
            self.rate_limit
        )
    }

    pub fn parse_toml(source: &str) -> Self {
        let mut cfg = Self::default();
        for raw_line in source.lines() {
            let line = strip_toml_comment(raw_line).trim();
            if line.is_empty() || (line.starts_with('[') && line.ends_with(']')) {
                continue;
            }
            let Some((raw_key, raw_val)) = split_toml_kv(line) else {
                continue;
            };
            let key = unquote_toml_string(raw_key.trim());
            let val = raw_val.trim();
            match key.as_str() {
                "default_channel_mode" => {
                    if let Some(v) = ChannelMode::parse(&unquote_toml_string(val)) {
                        cfg.default_channel_mode = v;
                    }
                }
                "player_tracking_enabled" => {
                    if let Ok(v) = val.parse::<bool>() {
                        cfg.player_tracking_enabled = v;
                    }
                }
                "ms_to_regenerate" => {
                    if let Ok(v) = val.parse::<i32>() {
                        cfg.ms_to_regenerate = v;
                    }
                }
                "rate_limit" => {
                    if let Ok(v) = val.parse::<i32>() {
                        cfg.rate_limit = v;
                    }
                }
                _ => {}
            }
        }
        cfg.validate();
        cfg
    }
}

static CLIENT_CONFIG: RwLock<Option<ClientConfig>> = RwLock::new(None);
static SERVER_CONFIG: RwLock<ServerConfig> = RwLock::new(ServerConfig {
    default_channel_mode: ChannelMode::Auto,
    player_tracking_enabled: true,
    ms_to_regenerate: 1000,
    rate_limit: 5,
});

pub fn client_config() -> ClientConfig {
    CLIENT_CONFIG
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .unwrap_or_default()
}

pub fn set_client_config(mut cfg: ClientConfig) {
    cfg.validate();
    *CLIENT_CONFIG
        .write()
        .unwrap_or_else(PoisonError::into_inner) = Some(cfg);
}

pub fn update_client_config(f: impl FnOnce(&mut ClientConfig)) -> ClientConfig {
    let mut guard = CLIENT_CONFIG
        .write()
        .unwrap_or_else(PoisonError::into_inner);
    let cfg = guard.get_or_insert_with(ClientConfig::default);
    f(cfg);
    cfg.validate();
    cfg.clone()
}

pub fn persist_client_config(host: &dyn ClientHost, cfg: &ClientConfig) {
    if let Ok(path) = ConfigPath::new(CLIENT_CONFIG_FILE) {
        let _ = host.config_files().write(&path, cfg.to_toml().as_bytes());
    }
}

pub fn server_config() -> ServerConfig {
    SERVER_CONFIG
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

pub fn set_server_config(mut cfg: ServerConfig) {
    cfg.validate();
    *SERVER_CONFIG
        .write()
        .unwrap_or_else(PoisonError::into_inner) = cfg;
}

pub fn update_server_config(f: impl FnOnce(&mut ServerConfig)) -> ServerConfig {
    let mut guard = SERVER_CONFIG
        .write()
        .unwrap_or_else(PoisonError::into_inner);
    f(&mut guard);
    guard.validate();
    guard.clone()
}

pub fn persist_server_config(host: &dyn ServerHost, cfg: &ServerConfig) {
    if let Ok(path) = ConfigPath::new(SERVER_CONFIG_FILE) {
        let _ = host.config_files().write(&path, cfg.to_toml().as_bytes());
    }
}

pub fn register_mod_configs(configs: &mut Vec<Config>) {
    let client_default = ClientConfig::default_toml();
    configs.push(Config::new(MOD_ID, ConfigKind::Client, &client_default));
    configs.push(Config::new(
        MOD_ID,
        ConfigKind::Server,
        ServerConfig::default_toml(),
    ));
}

pub fn register_settings(r: &mut dyn Registrar) {
    let _ = r.register_setting(SettingSpec::range(
        SETTING_PING_VOLUME,
        "pingwheel.setting.ping_volume",
        0.0,
        100.0,
        1.0,
        100.0,
    ));
    let _ = r.register_setting(SettingSpec::range(
        SETTING_PING_DURATION,
        "pingwheel.setting.ping_duration",
        1.0,
        f64::from(MAX_PING_DURATION),
        1.0,
        7.0,
    ));
    let _ = r.register_setting(SettingSpec::range(
        SETTING_PING_DISTANCE,
        "pingwheel.setting.ping_distance",
        0.0,
        f64::from(MAX_PING_DISTANCE),
        16.0,
        f64::from(MAX_PING_DISTANCE),
    ));
    let _ = r.register_setting(SettingSpec::range(
        SETTING_CORRECTION_PERIOD,
        "pingwheel.setting.correction_period",
        0.1,
        f64::from(MAX_CORRECTION_PERIOD),
        0.1,
        1.0,
    ));
    let _ = r.register_setting(SettingSpec::toggle(
        SETTING_ITEM_ICON_VISIBLE,
        "settings.pingwheel.item_icon_visible",
        true,
    ));
    let _ = r.register_setting(SettingSpec::toggle(
        SETTING_DIRECTION_INDICATOR_VISIBLE,
        "settings.pingwheel.direction_indicator_visible",
        true,
    ));
    let _ = r.register_setting(SettingSpec::choice(
        SETTING_PLAYER_INFO_MODE,
        "settings.pingwheel.player_info_mode",
        [
            "value.pingwheel.hold",
            "value.pingwheel.disabled",
            "value.pingwheel.always",
            "value.pingwheel.compact",
        ],
        0,
    ));
    let _ = r.register_setting(SettingSpec::choice(
        SETTING_TEAM_COLOR_MODE,
        "settings.pingwheel.team_color_mode",
        [
            "value.pingwheel.full",
            "value.pingwheel.disabled",
            "value.pingwheel.ping_only",
            "value.pingwheel.labels_only",
        ],
        0,
    ));
    let _ = r.register_setting(SettingSpec::range(
        SETTING_PING_SIZE,
        "pingwheel.setting.ping_size",
        40.0,
        300.0,
        10.0,
        100.0,
    ));
}

pub fn truncate_channel(s: &str) -> String {
    s.chars().take(MAX_CHANNEL_LENGTH).collect()
}

fn escape_toml_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

fn unquote_toml_string(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        let inner = &trimmed[1..trimmed.len() - 1];
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                match chars.next() {
                    Some('\\') => out.push('\\'),
                    Some('"') => out.push('"'),
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some('t') => out.push('\t'),
                    Some(other) => {
                        out.push('\\');
                        out.push(other);
                    }
                    None => break,
                }
            } else {
                out.push(ch);
            }
        }
        return out;
    }
    trimmed.to_owned()
}

fn strip_toml_comment(line: &str) -> &str {
    let mut in_quotes = false;
    let mut escaped = false;
    for (idx, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' && in_quotes {
            escaped = true;
            continue;
        }
        if ch == '"' {
            in_quotes = !in_quotes;
            continue;
        }
        if ch == '#' && !in_quotes {
            return &line[..idx];
        }
    }
    line
}

fn split_toml_kv(line: &str) -> Option<(&str, &str)> {
    let mut in_quotes = false;
    let mut escaped = false;
    for (idx, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' && in_quotes {
            escaped = true;
            continue;
        }
        if ch == '"' {
            in_quotes = !in_quotes;
            continue;
        }
        if ch == '=' && !in_quotes {
            return Some((&line[..idx], &line[idx + 1..]));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_config_toml_roundtrip() {
        let mut cfg = ClientConfig {
            ping_volume: 65,
            ping_duration: 15,
            ping_distance: 512,
            correction_period: 2.5,
            item_icon_visible: false,
            direction_indicator_visible: false,
            player_info_mode: PlayerInfoMode::Compact,
            team_color_mode: TeamColorMode::PingOnly,
            ping_size: 140,
            channel: "squad-alpha".to_owned(),
            ..ClientConfig::default()
        };
        cfg.server_channels
            .insert("play.example.com:25565".to_owned(), "raid#1".to_owned());

        let toml = cfg.to_toml();
        let parsed = ClientConfig::parse_toml(&toml);
        assert_eq!(parsed, cfg);
    }

    #[test]
    fn server_config_toml_roundtrip_and_clamping() {
        let cfg = ServerConfig {
            default_channel_mode: ChannelMode::TeamOnly,
            player_tracking_enabled: false,
            ms_to_regenerate: 2500,
            rate_limit: 3,
        };
        let parsed = ServerConfig::parse_toml(&cfg.to_toml());
        assert_eq!(parsed, cfg);

        let bad = ServerConfig::parse_toml("ms_to_regenerate = -5\nrate_limit = -2\n");
        assert_eq!(bad.ms_to_regenerate, 1000);
        assert_eq!(bad.rate_limit, 0);
    }

    #[test]
    fn channel_length_truncates_to_128_chars() {
        let long = "a".repeat(200);
        assert_eq!(truncate_channel(&long).len(), MAX_CHANNEL_LENGTH);
    }
}
