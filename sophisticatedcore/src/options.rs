//! The options of the three jars' configs that this port's rules use (`sophisticatedbackpacks-server.toml`,
//! `sophisticatedstorage-server.toml`, `sophisticatedcore-client.toml`), as mod settings with the jars' defaults.

use std::sync::{OnceLock, RwLock};

use mod_api::{ConfigPath, Host};

use crate::storage_items::{self, Family};

pub const MOD: &str = "sophisticatedcore";

pub mod key {
    pub const CHEST_LOOT: &str = "chest_loot_enabled";
    pub const IN_CONTAINER_ITEMS: &str = "can_be_placed_in_container_items";
    pub const SLOWNESS: &str = "too_many_backpacks_slowness";
    pub const MAX_BACKPACKS: &str = "max_number_of_backpacks";
    pub const SLOWNESS_RATIO: &str = "slowness_levels_per_additional_backpack";
    pub const ONLY_WORN: &str = "only_worn_backpack_triggers_upgrades";
    pub const DROP_PACKED: &str = "drop_packed";
    pub const TOO_MANY_DROPS: &str = "too_many_item_entity_drops";
    pub const SINGLE_WOOD: &str = "show_single_wood_variant_only";
    pub const MAGNET_RANGE: &str = "magnet_range";
    pub const ADVANCED_MAGNET_RANGE: &str = "advanced_magnet_range";
    pub const MOB_BACKPACK_CHANCE: &str = "mob_backpack_chance";
    pub const MOB_BACKPACK_LOOT: &str = "mob_backpack_loot";
    pub const MOB_BUFF_HEALTH: &str = "mob_backpack_buff_health";
    pub const MOB_POTIONS: &str = "mob_backpack_potion_effects";
    pub const MOB_ARMOUR: &str = "mob_backpack_armour";
    pub const BACKPACK_DROP_CHANCE: &str = "mob_backpack_drop_chance";
    pub const MOB_JUKEBOX: &str = "mob_backpack_plays_jukebox";
    pub const LOOTING_CHANCE: &str = "mob_backpack_looting_chance";
    pub const BACKPACK_CONTAINERS_DENIED: &str = "backpack_container_items_disallowed";
    pub const STORAGE_CONTAINERS_DENIED: &str = "shulker_box_container_items_disallowed";
}

const TIERS: [&str; 6] = ["leather", "copper", "iron", "gold", "diamond", "netherite"];
const BACKPACK_SLOTS: [i64; 6] = [27, 45, 54, 81, 108, 120];
const BACKPACK_UPGRADES: [i64; 6] = [1, 1, 2, 3, 5, 7];
const STORAGE_SLOTS: [i64; 6] = [27, 45, 54, 81, 108, 132];
const STORAGE_UPGRADES: [i64; 6] = [1, 1, 2, 3, 4, 5];

/// The upgrade-group limits (`maxUpgradesPerStorage`): backpack and storage, per group.
pub const GROUPS: [&str; 3] = ["stack_upgrades", "furnace_upgrades", "jukebox_upgrade"];
const BACKPACK_GROUP_DEFAULTS: [i64; 3] = [3, 1, 1];
const STORAGE_GROUP_DEFAULTS: [i64; 3] = [2, 1, 1];

/// The filtered upgrades and their `filterSlots` defaults, basic and advanced.
pub const FILTERED: [(&str, i64, i64); 9] = [
    ("pickup", 9, 16),
    ("filter", 9, 16),
    ("magnet", 9, 16),
    ("feeding", 9, 16),
    ("compacting", 9, 16),
    ("void", 9, 16),
    ("restock", 9, 16),
    ("deposit", 9, 16),
    ("refill", 6, 12),
];
/// `JukeboxUpgradeConfig.numberOfSlots` of the advanced jukebox upgrade (the basic one holds one disc).
const JUKEBOX_SLOTS: i64 = 12;
/// `HopperUpgradeConfig`: ticks between transfers and the most items per transfer, basic then advanced.
const HOPPER_TICKS: [i64; 2] = [8, 2];
const HOPPER_STACK: [i64; 2] = [1, 4];

/// The cooking upgrades, in the order of [`Tune::cooking`]: the manual ones, then the automatic ones.
pub const COOKING: [&str; 6] = [
    "smelting_upgrade",
    "smoking_upgrade",
    "blasting_upgrade",
    "auto_smelting_upgrade",
    "auto_smoking_upgrade",
    "auto_blasting_upgrade",
];

