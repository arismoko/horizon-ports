use mod_api::{
    ClientHost, ConfigKind, ConfigSpec, Host, Identifier, Registrar, SettingKey, SettingSpec,
    SettingValue, Wire,
};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, RwLock};

pub const MOD_ID: &str = "badwithernocookie";
pub const LEGACY_NAMESPACE: &str = "bwncr";
pub const CONFIG_FILE_NAME: &str = "badwithernocookie-common.toml";

pub const KEY_SILENCE_WITHER: &str = "silenceWither";
pub const KEY_SILENCE_TRADER: &str = "silenceTrader";
pub const KEY_SILENCE_DRAGON: &str = "silenceDragon";
pub const KEY_SILENCE_LIGHTNING: &str = "silenceLightning";
pub const KEY_SILENCE_EXPLOSION: &str = "silenceExplosion";
pub const KEY_DEBUG_MODE: &str = "debugMode";

/// Snapshot of all seven Bad Wither No Cookie - Reloaded config options.
#[derive(Clone, Debug, PartialEq, Eq, Wire)]
pub struct SilenceSettings {
    pub silence_wither: bool,
    pub silence_trader: bool,
    pub silence_dragon: bool,
    pub silence_lightning: bool,
    pub silence_explosion: bool,
    pub debug_mode: bool,
    pub silence_us: Vec<String>,
}

impl Default for SilenceSettings {
    fn default() -> Self {
        Self {
            silence_wither: true,
            silence_trader: true,
            silence_dragon: true,
            silence_lightning: true,
            silence_explosion: false,
            debug_mode: false,
            silence_us: Vec::new(),
        }
    }
}

static LOCAL_SETTINGS: LazyLock<RwLock<SilenceSettings>> =
    LazyLock::new(|| RwLock::new(SilenceSettings::default()));
static REMOTE_SETTINGS: LazyLock<RwLock<Option<SilenceSettings>>> =
    LazyLock::new(|| RwLock::new(None));

