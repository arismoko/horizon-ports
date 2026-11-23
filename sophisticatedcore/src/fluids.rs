//! The tank, battery, pump and XP pump upgrades (TankUpgradeWrapper, BatteryUpgradeWrapper, PumpUpgradeWrapper,
//! XpPumpUpgradeWrapper). A storage holds at most one tank and one battery; the pumps and the XP pump work on the
//! tank. Amounts are millibuckets (energy in FE). The state lives in each upgrade's `custom_data` under the jar's
//! keys: `contents` (`FluidName`, `Amount`), `inventory` (the input and output item slots) and `energy` for the
//! tank and the battery; `input`, `interactWithHand`, `interactWithWorld` and `fluidFilters` for the pumps;
//! `direction`, `level`, `levelsToStore`, `levelsToTake` and `mendItems` for the XP pump.
//!
//! Experience is held in the tank as this mod's own `sophisticatedcore:xp_still` fluid, 20 mB to the point
//! (XpHelper.RATIO). It is a tank-only fluid (no block, as in the original) that travels in the `xp_bucket` item.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use mod_api::{
    Compound, Direction, FluidResource, FluidStack, Hand, Host, HostTransaction, ItemPlace,
    ItemStack, LevelRef, PlayerRef, ResourceHandler, ServerHost, Tag, Transaction,
};

use crate::contents::{self, Contents, Nbt, custom_data, put, read_items, set_custom_data};
use crate::upgrades::{self, Kind};

pub const INPUT: usize = 0;
pub const OUTPUT: usize = 1;
/// A bucket, the most a container or the world moves at once.
pub const BUCKET: u32 = 1000;
/// The fluid experience is held as.
pub const XP_FLUID: &str = "sophisticatedcore:xp_still";
/// Millibuckets to an experience point (`XpHelper.RATIO`).
pub const MB_PER_POINT: u32 = 20;

const DID_NOTHING: i64 = 40;
const HAND: i64 = 3;
const WORLD: i64 = 20;
const HANDLER: i64 = 20;
const PLAYER_RANGE: f64 = 3.0;
const WORLD_RANGE_SQ: i32 = 16;
const XP_COOLDOWN: i64 = 5;

// ---- numbers ----

/// `getNumberOfSlotRows`: twelve slots to a row in the big inventories, else nine.
pub fn rows(slots: usize) -> u32 {
    let width = if slots > 81 { 12 } else { 9 };
    slots.div_ceil(width).max(1) as u32
}

/// `getAdjustedStackMultiplier`, the stack upgrades' multiplier weighed by the upgrade's ratio.
pub fn adjusted(ratio: f64, mult: f64) -> f64 {
    1.0 + ratio * (mult - 1.0)
}

fn clamp(x: f64) -> u32 {
    if x.is_nan() {
        0
    } else {
        x.clamp(0.0, f64::from(i32::MAX)) as u32
    }
}

/// `getTankCapacity` (and the battery's `getMaxEnergyStored`).
pub fn capacity(per_row: u32, rows: u32, ratio: f64, mult: f64) -> u32 {
    clamp(f64::from(per_row) * f64::from(rows) * adjusted(ratio, mult))
}

/// The tank's and the pump's `getMaxInOut`: never under a bucket.
pub fn fluid_io(io: u32, rows: u32, adjusted: f64) -> u32 {
    clamp(f64::from(io) * f64::from(rows) * adjusted).max(BUCKET)
}

/// The battery's `getMaxInOut`.
pub fn energy_io(io: u32, rows: u32, ratio: f64, mult: f64) -> u32 {
    clamp(f64::from(io) * f64::from(rows) * adjusted(ratio, mult))
}

/// `XpHelper.getExperienceForLevel`.
pub fn xp_for_level(level: i32) -> i32 {
    match level {
        ..=0 => 0,
        1..=15 => level * (12 + level * 2) / 2,
        16..=30 => (level - 15) * (69 + (level - 15) * 5) / 2 + 315,
        _ => (level - 30) * (215 + (level - 30) * 9) / 2 + 1395,
    }
}

/// `XpHelper.getExperienceLimitOnLevel`: the points a level needs to fill.
pub fn xp_limit(level: i32) -> i32 {
    if level >= 30 {
        112 + (level - 30) * 9
    } else if level >= 15 {
        37 + (level - 15) * 5
    } else {
        7 + level * 2
    }
}

/// `XpHelper.getLevelForExperience`.
pub fn level_for(points: i32) -> i32 {
    let mut level = 0;
    while xp_for_level(level + 1) <= points {
        level += 1;
    }
    level
}

/// `XpHelper.getPlayerTotalExperience`: the points of the level plus those of the bar.
pub fn player_points(level: i32, progress: f32) -> i32 {
    xp_for_level(level) + (progress * xp_limit(level) as f32) as i32
}

/// What the XP pump does with a player at `have` (level, progress) against its target `level`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Flow {
    Store,
    Give,
    Idle,
}

