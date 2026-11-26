#![deny(unsafe_code)]

//! Bridging Mod for Sunrise: Bedrock-style reach-around block placement,
//! crosshair directional indicators, block outlines, and slab/trapdoor alignment.

mod config;
mod math;
mod render;
mod screen;

use crate::config::{
    BridgingConfig, CONFIG_FILE_NAME, MOD_ID, SET_ENABLE_BRIDGING, SettingKeys, SourcePerspective,
    register_config_file, register_settings,
};
use crate::math::{
    Direction, NEAR_ZERO, aabb_intersects_block, clamp_point_within_reach, compute_view_block_path,
    valid_assist_sides, view_vector,
};
use crate::render::{RenderSnapshot, render_crosshair_indicator, render_world_outlines};
use mod_api::text::TextStyle;
use mod_api::{
    BlockClass, BlockHighlightEvent, BlockHit, BlockKey, BlockPos, ClientHost, ClientPickEvent,
    ClientTickPostEvent, ConfigEvent, ConfigSnapshot, CrosshairTarget, Flow, Hand, Hooks, Host,
    HostBlockStates, HostSide, InteractionAttempt, InteractionKey,
    InteractionKeyMappingTriggeredEvent, ItemKey, KeyConflictContext, KeyRegistration, LevelRef,
    ModKeyMappingRef, PlayerRef, Registrar, RegistrationContext, RenderGuiPostEvent,
    ScreenInitPostEvent, ScreenRenderPostEvent, SettingChangedEvent, SettingValue,
    SubmitCustomGeometryEvent, Text,
};
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex, MutexGuard};

const KEY_TOGGLE_BRIDGING: &str = "toggle_bridging";
const DEFAULT_BLOCK_REACH: f64 = 4.5;
const TRAPDOOR_HEIGHT: f64 = 3.0 / 16.0;
const SLAB_HEIGHT: f64 = 8.0 / 16.0;

/// Active injected bridging pick, used to guard Attack/PickBlock/block_highlight and delay.
#[derive(Clone, Copy, Debug)]
struct ActiveBridgingPick {
    /// Block position where the new block will be placed (drawn by the outline).
    placement_pos: [i32; 3],
    /// Direction from `placement_pos` toward the supporting block.
    support_dir: Direction,
    /// Clicked position passed in `CrosshairTarget::Block` (equals `placement_pos` except
    /// for vertical slab combining, where it is the adjacent slab's position).
    click_pos: [i32; 3],
    /// Exact `BlockHit` injected into `client_pick`.
    hit: BlockHit,
}

struct Runtime {
    config: BridgingConfig,
    /// Owner-bound keys of the registered settings.
    keys: SettingKeys,
    settings_synced: bool,
    key_toggle_bridging: Option<ModKeyMappingRef>,
    /// Player Y fractional part while standing on ground (`BridgingStateTracker.lastKnownYFrac`).
    last_known_y_frac: f64,
    /// A bridged placement just started; the end of this tick sets the use key's repeat delay.
    post_bridging_delay_due: bool,
    /// Current bridging pick injected during the latest `client_pick`.
    active_pick: Option<ActiveBridgingPick>,
    /// Snapshot for HUD and world-outline rendering.
    render_snapshot: RenderSnapshot,
}

impl Runtime {
    fn new() -> Self {
        Self {
            config: BridgingConfig::default(),
            keys: SettingKeys::default(),
            settings_synced: false,
            key_toggle_bridging: None,
            last_known_y_frac: 0.0,
            post_bridging_delay_due: false,
            active_pick: None,
            render_snapshot: RenderSnapshot::default(),
        }
    }

    /// Write `config/bridgingmod.toml` beside the game's other configs.
    fn persist_config_file(&self, host: &dyn Host) {
        let path = host.config_dir().join(CONFIG_FILE_NAME);
        let _ = std::fs::write(path, self.config.to_toml());
    }
}

fn lock_runtime(runtime: &Mutex<Runtime>) -> MutexGuard<'_, Runtime> {
    runtime.lock().unwrap_or_else(|e| e.into_inner())
}

struct BridgingMod {
    runtime: Arc<Mutex<Runtime>>,
    /// The config screen tab that is showing.
    screen_tab: Arc<AtomicUsize>,
}

