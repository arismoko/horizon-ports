//! Configuration model, TOML persistence, Options > Mods settings, and ping text formatting.

use crate::color::ALPHA_MASK;
use mod_api::{
    ClientHost, ConfigKind, ConfigSpec, Registrar, SettingKey, SettingSpec, SettingValue,
};

pub const MOD_ID: &str = "betterpingdisplay";
pub const CONFIG_FILE_NAME: &str = "betterpingdisplay.toml";

/// Default static ping text colour (`0xA0A0A0`, matching `Config.DEFAULT_PING_TEXT_COLOR`).
pub const DEFAULT_PING_TEXT_COLOR: u32 = 0xA0A0A0;
/// Default ping text format string (`"%dms"`, matching `Config.DEFAULT_PING_TEXT_FORMAT`).
pub const DEFAULT_PING_TEXT_FORMAT: &str = "%dms";

// Options > Mods setting keys.
pub const SET_AUTO_COLOR: &str = "auto_color_ping_text";
pub const SET_RENDER_PING_BARS: &str = "render_ping_bars";
pub const SET_PING_TEXT_COLOR: &str = "ping_text_color";

/// Better Ping Display client configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BetterPingDisplayConfig {
    /// Colour ping text automatically based on latency (green → yellow → red).
    pub auto_color_ping_text: bool,
    /// Render vanilla's ping bars alongside the numeric ping text.
    pub render_ping_bars: bool,
    /// Static `0xRRGGBB` colour used when `auto_color_ping_text` is `false`.
    pub ping_text_color: u32,
    /// Format string containing `%d`, replaced with the player's latency in milliseconds.
    pub ping_text_format_string: String,
}

impl Default for BetterPingDisplayConfig {
    fn default() -> Self {
        Self {
            auto_color_ping_text: true,
            render_ping_bars: false,
            ping_text_color: DEFAULT_PING_TEXT_COLOR,
            ping_text_format_string: DEFAULT_PING_TEXT_FORMAT.to_owned(),
        }
    }
}

impl BetterPingDisplayConfig {
    /// Resolve the opaque ARGB text colour for `latency`.
    pub fn text_color_for(&self, latency: i32) -> u32 {
        if self.auto_color_ping_text {
            crate::color::ping_color(latency)
        } else {
            (self.ping_text_color & 0x00FF_FFFF) | ALPHA_MASK
        }
    }

    /// Format `latency` using `self.ping_text_format_string`.
    pub fn format_latency(&self, latency: i32) -> String {
        format_ping(&self.ping_text_format_string, latency)
    }

    /// Serialize this configuration as `config/betterpingdisplay.toml`.
    pub fn to_toml(&self) -> String {
        let color_hex = format!("#{:06x}", self.ping_text_color & 0x00FF_FFFF);
        let fmt_escaped = escape_toml_string(&self.ping_text_format_string);
        format!(
            "# Better Ping Display configuration\n\
             # Changes are hot-reloaded automatically.\n\
             \n\
             # Whether to color each player's ping based on their latency (green -> yellow -> red).\n\
             auto_color_ping_text = {}\n\
             \n\
             # Whether to also render the default Minecraft ping bars beside the ping number.\n\
             render_ping_bars = {}\n\
             \n\
             # Static RGB hex color used when auto_color_ping_text is false.\n\
             ping_text_color = \"{color_hex}\"\n\
             \n\
             # Format string for ping text. Must contain \"%d\", replaced by the ping value.\n\
             ping_text_format_string = \"{fmt_escaped}\"\n",
            self.auto_color_ping_text, self.render_ping_bars,
        )
    }

    /// Parse `config/betterpingdisplay.toml`, falling back to defaults for missing or invalid fields.
    pub fn parse_toml(input: &str) -> Self {
        let mut config = Self::default();

        for raw_line in input.lines() {
            let line = strip_toml_comment(raw_line).trim();
            if line.is_empty() || line.starts_with('[') {
                continue;
            }
            let Some((raw_key, raw_value)) = line.split_once('=') else {
                continue;
            };
            let key = raw_key.trim();
            let value = raw_value.trim();

            match key {
                "auto_color_ping_text" | "autoColorPingText" => {
                    if let Some(parsed) = parse_toml_bool(value) {
                        config.auto_color_ping_text = parsed;
                    }
                }
                "render_ping_bars" | "renderPingBars" => {
                    if let Some(parsed) = parse_toml_bool(value) {
                        config.render_ping_bars = parsed;
                    }
                }
                "ping_text_color" | "pingTextColor" => {
                    if let Some(parsed) = parse_color(value) {
                        config.ping_text_color = parsed;
                    }
                }
                "ping_text_format_string" | "pingTextFormatString" => {
                    if let Some(parsed) = parse_toml_string(value) {
                        config.ping_text_format_string = validate_format_string(&parsed);
                    }
                }
                _ => {}
            }
        }

        config
    }

