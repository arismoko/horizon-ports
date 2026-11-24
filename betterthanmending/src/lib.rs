//! Better Than Mending for Sunrise: sneak and use a damaged item that has a
//! `minecraft:repair_with_xp` enchantment (Mending) to repair it with your own
//! experience, as Better Than Mending (legobmw99) does.
//!
//! With more than 2 points of experience the use is taken on both sides. On the
//! server, 20 points go when you have at least 30 and the item lacks at least 40
//! durability, otherwise 2 points; the item gets back the durability Mending gives
//! for those points.
use mod_api::{
    EnchantmentEffect, Flow, Hooks, Host, HostSide, InteractionResult, ItemResource, ItemStack,
    LivingRef, PlayerRef, RightClickItemEvent, ServerHost,
};

/// Experience points to reach `level` from 0, as the original's
/// `Utilities.getExperienceForLevel` computes them (in doubles, truncated).
pub fn experience_for_level(level: i32) -> i32 {
    let l = f64::from(level);
    if level == 0 {
        0
    } else if level > 0 && level < 16 {
        (l * l + 6.0 * l) as i32
    } else if level > 15 && level < 32 {
        (2.5 * l * l - 40.5 * l + 360.0) as i32
    } else {
        (4.5 * l * l - 162.5 * l + 2220.0) as i32
    }
}

/// The level `experience` points make (`Utilities.getLevelForExperience`).
pub fn level_for_experience(experience: i32) -> i32 {
    let mut level = 0;
    while experience_for_level(level) <= experience {
        level += 1;
    }
    level - 1
}

/// The points needed to fill `level`'s bar (`Player.getXpNeededForNextLevel`).
pub fn xp_needed_for_next_level(level: i32) -> i32 {
    if level >= 30 {
        112 + (level - 30) * 9
    } else if level >= 15 {
        37 + (level - 15) * 5
    } else {
        7 + level * 2
    }
}

/// A player's experience in points (`Utilities.getPlayerXP`): the level's points plus the
/// bar's share of the next, added as a float and truncated.
pub fn player_xp(level: i32, progress: f32, needed: i32) -> i32 {
    (experience_for_level(level) as f32 + progress * needed as f32) as i32
}

/// The points one use spends (`Common.doMend`): 20 with at least 30 points on an item
/// that lacks at least 40, else 2.
pub fn points_to_spend(xp: i32, damage: u32) -> u32 {
    if xp >= 30 && damage >= 40 { 20 } else { 2 }
}

/// Vanilla Mending's `minecraft:repair_with_xp` effect: two durability per point.
/// The original asks `EnchantmentHelper.modifyDurabilityToRepairFromXp`, which a data
/// pack's change to the effect would follow; the API has no such read (sunrise#5350).
const MENDING_REPAIR_PER_POINT: u32 = 2;

/// The new damage after spending `points` on an item with `damage`.
pub fn repaired_damage(damage: u32, points: u32) -> u32 {
    damage - (points * MENDING_REPAIR_PER_POINT).min(damage)
}

/// Level, points into the bar, and total after taking `points` (`Utilities.addPlayerXP`
/// with a negative amount). None when that would go below zero: nothing changes.
pub fn experience_after(xp: i32, points: u32) -> Option<(i32, i32, i32)> {
    let experience = xp - points as i32;
    if experience < 0 {
        return None;
    }
    let level = level_for_experience(experience);
    Some((level, experience - experience_for_level(level), experience))
}

fn repair_with_xp() -> EnchantmentEffect {
    EnchantmentEffect::new("minecraft:repair_with_xp").expect("a namespaced id")
}

/// The player's experience in points when this use mends (`Common.willMend`).
fn will_mend(host: &dyn Host, player: PlayerRef, stack: &ItemStack) -> Option<i32> {
    if player.entity().is_shift_key_down(host) != Some(true)
        || !stack.is_damaged()
        || !ItemResource::of(stack).has_enchantment_effect(host, repair_with_xp())
    {
        return None;
    }
    let exp = player.experience(host)?;
    let needed = player.xp_needed_for_next_level(host)?;
    let xp = player_xp(exp.level, exp.progress, needed);
    (xp > 2).then_some(xp)
}

/// `Common.doMend` on the server: repairs the hand's item, then takes the points.
fn mend(host: &mut dyn ServerHost, player: PlayerRef, event: &RightClickItemEvent<'_>, xp: i32) {
    let click = event.click;
    let points = points_to_spend(xp, click.stack.damage());
    let mut stack = click.stack.clone();
    stack.set_damage(repaired_damage(stack.damage(), points));
    if !matches!(
        LivingRef::new(player.entity()).set_hand_item(host, click.hand, Some(stack)),
        Ok(true)
    ) {
        return;
    }
    let Some((level, into_bar, total)) = experience_after(xp, points) else {
        return;
    };
    player.set_experience_levels(host, level);
    player.set_experience_points(host, into_bar);
    player.set_total_experience(host, total);
}

#[derive(Default)]
struct Mod;

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn right_click_item(&self, host: HostSide<'_>, event: &mut RightClickItemEvent<'_>) -> Flow {
        let player_id = event.click.player;
        match host {
            HostSide::Server(host) => {
                let Some(player) = host.player(player_id) else {
                    return Flow::Continue;
                };
                let Some(xp) = will_mend(&*host, player, event.click.stack) else {
                    return Flow::Continue;
                };
                mend(host, player, event, xp);
            }
            HostSide::Client(host) => {
                let host: &dyn Host = &*host;
                let Some(player) = host.player(player_id) else {
                    return Flow::Continue;
                };
                if will_mend(host, player, event.click.stack).is_none() {
                    return Flow::Continue;
                }
            }
        }
        *event.cancellation_result = InteractionResult::Success;
        event.cancelled = true;
        Flow::Cancel
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn experience_table_matches_vanilla_totals() {
        // The totals at the top of each vanilla formula band.
        assert_eq!(experience_for_level(1), 7);
        assert_eq!(experience_for_level(15), 315);
        assert_eq!(experience_for_level(16), 352);
        assert_eq!(experience_for_level(30), 1395);
        assert_eq!(experience_for_level(32), 1628);
        for level in 0..40 {
            assert_eq!(
                experience_for_level(level + 1) - experience_for_level(level),
                xp_needed_for_next_level(level)
            );
            assert_eq!(level_for_experience(experience_for_level(level)), level);
        }
    }

    #[test]
    fn spends_as_the_original() {
        assert_eq!(points_to_spend(30, 40), 20);
        assert_eq!(points_to_spend(29, 100), 2);
        assert_eq!(points_to_spend(100, 39), 2);
        assert_eq!(repaired_damage(100, 20), 60);
        assert_eq!(repaired_damage(3, 2), 0);
    }

    #[test]
    fn takes_points_off_the_bar_and_level() {
        // Level 3 with 3 of 13 points: 30 points; 20 go, leaving level 1 with 3 of 9.
        assert_eq!(player_xp(3, 3.0 / 13.0, 13), 30);
        assert_eq!(experience_after(30, 20), Some((1, 3, 10)));
        assert_eq!(experience_after(1, 2), None);
    }
}
