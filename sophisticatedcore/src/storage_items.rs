//! The storage items the core serves:
//! - Sophisticated Backpacks' six tiers (BackpackItem, with Homestead's sophisticatedbackpacks-server.toml slot counts,
//!   which are the jar's defaults);
//! - Sophisticated Storage's chests, barrels, limited barrels and shulker boxes (Homestead's
//!   sophisticatedstorage-server.toml: 27/45/54/81/108/132 slots and 1/1/2/3/4/5 upgrade slots from basic to
//!   netherite; the limited barrels' 1 to 4 slots with their base stack multipliers).

/// What kind of storage an item is: decides placing, breaking and the upgrade limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Backpack,
    Chest,
    Barrel,
    LimitedBarrel,
    ShulkerBox,
}

/// One storage item: its id, the block it places (if any), its inventory and upgrade slots, its name and family,
/// and the base stack multiplier of a slot (LimitedBarrelBlock.getBaseStackSizeMultiplier; 1 for the others).
#[derive(Clone, Copy, Debug)]
pub struct StorageItem {
    pub id: &'static str,
    pub block: Option<&'static str>,
    /// The jar's default slot count (a limited barrel's slots are fixed); the settings' values are read through
    /// `slot_count` and `upgrade_count`.
    pub slots: usize,
    pub name: &'static str,
    pub family: Family,
    base: u32,
    /// The tier: 0 basic (leather for backpacks) to 5 netherite.
    pub tier: u8,
}

const fn bp(id: &'static str, slots: usize, name: &'static str, tier: u8) -> StorageItem {
    StorageItem { id, block: Some(id), slots, name, family: Family::Backpack, base: 1, tier }
}

pub const BACKPACKS: [StorageItem; 6] = [
    bp("sophisticatedbackpacks:backpack", 27, "Backpack", 0),
    bp("sophisticatedbackpacks:copper_backpack", 45, "Copper Backpack", 1),
    bp("sophisticatedbackpacks:iron_backpack", 54, "Iron Backpack", 2),
    bp("sophisticatedbackpacks:gold_backpack", 81, "Gold Backpack", 3),
    bp("sophisticatedbackpacks:diamond_backpack", 108, "Diamond Backpack", 4),
    bp("sophisticatedbackpacks:netherite_backpack", 120, "Netherite Backpack", 5),
];

/// The tiers' default slots (Homestead's sophisticatedstorage-server.toml).
const SLOTS: [usize; 6] = [27, 45, 54, 81, 108, 132];

const fn st(id: &'static str, family: Family, tier: u8, name: &'static str) -> StorageItem {
    StorageItem { id, block: Some(id), slots: SLOTS[tier as usize], name, family, base: 1, tier }
}

const fn lim(id: &'static str, tier: u8, n: usize, base: u32, name: &'static str) -> StorageItem {
    StorageItem { id, block: Some(id), slots: n, name, family: Family::LimitedBarrel, base, tier }
}

use Family::{Barrel, Chest, ShulkerBox};

