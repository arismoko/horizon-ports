//! The smelting, smoking and blasting upgrades (CookingLogic, CookingUpgradeWrapper and AutoCookingUpgradeWrapper):
//! a furnace inside the upgrade, ticking while its storage is carried or placed. The automatic ones also pull their
//! input and fuel out of the storage, by filters, and push the output (and fuel that burns no more) back.
//!
//! The rules are a state machine over a stack type (`Env` gives the recipe, fuel and stack arithmetic), so they run
//! the same in the tests with a stand-in stack. The state lives in the upgrade's `custom_data` under the jar's
//! keys: `cookingInventory` (input, fuel, output), `burnTimeFinish`, `burnTimeTotal`, `cookTimeFinish` (-1 when
//! never started), `cookTimeTotal`, `isCooking`. A disabled or removed upgrade keeps no paused progress: its clock
//! runs on without it.

use std::collections::HashMap;
use std::sync::Mutex;

use mod_api::{Compound, Host, Identifier, ItemStack, LevelRef, RecipeInput, Tag};

use crate::contents::{
    self, Contents, Nbt, custom_data, put, read_items, set_custom_data, write_items,
};
use crate::upgrades;

pub const INPUT: usize = 0;
pub const FUEL: usize = 1;
pub const OUTPUT: usize = 2;

/// `AutoCookingUpgradeWrapper.NOTHING_TO_DO_COOLDOWN` and `NO_INVENTORY_SPACE_COOLDOWN`.
const NOTHING_TO_DO: i64 = 10;
const NO_SPACE: u32 = 60;

/// Which cooking recipe type an upgrade uses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cooking {
    /// 0 smelting, 1 smoking, 2 blasting.
    pub recipe: u8,
    pub auto: bool,
}

impl Cooking {
    pub fn recipe_type(self) -> &'static str {
        [
            "minecraft:smelting",
            "minecraft:smoking",
            "minecraft:blasting",
        ][usize::from(self.recipe)]
    }

    /// Smoking and blasting burn fuel at twice the speed (`burnTimeModifier` 0.5).
    pub fn burn_modifier(self) -> f64 {
        if self.recipe == 0 { 1.0 } else { 0.5 }
    }

    /// The upgrade's index in `options::COOKING`.
    pub fn index(self) -> usize {
        usize::from(self.recipe) + if self.auto { 3 } else { 0 }
    }
}

/// What the rules need from the game.
pub trait Env {
    type Stack: Clone + PartialEq;
    fn now(&self) -> i64;
    /// The cooking recipe's output and its cooking time in ticks.
    fn recipe(&self, input: &Self::Stack) -> Option<(Self::Stack, u32)>;
    /// Ticks the item burns as fuel (0 for none).
    fn burn(&self, fuel: &Self::Stack) -> i32;
    /// What burning the item leaves (a bucket).
    fn remainder(&self, fuel: &Self::Stack) -> Option<Self::Stack>;
    fn count(&self, s: &Self::Stack) -> u32;
    /// The stack with another count (none for 0).
    fn resized(&self, s: &Self::Stack, n: u32) -> Option<Self::Stack>;
    fn same(&self, a: &Self::Stack, b: &Self::Stack) -> bool;
    /// How many of the item one stack holds.
    fn max(&self, s: &Self::Stack) -> u32;
    /// A wet sponge cooked with a bucket as fuel leaves a water bucket.
    fn sponge_swap(&self, input: &Self::Stack, fuel: &Self::Stack) -> Option<Self::Stack>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cook<S> {
    pub slots: [Option<S>; 3],
    pub burn_finish: i64,
    pub burn_total: i32,
    pub cook_finish: i64,
    pub cook_total: i32,
    pub cooking: bool,
}

impl<S> Default for Cook<S> {
    fn default() -> Self {
        Self {
            slots: [None, None, None],
            burn_finish: 0,
            burn_total: 0,
            cook_finish: -1,
            cook_total: 0,
            cooking: false,
        }
    }
}

/// The settings that scale a cooking upgrade.
#[derive(Clone, Copy, Debug)]
pub struct Rates {
    pub speed: f64,
    pub fuel: f64,
    pub burn_modifier: f64,
}

impl<S: Clone + PartialEq> Cook<S> {
    pub fn burning(&self, now: i64) -> bool {
        self.burn_finish >= now
    }

