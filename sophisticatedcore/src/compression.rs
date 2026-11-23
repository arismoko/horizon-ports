//! The compression upgrade (CompressionUpgradeItem, CompressionInventoryPart).
//!
//! The upgrade takes over a range of the storage's slots (`firstInventorySlot`, how many in `slotCount`; at most
//! `compression_upgrade_max_number_of_slots`). Those slots hold one material in every form it compresses to: the
//! last used slot holds the base item, each slot before it what 4 or 9 of the next one craft into, each slot after
//! it what one of the base item crafts into (`additional_decompressible_items.txt` adds the shapes no recipe
//! reverses). A slot shows, and hands out, everything held in it and in the slots before it, counted in its own
//! item; putting in or taking out carries between the slots.
//!
//! The slots keep the real stacks (`inventory`), so nothing here is stored twice: the chain is worked out from what
//! the range holds each time, and the range is plain slots again once the upgrade is gone. The core below is
//! generic over the item type, so its rules are tested without a game.
use std::collections::VecDeque;

use mod_api::{Host, Identifier, ItemStack, LevelRef, RecipeInput};

use crate::contents::{self, Contents, Nbt, custom_data, get_int, put, set_custom_data};
use crate::storage_items::StorageItem;
use crate::upgrades::{self, Kind};

/// `firstInventorySlot` of the upgrade's own tag, and how many slots the range has.
pub const FIRST: &str = "firstInventorySlot";
pub const COUNT: &str = "slotCount";

/// The int limit the original counts in.
const CAP: u64 = i32::MAX as u64;

/// How many follow-up compactions are tried before a loop is assumed (`MAX_FOLLOW_UP_COMPACTING_RECIPES`).
const MAX_FOLLOW_UP: usize = 30;

/// A slot of the range: the item and how many of it.
pub type Slot<T> = Option<(T, u32)>;

/// What the recipes say about items; `T` is an item with its components.
pub trait Recipes<T> {
    /// What a `size` by `size` grid of `item` crafts into.
    fn craft(&self, item: &T, size: u32) -> Option<T>;
    /// What one `item` crafts into, and how many.
    fn uncraft(&self, item: &T) -> Option<(T, u32)>;
    /// `additionalDecompressibleItems`: what a compressed `item` breaks into, and how many.
    fn configured(&self, item: &T) -> Option<(T, u32)>;
    /// Whether the item has no components (the configured shapes only apply to those).
    fn plain(&self, item: &T) -> bool;
}

/// `item` crafted `size` by `size`, when that is another item that doesn't lead back to `item`; and whether the
/// result crafts back into `size * size` of `item`.
fn compaction<T: Clone + PartialEq>(r: &dyn Recipes<T>, item: &T, size: u32) -> Option<(T, bool)> {
    let result = r.craft(item, size)?;
    if result == *item || in_loop(r, item, &result) {
        return None;
    }
    let back = r
        .uncraft(&result)
        .is_some_and(|(u, n)| u == *item && n == size * size);
    Some((result, back))
}

/// `isPartOfCompactingLoop`: whether compacting `first_result` further ends up at `first` again.
fn in_loop<T: Clone + PartialEq>(r: &dyn Recipes<T>, first: &T, first_result: &T) -> bool {
    let mut queue = VecDeque::from([first_result.clone()]);
    let mut seen: Vec<T> = Vec::new();
    let mut rounds = 0;
    while let Some(item) = queue.pop_front() {
        for size in [2, 3] {
            if let Some(next) = r.craft(&item, size) {
                if next == *first {
                    return true;
                }
                if seen.contains(&next) {
                    return false;
                }
                queue.push_back(next);
            }
        }
        seen.push(item);
        rounds += 1;
        if rounds > MAX_FOLLOW_UP {
            return true;
        }
    }
    false
}

/// What `item` compresses into, and how many it takes (`getCompressionShape`): a recipe that reverses, 3x3 before
/// 2x2, then one the config knows how to reverse.
pub fn shape<T: Clone + PartialEq>(r: &dyn Recipes<T>, item: &T) -> Option<(T, u32)> {
    let s3 = compaction(r, item, 3);
    let s2 = compaction(r, item, 2);
    if let Some((x, true)) = &s3 {
        return Some((x.clone(), 9));
    }
    if let Some((x, true)) = &s2 {
        return Some((x.clone(), 4));
    }
    if let Some((x, false)) = &s3
        && r.configured(x).is_some()
    {
        return Some((x.clone(), 9));
    }
    if let Some((x, false)) = &s2
        && r.configured(x).is_some()
    {
        return Some((x.clone(), 4));
    }
    None
}

