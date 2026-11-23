//! Sophisticated Storage's placed blocks (StorageBlockBase and its chests, barrels, limited barrels and shulker boxes).
//! A placed storage is a block whose block entity holds one stack: the storage's own item with its contents in
//! `custom_data` (as a backpack's), so the menu, the upgrades and the crafting kinds treat it like any storage item,
//! and it goes back to an item as it is. Here:
//! - placing (the item's own placement, then `placed`): the block entity takes the stack, the block its `wood`
//!   (`woodType`), a chest joins the chest beside it into a double chest (ChestBlock.getStateForPlacement);
//! - opening, the limited barrels' front (deposit with use, take with attack), the barrels' `open` state;
//! - breaking (`broken`): chests and barrels spill their contents and drop the empty storage
//!   (`tooManyItemEntityDrops = 200`: more stacks than that drop it packed), shulker boxes and packed storages drop
//!   with their contents; a double chest's half takes its half of the contents along;
//! - the tier upgrade items on a placed storage or a vanilla chest, barrel or shulker box (StorageTierUpgradeItem),
//!   the packing tape;
//! - the item capability (hoppers, pipes) and the upgrades that work on a placed storage: hopper, magnet,
//!   compacting, void.
use std::collections::HashSet;
use std::sync::Mutex;

use mod_api::{
    BlockEntityRef, BlockUpdateFlags, Compound, Direction, Host, HostTransaction, Identifier,
    ItemResource, ItemStack, LevelRef, Overanswered, PlayerRef, ResourceHandler, ServerHost,
    SoundSource, Tag, Transaction,
};

use crate::contents::{
    self, Contents, Nbt, custom_data, get_bool, get_int, get_str, put, set_custom_data,
};
use crate::places::{self, Loc};
use crate::storage_items::{self, Family, StorageItem};
use crate::upgrades::{self, Kind};

pub const PACKED: &str = "packed";
pub const WOOD: &str = "woodType";

/// The Sophisticated Storage block `id` is (not a backpack).
pub fn kind_of_block(id: &str) -> Option<&'static StorageItem> {
    storage_items::by_block(id).filter(|k| k.family != Family::Backpack)
}

pub fn block_id(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> String {
    level
        .block(host, pos)
        .and_then(|k| k.id(host))
        .unwrap_or_default()
        .to_owned()
}

/// The entities whose boxes meet `min`..`max` (`Level.getEntities`), or None when the level can't be read.
pub fn entities_in(
    host: &dyn Host,
    level: LevelRef,
    min: [f64; 3],
    max: [f64; 3],
) -> Option<Vec<mod_api::EntityRef>> {
    let mut found = Vec::new();
    let _ = level
        .visit_entities_in(host, min, max, &mut |e| {
            found.push(e);
            std::ops::ControlFlow::Continue(())
        })
        .ok()?;
    Some(found)
}

pub fn add(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn dir_vec(name: &str) -> [i32; 3] {
    match name {
        "north" => [0, 0, -1],
        "south" => [0, 0, 1],
        "west" => [-1, 0, 0],
        "east" => [1, 0, 0],
        "up" => [0, 1, 0],
        _ => [0, -1, 0],
    }
}

/// Direction.getClockWise (seen from above).
pub(crate) fn clockwise(f: &str) -> &'static str {
    match f {
        "north" => "east",
        "east" => "south",
        "south" => "west",
        _ => "north",
    }
}

pub(crate) fn counter_clockwise(f: &str) -> &'static str {
    match f {
        "north" => "west",
        "west" => "south",
        "south" => "east",
        _ => "north",
    }
}

// ---- block states ----

/// The block at `pos` and its properties.
pub fn state_props(
    host: &dyn Host,
    level: LevelRef,
    pos: [i32; 3],
) -> Option<(String, Vec<(String, String)>)> {
    let state = level.block_state(host, pos)?;
    let s = host.block_state_string(state)?;
    let (block, rest) = match s.split_once('[') {
        Some((b, r)) => (b.to_string(), r.trim_end_matches(']').to_string()),
        None => (s.clone(), String::new()),
    };
    let props = rest
        .split(',')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    Some((block, props))
}

pub fn prop<'a>(props: &'a [(String, String)], k: &str) -> Option<&'a str> {
    props.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
}

pub fn set_prop(props: &mut Vec<(String, String)>, k: &str, v: &str) {
    match props.iter_mut().find(|(n, _)| n == k) {
        Some(p) => p.1 = v.to_string(),
        None => props.push((k.to_string(), v.to_string())),
    }
}

