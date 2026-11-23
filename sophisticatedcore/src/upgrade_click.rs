//! Putting upgrades into a placed storage with a click (StorageBlockBase.tryAddSingleUpgrade, `tryFillUpgrades`, and
//! ControllerBlock.use / trySneakItemInteraction): a click on a storage's front with an upgrade adds one; sneaking
//! adds every one in the hand that fits. A click on a controller does the same to every storage of its network (and
//! applies a held tier upgrade to them).
use mod_api::{ItemStack, LevelRef, PlayerRef, ServerHost};

use crate::contents::{self, Contents};
use crate::storage::{self, block_id, kind_of_block, main_pos, read_be, write_be};
use crate::{menu, network, places, upgrades};

/// One of `up` into the first upgrade slot of the storage at `pos` that takes it (the original's insert into the
/// upgrade handler after `canAddUpgradeTo`).
pub fn add_one(host: &mut dyn ServerHost, level: LevelRef, pos: [i32; 3], up: &ItemStack) -> bool {
    let main = main_pos(&*host, level, pos);
    let Some(kind) = kind_of_block(&block_id(&*host, level, main)) else {
        return false;
    };
    // A tier upgrade is an upgrade item too, but it goes onto the block, not into a slot.
    if upgrades::of(&*host, up).is_none()
        || storage::tier_upgrade(&contents::item_id(&*host, up)).is_some()
    {
        return false;
    }
    let Some(mut stack) = read_be(&*host, level, main) else {
        return false;
    };
    let mut c = Contents::read(&*host, &stack, kind);
    let Some(mut one) = contents::with_count(up, 1) else {
        return false;
    };
    let Some(j) =
        (0..c.upgrades.len()).find(|&j| menu::may_add_upgrade(&*host, kind, &c, j, &one).is_ok())
    else {
        return false;
    };
    crate::compression::on_added(&*host, &c, &mut one);
    c.upgrades[j] = Some(one);
    c.write(&*host, &mut stack, kind);
    write_be(host, level, main, Some(stack))
}

/// What the player holds in `slot` now.
fn held(host: &mut dyn ServerHost, player: PlayerRef, slot: u32) -> Option<ItemStack> {
    places::inventory(host, player)
        .get(slot as usize)
        .cloned()
        .flatten()
        .filter(|s| s.count() > 0)
}

/// Takes one off the stack in `slot`.
fn take_one(host: &mut dyn ServerHost, player: PlayerRef, slot: u32, held: &ItemStack) {
    places::set_slot(host, player, slot, held.clone().shrink(1));
}

/// One upgrade, or with `fill` every one in the hand that fits, into the storage at `pos`; whether any went in.
fn into_storage(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    slot: u32,
    level: LevelRef,
    pos: [i32; 3],
    fill: bool,
) -> bool {
    let mut any = false;
    while let Some(h) = held(host, player, slot) {
        if !add_one(host, level, pos, &h) {
            break;
        }
        take_one(host, player, slot, &h);
        any = true;
        if !fill {
            break;
        }
    }
    any
}

/// The click of a held upgrade on the block at `pos` (`face` the face clicked): whether it did something, so the
/// click is spent. A storage takes it on its front only; a controller gives it to each storage of its network.
pub fn click(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    slot: u32,
    level: LevelRef,
    pos: [i32; 3],
    face: [i32; 3],
    sneaking: bool,
) -> bool {
    let block = block_id(&*host, level, pos);
    if block == network::CONTROLLER {
        let net = network::net_of(&*host, level, pos);
        let mut any = false;
        for &at in &net.storages {
            if sneaking {
                any |= into_storage(host, player, slot, level, at, true);
                continue;
            }
            let Some(h) = held(host, player, slot) else {
                break;
            };
            any |= into_storage(host, player, slot, level, at, false)
                || storage::use_tool(host, player, slot, &h, level, at);
        }
        return any;
    }
    if kind_of_block(&block).is_none() {
        return false;
    }
    let facing = level
        .block_property(&*host, pos, "facing")
        .unwrap_or_default();
    if face != storage::dir_vec(&facing) {
        return false;
    }
    into_storage(host, player, slot, level, pos, sneaking)
}