pub enum Kind {
    Toggle(bool),
    /// Minimum, maximum, step, default.
    Range(f64, f64, f64, f64),
}

pub const SPECS: &[(&str, Kind)] = &[
    (key::CHEST_LOOT, Kind::Toggle(true)),
    (key::IN_CONTAINER_ITEMS, Kind::Toggle(false)),
    (key::SLOWNESS, Kind::Toggle(false)),
    (key::MAX_BACKPACKS, Kind::Range(1.0, 27.0, 1.0, 3.0)),
    (key::SLOWNESS_RATIO, Kind::Range(0.1, 5.0, 0.1, 1.0)),
    (key::ONLY_WORN, Kind::Toggle(false)),
    (key::DROP_PACKED, Kind::Toggle(false)),
    (key::TOO_MANY_DROPS, Kind::Range(1.0, 1000.0, 1.0, 200.0)),
    (key::SINGLE_WOOD, Kind::Toggle(false)),
    (key::MAGNET_RANGE, Kind::Range(1.0, 16.0, 1.0, 3.0)),
    (key::ADVANCED_MAGNET_RANGE, Kind::Range(1.0, 16.0, 1.0, 5.0)),
    (key::MOB_BACKPACK_CHANCE, Kind::Range(0.0, 1.0, 0.005, 0.01)),
    (key::MOB_BACKPACK_LOOT, Kind::Toggle(true)),
    (key::MOB_BUFF_HEALTH, Kind::Toggle(true)),
    (key::MOB_POTIONS, Kind::Toggle(true)),
    (key::MOB_ARMOUR, Kind::Toggle(true)),
    (key::BACKPACK_DROP_CHANCE, Kind::Range(0.0, 1.0, 0.01, 0.5)),
    (key::MOB_JUKEBOX, Kind::Toggle(true)),
    (key::LOOTING_CHANCE, Kind::Range(0.0, 0.3, 0.01, 0.15)),
    (key::BACKPACK_CONTAINERS_DENIED, Kind::Toggle(false)),
    (key::STORAGE_CONTAINERS_DENIED, Kind::Toggle(false)),
];

/// The options as they stand now.
pub struct Options {
    pub in_container_items: bool,
    pub slowness: bool,
    pub max_backpacks: usize,
    pub slowness_ratio: f64,
    pub only_worn: bool,
    pub drop_packed: bool,
    pub too_many_drops: usize,
    pub single_wood: bool,
    pub magnet_range: f64,
    pub advanced_magnet_range: f64,
    pub mob_chance: f64,
    pub mob_loot: bool,
    pub mob_health: bool,
    pub mob_potions: bool,
    pub mob_jukebox: bool,
    pub mob_armour: bool,
    pub drop_chance: f64,
    pub looting_chance: f64,
    /// `containerItemsDisallowed` of the backpacks and of the shulker boxes.
    pub backpack_containers_denied: bool,
    pub storage_containers_denied: bool,
}

fn flag(text: Option<&str>, default: bool) -> bool {
    text.map_or(default, |t| t.trim() == "true")
}

fn number(text: Option<&str>, default: f64) -> f64 {
    text.and_then(|t| t.trim().parse().ok()).unwrap_or(default)
}

