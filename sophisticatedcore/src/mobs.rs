//! Monsters that spawn wearing a backpack and may drop it (`EntityBackpackAdditionHandler`).
//!
//! A new monster of a listed kind, by a small chance, wears a backpack in its chest slot, more armour and sometimes
//! more health the better the backpack. The mob is tagged with the tier and its difficulty; when a player kills it, the
//! backpack drops with loot rolled into it from the kind's loot table. The dice are drawn from the spawn's own
//! inputs, since the spawn hook must be a pure function of them.

use mod_api::{
    AttributeModifier, AttributeOperation, Host, Identifier, ItemDrop, ItemStack, ServerHost,
};

use crate::contents::{self, Contents, Nbt};
use crate::options::Lists;
use crate::storage_items::{self, Family};
use crate::upgrades;

const MAX_DIFFICULTY: i32 = 3;
const MAX_LOCAL_DIFFICULTY: f32 = 6.75;
pub const TIER_TAG: &str = "sophisticatedbackpacks:backpack_tier_";
pub const DIFFICULTY_TAG: &str = "sophisticatedbackpacks:backpack_difficulty_";
/// Set at spawn on a bearer that is owed its potion effects (`buffWithPotionEffects`); the number is its difficulty.
pub const EFFECTS_TAG: &str = "sophisticatedbackpacks:backpack_effects_";
/// Set once the effects were given, so a bearer entering a level again isn't buffed twice.
pub const EFFECTS_DONE_TAG: &str = "sophisticatedbackpacks:backpack_effects_given";
const HEALTH_BONUS: &str = "sophisticatedbackpacks:backpack_bearer_health_bonus";

/// The armour tiers, rarest first: weight and item suffix.
const ARMOUR: [(u32, &str); 5] = [
    (1, "netherite"),
    (3, "diamond"),
    (9, "golden"),
    (27, "iron"),
    (81, "leather"),
];

/// A backpack tier the spawn may give: its weight, minimum difficulty, and the armour ranges (helmet, leggings, boots)
/// as indexes into [`ARMOUR`].
struct Addition {
    weight: u32,
    min_difficulty: i32,
    tier: usize,
    armour: [(usize, usize); 3],
}

const ADDITIONS: [Addition; 6] = [
    Addition {
        weight: 1,
        min_difficulty: 4,
        tier: 5,
        armour: [(0, 1), (0, 1), (0, 1)],
    },
    Addition {
        weight: 5,
        min_difficulty: 3,
        tier: 4,
        armour: [(0, 2), (0, 2), (0, 2)],
    },
    Addition {
        weight: 25,
        min_difficulty: 2,
        tier: 3,
        armour: [(1, 3), (1, 3), (1, 3)],
    },
    Addition {
        weight: 125,
        min_difficulty: 1,
        tier: 2,
        armour: [(2, 4), (2, 4), (2, 4)],
    },
    Addition {
        weight: 250,
        min_difficulty: 1,
        tier: 1,
        armour: [(2, 4), (3, 5), (3, 5)],
    },
    Addition {
        weight: 625,
        min_difficulty: 0,
        tier: 0,
        armour: [(3, 5), (3, 5), (3, 5)],
    },
];

/// `APPLICABLE_EFFECTS`: the effects a bearer may get, with the highest amplifier each allows. Resistance and
/// regeneration are one entry that picks between them.
const EFFECTS: [(&[&str], i32); 6] = [
    (&["minecraft:resistance", "minecraft:regeneration"], 1),
    (&["minecraft:fire_resistance"], i32::MAX),
    (&["minecraft:absorption"], i32::MAX),
    (&["minecraft:health_boost"], i32::MAX),
    (&["minecraft:speed"], i32::MAX),
    (&["minecraft:strength"], i32::MAX),
];

/// How long a bearer's effects last (`30 * 60 * 20` ticks).
pub const EFFECT_TICKS: i32 = 36_000;