/// What one `item` breaks into and how many (`addPreviousItems`): a recipe whose result compacts back, or the config.
fn previous<T: Clone + PartialEq>(r: &dyn Recipes<T>, item: &T) -> Option<(T, u32)> {
    if let Some((u, n)) = r.uncraft(item) {
        let size = match n {
            9 => 3,
            4 => 2,
            _ => 0,
        };
        if size > 0 && r.craft(&u, size).is_some_and(|c| c == *item) {
            return Some((u, n));
        }
    }
    if r.plain(item) {
        return r.configured(item);
    }
    None
}

/// One slot of the range in a chain.
#[derive(Clone, Debug, PartialEq)]
pub struct Def<T> {
    pub item: T,
    /// How many of this slot's item make one of the slot before's.
    pub mult: u32,
    /// Most of this item the slot and the slots before it hold together.
    pub limit: u64,
    /// Whether the slot trades with a neighbour.
    pub compressible: bool,
}

/// The range's slots: `None` is a slot nothing can go in.
#[derive(Clone, Debug, PartialEq)]
pub struct Chain<T> {
    pub defs: Vec<Option<Def<T>>>,
}

fn capped(v: u128) -> u64 {
    v.min(u128::from(CAP)) as u64
}

impl<T: Clone + PartialEq> Chain<T> {
    /// The chain the range's `slots` make, or, when it is empty, the one an `item` put in slot `seed` would start.
    /// `base` is how many of an item one slot holds.
    pub fn derive(
        slots: &[Slot<T>],
        seed: Option<(usize, &T)>,
        r: &dyn Recipes<T>,
        base: &dyn Fn(&T) -> u32,
    ) -> Chain<T> {
        let n = slots.len();
        let mut defs: Vec<Option<Def<T>>> = vec![None; n];
        let last = slots.iter().rposition(Option::is_some);
        let start = match (last, seed) {
            (Some(i), _) => slots[i].as_ref().map(|(t, _)| (i, t.clone())),
            (None, Some((i, t))) => Some((i, t.clone())),
            (None, None) => None,
        };
        let Some((last, first)) = start else {
            return Chain { defs };
        };
        let mut cur = first.clone();
        for slot in last + 1..n {
            let Some((item, m)) = previous(r, &cur) else {
                break;
            };
            defs[slot] = Some(Def {
                item: item.clone(),
                mult: m,
                limit: 0,
                compressible: false,
            });
            cur = item;
        }
        let mut prev = first;
        for slot in (0..=last).rev() {
            if slots[slot].as_ref().is_some_and(|(t, _)| *t != prev) {
                // Something that doesn't belong: no compression at all.
                defs.iter_mut().for_each(|d| *d = None);
                break;
            }
            let (mult, next) = match shape(r, &prev) {
                Some((next, m)) => (m, Some(next)),
                None => (1, None),
            };
            defs[slot] = Some(Def {
                item: prev.clone(),
                mult,
                limit: 0,
                compressible: false,
            });
            match next {
                Some(next) => prev = next,
                None => break,
            }
        }
        let mut total: u128 = 0;
        for d in defs.iter_mut().flatten() {
            total = u128::from(base(&d.item)) + u128::from(d.mult) * total;
            total = u128::from(capped(total));
            d.limit = total as u64;
        }
        for (slot, d) in defs.iter_mut().enumerate() {
            if d.is_none()
                && let Some((t, _)) = &slots[slot]
            {
                *d = Some(Def {
                    item: t.clone(),
                    mult: 1,
                    limit: u64::from(base(t)),
                    compressible: false,
                });
            }
        }
        let accessible: Vec<Option<u32>> =
            defs.iter().map(|d| d.as_ref().map(|d| d.mult)).collect();
        for (i, d) in defs.iter_mut().enumerate() {
            let Some(d) = d else { continue };
            let from_prev = i > 0 && accessible[i - 1].is_some() && d.mult > 1;
            let from_next = accessible
                .get(i + 1)
                .copied()
                .flatten()
                .is_some_and(|m| m > 1);
            d.compressible = from_prev || from_next;
        }
        Chain { defs }
    }

    /// What each slot shows: its own count plus everything compressed before it, in its own item.
    pub fn calculated(&self, slots: &[Slot<T>]) -> Vec<u64> {
        let mut out = vec![0; self.defs.len()];
        let mut total: u128 = 0;
        for (i, d) in self.defs.iter().enumerate() {
            let Some(d) = d else { continue };
            let held = slots[i].as_ref().map_or(0, |(_, c)| u128::from(*c));
            if d.compressible {
                total = total * u128::from(d.mult) + held;
                out[i] = capped(total);
            } else {
                out[i] = capped(held);
            }
        }
        out
    }