impl Default for BridgingMod {
    fn default() -> Self {
        Self {
            runtime: Arc::new(Mutex::new(Runtime::new())),
            screen_tab: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl BridgingMod {
    fn lock(&self) -> MutexGuard<'_, Runtime> {
        lock_runtime(&self.runtime)
    }

    /// Push the loaded config into the settings store once. Each write notifies
    /// `setting_changed`, which takes the runtime lock, so none is held across the writes.
    fn sync_initial_settings(&self, host: &mut dyn ClientHost) {
        let (config, keys) = {
            let mut rt = self.lock();
            if rt.settings_synced {
                return;
            }
            rt.settings_synced = true;
            (rt.config.clone(), rt.keys.clone())
        };
        config.write_to_host_settings(host, &keys);
    }

    fn poll_toggle_key(&self, host: &mut dyn ClientHost) {
        let key = {
            let mut rt = self.lock();
            if rt.key_toggle_bridging.is_none() {
                rt.key_toggle_bridging = host.mod_key_mapping(MOD_ID, KEY_TOGGLE_BRIDGING).ok();
            }
            rt.key_toggle_bridging
        };
        let Some(key) = key else {
            return;
        };

        let mut clicks = 0_u32;
        while key.consume_click(host).unwrap_or(false) {
            clicks = clicks.saturating_add(1);
        }
        if clicks == 0 {
            return;
        }

        let (enabled, setting) = {
            let mut rt = self.lock();
            if clicks % 2 == 1 {
                rt.config.enable_bridging_assist = !rt.config.enable_bridging_assist;
            }
            rt.persist_config_file(&*host);
            (
                rt.config.enable_bridging_assist,
                rt.keys.get(SET_ENABLE_BRIDGING),
            )
        };
        if let Some(setting) = setting {
            let _ = host.set_setting(setting, SettingValue::Toggle(enabled));
        }
        let _ = host.set_overlay_message(build_toggle_message(enabled), false);
    }
}

#[mod_api::sunrise_mod]
impl Hooks for BridgingMod {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        let keys = register_settings(r);
        self.lock().keys = keys;

        let runtime = Arc::clone(&self.runtime);
        let listener =
            move |event: ConfigEvent<'_>, snapshot: &ConfigSnapshot| -> Result<(), String> {
                match event {
                    ConfigEvent::Loading(_) => {
                        lock_runtime(&runtime).config = BridgingConfig::parse_toml(&snapshot.text);
                    }
                    ConfigEvent::Reloading(HostSide::Client(client)) => {
                        let (config, keys) = {
                            let mut rt = lock_runtime(&runtime);
                            rt.config = BridgingConfig::parse_toml(&snapshot.text);
                            rt.settings_synced = true;
                            (rt.config.clone(), rt.keys.clone())
                        };
                        config.write_to_host_settings(client, &keys);
                    }
                    _ => {}
                }
                Ok(())
            };
        register_config_file(r, Arc::new(listener));

        // The Mods screen's Config button opens the tabbed screen below, starting on its first tab.
        let tab = Arc::clone(&self.screen_tab);
        let _ = r.register_config_screen(Arc::new(move |_host: &dyn ClientHost| {
            tab.store(0, std::sync::atomic::Ordering::Relaxed);
            true
        }));

        r.register_key_mapping(
            KeyRegistration::new(KEY_TOGGLE_BRIDGING, "key.keyboard.comma")
                .label("key.bridgingmod.toggle_bridging")
                .category("key.category.bridgingmod.category")
                .context(KeyConflictContext::InGame),
        );
    }

    fn setting_changed(&self, host: &dyn Host, event: &mut SettingChangedEvent<'_>) {
        if event.mod_id != MOD_ID {
            return;
        }
        let mut rt = self.lock();
        if rt.config.apply_single_setting(event.key, event.value) {
            rt.persist_config_file(host);
        }
    }

    fn screen_init_post(&self, host: &dyn ClientHost, event: &mut ScreenInitPostEvent<'_>) {
        if event.screen.mod_screen != Some(MOD_ID) || !host.on(MOD_ID) {
            return;
        }
        let keys = self.lock().keys.clone();
        screen::mount(event.widgets, event.screen.rect, &self.screen_tab, &keys);
    }

