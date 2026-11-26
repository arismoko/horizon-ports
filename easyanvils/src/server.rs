//! Server-side anvil block part, menu part, viewer synchronization, iron-block anvil repair,
//! and dispenser automation.

use crate::anvil::{
    AnvilResult, compute_anvil_result, damaged_anvil_state, is_anvil_state, is_unaltered_anvil,
    repaired_anvil_state,
};
use crate::config::server_config;
use crate::formatting::{filter_text, visible_length};
use crate::payloads::{RenameItemPayload, channels};
use crate::storage::{
    AnvilSlots, comparator_signal, drop_and_clear_anvil_slots, read_anvil_slots,
    read_anvil_slots_in, write_anvil_slots,
};
use mod_api::item::menu::{AnvilUpdateInputs, StackResult};
use mod_api::{
    AnalogOutputEvent, BlockKey, BlockPart, BlockState, BlockUpdateFlags, DispenseMethodEvent,
    Dispensed, GameMode, HasAnalogOutputEvent, Host, HostSide, HostTransaction, IPayloadContext,
    InteractionResult, ItemPlace, ItemResource, ItemStack, LevelHost, LevelRandomHost, LevelRef,
    LevelSide, LoopContext, MenuContainer, MenuKey, MenuPart, MenuRef, MenuRemovedEvent,
    MenuReplace, MenuSlotChangedEvent, MenuSlotRulesEvent, MenuSlotTakeEvent, MenuStillValidEvent,
    ModMenuOpen, NeighbourUpdate, OnBlockExplodedEvent, OnBlockStateChangeEvent,
    OnDestroyedByPlayerEvent, ParticleKey, ParticleOptions, PlayerRef, ServerHost, StatType, Text,
    UseItemOnEvent, UseWithoutItemEvent,
};
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock, PoisonError};

pub const MENU_ID: &str = "easyanvils:anvil";

/// Vanilla `LevelEvent.SOUND_ANVIL_BROKEN`.
const LEVEL_EVENT_ANVIL_BROKEN: i32 = 1029;
/// Vanilla `LevelEvent.SOUND_ANVIL_USED`.
const LEVEL_EVENT_ANVIL_USED: i32 = 1030;
/// Vanilla `FriendlyByteBuf.MAX_STRING_LENGTH` (used by `ServerboundRenameItemPacket` via
/// `FriendlyByteBuf.readUtf()`). Easy Anvils uses vanilla's `ServerboundRenameItemPacket`
/// unchanged.
const MAX_PACKET_NAME_CHARS: usize = 32_767;
/// Vanilla `Utf8String.read` / `ByteBufUtil.utf8MaxBytes(FriendlyByteBuf.MAX_STRING_LENGTH)`
/// (`32767 * 3`).
const MAX_PACKET_NAME_BYTES: usize = MAX_PACKET_NAME_CHARS * 3;
/// Vanilla `AnvilMenu.MAX_NAME_LENGTH` (also used by Easy Anvils' `ModAnvilMenu.setItemName`
/// via `FormattedStringUtil.stringLength(itemName) <= 50`).
const MAX_NAME_LENGTH: usize = 50;

static ANVIL_KEYS: OnceLock<[BlockKey; 3]> = OnceLock::new();
static BLOCK_PARTICLE: OnceLock<ParticleKey> = OnceLock::new();

pub fn init_anvil_keys(keys: [BlockKey; 3]) {
    let _ = ANVIL_KEYS.set(keys);
}

pub fn init_block_particle(key: ParticleKey) {
    let _ = BLOCK_PARTICLE.set(key);
}

fn is_known_anvil_key(key: BlockKey) -> bool {
    ANVIL_KEYS.get().is_some_and(|keys| keys.contains(&key))
}

fn find_menu_key(host: &dyn Host) -> Option<MenuKey> {
    host.menu_keys()
        .into_iter()
        .find(|k| k.id(host).is_some_and(|n| n.as_str() == MENU_ID))
}

fn notify_comparator(host: &mut dyn ServerHost, level: LevelRef, pos: [i32; 3]) {
    if let Some(state) = level.block_state(&*host, pos) {
        let _ = level.update_neighbors(host, pos, state, NeighbourUpdate::OutputSignal);
    }
}

