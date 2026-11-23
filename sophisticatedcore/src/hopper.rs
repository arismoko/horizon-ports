//! The hopper upgrades (HopperUpgradeWrapper) of a placed storage: every `transferSpeedTicks`, from each side set to
//! pull, one stack of up to `maxTransferStackSize` items comes in from the neighbouring inventory (or container
//! minecart), and from each side set to push, one goes out. An input filter limits what comes in and an output
//! filter what goes out; each is an allow list, a block list or, as the original's contents filters, "what the
//! inventory already holds". The advanced upgrade sets its sides; the basic one pushes out of the bottom and pulls
//! into the top, as the jar always resets it to.

use mod_api::{
    Capability, Host, HostTransaction, ItemResource, ItemStack, LevelRef, ResourceHandler,
    ServerHost, Tag,
};

use crate::contents::{self, Contents, Nbt, custom_data, put, set_custom_data};
use crate::cooking::Filter;
use crate::storage::{self, add, dir_of, dir_vec};
use crate::storage_items::StorageItem;
use crate::upgrades::{self, Upgrade};

type Items = Capability<dyn ResourceHandler<ItemResource>>;

pub const INPUT: &str = "inputFilter";
pub const OUTPUT: &str = "outputFilter";

/// The six directions' serialized names.
pub const DIRS: [&str; 6] = ["down", "up", "north", "south", "west", "east"];

/// A face of the storage as its owner sees it (BlockSide).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Top,
    Bottom,
    Front,
    Back,
    Left,
    Right,
}

pub const SIDES: [Side; 6] = [
    Side::Top,
    Side::Bottom,
    Side::Front,
    Side::Back,
    Side::Left,
    Side::Right,
];

impl Side {
    pub fn label(self) -> &'static str {
        match self {
            Side::Top => "Top",
            Side::Bottom => "Bottom",
            Side::Front => "Front",
            Side::Back => "Back",
            Side::Left => "Left",
            Side::Right => "Right",
        }
    }
}

fn opposite(d: &str) -> &'static str {
    match d {
        "down" => "up",
        "up" => "down",
        "north" => "south",
        "south" => "north",
        "west" => "east",
        _ => "west",
    }
}

/// BlockSide.toDirection for a storage in the state `facing` (a barrel or shulker box may face up or down, which
/// StorageBlockBase.getVerticalFacing turns into a vertical facing over a horizontal north or south).
pub fn side_dir(side: Side, facing: &str) -> &'static str {
    let (h, v) = match facing {
        "up" => ("north", Some("up")),
        "down" => ("south", Some("down")),
        "south" => ("south", None),
        "east" => ("east", None),
        "west" => ("west", None),
        _ => ("north", None),
    };
    match (side, v) {
        (Side::Top, None) => "up",
        (Side::Top, Some("up")) => opposite(h),
        (Side::Top, Some(_)) => h,
        (Side::Bottom, None) => "down",
        (Side::Bottom, Some("up")) => h,
        (Side::Bottom, Some(_)) => opposite(h),
        (Side::Front, None) => h,
        (Side::Front, Some(v)) => v_name(v),
        (Side::Back, None) => opposite(h),
        (Side::Back, Some(v)) => opposite(v),
        (Side::Left, _) => storage::clockwise(h),
        (Side::Right, _) => storage::counter_clockwise(h),
    }
}

fn v_name(v: &str) -> &'static str {
    if v == "up" { "up" } else { "down" }
}

/// What a side does with the neighbouring inventory (IOMode).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Io {
    Push,
    Pull,
    PushPull,
    Off,
}

impl Io {
    /// IOMode.next, skipping DISABLED, which a hopper's sides can't be.
    pub fn next(self) -> Io {
        match self {
            Io::Push => Io::Pull,
            Io::Pull => Io::PushPull,
            Io::PushPull => Io::Off,
            Io::Off => Io::Push,
        }
    }
}

/// The sides it pulls from and pushes to, in the order they were set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dirs {
    pub pull: Vec<&'static str>,
    pub push: Vec<&'static str>,
}

impl Dirs {
    /// initDirections: push out of the bottom, pull in from the top.
    pub fn initial(facing: &str) -> Dirs {
        Dirs {
            pull: vec![side_dir(Side::Top, facing)],
            push: vec![side_dir(Side::Bottom, facing)],
        }
    }

    pub fn io(&self, dir: &str) -> Io {
        match (self.pull.contains(&dir), self.push.contains(&dir)) {
            (true, true) => Io::PushPull,
            (true, false) => Io::Pull,
            (false, true) => Io::Push,
            _ => Io::Off,
        }
    }

    pub fn set_io(&mut self, dir: &'static str, io: Io) {
        self.pull.retain(|d| *d != dir);
        self.push.retain(|d| *d != dir);
        if matches!(io, Io::Pull | Io::PushPull) {
            self.pull.push(dir);
        }
        if matches!(io, Io::Push | Io::PushPull) {
            self.push.push(dir);
        }
    }
}

