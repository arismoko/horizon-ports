//! The storage network (ControllerBlockEntityBase, StorageLinkBlockEntity, StorageIOBlockEntity): a controller and the
//! storages connected to it, which together answer as one item handler for the controller, the input, output and
//! input/output blocks.
//!
//! The original keeps a registry in the controller that storages feed as they change. Here the network is read from
//! the world when it is used: the controller searches outwards from its own faces through storages (they conduct, and
//! join), and from the blocks the storage tool linked to it (which join wherever they are). Only the tool's links are
//! kept: the controller holds the list of positions linked to it, and a linked block holds its controller's position.
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

use mod_api::{
    AttachmentCodec, AttachmentType, BlockEntityPart, BlockEntityPartState, BlockPart, Compound,
    Host, ItemResource, LevelRef, Overanswered, PlayerRef, ResourceHandler, ServerHost, Tag,
    Transaction,
};

use mod_api::{BlockEntityDrawContext, ClientHost, HostTransaction};

use crate::contents::{Nbt, custom_data, get_bool, get_long, put};
use crate::places;
use crate::storage::{self, Handler};
use crate::storage_items::{Family, StorageItem};
use crate::tool::{pack, unpack};

pub const DEBUG_TOOL: &str = "sophisticatedstorage:debug_tool";
pub const CONTROLLER: &str = "sophisticatedstorage:controller";
pub const LINK: &str = "sophisticatedstorage:storage_link";
pub const IO: &str = "sophisticatedstorage:storage_io";
pub const INPUT: &str = "sophisticatedstorage:storage_input";
pub const OUTPUT: &str = "sophisticatedstorage:storage_output";
/// `ControllerBlockEntityBase.SEARCH_RANGE`: how far from the controller, on each axis, the search reaches.
pub const RANGE: i32 = 15;
const CONTROLLER_POS: &str = "controllerPos";
const LINKED: &str = "linked";

/// What a block is to the search.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// A storage with contents. `linked`: the controller the storage tool linked it to.
    Storage {
        linked: Option<[i32; 3]>,
    },
    /// The half of a double chest that holds nothing: it conducts like a storage but has no contents.
    Conductor,
    /// A storage link: once linked it joins the block behind it to the network.
    Link {
        linked: Option<[i32; 3]>,
        behind: [i32; 3],
    },
    /// An input, output or input/output block.
    Io {
        linked: Option<[i32; 3]>,
    },
    Other,
}

/// The blocks of one controller's network, in the order they were found.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Net {
    pub storages: Vec<[i32; 3]>,
    pub ios: Vec<[i32; 3]>,
    pub links: Vec<[i32; 3]>,
}

fn neighbours(p: [i32; 3]) -> [[i32; 3]; 6] {
    [
        [p[0], p[1] - 1, p[2]],
        [p[0], p[1] + 1, p[2]],
        [p[0], p[1], p[2] - 1],
        [p[0], p[1], p[2] + 1],
        [p[0] - 1, p[1], p[2]],
        [p[0] + 1, p[1], p[2]],
    ]
}

fn within(centre: [i32; 3], p: [i32; 3]) -> bool {
    (0..3).all(|a| (p[a] - centre[a]).abs() <= RANGE)
}

/// ControllerBlockEntityBase.searchAndAddBoundables: from the controller's six faces and from the positions linked to
/// it, through the storages (and the links the tool made) that conduct.
pub fn search(controller: [i32; 3], linked: &[[i32; 3]], role: &dyn Fn([i32; 3]) -> Role) -> Net {
    let mut net = Net::default();
    let mut seen = HashSet::new();
    let mut queue: VecDeque<([i32; 3], bool)> = VecDeque::new();
    queue.extend(linked.iter().map(|p| (*p, true)));
    queue.extend(neighbours(controller).into_iter().map(|p| (p, false)));
    let expand = |p, seen: &HashSet<[i32; 3]>, queue: &mut VecDeque<_>| {
        for n in neighbours(p) {
            if within(controller, n) && !seen.contains(&n) {
                queue.push_back((n, false));
            }
        }
    };
    while let Some((p, seeded)) = queue.pop_front() {
        if !seen.insert(p) {
            continue;
        }
        match role(p) {
            Role::Storage { linked: Some(c) } => {
                // A linked storage joins on its own; it does not conduct.
                if c == controller {
                    net.storages.push(p);
                }
            }
            Role::Storage { linked: None } if !seeded => {
                net.storages.push(p);
                expand(p, &seen, &mut queue);
            }
            Role::Conductor if !seeded => expand(p, &seen, &mut queue),
            Role::Link {
                linked: Some(c),
                behind,
            } if seeded && c == controller => {
                net.links.push(p);
                expand(p, &seen, &mut queue);
                if within(controller, behind) {
                    queue.push_back((behind, false));
                }
            }
            Role::Io { linked } if linked == Some(controller) || (linked.is_none() && !seeded) => {
                net.ios.push(p);
            }
            _ => {}
        }
    }
    net
}