/// The pump's direction names, in `AutomationDirection`'s order.
pub const DIRECTIONS: [&str; 4] = ["input", "output", "keep", "off"];

/// `XpPumpUpgradeWrapper.interactWithPlayer`'s choice.
pub fn flow(direction: &str, level: i32, player_level: i32, progress: f32) -> Flow {
    let store = matches!(direction, "input" | "keep")
        && (level < player_level || (level == player_level && progress > 0.0));
    let give = matches!(direction, "output" | "keep") && level > player_level;
    if store {
        Flow::Store
    } else if give {
        Flow::Give
    } else {
        Flow::Idle
    }
}

// ---- the tank ----

/// The fluid in the tank upgrade.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tank {
    pub fluid: Option<String>,
    pub amount: u32,
}

impl Tank {
    pub fn accepts(&self, fluid: &str) -> bool {
        self.amount == 0 || self.fluid.as_deref() == Some(fluid)
    }

    pub fn room(&self, cap: u32) -> u32 {
        cap.saturating_sub(self.amount)
    }

    /// `TankUpgradeWrapper.fill`: how much went in, at most `io` when given.
    pub fn fill(&mut self, fluid: &str, max: u32, cap: u32, io: Option<u32>) -> u32 {
        if self.amount >= cap || !self.accepts(fluid) {
            return 0;
        }
        let mut n = self.room(cap).min(max);
        if let Some(io) = io {
            n = n.min(io);
        }
        if n > 0 {
            self.fluid = Some(fluid.to_owned());
            self.amount += n;
        }
        n
    }

    /// `TankUpgradeWrapper.drain`: how much came out, at most `io` when given.
    pub fn drain(&mut self, max: u32, io: Option<u32>) -> u32 {
        if self.amount == 0 {
            return 0;
        }
        let mut n = max.min(self.amount);
        if let Some(io) = io {
            n = n.min(io);
        }
        self.amount -= n;
        if self.amount == 0 {
            self.fluid = None;
        }
        n
    }
}

/// The pump's fluid filter: an empty one lets everything through (`FluidFilterLogic.fluidMatches`).
pub fn filter_matches(filters: &[Option<String>], fluid: &str) -> bool {
    filters.iter().all(Option::is_none) || filters.iter().flatten().any(|f| f == fluid)
}

fn long(c: &Compound, k: &str) -> Option<i64> {
    match c.at(k)? {
        Tag::Long(v) => Some(*v),
        Tag::Int(v) => Some(i64::from(*v)),
        _ => None,
    }
}

pub fn read_tank(up: &ItemStack) -> Tank {
    let d = custom_data(up);
    let Some(Tag::Compound(c)) = d.at("contents") else {
        return Tank::default();
    };
    let amount = long(c, "Amount").unwrap_or(0).clamp(0, i64::from(u32::MAX)) as u32;
    match contents::get_str(c, "FluidName") {
        Some(f) if amount > 0 => Tank {
            fluid: Some(f),
            amount,
        },
        _ => Tank::default(),
    }
}

pub fn write_tank(up: &mut ItemStack, t: &Tank) {
    let mut d = custom_data(up);
    match &t.fluid {
        Some(f) if t.amount > 0 => put(
            &mut d,
            "contents",
            Tag::compound([
                ("FluidName", Tag::String(f.as_str().into())),
                ("Amount", Tag::Long(i64::from(t.amount))),
            ]),
        ),
        _ => {
            d.del("contents");
        }
    }
    set_custom_data(up, d);
}

pub fn read_energy(up: &ItemStack) -> u32 {
    long(&custom_data(up), "energy")
        .unwrap_or(0)
        .clamp(0, i64::from(u32::MAX)) as u32
}

pub fn write_energy(up: &mut ItemStack, e: u32) {
    let mut d = custom_data(up);
    put(&mut d, "energy", Tag::Long(i64::from(e)));
    set_custom_data(up, d);
}

/// The two item slots of a tank or battery upgrade.
pub fn read_slots(host: &dyn Host, up: &ItemStack) -> Vec<Option<ItemStack>> {
    read_items(host, custom_data(up).at("inventory"), 2)
}

pub fn write_slots(host: &dyn Host, up: &mut ItemStack, slots: &[Option<ItemStack>]) {
    let mut d = custom_data(up);
    put(&mut d, "inventory", contents::write_items(host, slots));
    set_custom_data(up, d);
}

/// A pump's settings (`input`, `interactWithHand`, `interactWithWorld`); the hand and world defaults are the
/// upgrade's (off for the basic pump, on for the advanced).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pump {
    pub input: bool,
    pub hand: bool,
    pub world: bool,
}

pub fn read_pump(up: &ItemStack, advanced: bool) -> Pump {
    Pump {
        input: upgrades::flag(up, "input", true),
        hand: upgrades::flag(up, "interactWithHand", advanced),
        world: upgrades::flag(up, "interactWithWorld", advanced),
    }
}

