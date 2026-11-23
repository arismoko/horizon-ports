//! A storage item's contents, kept in its stack (StorageWrapper's `contents`, which the jar keeps in the world's
//! `sophisticatedbackpacks.dat` by `contentsUuid`; here they ride in the stack, so they go wherever the item goes).
//!
//! `minecraft:custom_data` holds the jar's keys:
//! - `contentsUuid` (int array), `inventorySlots`, `upgradeSlots`, `clothColor`, `borderColor`;
//! - `inventory` and `upgradeInventory`: `{Items: [{Slot, id, count, components}]}`, counts past 99 allowed (stack
//!   upgrades). An entry in the 1.20 form (`Count`/`realCount`, `tag`) is read too, so the jar's own contents can be
//!   copied in as they are (notes/ports/sophisticatedbackpacks.md, the live world).
use mod_api::serde_json::Value;
use mod_api::{Compound, Host, ItemStack, Tag};

use crate::storage_items::{Family, StorageItem};

pub const INVENTORY: &str = "inventory";
pub const UPGRADES: &str = "upgradeInventory";

/// Reads of a compound by `&str` key (the key type, horizon_core's JavaString, isn't exported).
pub trait Nbt {
    fn at(&self, k: &str) -> Option<&Tag>;
    fn has(&self, k: &str) -> bool;
    fn del(&mut self, k: &str) -> Option<Tag>;
}

impl Nbt for Compound {
    fn at(&self, k: &str) -> Option<&Tag> {
        self.get(&k.into())
    }
    fn has(&self, k: &str) -> bool {
        self.contains_key(&k.into())
    }
    fn del(&mut self, k: &str) -> Option<Tag> {
        self.remove(&k.into())
    }
}

/// The stack's `minecraft:custom_data`, empty when it has none.
pub fn custom_data(stack: &ItemStack) -> Compound {
    match stack
        .components()
        .and_then(|c| c.as_nbt().at("minecraft:custom_data"))
    {
        Some(Tag::Compound(c)) => c.clone(),
        _ => Compound::new(),
    }
}

pub fn set_custom_data(stack: &mut ItemStack, data: Compound) {
    let mut patch = Compound::new();
    if data.is_empty() {
        patch.insert(
            "!minecraft:custom_data".into(),
            Tag::Compound(Compound::new()),
        );
    } else {
        patch.insert("minecraft:custom_data".into(), Tag::Compound(data));
    }
    let _ = stack.apply_components(&patch);
}

pub fn get_int(c: &Compound, k: &str) -> Option<i32> {
    match c.at(k)? {
        Tag::Byte(v) => Some(i32::from(*v)),
        Tag::Short(v) => Some(i32::from(*v)),
        Tag::Int(v) => Some(*v),
        Tag::Long(v) => Some(*v as i32),
        _ => None,
    }
}

pub fn get_bool(c: &Compound, k: &str) -> Option<bool> {
    get_int(c, k).map(|v| v != 0)
}

pub fn get_str(c: &Compound, k: &str) -> Option<String> {
    match c.at(k)? {
        Tag::String(s) => Some(s.to_string_lossy()),
        _ => None,
    }
}

pub fn put(c: &mut Compound, k: &str, v: Tag) {
    c.insert(k.into(), v);
}

/// The contents' identity (`contentsUuid`).
pub fn uuid(stack: &ItemStack) -> Option<[i32; 4]> {
    match custom_data(stack).at("contentsUuid")? {
        Tag::IntArray(v) if v.len() == 4 => Some([v[0], v[1], v[2], v[3]]),
        _ => None,
    }
}

/// A new `contentsUuid` (version-4 shaped, from the clock and a counter: no random source here).
pub fn new_uuid() -> [i32; 4] {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut x = t ^ N.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed);
    let mut next = || {
        // splitmix64
        x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = x;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    let (a, b) = (next(), next());
    let a = (a & 0xFFFF_FFFF_FFFF_0FFF) | 0x4000;
    let b = (b & 0x3FFF_FFFF_FFFF_FFFF) | 0x8000_0000_0000_0000;
    [(a >> 32) as i32, a as i32, (b >> 32) as i32, b as i32]
}