impl Options {
    pub fn read(host: &dyn Host) -> Self {
        let get = |key: &str| host.setting(MOD, key);
        Self {
            in_container_items: flag(get(key::IN_CONTAINER_ITEMS).as_deref(), false),
            slowness: flag(get(key::SLOWNESS).as_deref(), false),
            max_backpacks: number(get(key::MAX_BACKPACKS).as_deref(), 3.0).max(1.0) as usize,
            slowness_ratio: number(get(key::SLOWNESS_RATIO).as_deref(), 1.0),
            only_worn: flag(get(key::ONLY_WORN).as_deref(), false),
            drop_packed: flag(get(key::DROP_PACKED).as_deref(), false),
            too_many_drops: number(get(key::TOO_MANY_DROPS).as_deref(), 200.0).max(1.0) as usize,
            single_wood: flag(get(key::SINGLE_WOOD).as_deref(), false),
            magnet_range: number(get(key::MAGNET_RANGE).as_deref(), 3.0),
            advanced_magnet_range: number(get(key::ADVANCED_MAGNET_RANGE).as_deref(), 5.0),
            mob_chance: number(get(key::MOB_BACKPACK_CHANCE).as_deref(), 0.01),
            mob_loot: flag(get(key::MOB_BACKPACK_LOOT).as_deref(), true),
            mob_health: flag(get(key::MOB_BUFF_HEALTH).as_deref(), true),
            mob_potions: flag(get(key::MOB_POTIONS).as_deref(), true),
            mob_jukebox: flag(get(key::MOB_JUKEBOX).as_deref(), true),
            mob_armour: flag(get(key::MOB_ARMOUR).as_deref(), true),
            drop_chance: number(get(key::BACKPACK_DROP_CHANCE).as_deref(), 0.5),
            looting_chance: number(get(key::LOOTING_CHANCE).as_deref(), 0.15),
            backpack_containers_denied: flag(
                get(key::BACKPACK_CONTAINERS_DENIED).as_deref(),
                false,
            ),
            storage_containers_denied: flag(get(key::STORAGE_CONTAINERS_DENIED).as_deref(), false),
        }
    }

    /// The magnet's reach for a basic or advanced upgrade.
    pub fn magnet(&self, advanced: bool) -> f64 {
        if advanced {
            self.advanced_magnet_range
        } else {
            self.magnet_range
        }
    }
}

