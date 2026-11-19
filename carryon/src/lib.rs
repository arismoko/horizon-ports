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
//! The values are Homestead's `config/carryon-common.json`. Not ported: picking up players (a player can't be
//! a vehicle on this engine), the raised arms (the arm pose hooks aren't asked for an empty hand), custom
//! pickup scripts and conditions (off in Homestead), the model overrides.
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use mod_api::{
    ActiveEffect, AttachmentCodec, HostBlockStates, AttachmentType, AttackEntityEvent, BlockUpdateFlags, BreakBlockEvent, ClientHost,
    Compound, EntityInteractEvent, EntityRef, Flow, GameMode, GuiEntity, GuiEntityLook, GuiEntityView,
    GuiRenderPostEvent, Hand, Holder, Host, HostSide, Hooks, IPayloadContext, IPayloadHandler, InteractionResult,
    ItemDisplayContext, ItemKey, ItemStack, LeftClickBlockEvent, LivingDeathEvent, LivingRef, MobCategory,
    MouseScrollingEvent, PayloadBytes, PayloadHandleError, PayloadId, PlayerRef, PlayerSwitchHotbarSlotPreEvent,
    Registrar, RegistrationContext, RemovalReason, RenderHandEvent, RenderPlayerPostEvent, RightClickBlockEvent,
    ServerHost, ServerTickEvent, ServerTickPhase, SoundSource, Tag,
};

const CHANNEL: &str = "carryon:carried";

// Homestead's config/carryon-common.json.
const MAX_DISTANCE: f64 = 2.5;
const MAX_ENTITY_WIDTH: f64 = 1.5;
const MAX_ENTITY_HEIGHT: f64 = 2.5;
const BLOCK_SLOWNESS: f64 = 1.0;
const ENTITY_SLOWNESS: f64 = 1.0;
const MAX_ENTITY_STACK: usize = 10;
const SLOWNESS_IN_CREATIVE: bool = true;
const PICKUP_HOSTILE: bool = false;
const PICKUP_UNBREAKABLE: bool = false;
const STACKABLE_ENTITIES: bool = true;
const SIZE_MATTERS_STACKING: bool = true;
const FORBIDDEN_TILES: &[&str] = &[
    "vinery:*_wine_rack_small", "vinery:*_wine_rack_mid", "vinery:*_wine_rack_big", "minecraft:end_portal",
    "minecraft:piston_head", "minecraft:end_gateway", "minecraft:tall_grass", "minecraft:large_fern",
    "minecraft:peony", "minecraft:rose_bush", "minecraft:lilac", "minecraft:sunflower", "minecraft:*_bed",
    "minecraft:*_door", "minecraft:big_dripleaf_stem", "minecraft:waterlily", "minecraft:cake",
    "minecraft:nether_portal", "minecraft:tall_seagrass", "colossalchests:*", "create:*", "ftbquests:*",
    "waystones:*", "framedblocks:*", "copycats:*", "spudaciousshops:*", "connectiblechains:*", "hexalia:*",
    "sophisticatedstorage:*", "sophisticatedbackpacks:*", "sophisticatedcore:*", "brewery:*_brewingstation",
    "brewery:brew_kettle", "brewery:brew_oven", "brewery:brew_timer", "brewery:brew_whistle",
    "cookingforblockheads:*", "advancementtrophies:trophy", "storagedrawers:framingtable", "refinedstorage:*",
    "enderstorage:*", "securitycraft:*",
];
const FORBIDDEN_ENTITIES: &[&str] = &[
    "minecraft:end_crystal", "minecraft:ender_dragon", "minecraft:ghast", "minecraft:shulker",
    "minecraft:leash_knot", "minecraft:armor_stand", "minecraft:item_frame", "minecraft:painting",
    "minecraft:shulker_bullet", "securitycraft:*", "taterzens:npc", "easy_npc:*", "bodiesbodies:dead_body",
    "spawn:hamster", "connectiblechains:*",
];
const FORBIDDEN_STACKING: &[&str] = &["minecraft:horse"];

