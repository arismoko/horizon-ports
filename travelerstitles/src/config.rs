//! Options: the numbers, switches and colours are settings on Options > Mods > Traveler's
//! Titles; the two blacklists (lists of ids) live in `config/travelerstitles.toml`, which is
//! reloaded when it is edited.

use crate::titles::Style;
use mod_api::{
    ClientHost, ConfigEvent, ConfigKind, ConfigSnapshot, ConfigSpec, Registrar, SettingKey,
    SettingSpec,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

pub const MOD_ID: &str = "travelerstitles";
pub const CONFIG_FILE: &str = "travelerstitles.toml";

const DEFAULT_BIOME_BLACKLIST: [&str; 3] = [
    "minecraft:the_end",
    "minecraft:river",
    "minecraft:frozen_river",
];

/// Which of the two title kinds a setting belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Biome,
    Dimension,
}

impl Kind {
    fn prefix(self) -> &'static str {
        match self {
            Kind::Biome => "biomes",
            Kind::Dimension => "dimensions",
        }
    }
}

fn key(kind: Kind, name: &str) -> String {
    format!("{}_{name}", kind.prefix())
}

fn label(key: &str) -> String {
    format!("config.travelerstitles.{key}")
}

/// The section a setting sits under on Options > Mods: one per title kind, and the sounds.
fn category(name: &str) -> &'static str {
    if name.starts_with("sound_") {
        "config.travelerstitles.category.sounds"
    } else if name.starts_with("dimensions_") {
        "config.travelerstitles.category.dimensions"
    } else {
        "config.travelerstitles.category.biomes"
    }
}

fn tooltip(key: &str) -> String {
    format!("config.travelerstitles.{key}.tooltip")
}

/// The keys `register_setting` returned, by setting name, kept for the process.
static KEYS: OnceLock<HashMap<String, SettingKey>> = OnceLock::new();

fn setting_key(name: &str) -> Option<SettingKey> {
    KEYS.get()?.get(name).copied()
}

fn read_bool(host: &dyn ClientHost, name: &str) -> Option<bool> {
    host.setting_bool(setting_key(name)?).ok()
}

fn read_f64(host: &dyn ClientHost, name: &str) -> Option<f64> {
    host.setting_f64(setting_key(name)?).ok()
}

fn read_color(host: &dyn ClientHost, name: &str) -> Option<u32> {
    host.setting_color(setting_key(name)?).ok()
}

/// Read one kind's style; a missing setting reads as its default.
pub fn read_style(host: &dyn ClientHost, kind: Kind) -> Style {
    let defaults = defaults(kind);
    let toggle = |name: &str, default: bool| read_bool(host, &key(kind, name)).unwrap_or(default);
    let number = |name: &str, default: f64| {
        read_f64(host, &key(kind, name))
            .filter(|v| v.is_finite())
            .unwrap_or(default)
    };
    Style {
        enabled: toggle("enabled", defaults.enabled),
        fade_in: number("fade_in", f64::from(defaults.fade_in)) as i32,
        display: number("display", f64::from(defaults.display)) as i32,
        fade_out: number("fade_out", f64::from(defaults.fade_out)) as i32,
        color: read_color(host, &key(kind, "color")).unwrap_or(defaults.color),
        size: number("size", f64::from(defaults.size)) as f32,
        shadow: toggle("shadow", defaults.shadow),
        x: number("x", f64::from(defaults.x)) as i32,
        y: number("y", f64::from(defaults.y)) as i32,
        centered: toggle("center", defaults.centered),
    }
}

/// The original's defaults for a kind.
pub fn defaults(kind: Kind) -> Style {
    match kind {
        Kind::Biome => Style {
            enabled: true,
            fade_in: 10,
            display: 50,
            fade_out: 10,
            color: 0xFF_FFFF,
            size: 2.1,
            shadow: true,
            x: 0,
            y: -33,
            centered: true,
        },
        Kind::Dimension => Style {
            enabled: true,
            fade_in: 10,
            display: 70,
            fade_out: 20,
            color: 0xFF_FFFF,
            size: 3.0,
            shadow: true,
            x: 0,
            y: -32,
            centered: true,
        },
    }
}

/// Biome title cooldown, ticks.
pub fn biome_cooldown(host: &dyn ClientHost) -> i32 {
    read_f64(host, "biomes_cooldown")
        .filter(|v| v.is_finite())
        .map_or(80, |v| v as i32)
}

/// How many recent biomes are remembered.
pub fn biome_cache_size(host: &dyn ClientHost) -> usize {
    read_f64(host, "biomes_cache")
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map_or(5, |v| v as usize)
}

pub fn biome_reset_cache_on_dimension_change(host: &dyn ClientHost) -> bool {
    read_bool(host, "biomes_reset_cache").unwrap_or(true)
}

/// Only show titles in a roofless dimension when the player sees the sky.
pub fn only_at_surface(host: &dyn ClientHost, kind: Kind) -> bool {
    read_bool(host, &key(kind, "surface_only")).unwrap_or(kind == Kind::Biome)
}

