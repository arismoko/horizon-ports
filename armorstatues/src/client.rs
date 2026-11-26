//! Client-side Armor Statues screen, drawing, entity previews, tooltips, and input hooks.
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, OnceLock, PoisonError};

use mod_api::client::widgets::TextEdit;
use mod_api::text::{TextContent, TextStyle};
use mod_api::{
    ClientHost, DrawContext, EntityRef, Flow, GuiEntity, GuiEntityError, GuiEntityLook,
    GuiEntityView, IPayloadContext, Identifier, ItemTooltipEvent, MenuScreen, MenuScreenLayout,
    MenuScreenOpen, ScreenClosingEvent, ScreenInitPostEvent, ScreenKeyPressedPreEvent,
    ScreenMouseButtonReleasedPreEvent, ScreenMouseScrolledPreEvent, ScreenRenderBackgroundEvent,
    ScreenRenderPostEvent, ScreenSlot, ScreenView, SoundPlay, Text, ToServer, decode_payload,
};

use crate::commands;
use crate::model::{
    Alignment, DEFAULT_POSE, DEFAULT_SCALE, DEGREES_SNAP_INTERVAL, EQUIPMENT_EMPTY_ICONS,
    EQUIPMENT_SLOT_POSITIONS, EditMessage, MAX_SCALE, MENU_EQUIPMENT_SLOTS, MENU_ID, MIN_SCALE,
    MOVE_INCREMENTS, MUTATORS, POSES, StandSnapshot, StyleOption, SyncMessage, SyncMode, Tab,
    format_four_decimals, format_two_decimals, is_slot_restricted, local_position, snap_value,
    to_log_scale, to_wrapped_degrees,
};
use crate::widgets::{
    AlignAction, AlignmentButton, CloseButtonWidget, CreditsButtonWidget, FocusRowButton,
    IncrementButton, NameBoxWidget, PANEL_H, PANEL_W, PosSliderKind, PoseCardButton,
    PosePageButton, PositionNudgeButton, PositionReadoutWidget, PositionSliderWidget, RotAction,
    RotActionButton, ScaleResetButton, SquareSliderWidget, StyleCheckboxWidget, TAB_H, TAB_STRIDE,
    TAB_W, TabBarWidget, VanillaTweaksAction, VanillaTweaksButton, ZSliderWidget, alignment_row_y,
    hovered_tab, make_name_edit, position_row_visible, position_row_y, style_checkbox_y,
    tabs_start_y,
};

pub const MOD_SCREEN_ID: &str = "armorstatues";

pub type SharedState = Arc<Mutex<ClientState>>;

struct PendingEdit {
    mode: SyncMode,
    entity_id: u32,
    prev_pos: [f64; 3],
    prev_yaw: f32,
    prev_pose: [[f32; 3]; 6],
    msg: EditMessage,
}

/// Persistent UI choices that survive closing and reopening the statue screen.
static LAST_TAB: LazyLock<Mutex<Tab>> = LazyLock::new(|| Mutex::new(Tab::Rotations));
static CLAMP_ROTATIONS: AtomicBool = AtomicBool::new(true);
static CLIPBOARD: LazyLock<Mutex<Option<[[f32; 3]; 6]>>> = LazyLock::new(|| Mutex::new(None));
static FIRST_POSE_INDEX: AtomicUsize = AtomicUsize::new(0);
static MOVE_INCREMENT_INDEX: AtomicUsize = AtomicUsize::new(0);
/// Session cache of client-side edits per entity id for servers without `armorstatues`.
static CLIENT_CACHE: LazyLock<Mutex<HashMap<u32, StandSnapshot>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static EDIT_TOKEN: OnceLock<ToServer<EditMessage>> = OnceLock::new();
static ACTIVE: LazyLock<Mutex<Option<SharedState>>> = LazyLock::new(|| Mutex::new(None));
static PENDING_EDITS: LazyLock<Mutex<Vec<PendingEdit>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static LOGGED_PREVIEW_ERRORS: LazyLock<Mutex<Vec<(&'static str, GuiEntityError)>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

pub fn set_edit_token(token: ToServer<EditMessage>) {
    let _ = EDIT_TOKEN.set(token);
}

pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn get_clamp_rotations() -> bool {
    CLAMP_ROTATIONS.load(Ordering::Relaxed)
}

pub fn set_clamp_rotations(v: bool) {
    CLAMP_ROTATIONS.store(v, Ordering::Relaxed);
}

pub fn get_clipboard() -> Option<[[f32; 3]; 6]> {
    *lock(&CLIPBOARD)
}

pub fn set_clipboard(pose: [[f32; 3]; 6]) {
    *lock(&CLIPBOARD) = Some(pose);
}

pub fn get_first_pose_index() -> usize {
    FIRST_POSE_INDEX.load(Ordering::Relaxed)
}

pub fn set_first_pose_index(idx: usize) {
    FIRST_POSE_INDEX.store(idx, Ordering::Relaxed);
}

pub fn get_move_increment_index() -> usize {
    MOVE_INCREMENT_INDEX
        .load(Ordering::Relaxed)
        .min(MOVE_INCREMENTS.len() - 1)
}

pub fn set_move_increment_index(idx: usize) {
    MOVE_INCREMENT_INDEX.store(idx.min(MOVE_INCREMENTS.len() - 1), Ordering::Relaxed);
}

/// Used by `menu_slot_rules` on the client side during slot prediction.
pub fn current_disabled_slots() -> i32 {
    lock(&ACTIVE)
        .as_ref()
        .map_or(0, |st| lock(st).stand.disabled_slots)
}

/// Dispatches one or more edits as a single atomic operation so multi-edit actions (alignments,
/// multi-field dirty flushes, pending edit batches) finalize once instead of hitting "Queue Not
/// Empty" in client-only command modes.
fn send_edits(
    host: &mut dyn ClientHost,
    state: &mut ClientState,
    edits: &[(EditMessage, bool)],
) -> bool {
    if edits.is_empty() {
        return true;
    }
    match state.mode {
        SyncMode::Network => {
            if let Some(token) = EDIT_TOKEN.get() {
                for (msg, _) in edits {
                    let _ = token.send(host, msg);
                }
            }
            for (msg, _) in edits {
                apply_synced_tracking(state, msg);
            }
            true
        }
        SyncMode::Commands | SyncMode::VanillaTweaks => {
            let vanilla_tweaks = state.mode == SyncMode::VanillaTweaks;
            let built = commands::build_edits_batches(
                vanilla_tweaks,
                state.synced_pos,
                state.synced_yaw,
                state.synced_pose,
                edits,
            );
            let ok = commands::enqueue_batches(
                host,
                state.stand.entity_id,
                vanilla_tweaks,
                built.batches,
            );
            if ok {
                state.synced_pos = built.pos;
                state.synced_yaw = built.yaw;
                state.synced_pose = built.pose;
                lock(&CLIENT_CACHE).insert(state.stand.entity_id, state.stand.clone());
            }
            ok
        }
    }
}

fn dispatch_pending_edits(host: &mut dyn ClientHost, pending: Vec<PendingEdit>) {
    let mut i = 0;
    while i < pending.len() {
        let mode = pending[i].mode;
        let entity_id = pending[i].entity_id;
        let mut end = i + 1;
        while end < pending.len()
            && pending[end].mode == mode
            && pending[end].entity_id == entity_id
        {
            end += 1;
        }
        let group = &pending[i..end];
        match mode {
            SyncMode::Network => {
                if let Some(token) = EDIT_TOKEN.get() {
                    for edit in group {
                        let _ = token.send(host, &edit.msg);
                    }
                }
            }
            SyncMode::Commands | SyncMode::VanillaTweaks => {
                let vanilla_tweaks = mode == SyncMode::VanillaTweaks;
                let mut batches = Vec::new();
                for edit in group {
                    let step = if vanilla_tweaks {
                        commands::build_vt_commands(
                            edit.prev_pos,
                            edit.prev_yaw,
                            &edit.prev_pose,
                            &edit.msg,
                            false,
                        )
                    } else {
                        commands::build_data_commands(edit.prev_pos, &edit.prev_pose, &edit.msg)
                    };
                    batches.extend(step);
                }
                let _ = commands::enqueue_batches(host, entity_id, vanilla_tweaks, batches);
            }
        }
        i = end;
    }
}

pub fn send_edit(host: &mut dyn ClientHost, state: &mut ClientState, msg: &EditMessage) {
    send_edit_with_hint(host, state, msg, false);
}

fn send_edit_with_hint(
    host: &mut dyn ClientHost,
    state: &mut ClientState,
    msg: &EditMessage,
    is_mirrored: bool,
) {
    let _ = send_edits(host, state, &[(msg.clone(), is_mirrored)]);
}

pub fn flush_and_send_edit(
    host: &mut dyn ClientHost,
    state: &mut ClientState,
    msg: EditMessage,
) -> bool {
    let mut edits: Vec<(EditMessage, bool)> = take_dirty_edits(state)
        .into_iter()
        .map(|m| (m, false))
        .collect();
    edits.push((msg, false));
    send_edits(host, state, &edits)
}

fn apply_synced_tracking(state: &mut ClientState, msg: &EditMessage) {
    match msg {
        EditMessage::Position(pos) => state.synced_pos = *pos,
        EditMessage::Rotation(yaw) => state.synced_yaw = *yaw,
        EditMessage::Pose(pose) => state.synced_pose = *pose,
        EditMessage::Name(_) | EditMessage::Style { .. } | EditMessage::Scale(_) => {}
    }
}

pub fn send_vt_trigger(host: &mut dyn ClientHost, state: &ClientState, trigger_id: i32) {
    let _ = commands::enqueue_single_trigger(host, state.stand.entity_id, trigger_id);
}

pub fn click_sound(host: &dyn ClientHost) {
    let _ = host.play_local_sound(&SoundPlay::ui("minecraft:ui.button.click"), 0);
}

pub fn tr(host: &dyn ClientHost, key: &str, fallback: &str) -> String {
    host.translate(key).unwrap_or_else(|| fallback.to_owned())
}

pub struct ClientState {
    pub mode: SyncMode,
    pub stand: StandSnapshot,
    pub synced_pos: [f64; 3],
    pub synced_yaw: f32,
    pub synced_pose: [[f32; 3]; 6],
    pub tab: Tab,
    pub origin: [f32; 2],
    pub focus_row: Option<usize>,
    pub alt_held: bool,
    pub name_edit: TextEdit,
    pub name_focused: bool,
    pub name_dirty_ticks: i32,
    pub dirty_pose: bool,
    pub dirty_scale: bool,
    pub dirty_rotation: bool,
    pub confirm_ticks: HashMap<u32, u32>,
    pub tip_index: u8,
    pub pos_refresh_ticks: u32,
    pub slots_shown: bool,
    pub close_requested: bool,
}

impl ClientState {
    pub fn tabs(&self) -> &'static [Tab] {
        Tab::tabs_for(self.mode)
    }
}