fn next_unit_float(player: u128, pos: [i32; 3]) -> f64 {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut x = (player as u64)
        ^ ((player >> 64) as u64).rotate_left(17)
        ^ ((pos[0] as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
        ^ ((pos[1] as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9))
        ^ ((pos[2] as u64).wrapping_mul(0x94D0_49BB_1331_11EB))
        ^ n.wrapping_mul(0x517C_C1B7_2722_0A95);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    ((x >> 11) as f64) / ((1u64 << 53) as f64)
}

#[derive(Clone, Debug)]
struct ViewerEntry {
    window: i32,
    dimension: String,
    pos: [i32; 3],
    /// `None` = keep the input item's existing name; `Some("")` = strip custom name.
    item_name: Option<String>,
    cost: i32,
    repair_item_count_cost: u32,
    only_renaming: bool,
    can_pickup: bool,
    still_valid: bool,
}

type AnvilKey = (String, [i32; 3]);

static VIEWERS: LazyLock<Mutex<HashMap<u128, ViewerEntry>>> = LazyLock::new(Mutex::default);
static DIRTY_ANVILS: LazyLock<Mutex<HashMap<AnvilKey, i32>>> = LazyLock::new(Mutex::default);
static PENDING_DROPS: LazyLock<Mutex<Vec<AnvilKey>>> = LazyLock::new(Mutex::default);

pub fn reset_server_state() {
    VIEWERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    DIRTY_ANVILS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    PENDING_DROPS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
}

pub fn mark_anvil_dirty(dimension: &str, pos: [i32; 3], prev_signal: i32) {
    DIRTY_ANVILS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry((dimension.to_owned(), pos))
        .or_insert(prev_signal);
}

fn clear_anvil_dirty(dimension: &str, pos: [i32; 3]) {
    DIRTY_ANVILS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&(dimension.to_owned(), pos));
}

fn invalidate_viewers_at(dimension: &str, pos: [i32; 3]) {
    let mut map = VIEWERS.lock().unwrap_or_else(PoisonError::into_inner);
    for entry in map.values_mut() {
        if entry.dimension == dimension && entry.pos == pos {
            entry.still_valid = false;
        }
    }
}

fn viewers_at(dimension: &str, pos: [i32; 3]) -> Vec<(u128, i32)> {
    VIEWERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|(_, e)| e.still_valid && e.dimension == dimension && e.pos == pos)
        .map(|(&uuid, e)| (uuid, e.window))
        .collect()
}

fn viewer_of(uuid: u128) -> Option<ViewerEntry> {
    VIEWERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&uuid)
        .cloned()
}

thread_local! {
    /// Allows server-side updates to slot 2 via `ItemPlace::MenuSlot` to bypass
    /// `menu_slot_rules`'s player-facing `may_place = false` and `may_pickup` gates.
    static WRITING_OUTPUT_SLOT: Cell<bool> = const { Cell::new(false) };
}

struct OutputSlotWriteGuard(bool);

impl OutputSlotWriteGuard {
    fn enter() -> Self {
        let prev = WRITING_OUTPUT_SLOT.replace(true);
        Self(prev)
    }
}

impl Drop for OutputSlotWriteGuard {
    fn drop(&mut self) {
        WRITING_OUTPUT_SLOT.set(self.0);
    }
}

/// Replaces all 3 owned slots `[input0, input1, output]` of an open `easyanvils:anvil` menu.
fn set_menu_all_slots(host: &mut dyn ServerHost, menu: MenuRef, desired: &[Option<ItemStack>; 3]) {
    let _guard = OutputSlotWriteGuard::enter();
    let _ = host.transaction(|host, tx| {
        for (slot, target) in desired.iter().enumerate() {
            let place = ItemPlace::MenuSlot { menu, slot };
            if let Some(current_res) = place.resource(host) {
                let cur_amt = place.amount(host);
                if cur_amt > 0 {
                    let _ = place.extract(host, tx, &current_res, cur_amt);
                }
            }
            if let Some(stack) = target {
                let res = ItemResource::of(stack);
                let _ = place.insert(host, tx, &res, stack.count());
            }
        }
        Ok::<(), ()>(())
    });
}

/// Replaces only slot 2 (output preview) of an open `easyanvils:anvil` menu.
fn set_menu_output_slot(host: &mut dyn ServerHost, menu: MenuRef, output: Option<&ItemStack>) {
    let _guard = OutputSlotWriteGuard::enter();
    let _ = host.transaction(|host, tx| {
        let place = ItemPlace::MenuSlot { menu, slot: 2 };
        if let Some(current_res) = place.resource(host) {
            let cur_amt = place.amount(host);
            if cur_amt > 0 {
                let _ = place.extract(host, tx, &current_res, cur_amt);
            }
        }
        if let Some(stack) = output {
            let res = ItemResource::of(stack);
            let _ = place.insert(host, tx, &res, stack.count());
        }
        Ok::<(), ()>(())
    });
}

/// The player's open menu when it is the window this viewer was opened with.
fn live_menu(host: &dyn ServerHost, uuid: u128, window: i32) -> Option<MenuRef> {
    let menu = host.player(uuid)?.container_menu(host)?;
    (menu.container_id(host) == Some(window)).then_some(menu)
}

fn read_menu_input_slots(host: &dyn Host, menu: MenuRef) -> [Option<ItemStack>; 2] {
    [menu.slot(host, 0).flatten(), menu.slot(host, 1).flatten()]
}

