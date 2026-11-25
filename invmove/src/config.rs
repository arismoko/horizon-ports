//! InvMove's settings (Options > Mods) and its config file `config/invmove.toml`, which holds
//! what a flat settings page cannot: the extra keys it may update and the per-screen choices
//! for screens it has not been told about.

use std::collections::BTreeMap;

use mod_api::{ClientHost, Config, ConfigKind, Registrar, SettingSpec};

use crate::logic::{KNOWN_SCREENS, PauseScreenMode, SneakConfig, SneakMode};

pub const MOD_ID: &str = "invmove";
pub const CONFIG_FILE_NAME: &str = "invmove.toml";

pub const GENERAL_ENABLE: &str = "general.enable";
pub const MOVEMENT_ENABLE: &str = "movement.enable";
pub const MOVEMENT_SNEAK: &str = "movement.sneak";
pub const MOVEMENT_SNEAK_DISALLOWED: &str = "movement.sneak_disallowed";
pub const MOVEMENT_SNEAK_FLYING: &str = "movement.sneak_flying";
pub const MOVEMENT_DISMOUNT: &str = "movement.dismount";
pub const MOVEMENT_TEXT_FIELD: &str = "movement.text_field_disables";
pub const MOVEMENT_RECIPE_BOOK: &str = "movement.recipe_book";
pub const MOVEMENT_UNRECOGNIZED: &str = "movement.unrecognized_default";
pub const BACKGROUND_ENABLE: &str = "background.enable";
pub const BACKGROUND_HIDE_ON_PAUSE: &str = "background.hide_on_pause";
pub const BACKGROUND_UNRECOGNIZED: &str = "background.unrecognized_default";

/// The seven movement keys InvMove allows by default: vanilla mapping, setting key, label key.
pub const MOVEMENT_KEYS: [(&str, &str); 7] = [
    ("key.forward", "keys.forward"),
    ("key.back", "keys.back"),
    ("key.left", "keys.left"),
    ("key.right", "keys.right"),
    ("key.jump", "keys.jump"),
    ("key.sneak", "keys.sneak"),
    ("key.sprint", "keys.sprint"),
];

pub fn screen_movement_key(screen: &str) -> String {
    format!("screen.movement.{screen}")
}

pub fn screen_background_key(screen: &str) -> String {
    format!("screen.background.{screen}")
}

/// The file's contents.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileConfig {
    /// Extra vanilla key mappings, by name, whose held state is updated inside screens.
    pub extra_keys: Vec<String>,
    /// Screens not in the settings list, with their movement choice.
    pub unrecognized_movement: BTreeMap<String, bool>,
    /// The same for the background.
    pub unrecognized_background: BTreeMap<String, bool>,
}

impl FileConfig {
    pub fn to_toml(&self) -> String {
        let mut out = String::from(
            "# InvMove client configuration. Changes saved to this file are hot-reloaded.\n\n",
        );
        out.push_str("[keys]\n");
        out.push_str(
            "# Vanilla key mappings besides the seven movement keys that InvMove also updates\n\
             # inside screens, for example \"key.attack\". The movement keys are Options > Mods.\n",
        );
        let extra: Vec<String> = self
            .extra_keys
            .iter()
            .map(|k| format!("\"{}\"", escape(k)))
            .collect();
        out.push_str(&format!("extra = [{}]\n\n", extra.join(", ")));
        table(
            &mut out,
            "unrecognized_movement",
            "Screens InvMove was not told about, listed once they are opened: true lets you move.",
            &self.unrecognized_movement,
        );
        table(
            &mut out,
            "unrecognized_background",
            "The same for the background: true hides the dark tint.",
            &self.unrecognized_background,
        );
        out
    }

    /// Reads the file; whatever is not understood is skipped.
    pub fn parse(source: &str) -> Self {
        let mut cfg = Self::default();
        let mut section = String::new();
        for raw in source.lines() {
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix('[') {
                if let Some(name) = rest.strip_suffix(']') {
                    section = name.trim().to_owned();
                }
                continue;
            }
            match section.as_str() {
                "keys" => {
                    if let Some(("extra", value)) = split_pair(line) {
                        cfg.extra_keys = parse_string_array(value);
                    }
                }
                "unrecognized_movement" | "unrecognized_background" => {
                    let Some((key, value)) = split_pair(line) else {
                        continue;
                    };
                    let Some(on) = parse_bool(value) else {
                        continue;
                    };
                    let key = unquote(key);
                    if key.is_empty() {
                        continue;
                    }
                    if section == "unrecognized_movement" {
                        cfg.unrecognized_movement.insert(key, on);
                    } else {
                        cfg.unrecognized_background.insert(key, on);
                    }
                }
                _ => {}
            }
        }
        cfg
    }
}

