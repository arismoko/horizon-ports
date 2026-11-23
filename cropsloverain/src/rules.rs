//! The rules of Crops Love Rain as plain functions (`CropsLoveRain`, `CropsLoveRainConfig` and the
//! growth mixins of the original).

/// `CropsLoveRain.CropType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CropType {
    Bamboo,
    Cocoa,
    Crop,
    Sapling,
    SugarCane,
    SweetBerries,
    Melon,
    Pumpkin,
}

/// The settings of `CropsLoveRainConfig`, with the original's defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub use_rain_growth_speed: bool,
    pub rain_growth_speed: i32,
    pub use_individual_speeds: bool,
    pub bamboo: i32,
    pub cocoa: i32,
    pub crops: i32,
    pub sapling: i32,
    pub sugar_cane: i32,
    pub sweet_berry: i32,
    pub separate_stem_speed: bool,
    pub melon: i32,
    pub pumpkin: i32,
    pub debug_mode: bool,
    pub halt_regular_growth: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            use_rain_growth_speed: true,
            rain_growth_speed: 10,
            use_individual_speeds: false,
            bamboo: 10,
            cocoa: 10,
            crops: 10,
            sapling: 10,
            sugar_cane: 10,
            sweet_berry: 10,
            separate_stem_speed: false,
            melon: 10,
            pumpkin: 10,
            debug_mode: false,
            halt_regular_growth: false,
        }
    }
}

/// The file's text when it does not exist yet (NeoForge's layout of the original's spec).
pub const DEFAULT_CONFIG: &str = "[general]\n\tuse_rain_growth_speed = true\n\t#The chance describes a 1 out of x chance that a crop will grow extra each tick.\n\train_growth_speed = 10\n\n[individual]\n\t#The chance describes a 1 out of x chance that a crop will grow extra each tick.\n\t#Set to 0 to disable speed up for that crop.\n\tuse_individual_speeds = false\n\tbamboo_custom_speed = 10\n\tcocoa_custom_speed = 10\n\tcrops_custom_speed = 10\n\tsapling_custom_speed = 10\n\tsugar_cane_custom_speed = 10\n\tsweet_berry_custom_speed = 10\n\tseparate_stem_speed = false\n\tmelon_custom_speed = 10\n\tpumpkin_custom_speed = 10\n\n[debug]\n\tdebug_mode = false\n\thalt_regular_growth = false\n";

impl Config {
    /// The settings in a config file's text: `[section]` headers and `key = value` lines, the
    /// last of a key winning, anything unreadable left at its default.
    pub fn parse(text: &str) -> Self {
        let mut config = Self::default();
        let mut section = String::new();
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.trim().to_owned();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim().trim_matches('"');
            let flag = |current: bool| match value {
                "true" => true,
                "false" => false,
                _ => current,
            };
            let number = |current: i32| value.parse().unwrap_or(current);
            match (section.as_str(), key.trim()) {
                ("general", "use_rain_growth_speed") => {
                    config.use_rain_growth_speed = flag(config.use_rain_growth_speed);
                }
                ("general", "rain_growth_speed") => {
                    config.rain_growth_speed = number(config.rain_growth_speed);
                }
                ("individual", "use_individual_speeds") => {
                    config.use_individual_speeds = flag(config.use_individual_speeds);
                }
                ("individual", "bamboo_custom_speed") => config.bamboo = number(config.bamboo),
                ("individual", "cocoa_custom_speed") => config.cocoa = number(config.cocoa),
                ("individual", "crops_custom_speed") => config.crops = number(config.crops),
                ("individual", "sapling_custom_speed") => config.sapling = number(config.sapling),
                ("individual", "sugar_cane_custom_speed") => {
                    config.sugar_cane = number(config.sugar_cane);
                }
                ("individual", "sweet_berry_custom_speed") => {
                    config.sweet_berry = number(config.sweet_berry);
                }
                ("individual", "separate_stem_speed") => {
                    config.separate_stem_speed = flag(config.separate_stem_speed);
                }
                ("individual", "melon_custom_speed") => config.melon = number(config.melon),
                ("individual", "pumpkin_custom_speed") => config.pumpkin = number(config.pumpkin),
                ("debug", "debug_mode") => config.debug_mode = flag(config.debug_mode),
                ("debug", "halt_regular_growth") => {
                    config.halt_regular_growth = flag(config.halt_regular_growth);
                }
                _ => {}
            }
        }
        config
    }

    /// The debug switch that stops the game's own growth (`debugMode` and `haltRegularGrowth`).
    pub fn halts_regular_growth(&self) -> bool {
        self.debug_mode && self.halt_regular_growth
    }

    fn speed(&self, kind: CropType) -> i32 {
        match kind {
            CropType::Bamboo => self.bamboo,
            CropType::Cocoa => self.cocoa,
            CropType::Crop => self.crops,
            CropType::Sapling => self.sapling,
            CropType::SugarCane => self.sugar_cane,
            CropType::SweetBerries => self.sweet_berry,
            CropType::Melon => self.melon,
            CropType::Pumpkin => self.pumpkin,
        }
    }
}