fn is_our_screen(screen: &ScreenView<'_>) -> bool {
    screen.menu == Some(MENU_ID) || screen.mod_screen == Some(MOD_SCREEN_ID)
}

pub fn set_confirm(state: &mut ClientState, id: u32, ticks: u32) {
    state.confirm_ticks.insert(id, ticks);
}

pub fn confirm_active(state: &ClientState, id: u32) -> bool {
    state.confirm_ticks.get(&id).is_some_and(|&t| t > 0)
}

pub fn clear_name_focus(state: &mut ClientState) {
    state.name_focused = false;
}

pub fn apply_pose_now(
    host: &mut dyn ClientHost,
    state: &mut ClientState,
    pose: [[f32; 3]; 6],
    is_mirrored: bool,
) {
    state.stand.pose = pose;
    state.dirty_pose = false;
    send_edit_with_hint(host, state, &EditMessage::Pose(pose), is_mirrored);
}

pub fn apply_alignment(host: &mut dyn ClientHost, state: &mut ClientState, alignment: Alignment) {
    let pose = alignment.pose();
    let offset = alignment.offset(state.stand.small);
    let next_pos = local_position(state.stand.pos, state.stand.yaw, state.stand.pitch, offset);
    if state.mode == SyncMode::VanillaTweaks {
        if commands::enqueue_single_trigger(
            host,
            state.stand.entity_id,
            commands::vt_alignment_trigger(alignment),
        ) {
            state.stand.invisible = true;
            state.stand.no_gravity = true;
            state.stand.pose = pose;
            state.synced_pose = pose;
            state.dirty_pose = false;
            state.stand.pos = next_pos;
            state.synced_pos = next_pos;
            lock(&CLIENT_CACHE).insert(state.stand.entity_id, state.stand.clone());
        }
        return;
    }

    let mut edits = Vec::with_capacity(4);
    if !state.stand.invisible {
        edits.push((
            EditMessage::Style {
                option: StyleOption::Invisible,
                value: true,
            },
            false,
        ));
    }
    if !state.stand.no_gravity {
        edits.push((
            EditMessage::Style {
                option: StyleOption::Immovable,
                value: true,
            },
            false,
        ));
    }
    edits.push((EditMessage::Pose(pose), false));
    edits.push((EditMessage::Position(next_pos), false));

    let prev_stand = state.stand.clone();
    state.stand.invisible = true;
    state.stand.no_gravity = true;
    state.stand.pose = pose;
    state.stand.pos = next_pos;
    if send_edits(host, state, &edits) {
        state.dirty_pose = false;
    } else {
        state.stand = prev_stand;
    }
}

pub fn flush_dirty(host: &mut dyn ClientHost, state: &mut ClientState) {
    let edits: Vec<(EditMessage, bool)> = take_dirty_edits(state)
        .into_iter()
        .map(|msg| (msg, false))
        .collect();
    if !edits.is_empty() {
        let _ = send_edits(host, state, &edits);
    }
}

/// Queues dirty edits when only `&dyn ClientHost` is available; `client_tick_post` sends them.
pub fn queue_dirty(state: &mut ClientState) {
    let msgs = take_dirty_edits(state);
    if msgs.is_empty() {
        return;
    }
    let mut pending = lock(&PENDING_EDITS);
    for msg in msgs {
        let prev_pos = state.synced_pos;
        let prev_yaw = state.synced_yaw;
        let prev_pose = state.synced_pose;
        apply_synced_tracking(state, &msg);
        pending.push(PendingEdit {
            mode: state.mode,
            entity_id: state.stand.entity_id,
            prev_pos,
            prev_yaw,
            prev_pose,
            msg,
        });
    }
    if state.mode != SyncMode::Network {
        lock(&CLIENT_CACHE).insert(state.stand.entity_id, state.stand.clone());
    }
}

fn take_dirty_edits(state: &mut ClientState) -> Vec<EditMessage> {
    let mut out = Vec::new();
    if state.dirty_pose {
        state.dirty_pose = false;
        out.push(EditMessage::Pose(state.stand.pose));
    }
    if state.dirty_scale {
        state.dirty_scale = false;
        out.push(EditMessage::Scale(state.stand.scale));
    }
    if state.dirty_rotation {
        state.dirty_rotation = false;
        out.push(EditMessage::Rotation(state.stand.yaw));
    }
    if state.name_dirty_ticks >= 0 {
        state.name_dirty_ticks = -1;
        let trimmed = state.name_edit.value.trim().to_owned();
        if trimmed != state.stand.custom_name {
            state.stand.custom_name = trimmed.clone();
            state.stand.custom_name_visible = !trimmed.is_empty();
            out.push(EditMessage::Name(trimmed));
        }
    }
    out
}

pub fn switch_tab(
    host: &mut dyn ClientHost,
    state: &mut ClientState,
    new_tab: Tab,
    with_sound: bool,
) {
    if state.tab == new_tab {
        return;
    }
    flush_dirty(host, state);
    finish_switch_tab(host, state, new_tab, with_sound);
}

fn switch_tab_readonly(
    host: &dyn ClientHost,
    state: &mut ClientState,
    new_tab: Tab,
    with_sound: bool,
) {
    if state.tab == new_tab {
        return;
    }
    queue_dirty(state);
    finish_switch_tab(host, state, new_tab, with_sound);
}