fn can_player_pickup(player: PlayerRef, host: &dyn ServerHost, result: &AnvilResult) -> bool {
    if result.output.is_none() || result.cost < 0 {
        return false;
    }
    let creative = player.game_mode(host) == Some(GameMode::Creative);
    if creative {
        return true;
    }
    let level = player.experience(host).map_or(0, |e| e.level);
    level >= result.cost
}

/// Runs every mod's `anvil_update` listeners over this menu's own result, as NeoForge's
/// `CommonHooks.onAnvilUpdate` does for a menu that is not vanilla's `AnvilMenu`, and applies
/// what they left: a replaced output, a changed level cost and material cost, or a cleared
/// result when one canceled.
fn apply_anvil_update(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    pos: [i32; 3],
    inputs: &[Option<ItemStack>; 2],
    item_name: Option<&str>,
    result: AnvilResult,
) -> AnvilResult {
    let Some(left) = inputs[0].as_ref() else {
        return result;
    };
    let vanilla = AnvilUpdateInputs {
        left,
        right: inputs[1].as_ref(),
        name: item_name,
        vanilla_output: result.output.as_ref(),
        vanilla_xp_cost: result.cost.max(0),
        vanilla_material_cost: i32::try_from(result.repair_item_count_cost).unwrap_or(i32::MAX),
    };
    let Some(edits) = player.anvil_update(host, pos, &vanilla) else {
        return AnvilResult::empty(0);
    };
    AnvilResult {
        output: match edits.output {
            StackResult::Keep => result.output,
            StackResult::Set(stack) => stack,
        },
        cost: edits.xp_cost.max(0),
        repair_item_count_cost: edits.material_cost.max(0).unsigned_abs(),
        only_renaming: result.only_renaming,
    }
}

fn recompute_viewer(
    host: &mut dyn ServerHost,
    player_uuid: u128,
    window: i32,
    inputs: &[Option<ItemStack>; 2],
    write_inputs: bool,
) {
    let Some(player) = host.player(player_uuid) else {
        return;
    };
    let Some(menu) = live_menu(&*host, player_uuid, window) else {
        return;
    };
    let item_name = viewer_of(player_uuid).and_then(|v| v.item_name);
    let creative = player.game_mode(&*host) == Some(GameMode::Creative);
    let cfg = server_config();

    let result = compute_anvil_result(
        &*host,
        inputs[0].as_ref(),
        inputs[1].as_ref(),
        item_name.as_deref(),
        creative,
        &cfg,
    );
    let pos = viewer_of(player_uuid).map_or([0; 3], |v| v.pos);
    let result = apply_anvil_update(host, player, pos, inputs, item_name.as_deref(), result);
    let pickup = can_player_pickup(player, &*host, &result);

    if write_inputs {
        let all = [inputs[0].clone(), inputs[1].clone(), result.output.clone()];
        set_menu_all_slots(host, menu, &all);
    } else {
        set_menu_output_slot(host, menu, result.output.as_ref());
    }
    let _ = menu.set_data_slot(host, 0, result.cost);

    if let Some(entry) = VIEWERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_mut(&player_uuid)
        && entry.window == window
    {
        entry.cost = result.cost;
        entry.repair_item_count_cost = result.repair_item_count_cost;
        entry.only_renaming = result.only_renaming;
        entry.can_pickup = pickup;
    }
}

fn sync_viewers_for_anvil(
    host: &mut dyn ServerHost,
    dimension: &str,
    pos: [i32; 3],
    inputs: &[Option<ItemStack>; 2],
    exclude_player: Option<u128>,
) {
    for (viewer_uuid, window) in viewers_at(dimension, pos) {
        let write_inputs = exclude_player != Some(viewer_uuid);
        recompute_viewer(host, viewer_uuid, window, inputs, write_inputs);
    }
}

pub fn send_server_config_to(host: &mut dyn ServerHost, player: PlayerRef) {
    if let Some(ch) = channels() {
        let _ = ch.server_config.send(host, player, &server_config());
    }
}

pub fn broadcast_server_config(host: &mut dyn ServerHost) {
    if let Some(ch) = channels() {
        let _ = ch.server_config.send_to_all(host, &server_config());
    }
}

