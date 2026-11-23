//! What upgrades do while their storage is carried (ITickableUpgrade, IPickupResponseUpgrade,
//! IItemHandlerInteractionUpgrade): every backpack in a player's inventory, armour, off hand or trinkets back slot
//! works (Homestead's `onlyWornBackpackTriggersUpgrades = false`).
//! - pickup: a ground stack the player touches goes into the backpack (also what vanilla already put in the
//!   inventory, moved over); magnet: stacks within 3 blocks (advanced 5) are taken, and experience orbs pulled;
//! - void: matching picked-up items are destroyed (or only what doesn't fit, `shouldVoidOverflow`);
//! - feeding: food from the backpack when hungry enough for it (nutrition fits the missing hunger);
//! - compacting: 2x2 (advanced also 3x3) recipes that can be crafted back, e.g. nuggets into ingots;
//! - refill: listed items in the player's inventory are topped up from the backpack;
//! - restock / deposit: a sneak-click on an inventory block with the backpack in hand.
use mod_api::{HostTransaction, Identifier, ItemStack, PlayerRef, RecipeInput, ServerHost};

use crate::contents::{self, Contents};
use crate::places::{self, Loc};
use crate::storage_items::{self, StorageItem};
use crate::upgrades::{self, Kind, Upgrade};

/// A storage the player carries: where it is, the stack and its contents.
pub struct Carried {
    pub loc: Loc,
    pub stack: ItemStack,
    pub kind: &'static StorageItem,
    pub contents: Contents,
    pub contents_changed: bool,
}

impl Carried {
    pub fn ups(&self, host: &dyn mod_api::Host) -> Vec<(ItemStack, Upgrade)> {
        self.contents
            .upgrades
            .iter()
            .flatten()
            .filter(|u| upgrades::flag(u, "enabled", true))
            .filter_map(|u| upgrades::of(host, u).map(|k| (u.clone(), k)))
            .collect()
    }

    pub fn has(&self, host: &dyn mod_api::Host, kind: Kind) -> bool {
        self.ups(host).iter().any(|(_, u)| u.kind == kind)
    }

    /// Writes the contents back where the storage is (if it is still there).
    pub fn save(&mut self, host: &mut dyn ServerHost, uuid: u128) -> bool {
        let Some(id) = contents::uuid(&self.stack) else {
            let mut st = self.stack.clone();
            self.contents.write(&*host, &mut st, self.kind);
            return places::write(host, uuid, &self.loc, Some(st));
        };
        let Some(loc) = places::find(host, uuid, &self.loc, id) else {
            return false;
        };
        let Some(mut st) = places::read(host, uuid, &loc) else {
            return false;
        };
        self.contents.write(&*host, &mut st, self.kind);
        self.stack = st.clone();
        places::write(host, uuid, &loc, Some(st))
    }
}

/// The storages a player carries that have upgrades.
pub fn carried(host: &mut dyn ServerHost, uuid: u128) -> Vec<Carried> {
    let mut out = Vec::new();
    let Some(player) = host.player(uuid) else {
        return out;
    };
    let mut locs: Vec<(Loc, ItemStack)> = Vec::new();
    if let Some(st) = places::worn(host, uuid, places::BACK) {
        locs.push((Loc::Worn(places::BACK.into()), st));
    }
    for (slot, st) in places::inventory(host, player).into_iter().enumerate() {
        if let Some(st) = st {
            locs.push((Loc::Inv(slot as u32), st));
        }
    }
    for (loc, st) in locs {
        let Some(kind) = storage_items::by_item(&contents::item_id(&*host, &st)) else {
            continue;
        };
        if kind.family != storage_items::Family::Backpack {
            continue;
        }
        let c = Contents::read(&*host, &st, kind);
        if c.upgrades.iter().all(Option::is_none) {
            continue;
        }
        out.push(Carried {
            loc,
            stack: st,
            kind,
            contents: c,
            contents_changed: false,
        });
    }
    out
}