pub fn read_pump_filters(host: &dyn Host, up: &ItemStack, n: usize) -> Vec<Option<String>> {
    let mut out = vec![None; n];
    if let Some(Tag::List(list)) = custom_data(up).at("fluidFilters") {
        for (i, t) in list.iter().take(n).enumerate() {
            if let Tag::Compound(c) = t {
                out[i] = contents::get_str(c, "FluidName").filter(|f| host.fluid_key(f).is_some());
            }
        }
    }
    out
}

pub fn write_pump_filters(up: &mut ItemStack, filters: &[Option<String>]) {
    let mut d = custom_data(up);
    let list = filters
        .iter()
        .map(|f| match f {
            Some(f) => Tag::compound([
                ("FluidName", Tag::String(f.as_str().into())),
                ("Amount", Tag::Int(1)),
            ]),
            None => Tag::Compound(Compound::new()),
        })
        .collect();
    put(&mut d, "fluidFilters", Tag::List(list));
    set_custom_data(up, d);
}

/// The XP pump's settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Xp {
    pub direction: usize,
    pub level: i32,
    pub store: i32,
    pub take: i32,
    pub mend: bool,
}

pub fn read_xp(up: &ItemStack) -> Xp {
    let d = custom_data(up);
    let name = contents::get_str(&d, "direction");
    Xp {
        direction: name
            .and_then(|n| DIRECTIONS.iter().position(|x| *x == n))
            .unwrap_or(0),
        level: contents::get_int(&d, "level").unwrap_or(10),
        store: contents::get_int(&d, "levelsToStore").unwrap_or(1),
        take: contents::get_int(&d, "levelsToTake").unwrap_or(1),
        mend: contents::get_bool(&d, "mendItems").unwrap_or(true),
    }
}

pub fn write_xp(up: &mut ItemStack, x: &Xp) {
    let mut d = custom_data(up);
    put(
        &mut d,
        "direction",
        Tag::String(DIRECTIONS[x.direction % 4].into()),
    );
    put(&mut d, "level", Tag::Int(x.level));
    put(&mut d, "levelsToStore", Tag::Int(x.store));
    put(&mut d, "levelsToTake", Tag::Int(x.take));
    put(&mut d, "mendItems", Tag::Byte(i8::from(x.mend)));
    set_custom_data(up, d);
}

// ---- limits of a storage ----

/// What a storage's tank, battery and pumps may hold and move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    pub tank_cap: u32,
    pub tank_io: u32,
    pub pump_io: u32,
    pub battery_cap: u32,
    pub battery_io: u32,
}

pub fn limits(host: &dyn Host, kind: &crate::storage_items::StorageItem, c: &Contents) -> Limits {
    limits_with(host, kind, c.slots.len(), &c.upgrades)
}

pub fn limits_with(
    host: &dyn Host,
    kind: &crate::storage_items::StorageItem,
    slots: usize,
    ups: &[Option<ItemStack>],
) -> Limits {
    let l = crate::options::tune().liquids;
    let m = upgrades::multiplier(host, kind, ups);
    let r = rows(slots);
    Limits {
        tank_cap: capacity(l.tank_per_row, r, l.tank_ratio, m),
        tank_io: fluid_io(l.tank_io, r, adjusted(l.tank_ratio, m)),
        pump_io: fluid_io(
            l.pump_io,
            r,
            1.0 + (l.pump_ratio * (m - 1.0)).trunc().max(0.0),
        ),
        battery_cap: capacity(l.battery_per_row, r, l.battery_ratio, m),
        battery_io: energy_io(l.battery_io, r, l.battery_ratio, m),
    }
}

/// The first upgrade of a storage that is a tank (a storage takes only one).
pub fn tank_index(host: &dyn Host, c: &Contents) -> Option<usize> {
    c.upgrades.iter().position(|u| {
        u.as_ref()
            .and_then(|u| upgrades::of(host, u))
            .is_some_and(|u| u.kind == Kind::Tank)
    })
}

// ---- moving fluid ----

fn fluid_id(host: &dyn Host, r: &FluidResource) -> Option<String> {
    r.to_stack(1)
        .key()
        .and_then(|k| k.id(host).map(str::to_owned))
}

fn resource(host: &dyn Host, id: &str) -> Option<FluidResource> {
    let key = host.fluid_key(id)?;
    Some(FluidResource::of(&FluidStack::new(key, 1)))
}

type Fluids = dyn ResourceHandler<FluidResource>;

/// Takes up to `max` of one fluid out of `src` into the tank, in `tx`: how much. The tank's limit `io` and its
/// room bound it, and `allow` picks the fluids.
fn pull(
    host: &mut dyn ServerHost,
    tx: &Transaction,
    src: &Fluids,
    t: &mut Tank,
    (cap, io): (u32, u32),
    max: u32,
    allow: &dyn Fn(&str) -> bool,
) -> u32 {
    for i in 0..src.size(host) {
        let Some(res) = src.resource(host, i) else {
            continue;
        };
        let have = src.amount(host, i);
        let Some(id) = fluid_id(&*host, &res) else {
            continue;
        };
        if have == 0 || !allow(&id) || !t.accepts(&id) {
            continue;
        }
        let want = have.min(max).min(t.room(cap)).min(io);
        if want == 0 {
            continue;
        }
        let got = src.extract_at(host, tx, i, &res, want);
        if got > 0 {
            return t.fill(&id, got, cap, None);
        }
    }
    0
}

