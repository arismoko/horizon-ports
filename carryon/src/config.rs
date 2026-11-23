//! Carry On's options (`CarryConfig.Common`): the numbers and switches are mod settings, the
//! lists (what may be picked up, what may carry a stack, which placement properties are
//! recomputed) are text files in `config/carryon/`, written with the original's defaults when
//! missing. A list line is an id, an id with `*` for any run of characters, or a `#tag`; a line
//! starting with `;` is a comment.

use std::sync::OnceLock;

use mod_api::{ConfigPath, Host};

pub const MOD_ID: &str = "carryon";

/// The settings, with the original's defaults, as `CarryConfig.Common.Settings`.
pub struct Settings {
    pub max_distance: f64,
    pub scale_distance: bool,
    pub max_entity_width: f64,
    pub max_entity_height: f64,
    pub relative_player_scale: bool,
    pub block_slowness: f64,
    pub entity_slowness: f64,
    pub max_stack: usize,
    pub heavy_tiles: bool,
    pub pickup_all_blocks: bool,
    pub slowness_in_creative: bool,
    pub pickup_hostile: bool,
    pub heavy_entities: bool,
    pub allow_babies: bool,
    pub whitelist_blocks: bool,
    pub whitelist_entities: bool,
    pub whitelist_stacking: bool,
    pub hit_while_carrying: bool,
    pub drop_when_hit: bool,
    pub stackable_entities: bool,
    pub size_matters_stacking: bool,
    pub pickup_unbreakable: bool,
}

/// The setting keys, in the order [`SPECS`] lists them.
pub mod key {
    pub const MAX_DISTANCE: &str = "max_distance";
    pub const SCALE_DISTANCE: &str = "scale_distance";
    pub const MAX_ENTITY_WIDTH: &str = "max_entity_width";
    pub const MAX_ENTITY_HEIGHT: &str = "max_entity_height";
    pub const RELATIVE_SCALE: &str = "relative_player_scale";
    pub const BLOCK_SLOWNESS: &str = "block_slowness_multiplier";
    pub const ENTITY_SLOWNESS: &str = "entity_slowness_multiplier";
    pub const MAX_STACK: &str = "max_entity_stack_limit";
    pub const HEAVY_TILES: &str = "heavy_tiles";
    pub const PICKUP_ALL: &str = "pickup_all_blocks";
    pub const SLOWNESS_CREATIVE: &str = "slowness_in_creative";
    pub const PICKUP_HOSTILE: &str = "pickup_hostile_mobs";
    pub const HEAVY_ENTITIES: &str = "heavy_entities";
    pub const ALLOW_BABIES: &str = "allow_babies";
    pub const WHITELIST_BLOCKS: &str = "use_whitelist_blocks";
    pub const WHITELIST_ENTITIES: &str = "use_whitelist_entities";
    pub const WHITELIST_STACKING: &str = "use_whitelist_stacking";
    pub const HIT_WHILE_CARRYING: &str = "hit_while_carrying";
    pub const DROP_WHEN_HIT: &str = "drop_carried_when_hit";
    pub const STACKABLE: &str = "stackable_entities";
    pub const SIZE_MATTERS: &str = "entity_size_matters_stacking";
    pub const PICKUP_UNBREAKABLE: &str = "pickup_unbreakable_blocks";
    pub const FACE_PLAYER: &str = "face_player";
}

/// One setting's kind and default.
pub enum Kind {
    Toggle(bool),
    /// Minimum, maximum, step, default.
    Range(f64, f64, f64, f64),
}