/// Gives the stack a `contentsUuid` if it has none; its uuid.
pub fn ensure_uuid(stack: &mut ItemStack) -> [i32; 4] {
    if let Some(u) = uuid(stack) {
        return u;
    }
    let u = new_uuid();
    let mut data = custom_data(stack);
    put(&mut data, "contentsUuid", Tag::IntArray(u.to_vec()));
    set_custom_data(stack, data);
    u
}

/// A JSON value (1.20 text components in item NBT are JSON strings) as NBT.
fn json_nbt(v: &Value) -> Tag {
    match v {
        Value::Null => Tag::String("".into()),
        Value::Bool(b) => Tag::Byte(i8::from(*b)),
        Value::Number(n) => match n.as_i64() {
            Some(i) if i32::try_from(i).is_ok() => Tag::Int(i as i32),
            Some(i) => Tag::Long(i),
            None => Tag::Double(n.as_f64().unwrap_or(0.0).to_bits()),
        },
        Value::String(s) => Tag::String(s.as_str().into()),
        Value::Array(a) => Tag::List(a.iter().map(json_nbt).collect()),
        Value::Object(o) => Tag::Compound(
            o.iter()
                .map(|(k, v)| (k.as_str().into(), json_nbt(v)))
                .collect(),
        ),
    }
}

/// A 1.20 item entry (`id`, `Count` or `realCount`, `tag`) in 26.3's form: damage, name and enchantments become
/// their components, the rest of the tag becomes `custom_data`.
fn legacy_entry(e: &Compound) -> Compound {
    let mut out = Compound::new();
    if let Some(id) = e.at("id") {
        out.insert("id".into(), id.clone());
    }
    let count = get_int(e, "realCount")
        .or_else(|| get_int(e, "Count"))
        .unwrap_or(1)
        .max(1);
    put(&mut out, "count", Tag::Int(count));
    let mut comps = Compound::new();
    if let Some(Tag::Compound(tag)) = e.at("tag") {
        let mut rest = tag.clone();
        if let Some(d) = get_int(tag, "Damage") {
            rest.del("Damage");
            if d > 0 {
                put(&mut comps, "minecraft:damage", Tag::Int(d));
            }
        }
        if let Some(Tag::Compound(display)) = tag.at("display") {
            let mut display = display.clone();
            if let Some(name) = get_str(&display, "Name") {
                display.del("Name");
                if let Ok(v) = mod_api::serde_json::from_str::<Value>(&name) {
                    put(&mut comps, "minecraft:custom_name", json_nbt(&v));
                }
            }
            rest.del("display");
            if !display.is_empty() {
                put(&mut rest, "display", Tag::Compound(display));
            }
        }
        for (k, comp) in [
            ("Enchantments", "minecraft:enchantments"),
            ("StoredEnchantments", "minecraft:stored_enchantments"),
        ] {
            if let Some(Tag::List(list)) = tag.at(k) {
                let mut levels = Compound::new();
                for t in list {
                    if let Tag::Compound(en) = t
                        && let (Some(id), Some(lvl)) = (get_str(en, "id"), get_int(en, "lvl"))
                    {
                        put(&mut levels, &id, Tag::Int(lvl));
                    }
                }
                rest.del(k);
                if !levels.is_empty() {
                    put(&mut comps, comp, Tag::Compound(levels));
                }
            }
        }
        rest.del("RepairCost");
        if !rest.is_empty() {
            put(&mut comps, "minecraft:custom_data", Tag::Compound(rest));
        }
    }
    if !comps.is_empty() {
        put(&mut out, "components", Tag::Compound(comps));
    }
    out
}

/// The custom_data key of a stand-in for an entry this game can't make a stack of (an item of a mod that isn't
/// installed): the entry itself, written back unchanged.
pub const UNKNOWN: &str = "sophisticatedcore:unknown";

/// The raw entry (without `Slot`) a stand-in carries; None for any other stack.
pub fn unknown_entry(stack: &ItemStack) -> Option<Compound> {
    match custom_data(stack).at(UNKNOWN)? {
        Tag::Compound(c) => Some(c.clone()),
        _ => None,
    }
}

/// Whether `stack` is a stand-in for an unknown item: it stays in its slot, is never moved, taken, sorted, dropped
/// or used, and is written back as the entry it came from.
pub fn is_unknown(stack: &ItemStack) -> bool {
    unknown_entry(stack).is_some()
}

