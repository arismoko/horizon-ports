//! The infinity upgrades (InfinityUpgradeItem, InfinityInventoryPart): every filled slot holds an endless supply of
//! its item. Taking out copies the item and keeps the slot; only an empty slot takes a stack (by a player who has
//! the upgrade's permission level: 2 for the admin upgrade, 0 for the survival one), and nothing else goes in.
//! The upgrade is the storage's only one, and only such a player may remove it or break the placed backpack.
use mod_api::{Host, ItemStack, PlayerRef};

use crate::contents::{self, Contents};
use crate::storage_items::{Family, StorageItem};
use crate::upgrades::{self, Kind};

/// What an infinite slot shows as its count (`Integer.MAX_VALUE`).
pub const SHOWN: u32 = i32::MAX as u32;

/// The permission level the storage's infinity upgrade asks for (`getPermissionLevel`), if it has one.
pub fn level(host: &dyn Host, ups: &[Option<ItemStack>]) -> Option<u8> {
    ups.iter()
        .flatten()
        .find_map(|u| match upgrades::of(host, u) {
            Some(upgrades::Upgrade {
                kind: Kind::Infinity(level),
                ..
            }) => Some(level),
            _ => None,
        })
}

/// The stacks as the menu and pipes see them: filled slots with their endless count.
pub fn shown(slots: &[Option<ItemStack>]) -> Vec<Option<ItemStack>> {
    slots
        .iter()
        .map(|s| s.as_ref().and_then(|s| contents::with_count(s, SHOWN)))
        .collect()
}

/// Whether `slots` hold `stack`'s item: it can be taken out as much as asked.
pub fn has(slots: &[Option<ItemStack>], stack: &ItemStack) -> bool {
    slots.iter().flatten().any(|s| contents::same(s, stack))
}

/// Puts `stack` into the first empty slot; whether there was one.
pub fn put(slots: &mut [Option<ItemStack>], stack: &ItemStack) -> bool {
    match slots.iter_mut().find(|s| s.is_none()) {
        Some(cell) => {
            *cell = contents::with_count(stack, 1);
            true
        }
        None => false,
    }
}

/// The refusal for adding an upgrade: none is added next to infinity, and infinity only to a storage with none
/// (`UPGRADE_CONFLICT_DEFINITIONS`).
pub fn conflict(
    has_infinity: bool,
    adding_infinity: bool,
    any_upgrade: bool,
) -> Option<&'static str> {
    if has_infinity {
        Some("No upgrade can be added next to an infinity upgrade")
    } else if adding_infinity && any_upgrade {
        Some("There's already an upgrade in another upgrade slot")
    } else {
        None
    }
}

pub fn may_add(host: &dyn Host, c: &Contents, adding_infinity: bool) -> Result<(), &'static str> {
    match conflict(
        level(host, &c.upgrades).is_some(),
        adding_infinity,
        c.upgrades.iter().any(Option::is_some),
    ) {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Whether `player` may take the infinity upgrade out, or break or pick up a backpack that has it
/// (`canRemoveUpgradeFrom`, `handleBreakBackpackWithInfinityUpgrade`).
pub fn allowed(host: &dyn Host, player: PlayerRef, ups: &[Option<ItemStack>]) -> bool {
    level(host, ups).is_none_or(|need| permits(player.permission_level(host), need))
}

/// Whether a permission level is enough for `need`.
pub fn permits(have: Option<u8>, need: u8) -> bool {
    have.unwrap_or(0) >= need
}

/// Whether a placed backpack's contents bar `player` from breaking or picking it up.
pub fn guards(host: &dyn Host, player: PlayerRef, kind: &StorageItem, st: &ItemStack) -> bool {
    kind.family == Family::Backpack
        && !allowed(host, player, &Contents::read(host, st, kind).upgrades)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_levels_compare() {
        assert!(permits(Some(2), 2));
        assert!(permits(Some(4), 2));
        assert!(!permits(Some(0), 2));
        assert!(!permits(None, 2));
        assert!(permits(None, 0));
    }

    #[test]
    fn infinity_conflicts_both_ways() {
        assert!(conflict(true, false, true).is_some());
        assert!(conflict(false, true, true).is_some());
        assert!(conflict(false, true, false).is_none());
        assert!(conflict(false, false, true).is_none());
    }
}