/// What a player carries, as the jar's `CarryOnData` (saved with the player): empty when nothing.
/// `type` "block" with `block` (the state string) and `tile` (the block entity's data), or "entity" with `kind`,
/// `entity` (its data) and `size` (width, height); `selected` is the hotbar slot held.
static CARRY: AttachmentType<Compound> = AttachmentType {
    namespace: "carryon",
    name: "carry",
    default: Compound::new,
    codec: Some(AttachmentCodec { encode: |c| Ok(c.clone()), decode: |c| Ok(c.clone()) }),
    copy_on_death: true,
    sync: false,
};

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

/// StringHelper.matchesWildcards: `*` stands for any run of characters.
fn wildcard(name: &str, pattern: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return name == pattern;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !name.starts_with(first) || !name[first.len()..].ends_with(last) || name.len() < first.len() + last.len() {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for mid in &parts[1..parts.len() - 1] {
        match rest.find(mid) {
            Some(i) => rest = &rest[i + mid.len()..],
            None => return false,
        }
    }
    true
}

fn listed(name: &str, list: &[&str]) -> bool {
    list.iter().any(|p| wildcard(name, p))
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
    props.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
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
    "minecraft:barrel", "minecraft:dispenser", "minecraft:dropper", "minecraft:observer", "minecraft:crafter",
    "minecraft:command_block", "minecraft:chain_command_block", "minecraft:repeating_command_block",
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
            if p.face[1] != 0 { "down".to_string() } else { opposite(face).to_string() }
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
        set_prop(&mut props, "waterlogged", if p.in_water { "true" } else { "false" });
    }
    state_text(&id, &props)
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
        Some(Tag::IntArray(v)) if v.len() == 4 => {
            Some(v.iter().fold(0u128, |acc, i| (acc << 32) | u128::from(*i as u32)))
        }
        _ => None,
    }
}

enum Job {
    PickBlock { uuid: u128, dim: String, pos: [i32; 3] },
    PickEntity { uuid: u128, entity: u32 },
    Place { uuid: u128, dim: String, pos: [i32; 3], face: [i32; 3] },
    Stack { uuid: u128, target: u32 },
    /// Set down what a dead player carried, where they died.
    Drop { dim: String, at: [f64; 3], carry: Compound },
}

/// What a client draws for one carrying player.
#[derive(Clone)]
enum Shown {
    Block { id: String, props: Vec<(String, String)> },
    Entity { kind: String, data: Compound },
}

fn shown() -> &'static Mutex<HashMap<u32, Shown>> {
    static S: OnceLock<Mutex<HashMap<u32, Shown>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Spawn eggs by entity type, looked up once on the client (the API finds items by key only).
fn spawn_eggs(host: &dyn Host) -> &'static HashMap<String, ItemKey> {
    static E: OnceLock<HashMap<String, ItemKey>> = OnceLock::new();
    E.get_or_init(|| {
        host.item_keys()
            .into_iter()
            .filter_map(|k| {
                let id = k.id(host);
                id.strip_suffix("_spawn_egg").map(|kind| (kind.to_string(), k))
            })
            .collect()
    })
}

struct CarriedHandler;

impl IPayloadHandler for CarriedHandler {
    /// `[entity id u32, kind u8 (0 block, 1 entity), length u32, bytes]` repeated: a block's state string, or a
    /// mob's type, a 0 byte and its data as network NBT. The whole list each time.
    fn handle(&self, payload: PayloadBytes, _context: &mut dyn IPayloadContext) -> Result<(), PayloadHandleError> {
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
            if kind == 0 {
                let (bid, props) = parse_state(&String::from_utf8_lossy(body));
                map.insert(id, Shown::Block { id: bid, props });
            } else if let Some(z) = body.iter().position(|c| *c == 0) {
                let kind = String::from_utf8_lossy(&body[..z]).to_string();
                let data = match Tag::from_network_bytes(&body[z + 1..], None) {
                    Ok((Tag::Compound(c), _)) => c,
                    _ => Compound::new(),
                };
                map.insert(id, Shown::Entity { kind, data });
            }
        }
        *lock(shown()) = map;
        Ok(())
    }
}

#[derive(Default)]
struct Mod {
    ticks: AtomicU64,
    jobs: Mutex<Vec<Job>>,
    /// Whether the last list sent named anyone (an empty list is sent once after the last one is set down).
    sent_any: Mutex<bool>,
}

fn carry_of(host: &mut dyn ServerHost, uuid: u128) -> Compound {
    CARRY.get_existing(host, Holder::Player(uuid)).ok().flatten().unwrap_or_default()
}