/// IPickupResponseUpgrade.pickup with the void upgrade's IInsertResponseUpgrade: how many of `stack` the storage
/// takes (voided ones count as taken). `pull` is the magnet's own pull (its filter, `pickupItems`).
pub fn accept(host: &dyn mod_api::Host, c: &mut Carried, stack: &ItemStack, magnet: bool) -> u32 {
    let ups = c.ups(host);
    let takes = ups.iter().any(|(u, k)| match k.kind {
        Kind::Pickup => upgrades::matches(host, u, k, stack),
        Kind::Magnet => {
            (!magnet || upgrades::flag(u, "pickupItems", true))
                && upgrades::matches(host, u, k, stack)
        }
        _ => false,
    });
    if !takes || !crate::menu::may_store(host, c.kind, stack) {
        return 0;
    }
    let void = ups
        .iter()
        .find(|(u, k)| k.kind == Kind::Void && upgrades::matches(host, u, k, stack));
    let n = stack.count();
    if let Some((u, k)) = void
        && !upgrades::void_overflow(k, u)
    {
        return n;
    }
    let went = c.contents.add(host, c.kind, stack, n);
    if went < n && void.is_some() { n } else { went }
}

fn player_box(
    host: &dyn mod_api::Host,
    player: PlayerRef,
    r: f64,
    up: f64,
) -> Option<(String, [f64; 3], [f64; 3])> {
    let p = player.entity().position(host)?;
    let dim = player.dimension(host)?.to_string();
    Some((
        dim,
        [p[0] - r, p[1] - up, p[2] - r],
        [p[0] + r, p[1] + 1.8 + up, p[2] + r],
    ))
}

/// Takes ground stacks within the box into the carried storages; true when something changed.
fn take_items(
    host: &mut dyn ServerHost,
    uuid: u128,
    list: &mut [Carried],
    r: f64,
    magnet_only: bool,
) -> bool {
    let Some(player) = host.player(uuid) else {
        return false;
    };
    let Some((dim, min, max)) = player_box(&*host, player, r, if magnet_only { r } else { 0.5 })
    else {
        return false;
    };
    let Some(level) = host.level(&dim) else {
        return false;
    };
    let Some(ents) = crate::storage::entities_in(&*host, level, min, max) else {
        return false;
    };
    let mut changed = false;
    for e in ents {
        let Some(item) = host.item_entity(e) else {
            continue;
        };
        if item.pickup_delay(&*host).unwrap_or(0) > 0 {
            continue;
        }
        let Some(orig) = item.with_stack(&*host, |v| v.project()).flatten() else {
            continue;
        };
        let mut left = orig.count();
        for c in list.iter_mut() {
            if left == 0 {
                break;
            }
            let has_magnet = c.has(&*host, Kind::Magnet);
            let has_pickup = c.has(&*host, Kind::Pickup);
            if (magnet_only && !has_magnet) || (!magnet_only && !(has_pickup || has_magnet)) {
                continue;
            }
            let Some(offer) = contents::with_count(&orig, left) else {
                break;
            };
            let went = accept(&*host, c, &offer, magnet_only);
            if went > 0 {
                left -= went;
                c.contents_changed = true;
                changed = true;
            }
        }
        if left == orig.count() {
            continue;
        }
        match contents::with_count(&orig, left) {
            None => {
                let _ = item.entity().discard(host);
            }
            Some(rest) => {
                let _ = item.set_stack(host, rest);
            }
        }
    }
    changed
}

/// MagnetUpgradeWrapper's experience pull: orbs within range move to the player.
fn pull_xp(host: &mut dyn ServerHost, uuid: u128, r: f64) {
    let Some(player) = host.player(uuid) else {
        return;
    };
    let Some((dim, min, max)) = player_box(&*host, player, r, r) else {
        return;
    };
    let Some(at) = player.entity().position(&*host) else {
        return;
    };
    for orb in host.experience_orbs(&dim, min, max) {
        if let Some(e) = host.entity(orb.id) {
            let _ = e.teleport_to(host, at, None);
        }
    }
}