pub fn local_settings() -> SilenceSettings {
    LOCAL_SETTINGS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

pub fn set_local_settings(settings: SilenceSettings) {
    *LOCAL_SETTINGS.write().unwrap_or_else(|e| e.into_inner()) = settings;
}

pub fn update_local_settings<R>(f: impl FnOnce(&mut SilenceSettings) -> R) -> R {
    let mut guard = LOCAL_SETTINGS.write().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

pub fn remote_settings() -> Option<SilenceSettings> {
    REMOTE_SETTINGS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

pub fn set_remote_settings(settings: Option<SilenceSettings>) {
    *REMOTE_SETTINGS.write().unwrap_or_else(|e| e.into_inner()) = settings;
}

impl SilenceSettings {
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        match key {
            KEY_SILENCE_WITHER => Some(self.silence_wither),
            KEY_SILENCE_TRADER => Some(self.silence_trader),
            KEY_SILENCE_DRAGON => Some(self.silence_dragon),
            KEY_SILENCE_LIGHTNING => Some(self.silence_lightning),
            KEY_SILENCE_EXPLOSION => Some(self.silence_explosion),
            KEY_DEBUG_MODE => Some(self.debug_mode),
            _ => None,
        }
    }

    pub fn set_bool(&mut self, key: &str, value: bool) -> bool {
        match key {
            KEY_SILENCE_WITHER => self.silence_wither = value,
            KEY_SILENCE_TRADER => self.silence_trader = value,
            KEY_SILENCE_DRAGON => self.silence_dragon = value,
            KEY_SILENCE_LIGHTNING => self.silence_lightning = value,
            KEY_SILENCE_EXPLOSION => self.silence_explosion = value,
            KEY_DEBUG_MODE => self.debug_mode = value,
            _ => return false,
        }
        true
    }

    pub fn set_all_silences(&mut self, value: bool) {
        self.silence_wither = value;
        self.silence_trader = value;
        self.silence_dragon = value;
        self.silence_lightning = value;
        self.silence_explosion = value;
    }

    /// Parse `badwithernocookie-common.toml` (or a NeoForge `bwncr-common.toml` layout).
    /// Malformed lines are skipped; missing keys keep their defaults.
    pub fn parse_toml(text: &str) -> Self {
        let mut settings = Self::default();
        let mut in_general = true;
        let mut pending_array: Option<String> = None;

        for raw_line in text.lines() {
            let line = strip_toml_comment(raw_line).trim();
            if let Some(buf) = pending_array.as_mut() {
                if !line.is_empty() {
                    buf.push(' ');
                    buf.push_str(line);
                }
                if contains_unquoted_char(line, ']') {
                    let completed = pending_array.take().unwrap_or_default();
                    if let Some(entries) = parse_toml_string_array(&completed) {
                        settings.silence_us = entries;
                    }
                }
                continue;
            }
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                let section = line[1..line.len() - 1].trim();
                in_general = section.eq_ignore_ascii_case("general");
                continue;
            }
            if !in_general {
                continue;
            }
            let Some((raw_key, raw_val)) = line.split_once('=') else {
                continue;
            };
            let key = raw_key.trim();
            let val = raw_val.trim();
            match key {
                KEY_SILENCE_WITHER
                | KEY_SILENCE_TRADER
                | KEY_SILENCE_DRAGON
                | KEY_SILENCE_LIGHTNING
                | KEY_SILENCE_EXPLOSION
                | KEY_DEBUG_MODE => {
                    if let Some(parsed) = parse_toml_bool(val) {
                        settings.set_bool(key, parsed);
                    }
                }
                "silenceUs" => {
                    if val.starts_with('[') {
                        if contains_unquoted_char(val, ']') {
                            if let Some(entries) = parse_toml_string_array(val) {
                                settings.silence_us = entries;
                            }
                        } else {
                            pending_array = Some(val.to_owned());
                        }
                    }
                }
                _ => {}
            }
        }
        settings
    }

    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        out.push_str("#General settings\n");
        out.push_str("[general]\n");
        out.push_str("\t#Silence the server-wide Wither spawn and death broadcast sounds.\n");
        out.push_str(&format!("\tsilenceWither = {}\n", self.silence_wither));
        out.push_str("\t#Silence the wandering trader's ambient sound.\n");
        out.push_str(&format!("\tsilenceTrader = {}\n", self.silence_trader));
        out.push_str("\t#Silence the server-wide Ender Dragon Death broadcast sound.\n");
        out.push_str(&format!("\tsilenceDragon = {}\n", self.silence_dragon));
        out.push_str(
            "\t#Silence the server-wide Thunder broadcast sound caused by the Lightning event\n",
        );
        out.push_str(&format!(
            "\tsilenceLightning = {}\n",
            self.silence_lightning
        ));
        out.push_str(
            "\t#Silence explosion sounds: the explosion bang, primed TNT and primed creeper hissing, and wind charge bursts.\n",
        );
        out.push_str(&format!(
            "\tsilenceExplosion = {}\n",
            self.silence_explosion
        ));
        out.push_str(
            "\t#A list of sounds to silence, discoverable with the toggle command /bwncr listen\n",
        );
        out.push_str("\t#enter one sound event per line with no commas.\n");
        if self.silence_us.is_empty() {
            out.push_str("\tsilenceUs = [\"\"]\n");
        } else {
            let quoted: Vec<String> = self
                .silence_us
                .iter()
                .map(|s| format!("\"{}\"", escape_toml_string(s)))
                .collect();
            out.push_str(&format!("\tsilenceUs = [{}]\n", quoted.join(", ")));
        }
        out.push_str(
            "\t#If enabled the console will load up spam showing what sounds are being received and whether or not they are being canceled\n",
        );
        out.push_str(&format!("\tdebugMode = {}\n", self.debug_mode));
        out
    }

    /// Mirror these toggles into the settings store so Options > Mods shows the file's values.
    /// Every write notifies `setting_changed`, so `SYNCING` is set meanwhile and callers hold
    /// no lock.
    pub fn write_to_host_settings(&self, host: &mut dyn ClientHost) {
        SYNCING.store(true, Ordering::SeqCst);
        for name in TOGGLE_KEYS {
            if let (Some(key), Some(value)) = (setting_key(name), self.get_bool(name)) {
                let _ = host.set_setting(key, SettingValue::Toggle(value));
            }
        }
        SYNCING.store(false, Ordering::SeqCst);
    }

    pub fn apply_single_setting(&mut self, key: &str, value: &SettingValue) -> bool {
        let SettingValue::Toggle(val) = value else {
            return false;
        };
        if self.get_bool(key) == Some(*val) {
            return false;
        }
        self.set_bool(key, *val)
    }
}