    /// Mirror this config into Sunrise's Options > Mods settings store.
    pub fn write_to_host_settings(&self, host: &mut dyn ClientHost, keys: SettingKeys) {
        let _ = host.set_setting(
            keys.auto_color,
            SettingValue::Toggle(self.auto_color_ping_text),
        );
        let _ = host.set_setting(
            keys.render_ping_bars,
            SettingValue::Toggle(self.render_ping_bars),
        );
        let _ = host.set_setting(
            keys.ping_text_color,
            SettingValue::Color(self.ping_text_color & 0x00FF_FFFF),
        );
    }

    /// Apply one changed Options > Mods setting, returning `true` if the value changed.
    pub fn apply_single_setting(&mut self, key: &str, value: SettingValue) -> bool {
        match (key, value) {
            (SET_AUTO_COLOR, SettingValue::Toggle(v)) if v != self.auto_color_ping_text => {
                self.auto_color_ping_text = v;
                true
            }
            (SET_RENDER_PING_BARS, SettingValue::Toggle(v)) if v != self.render_ping_bars => {
                self.render_ping_bars = v;
                true
            }
            (SET_PING_TEXT_COLOR, SettingValue::Color(v)) => {
                let masked = v & 0x00FF_FFFF;
                if masked != self.ping_text_color {
                    self.ping_text_color = masked;
                    return true;
                }
                false
            }
            _ => false,
        }
    }
}

/// Validate a ping format string: must contain `%d`, otherwise falls back to `"%dms"`.
pub fn validate_format_string(raw: &str) -> String {
    if raw.contains("%d") {
        raw.to_owned()
    } else {
        DEFAULT_PING_TEXT_FORMAT.to_owned()
    }
}