pub const STORAGE: [StorageItem; 42] = [
    st("sophisticatedstorage:chest", Chest, 0, "Chest"),
    st("sophisticatedstorage:copper_chest", Chest, 1, "Copper Chest"),
    st("sophisticatedstorage:iron_chest", Chest, 2, "Iron Chest"),
    st("sophisticatedstorage:gold_chest", Chest, 3, "Gold Chest"),
    st("sophisticatedstorage:diamond_chest", Chest, 4, "Diamond Chest"),
    st("sophisticatedstorage:netherite_chest", Chest, 5, "Netherite Chest"),
    st("sophisticatedstorage:barrel", Barrel, 0, "Barrel"),
    st("sophisticatedstorage:copper_barrel", Barrel, 1, "Copper Barrel"),
    st("sophisticatedstorage:iron_barrel", Barrel, 2, "Iron Barrel"),
    st("sophisticatedstorage:gold_barrel", Barrel, 3, "Gold Barrel"),
    st("sophisticatedstorage:diamond_barrel", Barrel, 4, "Diamond Barrel"),
    st("sophisticatedstorage:netherite_barrel", Barrel, 5, "Netherite Barrel"),
    st("sophisticatedstorage:shulker_box", ShulkerBox, 0, "Shulker Box"),
    st("sophisticatedstorage:copper_shulker_box", ShulkerBox, 1, "Copper Shulker Box"),
    st("sophisticatedstorage:iron_shulker_box", ShulkerBox, 2, "Iron Shulker Box"),
    st("sophisticatedstorage:gold_shulker_box", ShulkerBox, 3, "Gold Shulker Box"),
    st("sophisticatedstorage:diamond_shulker_box", ShulkerBox, 4, "Diamond Shulker Box"),
    st("sophisticatedstorage:netherite_shulker_box", ShulkerBox, 5, "Netherite Shulker Box"),
    lim("sophisticatedstorage:limited_barrel_1", 0, 1, 32, "Limited Barrel I"),
    lim("sophisticatedstorage:limited_copper_barrel_1", 1, 1, 53, "Limited Copper Barrel I"),
    lim("sophisticatedstorage:limited_iron_barrel_1", 2, 1, 64, "Limited Iron Barrel I"),
    lim("sophisticatedstorage:limited_gold_barrel_1", 3, 1, 96, "Limited Gold Barrel I"),
    lim("sophisticatedstorage:limited_diamond_barrel_1", 4, 1, 128, "Limited Diamond Barrel I"),
    lim("sophisticatedstorage:limited_netherite_barrel_1", 5, 1, 160, "Limited Netherite Barrel I"),
    lim("sophisticatedstorage:limited_barrel_2", 0, 2, 16, "Limited Barrel II"),
    lim("sophisticatedstorage:limited_copper_barrel_2", 1, 2, 27, "Limited Copper Barrel II"),
    lim("sophisticatedstorage:limited_iron_barrel_2", 2, 2, 32, "Limited Iron Barrel II"),
    lim("sophisticatedstorage:limited_gold_barrel_2", 3, 2, 48, "Limited Gold Barrel II"),
    lim("sophisticatedstorage:limited_diamond_barrel_2", 4, 2, 64, "Limited Diamond Barrel II"),
    lim("sophisticatedstorage:limited_netherite_barrel_2", 5, 2, 80, "Limited Netherite Barrel II"),
    lim("sophisticatedstorage:limited_barrel_3", 0, 3, 10, "Limited Barrel III"),
    lim("sophisticatedstorage:limited_copper_barrel_3", 1, 3, 17, "Limited Copper Barrel III"),
    lim("sophisticatedstorage:limited_iron_barrel_3", 2, 3, 20, "Limited Iron Barrel III"),
    lim("sophisticatedstorage:limited_gold_barrel_3", 3, 3, 30, "Limited Gold Barrel III"),
    lim("sophisticatedstorage:limited_diamond_barrel_3", 4, 3, 40, "Limited Diamond Barrel III"),
    lim("sophisticatedstorage:limited_netherite_barrel_3", 5, 3, 50, "Limited Netherite Barrel III"),
    lim("sophisticatedstorage:limited_barrel_4", 0, 4, 8, "Limited Barrel IV"),
    lim("sophisticatedstorage:limited_copper_barrel_4", 1, 4, 13, "Limited Copper Barrel IV"),
    lim("sophisticatedstorage:limited_iron_barrel_4", 2, 4, 16, "Limited Iron Barrel IV"),
    lim("sophisticatedstorage:limited_gold_barrel_4", 3, 4, 24, "Limited Gold Barrel IV"),
    lim("sophisticatedstorage:limited_diamond_barrel_4", 4, 4, 32, "Limited Diamond Barrel IV"),
    lim("sophisticatedstorage:limited_netherite_barrel_4", 5, 4, 40, "Limited Netherite Barrel IV"),
];