/// A barrier standing in for `raw`, an entry whose item this game doesn't know, named after it ("Not ported:
/// hexalia:salt"). None for an entry with no item (no id, air).
pub fn stand_in(host: &dyn Host, raw: &Compound) -> Option<ItemStack> {
    let id =
        get_str(raw, "id").filter(|id| !id.is_empty() && id != "minecraft:air" && id != "air")?;
    let count = get_int(raw, "realCount")
        .or_else(|| get_int(raw, "count"))
        .or_else(|| get_int(raw, "Count"))
        .unwrap_or(1)
        .max(1);
    let key = host.item_key(&mod_api::Identifier::parse("minecraft:barrier").ok()?)?;
    let mut st = ItemStack::new(key, count.clamp(1, 64) as u32)?;
    let line = |t: &str, color: &str| {
        Tag::compound([
            ("text", Tag::String(t.into())),
            ("color", Tag::String(color.into())),
            ("italic", Tag::Byte(0)),
        ])
    };
    let mut patch = Compound::new();
    patch.insert(
        "minecraft:custom_name".into(),
        line(&format!("Not ported: {id}"), "red"),
    );
    patch.insert(
        "minecraft:lore".into(),
        Tag::List(vec![
            line(&format!("{count} kept here as saved"), "gray"),
            line("Comes back when its mod is installed", "gray"),
        ]),
    );
    let mut raw = raw.clone();
    raw.del("Slot");
    patch.insert(
        "minecraft:custom_data".into(),
        Tag::compound([(UNKNOWN, Tag::Compound(raw))]),
    );
    st.apply_components(&patch).ok()?;
    Some(st)
}

/// One `Items` entry: its slot and stack. An entry whose item can't be made (unknown id, bad components) comes back
/// as a stand-in (`stand_in`), so its slot stays taken and the entry is written back unchanged.
pub fn decode_entry(host: &dyn Host, e: &Compound) -> Option<(usize, ItemStack)> {
    let slot = usize::try_from(get_int(e, "Slot")?).ok()?;
    let legacy = e.has("Count") || e.has("tag") || e.has("realCount");
    let mut nbt = if legacy { legacy_entry(e) } else { e.clone() };
    nbt.del("Slot");
    match ItemStack::from_nbt(host, &nbt) {
        Some(stack) => Some((slot, stack)),
        None => Some((slot, stand_in(host, e)?)),
    }
}

pub fn encode_entry(host: &dyn Host, slot: usize, stack: &ItemStack) -> Option<Compound> {
    let mut nbt = match unknown_entry(stack) {
        Some(raw) => raw,
        None => stack.to_nbt(host)?,
    };
    put(&mut nbt, "Slot", Tag::Int(slot as i32));
    Some(nbt)
}

/// The stacks of an item handler compound (`{Items: [..]}`), `size` long.
pub fn read_items(host: &dyn Host, c: Option<&Tag>, size: usize) -> Vec<Option<ItemStack>> {
    read_items_kept(host, c, size).0
}

/// `read_items`, plus the entries it can't place (a slot past `size`): they are kept and written back as they are.
pub fn read_items_kept(
    host: &dyn Host,
    c: Option<&Tag>,
    size: usize,
) -> (Vec<Option<ItemStack>>, Vec<Compound>) {
    let mut out = vec![None; size];
    let mut kept = Vec::new();
    if let Some(Tag::Compound(c)) = c
        && let Some(Tag::List(items)) = c.at("Items")
    {
        for t in items {
            let Tag::Compound(e) = t else { continue };
            match decode_entry(host, e) {
                Some((slot, stack)) if slot < size && out[slot].is_none() => {
                    out[slot] = Some(stack)
                }
                Some(_) => kept.push(e.clone()),
                None => {}
            }
        }
    }
    (out, kept)
}

pub fn write_items(host: &dyn Host, stacks: &[Option<ItemStack>]) -> Tag {
    write_items_kept(host, stacks, &[])
}