/// FeedingUpgradeWrapper.tick: one food when the missing hunger is at least its nutrition (half of it for the
/// advanced upgrade's default), or when hurt and hungry at all.
fn feed(host: &mut dyn ServerHost, uuid: u128, c: &mut Carried) -> bool {
    let Some(player) = host.player(uuid) else {
        return false;
    };
    let Some(food) = player.food(&*host) else {
        return false;
    };
    let hunger = 20 - food.level;
    if hunger <= 0 {
        return false;
    }
    let ups = c.ups(&*host);
    let Some((up, k)) = ups.iter().find(|(_, k)| k.kind == Kind::Feeding).cloned() else {
        return false;
    };
    let visible = c.contents.visible(&*host, c.kind);
    for (i, st) in visible.into_iter().enumerate() {
        let Some(st) = st else {
            continue;
        };
        let Some((n, sat)) = upgrades::food(&*host, &st) else {
            continue;
        };
        if !upgrades::matches(&*host, &up, &k, &st) {
            continue;
        }
        let need = if k.advanced { (n + 1) / 2 } else { n };
        if hunger < need {
            continue;
        }
        if !player.feed(host, n, sat) {
            return false;
        }
        c.contents.take_from(&*host, c.kind, i, 1);
        if let Some(rem) = upgrades::use_remainder(&*host, &st) {
            let went = c.contents.add(&*host, c.kind, &rem, rem.count());
            if let Some(rest) = rem.clone().shrink(went) {
                places::give_or_drop(host, player, &rest);
            }
        }
        if let Some(pos) = player.entity().position(&*host) {
            let _ = player.play_sound(
                host,
                "minecraft:entity.generic.eat",
                mod_api::SoundSource::Players,
                0.5,
                1.0,
            );
            let _ = pos;
        }
        return true;
    }
    false
}

/// CompactingUpgradeWrapper: an item whose 2x2 (or, advanced, 3x3) recipe gives one result that crafts back into as
/// many of it (`shouldCompactNonUncraftable` off), compacted while there are enough and the result fits.
fn compact(host: &mut dyn ServerHost, uuid: u128, c: &mut Carried) -> bool {
    let Some(player) = host.player(uuid) else {
        return false;
    };
    let Some(dim) = player.dimension(&*host).map(|d| d.to_string()) else {
        return false;
    };
    let Some(level) = host.level(&dim) else {
        return false;
    };
    compact_contents(&*host, level, c.kind, &mut c.contents)
}

/// The tank, battery, pump and XP pump upgrades of a carried storage, every tick.
fn liquids(host: &mut dyn ServerHost, uuid: u128, c: &mut Carried) -> bool {
    let Some(id) = contents::uuid(&c.stack) else {
        return false;
    };
    let Some(player) = host.player(uuid) else {
        return false;
    };
    let Some(dim) = player.dimension(&*host).map(|d| d.to_string()) else {
        return false;
    };
    let (Some(level), Some(pos)) = (host.level(&dim), player.entity().block_position(&*host))
    else {
        return false;
    };
    let site = crate::fluids::Site {
        level,
        pos,
        id,
        carrier: Some(player),
    };
    crate::fluids::run(host, &site, c.kind, &mut c.contents)
}

/// The cooking upgrades of a carried storage, every tick.
fn cook(host: &mut dyn ServerHost, uuid: u128, c: &mut Carried) -> bool {
    let Some(id) = contents::uuid(&c.stack) else {
        return false;
    };
    let Some(player) = host.player(uuid) else {
        return false;
    };
    let Some(dim) = player.dimension(&*host).map(|d| d.to_string()) else {
        return false;
    };
    let Some(level) = host.level(&dim) else {
        return false;
    };
    crate::cooking::run(&*host, level, id, c.kind, &mut c.contents)
}

