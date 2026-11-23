//! The upgrade items (UpgradeItemBase and its kinds), by the item's path, so the same code serves
//! `sophisticatedbackpacks:` and, later, `sophisticatedstorage:` upgrades. Their settings live in the upgrade's
//! own `custom_data`, under the jar's keys (FilterLogicBase: `filters`, `isAllowList`, `primaryMatch`; void:
//! `shouldVoidOverflow`; magnet: `pickupItems`, `pickupXp`; refill/crafting below).
use mod_api::{Compound, Host, ItemStack, Tag};

use crate::contents::{
    self, Nbt, custom_data, get_bool, get_str, put, read_items, set_custom_data, write_items,
};
use crate::storage_items::{Family, StorageItem};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Pickup,
    Filter,
    Magnet,
    Feeding,
    Compacting,
    Void,
    Restock,
    Deposit,
    Refill,
    Crafting,
    /// The jukebox upgrades: music discs played from the storage.
    Jukebox,
    /// HopperUpgradeWrapper on a placed storage: pulls from the inventory above, pushes to the one below.
    Hopper,
    Stack(f64),
    /// An upgrade this port has no rules for: it sits in its slot and does nothing.
    Inert,
}

#[derive(Clone, Copy, Debug)]
pub struct Upgrade {
    pub kind: Kind,
    pub advanced: bool,
    /// Filter slots (Homestead's sophisticatedbackpacks-server.toml, the jar's defaults).
    pub filters: usize,
    /// The filter's default: an allow list (void) or a block list (the rest).
    pub allow_default: bool,
}

/// The upgrade kind of an item id, if it is a Sophisticated upgrade.
pub fn upgrade(id: &str) -> Option<Upgrade> {
    let (ns, path) = id.split_once(':')?;
    if !ns.starts_with("sophisticated") {
        return None;
    }
    let advanced = path.starts_with("advanced_");
    let base = path.strip_prefix("advanced_").unwrap_or(path);
    let f = crate::options::filter_slots(base.strip_suffix("_upgrade").unwrap_or(base), advanced);
    let u = |kind, filters, allow_default| {
        Some(Upgrade {
            kind,
            advanced,
            filters,
            allow_default,
        })
    };
    match base {
        "pickup_upgrade" => u(Kind::Pickup, f, false),
        "filter_upgrade" => u(Kind::Filter, f, false),
        "magnet_upgrade" => u(Kind::Magnet, f, false),
        "feeding_upgrade" => u(Kind::Feeding, f, false),
        "compacting_upgrade" => u(Kind::Compacting, f, false),
        "void_upgrade" => u(Kind::Void, f, true),
        "restock_upgrade" => u(Kind::Restock, f, false),
        "deposit_upgrade" => u(Kind::Deposit, f, false),
        "refill_upgrade" => u(Kind::Refill, f, true),
        "crafting_upgrade" => u(Kind::Crafting, 0, false),
        "jukebox_upgrade" => u(Kind::Jukebox, 0, false),
        "hopper_upgrade" => u(Kind::Hopper, 0, false),
        "stack_upgrade_starter_tier" => u(Kind::Stack(1.5), 0, false),
        "stack_upgrade_tier_1" => u(Kind::Stack(2.0), 0, false),
        "stack_upgrade_tier_1_plus" => u(Kind::Stack(3.0), 0, false),
        "stack_upgrade_tier_5" => u(Kind::Stack(32.0), 0, false),
        "stack_upgrade_tier_2" => u(Kind::Stack(4.0), 0, false),
        "stack_upgrade_tier_3" => u(Kind::Stack(8.0), 0, false),
        "stack_upgrade_tier_4" => u(Kind::Stack(16.0), 0, false),
        "stack_upgrade_omega_tier" => u(Kind::Stack(2_147_483_647.0), 0, false),
        "stack_downgrade_tier_1" => u(Kind::Stack(0.125), 0, false),
        "stack_downgrade_tier_2" => u(Kind::Stack(0.0625), 0, false),
        "stack_downgrade_tier_3" => u(Kind::Stack(0.03125), 0, false),
        p if p.ends_with("_upgrade") => u(Kind::Inert, 0, false),
        _ => None,
    }
}

/// The upgrade of a stack.
pub fn of(host: &dyn Host, stack: &ItemStack) -> Option<Upgrade> {
    upgrade(&contents::item_id(host, stack))
}