    /// The first slot of the unbroken run of slots that ends at `slot`.
    fn top(&self, slot: usize) -> usize {
        let mut j = slot;
        while j > 0 && self.defs[j - 1].is_some() {
            j -= 1;
        }
        j
    }

    /// Spreads `value` (counted in `slot`'s item) over the run's slots, as much into the earlier ones as they
    /// hold.
    fn spread(
        &self,
        slots: &mut [Slot<T>],
        slot: usize,
        mut value: u128,
        base: &dyn Fn(&T) -> u32,
    ) {
        let top = self.top(slot);
        let mut weight = vec![1u128; slot - top + 1];
        for k in (top..slot).rev() {
            let mult = self.defs[k + 1].as_ref().map_or(1, |d| d.mult);
            weight[k - top] = weight[k + 1 - top] * u128::from(mult);
        }
        for k in top..=slot {
            let Some(d) = &self.defs[k] else { continue };
            let w = weight[k - top];
            let put = (value / w).min(u128::from(base(&d.item)));
            value -= put * w;
            slots[k] = (put > 0).then(|| (d.item.clone(), put as u32));
        }
        if value > 0
            && let Some(d) = &self.defs[slot]
        {
            // More than the slots hold apiece (a stack upgrade taken out): keep it in the last one.
            let held = slots[slot].as_ref().map_or(0, |(_, c)| u128::from(*c));
            slots[slot] = Some((
                d.item.clone(),
                (held + value).min(u128::from(u32::MAX)) as u32,
            ));
        }
    }

    /// Puts up to `amount` of `item` into `slot`; how many went in.
    pub fn insert(
        &self,
        slots: &mut [Slot<T>],
        slot: usize,
        item: &T,
        amount: u32,
        base: &dyn Fn(&T) -> u32,
    ) -> u32 {
        let Some(d) = self.defs.get(slot).and_then(Option::as_ref) else {
            return 0;
        };
        if d.item != *item {
            return 0;
        }
        let shown = self.calculated(slots)[slot];
        let put = d.limit.saturating_sub(shown).min(u64::from(amount)) as u32;
        if put == 0 {
            return 0;
        }
        if d.compressible {
            self.spread(slots, slot, u128::from(shown) + u128::from(put), base);
        } else {
            let held = slots[slot].as_ref().map_or(0, |(_, c)| *c);
            slots[slot] = Some((d.item.clone(), held.saturating_add(put)));
        }
        put
    }

    /// Takes up to `amount` out of `slot`, breaking compressed items down where the slot's own run short; how many.
    pub fn take(
        &self,
        slots: &mut [Slot<T>],
        slot: usize,
        amount: u32,
        base: &dyn Fn(&T) -> u32,
    ) -> u32 {
        let Some(d) = self.defs.get(slot).and_then(Option::as_ref) else {
            return 0;
        };
        let shown = self.calculated(slots)[slot];
        let took = shown.min(u64::from(amount)) as u32;
        if took == 0 {
            return 0;
        }
        if d.compressible {
            self.spread(slots, slot, u128::from(shown) - u128::from(took), base);
        } else if let Some((t, c)) = slots[slot].take()
            && c > took
        {
            slots[slot] = Some((t, c - took));
        }
        took
    }
}

/// Whether a range holding `slots` may start compressing (`canUseForCompression`): from the last slot back, each
/// used slot holds what the one after it compresses into.
pub fn compatible<T: Clone + PartialEq>(slots: &[Slot<T>], r: &dyn Recipes<T>) -> bool {
    let mut rest_empty = false;
    let mut expected: Option<T> = None;
    for i in (0..slots.len()).rev() {
        let Some((item, _)) = &slots[i] else { continue };
        if rest_empty {
            return false;
        }
        if expected.as_ref().is_some_and(|e| e != item) {
            return false;
        }
        if i > 0 {
            let reversible = [3, 2]
                .into_iter()
                .find_map(|size| compaction(r, item, size).filter(|c| c.1));
            match reversible {
                Some((next, _)) => expected = Some(next),
                None => rest_empty = true,
            }
        }
    }
    true
}

/// Where a new range goes: the first slot no range covers, up to `most` slots, short of the next range.
pub fn allocate(total: usize, taken: &[(usize, usize)], most: usize) -> Option<(usize, usize)> {
    let first = (0..total).find(|s| !taken.iter().any(|(f, n)| (*f..*f + *n).contains(s)))?;
    let end = taken
        .iter()
        .map(|(f, _)| *f)
        .filter(|f| *f > first)
        .min()
        .unwrap_or(total);
    let len = (end - first).min(most);
    (len > 0).then_some((first, len))
}