    fn screen_render_post(&self, host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
        if event.screen.mod_screen != Some(MOD_ID) {
            return;
        }
        screen::render_title(host, event.draw);
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPostEvent) {
        if !host.on(MOD_ID) {
            return;
        }

        let ground_y_frac = host.local_player().and_then(|player| {
            let entity = player.entity();
            if entity.on_ground(&*host) == Some(true) {
                let pos = entity.position(&*host)?;
                Some(pos[1] - pos[1].floor())
            } else {
                None
            }
        });

        {
            let mut rt = self.lock();
            if let Some(frac) = ground_y_frac {
                rt.last_known_y_frac = frac;
            }
        }
        let post_delay = {
            let mut rt = self.lock();
            std::mem::take(&mut rt.post_bridging_delay_due).then_some(rt.config.delay_post_bridging)
        };
        if let Some(ticks) = post_delay {
            host.set_right_click_delay(ticks);
        }
        self.sync_initial_settings(host);
        self.poll_toggle_key(host);
    }

    fn client_pick(&self, host: &mut dyn ClientHost, event: &mut ClientPickEvent<'_>) {
        if !host.on(MOD_ID) {
            return;
        }

        self.sync_initial_settings(host);
        let (config, last_known_y_frac) = {
            let rt = self.lock();
            (rt.config.clone(), rt.last_known_y_frac)
        };

        let Some(player) = host.local_player() else {
            let mut rt = self.lock();
            rt.active_pick = None;
            rt.render_snapshot = RenderSnapshot::default();
            return;
        };

        let entity = player.entity();
        let is_crouching = entity.is_shift_key_down(&*host).unwrap_or(false);
        let held = read_held_placeable(&*host, player, config.skip_torch_bridging);

        // Compute non-bridging outline target when aiming at a real block.
        let non_bridging_target = match event.original {
            Some(CrosshairTarget::Block { pos, hit, .. }) if held.has_placeable => {
                let place_pos = [
                    pos.x.saturating_add(hit.face[0]),
                    pos.y.saturating_add(hit.face[1]),
                    pos.z.saturating_add(hit.face[2]),
                ];
                let overlaps_player = entity
                    .bounding_box(&*host)
                    .is_some_and(|[min, max]| aabb_intersects_block(min, max, place_pos));
                if overlaps_player {
                    None
                } else {
                    Some(place_pos)
                }
            }
            _ => None,
        };

        let want_debug_trace = host.debug_overlay_visible() && config.show_debug_trace;

        // Gather view path when needed for either bridging or F3 debug trace.
        let path_info = compute_perspective_and_path(&*host, player, event.origin, &config);

        let mut new_active_pick = None;
        let bridging_allowed = config.enable_bridging_assist
            && (!config.only_bridge_when_crouched || is_crouching)
            && event.original.is_none()
            && event.result.is_none()
            && held.placeable_block.is_some()
            && !held.main_hand_is_skipped_torch;

        if bridging_allowed
            && let (Some((path, cam_dir, reach)), Some(level), Some(held_block)) = (
                path_info.as_ref(),
                host.level(host.dimension()),
                held.placeable_block,
            )
            && let Some((placement_pos, support_dir)) = find_closest_assist_target(
                &*host,
                level,
                entity,
                path,
                *cam_dir,
                is_crouching,
                &config,
            )
        {
            let place_dir = support_dir.opposite();
            if let Some((click_pos, hit)) = build_place_assist_target(
                &*host,
                level,
                held_block,
                place_dir,
                placement_pos,
                event.origin,
                *reach,
                last_known_y_frac,
                config.enable_slab_assist,
            ) && let Some(click_state) = level.block_state(&*host, click_pos)
            {
                *event.result = Some(CrosshairTarget::Block {
                    pos: BlockPos::new(click_pos[0], click_pos[1], click_pos[2]),
                    state: click_state,
                    hit,
                });
                new_active_pick = Some(ActiveBridgingPick {
                    placement_pos,
                    support_dir,
                    click_pos,
                    hit,
                });
            }
        }

        let mut rt = self.lock();
        rt.active_pick = new_active_pick;
        rt.render_snapshot = RenderSnapshot {
            bridging_target: new_active_pick.map(|p| (p.placement_pos, p.support_dir)),
            non_bridging_target,
            is_crouching,
            debug_trace: if want_debug_trace {
                path_info.map(|(path, _, _)| path).unwrap_or_default()
            } else {
                Vec::new()
            },
        };
    }