fn table(out: &mut String, name: &str, comment: &str, map: &BTreeMap<String, bool>) {
    out.push_str(&format!("[{name}]\n# {comment}\n"));
    for (key, value) in map {
        out.push_str(&format!("\"{}\" = {value}\n", escape(key)));
    }
    out.push('\n');
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Cuts a `#` comment that is not inside a quoted string.
fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '#' if !quoted => return &line[..i],
            _ => {}
        }
    }
    line
}

/// Splits `key = value` at the first `=` outside quotes.
fn split_pair(line: &str) -> Option<(&str, &str)> {
    let mut quoted = false;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '=' if !quoted => return Some((line[..i].trim(), line[i + 1..].trim())),
            _ => {}
        }
    }
    None
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn unquote(text: &str) -> String {
    let text = text.trim();
    let inner = text
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(text);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_string_array(value: &str) -> Vec<String> {
    let Some(inner) = value
        .trim()
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
    else {
        return Vec::new();
    };
    let mut items = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for c in inner.chars() {
        match c {
            '\\' if quoted && !escaped => {
                escaped = true;
                continue;
            }
            '"' if !escaped => {
                quoted = !quoted;
                if !quoted {
                    items.push(std::mem::take(&mut current));
                }
            }
            _ if quoted => current.push(c),
            _ => {}
        }
        escaped = false;
    }
    items.retain(|item| !item.is_empty());
    items
}

pub fn register_mod_configs(configs: &mut Vec<Config>) {
    let default_toml = FileConfig::default().to_toml();
    configs.push(Config::new(MOD_ID, ConfigKind::Client, &default_toml).file(CONFIG_FILE_NAME));
}

/// The settings as read this tick.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub enabled: bool,
    pub movement: bool,
    pub sneak: SneakConfig,
    pub text_field_disables: bool,
    pub recipe_book: bool,
    pub unrecognized_movement: bool,
    pub background: bool,
    pub hide_on_pause: PauseScreenMode,
    pub unrecognized_background: bool,
}

impl Options {
    pub fn read(host: &dyn ClientHost) -> Self {
        let toggle = |key: &str, default: bool| host.setting_bool(key).unwrap_or(default);
        let choice = |key: &str, default: u32| host.setting_choice(key).unwrap_or(default);
        Self {
            enabled: toggle(GENERAL_ENABLE, true),
            movement: toggle(MOVEMENT_ENABLE, true),
            sneak: SneakConfig {
                sneak: SneakMode::from_choice(choice(MOVEMENT_SNEAK, 1)),
                flying: SneakMode::from_choice(choice(MOVEMENT_SNEAK_FLYING, 2)),
                disallowed: SneakMode::from_choice(choice(MOVEMENT_SNEAK_DISALLOWED, 1)),
                dismount: toggle(MOVEMENT_DISMOUNT, false),
            },
            text_field_disables: toggle(MOVEMENT_TEXT_FIELD, true),
            recipe_book: toggle(MOVEMENT_RECIPE_BOOK, true),
            unrecognized_movement: toggle(MOVEMENT_UNRECOGNIZED, true),
            background: toggle(BACKGROUND_ENABLE, true),
            hide_on_pause: PauseScreenMode::from_choice(choice(BACKGROUND_HIDE_ON_PAUSE, 0)),
            unrecognized_background: toggle(BACKGROUND_UNRECOGNIZED, true),
        }
    }
}

const SNEAK_OPTIONS: [&str; 4] = [
    "config.invmove.sneak.off",
    "config.invmove.sneak.maintain",
    "config.invmove.sneak.pressed",
    "config.invmove.sneak.maintain_while_pressed",
];