// ---- the game's side ----

/// An item with its components (count 1), compared as stacks that stack.
#[derive(Clone)]
pub struct Key(ItemStack);

impl PartialEq for Key {
    fn eq(&self, other: &Key) -> bool {
        contents::same(&self.0, &other.0)
    }
}

/// The crafting recipes of the world and `additional_decompressible_items.txt`.
struct World<'a> {
    host: &'a dyn Host,
    level: Option<LevelRef>,
    crafting: Option<Identifier>,
    extra: Vec<(Key, Key, u32)>,
}

impl World<'_> {
    fn run(&self, width: u32, height: u32, item: &Key) -> Option<ItemStack> {
        let input = RecipeInput::Crafting {
            width,
            height,
            slots: vec![Some(item.0.clone()); (width * height) as usize],
        };
        self.level?
            .recipe_for(self.host, self.crafting.as_ref()?, &input)
            .ok()
            .flatten()
            .map(|m| m.output)
    }
}

impl Recipes<Key> for World<'_> {
    fn craft(&self, item: &Key, size: u32) -> Option<Key> {
        let out = self.run(size, size, item)?;
        Some(Key(contents::with_count(&out, 1)?))
    }
    fn uncraft(&self, item: &Key) -> Option<(Key, u32)> {
        let out = self.run(1, 1, item)?;
        Some((Key(contents::with_count(&out, 1)?), out.count()))
    }
    fn configured(&self, item: &Key) -> Option<(Key, u32)> {
        self.extra
            .iter()
            .find(|(from, _, _)| from == item)
            .map(|(_, to, n)| (to.clone(), *n))
    }
    fn plain(&self, item: &Key) -> bool {
        item.0.components().is_none_or(|c| c.as_nbt().is_empty())
    }
}

/// `additionalDecompressibleItems`: `compressed=Nxbroken` lines, N 4 or 9.
fn configured_items(host: &dyn Host) -> Vec<(Key, Key, u32)> {
    crate::options::Lists::get(host)
        .decompressible
        .iter()
        .filter_map(|line| {
            let (from, rest) = line.split_once('=')?;
            let (n, to) = rest.split_once('x')?;
            let n: u32 = n.parse().ok().filter(|n| matches!(n, 4 | 9))?;
            Some((
                Key(crate::menu::item(host, from)?),
                Key(crate::menu::item(host, to)?),
                n,
            ))
        })
        .collect()
}

fn world(host: &dyn Host) -> World<'_> {
    World {
        host,
        level: host.level("minecraft:overworld"),
        crafting: Identifier::parse("minecraft:crafting").ok(),
        extra: configured_items(host),
    }
}

fn to_slots(stacks: &[Option<ItemStack>]) -> Vec<Slot<Key>> {
    stacks
        .iter()
        .map(|s| {
            s.as_ref()
                .and_then(|s| Some((Key(contents::with_count(s, 1)?), s.count())))
        })
        .collect()
}

fn from_slots(slots: Vec<Slot<Key>>) -> Vec<Option<ItemStack>> {
    slots
        .into_iter()
        .map(|s| s.and_then(|(k, c)| contents::with_count(&k.0, c)))
        .collect()
}

/// A range of slots under a compression upgrade.
pub struct Bank {
    pub first: usize,
    pub len: usize,
}

/// The compression ranges of a storage, and the rules to go through them.
pub struct Banks<'a> {
    host: &'a dyn Host,
    list: Vec<Bank>,
    mult: f64,
}

/// The ranges the compression upgrades in `c` took.
fn ranges(host: &dyn Host, c: &Contents) -> Vec<(usize, usize)> {
    c.upgrades
        .iter()
        .flatten()
        .filter(|u| upgrades::of(host, u).is_some_and(|u| u.kind == Kind::Compression))
        .filter_map(|u| {
            let d = custom_data(u);
            let first = usize::try_from(get_int(&d, FIRST)?).ok()?;
            let len = usize::try_from(get_int(&d, COUNT)?).ok()?;
            let len = len.min(c.slots.len().saturating_sub(first));
            (len > 0).then_some((first, len))
        })
        .collect()
}

