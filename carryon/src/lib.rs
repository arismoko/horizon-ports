//! carryon: Carry On 2.1.2.7 (Tschipp, Purplicious_Cow, cy4n; LGPL-3.0), from the decompiled jar
//! (tools/dl/carryon/src). data/ and assets/ are the content mod mods/carryon (tools/carryon_port.py). All of the
//! rules are the mod API: no game module, no mixin.
//!
//! - PickupHandler.tryPickUpBlock / tryPickupEntity: sneak (the jar's carry key is unbound on Fabric, so it falls
//!   back to sneaking) and right-click with both hands empty. A block needs a block entity (`pickupAllBlocks`
//!   false); its state and block entity data go into the player's `carryon:carry` attachment, and the block goes
//!   without drops. A mob's data (`/data get entity`) goes there too and the mob is removed.
//! - PlacementHandler.tryPlaceBlock / tryPlaceEntity / tryStackEntity: a right-click sets the block down (on the
//!   clicked block when it can be replaced, else beside it) with its data back, its facing, axis and rotation
//!   taken from the player as a placement would; a mob is summoned there from its data, or onto the top of a
//!   stack of mobs clicked.
//! - CarryOnCommon.onCarryTick: Slowness by weight (mobs by size, blocks by the size of their data), the hotbar
//!   slot held, nothing in the main hand; no breaking or hitting while carrying (`hitWhileCarrying` false).
//! - PlacementHandler.placeCarriedOnDeath: on death without keepInventory the carried thing is set down where the
//!   player died; with it, it stays carried.
//! - Drawing (CarriedObjectRender): the server sends who carries what on `carryon:carried`; a carried block is
//!   drawn in the hands in first person and in front of the chest on players seen in third person; a carried mob
//!   is drawn over the hotbar in first person (the API draws a living entity only in a screen or the HUD) and
//!   as its spawn egg on players seen in third person.
//!
//! The options are `config.rs`: switches and numbers are mod settings, the lists are text files in
//! `config/carryon/`. The carry key is a Controls binding (left shift unless rebound) the client reports to the server;
//! `/carryon debug|clear|place` are the original's commands.
//! - Pickup scripts (`scripts.rs`, `common/scripting`): with `use_scripts` on, a data pack's
//!   `carryon/scripts/*.json` may match a block or mob, skip checks, add conditions on the player, run commands
//!   on pickup, each carried tick and placement, and change how the carried thing is drawn.
//!
//! Not ported: picking up players (a player can't be a vehicle on this engine), the raised arms and a script's
//! arm rotations (the arm pose hooks aren't asked for an empty hand), Game Stages pickup conditions (without
//! Game Stages the original passes them all), the model overrides.

mod config;
mod scripts;

use std::collections::{HashMap, HashSet};
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use mod_api::{
    ActiveEffect, AddServerReloadListenersEvent, AttachmentCodec, AttachmentDescriptor,
    AttachmentKey, AttackEntityEvent, BlockPos, BlockUpdateFlags, BreakBlockEvent, CameraType,
    ClientHost, ClientTickPostEvent, Command, CommandArgumentValue, CommandSource, Compound,
    Direction, EntityInteractEvent, EntityRef, FileToIdConverter, Flow, GameMode, GuiEntity,
    GuiEntityLook, GuiEntityView, Hand, HitHost, Holder, Hooks, Host, HostBlockStates, HostSide,
    HudMode, IPayloadContext, IPayloadHandler, Identifier, InteractionResult, ItemDisplayContext,
    ItemKey, ItemStack, KeyRegistration, LeftClickBlockEvent, LivingDeathEvent,
    LivingIncomingDamageEvent, LivingRef, MainHand, MobCategory, MouseScrollingEvent, PayloadBytes,
    PayloadHandleError, PayloadId, PlacementContext, PlacementSource, PlayerRef,
    PlayerSwitchHotbarSlotPreEvent, RegisterCommandsEvent, Registrar, RegistrationContext,
    ReloadContext, RemovalReason, RenderGuiPostEvent, RenderHandEvent, RenderPlayerPostEvent,
    RightClickBlockEvent, ServerHost, ServerTickEvent, ServerTickPhase, SoundSource, Tag, Text,
};

use config::{Lists, Settings, permitted};

const CHANNEL: &str = "carryon:carried";
const KEY_CHANNEL: &str = "carryon:key";

// The original's default config (`CarryConfig`, `carryon-common.json`).
pub(crate) const FORBIDDEN_TILES: &[&str] = &[
    "#forge:immovable",
    "#forge:relocation_not_supported",
    "#neoforge:immovable",
    "#neoforge:relocation_not_supported",
    "minecraft:end_portal",
    "minecraft:piston_head",
    "#c:relocation_not_supported",
    "minecraft:end_gateway",
    "minecraft:tall_grass",
    "minecraft:large_fern",
    "minecraft:peony",
    "minecraft:rose_bush",
    "minecraft:lilac",
    "minecraft:sunflower",
    "minecraft:*_bed",
    "minecraft:*_door",
    "minecraft:big_dripleaf_stem",
    "minecraft:waterlily",
    "minecraft:cake",
    "minecraft:nether_portal",
    "minecraft:tall_seagrass",
    "animania:block_trough",
    "animania:block_invisiblock",
    "colossalchests:*",
    "ic2:*",
    "bigreactors:*",
    "forestry:*",
    "tconstruct:*",
    "rustic:*",
    "botania:*",
    "astralsorcery:*",
    "quark:colored_bed_*",
    "immersiveengineering:*",
    "embers:block_furnace",
    "embers:ember_bore",
    "embers:ember_activator",
    "embers:mixer",
    "embers:heat_coil",
    "embers:large_tank",
    "embers:crystal_cell",
    "embers:alchemy_pedestal",
    "embers:boiler",
    "embers:combustor",
    "embers:catalzyer",
    "embers:field_chart",
    "embers:inferno_forge",
    "storagedrawers:framingtable",
    "skyresources:*",
    "lootbags:*",
    "exsartagine:*",
    "aquamunda:tank",
    "opencomputers:*",
    "malisisdoors:*",
    "industrialforegoing:*",
    "minecolonies:*",
    "thaumcraft:pillar*",
    "thaumcraft:infernal_furnace",
    "thaumcraft:placeholder*",
    "thaumcraft:infusion_matrix",
    "thaumcraft:golem_builder",
    "thaumcraft:thaumatorium*",
    "magneticraft:oil_heater",
    "magneticraft:solar_panel",
    "magneticraft:steam_engine",
    "magneticraft:shelving_unit",
    "magneticraft:grinder",
    "magneticraft:sieve",
    "magneticraft:solar_tower",
    "magneticraft:solar_mirror",
    "magneticraft:container",
    "magneticraft:pumpjack",
    "magneticraft:refinery",
    "magneticraft:hydraulic_press",
    "magneticraft:multiblock_gap",
    "refinedstorage:*",
    "mcmultipart:*",
    "enderstorage:*",
    "betterstorage:*",
    "practicallogistics2:*",
    "wearablebackpacks:*",
    "rftools:screen",
    "rftools:creative_screen",
    "create:*",
    "magic_doorknob:*",
    "iceandfire:*",
    "ftbquests:*",
    "waystones:*",
    "contact:*",
    "framedblocks:*",
    "securitycraft:*",
    "forgemultipartcbe:*",
    "integrateddynamics:cable",
    "mekanismgenerators:wind_generator",
    "cookingforblockheads:cabinet",
    "cookingforblockheads:corner",
    "cookingforblockheads:counter",
    "cookingforblockheads:oven",
    "cookingforblockheads:toaster",
    "cookingforblockheads:milk_jar",
    "cookingforblockheads:cow_jar",
    "cookingforblockheads:fruit_basket",
    "cookingforblockheads:cooking_table",
    "cookingforblockheads:fridge",
    "cookingforblockheads:sink",
    "chipped:*",
    "irons_spellbooks:*",
    "create*:*",
    "simple_pipes:*",
    "libmultipart:*",
    "quark:tiny_potato",
    "ait:*",
    "vampirism:*",
    "extrastorage:*",
    "relics:researching_table",
    "sophisticatedstorage:*chest",
    "powah:*",
    "advancementtrophies:trophy",
    "mekanismgenerators:heat_generator",
    "mna:filler_block",
    "create_enchantment_industry:*",
    "graveyard:*",
    "immersivepetroleum:*",
    "tardis:interior_door",
    "cuffed:*",
    "littletiles:*",
    "butchersdelight:*",
    "extended_drawers:*",
    "functionalstorage:*",
    "sophisticatedstorage:*",
    "farmersdelight:*",
    "modern_industrialization:*_cable",
    "modern_industrialization:item_pipe",
    "modern_industrialization:*_item_pipe",
    "modern_industrialization:fluid_pipe",
    "modern_industrialization:*_fluid_pipe",
    "yigd:*",
    "domum_ornamentum:*",
    "connectiblechains:*",
    "cobblemon:*",
    "snowrealmagic:*",
    "forbidden_arcanus:black_hole",
    "sgjourney:*",
    "copycats:*",
    "expandedstorage:*",
    "replication:*",
    "coloreditemframes:*",
];
pub(crate) const FORBIDDEN_ENTITIES: &[&str] = &[
    "#c:capturing_not_supported",
    "#c:teleporting_not_supported",
    "minecraft:end_crystal",
    "minecraft:ender_dragon",
    "minecraft:ghast",
    "minecraft:fireball",
    "minecraft:small_fireball",
    "minecraft:whither_skull",
    "minecraft:shulker",
    "minecraft:leash_knot",
    "minecraft:armor_stand",
    "minecraft:whither_skull_dangerous",
    "minecraft:dragon_fireball",
    "minecraft:item_frame",
    "minecraft:painting",
    "minecraft:shulker_bullet",
    "minecraft:evoker_fangs",
    "minecraft:glow_item_frame",
    "minecraft:tnt",
    "minecraft:trident",
    "minecraft:arrow",
    "minecraft:spectral_arrow",
    "minecraft:interaction",
    "minecraft:marker",
    "minecraft:block_display",
    "minecraft:item_display",
    "minecraft:text_display",
    "minecraft:cushion",
    "animania:hamster",
    "animania:ferret*",
    "animania:hedgehog*",
    "animania:cart",
    "animania:wagon",
    "mynko:*",
    "pixelmon:*",
    "mocreatures:*",
    "quark:totem",
    "vehicle:*",
    "securitycraft:*",
    "taterzens:npc",
    "easy_npc:*",
    "bodiesbodies:dead_body",
    "littletiles:*",
    "connectiblechains:*",
    "cobblemon:*",
    "create:*",
    "swem:*",
    "toms_mobs:*",
    "simulated:*",
    "aeronautics:*",
    "offroad:*",
    "coloreditemframes:*",
];
pub(crate) const FORBIDDEN_STACKING: &[&str] = &["minecraft:horse", "minecraft:ender_dragon"];