/// `applyPotions`: `difficulty + 2` of the effects (each at most once), each with an amplifier between the tier's
/// minimum difficulty and a roll up to the difficulty, capped by what the effect allows. (effect id, amplifier)
pub fn effects(entity: u32, pos: [f64; 3], difficulty: i32, min_difficulty: i32) -> Vec<(&'static str, i32)> {
    let mut dice = Dice::seeded(&[u64::from(entity), pos[0].to_bits(), pos[2].to_bits()], "effects");
    let mut left: Vec<usize> = (0..EFFECTS.len()).collect();
    let mut out = Vec::new();
    for _ in 0..(difficulty + 2).max(0) {
        if left.is_empty() {
            break;
        }
        let (names, max) = EFFECTS[left.remove(dice.below(left.len() as u64) as usize)];
        let amplifier = min_difficulty.max(dice.below(difficulty.max(0) as u64 + 1) as i32).min(max);
        out.push((names[dice.below(names.len() as u64) as usize], amplifier));
    }
    out
}

/// How much a tier raises the drop chance (`dropChanceMultiplier`), leather to netherite.
const DROP_MULTIPLIER: [f64; 6] = [1.0, 1.25, 1.5, 3.0, 4.5, 6.0];

/// The entities that may wear a backpack and the loot table their backpack is filled from (the jar's default).
pub const DEFAULT_LOOT: &str = "minecraft:creeper|minecraft:chests/desert_pyramid
minecraft:drowned|minecraft:chests/shipwreck_treasure
minecraft:enderman|minecraft:chests/end_city_treasure
minecraft:evoker|minecraft:chests/woodland_mansion
minecraft:husk|minecraft:chests/desert_pyramid
minecraft:piglin|minecraft:chests/bastion_bridge
minecraft:piglin_brute|minecraft:chests/bastion_treasure
minecraft:pillager|minecraft:chests/pillager_outpost
minecraft:skeleton|minecraft:chests/simple_dungeon
minecraft:stray|minecraft:chests/igloo_chest
minecraft:vex|minecraft:chests/woodland_mansion
minecraft:vindicator|minecraft:chests/woodland_mansion
minecraft:witch|minecraft:chests/buried_treasure
minecraft:wither_skeleton|minecraft:chests/nether_bridge
minecraft:zombie|minecraft:chests/simple_dungeon
minecraft:zombie_villager|minecraft:chests/village/village_armorer
minecraft:zombified_piglin|minecraft:chests/bastion_other
";

/// The loot table for a kind that may wear a backpack, from the list file.
pub fn loot_table(host: &dyn Host, kind: &str) -> Option<String> {
    Lists::get(host).entity_loot.iter().find_map(|line| {
        let (entity, table) = line.split_once('|')?;
        (entity.trim() == kind).then(|| table.trim().to_owned())
    })
}

/// A small deterministic stream (splitmix64) seeded from the spawn's inputs.
struct Dice(u64);