impl<'a> Banks<'a> {
    pub fn of(host: &'a dyn Host, kind: &StorageItem, c: &Contents) -> Banks<'a> {
        let list = ranges(host, c)
            .into_iter()
            .map(|(first, len)| Bank { first, len })
            .collect();
        Banks {
            host,
            list,
            mult: upgrades::multiplier(host, kind, &c.upgrades),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn covers(&self, slot: usize) -> bool {
        self.list
            .iter()
            .any(|b| (b.first..b.first + b.len).contains(&slot))
    }

    fn base(&self) -> impl Fn(&Key) -> u32 + '_ {
        |k| upgrades::limit(self.host, &k.0, self.mult)
    }

    fn bank_of(&self, slot: usize) -> Option<&Bank> {
        self.list
            .iter()
            .find(|b| (b.first..b.first + b.len).contains(&slot))
    }

    /// The chain of `bank`'s range, with a seed for an empty one.
    fn chain(
        &self,
        c: &Contents,
        bank: &Bank,
        seed: Option<(usize, &ItemStack)>,
    ) -> (Chain<Key>, Vec<Slot<Key>>) {
        let slots = to_slots(&c.slots[bank.first..bank.first + bank.len]);
        let key = seed.and_then(|(i, s)| Some((i - bank.first, Key(contents::with_count(s, 1)?))));
        let chain = Chain::derive(
            &slots,
            key.as_ref().map(|(i, k)| (*i, k)),
            &world(self.host),
            &self.base(),
        );
        (chain, slots)
    }

    fn put_back(c: &mut Contents, bank: &Bank, slots: Vec<Slot<Key>>) {
        for (k, s) in from_slots(slots).into_iter().enumerate() {
            c.slots[bank.first + k] = s;
        }
    }

    /// What slot `slot` shows (everything compressed before it, counted in its item).
    pub fn shown(&self, c: &Contents, slot: usize) -> Option<ItemStack> {
        let bank = self.bank_of(slot)?;
        let (chain, slots) = self.chain(c, bank, None);
        let at = slot - bank.first;
        let n = chain.calculated(&slots)[at];
        let d = chain.defs[at].as_ref()?;
        contents::with_count(&d.item.0, n as u32)
    }

    /// Most that slot `slot` holds of what it holds (what the pipes see as its capacity).
    pub fn limit(&self, c: &Contents, slot: usize) -> Option<u32> {
        let bank = self.bank_of(slot)?;
        let (chain, _) = self.chain(c, bank, None);
        Some(
            chain.defs[slot - bank.first]
                .as_ref()
                .map_or(0, |d| d.limit as u32),
        )
    }

    /// Puts `amount` of `stack` into slot `slot` of its range.
    pub fn insert_at(&self, c: &mut Contents, slot: usize, stack: &ItemStack, amount: u32) -> u32 {
        let Some(bank) = self.bank_of(slot) else {
            return 0;
        };
        let (chain, mut slots) = self.chain(c, bank, Some((slot, stack)));
        let Some(key) = contents::with_count(stack, 1).map(Key) else {
            return 0;
        };
        let put = chain.insert(&mut slots, slot - bank.first, &key, amount, &self.base());
        if put > 0 {
            Self::put_back(c, bank, slots);
        }
        put
    }

    /// Takes up to `amount` out of slot `slot`'s range.
    pub fn take_at(&self, c: &mut Contents, slot: usize, amount: u32) -> u32 {
        let Some(bank) = self.bank_of(slot) else {
            return 0;
        };
        let (chain, mut slots) = self.chain(c, bank, None);
        let took = chain.take(&mut slots, slot - bank.first, amount, &self.base());
        if took > 0 {
            Self::put_back(c, bank, slots);
        }
        took
    }

    /// Puts `stack` into the ranges, the first slot of an empty one starting it; how many went in.
    pub fn insert(&self, c: &mut Contents, stack: &ItemStack, amount: u32) -> u32 {
        let mut left = amount;
        for bank in &self.list {
            for slot in bank.first..bank.first + bank.len {
                if left == 0 {
                    return amount;
                }
                left -= self.insert_at(c, slot, stack, left);
            }
        }
        amount - left
    }

    /// Takes up to `amount` of `stack`'s item out of the ranges.
    pub fn extract(&self, c: &mut Contents, stack: &ItemStack, amount: u32) -> u32 {
        let mut left = amount;
        for bank in &self.list {
            for slot in (bank.first..bank.first + bank.len).rev() {
                if left == 0 {
                    return amount;
                }
                if self
                    .shown(c, slot)
                    .is_some_and(|s| contents::same(&s, stack))
                {
                    left -= self.take_at(c, slot, left);
                }
            }
        }
        amount - left
    }
}

impl Contents {
    /// Puts `amount` of `stack` in: into a compression range if one takes it, then onto stacks and into free
    /// slots; how many went in.
    pub fn add(
        &mut self,
        host: &dyn Host,
        kind: &StorageItem,
        stack: &ItemStack,
        amount: u32,
    ) -> u32 {
        let banks = Banks::of(host, kind, self);
        let lim = upgrades::limit(host, stack, banks.mult);
        if banks.is_empty() {
            return contents::insert(&mut self.slots, stack, amount, lim);
        }
        let first = banks.insert(self, stack, amount);
        first
            + self.plain(&banks, |slots| {
                contents::insert(slots, stack, amount - first, lim)
            })
    }

    /// Takes up to `amount` of `stack`'s item out of the plain slots and the compression ranges; how many.
    pub fn remove(
        &mut self,
        host: &dyn Host,
        kind: &StorageItem,
        stack: &ItemStack,
        amount: u32,
    ) -> u32 {
        let banks = Banks::of(host, kind, self);
        if banks.is_empty() {
            return contents::extract(&mut self.slots, stack, amount);
        }
        let first = self.plain(&banks, |slots| contents::extract(slots, stack, amount));
        first + banks.extract(self, stack, amount - first)
    }

    /// What each slot shows: the stacks, except in a compression range.
    pub fn visible(&self, host: &dyn Host, kind: &StorageItem) -> Vec<Option<ItemStack>> {
        let banks = Banks::of(host, kind, self);
        if banks.is_empty() {
            return self.slots.clone();
        }
        (0..self.slots.len())
            .map(|i| {
                if banks.covers(i) {
                    banks.shown(self, i)
                } else {
                    self.slots[i].clone()
                }
            })
            .collect()
    }

    /// What slot `slot` shows.
    pub fn shown_at(&self, host: &dyn Host, kind: &StorageItem, slot: usize) -> Option<ItemStack> {
        let banks = Banks::of(host, kind, self);
        if banks.covers(slot) {
            banks.shown(self, slot)
        } else {
            self.slots.get(slot).cloned().flatten()
        }
    }

    /// Takes up to `amount` out of slot `slot` as shown; how many.
    pub fn take_from(
        &mut self,
        host: &dyn Host,
        kind: &StorageItem,
        slot: usize,
        amount: u32,
    ) -> u32 {
        let banks = Banks::of(host, kind, self);
        if banks.covers(slot) {
            return banks.take_at(self, slot, amount);
        }
        let Some((st, n)) = self.slots.get(slot).cloned().flatten().map(|s| {
            let n = s.count().min(amount);
            (s, n)
        }) else {
            return 0;
        };
        self.slots[slot] = contents::with_count(&st, st.count() - n);
        n
    }

    /// Runs `f` on the slots outside the compression ranges.
    pub fn plain(&mut self, banks: &Banks, f: impl FnOnce(&mut [Option<ItemStack>]) -> u32) -> u32 {
        let at: Vec<usize> = (0..self.slots.len())
            .filter(|i| !banks.covers(*i))
            .collect();
        let mut part: Vec<Option<ItemStack>> = at.iter().map(|i| self.slots[*i].take()).collect();
        let r = f(&mut part);
        for (k, i) in at.iter().enumerate() {
            self.slots[*i] = part[k].take();
        }
        r
    }
}

/// Why compression upgrade `up` can't go into upgrade slot `j` of `c`, if it can't.
pub fn may_add(host: &dyn Host, kind: &StorageItem, c: &Contents) -> Result<(), &'static str> {
    if c.upgrades
        .iter()
        .flatten()
        .any(|u| upgrades::of(host, u).is_some_and(|u| u.kind == Kind::Compacting))
    {
        return Err("There's already a Compacting upgrade in another upgrade slot");
    }
    let Some((first, len)) = free_range(host, c) else {
        return Err("There are no additional slots this compression upgrade can manage");
    };
    let _ = kind;
    let slots = to_slots(&c.slots[first..first + len]);
    if !compatible(&slots, &world(host)) {
        return Err("Items in those slots can't be compressed from the previous slot");
    }
    Ok(())
}

/// Where the next compression upgrade's range goes.
fn free_range(host: &dyn Host, c: &Contents) -> Option<(usize, usize)> {
    allocate(
        c.slots.len(),
        &ranges(host, c),
        crate::options::tune().compression_slots,
    )
}

/// A compression upgrade just put in: it takes its range (`onAdded`).
pub fn on_added(host: &dyn Host, c: &Contents, up: &mut ItemStack) {
    let Some((first, len)) = free_range(host, c) else {
        return;
    };
    let mut d = custom_data(up);
    put(&mut d, FIRST, mod_api::Tag::Int(first as i32));
    put(&mut d, COUNT, mod_api::Tag::Int(len as i32));
    set_custom_data(up, d);
}

/// Takes the range keys off an upgrade stack taken out (`getCleanedUpgradeStack`).
pub fn cleaned(up: &mut ItemStack) {
    let mut d = custom_data(up);
    d.del(FIRST);
    d.del(COUNT);
    set_custom_data(up, d);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nuggets, ingots, blocks: 9 to one; "dust" to "glow" 4 to one only by config; everything else alone.
    struct Fake;
    const CRAFTS: [(&str, u32, &str); 4] = [
        ("nugget", 3, "ingot"),
        ("ingot", 3, "block"),
        ("dust", 2, "glow"),
        ("snow", 2, "snow_block"),
    ];
    const UNCRAFTS: [(&str, &str, u32); 3] = [
        ("ingot", "nugget", 9),
        ("block", "ingot", 9),
        ("snow_block", "snow", 4),
    ];
    impl Recipes<&'static str> for Fake {
        fn craft(&self, item: &&'static str, size: u32) -> Option<&'static str> {
            CRAFTS
                .iter()
                .find(|c| c.0 == *item && c.1 == size)
                .map(|c| c.2)
        }
        fn uncraft(&self, item: &&'static str) -> Option<(&'static str, u32)> {
            UNCRAFTS.iter().find(|c| c.0 == *item).map(|c| (c.1, c.2))
        }
        fn configured(&self, item: &&'static str) -> Option<(&'static str, u32)> {
            (*item == "glow").then_some(("dust", 4))
        }
        fn plain(&self, _: &&'static str) -> bool {
            true
        }
    }

    fn base(_: &&'static str) -> u32 {
        64
    }

    fn chain(slots: &[Slot<&'static str>]) -> Chain<&'static str> {
        Chain::derive(slots, None, &Fake, &base)
    }

    fn items(c: &Chain<&'static str>) -> Vec<Option<&'static str>> {
        c.defs.iter().map(|d| d.as_ref().map(|d| d.item)).collect()
    }

    #[test]
    fn the_last_used_slot_is_the_base_and_the_rest_follow_it() {
        let s = [None, Some(("ingot", 5)), None, None, None];
        let c = chain(&s);
        assert_eq!(
            items(&c),
            [Some("block"), Some("ingot"), Some("nugget"), None, None]
        );
        // Each slot's multiplier is how many of its item make one of the slot before's.
        let m: Vec<u32> = c.defs.iter().flatten().map(|d| d.mult).collect();
        assert_eq!(m, [1, 9, 9]);
        // And a slot holds what it does plus what the earlier ones hold at that size.
        let l: Vec<u64> = c.defs.iter().flatten().map(|d| d.limit).collect();
        assert_eq!(l, [64, 64 + 9 * 64, 64 + 9 * (64 + 9 * 64)]);
    }

    #[test]
    fn a_stranger_in_the_range_stops_compression() {
        let s = [Some(("snow", 1)), Some(("ingot", 5)), None];
        let c = chain(&s);
        assert_eq!(items(&c), [Some("snow"), Some("ingot"), None]);
        assert!(
            c.defs
                .iter()
                .flatten()
                .all(|d| d.mult == 1 && !d.compressible)
        );
    }

    #[test]
    fn an_empty_range_has_no_chain_until_something_is_put_in() {
        let s: [Slot<&str>; 3] = [None, None, None];
        assert!(chain(&s).defs.iter().all(Option::is_none));
        let c = Chain::derive(&s, Some((0, &"ingot")), &Fake, &base);
        assert_eq!(items(&c), [Some("ingot"), Some("nugget"), None]);
    }

    #[test]
    fn a_configured_shape_is_used_when_no_recipe_reverses() {
        // dust crafts 2x2 into glow, but nothing crafts glow back: only the config knows it breaks into 4 dust.
        let s = [None, Some(("dust", 3))];
        let c = chain(&s);
        assert_eq!(items(&c), [Some("glow"), Some("dust")]);
        assert_eq!(c.defs[1].as_ref().map(|d| d.mult), Some(4));
    }

    #[test]
    fn slots_show_what_is_compressed_before_them() {
        let s = [Some(("block", 2)), Some(("ingot", 4)), Some(("nugget", 5))];
        let c = chain(&s);
        assert_eq!(c.calculated(&s), [2, 22, 203]);
    }

    #[test]
    fn putting_in_carries_up() {
        let mut s = [Some(("block", 1)), Some(("ingot", 8)), Some(("nugget", 8))];
        let c = chain(&s);
        // 1 block, 8 ingots and 8 nuggets are 161 nuggets; one more makes 162, which is 2 blocks.
        assert_eq!(c.insert(&mut s, 2, &"nugget", 1, &base), 1);
        assert_eq!(s, [Some(("block", 2)), None, None]);
        assert_eq!(c.insert(&mut s, 1, &"ingot", 1, &base), 1);
        assert_eq!(s, [Some(("block", 2)), Some(("ingot", 1)), None]);
        // 19 ingots and 20 more are 39: 4 blocks and 3 ingots.
        assert_eq!(c.insert(&mut s, 1, &"ingot", 20, &base), 20);
        assert_eq!(s, [Some(("block", 4)), Some(("ingot", 3)), None]);
    }

    #[test]
    fn taking_out_breaks_compressed_items_down() {
        let mut s = [Some(("block", 1)), Some(("ingot", 2)), None];
        let c = chain(&s);
        assert_eq!(c.take(&mut s, 1, 5, &base), 5);
        assert_eq!(s, [None, Some(("ingot", 6)), None]);
        // Nuggets can be taken from a block's worth, and no more than there is.
        let mut s = [Some(("block", 1)), None, None];
        let c = Chain::derive(&s, None, &Fake, &base);
        assert_eq!(c.take(&mut s, 2, 100, &base), 81);
        assert_eq!(c.take(&mut s, 2, 1, &base), 0);
    }

    #[test]
    fn a_slot_takes_no_more_than_the_run_holds() {
        let mut s = [Some(("block", 64)), Some(("ingot", 8)), Some(("nugget", 8))];
        let c = chain(&s);
        // Blocks are full, so 8 ingots and 8 nuggets stay as they are and only 1 more nugget fits.
        let top = c.defs[2].as_ref().map(|d| d.limit).unwrap_or(0);
        assert_eq!(top, 64 + 9 * (64 + 9 * 64));
        assert_eq!(c.insert(&mut s, 0, &"block", 1, &base), 0);
        assert_eq!(c.insert(&mut s, 2, &"ingot", 1, &base), 0, "wrong item");
    }

    #[test]
    fn a_plain_slot_just_counts() {
        let mut s = [None, Some(("snow", 3)), None];
        s[1] = Some(("snow", 3));
        let c = chain(&s);
        // snow compresses 2x2 into snow_block, which uncrafts back to 4 of it.
        assert_eq!(items(&c), [Some("snow_block"), Some("snow"), None]);
        // The fourth snowball makes a snow block.
        assert_eq!(c.insert(&mut s, 1, &"snow", 1, &base), 1);
        assert_eq!(s, [Some(("snow_block", 1)), None, None]);
    }

    #[test]
    fn the_range_must_hold_one_material() {
        assert!(compatible(
            &[None, Some(("ingot", 1)), Some(("nugget", 1))],
            &Fake
        ));
        assert!(!compatible(
            &[Some(("block", 1)), Some(("nugget", 1))],
            &Fake
        ));
        assert!(!compatible(&[Some(("snow", 1)), Some(("ingot", 1))], &Fake));
        assert!(compatible(&[None, None], &Fake));
    }

    #[test]
    fn ranges_start_where_the_last_one_ended() {
        assert_eq!(allocate(27, &[], 5), Some((0, 5)));
        assert_eq!(allocate(27, &[(0, 5)], 5), Some((5, 5)));
        assert_eq!(allocate(27, &[(0, 5), (5, 5)], 9), Some((10, 9)));
        // A gap left by a removed upgrade is the next space, as large as it is.
        assert_eq!(allocate(27, &[(0, 3), (5, 5)], 5), Some((3, 2)));
        assert_eq!(allocate(4, &[(0, 4)], 5), None);
        assert_eq!(allocate(3, &[], 5), Some((0, 3)));
    }

    #[test]
    fn a_loop_of_recipes_is_not_compression() {
        struct Loop;
        impl Recipes<u8> for Loop {
            fn craft(&self, item: &u8, size: u32) -> Option<u8> {
                (size == 2).then_some((item + 1) % 3)
            }
            fn uncraft(&self, _: &u8) -> Option<(u8, u32)> {
                None
            }
            fn configured(&self, _: &u8) -> Option<(u8, u32)> {
                None
            }
            fn plain(&self, _: &u8) -> bool {
                true
            }
        }
        assert!(in_loop(&Loop, &0, &1));
        assert_eq!(shape(&Loop, &0), None);
    }
}
