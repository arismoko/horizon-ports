#![deny(unsafe_code)]
//! Clumps for Sunrise: merges nearby experience orbs into a single orb carrying their
//! summed XP value and clears `Player.takeXpDelay` so players collect orbs without the
//! vanilla 2-tick pickup cooldown.
use mod_api::{
    ExperienceOrbCanMergeEvent, ExperienceOrbMergeEvent, Flow, Hooks, PickupXpEvent,
    PlayerExpCooldownChangeEvent, ServerHost, ServerTickEvent, ServerTickPhase,
};

/// Posted when two orbs clump, before the merged value is written (Clumps' `ValueEvent`).
/// A listener may change `value`; a result outside `1..=`[`MAX_ORB_VALUE`] is ignored.
#[derive(mod_api::Event)]
#[event(id = "clumps:value", side = server)]
#[non_exhaustive]
pub struct ValueEvent {
    /// The entity id of the orb the clump is written into.
    pub orb: u32,
    /// The clump's combined XP.
    pub value: i32,
}

/// Posted before a picked-up clump's XP runs Mending (Clumps' `RepairEvent`). A listener may
/// change `points`, or cancel the event to skip the repair so all points go to the player.
#[derive(mod_api::Event)]
#[event(id = "clumps:repair", side = server, cancellable)]
#[non_exhaustive]
pub struct RepairEvent {
    /// The picking-up player's uuid.
    pub player: u128,
    /// The XP points offered to Mending.
    pub points: i32,
    /// Whether a listener cancelled the repair.
    pub cancelled: bool,
}

/// The XP a pickup of `count` orbs of `value` awards, or `None` for a non-positive or
/// overflowing total.
pub fn pickup_points(value: i32, count: u32) -> Option<i32> {
    let total = i64::from(value).checked_mul(i64::from(count))?;
    if total < 1 {
        return None;
    }
    i32::try_from(total).ok()
}

/// The points to give the player after Mending: the engine's leftover, or all `points` when
/// the repair did not run (cancelled event or stale player).
pub fn points_after_repair(points: i32, repaired: Option<i32>) -> i32 {
    repaired.map_or(points, |left| left.clamp(0, points))
}

/// Maximum XP value an experience orb can store in vanilla NBT (`Value` is a signed short,
/// and saving truncates the live `i32` value to a short, so clumps stay within it).
pub const MAX_ORB_VALUE: i32 = i16::MAX as i32;

/// Whether two experience orbs should be allowed to clump together, relaxing vanilla's
/// `ExperienceOrb.canMerge` same-value and `id % 40` restrictions.
///
/// Only positive-value orbs are clumped: non-positive orbs are left to vanilla so a
/// zero-value orb never triggers a count increment when the summed value is unchanged and
/// a negative-value orb never subtracts XP from a positive clump. Pairs whose individual
/// values already exceed [`MAX_ORB_VALUE`] are also refused here so they form a separate
/// clump rather than overflowing `i16::MAX`.
pub fn can_clump_merge(into: u32, from: u32, into_value: i32, from_value: i32) -> bool {
    if into == from || into_value <= 0 || from_value <= 0 {
        return false;
    }
    into_value
        .checked_add(from_value)
        .is_some_and(|sum| sum <= MAX_ORB_VALUE)
}

/// Computes the exact combined XP value for merging `(from_value, from_count)` into
/// `(into_value, into_count)` as a single `count = 1` orb.
///
/// Returns `None` if either orb has non-positive value or zero count, if the arithmetic
/// overflows, or if the combined total exceeds [`MAX_ORB_VALUE`]. Because all inputs are
/// strictly positive when `Some(total)` is returned, `total > into_value` always holds,
/// guaranteeing `(total, 1) != (into_value, into_count)` so Sunrise accepts the write as
/// the merged orb without incrementing `count`.
pub fn merged_orb_value(
    into_value: i32,
    into_count: u32,
    from_value: i32,
    from_count: u32,
) -> Option<i32> {
    if into_value <= 0 || from_value <= 0 || into_count == 0 || from_count == 0 {
        return None;
    }
    let into_total = i64::from(into_value).checked_mul(i64::from(into_count))?;
    let from_total = i64::from(from_value).checked_mul(i64::from(from_count))?;
    let total = into_total.checked_add(from_total)?;
    if total < 1 || total > i64::from(MAX_ORB_VALUE) {
        return None;
    }
    i32::try_from(total).ok()
}