    fn can_smelt<E: Env<Stack = S>>(&self, env: &E, out: &S) -> bool {
        if self.slots[INPUT].is_none() {
            return false;
        }
        let add = env.count(out);
        if add == 0 {
            return false;
        }
        match &self.slots[OUTPUT] {
            None => true,
            Some(o) => env.same(o, out) && env.count(o) + add <= env.max(o),
        }
    }

    fn smelt<E: Env<Stack = S>>(&mut self, env: &E, out: &S) {
        if !self.can_smelt(env, out) {
            return;
        }
        self.slots[OUTPUT] = match &self.slots[OUTPUT] {
            None => Some(out.clone()),
            Some(o) => env.resized(o, env.count(o) + env.count(out)),
        };
        if let (Some(i), Some(f)) = (&self.slots[INPUT], &self.slots[FUEL])
            && let Some(w) = env.sponge_swap(i, f)
        {
            self.slots[FUEL] = Some(w);
        }
        if let Some(i) = &self.slots[INPUT] {
            self.slots[INPUT] = env.resized(i, env.count(i) - 1);
        }
    }

    fn set_cook_time(&mut self, now: i64, t: i64) {
        self.cook_finish = now + t;
        self.cook_total = t as i32;
    }

    fn update_fuel<E: Env<Stack = S>>(&mut self, env: &E, r: &Rates, out: &S) {
        let now = env.now();
        if self.burning(now) || !self.can_smelt(env, out) {
            return;
        }
        let Some(fuel) = self.slots[FUEL].clone() else {
            return;
        };
        let base = (f64::from(env.burn(&fuel)) * r.burn_modifier) as i32;
        if base <= 0 {
            return;
        }
        let t = (f64::from(base) * r.fuel / r.speed) as i64;
        self.burn_finish = now + t;
        self.burn_total = t as i32;
        if self.burning(now) {
            self.slots[FUEL] = match env.remainder(&fuel) {
                Some(rem) => Some(rem),
                None => env.resized(&fuel, env.count(&fuel) - 1),
            };
        }
    }

    fn progress<E: Env<Stack = S>>(&mut self, env: &E, r: &Rates, out: &S, time: u32) {
        let now = env.now();
        let total = (f64::from(time) * (1.0 / r.speed)) as i64;
        if self.cooking && self.cook_finish <= now {
            self.smelt(env, out);
            if self.can_smelt(env, out) {
                self.set_cook_time(now, total);
            } else {
                self.cooking = false;
            }
        } else if !self.cooking {
            self.cooking = true;
            self.set_cook_time(now, total);
        }
    }