/// Repairs a chipped or damaged anvil by one stage, playing the anvil-use sound and block-break
/// particles without playing a block-break sound (`ItemInteractionHandler.tryRepairAnvil`).
pub fn try_repair_anvil(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    state: BlockState,
) -> bool {
    let Some(repaired) = repaired_anvil_state(&*host, state) else {
        return false;
    };
    if level
        .set_block(host, pos, repaired, BlockUpdateFlags::CLIENTS)
        .ok()
        != Some(true)
    {
        return false;
    }
    let _ = level.level_event(host, LEVEL_EVENT_ANVIL_USED, pos, 0);
    if let Some(&particle_key) = BLOCK_PARTICLE.get() {
        let _ = level.send_particles(
            host,
            &ParticleOptions::simple(particle_key),
            Some(repaired),
            [
                f64::from(pos[0]) + 0.5,
                f64::from(pos[1]) + 0.5,
                f64::from(pos[2]) + 0.5,
            ],
            [0.35, 0.35, 0.35],
            0.15,
            24,
        );
    }
    true
}

/// Validates an incoming rename string against vanilla's `ServerboundRenameItemPacket` /
/// `FriendlyByteBuf.readUtf()` length cap (`FriendlyByteBuf.MAX_STRING_LENGTH` = 32767 UTF-16
/// code units, `32767 * 3` UTF-8 bytes) and `AnvilMenu.validateName` / Easy Anvils'
/// `ModAnvilMenu.setItemName` 50-character limit (`AnvilMenu.MAX_NAME_LENGTH`).
fn validate_rename_name(name: &str, allow_formatting: bool) -> Option<String> {
    if name.len() > MAX_PACKET_NAME_BYTES || name.encode_utf16().count() > MAX_PACKET_NAME_CHARS {
        return None;
    }
    let filtered = filter_text(name, allow_formatting);
    if visible_length(&filtered, allow_formatting) > MAX_NAME_LENGTH {
        return None;
    }
    Some(filtered.trim().to_owned())
}

/// Handles `easyanvils:rename_item` sent by the client's rename box.
pub fn handle_rename_item(
    payload: RenameItemPayload,
    ctx: &mut dyn IPayloadContext,
) -> Result<(), String> {
    let Ok(player) = ctx.player() else {
        return Ok(());
    };
    let player_uuid = player.uuid();
    let Some(viewer) = viewer_of(player_uuid) else {
        return Ok(());
    };
    if !viewer.still_valid || viewer.window != payload.window {
        return Ok(());
    }

    let cfg = server_config();
    let Some(trimmed) = validate_rename_name(&payload.name, cfg.renaming_supports_formatting)
    else {
        return Ok(());
    };

    {
        let mut map = VIEWERS.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(entry) = map.get_mut(&player_uuid)
            && entry.window == payload.window
        {
            if entry.item_name.as_deref() == Some(trimmed.as_str()) {
                return Ok(());
            }
            entry.item_name = Some(trimmed);
        } else {
            return Ok(());
        }
    }

    let Ok(host) = ctx.server_host() else {
        return Ok(());
    };
    let Some(menu) = live_menu(&*host, player_uuid, payload.window) else {
        return Ok(());
    };
    let inputs = read_menu_input_slots(&*host, menu);
    recompute_viewer(host, player_uuid, payload.window, &inputs, false);
    Ok(())
}

/// `BlockPart` attached to `minecraft:anvil`, `minecraft:chipped_anvil`, and `minecraft:damaged_anvil`.
pub struct AnvilBlockPart;

#[mod_api::sunrise_part]
impl BlockPart for AnvilBlockPart {
    fn use_item_on(
        &self,
        host: HostSide<'_>,
        event: &mut UseItemOnEvent<'_>,
    ) -> Option<InteractionResult> {
        let HostSide::Server(host) = host else {
            return None;
        };
        let cfg = server_config();
        if !cfg.anvil_repairing {
            return None;
        }
        let stack = event.click.stack?;
        if stack.item().id(&*host) != Some("minecraft:iron_block") {
            return None;
        }
        let player = host.player(event.click.player)?;
        let dim = event.click.dimension.key().id().to_owned();
        let level = host.level(&dim)?;
        if !try_repair_anvil(host, level, event.click.pos, event.state) {
            return None;
        }
        let creative = player.game_mode(&*host) == Some(GameMode::Creative);
        if !creative {
            let player_uuid = event.click.player;
            let hand = event.click.hand;
            let res = ItemResource::of(stack);
            let _ = host.transaction(|host, tx| {
                let place = ItemPlace::PlayerHand {
                    player: player_uuid,
                    hand,
                };
                let _ = place.extract(host, tx, &res, 1);
                Ok::<(), ()>(())
            });
        }
        Some(InteractionResult::Success)
    }