fn finish_switch_tab(
    host: &dyn ClientHost,
    state: &mut ClientState,
    new_tab: Tab,
    with_sound: bool,
) {
    state.name_focused = false;
    state.focus_row = None;
    state.tab = new_tab;
    *lock(&LAST_TAB) = new_tab;
    sync_slots(host, state);
    if with_sound {
        click_sound(host);
    }
}

/// Slot geometry for all 42 slots of `armorstatues:armor_stand`.
pub fn slot_position(slot: usize) -> [f32; 2] {
    if slot < 6 {
        EQUIPMENT_SLOT_POSITIONS[slot]
    } else if slot < 33 {
        let i = slot - 6;
        let row = (i / 9) as f32;
        let col = (i % 9) as f32;
        [25.0 + col * 18.0, 96.0 + row * 18.0]
    } else {
        let col = (slot - 33) as f32;
        [25.0 + col * 18.0, 154.0]
    }
}

fn sync_slots(host: &dyn ClientHost, state: &mut ClientState) {
    if state.mode != SyncMode::Network {
        return;
    }
    let want_visible = state.tab == Tab::Equipment && state.focus_row.is_none();
    if state.slots_shown == want_visible {
        return;
    }
    let mut all_ok = true;
    for slot in 0..42 {
        let pos = want_visible.then(|| slot_position(slot));
        if host.set_screen_slot(MENU_ID, slot, pos).is_err() {
            all_ok = false;
        }
    }
    if all_ok {
        state.slots_shown = want_visible;
    }
}

fn initial_tab_for(mode: SyncMode) -> Tab {
    let last = *lock(&LAST_TAB);
    let tabs = Tab::tabs_for(mode);
    if tabs.contains(&last) {
        last
    } else {
        Tab::Rotations
    }
}

/// Permission level `/data` and `/attribute` need (`Commands.LEVEL_GAMEMASTERS`).
const COMMAND_PERMISSION_LEVEL: u8 = 2;

/// Reads an armor stand's synced custom name on the client: `None` when it has none.
fn read_client_custom_name(host: &dyn ClientHost, entity: EntityRef) -> Option<String> {
    let name = entity.custom_name(host).ok().flatten()?;
    Some(host.text_string(&name)).filter(|name| !name.is_empty())
}

/// Opens the client-only Armor Statues screen on a server that does not have `armorstatues`.
pub fn open_client_only_screen(host: &mut dyn ClientHost, entity_id: u32) -> bool {
    let Some(player) = host.local_player() else {
        return false;
    };
    if player.may_build(host) != Some(true) {
        return false;
    }
    // The permission level the server last sent (`LocalPlayer.permissionLevel`); `None` before
    // the server has sent one. The stand editor's commands (`/data merge`, `/attribute`) need
    // the gamemaster level (2), so a player known to lack it is refused here instead of
    // queueing commands the server would reject.
    if player
        .permission_level(host)
        .is_some_and(|level| level < COMMAND_PERMISSION_LEVEL)
    {
        return false;
    }
    let Some(entity) = host.entity(entity_id) else {
        return false;
    };
    let pos = entity.position(host).unwrap_or([0.0, 0.0, 0.0]);
    let [yaw, pitch] = entity.rotation(host).unwrap_or([0.0, 0.0]);
    let live_name = read_client_custom_name(host, entity);
    let creative = player.is_creative(host).unwrap_or(false);

    let mode = if crate::VT_TRIGGERS_SETTING
        .get()
        .is_some_and(|key| matches!(host.setting_bool(*key), Ok(true)))
    {
        SyncMode::VanillaTweaks
    } else {
        SyncMode::Commands
    };

    let mut stand = lock(&CLIENT_CACHE)
        .get(&entity_id)
        .cloned()
        .unwrap_or(StandSnapshot {
            entity_id,
            creative,
            custom_name: live_name.clone().unwrap_or_default(),
            custom_name_visible: live_name.is_some(),
            show_arms: false,
            small: false,
            invisible: false,
            no_base_plate: false,
            no_gravity: false,
            invulnerable: false,
            disabled_slots: 0,
            scale: DEFAULT_SCALE,
            yaw,
            pitch,
            pos,
            pose: DEFAULT_POSE,
            equipment: None,
        });
    stand.entity_id = entity_id;
    stand.creative = creative;
    if let (Some(pose), Some(flags)) = (
        entity.armor_stand_pose(host),
        entity.armor_stand_flags(host),
    ) {
        stand.apply_live(
            [
                pose.head,
                pose.body,
                pose.left_arm,
                pose.right_arm,
                pose.left_leg,
                pose.right_leg,
            ],
            flags.small,
            flags.show_arms,
            flags.show_base_plate,
        );
    }
    stand.pos = pos;
    stand.yaw = yaw;
    stand.pitch = pitch;
    if let Some(name) = live_name {
        stand.custom_name = name;
    }

    let initial_tab = initial_tab_for(mode);
    let tip_index = if host.millis().is_multiple_of(2) {
        1
    } else {
        2
    };
    let name_edit = make_name_edit(&stand.custom_name);
    let synced_pos = stand.pos;
    let synced_yaw = stand.yaw;
    let synced_pose = stand.pose;

    let shared = Arc::new(Mutex::new(ClientState {
        mode,
        stand,
        synced_pos,
        synced_yaw,
        synced_pose,
        tab: initial_tab,
        origin: [0.0, 0.0],
        focus_row: None,
        alt_held: host.modifiers().alt(),
        name_edit,
        name_focused: false,
        name_dirty_ticks: -1,
        dirty_pose: false,
        dirty_scale: false,
        dirty_rotation: false,
        confirm_ticks: HashMap::new(),
        tip_index,
        pos_refresh_ticks: 10,
        slots_shown: false,
        close_requested: false,
    }));
    *lock(&ACTIVE) = Some(shared);
    host.mod_screen_open(MOD_SCREEN_ID)
}

/// Receives server-pushed `ShowArms` / `Equipment` updates while the screen is open.
pub fn handle_sync(msg: SyncMessage, _ctx: &mut dyn IPayloadContext) -> Result<(), String> {
    if let Some(shared) = lock(&ACTIVE).as_ref().cloned() {
        let mut s = lock(&shared);
        match msg {
            SyncMessage::ShowArms(on) => s.stand.show_arms = on,
            SyncMessage::Equipment(eq) => s.stand.equipment = eq,
        }
    }
    Ok(())
}

/// Constructs the `MenuScreen` when the server opens `armorstatues:armor_stand`.
pub fn create_menu_screen(host: &dyn ClientHost, open: MenuScreenOpen<'_>) -> Option<MenuScreen> {
    let stand = decode_payload::<StandSnapshot>(open.extra).ok()?;
    let initial_tab = initial_tab_for(SyncMode::Network);
    let show_slots = initial_tab == Tab::Equipment;

    // Always declare all 42 slots with their empty-slot icons so `set_screen_slot` preserves
    // equipment silhouettes when switching to the Equipment tab; place offscreen initially and
    // hide immediately via `sync_slots` in `screen_init_post` / `draw_screen` when starting elsewhere.
    let mut slots = Vec::with_capacity(42);
    for i in 0..42 {
        let position = if show_slots {
            slot_position(i)
        } else {
            [-10_000.0, -10_000.0]
        };
        let mut slot = ScreenSlot {
            position,
            ..Default::default()
        };
        slot.no_item_icon = EQUIPMENT_EMPTY_ICONS.get(i).map(|s| (*s).to_owned());
        slots.push(Some(slot));
    }

    let tip_index = if host.millis().is_multiple_of(2) {
        1
    } else {
        2
    };
    let name_edit = make_name_edit(&stand.custom_name);
    let synced_pos = stand.pos;
    let synced_yaw = stand.yaw;
    let synced_pose = stand.pose;
    let shared = Arc::new(Mutex::new(ClientState {
        mode: SyncMode::Network,
        stand,
        synced_pos,
        synced_yaw,
        synced_pose,
        tab: initial_tab,
        origin: [0.0, 0.0],
        focus_row: None,
        alt_held: host.modifiers().alt(),
        name_edit,
        name_focused: false,
        name_dirty_ticks: -1,
        dirty_pose: false,
        dirty_scale: false,
        dirty_rotation: false,
        confirm_ticks: HashMap::new(),
        tip_index,
        pos_refresh_ticks: 10,
        // All 42 slots begin declared as active so `sync_slots` hides them immediately if needed.
        slots_shown: true,
        close_requested: false,
    }));
    *lock(&ACTIVE) = Some(Arc::clone(&shared));

    let layout = MenuScreenLayout {
        width: PANEL_W,
        height: PANEL_H,
        slots,
        inventory_x: 25.0,
        inventory_y: 96.0,
        own_labels: true,
        own_background: true,
        ..Default::default()
    };

    Some(MenuScreen::new(layout).with_draw(move |host, frame, draw| {
        draw_screen_content(
            host,
            &shared,
            frame.origin,
            [frame.pointer[0] as f32, frame.pointer[1] as f32],
            draw,
        );
    }))
}