/// Format `latency` into `format_str`, replacing `%d` with the decimal value and `%%` with `%`.
///
/// Never panics on malformed format specifiers.
pub fn format_ping(format_str: &str, latency: i32) -> String {
    let latency_str = latency.to_string();
    let mut out = String::with_capacity(format_str.len() + latency_str.len());
    let mut chars = format_str.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '%' {
            match chars.peek().copied() {
                Some('d') => {
                    chars.next();
                    out.push_str(&latency_str);
                }
                Some('%') => {
                    chars.next();
                    out.push('%');
                }
                _ => out.push('%'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// The client config file `config/betterpingdisplay.toml` with its default text.
pub fn config_spec() -> ConfigSpec {
    ConfigSpec::new(
        ConfigKind::Client,
        BetterPingDisplayConfig::default().to_toml(),
    )
    .file(CONFIG_FILE_NAME)
}

/// The owner-bound handles of this mod's Options > Mods settings.
#[derive(Clone, Copy, Debug)]
pub struct SettingKeys {
    pub auto_color: SettingKey,
    pub render_ping_bars: SettingKey,
    pub ping_text_color: SettingKey,
}

const CATEGORY_TEXT: &str = "betterpingdisplay.settings.category.text";
const CATEGORY_BARS: &str = "betterpingdisplay.settings.category.bars";

/// Register the in-game settings on Sunrise's Options > Mods > Better Ping Display page.
/// Returns `None` if the game refused one of them.
pub fn register_settings(r: &mut dyn Registrar) -> Option<SettingKeys> {
    let auto_color = r
        .register_setting(
            SettingSpec::toggle(
                SET_AUTO_COLOR,
                "betterpingdisplay.settings.autoColorPingText",
                true,
            )
            .tooltip("betterpingdisplay.settings.autoColorPingText.description"),
        )
        .ok()?;
    let ping_text_color = r
        .register_setting(
            SettingSpec::color(
                SET_PING_TEXT_COLOR,
                "betterpingdisplay.settings.pingTextColor",
                DEFAULT_PING_TEXT_COLOR,
            )
            .tooltip("betterpingdisplay.settings.pingTextColor.description"),
        )
        .ok()?;
    let render_ping_bars = r
        .register_setting(
            SettingSpec::toggle(
                SET_RENDER_PING_BARS,
                "betterpingdisplay.settings.renderPingBars",
                false,
            )
            .tooltip("betterpingdisplay.settings.renderPingBars.description"),
        )
        .ok()?;
    // A failed grouping only leaves the setting in the flat list.
    for (key, category) in [
        (auto_color, CATEGORY_TEXT),
        (ping_text_color, CATEGORY_TEXT),
        (render_ping_bars, CATEGORY_BARS),
    ] {
        let _ = r.categorize_setting(key, category);
    }
    Some(SettingKeys {
        auto_color,
        render_ping_bars,
        ping_text_color,
    })
}

fn strip_toml_comment(line: &str) -> &str {
    let mut in_double = false;
    let mut in_single = false;
    let mut escaped = false;
    for (idx, ch) in line.char_indices() {
        if in_double {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_double = false;
            }
        } else if in_single {
            if ch == '\'' {
                in_single = false;
            }
        } else {
            match ch {
                '"' => in_double = true,
                '\'' => in_single = true,
                '#' => return &line[..idx],
                _ => {}
            }
        }
    }
    line
}

fn parse_toml_bool(value: &str) -> Option<bool> {
    match value.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn parse_toml_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if let Some(inner) = trimmed
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
    {
        return Some(inner.to_owned());
    }
    let inner = trimmed.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    Some(out)
}

fn escape_toml_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

fn parse_color(value: &str) -> Option<u32> {
    if let Some(s) = parse_toml_string(value) {
        let hex = s
            .trim()
            .strip_prefix('#')
            .or_else(|| s.trim().strip_prefix("0x"))
            .or_else(|| s.trim().strip_prefix("0X"))
            .unwrap_or(s.trim());
        if hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return u32::from_str_radix(hex, 16).ok();
        }
        return None;
    }
    let trimmed = value.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        return u32::from_str_radix(hex, 16).ok().map(|c| c & 0x00FF_FFFF);
    }
    trimmed.parse::<u32>().ok().map(|c| c & 0x00FF_FFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_round_trip_preserves_all_fields() {
        let cfg = BetterPingDisplayConfig {
            auto_color_ping_text: false,
            render_ping_bars: true,
            ping_text_color: 0x12AB34,
            ping_text_format_string: "[%d ms]".to_owned(),
        };
        let parsed = BetterPingDisplayConfig::parse_toml(&cfg.to_toml());
        assert_eq!(parsed, cfg);
    }

    #[test]
    fn invalid_format_string_falls_back_to_default() {
        let parsed = BetterPingDisplayConfig::parse_toml(
            "ping_text_format_string = \"no placeholder here\"\n",
        );
        assert_eq!(parsed.ping_text_format_string, DEFAULT_PING_TEXT_FORMAT);
    }

    #[test]
    fn parse_toml_handles_comments_and_camel_case_aliases() {
        let toml = r##"
            # Fabric-style camelCase keys also parse cleanly
            autoColorPingText = false # inline comment
            renderPingBars = true
            pingTextColor = "#ff8800" # hex hash inside quotes must not be stripped
            pingTextFormatString = "%d ms"
        "##;
        let parsed = BetterPingDisplayConfig::parse_toml(toml);
        assert!(!parsed.auto_color_ping_text);
        assert!(parsed.render_ping_bars);
        assert_eq!(parsed.ping_text_color, 0xFF8800);
        assert_eq!(parsed.ping_text_format_string, "%d ms");
    }

    #[test]
    fn format_ping_handles_standard_and_edge_cases() {
        assert_eq!(format_ping("%dms", 42), "42ms");
        assert_eq!(format_ping("[%d ms]", 0), "[0 ms]");
        assert_eq!(format_ping("%dms", -1), "-1ms");
        assert_eq!(format_ping("%d%%", 50), "50%");
        assert_eq!(format_ping("%%d %d", 7), "%d 7");
        assert_eq!(format_ping("%s %d %", 9), "%s 9 %");
    }

    #[test]
    fn text_color_for_respects_auto_color_toggle() {
        let mut cfg = BetterPingDisplayConfig::default();
        assert_eq!(cfg.text_color_for(0), 0xFF00_E676);

        cfg.auto_color_ping_text = false;
        cfg.ping_text_color = 0x112233;
        assert_eq!(cfg.text_color_for(0), 0xFF11_2233);
        assert_eq!(cfg.text_color_for(999), 0xFF11_2233);
    }
}