/// The compacting upgrade's work on `contents` (in a carried or a placed storage).
pub fn compact_contents(
    host: &dyn mod_api::Host,
    level: mod_api::LevelRef,
    kind: &StorageItem,
    contents: &mut Contents,
) -> bool {
    let ups: Vec<(ItemStack, Upgrade)> = contents
        .upgrades
        .iter()
        .flatten()
        .filter_map(|u| upgrades::of(host, u).map(|k| (u.clone(), k)))
        .collect();
    let Some((up, k)) = ups
        .iter()
        .find(|(_, k)| k.kind == Kind::Compacting)
        .cloned()
    else {
        return false;
    };
    let Ok(crafting) = Identifier::parse("minecraft:crafting") else {
        return false;
    };
    let mut changed = false;
    let mut seen: Vec<ItemStack> = Vec::new();
    for st in contents.slots.clone().into_iter().flatten() {
        if seen.iter().any(|s| contents::same(s, &st)) {
            continue;
        }
        seen.push(st.clone());
        if !upgrades::matches(host, &up, &k, &st) {
            continue;
        }
        for size in if k.advanced {
            vec![3u32, 2]
        } else {
            vec![2u32]
        } {
            let n = size * size;
            let one = contents::with_count(&st, 1).unwrap_or(st.clone());
            let input = RecipeInput::Crafting {
                width: size,
                height: size,
                slots: vec![Some(one.clone()); n as usize],
            };
            let Ok(Some(m)) = level.recipe_for(host, &crafting, &input) else {
                continue;
            };
            if m.output.count() != 1
                || m.remainders.iter().any(Option::is_some)
                || contents::same(&m.output, &st)
            {
                continue;
            }
            // Crafts back into as many as went in.
            let back = RecipeInput::Crafting {
                width: 1,
                height: 1,
                slots: vec![Some(m.output.clone())],
            };
            match level.recipe_for(host, &crafting, &back) {
                Ok(Some(b)) if contents::same(&b.output, &one) && b.output.count() == n => {}
                _ => continue,
            }
            let mult = upgrades::multiplier(host, kind, &contents.upgrades);
            let lim = upgrades::limit(host, &m.output, mult);
            loop {
                if contents::count_of(&contents.slots, &st) < u64::from(n) {
                    break;
                }
                let mut trial = contents.slots.clone();
                contents::extract(&mut trial, &st, n);
                if contents::insert(&mut trial, &m.output, 1, lim) != 1 {
                    break;
                }
                contents.slots = trial;
                changed = true;
            }
            break;
        }
    }
    changed
}

/// RefillUpgradeWrapper: each listed item the player has in the inventory is topped up to a full stack.
fn refill(host: &mut dyn ServerHost, uuid: u128, c: &mut Carried) -> bool {
    let ups = c.ups(&*host);
    let Some((up, k)) = ups.iter().find(|(_, k)| k.kind == Kind::Refill).cloned() else {
        return false;
    };
    let Some(player) = host.player(uuid) else {
        return false;
    };
    let wanted: Vec<ItemStack> = upgrades::filters(&*host, &up, k.filters)
        .into_iter()
        .flatten()
        .collect();
    if wanted.is_empty() {
        return false;
    }
    let inv = places::inventory(host, player);
    let mut changed = false;
    for w in wanted {
        for slot in 0..36 {
            let Some(st) = inv[slot].clone() else {
                continue;
            };
            if !st.same_item(&w) {
                continue;
            }
            let max = host.max_stack_size(st.item()).unwrap_or(64);
            if st.count() >= max {
                break;
            }
            let got = c.contents.remove(&*host, c.kind, &st, max - st.count());
            if got > 0 {
                places::set_slot(
                    host,
                    player,
                    slot as u32,
                    contents::with_count(&st, st.count() + got),
                );
                changed = true;
            }
            break;
        }
    }
    changed
}