/// Draws the client-only mod screen panel and entity previews before mounted widgets.
pub fn on_screen_render_background(
    host: &dyn ClientHost,
    event: &mut ScreenRenderBackgroundEvent<'_>,
) {
    if event.screen.mod_screen != Some(MOD_SCREEN_ID) {
        return;
    }
    let Some(shared) = lock(&ACTIVE).as_ref().cloned() else {
        return;
    };
    let r = event.screen.rect;
    let origin = [
        ((r[2] - PANEL_W) * 0.5).floor(),
        ((r[3] - PANEL_H) * 0.5).floor(),
    ];
    draw_screen_content(host, &shared, origin, event.screen.mouse, event.draw);
}

/// Draws an armor stand preview via `DrawContext::entity`. Records any `GuiEntityError` once per
/// `(call_kind, error)` instead of silently discarding it, and falls back to the armor stand item
/// icon when the engine refuses to draw `minecraft:armor_stand` in a GUI.
fn draw_stand_preview(
    host: &dyn ClientHost,
    draw: &mut dyn DrawContext,
    entity: &GuiEntity,
    view: GuiEntityView,
) {
    if let Err(err) = draw.entity(entity, view) {
        let label = match entity {
            GuiEntity::Existing(_) => "GuiEntity::Existing(armor_stand)",
            GuiEntity::Kind { .. } => "GuiEntity::Kind(minecraft:armor_stand)",
            _ => "GuiEntity",
        };
        let mut logged = lock(&LOGGED_PREVIEW_ERRORS);
        if !logged.contains(&(label, err)) {
            logged.push((label, err));
            eprintln!("armorstatues: DrawContext::entity({label}) failed: {err:?}");
        }
        drop(logged);
        if let Ok(id) = Identifier::parse("minecraft:armor_stand")
            && let Some(key) = host.item_key(&id)
        {
            let [x0, y0, x1, y1] = view.rect;
            let icon = 32.0_f32.min((x1 - x0) - 4.0).min((y1 - y0) - 4.0);
            let ix = ((x0 + x1 - icon) * 0.5).floor();
            let iy = ((y0 + y1 - icon) * 0.5).floor();
            draw.item(key, [ix, iy], icon);
        }
    }
}

fn draw_screen_content(
    host: &dyn ClientHost,
    shared: &SharedState,
    origin: [f32; 2],
    mouse: [f32; 2],
    draw: &mut dyn DrawContext,
) {
    let (mode, tab, focus_row, stand) = {
        let mut s = lock(shared);
        s.origin = origin;
        s.alt_held = host.modifiers().alt();
        sync_slots(host, &mut s);
        (s.mode, s.tab, s.focus_row, s.stand.clone())
    };
    let [left, top] = origin;

    if focus_row.is_none() {
        draw_panel(draw, [left, top, PANEL_W, PANEL_H]);
        draw_tabs(host, draw, origin, Tab::tabs_for(mode), tab);

        match tab {
            Tab::Equipment => {
                for slot in 0..42 {
                    let [sx, sy] = slot_position(slot);
                    draw_slot_frame(draw, left + sx - 1.0, top + sy - 1.0);
                    if let Some(&eq_slot) = MENU_EQUIPMENT_SLOTS.get(slot)
                        && is_slot_restricted(stand.disabled_slots, eq_slot)
                    {
                        draw_disabled_slot_overlay(draw, left + sx - 1.0, top + sy - 1.0);
                    }
                }
                // Small entity preview at (81, 20) following the mouse.
                draw_preview_box(draw, [left + 80.0, top + 19.0, 50.0, 72.0]);
                let rect = [left + 81.0, top + 20.0, left + 129.0, top + 90.0];
                let view = GuiEntityView::new(rect, 30.0, 0.0625, GuiEntityLook::At(mouse));
                let entity = host
                    .entity(stand.entity_id)
                    .map(GuiEntity::existing)
                    .unwrap_or_else(|| {
                        GuiEntity::with_data("minecraft:armor_stand", stand.preview_nbt(None))
                    });
                draw_stand_preview(host, draw, &entity, view);
            }
            Tab::Style => {
                // Name box background at (14, 30, 76, 12).
                draw.gui_nine_slice(
                    "widget/text_field",
                    [left + 14.0, top + 30.0, 76.0, 12.0],
                    0xFFFFFFFF,
                );
                // Large preview at (14, 50): 76x108 frame, 74x106 scissor, scale 45.
                draw_preview_box(draw, [left + 14.0, top + 50.0, 76.0, 108.0]);
                let rect = [left + 15.0, top + 51.0, left + 89.0, top + 157.0];
                let center = [left + 52.0, top + 104.0];
                let view = GuiEntityView::new(rect, 45.0, 0.0625, GuiEntityLook::At(center));
                let entity = GuiEntity::with_data("minecraft:armor_stand", stand.preview_nbt(None));
                draw_stand_preview(host, draw, &entity, view);
            }
            Tab::Poses => {
                // Main large preview at (5, 40): 76x108 frame, 74x106 scissor, scale 45.
                draw_preview_box(draw, [left + 5.0, top + 40.0, 76.0, 108.0]);
                let rect = [left + 6.0, top + 41.0, left + 80.0, top + 147.0];
                let center = [left + 43.0, top + 94.0];
                let view = GuiEntityView::new(rect, 45.0, 0.0625, GuiEntityLook::At(center));
                let entity = GuiEntity::with_data("minecraft:armor_stand", stand.preview_nbt(None));
                draw_stand_preview(host, draw, &entity, view);

                // 4 pose preset preview cards at (83 + col*62, 9 + row*88), 60x82 frame, 48x70 scissor.
                let first = get_first_pose_index();
                for i in 0..4 {
                    let Some(preset) = POSES.get(first + i) else {
                        continue;
                    };
                    let col = (i % 2) as f32;
                    let row = (i / 2) as f32;
                    let card_x = left + 83.0 + col * 62.0;
                    let card_y = top + 9.0 + row * 88.0;
                    let hovered = point_in_rect(mouse, [card_x, card_y, 60.0, 82.0]);
                    draw_button_bg(draw, [card_x, card_y, 60.0, 82.0], true, hovered);
                    let px = left + 89.0 + col * 62.0;
                    let py = top + 15.0 + row * 88.0;
                    draw_preview_box(draw, [px - 1.0, py - 1.0, 50.0, 72.0]);
                    let cx = px + 24.0;
                    let cy = py + 35.0;
                    let look = if hovered {
                        [cx + (mouse[0] - cx) * 4.0, cy + (mouse[1] - cy) * 4.0]
                    } else {
                        [cx, cy]
                    };
                    let view = GuiEntityView::new(
                        [px, py, px + 48.0, py + 70.0],
                        30.0,
                        0.0625,
                        GuiEntityLook::At(look),
                    );
                    let entity = GuiEntity::with_data(
                        "minecraft:armor_stand",
                        stand.preview_nbt(Some(&preset.pose)),
                    );
                    draw_stand_preview(host, draw, &entity, view);
                }
            }
            Tab::Rotations => {
                // Small preview at (80, 58): 50x72 frame, 48x70 scissor, scale 30.
                draw_preview_box(draw, [left + 80.0, top + 58.0, 50.0, 72.0]);
                let rect = [left + 81.0, top + 59.0, left + 129.0, top + 129.0];
                let center = [left + 105.0, top + 94.0];
                let view = GuiEntityView::new(rect, 30.0, 0.0625, GuiEntityLook::At(center));
                let entity = GuiEntity::with_data("minecraft:armor_stand", stand.preview_nbt(None));
                draw_stand_preview(host, draw, &entity, view);
            }
            Tab::Position => {
                draw_position_labels(host, draw, origin, None);
            }
            Tab::Alignments | Tab::VanillaTweaks => {}
        }
    } else if tab == Tab::Position {
        draw_position_labels(host, draw, origin, focus_row);
    }
}