    fn use_without_item(
        &self,
        host: HostSide<'_>,
        event: &mut UseWithoutItemEvent<'_>,
    ) -> Option<InteractionResult> {
        let HostSide::Server(host) = host else {
            return None;
        };
        let cfg = server_config();
        if !cfg.convert_vanilla_anvil_when_interacting {
            return None;
        }
        if is_unaltered_anvil(&*host, event.state.block()) {
            return None;
        }
        let mkey = find_menu_key(&*host)?;
        let player_uuid = event.click.player;
        let pos = event.click.pos;
        let dim = event.click.dimension.key().id().to_owned();
        let player = host.player(player_uuid)?;

        let stored = read_anvil_slots(host, &dim, pos);
        let inputs = stored.to_stacks(&*host);
        let creative = player.game_mode(&*host) == Some(GameMode::Creative);
        let initial = compute_anvil_result(
            &*host,
            inputs[0].as_ref(),
            inputs[1].as_ref(),
            None,
            creative,
            &cfg,
        );
        let initial = apply_anvil_update(host, player, pos, &inputs, None, initial);
        let pickup = can_player_pickup(player, &*host, &initial);

        let title = Text::translate("container.repair", Vec::new());
        let mut open = ModMenuOpen::new(mkey, title);
        open.data = vec![initial.cost];
        open.pos = Some(pos);
        open.replace = MenuReplace::CloseFirst;

        let Ok(Some(_)) = player.open_mod_menu(host, open) else {
            return None;
        };

        let _ = player.award_stat(host, StatType::Custom, "minecraft:interact_with_anvil", 1);

        // The returned handle may name only a staged open; read the installed window instead.
        let installed = player
            .container_menu(&*host)
            .and_then(|menu| Some((menu, menu.container_id(&*host)?)));
        let Some((menu, window)) = installed else {
            return Some(InteractionResult::Success);
        };
        let all = [inputs[0].clone(), inputs[1].clone(), initial.output.clone()];
        set_menu_all_slots(host, menu, &all);

        VIEWERS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                player_uuid,
                ViewerEntry {
                    window,
                    dimension: dim,
                    pos,
                    item_name: None,
                    cost: initial.cost,
                    repair_item_count_cost: initial.repair_item_count_cost,
                    only_renaming: initial.only_renaming,
                    can_pickup: pickup,
                    still_valid: true,
                },
            );

        Some(InteractionResult::Success)
    }

    fn dispense_method(
        &self,
        host: &mut dyn ServerHost,
        event: &mut DispenseMethodEvent<'_>,
    ) -> Option<Dispensed> {
        let cfg = server_config();
        if !cfg.anvil_repairing {
            return None;
        }
        if event.stack.item().id(&*host) != Some("minecraft:iron_block") {
            return None;
        }
        let level = host.level(event.dimension)?;
        let n = event.facing.normal();
        let anvil_pos = [
            event.pos[0] + n[0],
            event.pos[1] + n[1],
            event.pos[2] + n[2],
        ];
        if try_repair_anvil(host, level, anvil_pos, event.state) {
            Some((event.stack.clone().shrink(1), 1000))
        } else {
            // Intact anvil: fail sound, keep item (OptionalDispenseItemBehavior setSuccess(false))
            Some((Some(event.stack.clone()), 1001))
        }
    }

    fn has_analog_output(&self, _level: &dyn LevelHost, event: &mut HasAnalogOutputEvent) {
        event.output = true;
    }

    fn analog_output(&self, level: &mut dyn LevelRandomHost, event: &mut AnalogOutputEvent) {
        let pos = [event.pos.x, event.pos.y, event.pos.z];
        let slots = read_anvil_slots_in(level, pos);
        event.output = comparator_signal(&slots);
    }

    fn on_destroyed_by_player(
        &self,
        host: &mut dyn ServerHost,
        event: &mut OnDestroyedByPlayerEvent<'_>,
    ) -> Option<bool> {
        let dim = host.dimension().to_owned();
        invalidate_viewers_at(&dim, event.pos);
        clear_anvil_dirty(&dim, event.pos);
        if let Some(level) = host.level(&dim) {
            drop_and_clear_anvil_slots(host, level, &dim, event.pos);
        }
        None
    }

    fn on_block_exploded(&self, host: &mut dyn ServerHost, event: &mut OnBlockExplodedEvent<'_>) {
        let pos = [event.pos.x, event.pos.y, event.pos.z];
        let dim = host.dimension().to_owned();
        invalidate_viewers_at(&dim, pos);
        clear_anvil_dirty(&dim, pos);
        if let Some(level) = host.level(&dim) {
            drop_and_clear_anvil_slots(host, level, &dim, pos);
        }
    }

    fn on_block_state_change(&self, level: LevelSide, event: &mut OnBlockStateChangeEvent) {
        if !matches!(level, LevelSide::Server(_)) {
            return;
        }
        // Preserve stored contents across anvil damage/repair transitions (anvil <-> chipped <-> damaged).
        if is_known_anvil_key(event.new.block()) {
            return;
        }
        let pos = [event.pos.x, event.pos.y, event.pos.z];
        let dim = event.dimension.key().id().to_owned();
        invalidate_viewers_at(&dim, pos);
        clear_anvil_dirty(&dim, pos);
        let mut drops = PENDING_DROPS.lock().unwrap_or_else(PoisonError::into_inner);
        let key = (dim, pos);
        if !drops.contains(&key) {
            drops.push(key);
        }
    }
}