/// Puts up to `max` of the tank's fluid into `dst`, in `tx`: how much.
fn push(
    host: &mut dyn ServerHost,
    tx: &Transaction,
    dst: &Fluids,
    t: &mut Tank,
    io: u32,
    max: u32,
    allow: &dyn Fn(&str) -> bool,
) -> u32 {
    let Some(id) = t.fluid.clone() else {
        return 0;
    };
    if !allow(&id) {
        return 0;
    }
    let Some(res) = resource(&*host, &id) else {
        return 0;
    };
    let n = t.amount.min(max).min(io);
    if n == 0 {
        return 0;
    }
    let put = dst.insert(host, tx, &res, n).unwrap_or(0);
    t.drain(put, None)
}

/// The tank's own container slots: the input slot's fluid goes into the tank, the output slot is filled from it.
/// True when something moved.
fn tank_tick(host: &mut dyn ServerHost, up: &mut ItemStack, t: &mut Tank, l: &Limits) -> bool {
    let mut slots = read_slots(&*host, up);
    let mut did = false;
    for (i, filling) in [(INPUT, false), (OUTPUT, true)] {
        let Some(st) = slots[i].clone() else {
            continue;
        };
        let before = t.clone();
        let moved = host.transaction(|host, tx| {
            let place = host
                .scratch_slots(tx, st.clone())
                .map_err(|_| ())?
                .ok_or(())?;
            let cap = place.capability(host, &mod_api::ITEM_FLUIDS).ok_or(())?;
            let n = if filling {
                push(host, tx, &*cap, t, l.tank_io, BUCKET, &|_| true)
            } else {
                pull(host, tx, &*cap, t, (l.tank_cap, l.tank_io), BUCKET, &|_| {
                    true
                })
            };
            if n == 0 {
                return Err(());
            }
            let (a, b) = host.scratch_take_slots(tx, &place).ok_or(())?;
            Ok::<_, ()>(a.or(b))
        });
        match moved {
            Ok(new) => {
                slots[i] = new;
                did = true;
            }
            Err(()) => *t = before,
        }
    }
    if did {
        write_slots(&*host, up, &slots);
    }
    did
}

/// Energy moved between the battery and an item in one of its slots, in one transaction: how much.
fn item_energy(
    host: &mut dyn ServerHost,
    st: &ItemStack,
    from_item: bool,
    max: u32,
) -> Option<(Option<ItemStack>, u32)> {
    host.transaction(|host, tx| {
        let place = host
            .scratch_stack(tx, st.clone())
            .map_err(|_| ())?
            .ok_or(())?;
        let cap = place.capability(host, &mod_api::ITEM_ENERGY).ok_or(())?;
        let n = if from_item {
            cap.extract(host, tx, max)
        } else {
            cap.insert(host, tx, max)
        };
        if n == 0 {
            return Err(());
        }
        let back = host.scratch_take(tx, &place);
        Ok::<_, ()>((back, n))
    })
    .ok()
}

/// BatteryUpgradeWrapper.tick: the input slot charges the battery, the output slot and the neighbours drain it.
fn battery_tick(host: &mut dyn ServerHost, up: &mut ItemStack, site: &Site, l: &Limits) -> bool {
    let mut e = read_energy(up);
    let before = e;
    let mut slots = read_slots(&*host, up);
    let mut slots_changed = false;
    if e < l.battery_cap
        && let Some(st) = slots[INPUT].clone()
    {
        let want = l.battery_cap.saturating_sub(e).min(l.battery_io);
        if let Some((back, n)) = item_energy(host, &st, true, want) {
            e += n;
            slots[INPUT] = back;
            slots_changed = true;
        }
    }
    if e > 0
        && let Some(st) = slots[OUTPUT].clone()
        && let Some((back, n)) = item_energy(host, &st, false, e.min(l.battery_io))
    {
        e -= n;
        slots[OUTPUT] = back;
        slots_changed = true;
    }
    if e > 0 {
        for dir in DIRECTIONS_6 {
            if e == 0 {
                break;
            }
            let at = add(site.pos, dir.1);
            let Some(h) = site
                .level
                .capability(host, &mod_api::ENERGY, at, Some(opposite(dir.0)))
            else {
                continue;
            };
            let want = e.min(l.battery_io);
            let put = host
                .transaction(|host, tx| Ok::<u32, ()>(h.insert(host, tx, want)))
                .unwrap_or(0);
            e -= put.min(e);
        }
    }
    if slots_changed {
        write_slots(&*host, up, &slots);
    }
    if e != before {
        write_energy(up, e);
    }
    slots_changed || e != before
}