/// `maxUpgradesPerStorage`: the group an upgrade counts against, and its limit in a storage of `family`. Homestead's
/// backpacks: stack_upgrades|3, furnace_upgrades|1, jukebox_upgrade|1; its storages: stack_upgrades|2 in chests,
/// barrels and shulker boxes (none in limited barrels), furnace and jukebox upgrades 1.
pub fn group(id: &str, family: Family) -> Option<(&'static str, usize)> {
    let path = id.split_once(':').map_or(id, |(_, p)| p);
    let t = crate::options::tune();
    let limits = if family == Family::Backpack {
        t.backpack_groups
    } else {
        t.storage_groups
    };
    if path.starts_with("stack_upgrade") || path.starts_with("stack_downgrade") {
        (family != Family::LimitedBarrel).then_some(("stack_upgrades", limits[0]))
    } else if [
        "smelting_upgrade",
        "auto_smelting_upgrade",
        "smoking_upgrade",
        "auto_smoking_upgrade",
        "blasting_upgrade",
        "auto_blasting_upgrade",
    ]
    .contains(&path)
    {
        Some(("furnace_upgrades", limits[1]))
    } else if path == "jukebox_upgrade" || path == "advanced_jukebox_upgrade" {
        Some(("jukebox_upgrade", limits[2]))
    } else {
        None
    }
}

/// A slot's multiplier in a storage of `kind`: its base (the limited barrels') times the stack upgrades'.
pub fn multiplier(host: &dyn Host, kind: &StorageItem, upgrades: &[Option<ItemStack>]) -> f64 {
    stack_multiplier(host, upgrades) * f64::from(kind.base_multiplier())
}

/// What a storage's upgrades do to a slot's limit: StackUpgradeItem.getInventorySlotLimit's multiplier (the
/// product of the stack upgrades').
pub fn stack_multiplier(host: &dyn Host, upgrades: &[Option<ItemStack>]) -> f64 {
    let mut m = 1.0f64;
    for u in upgrades.iter().flatten() {
        if let Some(Upgrade {
            kind: Kind::Stack(x),
            ..
        }) = of(host, u)
        {
            m *= x;
        }
    }
    m
}

/// `stackUpgrade.nonStackableItems`: items stack upgrades don't change (the list file; shulker boxes always).
fn never_stacked(host: &dyn Host, id: &str) -> bool {
    id.ends_with("_shulker_box")
        || crate::options::listed(&crate::options::Lists::get(host).non_stackable, id)
}

/// InventoryHandler.getBaseStackLimit: how many of `stack` one slot holds.
pub fn limit(host: &dyn Host, stack: &ItemStack, multiplier: f64) -> u32 {
    let max = host.max_stack_size(stack.item()).unwrap_or(64).max(1);
    if never_stacked(host, &contents::item_id(host, stack)) {
        return max;
    }
    let v = (f64::from(max) * multiplier).floor();
    if v >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        (v as u32).max(1)
    }
}

// ---- filters (FilterLogic) ----

pub fn filters(host: &dyn Host, upgrade: &ItemStack, n: usize) -> Vec<Option<ItemStack>> {
    read_items(host, custom_data(upgrade).at("filters"), n)
}

pub fn set_filters(host: &dyn Host, upgrade: &mut ItemStack, f: &[Option<ItemStack>]) {
    let mut d = custom_data(upgrade);
    put(&mut d, "filters", write_items(host, f));
    set_custom_data(upgrade, d);
}

pub fn allow_list(upgrade: &ItemStack, u: &Upgrade) -> bool {
    get_bool(&custom_data(upgrade), "isAllowList").unwrap_or(u.allow_default)
}

pub fn match_mod(upgrade: &ItemStack) -> bool {
    get_str(&custom_data(upgrade), "primaryMatch").as_deref() == Some("mod")
}

pub fn flag(upgrade: &ItemStack, key: &str, default: bool) -> bool {
    get_bool(&custom_data(upgrade), key).unwrap_or(default)
}

pub fn set_flag(upgrade: &mut ItemStack, key: &str, v: bool) {
    let mut d = custom_data(upgrade);
    put(&mut d, key, Tag::Byte(i8::from(v)));
    set_custom_data(upgrade, d);
}

pub fn set_str(upgrade: &mut ItemStack, key: &str, v: &str) {
    let mut d = custom_data(upgrade);
    put(&mut d, key, Tag::String(v.into()));
    set_custom_data(upgrade, d);
}

/// FilterLogic.matchesFilter: an allow list matches what it lists (an empty one matches nothing), a block list
/// what it doesn't list (an empty one matches everything). Items match by item, or by mod.
pub fn matches(host: &dyn Host, upgrade: &ItemStack, u: &Upgrade, stack: &ItemStack) -> bool {
    // The stand-in of an unknown item is never fed, compacted, deposited, voided or pulled.
    if contents::is_unknown(stack) {
        return false;
    }
    if u.filters == 0 {
        return true;
    }
    let f = filters(host, upgrade, u.filters);
    let by_mod = match_mod(upgrade);
    let ns = |s: &ItemStack| {
        contents::item_id(host, s)
            .split(':')
            .next()
            .unwrap_or("")
            .to_string()
    };
    let hit = f.iter().flatten().any(|x| {
        if by_mod {
            ns(x) == ns(stack)
        } else {
            x.same_item(stack)
        }
    });
    if allow_list(upgrade, u) { hit } else { !hit }
}