/// Sound volume and pitch for a kind's entry sound.
pub fn sound(host: &dyn ClientHost, kind: Kind) -> (f32, f32) {
    let read = |name: &str| {
        read_f64(host, &format!("sound_{}_{name}", kind.prefix()))
            .filter(|v| v.is_finite())
            .unwrap_or(1.0) as f32
    };
    (read("volume"), read("pitch"))
}

pub fn register_settings(r: &mut dyn Registrar) {
    let mut specs: Vec<(String, SettingSpec)> = Vec::new();
    for kind in [Kind::Biome, Kind::Dimension] {
        let d = defaults(kind);
        let k = |name: &str| key(kind, name);
        let toggle = |name: &str, default: bool| {
            let key = key(kind, name);
            SettingSpec::toggle(&key, label(&key), default).tooltip(tooltip(&key))
        };
        let range = |name: &str, min: f64, max: f64, step: f64, default: f64| {
            let key = key(kind, name);
            SettingSpec::range(&key, label(&key), min, max, step, default).tooltip(tooltip(&key))
        };
        let mut add = |name: &str, spec: SettingSpec| specs.push((key(kind, name), spec));
        add("enabled", toggle("enabled", d.enabled));
        add(
            "fade_in",
            range("fade_in", 0.0, 200.0, 1.0, f64::from(d.fade_in)),
        );
        add(
            "display",
            range("display", 0.0, 600.0, 1.0, f64::from(d.display)),
        );
        add(
            "fade_out",
            range("fade_out", 0.0, 200.0, 1.0, f64::from(d.fade_out)),
        );
        if kind == Kind::Biome {
            add("cooldown", range("cooldown", 0.0, 1200.0, 1.0, 80.0));
        }
        add(
            "color",
            SettingSpec::color(k("color"), label(&k("color")), d.color)
                .tooltip(tooltip(&k("color"))),
        );
        add("size", range("size", 0.5, 8.0, 0.1, f64::from(d.size)));
        add("shadow", toggle("shadow", d.shadow));
        add("y", range("y", -300.0, 300.0, 1.0, f64::from(d.y)));
        add("x", range("x", -300.0, 300.0, 1.0, f64::from(d.x)));
        if kind == Kind::Biome {
            add("cache", range("cache", 0.0, 30.0, 1.0, 5.0));
        }
        add("center", toggle("center", d.centered));
        if kind == Kind::Biome {
            add("reset_cache", toggle("reset_cache", true));
        }
        add("surface_only", toggle("surface_only", kind == Kind::Biome));
    }
    for (name, default, min, max) in [
        ("sound_biomes_volume", 1.0, 0.0, 2.0),
        ("sound_biomes_pitch", 1.0, 0.5, 2.0),
        ("sound_dimensions_volume", 1.0, 0.0, 2.0),
        ("sound_dimensions_pitch", 1.0, 0.5, 2.0),
    ] {
        specs.push((
            name.to_owned(),
            SettingSpec::range(name, label(name), min, max, 0.05, default).tooltip(tooltip(name)),
        ));
    }
    let mut keys = HashMap::new();
    for (name, spec) in specs {
        match r.register_setting(spec) {
            Ok(setting) => {
                if let Err(error) = r.categorize_setting(&name, category(&name)) {
                    eprintln!("travelerstitles: setting {name} not categorized: {error}");
                }
                keys.insert(name, setting);
            }
            Err(error) => eprintln!("travelerstitles: setting {name} not registered: {error}"),
        }
    }
    // A second registration in one process keeps the first keys, which name the same slots.
    let _ = KEYS.set(keys);
}

/// The two blacklists in `config/travelerstitles.toml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lists {
    pub biomes: Vec<String>,
    pub dimensions: Vec<String>,
}

impl Default for Lists {
    fn default() -> Self {
        Self {
            biomes: DEFAULT_BIOME_BLACKLIST
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            dimensions: Vec::new(),
        }
    }
}

fn write_list(out: &mut String, name: &str, values: &[String]) {
    let quoted: Vec<String> = values.iter().map(|v| format!("\"{v}\"")).collect();
    out.push_str(&format!("{name} = [{}]\n", quoted.join(", ")));
}

impl Lists {
    pub fn to_toml(&self) -> String {
        let mut out = String::from(
            "# Traveler's Titles. The sizes, times, colours and sounds are on Options > Mods.\n\n\
             [biomes]\n\
             # Biomes that should not have any title displayed when the player enters them.\n\
             # Example: [\"minecraft:plains\", \"minecraft:desert\"]\n",
        );
        write_list(&mut out, "blacklisted_biomes", &self.biomes);
        out.push_str(
            "\n[dimensions]\n\
             # Dimensions that should not have any title displayed when the player enters them.\n\
             # Example: [\"minecraft:overworld\", \"minecraft:the_nether\"]\n",
        );
        write_list(&mut out, "blacklisted_dimensions", &self.dimensions);
        out
    }