    fn interaction_key_mapping_triggered(
        &self,
        host: &mut dyn ClientHost,
        event: &mut InteractionKeyMappingTriggeredEvent,
    ) -> Flow {
        if !host.on(MOD_ID) {
            return Flow::Continue;
        }

        let crosshair = host.crosshair_target();
        let mut rt = self.lock();
        let Some(active) = rt.active_pick else {
            return Flow::Continue;
        };

        // Confirm the current crosshair target is the one we injected.
        let is_our_pick = matches!(
            crosshair,
            Some(CrosshairTarget::Block { pos, hit, .. })
                if [pos.x, pos.y, pos.z] == active.click_pos && hit == active.hit
        );
        if !is_our_pick {
            return Flow::Continue;
        }

        match event.key {
            InteractionKey::Attack => {
                // Never mine air or an adjacent slab through an injected bridging target.
                // On a fresh press, preserve an empty-air swing; on held, suppress silently.
                event.swing_hand = event.attempt == InteractionAttempt::Press;
                Flow::Cancel
            }
            InteractionKey::PickBlock => {
                // Never middle-click pick air or an adjacent slab through a bridging target.
                Flow::Cancel
            }
            InteractionKey::Use => {
                // Vanilla sets the repeat delay to 4 as this use starts; the end of the
                // tick replaces it with the configured post-bridging delay.
                if event.hand == Hand::Main {
                    rt.post_bridging_delay_due = true;
                }
                Flow::Continue
            }
            _ => Flow::Continue,
        }
    }

    fn block_highlight(&self, host: &dyn ClientHost, event: &mut BlockHighlightEvent) -> Flow {
        if !host.on(MOD_ID) {
            return Flow::Continue;
        }
        let rt = self.lock();
        if let Some(active) = rt.active_pick
            && event.pos == active.click_pos
        {
            // Suppress vanilla's selection box on replaceable blocks or vertical-slab
            // support blocks when Bridging Mod's own outline is active.
            return Flow::Cancel;
        }
        Flow::Continue
    }

    fn submit_custom_geometry(
        &self,
        host: &dyn ClientHost,
        event: &mut SubmitCustomGeometryEvent<'_>,
    ) {
        if !host.on(MOD_ID) {
            return;
        }
        let (snapshot, config) = {
            let rt = self.lock();
            (rt.render_snapshot.clone(), rt.config.clone())
        };
        render_world_outlines(host, &mut *event.draw, &snapshot, &config);
    }

    fn render_gui_post(&self, host: &dyn ClientHost, event: &mut RenderGuiPostEvent<'_>) {
        if !host.on(MOD_ID) {
            return;
        }
        let (snapshot, config) = {
            let rt = self.lock();
            (rt.render_snapshot.clone(), rt.config.clone())
        };
        render_crosshair_indicator(host, event.draw, &snapshot, &config);
    }
}

/// Summary of placeable blocks held in the player's main hand and offhand.
struct HeldPlaceable {
    /// Whether either hand holds a placeable block (for non-bridging outline).
    has_placeable: bool,
    /// Block key of the first placeable hand (main hand preferred, else offhand).
    placeable_block: Option<BlockKey>,
    /// True when `skip_torch_bridging` is active and the main hand holds a torch.
    main_hand_is_skipped_torch: bool,
}

fn read_held_placeable(
    host: &dyn Host,
    player: PlayerRef,
    skip_torch_bridging: bool,
) -> HeldPlaceable {
    let selected_slot = player.selected_slot(host).unwrap_or(0) as u32;
    let (main_item, off_item): (Option<ItemKey>, Option<ItemKey>) = player
        .with_inventory(host, |inv| {
            let main = inv
                .get(selected_slot)
                .filter(|s| s.count() > 0)
                .map(|s| s.item());
            let off = inv.get(40).filter(|s| s.count() > 0).map(|s| s.item());
            (main, off)
        })
        .unwrap_or((None, None));

    let main_block = main_item.and_then(|item| host.item_block(item));
    let off_block = off_item.and_then(|item| host.item_block(item));

    let main_is_skipped_torch =
        skip_torch_bridging && main_block.is_some_and(|block| is_torch_block(host, block));

    let main_placeable = main_block.filter(|&b| !skip_torch_bridging || !is_torch_block(host, b));
    let off_placeable = off_block.filter(|&b| !skip_torch_bridging || !is_torch_block(host, b));

    let placeable_block = main_placeable.or(off_placeable);
    HeldPlaceable {
        has_placeable: placeable_block.is_some(),
        placeable_block,
        main_hand_is_skipped_torch: main_is_skipped_torch,
    }
}