/// Sets `block[props]` at `pos` (properties `block` doesn't have are left out).
pub fn set_state(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    block: &str,
    props: &[(String, String)],
) -> bool {
    let full = |ps: &[(String, String)]| {
        if ps.is_empty() {
            block.to_string()
        } else {
            format!(
                "{block}[{}]",
                ps.iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
    };
    let mut state = host.block_state_id(&full(props));
    if state.is_none() {
        // Keep only the properties the block has.
        let ok: Vec<(String, String)> = props
            .iter()
            .filter(|(k, v)| host.block_state_id(&format!("{block}[{k}={v}]")).is_some())
            .cloned()
            .collect();
        state = host.block_state_id(&full(&ok));
    }
    let Some(state) = state else { return false };
    matches!(
        level.set_block(
            host,
            pos,
            state,
            BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS
        ),
        Ok(true)
    )
}

/// Changes properties of the block at `pos`, keeping the block and its block entity.
pub fn change_props(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    changes: &[(&str, &str)],
) -> bool {
    let Some((block, mut props)) = state_props(&*host, level, pos) else {
        return false;
    };
    if changes.iter().all(|(k, v)| prop(&props, k) == Some(v)) {
        return true;
    }
    for (k, v) in changes {
        set_prop(&mut props, k, v);
    }
    set_state(host, level, pos, &block, &props)
}

// ---- the block entity's stack ----

pub fn read_be(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> Option<ItemStack> {
    host.block_entity(level, pos)?
        .item_slot(host, 0)
        .ok()
        .flatten()
}

pub fn write_be(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    stack: Option<ItemStack>,
) -> bool {
    let Some(be) = host.block_entity(level, pos) else {
        return false;
    };
    let ok = host
        .transaction(|host, tx| be.set_item_slot(host, tx, 0, stack.clone()).map_err(|_| ()))
        .is_ok();
    if ok {
        let _ = be.set_changed(host);
    }
    ok
}

// ---- stacks ----

pub fn is_packed(stack: &ItemStack) -> bool {
    get_bool(&custom_data(stack), PACKED) == Some(true)
}

pub fn wood_of(stack: &ItemStack) -> Option<String> {
    get_str(&custom_data(stack), WOOD).filter(|w| storage_items::WOODS.contains(&w.as_str()))
}

/// WoodStorageBlockItem.setWoodType, with what Horizon draws and names from: the model's `custom_model_data` string
/// and the `item_name` ("Oak Chest").
pub fn set_wood(stack: &mut ItemStack, kind: &StorageItem, wood: &str) {
    let mut d = custom_data(stack);
    put(&mut d, WOOD, Tag::String(wood.into()));
    set_custom_data(stack, d);
    let path = kind.id.split(':').nth(1).unwrap_or("");
    let name = Tag::compound([
        (
            "translate",
            Tag::String(
                format!("block.sophisticatedstorage.{path}.wooden")
                    .as_str()
                    .into(),
            ),
        ),
        (
            "with",
            Tag::List(vec![
                Tag::compound([(
                    "translate",
                    Tag::String(
                        format!("wood_name.sophisticatedstorage.{wood}")
                            .as_str()
                            .into(),
                    ),
                )]),
                Tag::compound([("text", Tag::String(" ".into()))]),
            ]),
        ),
    ]);
    let mut patch = Compound::new();
    patch.insert(
        "minecraft:custom_model_data".into(),
        Tag::compound([("strings", Tag::List(vec![Tag::String(wood.into())]))]),
    );
    patch.insert("minecraft:item_name".into(), name);
    let _ = stack.apply_components(&patch);
}

pub fn new_stack(host: &dyn Host, id: &str) -> Option<ItemStack> {
    let key = host.item_key(&Identifier::parse(id).ok()?)?;
    ItemStack::new(key, 1)
}

/// `from` as storage `to`: the same components on the other item (a wooden one renamed after its new item).
pub fn retype(host: &dyn Host, from: &ItemStack, to: &StorageItem) -> Option<ItemStack> {
    let mut out = new_stack(host, to.id)?;
    if let Some(c) = from.components() {
        let _ = out.apply_components(c.as_nbt());
    }
    fix_name(&mut out, to);
    Some(out)
}

/// After a stack's components were copied onto another storage item: a wooden one gets its wood name again
/// ("Oak Iron Chest"), any other drops the copied `item_name` (a chest's name on a shulker box).
pub fn fix_name(stack: &mut ItemStack, to: &StorageItem) {
    match wood_of(stack) {
        Some(w) if storage_items::wooden(to) => set_wood(stack, to, &w),
        _ => {
            let mut patch = Compound::new();
            patch.insert(
                "!minecraft:item_name".into(),
                Tag::Compound(Compound::new()),
            );
            let _ = stack.apply_components(&patch);
        }
    }
}

/// Splits stacks larger than their item's max stack size (a stack upgrade's slots) into droppable stacks.
fn droppable(host: &dyn Host, stacks: impl Iterator<Item = ItemStack>) -> Vec<ItemStack> {
    let mut out = Vec::new();
    for st in stacks {
        let max = host.max_stack_size(st.item()).unwrap_or(64).max(1);
        let mut left = st.count();
        while left > 0 {
            let n = left.min(max);
            if let Some(s) = contents::with_count(&st, n) {
                out.push(s);
            }
            left -= n;
        }
    }
    out
}

// ---- double chests ----

/// The other half of a double chest at `pos` (ChestBlock.getConnectedDirection).
pub fn partner(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> Option<[i32; 3]> {
    let (block, props) = state_props(host, level, pos)?;
    let facing = prop(&props, "facing")?.to_string();
    let dir = match prop(&props, "type")? {
        "left" => clockwise(&facing),
        "right" => counter_clockwise(&facing),
        _ => return None,
    };
    let p = add(pos, dir_vec(dir));
    (block_id(host, level, p) == block).then_some(p)
}

/// Where a storage's contents are: a double chest's main half (the one whose stack holds both halves).
pub fn main_pos(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> [i32; 3] {
    let Some(p) = partner(host, level, pos) else {
        return pos;
    };
    let main = |at| {
        read_be(host, level, at)
            .is_some_and(|s| get_bool(&custom_data(&s), contents::DOUBLE) == Some(true))
    };
    if !main(pos) && main(p) { p } else { pos }
}

/// A chest placed beside a single chest of the same block, facing and wood joins it (ChestBlock.getStateForPlacement,
/// without sneaking): the chest that was there keeps the contents, now twice as many slots.
fn join_chest(host: &mut dyn ServerHost, level: LevelRef, pos: [i32; 3]) {
    let Some((block, props)) = state_props(&*host, level, pos) else {
        return;
    };
    let Some(facing) = prop(&props, "facing").map(str::to_string) else {
        return;
    };
    let wood = prop(&props, "wood").map(str::to_string);
    for (dir, mine, theirs) in [
        (clockwise(&facing), "left", "right"),
        (counter_clockwise(&facing), "right", "left"),
    ] {
        let n = add(pos, dir_vec(dir));
        let Some((nb, np)) = state_props(&*host, level, n) else {
            continue;
        };
        if nb != block
            || prop(&np, "type") != Some("single")
            || prop(&np, "facing") != Some(facing.as_str())
            || prop(&np, "wood").map(str::to_string) != wood
            || prop(&np, "packed") == Some("true")
        {
            continue;
        }
        let Some(mut other) = read_be(&*host, level, n) else {
            continue;
        };
        if get_bool(&custom_data(&other), contents::DOUBLE) == Some(true) || is_packed(&other) {
            continue;
        }
        let mut d = custom_data(&other);
        put(&mut d, contents::DOUBLE, Tag::Byte(1));
        set_custom_data(&mut other, d);
        write_be(host, level, n, Some(other));
        change_props(host, level, n, &[("type", theirs)]);
        change_props(host, level, pos, &[("type", mine)]);
        return;
    }
}

/// Before a double chest's half at `pos` goes: the main half's slots past one chest go with the half that goes, the
/// other keeps the rest and the upgrades, and becomes a single chest. The stack the leaving half should spill.
fn split_double(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    kind: &StorageItem,
    own: ItemStack,
) -> ItemStack {
    let Some(p) = partner(&*host, level, pos) else {
        return own;
    };
    let Some(other) = read_be(&*host, level, p) else {
        return own;
    };
    let own_main = get_bool(&custom_data(&own), contents::DOUBLE) == Some(true);
    let (main_stack, mut rest_stack) = if own_main {
        (own.clone(), other.clone())
    } else {
        (other.clone(), own.clone())
    };
    let c = Contents::read(&*host, &main_stack, kind);
    let n = kind.slot_count().min(c.slots.len());
    let keep = Contents {
        slots: c.slots[..n].to_vec(),
        upgrades: c.upgrades.clone(),
        kept: c.kept.clone(),
        kept_upgrades: c.kept_upgrades.clone(),
        locked: false,
    };
    let leave = Contents {
        slots: c.slots[n..].to_vec(),
        upgrades: vec![None; kind.upgrade_count()],
        ..Default::default()
    };
    let unflag = |s: &mut ItemStack| {
        let mut d = custom_data(s);
        d.del(contents::DOUBLE);
        set_custom_data(s, d);
    };
    // The staying half: the first chest's slots and the upgrades.
    let mut staying = if own_main {
        rest_stack.clone()
    } else {
        main_stack.clone()
    };
    unflag(&mut staying);
    keep.write(&*host, &mut staying, kind);
    write_be(host, level, p, Some(staying));
    change_props(host, level, p, &[("type", "single")]);
    // The leaving half: the second chest's slots.
    if own_main {
        rest_stack = main_stack;
    }
    let mut leaving = rest_stack;
    unflag(&mut leaving);
    let mut d = custom_data(&leaving);
    d.del("contentsUuid");
    set_custom_data(&mut leaving, d);
    leave.write(&*host, &mut leaving, kind);
    leaving
}

// ---- placing ----

/// StorageBlockItem's placement, after the block is set: the block entity takes the stack (unpacked), the block its
/// wood; a chest joins its neighbour.
pub fn placed(
    host: &mut dyn ServerHost,
    player: Option<PlayerRef>,
    pos: [i32; 3],
    stack: &ItemStack,
) -> Option<(LevelRef, [i32; 3])> {
    let kind = storage_items::by_item(&contents::item_id(&*host, stack))
        .filter(|k| k.family != Family::Backpack)?;
    let player = player?;
    let dim = player.dimension(&*host)?.to_string();
    let level = host.level(&dim)?;
    kind_of_block(&block_id(&*host, level, pos))?;
    let mut st = contents::with_count(stack, 1)?;
    let mut d = custom_data(&st);
    let had_contents = d.has(contents::INVENTORY) || d.has(contents::UPGRADES);
    d.del(PACKED);
    if places::creative(&*host, player.uuid()) && d.has("contentsUuid") {
        put(
            &mut d,
            "contentsUuid",
            Tag::IntArray(contents::new_uuid().to_vec()),
        );
    }
    set_custom_data(&mut st, d);
    if !write_be(host, level, pos, Some(st.clone())) {
        eprintln!("[sophisticatedcore] the placed storage's block entity took nothing at {pos:?}");
    }
    if storage_items::wooden(kind) {
        let wood = wood_of(&st).unwrap_or_else(|| "oak".into());
        change_props(
            host,
            level,
            pos,
            &[("wood", wood.as_str()), ("packed", "false")],
        );
    } else {
        change_props(host, level, pos, &[("packed", "false")]);
    }
    if kind.family == Family::Chest && !places::sneaking(&*host, player.uuid()) && !had_contents {
        join_chest(host, level, pos);
    }
    Some((level, pos))
}

/// The state a storage is placed in (facing; the wood and a chest's type come in `placed`): a chest faces the
/// player, a barrel or limited barrel looks at the player (vanilla's BarrelBlock), a shulker box faces away from the
/// clicked face.
pub fn placement_facing(family: Family, yaw: f32, pitch: f32, face: Direction) -> &'static str {
    let horizontal = |yaw: f32| match ((yaw / 90.0 + 0.5).floor() as i32) & 3 {
        0 => "south",
        1 => "west",
        2 => "north",
        _ => "east",
    };
    // The player's look direction, flipped.
    let toward = |yaw: f32| match horizontal(yaw) {
        "south" => "north",
        "west" => "east",
        "north" => "south",
        _ => "west",
    };
    match family {
        Family::Chest => toward(yaw),
        Family::ShulkerBox => match face {
            Direction::Up => "up",
            Direction::Down => "down",
            Direction::North => "north",
            Direction::South => "south",
            Direction::West => "west",
            Direction::East => "east",
        },
        _ => {
            if pitch > 45.0 {
                "up"
            } else if pitch < -45.0 {
                "down"
            } else {
                toward(yaw)
            }
        }
    }
}

// ---- breaking ----

/// A player breaks the storage at `pos` (before the block goes): chests and barrels spill their contents and keep
/// the empty storage in the block entity, which then drops it; shulker boxes and packed storages keep everything.
pub fn broken(host: &mut dyn ServerHost, player: PlayerRef, level: LevelRef, pos: [i32; 3]) {
    let Some(kind) = kind_of_block(&block_id(&*host, level, pos)) else {
        return;
    };
    let Some(mut st) = read_be(&*host, level, pos) else {
        return;
    };
    let creative = player.is_creative(&*host) == Some(true);
    if kind.family == Family::Chest && !is_packed(&st) {
        st = split_double(host, level, pos, kind, st);
    }
    let c = Contents::read(&*host, &st, kind);
    // Stand-ins of unknown items can't be dropped as items: the storage keeps them, even broken in creative.
    let unknown = c.unknown_only();
    // `dropPacked`: a chest or barrel always comes away packed, with its contents (a double chest is split first).
    if crate::options::Options::read(&*host).drop_packed
        && kind.family != Family::ShulkerBox
        && !is_packed(&st)
        && !c.is_empty()
    {
        let mut d = custom_data(&st);
        put(&mut d, PACKED, Tag::Byte(1));
        set_custom_data(&mut st, d);
        write_be(host, level, pos, Some(st));
        return;
    }
    if kind.family == Family::ShulkerBox || is_packed(&st) {
        if creative && c.is_empty() {
            write_be(host, level, pos, None);
        } else if kind.family == Family::Chest {
            write_be(host, level, pos, Some(st));
        }
        return;
    }
    let drops = droppable(
        &*host,
        c.slots
            .iter()
            .chain(c.upgrades.iter())
            .flatten()
            .filter(|s| !contents::is_unknown(s))
            .cloned(),
    );
    if drops.len() > crate::options::Options::read(&*host).too_many_drops {
        // StorageBlockBase.playerWillDestroy: too many item entities, so it drops packed.
        let mut d = custom_data(&st);
        put(&mut d, PACKED, Tag::Byte(1));
        set_custom_data(&mut st, d);
        write_be(host, level, pos, Some(st));
        return;
    }
    for d in drops {
        let _ = level.pop_resource(host, pos, d);
    }
    if creative && unknown.is_none() {
        write_be(host, level, pos, None);
    } else {
        Contents::strip(&mut st);
        if let Some(u) = unknown {
            u.write(&*host, &mut st, kind);
        }
        write_be(host, level, pos, Some(st));
    }
}

// ---- tier upgrades and packing tape ----

/// A tier upgrade item's tiers: what it upgrades from (`None` vanilla and basic, for `basic_*`) and to.
fn tier_upgrade(id: &str) -> Option<(Option<u8>, u8)> {
    let path = id
        .strip_prefix("sophisticatedstorage:")?
        .strip_suffix("_tier_upgrade")?;
    let tier = |s: &str| {
        ["basic", "copper", "iron", "gold", "diamond", "netherite"]
            .iter()
            .position(|t| *t == s)
            .map(|i| i as u8)
    };
    if path == "basic" {
        return Some((None, 0));
    }
    let (from, to) = path.split_once("_to_")?;
    let to = tier(to)?;
    Some((
        if from == "basic" {
            None
        } else {
            Some(tier(from)?)
        },
        to,
    ))
}

const VANILLA_SHULKERS: [(&str, i32); 17] = [
    ("minecraft:shulker_box", -1),
    ("minecraft:white_shulker_box", 0xF9FFFE),
    ("minecraft:orange_shulker_box", 0xF9801D),
    ("minecraft:magenta_shulker_box", 0xC74EBD),
    ("minecraft:light_blue_shulker_box", 0x3AB3DA),
    ("minecraft:yellow_shulker_box", 0xFED83D),
    ("minecraft:lime_shulker_box", 0x80C71F),
    ("minecraft:pink_shulker_box", 0xF38BAA),
    ("minecraft:gray_shulker_box", 0x474F52),
    ("minecraft:light_gray_shulker_box", 0x9D9D97),
    ("minecraft:cyan_shulker_box", 0x169C9C),
    ("minecraft:purple_shulker_box", 0x8932B8),
    ("minecraft:blue_shulker_box", 0x3C44AA),
    ("minecraft:brown_shulker_box", 0x835432),
    ("minecraft:green_shulker_box", 0x5E7C16),
    ("minecraft:red_shulker_box", 0xB02E26),
    ("minecraft:black_shulker_box", 0x1D1D21),
];

/// Whether a held item acts on the block `block` before the block's own use (so the right click is taken).
pub fn takes_click(held: &str, block: &str) -> bool {
    if held == "sophisticatedstorage:packing_tape" {
        return kind_of_block(block).is_some_and(|k| k.family != Family::ShulkerBox);
    }
    let Some((from, to)) = tier_upgrade(held) else {
        return false;
    };
    if let Some(k) = kind_of_block(block) {
        return match from {
            None => to > 0 && k.tier == 0,
            Some(a) => k.tier == a,
        };
    }
    from.is_none()
        && (block == "minecraft:chest"
            || block == "minecraft:barrel"
            || VANILLA_SHULKERS.iter().any(|(b, _)| *b == block))
}

/// The vanilla container's items (its saved `Items`), emptied.
fn take_vanilla_items(
    host: &mut dyn ServerHost,
    be: BlockEntityRef,
    size: usize,
) -> Vec<Option<ItemStack>> {
    let mut out = vec![None; size];
    if let Some(data) = be.data(&*host)
        && let Some(Tag::List(items)) = data.at("Items")
    {
        for t in items {
            if let Tag::Compound(e) = t
                && let Some((slot, st)) = contents::decode_entry(&*host, e)
                && slot < size
            {
                out[slot] = Some(st);
            }
        }
    }
    let mut empty = be.data(&*host).unwrap_or_default();
    empty.insert("Items".into(), Tag::List(Vec::new()));
    let _ = be.replace_data(host, empty);
    out
}

/// One block's upgrade: the block replaced by `to`'s, the block entity's stack by `to`'s item with the same components.
fn upgrade_block(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    to: &'static StorageItem,
) -> bool {
    let Some(st) = read_be(&*host, level, pos) else {
        return false;
    };
    let Some(new) = retype(&*host, &st, to) else {
        return false;
    };
    let Some((_, props)) = state_props(&*host, level, pos) else {
        return false;
    };
    write_be(host, level, pos, None);
    if !set_state(host, level, pos, to.block.unwrap_or(to.id), &props) {
        write_be(host, level, pos, Some(st));
        return false;
    }
    write_be(host, level, pos, Some(new))
}

/// A vanilla chest, barrel or shulker box becoming storage `to` (StorageTierUpgradeItem.VanillaTierUpgradeDefinition):
/// its items move into the new storage; a double chest's two halves become a double chest.
fn upgrade_vanilla(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    block: &str,
    to_tier: u8,
) -> bool {
    let family = if block == "minecraft:chest" {
        Family::Chest
    } else if block == "minecraft:barrel" {
        Family::Barrel
    } else {
        Family::ShulkerBox
    };
    let Some(to) = storage_items::STORAGE
        .iter()
        .find(|s| s.family == family && s.tier == to_tier)
    else {
        return false;
    };
    let Some((_, mut props)) = state_props(&*host, level, pos) else {
        return false;
    };
    let double = family == Family::Chest && prop(&props, "type").is_some_and(|t| t != "single");
    let other = if double {
        let facing = prop(&props, "facing").unwrap_or("north").to_string();
        let dir = if prop(&props, "type") == Some("left") {
            clockwise(&facing)
        } else {
            counter_clockwise(&facing)
        };
        let p = add(pos, dir_vec(dir));
        (block_id(&*host, level, p) == "minecraft:chest").then_some(p)
    } else {
        None
    };
    let Some(be) = host.block_entity(level, pos) else {
        return false;
    };
    let mut items = take_vanilla_items(host, be, 27);
    let mut other_props = None;
    if let Some(p) = other
        && let Some(obe) = host.block_entity(level, p)
    {
        items.extend(take_vanilla_items(host, obe, 27));
        other_props = state_props(&*host, level, p).map(|s| s.1);
    }
    let Some(mut st) = new_stack(&*host, to.id) else {
        return false;
    };
    match family {
        Family::Chest => set_wood(&mut st, to, "oak"),
        Family::Barrel => set_wood(&mut st, to, "spruce"),
        _ => {
            if let Some((_, c)) = VANILLA_SHULKERS.iter().find(|(b, _)| *b == block)
                && *c >= 0
            {
                let mut d = custom_data(&st);
                put(&mut d, "mainColor", Tag::Int(*c));
                put(&mut d, "accentColor", Tag::Int(*c));
                set_custom_data(&mut st, d);
            }
        }
    }
    if other_props.is_some() {
        let mut d = custom_data(&st);
        put(&mut d, contents::DOUBLE, Tag::Byte(1));
        set_custom_data(&mut st, d);
    } else if double {
        set_prop(&mut props, "type", "single");
    }
    let n = contents::slots_of(&st, to);
    items.resize(n, None);
    let c = Contents {
        slots: items,
        upgrades: vec![None; to.upgrade_count()],
        ..Default::default()
    };
    c.write(&*host, &mut st, to);
    let wood = if family == Family::Barrel {
        "spruce"
    } else {
        "oak"
    };
    set_prop(&mut props, "wood", wood);
    set_prop(&mut props, "packed", "false");
    set_prop(&mut props, "open", "false");
    if !set_state(host, level, pos, to.id, &props) {
        return false;
    }
    write_be(host, level, pos, Some(st.clone()));
    if let (Some(p), Some(mut op)) = (other, other_props) {
        set_prop(&mut op, "wood", wood);
        set_prop(&mut op, "packed", "false");
        if set_state(host, level, p, to.id, &op) {
            let mut bare = st.clone();
            Contents::strip(&mut bare);
            write_be(host, level, p, Some(bare));
        }
    }
    true
}

/// The click of a held tier upgrade or packing tape on the block at `pos`; whether it did something.
pub fn use_tool(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    slot: u32,
    held: &ItemStack,
    level: LevelRef,
    pos: [i32; 3],
) -> bool {
    let held_id = contents::item_id(&*host, held);
    let block = block_id(&*host, level, pos);
    if !takes_click(&held_id, &block) {
        return false;
    }
    let uuid = player.uuid();
    let creative = player.is_creative(&*host) == Some(true);
    if held_id == "sophisticatedstorage:packing_tape" {
        if partner(&*host, level, pos).is_some() {
            places::message(host, uuid, "A double chest can't be packed");
            return true;
        }
        let Some(mut st) = read_be(&*host, level, pos) else {
            return false;
        };
        if is_packed(&st) {
            return true;
        }
        let mut d = custom_data(&st);
        put(&mut d, PACKED, Tag::Byte(1));
        set_custom_data(&mut st, d);
        write_be(host, level, pos, Some(st));
        change_props(host, level, pos, &[("packed", "true")]);
        if !creative {
            let dmg = held
                .components()
                .and_then(|c| get_int(c.as_nbt(), "minecraft:damage"))
                .unwrap_or(0)
                + 1;
            let max = host
                .item_default_component(held.item(), "minecraft:max_damage")
                .and_then(|t| match t {
                    Tag::Int(v) => Some(v),
                    _ => None,
                });
            let left = if max.is_some_and(|m| dmg >= m) {
                None
            } else {
                let mut h = held.clone();
                let mut patch = Compound::new();
                patch.insert("minecraft:damage".into(), Tag::Int(dmg));
                let _ = h.apply_components(&patch);
                Some(h)
            };
            places::set_slot(host, player, slot, left);
        }
        let at = pos.map(|v| f64::from(v) + 0.5);
        level.play_sound(
            host,
            "minecraft:item.armor.equip_leather",
            at,
            SoundSource::Blocks,
            1.0,
            1.2,
            None,
        );
        return true;
    }
    let Some((_, to)) = tier_upgrade(&held_id) else {
        return false;
    };
    let done = if let Some(kind) = kind_of_block(&block) {
        let Some(to_kind) = storage_items::with_tier(kind, to) else {
            return false;
        };
        let main = main_pos(&*host, level, pos);
        if read_be(&*host, level, main).is_some_and(|s| is_packed(&s)) {
            return true;
        }
        let other = partner(&*host, level, pos);
        let need = if other.is_some() { 2 } else { 1 };
        if held.count() < need {
            places::message(host, uuid, "A double chest needs two tier upgrades");
            return true;
        }
        let mut ok = upgrade_block(host, level, pos, to_kind);
        if ok && let Some(p) = other {
            ok = upgrade_block(host, level, p, to_kind);
        }
        ok.then_some(need)
    } else {
        let double = block == "minecraft:chest"
            && level
                .block_property(&*host, pos, "type")
                .is_some_and(|t| t != "single");
        let need = if double { 2 } else { 1 };
        if held.count() < need {
            places::message(host, uuid, "A double chest needs two tier upgrades");
            return true;
        }
        upgrade_vanilla(host, level, pos, &block, to).then_some(need)
    };
    match done {
        Some(need) => {
            if !creative {
                places::set_slot(host, player, slot, held.clone().shrink(need));
            }
            let at = pos.map(|v| f64::from(v) + 0.5);
            level.play_sound(
                host,
                "minecraft:block.smithing_table.use",
                at,
                SoundSource::Blocks,
                1.0,
                1.0,
                None,
            );
            true
        }
        None => false,
    }
}

// ---- limited barrels ----

/// Which of a limited barrel's slots a point on its front is (LimitedBarrelBlock.getInteractionSlot): for two slots
/// top and bottom, for three the top and the bottom's left and right, for four the quarters.
pub fn limited_slot(kind: &StorageItem, facing: &str, pos: [i32; 3], point: [f64; 3]) -> usize {
    if kind.slot_count() <= 1 {
        return 0;
    }
    let rel = [
        point[0] - f64::from(pos[0]),
        point[1] - f64::from(pos[1]),
        point[2] - f64::from(pos[2]),
    ];
    // (right, up) on the front, seen from outside.
    let (x, y) = match facing {
        "north" => (1.0 - rel[0], rel[1]),
        "south" => (rel[0], rel[1]),
        "west" => (rel[2], rel[1]),
        "east" => (1.0 - rel[2], rel[1]),
        "up" => (rel[0], 1.0 - rel[2]),
        _ => (rel[0], rel[2]),
    };
    let (top, right) = (y > 0.5, x > 0.5);
    match kind.slot_count() {
        2 => usize::from(!top),
        3 => {
            if top {
                0
            } else if right {
                2
            } else {
                1
            }
        }
        _ => match (top, right) {
            (true, false) => 0,
            (true, true) => 1,
            (false, false) => 2,
            (false, true) => 3,
        },
    }
}

/// LimitedBarrelBlockEntity.depositItem: the held stack (or, on a second click within 10 ticks, every stack of the
/// slot's item in the inventory) into slot `slot`.
pub fn limited_deposit(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    level: LevelRef,
    pos: [i32; 3],
    slot: usize,
    all: bool,
) -> bool {
    let Some(kind) = kind_of_block(&block_id(&*host, level, pos)) else {
        return false;
    };
    let Some(mut st) = read_be(&*host, level, pos) else {
        return false;
    };
    let mut c = Contents::read(&*host, &st, kind);
    if slot >= c.slots.len() {
        return false;
    }
    let m = upgrades::multiplier(&*host, kind, &c.upgrades);
    let inv = places::inventory(host, player);
    let hand = player.selected_slot(&*host).unwrap_or(0) as u32;
    let candidates: Vec<u32> = if all { (0..36).collect() } else { vec![hand] };
    let mut moved = false;
    for i in candidates {
        let Some(h) = inv[i as usize].clone() else {
            continue;
        };
        let shown = c.shown_at(&*host, kind, slot);
        match &shown {
            Some(cur) if !contents::same(cur, &h) => continue,
            None if all => continue,
            _ => {}
        }
        if !crate::menu::may_store(&*host, kind, &h) {
            continue;
        }
        let banks = crate::compression::Banks::of(&*host, kind, &c);
        let put = if banks.covers(slot) {
            banks.insert_at(&mut c, slot, &h, h.count())
        } else {
            let lim = upgrades::limit(&*host, &h, m);
            let have = c.slots[slot].as_ref().map_or(0, |s| s.count());
            let put = lim.saturating_sub(have).min(h.count());
            if put > 0 {
                c.slots[slot] = contents::with_count(&h, have + put);
            }
            put
        };
        if put == 0 {
            continue;
        }
        places::set_slot(host, player, i, h.clone().shrink(put));
        moved = true;
    }
    if moved {
        c.write(&*host, &mut st, kind);
        write_be(host, level, pos, Some(st));
    }
    moved
}

/// LimitedBarrelBlockEntity.tryToTakeItem: one of the slot's item (a stack when sneaking) to the player.
pub fn limited_take(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    level: LevelRef,
    pos: [i32; 3],
    slot: usize,
    stack: bool,
) -> bool {
    let Some(kind) = kind_of_block(&block_id(&*host, level, pos)) else {
        return false;
    };
    let Some(mut st) = read_be(&*host, level, pos) else {
        return false;
    };
    if is_packed(&st) {
        return false;
    }
    let mut c = Contents::read(&*host, &st, kind);
    let Some(cur) = c
        .shown_at(&*host, kind, slot)
        .filter(|s| !contents::is_unknown(s))
    else {
        return false;
    };
    let max = host.max_stack_size(cur.item()).unwrap_or(64).max(1);
    let n = if stack { max.min(cur.count()) } else { 1 };
    let Some(out) = contents::with_count(&cur, n) else {
        return false;
    };
    c.take_from(&*host, kind, slot, n);
    c.write(&*host, &mut st, kind);
    write_be(host, level, pos, Some(st));
    places::give_or_drop(host, player, &out);
    true
}

/// Where the player's look meets the front face of the block at `pos` (for a left click, which carries no point).
pub fn look_point(
    host: &dyn Host,
    player: PlayerRef,
    pos: [i32; 3],
    facing: &str,
) -> Option<[f64; 3]> {
    let e = player.entity();
    let eye = e.eye_position(host)?;
    let v = e.view_vector(host)?;
    let n = dir_vec(facing);
    let axis = n.iter().position(|c| *c != 0)?;
    let plane = f64::from(pos[axis]) + if n[axis] > 0 { 1.0 } else { 0.0 };
    if v[axis].abs() < 1e-6 {
        return None;
    }
    let t = (plane - eye[axis]) / v[axis];
    (t > 0.0).then(|| [eye[0] + v[0] * t, eye[1] + v[1] * t, eye[2] + v[2] * t])
}

// ---- the item capability ----

/// A placed storage as an item handler (the jar's storage wrapper's inventory): its slots, each up to its limit;
/// what goes in passes the void upgrade. Reads the block entity's stack once and writes it back on each change.
/// The provider only sees the block, so the double chest's main half and the packed check are resolved on first use:
/// a packed storage reads as no slots.
pub struct Handler {
    level: LevelRef,
    pos: [i32; 3],
    kind: &'static StorageItem,
    /// The main half's position, its stack and contents.
    cache: Mutex<Option<([i32; 3], ItemStack, Contents)>>,
}

impl Handler {
    pub fn new(level: LevelRef, pos: [i32; 3], kind: &'static StorageItem) -> Handler {
        Handler {
            level,
            pos,
            kind,
            cache: Mutex::new(None),
        }
    }

    fn with<R>(&self, host: &dyn Host, f: impl FnOnce(&ItemStack, &Contents) -> R) -> Option<R> {
        let mut g = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if g.is_none() {
            let main = main_pos(host, self.level, self.pos);
            let st = read_be(host, self.level, main)?;
            if is_packed(&st) {
                return None;
            }
            let c = Contents::read(host, &st, self.kind);
            *g = Some((main, st, c));
        }
        g.as_ref().map(|(_, s, c)| f(s, c))
    }

    fn store(
        &self,
        host: &mut dyn ServerHost,
        tx: &Transaction,
        mut st: ItemStack,
        c: Contents,
    ) -> bool {
        let Some(main) = self
            .cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|(main, _, _)| *main)
        else {
            return false;
        };
        c.write(&*host, &mut st, self.kind);
        let Some(be) = host.block_entity(self.level, main) else {
            return false;
        };
        let ok = be
            .set_item_slot(host, tx, 0, Some(st.clone()))
            .is_ok_and(|b| b);
        if ok {
            *self.cache.lock().unwrap_or_else(|e| e.into_inner()) = Some((main, st, c));
        }
        ok
    }

    fn limit(&self, host: &dyn Host, c: &Contents, stack: &ItemStack) -> u32 {
        upgrades::limit(
            host,
            stack,
            upgrades::multiplier(host, self.kind, &c.upgrades),
        )
    }

    /// InventoryIOHandler: whether the filter upgrades let `stack` through in (`input`) or out.
    fn passes(&self, host: &dyn Host, c: &Contents, stack: &ItemStack, input: bool) -> bool {
        crate::filter::io_allowed(host, &c.upgrades, stack, c.slots.iter().flatten(), input)
    }

    /// The void upgrade's answer for `stack`: Some(true) voids it all, Some(false) only what doesn't fit.
    fn void(&self, host: &dyn Host, c: &Contents, stack: &ItemStack) -> Option<bool> {
        c.upgrades.iter().flatten().find_map(|u| {
            let k = upgrades::of(host, u)?;
            (k.kind == Kind::Void && upgrades::matches(host, u, &k, stack))
                .then(|| !upgrades::void_overflow(&k, u))
        })
    }
}

/// Slot `index`'s stack unless it is the stand-in of an unknown item (hidden from hoppers and other mods).
fn movable(host: &dyn Host, kind: &StorageItem, c: &Contents, index: usize) -> Option<ItemStack> {
    c.shown_at(host, kind, index)
        .filter(|s| !contents::is_unknown(s))
}

impl ResourceHandler<ItemResource> for Handler {
    fn size(&self, host: &mut dyn ServerHost) -> usize {
        self.with(&*host, |_, c| c.slots.len()).unwrap_or(0)
    }
    fn resource(&self, host: &mut dyn ServerHost, index: usize) -> Option<ItemResource> {
        let h: &dyn Host = &*host;
        self.with(h, |_, c| {
            movable(h, self.kind, c, index).map(|s| ItemResource::of(&s))
        })
        .flatten()
    }
    fn amount(&self, host: &mut dyn ServerHost, index: usize) -> u32 {
        let h: &dyn Host = &*host;
        self.with(h, |_, c| {
            movable(h, self.kind, c, index).map_or(0, |s| s.count())
        })
        .unwrap_or(0)
    }
    /// With no resource, the slot's limit for an item that stacks to 64 (`getSlotLimit`).
    fn capacity(
        &self,
        host: &mut dyn ServerHost,
        index: usize,
        resource: Option<&ItemResource>,
    ) -> u32 {
        let h: &dyn Host = &*host;
        self.with(h, |_, c| match resource {
            Some(resource) => crate::compression::Banks::of(h, self.kind, c)
                .limit(c, index)
                .filter(|l| *l > 0)
                .unwrap_or_else(|| self.limit(h, c, resource.as_stack())),
            None => {
                let v = (64.0 * upgrades::multiplier(h, self.kind, &c.upgrades)).floor();
                if v >= f64::from(u32::MAX) {
                    u32::MAX
                } else {
                    (v as u32).max(1)
                }
            }
        })
        .unwrap_or(0)
    }
    fn is_valid(&self, host: &mut dyn ServerHost, _index: usize, resource: &ItemResource) -> bool {
        let h: &dyn Host = &*host;
        crate::menu::may_store(h, self.kind, resource.as_stack())
            && self
                .with(h, |_, c| self.passes(h, c, resource.as_stack(), true))
                .unwrap_or(false)
    }
    fn insert_at(
        &self,
        host: &mut dyn ServerHost,
        tx: &Transaction,
        index: usize,
        resource: &ItemResource,
        amount: u32,
    ) -> u32 {
        let h: &dyn Host = &*host;
        let Some((st, mut c, lim, void, open)) = self.with(h, |s, c| {
            (
                s.clone(),
                c.clone(),
                self.limit(h, c, resource.as_stack()),
                self.void(h, c, resource.as_stack()),
                self.passes(h, c, resource.as_stack(), true),
            )
        }) else {
            return 0;
        };
        if !open || !crate::menu::may_store(&*host, self.kind, resource.as_stack()) {
            return 0;
        }
        if void == Some(true) {
            return amount;
        }
        let banks = crate::compression::Banks::of(h, self.kind, &c);
        if banks.covers(index) {
            let put = banks.insert_at(&mut c, index, resource.as_stack(), amount);
            if put > 0 && !self.store(host, tx, st, c) {
                return 0;
            }
            return if void == Some(false) { amount } else { put };
        }
        let Some(slot) = c.slots.get_mut(index) else {
            return 0;
        };
        let have = match slot {
            Some(cur) if !resource.matches(cur) => return 0,
            Some(cur) => cur.count(),
            None => 0,
        };
        let put = lim.saturating_sub(have).min(amount);
        if put > 0 {
            *slot = resource.to_stack(have + put);
            if !self.store(host, tx, st, c) {
                return 0;
            }
        }
        if void == Some(false) { amount } else { put }
    }
    fn extract_at(
        &self,
        host: &mut dyn ServerHost,
        tx: &Transaction,
        index: usize,
        resource: &ItemResource,
        amount: u32,
    ) -> u32 {
        let h: &dyn Host = &*host;
        let Some((st, mut c, open)) = self.with(h, |s, c| {
            (
                s.clone(),
                c.clone(),
                self.passes(h, c, resource.as_stack(), false),
            )
        }) else {
            return 0;
        };
        let banks = crate::compression::Banks::of(h, self.kind, &c);
        if banks.covers(index) {
            let shown = banks.shown(&c, index);
            if !open || !shown.is_some_and(|s| resource.matches(&s)) {
                return 0;
            }
            let take = banks.take_at(&mut c, index, amount);
            if take > 0 && !self.store(host, tx, st, c) {
                return 0;
            }
            return take;
        }
        let Some(slot) = c.slots.get_mut(index) else {
            return 0;
        };
        let Some(cur) = slot.clone() else { return 0 };
        if !open || contents::is_unknown(&cur) || !resource.matches(&cur) {
            return 0;
        }
        let take = cur.count().min(amount);
        *slot = contents::with_count(&cur, cur.count() - take);
        if take > 0 && !self.store(host, tx, st, c) {
            return 0;
        }
        take
    }
    /// Any slot: stacks onto what matches first, then empty slots (`insertItem` over the whole inventory).
    fn insert(
        &self,
        host: &mut dyn ServerHost,
        tx: &Transaction,
        resource: &ItemResource,
        amount: u32,
    ) -> Result<u32, Overanswered> {
        let h: &dyn Host = &*host;
        let Some((st, mut c, void, open)) = self.with(h, |s, c| {
            (
                s.clone(),
                c.clone(),
                self.void(h, c, resource.as_stack()),
                self.passes(h, c, resource.as_stack(), true),
            )
        }) else {
            return Ok(0);
        };
        if !open || !crate::menu::may_store(&*host, self.kind, resource.as_stack()) {
            return Ok(0);
        }
        if void == Some(true) {
            return Ok(amount);
        }
        let went = c.add(&*host, self.kind, resource.as_stack(), amount);
        if went > 0 && !self.store(host, tx, st, c) {
            return Ok(0);
        }
        Ok(if void == Some(false) { amount } else { went })
    }
}

// ---- placed upgrades ----

/// The placed storages seen (placed, loaded, opened): the ones whose upgrades work on their own.
#[derive(Default)]
pub struct Placed {
    pub at: Mutex<HashSet<(LevelRef, [i32; 3])>>,
}

impl Placed {
    pub fn note(&self, level: LevelRef, pos: [i32; 3]) {
        self.at
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert((level, pos));
    }
}

pub(crate) fn dir_of(v: [i32; 3]) -> Direction {
    match v {
        [0, 1, 0] => Direction::Up,
        [0, -1, 0] => Direction::Down,
        [0, 0, -1] => Direction::North,
        [0, 0, 1] => Direction::South,
        [-1, 0, 0] => Direction::West,
        _ => Direction::East,
    }
}

/// MagnetUpgradeWrapper on a block: item entities within range (3, advanced 5) of the storage go in.
fn magnet(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    kind: &StorageItem,
    c: &mut Contents,
) -> bool {
    let ups: Vec<(ItemStack, upgrades::Upgrade)> = c
        .upgrades
        .iter()
        .flatten()
        .filter_map(|u| upgrades::of(&*host, u).map(|k| (u.clone(), k)))
        .collect();
    let Some(r) = ups
        .iter()
        .filter(|(u, k)| k.kind == Kind::Magnet && upgrades::flag(u, "pickupItems", true))
        .map(|(_, k)| if k.advanced { 5.0 } else { 3.0 })
        .reduce(f64::max)
    else {
        return false;
    };
    let center = pos.map(|v| f64::from(v) + 0.5);
    let min = [center[0] - r, center[1] - r, center[2] - r];
    let max = [center[0] + r, center[1] + r, center[2] + r];
    let Some(ents) = entities_in(&*host, level, min, max) else {
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
        let Some(st) = item.with_stack(&*host, |v| v.project()).flatten() else {
            continue;
        };
        let wanted = ups
            .iter()
            .any(|(u, k)| k.kind == Kind::Magnet && upgrades::matches(&*host, u, k, &st));
        if !wanted || !crate::menu::may_store(&*host, kind, &st) {
            continue;
        }
        let went = c.add(&*host, kind, &st, st.count());
        if went == 0 {
            continue;
        }
        changed = true;
        match contents::with_count(&st, st.count() - went) {
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

/// The placed storages' upgrades, tick `n`. Storages gone from the world are forgotten.
pub fn tick(host: &mut dyn ServerHost, placed: &Placed, n: u64) {
    if !n.is_multiple_of(2) {
        return;
    }
    let list: Vec<(LevelRef, [i32; 3])> = placed
        .at
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .copied()
        .collect();
    let mut gone = Vec::new();
    for (level, pos) in list {
        if !level.is_loaded(&*host, pos) {
            continue;
        }
        let Some(kind) = kind_of_block(&block_id(&*host, level, pos)) else {
            gone.push((level, pos));
            continue;
        };
        let Some(mut st) = read_be(&*host, level, pos) else {
            continue;
        };
        if is_packed(&st) {
            continue;
        }
        let d = custom_data(&st);
        let has_ups = matches!(d.at(contents::UPGRADES), Some(Tag::Compound(u)) if matches!(u.at("Items"), Some(Tag::List(l)) if !l.is_empty()));
        if !has_ups {
            continue;
        }
        let mut c = Contents::read(&*host, &st, kind);
        let kinds: Vec<upgrades::Upgrade> = c
            .upgrades
            .iter()
            .flatten()
            .filter_map(|u| upgrades::of(&*host, u))
            .collect();
        let mut changed = false;
        let tune = crate::options::tune();
        let hoppers: Vec<(ItemStack, upgrades::Upgrade)> = c
            .upgrades
            .iter()
            .flatten()
            .filter_map(|u| upgrades::of(&*host, u).map(|k| (u.clone(), k)))
            .filter(|(_, k)| k.kind == Kind::Hopper)
            .collect();
        for (up, k) in &hoppers {
            if n.is_multiple_of(tune.hopper_ticks[usize::from(k.advanced)] as u64) {
                changed |= crate::hopper::run(host, level, pos, kind, &mut c, up, k);
            }
        }
        if kinds.iter().any(|k| matches!(k.kind, Kind::Cooking(_))) {
            let id = contents::uuid(&st).unwrap_or([pos[0], pos[1], pos[2], i32::MIN]);
            changed |= crate::cooking::run(&*host, level, id, kind, &mut c);
        }
        if kinds.iter().any(|k| {
            matches!(
                k.kind,
                Kind::Tank | Kind::Battery | Kind::Pump | Kind::XpPump
            )
        }) {
            let id = contents::uuid(&st).unwrap_or([pos[0], pos[1], pos[2], i32::MIN]);
            let site = crate::fluids::Site {
                level,
                pos,
                id,
                carrier: None,
            };
            changed |= crate::fluids::run(host, &site, kind, &mut c);
        }
        if n.is_multiple_of(10) && kinds.iter().any(|k| k.kind == Kind::Magnet) {
            changed |= magnet(host, level, pos, kind, &mut c);
        }
        if n.is_multiple_of(20) && kinds.iter().any(|k| k.kind == Kind::Compacting) {
            changed |= crate::ticking::compact_contents(&*host, level, kind, &mut c);
        }
        if changed {
            c.write(&*host, &mut st, kind);
            write_be(host, level, pos, Some(st));
        }
    }
    if !gone.is_empty() {
        let mut g = placed.at.lock().unwrap_or_else(|e| e.into_inner());
        for x in gone {
            g.remove(&x);
        }
    }
}

/// The location the menu reads for the storage at `pos` (a double chest's main half).
pub fn menu_loc(host: &dyn Host, dim: &str, level: LevelRef, pos: [i32; 3]) -> Loc {
    Loc::Block {
        dim: dim.to_string(),
        pos: main_pos(host, level, pos),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_upgrades() {
        assert_eq!(
            tier_upgrade("sophisticatedstorage:basic_tier_upgrade"),
            Some((None, 0))
        );
        assert_eq!(
            tier_upgrade("sophisticatedstorage:basic_to_gold_tier_upgrade"),
            Some((None, 3))
        );
        assert_eq!(
            tier_upgrade("sophisticatedstorage:iron_to_netherite_tier_upgrade"),
            Some((Some(2), 5))
        );
        assert!(takes_click(
            "sophisticatedstorage:basic_tier_upgrade",
            "minecraft:chest"
        ));
        assert!(!takes_click(
            "sophisticatedstorage:basic_tier_upgrade",
            "sophisticatedstorage:chest"
        ));
        assert!(takes_click(
            "sophisticatedstorage:basic_to_iron_tier_upgrade",
            "sophisticatedstorage:chest"
        ));
        assert!(!takes_click(
            "sophisticatedstorage:basic_to_iron_tier_upgrade",
            "sophisticatedstorage:iron_chest"
        ));
        assert!(takes_click(
            "sophisticatedstorage:iron_to_gold_tier_upgrade",
            "sophisticatedstorage:limited_iron_barrel_2"
        ));
        assert!(!takes_click(
            "sophisticatedstorage:iron_to_gold_tier_upgrade",
            "sophisticatedstorage:copper_barrel"
        ));
        assert!(takes_click(
            "sophisticatedstorage:packing_tape",
            "sophisticatedstorage:barrel"
        ));
        assert!(!takes_click(
            "sophisticatedstorage:packing_tape",
            "sophisticatedstorage:shulker_box"
        ));
    }

    #[test]
    fn limited_slots() {
        let k4 = storage_items::by_item("sophisticatedstorage:limited_barrel_4").unwrap();
        // Facing south: x grows to the right seen from outside.
        assert_eq!(limited_slot(k4, "south", [0, 0, 0], [0.2, 0.8, 1.0]), 0);
        assert_eq!(limited_slot(k4, "south", [0, 0, 0], [0.8, 0.2, 1.0]), 3);
        let k3 = storage_items::by_item("sophisticatedstorage:limited_barrel_3").unwrap();
        assert_eq!(limited_slot(k3, "north", [0, 0, 0], [0.2, 0.2, 0.0]), 2);
    }
}