/// StorageIOBlockEntity: an input or output block not linked by the tool finds its controller through the storages
/// around it (`addToAdjacentController`), the nearest one first.
pub fn find_controller(
    start: [i32; 3],
    role: &dyn Fn([i32; 3]) -> Role,
    is_controller: &dyn Fn([i32; 3]) -> bool,
) -> Option<[i32; 3]> {
    let mut seen = HashSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some(p) = queue.pop_front() {
        for n in neighbours(p) {
            if !seen.insert(n) {
                continue;
            }
            if is_controller(n) {
                return Some(n);
            }
            if matches!(role(n), Role::Storage { linked: None } | Role::Conductor)
                && within(start, n)
            {
                queue.push_back(n);
            }
        }
    }
    None
}

// ---- the block entities ----

/// The controller's state: the positions the tool linked to it.
pub static CONTROLLER_STATE: AttachmentType<Compound> = AttachmentType {
    namespace: "sophisticatedstorage",
    name: "controller",
    default: Compound::new,
    codec: Some(AttachmentCodec {
        encode: |c| Ok(c.clone()),
        decode: |c| Ok(c.clone()),
    }),
    copy_on_death: false,
    sync: true,
};

/// A link, input, output or input/output block's state: the controller the tool linked it to.
pub static NODE_STATE: AttachmentType<Compound> = AttachmentType {
    namespace: "sophisticatedstorage",
    name: "network_node",
    default: Compound::new,
    codec: Some(AttachmentCodec {
        encode: |c| Ok(c.clone()),
        decode: |c| Ok(c.clone()),
    }),
    copy_on_death: false,
    sync: true,
};

pub struct ControllerEntity;

#[mod_api::sunrise_part]
impl BlockPart for ControllerEntity {}

#[mod_api::sunrise_part]
impl BlockEntityPart for ControllerEntity {
    fn block_entity_submit(
        &self,
        host: &dyn ClientHost,
        event: &mut mod_api::BlockEntitySubmitEvent<'_>,
    ) {
        debug_view(host, event.pos, &mut *event.draw);
        note_link_view(host, event.pos, &mut *event.draw);
    }
}

impl BlockEntityPartState for ControllerEntity {
    type Data = Compound;
    const DATA: &'static AttachmentType<Compound> = &CONTROLLER_STATE;
}

pub struct NodeEntity;

#[mod_api::sunrise_part]
impl BlockPart for NodeEntity {}

#[mod_api::sunrise_part]
impl BlockEntityPart for NodeEntity {}

impl BlockEntityPartState for NodeEntity {
    type Data = Compound;
    const DATA: &'static AttachmentType<Compound> = &NODE_STATE;
}