/// One server tick for a player's carried storages. `n` counts ticks.
pub fn tick(host: &mut dyn ServerHost, uuid: u128, n: u64) -> Vec<[i32; 4]> {
    let mut list = carried(host, uuid);
    if crate::options::Options::read(&*host).only_worn {
        list.retain(|c| matches!(c.loc, Loc::Worn(_)));
    }
    if list.is_empty() {
        return Vec::new();
    }
    for c in list.iter_mut() {
        c.contents_changed = false;
    }
    // Pickup reach, every tick; the magnet's range every 2 ticks.
    let opts = crate::options::Options::read(&*host);
    take_items(host, uuid, &mut list, 1.0, false);
    if n.is_multiple_of(2) {
        let range = list
            .iter()
            .flat_map(|c| c.ups(&*host))
            .filter(|(_, k)| k.kind == Kind::Magnet)
            .map(|(_, k)| opts.magnet(k.advanced))
            .fold(0.0f64, f64::max);
        if range > 0.0 {
            take_items(host, uuid, &mut list, range, true);
            let xp = list
                .iter()
                .flat_map(|c| c.ups(&*host))
                .any(|(u, k)| k.kind == Kind::Magnet && upgrades::flag(&u, "pickupXp", true));
            if xp {
                pull_xp(host, uuid, range);
            }
        }
    }
    if n.is_multiple_of(20) {
        for c in list.iter_mut() {
            if c.has(&*host, Kind::Feeding) && feed(host, uuid, c) {
                c.contents_changed = true;
            }
            if c.has(&*host, Kind::Compacting) && compact(host, uuid, c) {
                c.contents_changed = true;
            }
        }
    }
    if n.is_multiple_of(5) {
        for c in list.iter_mut() {
            if c.has(&*host, Kind::Refill) && refill(host, uuid, c) {
                c.contents_changed = true;
            }
        }
    }
    for c in list.iter_mut() {
        let cooks = c
            .ups(&*host)
            .iter()
            .any(|(_, u)| matches!(u.kind, Kind::Cooking(_)));
        if cooks && cook(host, uuid, c) {
            c.contents_changed = true;
        }
        let pumps = c.ups(&*host).iter().any(|(_, u)| {
            matches!(
                u.kind,
                Kind::Tank | Kind::Battery | Kind::Pump | Kind::XpPump
            )
        });
        if pumps && liquids(host, uuid, c) {
            c.contents_changed = true;
        }
    }
    let mut saved = Vec::new();
    for c in list.iter_mut() {
        if c.contents_changed
            && c.save(host, uuid)
            && let Some(id) = contents::uuid(&c.stack)
        {
            saved.push(id);
        }
    }
    saved
}

/// What vanilla's pickup already put in the inventory moves over to a storage with a pickup (or magnet) upgrade
/// (the jar's pickup runs before the inventory takes the stack).
pub fn picked_up(host: &mut dyn ServerHost, uuid: u128, picked: &ItemStack) -> Vec<[i32; 4]> {
    let mut list = carried(host, uuid);
    let Some(player) = host.player(uuid) else {
        return Vec::new();
    };
    let mut left = picked.count();
    let mut saved = Vec::new();
    for c in list.iter_mut() {
        if left == 0 {
            break;
        }
        if !(c.has(&*host, Kind::Pickup) || c.has(&*host, Kind::Magnet)) {
            continue;
        }
        let Some(want) = contents::with_count(picked, left) else {
            break;
        };
        // Take what the inventory holds of it, up to what came in.
        let inv = places::inventory(host, player);
        let held: u32 = inv[..36]
            .iter()
            .flatten()
            .filter(|s| contents::same(s, picked))
            .map(|s| s.count())
            .sum();
        let Some(offer) = contents::with_count(&want, want.count().min(held)) else {
            break;
        };
        let went = accept(&*host, c, &offer, false);
        if went == 0 {
            continue;
        }
        // Remove `went` of it from the inventory, last slots first.
        let mut rest = went;
        for slot in (0..36).rev() {
            if rest == 0 {
                break;
            }
            if let Some(s) = &inv[slot]
                && contents::same(s, picked)
            {
                let take = s.count().min(rest);
                places::set_slot(host, player, slot as u32, s.clone().shrink(take));
                rest -= take;
            }
        }
        left -= went;
        if c.save(host, uuid)
            && let Some(id) = contents::uuid(&c.stack)
        {
            saved.push(id);
        }
    }
    saved
}