    /// `CookingLogic.tick`: false when there was nothing for it to do.
    pub fn tick<E: Env<Stack = S>>(&mut self, env: &E, r: &Rates) -> bool {
        let now = env.now();
        let mut did = true;
        let ready = self.slots[FUEL].is_some() && self.slots[INPUT].is_some();
        if self.burning(now) || ready {
            let found = self.slots[INPUT].as_ref().and_then(|i| env.recipe(i));
            if found.is_none() && self.cooking {
                self.cooking = false;
            }
            if let Some((out, time)) = found {
                self.update_fuel(env, r, &out);
                if self.burning(now) && self.can_smelt(env, &out) {
                    self.progress(env, r, &out, time);
                } else if !self.burning(now) {
                    did = false;
                }
            }
        }
        if !self.burning(now) && self.cooking {
            // The heat leaves: progress drains by two ticks a tick.
            let remaining = self.cook_finish - now;
            if remaining + 2 > i64::from(self.cook_total) {
                self.cooking = false;
            } else {
                self.cook_finish = now + (remaining + 2).min(i64::from(self.cook_total));
            }
        } else {
            did = false;
        }
        did
    }
}

// ---- the automatic upgrades ----

/// A filter of an automatic cooking upgrade (FilterLogic under `inputFilter` or `fuelFilter`).
#[derive(Clone, Debug)]
pub struct Filter {
    pub items: Vec<Option<ItemStack>>,
    pub allow: bool,
    pub by_mod: bool,
    /// The fuel filter: an empty allow list lets everything through.
    pub empty_allows_all: bool,
    /// A hopper upgrade's contents mode (`filterByStorage`): items match by what the inventory already holds.
    pub by_storage: bool,
}

impl Filter {
    pub fn read(host: &dyn Host, up: &ItemStack, key: &str, n: usize) -> Self {
        let fuel = key == "fuelFilter";
        let d = custom_data(up);
        let sub = match d.at(key) {
            Some(Tag::Compound(c)) => c.clone(),
            _ => Compound::new(),
        };
        Self {
            items: read_items(host, sub.at("filters"), n),
            allow: contents::get_bool(&sub, "isAllowList").unwrap_or(fuel),
            by_mod: contents::get_str(&sub, "primaryMatch").as_deref() == Some("mod"),
            empty_allows_all: fuel,
            by_storage: contents::get_bool(&sub, "filterByStorage").unwrap_or(false),
        }
    }

    pub fn write(&self, host: &dyn Host, up: &mut ItemStack, key: &str) {
        let mut d = custom_data(up);
        let mut sub = match d.at(key) {
            Some(Tag::Compound(c)) => c.clone(),
            _ => Compound::new(),
        };
        put(&mut sub, "filters", write_items(host, &self.items));
        put(&mut sub, "isAllowList", Tag::Byte(i8::from(self.allow)));
        put(
            &mut sub,
            "primaryMatch",
            Tag::String(if self.by_mod { "mod" } else { "item" }.into()),
        );
        if self.by_storage {
            put(&mut sub, "filterByStorage", Tag::Byte(1));
        } else {
            sub.del("filterByStorage");
        }
        put(&mut d, key, Tag::Compound(sub));
        set_custom_data(up, d);
    }

    /// ContentsFilterLogic.matchesFilter: in contents mode a stack matches when it is the item (or from the mod) of
    /// one `held` in the inventory the filter watches; otherwise the list decides.
    pub fn matches_contents<'a>(
        &self,
        host: &dyn Host,
        stack: &ItemStack,
        held: impl IntoIterator<Item = &'a ItemStack>,
    ) -> bool {
        if !self.by_storage {
            return self.matches(host, stack);
        }
        if contents::is_unknown(stack) {
            return false;
        }
        let ns = |s: &ItemStack| {
            contents::item_id(host, s)
                .split(':')
                .next()
                .unwrap_or("")
                .to_owned()
        };
        held.into_iter().any(|x| {
            if self.by_mod {
                ns(x) == ns(stack)
            } else {
                x.same_item(stack)
            }
        })
    }

    pub fn matches(&self, host: &dyn Host, stack: &ItemStack) -> bool {
        if contents::is_unknown(stack) {
            return false;
        }
        let empty = self.items.iter().all(Option::is_none);
        let ns = |s: &ItemStack| {
            contents::item_id(host, s)
                .split(':')
                .next()
                .unwrap_or("")
                .to_owned()
        };
        let hit = self.items.iter().flatten().any(|x| {
            if self.by_mod {
                ns(x) == ns(stack)
            } else {
                x.same_item(stack)
            }
        });
        if self.allow {
            (empty && self.empty_allows_all) || hit
        } else {
            empty || !hit
        }
    }
}

/// What an automatic upgrade keeps between ticks (the jar's fields, lost with the server).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Auto {
    out: u32,
    fuel: u32,
    input: u32,
    until: i64,
}