fn draw_position_labels(
    host: &dyn ClientHost,
    draw: &mut dyn DrawContext,
    origin: [f32; 2],
    focus_row: Option<usize>,
) {
    const KEYS: [(&str, &str); 6] = [
        ("screen.statuemenus.position.scale", "Scale:"),
        ("screen.statuemenus.position.rotation", "Rotation:"),
        ("screen.statuemenus.position.move_by", "Move By:"),
        ("screen.statuemenus.position.x", "X-Position:"),
        ("screen.statuemenus.position.y", "Y-Position:"),
        ("screen.statuemenus.position.z", "Z-Position:"),
    ];
    let focused_mode = focus_row.is_some();
    for (row, (key, fallback)) in KEYS.iter().enumerate() {
        if !position_row_visible(focus_row, row) {
            continue;
        }
        let label = tr(host, key, fallback);
        let cx = origin[0] + 8.0 + 36.0;
        let y = position_row_y(origin, row) + 6.0;
        if focused_mode {
            let w = draw.text_width(&label);
            draw.fill([cx - w * 0.5 - 2.0, y - 2.0, w + 4.0, 13.0], 0x40000000);
            draw_centered_str(draw, &label, cx, y, 0xFFFFFFFF, true);
        } else {
            draw_centered_str(draw, &label, cx, y, 0xFF404040, false);
        }
    }
}

fn draw_tabs(
    host: &dyn ClientHost,
    draw: &mut dyn DrawContext,
    origin: [f32; 2],
    tabs: &[Tab],
    current: Tab,
) {
    let start_y = tabs_start_y(tabs.len());
    for (i, &tab) in tabs.iter().enumerate() {
        let tx = origin[0] - 32.0;
        let ty = origin[1] + start_y + TAB_STRIDE * i as f32;
        let selected = tab == current;
        let sprite = if selected {
            "recipe_book/tab_selected"
        } else {
            "recipe_book/tab"
        };
        let draw_x = if selected { tx } else { tx + 3.0 };
        let draw_w = if selected { TAB_W } else { TAB_W - 3.0 };
        draw.gui_nine_slice(sprite, [draw_x, ty, draw_w, TAB_H], 0xFFFFFFFF);
        if let Ok(id) = Identifier::parse(tab.icon_item())
            && let Some(key) = host.item_key(&id)
        {
            draw.item(key, [tx + 10.0, ty + 5.0], 16.0);
        }
    }
}

pub fn draw_panel(draw: &mut dyn DrawContext, [x, y, w, h]: [f32; 4]) {
    // Classic vanilla container panel bevel.
    draw.fill([x + 1.0, y, w - 2.0, h], 0xFF000000);
    draw.fill([x, y + 1.0, w, h - 2.0], 0xFF000000);
    draw.fill([x + 1.0, y + 1.0, w - 2.0, h - 2.0], 0xFFC6C6C6);
    // White top/left highlight
    draw.fill([x + 1.0, y + 1.0, w - 3.0, 2.0], 0xFFFFFFFF);
    draw.fill([x + 1.0, y + 1.0, 2.0, h - 3.0], 0xFFFFFFFF);
    // Dark gray bottom/right shadow
    draw.fill([x + 2.0, y + h - 3.0, w - 3.0, 2.0], 0xFF555555);
    draw.fill([x + w - 3.0, y + 2.0, 2.0, h - 3.0], 0xFF555555);
}

fn draw_slot_frame(draw: &mut dyn DrawContext, x: f32, y: f32) {
    draw.fill([x, y, 18.0, 18.0], 0xFF8B8B8B);
    draw.fill([x, y, 17.0, 1.0], 0xFF373737);
    draw.fill([x, y, 1.0, 17.0], 0xFF373737);
    draw.fill([x + 1.0, y + 17.0, 17.0, 1.0], 0xFFFFFFFF);
    draw.fill([x + 17.0, y + 1.0, 1.0, 17.0], 0xFFFFFFFF);
}

fn draw_disabled_slot_overlay(draw: &mut dyn DrawContext, x: f32, y: f32) {
    draw.gui_nine_slice(
        "container/crafter/disabled_slot",
        [x, y, 18.0, 18.0],
        0xFFFFFFFF,
    );
}

fn draw_preview_box(draw: &mut dyn DrawContext, [x, y, w, h]: [f32; 4]) {
    draw.fill([x, y, w, h], 0xFF1A1A1A);
    draw.fill([x, y, w - 1.0, 1.0], 0xFF373737);
    draw.fill([x, y, 1.0, h - 1.0], 0xFF373737);
    draw.fill([x + 1.0, y + h - 1.0, w - 1.0, 1.0], 0xFFFFFFFF);
    draw.fill([x + w - 1.0, y + 1.0, 1.0, h - 1.0], 0xFFFFFFFF);
}

pub fn draw_slider_track(draw: &mut dyn DrawContext, [x, y, w, h]: [f32; 4]) {
    draw.fill([x, y, w, h], 0xFF373737);
    draw.fill([x + 1.0, y + 1.0, w - 2.0, h - 2.0], 0xFF222222);
}

pub fn draw_button_bg(draw: &mut dyn DrawContext, rect: [f32; 4], active: bool, hovered: bool) {
    let sprite = if !active {
        "widget/button_disabled"
    } else if hovered {
        "widget/button_highlighted"
    } else {
        "widget/button"
    };
    draw.gui_nine_slice(sprite, rect, 0xFFFFFFFF);
}

pub fn draw_checkbox_bg(draw: &mut dyn DrawContext, rect: [f32; 4], ticked: bool, hovered: bool) {
    draw.fill(rect, if hovered { 0xFFFFFFFF } else { 0xFF373737 });
    draw.fill(
        [rect[0] + 1.0, rect[1] + 1.0, rect[2] - 2.0, rect[3] - 2.0],
        0xFF202020,
    );
    if ticked {
        draw_centered_str(
            draw,
            "✓",
            rect[0] + rect[2] * 0.5,
            rect[1] + 4.0,
            0xFF55FF55,
            true,
        );
    }
}

pub fn draw_centered_str(
    draw: &mut dyn DrawContext,
    text: &str,
    cx: f32,
    y: f32,
    argb: u32,
    shadow: bool,
) {
    let w = draw.text_width(text);
    draw.text(text, [(cx - w * 0.5).floor(), y], argb, shadow);
}

fn point_in_rect(p: [f32; 2], r: [f32; 4]) -> bool {
    p[0] >= r[0] && p[0] < r[0] + r[2] && p[1] >= r[1] && p[1] < r[1] + r[3]
}