/// `CommonEventHandler.onWorldTick`: the Slowness amplifier for `count` backpacks carried, if there are more than
/// `max`: the levels are the excess times the ratio, rounded up, at most 10, and the amplifier is one less.
pub fn slowness_amplifier(count: usize, max: usize, ratio: f64) -> Option<i32> {
    if count <= max {
        return None;
    }
    let levels = (((count - max) as f64 * ratio).ceil() as i32).min(10);
    Some(levels - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tables_start_at_the_jars_defaults() {
        let t = read_tune(&|_| None);
        assert_eq!(t.backpack_slots, [27, 45, 54, 81, 108, 120]);
        assert_eq!(t.backpack_upgrades, [1, 1, 2, 3, 5, 7]);
        assert_eq!(t.limited_base[2][0], 10);
        assert_eq!(t.limited_base[0][5], 160);
        assert_eq!(t.storage_groups, [2, 1, 1]);
        let t = read_tune(&|k| (k == "backpack_iron_slots").then(|| "60".to_owned()));
        assert_eq!(t.backpack_slots[2], 60);
    }

    #[test]
    fn upgrade_settings_start_at_the_jars_defaults() {
        let t = read_tune(&|_| None);
        assert_eq!(&t.filters[..2], &[9, 16]);
        assert_eq!(&t.filters[16..], &[6, 12]);
        assert_eq!(t.jukebox_slots, 12);
        assert_eq!((t.hopper_ticks, t.hopper_stack), ([8, 2], [1, 4]));
        assert_eq!(t.void_anything, [true, true]);
        let t = read_tune(&|k| match k {
            "advanced_void_upgrade_filter_slots" => Some("30".to_owned()),
            "hopper_upgrade_transfer_speed_ticks" => Some("20".to_owned()),
            "void_upgrade_void_anything_enabled" => Some("false".to_owned()),
            _ => None,
        });
        assert_eq!(
            t.filters[11], 20,
            "filter slots stop at the setting's maximum"
        );
        assert_eq!(t.hopper_ticks[0], 20);
        assert_eq!(t.void_anything, [false, true]);
    }

    #[test]
    fn list_entries_take_wildcards() {
        let l = vec![
            "minecraft:*_shulker_box".to_owned(),
            "mymod:thing".to_owned(),
        ];
        assert!(listed(&l, "minecraft:red_shulker_box"));
        assert!(listed(&l, "mymod:thing"));
        assert!(!listed(&l, "minecraft:chest"));
    }

    #[test]
    fn carrying_too_many_backpacks_slows_by_the_excess() {
        assert_eq!(slowness_amplifier(3, 3, 1.0), None);
        assert_eq!(slowness_amplifier(4, 3, 1.0), Some(0));
        assert_eq!(slowness_amplifier(6, 3, 1.0), Some(2));
        assert_eq!(slowness_amplifier(5, 3, 0.5), Some(0));
        assert_eq!(slowness_amplifier(27, 1, 5.0), Some(9));
    }
}

fn int(text: Option<String>, default: i64, min: i64, max: i64) -> i64 {
    text.and_then(|t| t.trim().parse::<f64>().ok())
        .map_or(default, |v| v as i64)
        .clamp(min, max)
}

/// Every setting of the tables below, as (key, kind): the per-tier slot counts, upgrade slots, limited barrel
/// multipliers and per-group upgrade limits.
pub fn tier_specs() -> Vec<(String, Kind)> {
    let mut out = Vec::new();
    let mut range = |key: String, min: i64, max: i64, default: i64| {
        out.push((
            key,
            Kind::Range(min as f64, max as f64, 1.0, default as f64),
        ));
    };
    for (t, name) in TIERS.iter().enumerate() {
        range(format!("backpack_{name}_slots"), 1, 144, BACKPACK_SLOTS[t]);
        range(
            format!("backpack_{name}_upgrade_slots"),
            0,
            10,
            BACKPACK_UPGRADES[t],
        );
        range(format!("storage_{name}_slots"), 1, 180, STORAGE_SLOTS[t]);
        range(
            format!("storage_{name}_upgrade_slots"),
            0,
            10,
            STORAGE_UPGRADES[t],
        );
        range(
            format!("limited_barrel_{name}_upgrade_slots"),
            0,
            10,
            STORAGE_UPGRADES[t],
        );
        for size in 1..=4 {
            let base = storage_items::STORAGE
                .iter()
                .find(|s| {
                    s.family == Family::LimitedBarrel && s.slots == size && s.tier as usize == t
                })
                .map_or(1, |s| i64::from(s.default_base()));
            range(
                format!("limited_barrel_{size}_{name}_base_multiplier"),
                1,
                8192,
                base,
            );
        }
    }
    for (name, basic, advanced) in FILTERED {
        range(format!("{name}_upgrade_filter_slots"), 1, 20, basic);
        range(
            format!("advanced_{name}_upgrade_filter_slots"),
            1,
            20,
            advanced,
        );
    }
    range(
        "advanced_jukebox_upgrade_slots".to_owned(),
        1,
        16,
        JUKEBOX_SLOTS,
    );
    for (a, name) in ["hopper_upgrade", "advanced_hopper_upgrade"]
        .iter()
        .enumerate()
    {
        range(
            format!("{name}_transfer_speed_ticks"),
            1,
            100,
            HOPPER_TICKS[a],
        );
        range(
            format!("{name}_max_transfer_stack_size"),
            1,
            64,
            HOPPER_STACK[a],
        );
    }
    for (g, name) in GROUPS.iter().enumerate() {
        range(
            format!("backpack_max_{name}"),
            0,
            10,
            BACKPACK_GROUP_DEFAULTS[g],
        );
        range(
            format!("storage_max_{name}"),
            0,
            10,
            STORAGE_GROUP_DEFAULTS[g],
        );
    }
    for name in &COOKING[3..] {
        range(format!("{name}_input_filter_slots"), 1, 20, 8);
        range(format!("{name}_fuel_filter_slots"), 1, 20, 4);
    }
    for name in ["void_upgrade", "advanced_void_upgrade"] {
        out.push((format!("{name}_void_anything_enabled"), Kind::Toggle(true)));
    }
    for name in COOKING {
        for key in ["smelting_speed_multiplier", "fuel_efficiency_multiplier"] {
            out.push((format!("{name}_{key}"), Kind::Range(0.25, 4.0, 0.05, 1.0)));
        }
    }
    out
}

/// The tables the settings give the storage rules, as of the last refresh.
#[derive(Clone, Default)]
pub struct Tune {
    pub backpack_slots: [usize; 6],
    pub backpack_upgrades: [usize; 6],
    pub storage_slots: [usize; 6],
    pub storage_upgrades: [usize; 6],
    pub limited_upgrades: [usize; 6],
    /// By size (1 to 4, index 0 to 3) then tier.
    pub limited_base: [[u32; 6]; 4],
    pub backpack_groups: [usize; 3],
    pub storage_groups: [usize; 3],
    /// Filter slots by [`FILTERED`] index, basic at `2 * i`, advanced after it.
    pub filters: [usize; 18],
    pub jukebox_slots: usize,
    /// Basic then advanced.
    pub hopper_ticks: [usize; 2],
    pub hopper_stack: [usize; 2],
    pub void_anything: [bool; 2],
    /// Smelting speed and fuel efficiency multipliers by [`COOKING`] index.
    pub cooking: [(f64, f64); 6],
    /// Input and fuel filter slots of the automatic cooking upgrades.
    pub cook_filters: [[usize; 2]; 3],
}

static TUNE: RwLock<Option<Tune>> = RwLock::new(None);

/// The tables as of the last refresh (the jar's defaults before the first).
pub fn tune() -> Tune {
    if let Some(t) = TUNE.read().ok().and_then(|g| g.clone()) {
        return t;
    }
    read_tune(&|_| None)
}

fn read_tune(get: &dyn Fn(&str) -> Option<String>) -> Tune {
    let mut t = Tune::default();
    for (i, name) in TIERS.iter().enumerate() {
        let n = |key: String, d: i64, lo: i64, hi: i64| int(get(&key), d, lo, hi);
        t.backpack_slots[i] =
            n(format!("backpack_{name}_slots"), BACKPACK_SLOTS[i], 1, 144) as usize;
        t.backpack_upgrades[i] = n(
            format!("backpack_{name}_upgrade_slots"),
            BACKPACK_UPGRADES[i],
            0,
            10,
        ) as usize;
        t.storage_slots[i] = n(format!("storage_{name}_slots"), STORAGE_SLOTS[i], 1, 180) as usize;
        t.storage_upgrades[i] = n(
            format!("storage_{name}_upgrade_slots"),
            STORAGE_UPGRADES[i],
            0,
            10,
        ) as usize;
        t.limited_upgrades[i] = n(
            format!("limited_barrel_{name}_upgrade_slots"),
            STORAGE_UPGRADES[i],
            0,
            10,
        ) as usize;
        for size in 1..=4usize {
            let default = storage_items::STORAGE
                .iter()
                .find(|s| {
                    s.family == Family::LimitedBarrel && s.slots == size && s.tier as usize == i
                })
                .map_or(1, |s| i64::from(s.default_base()));
            t.limited_base[size - 1][i] = n(
                format!("limited_barrel_{size}_{name}_base_multiplier"),
                default,
                1,
                8192,
            ) as u32;
        }
    }
    for (i, (name, basic, advanced)) in FILTERED.iter().enumerate() {
        t.filters[2 * i] =
            int(get(&format!("{name}_upgrade_filter_slots")), *basic, 1, 20) as usize;
        t.filters[2 * i + 1] = int(
            get(&format!("advanced_{name}_upgrade_filter_slots")),
            *advanced,
            1,
            20,
        ) as usize;
    }
    t.jukebox_slots = int(get("advanced_jukebox_upgrade_slots"), JUKEBOX_SLOTS, 1, 16) as usize;
    for (a, name) in ["hopper_upgrade", "advanced_hopper_upgrade"]
        .iter()
        .enumerate()
    {
        t.hopper_ticks[a] = int(
            get(&format!("{name}_transfer_speed_ticks")),
            HOPPER_TICKS[a],
            1,
            100,
        ) as usize;
        t.hopper_stack[a] = int(
            get(&format!("{name}_max_transfer_stack_size")),
            HOPPER_STACK[a],
            1,
            64,
        ) as usize;
    }
    for (a, name) in ["void_upgrade", "advanced_void_upgrade"].iter().enumerate() {
        t.void_anything[a] = flag(
            get(&format!("{name}_void_anything_enabled")).as_deref(),
            true,
        );
    }
    for (i, name) in COOKING.iter().enumerate() {
        let m = |key: &str| {
            get(&format!("{name}_{key}"))
                .and_then(|t| t.trim().parse::<f64>().ok())
                .map_or(1.0, |v| v.clamp(0.25, 4.0))
        };
        t.cooking[i] = (
            m("smelting_speed_multiplier"),
            m("fuel_efficiency_multiplier"),
        );
        if i >= 3 {
            t.cook_filters[i - 3] = [
                int(get(&format!("{name}_input_filter_slots")), 8, 1, 20) as usize,
                int(get(&format!("{name}_fuel_filter_slots")), 4, 1, 20) as usize,
            ];
        }
    }
    for (g, name) in GROUPS.iter().enumerate() {
        t.backpack_groups[g] = int(
            get(&format!("backpack_max_{name}")),
            BACKPACK_GROUP_DEFAULTS[g],
            0,
            10,
        ) as usize;
        t.storage_groups[g] = int(
            get(&format!("storage_max_{name}")),
            STORAGE_GROUP_DEFAULTS[g],
            0,
            10,
        ) as usize;
    }
    t
}

/// Filter slots of the filtered upgrade `base` (`pickup`, `void`...).
pub fn filter_slots(base: &str, advanced: bool) -> usize {
    let at = FILTERED.iter().position(|f| f.0 == base).unwrap_or(0);
    let i = 2 * at + usize::from(advanced);
    match TUNE.read() {
        Ok(g) if g.is_some() => g.as_ref().map_or(0, |t| t.filters[i]),
        _ => {
            (if advanced {
                FILTERED[at].2
            } else {
                FILTERED[at].1
            }) as usize
        }
    }
}

/// Re-reads the settings into the tables.
pub fn refresh(host: &dyn Host) {
    let t = read_tune(&|key| host.setting(MOD, key));
    if let Ok(mut g) = TUNE.write() {
        *g = Some(t);
    }
}

/// The text lists of `config/sophisticatedcore/`: items the backpacks refuse, items the shulker boxes refuse,
/// blocks the inventory interaction upgrades leave alone, and items the stack upgrades don't multiply. One id per
/// line, `*` for any run of characters, `;` starts a comment.
pub struct Lists {
    pub backpack_disallowed: Vec<String>,
    pub shulker_disallowed: Vec<String>,
    pub no_interaction: Vec<String>,
    pub non_stackable: Vec<String>,
    /// `entity|loot table` lines: which monsters may wear a backpack and what fills it.
    pub entity_loot: Vec<String>,
    /// Discs entities with a jukebox upgrade never play (`discBlockList`).
    pub disc_block: Vec<String>,
}

fn read_list(host: &dyn Host, file: &str, help: &str, defaults: &str) -> Vec<String> {
    let text = match ConfigPath::new(&format!("sophisticatedcore/{file}")) {
        Ok(path) => {
            let files = host.config_files();
            match files.read(&path) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                Err(_) => {
                    let _ = files.write(&path, format!("; {help}\n; One id per line; * stands for any run of characters.\n{defaults}").as_bytes());
                    defaults.to_owned()
                }
            }
        }
        Err(_) => defaults.to_owned(),
    };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with(';'))
        .map(str::to_owned)
        .collect()
}