fn carrying(host: &mut dyn ServerHost, uuid: u128) -> bool {
    !carry_of(host, uuid).is_empty()
}

fn set_carry(host: &mut dyn ServerHost, uuid: u128, c: Compound) {
    if let Err(e) = CARRY.set(host, Holder::Player(uuid), c) {
        eprintln!("[carryon] carry data not saved: {e:?}");
    }
}

fn hands_empty(host: &dyn Host, player: PlayerRef) -> bool {
    let living = LivingRef::new(player.entity());
    [Hand::Main, Hand::Off].into_iter().all(|h| living.hand_item(host, h).is_none_or(|s| s.count() == 0))
}

/// PickupHandler.canCarryGeneral, but for the key (sneaking) and distance.
fn may_pick(host: &mut dyn ServerHost, player: PlayerRef) -> bool {
    let reader: &dyn ServerHost = &*host;
    if player.entity().is_shift_key_down(reader) != Some(true) || !hands_empty(reader, player) {
        return false;
    }
    if matches!(player.game_mode(reader), Some(GameMode::Adventure) | Some(GameMode::Spectator)) {
        return false;
    }
    !carrying(host, player.uuid())
}

fn creative(host: &dyn Host, player: PlayerRef) -> bool {
    player.is_creative(host) == Some(true)
}

fn air(host: &dyn Host) -> Option<mod_api::BlockState> {
    host.block_state_id("minecraft:air")
}

fn sound_at(host: &mut dyn ServerHost, dim: &str, sound: &str, at: [f64; 3], source: SoundSource, volume: f32, pitch: f32) {
    if let Some(level) = host.level(dim) {
        level.play_sound(host, sound, at, source, volume, pitch);
    }
}

fn entity_kind(host: &dyn Host, e: EntityRef) -> String {
    e.kind(host).map(|k| k.id(host)).unwrap_or_default()
}

impl Mod {
    fn queue(&self, job: Job) {
        lock(&self.jobs).push(job);
    }

    /// PickupHandler.tryPickUpBlock, the checks the click can make.
    fn block_pickable(host: &mut dyn ServerHost, player: PlayerRef, dim: &str, pos: [i32; 3]) -> bool {
        let Some(level) = host.level(dim) else { return false };
        let reader: &dyn ServerHost = &*host;
        let center = pos.map(|c| f64::from(c) + 0.5);
        let Some(feet) = player.entity().position(reader) else { return false };
        if dist(feet, center) > MAX_DISTANCE {
            return false;
        }
        let Some(state) = level.block_state(reader, pos) else { return false };
        let Some(block) = level.block(reader, pos).map(|k| k.id(reader)) else { return false };
        if block == "minecraft:air" || listed(&block, FORBIDDEN_TILES) {
            return false;
        }
        if reader.block_states().destroy_speed(state).is_some_and(|s| s < 0.0) && !creative(reader, player) && !PICKUP_UNBREAKABLE {
            return false;
        }
        let Some(be) = reader.block_entity(level, pos) else { return false };
        // A locked container stays.
        if be.lock(reader).is_some_and(|l| !l.is_empty()) {
            return false;
        }
        true
    }

    fn pick_block(&self, host: &mut dyn ServerHost, uuid: u128, dim: &str, pos: [i32; 3]) {
        let Some(player) = host.player(uuid) else { return };
        if !may_pick(host, player) || !Self::block_pickable(host, player, dim, pos) {
            return;
        }
        let Some(level) = host.level(dim) else { return };
        let reader: &dyn ServerHost = &*host;
        let Some(state) = level.block_state(reader, pos) else { return };
        let Some(text) = reader.block_state_string(state) else { return };
        let tile = reader.block_entity(level, pos).and_then(|be| be.data(reader));
        let (id, mut props) = parse_state(&text);
        // CarryOnData.setBlock: a powered block is carried unpowered.
        set_prop(&mut props, "powered", "false");
        let water = prop(&props, "waterlogged") == Some("true");
        let sound = reader.block_states().sound(state);
        let Some(left) = (if water { reader.block_state_id("minecraft:water") } else { air(reader) }) else { return };
        let slot = player.selected_slot(reader).unwrap_or(0) as i32;
        let mut c = Compound::new();
        c.insert("type".into(), s("block"));
        c.insert("block".into(), s(&state_text(&id, &props)));
        if let Some(t) = tile {
            c.insert("tile".into(), Tag::Compound(t));
        }
        c.insert("selected".into(), Tag::Int(slot));
        // The block entity goes with the player: no drops, no container spill.
        let flags = BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS
            | BlockUpdateFlags::SKIP_BLOCK_ENTITY_SIDE_EFFECTS | BlockUpdateFlags::SUPPRESS_DROPS;
        match level.set_block(host, pos, left, flags) {
            Ok(true) => {}
            other => {
                eprintln!("[carryon] pickup at {pos:?} refused: {other:?}");
                return;
            }
        }
        set_carry(host, uuid, c);
        if let Some(snd) = sound {
            sound_at(host, dim, &snd.hit, pos.map(|v| f64::from(v) + 0.5), SoundSource::Blocks, 1.0, 0.5);
        }
    }

