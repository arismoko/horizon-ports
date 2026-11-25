//! Clumps for Sunrise: merges nearby experience orbs into a single orb carrying their
//! summed XP value and clears `Player.takeXpDelay` so players collect orbs without the
//! vanilla 2-tick pickup cooldown.
use mod_api::{
    ExperienceOrbCanMergeEvent, ExperienceOrbMergeEvent, Flow, Hooks, PlayerExpCooldownChangeEvent,
    ServerHost, ServerTickEvent, ServerTickPhase,
};

/// Maximum XP value an experience orb can store in vanilla NBT (`Value` is a signed short,
/// and `ServerHost::set_experience_orb` refuses values outside `i16::MIN..=i16::MAX`).
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
        if host.set_experience_orb(event.dimension, event.into, Some(total), Some(1), None) {
            Flow::Continue
        } else {
            Flow::Cancel
        }
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
