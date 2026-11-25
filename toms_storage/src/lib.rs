//! toms_storage: Tom's Simple Storage 1.7.1 (tom5454; MIT) for Sunrise. data/ and assets/ are the content; all of
//! the rules are the typed mod API: no `[[native.mixin]]` row and no game module.
//!
//! - InventoryConnectorBlockEntity: the inventories touching the connector, or touching those (through trims as
//!   well), within 16 blocks of it, are one network; double chests count once. Cable connectors put the
//!   inventory they face on the network of the connector their cables reach (AbstractInventoryCableConnector-
//!   BlockEntity, up to 2048 cables). The network is worked out when it is asked for, not every 20 ticks.
//! - The connector, every cable connector on its cables and an inventory proxy answer the item capability
//!   (`ITEMS`): the network for the first two (so a hopper feeds the network), the faced inventory for the proxy
//!   (InventoryProxyBlockEntity, without its filter slot).
//! - StorageTerminalBlockEntity / CraftingTerminalBlockEntity: a terminal reads the inventory it was put on (a
//!   connector or cable connector: the whole network). Its screen is a server menu, a chest screen of six rows,
//!   in place of the jar's own screen: the items by page with their stored amount, sorting (amount, name, mod),
//!   a search typed straight into the screen, and for the crafting terminal a 3x3 grid whose result is crafted
//!   from it and refilled from the network. A mod menu's cursor and player slots can't be written for the
//!   player who hosts the world (the engine backs them for LAN guests only), so there is no cursor: a click
//!   on a stored item moves it to the inventory (a stack, half with the right button), a click on an inventory
//!   stack puts it in (one with the right button). On the crafting terminal a right click on a stored item
//!   picks it, and a click on an empty grid slot puts one of it there.
//! - WirelessTerminalItem (16 blocks, the terminal looked at) and AdvWirelessTerminalItem (bound by sneaking on a
//!   terminal, 64 blocks).
//! - The block placements: terminals against the clicked block, cable connectors and the six-way blocks facing it.
//!
//! The ranges are the original's defaults. Not ported: the inventory hopper, level
//! emitter, open crate, filters and filtered connector rules, inventory link channels, the beacon range bonus,
//! painted blocks; the terminal's own screen (search history, JEI/REI fill, terminal sizes).
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use mod_api::{
    BlockEntityPart, BlockPart, BlockSide, BlockState, Capability, ClientHost, ClientTickPostEvent, Compound, Direction,
    Flow, Hand, Hooks, Host, HostSide, HostTransaction, IPayloadContext, IPayloadHandler, ITEMS, Identifier,
    InteractionResult, ItemResource, ItemStack, LevelHost, LevelRef, LevelSide, MenuClickedEvent, MenuPart, MenuRef,
    MenuRemovedEvent, MenuReplace, Overanswered, PayloadBytes, PayloadHandleError, PayloadId, PlacementSource,
    PlayerRef, RecipeInput, Registrar, RegistrationContext, ResourceHandler, RightClickBlockEvent,
    RightClickItemEvent, ScreenCharTypedPreEvent, ScreenKeyPressedPreEvent, ServerHost, ServerTickEvent,
    ServerTickPhase, StateForPlacementEvent, Tag, Text, TransactionContext, TransactionError,
};

const NS: &str = "toms_storage:";
const TERMINAL: &str = "toms_storage:ts.storage_terminal";
const CRAFTING: &str = "toms_storage:ts.crafting_terminal";
const CONNECTOR: &str = "toms_storage:ts.inventory_connector";
const PROXY: &str = "toms_storage:ts.inventory_proxy";
const CABLE_CONNECTORS: [&str; 3] = [
    "toms_storage:ts.inventory_cable_connector",
    "toms_storage:ts.inventory_cable_connector_filtered",
    "toms_storage:ts.inventory_cable_connector_framed",
];
const CABLES: [&str; 2] = ["toms_storage:ts.inventory_cable", "toms_storage:ts.inventory_cable_framed"];
const TRIMS: [&str; 1] = ["toms_storage:ts.trim"];
const SIX_WAY: [&str; 3] = ["toms_storage:ts.inventory_proxy", "toms_storage:ts.open_crate", "toms_storage:ts.inventory_hopper_basic"];
const WIRELESS: &str = "toms_storage:ts.wireless_terminal";
const ADV_WIRELESS: &str = "toms_storage:ts.adv_wireless_terminal";

// The original's defaults (config/toms_storage.json).
const INV_RANGE: i32 = 16;
const WIRELESS_RANGE: f64 = 16.0;
const ADV_WIRELESS_RANGE: f64 = 64.0;
const MAX_CABLES: usize = 2048;

const SEARCH_CHANNEL: &str = "toms_storage:search";
const STORAGE_TITLE: &str = "Storage Terminal";
const CRAFTING_TITLE: &str = "Crafting Terminal";
const DIRS: [Direction; 6] = [Direction::Down, Direction::Up, Direction::North, Direction::South, Direction::West, Direction::East];

type Handler = Capability<dyn ResourceHandler<ItemResource>>;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn vec_of(d: Direction) -> [i32; 3] {
    match d {
        Direction::Down => [0, -1, 0],
        Direction::Up => [0, 1, 0],
        Direction::North => [0, 0, -1],
        Direction::South => [0, 0, 1],
        Direction::West => [-1, 0, 0],
        Direction::East => [1, 0, 0],
    }
}

fn opposite(d: Direction) -> Direction {
    match d {
        Direction::Down => Direction::Up,
        Direction::Up => Direction::Down,
        Direction::North => Direction::South,
        Direction::South => Direction::North,
        Direction::West => Direction::East,
        Direction::East => Direction::West,
    }
}

fn dir_named(s: &str) -> Option<Direction> {
    Some(match s {
        "down" => Direction::Down,
        "up" => Direction::Up,
        "north" => Direction::North,
        "south" => Direction::South,
        "west" => Direction::West,
        "east" => Direction::East,
        _ => return None,
    })
}

fn dir_name(d: Direction) -> &'static str {
    match d {
        Direction::Down => "down",
        Direction::Up => "up",
        Direction::North => "north",
        Direction::South => "south",
        Direction::West => "west",
        Direction::East => "east",
    }
}

