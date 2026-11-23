//! Where a storage item is: a player's inventory slot, a trinkets slot, or a placed block's block entity; and the
//! player's inventory as the menu and the upgrades read and write it.
use mod_api::{
    AttachmentCodec, AttachmentError, AttachmentType, Compound, Holder, Host, HostTransaction,
    ItemStack, PlayerRef, ServerHost, Tag, Text,
};

use crate::contents::{self, Nbt};
use crate::storage_items::{self, StorageItem};

fn same_compound(c: &Compound) -> Result<Compound, AttachmentError> {
    Ok(c.clone())
}
fn empty() -> Compound {
    Compound::new()
}

/// native-next/trinkets' worn stacks (slot id -> item NBT), read through the same attachment key, as things,
/// elytraslot and universalgraves do. The back slot is where Homestead's players wore their backpacks.
pub static WORN: AttachmentType<Compound> = AttachmentType {
    namespace: "trinkets",
    name: "worn",
    default: empty,
    codec: Some(AttachmentCodec {
        encode: same_compound,
        decode: same_compound,
    }),
    copy_on_death: true,
    sync: true,
};

pub const BACK: &str = "chest/back";

#[derive(Clone, Debug, PartialEq)]
pub enum Loc {
    /// `Inventory` slot: 0 to 35 the main inventory, 38 the chest, 40 the off hand.
    Inv(u32),
    /// A trinkets slot id.
    Worn(String),
    /// A placed storage block's block entity (its one slot holds the item).
    Block { dim: String, pos: [i32; 3] },
}

pub fn worn(host: &mut dyn Host, uuid: u128, slot: &str) -> Option<ItemStack> {
    let c = WORN.get(host, Holder::Player(uuid)).unwrap_or_default();
    match c.at(slot) {
        Some(Tag::Compound(t)) => ItemStack::from_nbt(&*host, t),
        _ => None,
    }
}

pub fn set_worn(host: &mut dyn Host, uuid: u128, slot: &str, stack: Option<ItemStack>) -> bool {
    let mut c = WORN.get(host, Holder::Player(uuid)).unwrap_or_default();
    match stack.and_then(|s| s.to_nbt(&*host)) {
        Some(t) => {
            c.insert(slot.into(), Tag::Compound(t));
        }
        None => {
            c.del(slot);
        }
    }
    WORN.set(host, Holder::Player(uuid), c).is_ok()
}

/// The player's inventory slots 0..=40 (main, armour, off hand), as owned stacks.
fn readable(host: &dyn Host, player: PlayerRef) -> Option<(Vec<Option<ItemStack>>, bool)> {
    player.with_inventory(host, |inventory| {
        let mut inv = vec![None; 41];
        let mut whole = true;
        for (slot, view) in inventory.iter() {
            if (slot as usize) < 41 {
                match view.project() {
                    Some(stack) => inv[slot as usize] = Some(stack),
                    None => whole = false,
                }
            }
        }
        (inv, whole)
    })
}

/// The player's inventory slots 0..=40 (main, armour, off hand).
///
/// A slot whose stack can't be handed to a mod as an owned value reads as empty: it is not. So nothing may be put
/// into a slot this reads as empty unless `complete` says the read was whole (`give` and the pickup check it).
pub fn inventory(host: &mut dyn ServerHost, player: PlayerRef) -> Vec<Option<ItemStack>> {
    readable(&*host, player).map_or_else(|| vec![None; 41], |(inv, _)| inv)
}

/// Whether `inventory` reads every slot as it is.
pub fn complete(host: &dyn Host, player: PlayerRef) -> bool {
    readable(host, player).is_some_and(|(_, whole)| whole)
}

pub fn set_slot(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    slot: u32,
    stack: Option<ItemStack>,
) -> bool {
    match player.set_inventory_slot(host, slot, stack) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("[sophisticatedcore] set_inventory_slot {slot}: {e:?}");
            false
        }
    }
}

pub fn read(host: &mut dyn ServerHost, uuid: u128, loc: &Loc) -> Option<ItemStack> {
    match loc {
        Loc::Inv(slot) => {
            let p = host.player(uuid)?;
            inventory(host, p).get(*slot as usize).cloned().flatten()
        }
        Loc::Worn(slot) => worn(host, uuid, slot),
        Loc::Block { dim, pos } => {
            let level = host.level(dim)?;
            let be = host.block_entity(level, *pos)?;
            be.item_slot(&*host, 0).ok().flatten()
        }
    }
}