/// What a player carries, as the jar's `CarryOnData` (saved with the player): empty when nothing.
/// `type` "block" with `block` (the state string) and `tile` (the block entity's data), or "entity" with `kind`,
/// `entity` (its data) and `size` (width, height); `selected` is the hotbar slot held.
static CARRY: RwLock<Option<AttachmentKey<Compound>>> = RwLock::new(None);

/// The key `register` was issued for the carry attachment; `None` if registration was refused.
fn carry_key() -> Option<AttachmentKey<Compound>> {
    CARRY.read().unwrap_or_else(|e| e.into_inner()).clone()
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn s(t: &str) -> Tag {
    Tag::String(t.into())
}

fn get_str(c: &Compound, k: &str) -> Option<String> {
    match c.get(&k.into()) {
        Some(Tag::String(v)) => Some(v.to_string_lossy()),
        _ => None,
    }
}

fn get_int(c: &Compound, k: &str) -> Option<i32> {
    match c.get(&k.into()) {
        Some(Tag::Int(v)) => Some(*v),
        _ => None,
    }
}

/// `ns:block[k=v,...]` as its id and properties, in order.
fn parse_state(text: &str) -> (String, Vec<(String, String)>) {
    let (id, rest) = text.split_once('[').unwrap_or((text, ""));
    let props = rest
        .trim_end_matches(']')
        .split(',')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    (id.to_string(), props)
}

fn state_text(id: &str, props: &[(String, String)]) -> String {
    if props.is_empty() {
        return id.to_string();
    }
    let p: Vec<String> = props.iter().map(|(k, v)| format!("{k}={v}")).collect();
    format!("{id}[{}]", p.join(","))
}

fn set_prop(props: &mut [(String, String)], key: &str, value: &str) {
    if let Some(p) = props.iter_mut().find(|(k, _)| k == key) {
        p.1 = value.to_string();
    }
}

fn prop<'a>(props: &'a [(String, String)], key: &str) -> Option<&'a str> {
    props
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// `Direction.fromYRot`: where a player turned `yaw` looks.
fn horizontal(yaw: f32) -> &'static str {
    match ((yaw / 90.0 + 0.5).floor() as i32) & 3 {
        0 => "south",
        1 => "west",
        2 => "north",
        _ => "east",
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

fn face_name(face: [i32; 3]) -> &'static str {
    match face {
        [0, 1, 0] => "up",
        [0, -1, 0] => "down",
        [0, 0, -1] => "north",
        [0, 0, 1] => "south",
        [-1, 0, 0] => "west",
        _ => "east",
    }
}

/// `Direction.orderedByNearest(entity)[0]`: the way the player looks most.
fn nearest_looking(yaw: f32, pitch: f32) -> &'static str {
    if pitch > 45.0 {
        "down"
    } else if pitch < -45.0 {
        "up"
    } else {
        horizontal(yaw)
    }
}

/// Blocks whose `facing` takes the placer's look up and down too (`getNearestLookingDirection`); any other
/// block with a block entity faces the placer horizontally (a Farmer's Delight cabinet has six facings but is
/// placed by the horizontal look).
const SIX_WAY: &[&str] = &[
    "minecraft:barrel",
    "minecraft:dispenser",
    "minecraft:dropper",
    "minecraft:observer",
    "minecraft:crafter",
    "minecraft:command_block",
    "minecraft:chain_command_block",
    "minecraft:repeating_command_block",
];

/// Click and look of a placement, for `getPlacementState`.
struct Placement {
    face: [i32; 3],
    yaw: f32,
    pitch: f32,
    in_water: bool,
}

/// PlacementHandler.getPlacementState: the carried state keeps its properties but those a placement decides
/// (direction properties, axes and the config's exceptions), here from the vanilla rules for the blocks that
/// carry a block entity: chests, furnaces and most mod blocks face the placer, six-way blocks (barrels,
/// dispensers) look back at them, hoppers point into the clicked block, shulker boxes away from it; an axis
/// follows the clicked face, a sign's or skull's rotation the placer's turn, a chest stands alone, and a block
/// set into still water is waterlogged.
fn placement_state(host: &dyn ServerHost, carried: &str, p: &Placement) -> String {
    let (id, mut props) = parse_state(carried);
    let face = face_name(p.face);
    if let Some(cur) = prop(&props, "facing").map(str::to_string) {
        let try_dir = |props: &[(String, String)], d: &str| {
            let mut t = props.to_vec();
            set_prop(&mut t, "facing", d);
            host.block_state_id(&state_text(&id, &t)).is_some()
        };
        let want = if id == "minecraft:hopper" {
            if p.face[1] != 0 {
                "down".to_string()
            } else {
                opposite(face).to_string()
            }
        } else if id.ends_with("shulker_box") {
            face.to_string()
        } else if SIX_WAY.contains(&id.as_str()) {
            opposite(nearest_looking(p.yaw, p.pitch)).to_string()
        } else {
            opposite(horizontal(p.yaw)).to_string()
        };
        let pick = if try_dir(&props, &want) { want } else { cur };
        set_prop(&mut props, "facing", &pick);
    }
    if prop(&props, "horizontal_facing").is_some() {
        set_prop(&mut props, "horizontal_facing", opposite(horizontal(p.yaw)));
    }
    if prop(&props, "axis").is_some() {
        let axis = match face {
            "up" | "down" => "y",
            "north" | "south" => "z",
            _ => "x",
        };
        set_prop(&mut props, "axis", axis);
    }
    if prop(&props, "rotation").is_some() {
        let r = (((p.yaw + 180.0) * 16.0 / 360.0 + 0.5).floor() as i32).rem_euclid(16);
        set_prop(&mut props, "rotation", &r.to_string());
    }
    if id.ends_with("chest") && prop(&props, "type").is_some() {
        set_prop(&mut props, "type", "single");
    }
    if prop(&props, "waterlogged").is_some() {
        set_prop(
            &mut props,
            "waterlogged",
            if p.in_water { "true" } else { "false" },
        );
    }
    state_text(&id, &props)
}

/// A value of a `Direction` property (`facing`, `hopper`'s facing...), not a side's boolean.
fn is_direction(value: &str) -> bool {
    matches!(value, "north" | "south" | "east" | "west" | "up" | "down")
}

/// `PlacementHandler.getPlacementState`: the carried state, with the properties a placement
/// decides taken from the state vanilla would place: every direction and axis, the listed
/// exceptions, and waterlogging. A placement of another block leaves the carried state alone.
fn merge_placement(carried: &str, placed: &str, exception: &dyn Fn(&str, &str) -> bool) -> String {
    let (id, mut props) = parse_state(carried);
    let (placed_id, placed_props) = parse_state(placed);
    if id != placed_id {
        return carried.to_owned();
    }
    for (name, value) in &placed_props {
        let direction = is_direction(value);
        let axis = name.ends_with("axis") && matches!(value.as_str(), "x" | "y" | "z");
        if (direction || axis || name == "waterlogged" || exception(&id, name))
            && prop(&props, name).is_some()
        {
            set_prop(&mut props, name, value);
        }
    }
    state_text(&id, &props)
}

fn direction_of(face: [i32; 3]) -> Direction {
    match face {
        [0, -1, 0] => Direction::Down,
        [0, 1, 0] => Direction::Up,
        [0, 0, -1] => Direction::North,
        [0, 0, 1] => Direction::South,
        [-1, 0, 0] => Direction::West,
        _ => Direction::East,
    }
}