const TOGGLE_KEYS: [&str; 6] = [
    KEY_SILENCE_WITHER,
    KEY_SILENCE_TRADER,
    KEY_SILENCE_DRAGON,
    KEY_SILENCE_LIGHTNING,
    KEY_SILENCE_EXPLOSION,
    KEY_DEBUG_MODE,
];

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

/// The common config file `config/badwithernocookie-common.toml`.
pub fn config_spec() -> ConfigSpec {
    ConfigSpec::new(ConfigKind::Common, SilenceSettings::default().to_toml())
}

pub fn register_settings(r: &mut dyn Registrar) {
    let specs = [
        SettingSpec::toggle(KEY_SILENCE_WITHER, "bwncr.config.silenceWither", true)
            .tooltip("bwncr.config.silenceWither.tooltip"),
        SettingSpec::toggle(KEY_SILENCE_TRADER, "bwncr.config.silenceTrader", true)
            .tooltip("bwncr.config.silenceTrader.tooltip"),
        SettingSpec::toggle(KEY_SILENCE_DRAGON, "bwncr.config.silenceDragon", true)
            .tooltip("bwncr.config.silenceDragon.tooltip"),
        SettingSpec::toggle(KEY_SILENCE_LIGHTNING, "bwncr.config.silenceLightning", true)
            .tooltip("bwncr.config.silenceLightning.tooltip"),
        SettingSpec::toggle(
            KEY_SILENCE_EXPLOSION,
            "bwncr.config.silenceExplosion",
            false,
        )
        .tooltip("bwncr.config.silenceExplosion.tooltip"),
        SettingSpec::toggle(KEY_DEBUG_MODE, "bwncr.config.debugMode", false)
            .tooltip("bwncr.config.debugMode.tooltip"),
    ];
    let mut issued = Vec::new();
    for (name, spec) in TOGGLE_KEYS.into_iter().zip(specs) {
        if let Ok(key) = r.register_setting(spec) {
            issued.push((name, key));
        }
    }
    *SETTING_KEYS.lock().unwrap_or_else(|e| e.into_inner()) = issued;
}

pub fn persist_config_file(host: &dyn Host, settings: &SilenceSettings) {
    let path = host.config_dir().join(CONFIG_FILE_NAME);
    let text = settings.to_toml();
    if std::fs::read_to_string(&path).is_ok_and(|current| current == text) {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, text);
}

/// Validates a sound path (`entity.sheep.step`) or full namespaced id (`othermod:entity.thing.hurt`).
pub fn validate_sound(raw: &str) -> Result<(), &'static str> {
    if raw.is_empty() {
        return Err("it is empty");
    }
    if raw
        .chars()
        .any(|c| c.is_whitespace() || c == ',' || c == '"')
    {
        return Err("it contains spaces, commas or quotes");
    }
    if raw.contains(':') {
        if Identifier::parse(raw).is_err() {
            return Err("it is not a valid namespaced id");
        }
    } else if !raw
        .bytes()
        .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.' | b'/' | b'-'))
    {
        return Err("use lower-case letters, digits and _ . / - only");
    }
    Ok(())
}

/// Splits a comma-separated sound list; empty segments are preserved so they can be rejected.
pub fn split_sound_list(raw: &str) -> Vec<String> {
    raw.split(',').map(|part| part.trim().to_owned()).collect()
}

/// Deduplicates entries while preserving order and skipping blank entries.
pub fn normalize_sound_list(entries: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for entry in entries {
        let trimmed = entry.trim();
        if !trimmed.is_empty() && seen.insert(trimmed.to_owned()) {
            out.push(trimmed.to_owned());
        }
    }
    out
}

