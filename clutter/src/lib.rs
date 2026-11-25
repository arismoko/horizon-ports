//! Clutter 0.6.2 (EmilSG, MIT) for Sunrise. `data/` and `assets/` are the original's content; `classes.rs` lists
//! every block's Java class. All of the rules are the typed mod API: no `[[native.mixin]]` rows.
//!
//! One `BlockPart` (`Rules`) on every block, going by its Java class:
//! - `state_for_placement`: the facing each class gives (SeatBlock and its kin face the placer, the cupboards,
//!   curtains, benches and most small decor face the way the player looks, wall candles away from the wall they
//!   hang on), table legs and bench legs from their neighbours, the tall curtain's upper half where it's clicked
//!   (the lower half goes below), the tall lamp's lower half, the large chandelier's centre only where its eight
//!   neighbours are free, the trellis under a block, and one more on a pile (seashells, small lily pads, bottles,
//!   polypores) when its own item is used on it;
//! - `update_shape`: table and bench legs, the curtains' `direction_shape`, the half of a tall curtain or lamp
//!   whose other half went, the large chandelier's pieces once one goes;
//! - `use_item_on`: flint and steel or a fire charge lights candelabras, chandeliers, wall candles, large candles
//!   and bonfires (a shovel puts a bonfire out); an axe strips chairs, tables, benches and redwood logs and scrapes
//!   copper (a stage back, or the wax off); honeycomb waxes copper; shears clip glowlilies; eight coins add a layer
//!   to a coin stack;
//! - `use_without_item`: seats (chairs, armchairs, short benches, benches, floor seating, the toilet) put the
//!   player on an invisible marker armour stand tagged `clutter_seat`; an empty hand puts a lit candle out; lamps,
//!   tall lamps and the tube TV switch; curtains open and close with every connected curtain of the same colour and
//!   facing (TallCurtainBlock's flood fill); chimneys and the toilet lid open and close; ripe thornblooms and
//!   glowlilies give their fruit;
//! - `neighbor_changed` + `tick_block`: lamps and tall lamps follow redstone (LampBlock.neighborChanged);
//! - `random_tick` and bone meal: Clutter's crops and plants grow (kiwi and thornbloom crops into their sapling or
//!   bush, glowlily crops into glowlilies, glowlilies, ripe kiwi leaves, giant lily pad seedlings into a 2x2 giant
//!   lily pad), budding onyx grows onyx buds, and unwaxed copper weathers a stage.
//!
//! Block entity types with the jar's ids hold the storage blocks' slots (`cupboard` 27, `wall_cupboard` 9,
//! `mailbox` 9, `cardboard_box` 9, `present` 9, `wall_bookshelf` 5); their barrel template opens the menu. A seat
//! goes once nobody sits on it or its block is gone (`server_tick`).
//!
//! Lamps follow redstone straight from `neighbor_changed`, which can write on Dev 1.
use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::ControlFlow;
use std::sync::{Mutex, OnceLock};

use mod_api::{
    BlockEntityPart, BlockPart, BlockState, BlockUpdateFlags, Compound, Direction, EntityRef,
    EquipmentSlot, Flow, Hand, Hooks, HostBlockStates, HostSide, Identifier, InteractionResult,
    IsBonemealSuccessEvent, IsValidBonemealTargetEvent, ItemPart, ItemStack, LevelHost,
    LevelRandomHost, LevelRef, LevelSide, LevelWriteHost, LivingRef, ModContainer,
    NeighborChangedEvent, OnBlockPlacedByEvent, PerformBonemealEvent, PlacementSource, PlayerRef,
    RandomTickEvent, Registrar, RegistrationContext, ServerHost, ServerStartedEvent,
    ServerStoppedEvent, ServerTickEvent, ServerTickPhase, SoundSource, StateForPlacementEvent, Tag,
    UpdateShapeEvent, UseItemOnEvent, UseWithoutItemEvent,
};

mod classes;

const NS: &str = "clutter:";
const SEAT_TAG: &str = "clutter_seat";

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn class_map() -> &'static HashMap<&'static str, &'static str> {
    static M: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    M.get_or_init(|| classes::CLASSES.iter().copied().collect())
}

/// The Java class of a block by its path, or "" for none of ours.
fn class_of(name: &str) -> &'static str {
    class_map().get(name).copied().unwrap_or("")
}

// ---------------------------------------------------------------------------------------------
// States as text and directions
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
struct St {
    id: String,
    props: Vec<(String, String)>,
}

impl St {
    fn parse(s: &str) -> St {
        let (id, rest) = s.split_once('[').unwrap_or((s, ""));
        let props = rest
            .trim_end_matches(']')
            .split(',')
            .filter_map(|kv| kv.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        St {
            id: id.to_string(),
            props,
        }
    }
    fn get(&self, k: &str) -> Option<&str> {
        self.props
            .iter()
            .find(|(a, _)| a == k)
            .map(|(_, v)| v.as_str())
    }
    fn is(&self, k: &str, v: &str) -> bool {
        self.get(k) == Some(v)
    }
    fn set(&mut self, k: &str, v: &str) {
        if let Some(p) = self.props.iter_mut().find(|(a, _)| a == k) {
            p.1 = v.to_string();
        }
    }
    fn with(&self, k: &str, v: &str) -> St {
        let mut s = self.clone();
        s.set(k, v);
        s
    }
    fn renamed(&self, id: &str) -> St {
        St {
            id: id.to_string(),
            props: self.props.clone(),
        }
    }
    fn text(&self) -> String {
        if self.props.is_empty() {
            return self.id.clone();
        }
        let p: Vec<String> = self.props.iter().map(|(k, v)| format!("{k}={v}")).collect();
        format!("{}[{}]", self.id, p.join(","))
    }
    fn name(&self) -> &str {
        self.id.strip_prefix(NS).unwrap_or("")
    }
    fn class(&self) -> &'static str {
        class_of(self.name())
    }
    fn facing(&self) -> &str {
        self.get("facing").unwrap_or("north")
    }
    fn int(&self, k: &str) -> i32 {
        self.get(k).and_then(|v| v.parse().ok()).unwrap_or(0)
    }
}

const H4: [&str; 4] = ["north", "east", "south", "west"];

fn vec_of(d: &str) -> [i32; 3] {
    match d {
        "north" => [0, 0, -1],
        "south" => [0, 0, 1],
        "west" => [-1, 0, 0],
        "east" => [1, 0, 0],
        "up" => [0, 1, 0],
        _ => [0, -1, 0],
    }
}
fn opposite(d: &str) -> &'static str {
    match d {
        "north" => "south",
        "south" => "north",
        "west" => "east",
        "east" => "west",
        "up" => "down",
        _ => "up",
    }
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
fn at(p: [i32; 3], d: &str) -> [i32; 3] {
    add(p, vec_of(d))
}
fn bp(p: mod_api::BlockPos) -> [i32; 3] {
    [p.x, p.y, p.z]
}
/// `Direction.from2DDataValue(floor(yaw / 90 + 0.5) & 3)`: the way the player looks.
fn horizontal(yaw: f32) -> &'static str {
    ["south", "west", "north", "east"][((yaw / 90.0 + 0.5).floor() as i32 & 3) as usize]
}

type Look<'a> = &'a dyn Fn([i32; 3]) -> Option<St>;

fn level_look(level: &dyn LevelHost) -> impl Fn([i32; 3]) -> Option<St> + '_ {
    move |p| {
        level
            .block_state(p)
            .and_then(|s| level.state_string(s))
            .map(|t| St::parse(&t))
    }
}

fn host_look<'a>(
    host: &'a dyn ServerHost,
    level: LevelRef,
) -> impl Fn([i32; 3]) -> Option<St> + 'a {
    move |p| {
        level
            .block_state(host, p)
            .and_then(|s| host.block_state_string(s))
            .map(|t| St::parse(&t))
    }
}

fn replaceable(s: Option<St>) -> bool {
    s.is_some_and(|s| {
        matches!(
            s.id.as_str(),
            "minecraft:air"
                | "minecraft:cave_air"
                | "minecraft:water"
                | "minecraft:short_grass"
                | "minecraft:fern"
                | "minecraft:snow"
                | "minecraft:dead_bush"
                | "minecraft:vine"
                | "minecraft:tall_grass"
        )
    })
}