    /// PickupHandler.tryPickupEntity, the checks the click can make.
    fn entity_pickable(host: &mut dyn ServerHost, player: PlayerRef, e: EntityRef) -> bool {
        let reader: &dyn ServerHost = &*host;
        let kind = entity_kind(reader, e);
        if kind.is_empty() || kind == "minecraft:player" || listed(&kind, FORBIDDEN_ENTITIES) {
            return false;
        }
        let (Some(feet), Some(at)) = (player.entity().position(reader), e.position(reader)) else { return false };
        if dist(feet, at) > MAX_DISTANCE {
            return false;
        }
        if !creative(reader, player) {
            if !PICKUP_HOSTILE && e.classification(reader) == Some(MobCategory::Monster) {
                return false;
            }
            if let Some([w, h]) = e.dimensions(reader) {
                if w > MAX_ENTITY_WIDTH || h > MAX_ENTITY_HEIGHT {
                    return false;
                }
            }
        }
        // Someone else's pet stays.
        let owner = e.nbt(host).and_then(|n| uuid_of(n.get(&"Owner".into())));
        owner.is_none_or(|o| o == player.uuid())
    }

    fn pick_entity(&self, host: &mut dyn ServerHost, uuid: u128, id: u32) {
        let Some(player) = host.player(uuid) else { return };
        let Some(e) = host.entity(id) else { return };
        if !may_pick(host, player) || !Self::entity_pickable(host, player, e) {
            return;
        }
        let reader: &dyn ServerHost = &*host;
        let kind = entity_kind(reader, e);
        let [w, h] = e.dimensions(reader).unwrap_or([0.6, 0.6]);
        let slot = player.selected_slot(reader).unwrap_or(0) as i32;
        e.detach_leash(host, true);
        e.eject_passengers(host);
        e.stop_riding(host);
        let Some(mut data) = e.nbt(host) else { return };
        for k in ["Passengers", "leash", "Leash", "Pos", "Motion"] {
            data.remove(&(*k).into());
        }
        if !e.remove(host, RemovalReason::Discarded) {
            return;
        }
        let mut c = Compound::new();
        c.insert("type".into(), s("entity"));
        c.insert("kind".into(), s(&kind));
        c.insert("entity".into(), Tag::Compound(data));
        c.insert("size".into(), Tag::List(vec![Tag::Double(w.to_bits()), Tag::Double(h.to_bits())]));
        c.insert("selected".into(), Tag::Int(slot));
        set_carry(host, uuid, c);
        if let Some(ent) = host.player(uuid).map(|p| p.entity()) {
            ent.play_sound(host, "minecraft:entity.item.pickup", 1.0, 0.5);
        }
    }