/// How an automatic upgrade tells what may go into the furnace.
pub struct Accept<'a, S> {
    pub input: &'a dyn Fn(&S) -> bool,
    pub fuel: &'a dyn Fn(&S) -> bool,
}

fn put_in<E: Env>(
    env: &E,
    store: &mut [Option<E::Stack>],
    s: &E::Stack,
    want: u32,
    limit: u32,
) -> u32 {
    let mut left = want;
    for slot in store.iter_mut() {
        if left == 0 {
            break;
        }
        if let Some(held) = slot.clone()
            && env.same(&held, s)
            && env.count(&held) < limit
        {
            let add = (limit - env.count(&held)).min(left);
            *slot = env.resized(&held, env.count(&held) + add);
            left -= add;
        }
    }
    for slot in store.iter_mut() {
        if left == 0 {
            break;
        }
        if slot.is_none() {
            let add = limit.min(left);
            *slot = env.resized(s, add);
            left -= add;
        }
    }
    want - left
}

fn take_out<E: Env>(env: &E, store: &mut [Option<E::Stack>], like: &E::Stack, want: u32) -> u32 {
    let mut left = want;
    for slot in store.iter_mut().rev() {
        if left == 0 {
            break;
        }
        if let Some(held) = slot.clone()
            && env.same(&held, like)
        {
            let n = env.count(&held).min(left);
            *slot = env.resized(&held, env.count(&held) - n);
            left -= n;
        }
    }
    want - left
}

/// `tryPullingGetUnsucessful`: tops the furnace slot up from the storage; true when nothing came.
fn pull<E: Env>(
    env: &E,
    store: &mut [Option<E::Stack>],
    slot: &mut Option<E::Stack>,
    valid: &dyn Fn(&E::Stack) -> bool,
) -> bool {
    let (like, want) = match slot {
        None => {
            let Some(first) = store.iter().flatten().find(|s| valid(s)).cloned() else {
                return true;
            };
            let m = env.max(&first);
            (first, m)
        }
        Some(s) => {
            let m = env.max(s);
            if env.count(s) >= m || !valid(s) {
                return true;
            }
            (s.clone(), m - env.count(s))
        }
    };
    let got = take_out(env, store, &like, want);
    if got == 0 {
        return true;
    }
    let held = slot.as_ref().map_or(0, |s| env.count(s));
    *slot = env.resized(&like, got + held);
    false
}

/// `AutoCookingUpgradeWrapper.tick`; `limit` is the storage slot's size for an item. Returns whether the storage
/// changed.
pub fn auto_tick<E: Env>(
    env: &E,
    r: &Rates,
    cook: &mut Cook<E::Stack>,
    store: &mut [Option<E::Stack>],
    limit: &dyn Fn(&E::Stack) -> u32,
    accept: &Accept<'_, E::Stack>,
    cd: &mut Auto,
) -> bool {
    let now = env.now();
    if now < cd.until {
        return false;
    }
    let mut changed = false;
    // tryPushingOutput
    if cd.out > 0 {
        cd.out -= 1;
    } else {
        let mut pushed = false;
        if let Some(o) = cook.slots[OUTPUT].clone() {
            let n = put_in(env, store, &o, env.count(&o), limit(&o));
            if n > 0 {
                cook.slots[OUTPUT] = env.resized(&o, env.count(&o) - n);
                pushed = true;
                changed = true;
            }
        }
        if !pushed {
            cd.out = NO_SPACE;
        }
        if let Some(f) = cook.slots[FUEL].clone()
            && env.burn(&f) <= 0
        {
            let n = put_in(env, store, &f, env.count(&f), limit(&f));
            if n > 0 {
                cook.slots[FUEL] = env.resized(&f, env.count(&f) - n);
                changed = true;
            }
        }
    }
    if cd.fuel > 0 {
        cd.fuel -= 1;
    } else if pull(env, store, &mut cook.slots[FUEL], accept.fuel) {
        cd.fuel = NO_SPACE;
    } else {
        changed = true;
    }
    if cd.input > 0 {
        cd.input -= 1;
    } else if pull(env, store, &mut cook.slots[INPUT], accept.input) {
        cd.input = NO_SPACE;
    } else {
        changed = true;
    }
    if !cook.tick(env, r) && cd.out == 0 && cd.fuel == 0 && cd.input == 0 {
        cd.until = now + NOTHING_TO_DO;
    }
    changed
}