/// Mounts all custom widgets on our screen during `screen_init_post`.
pub fn on_screen_init_post(host: &dyn ClientHost, event: &mut ScreenInitPostEvent<'_>) {
    if !is_our_screen(event.screen) {
        return;
    }
    let Some(shared) = lock(&ACTIVE).as_ref().cloned() else {
        return;
    };
    {
        let mut s = lock(&shared);
        s.origin = if event.screen.menu == Some(MENU_ID) {
            [event.screen.rect[0], event.screen.rect[1]]
        } else {
            let r = event.screen.rect;
            [
                ((r[2] - PANEL_W) * 0.5).floor(),
                ((r[3] - PANEL_H) * 0.5).floor(),
            ]
        };
        sync_slots(host, &mut s);
    }

    let w = &mut event.widgets;
    // Shared controls across tabs
    w.add_widget(Box::new(TabBarWidget {
        state: Arc::clone(&shared),
    }));
    w.add_widget(Box::new(CloseButtonWidget {
        state: Arc::clone(&shared),
    }));
    w.add_widget(Box::new(CreditsButtonWidget {
        state: Arc::clone(&shared),
    }));

    // Rotations tab
    for action in [
        RotAction::Lock,
        RotAction::Tip,
        RotAction::Randomize,
        RotAction::Reset,
        RotAction::Mirror,
        RotAction::Copy,
        RotAction::Paste,
    ] {
        w.add_widget(Box::new(RotActionButton {
            state: Arc::clone(&shared),
            action,
        }));
    }
    for i in 0..MUTATORS.len() {
        w.add_widget(Box::new(SquareSliderWidget::new(Arc::clone(&shared), i)));
        w.add_widget(Box::new(ZSliderWidget::new(Arc::clone(&shared), i)));
    }

    // Poses tab
    w.add_widget(Box::new(PosePageButton {
        state: Arc::clone(&shared),
        prev: true,
    }));
    w.add_widget(Box::new(PosePageButton {
        state: Arc::clone(&shared),
        prev: false,
    }));
    for i in 0..4 {
        w.add_widget(Box::new(PoseCardButton {
            state: Arc::clone(&shared),
            card_idx: i,
        }));
    }

    // Style tab
    w.add_widget(Box::new(NameBoxWidget {
        state: Arc::clone(&shared),
    }));
    for (i, &option) in StyleOption::STYLE_TAB.iter().enumerate() {
        w.add_widget(Box::new(StyleCheckboxWidget {
            state: Arc::clone(&shared),
            index: i,
            option,
        }));
    }

    // Position tab
    w.add_widget(Box::new(PositionSliderWidget::new(
        Arc::clone(&shared),
        PosSliderKind::Scale,
    )));
    w.add_widget(Box::new(ScaleResetButton {
        state: Arc::clone(&shared),
    }));
    w.add_widget(Box::new(PositionSliderWidget::new(
        Arc::clone(&shared),
        PosSliderKind::Rotation,
    )));
    for i in 0..MOVE_INCREMENTS.len() {
        w.add_widget(Box::new(IncrementButton {
            state: Arc::clone(&shared),
            index: i,
        }));
    }
    for axis in 0..3 {
        w.add_widget(Box::new(PositionReadoutWidget {
            state: Arc::clone(&shared),
            axis,
        }));
        w.add_widget(Box::new(PositionNudgeButton {
            state: Arc::clone(&shared),
            axis,
            up: true,
        }));
        w.add_widget(Box::new(PositionNudgeButton {
            state: Arc::clone(&shared),
            axis,
            up: false,
        }));
    }
    for row in 0..6 {
        w.add_widget(Box::new(FocusRowButton {
            state: Arc::clone(&shared),
            row,
            alt_hides: row == 0,
        }));
    }

    // Alignments tab
    w.add_widget(Box::new(AlignmentButton {
        state: Arc::clone(&shared),
        action: AlignAction::Centered,
    }));
    w.add_widget(Box::new(AlignmentButton {
        state: Arc::clone(&shared),
        action: AlignAction::Cornered,
    }));
    for alignment in Alignment::ALL {
        w.add_widget(Box::new(AlignmentButton {
            state: Arc::clone(&shared),
            action: AlignAction::Surface(alignment),
        }));
    }
    // Vanilla Tweaks tab buttons (visible only in VanillaTweaks client-only mode)
    for action in VanillaTweaksAction::ALL {
        w.add_widget(Box::new(VanillaTweaksButton {
            state: Arc::clone(&shared),
            action,
        }));
    }
}

/// Hotbar number keys `1..=N` switch tabs (when not typing in Name box and not dragging/hovering an item).
/// Also closes the client-only mod screen when the inventory key is pressed.
pub fn on_screen_key_pressed_pre(
    host: &dyn ClientHost,
    event: &mut ScreenKeyPressedPreEvent<'_>,
) -> Flow {
    if !is_our_screen(event.screen) {
        return Flow::Continue;
    }
    let Some(shared) = lock(&ACTIVE).as_ref().cloned() else {
        return Flow::Continue;
    };
    let (mode, tab, name_focused, focus_row) = {
        let s = lock(&shared);
        (s.mode, s.tab, s.name_focused, s.focus_row)
    };
    if name_focused || focus_row.is_some() {
        return Flow::Continue;
    }
    if mode != SyncMode::Network {
        let is_inv_key = host
            .vanilla_key_mapping("key.inventory")
            .ok()
            .and_then(|m| m.binding(host).ok())
            .map_or_else(|| event.key == "key.keyboard.e", |b| b == event.key);
        if is_inv_key {
            let mut s = lock(&shared);
            queue_dirty(&mut s);
            s.close_requested = true;
            return Flow::Cancel;
        }
    }
    if tab == Tab::Equipment {
        let carrying = host
            .local_player()
            .and_then(|p| p.container_menu(host))
            .and_then(|m| m.carried(host))
            .flatten()
            .is_some();
        if carrying || host.screen_hovered_stack().is_some() {
            return Flow::Continue;
        }
    }
    for (i, &target_tab) in Tab::tabs_for(mode).iter().enumerate() {
        let map_name = format!("key.hotbar.{}", i + 1);
        let fallback = format!("key.keyboard.{}", i + 1);
        let matched = host
            .vanilla_key_mapping(&map_name)
            .ok()
            .and_then(|m| m.binding(host).ok())
            .map_or_else(|| event.key == fallback, |b| b == event.key);
        if matched {
            let mut s = lock(&shared);
            if s.tab != target_tab {
                switch_tab_readonly(host, &mut s, target_tab, true);
                return Flow::Cancel;
            }
        }
    }
    Flow::Continue
}

/// Mouse wheel over tabs cycles tabs (no sound); wheel over Poses panel pages poses by 4.
pub fn on_screen_mouse_scrolled_pre(
    host: &dyn ClientHost,
    event: &mut ScreenMouseScrolledPreEvent<'_>,
) -> Flow {
    if !is_our_screen(event.screen) {
        return Flow::Continue;
    }
    let Some(shared) = lock(&ACTIVE).as_ref().cloned() else {
        return Flow::Continue;
    };
    let delta = event.scroll[1];
    if delta == 0.0 {
        return Flow::Continue;
    }
    let mut s = lock(&shared);
    if s.focus_row.is_none() && hovered_tab(s.origin, s.tabs(), event.screen.mouse).is_some() {
        let next = s.tab.cycle_in(s.tabs(), delta > 0.0);
        switch_tab_readonly(host, &mut s, next, false);
        return Flow::Cancel;
    }
    if s.tab == Tab::Poses
        && point_in_rect(
            event.screen.mouse,
            [s.origin[0], s.origin[1], PANEL_W, PANEL_H],
        )
    {
        let idx = get_first_pose_index() as isize;
        let step = if delta > 0.0 { -4 } else { 4 };
        let next = idx + step;
        if next >= 0 && (next as usize) < POSES.len() {
            set_first_pose_index(next as usize);
        }
        return Flow::Cancel;
    }
    Flow::Continue
}

/// Flushes any dirty slider even if mouse was released outside the widget.
pub fn on_screen_mouse_button_released_pre(
    _host: &dyn ClientHost,
    event: &mut ScreenMouseButtonReleasedPreEvent<'_>,
) -> Flow {
    if !is_our_screen(event.screen) {
        return Flow::Continue;
    }
    if let Some(shared) = lock(&ACTIVE).as_ref().cloned() {
        let mut s = lock(&shared);
        if s.dirty_pose || s.dirty_scale || s.dirty_rotation {
            queue_dirty(&mut s);
        }
    }
    Flow::Continue
}

pub fn on_screen_closing(_host: &dyn ClientHost, event: &mut ScreenClosingEvent<'_>) {
    if !is_our_screen(event.screen) {
        return;
    }
    if let Some(shared) = lock(&ACTIVE).take() {
        let mut s = lock(&shared);
        queue_dirty(&mut s);
    }
}