// ---------------------------------------------------------------------------------------------
// Classes
// ---------------------------------------------------------------------------------------------

/// SeatBlock's seat height (getYOffset).
fn seat_offset(cls: &str) -> Option<f64> {
    Some(match cls {
        "WoodenChairBlock" | "WoodenBenchBlock" | "ToiletBlock" => 0.3,
        "ArmchairBlock" => 0.25,
        "ShortBenchBlock" => 0.15,
        "FloorSeatBlock" => 0.0,
        _ => return None,
    })
}

/// Lit by flint and steel or a fire charge, put out by an empty hand (WaterloggableLitBlock, WallCandleBlock).
fn candle_lit(cls: &str) -> bool {
    matches!(
        cls,
        "CandelabraBlock"
            | "OxidizableCandelabraBlock"
            | "ChandelierBlock"
            | "OxidizableChandelierBlock"
            | "LargeCandleBlock"
            | "WallCandleBlock"
            | "OxidizableWallCandleBlock"
    )
}

/// Classes whose placement faces the placer (`getHorizontalDirection().getOpposite()`).
fn faces_placer(cls: &str) -> bool {
    matches!(
        cls,
        "WoodenChairBlock"
            | "ArmchairBlock"
            | "ShortBenchBlock"
            | "ToiletBlock"
            | "FloorSeatBlock"
            | "SinkBlock"
            | "FoodBoxBlock"
            | "FoodBoxWithParticles"
            | "FrogStatueBlock"
            | "NautilusShellBlock"
            | "PearlClamBlock"
            | "SeashellBlock"
            | "SmallLilyPadBlock"
            | "SmallTargetBlock"
            | "LabBottleBlock"
            | "TallBottleBlock"
            | "TubeTvBlock"
            | "FishTankBlock"
            | "BrickKilnBlock"
    ) || cls.ends_with("PlushieBlock")
}

/// Classes whose placement faces the way the player looks (`getHorizontalDirection()`).
fn faces_look(cls: &str) -> bool {
    matches!(
        cls,
        "AnchorBlock"
            | "BeerMugBlock"
            | "MugBlock"
            | "BowlBlock"
            | "BreadBlock"
            | "CandelabraBlock"
            | "OxidizableCandelabraBlock"
            | "CardboardBoxBlock"
            | "CupboardBlock"
            | "KitchenCurtainBlock"
            | "MailBoxBlock"
            | "PresentBlock"
            | "TallCurtainBlock"
            | "TowelBlock"
            | "TrellisBlock"
            | "WallBookshelfBlock"
            | "WallCupboardBlock"
            | "WindowSillBlock"
            | "WoodenBenchBlock"
            | "BookPileBlock"
            | "ShelfBlock"
            | "ShowerBlock"
            | "DrinkingGlassBlock"
    )
}

/// Piles one more of its own item makes taller: (property, most).
fn pile(cls: &str) -> Option<(&'static str, i32)> {
    Some(match cls {
        "SeashellBlock" => ("amount", 4),
        "SmallLilyPadBlock" => ("pad_amount", 4),
        "LabBottleBlock" | "TallBottleBlock" => ("count", 4),
        "PolyporeBlock" => ("count", 2),
        _ => return None,
    })
}

/// The item that places a pile block, when it isn't the block's own.
fn pile_item(name: &str) -> String {
    match name {
        "seashell_block" => format!("{NS}seashell"),
        n => format!("{NS}{n}"),
    }
}

/// Storage blocks: (block entity type, slots, menu).
fn storage(cls: &str) -> Option<(&'static str, usize, &'static str)> {
    Some(match cls {
        "CupboardBlock" => ("cupboard", 27, "minecraft:generic_9x3"),
        "WallCupboardBlock" => ("wall_cupboard", 9, "minecraft:generic_9x1"),
        "MailBoxBlock" => ("mailbox", 9, "minecraft:generic_3x3"),
        "CardboardBoxBlock" => ("cardboard_box", 9, "minecraft:generic_3x3"),
        "PresentBlock" => ("present", 9, "minecraft:generic_3x3"),
        "WallBookshelfBlock" => ("wall_bookshelf", 5, "minecraft:hopper"),
        _ => return None,
    })
}

// ---------------------------------------------------------------------------------------------
// Connecting shapes
// ---------------------------------------------------------------------------------------------

fn is_table(s: &Option<St>) -> bool {
    s.as_ref().is_some_and(|s| s.class() == "TableBlock")
}

/// TableBlock.updateShape / updateTableLegs: (leg_positions, legs).
fn table_legs(look: Look<'_>, p: [i32; 3]) -> (&'static str, bool) {
    let t = |d: &str| is_table(&look(at(p, d)));
    let td = |a: &str, b: &str| is_table(&look(at(at(p, a), b)));
    let (e, w, n, s) = (t("east"), t("west"), t("north"), t("south"));
    if !e && !w && !n && !s {
        return ("all", true);
    }
    if (e && w) || (n && s) {
        return ("none", false);
    }
    if w && s && !td("north", "west") {
        return ("south_west", true);
    }
    if w && n && !td("south", "west") {
        return ("north_west", true);
    }
    if e && n && !td("south", "east") {
        return ("north_east", true);
    }
    if e && s && !td("north", "east") {
        return ("south_east", true);
    }
    let lp = from_neighbours(w, n, e, s);
    (lp, lp != "none")
}

/// LegPosition.fromNeighborBlocks.
fn from_neighbours(w: bool, n: bool, e: bool, s: bool) -> &'static str {
    if w && n {
        "north_west"
    } else if w && s {
        "south_west"
    } else if e && n {
        "north_east"
    } else if e && s {
        "south_east"
    } else if w {
        "west"
    } else if e {
        "east"
    } else if n {
        "north"
    } else if s {
        "south"
    } else {
        "none"
    }
}

