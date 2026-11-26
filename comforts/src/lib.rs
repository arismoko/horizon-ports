#![deny(unsafe_code)]
//! Comforts for Sunrise: sleeping bags, hammocks, and rope & nail.
//!
//! Pure rules, config parsing, sleep math, hammock geometry, and conditional recipe
//! matching live in `rules.rs` and `recipes.rs`; this file binds them to Sunrise's
//! typed mod API (`sunrise-mod-api`).

mod recipes;
mod rules;

use std::collections::{BTreeMap, HashMap};
use std::ops::ControlFlow;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, OnceLock, RwLock};

use mod_api::{
    ActiveEffect, AttachmentCodec, AttachmentDescriptor, AttachmentKey, Block, BlockFace, BlockKey,
    BlockMetadata, BlockPart, BlockPathTypeEvent, BlockProperties, BlockShapes, BlockState,
    BlockStateProperty, BlockUpdateFlags, BlocksEvent, BuildCreativeModeTabContentsEvent,
    CanContinueSleepingEvent, CanPlayerSleepEvent, CanSurviveEvent, ClientHost, ClockAdjustment,
    Compound, ConfigEvent, ConfigKind, ConfigSnapshot, ConfigSpec, CreativeTabPlacement,
    CreativeTabVisibility, Direction, EntityTypeKey, ExplosionInteraction, ExplosionSettings, Flow,
    FluidKey, FluidState, Hand, Holder, Hooks, Host, HostBlockStates, HostSide, Identifier,
    InteractionResult, ItemKey, ItemPart, ItemStack, JavaString, LevelHost, LevelRef, LevelSide,
    LevelTicksHost, LivingRef, MobCategory, NoteInstrument, OnDestroyedByPlayerEvent, PathType,
    PayloadId, PlacementSource, PlayerBedEnterEvent, PlayerBedLeaveEvent, PlayerLoggedInEvent,
    PlayerRef, PlayerSetSpawnEvent, PlayerTickPostEvent, Registrar, RegistrationContext,
    ScreenKeyPressedPostEvent, ServerHost, SetPlacedByEvent, SleepFinishedTimeEvent, SleepProblem,
    SleepStatusAnnounceEvent, SleepersNeededEvent, SoundSource, StatType, StateConstraint,
    StateForPlacementEvent, SupportType, Tag, TagKey, Text, ToClient, TooltipEvent,
    UpdateShapeEvent, UseOnEvent, UseWithoutItemEvent, Wire,
};

use recipes::{ConditionalShapedKind, ConditionalShapelessKind, SHAPED_ID, SHAPELESS_ID};
use rules::{
    BedKind, COLORS, ClientSyncConfig, CommonConfig, DEFAULT_COMMON, DEFAULT_SERVER,
    HammockProbeCell, MAP_COLOR_WOOL, MOD_ID, SLEEP_EFFECT_THRESHOLD_TICKS, ServerConfig,
    SleepData, TimeUse, can_sleep_for_time, compute_break_chance, compute_tired_time,
    compute_wake_time, direction_name, hammock_layout_from_rope, hammock_placement_error_key,
    hammock_positions, horizontal_step, is_bright_outside, look_at_yaw, offset, parse_direction,
    sleepers_needed, stand_up_offsets, within_bed_reach,
};

const ATTACH_SLEEP_DATA: &str = "comforts:sleep_data";
const CHANNEL_CLIENT_SYNC: &str = "comforts:client_sync";
const OVERWORLD_WAKE_MARKER: &str = "minecraft:wake_up_from_sleep";
const NAP_WAKE_MARKER: &str = "comforts:wake_up_from_nap";
const TIME_SINCE_REST: &str = "minecraft:time_since_rest";
const SLEEP_IN_BED: &str = "minecraft:sleep_in_bed";
const KEY_TOGGLE_PERSPECTIVE: &str = "key.togglePerspective";

const GRAY_COLOR: u32 = 0xFFAA_AAAA;

#[derive(Clone, Debug, PartialEq, Eq, Wire)]
struct ClientSyncPayload {
    bytes: [u8; 3],
}

/// Issued key of the persisted per-player Comforts sleep data (`comforts:sleep_data`),
/// registered in `Hooks::register`. Copied across death by the descriptor's policy.
static SLEEP_DATA: RwLock<Option<AttachmentKey<SleepData>>> = RwLock::new(None);

fn sleep_data_key() -> Option<AttachmentKey<SleepData>> {
    SLEEP_DATA.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Registry keys resolved once at registration.
pub(crate) struct Keys {
    pub sleeping_bags: [BlockKey; 16],
    pub sleeping_bag_items: [ItemKey; 16],
    pub hammocks: [BlockKey; 16],
    pub hammock_items: [ItemKey; 16],
    pub rope_and_nail: BlockKey,
    pub rope_and_nail_item: ItemKey,
    pub water_fluid: FluidKey,
    pub villager_kind: EntityTypeKey,
    pub tag_ropes: TagKey,
    items_by_id: HashMap<String, ItemKey>,
    tags_by_id: HashMap<String, TagKey>,
}

impl Keys {
    pub fn lookup_item(&self, id: &str) -> Option<ItemKey> {
        self.items_by_id.get(id).copied()
    }

    pub fn lookup_tag(&self, id: &str) -> Option<TagKey> {
        self.tags_by_id.get(id).copied()
    }

    pub fn bed_kind(&self, block: BlockKey) -> Option<BedKind> {
        if self.sleeping_bags.contains(&block) {
            Some(BedKind::SleepingBag)
        } else if self.hammocks.contains(&block) {
            Some(BedKind::Hammock)
        } else {
            None
        }
    }

    pub fn sleeping_bag_color(&self, block: BlockKey) -> Option<usize> {
        self.sleeping_bags.iter().position(|&k| k == block)
    }
}

static KEYS: OnceLock<Keys> = OnceLock::new();
static SYNC_CHANNEL: OnceLock<ToClient<ClientSyncPayload>> = OnceLock::new();
static COMMON_CONFIG: LazyLock<Mutex<CommonConfig>> =
    LazyLock::new(|| Mutex::new(CommonConfig::default()));
static SERVER_CONFIG: LazyLock<Mutex<ServerConfig>> =
    LazyLock::new(|| Mutex::new(ServerConfig::default()));
static CLIENT_SYNC: LazyLock<Mutex<ClientSyncConfig>> =
    LazyLock::new(|| Mutex::new(ClientSyncConfig::default()));

pub(crate) fn keys() -> &'static Keys {
    KEYS.get().expect("comforts keys registered at startup")
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) fn common_config() -> CommonConfig {
    *lock(&COMMON_CONFIG)
}

fn server_config() -> ServerConfig {
    lock(&SERVER_CONFIG).clone()
}

fn client_sync_config() -> ClientSyncConfig {
    *lock(&CLIENT_SYNC)
}

fn apply_config(kind: ConfigKind, text: &str) {
    match kind {
        ConfigKind::Common => *lock(&COMMON_CONFIG) = CommonConfig::parse(text),
        ConfigKind::Server => {
            let parsed = ServerConfig::parse(text);
            *lock(&CLIENT_SYNC) = parsed.client_sync();
            *lock(&SERVER_CONFIG) = parsed;
        }
        _ => {}
    }
}

fn send_client_sync(host: &mut dyn ServerHost, player: PlayerRef) {
    if let Some(ch) = SYNC_CHANNEL.get() {
        let payload = ClientSyncPayload {
            bytes: server_config().client_sync().encode(),
        };
        let _ = ch.send(host, player, &payload);
    }
}

fn sync_config_to_all(host: &mut dyn ServerHost) {
    if let Some(ch) = SYNC_CHANNEL.get() {
        let payload = ClientSyncPayload {
            bytes: server_config().client_sync().encode(),
        };
        let _ = ch.send_to_all(host, &payload);
    }
}

fn read_sleep_data(host: &mut dyn ServerHost, uuid: u128) -> SleepData {
    sleep_data_key()
        .and_then(|key| {
            key.get_existing(host, Holder::Player(uuid), |value| value.cloned())
                .ok()
                .flatten()
        })
        .unwrap_or_default()
}

fn write_sleep_data(host: &mut dyn ServerHost, uuid: u128, data: SleepData) {
    if let Some(key) = sleep_data_key() {
        let _ = key.set(host, Holder::Player(uuid), data);
    }
}

fn block_flags() -> BlockUpdateFlags {
    BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS
}

fn silent_block_flags() -> BlockUpdateFlags {
    BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS | BlockUpdateFlags::SUPPRESS_DROPS
}

fn is_water_source(fluid: FluidState) -> bool {
    fluid.is_source() && fluid.fluid == keys().water_fluid
}

fn custom_name_tag(stack: &ItemStack) -> Option<Tag> {
    stack
        .components()
        .and_then(|c| c.as_nbt().get(&JavaString::from("minecraft:custom_name")))
        .cloned()
}

fn copy_custom_name_to_block_entity(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    stack: &ItemStack,
) {
    let Some(tag) = custom_name_tag(stack) else {
        return;
    };
    if let Some(be) = host.block_entity(level, pos) {
        let mut data = Compound::new();
        data.insert("CustomName".into(), tag);
        let _ = be.merge_data(host, data);
    }
}

const fn dir_to_face(dir: Direction) -> BlockFace {
    match dir {
        Direction::Down => BlockFace::Down,
        Direction::Up => BlockFace::Up,
        Direction::North => BlockFace::North,
        Direction::South => BlockFace::South,
        Direction::West => BlockFace::West,
        Direction::East => BlockFace::East,
    }
}

fn prop_str<'a>(meta: &'a dyn BlockMetadata, state: BlockState, name: &str) -> Option<&'a str> {
    state.property(meta, name).ok().flatten()
}

fn prop_bool(meta: &dyn BlockMetadata, state: BlockState, name: &str) -> bool {
    prop_str(meta, state, name) == Some("true")
}

fn prop_dir(meta: &dyn BlockMetadata, state: BlockState, name: &str) -> Option<Direction> {
    prop_str(meta, state, name).and_then(parse_direction)
}