/// `CropsLoveRain.shouldGrowExtra`. `raining_at` is `Level.isRainingAt(pos)` and `raining`
/// `Level.isRaining()`; `draw(bound)` is `RandomSource.nextInt(bound)`, asked only when the
/// rules get that far.
pub fn should_grow_extra(
    config: &Config,
    kind: CropType,
    raining_at: bool,
    raining: bool,
    mut draw: impl FnMut(i32) -> i32,
) -> bool {
    if (!raining_at && kind != CropType::Bamboo) || !config.use_rain_growth_speed {
        return false;
    }
    if kind == CropType::Bamboo && !raining {
        return false;
    }
    let speed = config.speed(kind);
    if speed == 0 {
        return false;
    }
    let bound = if config.use_individual_speeds {
        speed
    } else {
        config.rain_growth_speed
    };
    bound > 0 && draw(bound) == 0
}

/// The crop type of a stem: its own when the stems have their own speed, else a crop's.
pub fn stem_type(config: &Config, melon: bool) -> CropType {
    if config.separate_stem_speed {
        if melon {
            CropType::Melon
        } else {
            CropType::Pumpkin
        }
    } else {
        CropType::Crop
    }
}

/// A bamboo stalk's leaves (`BambooLeaves`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leaves {
    None,
    Small,
    Large,
}

impl Leaves {
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Small => "small",
            Self::Large => "large",
        }
    }

    pub fn of(name: &str) -> Self {
        match name {
            "small" => Self::Small,
            "large" => Self::Large,
            _ => Self::None,
        }
    }
}

/// `BambooStalkBlock.growBamboo`'s plan for a stalk at height `height` (blocks below it, plus
/// one): the leaves of the new stalk, whether the stalk below shrinks its leaves and the one
/// under that loses them, the age and the stage. `below` and `below2` are the leaves of the
/// stalks under the grown one (`None` when that cell is not a stalk of the same block).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BambooGrowth {
    pub leaves: Leaves,
    /// The stalk below becomes small and the one under it bare.
    pub thin_below: bool,
    pub age: u8,
    pub stage: u8,
}

