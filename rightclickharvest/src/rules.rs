//! The rules of Right Click Harvest as plain functions over small values (`RightClickHarvest`,
//! `Config` in the original).

/// `Config.HungerLevel`: how much food a harvest costs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hunger {
    None,
    Low,
    Normal,
    High,
}

impl Hunger {
    /// `HungerLevel.modifier`.
    pub fn modifier(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Low => 0.5,
            Self::Normal => 1.0,
            Self::High => 2.0,
        }
    }
}

/// `Config.ExperienceType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Experience {
    None,
    Cost,
    Reward,
}

/// The mod's settings (`Config`), with the original's defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub require_hoe: bool,
    pub harvest_in_radius: bool,
    pub hunger: Hunger,
    pub experience: Experience,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            require_hoe: false,
            harvest_in_radius: true,
            hunger: Hunger::None,
            experience: Experience::None,
        }
    }
}

/// The file's text when it does not exist yet.
pub const DEFAULT_CONFIG: &str = "# Only allow harvesting certain crops (e.g. wheat, carrots) if the player is holding a hoe.\nrequireHoe = false\n# Make hoes of different tiers harvest multiple blocks in a radius.\nharvestInRadius = true\n# The food usage when harvesting crops: none, low, normal or high.\nhungerLevel = \"none\"\n# Whether XP should be taken or given for harvesting: none, cost or reward.\nexperienceType = \"none\"\n";

impl Config {
    /// The settings in a config file's text: `key = value` lines, the last of a key winning,
    /// anything unreadable left at its default.
    pub fn parse(text: &str) -> Self {
        let mut config = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value
                .split('#')
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches('"');
            let value = value.to_ascii_lowercase();
            match key.trim() {
                "requireHoe" => config.require_hoe = value == "true",
                "harvestInRadius" => config.harvest_in_radius = value != "false",
                "hungerLevel" => {
                    config.hunger = match value.as_str() {
                        "low" => Hunger::Low,
                        "normal" => Hunger::Normal,
                        "high" => Hunger::High,
                        _ => Hunger::None,
                    }
                }
                "experienceType" => {
                    config.experience = match value.as_str() {
                        "cost" => Experience::Cost,
                        "reward" => Experience::Reward,
                        _ => Experience::None,
                    }
                }
                _ => {}
            }
        }
        config
    }
}

/// What a block is to the harvester.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A `CropBlock`: grown at its largest age, replanted at 0.
    Crop,
    /// `CocoaBlock`.
    Cocoa,
    /// `NetherWartBlock`.
    NetherWart,
    /// `SugarCaneBlock`.
    SugarCane,
    /// `CactusBlock`.
    Cactus,
}

impl Kind {
    /// The kind of a block. `crop_tagged` and `has_age` say it is in `#minecraft:crops` and has an
    /// `age` property, which is how a modded `CropBlock` shows; stems and the pitcher crop are
    /// not `CropBlock`s.
    pub fn of(id: &str, crop_tagged: bool, has_age: bool) -> Option<Self> {
        match id {
            "minecraft:cocoa" => Some(Self::Cocoa),
            "minecraft:nether_wart" => Some(Self::NetherWart),
            "minecraft:sugar_cane" => Some(Self::SugarCane),
            "minecraft:cactus" => Some(Self::Cactus),
            "minecraft:melon_stem" | "minecraft:pumpkin_stem" | "minecraft:pitcher_crop" => None,
            _ if crop_tagged && has_age => Some(Self::Crop),
            _ => None,
        }
    }

    /// A block that grows in a column: a harvest breaks its second block.
    pub fn is_column(self) -> bool {
        matches!(self, Self::SugarCane | Self::Cactus)
    }
}

/// A hoe's tier tag (`rightclickharvest:low_tier_hoes` and the others).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Tiers {
    pub low: bool,
    pub mid: bool,
    pub high: bool,
}