/// Every setting, as `CarryConfig`'s `Settings` and `Client` categories declare them.
pub const SPECS: &[(&str, Kind)] = &[
    (key::MAX_DISTANCE, Kind::Range(0.0, 16.0, 0.1, 2.5)),
    (key::SCALE_DISTANCE, Kind::Toggle(true)),
    (key::MAX_ENTITY_WIDTH, Kind::Range(0.0, 10.0, 0.1, 1.5)),
    (key::MAX_ENTITY_HEIGHT, Kind::Range(0.0, 10.0, 0.1, 2.5)),
    (key::RELATIVE_SCALE, Kind::Toggle(true)),
    (key::BLOCK_SLOWNESS, Kind::Range(0.0, 10.0, 0.1, 1.0)),
    (key::ENTITY_SLOWNESS, Kind::Range(0.0, 10.0, 0.1, 1.0)),
    (key::MAX_STACK, Kind::Range(1.0, 64.0, 1.0, 10.0)),
    (key::HEAVY_TILES, Kind::Toggle(true)),
    (key::PICKUP_ALL, Kind::Toggle(false)),
    (key::SLOWNESS_CREATIVE, Kind::Toggle(true)),
    (key::PICKUP_HOSTILE, Kind::Toggle(false)),
    (key::HEAVY_ENTITIES, Kind::Toggle(true)),
    (key::ALLOW_BABIES, Kind::Toggle(false)),
    (key::WHITELIST_BLOCKS, Kind::Toggle(false)),
    (key::WHITELIST_ENTITIES, Kind::Toggle(false)),
    (key::WHITELIST_STACKING, Kind::Toggle(false)),
    (key::HIT_WHILE_CARRYING, Kind::Toggle(false)),
    (key::DROP_WHEN_HIT, Kind::Toggle(false)),
    (key::STACKABLE, Kind::Toggle(true)),
    (key::SIZE_MATTERS, Kind::Toggle(true)),
    (key::PICKUP_UNBREAKABLE, Kind::Toggle(false)),
    (key::FACE_PLAYER, Kind::Toggle(true)),
];

pub fn flag(host: &dyn Host, key: &str, default: bool) -> bool {
    host.setting(MOD_ID, key)
        .map_or(default, |v| v.trim() == "true")
}

fn number(host: &dyn Host, key: &str, default: f64) -> f64 {
    host.setting(MOD_ID, key)
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

impl Settings {
    pub fn read(host: &dyn Host) -> Self {
        Self {
            max_distance: number(host, key::MAX_DISTANCE, 2.5),
            scale_distance: flag(host, key::SCALE_DISTANCE, true),
            max_entity_width: number(host, key::MAX_ENTITY_WIDTH, 1.5),
            max_entity_height: number(host, key::MAX_ENTITY_HEIGHT, 2.5),
            relative_player_scale: flag(host, key::RELATIVE_SCALE, true),
            block_slowness: number(host, key::BLOCK_SLOWNESS, 1.0),
            entity_slowness: number(host, key::ENTITY_SLOWNESS, 1.0),
            max_stack: number(host, key::MAX_STACK, 10.0).max(1.0) as usize,
            heavy_tiles: flag(host, key::HEAVY_TILES, true),
            pickup_all_blocks: flag(host, key::PICKUP_ALL, false),
            slowness_in_creative: flag(host, key::SLOWNESS_CREATIVE, true),
            pickup_hostile: flag(host, key::PICKUP_HOSTILE, false),
            heavy_entities: flag(host, key::HEAVY_ENTITIES, true),
            allow_babies: flag(host, key::ALLOW_BABIES, false),
            whitelist_blocks: flag(host, key::WHITELIST_BLOCKS, false),
            whitelist_entities: flag(host, key::WHITELIST_ENTITIES, false),
            whitelist_stacking: flag(host, key::WHITELIST_STACKING, false),
            hit_while_carrying: flag(host, key::HIT_WHILE_CARRYING, false),
            drop_when_hit: flag(host, key::DROP_WHEN_HIT, false),
            stackable_entities: flag(host, key::STACKABLE, true),
            size_matters_stacking: flag(host, key::SIZE_MATTERS, true),
            pickup_unbreakable: flag(host, key::PICKUP_UNBREAKABLE, false),
        }
    }
}

/// The lists, as the config files held them when first read.
pub struct Lists {
    pub forbidden_tiles: Vec<String>,
    pub forbidden_entities: Vec<String>,
    pub forbidden_stacking: Vec<String>,
    pub allowed_blocks: Vec<String>,
    pub allowed_entities: Vec<String>,
    pub allowed_stacking: Vec<String>,
    /// `modid:block` and the properties of it a placement recomputes.
    pub placement_exceptions: Vec<(String, Vec<String>)>,
}

/// The original's default `placementStateExceptions`.
const DEFAULT_EXCEPTIONS: &[&str] = &[
    "minecraft:chest[type]",
    "minecraft:stone_button[face]",
    "minecraft:vine[north,east,south,west,up]",
    "minecraft:creeper_head[rotation]",
    "minecraft:glow_lichen[north,east,south,west,up,down]",
    "minecraft:oak_sign[rotation]",
    "minecraft:oak_trapdoor[half]",
];

/// The lines of a list file: trimmed, with blanks and `;` comments left out.
pub fn parse_list(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with(';'))
        .map(str::to_owned)
        .collect()
}