pub fn grow_bamboo(
    height: i32,
    own_age: u8,
    below: Option<Leaves>,
    below2_is_stalk: bool,
    unit: f32,
) -> BambooGrowth {
    let mut leaves = Leaves::None;
    let mut thin_below = false;
    if height >= 1 {
        match below {
            Some(Leaves::Small | Leaves::Large) => {
                leaves = Leaves::Large;
                thin_below = below2_is_stalk;
            }
            _ => leaves = Leaves::Small,
        }
    }
    BambooGrowth {
        leaves,
        thin_below,
        age: u8::from(own_age == 1 || below2_is_stalk),
        stage: u8::from((height >= 11 && unit < 0.25) || height == 15),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_file_is_the_default_config() {
        assert_eq!(Config::parse(DEFAULT_CONFIG), Config::default());
    }

    #[test]
    fn the_config_reads_sections_and_keys() {
        let text = "[general]\nrain_growth_speed = 3\n[individual]\nuse_individual_speeds = true\ncocoa_custom_speed = 0 # off\nmelon_custom_speed = 4\n[debug]\ndebug_mode = true\nhalt_regular_growth = true\n";
        let config = Config::parse(text);
        assert_eq!(config.rain_growth_speed, 3);
        assert!(config.use_individual_speeds);
        assert_eq!((config.cocoa, config.melon, config.crops), (0, 4, 10));
        assert!(config.halts_regular_growth());
        assert!(!Config::parse("[debug]\nhalt_regular_growth = true").halts_regular_growth());
    }

    #[test]
    fn extra_growth_needs_rain_on_the_block() {
        let config = Config::default();
        let asked = std::cell::Cell::new(0);
        let draw = |_| {
            asked.set(asked.get() + 1);
            0
        };
        assert!(!should_grow_extra(
            &config,
            CropType::Crop,
            false,
            true,
            draw
        ));
        assert_eq!(
            asked.get(),
            0,
            "nothing is drawn when it does not rain on the block"
        );
        assert!(should_grow_extra(&config, CropType::Crop, true, true, draw));
        assert_eq!(asked.get(), 1);
    }

    #[test]
    fn bamboo_follows_the_levels_rain_not_the_blocks() {
        let config = Config::default();
        assert!(should_grow_extra(
            &config,
            CropType::Bamboo,
            false,
            true,
            |_| 0
        ));
        assert!(!should_grow_extra(
            &config,
            CropType::Bamboo,
            true,
            false,
            |_| 0
        ));
    }

    #[test]
    fn the_chance_is_one_in_the_speed() {
        let config = Config::default();
        let mut bound = 0;
        let hit = should_grow_extra(&config, CropType::Crop, true, true, |b| {
            bound = b;
            b - 1
        });
        assert!(!hit);
        assert_eq!(bound, 10);
        let individual = Config {
            use_individual_speeds: true,
            sugar_cane: 4,
            ..config
        };
        should_grow_extra(&individual, CropType::SugarCane, true, true, |b| {
            bound = b;
            1
        });
        assert_eq!(bound, 4);
    }

    #[test]
    fn a_speed_of_zero_or_the_master_switch_turns_a_crop_off() {
        let off = Config {
            use_individual_speeds: true,
            cocoa: 0,
            ..Config::default()
        };
        assert!(!should_grow_extra(&off, CropType::Cocoa, true, true, |_| 0));
        let none = Config {
            use_rain_growth_speed: false,
            ..Config::default()
        };
        assert!(!should_grow_extra(&none, CropType::Crop, true, true, |_| 0));
    }

    #[test]
    fn stems_use_the_crop_speed_unless_separate() {
        let config = Config::default();
        assert_eq!(stem_type(&config, true), CropType::Crop);
        let separate = Config {
            separate_stem_speed: true,
            ..config
        };
        assert_eq!(stem_type(&separate, true), CropType::Melon);
        assert_eq!(stem_type(&separate, false), CropType::Pumpkin);
    }

    #[test]
    fn the_first_bamboo_stalk_has_small_leaves_and_the_next_large() {
        let first = grow_bamboo(1, 0, None, false, 0.5);
        assert_eq!((first.leaves, first.thin_below), (Leaves::Small, false));
        let second = grow_bamboo(2, 0, Some(Leaves::Small), false, 0.5);
        assert_eq!(second.leaves, Leaves::Large);
        assert!(!second.thin_below);
        let third = grow_bamboo(3, 0, Some(Leaves::Large), true, 0.5);
        assert_eq!(third.leaves, Leaves::Large);
        assert!(third.thin_below);
        assert_eq!(third.age, 1);
    }

    #[test]
    fn a_tall_stalk_ends_with_a_stage_of_one() {
        assert_eq!(grow_bamboo(15, 0, Some(Leaves::Large), true, 0.9).stage, 1);
        assert_eq!(grow_bamboo(11, 0, Some(Leaves::Large), true, 0.1).stage, 1);
        assert_eq!(grow_bamboo(11, 0, Some(Leaves::Large), true, 0.5).stage, 0);
        assert_eq!(grow_bamboo(5, 0, Some(Leaves::Large), true, 0.1).stage, 0);
    }
}