/// `MenuPart` attached to `easyanvils:anvil`.
pub struct AnvilMenuPart;

#[mod_api::sunrise_part]
impl MenuPart for AnvilMenuPart {
    fn menu_slot_rules(&self, _host: &dyn Host, event: &mut MenuSlotRulesEvent<'_>) {
        if event.container != MenuContainer::Main {
            return;
        }
        if event.slot == 2 {
            if WRITING_OUTPUT_SLOT.get() {
                return;
            }
            event.may_place = false;
            let allowed = event
                .context
                .player
                .and_then(viewer_of)
                .map_or_else(crate::client::client_can_pickup, |v| v.can_pickup);
            event.may_pickup = allowed;
        }
    }

    fn menu_slot_changed(&self, host: &mut dyn ServerHost, event: &mut MenuSlotChangedEvent<'_>) {
        let Some(player_uuid) = event.context.player else {
            return;
        };
        let Some(viewer) = viewer_of(player_uuid) else {
            return;
        };
        if !viewer.still_valid {
            return;
        }

        let input_changed = event
            .changed
            .iter()
            .any(|c| c.container == MenuContainer::Main && (c.slot == 0 || c.slot == 1));
        if !input_changed {
            return;
        }

        let slot0_changed = event
            .changed
            .iter()
            .any(|c| c.container == MenuContainer::Main && c.slot == 0);
        if slot0_changed {
            // Reset pending rename whenever the left input changes so a new item isn't renamed by
            // stale text from the previous item.
            let mut map = VIEWERS.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(entry) = map.get_mut(&player_uuid) {
                entry.item_name = None;
            }
        }

        let inputs = [
            event.context.slot(MenuContainer::Main, 0),
            event.context.slot(MenuContainer::Main, 1),
        ];
        let stored = AnvilSlots::from_stacks(&*host, &inputs);
        write_anvil_slots(host, &viewer.dimension, viewer.pos, &stored);
        clear_anvil_dirty(&viewer.dimension, viewer.pos);

        if let Some(level) = host.level(&viewer.dimension) {
            notify_comparator(host, level, viewer.pos);
        }

        sync_viewers_for_anvil(
            host,
            &viewer.dimension,
            viewer.pos,
            &inputs,
            Some(player_uuid),
        );
    }

    fn menu_slot_take(&self, event: &mut MenuSlotTakeEvent<'_>) {
        if event.slot != 2 {
            return;
        }
        let player_uuid = event.player;
        let Some(viewer) = viewer_of(player_uuid) else {
            return;
        };
        if !viewer.still_valid {
            return;
        }

        let host = event.subject.host();
        let Some(player) = host.player(player_uuid) else {
            return;
        };
        let Some(level) = host.level(&viewer.dimension) else {
            return;
        };
        let Some(menu) = live_menu(&*host, player_uuid, viewer.window) else {
            return;
        };

        let cfg = server_config();
        let creative = player.game_mode(&*host) == Some(GameMode::Creative);
        if !creative && viewer.cost > 0 {
            let _ = player.give_levels(host, -viewer.cost);
        }

        let mut inputs = read_menu_input_slots(&*host, menu);
        inputs[0] = None;
        if viewer.repair_item_count_cost > 0 {
            if let Some(sec) = inputs[1].take() {
                inputs[1] = sec.shrink(viewer.repair_item_count_cost);
            }
        } else if !viewer.only_renaming {
            inputs[1] = None;
        }

        // Reset viewer's pending rename now that the output was taken.
        if let Some(entry) = VIEWERS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(&player_uuid)
        {
            entry.item_name = None;
            entry.cost = 0;
            entry.repair_item_count_cost = 0;
            entry.only_renaming = false;
            entry.can_pickup = false;
        }

        // Persist updated inputs before damaging/breaking the anvil so if the anvil breaks, any
        // leftover repair materials in slot 1 drop cleanly.
        let stored = AnvilSlots::from_stacks(&*host, &inputs);
        write_anvil_slots(host, &viewer.dimension, viewer.pos, &stored);
        clear_anvil_dirty(&viewer.dimension, viewer.pos);

        // Anvil wear & break chance (`ItemInteractionHandler.onTakeAnvilOutputItemStack`)
        let break_chance = if cfg.risk_free_anvil_renaming && viewer.only_renaming {
            0.0
        } else {
            cfg.anvil_break_chance
        };
        let mut anvil_broke = false;
        if !creative
            && break_chance > 0.0
            && let Some(state) = level.block_state(&*host, viewer.pos)
            && is_anvil_state(&*host, state)
            && next_unit_float(player_uuid, viewer.pos) < break_chance
        {
            if let Some(damaged) = damaged_anvil_state(&*host, state) {
                let _ = level.set_block(host, viewer.pos, damaged, BlockUpdateFlags::CLIENTS);
                let _ = level.level_event(host, LEVEL_EVENT_ANVIL_USED, viewer.pos, 0);
            } else {
                anvil_broke = true;
                invalidate_viewers_at(&viewer.dimension, viewer.pos);
                let _ = level.remove_block(host, viewer.pos, false);
                let _ = level.level_event(host, LEVEL_EVENT_ANVIL_BROKEN, viewer.pos, 0);
                drop_and_clear_anvil_slots(host, level, &viewer.dimension, viewer.pos);
            }
        } else {
            let _ = level.level_event(host, LEVEL_EVENT_ANVIL_USED, viewer.pos, 0);
        }

        if !anvil_broke {
            notify_comparator(host, level, viewer.pos);
            sync_viewers_for_anvil(host, &viewer.dimension, viewer.pos, &inputs, None);
        }
    }