fn longs(c: &Compound) -> Vec<[i32; 3]> {
    match c.at(LINKED) {
        Some(Tag::List(l)) => l
            .iter()
            .filter_map(|t| match t {
                Tag::Long(v) => Some(unpack(*v)),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The positions linked to the controller at `pos`.
pub fn linked_to(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> Vec<[i32; 3]> {
    host.block_entity(level, pos)
        .and_then(|be| ControllerEntity::state(host, be).ok())
        .map(|c| longs(&c))
        .unwrap_or_default()
}

/// Adds (or, with `on` false, removes) `target` in the controller's linked list.
fn set_linked(
    host: &mut dyn ServerHost,
    level: LevelRef,
    controller: [i32; 3],
    target: [i32; 3],
    on: bool,
) {
    let Some(be) = host.block_entity(level, controller) else {
        return;
    };
    let _ = ControllerEntity::edit_state(host, be, |_, c| {
        let mut all = longs(c);
        all.retain(|p| *p != target);
        if on {
            all.push(target);
        }
        put(
            c,
            LINKED,
            Tag::List(all.into_iter().map(|p| Tag::Long(pack(p))).collect()),
        );
    });
    let _ = be.set_changed(host);
}

/// The controller the tool linked the link or input/output block at `pos` to.
fn node_controller(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> Option<[i32; 3]> {
    let be = host.block_entity(level, pos)?;
    let c = NodeEntity::state(host, be).ok()?;
    get_long(&c, CONTROLLER_POS).map(unpack)
}

fn set_node_controller(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    to: Option<[i32; 3]>,
) {
    let Some(be) = host.block_entity(level, pos) else {
        return;
    };
    let _ = NodeEntity::edit_state(host, be, |_, c| match to {
        Some(p) => put(c, CONTROLLER_POS, Tag::Long(pack(p))),
        None => {
            c.del(CONTROLLER_POS);
        }
    });
    let _ = be.set_changed(host);
}

/// What a storage's block entity tells the search: whether it is packed, and the controller it is linked to.
type Info = (bool, Option<[i32; 3]>);

/// What the block at `pos` is to the search, with `info` reading a storage's block entity.
fn role_by(
    host: &dyn Host,
    level: LevelRef,
    pos: [i32; 3],
    info: &dyn Fn(&dyn Host, LevelRef, [i32; 3]) -> Option<Info>,
) -> Role {
    let block = storage::block_id(host, level, pos);
    if block == LINK {
        let facing = level
            .block_property(host, pos, "facing")
            .unwrap_or_default();
        let n = step(&facing);
        return Role::Link {
            linked: node_controller(host, level, pos),
            behind: [pos[0] - n[0], pos[1] - n[1], pos[2] - n[2]],
        };
    }
    if [IO, INPUT, OUTPUT].contains(&block.as_str()) {
        return Role::Io {
            linked: node_controller(host, level, pos),
        };
    }
    let Some(kind) = storage::kind_of_block(&block) else {
        return Role::Other;
    };
    if kind.family == Family::Chest && storage::main_pos(host, level, pos) != pos {
        return Role::Conductor;
    }
    match info(host, level, pos) {
        Some((false, linked)) => Role::Storage { linked },
        _ => Role::Other,
    }
}

/// What the block at `pos` is to the search, on the server.
pub fn role(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> Role {
    role_by(host, level, pos, &|host, level, pos| {
        let stack = storage::read_be(host, level, pos)?;
        Some((
            storage::is_packed(&stack),
            get_long(&custom_data(&stack), CONTROLLER_POS).map(unpack),
        ))
    })
}

/// What the block at `pos` is to the search, on a client: a storage's block entity holds only what `render::update_tag`
/// sent.
fn client_role(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> Role {
    role_by(host, level, pos, &|host, level, pos| {
        let data = host.block_entity(level, pos)?.data(host)?;
        let linked = get_long(&data, crate::render::LINKED_TO).filter(|v| *v != i64::MIN);
        Some((
            get_bool(&data, crate::storage::PACKED) == Some(true),
            linked.map(unpack),
        ))
    })
}

/// The block `facing` names, as a step.
fn step(facing: &str) -> [i32; 3] {
    match facing {
        "down" => [0, -1, 0],
        "north" => [0, 0, -1],
        "south" => [0, 0, 1],
        "west" => [-1, 0, 0],
        "east" => [1, 0, 0],
        _ => [0, 1, 0],
    }
}

/// Networks found already, by controller: reused until a block changes in the controller's range or at a linked
/// position, or the tool links or unlinks something. `K` names the level and the controller.
struct NetCache<K> {
    entries: HashMap<K, (Arc<Net>, Vec<[i32; 3]>, i64)>,
}

impl<K: Copy + Eq + Hash> NetCache<K> {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// The network under `key` found at `now`, searching with `find` when none is kept or the kept one is older than
    /// `ttl` ticks (`i64::MAX` for never).
    fn get(
        &mut self,
        key: K,
        now: i64,
        ttl: i64,
        find: impl FnOnce() -> (Net, Vec<[i32; 3]>),
    ) -> Arc<Net> {
        if let Some((net, _, at)) = self.entries.get(&key)
            && now.saturating_sub(*at) < ttl
        {
            return net.clone();
        }
        let (net, linked) = find();
        let net = Arc::new(net);
        self.entries.insert(key, (net.clone(), linked, now));
        net
    }

    /// A block changed at `pos`: forget every network it can be part of. `centre` gives an entry's controller.
    fn changed(&mut self, pos: [i32; 3], centre: impl Fn(&K) -> [i32; 3]) {
        self.entries
            .retain(|k, (_, linked, _)| !(within(centre(k), pos) || linked.contains(&pos)));
    }
}

type Key = (LevelRef, [i32; 3]);

static SERVER: LazyLock<Mutex<NetCache<Key>>> = LazyLock::new(|| Mutex::new(NetCache::new()));
static CLIENT: LazyLock<Mutex<NetCache<Key>>> = LazyLock::new(|| Mutex::new(NetCache::new()));
/// How long the client keeps a network it searched (ticks): block entity data arrives without a block change.
const CLIENT_TTL: i64 = 20;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// A block changed at `pos` on the server.
pub fn changed(pos: [i32; 3]) {
    lock(&SERVER).changed(pos, |k| k.1);
}

/// A block changed at `pos` on the client.
pub fn client_changed(pos: [i32; 3]) {
    lock(&CLIENT).changed(pos, |k| k.1);
}

/// The network of the controller at `pos`.
pub fn net_of(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> Arc<Net> {
    let mut cache = lock(&SERVER);
    cache.get((level, pos), 0, i64::MAX, || {
        let linked = linked_to(host, level, pos);
        (search(pos, &linked, &|p| role(host, level, p)), linked)
    })
}

/// The controller an input, output or input/output block at `pos` works through.
pub fn controller_of(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> Option<[i32; 3]> {
    if let Some(c) = node_controller(host, level, pos) {
        return Some(c);
    }
    find_controller(pos, &|p| role(host, level, p), &|p| {
        storage::block_id(host, level, p) == CONTROLLER
    })
}

// ---- linking (the storage tool) ----

/// Links the block at `pos` to the controller at `to`, or with `None` frees it. A storage keeps the controller in its
/// stack; a link or input/output block in its own state. The controller keeps the list.
pub fn link(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    to: Option<[i32; 3]>,
) -> bool {
    let block = storage::block_id(&*host, level, pos);
    let was = if storage::kind_of_block(&block).is_some() {
        storage::read_be(&*host, level, pos)
            .and_then(|s| get_long(&custom_data(&s), CONTROLLER_POS).map(unpack))
    } else {
        node_controller(&*host, level, pos)
    };
    if let Some(old) = was.filter(|o| Some(*o) != to) {
        changed(old);
        set_linked(host, level, old, pos, false);
    }
    if let Some(c) = to {
        if storage::block_id(&*host, level, c) != CONTROLLER {
            return false;
        }
        set_linked(host, level, c, pos, true);
    }
    if storage::kind_of_block(&block).is_some() {
        let Some(mut st) = storage::read_be(&*host, level, pos) else {
            return false;
        };
        let mut d = custom_data(&st);
        match to {
            Some(c) => put(&mut d, CONTROLLER_POS, Tag::Long(pack(c))),
            None => {
                d.del(CONTROLLER_POS);
            }
        }
        crate::contents::set_custom_data(&mut st, d);
        return storage::write_be(host, level, pos, Some(st));
    }
    set_node_controller(host, level, pos, to);
    true
}

/// Whether the tool can link the block `block` (a storage, a link, or an input/output block).
pub fn linkable(block: &str) -> bool {
    storage::kind_of_block(block).is_some() || [LINK, IO, INPUT, OUTPUT].contains(&block)
}

// ---- the item handler ----

/// How a block shows the network (StorageInputBlockEntity, StorageOutputBlockEntity).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// The controller and the input/output block: everything in and out.
    Full,
    /// Only in: a single slot that always reads empty.
    Input,
    /// Only out.
    Output,
}

/// The network as one item handler: its storages' slots one after the other.
pub struct NetHandler {
    level: LevelRef,
    /// The controller's position, or an input/output block's (`node`), whose controller is found on first use.
    at: [i32; 3],
    node: bool,
    view: View,
    parts: OnceLock<Vec<Handler>>,
}

impl NetHandler {
    pub fn new(level: LevelRef, at: [i32; 3], node: bool, view: View) -> Self {
        Self {
            level,
            at,
            node,
            view,
            parts: OnceLock::new(),
        }
    }

    /// Whether a storage of the network keeps a memory slot for `stack` (ControllerBlockEntityBase.isMemorizedItem).
    fn memorizes(&self, host: &dyn Host, stack: &mod_api::ItemStack) -> bool {
        self.parts(host).iter().any(|h| h.memorizes(host, stack))
    }

    fn parts(&self, host: &dyn Host) -> &[Handler] {
        self.parts.get_or_init(|| {
            let controller = if self.node {
                controller_of(host, self.level, self.at)
            } else {
                Some(self.at)
            };
            controller
                .map(|c| net_of(host, self.level, c).storages.clone())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|p| {
                    let kind: &'static StorageItem =
                        storage::kind_of_block(&storage::block_id(host, self.level, p))?;
                    Some(Handler::new(self.level, p, kind))
                })
                .collect()
        })
    }

    /// The storage and its own slot a network slot is.
    fn locate(&self, host: &mut dyn ServerHost, index: usize) -> Option<(&Handler, usize)> {
        let mut left = index;
        for h in self.parts(&*host) {
            let n = h.size(host);
            if left < n {
                return Some((h, left));
            }
            left -= n;
        }
        None
    }

    fn total(&self, host: &mut dyn ServerHost) -> usize {
        self.parts(&*host).iter().map(|h| h.size(host)).sum()
    }
}

impl ResourceHandler<ItemResource> for NetHandler {
    fn size(&self, host: &mut dyn ServerHost) -> usize {
        let n = self.total(host);
        if self.view == View::Input {
            n.min(1)
        } else {
            n
        }
    }

    fn resource(&self, host: &mut dyn ServerHost, index: usize) -> Option<ItemResource> {
        if self.view == View::Input {
            return None;
        }
        let (h, i) = self.locate(host, index)?;
        h.resource(host, i)
    }

    fn amount(&self, host: &mut dyn ServerHost, index: usize) -> u32 {
        if self.view == View::Input {
            return 0;
        }
        self.locate(host, index)
            .map_or(0, |(h, i)| h.amount(host, i))
    }

    fn capacity(
        &self,
        host: &mut dyn ServerHost,
        index: usize,
        resource: Option<&ItemResource>,
    ) -> u32 {
        self.locate(host, index)
            .map_or(0, |(h, i)| h.capacity(host, i, resource))
    }

    fn is_valid(&self, host: &mut dyn ServerHost, index: usize, resource: &ItemResource) -> bool {
        if self.view == View::Output {
            return false;
        }
        self.locate(host, index)
            .is_some_and(|(h, i)| h.is_valid(host, i, resource))
    }

    fn insert_at(
        &self,
        host: &mut dyn ServerHost,
        tx: &Transaction,
        index: usize,
        resource: &ItemResource,
        amount: u32,
    ) -> u32 {
        if self.view == View::Output {
            return 0;
        }
        // The controller's insert, whichever slot is asked: ControllerBlockEntityBase.insertSlot.
        self.insert(host, tx, resource, amount)
            .ok()
            .filter(|_| self.locate(host, index).is_some())
            .unwrap_or(0)
    }

    fn extract_at(
        &self,
        host: &mut dyn ServerHost,
        tx: &Transaction,
        index: usize,
        resource: &ItemResource,
        amount: u32,
    ) -> u32 {
        if self.view == View::Input {
            return 0;
        }
        match self.locate(host, index) {
            Some((h, i)) => h.extract_at(host, tx, i, resource, amount),
            None => 0,
        }
    }

    /// `insert(resource, amount, ctx, true)`: the storages already holding the item first, then any with room.
    fn insert(
        &self,
        host: &mut dyn ServerHost,
        tx: &Transaction,
        resource: &ItemResource,
        amount: u32,
    ) -> Result<u32, Overanswered> {
        if self.view == View::Output {
            return Ok(0);
        }
        let mut left = amount;
        let parts = self.parts(&*host);
        let holds = |host: &mut dyn ServerHost, h: &Handler| {
            (0..h.size(host)).any(|i| h.resource(host, i).is_some_and(|r| &r == resource))
        };
        let stack = resource.as_stack();
        let mut done = vec![false; parts.len()];
        // The storages with a memory slot for the item first, then those already holding it, then any with room
        // (the original's `memorizedItemStorages` before the stacks it holds).
        for pass in 0..3 {
            for (i, h) in parts.iter().enumerate() {
                if left == 0 {
                    return Ok(amount);
                }
                let skip = match pass {
                    0 => !h.memorizes(&*host, stack),
                    1 => !holds(host, h),
                    _ => false,
                };
                if done[i] || skip {
                    continue;
                }
                done[i] = true;
                left -= h.insert(host, tx, resource, left)?.min(left);
            }
        }
        Ok(amount - left)
    }

    fn extract(
        &self,
        host: &mut dyn ServerHost,
        tx: &Transaction,
        resource: &ItemResource,
        amount: u32,
    ) -> Result<u32, Overanswered> {
        if self.view == View::Input {
            return Ok(0);
        }
        let mut left = amount;
        for h in self.parts(&*host) {
            if left == 0 {
                break;
            }
            left -= h.extract(host, tx, resource, left)?.min(left);
        }
        Ok(amount - left)
    }
}

/// The face of a block at offset `d` from the player that looks toward the player (the longest axis of the offset).
pub fn face_toward(d: [f64; 3]) -> &'static str {
    let a = [d[0].abs(), d[1].abs(), d[2].abs()];
    if a[0] >= a[1] && a[0] >= a[2] {
        if d[0] < 0.0 { "east" } else { "west" }
    } else if a[1] >= a[2] {
        if d[1] < 0.0 { "up" } else { "down" }
    } else if d[2] < 0.0 {
        "south"
    } else {
        "north"
    }
}

/// The debug tool's view (ControllerRenderer): with the debug tool in the main hand, each storage of the network shows its
/// order and slot count on the face nearest the player. The controller draws it all, since only it knows the network.
pub fn debug_view(
    host: &dyn ClientHost,
    controller: [i32; 3],
    draw: &mut dyn BlockEntityDrawContext,
) {
    let Some(local) = host.local_player() else {
        return;
    };
    let held = crate::render::held_stack(host);
    let host: &dyn Host = host;
    if held.is_none_or(|h| crate::contents::item_id(host, &h) != DEBUG_TOOL) {
        return;
    }
    let (Some(dim), Some(eye)) = (local.dimension(host), local.entity().position(host)) else {
        return;
    };
    let Some(level) = host.level(&dim.to_string()) else {
        return;
    };
    let net = lock(&CLIENT).get((level, controller), draw.game_time(), CLIENT_TTL, || {
        let linked = linked_to(host, level, controller);
        (
            search(controller, &linked, &|p| client_role(host, level, p)),
            linked,
        )
    });
    for (order, p) in net.storages.iter().enumerate() {
        let Some(kind) = storage::kind_of_block(&storage::block_id(host, level, *p)) else {
            continue;
        };
        let face = face_toward([
            f64::from(p[0]) + 0.5 - eye[0],
            f64::from(p[1]) + 0.5 - eye[1],
            f64::from(p[2]) + 0.5 - eye[2],
        ]);
        let at = [
            (p[0] - controller[0]) as f32,
            (p[1] - controller[1]) as f32,
            (p[2] - controller[2]) as f32,
        ];
        crate::render::debug_text(draw, at, face, order + 1, kind.slot_count());
    }
}

/// What the controller's use puts in (ControllerBlock.use, `depositStacks`): the held stack, or with `all` every
/// inventory stack, of the items the network already holds. Returns whether anything moved.
pub fn deposit(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    level: LevelRef,
    controller: [i32; 3],
    all: bool,
) -> bool {
    let net = NetHandler::new(level, controller, false, View::Full);
    let mut held = Vec::new();
    for i in 0..net.size(host) {
        if let Some(r) = net.resource(host, i) {
            held.push(r.item());
        }
    }
    let inv = places::inventory(host, player);
    let hand = player.selected_slot(&*host).unwrap_or(0) as u32;
    let slots: Vec<u32> = if all { (0..36).collect() } else { vec![hand] };
    let mut moved = false;
    for i in slots {
        let Some(stack) = inv.get(i as usize).cloned().flatten() else {
            continue;
        };
        if !held.contains(&stack.item()) && !net.memorizes(&*host, &stack) {
            continue;
        }
        let res = ItemResource::of(&stack);
        let n = stack.count();
        let put = host
            .transaction(|host, tx| Ok::<u32, ()>(net.insert(host, tx, &res, n).unwrap_or(0)))
            .unwrap_or(0);
        if put > 0 {
            places::set_slot(
                host,
                player,
                i,
                crate::contents::with_count(&stack, n - put),
            );
            moved = true;
        }
    }
    moved
}

/// `DyeColor.LIGHT_BLUE.getTextureDiffuseColors()` and `LIME`'s, as opaque ARGB: the controller the tool holds, and
/// the blocks linked to a controller.
const SELECTED_COLOR: u32 = 0xFF66_99D8;
const LINKED_COLOR: u32 = 0xFF80_CC1A;
/// The controllers drawn with the storage tool in Link mode, with the game time they were last seen at and whether
/// the tool holds them. Only the controller's renderer knows it is in view, and only the level stage can draw lines.
static LINK_VIEW: LazyLock<Mutex<HashMap<(LevelRef, [i32; 3]), (i64, bool)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// How many ticks a controller stays drawn after its renderer last ran (the stage may run before the renderer).
const LINK_VIEW_TTL: i64 = 2;

/// The twelve edges of the box `min`..`max`.
fn box_edges(min: [f64; 3], max: [f64; 3]) -> Vec<([f64; 3], [f64; 3])> {
    let corner = |i: usize| {
        [
            if i & 1 == 0 { min[0] } else { max[0] },
            if i & 2 == 0 { min[1] } else { max[1] },
            if i & 4 == 0 { min[2] } else { max[2] },
        ]
    };
    (0..8usize)
        .flat_map(|i| (0..3).map(move |b| (i, i | (1 << b))))
        .filter(|(i, j)| i != j)
        .map(|(i, j)| (corner(i), corner(j)))
        .collect()
}

/// What the storage tool in Link mode shows (ControllerRenderer): an outline of the controller the tool holds, and
/// for each block linked to a controller a line from the controller's centre to its own and its outline. Shapes are
/// taken as whole blocks.
fn link_geometry(
    controller: [i32; 3],
    selected: bool,
    linked: &[[i32; 3]],
) -> Vec<([f64; 3], [f64; 3], u32)> {
    let at = |p: [i32; 3]| [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])];
    let outline = |p: [i32; 3], color: u32| {
        let min = at(p);
        box_edges(min, [min[0] + 1.0, min[1] + 1.0, min[2] + 1.0])
            .into_iter()
            .map(move |(a, b)| (a, b, color))
    };
    let mut out = Vec::new();
    if selected {
        out.extend(outline(controller, SELECTED_COLOR));
    }
    for &p in linked {
        let centre = |p: [i32; 3]| at(p).map(|v| v + 0.5);
        out.push((centre(controller), centre(p), LINKED_COLOR));
        out.extend(outline(p, LINKED_COLOR));
    }
    out
}

/// Notes that the controller at `pos` is in view while the player holds the storage tool in Link mode.
fn note_link_view(host: &dyn ClientHost, pos: [i32; 3], draw: &mut dyn BlockEntityDrawContext) {
    let Some(local) = host.local_player() else {
        return;
    };
    let Some(held) = crate::render::held_stack(host) else {
        return;
    };
    let host: &dyn Host = host;
    if crate::contents::item_id(host, &held) != crate::tool::TOOL
        || crate::tool::mode(&held) != crate::tool::Mode::Link
    {
        return;
    }
    let Some(dim) = local.dimension(host) else {
        return;
    };
    let Some(level) = host.level(&dim.to_string()) else {
        return;
    };
    let selected = crate::tool::controller_link(&held) == Some(pos);
    lock(&LINK_VIEW).insert((level, pos), (draw.game_time(), selected));
}

/// The link lines and outlines of the controllers seen lately, over the world (the original's line type has no
/// depth test).
pub fn draw_links(
    host: &dyn ClientHost,
    view: &mod_api::WorldRenderView,
    draw: &mut dyn mod_api::WorldDrawContext,
) {
    let host: &dyn Host = host;
    let seen: Vec<_> = {
        let mut m = lock(&LINK_VIEW);
        m.retain(|_, (t, _)| view.game_time.saturating_sub(*t) <= LINK_VIEW_TTL);
        m.iter().map(|(k, v)| (*k, v.1)).collect()
    };
    let lines: Vec<mod_api::WorldLine> = seen
        .into_iter()
        .flat_map(|((level, pos), selected)| {
            link_geometry(pos, selected, &linked_to(host, level, pos))
        })
        .map(|(a, b, color)| {
            mod_api::WorldLine::new(a, b, color, 2.0, mod_api::WorldDepth::ThroughWalls)
        })
        .collect();
    if !lines.is_empty() {
        let _ = draw.lines(&lines);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const C: [i32; 3] = [0, 0, 0];

    fn world(blocks: &[([i32; 3], Role)]) -> impl Fn([i32; 3]) -> Role + use<> {
        let m: HashMap<_, _> = blocks.iter().copied().collect();
        move |p| m.get(&p).copied().unwrap_or(Role::Other)
    }

    const S: Role = Role::Storage { linked: None };

    #[test]
    fn a_box_has_twelve_edges_each_along_one_axis() {
        let e = box_edges([0.0; 3], [1.0; 3]);
        assert_eq!(e.len(), 12);
        for (a, b) in e {
            let differing = (0..3).filter(|&i| a[i] != b[i]).count();
            assert_eq!(differing, 1);
        }
    }

    #[test]
    fn links_show_a_line_and_an_outline_each_and_the_held_controller_an_outline() {
        let none = link_geometry(C, false, &[]);
        assert!(none.is_empty());
        let held = link_geometry(C, true, &[]);
        assert_eq!(held.len(), 12);
        assert!(held.iter().all(|g| g.2 == SELECTED_COLOR));
        let linked = link_geometry(C, false, &[[3, 0, 0], [0, 2, 0]]);
        assert_eq!(linked.len(), 2 * 13);
        let line = linked[0];
        assert_eq!((line.0, line.1), ([0.5; 3], [3.5, 0.5, 0.5]));
    }

    #[test]
    fn storages_conduct_outwards_from_the_controller() {
        let w = world(&[
            ([1, 0, 0], S),
            ([2, 0, 0], S),
            ([2, 1, 0], S),
            ([4, 0, 0], S),
        ]);
        let net = search(C, &[], &w);
        assert_eq!(net.storages.len(), 3);
        assert!(!net.storages.contains(&[4, 0, 0]));
    }

    #[test]
    fn the_far_edge_of_the_range_is_not_crossed() {
        let blocks: Vec<_> = (1..=17).map(|x| ([x, 0, 0], S)).collect();
        let net = search(C, &[], &world(&blocks));
        assert_eq!(net.storages.len(), RANGE as usize);
    }

    #[test]
    fn a_double_chests_empty_half_conducts_without_joining() {
        let w = world(&[([1, 0, 0], Role::Conductor), ([2, 0, 0], S)]);
        let net = search(C, &[], &w);
        assert_eq!(net.storages, vec![[2, 0, 0]]);
    }

    #[test]
    fn a_linked_storage_joins_from_afar_and_does_not_conduct() {
        let far = [9, 9, 9];
        let w = world(&[(far, Role::Storage { linked: Some(C) }), ([10, 9, 9], S)]);
        let net = search(C, &[far], &w);
        assert_eq!(net.storages, vec![far]);
    }

    #[test]
    fn a_storage_linked_to_another_controller_stays_out() {
        let w = world(&[(
            [1, 0, 0],
            Role::Storage {
                linked: Some([5, 5, 5]),
            },
        )]);
        assert!(search(C, &[], &w).storages.is_empty());
    }

    #[test]
    fn a_linked_link_block_joins_what_is_behind_it() {
        let link = [8, 0, 0];
        let w = world(&[
            (
                link,
                Role::Link {
                    linked: Some(C),
                    behind: [8, 1, 0],
                },
            ),
            ([8, 1, 0], S),
            ([8, 2, 0], S),
        ]);
        let net = search(C, &[link], &w);
        assert_eq!(net.links, vec![link]);
        assert_eq!(net.storages.len(), 2);
    }

    #[test]
    fn an_unlinked_link_block_does_nothing() {
        let w = world(&[
            (
                [1, 0, 0],
                Role::Link {
                    linked: None,
                    behind: [2, 0, 0],
                },
            ),
            ([2, 0, 0], S),
        ]);
        let net = search(C, &[], &w);
        assert!(net.links.is_empty() && net.storages.is_empty());
    }

    #[test]
    fn io_blocks_join_beside_a_conducting_storage_but_do_not_conduct() {
        let w = world(&[
            ([1, 0, 0], S),
            ([1, 1, 0], Role::Io { linked: None }),
            ([1, 2, 0], S),
        ]);
        let net = search(C, &[], &w);
        assert_eq!(net.ios, vec![[1, 1, 0]]);
        assert_eq!(net.storages, vec![[1, 0, 0]]);
    }

    #[test]
    fn an_io_block_finds_the_controller_through_its_storages() {
        let w = world(&[([1, 0, 0], S), ([2, 0, 0], S)]);
        let found = find_controller([3, 0, 0], &w, &|p| p == C);
        assert_eq!(found, Some(C));
        assert_eq!(find_controller([3, 5, 0], &w, &|p| p == C), None);
    }

    #[test]
    fn a_kept_network_is_reused_and_dropped_when_a_block_in_range_changes() {
        let mut cache = NetCache::new();
        let mut searches = 0;
        let mut get = |cache: &mut NetCache<[i32; 3]>, with: bool| {
            cache.get(C, 0, i64::MAX, || {
                searches += 1;
                let blocks: Vec<_> = if with { vec![([1, 0, 0], S)] } else { vec![] };
                (search(C, &[], &world(&blocks)), vec![])
            })
        };
        assert_eq!(get(&mut cache, true).storages.len(), 1);
        // Broken, but nothing told yet: the kept network answers.
        assert_eq!(get(&mut cache, false).storages.len(), 1);
        cache.changed([30, 0, 0], |_| C);
        assert_eq!(get(&mut cache, false).storages.len(), 1);
        cache.changed([1, 0, 0], |_| C);
        assert_eq!(get(&mut cache, false).storages.len(), 0);
        assert_eq!(searches, 2);
    }

    #[test]
    fn a_change_at_a_linked_position_drops_the_network_wherever_it_is() {
        let mut cache: NetCache<[i32; 3]> = NetCache::new();
        let far = [90, 0, 0];
        cache.get(C, 0, i64::MAX, || (Net::default(), vec![far]));
        cache.changed(far, |_| C);
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn the_client_keeps_a_network_only_for_its_ttl() {
        let mut cache: NetCache<[i32; 3]> = NetCache::new();
        let mut n = 0;
        for now in [0, 5, 25] {
            cache.get(C, now, 20, || {
                n += 1;
                (Net::default(), vec![])
            });
        }
        assert_eq!(n, 2);
    }

    #[test]
    fn the_debug_text_goes_on_the_face_toward_the_player() {
        assert_eq!(face_toward([-3.0, 0.5, 1.0]), "east");
        assert_eq!(face_toward([2.0, 0.5, -1.0]), "west");
        assert_eq!(face_toward([0.5, -4.0, 1.0]), "up");
        assert_eq!(face_toward([0.5, 0.2, 3.0]), "north");
        assert_eq!(face_toward([0.5, 0.2, -3.0]), "south");
    }
}