/// Ticks debounced name sync, queued edits, client command queue, confirmation timers, Alt state,
/// and live position refresh.
pub fn on_client_tick_post(host: &mut dyn ClientHost) {
    let active = lock(&ACTIVE).as_ref().cloned();
    if let Some(shared) = active.as_ref() {
        let mut s = lock(shared);
        if s.name_dirty_ticks > 0 {
            s.name_dirty_ticks -= 1;
            if s.name_dirty_ticks == 0 {
                queue_dirty(&mut s);
            }
        }
    }
    let pending: Vec<PendingEdit> = std::mem::take(&mut *lock(&PENDING_EDITS));
    dispatch_pending_edits(host, pending);
    commands::tick(host);
    let Some(shared) = active else {
        return;
    };
    let close_now = {
        let mut s = lock(&shared);
        if s.close_requested {
            s.close_requested = false;
            true
        } else {
            s.alt_held = host.modifiers().alt();
            s.confirm_ticks.retain(|_, ticks| {
                if *ticks > 0 {
                    *ticks -= 1;
                }
                *ticks > 0
            });
            if s.tab == Tab::Position {
                if s.pos_refresh_ticks > 0 {
                    s.pos_refresh_ticks -= 1;
                } else {
                    s.pos_refresh_ticks = 10;
                    if let Some(pos) = host
                        .entity(s.stand.entity_id)
                        .and_then(|e| e.position(host))
                    {
                        s.stand.pos = pos;
                    }
                }
            }
            false
        }
    };
    if close_now {
        host.close_screen();
    }
}

/// Draws hover tooltips for tabs and custom widgets above mounted widgets.
pub fn on_screen_render_post(host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
    if !is_our_screen(event.screen) {
        return;
    }
    let Some(shared) = lock(&ACTIVE).as_ref().cloned() else {
        return;
    };
    let (mode, tab, focus_row, origin, stand, tip_index, alt_held) = {
        let s = lock(&shared);
        (
            s.mode,
            s.tab,
            s.focus_row,
            s.origin,
            s.stand.clone(),
            s.tip_index,
            s.alt_held,
        )
    };
    let mouse = event.screen.mouse;

    // On Equipment tab, suppress tab tooltips while carrying an item.
    let carrying = tab == Tab::Equipment
        && host
            .local_player()
            .and_then(|p| p.container_menu(host))
            .and_then(|m| m.carried(host))
            .flatten()
            .is_some();
    if focus_row.is_none()
        && !carrying
        && let Some(htab) = hovered_tab(origin, Tab::tabs_for(mode), mouse)
    {
        let _ = event
            .draw
            .gui_tooltip(&[Text::translate(htab.translation_key(), vec![])], mouse);
        return;
    }

    if let Some((lines, at)) = widget_tooltip(
        host, event.draw, tab, focus_row, origin, mouse, &stand, tip_index, alt_held,
    ) {
        let _ = event.draw.gui_tooltip(&lines, at);
    }
}