/// The crafting upgrade's grid (CraftingUpgradeWrapper keeps it in the upgrade: `craftingInventory`).
pub fn grid(host: &dyn Host, upgrade: &ItemStack) -> Vec<Option<ItemStack>> {
    read_items(host, custom_data(upgrade).at("craftingInventory"), 9)
}

pub fn set_grid(host: &dyn Host, upgrade: &mut ItemStack, g: &[Option<ItemStack>]) {
    let mut d = custom_data(upgrade);
    put(&mut d, "craftingInventory", write_items(host, g));
    set_custom_data(upgrade, d);
}

/// A copy of `c` without the keys a settings view writes (so two upgrades compare by kind).
pub fn _settings_keys(c: &mut Compound) {
    for k in [
        "filters",
        "isAllowList",
        "primaryMatch",
        "shouldVoidOverflow",
        "pickupItems",
        "pickupXp",
        "craftingInventory",
        "discInventory",
        "isPlaying",
        "shuffle",
        "repeatMode",
        "discSlotActive",
        "discPlayed",
    ] {
        c.del(k);
    }
}

/// Feeding's food: (nutrition, saturation modifier) of an edible stack (`minecraft:food` with nutrition >= 1).
pub fn food(host: &dyn Host, stack: &ItemStack) -> Option<(i32, f32)> {
    let own = stack
        .components()
        .and_then(|c| c.as_nbt().at("minecraft:food").cloned());
    let t = own.or_else(|| host.item_default_component(stack.item(), "minecraft:food"))?;
    let Tag::Compound(c) = t else { return None };
    let n = contents::get_int(&c, "nutrition")?;
    let s = match c.at("saturation") {
        Some(Tag::Float(b)) => f32::from_bits(*b),
        Some(Tag::Double(b)) => f64::from_bits(*b) as f32,
        _ => 0.0,
    };
    (n >= 1).then_some((n, s))
}

/// What eating leaves (`minecraft:use_remainder`), e.g. a bowl.
pub fn use_remainder(host: &dyn Host, stack: &ItemStack) -> Option<ItemStack> {
    let own = stack
        .components()
        .and_then(|c| c.as_nbt().at("minecraft:use_remainder").cloned());
    let t = own.or_else(|| host.item_default_component(stack.item(), "minecraft:use_remainder"))?;
    let Tag::Compound(c) = t else { return None };
    ItemStack::from_nbt(host, &c)
}

/// Disc slots of a jukebox upgrade (the basic one holds one, the advanced one `advancedJukeboxUpgrade`'s `numberOfSlots`).
pub fn disc_slots(u: &Upgrade) -> usize {
    if u.advanced {
        crate::options::tune().jukebox_slots
    } else {
        1
    }
}

/// `VoidUpgradeWrapper.shouldVoidOverflow`: overflow only, always when the setting turns "void anything" off.
pub fn void_overflow(u: &Upgrade, stack: &ItemStack) -> bool {
    !crate::options::tune().void_anything[usize::from(u.advanced)]
        || flag(stack, "shouldVoidOverflow", false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds() {
        assert_eq!(
            upgrade("sophisticatedbackpacks:pickup_upgrade")
                .unwrap()
                .kind,
            Kind::Pickup
        );
        let a = upgrade("sophisticatedbackpacks:advanced_magnet_upgrade").unwrap();
        assert!(a.advanced && a.filters == 16 && a.kind == Kind::Magnet);
        assert_eq!(
            upgrade("sophisticatedbackpacks:stack_upgrade_tier_2")
                .unwrap()
                .kind,
            Kind::Stack(4.0)
        );
        assert_eq!(
            upgrade("sophisticatedbackpacks:tank_upgrade").unwrap().kind,
            Kind::Inert
        );
        assert!(upgrade("sophisticatedbackpacks:upgrade_base").is_none());
        assert!(upgrade("minecraft:stick").is_none());
        assert_eq!(
            group(
                "sophisticatedbackpacks:stack_downgrade_tier_1",
                Family::Backpack
            ),
            Some(("stack_upgrades", 3))
        );
        assert_eq!(
            upgrade("sophisticatedstorage:stack_upgrade_tier_5")
                .unwrap()
                .kind,
            Kind::Stack(32.0)
        );
    }
}
