//! InvMove's settings (shown on its own config screen) and its config file
//! `config/invmove.toml`, which holds what a setting cannot: the extra keys it may update and the
//! per-screen choices for screens it has not been told about.

use std::collections::{BTreeMap, HashMap};

use mod_api::{
    ClientHost, ConfigKind, ConfigSpec, Registrar, SettingKey, SettingKind, SettingSpec,
};

use crate::logic::{KNOWN_SCREENS, PauseScreenMode, ScreenKey, SneakConfig, SneakMode};

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
             # inside screens, for example \"key.attack\". The movement keys are on the InvMove config screen.\n",
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

/// The declaration of `config/invmove.toml`, read once on the window and reloaded when saved.
pub fn config_spec() -> ConfigSpec {
    ConfigSpec::new(ConfigKind::Client, FileConfig::default().to_toml()).file(CONFIG_FILE_NAME)
}

/// Writes the file under the game's `config/` folder; a failed write is dropped, as the next
/// change writes it again.
pub fn write_config_file(host: &dyn ClientHost, text: &str) {
    let path = host.config_dir().join(CONFIG_FILE_NAME);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, text);
}

/// What the config screen shows of a registered setting.
#[derive(Clone, Debug)]
pub struct Meta {
    /// Translation key of the label.
    pub label: String,
    /// Translation key of the tooltip, if any.
    pub tooltip: Option<String>,
    /// Translation keys of a choice's options; `None` for a toggle.
    pub options: Option<Vec<String>>,
}

/// The owner-bound handles of every setting, issued at registration.
#[derive(Clone, Debug)]
pub struct Keys {
    pub enable: SettingKey,
    pub movement: SettingKey,
    pub sneak: SettingKey,
    pub sneak_disallowed: SettingKey,
    pub sneak_flying: SettingKey,
    pub dismount: SettingKey,
    pub text_field: SettingKey,
    pub recipe_book: SettingKey,
    pub unrecognized_movement: SettingKey,
    pub background: SettingKey,
    pub hide_on_pause: SettingKey,
    pub unrecognized_background: SettingKey,
    /// The allowed-key switches, by vanilla mapping, in `MOVEMENT_KEYS` order.
    pub allowed: Vec<(&'static str, SettingKey)>,
    pub screen_movement: Vec<(ScreenKey, SettingKey)>,
    pub screen_background: Vec<(ScreenKey, SettingKey)>,
    pub meta: HashMap<SettingKey, Meta>,
}

impl Keys {
    /// The switch of a movement key by its vanilla mapping name.
    pub fn allowed_key(&self, mapping: &str) -> Option<SettingKey> {
        self.allowed
            .iter()
            .find(|(name, _)| *name == mapping)
            .map(|(_, key)| *key)
    }

    pub fn screen_movement_key(&self, screen: &str) -> Option<SettingKey> {
        self.screen_movement
            .iter()
            .find(|(name, _)| *name == screen)
            .map(|(_, key)| *key)
    }

    pub fn screen_background_key(&self, screen: &str) -> Option<SettingKey> {
        self.screen_background
            .iter()
            .find(|(name, _)| *name == screen)
            .map(|(_, key)| *key)
    }