fn read_list(up: &ItemStack, key: &str) -> Option<Vec<&'static str>> {
    let d = custom_data(up);
    let Some(Tag::List(l)) = d.at(key) else {
        return None;
    };
    Some(
        l.iter()
            .filter_map(|t| match t {
                Tag::String(s) => {
                    let s = s.to_string_lossy();
                    DIRS.iter().copied().find(|d| *d == s)
                }
                _ => None,
            })
            .collect(),
    )
}

/// The sides an upgrade is set to (`pullDirections`, `pushDirections`). The basic upgrade always has its initial
/// ones; the advanced one has them until a side is set.
pub fn read_dirs(up: &ItemStack, advanced: bool, facing: &str) -> Dirs {
    let pull = read_list(up, "pullDirections");
    let push = read_list(up, "pushDirections");
    if !advanced || (pull.is_none() && push.is_none()) {
        return Dirs::initial(facing);
    }
    Dirs {
        pull: pull.unwrap_or_default(),
        push: push.unwrap_or_default(),
    }
}

pub fn write_dirs(up: &mut ItemStack, d: &Dirs) {
    let list = |v: &[&str]| Tag::List(v.iter().map(|s| Tag::String((*s).into())).collect());
    let mut data = custom_data(up);
    put(&mut data, "pullDirections", list(&d.pull));
    put(&mut data, "pushDirections", list(&d.push));
    set_custom_data(up, data);
}

/// ContentsFilterType.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Allow,
    Block,
    Storage,
}

pub fn mode(f: &Filter) -> Mode {
    if f.by_storage {
        Mode::Storage
    } else if f.allow {
        Mode::Allow
    } else {
        Mode::Block
    }
}

/// ContentsFilterType.next, set the way ContentsFilterLogic.setDepositFilterType does.
pub fn cycle(f: &mut Filter) {
    match mode(f) {
        Mode::Allow => {
            f.by_storage = false;
            f.allow = false;
        }
        Mode::Block => f.by_storage = true,
        Mode::Storage => {
            f.by_storage = false;
            f.allow = true;
        }
    }
}

/// The hopper upgrade's two filters.
pub fn filters(host: &dyn Host, up: &ItemStack, advanced: bool) -> (Filter, Filter) {
    let n = crate::options::tune().hopper_filters[usize::from(advanced)];
    (
        Filter::read(host, up, INPUT, n[0]),
        Filter::read(host, up, OUTPUT, n[1]),
    )
}