/// Compute camera & eye perspectives and trace the candidate voxel path.
fn compute_perspective_and_path(
    host: &dyn ClientHost,
    player: PlayerRef,
    eye_origin: [f64; 3],
    config: &BridgingConfig,
) -> Option<(Vec<[i32; 3]>, [f64; 3], f64)> {
    let entity = player.entity();
    let [player_yaw, player_pitch] = entity.rotation(host)?;
    let eye_dir = view_vector(player_yaw, player_pitch);

    let reach = host
        .living(entity)
        .and_then(|living| living.attribute_value(host, "minecraft:block_interaction_range"))
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(DEFAULT_BLOCK_REACH);

    let freelook_loaded = host.is_loaded("freelook");
    let (cam_pos, cam_dir) = match config.effective_perspective(freelook_loaded) {
        SourcePerspective::CopyTogglePerspective | SourcePerspective::LetBridgingModDecide => {
            if let Some(cam) = host.camera() {
                (cam.position, view_vector(cam.yaw, cam.pitch))
            } else {
                (eye_origin, eye_dir)
            }
        }
        SourcePerspective::AlwaysEyeline => (eye_origin, eye_dir),
    };

    let path = compute_view_block_path(
        cam_pos,
        cam_dir,
        eye_origin,
        eye_dir,
        reach,
        config.min_bridge_distance,
        config.bridging_adjacency,
        config.bridging_snap_strength,
    );

    Some((path, cam_dir, reach))
}

/// Walk the candidate voxel path and return the first valid `(placement_pos, support_dir)`.
fn find_closest_assist_target(
    host: &dyn Host,
    level: LevelRef,
    player_entity: mod_api::EntityRef,
    path: &[[i32; 3]],
    view_dir: [f64; 3],
    is_crouching: bool,
    config: &BridgingConfig,
) -> Option<([i32; 3], Direction)> {
    let valid_sides = valid_assist_sides(view_dir);
    if valid_sides.is_empty() {
        return None;
    }

    let active_axis_mode = if is_crouching {
        config
            .supported_bridge_axes_when_crouched
            .resolve(config.supported_bridge_axes)
    } else {
        config.supported_bridge_axes
    };

    let player_bb = player_entity.bounding_box(host);
    let block_states = host.block_states();

    for &pos in path {
        let Some(target_state) = level.block_state(host, pos) else {
            continue;
        };

        let allowed_here = if config.enable_non_solid_replace {
            block_states.replaceable(target_state).unwrap_or(false)
        } else {
            is_air_block(host, target_state.block())
        };
        if !allowed_here {
            continue;
        }

        if let Some([min, max]) = player_bb
            && aabb_intersects_block(min, max, pos)
        {
            continue;
        }

        for &check_side in &valid_sides {
            if !check_side.allowed_by(active_axis_mode) {
                continue;
            }
            let support_pos = check_side.offset(pos);
            let Some(support_state) = level.block_state(host, support_pos) else {
                continue;
            };
            if is_air_block(host, support_state.block())
                || is_liquid_block(host, support_state.block())
                || block_states.replaceable(support_state).unwrap_or(true)
            {
                continue;
            }
            return Some((pos, check_side));
        }
    }

    None
}