pub fn write(host: &mut dyn ServerHost, uuid: u128, loc: &Loc, stack: Option<ItemStack>) -> bool {
    match loc {
        Loc::Inv(slot) => match host.player(uuid) {
            Some(p) => set_slot(host, p, *slot, stack),
            None => false,
        },
        Loc::Worn(slot) => set_worn(host, uuid, slot, stack),
        Loc::Block { dim, pos } => {
            let Some(level) = host.level(dim) else {
                return false;
            };
            let Some(be) = host.block_entity(level, *pos) else {
                return false;
            };
            let ok = host
                .transaction(|host, tx| {
                    be.set_item_slot(host, tx, 0, stack.clone()).map_err(|_| ())
                })
                .is_ok();
            if ok {
                let _ = be.set_changed(host);
                host.block_entity_sync(*pos);
            }
            ok
        }
    }
}

/// The storage item at `loc`, if it is one.
pub fn storage_at(
    host: &mut dyn ServerHost,
    uuid: u128,
    loc: &Loc,
) -> Option<(ItemStack, &'static StorageItem)> {
    let st = read(host, uuid, loc)?;
    let kind = storage_items::by_item(&contents::item_id(&*host, &st))?;
    Some((st, kind))
}

/// Where the storage with contents `id` is now: `loc` if it is still there, else (for an inventory slot) the
/// slot it moved to.
pub fn find(host: &mut dyn ServerHost, uuid: u128, loc: &Loc, id: [i32; 4]) -> Option<Loc> {
    if let Some((st, _)) = storage_at(host, uuid, loc)
        && contents::uuid(&st) == Some(id)
    {
        return Some(loc.clone());
    }
    if let Loc::Inv(_) = loc {
        let p = host.player(uuid)?;
        for (slot, st) in inventory(host, p).into_iter().enumerate() {
            if let Some(st) = st
                && contents::uuid(&st) == Some(id)
            {
                return Some(Loc::Inv(slot as u32));
            }
        }
    }
    None
}

/// Puts `stack` into the player's inventory (Inventory.add: onto the same item first, then empty slots, hotbar
/// first); what didn't fit.
pub fn give(host: &mut dyn ServerHost, player: PlayerRef, stack: &ItemStack) -> u32 {
    if !complete(&*host, player) {
        // Some slot can't be read: the game's own Inventory.add knows what is where. What doesn't fit is thrown to
        // the player (only they may pick it up), so nothing is left over.
        return match player.give_stack(host, stack.clone()) {
            Ok(rest) => rest.map_or(0, |rest| rest.count()),
            Err(e) => {
                eprintln!("[sophisticatedcore] give_stack: {e:?}");
                stack.count()
            }
        };
    }
    let mut inv = inventory(host, player);
    let max = host.max_stack_size(stack.item()).unwrap_or(64).max(1);
    let mut left = stack.count();
    let mut changed = Vec::new();
    for pass in 0..2 {
        for slot in 0..36 {
            if left == 0 {
                break;
            }
            let put = match (&inv[slot], pass) {
                (Some(h), 0) if contents::same(h, stack) => max.saturating_sub(h.count()).min(left),
                (None, 1) => max.min(left),
                _ => 0,
            };
            if put == 0 {
                continue;
            }
            let count = inv[slot].as_ref().map_or(0, |h| h.count()) + put;
            inv[slot] = contents::with_count(stack, count);
            left -= put;
            changed.push(slot);
        }
    }
    for slot in changed {
        set_slot(host, player, slot as u32, inv[slot].clone());
    }
    left
}

/// Gives `stack`, dropping at the player's feet what doesn't fit.
pub fn give_or_drop(host: &mut dyn ServerHost, player: PlayerRef, stack: &ItemStack) {
    let left = give(host, player, stack);
    if left > 0
        && let Some(rest) = contents::with_count(stack, left)
    {
        drop_at_player(host, player, rest);
    }
}

pub fn drop_at_player(host: &mut dyn ServerHost, player: PlayerRef, stack: ItemStack) {
    let Some(dim) = player.dimension(&*host).map(|d| d.to_string()) else {
        return;
    };
    let Some(level) = host.level(&dim) else {
        return;
    };
    let Some(pos) = player.entity().position(&*host) else {
        return;
    };
    let at = [
        pos[0].floor() as i32,
        pos[1].floor() as i32,
        pos[2].floor() as i32,
    ];
    let _ = level.pop_resource(host, at, stack);
}

pub fn message(host: &mut dyn ServerHost, uuid: u128, text: &str) {
    if let Some(p) = host.player(uuid) {
        let _ = p.action_bar(host, Text::literal(text));
    }
}

pub fn creative(host: &dyn Host, uuid: u128) -> bool {
    host.player(uuid).and_then(|p| p.is_creative(host)) == Some(true)
}

pub fn sneaking(host: &dyn Host, uuid: u128) -> bool {
    host.player(uuid)
        .and_then(|p| p.entity().is_shift_key_down(host))
        == Some(true)
}