/// The facing of the storage block at `pos`.
pub fn facing(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> String {
    storage::state_props(host, level, pos)
        .and_then(|(_, p)| storage::prop(&p, "facing").map(str::to_owned))
        .unwrap_or_else(|| "north".to_owned())
}

/// Whether the stack of `kind`'s block at `at` is the one at `pos` (a double chest's other half shares it).
fn same_storage(host: &dyn Host, level: LevelRef, pos: [i32; 3], at: [i32; 3]) -> bool {
    storage::kind_of_block(&storage::block_id(host, level, at)).is_some()
        && storage::main_pos(host, level, at) == storage::main_pos(host, level, pos)
}

/// moveItems into the storage: the first stack the input filter lets through and the storage takes.
fn pull(
    host: &mut dyn ServerHost,
    src: &Items,
    kind: &StorageItem,
    c: &mut Contents,
    filter: &Filter,
    mult: f64,
    max: u32,
) -> bool {
    for i in 0..src.size(host) {
        let Some(res) = src.resource(host, i) else {
            continue;
        };
        let stack = res.as_stack().clone();
        let amount = src.amount(host, i);
        if amount == 0
            || !crate::menu::may_store(&*host, kind, &stack)
            || !filter.matches_contents(&*host, &stack, c.slots.iter().flatten())
        {
            continue;
        }
        let lim = upgrades::limit(&*host, &stack, mult);
        let mut trial = c.slots.clone();
        let fits = contents::insert(&mut trial, &stack, max.min(amount), lim);
        if fits == 0 {
            continue;
        }
        let got = host
            .transaction(|host, tx| Ok::<u32, ()>(src.extract_at(host, tx, i, &res, fits)))
            .unwrap_or(0);
        if got > 0 {
            contents::insert(&mut c.slots, &stack, got, lim);
            return true;
        }
    }
    false
}

/// moveItems out of the storage: the first stack the output filter lets through and the target takes.
fn push(
    host: &mut dyn ServerHost,
    dst: &Items,
    c: &mut Contents,
    filter: &Filter,
    max: u32,
) -> bool {
    let held: Vec<ItemStack> = if filter.by_storage {
        (0..dst.size(host))
            .filter_map(|i| dst.resource(host, i).map(|r| r.as_stack().clone()))
            .collect()
    } else {
        Vec::new()
    };
    for i in 0..c.slots.len() {
        let Some(st) = c.slots[i].clone().filter(|s| !contents::is_unknown(s)) else {
            continue;
        };
        if !filter.matches_contents(&*host, &st, &held) {
            continue;
        }
        let res = ItemResource::of(&st);
        let n = st.count().min(max);
        let put = host
            .transaction(|host, tx| Ok::<u32, ()>(dst.insert(host, tx, &res, n).unwrap_or(0)))
            .unwrap_or(0);
        if put > 0 {
            c.slots[i] = st.clone().shrink(put);
            return true;
        }
    }
    false
}

/// The inventories next to `pos` on `dir`: the block's, then (when it has none) a container entity's.
fn neighbours(host: &mut dyn ServerHost, level: LevelRef, pos: [i32; 3], dir: &str) -> Vec<Items> {
    let at = add(pos, dir_vec(dir));
    if same_storage(&*host, level, pos, at) {
        return Vec::new();
    }
    let side = dir_of(dir_vec(opposite(dir)));
    if let Some(block) = level.capability(host, &mod_api::ITEMS, at, Some(side)) {
        return vec![block];
    }
    let min = at.map(f64::from);
    let max = at.map(|v| f64::from(v) + 1.0);
    let Some(found) = storage::entities_in(&*host, level, min, max) else {
        return Vec::new();
    };
    let mut boxes: Vec<Items> = found
        .into_iter()
        .filter_map(|e| e.capability(host, &mod_api::ENTITY_AUTOMATION, Some(side)))
        .collect();
    if boxes.len() > 1 {
        // One of several minecarts, at random.
        let n = i32::try_from(boxes.len()).unwrap_or(i32::MAX);
        let k = level
            .with_random(host, |r| r.next_int_bound(n))
            .unwrap_or(0);
        let last = boxes.len() - 1;
        boxes.swap(0, usize::try_from(k).unwrap_or(0).min(last));
    }
    boxes.truncate(1);
    boxes
}

/// One transfer round of a hopper upgrade `up` on the storage at `pos`: whether the storage's contents changed.
pub fn run(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    kind: &StorageItem,
    c: &mut Contents,
    up: &ItemStack,
    u: &Upgrade,
) -> bool {
    let max = crate::options::tune().hopper_stack[usize::from(u.advanced)] as u32;
    let mult = upgrades::multiplier(&*host, kind, &c.upgrades);
    let (input, output) = filters(&*host, up, u.advanced);
    let dirs = read_dirs(up, u.advanced, &facing(&*host, level, pos));
    let mut changed = false;
    for dir in &dirs.push {
        for dst in neighbours(host, level, pos, dir) {
            if push(host, &dst, c, &output, max) {
                changed = true;
                break;
            }
        }
    }
    for dir in &dirs.pull {
        for src in neighbours(host, level, pos, dir) {
            if pull(host, &src, kind, c, &input, mult, max) {
                changed = true;
                break;
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sides_follow_the_blocks_facing() {
        let of = |f| SIDES.map(|s| side_dir(s, f));
        // top, bottom, front, back, left, right
        assert_eq!(
            of("north"),
            ["up", "down", "north", "south", "east", "west"]
        );
        assert_eq!(of("east"), ["up", "down", "east", "west", "south", "north"]);
        assert_eq!(of("up"), ["south", "north", "up", "down", "east", "west"]);
        assert_eq!(of("down"), ["south", "north", "down", "up", "west", "east"]);
    }

    #[test]
    fn every_facing_reaches_all_six_directions() {
        for f in ["north", "south", "east", "west", "up", "down"] {
            let mut seen = SIDES.map(|s| side_dir(s, f));
            seen.sort_unstable();
            let mut all = DIRS;
            all.sort_unstable();
            assert_eq!(seen, all, "facing {f}");
        }
    }

    #[test]
    fn a_side_cycles_through_its_modes() {
        let mut d = Dirs::initial("north");
        assert_eq!(d.push, ["down"]);
        assert_eq!(d.pull, ["up"]);
        assert_eq!(d.io("up"), Io::Pull);
        assert_eq!(d.io("east"), Io::Off);
        let mut io = d.io("east");
        let mut seen = Vec::new();
        for _ in 0..4 {
            io = io.next();
            d.set_io("east", io);
            seen.push((io, d.pull.contains(&"east"), d.push.contains(&"east")));
        }
        assert_eq!(
            seen,
            [
                (Io::Push, false, true),
                (Io::Pull, true, false),
                (Io::PushPull, true, true),
                (Io::Off, false, false),
            ]
        );
    }

    #[test]
    fn the_filter_type_cycles_allow_block_storage() {
        let mut f = Filter {
            items: vec![None; 2],
            allow: true,
            by_mod: false,
            empty_allows_all: false,
            by_storage: false,
        };
        let mut seen = vec![mode(&f)];
        for _ in 0..3 {
            cycle(&mut f);
            seen.push(mode(&f));
        }
        assert_eq!(seen, [Mode::Allow, Mode::Block, Mode::Storage, Mode::Allow]);
    }
}