#[derive(Default)]
struct Mod;

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn experience_orb_can_merge(
        &self,
        _host: &mut dyn ServerHost,
        event: &mut ExperienceOrbCanMergeEvent<'_>,
    ) {
        if event.into_value <= 0 || event.from_value <= 0 {
            return;
        }
        event.can_merge =
            can_clump_merge(event.into, event.from, event.into_value, event.from_value);
    }

    fn experience_orb_merge(
        &self,
        host: &mut dyn ServerHost,
        event: &mut ExperienceOrbMergeEvent<'_>,
    ) -> Flow {
        if event.into_value <= 0 || event.from_value <= 0 {
            return Flow::Continue;
        }
        let Some(total) = merged_orb_value(
            event.into_value,
            event.into_count,
            event.from_value,
            event.from_count,
        ) else {
            return Flow::Cancel;
        };
        let mut value_event = ValueEvent {
            orb: event.into,
            value: total,
        };
        host.post(&mut value_event);
        let total = if (1..=MAX_ORB_VALUE).contains(&value_event.value) {
            value_event.value
        } else {
            total
        };
        let Some(entity) = host.entity(event.into) else {
            return Flow::Cancel;
        };
        let Ok(Some(orb)) = host.experience_orb(entity) else {
            return Flow::Cancel;
        };
        // Count first so an invalid count changes nothing; the value write then makes
        // `(total, 1)` the merged orb.
        if orb.set_count(host, 1).is_err() || orb.set_value(host, total).is_err() {
            return Flow::Cancel;
        }
        Flow::Continue
    }

    /// Takes over the pickup so Mending runs through the engine's `repairPlayerItems` on the
    /// clump's whole XP (honouring `xp_repair_ratio` and `player_item_mend`), then the rest
    /// goes to the player. Anything unreadable leaves vanilla's pickup untouched.
    fn pickup_xp(&self, host: &mut dyn ServerHost, event: &mut PickupXpEvent<'_>) -> Flow {
        let orb = event.orb;
        let (Ok(value), Ok(count)) = (orb.value(&*host), orb.count(&*host)) else {
            return Flow::Continue;
        };
        let Some(points) = pickup_points(value, count) else {
            return Flow::Continue;
        };
        let Some(player) = host.player(event.player) else {
            return Flow::Continue;
        };
        let mut repair = RepairEvent {
            player: event.player,
            points,
            cancelled: false,
        };
        host.post(&mut repair);
        let offered = repair.points.clamp(0, points);
        let repaired = if repair.cancelled || offered == 0 {
            None
        } else {
            player
                .repair_items_with_xp(host, offered)
                .map(|left| left + (points - offered))
        };
        let _ = orb.entity().kill(host);
        let rest = points_after_repair(points, repaired);
        if rest > 0 {
            let _ = player.give_experience_points(host, rest);
        }
        Flow::Cancel
    }

    fn player_exp_cooldown_change(
        &self,
        host: &mut dyn ServerHost,
        event: &mut PlayerExpCooldownChangeEvent<'_>,
    ) {
        if event.ticks > 0
            && let Some(player) = host.player(event.player)
        {
            let _ = player.set_exp_cooldown(host, 0);
        }
    }

    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        if !matches!(event.phase, ServerTickPhase::ServerTickEnd { .. }) {
            return;
        }
        for player in host.players().server_players() {
            if player.exp_cooldown(&*host).is_some_and(|ticks| ticks > 0) {
                let _ = player.set_exp_cooldown(host, 0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pickup_points_multiplies_and_rejects_bad_totals() {
        assert_eq!(pickup_points(76, 1), Some(76));
        assert_eq!(pickup_points(5, 3), Some(15));
        assert_eq!(pickup_points(0, 4), None);
        assert_eq!(pickup_points(-3, 2), None);
        assert_eq!(pickup_points(5, 0), None);
        assert_eq!(pickup_points(i32::MAX, 2), None);
    }

    #[test]
    fn points_after_repair_keeps_leftover_or_everything() {
        assert_eq!(points_after_repair(76, Some(10)), 10);
        assert_eq!(points_after_repair(76, Some(0)), 0);
        assert_eq!(points_after_repair(76, None), 76);
        assert_eq!(points_after_repair(76, Some(200)), 76);
        assert_eq!(points_after_repair(76, Some(-4)), 0);
    }

    #[test]
    fn allows_merging_any_two_distinct_positive_orbs_within_limit() {
        assert!(can_clump_merge(1, 2, 3, 5));
        assert!(can_clump_merge(1, 0, 10, 7)); // spawning orb (from == 0)
        assert!(can_clump_merge(1, 2, 20_000, 12_767)); // exact i16::MAX
    }

    #[test]
    fn rejects_self_non_positive_or_overflowing_can_merge() {
        assert!(!can_clump_merge(5, 5, 3, 3));
        assert!(!can_clump_merge(1, 2, 0, 5));
        assert!(!can_clump_merge(1, 2, 5, 0));
        assert!(!can_clump_merge(1, 2, -5, 10));
        assert!(!can_clump_merge(1, 2, 20_000, 12_768));
        assert!(!can_clump_merge(1, 2, i32::MAX, 1));
    }

    #[test]
    fn sums_values_and_counts_into_single_orb() {
        assert_eq!(merged_orb_value(3, 1, 5, 1), Some(8));
        assert_eq!(merged_orb_value(3, 2, 5, 1), Some(11));
        assert_eq!(merged_orb_value(7, 3, 11, 2), Some(43));
        assert_eq!(merged_orb_value(20_000, 1, 12_767, 1), Some(MAX_ORB_VALUE));
    }

    #[test]
    fn refuses_merge_past_nbt_short_ceiling_or_invalid_inputs() {
        assert_eq!(merged_orb_value(20_000, 1, 12_768, 1), None);
        assert_eq!(merged_orb_value(10_000, 3, 2_768, 1), None);
        assert_eq!(merged_orb_value(3, 0, 5, 1), None);
        assert_eq!(merged_orb_value(3, 1, 5, 0), None);
        assert_eq!(merged_orb_value(0, 1, 5, 1), None);
        assert_eq!(merged_orb_value(5, 1, 0, 1), None);
        assert_eq!(merged_orb_value(-1, 1, 5, 1), None);
        assert_eq!(merged_orb_value(1, u32::MAX, 1, 1), None);
        assert_eq!(
            merged_orb_value(i32::MAX, u32::MAX, i32::MAX, u32::MAX),
            None
        );
    }

    #[test]
    fn accepted_merge_always_changes_orb_value() {
        for (iv, ic, fv, fc) in [(1, 1, 1, 1), (3, 2, 1, 1), (100, 1, 50, 2)] {
            let total = merged_orb_value(iv, ic, fv, fc).expect("valid merge");
            assert!(
                total > iv,
                "total {total} must strictly exceed into_value {iv}"
            );
            assert_ne!((total, 1), (iv, ic));
        }
    }
}