/// InventoryInteractionHelper: a sneak-click on an inventory block with a storage that has restock or deposit
/// upgrades. Restock fills the storage from the block (what its filter takes, by default what it already holds);
/// deposit empties the storage into the block (likewise). False when the storage has neither.
pub fn interact(
    host: &mut dyn ServerHost,
    uuid: u128,
    loc: &Loc,
    level: mod_api::LevelRef,
    pos: [i32; 3],
    face: [i32; 3],
) -> bool {
    let Some((st, kind)) = places::storage_at(host, uuid, loc) else {
        return false;
    };
    let mut c = Carried {
        loc: loc.clone(),
        contents: Contents::read(&*host, &st, kind),
        stack: st,
        kind,
        contents_changed: false,
    };
    let ups = c.ups(&*host);
    let restock: Vec<_> = ups
        .iter()
        .filter(|(_, k)| k.kind == Kind::Restock)
        .cloned()
        .collect();
    let deposit: Vec<_> = ups
        .iter()
        .filter(|(_, k)| k.kind == Kind::Deposit)
        .cloned()
        .collect();
    if restock.is_empty() && deposit.is_empty() {
        return false;
    }
    let side = dir_of(face);
    let blocked = level
        .block(&*host, pos)
        .and_then(|k| k.id(&*host).map(str::to_owned));
    if blocked.is_some_and(|id| {
        crate::options::listed(&crate::options::Lists::get(&*host).no_interaction, &id)
    }) {
        return false;
    }
    let Some(handler) = level.capability(host, &mod_api::ITEMS, pos, side) else {
        return false;
    };
    // ContentsFilterLogic with no filters: the storage's own contents decide (`filterByStorage` is the jar's
    // default for these two only when set; an empty block list matches everything, as here).
    let mut moved_in = 0;
    let mut moved_out = 0;
    let size = handler.size(host);
    for (up, k) in &restock {
        for i in 0..size {
            let Some(res) = handler.resource(host, i) else {
                continue;
            };
            let amount = handler.amount(host, i);
            let Some(stack) = res.to_stack(amount) else {
                continue;
            };
            if !upgrades::matches(&*host, up, k, &stack) {
                continue;
            }
            let mut trial = c.contents.clone();
            let fits = trial.add(&*host, c.kind, &stack, amount);
            if fits == 0 {
                continue;
            }
            let got = host
                .transaction(|host, tx| Ok::<u32, ()>(handler.extract_at(host, tx, i, &res, fits)))
                .unwrap_or(0);
            if got > 0 {
                c.contents.add(&*host, c.kind, &stack, got);
                moved_in += 1;
            }
        }
    }
    for (up, k) in &deposit {
        let visible = c.contents.visible(&*host, c.kind);
        for (i, stack) in visible.into_iter().enumerate() {
            let Some(stack) = stack else {
                continue;
            };
            if !upgrades::matches(&*host, up, k, &stack) {
                continue;
            }
            let res = mod_api::ItemResource::of(&stack);
            let n = stack.count();
            let put = host
                .transaction(|host, tx| {
                    Ok::<u32, ()>(handler.insert(host, tx, &res, n).unwrap_or(0))
                })
                .unwrap_or(0);
            if put > 0 {
                c.contents.take_from(&*host, c.kind, i, put);
                moved_out += 1;
            }
        }
    }
    if moved_in + moved_out > 0 {
        c.save(host, uuid);
    }
    let msg = match (restock.is_empty(), deposit.is_empty()) {
        (false, true) if moved_in > 0 => format!("{moved_in} stacks restocked from inventory"),
        (false, true) => "No stacks could be restocked from inventory".to_string(),
        (true, false) if moved_out > 0 => format!("{moved_out} stacks deposited to inventory"),
        (true, false) => "No stacks could be deposited to inventory".to_string(),
        _ => format!("{moved_in} stacks restocked, {moved_out} deposited"),
    };
    places::message(host, uuid, &msg);
    true
}

fn dir_of(face: [i32; 3]) -> Option<mod_api::Direction> {
    use mod_api::Direction::*;
    Some(match face {
        [0, -1, 0] => Down,
        [0, 1, 0] => Up,
        [0, 0, -1] => North,
        [0, 0, 1] => South,
        [-1, 0, 0] => West,
        [1, 0, 0] => East,
        _ => return None,
    })
}