fn parse_toml_bool(raw: &str) -> Option<bool> {
    match raw {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
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
            other => out.push(other),
        }
    }
    out
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

fn contains_unquoted_char(line: &str, target: char) -> bool {
    let mut in_quotes = false;
    let mut escaped = false;
    for ch in line.chars() {
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
        if ch == target && !in_quotes {
            return true;
        }
    }
    false
}

fn parse_toml_string_array(raw: &str) -> Option<Vec<String>> {
    let start = raw.find('[')?;
    let end = raw.rfind(']')?;
    if end < start {
        return None;
    }
    let inner = &raw[start + 1..end];
    let mut out = Vec::new();
    let mut chars = inner.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '"' {
            let mut value = String::new();
            let mut closed = false;
            while let Some(c) = chars.next() {
                if c == '\\' {
                    if let Some(esc) = chars.next() {
                        match esc {
                            '\\' => value.push('\\'),
                            '"' => value.push('"'),
                            'n' => value.push('\n'),
                            'r' => value.push('\r'),
                            't' => value.push('\t'),
                            other => value.push(other),
                        }
                    }
                } else if c == '"' {
                    closed = true;
                    break;
                } else {
                    value.push(c);
                }
            }
            if !closed {
                return None;
            }
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                out.push(trimmed.to_owned());
            }
        }
    }
    Some(normalize_sound_list(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_toml_round_trips() {
        let original = SilenceSettings {
            silence_wither: true,
            silence_trader: false,
            silence_dragon: true,
            silence_lightning: false,
            silence_explosion: true,
            debug_mode: true,
            silence_us: vec![
                "entity.sheep.step".to_owned(),
                "othermod:entity.thing.hurt".to_owned(),
            ],
        };
        let serialized = original.to_toml();
        let parsed = SilenceSettings::parse_toml(&serialized);
        assert_eq!(parsed, original);
    }

    #[test]
    fn multiline_silence_us_and_blank_entries_parse_cleanly() {
        let toml = r#"
[general]
silenceWither = false
silenceUs = [
    "",
    "entity.cow.ambient", # inline comment
    "  ",
    "entity.cow.ambient",
    "custom:alarm.bell",
]
"#;
        let parsed = SilenceSettings::parse_toml(toml);
        assert!(!parsed.silence_wither);
        assert!(parsed.silence_dragon);
        assert_eq!(
            parsed.silence_us,
            vec![
                "entity.cow.ambient".to_owned(),
                "custom:alarm.bell".to_owned()
            ]
        );
    }

    #[test]
    fn garbage_toml_never_panics() {
        let parsed = SilenceSettings::parse_toml(
            "[broken\nsilenceWither = maybe\nsilenceUs = [\"unclosed\n",
        );
        assert_eq!(parsed, SilenceSettings::default());
    }

    #[test]
    fn validate_sound_matches_original_rules() {
        assert!(validate_sound("entity.sheep.step").is_ok());
        assert!(validate_sound("othermod:entity.thing.hurt").is_ok());
        assert!(validate_sound("").is_err());
        assert!(validate_sound("entity sheep").is_err());
        assert!(validate_sound("a,b").is_err());
        assert!(validate_sound("Entity.Sheep").is_err());
        assert!(validate_sound("bad:ns:path").is_err());
    }

    #[test]
    fn apply_single_setting_changes_only_on_new_toggle_values() {
        let mut settings = SilenceSettings::default();
        assert!(!settings.apply_single_setting(KEY_SILENCE_WITHER, &SettingValue::Toggle(true)));
        assert!(settings.apply_single_setting(KEY_SILENCE_WITHER, &SettingValue::Toggle(false)));
        assert!(!settings.silence_wither);
        assert!(!settings.apply_single_setting(KEY_SILENCE_WITHER, &SettingValue::Choice(1)));
        assert!(!settings.apply_single_setting("unknown", &SettingValue::Toggle(true)));
    }
}