/// Whether `name` matches a list entry (`*` for any run of characters).
pub fn listed(list: &[String], name: &str) -> bool {
    list.iter().any(|p| wildcard(name, p))
}

fn wildcard(name: &str, pattern: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    let [first, .., last] = parts.as_slice() else {
        return name == pattern;
    };
    if !name.starts_with(first) || name.len() < first.len() + last.len() || !name.ends_with(last) {
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

impl Lists {
    /// The lists, read once per run.
    pub fn get(host: &dyn Host) -> &'static Lists {
        static LISTS: OnceLock<Lists> = OnceLock::new();
        LISTS.get_or_init(|| Lists {
            backpack_disallowed: read_list(host, "backpack_disallowed.txt", "Items backpacks refuse.", ""),
            shulker_disallowed: read_list(host, "shulker_box_disallowed.txt", "Items shulker boxes refuse.", ""),
            no_interaction: read_list(host, "no_interaction_blocks.txt", "Blocks the restock and deposit upgrades leave alone.", ""),
            entity_loot: read_list(
                host,
                "entity_loot_tables.txt",
                "Monsters that may spawn wearing a backpack, and the loot table that fills it: entity|table.",
                crate::mobs::DEFAULT_LOOT,
            ),
            disc_block: read_list(
                host,
                "disc_block_list.txt",
                "Music discs that entities wearing a backpack never play.",
                "botania:record_gaia_1\nbotania:record_gaia_2\n",
            ),
            non_stackable: read_list(
                host,
                "non_stackable.txt",
                "Items the stack upgrades don't multiply.",
                "minecraft:bundle\nminecraft:shulker_box\n",
            ),
        })
    }
}