// ---- in the game ----

static AUTO: Mutex<Option<HashMap<([i32; 4], usize), Auto>>> = Mutex::new(None);

/// The game behind the rules.
pub struct Real<'a> {
    pub host: &'a dyn Host,
    pub level: LevelRef,
    pub kind: Cooking,
}

impl Env for Real<'_> {
    type Stack = ItemStack;

    fn now(&self) -> i64 {
        self.level.game_time(self.host).unwrap_or(0)
    }

    fn recipe(&self, input: &ItemStack) -> Option<(ItemStack, u32)> {
        let ty = Identifier::parse(self.kind.recipe_type()).ok()?;
        let one = contents::with_count(input, 1)?;
        let m = self
            .level
            .recipe_for(self.host, &ty, &RecipeInput::Single(one))
            .ok()??;
        Some((m.output, m.processing_time))
    }

    fn burn(&self, fuel: &ItemStack) -> i32 {
        self.host.item_burn_time(fuel.item()).unwrap_or(0).max(0)
    }

    fn remainder(&self, fuel: &ItemStack) -> Option<ItemStack> {
        let id = "minecraft:crafting_remainder";
        let own = fuel.components().and_then(|c| c.as_nbt().at(id).cloned());
        let Tag::Compound(c) = own.or_else(|| self.host.item_default_component(fuel.item(), id))?
        else {
            return None;
        };
        ItemStack::from_nbt(self.host, &c)
    }

    fn count(&self, s: &ItemStack) -> u32 {
        s.count()
    }

    fn resized(&self, s: &ItemStack, n: u32) -> Option<ItemStack> {
        contents::with_count(s, n)
    }

    fn same(&self, a: &ItemStack, b: &ItemStack) -> bool {
        contents::same(a, b)
    }

    fn max(&self, s: &ItemStack) -> u32 {
        self.host.max_stack_size(s.item()).unwrap_or(64).max(1)
    }

    fn sponge_swap(&self, input: &ItemStack, fuel: &ItemStack) -> Option<ItemStack> {
        (contents::item_id(self.host, input) == "minecraft:wet_sponge"
            && contents::item_id(self.host, fuel) == "minecraft:bucket")
            .then(|| crate::menu::item(self.host, "minecraft:water_bucket"))
            .flatten()
    }
}

fn long(c: &Compound, k: &str) -> Option<i64> {
    match c.at(k)? {
        Tag::Long(v) => Some(*v),
        Tag::Int(v) => Some(i64::from(*v)),
        _ => None,
    }
}

/// The furnace state kept in an upgrade stack.
pub fn read(host: &dyn Host, up: &ItemStack) -> Cook<ItemStack> {
    let d = custom_data(up);
    let inv = d
        .at("cookingInventory")
        .or_else(|| d.at("smeltingInventory"));
    let mut s = read_items(host, inv, 3);
    s.resize(3, None);
    let mut it = s.into_iter();
    let mut next = || it.next().flatten();
    Cook {
        slots: [next(), next(), next()],
        burn_finish: long(&d, "burnTimeFinish").unwrap_or(0),
        burn_total: contents::get_int(&d, "burnTimeTotal").unwrap_or(0),
        cook_finish: long(&d, "cookTimeFinish").unwrap_or(-1),
        cook_total: contents::get_int(&d, "cookTimeTotal").unwrap_or(0),
        cooking: contents::get_bool(&d, "isCooking").unwrap_or(false),
    }
}