    /// Read the file; a missing or unreadable list keeps its default.
    pub fn parse(source: &str) -> Self {
        let mut lists = Self::default();
        let mut section = String::new();
        let mut pending: Option<(String, String)> = None;
        for raw in source.lines() {
            let line = strip_comment(raw).trim();
            if let Some((name, mut value)) = pending.take() {
                value.push(' ');
                value.push_str(line);
                if line.contains(']') {
                    lists.assign(&section, &name, &value);
                } else {
                    pending = Some((name, value));
                }
                continue;
            }
            if line.is_empty() {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
                section = name.trim().to_ascii_lowercase();
                continue;
            }
            let Some((name, value)) = line.split_once('=') else {
                continue;
            };
            let (name, value) = (name.trim().to_ascii_lowercase(), value.trim().to_owned());
            if value.starts_with('[') && !value.contains(']') {
                pending = Some((name, value));
            } else {
                lists.assign(&section, &name, &value);
            }
        }
        lists
    }

    fn assign(&mut self, section: &str, name: &str, value: &str) {
        match (section, name) {
            ("biomes", "blacklisted_biomes") => self.biomes = parse_array(value),
            ("dimensions", "blacklisted_dimensions") => self.dimensions = parse_array(value),
            _ => {}
        }
    }
}

/// `["a", "b"]`, or the original's `[a, b]`, as a list of ids.
fn parse_array(value: &str) -> Vec<String> {
    let inner = value.trim();
    let inner = inner.strip_prefix('[').unwrap_or(inner);
    let inner = inner.rsplit_once(']').map_or(inner, |(head, _)| head);
    inner
        .split(',')
        .map(|item| item.trim().trim_matches(['"', '\'']).trim())
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The text before an unquoted `#`.
fn strip_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    for (i, c) in line.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '#') => return &line[..i],
            _ => {}
        }
    }
    line
}

/// Register the client file `config/travelerstitles.toml`; its text goes to `state`
/// when it loads and each time it changes on disk.
pub fn register_mod_config(r: &mut dyn Registrar, state: &Arc<Mutex<crate::State>>) {
    let state = Arc::clone(state);
    let spec = ConfigSpec::new(ConfigKind::Client, Lists::default().to_toml()).file(CONFIG_FILE);
    let registered = r.register_config(
        spec,
        Some(Arc::new(
            move |event: ConfigEvent<'_>, snapshot: &ConfigSnapshot| {
                if matches!(event, ConfigEvent::Loading(_) | ConfigEvent::Reloading(_)) {
                    let lists = Lists::parse(&snapshot.text);
                    crate::with_state(&state, |state| state.lists = lists);
                }
                Ok(())
            },
        )),
    );
    if let Err(error) = registered {
        eprintln!("travelerstitles: config not registered: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_file_reads_back_as_the_defaults() {
        let lists = Lists::default();
        assert_eq!(Lists::parse(&lists.to_toml()), lists);
        assert_eq!(lists.biomes.len(), 3);
        assert!(lists.dimensions.is_empty());
    }

    #[test]
    fn lists_read_quoted_unquoted_and_multiline_arrays() {
        let lists = Lists::parse(
            "[biomes]\nblacklisted_biomes = [\"a:b\", 'c:d'] # note\n\
             [dimensions]\nblacklisted_dimensions = [\n  \"minecraft:overworld\",\n  x:y,\n]\n",
        );
        assert_eq!(lists.biomes, ["a:b", "c:d"]);
        assert_eq!(lists.dimensions, ["minecraft:overworld", "x:y"]);
    }

    #[test]
    fn an_empty_list_is_empty_and_bad_input_keeps_defaults() {
        let lists = Lists::parse("[biomes]\nblacklisted_biomes = []\n");
        assert!(lists.biomes.is_empty());
        let junk = Lists::parse("= = [\n\u{0}\n[[\n");
        assert_eq!(junk, Lists::default());
    }

    #[test]
    fn a_hash_inside_quotes_is_not_a_comment() {
        assert_eq!(strip_comment("a = \"x#y\" # z"), "a = \"x#y\" ");
    }

    #[test]
    fn the_config_file_is_named_for_the_mod() {
        assert!(CONFIG_FILE.starts_with(MOD_ID));
    }

    #[test]
    fn every_setting_has_a_category_by_its_name() {
        assert!(category("biomes_enabled").ends_with("biomes"));
        assert!(category("dimensions_color").ends_with("dimensions"));
        assert!(category("sound_biomes_pitch").ends_with("sounds"));
        assert!(category("sound_dimensions_volume").ends_with("sounds"));
    }

    #[test]
    fn setting_keys_are_valid_and_unique() {
        let mut keys = Vec::new();
        for kind in [Kind::Biome, Kind::Dimension] {
            for name in ["enabled", "fade_in", "color", "surface_only"] {
                keys.push(key(kind, name));
            }
        }
        for k in &keys {
            assert!(k.len() <= 64 && k.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'));
        }
        let mut sorted = keys.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), keys.len());
    }
}