pub fn write_items_kept(host: &dyn Host, stacks: &[Option<ItemStack>], kept: &[Compound]) -> Tag {
    let mut items: Vec<Tag> = stacks
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            s.as_ref()
                .and_then(|s| encode_entry(host, i, s))
                .map(Tag::Compound)
        })
        .collect();
    items.extend(kept.iter().cloned().map(Tag::Compound));
    Tag::compound([("Items", Tag::List(items))])
}

/// What a storage stack holds.
#[derive(Clone, Debug, Default)]
pub struct Contents {
    pub slots: Vec<Option<ItemStack>>,
    pub upgrades: Vec<Option<ItemStack>>,
    /// Saved entries past the slot count (`read_items_kept`), written back unchanged.
    pub kept: Vec<Compound>,
    pub kept_upgrades: Vec<Compound>,
    /// The storage is locked (`locked`): see `tool::locked`.
    pub locked: bool,
}

/// `doubleChest` (ChestBlockItem.isDoubleChest's key): a chest whose stack holds a double chest's contents.
pub const DOUBLE: &str = "doubleChest";

/// How many slots the storage in `stack` has: its tier's, twice that for the main half of a double chest.
pub fn slots_of(stack: &ItemStack, kind: &StorageItem) -> usize {
    if kind.family == Family::Chest && get_bool(&custom_data(stack), DOUBLE) == Some(true) {
        kind.slot_count() * 2
    } else {
        kind.slot_count()
    }
}

/// A vanilla `minecraft:container` component (a vanilla shulker box crafted into a Sophisticated one), as slots.
fn container_component(
    host: &dyn Host,
    stack: &ItemStack,
    size: usize,
) -> Option<Vec<Option<ItemStack>>> {
    let comps = stack.components()?;
    let Some(Tag::List(list)) = comps.as_nbt().at("minecraft:container") else {
        return None;
    };
    let mut out = vec![None; size];
    for t in list {
        if let Tag::Compound(e) = t
            && let (Some(slot), Some(Tag::Compound(item))) = (get_int(e, "slot"), e.at("item"))
            && let Some(st) = ItemStack::from_nbt(host, item).or_else(|| stand_in(host, item))
            && (slot as usize) < size
        {
            out[slot as usize] = Some(st);
        }
    }
    Some(out)
}

impl Contents {
    pub fn read(host: &dyn Host, stack: &ItemStack, kind: &StorageItem) -> Contents {
        let data = custom_data(stack);
        let size = slots_of(stack, kind);
        let (slots, kept) = match data.at(INVENTORY) {
            None => (
                container_component(host, stack, size).unwrap_or_else(|| vec![None; size]),
                Vec::new(),
            ),
            inv => read_items_kept(host, inv, size),
        };
        let (upgrades, kept_upgrades) =
            read_items_kept(host, data.at(UPGRADES), kind.upgrade_count());
        Contents {
            slots,
            upgrades,
            kept,
            kept_upgrades,
            locked: get_bool(&data, "locked") == Some(true),
        }
    }

    /// Writes the contents into `stack`'s custom_data, its other keys kept.
    pub fn write(&self, host: &dyn Host, stack: &mut ItemStack, kind: &StorageItem) {
        let mut data = custom_data(stack);
        put(
            &mut data,
            INVENTORY,
            write_items_kept(host, &self.slots, &self.kept),
        );
        put(
            &mut data,
            UPGRADES,
            write_items_kept(host, &self.upgrades, &self.kept_upgrades),
        );
        put(
            &mut data,
            "inventorySlots",
            Tag::Int(self.slots.len() as i32),
        );
        put(
            &mut data,
            "upgradeSlots",
            Tag::Int(kind.upgrade_count() as i32),
        );
        if !data.has("contentsUuid") {
            put(
                &mut data,
                "contentsUuid",
                Tag::IntArray(new_uuid().to_vec()),
            );
        }
        set_custom_data(stack, data);
        if stack
            .components()
            .is_some_and(|c| c.as_nbt().has("minecraft:container"))
        {
            let mut patch = Compound::new();
            patch.insert(
                "!minecraft:container".into(),
                Tag::Compound(Compound::new()),
            );
            let _ = stack.apply_components(&patch);
        }
    }