/// Registers every setting on Options > Mods > InvMove.
pub fn register_settings(r: &mut dyn Registrar) {
    let mut specs = vec![
        SettingSpec::toggle(GENERAL_ENABLE, "config.invmove.general.enable", true)
            .tooltip("tooltip.config.invmove.general.enable"),
        SettingSpec::toggle(MOVEMENT_ENABLE, "config.invmove.movement.enable", true)
            .tooltip("tooltip.config.invmove.movement.enable"),
        SettingSpec::choice(
            MOVEMENT_SNEAK,
            "config.invmove.movement.sneak",
            SNEAK_OPTIONS,
            1,
        )
        .tooltip("tooltip.config.invmove.movement.sneak"),
        SettingSpec::choice(
            MOVEMENT_SNEAK_DISALLOWED,
            "config.invmove.movement.sneak_disallowed",
            SNEAK_OPTIONS,
            1,
        )
        .tooltip("tooltip.config.invmove.movement.sneak_disallowed"),
        SettingSpec::choice(
            MOVEMENT_SNEAK_FLYING,
            "config.invmove.movement.sneak_flying",
            SNEAK_OPTIONS,
            2,
        )
        .tooltip("tooltip.config.invmove.movement.sneak_flying"),
        SettingSpec::toggle(MOVEMENT_DISMOUNT, "config.invmove.movement.dismount", false)
            .tooltip("tooltip.config.invmove.movement.dismount"),
        SettingSpec::toggle(
            MOVEMENT_TEXT_FIELD,
            "config.invmove.movement.textFieldDisables",
            true,
        )
        .tooltip("tooltip.config.invmove.movement.textFieldDisables"),
        SettingSpec::toggle(
            MOVEMENT_UNRECOGNIZED,
            "config.invmove.movement.unrecognizedScreenDefault",
            true,
        )
        .tooltip("tooltip.config.invmove.movement.unrecognizedScreenDefault"),
        SettingSpec::toggle(
            MOVEMENT_RECIPE_BOOK,
            "config.invmove.movement.recipe_book",
            true,
        ),
    ];
    for (_, key) in MOVEMENT_KEYS {
        specs.push(
            SettingSpec::toggle(key, format!("config.invmove.{key}"), true)
                .tooltip("key.invmove.category.movement.keys.desc"),
        );
    }
    specs.push(
        SettingSpec::toggle(BACKGROUND_ENABLE, "config.invmove.background.enable", true)
            .tooltip("tooltip.config.invmove.background.enable"),
    );
    specs.push(
        SettingSpec::choice(
            BACKGROUND_HIDE_ON_PAUSE,
            "config.invmove.background.hideOnPause",
            [
                "config.invmove.pause.show",
                "config.invmove.pause.show_sp",
                "config.invmove.pause.allow_hide",
            ],
            0,
        )
        .tooltip("tooltip.config.invmove.background.hideOnPause"),
    );
    specs.push(
        SettingSpec::toggle(
            BACKGROUND_UNRECOGNIZED,
            "config.invmove.background.unrecognizedScreenDefault",
            true,
        )
        .tooltip("tooltip.config.invmove.background.unrecognizedScreenDefault"),
    );
    for (screen, _) in KNOWN_SCREENS {
        specs.push(SettingSpec::toggle(
            screen_movement_key(screen),
            format!("config.invmove.screen.movement.{screen}"),
            true,
        ));
    }
    for (screen, _) in KNOWN_SCREENS {
        specs.push(SettingSpec::toggle(
            screen_background_key(screen),
            format!("config.invmove.screen.background.{screen}"),
            true,
        ));
    }
    for spec in specs {
        let _ = r.register_setting(spec);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_round_trips() {
        let mut cfg = FileConfig {
            extra_keys: vec!["key.attack".into(), "key.use".into()],
            ..FileConfig::default()
        };
        cfg.unrecognized_movement
            .insert("minecraft:crafter_3x3".into(), false);
        cfg.unrecognized_background.insert("paused".into(), true);
        cfg.unrecognized_background
            .insert("mod:odd \"name\"".into(), false);
        assert_eq!(FileConfig::parse(&cfg.to_toml()), cfg);
    }

    #[test]
    fn the_default_file_is_empty_and_valid() {
        let default = FileConfig::default();
        assert_eq!(FileConfig::parse(&default.to_toml()), default);
    }

    #[test]
    fn garbage_is_skipped() {
        let cfg = FileConfig::parse(
            "[keys]\nextra = nonsense\n[unrecognized_movement]\n\"a\" = maybe\n= true\n\"b\" = true # note\nstray\n",
        );
        assert!(cfg.extra_keys.is_empty());
        assert_eq!(cfg.unrecognized_movement.len(), 1);
        assert_eq!(cfg.unrecognized_movement.get("b"), Some(&true));
        assert_eq!(FileConfig::parse(""), FileConfig::default());
    }

    #[test]
    fn comments_inside_strings_are_kept() {
        let cfg = FileConfig::parse("[unrecognized_background]\n\"a#b\" = false\n");
        assert_eq!(cfg.unrecognized_background.get("a#b"), Some(&false));
    }

    #[test]
    fn the_config_file_is_owned_by_the_mod() {
        let mut configs = Vec::new();
        register_mod_configs(&mut configs);
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].mod_id, MOD_ID);
        assert_eq!(configs[0].kind, ConfigKind::Client);
        assert!(mod_api::ConfigPath::new(&configs[0].file).is_ok_and(|p| p.owned_by(MOD_ID)));
    }

    #[test]
    fn setting_keys_are_valid() {
        let mut keys = vec![
            GENERAL_ENABLE.to_owned(),
            MOVEMENT_SNEAK.to_owned(),
            BACKGROUND_HIDE_ON_PAUSE.to_owned(),
        ];
        for (screen, _) in KNOWN_SCREENS {
            keys.push(screen_movement_key(screen));
            keys.push(screen_background_key(screen));
        }
        for (_, key) in MOVEMENT_KEYS {
            keys.push(key.to_owned());
        }
        for key in keys {
            let spec = SettingSpec::toggle(key.clone(), "label", true);
            assert!(spec.check().is_ok(), "{key}");
        }
    }
}