    fn menu_removed(&self, _host: &mut dyn ServerHost, event: &mut MenuRemovedEvent<'_>) {
        let mut map = VIEWERS.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(entry) = map.get(&event.uuid)
            && entry.window == event.window
        {
            map.remove(&event.uuid);
        }
    }

    fn menu_still_valid(&self, _context: &dyn LoopContext, event: &mut MenuStillValidEvent<'_>) {
        let Some(viewer) = viewer_of(event.player) else {
            event.valid = false;
            return;
        };
        if !viewer.still_valid {
            event.valid = false;
            return;
        }
        let dx = event.eye[0] - (f64::from(viewer.pos[0]) + 0.5);
        let dy = event.eye[1] - (f64::from(viewer.pos[1]) + 0.5);
        let dz = event.eye[2] - (f64::from(viewer.pos[2]) + 0.5);
        // Vanilla default block reach (4.5) + AbstractContainerMenu.stillValid margin (4.0)
        if dx * dx + dy * dy + dz * dz > 72.25 {
            event.valid = false;
        }
    }
}

pub fn on_server_tick_post(host: &mut dyn ServerHost, level: LevelRef) {
    let active_dim = level
        .dimension()
        .id(&*host)
        .unwrap_or_else(|| host.dimension())
        .to_owned();

    // 1. Drain pending drops for replaced/removed anvils.
    let drops: Vec<[i32; 3]> = {
        let mut guard = PENDING_DROPS.lock().unwrap_or_else(PoisonError::into_inner);
        let mut matched = Vec::new();
        guard.retain(|(dim, pos)| {
            if *dim == active_dim {
                matched.push(*pos);
                false
            } else {
                true
            }
        });
        matched
    };
    for pos in drops {
        let still_anvil = level
            .block_state(&*host, pos)
            .is_some_and(|s| is_anvil_state(&*host, s));
        if !still_anvil {
            drop_and_clear_anvil_slots(host, level, &active_dim, pos);
        }
    }

    // 2. Drain hopper-dirtied anvils (post-commit) and publish comparator + viewer syncs.
    let dirty: Vec<([i32; 3], i32)> = {
        let mut guard = DIRTY_ANVILS.lock().unwrap_or_else(PoisonError::into_inner);
        let mut matched = Vec::new();
        guard.retain(|(dim, pos), prev_signal| {
            if *dim == active_dim {
                matched.push((*pos, *prev_signal));
                false
            } else {
                true
            }
        });
        matched
    };
    for (pos, prev_signal) in dirty {
        let still_anvil = level
            .block_state(&*host, pos)
            .is_some_and(|s| is_anvil_state(&*host, s));
        if !still_anvil {
            continue;
        }
        let stored = read_anvil_slots(host, &active_dim, pos);
        if comparator_signal(&stored) != prev_signal {
            notify_comparator(host, level, pos);
        }
        let anvil_viewers = viewers_at(&active_dim, pos);
        if !anvil_viewers.is_empty() {
            let needs_sync = anvil_viewers.iter().any(|&(uuid, window)| {
                live_menu(&*host, uuid, window).is_some_and(|menu| {
                    let cur = read_menu_input_slots(&*host, menu);
                    AnvilSlots::from_stacks(&*host, &cur) != stored
                })
            });
            if needs_sync {
                let inputs = stored.to_stacks(&*host);
                sync_viewers_for_anvil(host, &active_dim, pos, &inputs, None);
            }
        }
    }

    // 3. Validate open viewers (broken anvils & live XP/gamemode changes affecting may_pickup).
    let active: Vec<(u128, ViewerEntry)> = VIEWERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|(_, e)| e.dimension == active_dim)
        .map(|(&u, e)| (u, e.clone()))
        .collect();
    if active.is_empty() {
        return;
    }

    for (uuid, entry) in active {
        if !entry.still_valid {
            continue;
        }
        let still_anvil = level
            .block_state(&*host, entry.pos)
            .is_some_and(|s| is_anvil_state(&*host, s));
        if !still_anvil {
            invalidate_viewers_at(&active_dim, entry.pos);
            continue;
        }
        if let Some(player) = host.player(uuid) {
            let creative = player.game_mode(&*host) == Some(GameMode::Creative);
            let exp_level = player.experience(&*host).map_or(0, |e| e.level);
            let has_output = live_menu(&*host, uuid, entry.window)
                .and_then(|m| m.slot(&*host, 2).flatten())
                .is_some();
            let pickup = has_output && entry.cost >= 0 && (creative || exp_level >= entry.cost);
            if pickup != entry.can_pickup
                && let Some(v) = VIEWERS
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .get_mut(&uuid)
            {
                v.can_pickup = pickup;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rename_name_enforces_packet_byte_bound() {
        // 32,768 repetitions of '€' (3 UTF-8 bytes each) = 98,304 bytes > MAX_PACKET_NAME_BYTES.
        let over_bytes = "€".repeat(MAX_PACKET_NAME_CHARS + 1);
        assert!(over_bytes.len() > MAX_PACKET_NAME_BYTES);
        assert_eq!(validate_rename_name(&over_bytes, true), None);
        assert_eq!(validate_rename_name(&over_bytes, false), None);

        // 49,151 '§' chars (2 UTF-8 bytes each) = 98,302 bytes > MAX_PACKET_NAME_BYTES.
        let over_section_bytes = "§".repeat(MAX_PACKET_NAME_BYTES / 2 + 1);
        assert!(over_section_bytes.len() > MAX_PACKET_NAME_BYTES);
        assert_eq!(validate_rename_name(&over_section_bytes, true), None);
    }

    #[test]
    fn validate_rename_name_enforces_packet_utf16_bound() {
        // 16,383 "§a" pairs + "x" = 32,767 UTF-16 code units (exact FriendlyByteBuf.MAX_STRING_LENGTH)
        // with 1 visible character when formatting is enabled.
        let mut at_limit = "§a".repeat(MAX_PACKET_NAME_CHARS / 2);
        at_limit.push('x');
        assert_eq!(at_limit.encode_utf16().count(), MAX_PACKET_NAME_CHARS);
        assert_eq!(
            validate_rename_name(&at_limit, true),
            Some(at_limit.clone())
        );

        // 16,384 "§a" pairs = 32,768 UTF-16 code units > MAX_PACKET_NAME_CHARS, even though
        // visible_length is 0 and byte length (49,152) is below MAX_PACKET_NAME_BYTES.
        let over_pairs = "§a".repeat(MAX_PACKET_NAME_CHARS / 2 + 1);
        assert!(over_pairs.len() <= MAX_PACKET_NAME_BYTES);
        assert_eq!(over_pairs.encode_utf16().count(), MAX_PACKET_NAME_CHARS + 1);
        assert_eq!(validate_rename_name(&over_pairs, true), None);

        // Supplementary characters count as 2 UTF-16 code units each (16,384 * 2 = 32,768 > 32,767).
        let over_surrogates = "😀".repeat(MAX_PACKET_NAME_CHARS / 2 + 1);
        assert!(over_surrogates.len() <= MAX_PACKET_NAME_BYTES);
        assert_eq!(
            over_surrogates.encode_utf16().count(),
            MAX_PACKET_NAME_CHARS + 1
        );
        assert_eq!(validate_rename_name(&over_surrogates, true), None);
    }

    #[test]
    fn validate_rename_name_enforces_visible_name_length() {
        let fifty = "a".repeat(MAX_NAME_LENGTH);
        assert_eq!(validate_rename_name(&fifty, false), Some(fifty.clone()));
        assert_eq!(validate_rename_name(&fifty, true), Some(fifty.clone()));

        let fifty_one = "a".repeat(MAX_NAME_LENGTH + 1);
        assert_eq!(validate_rename_name(&fifty_one, false), None);
        assert_eq!(validate_rename_name(&fifty_one, true), None);

        // Formatting pair does not count toward MAX_NAME_LENGTH when formatting is enabled,
        // but when disabled '§' is stripped leaving '6' + 50 'a's = 51 characters.
        let formatted_fifty = format!("§6{fifty}");
        assert_eq!(
            validate_rename_name(&formatted_fifty, true),
            Some(formatted_fifty.clone())
        );
        assert_eq!(validate_rename_name(&formatted_fifty, false), None);

        // Trims surrounding whitespace and filters disallowed control characters.
        assert_eq!(
            validate_rename_name("  §6Sword\n\x7f  ", true),
            Some("§6Sword".to_owned())
        );
    }
}