/// `modid:block[a,b]` as the block and its properties.
pub fn parse_exception(line: &str) -> Option<(String, Vec<String>)> {
    let (name, rest) = line.split_once('[')?;
    let props = rest.split_once(']')?.0;
    Some((
        name.to_owned(),
        props
            .split(',')
            .map(|p| p.trim().to_owned())
            .filter(|p| !p.is_empty())
            .collect(),
    ))
}

/// Reads `config/carryon/<file>`; a missing file is written with `defaults` first.
fn load(host: &dyn Host, file: &str, defaults: &[&str], help: &str) -> Vec<String> {
    let Ok(path) = ConfigPath::new(&format!("{MOD_ID}/{file}")) else {
        return defaults.iter().map(|s| (*s).to_owned()).collect();
    };
    let files = host.config_files();
    match files.read(&path) {
        Ok(bytes) => parse_list(&String::from_utf8_lossy(&bytes)),
        Err(_) => {
            let mut text = format!(
                "; {help}\n; One entry per line: an id, an id with * for any run of characters, or a #tag.\n"
            );
            for d in defaults {
                text.push_str(d);
                text.push('\n');
            }
            let _ = files.write(&path, text.as_bytes());
            defaults.iter().map(|s| (*s).to_owned()).collect()
        }
    }
}

impl Lists {
    /// The lists, read once per run.
    pub fn get(host: &dyn Host) -> &'static Lists {
        static LISTS: OnceLock<Lists> = OnceLock::new();
        LISTS.get_or_init(|| {
            let exceptions = load(
                host,
                "placement_state_exceptions.txt",
                DEFAULT_EXCEPTIONS,
                "Properties a placed block recomputes instead of keeping, as modid:block[property,property].",
            );
            Lists {
                forbidden_tiles: load(host, "forbidden_blocks.txt", crate::FORBIDDEN_TILES, "Blocks that can't be picked up."),
                forbidden_entities: load(host, "forbidden_entities.txt", crate::FORBIDDEN_ENTITIES, "Entities that can't be picked up."),
                forbidden_stacking: load(host, "forbidden_stacking.txt", crate::FORBIDDEN_STACKING, "Entities that can't have others stacked on them."),
                allowed_blocks: load(host, "allowed_blocks.txt", &[], "Blocks that can be picked up (use_whitelist_blocks must be on)."),
                allowed_entities: load(host, "allowed_entities.txt", &[], "Entities that can be picked up (use_whitelist_entities must be on)."),
                allowed_stacking: load(host, "allowed_stacking.txt", &[], "Entities that can have others stacked on them (use_whitelist_stacking must be on)."),
                placement_exceptions: exceptions.iter().filter_map(|l| parse_exception(l)).collect(),
            }
        })
    }

    /// Whether a placed `block` recomputes its `property`.
    pub fn recomputes(&self, block: &str, property: &str) -> bool {
        self.placement_exceptions
            .iter()
            .any(|(name, props)| name == block && props.iter().any(|p| p == property))
    }
}