impl Dice {
    fn seeded(parts: &[u64], text: &str) -> Self {
        let mut h = 0xCBF2_9CE4_8422_2325u64;
        for b in text.bytes().map(u64::from).chain(parts.iter().copied()) {
            h = (h ^ b).wrapping_mul(0x0000_0100_0000_01B3);
        }
        Self(h)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// 0 up to but not including `n`.
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// `RandHelper.getRandomWeightedElement`.
    fn weighted(&mut self, weights: &[u32]) -> usize {
        let total: u64 = weights.iter().map(|&w| u64::from(w)).sum();
        let mut roll = self.below(total);
        for (i, &w) in weights.iter().enumerate() {
            if roll < u64::from(w) {
                return i;
            }
            roll -= u64::from(w);
        }
        weights.len() - 1
    }
}

/// What a spawn gets: the backpack tier, the difficulty rolled, and the armour ids by slot.
pub struct Wearer {
    pub tier: usize,
    pub difficulty: i32,
    pub min_difficulty: i32,
    pub armour: [Option<String>; 3],
}

/// Rolls whether a new monster wears a backpack, and what comes with it (`addBackpack`). `local` is the spawn's
/// effective difficulty and `chance` the setting.
pub fn roll(
    kind: &str,
    pos: [f64; 3],
    dimension: &str,
    local: f32,
    chance: f64,
    armour: bool,
) -> Option<Wearer> {
    if chance <= 0.0 {
        return None;
    }
    let mut dice = Dice::seeded(
        &[pos[0].to_bits(), pos[1].to_bits(), pos[2].to_bits()],
        &format!("{dimension}{kind}"),
    );
    if dice.below((1.0 / chance) as u64) != 0 {
        return None;
    }
    // The pool narrows as local difficulty rises: all tiers, the middle four, the lower three.
    let index = ((3.0 / MAX_LOCAL_DIFFICULTY * local - 0.1).floor() as i32).clamp(0, 2);
    let pool: &[Addition] = match index {
        0 => &ADDITIONS,
        1 => &ADDITIONS[1..5],
        _ => &ADDITIONS[2..5],
    };
    let pick = &pool[dice.weighted(&pool.iter().map(|a| a.weight).collect::<Vec<_>>())];
    let difficulty = pick
        .min_difficulty
        .max(dice.below(MAX_DIFFICULTY as u64 + 1) as i32);
    let mut worn = [None, None, None];
    if armour {
        for (i, piece) in ["helmet", "leggings", "boots"].iter().enumerate() {
            let (lo, hi) = pick.armour[i];
            let weights: Vec<u32> = ARMOUR[lo..hi].iter().map(|a| a.0).collect();
            let tier = ARMOUR[lo + dice.weighted(&weights)].1;
            worn[i] = Some(format!("minecraft:{tier}_{piece}"));
        }
    }
    Some(Wearer {
        tier: pick.tier,
        difficulty,
        min_difficulty: pick.min_difficulty,
        armour: worn,
    })
}

/// The health added: the base times the minimum difficulty (`raiseHealth`), nothing under a tenth.
pub fn health_bonus(base: f64, min_difficulty: i32) -> Option<f64> {
    let add = base * f64::from(min_difficulty);
    (add > 0.1).then_some(add)
}

/// The modifier that raises a bearer's health.
pub fn health_modifier(amount: f64) -> Option<AttributeModifier> {
    Some(AttributeModifier::new(
        Identifier::parse(HEALTH_BONUS).ok()?,
        amount,
        AttributeOperation::AddValue,
    ))
}

/// Whether a kill drops the backpack (`shouldDropBackpack`): the drop chance plus the looting bonus, times the tier's
/// multiplier, against a roll.
pub fn drops(tier: usize, looting: i32, base: f64, per_level: f64, roll: f64) -> bool {
    roll < (base + f64::from(looting) * per_level) * DROP_MULTIPLIER[tier]
}

/// The looting level on `stack` (`minecraft:enchantments`).
pub fn looting_level(stack: &ItemStack) -> i32 {
    let Some(comps) = stack.components() else {
        return 0;
    };
    match comps.as_nbt().at("minecraft:enchantments") {
        Some(mod_api::Tag::Compound(c)) => {
            let levels = match c.at("levels") {
                Some(mod_api::Tag::Compound(l)) => l,
                _ => c,
            };
            contents::get_int(levels, "minecraft:looting").unwrap_or(0)
        }
        _ => 0,
    }
}

/// The armour the mob should get, as stacks.
pub fn armour_stack(host: &dyn Host, id: &str) -> Option<ItemStack> {
    let key = host.item_key(&Identifier::parse(id).ok()?)?;
    ItemStack::new(key, 1)
}

/// A fresh backpack of `tier`.
pub fn backpack(host: &dyn Host, tier: usize) -> Option<ItemStack> {
    storage_items::BACKPACKS
        .get(tier)
        .and_then(|k| storage::new_stack(host, k.id))
}

use crate::storage;

/// Tagged on a bearer that spawned with a jukebox upgrade.
pub const JUKEBOX_TAG: &str = "sophisticatedbackpacks:jukebox";

/// `addJukeboxUpgradeAndRandomDisc`: one spawn in four (`play_jukebox`) puts a jukebox upgrade in the backpack's
/// first upgrade slot, advanced one time in four, with a random disc not on the block list: one in the basic
/// upgrade, `1 + rnd(slots / 3)` in the advanced. True when it did.
pub fn add_jukebox(host: &dyn Host, pack: &mut ItemStack, entity_key: &[u64], blocked: &[String]) -> bool {
    let mut dice = Dice::seeded(entity_key, "jukebox");
    if dice.below(4) != 0 {
        return false;
    }
    let Some(kind) = storage_items::by_item(&contents::item_id(host, pack)) else { return false };
    if kind.upgrade_count() == 0 {
        return false;
    }
    let advanced = dice.unit() < 0.25;
    let id = if advanced { "sophisticatedbackpacks:advanced_jukebox_upgrade" } else { "sophisticatedbackpacks:jukebox_upgrade" };
    let Some(mut up) = storage::new_stack(host, id) else { return false };
    let Some(info) = upgrades::of(host, &up) else { return false };
    let discs = crate::jukebox::mob_discs(blocked);
    if discs.is_empty() {
        return false;
    }
    let slots = upgrades::disc_slots(&info);
    let count = if advanced { dice.below((slots / 3).max(1) as u64) as usize + 1 } else { 1 };
    let mut deck = crate::jukebox::Deck::read(host, &up, slots);
    for slot in deck.discs.iter_mut().take(count) {
        *slot = storage::new_stack(host, &discs[dice.below(discs.len() as u64) as usize]);
    }
    deck.write(host, &mut up);
    let mut c = Contents::read(host, pack, kind);
    c.upgrades[0] = Some(up);
    c.write(host, pack, kind);
    true
}

/// The tier and difficulty a mob was tagged with at spawn.
pub fn tags_of(host: &dyn Host, entity: mod_api::EntityRef) -> Option<(usize, i32)> {
    let tags = entity.tags(host)?;
    let tier = (0..6).find(|t| tags.contains(&format!("{TIER_TAG}{t}")))?;
    let difficulty = (0..=MAX_DIFFICULTY)
        .find(|d| tags.contains(&format!("{DIFFICULTY_TAG}{d}")))
        .unwrap_or(0);
    Some((tier, difficulty))
}

/// The difficulty a bearer was tagged to receive potion effects at, if it still owes them.
pub fn effects_owed(host: &dyn Host, entity: mod_api::EntityRef) -> Option<(i32, i32)> {
    let tags = entity.tags(host)?;
    if tags.contains(EFFECTS_DONE_TAG) {
        return None;
    }
    let difficulty = (0..=MAX_DIFFICULTY).find(|d| tags.contains(&format!("{EFFECTS_TAG}{d}")))?;
    let tier = (0..6).find(|t| tags.contains(&format!("{TIER_TAG}{t}")))?;
    Some((difficulty, ADDITIONS.iter().find(|a| a.tier == tier)?.min_difficulty))
}

/// A drop of a backpack of `tier`, with loot of the mob's kind rolled in: one roll per difficulty level (nothing at
/// difficulty 0, as `setLoot` skips it).
pub fn dropped_backpack(
    host: &mut dyn ServerHost,
    entity: mod_api::EntityRef,
    kind: &str,
    tier: usize,
    difficulty: i32,
    pos: [f64; 3],
) -> Option<ItemDrop> {
    let item = *storage_items::BACKPACKS.get(tier)?;
    debug_assert_eq!(item.family, Family::Backpack);
    let mut stack = backpack(&*host, tier)?;
    let mut c = Contents::read(&*host, &stack, &item);
    if difficulty > 0
        && let Some(table) = loot_table(&*host, kind)
        && let Some(level) = entity.level(&*host)
    {
        let mult = upgrades::multiplier(&*host, &item, &c.upgrades);
        for _ in 0..difficulty {
            let Ok(Some(rolled)) = level.roll_loot(host, &table, pos, None, Some(entity)) else {
                break;
            };
            for st in rolled {
                let lim = upgrades::limit(&*host, &st, mult);
                let n = st.count();
                contents::insert(&mut c.slots, &st, n, lim);
            }
        }
    }
    c.write(&*host, &mut stack, &item);
    Some(ItemDrop::new(stack, pos, [0.0, 0.2, 0.0], 10))
}

/// Whether `id` names one of the backpack items.
pub fn is_backpack(id: &str) -> bool {
    storage_items::by_item(id).is_some_and(|k| k.family == Family::Backpack)
}

/// The roll for a kill's backpack drop, 0 up to 1, the same for the same mob.
pub fn drop_roll(entity: u32, pos: [f64; 3]) -> f64 {
    Dice::seeded(
        &[u64::from(entity), pos[0].to_bits(), pos[2].to_bits()],
        "drop",
    )
    .unit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_roll_is_the_same_for_the_same_spawn() {
        let a = roll(
            "minecraft:zombie",
            [1.0, 64.0, 2.0],
            "minecraft:overworld",
            3.0,
            1.0,
            true,
        );
        let b = roll(
            "minecraft:zombie",
            [1.0, 64.0, 2.0],
            "minecraft:overworld",
            3.0,
            1.0,
            true,
        );
        let (a, b) = (a.unwrap(), b.unwrap());
        assert_eq!((a.tier, a.difficulty), (b.tier, b.difficulty));
    }

    #[test]
    fn no_chance_means_no_backpack_and_every_wearer_has_its_minimum_difficulty() {
        assert!(roll("minecraft:zombie", [0.0; 3], "d", 3.0, 0.0, true).is_none());
        for x in 0..200 {
            let w = roll(
                "minecraft:zombie",
                [f64::from(x), 1.0, 1.0],
                "d",
                6.0,
                1.0,
                true,
            )
            .unwrap();
            assert!(
                w.difficulty >= w.min_difficulty
                    && w.difficulty <= MAX_DIFFICULTY.max(w.min_difficulty)
            );
            assert!(
                w.tier <= 4,
                "high local difficulty skips the rarest tier and leather's pool"
            );
        }
    }

    #[test]
    fn drop_chance_scales_with_looting_and_tier() {
        assert!(drops(0, 0, 0.5, 0.15, 0.49));
        assert!(!drops(0, 0, 0.5, 0.15, 0.5));
        assert!(drops(0, 1, 0.5, 0.15, 0.64));
        assert!(drops(5, 0, 0.5, 0.15, 0.99));
    }

    #[test]
    fn effects_are_distinct_and_capped() {
        for e in 0..100 {
            let got = effects(e, [3.0, 70.0, 9.0], 3, 1);
            assert_eq!(got.len(), 5);
            let mut kinds: Vec<&str> = got.iter().map(|(n, _)| *n).collect();
            kinds.sort_unstable();
            kinds.dedup();
            assert_eq!(kinds.len(), 5, "each entry once");
            for (name, amp) in got {
                assert!((1..=3).contains(&amp));
                if name == "minecraft:resistance" || name == "minecraft:regeneration" {
                    assert_eq!(amp, 1, "capped at amplifier 1");
                }
            }
        }
        assert_eq!(effects(1, [0.0; 3], 0, 0).len(), 2);
    }

    #[test]
    fn health_is_raised_by_the_minimum_difficulty() {
        assert_eq!(health_bonus(20.0, 2), Some(40.0));
        assert_eq!(health_bonus(20.0, 0), None);
    }
}