fn sixteenths(boxes: &[[f64; 6]]) -> Vec<[f64; 6]> {
    boxes.iter().map(|b| b.map(|v| v / 16.0)).collect()
}

fn yaw_to_horizontal_dir(yaw: f32) -> Direction {
    match ((f64::from(yaw) / 90.0 + 0.5).floor() as i64 & 3) as i32 {
        0 => Direction::South,
        1 => Direction::West,
        2 => Direction::North,
        _ => Direction::East,
    }
}

fn horizontal_dir_from_source(source: PlacementSource) -> Direction {
    match source {
        PlacementSource::Player { yaw, .. } => yaw_to_horizontal_dir(yaw),
        PlacementSource::Directional { direction } => match direction {
            Direction::North | Direction::South | Direction::East | Direction::West => direction,
            _ => Direction::North,
        },
        _ => Direction::North,
    }
}

fn is_head(meta: &dyn BlockMetadata, state: BlockState) -> bool {
    prop_str(meta, state, "part") == Some("head")
}

/// Direction from this bed half toward its partner half (`BedBlock.getNeighbourDirection`).
fn bed_partner_dir(facing: Direction, head: bool) -> Direction {
    if head { facing.opposite() } else { facing }
}

/// Resolve the head and foot coordinates of a Comforts bed from either half.
fn resolve_bed_halves(
    meta: &dyn BlockMetadata,
    pos: [i32; 3],
    state: BlockState,
) -> Option<([i32; 3], [i32; 3], Direction)> {
    let facing = prop_dir(meta, state, "facing")?;
    let head_here = is_head(meta, state);
    let step = horizontal_step(facing);
    if head_here {
        Some((pos, offset(pos, step, -1), facing))
    } else {
        Some((offset(pos, step, 1), pos, facing))
    }
}

/// Whether `level` is currently in daytime (`Level.isBrightOutside`).
fn level_is_bright(host: &dyn Host, level: LevelRef) -> bool {
    let clock_time = level.clock_time(host).unwrap_or(0);
    let raining = level.is_raining(host).unwrap_or(false);
    let thundering = level.is_thundering(host).unwrap_or(false);
    is_bright_outside(clock_time, raining, thundering)
}

/// Replacement state when clearing a waterloggable block.
fn replacement_after_remove(host: &dyn Host, state: BlockState) -> Option<BlockState> {
    let meta = host.block_states();
    if prop_bool(&meta, state, "waterlogged") {
        host.block_state_id("minecraft:water")
    } else {
        host.block_state_id("minecraft:air")
    }
}

/// Clear a Comforts bed (or single block) back to air or water when waterlogged.
fn clear_block_keeping_water(host: &mut dyn ServerHost, level: LevelRef, pos: [i32; 3]) {
    let Some(state) = level.block_state(host, pos) else {
        return;
    };
    if let Some(next) = replacement_after_remove(host, state) {
        let _ = level.set_block(host, pos, next, silent_block_flags());
    }
}

/// Remove both halves of a Comforts bed (preserving water if waterlogged).
fn remove_bed_halves(
    host: &mut dyn ServerHost,
    level: LevelRef,
    head: [i32; 3],
    foot: [i32; 3],
    block: BlockKey,
) {
    if level.block_state(host, head).map(BlockState::block) == Some(block) {
        clear_block_keeping_water(host, level, head);
    }
    if level.block_state(host, foot).map(BlockState::block) == Some(block) {
        clear_block_keeping_water(host, level, foot);
    }
}

/// Reset a `rope_and_nail` at `rope_pos` to `supporting=false` if it currently supports a hammock.
fn release_rope(host: &mut dyn ServerHost, level: LevelRef, rope_pos: [i32; 3]) {
    let Some(state) = level.block_state(host, rope_pos) else {
        return;
    };
    if state.block() != keys().rope_and_nail {
        return;
    }
    let next = {
        let meta = host.block_states();
        if !prop_bool(&meta, state, "supporting") {
            return;
        }
        state.with(&meta, "supporting", "false").ok()
    };
    if let Some(next) = next {
        let _ = level.set_block(host, rope_pos, next, block_flags());
    }
}

/// Drop any supporting ropes adjacent to a hammock before/after removal.
fn release_hammock_ropes(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    facing: Direction,
    head_here: bool,
) {
    let (_, own_rope, partner_rope) = hammock_positions(pos, facing, head_here);
    release_rope(host, level, own_rope);
    release_rope(host, level, partner_rope);
}