/// Build the `BlockHit` and clicked `BlockPos` for bridging placement, including horizontal
/// slab/trapdoor alignment and vertical slab combining (`Bridge.getDefaultPlaceAssistTarget`).
#[allow(clippy::too_many_arguments)]
fn build_place_assist_target(
    host: &dyn Host,
    level: LevelRef,
    held_block: BlockKey,
    place_dir: Direction,
    pos: [i32; 3],
    eye_origin: [f64; 3],
    reach: f64,
    last_known_y_frac: f64,
    slab_assist_enabled: bool,
) -> Option<([i32; 3], BlockHit)> {
    if slab_assist_enabled {
        let is_slab = is_slab_block(host, held_block);
        let is_trapdoor = is_trapdoor_block(host, held_block);

        if is_slab || is_trapdoor {
            if place_dir.is_horizontal() {
                let target_lower_half = last_known_y_frac > TRAPDOOR_HEIGHT - NEAR_ZERO
                    && last_known_y_frac < SLAB_HEIGHT + NEAR_ZERO;
                let y_offset = if target_lower_half { 0.1 } else { 0.9 };
                let raw_point = [
                    f64::from(pos[0]) + 0.5,
                    f64::from(pos[1]) + y_offset,
                    f64::from(pos[2]) + 0.5,
                ];
                let face = if target_lower_half {
                    Direction::Up.normal()
                } else {
                    Direction::Down.normal()
                };
                if let Some(point) =
                    clamp_point_within_reach(eye_origin, raw_point, pos, reach, true)
                {
                    return Some((pos, BlockHit::new(point, face, false)));
                }
            } else if is_slab {
                let building_off_pos = place_dir.opposite().offset(pos);
                let block_states = host.block_states();
                if let Some(local_state) = level.block_state(host, building_off_pos)
                    && is_slab_block(host, local_state.block())
                    && let Ok(Some(slab_type)) = local_state.property(&block_states, "type")
                {
                    let can_combine = (slab_type == "top" && place_dir == Direction::Down)
                        || (slab_type == "bottom" && place_dir == Direction::Up);
                    if can_combine {
                        let raw_point = [
                            f64::from(pos[0]) + 0.5,
                            f64::from(pos[1]) + 0.5,
                            f64::from(pos[2]) + 0.5,
                        ];
                        if let Some(point) = clamp_point_within_reach(
                            eye_origin,
                            raw_point,
                            building_off_pos,
                            reach,
                            false,
                        ) {
                            return Some((
                                building_off_pos,
                                BlockHit::new(point, place_dir.normal(), false),
                            ));
                        }
                    }
                }
            }
        }
    }

    let center = [
        f64::from(pos[0]) + 0.5,
        f64::from(pos[1]) + 0.5,
        f64::from(pos[2]) + 0.5,
    ];
    let point = clamp_point_within_reach(eye_origin, center, pos, reach, false)?;
    Some((pos, BlockHit::new(point, place_dir.normal(), true)))
}

fn is_air_block(host: &dyn Host, block: BlockKey) -> bool {
    matches!(
        block.id(host),
        Some("minecraft:air" | "minecraft:cave_air" | "minecraft:void_air")
    )
}

fn is_liquid_block(host: &dyn Host, block: BlockKey) -> bool {
    matches!(block.id(host), Some("minecraft:water" | "minecraft:lava"))
}

fn is_torch_block(host: &dyn Host, block: BlockKey) -> bool {
    matches!(
        block.id(host),
        Some(
            "minecraft:torch"
                | "minecraft:wall_torch"
                | "minecraft:soul_torch"
                | "minecraft:soul_wall_torch"
                | "minecraft:redstone_torch"
                | "minecraft:redstone_wall_torch"
                | "minecraft:copper_torch"
                | "minecraft:copper_wall_torch"
        )
    )
}

fn is_slab_block(host: &dyn Host, block: BlockKey) -> bool {
    let block_states = host.block_states();
    let Some(default_state) = host.block_state_default(block) else {
        return false;
    };
    matches!(
        default_state.property(&block_states, "type"),
        Ok(Some("bottom" | "top" | "double"))
    )
}

fn is_trapdoor_block(host: &dyn Host, block: BlockKey) -> bool {
    let Some(default_state) = host.block_state_default(block) else {
        return false;
    };
    matches!(
        host.block_states()
            .block_class(default_state, BlockClass::TrapDoor),
        Ok(Some(true))
    )
}

fn build_toggle_message(enabled: bool) -> Text {
    let mut gold = TextStyle::default();
    gold.color = Some("gold".to_owned());

    let mut state_style = TextStyle::default();
    state_style.color = Some(if enabled { "green" } else { "red" }.to_owned());

    let state_key = if enabled {
        "notif.bridgingmod.action.enabled"
    } else {
        "notif.bridgingmod.action.disabled"
    };

    Text::translate("notif.bridgingmod.toggle_bridging", Vec::new())
        .with_style(gold)
        .append(Text::literal(": "))
        .append(Text::translate(state_key, Vec::new()).with_style(state_style))
}