#[allow(clippy::too_many_arguments)]
fn widget_tooltip(
    host: &dyn ClientHost,
    draw: &dyn DrawContext,
    tab: Tab,
    focus_row: Option<usize>,
    o: [f32; 2],
    mouse: [f32; 2],
    stand: &StandSnapshot,
    tip_index: u8,
    alt_held: bool,
) -> Option<(Vec<Text>, [f32; 2])> {
    if focus_row.is_none()
        && matches!(tab, Tab::Poses | Tab::Alignments | Tab::VanillaTweaks)
        && point_in_rect(mouse, [o[0] + 6.0, o[1] + 6.0, 20.0, 20.0])
    {
        return Some((
            vec![Text::translate("screen.statuemenus.poses.credits", vec![])],
            mouse,
        ));
    }

    match tab {
        Tab::Rotations => {
            if point_in_rect(mouse, [o[0] + 83.0, o[1] + 10.0, 20.0, 20.0]) {
                let key = if get_clamp_rotations() {
                    "screen.statuemenus.rotations.limited"
                } else {
                    "screen.statuemenus.rotations.unlimited"
                };
                return Some((vec![Text::translate(key, vec![])], mouse));
            }
            if point_in_rect(mouse, [o[0] + 107.0, o[1] + 10.0, 20.0, 20.0]) {
                let key = format!("screen.statuemenus.rotations.tip{tip_index}");
                return Some((vec![Text::translate(key, vec![])], mouse));
            }
            if point_in_rect(mouse, [o[0] + 83.0, o[1] + 34.0, 20.0, 20.0]) {
                return Some((
                    vec![Text::translate(
                        "screen.statuemenus.rotations.randomize",
                        vec![],
                    )],
                    mouse,
                ));
            }
            if point_in_rect(mouse, [o[0] + 107.0, o[1] + 34.0, 20.0, 20.0]) {
                return Some((
                    vec![Text::translate(
                        "screen.statuemenus.rotations.reset",
                        vec![],
                    )],
                    mouse,
                ));
            }
            if point_in_rect(mouse, [o[0] + 83.0, o[1] + 134.0, 44.0, 20.0]) {
                return Some((
                    vec![Text::translate(
                        "screen.statuemenus.rotations.mirror",
                        vec![],
                    )],
                    mouse,
                ));
            }
            if point_in_rect(mouse, [o[0] + 83.0, o[1] + 158.0, 20.0, 20.0]) {
                return Some((
                    vec![Text::translate("screen.statuemenus.rotations.copy", vec![])],
                    mouse,
                ));
            }
            if point_in_rect(mouse, [o[0] + 107.0, o[1] + 158.0, 20.0, 20.0]) {
                return Some((
                    vec![Text::translate(
                        "screen.statuemenus.rotations.paste",
                        vec![],
                    )],
                    mouse,
                ));
            }
            for (i, m) in MUTATORS.iter().enumerate() {
                let is_left = i % 2 == 0;
                let col = (i % 2) as f32;
                let row = (i / 2) as f32;
                let sq = [
                    o[0] + 23.0 + col * 110.0,
                    o[1] + 7.0 + row * 60.0,
                    54.0,
                    54.0,
                ];
                if point_in_rect(mouse, sq) {
                    let x_val = format_two_decimals(snap_value(
                        m.display_axis(&stand.pose, 0),
                        DEGREES_SNAP_INTERVAL,
                    ));
                    let y_val = format_two_decimals(snap_value(
                        m.display_axis(&stand.pose, 1),
                        DEGREES_SNAP_INTERVAL,
                    ));
                    let lines = vec![
                        Text::translate(m.translation_key, vec![]),
                        Text::translate(
                            crate::model::PartMutator::axis_translation_key(0),
                            vec![Text::literal(x_val)],
                        ),
                        Text::translate(
                            crate::model::PartMutator::axis_translation_key(1),
                            vec![Text::literal(y_val)],
                        ),
                    ];
                    let pos = if is_left {
                        let w = max_line_width(host, draw, &lines);
                        [(mouse[0] - 24.0 - w).max(4.0), mouse[1]]
                    } else {
                        mouse
                    };
                    return Some((lines, pos));
                }
                let zs = [
                    o[0] + 6.0 + col * 183.0,
                    o[1] + 7.0 + row * 60.0,
                    15.0,
                    54.0,
                ];
                if point_in_rect(mouse, zs) {
                    let z_val = format_two_decimals(snap_value(
                        m.display_axis(&stand.pose, 2),
                        DEGREES_SNAP_INTERVAL,
                    ));
                    let lines = vec![
                        Text::translate(m.translation_key, vec![]),
                        Text::translate(
                            crate::model::PartMutator::axis_translation_key(2),
                            vec![Text::literal(z_val)],
                        ),
                    ];
                    let pos = if is_left {
                        let w = max_line_width(host, draw, &lines);
                        [(mouse[0] - 24.0 - w).max(4.0), mouse[1]]
                    } else {
                        mouse
                    };
                    return Some((lines, pos));
                }
            }
            None
        }
        Tab::Poses => {
            let first = get_first_pose_index();
            for i in 0..4 {
                let Some(preset) = POSES.get(first + i) else {
                    continue;
                };
                let col = (i % 2) as f32;
                let row = (i / 2) as f32;
                let r = [
                    o[0] + 83.0 + col * 62.0,
                    o[1] + 9.0 + row * 88.0,
                    60.0,
                    82.0,
                ];
                if point_in_rect(mouse, r) {
                    let mut blue = TextStyle::default();
                    blue.color = Some("blue".to_owned());
                    return Some((
                        vec![
                            Text::translate(preset.translation_key, vec![]),
                            Text::translate(
                                "screen.statuemenus.poses.by",
                                vec![Text::literal(preset.source_name)],
                            )
                            .with_style(blue),
                        ],
                        mouse,
                    ));
                }
            }
            None
        }
        Tab::Style => {
            if point_in_rect(mouse, [o[0] + 14.0, o[1] + 30.0, 76.0, 12.0]) {
                return Some((
                    vec![Text::translate("screen.statuemenus.style.tooltip", vec![])],
                    mouse,
                ));
            }
            for (i, &opt) in StyleOption::STYLE_TAB.iter().enumerate() {
                if opt.creative_only() && !stand.creative {
                    continue;
                }
                let r = [
                    o[0] + 96.0,
                    style_checkbox_y(o, stand.creative, i),
                    102.0,
                    20.0,
                ];
                if point_in_rect(mouse, r) {
                    return Some((vec![Text::translate(opt.description_key(), vec![])], mouse));
                }
            }
            None
        }
        Tab::Position => {
            // Row 0: Scale slider + Focus/Reset
            if position_row_visible(focus_row, 0) {
                let ry = position_row_y(o, 0);
                let sr = [o[0] + 8.0 + 76.0, ry + 1.0, 90.0, 20.0];
                if point_in_rect(mouse, sr) {
                    let mv = ((f64::from(mouse[0]) - f64::from(sr[0])) / f64::from(sr[2]))
                        .clamp(0.0, 1.0);
                    let raw = to_log_scale(mv);
                    let truncated =
                        ((raw * 100.0) as i32 as f32 / 100.0).clamp(MIN_SCALE, MAX_SCALE);
                    return Some((
                        vec![Text::literal(format_two_decimals(f64::from(truncated)))],
                        mouse,
                    ));
                }
                if point_in_rect(mouse, [o[0] + 8.0 + 174.0, ry + 1.0, 20.0, 20.0]) {
                    let key = if alt_held {
                        "screen.statuemenus.rotations.reset"
                    } else {
                        "screen.statuemenus.position.focus"
                    };
                    return Some((vec![Text::translate(key, vec![])], mouse));
                }
            }
            // Row 1: Rotation slider + Focus
            if position_row_visible(focus_row, 1) {
                let ry = position_row_y(o, 1);
                let sr = [o[0] + 8.0 + 76.0, ry + 1.0, 90.0, 20.0];
                if point_in_rect(mouse, sr) {
                    let mv = snap_value(
                        ((f64::from(mouse[0]) - f64::from(sr[0])) / f64::from(sr[2]))
                            .clamp(0.0, 1.0),
                        DEGREES_SNAP_INTERVAL,
                    );
                    let deg = format_two_decimals(f64::from(to_wrapped_degrees(mv)));
                    return Some((
                        vec![Text::translate(
                            "screen.statuemenus.position.degrees",
                            vec![Text::literal(deg)],
                        )],
                        mouse,
                    ));
                }
                if point_in_rect(mouse, [o[0] + 8.0 + 174.0, ry + 1.0, 20.0, 20.0]) {
                    return Some((
                        vec![Text::translate("screen.statuemenus.position.focus", vec![])],
                        mouse,
                    ));
                }
            }
            // Row 2: Move By increment buttons + Focus
            if position_row_visible(focus_row, 2) {
                let ry = position_row_y(o, 2);
                for (i, &inc) in MOVE_INCREMENTS.iter().enumerate() {
                    let extra = if i > 1 { 1.0 } else { 0.0 };
                    let r = [
                        o[0] + 8.0 + 76.0 + i as f32 * 24.0 + extra,
                        ry + 1.0,
                        20.0,
                        20.0,
                    ];
                    if point_in_rect(mouse, r) {
                        let px = (inc * 16.0).round() as i32;
                        return Some((
                            vec![
                                Text::translate(
                                    "screen.statuemenus.position.pixels",
                                    vec![Text::literal(px.to_string())],
                                ),
                                Text::translate(
                                    "screen.statuemenus.position.blocks",
                                    vec![Text::literal(format_four_decimals(inc))],
                                ),
                            ],
                            mouse,
                        ));
                    }
                }
                if point_in_rect(mouse, [o[0] + 8.0 + 174.0, ry + 1.0, 20.0, 20.0]) {
                    return Some((
                        vec![Text::translate("screen.statuemenus.position.focus", vec![])],
                        mouse,
                    ));
                }
            }
            // Rows 3..6: X/Y/Z nudge buttons + Focus
            let px = (MOVE_INCREMENTS[get_move_increment_index()] * 16.0).round() as i32;
            for axis in 0..3 {
                let row = 3 + axis;
                if !position_row_visible(focus_row, row) {
                    continue;
                }
                let ry = position_row_y(o, row);
                if point_in_rect(mouse, [o[0] + 8.0 + 149.0, ry + 1.0, 20.0, 10.0]) {
                    return Some((
                        vec![Text::translate(
                            "screen.statuemenus.position.increment",
                            vec![Text::translate(
                                "screen.statuemenus.position.pixels",
                                vec![Text::literal(px.to_string())],
                            )],
                        )],
                        mouse,
                    ));
                }
                if point_in_rect(mouse, [o[0] + 8.0 + 149.0, ry + 11.0, 20.0, 10.0]) {
                    return Some((
                        vec![Text::translate(
                            "screen.statuemenus.position.decrement",
                            vec![Text::translate(
                                "screen.statuemenus.position.pixels",
                                vec![Text::literal(px.to_string())],
                            )],
                        )],
                        mouse,
                    ));
                }
                if point_in_rect(mouse, [o[0] + 8.0 + 174.0, ry + 1.0, 20.0, 20.0]) {
                    return Some((
                        vec![Text::translate("screen.statuemenus.position.focus", vec![])],
                        mouse,
                    ));
                }
            }
            None
        }
        Tab::Alignments => {
            let r0 = alignment_row_y(o, 0);
            if point_in_rect(mouse, [o[0] + 8.0, r0 + 1.0, 94.0, 20.0]) {
                return Some((
                    vec![Text::translate(
                        "screen.statuemenus.position.centered.description",
                        vec![],
                    )],
                    mouse,
                ));
            }
            if point_in_rect(mouse, [o[0] + 8.0 + 100.0, r0 + 1.0, 94.0, 20.0]) {
                return Some((
                    vec![Text::translate(
                        "screen.statuemenus.position.cornered.description",
                        vec![],
                    )],
                    mouse,
                ));
            }
            for (i, a) in Alignment::ALL.iter().enumerate() {
                let ry = alignment_row_y(o, 1 + i);
                if point_in_rect(mouse, [o[0] + 8.0, ry + 1.0, 194.0, 20.0]) {
                    return Some((vec![Text::translate(a.description_key(), vec![])], mouse));
                }
            }
            None
        }
        Tab::VanillaTweaks => {
            for action in VanillaTweaksAction::ALL {
                if point_in_rect(mouse, action.rect(o)) {
                    return Some((
                        vec![Text::translate(action.description_key(), vec![])],
                        mouse,
                    ));
                }
            }
            None
        }
        Tab::Equipment => None,
    }
}

fn max_line_width(host: &dyn ClientHost, draw: &dyn DrawContext, lines: &[Text]) -> f32 {
    lines
        .iter()
        .map(|t| match &t.content {
            TextContent::Literal(s) => draw.text_width(s),
            TextContent::Translatable { key, arguments, .. } => {
                let mut resolved = tr(host, key, key);
                for arg in arguments {
                    if let TextContent::Literal(s) = &arg.content {
                        resolved = resolved.replacen("%s", s, 1);
                    }
                }
                draw.text_width(&resolved)
            }
            _ => 60.0,
        })
        .fold(0.0_f32, f32::max)
}

/// Armor stand item hover tooltip (`ArmorStandInteractHelper.getArmorStandHoverText`).
pub fn on_item_tooltip(host: &dyn ClientHost, event: &mut ItemTooltipEvent<'_>) {
    if event.stack.item().id(host) != Some("minecraft:armor_stand") {
        return;
    }
    let mut purple = TextStyle::default();
    purple.color = Some("light_purple".to_owned());
    let sneak = Text::new(TextContent::Keybind("key.sneak".into())).with_style(purple.clone());
    let use_key = Text::new(TextContent::Keybind("key.use".into())).with_style(purple);
    let mut gray = TextStyle::default();
    gray.color = Some("gray".to_owned());
    let line = Text::translate(
        "item.minecraft.armor_stand.description",
        vec![sneak, use_key],
    )
    .with_style(gray);
    let _ = event.lines.push(line, u32::MAX);
}