fn add(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// UUID ints as NBT stores them.
fn uuid_of(tag: Option<&Tag>) -> Option<u128> {
    match tag {
        Some(Tag::IntArray(v)) if v.len() == 4 => Some(
            v.iter()
                .fold(0u128, |acc, i| (acc << 32) | u128::from(*i as u32)),
        ),
        _ => None,
    }
}

enum Job {
    PickBlock {
        uuid: u128,
        dim: String,
        pos: [i32; 3],
    },
    PickEntity {
        uuid: u128,
        entity: u32,
    },
    Place {
        uuid: u128,
        dim: String,
        pos: [i32; 3],
        face: [i32; 3],
    },
    Stack {
        uuid: u128,
        target: u32,
    },
    /// Set down what a dead player carried, where they died.
    Drop {
        dim: String,
        at: [f64; 3],
        carry: Compound,
    },
}

/// What a client draws for one carrying player.
#[derive(Clone)]
enum Shown {
    Block {
        id: String,
        props: Vec<(String, String)>,
    },
    Entity {
        kind: String,
        data: Compound,
    },
}

/// What a client draws for one carrying player, and how its script changes the drawing.
#[derive(Clone)]
struct Carried {
    what: Shown,
    render: Option<scripts::Render>,
}

fn shown() -> &'static Mutex<HashMap<u32, Carried>> {
    static S: OnceLock<Mutex<HashMap<u32, Carried>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Spawn eggs by entity type, looked up once on the client (the API finds items by key only).
fn spawn_eggs(host: &dyn Host) -> &'static HashMap<String, ItemKey> {
    static E: OnceLock<HashMap<String, ItemKey>> = OnceLock::new();
    E.get_or_init(|| {
        host.item_keys()
            .into_iter()
            .filter_map(|k| {
                let id = k.id(host)?;
                id.strip_suffix("_spawn_egg")
                    .map(|kind| (kind.to_string(), k))
            })
            .collect()
    })
}

/// The client's carry key, held or let go (`[1]` or `[0]`).
struct KeyHandler;

impl IPayloadHandler for KeyHandler {
    fn handle(
        &self,
        payload: PayloadBytes,
        context: &mut dyn IPayloadContext,
    ) -> Result<(), PayloadHandleError> {
        if let Ok(player) = context.player() {
            let mut held = key_held();
            if payload.0.first() == Some(&1) {
                held.insert(player.uuid());
            } else {
                held.remove(&player.uuid());
            }
        }
        Ok(())
    }
}

struct CarriedHandler;

impl IPayloadHandler for CarriedHandler {
    /// `[entity id u32, kind u8 (0 block, 1 entity), length u32, bytes, length u32, render]` repeated: a block's
    /// state string, or a mob's type, a 0 byte and its data as network NBT; then its script's render JSON,
    /// empty for none. The whole list each time.
    fn handle(
        &self,
        payload: PayloadBytes,
        _context: &mut dyn IPayloadContext,
    ) -> Result<(), PayloadHandleError> {
        let b = &payload.0;
        let mut map = HashMap::new();
        let mut i = 0;
        while i + 9 <= b.len() {
            let id = u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
            let kind = b[i + 4];
            let len = u32::from_be_bytes([b[i + 5], b[i + 6], b[i + 7], b[i + 8]]) as usize;
            i += 9;
            if i + len > b.len() {
                break;
            }
            let body = &b[i..i + len];
            i += len;
            if i + 4 > b.len() {
                break;
            }
            let render_len = u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as usize;
            i += 4;
            if i + render_len > b.len() {
                break;
            }
            let render = std::str::from_utf8(&b[i..i + render_len])
                .ok()
                .and_then(scripts::Render::from_text);
            i += render_len;
            let what = if kind == 0 {
                let (bid, props) = parse_state(&String::from_utf8_lossy(body));
                Shown::Block { id: bid, props }
            } else if let Some(z) = body.iter().position(|c| *c == 0) {
                let kind = String::from_utf8_lossy(&body[..z]).to_string();
                let data = match Tag::from_network_bytes(&body[z + 1..], None) {
                    Ok((Tag::Compound(c), _)) => c,
                    _ => Compound::new(),
                };
                Shown::Entity { kind, data }
            } else {
                continue;
            };
            map.insert(id, Carried { what, render });
        }
        *lock(shown()) = map;
        Ok(())
    }
}

#[derive(Default)]
struct Mod {
    ticks: AtomicU64,
    /// Players (entity ids) hurt since the last tick, for `dropCarriedWhenHit`.
    hit: Mutex<Vec<u32>>,
    /// The client's carry key as last sent.
    key_down: AtomicBool,
    jobs: Mutex<Vec<Job>>,
    /// Whether the last list sent named anyone (an empty list is sent once after the last one is set down).
    sent_any: Mutex<bool>,
}

fn carry_of(host: &mut dyn ServerHost, uuid: u128) -> Compound {
    let Some(key) = carry_key() else {
        return Compound::new();
    };
    key.get_existing(host, Holder::Player(uuid), |value| value.cloned())
        .ok()
        .flatten()
        .unwrap_or_default()
}

fn carrying(host: &mut dyn ServerHost, uuid: u128) -> bool {
    !carry_of(host, uuid).is_empty()
}

/// Stores what the player carries; `false` when it could not be stored.
fn set_carry(host: &mut dyn ServerHost, uuid: u128, c: Compound) -> bool {
    let Some(key) = carry_key() else {
        return false;
    };
    match key.set(host, Holder::Player(uuid), c) {
        Ok(_) => true,
        Err(e) => {
            eprintln!("[carryon] carry data not saved: {e:?}");
            false
        }
    }
}

fn hands_empty(host: &dyn Host, player: PlayerRef) -> bool {
    let living = LivingRef::new(player.entity());
    [Hand::Main, Hand::Off].into_iter().all(|h| {
        living
            .with_hand_item(host, h, |s| s.count())
            .is_none_or(|n| n == 0)
    })
}

/// PickupHandler.canCarryGeneral: the carry key is held (it is left shift unless the player rebinds it), hands are empty.
fn may_pick(host: &mut dyn ServerHost, player: PlayerRef) -> bool {
    let reader: &dyn ServerHost = &*host;
    if !key_held().contains(&player.uuid()) || !hands_empty(reader, player) {
        return false;
    }
    if matches!(
        player.game_mode(reader),
        Some(GameMode::Adventure) | Some(GameMode::Spectator)
    ) {
        return false;
    }
    !carrying(host, player.uuid())
}

fn creative(host: &dyn Host, player: PlayerRef) -> bool {
    player.is_creative(host) == Some(true)
}

/// `SizeHelper.getPlayerScaleFactor` for `player`.
fn scale_of(host: &dyn Host, player: PlayerRef, cfg: &Settings) -> f64 {
    let scale = LivingRef::new(player.entity())
        .attribute_value(host, "minecraft:scale")
        .unwrap_or(1.0);
    config::scale_factor(cfg.relative_player_scale, scale)
}

/// `PickupHandler.isInDistance`: within `maxDistance`, stretched by the player's reach against its
/// base when `scaleDistance` is on.
fn in_reach(host: &dyn Host, player: PlayerRef, to: [f64; 3], reach: &str, cfg: &Settings) -> bool {
    let Some(feet) = player.entity().position(host) else {
        return false;
    };
    let living = LivingRef::new(player.entity());
    let stretch = if cfg.scale_distance {
        match (
            living.attribute_value(host, reach),
            living.attribute_base(host, reach),
        ) {
            (Some(now), Some(base)) if base != 0.0 => now / base,
            _ => 1.0,
        }
    } else {
        1.0
    };
    let limit = cfg.max_distance * stretch;
    let d = dist(feet, to);
    d * d < limit * limit
}

/// Whether a player holds the carry key (`carryon:key`, sent by the client).
fn key_held() -> std::sync::MutexGuard<'static, HashSet<u128>> {
    static HELD: OnceLock<Mutex<HashSet<u128>>> = OnceLock::new();
    lock(HELD.get_or_init(Mutex::default))
}

fn air(host: &dyn Host) -> Option<mod_api::BlockState> {
    host.block_state_id("minecraft:air")
}

fn sound_at(
    host: &mut dyn ServerHost,
    dim: &str,
    sound: &str,
    at: [f64; 3],
    source: SoundSource,
    volume: f32,
    pitch: f32,
) {
    if let Some(level) = host.level(dim) {
        level.play_sound(host, sound, at, source, volume, pitch, None);
    }
}