/// WoodenBenchBlock.updateShape.
fn bench_legs(look: Look<'_>, p: [i32; 3], facing: &str) -> &'static str {
    let b = |d: &str| {
        look(at(p, d)).is_some_and(|s| s.class() == "WoodenBenchBlock" && s.facing() == facing)
    };
    let (e, w, n, s) = (b("east"), b("west"), b("north"), b("south"));
    match facing {
        "north" | "south" => match (e, w) {
            (true, true) => "none",
            (false, false) => "all",
            (_, true) => "east",
            _ => "west",
        },
        _ => match (n, s) {
            (true, true) => "none",
            (false, false) => "all",
            (true, _) => "south",
            _ => "north",
        },
    }
}

fn is_curtain(s: &Option<St>) -> bool {
    s.as_ref().is_some_and(|s| s.class() == "TallCurtainBlock")
}

/// TallCurtainBlock.neighborChanged: alone, left, middle or right along the curtain's wall.
fn curtain_shape(look: Look<'_>, p: [i32; 3], facing: &str) -> &'static str {
    // (the side that makes it "right", the side that makes it "left") per facing, as the Java reads them.
    let (r, l) = match facing {
        "north" => ("west", "east"),
        "south" => ("east", "west"),
        "west" => ("south", "north"),
        _ => ("north", "south"),
    };
    let (hr, hl) = (is_curtain(&look(at(p, r))), is_curtain(&look(at(p, l))));
    match (hr, hl) {
        (true, true) => "middle",
        (true, false) => {
            if matches!(facing, "west" | "east") {
                "left"
            } else {
                "right"
            }
        }
        (false, true) => {
            if matches!(facing, "west" | "east") {
                "right"
            } else {
                "left"
            }
        }
        _ => "alone",
    }
}

/// The large chandelier's pieces: block_shape -> offset from the centre.
fn chandelier_offset(shape: &str) -> [i32; 3] {
    match shape {
        "north" => [0, 0, -1],
        "north_east" => [1, 0, -1],
        "east" => [1, 0, 0],
        "south_east" => [1, 0, 1],
        "south" => [0, 0, 1],
        "south_west" => [-1, 0, 1],
        "west" => [-1, 0, 0],
        "north_west" => [-1, 0, -1],
        _ => [0, 0, 0],
    }
}
fn chandelier_shape(off: [i32; 3]) -> Option<&'static str> {
    Some(match (off[0], off[2]) {
        (0, -1) => "north",
        (1, -1) => "north_east",
        (1, 0) => "east",
        (1, 1) => "south_east",
        (0, 1) => "south",
        (-1, 1) => "south_west",
        (-1, 0) => "west",
        (-1, -1) => "north_west",
        (0, 0) => "center",
        _ => return None,
    })
}
const CHANDELIER_RING: [&str; 8] = [
    "north",
    "north_east",
    "east",
    "south_east",
    "south",
    "south_west",
    "west",
    "north_west",
];

// ---------------------------------------------------------------------------------------------
// Placement
// ---------------------------------------------------------------------------------------------

fn place(level: &dyn LevelHost, event: &StateForPlacementEvent<'_>) -> Option<Option<St>> {
    let look = level_look(level);
    let mut st = St::parse(&level.state_string(event.state)?);
    let cls = st.class();
    if cls.is_empty() {
        return None;
    }
    let ctx = event.context;
    let pos = bp(ctx.pos);
    let looking = match ctx.source {
        PlacementSource::Player { yaw, .. } => horizontal(yaw),
        _ => "south",
    };
    let face = dir_name(ctx.face);
    let mut changed = false;
    // One more on a pile clicked with its item (canBeReplaced + getStateForPlacement).
    if let Some((prop, max)) = pile(cls)
        && let Some(cur) = look(pos).filter(|c| c.id == st.id)
    {
        let n = cur.int(prop);
        if n >= max {
            return Some(None);
        }
        return Some(Some(cur.with(prop, &(n + 1).to_string())));
    }
    if faces_placer(cls) {
        st.set("facing", opposite(looking));
        changed = true;
    } else if faces_look(cls) {
        st.set("facing", looking);
        changed = true;
    }
    match cls {
        "WallCandleBlock" | "OxidizableWallCandleBlock" => {
            st.set(
                "facing",
                if matches!(face, "up" | "down") {
                    opposite(looking)
                } else {
                    face
                },
            );
            changed = true;
        }
        "TableBlock" => {
            let (lp, legs) = table_legs(&look, pos);
            st.set("leg_positions", lp);
            st.set("legs", if legs { "true" } else { "false" });
            changed = true;
        }
        "WoodenBenchBlock" => {
            let f = st.facing().to_string();
            st.set("leg_positions", bench_legs(&look, pos, &f));
        }
        "TallCurtainBlock" => {
            if !replaceable(look(at(pos, "down"))) {
                return Some(None);
            }
            st.set("half", "upper");
            st.set("open", "false");
            let f = st.facing().to_string();
            st.set("direction_shape", curtain_shape(&look, pos, &f));
        }
        "TallLampBlock" => {
            if !replaceable(look(at(pos, "up"))) {
                return Some(None);
            }
            st.set("half", "lower");
            changed = true;
        }
        "LargeChandelierBlock" | "OxidizableLargeChandelierBlock" => {
            for s in CHANDELIER_RING {
                if !replaceable(look(add(pos, chandelier_offset(s)))) {
                    return Some(None);
                }
            }
            st.set("block_shape", "center");
            changed = true;
        }
        "TrellisBlock" => {
            st.set("under_block", if face == "down" { "true" } else { "false" });
        }
        _ => {}
    }
    changed.then_some(Some(st))
}

// ---------------------------------------------------------------------------------------------
// Seats
// ---------------------------------------------------------------------------------------------

struct Seat {
    dim: String,
    block: [i32; 3],
    stand: EntityRef,
    since: u64,
}

static SEATS: Mutex<Vec<Seat>> = Mutex::new(Vec::new());
static TICKS: Mutex<u64> = Mutex::new(0);

/// The Java rider has its feet `getYOffset() - 0.35` above the block's bottom (the seat entity's riding offset is
/// next to nothing, 1.20's player riding offset is -0.35). On this engine a rider's feet are 0.6 below a marker
/// armour stand, so the stand goes 0.4 above those feet (the lift Handcrafted's seats were checked with).
const SEAT_LIFT: f64 = 0.4;

fn sit(
    host: &mut dyn ServerHost,
    dim: &str,
    pos: [i32; 3],
    player: PlayerRef,
    offset: f64,
) -> bool {
    if player.entity().vehicle(&*host).is_some() {
        return false;
    }
    if lock(&SEATS).iter().any(|s| s.dim == dim && s.block == pos) {
        return false;
    }
    let feet = f64::from(pos[1]) + offset - 0.35;
    let at = [
        f64::from(pos[0]) + 0.5,
        feet + SEAT_LIFT,
        f64::from(pos[2]) + 0.5,
    ];
    let mut nbt = Compound::new();
    for k in ["Invisible", "Marker", "NoGravity", "Invulnerable", "Silent"] {
        nbt.insert(k.into(), Tag::Byte(1));
    }
    let Some(level) = host.level(dim) else {
        return false;
    };
    let Ok(Some(stand)) = level.summon(host, "minecraft:armor_stand", at, nbt) else {
        return false;
    };
    stand.add_tag(host, SEAT_TAG);
    if player.entity().start_riding(host, stand, true).is_err() {
        stand.discard(host);
        return false;
    }
    let since = *lock(&TICKS);
    lock(&SEATS).push(Seat {
        dim: dim.to_string(),
        block: pos,
        stand,
        since,
    });
    true
}

/// Forget the seats, saplings and tick count of the world that is closing or opening: they name runtime entity
/// ids and positions of one world, and a second world in the same game process starts its ids again from 0.
fn reset_world_state() {
    lock(&SEATS).clear();
    lock(&GROW).clear();
    *lock(&TICKS) = 0;
}

fn tend_seats(host: &mut dyn ServerHost) {
    let now = *lock(&TICKS);
    let seats = std::mem::take(&mut *lock(&SEATS));
    let mut keep = Vec::new();
    for s in seats {
        let reader: &dyn ServerHost = &*host;
        let stand = s.stand;
        // A handle can go stale: only ever discard the stand this seat summoned.
        if !stand.tags(reader).is_some_and(|t| t.contains(SEAT_TAG)) {
            continue;
        }
        let mut ridden = false;
        let _ = stand.visit_passengers(reader, &mut |_| {
            ridden = true;
            ControlFlow::Break(())
        });
        let block = host
            .level(&s.dim)
            .and_then(|l| host_look(reader, l)(s.block));
        let seat = block.is_some_and(|b| seat_offset(b.class()).is_some());
        if (ridden || now - s.since < 5) && seat {
            keep.push(s);
        } else {
            if ridden {
                stand.eject_passengers(host);
            }
            stand.discard(host);
        }
    }
    lock(&SEATS).extend(keep);
}

// ---------------------------------------------------------------------------------------------
// Clicks
// ---------------------------------------------------------------------------------------------

fn sneaking(host: &dyn ServerHost, player: u128) -> bool {
    host.player(player)
        .and_then(|p| p.entity().is_shift_key_down(host))
        == Some(true)
}

fn creative(host: &dyn ServerHost, player: u128) -> bool {
    host.player(player).and_then(|p| p.is_creative(host)) == Some(true)
}

fn item_id(host: &dyn ServerHost, stack: Option<&ItemStack>) -> String {
    stack
        .filter(|s| s.count() > 0)
        .and_then(|s| s.item().id(host))
        .unwrap_or_default()
        .to_owned()
}

fn stack_of(host: &dyn ServerHost, id: &str, n: u32) -> Option<ItemStack> {
    let key = host.item_key(&Identifier::parse(id).ok()?)?;
    ItemStack::new(key, n)
}

/// The held stack `n` smaller (not in creative).
fn use_up(host: &mut dyn ServerHost, click: &mod_api::BlockClick<'_>, n: u32) {
    if creative(host, click.player) {
        return;
    }
    let Some(st) = click.stack.cloned() else {
        return;
    };
    let Some(p) = host.player(click.player) else {
        return;
    };
    let slot = match click.hand {
        Hand::Main => p.selected_slot(&*host).map(|s| s as u32),
        _ => Some(40),
    };
    if let Some(slot) = slot {
        let _ = p.set_inventory_slot(host, slot, st.shrink(n));
    }
}

/// `hurtAndBreak(1)` on the held tool (not in creative).
fn damage_held(host: &mut dyn ServerHost, click: &mod_api::BlockClick<'_>) {
    if creative(host, click.player) {
        return;
    }
    let Some(p) = host.player(click.player) else {
        return;
    };
    let slot = if click.hand == Hand::Main {
        EquipmentSlot::MainHand
    } else {
        EquipmentSlot::OffHand
    };
    let _ = LivingRef::new(p.entity()).hurt_and_break(host, slot, 1);
}

fn sound(host: &mut dyn ServerHost, level: LevelRef, pos: [i32; 3], sound: &str, pitch: f32) {
    let at = pos.map(|v| f64::from(v) + 0.5);
    level.play_sound(host, sound, at, SoundSource::Blocks, 1.0, pitch, None);
}

fn give(host: &mut dyn ServerHost, level: LevelRef, pos: [i32; 3], id: &str, n: u32) {
    if let Some(st) = stack_of(host, id, n) {
        let _ = level.pop_resource(host, pos, st);
    }
}

fn set_state(host: &mut dyn ServerHost, level: LevelRef, pos: [i32; 3], st: &St) -> bool {
    let Some(state) = host.block_state_id(&st.text()) else {
        return false;
    };
    let flags = BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS;
    matches!(level.set_block(host, pos, state, flags), Ok(true))
}

fn exists(host: &dyn ServerHost, st: &St) -> bool {
    host.block_state_id(&st.text()).is_some()
}

fn click_level(
    host: &dyn ServerHost,
    click: &mod_api::BlockClick<'_>,
) -> Option<(String, LevelRef)> {
    let dim = click.dimension.key().id().to_string();
    let level = host.level(&dim)?;
    Some((dim, level))
}

/// A copper block's name split into (waxed, stage, rest): `waxed_exposed_copper_chain` is (true, 1, "copper_chain").
fn copper_parts(name: &str) -> Option<(bool, usize, &str)> {
    if !name.contains("copper") {
        return None;
    }
    let (waxed, rest) = match name.strip_prefix("waxed_") {
        Some(r) => (true, r),
        None => (false, name),
    };
    for (i, p) in ["exposed_", "weathered_", "oxidized_"].iter().enumerate() {
        if let Some(r) = rest.strip_prefix(p) {
            return Some((waxed, i + 1, r));
        }
    }
    Some((waxed, 0, rest))
}

fn copper_name(waxed: bool, stage: usize, rest: &str) -> String {
    let p = ["", "exposed_", "weathered_", "oxidized_"][stage];
    format!("{NS}{}{p}{rest}", if waxed { "waxed_" } else { "" })
}

fn is_axe(id: &str) -> bool {
    id.ends_with("_axe")
}

/// Everything a held item does to one of our blocks; true when it did something.
fn use_item(
    host: &mut dyn ServerHost,
    level: LevelRef,
    st: &St,
    held: &str,
    click: &mod_api::BlockClick<'_>,
) -> bool {
    let pos = click.pos;
    let cls = st.class();
    let name = st.name().to_string();
    // Lighting (WaterloggableLitBlock.use, WallCandleBlock.use, BonfireBlock.use).
    if (candle_lit(cls) || cls == "BonfireBlock")
        && !st.is("lit", "true")
        && !st.is("waterlogged", "true")
        && (held == "minecraft:flint_and_steel" || held == "minecraft:fire_charge")
    {
        if cls == "BonfireBlock" {
            set_connected(host, level, pos, st, "lit", "true");
        } else if !set_state(host, level, pos, &st.with("lit", "true")) {
            return false;
        }
        if held == "minecraft:flint_and_steel" {
            sound(host, level, pos, "minecraft:item.flintandsteel.use", 1.0);
            damage_held(host, click);
        } else {
            sound(host, level, pos, "minecraft:item.firecharge.use", 1.0);
            use_up(host, click, 1);
        }
        return true;
    }
    if cls == "BonfireBlock" && st.is("lit", "true") && held.ends_with("_shovel") {
        set_connected(host, level, pos, st, "lit", "false");
        sound(host, level, pos, "minecraft:block.fire.extinguish", 0.8);
        damage_held(host, click);
        return true;
    }
    // Axes: strip (chairs, tables, benches, redwood logs), scrape copper.
    if is_axe(held) {
        let stripped = st.renamed(&format!("{NS}stripped_{name}"));
        if !name.starts_with("stripped_")
            && exists(host, &stripped)
            && set_state(host, level, pos, &stripped)
        {
            sound(host, level, pos, "minecraft:item.axe.strip", 1.0);
            damage_held(host, click);
            return true;
        }
        if let Some((waxed, stage, rest)) = copper_parts(&name) {
            let to = if waxed {
                Some(copper_name(false, stage, rest))
            } else if stage > 0 {
                Some(copper_name(false, stage - 1, rest))
            } else {
                None
            };
            if let Some(to) = to {
                let n = st.renamed(&to);
                if exists(host, &n) && set_state(host, level, pos, &n) {
                    sound(
                        host,
                        level,
                        pos,
                        if waxed {
                            "minecraft:item.axe.wax_off"
                        } else {
                            "minecraft:item.axe.scrape"
                        },
                        1.0,
                    );
                    damage_held(host, click);
                    return true;
                }
            }
        }
    }
    if held == "minecraft:honeycomb"
        && let Some((false, stage, rest)) = copper_parts(&name)
    {
        let n = st.renamed(&copper_name(true, stage, rest));
        if exists(host, &n) && set_state(host, level, pos, &n) {
            sound(host, level, pos, "minecraft:item.honeycomb.wax_on", 1.0);
            use_up(host, click, 1);
            return true;
        }
    }
    // Shears clip a glowlily, which stops it growing.
    if held == "minecraft:shears"
        && matches!(cls, "GlowLilyCropBlock" | "GlowLilyPlantBlock")
        && !st.is("clipped", "true")
        && set_state(host, level, pos, &st.with("clipped", "true"))
    {
        sound(host, level, pos, "minecraft:block.growing_plant.crop", 1.0);
        damage_held(host, click);
        return true;
    }
    // CoinItem.useOn: eight coins add a layer to their stack.
    if cls.ends_with("CoinStackBlock") {
        let metal = name.trim_end_matches("_coin_stack");
        if held == format!("{NS}{metal}_coin") && click.stack.is_some_and(|s| s.count() >= 8) {
            let n = st.int("coin_layers");
            if n < 8
                && set_state(
                    host,
                    level,
                    pos,
                    &st.with("coin_layers", &(n + 1).to_string()),
                )
            {
                use_up(host, click, 8);
                return true;
            }
        }
    }
    // A pile clicked with its own item (when the engine doesn't send the click to the placement).
    if let Some((prop, max)) = pile(cls)
        && held == pile_item(&name)
        && !sneaking(host, click.player)
    {
        let n = st.int(prop);
        if n < max && set_state(host, level, pos, &st.with(prop, &(n + 1).to_string())) {
            let placed = host
                .block_state_id(&st.text())
                .and_then(|state| host.block_states().sound(state));
            if let Some(snd) = placed {
                sound(host, level, pos, snd.place.as_str(), 1.0);
            }
            use_up(host, click, 1);
            return true;
        }
    }
    false
}

/// What an empty hand does; true when it did something.
fn use_empty(
    host: &mut dyn ServerHost,
    dim: &str,
    level: LevelRef,
    st: &St,
    click: &mod_api::BlockClick<'_>,
) -> bool {
    let pos = click.pos;
    let cls = st.class();
    let main = click.hand == Hand::Main;
    if candle_lit(cls)
        && st.is("lit", "true")
        && set_state(host, level, pos, &st.with("lit", "false"))
    {
        sound(host, level, pos, "minecraft:block.candle.extinguish", 1.0);
        return true;
    }
    match cls {
        "LampBlock" => {
            let lit = !st.is("lit", "true");
            if set_state(
                host,
                level,
                pos,
                &st.with("lit", if lit { "true" } else { "false" }),
            ) {
                lamp_click(host, level, pos, lit);
                return true;
            }
        }
        "TallLampBlock" => {
            if st.is("powered", "true") {
                return false;
            }
            let lit = if st.is("lit", "true") {
                "false"
            } else {
                "true"
            };
            let other = at(pos, if st.is("half", "lower") { "up" } else { "down" });
            if set_state(host, level, pos, &st.with("lit", lit)) {
                let o = host_look(&*host, level)(other);
                if let Some(o) = o.filter(|o| o.id == st.id) {
                    set_state(host, level, other, &o.with("lit", lit));
                }
                lamp_click(host, level, pos, lit == "true");
                return true;
            }
        }
        "TubeTvBlock" if main => {
            let lit = !st.is("lit", "true");
            if set_state(
                host,
                level,
                pos,
                &st.with("lit", if lit { "true" } else { "false" }),
            ) {
                lamp_click(host, level, pos, lit);
                return true;
            }
        }
        "ChimneyBlock" if main && !sneaking(host, click.player) => {
            let open = if st.is("open", "true") {
                "false"
            } else {
                "true"
            };
            return set_state(host, level, pos, &st.with("open", open));
        }
        "ToiletBlock" if sneaking(host, click.player) => {
            let open = if st.is("open", "true") {
                "false"
            } else {
                "true"
            };
            if set_state(host, level, pos, &st.with("open", open)) {
                sound(host, level, pos, "minecraft:block.iron_trapdoor.close", 0.8);
                return true;
            }
        }
        "TallCurtainBlock" if main => {
            toggle_curtains(host, level, pos, st);
            sound(host, level, pos, "minecraft:block.wool.step", 1.0);
            return true;
        }
        "ThornBloomPlantBlock" => {
            let age = st.int("age");
            if age > 1 {
                let n = 1 + rand_int(host, level, 2) + u32::from(age == 3);
                give(host, level, pos, &format!("{NS}thornbloom_pear"), n);
                sound(
                    host,
                    level,
                    pos,
                    "minecraft:block.sweet_berry_bush.pick_berries",
                    1.0,
                );
                set_state(host, level, pos, &st.with("age", "1"));
                return true;
            }
        }
        "GlowLilyPlantBlock" => {
            let age = st.int("age");
            if age > 1 {
                let n = 1 + rand_int(host, level, 2) + u32::from(age == 2);
                give(host, level, pos, &format!("{NS}glowlily_bulb"), n);
                sound(
                    host,
                    level,
                    pos,
                    "minecraft:block.sweet_berry_bush.pick_berries",
                    1.0,
                );
                set_state(host, level, pos, &st.with("age", "0"));
                return true;
            }
        }
        _ => {}
    }
    // Seats: an empty main hand, not sneaking.
    if let Some(off) = seat_offset(cls)
        && main
        && !sneaking(host, click.player)
        && let Some(player) = host.player(click.player)
    {
        return sit(host, dim, pos, player, off);
    }
    false
}

/// `level.random.nextInt(bound)`.
fn rand_int(host: &mut dyn ServerHost, level: LevelRef, bound: u32) -> u32 {
    let b = bound.max(1) as i32;
    level
        .with_random(host, |r| r.next_int_bound(b))
        .unwrap_or(0) as u32
}

fn lamp_click(host: &mut dyn ServerHost, level: LevelRef, pos: [i32; 3], lit: bool) {
    let s = if lit {
        "minecraft:block.metal_pressure_plate.click_on"
    } else {
        "minecraft:block.metal_pressure_plate.click_off"
    };
    sound(host, level, pos, s, 1.25);
}

/// TallCurtainBlock.use: every curtain of the same kind, facing and openness joined to this one flips.
fn toggle_curtains(host: &mut dyn ServerHost, level: LevelRef, pos: [i32; 3], st: &St) {
    let open = st.get("open").unwrap_or("false").to_string();
    let to = if open == "true" { "false" } else { "true" };
    let facing = st.facing().to_string();
    let mut queue = VecDeque::from([pos]);
    let mut seen = HashSet::from([pos]);
    let mut todo = Vec::new();
    while let Some(p) = queue.pop_front() {
        if todo.len() >= 512 {
            break;
        }
        let Some(cur) = host_look(&*host, level)(p) else {
            continue;
        };
        if cur.id != st.id || cur.facing() != facing || cur.get("open") != Some(open.as_str()) {
            continue;
        }
        todo.push((p, cur));
        for d in ["north", "south", "west", "east", "up", "down"] {
            let n = at(p, d);
            if seen.insert(n) {
                queue.push_back(n);
            }
        }
    }
    for (p, cur) in todo {
        set_state(host, level, p, &cur.with("open", to));
    }
}

/// BonfireBlock.setLitStateForConnectedBonfires: every joined bonfire block of the same kind.
fn set_connected(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    st: &St,
    prop: &str,
    v: &str,
) {
    let mut queue = VecDeque::from([pos]);
    let mut seen = HashSet::from([pos]);
    while let Some(p) = queue.pop_front() {
        if seen.len() > 64 {
            break;
        }
        let Some(cur) = host_look(&*host, level)(p) else {
            continue;
        };
        if cur.id != st.id {
            continue;
        }
        set_state(host, level, p, &cur.with(prop, v));
        for d in H4 {
            let n = at(p, d);
            if seen.insert(n) {
                queue.push_back(n);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Level rules (random ticks, redstone, bone meal)
// ---------------------------------------------------------------------------------------------

fn lst(level: &dyn LevelHost, p: [i32; 3]) -> Option<St> {
    level_look(level)(p)
}

fn lset(level: &mut dyn LevelWriteHost, p: [i32; 3], st: &St) -> bool {
    let Some(state) = level.block_state_id(&st.text()) else {
        return false;
    };
    level.set_block(
        p,
        state,
        BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS,
    )
}

fn lset_default(
    level: &mut dyn LevelWriteHost,
    p: [i32; 3],
    id: &str,
    props: &[(&str, &str)],
) -> bool {
    let Some(d) = level.block_state_id(id).and_then(|s| level.state_string(s)) else {
        return false;
    };
    let mut st = St::parse(&d);
    for (k, v) in props {
        st.set(k, v);
    }
    lset(level, p, &st)
}

fn lrand(level: &mut dyn LevelWriteHost, bound: i32) -> i32 {
    level.with_random(|r| r.next_int_bound(bound)).unwrap_or(0)
}

/// One growth step; true when the block changed. `bonemeal` grows without the chance.
fn grow(level: &mut dyn LevelWriteHost, p: [i32; 3], st: &St, bonemeal: bool) -> bool {
    let cls = st.class();
    let age = st.int("age");
    match cls {
        // TorchflowerCropBlock with getStateForAge(2) the sapling / bush.
        "KiwiCropBlock" | "ThornbloomCropBlock" => {
            if !bonemeal && (level.raw_brightness(p, 0) < 9 || lrand(level, 3) != 0) {
                return false;
            }
            if age >= 1 {
                let to = if cls == "KiwiCropBlock" {
                    "clutter:kiwi_tree_sapling"
                } else {
                    "clutter:thornbloom"
                };
                lset_default(level, p, to, &[])
            } else {
                lset(level, p, &st.with("age", "1"))
            }
        }
        // GlowLilyCropBlock: ages 0-2, then the glowlily; clipped ones stop.
        "GlowLilyCropBlock" => {
            if st.is("clipped", "true") && !bonemeal {
                return false;
            }
            if !bonemeal && (level.raw_brightness(p, 0) < 9 || lrand(level, 5) != 0) {
                return false;
            }
            if age >= 2 {
                lset_default(level, p, "clutter:glowlily", &[])
            } else {
                lset(
                    level,
                    p,
                    &st.with("age", &(age + 1).to_string())
                        .with("clipped", "false"),
                )
            }
        }
        "GlowLilyPlantBlock" => {
            if age >= 2
                || (st.is("clipped", "true") && !bonemeal)
                || (!bonemeal && lrand(level, 5) != 0)
            {
                return false;
            }
            lset(level, p, &st.with("age", &(age + 1).to_string()))
        }
        // KiwiLeafBlock: kiwis ripen on the leaves (age 0-3).
        "KiwiLeafBlock" => {
            if age >= 3 || (!bonemeal && lrand(level, 5) != 0) {
                return false;
            }
            lset(level, p, &st.with("age", &(age + 1).to_string()))
        }
        // GiantLilyPadSeedlingBlock: ages up, then a 2x2 giant lily pad where there's water for it.
        "GiantLilyPadSeedlingBlock" => {
            if !bonemeal && lrand(level, 16) != 0 {
                return false;
            }
            if age < 3 {
                return lset(level, p, &st.with("age", &(age + 1).to_string()));
            }
            let cells = [
                (p, "south_west"),
                (at(p, "north"), "north_west"),
                (at(p, "east"), "south_east"),
                (at(at(p, "north"), "east"), "north_east"),
            ];
            let free = cells[1..].iter().all(|(c, _)| {
                lst(level, *c).is_some_and(|s| s.id == "minecraft:air")
                    && lst(level, at(*c, "down")).is_some_and(|s| s.id == "minecraft:water")
            });
            if !free {
                return false;
            }
            let flowering = if lrand(level, 2) == 0 {
                "true"
            } else {
                "false"
            };
            for (c, d) in cells {
                lset_default(
                    level,
                    c,
                    "clutter:giant_lily_pad",
                    &[("lily_pad_directions", d), ("flowering", flowering)],
                );
            }
            true
        }
        // BuddingOnyxBlock (BuddingAmethystBlock's rule): a bud on a random side, or the next bud size.
        "BuddingOnyxBlock" => {
            if lrand(level, 5) != 0 {
                return false;
            }
            let d = ["down", "up", "north", "south", "west", "east"][lrand(level, 6) as usize];
            let t = at(p, d);
            let Some(cur) = lst(level, t) else {
                return false;
            };
            let next = match cur.id.as_str() {
                "minecraft:air" | "minecraft:water" => "clutter:small_onyx_bud",
                "clutter:small_onyx_bud" => "clutter:medium_onyx_bud",
                "clutter:medium_onyx_bud" => "clutter:large_onyx_bud",
                "clutter:large_onyx_bud" => "clutter:onyx_cluster",
                _ => return false,
            };
            if cur.id.starts_with(NS) && cur.facing() != d {
                return false;
            }
            let wl = if cur.id == "minecraft:water" || cur.is("waterlogged", "true") {
                "true"
            } else {
                "false"
            };
            lset_default(level, t, next, &[("facing", d), ("waterlogged", wl)])
        }
        _ => {
            // WeatheringCopper: an unwaxed copper block takes the next stage now and then.
            let name = st.name().to_string();
            if cls.starts_with("Oxidizable")
                && !bonemeal
                && let Some((false, stage, rest)) = copper_parts(&name)
                && stage < 3
                && lrand(level, 20) == 0
            {
                let to = st.renamed(&copper_name(false, stage + 1, rest));
                return lset(level, p, &to);
            }
            false
        }
    }
}

fn bonemeal_target(cls: &str, st: &St) -> bool {
    match cls {
        "KiwiCropBlock"
        | "ThornbloomCropBlock"
        | "GlowLilyCropBlock"
        | "GiantLilyPadSeedlingBlock" => true,
        "GlowLilyPlantBlock" => st.int("age") < 2,
        "KiwiLeafBlock" => st.int("age") < 3,
        "SaplingBlock" => true,
        _ => false,
    }
}

/// Saplings whose tree grows at the end of the server tick: (dimension, position). The mod API places no feature from
/// a level hook, so the tree goes in through `place feature` from `server_tick`.
static GROW: Mutex<Vec<(String, [i32; 3])>> = Mutex::new(Vec::new());

/// SaplingBlock.advanceTree: stage 0 -> 1, then the tree.
fn advance_tree(level: &mut dyn LevelWriteHost, dim: &str, pos: [i32; 3], st: &St) {
    if st.int("stage") == 0 {
        lset(level, pos, &st.with("stage", "1"));
    } else {
        lock(&GROW).push((dim.to_string(), pos));
    }
}

/// The jar's tree growers: KiwiTreeSaplingGenerator (`kiwi_tree`) and RedwoodTreeSaplingGenerator (a 3x3 of
/// saplings `redwood`, a 2x2 `medium_redwood`, else `small_redwood`). The saplings go, the feature is placed, and
/// they come back when it can't be (AbstractTreeGrower.growTree).
fn grow_tree(host: &mut dyn ServerHost, dim: &str, pos: [i32; 3]) {
    let Some(level) = host.level(dim) else { return };
    let look = host_look(&*host, level);
    let Some(st) = look(pos).filter(|s| s.class() == "SaplingBlock") else {
        return;
    };
    let same =
        |dx: i32, dz: i32| look([pos[0] + dx, pos[1], pos[2] + dz]).is_some_and(|s| s.id == st.id);
    let square = |x: i32, z: i32, n: i32| (x..x + n).all(|i| (z..z + n).all(|j| same(i, j)));
    let (feature, corner, n) = if st.name() == "kiwi_tree_sapling" {
        ("kiwi_tree", (0, 0), 1)
    } else if let Some(c) = (-2..=0)
        .flat_map(|i| (-2..=0).map(move |j| (i, j)))
        .find(|&(i, j)| square(i, j, 3))
    {
        ("redwood", c, 3)
    } else if let Some(c) = (-1..=0)
        .flat_map(|i| (-1..=0).map(move |j| (i, j)))
        .find(|&(i, j)| square(i, j, 2))
    {
        ("medium_redwood", c, 2)
    } else {
        ("small_redwood", (0, 0), 1)
    };
    drop(look);
    let cells: Vec<[i32; 3]> = (0..n)
        .flat_map(|i| (0..n).map(move |j| [pos[0] + corner.0 + i, pos[1], pos[2] + corner.1 + j]))
        .collect();
    let Some(air) = host.block_state_id("minecraft:air") else {
        return;
    };
    let Some(sapling) = host.block_state_id(&st.text()) else {
        return;
    };
    for c in &cells {
        let _ = level.set_block(host, *c, air, BlockUpdateFlags::CLIENTS);
    }
    let at = cells[0];
    // The 3x3's feature goes at its middle (our stand-in giant trunk is 2x2 from there), the 2x2's at its corner.
    let at = if n == 3 {
        [at[0] + 1, at[1], at[2] + 1]
    } else {
        at
    };
    let cmd = format!(
        "execute in {dim} run place feature {NS}{feature} {} {} {}",
        at[0], at[1], at[2]
    );
    host.commands().run_command(&cmd);
    // The command reports success either way; a tree that went in has its trunk at the feature's origin `at`
    // (the 3x3's middle cell, not its corner). On a failure only cells still empty get their sapling back, so a
    // tree is never overwritten.
    let look = host_look(&*host, level);
    let air_at = |c: [i32; 3]| look(c).is_none_or(|s| s.id == "minecraft:air");
    if !air_at(at) {
        return;
    }
    let empty: Vec<[i32; 3]> = cells.iter().copied().filter(|&c| air_at(c)).collect();
    drop(look);
    for c in empty {
        let _ = level.set_block(host, c, sapling, BlockUpdateFlags::CLIENTS);
    }
}

/// LampBlock / TallLampBlock.neighborChanged.
fn lamp_redstone(level: &mut dyn LevelWriteHost, p: [i32; 3], st: &St) {
    let power = level.has_neighbor_signal(p).unwrap_or(false);
    if power == st.is("powered", "true") {
        return;
    }
    let v = if power { "true" } else { "false" };
    let mut n = st.with("powered", v);
    if st.is("lit", "true") != power {
        n.set("lit", v);
        let s = if power {
            "minecraft:block.metal_pressure_plate.click_on"
        } else {
            "minecraft:block.metal_pressure_plate.click_off"
        };
        level.play_sound(
            s,
            p.map(|v| f64::from(v) + 0.5),
            SoundSource::Blocks,
            1.0,
            1.25,
        );
    }
    lset(level, p, &n);
    if st.class() == "TallLampBlock" {
        let other = at(p, if st.is("half", "lower") { "up" } else { "down" });
        if let Some(o) = lst(level, other).filter(|o| o.id == st.id) {
            let mut on = o.with("powered", v);
            on.set("lit", n.get("lit").unwrap_or("false"));
            lset(level, other, &on);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Parts
// ---------------------------------------------------------------------------------------------

/// What an empty-handed click does that the client can tell from the block alone; the clicking client answers
/// `Success` for it, as vanilla's `useWithoutItem` does, so the click is not sent again for the off hand.
fn client_empty_acts(cls: &str, st: &St, main: bool, sneaking: bool) -> bool {
    if candle_lit(cls) && st.is("lit", "true") {
        return true;
    }
    match cls {
        "LampBlock" => true,
        "TallLampBlock" => !st.is("powered", "true"),
        "TubeTvBlock" | "TallCurtainBlock" => main,
        "ChimneyBlock" => main && !sneaking,
        "ToiletBlock" if sneaking => true,
        "ThornBloomPlantBlock" => st.int("age") > 1,
        "GlowLilyPlantBlock" => st.int("age") > 1,
        _ => main && !sneaking && seat_offset(cls).is_some(),
    }
}

struct Rules;

#[mod_api::sunrise_part]
impl BlockPart for Rules {
    fn state_for_placement(
        &self,
        level: LevelSide,
        event: &mut StateForPlacementEvent<'_>,
    ) -> Option<Option<BlockState>> {
        let (LevelSide::Server(level) | LevelSide::Client(level)) = level;
        match place(level, event)? {
            None => Some(None),
            Some(st) => level.block_state_id(&st.text()).map(Some),
        }
    }

    fn update_shape(
        &self,
        level: &mut dyn mod_api::LevelTicksHost,
        event: &mut UpdateShapeEvent,
    ) -> Option<BlockState> {
        let reader: &dyn LevelHost = &*level;
        let st = St::parse(&reader.state_string(event.state)?);
        let cls = st.class();
        let pos = bp(event.pos);
        let dir = dir_name(event.direction);
        let look = level_look(reader);
        let vertical = matches!(dir, "up" | "down");
        let air = || reader.block_state_id("minecraft:air");
        match cls {
            "TableBlock" if !vertical => {
                let (lp, legs) = table_legs(&look, pos);
                let n = st
                    .with("leg_positions", lp)
                    .with("legs", if legs { "true" } else { "false" });
                (n != st)
                    .then(|| reader.block_state_id(&n.text()))
                    .flatten()
            }
            "WoodenBenchBlock" if !vertical => {
                let n = st.with("leg_positions", bench_legs(&look, pos, st.facing()));
                (n != st)
                    .then(|| reader.block_state_id(&n.text()))
                    .flatten()
            }
            "TallCurtainBlock" | "TallLampBlock" => {
                if vertical {
                    let lower = st.is("half", "lower");
                    if lower != (dir == "up") {
                        return None;
                    }
                    let other = look(at(pos, dir));
                    let paired =
                        other.is_some_and(|o| o.id == st.id && o.get("half") != st.get("half"));
                    return if paired { None } else { air() };
                }
                if cls == "TallCurtainBlock" {
                    let n = st.with("direction_shape", curtain_shape(&look, pos, st.facing()));
                    return (n != st)
                        .then(|| reader.block_state_id(&n.text()))
                        .flatten();
                }
                None
            }
            "LargeChandelierBlock" | "OxidizableLargeChandelierBlock" if !vertical => {
                let shape = st.get("block_shape").unwrap_or("center");
                let off = chandelier_offset(shape);
                let center = [pos[0] - off[0], pos[1], pos[2] - off[2]];
                let n = at(pos, dir);
                let rel = [n[0] - center[0], 0, n[2] - center[2]];
                let want = chandelier_shape(rel)?;
                let ok =
                    look(n).is_some_and(|o| o.id == st.id && o.get("block_shape") == Some(want));
                if ok { None } else { air() }
            }
            _ => None,
        }
    }

    fn use_item_on(
        &self,
        host: HostSide<'_>,
        event: &mut UseItemOnEvent<'_>,
    ) -> Option<InteractionResult> {
        // The server does the work; the clicking client has no answer of its own for a held item.
        let HostSide::Server(host) = host else {
            return None;
        };
        let click = event.click;
        let held = item_id(&*host, click.stack);
        if held.is_empty() {
            return None;
        }
        let (_, level) = click_level(&*host, click)?;
        let st = St::parse(&host.block_state_string(event.state)?);
        if st.class().is_empty() {
            return None;
        }
        use_item(host, level, &st, &held, click).then_some(InteractionResult::Success)
    }

    fn use_without_item(
        &self,
        host: HostSide<'_>,
        event: &mut UseWithoutItemEvent<'_>,
    ) -> Option<InteractionResult> {
        let click = event.click;
        let host = match host {
            HostSide::Server(host) => host,
            HostSide::Client(host) => {
                let st = St::parse(&host.block_state_string(event.state)?);
                let cls = st.class();
                if cls.is_empty() {
                    return None;
                }
                let sneaking = host
                    .local_player()
                    .and_then(|p| p.entity().is_shift_key_down(&*host))
                    == Some(true);
                return client_empty_acts(cls, &st, click.hand == Hand::Main, sneaking)
                    .then_some(InteractionResult::Success);
            }
        };
        let (dim, level) = click_level(&*host, click)?;
        let st = St::parse(&host.block_state_string(event.state)?);
        if st.class().is_empty() {
            return None;
        }
        use_empty(host, &dim, level, &st, click).then_some(InteractionResult::Success)
    }

    /// LampBlock / TallLampBlock.neighborChanged.
    fn neighbor_changed(&self, level: &mut dyn LevelWriteHost, event: &mut NeighborChangedEvent) {
        let Some(st) = level.state_string(event.state).map(|t| St::parse(&t)) else {
            return;
        };
        if matches!(st.class(), "LampBlock" | "TallLampBlock") {
            lamp_redstone(level, bp(event.pos), &st);
        }
    }

    fn random_tick(&self, level: &mut dyn LevelWriteHost, event: &mut RandomTickEvent) -> Flow {
        let Some(st) = level.state_string(event.state).map(|t| St::parse(&t)) else {
            return Flow::Continue;
        };
        let cls = st.class();
        let ours = matches!(
            cls,
            "KiwiCropBlock"
                | "ThornbloomCropBlock"
                | "GlowLilyCropBlock"
                | "GlowLilyPlantBlock"
                | "GiantLilyPadSeedlingBlock"
                | "BuddingOnyxBlock"
        ) || cls.starts_with("Oxidizable");
        if cls == "SaplingBlock" {
            // SaplingBlock.randomTick: light 9 and one in seven, then advanceTree.
            let pos = bp(event.pos);
            if level.raw_brightness(at(pos, "up"), 0) >= 9 && lrand(level, 7) == 0 {
                advance_tree(level, event.dimension.key().id(), pos, &st);
            }
            return Flow::Cancel;
        }
        if cls == "KiwiLeafBlock" {
            // The leaves' own decay runs too.
            grow(level, bp(event.pos), &st, false);
            return Flow::Continue;
        }
        if !ours {
            return Flow::Continue;
        }
        grow(level, bp(event.pos), &st, false);
        Flow::Cancel
    }

    fn is_valid_bonemeal_target(
        &self,
        level: LevelSide,
        event: &mut IsValidBonemealTargetEvent,
    ) -> Option<bool> {
        let (LevelSide::Server(level) | LevelSide::Client(level)) = level;
        let st = St::parse(&level.state_string(event.state)?);
        let cls = st.class();
        matches!(
            cls,
            "KiwiCropBlock"
                | "ThornbloomCropBlock"
                | "GlowLilyCropBlock"
                | "GlowLilyPlantBlock"
                | "KiwiLeafBlock"
                | "GiantLilyPadSeedlingBlock"
                | "SaplingBlock"
        )
        .then(|| bonemeal_target(cls, &st))
    }

    fn is_bonemeal_success(
        &self,
        level: &mut dyn LevelRandomHost,
        event: &mut IsBonemealSuccessEvent,
    ) {
        let Some(st) = level.state_string(event.state).map(|t| St::parse(&t)) else {
            return;
        };
        match st.class() {
            // SaplingBlock.isBonemealSuccess: 0.45.
            "SaplingBlock" => {
                event.success = level.with_random(|r| r.next_float()).unwrap_or(1.0) < 0.45;
            }
            c if bonemeal_target(c, &st) => event.success = true,
            _ => {}
        }
    }

    fn perform_bonemeal(&self, level: &mut dyn LevelWriteHost, event: &mut PerformBonemealEvent) {
        let Some(st) = level.state_string(event.state).map(|t| St::parse(&t)) else {
            return;
        };
        if st.class() == "SaplingBlock" {
            advance_tree(level, event.dimension.key().id(), bp(event.pos), &st);
            return;
        }
        if bonemeal_target(st.class(), &st) {
            grow(level, bp(event.pos), &st, true);
        }
    }
}

/// The storage blocks' block entities: their slots are the declared container.
struct Storage;

#[mod_api::sunrise_part]
impl BlockPart for Storage {}

#[mod_api::sunrise_part]
impl BlockEntityPart for Storage {}

/// setPlacedBy of the blocks that are more than one block: the tall curtain's lower half, the tall lamp's upper half,
/// the large chandelier's ring.
struct Placer;

#[mod_api::sunrise_part]
impl ItemPart for Placer {
    fn on_block_placed_by(&self, host: &mut dyn ServerHost, event: &mut OnBlockPlacedByEvent<'_>) {
        let level = event.level;
        let Some(text) = host.block_state_string(event.state) else {
            return;
        };
        let st = St::parse(&text);
        let pos = event.pos;
        match st.class() {
            "TallCurtainBlock" => {
                let below = at(pos, "down");
                let shape = {
                    let look = host_look(&*host, level);
                    curtain_shape(&look, below, st.facing())
                };
                set_state(
                    host,
                    level,
                    below,
                    &st.with("half", "lower").with("direction_shape", shape),
                );
            }
            "TallLampBlock" => {
                set_state(host, level, at(pos, "up"), &st.with("half", "upper"));
            }
            "LargeChandelierBlock" | "OxidizableLargeChandelierBlock" => {
                for s in CHANDELIER_RING {
                    set_state(
                        host,
                        level,
                        add(pos, chandelier_offset(s)),
                        &st.with("block_shape", s),
                    );
                }
            }
            _ => {}
        }
    }
}

#[derive(Default)]
struct Mod;

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        let mut by_type: HashMap<&str, Vec<String>> = HashMap::new();
        for (name, cls) in classes::CLASSES {
            let id = format!("{NS}{name}");
            let key = r.block_key(&id);
            r.block_part(key, Rules);
            if let Some((be, slots, menu)) = storage(cls) {
                r.block_container(
                    &key,
                    ModContainer {
                        slots,
                        faces: std::array::from_fn(|_| (0..slots).collect()),
                        inputs: (0..slots).collect(),
                        outputs: (0..slots).collect(),
                        menu: Some(menu.into()),
                        ..Default::default()
                    },
                );
                by_type.entry(be).or_default().push(id.clone());
            }
            if matches!(
                *cls,
                "TallCurtainBlock"
                    | "TallLampBlock"
                    | "LargeChandelierBlock"
                    | "OxidizableLargeChandelierBlock"
            ) {
                let item = r.item_key(&id);
                r.item_part(item, Placer);
            }
        }
        for (be, ids) in by_type {
            let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
            let kind = r.block_entity_type(be).blocks(&refs).build();
            if let Err(e) = r.block_entity_part(&kind, Storage) {
                eprintln!("[clutter] block entity part {be} refused: {e:?}");
            }
        }
    }

    fn server_started(&self, _host: &mut dyn ServerHost, _event: &mut ServerStartedEvent) {
        reset_world_state();
    }

    fn server_stopped(&self, _host: &mut dyn ServerHost, _event: &mut ServerStoppedEvent) {
        reset_world_state();
    }

    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        if !matches!(event.phase, ServerTickPhase::ServerTickEnd { .. }) {
            return;
        }
        let n = {
            let mut t = lock(&TICKS);
            *t += 1;
            *t
        };
        if n == 20 {
            // Seats left in the world by an earlier session.
            host.commands().run_command(&format!(
                "kill @e[type=minecraft:armor_stand,tag={SEAT_TAG}]"
            ));
        }
        if !lock(&SEATS).is_empty() {
            tend_seats(host);
        }
        let grow = std::mem::take(&mut *lock(&GROW));
        for (dim, pos) in grow {
            grow_tree(host, &dim, pos);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes() {
        assert_eq!(class_of("oak_chair"), "WoodenChairBlock");
        assert_eq!(class_of("tall_pink_curtains"), "TallCurtainBlock");
        assert_eq!(class_of("large_golden_chandelier"), "LargeChandelierBlock");
    }

    #[test]
    fn copper() {
        assert_eq!(
            copper_parts("waxed_exposed_copper_chain"),
            Some((true, 1, "copper_chain"))
        );
        assert_eq!(
            copper_parts("oxidized_white_copper_candelabra"),
            Some((false, 3, "white_copper_candelabra"))
        );
        assert_eq!(
            copper_name(true, 2, "large_copper_chain"),
            "clutter:waxed_weathered_large_copper_chain"
        );
    }

    #[test]
    fn tables() {
        let look = |p: [i32; 3]| -> Option<St> {
            (p == [1, 0, 0] || p == [0, 0, 1])
                .then(|| St::parse("clutter:oak_table[leg_positions=all,legs=true]"))
        };
        assert_eq!(table_legs(&look, [0, 0, 0]), ("south_east", true));
    }

    #[test]
    fn empty_hand_prediction() {
        let lamp = St::parse("clutter:white_lamp[lit=false]");
        assert!(client_empty_acts("LampBlock", &lamp, false, false));
        let tall = St::parse("clutter:tall_white_lamp[powered=true]");
        assert!(!client_empty_acts("TallLampBlock", &tall, true, false));
        let chair = St::parse("clutter:oak_chair[facing=north]");
        assert!(client_empty_acts("WoodenChairBlock", &chair, true, false));
        assert!(!client_empty_acts("WoodenChairBlock", &chair, true, true));
        assert!(!client_empty_acts("WoodenChairBlock", &chair, false, false));
        let lit = St::parse("clutter:iron_candelabra[lit=true]");
        assert!(client_empty_acts("CandelabraBlock", &lit, false, true));
        let bloom = St::parse("clutter:thornbloom[age=1]");
        assert!(!client_empty_acts(
            "ThornBloomPlantBlock",
            &bloom,
            true,
            false
        ));
    }

    #[test]
    fn states_round_trip() {
        let st = St::parse("clutter:oak_table[leg_positions=all,legs=true]");
        assert_eq!(st.name(), "oak_table");
        assert_eq!(
            st.with("legs", "false").text(),
            "clutter:oak_table[leg_positions=all,legs=false]"
        );
        assert_eq!(St::parse("clutter:anchor").text(), "clutter:anchor");
        assert_eq!(st.int("legs"), 0);
    }

    #[test]
    fn looking_and_neighbours() {
        assert_eq!(horizontal(0.0), "south");
        assert_eq!(horizontal(90.0), "west");
        assert_eq!(horizontal(180.0), "north");
        assert_eq!(horizontal(-90.0), "east");
        assert_eq!(from_neighbours(true, true, false, false), "north_west");
        assert_eq!(from_neighbours(false, false, false, false), "none");
        assert_eq!(opposite("north"), "south");
    }

    #[test]
    fn benches_and_curtains() {
        let none = |_: [i32; 3]| -> Option<St> { None };
        assert_eq!(bench_legs(&none, [0, 0, 0], "north"), "all");
        let curtain = |p: [i32; 3]| {
            (p == [-1, 0, 0]).then(|| St::parse("clutter:tall_pink_curtains[facing=north]"))
        };
        assert_eq!(curtain_shape(&curtain, [0, 0, 0], "north"), "right");
        assert_eq!(curtain_shape(&none, [0, 0, 0], "north"), "alone");
    }

    #[test]
    fn piles_and_seats() {
        assert_eq!(pile("SeashellBlock"), Some(("amount", 4)));
        assert_eq!(pile("TableBlock"), None);
        assert_eq!(pile_item("seashell_block"), "clutter:seashell");
        assert_eq!(seat_offset("ArmchairBlock"), Some(0.25));
        assert_eq!(seat_offset("TableBlock"), None);
        assert_eq!(
            storage("CupboardBlock"),
            Some(("cupboard", 27, "minecraft:generic_9x3"))
        );
    }

    #[test]
    fn chandelier() {
        for s in CHANDELIER_RING {
            assert_eq!(chandelier_shape(chandelier_offset(s)), Some(s));
        }
    }
}