/// Whether `pos` is a valid replaceable cell for placing a bed/hammock half.
fn is_replaceable(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> bool {
    let Some(state) = level.block_state(host, pos) else {
        return false;
    };
    host.block_states().replaceable(state).unwrap_or(false)
}

/// Whether the cell at `pos` has solid collision blocking a sleeper's head or stand-up space.
fn has_solid_collision(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> bool {
    let aabb = [
        f64::from(pos[0]) + 0.05,
        f64::from(pos[1]) + 0.05,
        f64::from(pos[2]) + 0.05,
        f64::from(pos[0]) + 0.95,
        f64::from(pos[1]) + 0.95,
        f64::from(pos[2]) + 0.95,
    ];
    level.has_collisions(host, aabb).unwrap_or(false)
}

/// Find a safe stand-up position beside a Comforts bed (`AbstractBedBlock.findStandUpPosition`),
/// falling back to `head + (0.5, 0.1, 0.5)` above the head cell.
fn find_stand_up_pose(
    host: &dyn Host,
    level: LevelRef,
    head: [i32; 3],
    facing: Direction,
) -> ([f64; 3], f32) {
    let offsets = stand_up_offsets(facing);
    for dy in [0, -1, 1] {
        for [dx, dz] in offsets {
            let feet = [head[0] + dx, head[1] + dy, head[2] + dz];
            let above = [feet[0], feet[1] + 1, feet[2]];
            let below = [feet[0], feet[1] - 1, feet[2]];
            if !has_solid_collision(host, level, feet)
                && !has_solid_collision(host, level, above)
                && has_solid_collision(host, level, below)
            {
                let pos = [
                    f64::from(feet[0]) + 0.5,
                    f64::from(feet[1]),
                    f64::from(feet[2]) + 0.5,
                ];
                return (pos, look_at_yaw(pos, head));
            }
        }
    }
    let fallback = [
        f64::from(head[0]) + 0.5,
        f64::from(head[1]) + 0.1,
        f64::from(head[2]) + 0.5,
    ];
    (fallback, look_at_yaw(fallback, head))
}

/// Check if hostile monsters are within vanilla's 8x5x8 bed safety box around `head`.
fn monsters_nearby(host: &dyn Host, level: LevelRef, head: [i32; 3]) -> bool {
    let center = [
        f64::from(head[0]) + 0.5,
        f64::from(head[1]),
        f64::from(head[2]) + 0.5,
    ];
    let min = [center[0] - 8.0, center[1] - 5.0, center[2] - 8.0];
    let max = [center[0] + 8.0, center[1] + 5.0, center[2] + 8.0];
    let mut found = false;
    let _ = level.visit_entities_in(host, min, max, &mut |entity| {
        if entity.classification(host) == Some(MobCategory::Monster) {
            found = true;
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    });
    found
}

/// Kick a sleeping villager out of `head` if one occupies it, returning `true` if kicked.
fn kick_sleeping_villager(host: &mut dyn ServerHost, level: LevelRef, head: [i32; 3]) -> bool {
    let min = [
        f64::from(head[0]) - 0.5,
        f64::from(head[1]) - 0.5,
        f64::from(head[2]) - 0.5,
    ];
    let max = [
        f64::from(head[0]) + 1.5,
        f64::from(head[1]) + 1.5,
        f64::from(head[2]) + 1.5,
    ];
    let villager_kind = keys().villager_kind;
    let mut villager: Option<LivingRef> = None;
    let _ = level.visit_entities_in(host, min, max, &mut |entity| {
        if let Some(living) = host.living(entity)
            && living.sleeping_pos(host) == Some(head)
            && entity.kind(host) == Some(villager_kind)
        {
            villager = Some(living);
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    });
    if let Some(v) = villager {
        v.stop_sleeping(host)
    } else {
        false
    }
}

/// Check consecutive-sleep restriction (`ComfortsEvents.checkTime`).
fn check_tired_restriction(host: &mut dyn ServerHost, player: PlayerRef, level: LevelRef) -> bool {
    let cfg = server_config();
    if !cfg.restrict_sleeping {
        return true;
    }
    let Some(day_time) = level.clock_time(host) else {
        return true;
    };
    let uuid = player.uuid();
    let mut data = read_sleep_data(host, uuid);
    if data.wake_time > day_time {
        data.wake_time = 0;
        data.tired_time = 0;
        write_sleep_data(host, uuid, data);
        return true;
    }
    if data.tired_time > day_time {
        let _ = player.action_bar(
            host,
            Text::translate("capability.comforts.not_sleepy", vec![]),
        );
        return false;
    }
    true
}

/// Mark both halves occupied and place the player onto a Comforts bed with `sleep_timer = 0`.
///
/// `LivingRef::start_sleeping` checks `name.ends_with("_bed")` on bdad86d
/// (gap: sunrise-org/sunrise#5557), so we position the player on the bed surface and set
/// their sleeping position directly; `can_continue_sleeping` then sustains sleep.
fn place_player_in_comforts_bed(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    level: LevelRef,
    head: [i32; 3],
    foot: [i32; 3],
    kind: BedKind,
) {
    let next_head = level
        .block_state(host, head)
        .and_then(|s| s.with(&host.block_states(), "occupied", "true").ok());
    if let Some(next) = next_head {
        let _ = level.set_block(host, head, next, block_flags());
    }
    let next_foot = level
        .block_state(host, foot)
        .and_then(|s| s.with(&host.block_states(), "occupied", "true").ok());
    if let Some(next) = next_foot {
        let _ = level.set_block(host, foot, next, block_flags());
    }

    let sleep_pos = [
        f64::from(head[0]) + 0.5,
        f64::from(head[1]) + kind.sleep_surface_y(),
        f64::from(head[2]) + 0.5,
    ];
    let _ = player.entity().teleport_to(host, sleep_pos, None);
    if let Some(living) = host.living(player.entity()) {
        let _ = living.set_sleeping_pos(host, head);
    }
    let _ = player.set_sleep_timer(host, 0);
}

/// Dry-run or full sleep attempt on a Comforts bed (`BaseComfortsBlock.trySleep`).
/// Returns `Ok(())` when sleep is allowed (and started if `!dry_run`), or `Err(Some(key))`
/// when refused with a user-facing translation key.
fn try_sleep_in_comforts_bed(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    level: LevelRef,
    head: [i32; 3],
    foot: [i32; 3],
    kind: BedKind,
    dry_run: bool,
) -> Result<(), Option<&'static str>> {
    let cfg = server_config();
    let time_use = cfg.time_use_for(kind);
    let bed_can_sleep = level
        .dimension_type(host)
        .map_or_else(|| "when_dark".to_owned(), |d| d.bed_can_sleep.to_owned());
    let bed_can_sleep = bed_can_sleep.as_str();
    let bright = level_is_bright(host, level);
    let thundering = level.is_thundering(host).unwrap_or(false);

    let Some(player_pos) = player.entity().position(host) else {
        return Err(None);
    };
    if !within_bed_reach(player_pos, head, foot) {
        return Err(Some(kind.too_far_key()));
    }
    if has_solid_collision(host, level, [head[0], head[1] + 1, head[2]])
        || has_solid_collision(host, level, [foot[0], foot[1] + 1, foot[2]])
    {
        return Err(Some("block.minecraft.bed.obstructed"));
    }
    if !can_sleep_for_time(time_use, bed_can_sleep, bright, thundering) {
        return Err(Some(time_use.deny_message_key()));
    }
    if player.is_creative(host) != Some(true) && monsters_nearby(host, level, head) {
        return Err(Some("block.minecraft.bed.not_safe"));
    }
    if !check_tired_restriction(host, player, level) {
        return Err(None);
    }

    if dry_run {
        return Ok(());
    }

    // Record sleep start state & optional TIME_SINCE_REST preservation.
    let uuid = player.uuid();
    let mut data = read_sleep_data(host, uuid);
    data.sleep_time = level.clock_time(host).unwrap_or(0);
    data.sleeping_kind = Some(kind);
    if !cfg.stops_phantoms(kind) {
        data.saved_time_since_rest = player.stat(host, StatType::Custom, TIME_SINCE_REST);
    } else {
        data.saved_time_since_rest = None;
        player.reset_stat(host, StatType::Custom, TIME_SINCE_REST);
    }
    write_sleep_data(host, uuid, data);

    place_player_in_comforts_bed(host, player, level, head, foot, kind);
    player.award_stat(host, StatType::Custom, SLEEP_IN_BED, 1);

    Ok(())
}

/// Handle exploding a Comforts bed in dimensions where beds explode (`bed_destroy_on_use`).
fn explode_comforts_bed(
    host: &mut dyn ServerHost,
    level: LevelRef,
    head: [i32; 3],
    foot: [i32; 3],
    block: BlockKey,
    kind: BedKind,
    facing: Direction,
) {
    remove_bed_halves(host, level, head, foot, block);
    if kind == BedKind::Hammock {
        release_hammock_ropes(host, level, head, facing, true);
    }
    let center = [
        f64::from(head[0]) + 0.5,
        f64::from(head[1]) + 0.5,
        f64::from(head[2]) + 0.5,
    ];
    let settings = ExplosionSettings {
        damage_type: Some("minecraft:bad_respawn_point".into()),
        ..Default::default()
    };
    let _ = level.explode(
        host,
        None,
        center,
        5.0,
        true,
        ExplosionInteraction::Block,
        settings,
    );
}

/// Check if a player is attempting to use a Comforts bed (right-clicking either half).
fn use_comforts_bed(
    host: &mut dyn ServerHost,
    event: &UseWithoutItemEvent<'_>,
    kind: BedKind,
) -> InteractionResult {
    let Some(player) = host.player(event.click.player) else {
        return InteractionResult::Pass;
    };
    let Some(level) = player.level(host) else {
        return InteractionResult::Pass;
    };
    let Some((head, foot, facing)) =
        resolve_bed_halves(&host.block_states(), event.click.pos, event.state)
    else {
        return InteractionResult::Consume;
    };
    let Some(head_state) = level.block_state(host, head) else {
        return InteractionResult::Consume;
    };
    let block = event.state.block();
    if head_state.block() != block {
        return InteractionResult::Consume;
    }

    if server_config().time_use_for(kind) == TimeUse::UseWithoutSleeping {
        let uuid = player.uuid();
        let mut data = read_sleep_data(host, uuid);
        data.sleep_time = level.clock_time(host).unwrap_or(0);
        data.sleeping_kind = Some(kind);
        write_sleep_data(host, uuid, data);
        place_player_in_comforts_bed(host, player, level, head, foot, kind);
        return InteractionResult::SuccessServer;
    }

    if level
        .dimension_type(host)
        .is_some_and(|d| d.bed_destroy_on_use)
    {
        explode_comforts_bed(host, level, head, foot, block, kind, facing);
        return InteractionResult::SuccessServer;
    }

    if prop_bool(&host.block_states(), head_state, "occupied") {
        if !kick_sleeping_villager(host, level, head) {
            let _ = player.action_bar(host, Text::translate(kind.occupied_key(), vec![]));
        }
        return InteractionResult::SuccessServer;
    }

    match try_sleep_in_comforts_bed(host, player, level, head, foot, kind, false) {
        Ok(()) => InteractionResult::SuccessServer,
        Err(Some(key)) => {
            let _ = player.action_bar(host, Text::translate(key, vec![]));
            InteractionResult::SuccessServer
        }
        Err(None) => InteractionResult::SuccessServer,
    }
}

/// Shared wake-up logic for sleeping bags & hammocks (`ComfortsEvents.onWakeUp`).
fn handle_comforts_wake_up(host: &mut dyn ServerHost, player: PlayerRef, bed_pos: [i32; 3]) {
    let Some(level) = player.level(host) else {
        return;
    };
    let uuid = player.uuid();
    let mut data = read_sleep_data(host, uuid);
    let state_at_bed = level.block_state(host, bed_pos);
    let block_kind = state_at_bed.and_then(|s| keys().bed_kind(s.block()));
    let kind = block_kind.or(data.sleeping_kind);
    let Some(kind) = kind else {
        return;
    };

    let cfg = server_config();
    let wake_time = level.clock_time(host).unwrap_or(0);
    let time_slept = wake_time.saturating_sub(data.sleep_time);

    // Restore TIME_SINCE_REST when configured not to stop phantoms.
    if let Some(saved_rest) = data.saved_time_since_rest.take() {
        player.reset_stat(host, StatType::Custom, TIME_SINCE_REST);
        if saved_rest > 0 {
            player.award_stat(host, StatType::Custom, TIME_SINCE_REST, saved_rest);
        }
    }

    // Free occupied flag and stand player up beside the bed (since engine `bed_wake_pose`
    // only recognizes `_bed` ids on bdad86d). Guard on `block_kind.is_some()` so a bed
    // already broken by someone else while sleeping never duplicates an auto-used bag.
    if let Some(state) = state_at_bed
        && block_kind.is_some()
    {
        let (facing, free_head) = {
            let meta = host.block_states();
            (
                prop_dir(&meta, state, "facing").unwrap_or(Direction::North),
                state.with(&meta, "occupied", "false").ok(),
            )
        };
        let foot = offset(bed_pos, horizontal_step(facing), -1);
        if let Some(free_head) = free_head {
            let _ = level.set_block(host, bed_pos, free_head, block_flags());
        }
        let free_foot = level
            .block_state(host, foot)
            .filter(|fs| fs.block() == state.block())
            .and_then(|fs| fs.with(&host.block_states(), "occupied", "false").ok());
        if let Some(free_foot) = free_foot {
            let _ = level.set_block(host, foot, free_foot, block_flags());
        }

        let (stand_pos, yaw) = find_stand_up_pose(host, level, bed_pos, facing);
        let _ = player
            .entity()
            .teleport_to(host, stand_pos, Some([yaw, 0.0]));

        if kind == BedKind::SleepingBag {
            let mut broke = false;
            if time_slept > SLEEP_EFFECT_THRESHOLD_TICKS {
                let living = host.living(player.entity());
                if let Some(living) = living {
                    for spec in &cfg.sleeping_bag_effects {
                        if let Some(effect_key) = host.effect_key(&spec.id) {
                            let effect = ActiveEffect::new(
                                effect_key,
                                spec.duration_ticks,
                                spec.amplifier,
                                false,
                                true,
                                true,
                            );
                            living.add_effect(host, &effect);
                        }
                    }
                }
                let luck = living
                    .and_then(|l| l.attribute_value(host, "minecraft:luck"))
                    .unwrap_or(0.0);
                let chance = compute_break_chance(
                    cfg.sleeping_bag_break_chance,
                    luck,
                    cfg.sleeping_bag_break_chance_luck_multiplier,
                );
                let roll = level
                    .with_random(host, |rng| rng.next_double())
                    .unwrap_or(1.0);
                if chance > 0.0 && roll < chance {
                    broke = true;
                    remove_bed_halves(host, level, bed_pos, foot, state.block());
                    let _ = player.action_bar(
                        host,
                        Text::translate("item.comforts.sleeping_bag.broke", vec![]),
                    );
                    let at = [
                        f64::from(bed_pos[0]) + 0.5,
                        f64::from(bed_pos[1]) + 0.5,
                        f64::from(bed_pos[2]) + 0.5,
                    ];
                    level.play_sound(
                        host,
                        "minecraft:block.wool.break",
                        at,
                        SoundSource::Blocks,
                        1.0,
                        1.0,
                        None,
                    );
                }
            }

            // Pick up auto-used sleeping bag if it did not break, preserving any custom name.
            if !broke && data.auto_sleep_pos == Some(bed_pos) {
                let color_idx = keys().sleeping_bag_color(state.block());
                remove_bed_halves(host, level, bed_pos, foot, state.block());
                if player.is_creative(host) != Some(true)
                    && let Some(idx) = color_idx
                {
                    let mut stack = ItemStack::of(keys().sleeping_bag_items[idx]);
                    if let Some(tag) = data.auto_sleep_custom_name.take() {
                        let mut patch = Compound::new();
                        patch.insert("minecraft:custom_name".into(), tag);
                        let _ = stack.apply_components(&patch);
                    }
                    let _ = player.give_stack(host, stack);
                }
            }
        }
    }

    data.wake_time = wake_time;
    data.tired_time = compute_tired_time(wake_time, time_slept, cfg.rest_multiplier);
    data.auto_sleep_pos = None;
    data.auto_sleep_custom_name = None;
    data.sleeping_kind = None;
    write_sleep_data(host, uuid, data);
}

// --- BlockParts ---

struct SleepingBagPart {
    color: usize,
}

#[mod_api::sunrise_part]
impl BlockPart for SleepingBagPart {
    fn use_without_item(
        &self,
        host: HostSide<'_>,
        event: &mut UseWithoutItemEvent<'_>,
    ) -> Option<InteractionResult> {
        let HostSide::Server(host) = host else {
            return None;
        };
        Some(use_comforts_bed(host, event, BedKind::SleepingBag))
    }

    fn state_for_placement(
        &self,
        level: LevelSide,
        event: &mut StateForPlacementEvent<'_>,
    ) -> Option<Option<BlockState>> {
        let dir = horizontal_dir_from_source(event.context.source);
        let pos = event.context.pos;
        let head = offset([pos.x, pos.y, pos.z], horizontal_step(dir), 1);
        let level_host: &dyn LevelHost = match &level {
            LevelSide::Server(l) => *l,
            LevelSide::Client(l) => *l,
        };
        let head_state = level_host.block_state(head)?;
        let meta = level_host.block_states();
        if !meta.replaceable(head_state).unwrap_or(false) {
            return Some(None);
        }
        let waterlogged = is_water_source(event.fluid);
        let state = event
            .state
            .with(&meta, "facing", direction_name(dir))
            .ok()?
            .with(&meta, "part", "foot")
            .ok()?
            .with(&meta, "occupied", "false")
            .ok()?
            .with(
                &meta,
                "waterlogged",
                if waterlogged { "true" } else { "false" },
            )
            .ok()?;
        Some(Some(state))
    }

    fn set_placed_by(&self, host: &mut dyn ServerHost, event: &mut SetPlacedByEvent<'_>) {
        let Some(facing) = prop_dir(&host.block_states(), event.state, "facing") else {
            return;
        };
        let head_pos = offset(event.pos, horizontal_step(facing), 1);
        let head_water = event
            .level
            .fluid_state(host, head_pos)
            .is_some_and(is_water_source);
        let head_state = {
            let meta = host.block_states();
            event
                .state
                .with(&meta, "part", "head")
                .and_then(|s| {
                    s.with(
                        &meta,
                        "waterlogged",
                        if head_water { "true" } else { "false" },
                    )
                })
                .ok()
        };
        if let Some(head_state) = head_state {
            let _ = event
                .level
                .set_block(host, head_pos, head_state, block_flags());
            if let Some(stack) = event.stack {
                copy_custom_name_to_block_entity(host, event.level, head_pos, stack);
            }
        }
    }

    fn update_shape(
        &self,
        level: &mut dyn LevelTicksHost,
        event: &mut UpdateShapeEvent,
    ) -> Option<BlockState> {
        let waterlogged = {
            let meta = level.block_states();
            prop_bool(&meta, event.state, "waterlogged")
        };
        if waterlogged {
            level.schedule_fluid_tick(
                [event.pos.x, event.pos.y, event.pos.z],
                "minecraft:water",
                5,
            );
        }
        let (facing, head_here, partner_ok, partner_occ) = {
            let meta = level.block_states();
            let facing = prop_dir(&meta, event.state, "facing")?;
            let head_here = is_head(&meta, event.state);
            let partner_ok = event.neighbour.block() == event.state.block()
                && is_head(&meta, event.neighbour) != head_here;
            let occ = prop_str(&meta, event.neighbour, "occupied")
                .unwrap_or("false")
                .to_owned();
            (facing, head_here, partner_ok, occ)
        };
        if event.direction == bed_partner_dir(facing, head_here) {
            if partner_ok {
                let meta = level.block_states();
                return event.state.with(&meta, "occupied", &partner_occ).ok();
            }
            return level.block_state_id("minecraft:air");
        }
        Some(event.state)
    }

    fn on_destroyed_by_player(
        &self,
        host: &mut dyn ServerHost,
        event: &mut OnDestroyedByPlayerEvent<'_>,
    ) -> Option<bool> {
        let player = host.player(event.player)?;
        let level = player.level(host)?;
        let (head, foot, _, head_here) = {
            let meta = host.block_states();
            let (head, foot, facing) = resolve_bed_halves(&meta, event.pos, event.state)?;
            (head, foot, facing, is_head(&meta, event.state))
        };
        let creative = player.is_creative(host) == Some(true);
        let block = keys().sleeping_bags[self.color];
        if creative {
            remove_bed_halves(host, level, head, foot, block);
            return Some(true);
        }
        // Let survival break drop the item from the head half (loot table drops on part=head).
        if !head_here {
            let _ = level.destroy_block(host, head, true, None, None, false);
            clear_block_keeping_water(host, level, foot);
            return Some(true);
        }
        None
    }

    fn block_path_type(
        &self,
        _level: &dyn LevelHost,
        _event: &mut BlockPathTypeEvent,
    ) -> Option<PathType> {
        Some(PathType::Walkable)
    }
}

struct HammockPart {
    color: usize,
}

#[mod_api::sunrise_part]
impl BlockPart for HammockPart {
    fn use_without_item(
        &self,
        host: HostSide<'_>,
        event: &mut UseWithoutItemEvent<'_>,
    ) -> Option<InteractionResult> {
        let HostSide::Server(host) = host else {
            return None;
        };
        Some(use_comforts_bed(host, event, BedKind::Hammock))
    }

    fn update_shape(
        &self,
        level: &mut dyn LevelTicksHost,
        event: &mut UpdateShapeEvent,
    ) -> Option<BlockState> {
        let waterlogged = {
            let meta = level.block_states();
            prop_bool(&meta, event.state, "waterlogged")
        };
        if waterlogged {
            level.schedule_fluid_tick(
                [event.pos.x, event.pos.y, event.pos.z],
                "minecraft:water",
                5,
            );
        }
        let (facing, head_here, partner_ok, partner_occ, rope_ok) = {
            let meta = level.block_states();
            let facing = prop_dir(&meta, event.state, "facing")?;
            let head_here = is_head(&meta, event.state);
            let partner_ok = event.neighbour.block() == event.state.block()
                && is_head(&meta, event.neighbour) != head_here;
            let occ = prop_str(&meta, event.neighbour, "occupied")
                .unwrap_or("false")
                .to_owned();
            let rope_ok = event.neighbour.block() == keys().rope_and_nail
                && prop_bool(&meta, event.neighbour, "supporting");
            (facing, head_here, partner_ok, occ, rope_ok)
        };
        if event.direction == bed_partner_dir(facing, head_here) {
            if partner_ok {
                let meta = level.block_states();
                return event.state.with(&meta, "occupied", &partner_occ).ok();
            }
            return level.block_state_id("minecraft:air");
        }
        // If our supporting rope behind this half disappeared, break this hammock half.
        // Returning air lets `Block.updateOrDestroy` call `destroy_block` (dropping the
        // hammock from the head half and preserving water when waterlogged).
        let rope_dir = if head_here { facing } else { facing.opposite() };
        if event.direction == rope_dir && !rope_ok {
            return level.block_state_id("minecraft:air");
        }
        Some(event.state)
    }

    fn on_destroyed_by_player(
        &self,
        host: &mut dyn ServerHost,
        event: &mut OnDestroyedByPlayerEvent<'_>,
    ) -> Option<bool> {
        let player = host.player(event.player)?;
        let level = player.level(host)?;
        let (head, foot, facing, head_here) = {
            let meta = host.block_states();
            let (head, foot, facing) = resolve_bed_halves(&meta, event.pos, event.state)?;
            (head, foot, facing, is_head(&meta, event.state))
        };
        release_hammock_ropes(host, level, event.pos, facing, head_here);
        let creative = player.is_creative(host) == Some(true);
        let block = keys().hammocks[self.color];
        if creative {
            remove_bed_halves(host, level, head, foot, block);
            return Some(true);
        }
        if !head_here {
            let _ = level.destroy_block(host, head, true, None, None, false);
            clear_block_keeping_water(host, level, foot);
            return Some(true);
        }
        None
    }

    fn block_path_type(
        &self,
        _level: &dyn LevelHost,
        _event: &mut BlockPathTypeEvent,
    ) -> Option<PathType> {
        Some(PathType::Walkable)
    }
}

struct RopeAndNailPart;

#[mod_api::sunrise_part]
impl BlockPart for RopeAndNailPart {
    fn can_survive(&self, level: LevelSide, event: &mut CanSurviveEvent) -> Option<bool> {
        let level_host: &dyn LevelHost = match &level {
            LevelSide::Server(l) => *l,
            LevelSide::Client(l) => *l,
        };
        let (dir, supporting) = {
            let meta = level_host.block_states();
            (
                prop_dir(&meta, event.state, "facing")?,
                prop_bool(&meta, event.state, "supporting"),
            )
        };
        let support_pos = offset(
            [event.pos.x, event.pos.y, event.pos.z],
            horizontal_step(dir.opposite()),
            1,
        );
        let sturdy = level_host
            .block_face_sturdy(support_pos, dir_to_face(dir), SupportType::Full)
            .unwrap_or(false);
        if !sturdy {
            return Some(false);
        }
        if supporting {
            let hammock_pos = offset(
                [event.pos.x, event.pos.y, event.pos.z],
                horizontal_step(dir),
                1,
            );
            let has_hammock = level_host
                .block_state(hammock_pos)
                .is_some_and(|s| keys().hammocks.contains(&s.block()));
            return Some(has_hammock);
        }
        Some(true)
    }

    fn state_for_placement(
        &self,
        level: LevelSide,
        event: &mut StateForPlacementEvent<'_>,
    ) -> Option<Option<BlockState>> {
        let face = event.context.face;
        if !matches!(
            face,
            Direction::North | Direction::South | Direction::East | Direction::West
        ) {
            return Some(None);
        }
        let level_host: &dyn LevelHost = match &level {
            LevelSide::Server(l) => *l,
            LevelSide::Client(l) => *l,
        };
        let pos = [
            event.context.pos.x,
            event.context.pos.y,
            event.context.pos.z,
        ];
        let support_pos = offset(pos, horizontal_step(face.opposite()), 1);
        if !level_host
            .block_face_sturdy(support_pos, dir_to_face(face), SupportType::Full)
            .unwrap_or(false)
        {
            return Some(None);
        }
        let meta = level_host.block_states();
        let waterlogged = is_water_source(event.fluid);
        let state = event
            .state
            .with(&meta, "facing", direction_name(face))
            .ok()?
            .with(&meta, "supporting", "false")
            .ok()?
            .with(
                &meta,
                "waterlogged",
                if waterlogged { "true" } else { "false" },
            )
            .ok()?;
        Some(Some(state))
    }

    fn update_shape(
        &self,
        level: &mut dyn LevelTicksHost,
        event: &mut UpdateShapeEvent,
    ) -> Option<BlockState> {
        let (waterlogged, dir, supporting) = {
            let meta = level.block_states();
            (
                prop_bool(&meta, event.state, "waterlogged"),
                prop_dir(&meta, event.state, "facing")?,
                prop_bool(&meta, event.state, "supporting"),
            )
        };
        if waterlogged {
            level.schedule_fluid_tick(
                [event.pos.x, event.pos.y, event.pos.z],
                "minecraft:water",
                5,
            );
        }
        if event.direction == dir.opposite() {
            let support_pos = offset(
                [event.pos.x, event.pos.y, event.pos.z],
                horizontal_step(dir.opposite()),
                1,
            );
            if !level
                .block_face_sturdy(support_pos, dir_to_face(dir), SupportType::Full)
                .unwrap_or(false)
            {
                return level.block_state_id("minecraft:air");
            }
        }
        if event.direction == dir
            && supporting
            && !keys().hammocks.contains(&event.neighbour.block())
        {
            let meta = level.block_states();
            return event.state.with(&meta, "supporting", "false").ok();
        }
        Some(event.state)
    }

    fn on_destroyed_by_player(
        &self,
        host: &mut dyn ServerHost,
        event: &mut OnDestroyedByPlayerEvent<'_>,
    ) -> Option<bool> {
        let dir = {
            let meta = host.block_states();
            if !prop_bool(&meta, event.state, "supporting") {
                return None;
            }
            prop_dir(&meta, event.state, "facing")?
        };
        let player = host.player(event.player)?;
        let level = player.level(host)?;
        let (foot, head, partner_rope) = hammock_layout_from_rope(event.pos, dir);
        if let Some(s) = level.block_state(host, foot)
            && keys().hammocks.contains(&s.block())
        {
            let creative = player.is_creative(host) == Some(true);
            if creative {
                remove_bed_halves(host, level, head, foot, s.block());
            } else {
                let actual_head = if is_head(&host.block_states(), s) {
                    foot
                } else {
                    head
                };
                let actual_foot = if actual_head == head { foot } else { head };
                let _ = level.destroy_block(host, actual_head, true, None, None, false);
                clear_block_keeping_water(host, level, actual_foot);
            }
        }
        release_rope(host, level, partner_rope);
        None
    }
}

// --- ItemParts ---

struct SleepingBagItemPart {
    color: usize,
}

#[mod_api::sunrise_part]
impl ItemPart for SleepingBagItemPart {
    fn tooltip(&self, _host: &dyn ClientHost, event: &mut TooltipEvent<'_, '_>) {
        if client_sync_config().auto_use {
            let _ = event.lines.push((
                Text::translate("item.comforts.sleeping_bag.auto_use.tooltip.0", vec![]),
                GRAY_COLOR,
            ));
            let _ = event.lines.push((
                Text::translate("item.comforts.sleeping_bag.auto_use.tooltip.1", vec![]),
                GRAY_COLOR,
            ));
        }
    }

    fn use_on(&self, host: HostSide, event: &mut UseOnEvent<'_>) -> Option<InteractionResult> {
        let host = match host {
            HostSide::Server(host) => host,
            HostSide::Client(client) => {
                if !client_sync_config().auto_use {
                    return None;
                }
                let sneaking = client
                    .player(event.player)
                    .and_then(|p| p.is_secondary_use_active(client))
                    .unwrap_or(false);
                return if sneaking {
                    None
                } else {
                    Some(InteractionResult::Consume)
                };
            }
        };
        let cfg = server_config();
        let player = host.player(event.player)?;
        if !cfg.auto_use || player.is_secondary_use_active(host) == Some(true) {
            return None;
        }

        let level = player.level(host)?;
        let clicked = event.pos;
        let place_pos = if is_replaceable(host, level, clicked) {
            clicked
        } else {
            [
                clicked[0] + event.face[0],
                clicked[1] + event.face[1],
                clicked[2] + event.face[2],
            ]
        };

        let facing = player
            .entity()
            .direction(host)
            .and_then(parse_direction)
            .unwrap_or(Direction::North);
        let head_pos = offset(place_pos, horizontal_step(facing), 1);

        // Dry-run sleep check before placing the bag (matches SleepingBagItem.useOn).
        if let Err(msg) = try_sleep_in_comforts_bed(
            host,
            player,
            level,
            head_pos,
            place_pos,
            BedKind::SleepingBag,
            true,
        ) {
            if let Some(key) = msg {
                let _ = player.action_bar(host, Text::translate(key, vec![]));
            }
            return Some(InteractionResult::Fail);
        }

        // Verify both cells can hold the sleeping bag.
        if !is_replaceable(host, level, place_pos) || !is_replaceable(host, level, head_pos) {
            return Some(InteractionResult::Fail);
        }

        let block_key = keys().sleeping_bags[self.color];
        let foot_water = level
            .fluid_state(host, place_pos)
            .is_some_and(is_water_source);
        let head_water = level
            .fluid_state(host, head_pos)
            .is_some_and(is_water_source);
        let (foot_state, head_state) = {
            let meta = host.block_states();
            let foot = BlockState::of(
                &meta,
                block_key,
                &[
                    StateConstraint::new("facing", direction_name(facing)),
                    StateConstraint::new("part", "foot"),
                    StateConstraint::new("occupied", "false"),
                    StateConstraint::new("waterlogged", if foot_water { "true" } else { "false" }),
                ],
            )
            .ok()?;
            let head = BlockState::of(
                &meta,
                block_key,
                &[
                    StateConstraint::new("facing", direction_name(facing)),
                    StateConstraint::new("part", "head"),
                    StateConstraint::new("occupied", "false"),
                    StateConstraint::new("waterlogged", if head_water { "true" } else { "false" }),
                ],
            )
            .ok()?;
            (foot, head)
        };

        if !level
            .set_block(host, place_pos, foot_state, block_flags())
            .unwrap_or(false)
        {
            return Some(InteractionResult::Fail);
        }
        let _ = level.set_block(host, head_pos, head_state, block_flags());
        copy_custom_name_to_block_entity(host, level, head_pos, event.stack);

        // Play wool placement sound.
        let at = [
            f64::from(place_pos[0]) + 0.5,
            f64::from(place_pos[1]) + 0.5,
            f64::from(place_pos[2]) + 0.5,
        ];
        level.play_sound(
            host,
            "minecraft:block.wool.place",
            at,
            SoundSource::Blocks,
            1.0,
            0.8,
            None,
        );

        // Consume one sleeping bag in non-creative mode.
        if player.is_creative(host) != Some(true) {
            let slot = match event.hand {
                Hand::Main => player.selected_slot(host).map(|s| s as u32),
                Hand::Off => Some(40),
            };
            if let Some(slot) = slot {
                let mut next_stack = event.stack.clone();
                let remaining = next_stack.count().saturating_sub(1);
                if remaining == 0 {
                    let _ = player.set_inventory_slot(host, slot, None);
                } else if next_stack.set_count(remaining).is_ok() {
                    let _ = player.set_inventory_slot(host, slot, Some(next_stack));
                }
            }
        }

        // Lie down immediately without sleeping when configured as USE_WITHOUT_SLEEPING,
        // before checking bed_destroy_on_use (matches BaseComfortsBlock.useWithoutItem).
        if cfg.time_use_for(BedKind::SleepingBag) == TimeUse::UseWithoutSleeping {
            let uuid = player.uuid();
            let mut data = read_sleep_data(host, uuid);
            data.sleep_time = level.clock_time(host).unwrap_or(0);
            data.sleeping_kind = Some(BedKind::SleepingBag);
            data.auto_sleep_pos = Some(head_pos);
            data.auto_sleep_custom_name = custom_name_tag(event.stack);
            write_sleep_data(host, uuid, data);
            place_player_in_comforts_bed(
                host,
                player,
                level,
                head_pos,
                place_pos,
                BedKind::SleepingBag,
            );
            return Some(InteractionResult::SuccessServer);
        }

        // Explode if the dimension explodes beds, otherwise immediately sleep.
        if level
            .dimension_type(host)
            .is_some_and(|d| d.bed_destroy_on_use)
        {
            explode_comforts_bed(
                host,
                level,
                head_pos,
                place_pos,
                block_key,
                BedKind::SleepingBag,
                facing,
            );
            return Some(InteractionResult::SuccessServer);
        }

        match try_sleep_in_comforts_bed(
            host,
            player,
            level,
            head_pos,
            place_pos,
            BedKind::SleepingBag,
            false,
        ) {
            Ok(()) => {
                // Record auto-sleep position & custom name so waking picks the sleeping bag back up.
                let mut data = read_sleep_data(host, player.uuid());
                data.auto_sleep_pos = Some(head_pos);
                data.auto_sleep_custom_name = custom_name_tag(event.stack);
                write_sleep_data(host, player.uuid(), data);
            }
            Err(Some(key)) => {
                let _ = player.action_bar(host, Text::translate(key, vec![]));
            }
            Err(None) => {}
        }
        Some(InteractionResult::SuccessServer)
    }
}

fn probe_hammock_cell(
    host: &dyn Host,
    level: LevelRef,
    rope_pos: [i32; 3],
    dir: Direction,
    supporting: bool,
    dist: i32,
) -> HammockProbeCell {
    let pos = offset(rope_pos, horizontal_step(dir), dist);
    let Some(state) = level.block_state(host, pos) else {
        return HammockProbeCell::Solid;
    };
    let meta = host.block_states();
    if !supporting
        && state.block() == keys().rope_and_nail
        && prop_dir(&meta, state, "facing") == Some(dir.opposite())
        && !prop_bool(&meta, state, "supporting")
    {
        HammockProbeCell::PartnerRope
    } else if meta.replaceable(state).unwrap_or(false) {
        HammockProbeCell::Clear
    } else {
        HammockProbeCell::Solid
    }
}

struct HammockItemPart {
    color: usize,
}

#[mod_api::sunrise_part]
impl ItemPart for HammockItemPart {
    fn tooltip(&self, _host: &dyn ClientHost, event: &mut TooltipEvent<'_, '_>) {
        let rope_name = Text::translate("item.comforts.rope_and_nail", vec![]);
        let _ = event.lines.push((
            Text::translate("item.comforts.hammock.placement.tooltip", vec![rope_name]),
            GRAY_COLOR,
        ));
    }

    fn use_on(&self, host: HostSide, event: &mut UseOnEvent<'_>) -> Option<InteractionResult> {
        let HostSide::Server(host) = host else {
            return Some(InteractionResult::Consume);
        };
        let player = host.player(event.player)?;
        let level = player.level(host)?;
        let rope_pos = event.pos;
        let rope_state = level.block_state(host, rope_pos)?;

        if rope_state.block() != keys().rope_and_nail {
            let _ = player.action_bar(
                host,
                Text::translate("item.comforts.hammock.no_rope", vec![]),
            );
            return Some(InteractionResult::Fail);
        }

        let (supporting, dir) = {
            let meta = host.block_states();
            (
                prop_bool(&meta, rope_state, "supporting"),
                prop_dir(&meta, rope_state, "facing")?,
            )
        };

        let (foot_pos, head_pos, partner_pos) = hammock_layout_from_rope(rope_pos, dir);
        let has_partner = probe_hammock_cell(host, level, rope_pos, dir, supporting, 3)
            == HammockProbeCell::PartnerRope;

        if !has_partner {
            let key = hammock_placement_error_key(false, |d| {
                probe_hammock_cell(host, level, rope_pos, dir, supporting, d)
            });
            let _ = player.action_bar(host, Text::translate(key, vec![]));
            return Some(InteractionResult::Fail);
        }

        // Check both middle cells are replaceable.
        if !is_replaceable(host, level, foot_pos) || !is_replaceable(host, level, head_pos) {
            let key = hammock_placement_error_key(true, |d| {
                probe_hammock_cell(host, level, rope_pos, dir, supporting, d)
            });
            let _ = player.action_bar(host, Text::translate(key, vec![]));
            return Some(InteractionResult::Fail);
        }

        let partner_state = level.block_state(host, partner_pos);

        let block_key = keys().hammocks[self.color];
        let foot_water = level
            .fluid_state(host, foot_pos)
            .is_some_and(is_water_source);
        let head_water = level
            .fluid_state(host, head_pos)
            .is_some_and(is_water_source);

        let (foot_state, head_state, r1, r2) = {
            let meta = host.block_states();
            let foot = BlockState::of(
                &meta,
                block_key,
                &[
                    StateConstraint::new("facing", direction_name(dir)),
                    StateConstraint::new("part", "foot"),
                    StateConstraint::new("occupied", "false"),
                    StateConstraint::new("waterlogged", if foot_water { "true" } else { "false" }),
                ],
            )
            .ok()?;
            let head = BlockState::of(
                &meta,
                block_key,
                &[
                    StateConstraint::new("facing", direction_name(dir)),
                    StateConstraint::new("part", "head"),
                    StateConstraint::new("occupied", "false"),
                    StateConstraint::new("waterlogged", if head_water { "true" } else { "false" }),
                ],
            )
            .ok()?;
            let r1 = rope_state.with(&meta, "supporting", "true").ok();
            let r2 = partner_state.and_then(|p| p.with(&meta, "supporting", "true").ok());
            (foot, head, r1, r2)
        };

        // Update both ropes to supporting=true first so update_shape on the hammock sees them.
        if let Some(r1) = r1 {
            let _ = level.set_block(host, rope_pos, r1, block_flags());
        }
        if let Some(r2) = r2 {
            let _ = level.set_block(host, partner_pos, r2, block_flags());
        }
        let _ = level.set_block(host, foot_pos, foot_state, block_flags());
        let _ = level.set_block(host, head_pos, head_state, block_flags());
        copy_custom_name_to_block_entity(host, level, head_pos, event.stack);

        let at = [
            f64::from(foot_pos[0]) + 0.5,
            f64::from(foot_pos[1]) + 0.5,
            f64::from(foot_pos[2]) + 0.5,
        ];
        level.play_sound(
            host,
            "minecraft:block.wool.place",
            at,
            SoundSource::Blocks,
            1.0,
            0.8,
            None,
        );

        // Consume one hammock cloth in survival/adventure.
        if player.is_creative(host) != Some(true) {
            let slot = match event.hand {
                Hand::Main => player.selected_slot(host).map(|s| s as u32),
                Hand::Off => Some(40),
            };
            if let Some(slot) = slot {
                let mut next_stack = event.stack.clone();
                let remaining = next_stack.count().saturating_sub(1);
                if remaining == 0 {
                    let _ = player.set_inventory_slot(host, slot, None);
                } else if next_stack.set_count(remaining).is_ok() {
                    let _ = player.set_inventory_slot(host, slot, Some(next_stack));
                }
            }
        }

        Some(InteractionResult::SuccessServer)
    }
}

struct RopeAndNailItemPart;

#[mod_api::sunrise_part]
impl ItemPart for RopeAndNailItemPart {
    fn tooltip(&self, _host: &dyn ClientHost, event: &mut TooltipEvent<'_, '_>) {
        let _ = event.lines.push((
            Text::translate("item.comforts.rope_and_nail.placement.tooltip", vec![]),
            GRAY_COLOR,
        ));
    }
}

// --- Block declarations ---

fn sleeping_bag_shapes() -> BlockShapes {
    BlockShapes {
        properties: Vec::new(),
        values: BTreeMap::new(),
        default: Some(sixteenths(&[[0.0, 0.0, 0.0, 16.0, 3.0, 16.0]])),
    }
}

fn hammock_shapes() -> BlockShapes {
    let north = sixteenths(&[
        [1.0, 0.0, 1.0, 15.0, 1.0, 16.0],
        [0.0, 0.0, 0.0, 16.0, 1.0, 1.0],
    ]);
    let south = sixteenths(&[
        [1.0, 0.0, 0.0, 15.0, 1.0, 15.0],
        [0.0, 0.0, 15.0, 16.0, 1.0, 16.0],
    ]);
    let west = sixteenths(&[
        [1.0, 0.0, 1.0, 16.0, 1.0, 15.0],
        [0.0, 0.0, 0.0, 1.0, 1.0, 16.0],
    ]);
    let east = sixteenths(&[
        [0.0, 0.0, 1.0, 16.0, 1.0, 15.0],
        [15.0, 0.0, 0.0, 16.0, 1.0, 16.0],
    ]);
    let mut values = BTreeMap::new();
    // HammockBlock.getShape uses getConnectedDirection(state).getOpposite():
    // for part=head that equals `facing`, for part=foot that equals `facing.opposite()`.
    values.insert("north,head".into(), north.clone());
    values.insert("south,foot".into(), north);
    values.insert("south,head".into(), south.clone());
    values.insert("north,foot".into(), south);
    values.insert("west,head".into(), west.clone());
    values.insert("east,foot".into(), west);
    values.insert("east,head".into(), east.clone());
    values.insert("west,foot".into(), east);
    BlockShapes {
        properties: vec!["facing".into(), "part".into()],
        values,
        default: Some(sixteenths(&[[1.0, 0.0, 1.0, 15.0, 1.0, 15.0]])),
    }
}

fn rope_and_nail_shapes() -> BlockShapes {
    let mut values = BTreeMap::new();
    // Unsupporting (SHAPES_R)
    values.insert(
        "north,false".into(),
        sixteenths(&[[6.0, 0.0, 12.0, 10.0, 8.0, 16.0]]),
    );
    values.insert(
        "south,false".into(),
        sixteenths(&[[6.0, 0.0, 0.0, 10.0, 8.0, 4.0]]),
    );
    values.insert(
        "west,false".into(),
        sixteenths(&[[12.0, 0.0, 6.0, 16.0, 8.0, 10.0]]),
    );
    values.insert(
        "east,false".into(),
        sixteenths(&[[0.0, 0.0, 6.0, 4.0, 8.0, 10.0]]),
    );
    // Supporting (SHAPES_S)
    values.insert(
        "north,true".into(),
        sixteenths(&[[6.0, 3.0, 9.0, 10.0, 8.0, 16.0]]),
    );
    values.insert(
        "south,true".into(),
        sixteenths(&[[6.0, 3.0, 0.0, 10.0, 8.0, 7.0]]),
    );
    values.insert(
        "west,true".into(),
        sixteenths(&[[9.0, 3.0, 6.0, 16.0, 8.0, 10.0]]),
    );
    values.insert(
        "east,true".into(),
        sixteenths(&[[0.0, 3.0, 6.0, 7.0, 8.0, 10.0]]),
    );
    BlockShapes {
        properties: vec!["facing".into(), "supporting".into()],
        values,
        default: Some(sixteenths(&[[6.0, 0.0, 6.0, 10.0, 8.0, 10.0]])),
    }
}

// --- Mod Hooks ---

#[derive(Default)]
struct ComfortsMod;

#[mod_api::sunrise_mod]
impl Hooks for ComfortsMod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        if let Ok(key) = r.register_attachment_type(
            AttachmentDescriptor::new(ATTACH_SLEEP_DATA, SleepData::default)
                .serialize(AttachmentCodec {
                    encode: |v| Ok(v.to_compound()),
                    decode: |c| Ok(SleepData::from_compound(c)),
                })
                .copy_on_death(),
        ) {
            *SLEEP_DATA.write().unwrap_or_else(|e| e.into_inner()) = Some(key);
        }

        for (kind, default) in [
            (ConfigKind::Common, DEFAULT_COMMON),
            (ConfigKind::Server, DEFAULT_SERVER),
        ] {
            let listener =
                move |event: ConfigEvent<'_>, snapshot: &ConfigSnapshot| -> Result<(), String> {
                    match event {
                        ConfigEvent::Loading(_) => apply_config(kind, &snapshot.text),
                        ConfigEvent::Reloading(host) => {
                            apply_config(kind, &snapshot.text);
                            if kind == ConfigKind::Server
                                && let HostSide::Server(server) = host
                            {
                                sync_config_to_all(server);
                            }
                        }
                        _ => {}
                    }
                    Ok(())
                };
            if let Err(err) =
                r.register_config(ConfigSpec::new(kind, default), Some(Arc::new(listener)))
            {
                eprintln!("comforts: config registration refused: {err:?}");
            }
        }

        let mut items_by_id = HashMap::new();
        let mut tags_by_id = HashMap::new();

        let sleeping_bags = std::array::from_fn(|i| {
            let id = format!("{MOD_ID}:sleeping_bag_{}", COLORS[i]);
            let bk = r.block_key(&id);
            r.block_part(bk, SleepingBagPart { color: i });
            bk
        });
        let sleeping_bag_items = std::array::from_fn(|i| {
            let id = format!("{MOD_ID}:sleeping_bag_{}", COLORS[i]);
            let ik = r.item_key(&id);
            r.item_part(ik, SleepingBagItemPart { color: i });
            items_by_id.insert(id, ik);
            ik
        });
        let hammocks = std::array::from_fn(|i| {
            let id = format!("{MOD_ID}:hammock_{}", COLORS[i]);
            let bk = r.block_key(&id);
            r.block_part(bk, HammockPart { color: i });
            bk
        });
        let hammock_items = std::array::from_fn(|i| {
            let id = format!("{MOD_ID}:hammock_{}", COLORS[i]);
            let ik = r.item_key(&id);
            r.item_part(ik, HammockItemPart { color: i });
            items_by_id.insert(id, ik);
            ik
        });
        let rope_id = format!("{MOD_ID}:rope_and_nail");
        let rope_and_nail = r.block_key(&rope_id);
        let rope_and_nail_item = r.item_key(&rope_id);
        r.block_part(rope_and_nail, RopeAndNailPart);
        r.item_part(rope_and_nail_item, RopeAndNailItemPart);
        items_by_id.insert(rope_id, rope_and_nail_item);

        // Register block entity types for sleeping bags and hammocks so loot tables can
        // preserve `minecraft:custom_name` via `copy_components` from `block_entity`.
        let bag_ids: Vec<String> = COLORS
            .iter()
            .map(|c| format!("{MOD_ID}:sleeping_bag_{c}"))
            .collect();
        let bag_id_refs: Vec<&str> = bag_ids.iter().map(String::as_str).collect();
        let _ = r
            .block_entity_type("sleeping_bag")
            .blocks(&bag_id_refs)
            .build();
        let ham_ids: Vec<String> = COLORS
            .iter()
            .map(|c| format!("{MOD_ID}:hammock_{c}"))
            .collect();
        let ham_id_refs: Vec<&str> = ham_ids.iter().map(String::as_str).collect();
        let _ = r.block_entity_type("hammock").blocks(&ham_id_refs).build();

        let water_fluid = r.fluid_key("minecraft:water");
        let villager_kind = r.entity_type_key("minecraft:villager");

        // Pre-bind vanilla wool items & c: tags used by Comforts recipes.
        for color in COLORS {
            let wool = format!("minecraft:{color}_wool");
            items_by_id.insert(wool.clone(), r.item_key(&wool));
            let dye_tag = format!("c:dyes/{color}");
            tags_by_id.insert(dye_tag.clone(), r.tag_key(&dye_tag));
        }
        let tag_ropes = r.tag_key("c:ropes");
        for tag in ["c:strings", "c:rods/wooden", "c:ingots/iron", "c:ropes"] {
            tags_by_id.insert(tag.to_owned(), r.tag_key(tag));
        }

        let _ = KEYS.set(Keys {
            sleeping_bags,
            sleeping_bag_items,
            hammocks,
            hammock_items,
            rope_and_nail,
            rope_and_nail_item,
            water_fluid,
            villager_kind,
            tag_ropes,
            items_by_id,
            tags_by_id,
        });

        r.register_crafting_recipe_serializer(SHAPED_ID, ConditionalShapedKind);
        r.register_crafting_recipe_serializer(SHAPELESS_ID, ConditionalShapelessKind);

        let mut payloads = r.payload_handlers();
        let mut net = payloads.registrar("1").optional();
        let sync_ch = net.play_to_client_typed::<ClientSyncPayload>(
            PayloadId(CHANNEL_CLIENT_SYNC.into()),
            |payload, _| {
                if let Some(cfg) = ClientSyncConfig::decode(&payload.bytes) {
                    *lock(&CLIENT_SYNC) = cfg;
                }
                Ok(())
            },
        );
        let _ = SYNC_CHANNEL.set(sync_ch);
    }

    fn blocks(&self, _: &RegistrationContext, event: &mut BlocksEvent<'_>) {
        let bag_shapes = sleeping_bag_shapes();
        let ham_shapes = hammock_shapes();
        let rope_shapes = rope_and_nail_shapes();

        let bed_state_props = BTreeMap::from([("waterlogged".into(), BlockStateProperty::Boolean)]);
        let bed_default = BTreeMap::from([("waterlogged".into(), "false".into())]);

        for color in COLORS {
            // Sleeping bag: red_bed template gives facing/part/occupied + bed class.
            let mut bag = Block::new(
                format!("{MOD_ID}:sleeping_bag_{color}"),
                "minecraft:red_bed",
            )
            .with_item(true);
            bag.properties = BlockProperties {
                hardness: Some(0.1),
                explosion_resistance: Some(0.1),
                map_color: Some(MAP_COLOR_WOOL),
                sounds: Some("minecraft:white_wool".into()),
                occludes: Some(false),
                ignited_by_lava: Some(true),
                instrument: Some(NoteInstrument::Guitar),
                ..Default::default()
            };
            bag.state_properties = bed_state_props.clone();
            bag.default_state = bed_default.clone();
            bag.shapes = Some(bag_shapes.clone());
            bag.collision_shapes = Some(bag_shapes.clone());
            let _ = event.register(bag);

            // Hammock: red_bed template + custom waterlogged & hammock shapes.
            let mut ham = Block::new(format!("{MOD_ID}:hammock_{color}"), "minecraft:red_bed")
                .with_item(true);
            ham.properties = BlockProperties {
                hardness: Some(0.1),
                explosion_resistance: Some(0.1),
                map_color: Some(MAP_COLOR_WOOL),
                sounds: Some("minecraft:white_wool".into()),
                occludes: Some(false),
                ignited_by_lava: Some(true),
                instrument: Some(NoteInstrument::Guitar),
                ..Default::default()
            };
            ham.state_properties = bed_state_props.clone();
            ham.default_state = bed_default.clone();
            ham.shapes = Some(ham_shapes.clone());
            ham.collision_shapes = Some(ham_shapes.clone());
            let _ = event.register(ham);
        }

        // Rope and Nail: stone template with facing, supporting, waterlogged, no occlusion.
        let mut rope =
            Block::new(format!("{MOD_ID}:rope_and_nail"), "minecraft:stone").with_item(true);
        rope.properties = BlockProperties {
            hardness: Some(0.2),
            explosion_resistance: Some(0.2),
            map_color: Some(MAP_COLOR_WOOL),
            sounds: Some("minecraft:iron_block".into()),
            occludes: Some(false),
            ignited_by_lava: Some(true),
            requires_tool: Some(false),
            ..Default::default()
        };
        rope.state_properties = BTreeMap::from([
            (
                "facing".into(),
                BlockStateProperty::Enum {
                    values: vec!["north".into(), "south".into(), "west".into(), "east".into()],
                },
            ),
            ("supporting".into(), BlockStateProperty::Boolean),
            ("waterlogged".into(), BlockStateProperty::Boolean),
        ]);
        rope.default_state = BTreeMap::from([
            ("facing".into(), "north".into()),
            ("supporting".into(), "false".into()),
            ("waterlogged".into(), "false".into()),
        ]);
        rope.shapes = Some(rope_shapes.clone());
        rope.collision_shapes = Some(rope_shapes);
        let _ = event.register(rope);
    }

    fn player_logged_in(&self, host: &mut dyn ServerHost, event: &mut PlayerLoggedInEvent<'_>) {
        if let Some(player) = host.player(event.player) {
            send_client_sync(host, player);
        }
    }

    fn player_set_spawn(
        &self,
        host: &mut dyn ServerHost,
        event: &mut PlayerSetSpawnEvent<'_>,
    ) -> Flow {
        let Some(pos) = event.pos else {
            return Flow::Continue;
        };
        let Some(level) = host.level(event.dimension) else {
            return Flow::Continue;
        };
        if level
            .block_state(host, pos)
            .is_some_and(|s| keys().bed_kind(s.block()).is_some())
        {
            return Flow::Cancel;
        }
        Flow::Continue
    }

    fn can_player_sleep(&self, host: &mut dyn ServerHost, event: &mut CanPlayerSleepEvent<'_>) {
        let Some(player) = host.player(event.player) else {
            return;
        };
        let Some(level) = player.level(host) else {
            return;
        };
        // Enforce consecutive sleep restriction on vanilla beds too, matching upstream.
        if event.problem.is_none() && !check_tired_restriction(host, player, level) {
            *event.problem = Some("other_problem".into());
        }
    }

    fn player_bed_enter(
        &self,
        host: &mut dyn ServerHost,
        event: &mut PlayerBedEnterEvent<'_>,
    ) -> Flow {
        let Some(player) = host.player(event.player) else {
            return Flow::Continue;
        };
        let Some(level) = player.level(host) else {
            return Flow::Continue;
        };
        let mut data = read_sleep_data(host, event.player);
        data.sleep_time = level.clock_time(host).unwrap_or(0);
        write_sleep_data(host, event.player, data);
        Flow::Continue
    }

    fn can_continue_sleeping(
        &self,
        host: &mut dyn ServerHost,
        event: &mut CanContinueSleepingEvent<'_>,
    ) -> Option<bool> {
        if event.kind != "minecraft:player" {
            return None;
        }
        let player = host.player(event.entity)?;
        let level = player.level(host)?;
        let state = level.block_state(host, event.pos)?;
        let kind = keys().bed_kind(state.block())?;
        // Ensure bed head is still present.
        if !is_head(&host.block_states(), state) {
            return Some(false);
        }
        let cfg = server_config();
        let time_use = cfg.time_use_for(kind);
        let bed_can_sleep = level
            .dimension_type(host)
            .map_or_else(|| "when_dark".to_owned(), |d| d.bed_can_sleep.to_owned());
        let bed_can_sleep = bed_can_sleep.as_str();
        let bright = level_is_bright(host, level);
        let thundering = level.is_thundering(host).unwrap_or(false);
        let allowed = can_sleep_for_time(time_use, bed_can_sleep, bright, thundering);
        // If vanilla passed `NotPossibleHere` (because engine `is_bed` only checks `_bed`,
        // gap: sunrise-org/sunrise#5557) or `NotPossibleNow` (daytime hammock), answer based
        // on Comforts' own block & time-of-day rules.
        match event.problem {
            Some(SleepProblem::NotPossibleHere) | Some(SleepProblem::NotPossibleNow) | None => {
                Some(allowed)
            }
            _ => Some(allowed),
        }
    }

    fn player_tick_post(&self, host: HostSide, event: &mut PlayerTickPostEvent) {
        let HostSide::Server(host) = host else {
            return;
        };
        let player = event.player;
        let Some(living) = host.living(player.entity()) else {
            return;
        };
        let Some(bed_pos) = living.sleeping_pos(host) else {
            return;
        };
        let Some(level) = player.level(host) else {
            return;
        };
        let Some(state) = level.block_state(host, bed_pos) else {
            return;
        };
        let Some(kind) = keys().bed_kind(state.block()) else {
            return;
        };
        if server_config().time_use_for(kind) == TimeUse::UseWithoutSleeping {
            let _ = player.set_sleep_timer(host, 0);
        }
    }

    fn sleepers_needed(&self, host: &dyn Host, event: &mut SleepersNeededEvent<'_>) {
        let cfg = server_config();
        if cfg.day_sleeping_percentage < 0 {
            return;
        }
        let Some(level) = host.level(event.dimension) else {
            return;
        };
        if !level_is_bright(host, level) {
            return;
        }
        // `needed` starts at vanilla's count for `players_sleeping_percentage`; daytime uses
        // `daySleepingPercentage` instead, above or below it. The engine's night-skip check and
        // sleep status line both read the answer, so no separate skip veto is needed.
        event.needed = sleepers_needed(event.active, cfg.day_sleeping_percentage);
    }

    fn sleep_status_announce(
        &self,
        host: &mut dyn ServerHost,
        event: &mut SleepStatusAnnounceEvent<'_>,
    ) -> Flow {
        if event.active <= 1 {
            return Flow::Continue;
        }
        let Some(level) = host.level(event.dimension) else {
            return Flow::Continue;
        };
        if !level_is_bright(host, level) {
            return Flow::Continue;
        }
        // Daytime: replace vanilla's line with Comforts' `comforts.skipping_day` for everyone.
        let mut recipients = Vec::new();
        let _ = level.visit_online_players(host, &mut |p| {
            recipients.push(p);
            ControlFlow::Continue(())
        });
        let msg = Text::translate("comforts.skipping_day", vec![]);
        for p in recipients {
            let _ = p.action_bar(host, msg.clone());
        }
        Flow::Cancel
    }

    fn sleep_finished_time(
        &self,
        host: &mut dyn ServerHost,
        event: &mut SleepFinishedTimeEvent<'_>,
    ) -> Flow {
        let Some(level) = host.level(event.dimension) else {
            return Flow::Continue;
        };
        let cfg = server_config();
        let mut day_sleeping = false;
        let _ = level.visit_online_players(host, &mut |player| {
            if player.sleep_timer(host).unwrap_or(0) >= 100
                && let Some(living) = host.living(player.entity())
                && let Some(bed_pos) = living.sleeping_pos(host)
                && let Some(state) = level.block_state(host, bed_pos)
                && let Some(kind) = keys().bed_kind(state.block())
                && cfg.time_use_for(kind).allows_day_skip()
            {
                day_sleeping = true;
                return ControlFlow::Break(());
            }
            ControlFlow::Continue(())
        });

        let wake_from_nap = day_sleeping && level_is_bright(host, level);
        let marker_id = if wake_from_nap {
            NAP_WAKE_MARKER
        } else {
            OVERWORLD_WAKE_MARKER
        };
        let marker_tick = Identifier::parse(marker_id)
            .ok()
            .and_then(|id| level.clock_marker_time(host, &id));

        if wake_from_nap || cfg.day_wake_time_offset != 0 {
            let target = compute_wake_time(
                event.current,
                wake_from_nap,
                marker_tick,
                cfg.day_wake_time_offset,
                cfg.night_wake_time_offset,
            );
            *event.adjustment = ClockAdjustment::Absolute(target);
        }

        Flow::Continue
    }

    fn player_bed_leave(&self, host: &mut dyn ServerHost, event: &mut PlayerBedLeaveEvent<'_>) {
        let Some(player) = host.player(event.player) else {
            return;
        };
        let Some(level) = player.level(host) else {
            return;
        };
        let is_comforts_bed = level
            .block_state(host, event.pos)
            .is_some_and(|s| keys().bed_kind(s.block()).is_some())
            || read_sleep_data(host, event.player).sleeping_kind.is_some();

        if is_comforts_bed {
            handle_comforts_wake_up(host, player, event.pos);
        } else {
            // Update wake/tired timestamps for vanilla beds too when restrictSleeping is on.
            let cfg = server_config();
            let wake_time = level.clock_time(host).unwrap_or(0);
            let mut data = read_sleep_data(host, event.player);
            let time_slept = wake_time.saturating_sub(data.sleep_time);
            data.wake_time = wake_time;
            data.tired_time = compute_tired_time(wake_time, time_slept, cfg.rest_multiplier);
            write_sleep_data(host, event.player, data);
        }
    }

    fn screen_key_pressed_post(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenKeyPressedPostEvent<'_>,
    ) -> Flow {
        // InBedChatScreen reports screen.id == "chat"; check that the local player is actually
        // sleeping in a Comforts bed configured with USE_WITHOUT_SLEEPING.
        if event.screen.id != "chat" || event.repeat {
            return Flow::Continue;
        }
        let Some(player) = host.local_player() else {
            return Flow::Continue;
        };
        let Some(living) = host.living(player.entity()) else {
            return Flow::Continue;
        };
        let Some(bed_pos) = living.sleeping_pos(host) else {
            return Flow::Continue;
        };
        let Some(level) = host.level(host.dimension()) else {
            return Flow::Continue;
        };
        let Some(state) = level.block_state(host, bed_pos) else {
            return Flow::Continue;
        };
        let Some(kind) = keys().bed_kind(state.block()) else {
            return Flow::Continue;
        };
        if client_sync_config().time_use_for(kind) != TimeUse::UseWithoutSleeping {
            return Flow::Continue;
        }
        let Ok(mapping) = host.vanilla_key_mapping(KEY_TOGGLE_PERSPECTIVE) else {
            return Flow::Continue;
        };
        if mapping
            .binding(host)
            .is_ok_and(|binding| binding == event.key)
        {
            host.set_camera_type(host.camera_type().cycle());
            return Flow::Cancel;
        }
        Flow::Continue
    }

    fn build_creative_mode_tab_contents(
        &self,
        _host: HostSide,
        event: &mut BuildCreativeModeTabContentsEvent<'_>,
    ) {
        let is_colored = event.tab == "minecraft:colored_blocks";
        let is_functional = event.tab == "minecraft:functional_blocks";
        if !is_colored && !is_functional {
            return;
        }
        let k = keys();
        for item in k.sleeping_bag_items.iter().chain(k.hammock_items.iter()) {
            let _ = event.output.insert(
                CreativeTabPlacement::Last,
                ItemStack::of(*item),
                CreativeTabVisibility::Both,
            );
        }
        if is_functional {
            let _ = event.output.insert(
                CreativeTabPlacement::Last,
                ItemStack::of(k.rope_and_nail_item),
                CreativeTabVisibility::Both,
            );
        }
    }
}