/// What a carry keeps of its script (`CarryOnData.activeScript`): the commands still to run and the render the
/// clients draw by.
fn script_tag(script: &scripts::Script) -> Compound {
    let mut c = Compound::new();
    let effects = script.effects();
    let parts = [
        ("loop", effects.map_or("", |e| e.r#loop.as_str())),
        ("place", effects.map_or("", |e| e.place.as_str())),
        ("render", script.render().unwrap_or("")),
    ];
    for (key, value) in parts {
        if !value.is_empty() {
            c.insert(key.into(), s(value));
        }
    }
    c
}

/// A carry's script command or render, empty when it has none.
fn script_part(carry: &Compound, key: &str) -> String {
    match carry.get(&"script".into()) {
        Some(Tag::Compound(c)) => get_str(c, key).unwrap_or_default(),
        _ => String::new(),
    }
}

fn entity_kind(host: &dyn Host, e: EntityRef) -> String {
    e.kind(host)
        .and_then(|k| k.id(host))
        .map(|n| n.as_str().to_owned())
        .unwrap_or_default()
}

impl Mod {
    fn queue(&self, job: Job) {
        lock(&self.jobs).push(job);
    }

    /// PickupHandler.tryPickUpBlock, the checks the click can make: None when it can't be picked up, else the
    /// script that matched it, if any.
    fn block_pickable(
        host: &mut dyn ServerHost,
        player: PlayerRef,
        dim: &str,
        pos: [i32; 3],
    ) -> Option<Option<Arc<scripts::Script>>> {
        let level = host.level(dim)?;
        let reader: &dyn ServerHost = &*host;
        let cfg = Settings::read(reader);
        let center = pos.map(|c| f64::from(c) + 0.5);
        if !in_reach(
            reader,
            player,
            center,
            "minecraft:block_interaction_range",
            &cfg,
        ) {
            return None;
        }
        let state = level.block_state(reader, pos)?;
        let block = level.block(reader, pos).and_then(|k| k.id(reader))?;
        let be = reader.block_entity(level, pos);
        let script = if cfg.use_scripts {
            let data = be.and_then(|be| {
                let mut data = be.data(reader)?;
                let id = be.kind(reader).and_then(|k| k.id(reader))?;
                data.insert("id".into(), s(id));
                Some(data)
            });
            let states = reader.block_states();
            let found = scripts::inspect_block(
                reader,
                block,
                states.destroy_speed(state).unwrap_or(0.0),
                states.explosion_resistance(state).unwrap_or(0.0),
                data.as_ref(),
            );
            match found {
                Ok(found) => found,
                Err(e) => {
                    eprintln!("[carryon] pickup script: {e}");
                    return None;
                }
            }
        } else {
            None
        };
        let overrides = script.as_ref().is_some_and(|s| s.override_checks);
        let block_key = level.block(reader, pos);
        let tags = reader.tags();
        let in_tag = |tag: &str| {
            let (Some(block), Ok(id)) = (block_key, Identifier::parse(tag)) else {
                return false;
            };
            tags.key(&id).and_then(|key| block.is_in_tag(tags, key)) == Some(true)
        };
        let lists = Lists::get(reader);
        if block == "minecraft:air"
            || !permitted(
                block,
                cfg.whitelist_blocks,
                &lists.allowed_blocks,
                &lists.forbidden_tiles,
                &in_tag,
            )
        {
            return None;
        }
        // DoorBlock.HALF: the halves of tall blocks stay (a trapdoor's `half` is another property).
        if matches!(
            level.block_property(reader, pos, "half").as_deref(),
            Some("upper" | "lower")
        ) {
            return None;
        }
        if !overrides
            && reader
                .block_states()
                .destroy_speed(state)
                .is_some_and(|s| s < 0.0)
            && !creative(reader, player)
            && !cfg.pickup_unbreakable
        {
            return None;
        }
        if be.is_none() && !overrides && !cfg.pickup_all_blocks {
            return None;
        }
        // A locked container stays.
        if be.is_some_and(|be| be.lock(reader).is_some_and(|l| !l.is_empty())) {
            return None;
        }
        match script {
            Some(script) => match script.fulfills(host, player) {
                Ok(true) => Some(Some(script)),
                Ok(false) => None,
                Err(e) => {
                    eprintln!("[carryon] pickup script: {e}");
                    None
                }
            },
            None => Some(None),
        }
    }

    fn pick_block(&self, host: &mut dyn ServerHost, uuid: u128, dim: &str, pos: [i32; 3]) {
        let Some(player) = host.player(uuid) else {
            return;
        };
        if !may_pick(host, player) {
            return;
        }
        let Some(script) = Self::block_pickable(host, player, dim, pos) else {
            return;
        };
        let Some(level) = host.level(dim) else { return };
        let reader: &dyn ServerHost = &*host;
        let Some(state) = level.block_state(reader, pos) else {
            return;
        };
        let Some(text) = reader.block_state_string(state) else {
            return;
        };
        let tile = reader
            .block_entity(level, pos)
            .and_then(|be| be.data(reader));
        let (id, mut props) = parse_state(&text);
        // CarryOnData.setBlock: a powered block is carried unpowered.
        set_prop(&mut props, "powered", "false");
        let water = prop(&props, "waterlogged") == Some("true");
        let sound = reader.block_states().sound(state);
        let Some(left) = (if water {
            reader.block_state_id("minecraft:water")
        } else {
            air(reader)
        }) else {
            return;
        };
        let slot = player.selected_slot(reader).unwrap_or(0) as i32;
        let mut c = Compound::new();
        c.insert("type".into(), s("block"));
        c.insert("block".into(), s(&state_text(&id, &props)));
        if let Some(t) = tile {
            c.insert("tile".into(), Tag::Compound(t));
        }
        c.insert("selected".into(), Tag::Int(slot));
        if let Some(script) = &script {
            c.insert("script".into(), Tag::Compound(script_tag(script)));
            if let Some(effects) = script.effects() {
                scripts::run(host, player, &effects.pickup);
            }
        }
        // The carry is stored before the block goes, so a refused store never loses the block.
        if !set_carry(host, uuid, c) {
            return;
        }
        // The block entity goes with the player: no drops, no container spill.
        let flags = BlockUpdateFlags::NEIGHBORS
            | BlockUpdateFlags::CLIENTS
            | BlockUpdateFlags::SKIP_BLOCK_ENTITY_SIDE_EFFECTS
            | BlockUpdateFlags::SUPPRESS_DROPS;
        match level.set_block(host, pos, left, flags) {
            Ok(true) => {}
            other => {
                eprintln!("[carryon] pickup at {pos:?} refused: {other:?}");
                set_carry(host, uuid, Compound::new());
                return;
            }
        }
        if let Some(snd) = sound {
            sound_at(
                host,
                dim,
                snd.hit.as_str(),
                pos.map(|v| f64::from(v) + 0.5),
                SoundSource::Blocks,
                1.0,
                0.5,
            );
        }
    }

    /// PickupHandler.tryPickupEntity, the checks the click can make: None when it can't be picked up, else the
    /// script that matched it, if any.
    fn entity_pickable(
        host: &mut dyn ServerHost,
        player: PlayerRef,
        e: EntityRef,
    ) -> Option<Option<Arc<scripts::Script>>> {
        let nbt = e.nbt(host);
        let reader: &dyn ServerHost = &*host;
        let cfg = Settings::read(reader);
        let lists = Lists::get(reader);
        let kind = entity_kind(reader, e);
        if kind.is_empty() || kind == "minecraft:player" {
            return None;
        }
        let script = if cfg.use_scripts {
            let mut data = nbt.clone().unwrap_or_default();
            data.insert("id".into(), s(&kind));
            let [width, height] = e.dimensions(reader).unwrap_or([0.0; 2]);
            let found = scripts::inspect_entity(&scripts::EntityFacts {
                kind: &kind,
                width: width as f32,
                height: height as f32,
                health: LivingRef::new(e).health(reader).unwrap_or(0.0),
                nbt: &data,
            });
            match found {
                Ok(found) => found,
                Err(err) => {
                    eprintln!("[carryon] pickup script: {err}");
                    return None;
                }
            }
        } else {
            None
        };
        let overrides = script.as_ref().is_some_and(|s| s.override_checks);
        let baby = nbt
            .as_ref()
            .and_then(|n| get_int(n, "Age"))
            .is_some_and(|age| age < 0);
        let allowed = permitted(
            &kind,
            cfg.whitelist_entities,
            &lists.allowed_entities,
            &lists.forbidden_entities,
            &|tag| reader.entity_type_in_tag(&kind, tag),
        );
        // A baby can be carried though the grown mob is not allowed.
        if !allowed && !overrides && !(cfg.allow_babies && baby) {
            return None;
        }
        let at = e.position(reader)?;
        if !in_reach(
            reader,
            player,
            at,
            "minecraft:entity_interaction_range",
            &cfg,
        ) {
            return None;
        }
        if !creative(reader, player) {
            if !overrides
                && !cfg.pickup_hostile
                && e.classification(reader) == Some(MobCategory::Monster)
            {
                return None;
            }
            let scale = scale_of(reader, player, &cfg);
            if let Some([w, h]) = e.dimensions(reader)
                && (cfg.max_entity_width < w * scale || cfg.max_entity_height < h * scale)
            {
                return None;
            }
        }
        // Someone else's pet stays.
        let owner = nbt.and_then(|n| uuid_of(n.get(&"Owner".into())));
        if owner.is_some_and(|o| o != player.uuid()) {
            return None;
        }
        match script {
            Some(script) => match script.fulfills(host, player) {
                Ok(true) => Some(Some(script)),
                Ok(false) => None,
                Err(err) => {
                    eprintln!("[carryon] pickup script: {err}");
                    None
                }
            },
            None => Some(None),
        }
    }

    fn pick_entity(&self, host: &mut dyn ServerHost, uuid: u128, id: u32) {
        let Some(player) = host.player(uuid) else {
            return;
        };
        let Some(e) = host.entity(id) else { return };
        if !may_pick(host, player) {
            return;
        }
        let Some(script) = Self::entity_pickable(host, player, e) else {
            return;
        };
        let reader: &dyn ServerHost = &*host;
        let kind = entity_kind(reader, e);
        let [w, h] = e.dimensions(reader).unwrap_or([0.6, 0.6]);
        let slot = player.selected_slot(reader).unwrap_or(0) as i32;
        e.detach_leash(host, true);
        e.eject_passengers(host);
        e.stop_riding(host);
        if let Some(effects) = script.as_ref().and_then(|s| s.effects()) {
            scripts::run(host, player, &effects.pickup);
        }
        let Some(mut data) = e.nbt(host) else { return };
        for k in ["Passengers", "leash", "Leash", "Pos", "Motion"] {
            data.remove(&(*k).into());
        }
        let mut c = Compound::new();
        c.insert("type".into(), s("entity"));
        c.insert("kind".into(), s(&kind));
        c.insert("entity".into(), Tag::Compound(data));
        c.insert(
            "size".into(),
            Tag::List(vec![Tag::Double(w.to_bits()), Tag::Double(h.to_bits())]),
        );
        c.insert("selected".into(), Tag::Int(slot));
        if let Some(script) = &script {
            c.insert("script".into(), Tag::Compound(script_tag(script)));
        }
        // The carry is stored before the mob goes, so a refused store never loses the mob.
        if !set_carry(host, uuid, c) {
            return;
        }
        if !e.remove(host, RemovalReason::Discarded) {
            set_carry(host, uuid, Compound::new());
            return;
        }
        if let Some(ent) = host.player(uuid).map(|p| p.entity()) {
            ent.play_sound(host, "minecraft:entity.item.pickup", 1.0, 0.5);
        }
    }

    fn place(
        &self,
        host: &mut dyn ServerHost,
        uuid: u128,
        dim: &str,
        clicked: [i32; 3],
        face: [i32; 3],
    ) {
        let Some(player) = host.player(uuid) else {
            return;
        };
        let carry = carry_of(host, uuid);
        let Some(level) = host.level(dim) else { return };
        let reader: &dyn ServerHost = &*host;
        let replaceable = |pos| {
            level
                .block_state(reader, pos)
                .and_then(|st| reader.block_states().replaceable(st))
                == Some(true)
        };
        let pos = if replaceable(clicked) {
            clicked
        } else {
            add(clicked, face)
        };
        let fail = |host: &mut dyn ServerHost| {
            if let Some(ent) = host.player(uuid).map(|p| p.entity()) {
                ent.play_sound(host, "minecraft:block.note_block.bass", 0.5, 0.5);
            }
        };
        match get_str(&carry, "type").as_deref() {
            Some("block") => {
                if !replaceable(pos) {
                    return fail(host);
                }
                // Level.isUnobstructed: no mob or player in the way.
                let lo = pos.map(f64::from);
                let hi = pos.map(|v| f64::from(v) + 1.0);
                let blocked = level
                    .visit_entities_in(reader, lo, hi, &mut |e| {
                        let k = entity_kind(reader, e);
                        if matches!(
                            k.as_str(),
                            "minecraft:item" | "minecraft:experience_orb" | "minecraft:arrow"
                        ) {
                            ControlFlow::Continue(())
                        } else {
                            ControlFlow::Break(())
                        }
                    })
                    .is_ok_and(|flow| flow.is_break());
                if blocked {
                    return fail(host);
                }
                let [yaw, pitch] = player.entity().rotation(reader).unwrap_or([0.0, 0.0]);
                let in_water = level.block(reader, pos).and_then(|k| k.id(reader))
                    == Some("minecraft:water")
                    && level.block_property(reader, pos, "level").as_deref() == Some("0");
                let carried = get_str(&carry, "block").unwrap_or_default();
                let lists = Lists::get(reader);
                let (carried_id, _) = parse_state(&carried);
                let block_key = reader.block_key(&carried_id);
                let sneaking = player.entity().is_shift_key_down(reader) == Some(true);
                let mut context = PlacementContext::new(
                    BlockPos::new(pos[0], pos[1], pos[2]),
                    direction_of(face),
                    None,
                );
                context.source = PlacementSource::Player {
                    yaw,
                    pitch,
                    sneaking,
                };
                let vanilla = block_key
                    .and_then(|key| {
                        level
                            .state_for_placement(host, key, &context)
                            .ok()
                            .flatten()
                    })
                    .and_then(|state| host.block_state_string(state));
                let reader: &dyn ServerHost = &*host;
                let text = match vanilla {
                    Some(placed) => merge_placement(&carried, &placed, &|block, property| {
                        lists.recomputes(block, property)
                    }),
                    None => placement_state(
                        reader,
                        &carried,
                        &Placement {
                            face,
                            yaw,
                            pitch,
                            in_water,
                        },
                    ),
                };
                let Some(state) = reader
                    .block_state_id(&text)
                    .or_else(|| reader.block_state_id(&carried))
                else {
                    eprintln!("[carryon] unknown carried state {carried}");
                    return fail(host);
                };
                let sound = reader.block_states().sound(state);
                scripts::run(host, player, &script_part(&carry, "place"));
                let flags = BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS;
                if !matches!(level.set_block(host, pos, state, flags), Ok(true)) {
                    return fail(host);
                }
                if let Some(Tag::Compound(tile)) = carry.get(&"tile".into()) {
                    match host.block_entity(level, pos) {
                        Some(be) => {
                            if let Err(e) = be.replace_data(host, tile.clone()) {
                                eprintln!("[carryon] block entity data at {pos:?} refused: {e:?}");
                            }
                        }
                        None => {
                            eprintln!("[carryon] no block entity at {pos:?} for the carried data")
                        }
                    }
                }
                set_carry(host, uuid, Compound::new());
                if let Some(snd) = sound {
                    sound_at(
                        host,
                        dim,
                        snd.place.as_str(),
                        pos.map(|v| f64::from(v) + 0.5),
                        SoundSource::Blocks,
                        1.0,
                        0.5,
                    );
                }
            }
            Some("entity") => {
                let kind = get_str(&carry, "kind").unwrap_or_default();
                let data = match carry.get(&"entity".into()) {
                    Some(Tag::Compound(c)) => c.clone(),
                    _ => Compound::new(),
                };
                let at = [
                    f64::from(pos[0]) + 0.5,
                    f64::from(pos[1]),
                    f64::from(pos[2]) + 0.5,
                ];
                scripts::run(host, player, &script_part(&carry, "place"));
                let summoned = host
                    .level(dim)
                    .map(|level| level.summon(host, &kind, at, data));
                if !matches!(summoned, Some(Ok(Some(_)))) {
                    eprintln!("[carryon] {kind} not summoned at {at:?}");
                    return fail(host);
                }
                set_carry(host, uuid, Compound::new());
            }
            _ => {}
        }
    }

    /// PlacementHandler.tryStackEntity: the carried mob rides the top of the clicked mob's stack.
    fn stack(&self, host: &mut dyn ServerHost, uuid: u128, target: u32) {
        let cfg = Settings::read(&*host);
        if !cfg.stackable_entities {
            return;
        }
        let carry = carry_of(host, uuid);
        if get_str(&carry, "type").as_deref() != Some("entity") {
            return;
        }
        let Some(player) = host.player(uuid) else {
            return;
        };
        let Some(clicked) = host.entity(target) else {
            return;
        };
        let reader: &dyn ServerHost = &*host;
        let mut top = clicked.root_vehicle(reader).unwrap_or(clicked);
        let mut count = 0;
        loop {
            let mut first = None;
            let _ = top.visit_passengers(reader, &mut |p| {
                first = Some(p);
                ControlFlow::Break(())
            });
            let Some(next) = first else {
                break;
            };
            top = next;
            count += 1;
        }
        let fail = |host: &mut dyn ServerHost| {
            if let Some(ent) = host.player(uuid).map(|p| p.entity()) {
                ent.play_sound(host, "minecraft:block.note_block.bass", 0.5, 0.5);
            }
        };
        let lists = Lists::get(reader);
        let top_kind = entity_kind(reader, top);
        if count + 1 >= cfg.max_stack
            || !permitted(
                &top_kind,
                cfg.whitelist_stacking,
                &lists.allowed_stacking,
                &lists.forbidden_stacking,
                &|tag| reader.entity_type_in_tag(&top_kind, tag),
            )
        {
            return fail(host);
        }
        let size = match carry.get(&"size".into()) {
            Some(Tag::List(v)) => v
                .iter()
                .filter_map(|t| {
                    if let Tag::Double(d) = t {
                        Some(f64::from_bits(*d))
                    } else {
                        None
                    }
                })
                .product::<f64>(),
            _ => 0.36,
        };
        let top_size = top.dimensions(reader).map_or(0.0, |[w, h]| w * h);
        if cfg.size_matters_stacking && size > top_size {
            return fail(host);
        }
        let Some(at) = top.position(reader) else {
            return;
        };
        let Some(dim) = player.dimension(reader).map(|d| d.to_string()) else {
            return;
        };
        let kind = get_str(&carry, "kind").unwrap_or_default();
        let data = match carry.get(&"entity".into()) {
            Some(Tag::Compound(c)) => c.clone(),
            _ => Compound::new(),
        };
        let at = [at[0], at[1] + 2.6, at[2]];
        let Some(Ok(Some(e))) = host
            .level(&dim)
            .map(|level| level.summon(host, &kind, at, data))
        else {
            return fail(host);
        };
        let _ = e.start_riding(host, top, true);
        scripts::run(host, player, &script_part(&carry, "place"));
        set_carry(host, uuid, Compound::new());
        if let Some(ent) = host.player(uuid).map(|p| p.entity()) {
            ent.play_sound(host, "minecraft:entity.horse.saddle", 0.5, 1.5);
        }
    }

    /// PlacementHandler.placeCarried at death: the block at the nearest free spot (the jar looks 15 blocks
    /// around), the mob where the player died.
    fn drop_carried(&self, host: &mut dyn ServerHost, dim: &str, at: [f64; 3], carry: &Compound) {
        let Some(level) = host.level(dim) else { return };
        match get_str(carry, "type").as_deref() {
            Some("block") => {
                let reader: &dyn ServerHost = &*host;
                let base = at.map(|v| v.floor() as i32);
                let mut spots = Vec::new();
                for dx in -15..=15 {
                    for dy in -15..=15 {
                        for dz in -15..=15 {
                            spots.push([base[0] + dx, base[1] + dy, base[2] + dz]);
                        }
                    }
                }
                spots.sort_by_key(|p| {
                    (p[0] - base[0]).pow(2) + (p[1] - base[1]).pow(2) + (p[2] - base[2]).pow(2)
                });
                let free = spots.into_iter().find(|p| {
                    level
                        .block_state(reader, *p)
                        .and_then(|st| reader.block_states().replaceable(st))
                        == Some(true)
                });
                let Some(pos) = free else {
                    eprintln!("[carryon] no free spot near {at:?}");
                    return;
                };
                let carried = get_str(carry, "block").unwrap_or_default();
                let Some(state) = reader.block_state_id(&carried) else {
                    eprintln!("[carryon] unknown carried state {carried}");
                    return;
                };
                let flags = BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS;
                let r = level.set_block(host, pos, state, flags);
                if !matches!(r, Ok(true)) {
                    eprintln!("[carryon] {carried} not set down at {pos:?}: {r:?}");
                    return;
                }
                if let (Some(Tag::Compound(tile)), Some(be)) =
                    (carry.get(&"tile".into()), host.block_entity(level, pos))
                {
                    let _ = be.replace_data(host, tile.clone());
                }
            }
            Some("entity") => {
                let kind = get_str(carry, "kind").unwrap_or_default();
                if let Some(Tag::Compound(data)) = carry.get(&"entity".into()) {
                    if let Some(level) = host.level(dim) {
                        let _ = level.summon(host, &kind, at, data.clone());
                    }
                }
            }
            _ => {}
        }
    }

    fn run(&self, host: &mut dyn ServerHost, job: Job) {
        match job {
            Job::PickBlock { uuid, dim, pos } => self.pick_block(host, uuid, &dim, pos),
            Job::PickEntity { uuid, entity } => self.pick_entity(host, uuid, entity),
            Job::Place {
                uuid,
                dim,
                pos,
                face,
            } => self.place(host, uuid, &dim, pos, face),
            Job::Stack { uuid, target } => self.stack(host, uuid, target),
            Job::Drop { dim, at, carry } => self.drop_carried(host, &dim, at, &carry),
        }
    }

    /// CarryOnCommon.onCarryTick for every carrying player, and the list the clients draw from.
    fn carry_tick(&self, host: &mut dyn ServerHost, send: bool) {
        let slowness = host.effect_key("minecraft:slowness");
        let mut bytes = Vec::new();
        for player in host.players().server_players() {
            let uuid = player.uuid();
            let carry = carry_of(host, uuid);
            if carry.is_empty() {
                continue;
            }
            let kind = get_str(&carry, "type").unwrap_or_default();
            scripts::run(host, player, &script_part(&carry, "loop"));
            let reader: &dyn ServerHost = &*host;
            // Slowness: a mob by its area, a block by the size of its data (a level per 500 characters
            // of its NBT text, here its network size), up to IV; 1 tick, renewed each tick.
            let cfg = Settings::read(reader);
            let level = if kind == "entity" {
                let size = match carry.get(&"size".into()) {
                    Some(Tag::List(v)) => v
                        .iter()
                        .filter_map(|t| {
                            if let Tag::Double(d) = t {
                                Some(f64::from_bits(*d))
                            } else {
                                None
                            }
                        })
                        .product::<f64>(),
                    _ => 1.0,
                };
                let scale = scale_of(reader, player, &cfg);
                config::slowness_level(false, 0, size * scale * scale, &cfg)
            } else {
                let len = Tag::Compound(carry.clone())
                    .to_network_bytes()
                    .map_or(0, |b| b.len());
                config::slowness_level(true, len, 0.0, &cfg)
            };
            if let Some(id) = slowness
                && (cfg.slowness_in_creative || !creative(reader, player))
            {
                let effect = ActiveEffect::new(id, 3, level, false, false, false);
                LivingRef::new(player.entity()).add_effect(host, &effect);
            }
            // The hotbar slot stays where it was picked up, and stays empty.
            let selected = get_int(&carry, "selected").unwrap_or(0).clamp(0, 8) as usize;
            if player.selected_slot(&*host) != Some(selected) {
                player.set_selected_slot(host, selected);
            }
            let held = LivingRef::new(player.entity())
                .with_hand_item(&*host, Hand::Main, |s| s.project())
                .flatten();
            if let Some(stack) = held {
                // With the inventory unreadable no slot can be trusted to be free, so the held stack is dropped, not
                // written over one.
                let free = player
                    .with_inventory(&*host, |inventory| {
                        let used: Vec<u32> = inventory.iter().map(|(i, _)| i).collect();
                        (0..36u32).find(|i| *i as usize != selected && !used.contains(i))
                    })
                    .flatten();
                let _ = player.set_inventory_slot(host, selected as u32, None);
                match free {
                    Some(i) => {
                        let _ = player.set_inventory_slot(host, i, Some(stack));
                    }
                    None => {
                        let _ = player.entity().spawn_at_location(host, stack, 0.5);
                    }
                }
            }
            if send {
                let id = player.entity().id();
                let (k, body) = if kind == "block" {
                    (
                        0u8,
                        get_str(&carry, "block").unwrap_or_default().into_bytes(),
                    )
                } else {
                    let mut b = get_str(&carry, "kind").unwrap_or_default().into_bytes();
                    b.push(0);
                    let data = match carry.get(&"entity".into()) {
                        Some(Tag::Compound(c)) => c.clone(),
                        _ => Compound::new(),
                    };
                    b.extend(Tag::Compound(data).to_network_bytes().unwrap_or_default());
                    (1u8, b)
                };
                bytes.extend(id.to_be_bytes());
                bytes.push(k);
                bytes.extend((body.len() as u32).to_be_bytes());
                bytes.extend(body);
                let render = script_part(&carry, "render").into_bytes();
                bytes.extend((render.len() as u32).to_be_bytes());
                bytes.extend(render);
            }
        }
        if send {
            let mut any = lock(&self.sent_any);
            if bytes.is_empty() && !*any {
                return;
            }
            *any = !bytes.is_empty();
            drop(any);
            let channel = PayloadId(CHANNEL.into());
            for player in host.players().server_players() {
                let _ = player.send_payload(host, &channel, PayloadBytes(bytes.clone()));
            }
        }
    }
}

/// The player a command came from.
fn source_player(source: &CommandSource) -> Option<PlayerRef> {
    match source {
        CommandSource::Player { player, .. } => Some(*player),
        _ => None,
    }
}

/// `/carryon debug`'s lines for what is carried.
fn describe(carry: &Compound) -> Vec<String> {
    match get_str(carry, "type").as_deref() {
        Some("block") => {
            let mut lines = vec![format!(
                "BlockState: {}",
                get_str(carry, "block").unwrap_or_default()
            )];
            if let Some(Tag::Compound(tile)) = carry.get(&"tile".into()) {
                lines.push(format!("Block entity data: {} entries", tile.len()));
            }
            lines
        }
        Some("entity") => vec![format!(
            "Entity: {}",
            get_str(carry, "kind").unwrap_or_default()
        )],
        _ => Vec::new(),
    }
}

/// The client's own player's network id.
fn local_id(host: &dyn ClientHost) -> Option<u32> {
    host.local_player().map(|p| p.entity().id())
}

fn local_shown(host: &dyn ClientHost) -> Option<Carried> {
    let id = local_id(host)?;
    lock(shown()).get(&id).cloned()
}

fn block_props(props: &[(String, String)], facing: &str) -> Vec<(String, String)> {
    let mut p = props.to_vec();
    for k in ["facing", "horizontal_facing"] {
        if let Some(cur) = prop(&p, k)
            && !matches!(cur, "up" | "down")
        {
            set_prop(&mut p, k, facing);
        }
    }
    p
}

/// CarryRenderHelper.renderBakedModel draws the block's item (a chest's block model is empty: its item has the
/// chest renderer); a block with no item is drawn as its block model, its front to +z.
fn draw_block(
    host: &dyn Host,
    draw: &mut dyn mod_api::BlockEntityDrawContext,
    id: &str,
    props: &[(String, String)],
) {
    let Some(key) = host.block_key(id) else {
        return;
    };
    let item = host
        .block_item(key)
        .filter(|k| k.id(host) != Some("minecraft:air"))
        .and_then(|k| ItemStack::new(k, 1));
    if let Some(stack) = item {
        draw.push();
        draw.translate([0.5, 0.5, 0.5]);
        let drawn = draw.item(&stack, ItemDisplayContext::None, None, 0);
        draw.pop();
        if drawn {
            return;
        }
    }
    let state = host
        .block_state_id(&state_text(id, props))
        .or_else(|| host.block_state_default(key));
    if let Some(state) = state {
        draw.block(state);
    }
}

/// `CarryRenderHelper.performScriptTransformation`: the script's rotation (x, then y, then z), translation and
/// scale, about the centre of what is drawn. In third person from behind a block's z translation is turned
/// round, as the original does.
fn script_transform(
    host: &dyn ClientHost,
    draw: &mut dyn mod_api::BlockEntityDrawContext,
    render: &scripts::Render,
    centre: [f32; 3],
    block: bool,
) {
    draw.translate(centre);
    let [rx, ry, rz] = render.rotation.or(0.0);
    for (axis, degrees) in [
        ([1.0, 0.0, 0.0], rx),
        ([0.0, 1.0, 0.0], ry),
        ([0.0, 0.0, 1.0], rz),
    ] {
        draw.rotate_about(axis, degrees, [0.0; 3]);
    }
    let [x, y, z] = render.translation.or(0.0);
    let behind = block && host.camera_type() == CameraType::ThirdPersonBack;
    draw.translate([x, y, if behind { -z } else { z }]);
    draw.scale(render.scale.or(1.0));
    draw.translate(centre.map(|c| -c));
}

/// The block a script draws instead of the carried one (its default state), if it names one.
fn render_block(render: Option<&scripts::Render>) -> Option<String> {
    render.and_then(|r| r.name_block.clone())
}

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        let descriptor = AttachmentDescriptor::new("carryon:carry", Compound::new)
            .serialize(AttachmentCodec {
                encode: |c| Ok(c.clone()),
                decode: |c| Ok(c.clone()),
            })
            .copy_on_death();
        match r.register_attachment_type(descriptor) {
            Ok(key) => *CARRY.write().unwrap_or_else(|e| e.into_inner()) = Some(key),
            Err(e) => eprintln!("[carryon] carry attachment not registered: {e}"),
        }
        config::register_settings(r);
        r.register_key_mapping(
            KeyRegistration::new("carry", "key.keyboard.left.shift").label("key.carry.desc"),
        );
        let mut payloads = r.payload_handlers();
        let mut channel = payloads.registrar("2").optional();
        channel.play_to_client(PayloadId(CHANNEL.into()), Some(Arc::new(CarriedHandler)));
        channel.play_to_server(PayloadId(KEY_CHANNEL.into()), Arc::new(KeyHandler));
    }

    /// `ScriptReloadListener`: the pickup scripts in `data/<namespace>/carryon/scripts/`, read on every data
    /// load.
    fn add_server_reload_listeners(
        &self,
        _: &dyn ReloadContext,
        event: &mut AddServerReloadListenersEvent<'_>,
    ) {
        let added = Identifier::parse("carryon:scripts")
            .map_err(|e| e.to_string())
            .and_then(|id| {
                let files =
                    FileToIdConverter::json("carryon/scripts").map_err(|e| e.to_string())?;
                event
                    .listeners
                    .add_listener(id, files, Arc::new(scripts::Listener))
                    .map_err(|e| e.to_string())
            });
        if let Err(e) = added {
            eprintln!("[carryon] pickup scripts not loaded: {e}");
        }
    }

    /// `CommandCarryOn`: `/carryon debug`, and `clear` and `place` for yourself or (operators)
    /// for chosen players.
    fn register_commands(&self, _: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        event
            .commands
            .push(Command::new("carryon debug").executes(|host, source, _| {
                let Some(uuid) = source_player(source).map(|p| p.uuid()) else {
                    return Err(Box::new(Text::literal("Only a player carries.")));
                };
                let carry = carry_of(host, uuid);
                let lines = describe(&carry);
                for line in &lines {
                    let _ = host.commands().feedback(Text::literal(line.clone()), false);
                }
                Ok(i32::from(!lines.is_empty()))
            }));
        for (usage, level, place) in [
            ("carryon clear", 0, false),
            ("carryon clear <target:players>", 2, false),
            ("carryon place", 2, true),
            ("carryon place <target:players>", 2, true),
        ] {
            event.commands.push(
                Command::op(usage, level).executes(move |host, source, call| {
                    let targets = match call.argument("target") {
                        Some(CommandArgumentValue::Players(list)) => list.clone(),
                        _ => source_player(source).into_iter().collect(),
                    };
                    let mut done = 0;
                    for player in targets {
                        let carry = carry_of(host, player.uuid());
                        if carry.is_empty() {
                            continue;
                        }
                        let reader: &dyn ServerHost = &*host;
                        let spot = (player.entity().position(reader), player.dimension(reader));
                        set_carry(host, player.uuid(), Compound::new());
                        if let (true, (Some(at), Some(dim))) = (place, spot) {
                            Mod::default().drop_carried(host, &dim.to_string(), at, &carry);
                        }
                        done += 1;
                    }
                    Ok(done)
                }),
            );
        }
    }

    /// The carry key's state, for the server (`CarryOnKeybinds.onCarryKey`): sent when it changes.
    fn client_tick_post(&self, host: &mut dyn ClientHost, _: &mut ClientTickPostEvent) {
        let down = host
            .mod_key_mapping(config::MOD_ID, "carry")
            .ok()
            .and_then(|key| key.is_down(&*host).ok())
            .unwrap_or(false);
        if self.key_down.swap(down, Ordering::Relaxed) != down {
            let _ = host.send_payload_to_server(
                &PayloadId(KEY_CHANNEL.into()),
                PayloadBytes(vec![u8::from(down)]),
            );
        }
    }

    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        if !matches!(event.phase, ServerTickPhase::ServerTickEnd { .. }) {
            return;
        }
        let tick = self.ticks.fetch_add(1, Ordering::Relaxed);
        let hit = std::mem::take(&mut *lock(&self.hit));
        if !hit.is_empty() && Settings::read(&*host).drop_when_hit {
            for id in hit {
                let Some(e) = host.entity(id) else { continue };
                let Some(uuid) = e.uuid(host) else { continue };
                let carry = carry_of(host, uuid);
                let reader: &dyn ServerHost = &*host;
                let (Some(at), Some(dim)) = (
                    e.position(reader),
                    host.player(uuid).and_then(|p| p.dimension(reader)),
                ) else {
                    continue;
                };
                if !carry.is_empty() {
                    set_carry(host, uuid, Compound::new());
                    self.queue(Job::Drop {
                        dim: dim.to_string(),
                        at,
                        carry,
                    });
                }
            }
        }
        let jobs = std::mem::take(&mut *lock(&self.jobs));
        let changed = !jobs.is_empty();
        for job in jobs {
            self.run(host, job);
        }
        self.carry_tick(host, changed || tick.is_multiple_of(10));
    }

    /// The UseBlockCallback: pick up, or set down what is carried.
    fn right_click_block(&self, host: HostSide<'_>, event: &mut RightClickBlockEvent<'_>) -> Flow {
        let click = event.click;
        match host {
            HostSide::Server(host) => {
                let Some(player) = host.player(click.player) else {
                    return Flow::Continue;
                };
                let dim = click.dimension.key().id().to_string();
                if carrying(host, click.player) {
                    if matches!(click.hand, Hand::Main) {
                        let mut jobs = lock(&self.jobs);
                        if !jobs
                            .iter()
                            .any(|j| matches!(j, Job::Place { uuid, .. } if *uuid == click.player))
                        {
                            jobs.push(Job::Place {
                                uuid: click.player,
                                dim,
                                pos: click.pos,
                                face: click.hit.face,
                            });
                        }
                    }
                } else if may_pick(host, player)
                    && Self::block_pickable(host, player, &dim, click.pos).is_some()
                {
                    self.queue(Job::PickBlock {
                        uuid: click.player,
                        dim,
                        pos: click.pos,
                    });
                } else {
                    return Flow::Continue;
                }
            }
            HostSide::Client(host) => {
                // The window's own player: while carrying, a click only sets down (no block use predicted).
                if local_shown(&*host).is_none() {
                    return Flow::Continue;
                }
            }
        }
        *event.cancellation_result = InteractionResult::Success;
        event.cancelled = true;
        Flow::Cancel
    }

    /// The UseEntityCallback: pick up a mob, or stack the carried one on it.
    fn entity_interact(
        &self,
        host: HostSide<'_>,
        event: &mut EntityInteractEvent<'_>,
    ) -> Option<InteractionResult> {
        let HostSide::Server(host) = host else {
            return None;
        };
        let player = host.players().acting_player()?;
        let uuid = player.uuid();
        let id = u32::try_from(event.entity).ok()?;
        let carry = carry_of(host, uuid);
        if !carry.is_empty() {
            if get_str(&carry, "type").as_deref() == Some("entity") {
                self.queue(Job::Stack { uuid, target: id });
                return Some(InteractionResult::Success);
            }
            return None;
        }
        if !matches!(event.hand, Hand::Main) || !may_pick(host, player) {
            return None;
        }
        let e = host.entity(id)?;
        Self::entity_pickable(host, player, e)?;
        let mut jobs = lock(&self.jobs);
        if !jobs
            .iter()
            .any(|j| matches!(j, Job::PickEntity { uuid: u, .. } if *u == uuid))
        {
            jobs.push(Job::PickEntity { uuid, entity: id });
        }
        Some(InteractionResult::Success)
    }

    /// `hitWhileCarrying` false: no mining while carrying.
    fn left_click_block(
        &self,
        host: &mut dyn ServerHost,
        _event: &mut LeftClickBlockEvent,
    ) -> Flow {
        match host.players().acting_player() {
            Some(p) if !Settings::read(&*host).hit_while_carrying && carrying(host, p.uuid()) => {
                Flow::Cancel
            }
            _ => Flow::Continue,
        }
    }

    fn break_block(&self, host: &mut dyn ServerHost, _event: &mut BreakBlockEvent<'_>) -> Flow {
        match host.players().acting_player() {
            Some(p) if !Settings::read(&*host).hit_while_carrying && carrying(host, p.uuid()) => {
                Flow::Cancel
            }
            _ => Flow::Continue,
        }
    }

    fn attack_entity(&self, host: &mut dyn ServerHost, _event: &mut AttackEntityEvent<'_>) -> Flow {
        match host.players().acting_player() {
            Some(p) if !Settings::read(&*host).hit_while_carrying && carrying(host, p.uuid()) => {
                Flow::Cancel
            }
            _ => Flow::Continue,
        }
    }

    /// `PlayerMixinFabric.onHurt`: a hit player puts down what they carry (when the option is on).
    fn living_incoming_damage(
        &self,
        _: &mut dyn HitHost,
        event: &mut LivingIncomingDamageEvent<'_>,
    ) -> Flow {
        if event.kind == "minecraft:player"
            && let Ok(id) = u32::try_from(event.entity)
        {
            lock(&self.hit).push(id);
        }
        Flow::Continue
    }

    /// InventoryMixin / onCarryTick: the hotbar slot can't change while carrying.
    fn player_switch_hotbar_slot_pre(
        &self,
        host: &mut dyn ServerHost,
        event: &mut PlayerSwitchHotbarSlotPreEvent,
    ) -> Flow {
        if carrying(host, event.player) {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    fn mouse_scrolling(&self, host: &mut dyn ClientHost, _event: &mut MouseScrollingEvent) -> Flow {
        if local_shown(&*host).is_some() {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    /// placeCarriedOnDeath: kept with keepInventory (the attachment is copied to the respawned player), else set
    /// down where the player died.
    fn living_death(&self, host: &mut dyn ServerHost, event: &mut LivingDeathEvent<'_>) -> Flow {
        if event.kind != "minecraft:player" {
            return Flow::Continue;
        }
        let Some(e) = host.entity(event.entity) else {
            return Flow::Continue;
        };
        let Some(uuid) = e.uuid(host) else {
            eprintln!("[carryon] dead player {} has no uuid", event.entity);
            return Flow::Continue;
        };
        let carry = carry_of(host, uuid);
        if carry.is_empty() {
            return Flow::Continue;
        }
        let reader: &dyn ServerHost = &*host;
        let Some(level) = e.level(reader) else {
            return Flow::Continue;
        };
        let keep = matches!(
            level.game_rule(reader, "minecraft:keep_inventory"),
            Ok(Some(mod_api::GameRuleValue::Bool(true)))
        );
        if keep {
            return Flow::Continue;
        }
        let (Some(at), Some(dim)) = (
            e.position(reader),
            host.player(uuid).and_then(|p| p.dimension(reader)),
        ) else {
            eprintln!("[carryon] dead player {uuid:x}: no position or dimension");
            return Flow::Continue;
        };
        set_carry(host, uuid, Compound::new());
        self.queue(Job::Drop {
            dim: dim.to_string(),
            at,
            carry,
        });
        Flow::Continue
    }

    /// CarriedObjectRender.drawFirstPerson: the carried block in both hands, in place of the hands.
    fn render_hand(&self, host: &dyn ClientHost, event: &mut RenderHandEvent<'_, '_>) -> Flow {
        let Some(Carried { what, render }) = lock(shown())
            .get(&(event.render.player_entity_id as u32))
            .cloned()
        else {
            return Flow::Continue;
        };
        if matches!(event.render.hand, Hand::Main)
            && let Shown::Block { id, props } = what
        {
            let draw = &mut *event.draw;
            draw.push();
            // Low in front, the front turned to the player (facePlayer false draws a chest's lid side out,
            // the rest facing in), tipped a little as the jar does.
            let size = 0.85;
            draw.translate([-size / 2.0, -1.12, -1.7]);
            draw.scale([size, size, size]);
            draw.rotate_about([1.0, 0.0, 0.0], 8.0, [0.5, 0.5, 0.5]);
            if let Some(r) = &render {
                script_transform(host, draw, r, [0.5; 3], true);
            }
            match render_block(render.as_ref()) {
                Some(name) => draw_block(host, draw, &name, &[]),
                None => {
                    let props = if config::flag(host, config::key::FACE_PLAYER, true) {
                        block_props(&props, "south")
                    } else {
                        props
                    };
                    draw_block(host, draw, &id, &props);
                }
            }
            draw.pop();
        }
        // A script that hides the main arm's hand leaves the hands drawn.
        if let Some(r) = &render {
            let main_left =
                host.local_player().and_then(|p| p.entity().main_arm(host)) == Some(MainHand::Left);
            if (main_left && !r.left_arm) || (!main_left && !r.right_arm) {
                return Flow::Continue;
            }
        }
        event.cancelled = true;
        Flow::Cancel
    }

    /// CarriedObjectRender.drawThirdPerson: the carried block in front of a player's chest.
    fn render_player_post(&self, host: &dyn ClientHost, event: &mut RenderPlayerPostEvent<'_>) {
        let Some(Carried { what, render }) = lock(shown()).get(&event.player.id).cloned() else {
            return;
        };
        let draw = &mut *event.draw;
        draw.push();
        match what {
            Shown::Block { id, props } => {
                let size = 0.7;
                draw.translate([-size / 2.0, 0.75, 0.3]);
                draw.scale([size, size, size]);
                if let Some(r) = &render {
                    script_transform(host, draw, r, [0.5; 3], true);
                }
                match render_block(render.as_ref()) {
                    Some(name) => draw_block(host, draw, &name, &[]),
                    None => {
                        let props = if config::flag(host, config::key::FACE_PLAYER, true) {
                            block_props(&props, "south")
                        } else {
                            props
                        };
                        draw_block(host, draw, &id, &props);
                    }
                }
            }
            Shown::Entity { kind, .. } => {
                let kind = render
                    .as_ref()
                    .and_then(|r| r.name_entity.clone())
                    .unwrap_or(kind);
                if let Some(stack) = spawn_eggs(host)
                    .get(&kind)
                    .and_then(|k| ItemStack::new(*k, 1))
                {
                    draw.translate([0.0, 1.25, 0.45]);
                    draw.scale([0.6, 0.6, 0.6]);
                    if let Some(r) = &render {
                        script_transform(host, draw, r, [0.0; 3], false);
                    }
                    draw.item(&stack, ItemDisplayContext::Fixed, None, 0);
                }
            }
        }
        draw.pop();
    }

    /// CarriedObjectRender.drawFirstPersonEntity: the carried mob over the hotbar.
    fn render_gui_post(&self, host: &dyn ClientHost, event: &mut RenderGuiPostEvent<'_>) {
        if host.hud_mode() != HudMode::AllVisible || !host.camera_type().is_first_person() {
            return;
        }
        let Some(Carried {
            what: Shown::Entity { kind, data },
            render,
        }) = local_shown(host)
        else {
            return;
        };
        // CarryRenderHelper.getRenderEntity: a script may draw another mob, and loads its data over the mob's.
        let (kind, data) = match render {
            Some(scripts::Render {
                name_entity: Some(name),
                nbt,
                ..
            }) => (name, nbt.unwrap_or_default()),
            Some(scripts::Render { nbt: Some(nbt), .. }) => {
                let mut data = data;
                for (k, v) in nbt.iter() {
                    data.insert(k.clone(), v.clone());
                }
                (kind, data)
            }
            _ => (kind, data),
        };
        let [w, h] = event.draw.size();
        let rect = [w / 2.0 - 45.0, h - 125.0, w / 2.0 + 45.0, h - 30.0];
        let view = GuiEntityView::new(
            rect,
            30.0,
            0.0625,
            GuiEntityLook::Turned {
                yaw: 180.0,
                pitch: 0.0,
            },
        );
        let mut data = data;
        data.insert("id".into(), s(&kind));
        if event
            .draw
            .entity(&GuiEntity::with_data(kind.clone(), data), view)
            .is_err()
        {
            let _ = event.draw.entity(&GuiEntity::kind(kind), view);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_placement_decides_directions_axes_and_listed_properties() {
        let none = |_: &str, _: &str| false;
        let merged = merge_placement(
            "minecraft:barrel[facing=north,open=true]",
            "minecraft:barrel[facing=up,open=false]",
            &none,
        );
        assert_eq!(merged, "minecraft:barrel[facing=up,open=true]");
        let listed = |b: &str, p: &str| b == "minecraft:chest" && p == "type";
        assert_eq!(
            merge_placement(
                "minecraft:chest[facing=east,type=left,waterlogged=false]",
                "minecraft:chest[facing=south,type=single,waterlogged=true]",
                &listed
            ),
            "minecraft:chest[facing=south,type=single,waterlogged=true]"
        );
        assert_eq!(
            merge_placement("minecraft:chest[type=left]", "minecraft:stone", &listed),
            "minecraft:chest[type=left]"
        );
    }

    #[test]
    fn facing() {
        assert_eq!(horizontal(0.0), "south");
        assert_eq!(opposite(horizontal(0.0)), "north");
        assert_eq!(horizontal(90.0), "west");
    }
}