const DIRECTIONS_6: [(Direction, [i32; 3]); 6] = [
    (Direction::Down, [0, -1, 0]),
    (Direction::Up, [0, 1, 0]),
    (Direction::North, [0, 0, -1]),
    (Direction::South, [0, 0, 1]),
    (Direction::West, [-1, 0, 0]),
    (Direction::East, [1, 0, 0]),
];

fn opposite(d: Direction) -> Direction {
    match d {
        Direction::Down => Direction::Up,
        Direction::Up => Direction::Down,
        Direction::North => Direction::South,
        Direction::South => Direction::North,
        Direction::West => Direction::East,
        _ => Direction::West,
    }
}

fn add(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

// ---- running the upgrades ----

/// Where a storage's upgrades tick: the level, the block (a placed storage's, or a carrying player's) and the
/// storage's contents id.
pub struct Site {
    pub level: LevelRef,
    pub pos: [i32; 3],
    pub id: [i32; 4],
    /// The player carrying the storage; none for a placed one.
    pub carrier: Option<PlayerRef>,
}

#[derive(Default)]
struct Cooldown {
    until: i64,
    last_hand: i64,
}

static COOLDOWNS: Mutex<Option<HashMap<([i32; 4], usize), Cooldown>>> = Mutex::new(None);

/// One tick of every tank, battery, pump and XP pump of a storage. True when its contents changed.
pub fn run(
    host: &mut dyn ServerHost,
    site: &Site,
    kind: &crate::storage_items::StorageItem,
    c: &mut Contents,
) -> bool {
    if !c.upgrades.iter().flatten().any(|u| {
        upgrades::of(&*host, u).is_some_and(|u| {
            matches!(
                u.kind,
                Kind::Tank | Kind::Battery | Kind::Pump | Kind::XpPump
            )
        })
    }) {
        return false;
    }
    let l = limits(&*host, kind, c);
    let Some(now) = site.level.game_time(&*host) else {
        return false;
    };
    let ti = tank_index(&*host, c);
    let mut tank = ti
        .and_then(|i| c.upgrades[i].as_ref())
        .map(read_tank)
        .unwrap_or_default();
    let tank_before = tank.clone();
    let mut changed = false;
    for j in 0..c.upgrades.len() {
        let Some(mut up) = c.upgrades[j].clone() else {
            continue;
        };
        let Some(u) = upgrades::of(&*host, &up) else {
            continue;
        };
        if !upgrades::flag(&up, "enabled", true) && !matches!(u.kind, Kind::Tank | Kind::Battery) {
            continue;
        }
        let key = (site.id, j);
        let (until, last_hand) = {
            let g = COOLDOWNS.lock().unwrap_or_else(|e| e.into_inner());
            g.as_ref()
                .and_then(|m| m.get(&key))
                .map_or((0, -1), |c| (c.until, c.last_hand))
        };
        let mut next: Option<(i64, i64)> = None;
        match u.kind {
            Kind::Tank if Some(j) == ti => {
                if now >= until {
                    if tank_tick(host, &mut up, &mut tank, &l) {
                        next = Some((
                            now + i64::from(crate::options::tune().liquids.tank_cooldown),
                            last_hand,
                        ));
                        changed = true;
                    }
                    // The tank's contents are written at the end; its slots now.
                    c.upgrades[j] = Some(up);
                }
            }
            Kind::Battery => {
                if battery_tick(host, &mut up, site, &l) {
                    c.upgrades[j] = Some(up);
                    changed = true;
                }
            }
            Kind::Pump if ti.is_some() && now >= until => {
                let (cd, hand) =
                    pump_tick(host, site, &up, u.advanced, &mut tank, &l, now, last_hand);
                next = Some((now + cd, hand));
            }
            Kind::XpPump if ti.is_some() && now >= until => {
                xp_tick(host, site, &up, &mut tank, &l);
                next = Some((now + XP_COOLDOWN, last_hand));
            }
            _ => {}
        }
        if let Some((until, last_hand)) = next {
            COOLDOWNS
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get_or_insert_with(HashMap::new)
                .insert(key, Cooldown { until, last_hand });
        }
    }
    if let Some(i) = ti
        && tank != tank_before
        && let Some(mut up) = c.upgrades[i].clone()
    {
        write_tank(&mut up, &tank);
        c.upgrades[i] = Some(up);
        changed = true;
    }
    changed
}

fn near(host: &dyn Host, site: &Site, p: PlayerRef) -> bool {
    let level = p.dimension(host).and_then(|d| host.level(&d.to_string()));
    if level != Some(site.level) {
        return false;
    }
    let Some(at) = p.entity().position(host) else {
        return false;
    };
    (0..3).all(|i| {
        at[i] >= f64::from(site.pos[i]) - PLAYER_RANGE
            && at[i] < f64::from(site.pos[i]) + 1.0 + PLAYER_RANGE
    })
}

fn nearby(host: &dyn ServerHost, site: &Site) -> Vec<PlayerRef> {
    host.players()
        .server_players()
        .into_iter()
        .filter(|p| near(host, site, *p))
        .collect()
}

/// PumpUpgradeWrapper.tick: the cooldown to wait, and when a hand last worked.
#[allow(clippy::too_many_arguments)]
fn pump_tick(
    host: &mut dyn ServerHost,
    site: &Site,
    up: &ItemStack,
    advanced: bool,
    tank: &mut Tank,
    l: &Limits,
    now: i64,
    last_hand: i64,
) -> (i64, i64) {
    let set = read_pump(up, advanced);
    let filters = read_pump_filters(&*host, up, crate::options::tune().liquids.pump_filters);
    let allow = |f: &str| filter_matches(&filters, f);
    let cap = l.tank_cap;
    if set.hand {
        let mut players = nearby(&*host, site);
        if let Some(p) = site.carrier {
            players.retain(|q| q.uuid() != p.uuid());
            players.insert(0, p);
        }
        for p in players {
            for hand in Hand::BOTH {
                let place = ItemPlace::PlayerHand {
                    player: p.uuid(),
                    hand,
                };
                if place.amount(host) != 1 {
                    continue;
                }
                let Some(h) = place.capability(host, &mod_api::ITEM_FLUIDS) else {
                    continue;
                };
                let before = tank.clone();
                let moved = host.transaction(|host, tx| {
                    let n = if set.input {
                        pull(
                            host,
                            tx,
                            &*h,
                            tank,
                            (cap, l.pump_io.min(l.tank_io)),
                            BUCKET,
                            &allow,
                        )
                    } else {
                        push(
                            host,
                            tx,
                            &*h,
                            tank,
                            l.pump_io.min(l.tank_io),
                            BUCKET,
                            &allow,
                        )
                    };
                    if n == 0 { Err(()) } else { Ok(n) }
                });
                if moved.is_ok() {
                    return (HAND, now);
                }
                *tank = before;
            }
        }
    }
    if set.world {
        if set.input {
            if let Some(cd) = world_in(host, site, tank, cap, &allow) {
                return (cd, last_hand);
            }
        } else {
            for (dir, v) in DIRECTIONS_6 {
                if dir != Direction::Up && world_out(host, site, add(site.pos, v), tank, &allow) {
                    return (WORLD, last_hand);
                }
            }
        }
    }
    for (dir, v) in DIRECTIONS_6 {
        let at = add(site.pos, v);
        let Some(h) = site
            .level
            .capability(host, &mod_api::FLUIDS, at, Some(opposite(dir)))
        else {
            continue;
        };
        let before = tank.clone();
        let io = l.pump_io.min(l.tank_io);
        let moved = host.transaction(|host, tx| {
            let n = if set.input {
                pull(host, tx, &*h, tank, (cap, io), l.pump_io, &allow)
            } else {
                push(host, tx, &*h, tank, io, l.pump_io, &allow)
            };
            if n == 0 { Err(()) } else { Ok(n) }
        });
        if moved.is_ok() {
            return (HANDLER, last_hand);
        }
        *tank = before;
    }
    let cd = if last_hand + 10 * HAND > now {
        HAND
    } else {
        DID_NOTHING
    };
    (cd, last_hand)
}

fn is_air(id: &str) -> bool {
    matches!(
        id,
        "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
    )
}

/// `placeFluidInWorld`: a bucket of the tank's fluid into air or a flowing fluid at `at`.
fn world_out(
    host: &mut dyn ServerHost,
    site: &Site,
    at: [i32; 3],
    tank: &mut Tank,
    allow: &dyn Fn(&str) -> bool,
) -> bool {
    let Some(id) = tank.fluid.clone() else {
        return false;
    };
    if tank.amount < BUCKET || !allow(&id) {
        return false;
    }
    let free = is_air(&crate::storage::block_id(&*host, site.level, at))
        || site
            .level
            .fluid_state(&*host, at)
            .is_some_and(|s| s.amount > 0 && !s.source);
    let Some(key) = host.fluid_key(&id) else {
        return false;
    };
    if !free {
        return false;
    }
    let placed = host.transaction(|host, tx| {
        if site.level.place_fluid(host, at, key, None, false, tx) {
            Ok(())
        } else {
            Err(())
        }
    });
    if placed.is_ok() {
        tank.drain(BUCKET, None);
        true
    } else {
        false
    }
}

/// `fillFromBlockInRange`: the first source fluid within four blocks, found outwards from the pump. The cooldown
/// grows with the distance.
fn world_in(
    host: &mut dyn ServerHost,
    site: &Site,
    tank: &mut Tank,
    cap: u32,
    allow: &dyn Fn(&str) -> bool,
) -> Option<i64> {
    let mut seen = std::collections::HashSet::new();
    let mut queue = VecDeque::from([site.pos]);
    seen.insert(site.pos);
    while let Some(p) = queue.pop_front() {
        if site
            .level
            .fluid_state(&*host, p)
            .is_some_and(|s| s.amount > 0)
        {
            let before = tank.clone();
            let got = host.transaction(|host, tx| {
                let st = site.level.pickup_fluid(host, p, None, tx).ok_or(())?;
                let id = st
                    .key()
                    .and_then(|k| k.id(&*host).map(str::to_owned))
                    .ok_or(())?;
                if !allow(&id) || !tank.accepts(&id) || tank.room(cap) < st.amount {
                    return Err(());
                }
                Ok(tank.fill(&id, st.amount, cap, None))
            });
            if got.is_ok_and(|n| n > 0) {
                let d = (0..3).map(|i| (p[i] - site.pos[i]).pow(2)).sum::<i32>();
                return Some((f64::from(d).sqrt().max(1.0) * WORLD as f64) as i64);
            }
            *tank = before;
        }
        for (_, v) in DIRECTIONS_6 {
            let n = add(p, v);
            if seen.insert(n) {
                let d = (0..3).map(|i| (n[i] - site.pos[i]).pow(2)).sum::<i32>();
                if d < WORLD_RANGE_SQ {
                    queue.push_back(n);
                }
            }
        }
    }
    None
}

// ---- experience ----

/// Moves experience from the player into the tank down to `stop` level: how many points. `io` limits the amount
/// when given. Whole points only, so none are lost to the rounding.
pub fn xp_store(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    tank: &mut Tank,
    cap: u32,
    io: Option<u32>,
    stop: i32,
) -> i32 {
    let Some(e) = player.experience(&*host) else {
        return 0;
    };
    let points = player_points(e.level, e.progress) - xp_for_level(stop);
    if points <= 0 || !tank.accepts(XP_FLUID) {
        return 0;
    }
    let mut mb = (points as u32)
        .saturating_mul(MB_PER_POINT)
        .min(tank.room(cap));
    if let Some(io) = io {
        mb = mb.min(io);
    }
    let n = mb / MB_PER_POINT;
    if n == 0 {
        return 0;
    }
    tank.fill(XP_FLUID, n * MB_PER_POINT, cap, None);
    set_points(host, player, e.level, e.progress, e.total, -(n as i32));
    n as i32
}

/// Moves experience from the tank to the player up to `stop` level: how many points.
pub fn xp_give(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    tank: &mut Tank,
    io: Option<u32>,
    stop: i32,
) -> i32 {
    let Some(e) = player.experience(&*host) else {
        return 0;
    };
    if tank.fluid.as_deref() != Some(XP_FLUID) {
        return 0;
    }
    let points = xp_for_level(stop) - player_points(e.level, e.progress);
    if points <= 0 {
        return 0;
    }
    let mut mb = (points as u32)
        .saturating_mul(MB_PER_POINT)
        .min(tank.amount);
    if let Some(io) = io {
        mb = mb.min(io);
    }
    let n = mb / MB_PER_POINT;
    if n == 0 {
        return 0;
    }
    tank.drain(n * MB_PER_POINT, None);
    set_points(host, player, e.level, e.progress, e.total, n as i32);
    n as i32
}

/// `Player.giveExperiencePoints(delta)`: the level, the bar and the total moved by `delta` points.
fn set_points(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    level: i32,
    progress: f32,
    total: i32,
    delta: i32,
) {
    let now = (player_points(level, progress) + delta).max(0);
    let l = level_for(now);
    player.set_experience_levels(host, l);
    player.set_experience_points(host, now - xp_for_level(l));
    player.set_total_experience(host, (total + delta).max(0));
}

/// The first damaged mending item the player has in the hand, the off hand or armour, taken in turn by `now`.
fn mending_slot(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    now: i64,
) -> Option<(u32, ItemStack)> {
    let effect = mod_api::EnchantmentEffect::new("minecraft:repair_with_xp")?;
    let inv = crate::places::inventory(host, player);
    let held = player.selected_slot(&*host).unwrap_or(0) as u32;
    let found: Vec<(u32, ItemStack)> = [held, 36, 37, 38, 39, 40]
        .into_iter()
        .filter_map(|s| inv.get(s as usize).cloned().flatten().map(|st| (s, st)))
        .filter(|(_, st)| {
            st.is_damaged() && mod_api::ItemResource::of(st).has_enchantment_effect(&*host, effect)
        })
        .collect();
    if found.is_empty() {
        None
    } else {
        let n = found.len();
        found.into_iter().nth(now.unsigned_abs() as usize % n)
    }
}

/// `XpPumpUpgradeWrapper.tick` for one player: moves experience by the direction and level, then mends.
fn xp_player(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    x: &Xp,
    tank: &mut Tank,
    l: &Limits,
    now: i64,
) {
    if let Some(e) = player.experience(&*host) {
        match flow(DIRECTIONS[x.direction % 4], x.level, e.level, e.progress) {
            Flow::Store => {
                xp_store(host, player, tank, l.tank_cap, Some(l.tank_io), x.level);
            }
            Flow::Give => {
                xp_give(host, player, tank, Some(l.tank_io), x.level);
            }
            Flow::Idle => {}
        }
    }
    let t = crate::options::tune().liquids;
    if !t.xp_mending || !x.mend || tank.fluid.as_deref() != Some(XP_FLUID) {
        return;
    }
    let Some((slot, mut st)) = mending_slot(host, player, now) else {
        return;
    };
    // An experience point repairs two durability (XpHelper.getXpRepairRatio).
    let want = (st.damage() as f32 / 2.0).min(t.xp_per_mending as f32);
    let mb = (want * MB_PER_POINT as f32) as u32;
    let got = tank.drain(mb, Some(l.tank_io));
    // A remainder under a point goes back, so no experience is made or lost.
    let points = got / MB_PER_POINT;
    let rest = got - points * MB_PER_POINT;
    if rest > 0 {
        tank.fill(XP_FLUID, rest, l.tank_cap, None);
    }
    if points > 0 {
        st.set_damage(st.damage().saturating_sub(points * 2));
        crate::places::set_slot(host, player, slot, Some(st));
    }
}

fn xp_tick(host: &mut dyn ServerHost, site: &Site, up: &ItemStack, tank: &mut Tank, l: &Limits) {
    let x = read_xp(up);
    let now = site.level.game_time(&*host).unwrap_or(0);
    let players = match site.carrier {
        Some(p) => vec![p],
        None => nearby(&*host, site),
    };
    for p in players {
        xp_player(host, p, &x, tank, l, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_follow_the_inventory_width() {
        assert_eq!(rows(27), 3);
        assert_eq!(rows(81), 9);
        assert_eq!(rows(108), 9);
        assert_eq!(rows(120), 10);
        assert_eq!(rows(1), 1);
    }

    #[test]
    fn capacity_scales_with_rows_and_the_weighed_multiplier() {
        assert_eq!(capacity(4000, 3, 1.0, 1.0), 12_000);
        assert_eq!(capacity(4000, 3, 1.0, 2.0), 24_000);
        assert_eq!(capacity(4000, 3, 0.5, 3.0), 24_000);
        assert_eq!(capacity(4000, 3, 0.0, 8.0), 12_000);
        assert_eq!(capacity(20_000, 10, 5.0, 2_147_483_647.0), i32::MAX as u32);
    }

    #[test]
    fn fluid_transfer_never_drops_under_a_bucket() {
        assert_eq!(fluid_io(20, 3, 1.0), 1000);
        assert_eq!(fluid_io(1000, 3, 2.0), 6000);
        assert_eq!(energy_io(20, 3, 1.0, 1.0), 60);
    }

    #[test]
    fn the_tank_fills_and_drains_within_its_limits() {
        let mut t = Tank::default();
        assert_eq!(t.fill("minecraft:water", 5000, 4000, None), 4000);
        assert_eq!(t.fill("minecraft:water", 1, 4000, None), 0);
        assert_eq!(t.fill("minecraft:lava", 1, 8000, None), 0);
        assert_eq!(t.fill("minecraft:water", 500, 8000, Some(100)), 100);
        assert_eq!(t.amount, 4100);
        assert_eq!(t.drain(300, Some(100)), 100);
        assert_eq!(t.drain(10_000, None), 4000);
        assert_eq!(t, Tank::default());
    }

    #[test]
    fn experience_levels_match_the_game() {
        assert_eq!(xp_for_level(0), 0);
        assert_eq!(xp_for_level(10), 160);
        assert_eq!(xp_for_level(15), 315);
        assert_eq!(xp_for_level(16), 352);
        assert_eq!(xp_for_level(30), 1395);
        assert_eq!(xp_for_level(31), 1507);
        assert_eq!(xp_limit(0), 7);
        assert_eq!(xp_limit(15), 37);
        assert_eq!(xp_limit(30), 112);
        for level in [0, 1, 14, 15, 29, 30, 50] {
            assert_eq!(level_for(xp_for_level(level)), level);
            assert_eq!(level_for(xp_for_level(level + 1) - 1), level);
        }
        assert_eq!(player_points(10, 0.5), 160 + 13);
    }

    #[test]
    fn the_xp_pump_follows_its_direction() {
        assert_eq!(flow("input", 5, 10, 0.0), Flow::Store);
        assert_eq!(flow("input", 10, 10, 0.0), Flow::Idle);
        assert_eq!(flow("input", 10, 10, 0.3), Flow::Store);
        assert_eq!(flow("input", 10, 5, 0.0), Flow::Idle);
        assert_eq!(flow("output", 10, 5, 0.0), Flow::Give);
        assert_eq!(flow("output", 5, 10, 0.0), Flow::Idle);
        assert_eq!(flow("keep", 10, 5, 0.0), Flow::Give);
        assert_eq!(flow("keep", 5, 10, 0.0), Flow::Store);
        assert_eq!(flow("off", 10, 5, 0.0), Flow::Idle);
    }

    #[test]
    fn an_empty_pump_filter_lets_every_fluid_pass() {
        let none = [None, None];
        assert!(filter_matches(&none, "minecraft:lava"));
        let some = [None, Some("minecraft:water".to_owned())];
        assert!(filter_matches(&some, "minecraft:water"));
        assert!(!filter_matches(&some, "minecraft:lava"));
    }
}