pub fn write(host: &dyn Host, up: &mut ItemStack, c: &Cook<ItemStack>) {
    let mut d = custom_data(up);
    put(&mut d, "cookingInventory", write_items(host, &c.slots));
    put(&mut d, "burnTimeFinish", Tag::Long(c.burn_finish));
    put(&mut d, "burnTimeTotal", Tag::Int(c.burn_total));
    put(&mut d, "cookTimeFinish", Tag::Long(c.cook_finish));
    put(&mut d, "cookTimeTotal", Tag::Int(c.cook_total));
    put(&mut d, "isCooking", Tag::Byte(i8::from(c.cooking)));
    set_custom_data(up, d);
}

pub fn rates(k: Cooking) -> Rates {
    let (speed, fuel) = crate::options::tune().cooking[k.index()];
    Rates {
        speed,
        fuel,
        burn_modifier: k.burn_modifier(),
    }
}

/// The input and fuel filters of an automatic upgrade, with the settings' slot counts.
pub fn filters(host: &dyn Host, up: &ItemStack, k: Cooking) -> (Filter, Filter) {
    let n = crate::options::tune().cook_filters[usize::from(k.recipe)];
    (
        Filter::read(host, up, "inputFilter", n[0]),
        Filter::read(host, up, "fuelFilter", n[1]),
    )
}

