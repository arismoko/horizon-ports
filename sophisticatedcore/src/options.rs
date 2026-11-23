//! The options of the three jars' configs that this port's rules use (`sophisticatedbackpacks-server.toml`,
//! `sophisticatedstorage-server.toml`, `sophisticatedcore-client.toml`), as mod settings with the jars' defaults.

use mod_api::Host;

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
}

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
    fn carrying_too_many_backpacks_slows_by_the_excess() {
        assert_eq!(slowness_amplifier(3, 3, 1.0), None);
        assert_eq!(slowness_amplifier(4, 3, 1.0), Some(0));
        assert_eq!(slowness_amplifier(6, 3, 1.0), Some(2));
        assert_eq!(slowness_amplifier(5, 3, 0.5), Some(0));
        assert_eq!(slowness_amplifier(27, 1, 5.0), Some(9));
    }
}