impl Tiers {
    /// The radius of a harvest and whether it is a diamond (Manhattan) rather than a square: the
    /// highest tier the item has.
    pub fn radius(self) -> Option<(i32, bool)> {
        if self.high {
            Some((2, true))
        } else if self.mid {
            Some((1, false))
        } else if self.low {
            Some((1, true))
        } else {
            None
        }
    }
}

/// The cells around `center` a hoe of this radius harvests, in the original's order.
pub fn radius_positions(center: [i32; 3], radius: i32, circle: bool) -> Vec<[i32; 3]> {
    let [x, y, z] = center;
    if radius == 1 && circle {
        return vec![[x, y, z - 1], [x + 1, y, z], [x, y, z + 1], [x - 1, y, z]];
    }
    let mut cells = Vec::new();
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            if (dx == 0 && dz == 0) || (circle && dx.abs() + dz.abs() > radius) {
                continue;
            }
            cells.push([x + dx, y, z + dz]);
        }
    }
    cells
}

/// The player, as far as the rules ask.
#[derive(Clone, Copy, Debug)]
pub struct Player {
    pub spectator: bool,
    pub crouching: bool,
    pub main_hand: bool,
    pub creative: bool,
    pub food_level: i32,
    pub xp_level: i32,
}

/// What a click comes to before the block is looked at in detail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
    /// Leave the click to the game.
    Pass,
    /// Leave it, and tell a player with an empty hand to hold a hoe (the once-only warning).
    NeedHoe,
    /// Go on; `hoe_in_use` is whether the held item is a hoe.
    Go { hoe_in_use: bool },
}

/// The highest level `UniformInt.of(0, 2)` gives, which a harvest costs.
pub const XP_MAX: i32 = 2;

/// `RightClickHarvest.onBlockUse`'s checks before the block's kind: who may click, the
/// block list, hunger, XP and the hoe requirement.
pub fn gate(
    player: Player,
    config: Config,
    blacklisted: bool,
    hoe_never_required: bool,
    holding_hoe: bool,
) -> Gate {
    if player.spectator || player.crouching || !player.main_hand || blacklisted {
        return Gate::Pass;
    }
    if config.hunger != Hunger::None && !player.creative && player.food_level <= 0 {
        return Gate::Pass;
    }
    if config.experience == Experience::Cost && !player.creative && player.xp_level < XP_MAX {
        return Gate::Pass;
    }
    if !hoe_never_required && config.require_hoe && !holding_hoe {
        return Gate::NeedHoe;
    }
    Gate::Go {
        hoe_in_use: holding_hoe,
    }
}

/// The experience a harvest moves, given its roll of 0 to 2: negative when it is a cost.
pub fn experience_change(mode: Experience, roll: i32) -> i32 {
    match mode {
        Experience::Cost => -roll,
        Experience::Reward => roll,
        Experience::None => 0,
    }
}

/// The exhaustion of a harvest: 0.008 for each step of the hunger setting.
pub fn exhaustion(hunger: Hunger) -> f32 {
    0.008 * hunger.modifier()
}

/// The item a crop's harvest keeps back to replant (`getCloneItemStack`).
pub fn replant_item(block: &str) -> Option<&'static str> {
    match block {
        "minecraft:wheat" => Some("minecraft:wheat_seeds"),
        "minecraft:carrots" => Some("minecraft:carrot"),
        "minecraft:potatoes" => Some("minecraft:potato"),
        "minecraft:beetroots" => Some("minecraft:beetroot_seeds"),
        "minecraft:torchflower_crop" => Some("minecraft:torchflower_seeds"),
        "minecraft:nether_wart" => Some("minecraft:nether_wart"),
        "minecraft:cocoa" => Some("minecraft:cocoa_beans"),
        _ => None,
    }
}

/// Take one of `item` out of the first stack of it in `drops` (`dropStacks`'s replant).
pub fn take_replant(drops: &mut [(String, u32)], item: &str) {
    if let Some((_, count)) = drops.iter_mut().find(|(drop, _)| drop == item) {
        *count = count.saturating_sub(1);
    }
}