    /// The contents without the items (`inventory` and `upgradeInventory` removed from the stack).
    pub fn strip(stack: &mut ItemStack) {
        let mut data = custom_data(stack);
        for k in [
            INVENTORY,
            UPGRADES,
            "inventorySlots",
            "upgradeSlots",
            "contentsUuid",
            DOUBLE,
            "packed",
        ] {
            data.del(k);
        }
        set_custom_data(stack, data);
    }

    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
            && self.upgrades.iter().all(Option::is_none)
            && self.kept.is_empty()
            && self.kept_upgrades.is_empty()
    }

    /// Only what can't leave the storage: the stand-ins of unknown items (in their slots) and the kept entries.
    /// None when there is nothing of the kind.
    pub fn unknown_only(&self) -> Option<Contents> {
        let only = |v: &[Option<ItemStack>]| {
            v.iter()
                .map(|s| s.clone().filter(is_unknown))
                .collect::<Vec<_>>()
        };
        let c = Contents {
            slots: only(&self.slots),
            upgrades: only(&self.upgrades),
            kept: self.kept.clone(),
            kept_upgrades: self.kept_upgrades.clone(),
            locked: false,
        };
        (!c.is_empty()).then_some(c)
    }

    /// How many stand-ins and kept entries the storage holds.
    pub fn unknown_count(&self) -> usize {
        self.slots
            .iter()
            .chain(self.upgrades.iter())
            .flatten()
            .filter(|s| is_unknown(s))
            .count()
            + self.kept.len()
            + self.kept_upgrades.len()
    }
}

/// The stack's item id.
pub fn item_id(host: &dyn Host, stack: &ItemStack) -> String {
    stack.item().id(host).unwrap_or_default().to_owned()
}

/// `stack` with `count` (a count of 0 is nothing).
pub fn with_count(stack: &ItemStack, count: u32) -> Option<ItemStack> {
    if count == 0 {
        return None;
    }
    let mut s = stack.clone();
    s.set_count(count).ok()?;
    Some(s)
}

/// Whether two stacks are the same item with the same components (counts aside).
pub fn same(a: &ItemStack, b: &ItemStack) -> bool {
    a.is_stackable_with(b) || (a.same_item(b) && a.same_components(b))
}

/// Puts up to `amount` of `stack` into `slots`, onto the same item first and then into empty slots, no slot past
/// its limit; how many went in.
pub fn insert(slots: &mut [Option<ItemStack>], stack: &ItemStack, amount: u32, limit: u32) -> u32 {
    insert_with(slots, stack, amount, limit, true)
}

/// `insert`, with `fill_empty` false only onto stacks already there.
pub fn insert_with(
    slots: &mut [Option<ItemStack>],
    stack: &ItemStack,
    amount: u32,
    limit: u32,
    fill_empty: bool,
) -> u32 {
    let mut left = amount;
    for s in slots.iter_mut().flatten() {
        if left == 0 {
            break;
        }
        if same(s, stack) && s.count() < limit {
            let put = (limit - s.count()).min(left);
            let _ = s.set_count(s.count() + put);
            left -= put;
        }
    }
    for s in slots.iter_mut() {
        if left == 0 || !fill_empty {
            break;
        }
        if s.is_none() {
            let put = limit.min(left);
            *s = with_count(stack, put);
            left -= put;
        }
    }
    amount - left
}

/// Takes up to `amount` of `stack`'s item (same components) out of `slots`, last slots first; how many came out.
pub fn extract(slots: &mut [Option<ItemStack>], stack: &ItemStack, amount: u32) -> u32 {
    let mut left = amount;
    for s in slots.iter_mut().rev() {
        if left == 0 {
            break;
        }
        if let Some(held) = s.as_ref()
            && same(held, stack)
        {
            let take = held.count().min(left);
            *s = with_count(held, held.count() - take);
            left -= take;
        }
    }
    amount - left
}

/// How many of `stack`'s item (same components) `slots` hold.
pub fn count_of(slots: &[Option<ItemStack>], stack: &ItemStack) -> u64 {
    slots
        .iter()
        .flatten()
        .filter(|s| same(s, stack))
        .map(|s| u64::from(s.count()))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuids_differ() {
        let a = new_uuid();
        let b = new_uuid();
        assert_ne!(a, b);
        assert_eq!((a[1] >> 12) & 0xF, 4);
    }
}
