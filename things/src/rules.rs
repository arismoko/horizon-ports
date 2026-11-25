//! The rules of Things that need no game: numbers, command text and block tables, so they can be tested alone.
use std::time::{SystemTime, UNIX_EPOCH};

pub const REFRESH: u64 = 10;
pub const MOSS_REGEN: i32 = 2;
pub const MINING_MOMENTUM: i32 = 2;
pub const RIOT_STRENGTH: i32 = 1;
pub const MAGNET_COST: u32 = 50;
pub const MAGNET_CHARGE: u32 = 200;
pub const SCEPTER_USES: u32 = 64;
/// SockDataComponent: sockPerLevelSpeedAmplifier (Homestead: 0.02) per level, level + 1.
pub const SOCK_SPEED: f64 = 0.019_999_999_552_965_164;

/// A small xorshift generator seeded from the clock (Java's `Random` of the original is not observable).
#[derive(Default)]
pub struct Rng(u64);

impl Rng {
    #[cfg(test)]
    pub fn seeded(seed: u64) -> Self {
        Rng(seed | 1)
    }

    /// A float in [0, 1).
    pub fn float(&mut self) -> f64 {
        if self.0 == 0 {
            self.0 = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(1)
                | 1;
        }
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// The attribute amounts a worn set gives: socks speed, step height, reach, water efficiency.
pub fn wanted_modifiers(
    socks: Option<(i32, bool, bool)>,
    arm_extender: bool,
    wax_gland: bool,
) -> [f64; 4] {
    let mut want = [0.0f64; 4];
    if let Some((speed, jumpy, _)) = socks {
        want[0] = SOCK_SPEED * f64::from(speed + 1);
        if jumpy {
            want[1] = 0.45;
        }
    }
    if arm_extender {
        want[2] = 2.0;
    }
    if wax_gland {
        want[3] = 1.0;
    }
    want
}

/// The socks' tooltip (SocksItem.appendTooltip) as lore.
pub fn socks_lore(speed: i32, jumpy: bool, disabled: bool) -> Vec<String> {
    let mut lines = Vec::new();
    if jumpy {
        lines.push(format!(
            "↑ Jumpy{}",
            if disabled {
                " (off: Caps Lock to toggle)"
            } else {
                " (Caps Lock to toggle)"
            }
        ));
    }
    lines.push(match speed {
        0 => "☄ Average".to_owned(),
        1 => "☄ Speedy".to_owned(),
        2 => "☄ Usain Bolt".to_owned(),
        n => format!("☄ Naughty ({n})"),
    });
    lines.push("Wear it in a trinket slot (G).".to_owned());
    lines
}

pub fn effect(name: &str, effect: &str, seconds: i32, amplifier: i32) -> String {
    format!("effect give {name} {effect} {seconds} {amplifier} true")
}

/// A block the magnet's reach passes through.
pub fn passable(block: &str) -> bool {
    matches!(
        block,
        "minecraft:air"
            | "minecraft:cave_air"
            | "minecraft:void_air"
            | "minecraft:water"
            | "minecraft:lava"
            | "minecraft:bubble_column"
    )
}

/// A lockable container (LockableContainerBlockEntity's blocks).
pub fn lockable(block: &str) -> bool {
    let b = block.trim_start_matches("minecraft:");
    !b.contains("ender_chest")
        && (b.ends_with("chest")
            || b == "barrel"
            || b.ends_with("furnace")
            || b == "smoker"
            || b == "hopper"
            || b == "dispenser"
            || b == "dropper"
            || b.ends_with("shulker_box")
            || b == "brewing_stand"
            || b == "crafter")
}

/// A block id as the action bar names it: `minecraft:oak_chest` -> `Oak Chest`.
pub fn pretty(block: &str) -> String {
    block
        .rsplit(':')
        .next()
        .unwrap_or(block)
        .split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// ContainerKeyItem.maybeGetOtherChest: the other half of a double chest, from its `type` and `facing`.
pub fn other_half(kind: &str, facing: &str, p: [i32; 3]) -> Option<[i32; 3]> {
    // ChestBlock.getFacing: LEFT -> facing clockwise, RIGHT -> counterclockwise.
    let (cw, ccw) = match facing {
        "north" => ([1, 0], [-1, 0]),
        "east" => ([0, 1], [0, -1]),
        "south" => ([-1, 0], [1, 0]),
        "west" => ([0, -1], [0, 1]),
        _ => return None,
    };
    let d = match kind {
        "left" => cw,
        "right" => ccw,
        _ => return None,
    };
    Some([p[0] + d[0], p[1], p[2] + d[1]])
}

/// The Shock Absorber: falls minus 20% (at most 16), flying into a wall a quarter.
pub fn shock_damage(kind: &str, amount: f32) -> f32 {
    match kind {
        "minecraft:fall" => amount - (amount * 0.2).min(16.0).floor(),
        "minecraft:fly_into_wall" => amount / 4.0,
        _ => amount,
    }
}

/// ItemMagnetItem.inventoryTick: the damage after one tick (charge + 1 + charge / 80, up to 200).
pub fn magnet_recharged(damage: u32) -> u32 {
    let charge = MAGNET_CHARGE - damage.min(MAGNET_CHARGE);
    let next = (charge + 1 + charge / 80).min(MAGNET_CHARGE);
    MAGNET_CHARGE - next
}

/// ItemMagnetItem.use: the pull radius for a charge.
pub fn magnet_radius(charge: u32) -> f64 {
    1.25 + f64::from(charge) / 200.0 * 2.0
}

/// MossNecklaceItem.tick: block light over 7, or sky light over 7 by day.
pub fn moss_lit<T: Into<i64>>(block_light: T, sky_light: T, day_time: i64) -> bool {
    block_light.into() > 7 || (sky_light.into() > 7 && !(12500..=23500).contains(&day_time))
}

/// The Bater Wucket used as a water bucket: the command that places its water, if the click does anything.
/// `clicked` is the block clicked and `level` its `level` property; `target` is the block beside the clicked
/// face, where water goes when the clicked block is no cauldron.
pub fn wucket_plan(
    clicked: &str,
    level: Option<&str>,
    pos: [i32; 3],
    face: [i32; 3],
    target: &str,
    dim: &str,
) -> Option<String> {
    let set = |p: [i32; 3], state: &str| {
        format!(
            "execute in {dim} run setblock {} {} {} {state}",
            p[0], p[1], p[2]
        )
    };
    match clicked {
        "minecraft:cauldron" => Some(set(pos, "minecraft:water_cauldron[level=3]")),
        "minecraft:water_cauldron" if level != Some("3") => {
            Some(set(pos, "minecraft:water_cauldron[level=3]"))
        }
        "minecraft:water_cauldron" => None,
        _ if replaceable(target) => Some(set(
            [pos[0] + face[0], pos[1] + face[1], pos[2] + face[2]],
            "minecraft:water",
        )),
        _ => None,
    }
}

fn replaceable(block: &str) -> bool {
    matches!(
        block,
        "minecraft:air"
            | "minecraft:cave_air"
            | "minecraft:void_air"
            | "minecraft:short_grass"
            | "minecraft:tall_grass"
            | "minecraft:fern"
            | "minecraft:large_fern"
            | "minecraft:dead_bush"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socks_speed_grows_per_level() {
        let w = wanted_modifiers(Some((2, false, false)), false, false);
        assert!((w[0] - SOCK_SPEED * 3.0).abs() < 1e-12);
        assert_eq!(w[1], 0.0);
    }

    #[test]
    fn jumpy_socks_raise_step_height_and_nothing_else_does() {
        assert_eq!(
            wanted_modifiers(Some((0, true, true)), false, false)[1],
            0.45
        );
        assert_eq!(wanted_modifiers(None, true, true), [0.0, 0.0, 2.0, 1.0]);
        assert_eq!(wanted_modifiers(None, false, false), [0.0; 4]);
    }

    #[test]
    fn socks_lore_names_the_level_and_the_toggle() {
        assert_eq!(socks_lore(0, false, false)[0], "☄ Average");
        assert_eq!(socks_lore(2, false, false)[0], "☄ Usain Bolt");
        assert_eq!(socks_lore(5, false, false)[0], "☄ Naughty (5)");
        assert!(socks_lore(1, true, false)[0].ends_with("(Caps Lock to toggle)"));
        assert!(socks_lore(1, true, true)[0].contains("off"));
        assert_eq!(socks_lore(1, true, true).len(), 3);
    }

    #[test]
    fn lockable_blocks_are_the_lockable_containers() {
        for b in [
            "minecraft:chest",
            "minecraft:trapped_chest",
            "minecraft:barrel",
            "minecraft:blast_furnace",
            "minecraft:smoker",
            "minecraft:red_shulker_box",
            "minecraft:crafter",
        ] {
            assert!(lockable(b), "{b}");
        }
        for b in [
            "minecraft:ender_chest",
            "minecraft:furnace_x",
            "minecraft:stone",
            "minecraft:crafting_table",
        ] {
            assert!(!lockable(b), "{b}");
        }
    }

    #[test]
    fn pretty_names_blocks() {
        assert_eq!(pretty("minecraft:trapped_chest"), "Trapped Chest");
        assert_eq!(pretty("barrel"), "Barrel");
    }

    #[test]
    fn double_chest_halves_pair_up() {
        assert_eq!(other_half("left", "north", [0, 64, 0]), Some([1, 64, 0]));
        assert_eq!(other_half("right", "north", [0, 64, 0]), Some([-1, 64, 0]));
        assert_eq!(other_half("left", "east", [0, 64, 0]), Some([0, 64, 1]));
        assert_eq!(other_half("right", "south", [5, 1, 5]), Some([6, 1, 5]));
        assert_eq!(other_half("left", "west", [5, 1, 5]), Some([5, 1, 4]));
        assert_eq!(other_half("single", "north", [0, 0, 0]), None);
        assert_eq!(other_half("left", "up", [0, 0, 0]), None);
    }

    #[test]
    fn shock_absorber_trims_falls_and_wall_hits() {
        assert_eq!(shock_damage("minecraft:fall", 10.0), 8.0);
        assert_eq!(shock_damage("minecraft:fall", 100.0), 84.0);
        assert_eq!(shock_damage("minecraft:fly_into_wall", 8.0), 2.0);
        assert_eq!(shock_damage("minecraft:cactus", 3.0), 3.0);
    }

    #[test]
    fn magnet_recharges_up_to_full() {
        assert_eq!(magnet_recharged(0), 0);
        assert_eq!(magnet_recharged(200), 199);
        assert_eq!(magnet_recharged(150), 149); // charge 50 -> 51
        let mut d = 200;
        for _ in 0..400 {
            d = magnet_recharged(d);
        }
        assert_eq!(d, 0);
        assert_eq!(magnet_radius(200), 3.25);
        assert_eq!(magnet_radius(0), 1.25);
    }

    #[test]
    fn moss_necklace_wants_light() {
        assert!(moss_lit(8, 0, 18000));
        assert!(!moss_lit(7, 15, 18000));
        assert!(moss_lit(0, 8, 6000));
        assert!(moss_lit(0, 8, 24000));
        assert!(!moss_lit(0, 8, 13000));
    }

    #[test]
    fn wucket_fills_cauldrons_and_places_water_beside_the_face() {
        let at = [1, 2, 3];
        assert_eq!(
            wucket_plan(
                "minecraft:cauldron",
                None,
                at,
                [0, 1, 0],
                "minecraft:air",
                "minecraft:overworld"
            )
            .as_deref(),
            Some(
                "execute in minecraft:overworld run setblock 1 2 3 minecraft:water_cauldron[level=3]"
            )
        );
        assert_eq!(
            wucket_plan(
                "minecraft:water_cauldron",
                Some("3"),
                at,
                [0, 1, 0],
                "minecraft:air",
                "d"
            ),
            None
        );
        assert!(
            wucket_plan(
                "minecraft:water_cauldron",
                Some("1"),
                at,
                [0, 1, 0],
                "minecraft:air",
                "d"
            )
            .is_some()
        );
        assert_eq!(
            wucket_plan("minecraft:stone", None, at, [0, 1, 0], "minecraft:air", "d").as_deref(),
            Some("execute in d run setblock 1 3 3 minecraft:water")
        );
        assert_eq!(
            wucket_plan(
                "minecraft:stone",
                None,
                at,
                [0, 1, 0],
                "minecraft:stone",
                "d"
            ),
            None
        );
    }

    #[test]
    fn rng_floats_are_in_range_and_vary() {
        let mut r = Rng::seeded(42);
        let a = r.float();
        let b = r.float();
        assert!((0.0..1.0).contains(&a) && (0.0..1.0).contains(&b));
        assert_ne!(a, b);
        assert!((0.0..1.0).contains(&Rng::default().float()));
    }

    #[test]
    fn effect_command_is_hidden_particles() {
        assert_eq!(
            effect("Steve", "minecraft:haste", 2, 1),
            "effect give Steve minecraft:haste 2 1 true"
        );
        assert!(passable("minecraft:water") && !passable("minecraft:stone"));
    }
}