fn add(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn dist2(a: [i32; 3], b: [i32; 3]) -> i32 {
    (a[0] - b[0]).pow(2) + (a[1] - b[1]).pow(2) + (a[2] - b[2]).pow(2)
}

fn block_id(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> String {
    level.block(host, pos).and_then(|k| k.id(host).map(str::to_owned)).unwrap_or_default()
}

fn is_cable_like(id: &str) -> bool {
    CABLES.contains(&id) || CABLE_CONNECTORS.contains(&id)
}

/// An inventory of the network: a block and the side it is reached from.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct Inv {
    pos: [i32; 3],
    side: Option<Direction>,
}

fn cap(host: &mut dyn ServerHost, level: LevelRef, inv: Inv) -> Option<Handler> {
    level.capability(host, &ITEMS, inv.pos, inv.side)
}

/// The other half of a double chest (ChestBlock.getConnectedDirection), so it is counted once.
fn chest_other_half(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> Option<[i32; 3]> {
    let kind = level.block_property(host, pos, "type")?;
    let facing = dir_named(&level.block_property(host, pos, "facing")?)?;
    let clockwise = |d: Direction| match d {
        Direction::North => Direction::East,
        Direction::East => Direction::South,
        Direction::South => Direction::West,
        _ => Direction::North,
    };
    let d = match kind {
        "left" => clockwise(facing),
        "right" => opposite(clockwise(facing)),
        _ => return None,
    };
    Some(add(pos, vec_of(d)))
}

/// InventoryConnectorBlockEntity.updateServer: the inventories of the connector at `conn`.
fn network(host: &mut dyn ServerHost, level: LevelRef, conn: [i32; 3]) -> Vec<Inv> {
    let mut out = Vec::new();
    let mut seen: HashSet<[i32; 3]> = HashSet::from([conn]);
    let mut stack = vec![conn];
    let range2 = INV_RANGE * INV_RANGE;
    while let Some(cp) = stack.pop() {
        for d in DIRS {
            let p = add(cp, vec_of(d));
            if seen.contains(&p) || dist2(p, conn) >= range2 {
                continue;
            }
            seen.insert(p);
            let id = block_id(&*host, level, p);
            if TRIMS.contains(&id.as_str()) {
                stack.push(p);
                continue;
            }
            if id.is_empty() || id == "minecraft:air" || id.starts_with(NS) {
                continue;
            }
            if host.block_entity(level, p).is_none() {
                continue;
            }
            let inv = Inv { pos: p, side: Some(opposite(d)) };
            if cap(host, level, inv).is_none() {
                continue;
            }
            if let Some(o) = chest_other_half(&*host, level, p)
                && block_id(&*host, level, o) == id
                && seen.insert(o)
            {
                stack.push(o);
            }
            out.push(inv);
            stack.push(p);
        }
    }
    for inv in linked(host, level, conn) {
        if !out.iter().any(|i| i.pos == inv.pos) {
            out.push(inv);
        }
    }
    out
}

fn facing_of(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> Option<Direction> {
    dir_named(&level.block_property(host, pos, "facing")?)
}

/// The cable network next to `start`: each cable connector's faced inventory (not one facing `start` itself,
/// which reads the network instead), and the connector the cables reach.
fn cable_walk(host: &dyn Host, level: LevelRef, start: [i32; 3], skip_facing: bool) -> (Vec<Inv>, Option<[i32; 3]>) {
    let mut invs = Vec::new();
    let mut master = None;
    let mut seen: HashSet<[i32; 3]> = HashSet::from([start]);
    let mut stack = Vec::new();
    let start_facing = if skip_facing { facing_of(host, level, start) } else { None };
    for d in DIRS {
        if Some(d) == start_facing {
            continue;
        }
        stack.push(add(start, vec_of(d)));
    }
    while let Some(p) = stack.pop() {
        if !seen.insert(p) || seen.len() > MAX_CABLES {
            continue;
        }
        let id = block_id(host, level, p);
        if id == CONNECTOR {
            master.get_or_insert(p);
            continue;
        }
        if !is_cable_like(&id) {
            continue;
        }
        let facing = if CABLE_CONNECTORS.contains(&id.as_str()) { facing_of(host, level, p) } else { None };
        if let Some(f) = facing {
            let target = add(p, vec_of(f));
            let tid = block_id(host, level, target);
            if target != start && !tid.starts_with(NS) && !tid.is_empty() && tid != "minecraft:air" {
                invs.push(Inv { pos: target, side: Some(opposite(f)) });
            }
        }
        for d in DIRS {
            if Some(d) != facing {
                stack.push(add(p, vec_of(d)));
            }
        }
    }
    (invs, master)
}

fn linked(host: &mut dyn ServerHost, level: LevelRef, conn: [i32; 3]) -> Vec<Inv> {
    let (invs, _) = cable_walk(&*host, level, conn, false);
    invs.into_iter().filter(|i| cap(host, level, *i).is_some()).collect()
}

/// What a terminal, a cable connector or a hopper reaches through the block at `target` from `side`.
fn reach(host: &mut dyn ServerHost, level: LevelRef, target: [i32; 3], side: Direction) -> Vec<Inv> {
    let id = block_id(&*host, level, target);
    if id == CONNECTOR {
        return network(host, level, target);
    }
    if CABLE_CONNECTORS.contains(&id.as_str()) {
        let (_, master) = cable_walk(&*host, level, target, true);
        return master.map(|m| network(host, level, m)).unwrap_or_default();
    }
    if id == PROXY {
        let Some(f) = facing_of(&*host, level, target) else { return Vec::new() };
        let inv = Inv { pos: add(target, vec_of(f)), side: Some(opposite(f)) };
        return if cap(host, level, inv).is_some() { vec![inv] } else { Vec::new() };
    }
    let inv = Inv { pos: target, side: Some(side) };
    if cap(host, level, inv).is_some() { vec![inv] } else { Vec::new() }
}

/// StorageTerminalBlockEntity.updateServer: the inventory the terminal was put on.
fn terminal_source(host: &mut dyn ServerHost, level: LevelRef, term: [i32; 3]) -> Vec<Inv> {
    let reader: &dyn Host = &*host;
    let d = match level.block_property(reader, term, "pos").as_deref() {
        Some("up") => Direction::Up,
        Some("down") => Direction::Down,
        _ => facing_of(reader, level, term).unwrap_or(Direction::North),
    };
    reach(host, level, add(term, vec_of(d)), opposite(d))
}

/// The handlers of a network in order, each with its size.
fn handlers(host: &mut dyn ServerHost, level: LevelRef, invs: &[Inv]) -> Vec<(Handler, usize)> {
    let mut out = Vec::new();
    for i in invs {
        if let Some(h) = cap(host, level, *i) {
            let n = h.size(host);
            out.push((h, n));
        }
    }
    out
}

/// What a handler answers to a transfer: how much moved, or the refusal that rolls the journal back.
type Moved = Result<u32, TransactionError<Overanswered>>;

/// MergedStorage: the network as one handler.
struct Merged {
    level: LevelRef,
    /// Where the network starts: the connector, or the cable connector whose cables reach one.
    start: [i32; 3],
    /// The side asked from; the asker's own inventory is not part of what it reaches (a hopper on the connector).
    asker: Option<Direction>,
}

impl Merged {
    fn invs(&self, host: &mut dyn ServerHost) -> Vec<Inv> {
        let id = block_id(&*host, self.level, self.start);
        if id == CONNECTOR {
            let asker = self.asker.map(|d| add(self.start, vec_of(d)));
            return network(host, self.level, self.start).into_iter().filter(|i| Some(i.pos) != asker).collect();
        }
        let (_, master) = cable_walk(&*host, self.level, self.start, true);
        master.map(|m| network(host, self.level, m)).unwrap_or_default()
    }

    fn at(&self, host: &mut dyn ServerHost, index: usize) -> Option<(Handler, usize)> {
        let invs = self.invs(host);
        let mut i = index;
        for (h, n) in handlers(host, self.level, &invs) {
            if i < n {
                return Some((h, i));
            }
            i -= n;
        }
        None
    }
}

impl ResourceHandler<ItemResource> for Merged {
    fn size(&self, host: &mut dyn ServerHost) -> usize {
        let invs = self.invs(host);
        handlers(host, self.level, &invs).iter().map(|(_, n)| n).sum()
    }
    fn resource(&self, host: &mut dyn ServerHost, index: usize) -> Option<ItemResource> {
        let (h, i) = self.at(host, index)?;
        h.resource(host, i)
    }
    fn amount(&self, host: &mut dyn ServerHost, index: usize) -> u32 {
        self.at(host, index).map_or(0, |(h, i)| h.amount(host, i))
    }
    fn capacity(&self, host: &mut dyn ServerHost, index: usize, resource: Option<&ItemResource>) -> u32 {
        self.at(host, index).map_or(0, |(h, i)| h.capacity(host, i, resource))
    }
    fn is_valid(&self, host: &mut dyn ServerHost, index: usize, resource: &ItemResource) -> bool {
        self.at(host, index).is_some_and(|(h, i)| h.is_valid(host, i, resource))
    }
    fn insert_at(&self, host: &mut dyn ServerHost, tx: &TransactionContext, index: usize, resource: &ItemResource, amount: u32) -> Moved {
        if !host.transaction_matches(Some(tx)) {
            return Err(TransactionError::InvalidContext);
        }
        match self.at(host, index) {
            Some((h, i)) => h.insert_at(host, tx, i, resource, amount),
            None => Ok(0),
        }
    }
    fn extract_at(&self, host: &mut dyn ServerHost, tx: &TransactionContext, index: usize, resource: &ItemResource, amount: u32) -> Moved {
        if !host.transaction_matches(Some(tx)) {
            return Err(TransactionError::InvalidContext);
        }
        match self.at(host, index) {
            Some((h, i)) => h.extract_at(host, tx, i, resource, amount),
            None => Ok(0),
        }
    }
    fn insert(&self, host: &mut dyn ServerHost, tx: &TransactionContext, resource: &ItemResource, amount: u32) -> Moved {
        if !host.transaction_matches(Some(tx)) {
            return Err(TransactionError::InvalidContext);
        }
        let invs = self.invs(host);
        net_insert(host, tx, self.level, &invs, resource, amount)
    }
    fn extract(&self, host: &mut dyn ServerHost, tx: &TransactionContext, resource: &ItemResource, amount: u32) -> Moved {
        if !host.transaction_matches(Some(tx)) {
            return Err(TransactionError::InvalidContext);
        }
        let invs = self.invs(host);
        net_extract(host, tx, self.level, &invs, resource, amount)
    }
}

/// The proxy: what the faced inventory answers.
struct Forward {
    level: LevelRef,
    inv: Inv,
}

impl Forward {
    /// The faced inventory (not another block of this mod: the proxy never feeds the network back into itself).
    fn target(&self, host: &mut dyn ServerHost) -> Option<Handler> {
        if block_id(&*host, self.level, self.inv.pos).starts_with(NS) {
            return None;
        }
        cap(host, self.level, self.inv)
    }
}

impl ResourceHandler<ItemResource> for Forward {
    fn size(&self, host: &mut dyn ServerHost) -> usize {
        self.target(host).map_or(0, |h| h.size(host))
    }
    fn resource(&self, host: &mut dyn ServerHost, index: usize) -> Option<ItemResource> {
        self.target(host)?.resource(host, index)
    }
    fn amount(&self, host: &mut dyn ServerHost, index: usize) -> u32 {
        self.target(host).map_or(0, |h| h.amount(host, index))
    }
    fn capacity(&self, host: &mut dyn ServerHost, index: usize, resource: Option<&ItemResource>) -> u32 {
        self.target(host).map_or(0, |h| h.capacity(host, index, resource))
    }
    fn is_valid(&self, host: &mut dyn ServerHost, index: usize, resource: &ItemResource) -> bool {
        self.target(host).is_some_and(|h| h.is_valid(host, index, resource))
    }
    fn insert_at(&self, host: &mut dyn ServerHost, tx: &TransactionContext, index: usize, resource: &ItemResource, amount: u32) -> Moved {
        if !host.transaction_matches(Some(tx)) {
            return Err(TransactionError::InvalidContext);
        }
        match self.target(host) {
            Some(h) => h.insert_at(host, tx, index, resource, amount),
            None => Ok(0),
        }
    }
    fn extract_at(&self, host: &mut dyn ServerHost, tx: &TransactionContext, index: usize, resource: &ItemResource, amount: u32) -> Moved {
        if !host.transaction_matches(Some(tx)) {
            return Err(TransactionError::InvalidContext);
        }
        match self.target(host) {
            Some(h) => h.extract_at(host, tx, index, resource, amount),
            None => Ok(0),
        }
    }
}

/// MergedStorage.insert: first onto stacks of the same item, then wherever it goes.
fn net_insert(host: &mut dyn ServerHost, tx: &TransactionContext, level: LevelRef, invs: &[Inv], res: &ItemResource, amount: u32) -> Moved {
    let hs = handlers(host, level, invs);
    let mut went = 0;
    for (h, n) in &hs {
        for i in 0..*n {
            if went >= amount {
                return Ok(went);
            }
            if h.resource(host, i).as_ref() == Some(res) {
                went += h.insert_at(host, tx, i, res, amount - went)?;
            }
        }
    }
    for (h, _) in &hs {
        if went >= amount {
            break;
        }
        went += h.insert(host, tx, res, amount - went)?;
    }
    Ok(went)
}

fn net_extract(host: &mut dyn ServerHost, tx: &TransactionContext, level: LevelRef, invs: &[Inv], res: &ItemResource, amount: u32) -> Moved {
    let mut got = 0;
    for (h, n) in handlers(host, level, invs) {
        for i in 0..n {
            if got >= amount {
                return Ok(got);
            }
            if h.resource(host, i).as_ref() == Some(res) {
                got += h.extract_at(host, tx, i, res, amount - got)?;
            }
        }
    }
    Ok(got)
}

/// One kind of item on the network.
#[derive(Clone)]
struct Entry {
    key: String,
    id: String,
    sample: ItemStack,
    total: u64,
}

fn res_key(host: &dyn Host, stack: &ItemStack) -> String {
    format!("{}|{:?}", stack.item().id(host).unwrap_or_default(), stack.components().map(|c| c.as_nbt()))
}

fn contents(host: &mut dyn ServerHost, level: LevelRef, invs: &[Inv]) -> Vec<Entry> {
    let mut map: HashMap<String, Entry> = HashMap::new();
    for (h, n) in handlers(host, level, invs) {
        for i in 0..n {
            let Some(r) = h.resource(host, i) else { continue };
            let amount = h.amount(host, i);
            if amount == 0 {
                continue;
            }
            let Some(sample) = r.to_stack(1) else { continue };
            let key = res_key(&*host, &sample);
            let id = sample.item().id(&*host).unwrap_or_default().to_owned();
            map.entry(key.clone()).or_insert(Entry { key, id, sample, total: 0 }).total += u64::from(amount);
        }
    }
    map.into_values().collect()
}

fn max_stack(host: &dyn Host, stack: &ItemStack) -> u32 {
    host.max_stack_size(stack.item()).unwrap_or(64).max(1)
}

fn item(host: &dyn Host, id: &str) -> Option<ItemStack> {
    let key = host.item_key(&Identifier::parse(id).ok()?)?;
    ItemStack::new(key, 1)
}

fn named(mut stack: ItemStack, name: &str, lore: &[String]) -> ItemStack {
    let mut patch = Compound::new();
    patch.insert("minecraft:custom_name".into(), Tag::String(name.into()));
    if !lore.is_empty() {
        patch.insert("minecraft:lore".into(), Tag::List(lore.iter().map(|l| Tag::String(l.as_str().into())).collect()));
    }
    let _ = stack.apply_components(&patch);
    stack
}

// The chest screen's layout, six rows of nine.
const STORAGE_ITEMS: std::ops::Range<usize> = 0..45;
const CRAFTING_ITEMS: std::ops::Range<usize> = 0..27;
const GRID: [usize; 9] = [28, 29, 30, 37, 38, 39, 46, 47, 48];
const RESULT: usize = 41;
const OWN_SLOTS: usize = 54;

struct Layout {
    items: std::ops::Range<usize>,
    prev: usize,
    next: usize,
    sort: usize,
    search: usize,
    info: usize,
    clear: Option<usize>,
}

fn layout(crafting: bool) -> Layout {
    if crafting {
        Layout { items: CRAFTING_ITEMS, prev: 27, next: 35, sort: 33, search: 36, info: 44, clear: Some(53) }
    } else {
        Layout { items: STORAGE_ITEMS, prev: 45, next: 53, sort: 47, search: 49, info: 51, clear: None }
    }
}

const SORTS: [&str; 3] = ["Amount", "Name", "Mod"];

struct Session {
    window: i32,
    dim: String,
    term: [i32; 3],
    crafting: bool,
    page: usize,
    sort: usize,
    search: String,
    view: Vec<Entry>,
    grid: [Option<ItemStack>; 9],
    /// The crafting terminal's picked item: a click on an empty grid slot puts one of it there.
    pick: Option<ItemStack>,
}

impl Session {
    fn title(&self) -> Text {
        let base = if self.crafting { CRAFTING_TITLE } else { STORAGE_TITLE };
        if self.search.is_empty() {
            Text::literal(base)
        } else {
            Text::literal(format!("{base} - \"{}\"", self.search))
        }
    }
    fn per_page(&self) -> usize {
        layout(self.crafting).items.len()
    }
    fn pages(&self) -> usize {
        self.view.len().div_ceil(self.per_page()).max(1)
    }
}

/// Whether the item `id` answers the lowercase search `s`: a part of its name (spaces for underscores), or with a
/// leading `@` a part of its mod's namespace.
fn search_matches(id: &str, s: &str) -> bool {
    let (ns, path) = id.split_once(':').unwrap_or(("minecraft", id));
    match s.strip_prefix('@') {
        Some(m) => ns.contains(m),
        None => path.replace('_', " ").contains(s) || path.contains(s),
    }
}

/// The searched and sorted view (AbstractStorageTerminalScreen.updateSearch: `@mod` searches by mod).
fn filter_sort(mut v: Vec<Entry>, search: &str, sort: usize) -> Vec<Entry> {
    let s = search.to_lowercase();
    if !s.is_empty() {
        v.retain(|e| search_matches(&e.id, &s));
    }
    match sort {
        0 => v.sort_by(|a, b| b.total.cmp(&a.total).then(a.id.cmp(&b.id))),
        1 => v.sort_by(|a, b| a.id.split(':').nth(1).cmp(&b.id.split(':').nth(1)).then(a.key.cmp(&b.key))),
        _ => v.sort_by(|a, b| a.id.cmp(&b.id).then(a.key.cmp(&b.key))),
    }
    v
}

fn group(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The player's main inventory, 0..9 the hotbar and 9..36 the rest, and whether every slot could be read as an
/// owned stack (a slot the game can't hand to a mod reads as empty, but is not).
fn readable(host: &dyn Host, player: PlayerRef) -> Option<(Vec<Option<ItemStack>>, bool)> {
    player.with_inventory(host, |inventory| {
        let mut inv = vec![None; 36];
        let mut whole = true;
        for (slot, view) in inventory.iter() {
            if (slot as usize) < 36 {
                match view.project() {
                    Some(stack) => inv[slot as usize] = Some(stack),
                    None => whole = false,
                }
            }
        }
        (inv, whole)
    })
}

fn inventory(host: &dyn Host, player: PlayerRef) -> Vec<Option<ItemStack>> {
    readable(host, player).map_or_else(|| vec![None; 36], |(inv, _)| inv)
}

/// The menu slot of a player's inventory slot (the menu's own 54 first, then 27 inventory rows, then the hotbar).
fn inventory_slot(slot: i32) -> Option<u32> {
    match usize::try_from(slot).ok()?.checked_sub(OWN_SLOTS)? {
        rel @ 0..27 => Some(rel as u32 + 9),
        rel @ 27..36 => Some(rel as u32 - 27),
        _ => None,
    }
}

/// Puts a stack in a player's inventory (`Inventory.add`: onto the same item first, the hotbar before the
/// rest, then the empty slots); gives back what didn't fit. A mod menu's cursor and player slots can't be
/// written for the player who hosts the world, so the terminal moves items straight to the inventory.
fn give(host: &mut dyn ServerHost, player: PlayerRef, stack: &ItemStack) -> u32 {
    // A slot the game can't hand to a mod reads as empty and would be written over: the game's own
    // Inventory.add places the stack instead (what doesn't fit is thrown to the player).
    if !readable(&*host, player).is_some_and(|(_, whole)| whole) {
        return match player.give_stack(host, stack.clone()) {
            Ok(rest) => rest.map_or(0, |rest| rest.count()),
            Err(_) => stack.count(),
        };
    }
    let mut inv = inventory(&*host, player);
    let max = max_stack(&*host, stack);
    let mut left = stack.count();
    let mut changed = Vec::new();
    for pass in 0..2 {
        for (slot, cell) in inv.iter_mut().enumerate().take(36) {
            if left == 0 {
                break;
            }
            let put = match (&*cell, pass) {
                (Some(h), 0) if h.is_stackable_with(stack) => max.saturating_sub(h.count()).min(left),
                (None, 1) => max.min(left),
                _ => 0,
            };
            if put == 0 {
                continue;
            }
            let mut s = cell.clone().unwrap_or_else(|| stack.clone());
            let _ = s.set_count(if cell.is_some() { s.count() + put } else { put });
            *cell = Some(s);
            left -= put;
            changed.push(slot);
        }
    }
    for slot in changed {
        if let Err(e) = player.set_inventory_slot(host, slot as u32, inv[slot].clone()) {
            eprintln!("[toms_storage] set_inventory_slot {slot}: {e:?}");
        }
    }
    left
}

/// Takes up to `want` of `res` off the network into the player's inventory; what doesn't fit goes back.
fn take_to_player(host: &mut dyn ServerHost, player: PlayerRef, level: LevelRef, invs: &[Inv], res: &ItemResource, want: u32) -> u32 {
    let got = host.transaction(|host, tx| net_extract(host, tx, level, invs, res, want)).unwrap_or(0);
    let Some(st) = res.to_stack(got).filter(|_| got > 0) else { return 0 };
    let left = give(host, player, &st);
    if left > 0 {
        let back = host.transaction(|host, tx| net_insert(host, tx, level, invs, res, left)).unwrap_or(0);
        if back < left {
            if let Some(rest) = res.to_stack(left - back) {
                let _ = player.give_stack(host, rest);
            }
        }
    }
    got - left
}

/// Puts up to `n` of a stack into the network; gives back what was taken.
fn put_in(host: &mut dyn ServerHost, level: LevelRef, invs: &[Inv], stack: &ItemStack, n: u32) -> u32 {
    let res = ItemResource::of(stack);
    host.transaction(|host, tx| net_insert(host, tx, level, invs, &res, n.min(stack.count()))).unwrap_or(0)
}

enum Job {
    Open { uuid: u128, dim: String, term: [i32; 3], crafting: bool },
    Search { uuid: u128, text: String },
}

/// What the hooks, the payload handler and the menu part share.
#[derive(Default)]
struct Shared {
    jobs: Mutex<Vec<Job>>,
    sessions: Mutex<HashMap<u128, Session>>,
}

struct SearchHandler(Arc<Shared>);

impl IPayloadHandler for SearchHandler {
    fn handle(&self, payload: PayloadBytes, context: &mut dyn IPayloadContext) -> Result<(), PayloadHandleError> {
        let Ok(player) = context.player() else { return Ok(()) };
        let text: String = String::from_utf8_lossy(&payload.0).chars().take(48).collect();
        lock(&self.0.jobs).push(Job::Search { uuid: player.uuid(), text });
        Ok(())
    }
}

/// A block placed facing the block it was put against (or the placer, for the terminals' side).
struct Placement;

#[mod_api::sunrise_part]
impl BlockPart for Placement {
    /// AbstractStorageTerminalBlock / InventoryCableConnectorBlock / InventoryProxyBlock.getStateForPlacement.
    fn state_for_placement(&self, level: LevelSide, event: &mut StateForPlacementEvent<'_>) -> Option<Option<BlockState>> {
        let level: &dyn LevelHost = match &level {
            LevelSide::Server(l) => *l,
            LevelSide::Client(l) => *l,
        };
        let block = level.state_block(event.state)?.to_string();
        let ctx = event.context;
        let yaw = match ctx.source {
            PlacementSource::Player { yaw, .. } => yaw,
            _ => 0.0,
        };
        let toward = opposite(ctx.face);
        let water = event.fluid.is_source();
        let text = if block == TERMINAL || block == CRAFTING {
            let horizontal = horizontal_name(yaw);
            let (facing, pos) = match toward {
                Direction::Up => (horizontal, "up"),
                Direction::Down => (horizontal, "down"),
                d => (dir_name(d), "center"),
            };
            format!("{block}[facing={facing},pos={pos},waterlogged={water}]")
        } else if CABLE_CONNECTORS.contains(&block.as_str()) || SIX_WAY.contains(&block.as_str()) {
            let current = level.state_string(event.state)?;
            let rest = current.split_once('[').map(|(_, r)| r.trim_end_matches(']').to_string()).unwrap_or_default();
            let mut props: Vec<String> = rest.split(',').filter(|kv| !kv.is_empty() && !kv.starts_with("facing=")).map(str::to_string).collect();
            props.push(format!("facing={}", dir_name(toward)));
            format!("{block}[{}]", props.join(","))
        } else if block == "toms_storage:ts.level_emitter" {
            format!("{block}[facing={},powered=false]", dir_name(ctx.face))
        } else {
            return None;
        };
        level.block_state_id(&text).map(Some)
    }
}

/// The horizontal direction a placer facing `yaw` degrees looks toward, as `Direction.fromYRot` names it.
fn horizontal_name(yaw: f32) -> &'static str {
    match ((yaw / 90.0 + 0.5).floor() as i32) & 3 {
        0 => "south",
        1 => "west",
        2 => "north",
        _ => "east",
    }
}

/// The crafting terminal and storage terminal block entities (the jar's tiles; their data lives in the menu).
struct Tile;

#[mod_api::sunrise_part]
impl BlockPart for Tile {}

#[mod_api::sunrise_part]
impl BlockEntityPart for Tile {}

#[derive(Default)]
struct Mod {
    shared: Arc<Shared>,
    /// Client: the search typed into the open terminal, and whether it changed.
    typed: Mutex<(String, bool)>,
}

fn is_terminal_title(t: &str) -> bool {
    t.starts_with(STORAGE_TITLE) || t.starts_with(CRAFTING_TITLE)
}

/// The player's open menu, when it is the window `window`.
fn live(host: &dyn ServerHost, uuid: u128, window: i32) -> Option<MenuRef> {
    let menu = host.player(uuid)?.container_menu(host)?;
    (menu.container_id(host) == Some(window)).then_some(menu)
}

/// Opens the six-row chest screen; the window id of the menu that opened.
fn open_screen(host: &mut dyn ServerHost, uuid: u128, title: Text, items: Vec<Option<ItemStack>>, replace: MenuReplace) -> Option<i32> {
    let player = host.player(uuid)?;
    match player.open_chest_menu(host, 6, title, items, replace) {
        Ok(menu) => {
            let Some(window) = menu.container_id(&*host) else {
                let _ = menu.close_container(host);
                return None;
            };
            Some(window)
        }
        Err(e) => {
            eprintln!("[toms_storage] open_chest_menu: {e:?}");
            None
        }
    }
}

fn level_of(host: &dyn Host, dim: &str) -> Option<LevelRef> {
    host.level(dim)
}

impl Shared {
    fn queue(&self, job: Job) {
        lock(&self.jobs).push(job);
    }

    fn source(&self, host: &mut dyn ServerHost, s: &Session) -> Option<(LevelRef, Vec<Inv>)> {
        let level = level_of(&*host, &s.dim)?;
        let id = block_id(&*host, level, s.term);
        if id != TERMINAL && id != CRAFTING {
            return None;
        }
        Some((level, terminal_source(host, level, s.term)))
    }

    /// The menu's 54 slots for the session.
    fn items(&self, host: &mut dyn ServerHost, s: &mut Session) -> Vec<Option<ItemStack>> {
        let reader: &dyn Host = &*host;
        let mut out: Vec<Option<ItemStack>> = vec![None; OWN_SLOTS];
        let l = layout(s.crafting);
        let filler = item(reader, "minecraft:gray_stained_glass_pane").map(|p| named(p, " ", &[]));
        for i in OWN_SLOTS - 9..OWN_SLOTS {
            out[i] = filler.clone();
        }
        if s.crafting {
            for i in 27..54 {
                out[i] = filler.clone();
            }
        }
        s.page = s.page.min(s.pages() - 1);
        let start = s.page * s.per_page();
        for (slot, e) in l.items.clone().zip(s.view.iter().skip(start)) {
            let mut st = e.sample.clone();
            let shown = e.total.min(u64::from(max_stack(reader, &st))) as u32;
            let _ = st.set_count(shown.max(1));
            let lore = vec![format!("Stored: {}", group(e.total))];
            let mut patch = Compound::new();
            patch.insert("minecraft:lore".into(), Tag::List(lore.iter().map(|x| Tag::String(x.as_str().into())).collect()));
            let _ = st.apply_components(&patch);
            out[slot] = Some(st);
        }
        let pages = s.pages();
        out[l.prev] = item(reader, "minecraft:arrow").map(|a| named(a, &format!("Previous page ({}/{pages})", s.page + 1), &[]));
        out[l.next] = item(reader, "minecraft:spectral_arrow").map(|a| named(a, &format!("Next page ({}/{pages})", s.page + 1), &[]));
        out[l.sort] = item(reader, "minecraft:hopper").map(|a| {
            named(a, &format!("Sort by {}", SORTS[s.sort]), &["Click to change".to_string()])
        });
        let search = if s.search.is_empty() { "Type to search (@ for a mod)".to_string() } else { format!("Search: {}", s.search) };
        out[l.search] = item(reader, "minecraft:name_tag").map(|a| named(a, &search, &["Click to clear".to_string()]));
        let total: u64 = s.view.iter().map(|e| e.total).sum();
        out[l.info] = item(reader, "minecraft:paper").map(|a| {
            named(a, &format!("{} kinds, {} items", s.view.len(), group(total)), &[])
        });
        if let Some(c) = l.clear {
            out[c] = item(reader, "minecraft:barrier").map(|a| named(a, "Clear the grid", &["Back to the network".to_string()]));
        }
        if s.crafting {
            for (k, slot) in GRID.iter().enumerate() {
                out[*slot] = s.grid[k].clone();
            }
            out[40] = match &s.pick {
                Some(p) => Some(named(p.clone(), "Picked for the grid", &["Click an empty grid slot to put one".to_string(), "Right-click a stored item to pick another".to_string()])),
                None => item(reader, "minecraft:crafting_table").map(|a| named(a, "Right-click a stored item", &["to pick it for the grid".to_string()])),
            };
            out[RESULT] = None;
        }
        out
    }

    fn refresh(&self, host: &mut dyn ServerHost, uuid: u128, retitle: bool) {
        // The session is out of the map while the menu is touched, so a menu closing under it can't deadlock.
        let Some(mut s) = lock(&self.sessions).remove(&uuid) else { return };
        let Some((level, invs)) = self.source(host, &s) else {
            // The terminal or its network is gone: closing the screen returns the crafting grid.
            lock(&self.sessions).insert(uuid, s);
            if let Some(menu) = host.player(uuid).and_then(|p| p.container_menu(&*host)) {
                let _ = menu.close_container(host);
            }
            return;
        };
        let raw = contents(host, level, &invs);
        s.view = filter_sort(raw, &s.search, s.sort);
        let mut items = self.items(host, &mut s);
        if s.crafting {
            items[RESULT] = self.craft_result(host, level, &s).map(|m| m.output);
        }
        if retitle {
            if let Some(w) = open_screen(host, uuid, s.title(), items, MenuReplace::InPlace) {
                s.window = w;
            }
        } else if live(&*host, uuid, s.window).is_none_or(|m| m.set_items(host, items).is_err()) {
            eprintln!("[toms_storage] the terminal menu is gone");
        }
        lock(&self.sessions).insert(uuid, s);
    }

    fn craft_result(&self, host: &dyn Host, level: LevelRef, s: &Session) -> Option<mod_api::RecipeMatch> {
        if s.grid.iter().all(Option::is_none) {
            return None;
        }
        let input = RecipeInput::Crafting { width: 3, height: 3, slots: s.grid.to_vec() };
        let kind = Identifier::parse("minecraft:crafting").ok()?;
        level.recipe_for(host, &kind, &input).ok().flatten()
    }

    fn open(&self, host: &mut dyn ServerHost, uuid: u128, dim: String, term: [i32; 3], crafting: bool) {
        let old = lock(&self.sessions).remove(&uuid);
        if let Some(old) = old {
            for st in self.return_grid(host, &old) {
                if let Some(p) = host.player(uuid) {
                    let _ = p.give_stack(host, st);
                }
            }
        }
        let mut s = Session { window: -1, dim, term, crafting, page: 0, sort: 0, search: String::new(), view: Vec::new(), grid: Default::default(), pick: None };
        let Some((level, invs)) = self.source(host, &s) else { return };
        s.view = filter_sort(contents(host, level, &invs), "", 0);
        let items = self.items(host, &mut s);
        if let Some(w) = open_screen(host, uuid, s.title(), items, MenuReplace::CloseFirst) {
            s.window = w;
            lock(&self.sessions).insert(uuid, s);
        }
    }

    /// The crafting grid goes back to the network when the terminal closes (else to the player).
    fn return_grid(&self, host: &mut dyn ServerHost, s: &Session) -> Vec<ItemStack> {
        let mut rest = Vec::new();
        if s.grid.iter().all(Option::is_none) {
            return rest;
        }
        let invs = self.source(host, s);
        let _ = host.transaction(|host, tx| {
            for st in s.grid.iter().flatten() {
                let res = ItemResource::of(st);
                let mut left = st.count();
                if let Some((level, invs)) = &invs {
                    left -= net_insert(host, tx, *level, invs, &res, left)?;
                }
                if let Some(r) = res.to_stack(left).filter(|_| left > 0) {
                    rest.push(r);
                }
            }
            Ok::<(), TransactionError<Overanswered>>(())
        });
        rest
    }

    fn closed(&self, host: &mut dyn ServerHost, uuid: u128, window: i32) {
        let s = {
            let mut sessions = lock(&self.sessions);
            match sessions.get(&uuid) {
                Some(s) if s.window == window => sessions.remove(&uuid),
                _ => None,
            }
        };
        if let Some(s) = s {
            // What is still in the grid goes back to the network, and what doesn't fit to the player.
            for st in self.return_grid(host, &s) {
                if let Some(p) = host.player(uuid) {
                    if let Err(e) = p.give_stack(host, st) {
                        eprintln!("[toms_storage] grid stack lost: {e:?}");
                    }
                }
            }
        }
    }

    /// StorageTerminalMenu's clicks, without a cursor: a stored item goes to the inventory (a stack, half
    /// with the right button), an inventory stack goes in (one with the right button), shift does the same.
    fn click(&self, host: &mut dyn ServerHost, ev: &MenuClickedEvent) {
        let uuid = ev.uuid;
        let (crafting, page, per_page, dim, term) = {
            let sessions = lock(&self.sessions);
            let Some(s) = sessions.get(&uuid) else { return };
            if s.window != ev.window {
                return;
            }
            (s.crafting, s.page, s.per_page(), s.dim.clone(), s.term)
        };
        let Some(player) = host.player(uuid) else { return };
        let Some(level) = level_of(&*host, &dim) else { return };
        let invs = terminal_source(host, level, term);
        let l = layout(crafting);
        let (button, mode) = (ev.button, ev.mode);
        if !matches!(mode, 0 | 1) || ev.slot < 0 {
            return self.refresh(host, uuid, false);
        }
        let mut retitle = false;
        if let Some(inv_slot) = inventory_slot(ev.slot) {
            if let Some(st) = inventory(&*host, player)[inv_slot as usize].clone() {
                let n = if button == 1 && mode == 0 { 1 } else { st.count() };
                let went = put_in(host, level, &invs, &st, n);
                if went > 0 {
                    let rest = st.shrink(went);
                    if let Err(e) = player.set_inventory_slot(host, inv_slot, rest) {
                        eprintln!("[toms_storage] set_inventory_slot {inv_slot}: {e:?}");
                    }
                }
            }
        } else {
            let slot = ev.slot as usize;
            if l.items.contains(&slot) {
                let entry = {
                    let sessions = lock(&self.sessions);
                    sessions.get(&uuid).and_then(|s| s.view.get(page * per_page + (slot - l.items.start)).cloned())
                };
                if let Some(e) = entry {
                    let res = ItemResource::of(&e.sample);
                    if crafting && button == 1 && mode == 0 {
                        // Crafting terminal: a right click picks the item for the grid.
                        if let Some(s) = lock(&self.sessions).get_mut(&uuid) {
                            s.pick = Some(e.sample.clone());
                        }
                        let _ = res;
                    } else {
                        let stack = e.total.min(u64::from(max_stack(&*host, &e.sample))) as u32;
                        let want = if button == 1 && mode == 0 { stack.div_ceil(2) } else { stack };
                        take_to_player(host, player, level, &invs, &res, want);
                    }
                }
            } else if slot == l.prev || slot == l.next {
                if let Some(s) = lock(&self.sessions).get_mut(&uuid) {
                    let pages = s.pages();
                    s.page = if slot == l.prev { (s.page + pages - 1) % pages } else { (s.page + 1) % pages };
                }
            } else if slot == l.sort {
                if let Some(s) = lock(&self.sessions).get_mut(&uuid) {
                    s.sort = (s.sort + if button == 1 { SORTS.len() - 1 } else { 1 }) % SORTS.len();
                }
            } else if slot == l.search {
                if let Some(s) = lock(&self.sessions).get_mut(&uuid) {
                    if !s.search.is_empty() {
                        s.search.clear();
                        s.page = 0;
                        retitle = true;
                    }
                }
            } else if crafting {
                self.crafting_click(host, ev, player, level, &invs, slot);
            }
        }
        self.refresh(host, uuid, retitle);
    }

    /// One of the picked item from the network onto grid slot `k`.
    fn to_grid(&self, host: &mut dyn ServerHost, s: &mut Session, k: usize, level: LevelRef, invs: &[Inv]) {
        let Some(sample) = s.pick.clone() else { return };
        let max = max_stack(&*host, &sample);
        if s.grid[k].as_ref().is_some_and(|g| !g.is_stackable_with(&sample) || g.count() >= max) {
            return;
        }
        let res = ItemResource::of(&sample);
        let got = host.transaction(|host, tx| net_extract(host, tx, level, invs, &res, 1)).unwrap_or(0);
        if got == 1 {
            s.grid[k] = match s.grid[k].take() {
                Some(mut g) => {
                    let _ = g.grow(1);
                    Some(g)
                }
                None => res.to_stack(1),
            };
        }
    }

    fn crafting_click(&self, host: &mut dyn ServerHost, ev: &MenuClickedEvent, player: PlayerRef, level: LevelRef, invs: &[Inv], slot: usize) {
        let uuid = ev.uuid;
        if let Some(k) = GRID.iter().position(|g| *g == slot) {
            // A grid stack back to the network (one with the right button); what doesn't fit to the player.
            let mut sessions = lock(&self.sessions);
            let Some(s) = sessions.get_mut(&uuid) else { return };
            let picked = s.pick.as_ref().is_some_and(|p| s.grid[k].as_ref().is_none_or(|g| g.is_stackable_with(p)));
            if ev.mode == 0 && picked && (s.grid[k].is_none() || ev.button == 1) {
                // The picked item: one into an empty slot, or one more with the right button.
                return self.to_grid(host, s, k, level, invs);
            }
            let Some(st) = s.grid[k].take() else { return };
            let n = if ev.button == 1 && ev.mode == 0 { 1 } else { st.count() };
            let mut went = put_in(host, level, invs, &st, n);
            if went < n {
                let mut part = st.clone();
                let _ = part.set_count(n - went);
                went += (n - went) - give(host, player, &part);
            }
            s.grid[k] = st.shrink(went);
        } else if slot == RESULT {
            // CraftingTerminalBlockEntity.craft: once, or as often as it goes with shift.
            // Shift crafts again only while the grid still makes the same thing.
            let times = if ev.mode == 1 { 64 } else { 1 };
            let mut first: Option<ItemStack> = None;
            for _ in 0..times {
                let Some(out) = self.craft_once(host, uuid, player, level, invs, first.as_ref()) else { break };
                first.get_or_insert(out);
            }
        } else if slot == 40 {
            if let Some(s) = lock(&self.sessions).get_mut(&uuid) {
                s.pick = None;
            }
        } else if Some(slot) == layout(true).clear {
            let mut sessions = lock(&self.sessions);
            let Some(s) = sessions.get_mut(&uuid) else { return };
            for g in s.grid.iter_mut() {
                if let Some(st) = g.take() {
                    let n = st.count();
                    let went = put_in(host, level, invs, &st, n);
                    *g = st.shrink(went);
                }
            }
        }
    }

    /// The result to the inventory, one of each ingredient used, remainders kept, and the used slots refilled
    /// from the network.
    fn craft_once(&self, host: &mut dyn ServerHost, uuid: u128, player: PlayerRef, level: LevelRef, invs: &[Inv], same_as: Option<&ItemStack>) -> Option<ItemStack> {
        let mut sessions = lock(&self.sessions);
        let s = sessions.get_mut(&uuid)?;
        let m = self.craft_result(&*host, level, s)?;
        let out = m.output.clone();
        if same_as.is_some_and(|f| !f.is_stackable_with(&out)) {
            return None;
        }
        // Only when it all fits: `give` on a copy of the inventory would be cleaner, this checks the room.
        let inv = inventory(&*host, player);
        let max = max_stack(&*host, &out);
        let room: u32 = inv.iter().map(|h| match h {
            None => max,
            Some(h) if h.is_stackable_with(&out) => max.saturating_sub(h.count()),
            _ => 0,
        }).sum();
        if room < out.count() {
            return None;
        }
        give(host, player, &out);
        let before = s.grid.clone();
        for (k, before_k) in before.iter().enumerate().take(9) {
            let Some(st) = s.grid[k].take() else { continue };
            s.grid[k] = st.shrink(1);
            let rem = m.remainders.get(k).cloned().flatten().filter(|r| r.count() > 0);
            if s.grid[k].is_none() {
                let res = ItemResource::of(before_k.as_ref().expect("the grid slot held a stack"));
                let got = host.transaction(|host, tx| net_extract(host, tx, level, invs, &res, 1)).unwrap_or(0);
                if got == 1 {
                    s.grid[k] = res.to_stack(1);
                    if let Some(r) = rem {
                        let n = r.count();
                        let went = put_in(host, level, invs, &r, n);
                        if went < n {
                            let _ = give(host, player, &r.clone().shrink(went).unwrap_or(r));
                        }
                    }
                } else {
                    s.grid[k] = rem;
                }
            } else if let Some(r) = rem
                && give(host, player, &r) > 0
            {
                let n = r.count();
                put_in(host, level, invs, &r, n);
            }
        }
        Some(out)
    }

    /// WirelessTerminalItem.use: the terminal the player looks at within reach.
    fn looked_terminal(&self, host: &dyn Host, uuid: u128, range: f64) -> Option<(String, [i32; 3], bool)> {
        let player = host.player(uuid)?;
        let e = player.entity();
        let eye = e.eye_position(host)?;
        let v = e.view_vector(host)?;
        let level = player.level(host)?;
        let dim = level.dimension().id(host)?.to_string();
        let mut t = 0.0;
        while t <= range {
            let p = [(eye[0] + v[0] * t).floor() as i32, (eye[1] + v[1] * t).floor() as i32, (eye[2] + v[2] * t).floor() as i32];
            let id = block_id(host, level, p);
            if id == TERMINAL || id == CRAFTING {
                return Some((dim, p, id == CRAFTING));
            }
            if !id.is_empty() && id != "minecraft:air" && !id.contains("glass") && id != "minecraft:water" {
                return None;
            }
            t += 0.1;
        }
        None
    }
}

fn bound(stack: &ItemStack) -> Option<(String, [i32; 3])> {
    let comps = stack.components()?;
    let Some(Tag::Compound(data)) = comps.as_nbt().get(&"minecraft:custom_data".into()) else { return None };
    let int = |k: &str| match data.get(&k.into()) {
        Some(Tag::Int(v)) => Some(*v),
        _ => None,
    };
    let dim = match data.get(&"BindDim".into()) {
        Some(Tag::String(s)) => s.to_string_lossy(),
        _ => return None,
    };
    Some((dim, [int("BindX")?, int("BindY")?, int("BindZ")?]))
}

/// The terminals' menus are vanilla six-row chest menus the mod fills; their clicks are the mod's.
struct TerminalMenu(Arc<Shared>);

#[mod_api::sunrise_part]
impl MenuPart for TerminalMenu {
    /// A click in an open terminal is the terminal's alone: the vanilla click never moves a stack.
    fn menu_clicked(&self, host: &mut dyn ServerHost, event: &mut MenuClickedEvent<'_>) -> Flow {
        if lock(&self.0.sessions).get(&event.uuid).is_some_and(|s| s.window == event.window) {
            self.0.click(host, event);
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    fn menu_removed(&self, host: &mut dyn ServerHost, event: &mut MenuRemovedEvent<'_>) {
        self.0.closed(host, event.uuid, event.window);
    }
}

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        for b in [TERMINAL, CRAFTING].iter().chain(CABLE_CONNECTORS.iter()).chain(SIX_WAY.iter()).chain(["toms_storage:ts.level_emitter"].iter()) {
            let key = r.block_key(b);
            r.block_part(key, Placement);
        }
        for (name, block) in [("storage_terminal", TERMINAL), ("crafting_terminal", CRAFTING)] {
            let kind = r.block_entity_type(name).blocks(&[block]).build();
            if let Err(e) = r.block_entity_part(&kind, Tile) {
                eprintln!("[toms_storage] block entity part refused: {e:?}");
            }
        }
        // The connector answers for its network, a cable connector for its connector's network. The provider only
        // sees the block, so the network is worked out when the handler is used.
        let conn = r.block_key(CONNECTOR);
        r.block_capability(&ITEMS, &conn, |_, at: BlockSide, _, _, offer| {
            offer.offer(|| Box::new(Merged { level: at.level, start: at.pos, asker: at.side }) as Box<dyn ResourceHandler<ItemResource>>);
        });
        for cc in CABLE_CONNECTORS {
            let key = r.block_key(cc);
            r.block_capability(&ITEMS, &key, |_, at: BlockSide, _, _, offer| {
                offer.offer(|| Box::new(Merged { level: at.level, start: at.pos, asker: None }) as Box<dyn ResourceHandler<ItemResource>>);
            });
        }
        let proxy = r.block_key(PROXY);
        r.block_capability(&ITEMS, &proxy, |level, at: BlockSide, state, _, offer| {
            let Some(f) = state.property(&level.block_states(), "facing").ok().flatten().and_then(dir_named) else { return };
            if at.side == Some(f) {
                return;
            }
            let inv = Inv { pos: add(at.pos, vec_of(f)), side: Some(opposite(f)) };
            offer.offer(|| Box::new(Forward { level: at.level, inv }) as Box<dyn ResourceHandler<ItemResource>>);
        });
        r.menu_part("minecraft:generic_9x6", TerminalMenu(self.shared.clone()));
        r.payload_handlers()
            .registrar("1")
            .optional()
            .play_to_server(PayloadId(SEARCH_CHANNEL.into()), Arc::new(SearchHandler(self.shared.clone())));
    }

    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        if !matches!(event.phase, ServerTickPhase::ServerTickEnd { .. }) {
            return;
        }
        let jobs = std::mem::take(&mut *lock(&self.shared.jobs));
        for job in jobs {
            match job {
                Job::Open { uuid, dim, term, crafting } => self.shared.open(host, uuid, dim, term, crafting),
                Job::Search { uuid, text } => {
                    let changed = match lock(&self.shared.sessions).get_mut(&uuid) {
                        Some(s) if s.search != text => {
                            s.search = text;
                            s.page = 0;
                            true
                        }
                        _ => false,
                    };
                    if changed {
                        self.shared.refresh(host, uuid, true);
                    }
                }
            }
        }
    }

    /// AbstractStorageTerminalBlock.use, and binding the advanced wireless terminal.
    fn right_click_block(&self, host: HostSide<'_>, event: &mut RightClickBlockEvent<'_>) -> Flow {
        let click = event.click;
        let HostSide::Server(host) = host else {
            return Flow::Continue;
        };
        let dim = click.dimension.key().id().to_string();
        let Some(level) = host.level(&dim) else { return Flow::Continue };
        let id = block_id(&*host, level, click.pos);
        if id != TERMINAL && id != CRAFTING {
            return Flow::Continue;
        }
        let held = click.stack.map(|s| s.item().id(&*host).unwrap_or_default().to_owned()).unwrap_or_default();
        let sneaking = host.player(click.player).and_then(|p| p.entity().is_shift_key_down(&*host)) == Some(true);
        if held == ADV_WIRELESS && sneaking {
            // AdvWirelessTerminalItem.useOn: bind it to this terminal.
            if let Some(mut st) = click.stack.cloned() {
                let mut data = Compound::new();
                data.insert("BindX".into(), Tag::Int(click.pos[0]));
                data.insert("BindY".into(), Tag::Int(click.pos[1]));
                data.insert("BindZ".into(), Tag::Int(click.pos[2]));
                data.insert("BindDim".into(), Tag::String(dim.as_str().into()));
                let mut patch = Compound::new();
                patch.insert("minecraft:custom_data".into(), Tag::Compound(data));
                let _ = st.apply_components(&patch);
                if let Some(p) = host.player(click.player) {
                    let slot = match click.hand {
                        Hand::Main => p.selected_slot(&*host).map(|s| s as u32),
                        _ => Some(40),
                    };
                    if let Some(slot) = slot {
                        let _ = p.set_inventory_slot(host, slot, Some(st));
                    }
                }
                if let Some(name) = host.player(click.player).and_then(|p| p.name(&*host).map(str::to_owned)) {
                    host.commands().run_command(&format!("title {name} actionbar {{\"translate\":\"chat.toms_storage.terminal_bound\"}}"));
                }
            }
        } else if sneaking && click.stack.is_some() {
            return Flow::Continue;
        } else if matches!(click.hand, Hand::Main) {
            self.shared.queue(Job::Open { uuid: click.player, dim, term: click.pos, crafting: id == CRAFTING });
        }
        *event.cancellation_result = InteractionResult::Success;
        event.cancelled = true;
        Flow::Cancel
    }

    fn right_click_item(&self, host: HostSide<'_>, event: &mut RightClickItemEvent<'_>) -> Flow {
        let HostSide::Server(host) = host else { return Flow::Continue };
        let click = event.click;
        let held = click.stack.item().id(&*host).unwrap_or_default().to_owned();
        let target = if held == WIRELESS {
            self.shared.looked_terminal(&*host, click.player, WIRELESS_RANGE)
        } else if held == ADV_WIRELESS {
            bound(click.stack).and_then(|(dim, pos)| {
                let player = host.player(click.player)?;
                let here = player.level(&*host)?.dimension().id(&*host)?.to_string();
                let at = player.entity().position(&*host)?;
                let d = ((at[0] - f64::from(pos[0])).powi(2) + (at[1] - f64::from(pos[1])).powi(2) + (at[2] - f64::from(pos[2])).powi(2)).sqrt();
                let level = host.level(&dim)?;
                let id = block_id(&*host, level, pos);
                (here == dim && d <= ADV_WIRELESS_RANGE && (id == TERMINAL || id == CRAFTING)).then_some((dim, pos, id == CRAFTING))
            })
        } else {
            return Flow::Continue;
        };
        match target {
            Some((dim, term, crafting)) => self.shared.queue(Job::Open { uuid: click.player, dim, term, crafting }),
            None => {
                if let Some(name) = host.player(click.player).and_then(|p| p.name(&*host).map(str::to_owned)) {
                    host.commands().run_command(&format!("title {name} actionbar {{\"translate\":\"chat.toms_storage.terminal_out_of_range\"}}"));
                }
            }
        }
        *event.cancellation_result = InteractionResult::Success;
        event.cancelled = true;
        Flow::Cancel
    }

    /// The terminal's search field, typed straight into the screen: the keys don't reach the chest screen.
    fn screen_char_typed_pre(&self, host: &dyn ClientHost, event: &mut ScreenCharTypedPreEvent<'_>) -> Flow {
        if !host.screen_title().is_some_and(|t| is_terminal_title(&t)) || event.c.is_control() {
            return Flow::Continue;
        }
        let mut typed = lock(&self.typed);
        if typed.0.chars().count() < 48 {
            typed.0.push(event.c);
            typed.1 = true;
        }
        Flow::Cancel
    }

    fn screen_key_pressed_pre(&self, host: &dyn ClientHost, event: &mut ScreenKeyPressedPreEvent<'_>) -> Flow {
        let title = host.screen_title();
        if !title.as_deref().is_some_and(is_terminal_title) {
            return Flow::Continue;
        }
        let key = event.key.strip_prefix("key.keyboard.").unwrap_or(event.key);
        if key == "backspace" {
            let mut typed = lock(&self.typed);
            if typed.0.pop().is_some() {
                typed.1 = true;
            }
        } else if key.chars().count() != 1 && key != "space" {
            return Flow::Continue;
        }
        Flow::Cancel
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPostEvent) {
        let open = host.screen_title().is_some_and(|t| is_terminal_title(&t));
        let mut typed = lock(&self.typed);
        if !open {
            if !typed.0.is_empty() {
                *typed = (String::new(), false);
            }
            return;
        }
        if typed.1 {
            typed.1 = false;
            let bytes = typed.0.clone().into_bytes();
            drop(typed);
            let _ = host.send_payload_to_server(&PayloadId(SEARCH_CHANNEL.into()), PayloadBytes(bytes));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouping() {
        assert_eq!(group(1234567), "1,234,567");
        assert_eq!(group(12), "12");
        assert_eq!(group(0), "0");
        assert_eq!(group(1000), "1,000");
    }

    #[test]
    fn directions_round_trip() {
        for d in DIRS {
            assert_eq!(dir_named(dir_name(d)), Some(d));
            assert_eq!(opposite(opposite(d)), d);
            assert_eq!(add(vec_of(d), vec_of(opposite(d))), [0, 0, 0]);
        }
        assert_eq!(dir_named("sideways"), None);
    }

    #[test]
    fn distance_is_squared() {
        assert_eq!(dist2([0, 0, 0], [3, 4, 12]), 169);
        assert!(dist2([0, 0, 0], [15, 0, 0]) < INV_RANGE * INV_RANGE);
        assert!(dist2([0, 0, 0], [16, 0, 0]) >= INV_RANGE * INV_RANGE);
    }

    #[test]
    fn placer_yaw_names_the_facing() {
        assert_eq!(horizontal_name(0.0), "south");
        assert_eq!(horizontal_name(90.0), "west");
        assert_eq!(horizontal_name(180.0), "north");
        assert_eq!(horizontal_name(-90.0), "east");
        assert_eq!(horizontal_name(360.0), "south");
    }

    #[test]
    fn inventory_slots_follow_the_menu() {
        assert_eq!(inventory_slot(53), None);
        assert_eq!(inventory_slot(54), Some(9));
        assert_eq!(inventory_slot(80), Some(35));
        assert_eq!(inventory_slot(81), Some(0));
        assert_eq!(inventory_slot(89), Some(8));
        assert_eq!(inventory_slot(90), None);
        assert_eq!(inventory_slot(-1), None);
    }

    #[test]
    fn layouts_keep_controls_off_the_item_slots() {
        for crafting in [false, true] {
            let l = layout(crafting);
            let controls = [l.prev, l.next, l.sort, l.search, l.info];
            for c in controls.into_iter().chain(l.clear) {
                assert!(c < OWN_SLOTS && !l.items.contains(&c), "{c}");
            }
        }
        for g in GRID {
            assert!(!layout(true).items.contains(&g));
            assert_ne!(g, RESULT);
        }
    }

    #[test]
    fn search_by_name_and_mod() {
        assert!(search_matches("minecraft:oak_planks", "oak"));
        assert!(search_matches("minecraft:oak_planks", "oak planks"));
        assert!(!search_matches("minecraft:oak_planks", "birch"));
        assert!(search_matches("create:cogwheel", "@create"));
        assert!(!search_matches("create:cogwheel", "@minecraft"));
        assert!(search_matches("bare_id", "bare"));
    }

    #[test]
    fn cable_blocks_are_cable_like() {
        assert!(is_cable_like("toms_storage:ts.inventory_cable"));
        assert!(is_cable_like("toms_storage:ts.inventory_cable_connector_framed"));
        assert!(!is_cable_like(CONNECTOR));
        assert!(!is_cable_like("minecraft:chest"));
    }
}