/// One tick of every cooking upgrade of a storage (`id` is its contents id). True when its contents changed.
pub fn run(
    host: &dyn Host,
    level: LevelRef,
    id: [i32; 4],
    kind: &crate::storage_items::StorageItem,
    c: &mut Contents,
) -> bool {
    let mult = upgrades::multiplier(host, kind, &c.upgrades);
    let mut changed = false;
    for j in 0..c.upgrades.len() {
        let Some(mut up) = c.upgrades[j].clone() else {
            continue;
        };
        if !upgrades::flag(&up, "enabled", true) {
            continue;
        }
        let Some(upgrades::Upgrade {
            kind: upgrades::Kind::Cooking(k),
            ..
        }) = upgrades::of(host, &up)
        else {
            continue;
        };
        let env = Real {
            host,
            level,
            kind: k,
        };
        let r = rates(k);
        let mut cook = read(host, &up);
        let before = cook.clone();
        if k.auto {
            let (fi, ff) = filters(host, &up, k);
            let is_input = |s: &ItemStack| env.recipe(s).is_some() && fi.matches(host, s);
            let is_fuel = |s: &ItemStack| env.burn(s) > 0 && ff.matches(host, s);
            let mut cds = AUTO.lock().unwrap_or_else(|e| e.into_inner());
            let cd = cds
                .get_or_insert_with(HashMap::new)
                .entry((id, j))
                .or_default();
            let moved = auto_tick(
                &env,
                &r,
                &mut cook,
                &mut c.slots,
                &|s| upgrades::limit(host, s, mult),
                &Accept {
                    input: &is_input,
                    fuel: &is_fuel,
                },
                cd,
            );
            changed |= moved;
        } else {
            cook.tick(&env, &r);
        }
        if cook != before {
            write(host, &mut up, &cook);
            c.upgrades[j] = Some(up);
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in stack: item name and count.
    type S = (&'static str, u32);

    struct Fake {
        now: i64,
    }

    impl Env for Fake {
        type Stack = S;
        fn now(&self) -> i64 {
            self.now
        }
        fn recipe(&self, input: &S) -> Option<(S, u32)> {
            match input.0 {
                "ore" => Some((("ingot", 1), 200)),
                "log" => Some((("charcoal", 1), 200)),
                "sponge" => Some((("sponge_dry", 1), 200)),
                _ => None,
            }
        }
        fn burn(&self, fuel: &S) -> i32 {
            match fuel.0 {
                "coal" => 1600,
                "log" => 300,
                "lava" => 20000,
                _ => 0,
            }
        }
        fn remainder(&self, fuel: &S) -> Option<S> {
            (fuel.0 == "lava").then_some(("bucket", 1))
        }
        fn count(&self, s: &S) -> u32 {
            s.1
        }
        fn resized(&self, s: &S, n: u32) -> Option<S> {
            (n > 0).then_some((s.0, n))
        }
        fn same(&self, a: &S, b: &S) -> bool {
            a.0 == b.0
        }
        fn max(&self, _: &S) -> u32 {
            64
        }
        fn sponge_swap(&self, input: &S, fuel: &S) -> Option<S> {
            (input.0 == "sponge" && fuel.0 == "bucket").then_some(("water", 1))
        }
    }

    const R: Rates = Rates {
        speed: 1.0,
        fuel: 1.0,
        burn_modifier: 1.0,
    };

    #[test]
    fn a_furnace_lights_burns_fuel_and_smelts_one_item() {
        let mut c: Cook<S> = Cook::default();
        c.slots[INPUT] = Some(("ore", 2));
        c.slots[FUEL] = Some(("coal", 1));
        c.tick(&Fake { now: 100 }, &R);
        assert!(c.cooking);
        assert_eq!(c.slots[FUEL], None, "one coal is used up");
        assert_eq!((c.burn_finish, c.burn_total), (1700, 1600));
        assert_eq!((c.cook_finish, c.cook_total), (300, 200));
        c.tick(&Fake { now: 299 }, &R);
        assert_eq!(c.slots[OUTPUT], None);
        c.tick(&Fake { now: 300 }, &R);
        assert_eq!(c.slots[OUTPUT], Some(("ingot", 1)));
        assert_eq!(c.slots[INPUT], Some(("ore", 1)));
        assert_eq!(c.cook_finish, 500, "the next item starts at once");
    }

    #[test]
    fn speed_shortens_cooking_and_fuel_efficiency_lengthens_the_burn() {
        let mut c: Cook<S> = Cook::default();
        c.slots[INPUT] = Some(("ore", 1));
        c.slots[FUEL] = Some(("coal", 1));
        let r = Rates {
            speed: 2.0,
            fuel: 1.5,
            burn_modifier: 0.5,
        };
        c.tick(&Fake { now: 100 }, &r);
        assert_eq!(c.cook_total, 100);
        // 1600 * 0.5 = 800, times 1.5 over speed 2 is 600.
        assert_eq!(c.burn_total, 600);
    }

    #[test]
    fn a_full_output_stops_the_furnace_and_a_bucket_stays_behind() {
        let mut c: Cook<S> = Cook::default();
        c.slots[INPUT] = Some(("ore", 1));
        c.slots[FUEL] = Some(("lava", 1));
        c.slots[OUTPUT] = Some(("ingot", 64));
        assert!(!c.tick(&Fake { now: 100 }, &R));
        assert_eq!(
            c.slots[FUEL],
            Some(("lava", 1)),
            "nothing burns for nothing"
        );
        c.slots[OUTPUT] = Some(("ingot", 63));
        c.tick(&Fake { now: 100 }, &R);
        assert_eq!(c.slots[FUEL], Some(("bucket", 1)));
    }

    #[test]
    fn progress_drains_once_the_fuel_is_gone() {
        let mut c: Cook<S> = Cook::default();
        c.slots[INPUT] = Some(("ore", 1));
        c.slots[FUEL] = Some(("log", 1));
        c.tick(&Fake { now: 100 }, &R);
        assert_eq!(c.burn_finish, 400);
        c.tick(&Fake { now: 200 }, &R);
        // Burnt out at 401: the cooking clock runs backwards, two ticks a tick.
        c.tick(&Fake { now: 401 }, &R);
        assert!(c.cooking);
        assert_eq!(c.cook_finish, 302);
        for t in 403..900 {
            c.tick(&Fake { now: t }, &R);
        }
        assert!(!c.cooking, "the progress ran out");
    }

    #[test]
    fn a_wet_sponge_dries_into_a_water_bucket() {
        let mut c: Cook<S> = Cook::default();
        c.slots[INPUT] = Some(("sponge", 1));
        c.burn_finish = 10_000;
        c.cooking = true;
        c.cook_finish = 0;
        c.slots[FUEL] = Some(("bucket", 1));
        c.tick(&Fake { now: 5 }, &R);
        assert_eq!(c.slots[OUTPUT], Some(("sponge_dry", 1)));
        assert_eq!(c.slots[FUEL], Some(("water", 1)));
    }

    #[test]
    fn an_automatic_upgrade_pulls_cooks_and_pushes_back() {
        let env = Fake { now: 0 };
        let mut c: Cook<S> = Cook::default();
        let mut store: Vec<Option<S>> =
            vec![Some(("dirt", 5)), Some(("ore", 3)), Some(("coal", 2)), None];
        let mut cd = Auto::default();
        let limit = |_: &S| 64;
        let input = |s: &S| s.0 == "ore" || s.0 == "log";
        let fuel = |s: &S| s.0 == "coal" || s.0 == "log";
        assert!(auto_tick(
            &env,
            &R,
            &mut c,
            &mut store,
            &limit,
            &Accept {
                input: &input,
                fuel: &fuel
            },
            &mut cd
        ));
        assert_eq!(store[1], None, "the ore went into the input slot");
        assert_eq!(store[2], None, "and the coal into the fuel slot");
        assert!(c.cooking, "and it started cooking in the same tick");
        // The output goes back once there is some.
        c.slots[OUTPUT] = Some(("ingot", 2));
        let env = Fake { now: 1 };
        let mut cd = Auto::default();
        auto_tick(
            &env,
            &R,
            &mut c,
            &mut store,
            &limit,
            &Accept {
                input: &input,
                fuel: &fuel,
            },
            &mut cd,
        );
        assert_eq!(c.slots[OUTPUT], None);
        assert!(store.iter().flatten().any(|s| *s == ("ingot", 2)));
    }

    #[test]
    fn nothing_to_pull_waits_sixty_ticks_per_part_and_ten_overall() {
        let env = Fake { now: 0 };
        let mut c: Cook<S> = Cook::default();
        let mut store: Vec<Option<S>> = vec![Some(("dirt", 5))];
        let mut cd = Auto::default();
        let limit = |_: &S| 64;
        let input = |s: &S| s.0 == "ore" || s.0 == "log";
        let fuel = |s: &S| s.0 == "coal" || s.0 == "log";
        assert!(!auto_tick(
            &env,
            &R,
            &mut c,
            &mut store,
            &limit,
            &Accept {
                input: &input,
                fuel: &fuel
            },
            &mut cd
        ));
        assert_eq!((cd.out, cd.fuel, cd.input), (60, 60, 60));
        assert_eq!(cd.until, 0, "the part cooldowns still count down");
        auto_tick(
            &Fake { now: 1 },
            &R,
            &mut c,
            &mut store,
            &limit,
            &Accept {
                input: &input,
                fuel: &fuel,
            },
            &mut cd,
        );
        assert_eq!((cd.out, cd.fuel, cd.input), (59, 59, 59));
    }

    #[test]
    fn fuel_that_burns_no_more_leaves_the_furnace() {
        let env = Fake { now: 0 };
        let mut c: Cook<S> = Cook::default();
        c.slots[FUEL] = Some(("bucket", 1));
        let mut store: Vec<Option<S>> = vec![None, None];
        let mut cd = Auto::default();
        let limit = |_: &S| 64;
        let input = |s: &S| s.0 == "ore" || s.0 == "log";
        let fuel = |s: &S| s.0 == "coal" || s.0 == "log";
        auto_tick(
            &env,
            &R,
            &mut c,
            &mut store,
            &limit,
            &Accept {
                input: &input,
                fuel: &fuel,
            },
            &mut cd,
        );
        assert_eq!(c.slots[FUEL], None);
        assert_eq!(store[0], Some(("bucket", 1)));
    }
}
