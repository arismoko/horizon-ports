//! Preferred Gamerules for Sunrise: new worlds start with the game rules you prefer.
//!
//! The original reads `config/preferred-gamerules.properties` once at startup: each line
//! `gamerule.<rule id>=<value>` replaces that rule's default (`GameRule.defaultValue`), so every
//! world created afterwards starts with it, and worlds that already exist keep their own rules.
//! Sunrise lets no mod change a rule's default yet (sunrise-org/sunrise#5306), so this port sets
//! the preferred values on a new world as it opens, before its first tick.
//!
//! | Moment | Where | What it does |
//! |---|---|---|
//! | `server_started` | [`Mod::server_started`] | on a new world, reads the preferences and sets each rule |

use mod_api::{ConfigPath, GameRuleValue, Hooks, ServerHost, ServerStartedEvent};

/// The preferences file in the game's `config/` folder.
const FILE: &str = "preferredgamerules.properties";
/// Each preference's key is this followed by the rule's id (`PREFIX` in `Preferences`).
const PREFIX: &str = "gamerule.";

#[derive(Default)]
struct Mod;

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    /// A world whose overworld clock has not started has just been created: its rules are still
    /// the defaults, which the original would have replaced with the preferences.
    fn server_started(&self, host: &mut dyn ServerHost, _event: &mut ServerStartedEvent) {
        let Some(overworld) = host.level("minecraft:overworld") else {
            return;
        };
        if overworld.game_time(&*host) != Some(0) {
            return;
        }
        let Some(text) = read_or_create(&*host) else {
            return;
        };
        let current = host.game_rules();
        for (rule, raw) in preferences(&text) {
            let Some(id) = rule_id(rule) else {
                eprintln!("[preferredgamerules] Invalid gamerule ID: {rule}");
                continue;
            };
            let Some((_, default)) = current.iter().find(|(known, _)| *known == id) else {
                continue;
            };
            let parsed = parse(default, raw);
            if !parsed.is_some_and(|value| overworld.set_game_rule(host, &id, value)) {
                eprintln!("[preferredgamerules] Invalid value for gamerule {id}: \"{raw}\"");
            }
        }
    }
}

/// The preferences file's text, written empty when it is missing (`ConfigIO.GetOrCreate`).
fn read_or_create(host: &dyn mod_api::Host) -> Option<String> {
    let files = host.config_files();
    let path = ConfigPath::new(FILE).ok()?;
    match files.read(&path) {
        Ok(bytes) => Some(String::from_utf8_lossy(&bytes).into_owned()),
        Err(_) => {
            let _ = files.write(&path, b"");
            None
        }
    }
}

/// Each `gamerule.` line's rule and raw value, as `ConfigIO.ReadFile` splits lines: blank lines
/// and lines starting with `#` are skipped, the key ends at the first `=`, and a line without a
/// value makes the whole file unreadable, so nothing is applied.
fn preferences(text: &str) -> Vec<(&str, &str)> {
    let mut found = Vec::new();
    for (number, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match line.split_once('=') {
            Some((key, value)) if !value.is_empty() => {
                if let Some(rule) = key.strip_prefix(PREFIX) {
                    found.push((rule, value));
                }
            }
            _ => {
                eprintln!("[preferredgamerules] Missing value at line {number} in {FILE}");
                return Vec::new();
            }
        }
    }
    found
}

/// The rule's namespaced id, as `Identifier.tryParse` reads it: `minecraft:` when no namespace
/// is given, and `None` for characters an identifier refuses.
fn rule_id(rule: &str) -> Option<String> {
    let (namespace, path) = rule.split_once(':').unwrap_or(("minecraft", rule));
    let valid = |text: &str, extra: &[char]| {
        text.chars().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || "_-.".contains(c) || extra.contains(&c)
        })
    };
    (valid(namespace, &[]) && valid(path, &['/'])).then(|| format!("{namespace}:{path}"))
}

/// `GameRule.deserialize`: the rule's own argument reads the whole text, a boolean as `true` or
/// `false` (quoted or not) and an integer as Brigadier's number characters. The rule's range is
/// left to the set, which refuses a value outside it.
fn parse(kind: &GameRuleValue, raw: &str) -> Option<GameRuleValue> {
    match kind {
        GameRuleValue::Bool(_) => {
            let unquoted = ['"', '\'']
                .iter()
                .find_map(|q| raw.strip_prefix(*q)?.strip_suffix(*q))
                .unwrap_or(raw);
            match unquoted {
                "true" => Some(GameRuleValue::Bool(true)),
                "false" => Some(GameRuleValue::Bool(false)),
                _ => None,
            }
        }
        GameRuleValue::Int(_) => {
            if !raw
                .chars()
                .all(|c| c.is_ascii_digit() || c == '-' || c == '.')
            {
                return None;
            }
            raw.parse().ok().map(GameRuleValue::Int)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_originals_example() {
        let text = "# preferences\n\ngamerule.minecraft:fire_damage=false\ngamerule.minecraft:keep_inventory=true\nother=1\n";
        assert_eq!(
            preferences(text),
            vec![
                ("minecraft:fire_damage", "false"),
                ("minecraft:keep_inventory", "true")
            ]
        );
    }

    #[test]
    fn a_line_without_a_value_drops_the_file() {
        assert!(preferences("gamerule.keep_inventory=true\ngamerule.fire_damage=\n").is_empty());
        assert!(preferences("gamerule.keep_inventory\n").is_empty());
    }

    #[test]
    fn ids_default_to_minecraft_and_refuse_bad_characters() {
        assert_eq!(
            rule_id("keep_inventory").as_deref(),
            Some("minecraft:keep_inventory")
        );
        assert_eq!(rule_id("mymod:rule").as_deref(), Some("mymod:rule"));
        assert_eq!(rule_id("Keep Inventory"), None);
    }

    #[test]
    fn values_parse_as_the_rules_argument_does() {
        let bool = GameRuleValue::Bool(false);
        let int = GameRuleValue::Int(0);
        assert_eq!(parse(&bool, "true"), Some(GameRuleValue::Bool(true)));
        assert_eq!(parse(&bool, "\"false\""), Some(GameRuleValue::Bool(false)));
        assert_eq!(parse(&bool, "TRUE"), None);
        assert_eq!(parse(&bool, "true "), None);
        assert_eq!(parse(&int, "-3"), Some(GameRuleValue::Int(-3)));
        assert_eq!(parse(&int, "3.5"), None);
        assert_eq!(parse(&int, "+3"), None);
    }
}