impl StorageItem {
    /// The inventory slots (`inventorySlotCount`).
    pub fn slot_count(&self) -> usize {
        let t = crate::options::tune();
        match self.family {
            Family::Backpack => t.backpack_slots[self.tier as usize],
            Family::LimitedBarrel => self.slots,
            _ => t.storage_slots[self.tier as usize],
        }
    }

    /// The upgrade slots (`upgradeSlotCount`).
    pub fn upgrade_count(&self) -> usize {
        let t = crate::options::tune();
        match self.family {
            Family::Backpack => t.backpack_upgrades[self.tier as usize],
            Family::LimitedBarrel => t.limited_upgrades[self.tier as usize],
            _ => t.storage_upgrades[self.tier as usize],
        }
    }

    /// A slot's base multiplier (`baseSlotLimitMultiplier`; 1 outside the limited barrels).
    pub fn base_multiplier(&self) -> u32 {
        if self.family == Family::LimitedBarrel {
            crate::options::tune().limited_base[self.slots - 1][self.tier as usize]
        } else {
            1
        }
    }

    /// The jar's default base multiplier.
    pub fn default_base(&self) -> u32 {
        self.base
    }
}

/// Every storage item the core knows.
pub fn all() -> impl Iterator<Item = &'static StorageItem> {
    BACKPACKS.iter().chain(STORAGE.iter())
}

/// The storage item with this id.
pub fn by_item(id: &str) -> Option<&'static StorageItem> {
    all().find(|s| s.id == id)
}

/// The storage item whose block this is.
pub fn by_block(id: &str) -> Option<&'static StorageItem> {
    all().find(|s| s.block == Some(id))
}

/// The same family's storage of tier `tier` (a limited barrel keeps its slot count).
pub fn with_tier(s: &StorageItem, tier: u8) -> Option<&'static StorageItem> {
    STORAGE.iter().find(|o| o.family == s.family && o.tier == tier && (s.family != Family::LimitedBarrel || o.slots == s.slots))
}

/// The namespaces whose content must be loaded for the table's entries to be registered.
pub fn namespace(s: &StorageItem) -> &'static str {
    s.id.split(':').next().unwrap_or("")
}

/// Sophisticated Storage's wood types (WoodType.values() in 1.20, the jar's `woodType` values).
pub const WOODS: [&str; 11] = ["oak", "spruce", "birch", "acacia", "jungle", "dark_oak", "crimson", "mangrove", "warped", "cherry", "bamboo"];

pub fn wood_name(w: &str) -> &'static str {
    match w {
        "oak" => "Oak",
        "spruce" => "Spruce",
        "birch" => "Birch",
        "acacia" => "Acacia",
        "jungle" => "Jungle",
        "dark_oak" => "Dark Oak",
        "crimson" => "Crimson",
        "mangrove" => "Mangrove",
        "warped" => "Warped",
        "cherry" => "Cherry",
        "bamboo" => "Bamboo",
        _ => "",
    }
}

/// Whether the family keeps a wood type (chests and barrels).
pub fn wooden(s: &StorageItem) -> bool {
    matches!(s.family, Family::Chest | Family::Barrel | Family::LimitedBarrel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers() {
        let c = by_item("sophisticatedstorage:chest").unwrap();
        assert_eq!(with_tier(c, 3).unwrap().id, "sophisticatedstorage:gold_chest");
        let l = by_item("sophisticatedstorage:limited_barrel_3").unwrap();
        assert_eq!((l.slots, l.base), (3, 10));
        assert_eq!(with_tier(l, 5).unwrap().id, "sophisticatedstorage:limited_netherite_barrel_3");
        assert_eq!(by_block("sophisticatedstorage:netherite_shulker_box").unwrap().slots, 132);
    }
}