    /// Whether the player marked the movement key as one InvMove may read.
    pub fn key_allowed(&self, host: &dyn ClientHost, mapping: &str) -> bool {
        self.allowed_key(mapping)
            .and_then(|key| host.setting_bool(key).ok())
            .unwrap_or(true)
    }
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
    pub fn read(host: &dyn ClientHost, keys: &Keys) -> Self {
        let toggle = |key: SettingKey, default: bool| host.setting_bool(key).unwrap_or(default);
        let choice = |key: SettingKey, default: u32| host.setting_choice(key).unwrap_or(default);
        Self {
            enabled: toggle(keys.enable, true),
            movement: toggle(keys.movement, true),
            sneak: SneakConfig {
                sneak: SneakMode::from_choice(choice(keys.sneak, 1)),
                flying: SneakMode::from_choice(choice(keys.sneak_flying, 2)),
                disallowed: SneakMode::from_choice(choice(keys.sneak_disallowed, 1)),
                dismount: toggle(keys.dismount, false),
            },
            text_field_disables: toggle(keys.text_field, true),
            recipe_book: toggle(keys.recipe_book, true),
            unrecognized_movement: toggle(keys.unrecognized_movement, true),
            background: toggle(keys.background, true),
            hide_on_pause: PauseScreenMode::from_choice(choice(keys.hide_on_pause, 0)),
            unrecognized_background: toggle(keys.unrecognized_background, true),
        }
    }
}

const SNEAK_OPTIONS: [&str; 4] = [
    "config.invmove.sneak.off",
    "config.invmove.sneak.maintain",
    "config.invmove.sneak.pressed",
    "config.invmove.sneak.maintain_while_pressed",
];

pub const PAUSE_OPTIONS: [&str; 3] = [
    "config.invmove.pause.show",
    "config.invmove.pause.show_sp",
    "config.invmove.pause.allow_hide",
];

/// Every setting, in registration order.
fn setting_specs() -> Vec<SettingSpec> {
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
            PAUSE_OPTIONS,
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
    specs
}

/// Registers every setting and returns the handles the mod reads and writes them by; `None` when
/// the game refused one.
pub fn register_settings(r: &mut dyn Registrar) -> Option<Keys> {
    let mut issued: HashMap<String, SettingKey> = HashMap::new();
    let mut meta = HashMap::new();
    for spec in setting_specs() {
        let name = spec.key.clone();
        let described = Meta {
            label: spec.label.clone(),
            tooltip: spec.tooltip.clone(),
            options: match &spec.kind {
                SettingKind::Choice { options, .. } => Some(options.clone()),
                _ => None,
            },
        };
        let key = r.register_setting(spec).ok()?;
        issued.insert(name, key);
        meta.insert(key, described);
    }
    let get = |name: &str| issued.get(name).copied();
    Some(Keys {
        enable: get(GENERAL_ENABLE)?,
        movement: get(MOVEMENT_ENABLE)?,
        sneak: get(MOVEMENT_SNEAK)?,
        sneak_disallowed: get(MOVEMENT_SNEAK_DISALLOWED)?,
        sneak_flying: get(MOVEMENT_SNEAK_FLYING)?,
        dismount: get(MOVEMENT_DISMOUNT)?,
        text_field: get(MOVEMENT_TEXT_FIELD)?,
        recipe_book: get(MOVEMENT_RECIPE_BOOK)?,
        unrecognized_movement: get(MOVEMENT_UNRECOGNIZED)?,
        background: get(BACKGROUND_ENABLE)?,
        hide_on_pause: get(BACKGROUND_HIDE_ON_PAUSE)?,
        unrecognized_background: get(BACKGROUND_UNRECOGNIZED)?,
        allowed: MOVEMENT_KEYS
            .iter()
            .map(|(mapping, key)| Some((*mapping, get(key)?)))
            .collect::<Option<Vec<_>>>()?,
        screen_movement: KNOWN_SCREENS
            .iter()
            .map(|(screen, _)| Some((*screen, get(&screen_movement_key(screen))?)))
            .collect::<Option<Vec<_>>>()?,
        screen_background: KNOWN_SCREENS
            .iter()
            .map(|(screen, _)| Some((*screen, get(&screen_background_key(screen))?)))
            .collect::<Option<Vec<_>>>()?,
        meta,
    })
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
    fn setting_keys_are_valid_and_unique() {
        let specs = setting_specs();
        let mut seen = std::collections::HashSet::new();
        for spec in &specs {
            assert!(spec.check().is_ok(), "{}", spec.key);
            assert!(seen.insert(spec.key.clone()), "{}", spec.key);
        }
        assert_eq!(
            specs.len(),
            12 + MOVEMENT_KEYS.len() + 2 * KNOWN_SCREENS.len()
        );
    }
}