/// The cell a harvest of a sugar cane or cactus column breaks: the second block from the bottom,
/// and only where the column is at least two tall.
pub fn column_break(pos: [i32; 3], mut same: impl FnMut([i32; 3]) -> bool) -> Option<[i32; 3]> {
    let [x, mut y, z] = pos;
    while same([x, y - 1, z]) {
        y -= 1;
    }
    same([x, y + 1, z]).then_some([x, y + 1, z])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player() -> Player {
        Player {
            spectator: false,
            crouching: false,
            main_hand: true,
            creative: false,
            food_level: 20,
            xp_level: 0,
        }
    }

    #[test]
    fn the_config_reads_its_keys_and_defaults_the_rest() {
        assert_eq!(Config::parse(DEFAULT_CONFIG), Config::default());
        let text = "requireHoe = true\nharvestInRadius = false # off\nhungerLevel = \"High\"\nexperienceType = \"reward\"\n";
        assert_eq!(
            Config::parse(text),
            Config {
                require_hoe: true,
                harvest_in_radius: false,
                hunger: Hunger::High,
                experience: Experience::Reward,
            }
        );
        assert_eq!(Config::parse("hungerLevel = \"bogus\""), Config::default());
    }

    #[test]
    fn blocks_have_the_kind_their_class_gives() {
        assert_eq!(Kind::of("minecraft:wheat", true, true), Some(Kind::Crop));
        assert_eq!(
            Kind::of("farmersdelight:tomatoes", true, true),
            Some(Kind::Crop)
        );
        assert_eq!(Kind::of("minecraft:melon_stem", true, true), None);
        assert_eq!(Kind::of("minecraft:pitcher_crop", true, true), None);
        assert_eq!(Kind::of("minecraft:fern", false, false), None);
        assert_eq!(Kind::of("minecraft:cocoa", false, true), Some(Kind::Cocoa));
        assert_eq!(
            Kind::of("minecraft:nether_wart", false, true),
            Some(Kind::NetherWart)
        );
        assert!(
            Kind::of("minecraft:cactus", false, false)
                .unwrap()
                .is_column()
        );
        assert!(!Kind::Crop.is_column());
    }

    #[test]
    fn the_highest_tier_sets_the_radius() {
        assert_eq!(Tiers::default().radius(), None);
        assert_eq!(
            Tiers {
                low: true,
                ..Tiers::default()
            }
            .radius(),
            Some((1, true))
        );
        assert_eq!(
            Tiers {
                low: true,
                mid: true,
                high: false
            }
            .radius(),
            Some((1, false))
        );
        assert_eq!(
            Tiers {
                low: true,
                mid: true,
                high: true
            }
            .radius(),
            Some((2, true))
        );
    }

    #[test]
    fn a_low_tier_hoe_takes_the_four_neighbours_in_order() {
        assert_eq!(
            radius_positions([0, 5, 0], 1, true),
            vec![[0, 5, -1], [1, 5, 0], [0, 5, 1], [-1, 5, 0]]
        );
    }

    #[test]
    fn a_mid_tier_hoe_takes_the_square_and_a_high_tier_one_the_diamond() {
        assert_eq!(radius_positions([0, 0, 0], 1, false).len(), 8);
        let diamond = radius_positions([0, 0, 0], 2, true);
        assert_eq!(diamond.len(), 12);
        assert!(diamond.iter().all(|[x, _, z]| x.abs() + z.abs() <= 2));
        assert!(!diamond.contains(&[0, 0, 0]));
        assert!(!diamond.contains(&[2, 0, 2]));
    }

    #[test]
    fn the_click_is_left_to_the_game_unless_it_may_harvest() {
        let config = Config::default();
        for p in [
            Player {
                spectator: true,
                ..player()
            },
            Player {
                crouching: true,
                ..player()
            },
            Player {
                main_hand: false,
                ..player()
            },
        ] {
            assert_eq!(gate(p, config, false, false, false), Gate::Pass);
        }
        assert_eq!(gate(player(), config, true, false, false), Gate::Pass);
        assert_eq!(
            gate(player(), config, false, false, true),
            Gate::Go { hoe_in_use: true }
        );
        assert_eq!(
            gate(player(), config, false, false, false),
            Gate::Go { hoe_in_use: false }
        );
    }

    #[test]
    fn hunger_and_xp_settings_stop_a_player_who_cannot_pay() {
        let hungry = Config {
            hunger: Hunger::Low,
            ..Config::default()
        };
        let starving = Player {
            food_level: 0,
            ..player()
        };
        assert_eq!(gate(starving, hungry, false, false, false), Gate::Pass);
        assert_eq!(
            gate(
                Player {
                    creative: true,
                    ..starving
                },
                hungry,
                false,
                false,
                false
            ),
            Gate::Go { hoe_in_use: false }
        );
        assert_eq!(
            gate(starving, Config::default(), false, false, false),
            Gate::Go { hoe_in_use: false }
        );
        let costly = Config {
            experience: Experience::Cost,
            ..Config::default()
        };
        assert_eq!(
            gate(
                Player {
                    xp_level: 1,
                    ..player()
                },
                costly,
                false,
                false,
                false
            ),
            Gate::Pass
        );
        assert_eq!(
            gate(
                Player {
                    xp_level: 2,
                    ..player()
                },
                costly,
                false,
                false,
                false
            ),
            Gate::Go { hoe_in_use: false }
        );
    }

    #[test]
    fn requiring_a_hoe_holds_back_a_bare_hand_except_on_exempt_blocks() {
        let config = Config {
            require_hoe: true,
            ..Config::default()
        };
        assert_eq!(gate(player(), config, false, false, false), Gate::NeedHoe);
        assert_eq!(
            gate(player(), config, false, true, false),
            Gate::Go { hoe_in_use: false }
        );
        assert_eq!(
            gate(player(), config, false, false, true),
            Gate::Go { hoe_in_use: true }
        );
    }

    #[test]
    fn experience_is_taken_or_given() {
        assert_eq!(experience_change(Experience::Cost, 2), -2);
        assert_eq!(experience_change(Experience::Reward, 1), 1);
        assert_eq!(experience_change(Experience::None, 2), 0);
    }

    #[test]
    fn exhaustion_scales_with_the_hunger_setting() {
        assert_eq!(exhaustion(Hunger::None), 0.0);
        assert_eq!(exhaustion(Hunger::Normal), 0.008);
        assert_eq!(exhaustion(Hunger::High), 0.016);
    }

    #[test]
    fn a_harvest_keeps_one_seed_back() {
        let mut drops = vec![
            ("minecraft:wheat".to_owned(), 1),
            ("minecraft:wheat_seeds".to_owned(), 3),
        ];
        take_replant(&mut drops, replant_item("minecraft:wheat").unwrap());
        assert_eq!(drops[1].1, 2);
        assert_eq!(drops[0].1, 1);
        let mut one = vec![("minecraft:carrot".to_owned(), 1)];
        take_replant(&mut one, "minecraft:carrot");
        assert_eq!(one[0].1, 0);
        let mut none = vec![("minecraft:wheat".to_owned(), 1)];
        take_replant(&mut none, "minecraft:wheat_seeds");
        assert_eq!(none[0].1, 1);
    }

    #[test]
    fn a_column_breaks_its_second_block_and_only_when_two_tall() {
        let column =
            |top: i32| move |[x, y, z]: [i32; 3]| x == 0 && z == 0 && (60..=top).contains(&y);
        assert_eq!(column_break([0, 62, 0], column(63)), Some([0, 61, 0]));
        assert_eq!(column_break([0, 60, 0], column(62)), Some([0, 61, 0]));
        assert_eq!(column_break([0, 60, 0], column(60)), None);
    }
}