    fn place(&self, host: &mut dyn ServerHost, uuid: u128, dim: &str, clicked: [i32; 3], face: [i32; 3]) {
        let Some(player) = host.player(uuid) else { return };
        let carry = carry_of(host, uuid);
        let Some(level) = host.level(dim) else { return };
        let reader: &dyn ServerHost = &*host;
        let replaceable = |pos| level.block_state(reader, pos).and_then(|st| reader.block_states().replaceable(st)) == Some(true);
        let pos = if replaceable(clicked) { clicked } else { add(clicked, face) };
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
                let blocked = level.entities_in(reader, lo, hi).unwrap_or_default().into_iter().any(|e| {
                    let k = entity_kind(reader, e);
                    !matches!(k.as_str(), "minecraft:item" | "minecraft:experience_orb" | "minecraft:arrow")
                });
                if blocked {
                    return fail(host);
                }
                let [yaw, pitch] = player.entity().rotation(reader).unwrap_or([0.0, 0.0]);
                let in_water = level.block(reader, pos).map(|k| k.id(reader)).as_deref() == Some("minecraft:water")
                    && level.block_property(reader, pos, "level").as_deref() == Some("0");
                let carried = get_str(&carry, "block").unwrap_or_default();
                let text = placement_state(reader, &carried, &Placement { face, yaw, pitch, in_water });
                let Some(state) = reader.block_state_id(&text).or_else(|| reader.block_state_id(&carried)) else {
                    eprintln!("[carryon] unknown carried state {carried}");
                    return fail(host);
                };
                let sound = reader.block_states().sound(state);
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
                        None => eprintln!("[carryon] no block entity at {pos:?} for the carried data"),
                    }
                }
                set_carry(host, uuid, Compound::new());
                if let Some(snd) = sound {
                    sound_at(host, dim, &snd.place, pos.map(|v| f64::from(v) + 0.5), SoundSource::Blocks, 1.0, 0.5);
                }
            }
            Some("entity") => {
                let kind = get_str(&carry, "kind").unwrap_or_default();
                let data = match carry.get(&"entity".into()) {
                    Some(Tag::Compound(c)) => c.clone(),
                    _ => Compound::new(),
                };
                let at = [f64::from(pos[0]) + 0.5, f64::from(pos[1]), f64::from(pos[2]) + 0.5];
                if host.summon(dim, &kind, at, data).is_none() {
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
        if !STACKABLE_ENTITIES {
            return;
        }
        let carry = carry_of(host, uuid);
        if get_str(&carry, "type").as_deref() != Some("entity") {
            return;
        }
        let Some(player) = host.player(uuid) else { return };
        let Some(clicked) = host.entity(target) else { return };
        let reader: &dyn ServerHost = &*host;
        let mut top = clicked.root_vehicle(reader).unwrap_or(clicked);
        let mut count = 0;
        while let Some(next) = top.passengers(reader).and_then(|p| p.first().copied()) {
            top = next;
            count += 1;
        }
        let fail = |host: &mut dyn ServerHost| {
            if let Some(ent) = host.player(uuid).map(|p| p.entity()) {
                ent.play_sound(host, "minecraft:block.note_block.bass", 0.5, 0.5);
            }
        };
        if count + 1 >= MAX_ENTITY_STACK || listed(&entity_kind(reader, top), FORBIDDEN_STACKING) {
            return fail(host);
        }
        let size = match carry.get(&"size".into()) {
            Some(Tag::List(v)) => v
                .iter()
                .filter_map(|t| if let Tag::Double(d) = t { Some(f64::from_bits(*d)) } else { None })
                .product::<f64>(),
            _ => 0.36,
        };
        let top_size = top.dimensions(reader).map_or(0.0, |[w, h]| w * h);
        if SIZE_MATTERS_STACKING && size > top_size {
            return fail(host);
        }
        let Some(at) = top.position(reader) else { return };
        let Some(dim) = player.dimension(reader).map(|d| d.to_string()) else { return };
        let kind = get_str(&carry, "kind").unwrap_or_default();
        let data = match carry.get(&"entity".into()) {
            Some(Tag::Compound(c)) => c.clone(),
            _ => Compound::new(),
        };
        let Some(id) = host.summon(&dim, &kind, [at[0], at[1] + 2.6, at[2]], data) else { return fail(host) };
        if let Some(e) = host.entity(id) {
            e.start_riding(host, top, true);
        }
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
                spots.sort_by_key(|p| (p[0] - base[0]).pow(2) + (p[1] - base[1]).pow(2) + (p[2] - base[2]).pow(2));
                let free = spots.into_iter().find(|p| {
                    level.block_state(reader, *p).and_then(|st| reader.block_states().replaceable(st)) == Some(true)
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
                if let (Some(Tag::Compound(tile)), Some(be)) = (carry.get(&"tile".into()), host.block_entity(level, pos)) {
                    let _ = be.replace_data(host, tile.clone());
                }
            }
            Some("entity") => {
                let kind = get_str(carry, "kind").unwrap_or_default();
                if let Some(Tag::Compound(data)) = carry.get(&"entity".into()) {
                    host.summon(dim, &kind, at, data.clone());
                }
            }
            _ => {}
        }
    }

    fn run(&self, host: &mut dyn ServerHost, job: Job) {
        match job {
            Job::PickBlock { uuid, dim, pos } => self.pick_block(host, uuid, &dim, pos),
            Job::PickEntity { uuid, entity } => self.pick_entity(host, uuid, entity),
            Job::Place { uuid, dim, pos, face } => self.place(host, uuid, &dim, pos, face),
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
            let reader: &dyn ServerHost = &*host;
            // Slowness: a mob by its size (up to IV), a block by the size of its data (a level per 500
            // characters of its NBT text, here its network size, up to IV); 1 tick, renewed each tick.
            let level = if kind == "entity" {
                let size = match carry.get(&"size".into()) {
                    Some(Tag::List(v)) => v
                        .iter()
                        .filter_map(|t| if let Tag::Double(d) = t { Some(f64::from_bits(*d)) } else { None })
                        .product::<f64>(),
                    _ => 1.0,
                };
                ((size as i32).min(4) as f64 * ENTITY_SLOWNESS) as i32
            } else {
                let len = Tag::Compound(carry.clone()).to_network_bytes().map_or(0, |b| b.len());
                (((len / 500) as i32).min(4) as f64 * BLOCK_SLOWNESS) as i32
            };
            if let Some(id) = slowness {
                if SLOWNESS_IN_CREATIVE || !creative(reader, player) {
                    let effect = ActiveEffect::new(id, 3, level, false, false, false);
                    LivingRef::new(player.entity()).add_effect(host, &effect);
                }
            }
            // The hotbar slot stays where it was picked up, and stays empty.
            let selected = get_int(&carry, "selected").unwrap_or(0).clamp(0, 8) as usize;
            if player.selected_slot(&*host) != Some(selected) {
                player.set_selected_slot(host, selected);
            }
            let held = LivingRef::new(player.entity()).hand_item(&*host, Hand::Main).filter(|s| s.count() > 0);
            if let Some(stack) = held {
                // `inventory_slots` is None when a slot holds an item the game can't hand to a mod (a mod that isn't
                // installed): no slot can be trusted to be free then, so the held stack is dropped, not written over
                // one.
                let free = player.inventory_slots(&*host).and_then(|slots| {
                    let used: Vec<u32> = slots.into_iter().map(|(i, _)| i).collect();
                    (0..36u32).find(|i| *i as usize != selected && !used.contains(i))
                });
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
                    (0u8, get_str(&carry, "block").unwrap_or_default().into_bytes())
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

/// The client's own player's network id.
fn local_id(host: &dyn ClientHost) -> Option<u32> {
    host.local_player().map(|p| p.entity().id())
}

fn local_shown(host: &dyn ClientHost) -> Option<Shown> {
    let id = local_id(host)?;
    lock(shown()).get(&id).cloned()
}

fn block_props(props: &[(String, String)], facing: &str) -> Vec<(String, String)> {
    let mut p = props.to_vec();
    for k in ["facing", "horizontal_facing"] {
        if let Some(cur) = prop(&p, k) {
            if !matches!(cur, "up" | "down") {
                set_prop(&mut p, k, facing);
            }
        }
    }
    p
}

/// CarryRenderHelper.renderBakedModel draws the block's item (a chest's block model is empty: its item has the
/// chest renderer); a block with no item is drawn as its block model, its front to +z.
fn draw_block(host: &dyn Host, draw: &mut dyn mod_api::BlockEntityDrawContext, id: &str, props: &[(String, String)]) {
    let Some(key) = host.block_key(id) else { return };
    let item = host.block_item(key).filter(|k| k.id(host) != "minecraft:air").and_then(|k| ItemStack::new(k, 1));
    if let Some(stack) = item {
        draw.push();
        draw.translate([0.5, 0.5, 0.5]);
        let drawn = draw.item(&stack, ItemDisplayContext::None, None, 0);
        draw.pop();
        if drawn {
            return;
        }
    }
    let pairs: Vec<(&str, &str)> = props.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    if !draw.block(key, &pairs) {
        draw.block(key, &[]);
    }
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        r.payload_handlers()
            .registrar("1")
            .optional()
            .play_to_client(PayloadId(CHANNEL.into()), Some(Arc::new(CarriedHandler)));
    }

    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        if !matches!(event.phase, ServerTickPhase::ServerTickEnd { .. }) {
            return;
        }
        let tick = self.ticks.fetch_add(1, Ordering::Relaxed);
        let jobs = std::mem::take(&mut *lock(&self.jobs));
        let changed = !jobs.is_empty();
        for job in jobs {
            self.run(host, job);
        }
        self.carry_tick(host, changed || tick % 10 == 0);
    }

    /// The UseBlockCallback: pick up, or set down what is carried.
    fn right_click_block(&self, host: HostSide<'_>, event: &mut RightClickBlockEvent<'_>) -> Flow {
        let click = event.click;
        match host {
            HostSide::Server(host) => {
                let Some(player) = host.player(click.player) else { return Flow::Continue };
                let dim = click.dimension.key().id().to_string();
                if carrying(host, click.player) {
                    if matches!(click.hand, Hand::Main) {
                        let mut jobs = lock(&self.jobs);
                        if !jobs.iter().any(|j| matches!(j, Job::Place { uuid, .. } if *uuid == click.player)) {
                            jobs.push(Job::Place { uuid: click.player, dim, pos: click.pos, face: click.hit.face });
                        }
                    }
                } else if may_pick(host, player) && Self::block_pickable(host, player, &dim, click.pos) {
                    self.queue(Job::PickBlock { uuid: click.player, dim, pos: click.pos });
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
    fn entity_interact(&self, host: HostSide<'_>, event: &mut EntityInteractEvent<'_>) -> Option<InteractionResult> {
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
        if !Self::entity_pickable(host, player, e) {
            return None;
        }
        let mut jobs = lock(&self.jobs);
        if !jobs.iter().any(|j| matches!(j, Job::PickEntity { uuid: u, .. } if *u == uuid)) {
            jobs.push(Job::PickEntity { uuid, entity: id });
        }
        Some(InteractionResult::Success)
    }

    /// `hitWhileCarrying` false: no mining while carrying.
    fn left_click_block(&self, host: &mut dyn ServerHost, _event: &mut LeftClickBlockEvent) -> Flow {
        match host.players().acting_player() {
            Some(p) if carrying(host, p.uuid()) => Flow::Cancel,
            _ => Flow::Continue,
        }
    }

    fn break_block(&self, host: &mut dyn ServerHost, _event: &mut BreakBlockEvent<'_>) -> Flow {
        match host.players().acting_player() {
            Some(p) if carrying(host, p.uuid()) => Flow::Cancel,
            _ => Flow::Continue,
        }
    }

    fn attack_entity(&self, host: &mut dyn ServerHost, _event: &mut AttackEntityEvent<'_>) -> Flow {
        match host.players().acting_player() {
            Some(p) if carrying(host, p.uuid()) => Flow::Cancel,
            _ => Flow::Continue,
        }
    }

    /// InventoryMixin / onCarryTick: the hotbar slot can't change while carrying.
    fn player_switch_hotbar_slot_pre(&self, host: &mut dyn ServerHost, event: &mut PlayerSwitchHotbarSlotPreEvent) -> Flow {
        if carrying(host, event.player) { Flow::Cancel } else { Flow::Continue }
    }

    fn mouse_scrolling(&self, host: &mut dyn ClientHost, _event: &mut MouseScrollingEvent) -> Flow {
        if local_shown(&*host).is_some() { Flow::Cancel } else { Flow::Continue }
    }

    /// placeCarriedOnDeath: kept with keepInventory (the attachment is copied to the respawned player), else set
    /// down where the player died.
    fn living_death(&self, host: &mut dyn ServerHost, event: &mut LivingDeathEvent<'_>) -> Flow {
        if event.kind != "minecraft:player" {
            return Flow::Continue;
        }
        let Some(e) = host.entity(event.entity) else { return Flow::Continue };
        let Some(uuid) = e.uuid(host) else {
            eprintln!("[carryon] dead player {} has no uuid", event.entity);
            return Flow::Continue;
        };
        let carry = carry_of(host, uuid);
        if carry.is_empty() {
            return Flow::Continue;
        }
        let reader: &dyn ServerHost = &*host;
        let Some(level) = e.level(reader) else { return Flow::Continue };
        let keep = matches!(level.game_rule(reader, "minecraft:keep_inventory"), Ok(Some(mod_api::GameRuleValue::Bool(true))));
        if keep {
            return Flow::Continue;
        }
        let (Some(at), Some(dim)) = (e.position(reader), host.player(uuid).and_then(|p| p.dimension(reader))) else {
            eprintln!("[carryon] dead player {uuid:x}: no position or dimension");
            return Flow::Continue;
        };
        set_carry(host, uuid, Compound::new());
        self.queue(Job::Drop { dim: dim.to_string(), at, carry });
        Flow::Continue
    }

    /// CarriedObjectRender.drawFirstPerson: the carried block in both hands, in place of the hands.
    fn render_hand(&self, host: &dyn ClientHost, event: &mut RenderHandEvent<'_, '_>) -> Flow {
        let Some(what) = lock(shown()).get(&(event.render.player_entity_id as u32)).cloned() else { return Flow::Continue };
        if matches!(event.render.hand, Hand::Main) {
            if let Shown::Block { id, props } = what {
                let draw = &mut *event.draw;
                draw.push();
                // Low in front, the front turned to the player (facePlayer false draws a chest's lid side out,
                // the rest facing in), tipped a little as the jar does.
                let size = 0.85;
                draw.translate([-size / 2.0, -1.12, -1.7]);
                draw.scale([size, size, size]);
                draw.rotate_about([1.0, 0.0, 0.0], 8.0, [0.5, 0.5, 0.5]);
                draw_block(host, draw, &id, &block_props(&props, "south"));
                draw.pop();
            }
        }
        event.cancelled = true;
        Flow::Cancel
    }

    /// CarriedObjectRender.drawThirdPerson: the carried block in front of a player's chest.
    fn render_player_post(&self, host: &dyn ClientHost, event: &mut RenderPlayerPostEvent<'_>) {
        let Some(what) = lock(shown()).get(&event.player.id).cloned() else { return };
        let draw = &mut *event.draw;
        draw.push();
        match what {
            Shown::Block { id, props } => {
                let size = 0.7;
                draw.translate([-size / 2.0, 0.75, 0.3]);
                draw.scale([size, size, size]);
                draw_block(host, draw, &id, &block_props(&props, "south"));
            }
            Shown::Entity { kind, .. } => {
                if let Some(stack) = spawn_eggs(host).get(&kind).and_then(|k| ItemStack::new(*k, 1)) {
                    draw.translate([0.0, 1.25, 0.45]);
                    draw.scale([0.6, 0.6, 0.6]);
                    draw.item(&stack, ItemDisplayContext::Fixed, None, 0);
                }
            }
        }
        draw.pop();
    }

    /// CarriedObjectRender.drawFirstPersonEntity: the carried mob over the hotbar.
    fn gui_render_post(&self, host: &dyn ClientHost, event: &mut GuiRenderPostEvent<'_>) {
        if event.gui_hidden || !host.camera_type().is_first_person() {
            return;
        }
        let Some(Shown::Entity { kind, data }) = local_shown(host) else { return };
        let [w, h] = event.batch.size();
        let rect = [w / 2.0 - 45.0, h - 125.0, w / 2.0 + 45.0, h - 30.0];
        let view = GuiEntityView::new(rect, 30.0, 0.0625, GuiEntityLook::Turned { yaw: 180.0, pitch: 0.0 });
        let mut data = data;
        data.insert("id".into(), s(&kind));
        if event.batch.entity(&GuiEntity::with_data(kind.clone(), data), view).is_err() {
            let _ = event.batch.entity(&GuiEntity::kind(kind), view);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards() {
        assert!(wildcard("minecraft:red_bed", "minecraft:*_bed"));
        assert!(!wildcard("minecraft:bedrock", "minecraft:*_bed"));
        assert!(wildcard("create:anything", "create:*"));
        assert!(wildcard("vinery:oak_wine_rack_small", "vinery:*_wine_rack_small"));
        assert!(!wildcard("minecraft:chest", "minecraft:*_door"));
    }

    #[test]
    fn facing() {
        assert_eq!(horizontal(0.0), "south");
        assert_eq!(opposite(horizontal(0.0)), "north");
        assert_eq!(horizontal(90.0), "west");
    }
}