/// `StringHelper.matchesWildcards`: `*` stands for any run of characters.
pub fn wildcard(name: &str, pattern: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return name == pattern;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !name.starts_with(first)
        || !name[first.len()..].ends_with(last)
        || name.len() < first.len() + last.len()
    {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for mid in &parts[1..parts.len() - 1] {
        match rest.find(mid) {
            Some(i) => rest = &rest[i + mid.len()..],
            None => return false,
        }
    }
    true
}

/// Whether `name` matches an entry: an id pattern, or a `#tag` that `in_tag` answers.
pub fn listed(name: &str, list: &[String], in_tag: &dyn Fn(&str) -> bool) -> bool {
    list.iter().any(|p| match p.strip_prefix('#') {
        Some(tag) => in_tag(tag),
        None => wildcard(name, p),
    })
}

/// `ListHandler.isPermitted`: with the whitelist on, only what the allowed list names; with it
/// off, whatever the forbidden list does not.
pub fn permitted(
    name: &str,
    whitelist: bool,
    allowed: &[String],
    forbidden: &[String],
    in_tag: &dyn Fn(&str) -> bool,
) -> bool {
    if whitelist {
        listed(name, allowed, in_tag)
    } else {
        !listed(name, forbidden, in_tag)
    }
}

/// `CarryOnCommon.potionLevel` for a carried block of `data_len` characters of data, or a mob
/// of `area` (width times height, relative to the player's scale): the Slowness amplifier, up to
/// IV; with the heavy options off it is 1 before the multiplier.
pub fn slowness_level(block: bool, data_len: usize, area: f64, s: &Settings) -> i32 {
    let (base, multiplier) = if block {
        (
            if s.heavy_tiles {
                (data_len / 500) as i32
            } else {
                1
            },
            s.block_slowness,
        )
    } else {
        (
            if s.heavy_entities { area as i32 } else { 1 },
            s.entity_slowness,
        )
    };
    ((f64::from(base) * multiplier) as i32).min(4)
}

/// `SizeHelper.getPlayerScaleFactor`: sizes are measured against the player's own scale.
pub fn scale_factor(relative: bool, player_scale: f64) -> f64 {
    if relative {
        1.0 / player_scale.max(0.001)
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn a_list_file_skips_blanks_and_comments() {
        assert_eq!(
            parse_list("; note\n\n minecraft:chest \n#c:x\n"),
            v(&["minecraft:chest", "#c:x"])
        );
    }

    #[test]
    fn an_exception_names_its_properties() {
        assert_eq!(
            parse_exception("minecraft:vine[north,east]"),
            Some(("minecraft:vine".to_owned(), v(&["north", "east"])))
        );
        assert_eq!(parse_exception("minecraft:vine"), None);
    }

    #[test]
    fn the_whitelist_flips_the_meaning_of_the_lists() {
        let no_tag = |_: &str| false;
        let allowed = v(&["mymod:*"]);
        let forbidden = v(&["minecraft:bedrock"]);
        assert!(permitted("mymod:box", true, &allowed, &forbidden, &no_tag));
        assert!(!permitted(
            "minecraft:chest",
            true,
            &allowed,
            &forbidden,
            &no_tag
        ));
        assert!(permitted(
            "minecraft:chest",
            false,
            &allowed,
            &forbidden,
            &no_tag
        ));
        assert!(!permitted(
            "minecraft:bedrock",
            false,
            &allowed,
            &forbidden,
            &no_tag
        ));
        assert!(!permitted("x:y", false, &allowed, &v(&["#c:no"]), &|t| t == "c:no"));
    }

    #[test]
    fn wildcards() {
        assert!(wildcard("create:belt", "create:*"));
        assert!(wildcard("a:bcd", "a:*d"));
        assert!(!wildcard("a:bcd", "b:*"));
        assert!(wildcard("x:y", "x:y"));
    }

    fn defaults() -> Settings {
        Settings {
            max_distance: 2.5,
            scale_distance: true,
            max_entity_width: 1.5,
            max_entity_height: 2.5,
            relative_player_scale: true,
            block_slowness: 1.0,
            entity_slowness: 1.0,
            max_stack: 10,
            heavy_tiles: true,
            pickup_all_blocks: false,
            slowness_in_creative: true,
            pickup_hostile: false,
            heavy_entities: true,
            allow_babies: false,
            whitelist_blocks: false,
            whitelist_entities: false,
            whitelist_stacking: false,
            hit_while_carrying: false,
            drop_when_hit: false,
            stackable_entities: true,
            size_matters_stacking: true,
            pickup_unbreakable: false,
        }
    }

    #[test]
    fn heavier_things_slow_more_and_the_options_scale_it() {
        let mut s = defaults();
        assert_eq!(slowness_level(true, 1200, 0.0, &s), 2);
        assert_eq!(slowness_level(true, 90000, 0.0, &s), 4);
        assert_eq!(slowness_level(false, 0, 2.0, &s), 2);
        s.heavy_tiles = false;
        assert_eq!(slowness_level(true, 1200, 0.0, &s), 1);
        s.entity_slowness = 0.5;
        assert_eq!(slowness_level(false, 0, 4.0, &s), 2);
    }

    #[test]
    fn a_small_player_sees_things_bigger() {
        assert_eq!(scale_factor(true, 0.5), 2.0);
        assert_eq!(scale_factor(false, 0.5), 1.0);
        assert_eq!(scale_factor(true, 0.0), 1000.0);
    }
}
