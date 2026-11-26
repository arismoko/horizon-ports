use crate::config::{
    ClientConfig, MAX_CHANNEL_LENGTH, MAX_CORRECTION_PERIOD, MAX_PING_DISTANCE, MAX_PING_DURATION,
    MOD_ID, PlayerInfoMode, TPS, TeamColorMode, client_config, persist_client_config,
    update_client_config,
};
use crate::math::{
    ScreenPos, WHITE, calculate_angle_rect_intersection, calculate_ping_scale,
    directional_sound_pos, distance_3d, distance_sqr_3d, format_distance, view_direction,
    world_to_screen,
};
use crate::payloads::{PingLocationC2S, PingLocationS2C, UpdateChannelC2S, channels};
use crate::screen;
use mod_api::client::hud_layout::{Blend, PoseOp};
use mod_api::text::TextStyle;
use mod_api::{
    ClientCommandRegistration, ClientHost, ClipBlock, ClipFluid, Command, CommandArgumentValue,
    CrosshairTarget, DrawContext, GuiBlitRegion, GuiCanvas, GuiDrawError, Identifier, ItemStack,
    KeyConflictContext, KeyRegistration, ModKeyMappingRef, PackType, Registrar, SoundPlay,
    SoundRef, SoundSource, SoundState, Text, VanillaKeyMappingRef,
};
use std::collections::VecDeque;
use std::f32::consts::{FRAC_PI_4, PI};
use std::sync::{Mutex, MutexGuard, OnceLock};

pub const KEY_PING_LOCATION: &str = "ping_location";
pub const KEY_OPEN_SETTINGS: &str = "open_settings";

const SHADOW_BLACK: u32 = 0x4000_0000;
const ARROW_SPRITE: &str = "pingwheel:arrow";
const PING_TEXTURE: &str = "pingwheel:textures/ping.png";
const PING_SOUND: &str = "pingwheel:ping";

/// Cap on queued incoming `PingLocationS2C` packets before dropping the oldest.
/// Original Ping-Wheel (`6447533`) has no explicit cap on `Game.execute` runnables;
/// 128 bounds memory under a malicious server flood while comfortably exceeding
/// `ServerConfig.rateLimit` (default 5).
const MAX_PENDING_INCOMING: usize = 128;

/// Maximum incoming `PingLocationS2C` packets drained per client tick.
/// Original Ping-Wheel (`6447533`) drains `Game.execute` unboundedly; 16 is above
/// `ServerConfig.rateLimit` (default 5) while keeping per-tick work bounded.
const MAX_INCOMING_PER_TICK: usize = 16;

/// Cap on active HUD pings in `ClientState.pings` before dropping the oldest.
/// Original Ping-Wheel (`6447533`) leaves `PingManager.PING_REPO` uncapped;
/// 128 bounds HUD memory and per-frame rendering work under a malicious server.
const MAX_ACTIVE_PINGS: usize = 128;

/// Cap on tracked directional ping sounds before stopping and dropping the oldest.
/// Original Ping-Wheel (`6447533`) has no explicit cap on `DirectionalSoundInstance`;
/// vanilla 26.3's `SoundEngine` / `Library` caps static OpenAL channels at 247, and
/// 32 keeps one mod well within that budget.
const MAX_ACTIVE_SOUNDS: usize = 32;

/// Maximum directional ping sounds started in a single client tick, matching
/// Ping-Wheel (`6447533`) default `ServerConfig.rateLimit` (5).
const MAX_SOUNDS_PER_TICK: usize = 5;

struct PingView {
    pos: [f64; 3],
    entity_id: Option<u32>,
    author_uuid: u128,
    author_name: String,
    team_color: Option<u32>,
    item_stack: Option<ItemStack>,
    sequence: i32,
    dimension: String,
    inserted_order: u64,
    spawn_time: Option<i64>,
    age: i64,
    screen_pos: Option<ScreenPos>,
    distance: f64,
    scale: f32,
}

impl PingView {
    fn from_packet(packet: PingLocationS2C, inserted_order: u64) -> Self {
        Self {
            pos: packet.pos,
            entity_id: packet.entity_id,
            author_uuid: packet.author,
            author_name: packet.author_name,
            team_color: packet.team_color,
            item_stack: packet.item_stack,
            sequence: packet.sequence,
            dimension: packet.dimension,
            inserted_order,
            spawn_time: None,
            age: 0,
            screen_pos: None,
            distance: 0.0,
            scale: 1.0,
        }
    }

    fn is_expired(&self, cfg: &ClientConfig) -> bool {
        cfg.ping_duration < MAX_PING_DURATION && self.age > i64::from(cfg.ping_duration) * TPS
    }

    fn is_removable(&self, cfg: &ClientConfig, screen_size: [f32; 2]) -> bool {
        let past_correction = cfg.correction_period >= MAX_CORRECTION_PERIOD
            || (self.age as f32) > cfg.correction_period * (TPS as f32);
        if !past_correction {
            return false;
        }
        self.distance_to_center(screen_size) < cfg.remove_radius as f32
    }

    fn distance_to_center(&self, screen_size: [f32; 2]) -> f32 {
        match self.screen_pos {
            Some(sp) => sp.distance_to([screen_size[0] * 0.5, screen_size[1] * 0.5]),
            None => f32::MAX,
        }
    }

    fn effective_team_color(&self) -> u32 {
        self.team_color.unwrap_or(WHITE)
    }
}

struct ActiveSound {
    handle: SoundRef,
    ping_pos: [f64; 3],
}

#[derive(Default)]
struct ClientState {
    pings: Vec<PingView>,
    next_ping_order: u64,
    pending_incoming: VecDeque<PingLocationS2C>,
    active_sounds: VecDeque<ActiveSound>,
    ping_queued: bool,
    ping_sequence: i32,
    last_ping_time: i64,
    last_screen_size: [f32; 2],
    settings_synced: bool,
    has_custom_ping_texture: bool,
    custom_texture_checked: bool,
    key_ping: Option<ModKeyMappingRef>,
    key_settings: Option<ModKeyMappingRef>,
    key_pick_item: Option<VanillaKeyMappingRef>,
    key_playerlist: Option<VanillaKeyMappingRef>,
}

fn push_bounded<T>(queue: &mut VecDeque<T>, max_len: usize, item: T) -> Option<T> {
    let evicted = if queue.len() >= max_len {
        queue.pop_front()
    } else {
        None
    };
    if max_len > 0 {
        queue.push_back(item);
    }
    evicted
}

impl ClientState {
    fn push_pending_incoming(&mut self, packet: PingLocationS2C) {
        push_bounded(&mut self.pending_incoming, MAX_PENDING_INCOMING, packet);
    }

    fn drain_incoming_batch(&mut self) -> Vec<PingLocationS2C> {
        let count = self.pending_incoming.len().min(MAX_INCOMING_PER_TICK);
        self.pending_incoming.drain(..count).collect()
    }

    fn upsert_ping(&mut self, packet: PingLocationS2C) {
        let order = self.next_ping_order;
        self.next_ping_order = self.next_ping_order.wrapping_add(1);
        let view = PingView::from_packet(packet, order);

        if let Some(existing) = self
            .pings
            .iter_mut()
            .find(|p| p.author_uuid == view.author_uuid && p.sequence == view.sequence)
        {
            *existing = view;
            return;
        }

        while self.pings.len() >= MAX_ACTIVE_PINGS {
            if let Some((oldest_idx, _)) = self
                .pings
                .iter()
                .enumerate()
                .min_by_key(|(_, p)| p.inserted_order)
            {
                self.pings.swap_remove(oldest_idx);
            } else {
                break;
            }
        }
        self.pings.push(view);
    }
}

static CLIENT_STATE: OnceLock<Mutex<ClientState>> = OnceLock::new();

fn lock_state() -> MutexGuard<'static, ClientState> {
    CLIENT_STATE
        .get_or_init(|| Mutex::new(ClientState::default()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

pub fn register_key_mappings(r: &mut dyn Registrar) {
    r.register_key_mapping(
        KeyRegistration::new(KEY_PING_LOCATION, "key.mouse.5")
            .label("key.pingwheel.ping_location")
            .category("key.category.pingwheel.name")
            .context(KeyConflictContext::InGame),
    );
    r.register_key_mapping(
        KeyRegistration::new(KEY_OPEN_SETTINGS, "key.keyboard.unknown")
            .label("key.pingwheel.open_settings")
            .category("key.category.pingwheel.name")
            .context(KeyConflictContext::InGame),
    );
}

pub fn on_resources_reloaded() {
    let mut state = lock_state();
    state.custom_texture_checked = false;
}

pub fn on_incoming_ping_packet(packet: PingLocationS2C) {
    if !packet.pos.iter().all(|v| v.is_finite()) {
        return;
    }
    let mut state = lock_state();
    state.push_pending_incoming(packet);
}

pub fn on_client_logging_in(host: &mut dyn ClientHost) {
    {
        let mut state = lock_state();
        state.pings.clear();
        state.pending_incoming.clear();
        state.active_sounds.clear();
        state.ping_queued = false;
        state.key_ping = host.mod_key_mapping(MOD_ID, KEY_PING_LOCATION).ok();
        state.key_settings = host.mod_key_mapping(MOD_ID, KEY_OPEN_SETTINGS).ok();
        state.key_pick_item = host.vanilla_key_mapping("key.pickItem").ok();
        state.key_playerlist = host.vanilla_key_mapping("key.playerlist").ok();
    }
    send_channel_update(host);
}

pub fn on_client_disconnecting() {
    let mut state = lock_state();
    state.pings.clear();
    state.pending_incoming.clear();
    state.active_sounds.clear();
    state.ping_queued = false;
}

pub fn send_channel_update(host: &mut dyn ClientHost) {
    let Some(ch) = channels() else {
        return;
    };
    let server_addr = current_server_address(host);
    let channel = client_config().active_channel(server_addr.as_deref());
    let _ = ch
        .update_channel_c2s
        .send(host, &UpdateChannelC2S { channel });
}

fn current_server_address(host: &dyn ClientHost) -> Option<String> {
    host.client_session()
        .client_server()
        .and_then(|s| s.address)
        .map(str::to_owned)
}

pub fn on_client_tick_pre(host: &mut dyn ClientHost) {
    let cfg = client_config();

    // Sync config -> Options > Mods once on startup, and check custom ping.png override.
    {
        let mut state = lock_state();
        if !state.settings_synced {
            state.settings_synced = true;
            cfg.write_to_host_settings(host);
        }
        if !state.custom_texture_checked {
            state.custom_texture_checked = true;
            state.has_custom_ping_texture = check_custom_ping_texture(&*host);
        }
        if state.key_ping.is_none() {
            state.key_ping = host.mod_key_mapping(MOD_ID, KEY_PING_LOCATION).ok();
        }
        if state.key_settings.is_none() {
            state.key_settings = host.mod_key_mapping(MOD_ID, KEY_OPEN_SETTINGS).ok();
        }
        if state.key_pick_item.is_none() {
            state.key_pick_item = host.vanilla_key_mapping("key.pickItem").ok();
        }
        if state.key_playerlist.is_none() {
            state.key_playerlist = host.vanilla_key_mapping("key.playerlist").ok();
        }
    }

    // If SettingsScreen edited the channel and closed via Escape, sync it now.
    if host.mod_screen() != Some(MOD_ID) && screen::take_channel_dirty() {
        send_channel_update(host);
    }

    // Drain a bounded batch of incoming S2C pings per tick.
    let incoming: Vec<PingLocationS2C> = {
        let mut state = lock_state();
        state.drain_incoming_batch()
    };
    if !incoming.is_empty() {
        let active_channel = cfg.active_channel(current_server_address(&*host).as_deref());
        let player_pos = host
            .local_player()
            .and_then(|p| p.entity().position(&*host));
        let current_dim = host.dimension();
        let mut sounds_played = 0usize;

        for packet in incoming {
            if packet.channel != active_channel {
                continue;
            }
            if cfg.ping_distance < MAX_PING_DISTANCE
                && let Some(ppos) = player_pos
                && distance_3d(ppos, packet.pos) > f64::from(cfg.ping_distance)
            {
                continue;
            }
            let same_dim = packet.dimension == current_dim;
            let ping_pos = packet.pos;

            let mut state = lock_state();
            state.upsert_ping(packet);

            if same_dim
                && cfg.ping_volume > 0
                && sounds_played < MAX_SOUNDS_PER_TICK
                && let Some(ppos) = player_pos
            {
                let sound_pos = directional_sound_pos(ppos, ping_pos);
                let vol = (cfg.ping_volume as f32 / 100.0).clamp(0.0, 1.0);
                let play = SoundPlay::at(PING_SOUND, SoundSource::Master, sound_pos)
                    .with_volume(vol)
                    .with_pitch(1.0);
                if let Some(handle) = host.play_local_sound(&play, 0) {
                    sounds_played += 1;
                    if let Some(evicted) = push_bounded(
                        &mut state.active_sounds,
                        MAX_ACTIVE_SOUNDS,
                        ActiveSound { handle, ping_pos },
                    ) {
                        let _ = evicted.handle.stop(&*host);
                    }
                }
            }
        }
    }

    // Update playing directional ping sounds.
    if let Some(player_pos) = host
        .local_player()
        .and_then(|p| p.entity().position(&*host))
    {
        let mut state = lock_state();
        state
            .active_sounds
            .retain(|active| match active.handle.state(&*host) {
                Some(SoundState::Playing) => {
                    let pos = directional_sound_pos(player_pos, active.ping_pos);
                    let _ = active.handle.move_to(&*host, pos);
                    true
                }
                Some(SoundState::Waiting) => true,
                _ => false,
            });
    }

    // Poll Open Settings keybind.
    let open_settings_pressed = {
        let key = lock_state().key_settings;
        key.and_then(|k| k.consume_click(host).ok())
            .unwrap_or(false)
    };
    if open_settings_pressed {
        let _ = host.mod_screen_open(MOD_ID);
    }

    // Poll Ping Location keybind (with pick-item conflict sharing).
    if consume_ping_hotkey(host) {
        lock_state().ping_queued = true;
    }

    // Track entity positions and expire old pings.
    update_tracked_entities_and_expire(host, &cfg);

    // Handle queued ping action (remove existing ping under crosshair or send new ping).
    let should_ping = {
        let mut state = lock_state();
        if !state.ping_queued {
            false
        } else {
            let screen_size = state.last_screen_size;
            let mut remove_idx: Option<usize> = None;
            let mut best_dist = f32::MAX;
            for (idx, ping) in state.pings.iter().enumerate() {
                if ping.is_removable(&cfg, screen_size) {
                    let d = ping.distance_to_center(screen_size);
                    if d < best_dist {
                        best_dist = d;
                        remove_idx = Some(idx);
                    }
                }
            }
            if let Some(idx) = remove_idx {
                state.pings.remove(idx);
                state.ping_queued = false;
                false
            } else {
                true
            }
        }
    };

    if should_ping {
        perform_ping_action(host, &cfg);
    }
}

/// Whether a resource pack overrides the shipped `textures/ping.png`, which the HUD then
/// blits directly (`GuiCanvas::texture` reads standalone textures outside the GUI atlas).
fn check_custom_ping_texture(host: &dyn ClientHost) -> bool {
    let Ok(tex_id) = Identifier::parse(PING_TEXTURE) else {
        return false;
    };
    host.resource_stack(PackType::ClientResources, &tex_id)
        .len()
        > 1
}

fn consume_ping_hotkey(host: &mut dyn ClientHost) -> bool {
    let (key_ping, key_pick) = {
        let state = lock_state();
        (state.key_ping, state.key_pick_item)
    };
    let Some(key_ping) = key_ping else {
        return false;
    };

    let same_as_pick = match (
        key_ping.binding(&*host),
        key_pick.and_then(|p| p.binding(&*host).ok()),
    ) {
        (Ok((ping_bind, mods)), Some(pick_bind)) => {
            ping_bind == pick_bind && mods.bits == 0 && ping_bind != "key.keyboard.unknown"
        }
        _ => false,
    };

    if !same_as_pick {
        return key_ping.consume_click(host).unwrap_or(false);
    }

    // Shared with Pick Item: only trigger ping on miss, or on entity when not in creative.
    let is_creative = host
        .local_player()
        .and_then(|p| p.is_creative(&*host))
        .unwrap_or(false);
    let allow = match host.crosshair_target() {
        None => true,
        Some(CrosshairTarget::Entity { .. }) => !is_creative,
        Some(_) => false,
    };

    if allow {
        key_ping.consume_click(host).unwrap_or(false)
    } else {
        // Consume any stray click on our mapping so it does not fire on a later tick.
        let _ = key_ping.consume_click(host);
        false
    }
}

fn update_tracked_entities_and_expire(host: &dyn ClientHost, cfg: &ClientConfig) {
    let game_time = host.clock().map(|c| c.game_time).unwrap_or(0);
    let mut state = lock_state();
    for ping in &mut state.pings {
        let spawn = *ping.spawn_time.get_or_insert(game_time);
        ping.age = game_time.saturating_sub(spawn);
        if let Some(ent_id) = ping.entity_id
            && let Some(ent) = host.entity(ent_id)
            && let Some(pos) = ent.position(host)
        {
            let top_y = ent
                .bounding_box(host)
                .map(|[min, max]| pos[1] + (max[1] - min[1]))
                .unwrap_or(pos[1]);
            ping.pos = [pos[0], top_y, pos[2]];
        }
    }
    state.pings.retain(|p| !p.is_expired(cfg));
}

/// Clip modes for a ping ray: standing uses the visual block shape (`ClipContext.Block.VISUAL`)
/// and ignores fluids; sneaking uses block outlines and stops at any fluid
/// (`ClipContext.Fluid.ANY`).
fn clip_modes(sneaking: bool) -> (ClipBlock, ClipFluid) {
    if sneaking {
        (ClipBlock::Outline, ClipFluid::Any)
    } else {
        (ClipBlock::Visual, ClipFluid::None)
    }
}

fn perform_ping_action(host: &mut dyn ClientHost, cfg: &ClientConfig) {
    let game_time = host.clock().map(|c| c.game_time).unwrap_or(0);
    let sequence = {
        let mut state = lock_state();
        state.ping_queued = false;
        let elapsed = game_time.saturating_sub(state.last_ping_time);
        if cfg.correction_period < MAX_CORRECTION_PERIOD
            && (elapsed as f32) > cfg.correction_period * (TPS as f32)
        {
            state.ping_sequence = state.ping_sequence.wrapping_add(1);
        }
        state.last_ping_time = game_time;
        state.ping_sequence
    };

    let Some(camera) = host.camera() else {
        return;
    };
    let Some(cam_entity) = host.camera_entity() else {
        return;
    };
    let max_dist = f64::from(cfg.raycast_distance.min(cfg.ping_distance));
    if max_dist <= 0.0 {
        return;
    }

    let start = camera.position;
    let dir = view_direction(camera.yaw, camera.pitch);
    let end = [
        start[0] + dir[0] * max_dist,
        start[1] + dir[1] * max_dist,
        start[2] + dir[2] * max_dist,
    ];
    let hit_translucent = cam_entity.is_shift_key_down(&*host).unwrap_or(false);
    let (clip_block, clip_fluid) = clip_modes(hit_translucent);

    let level = host.level(&camera.dimension);

    let block_hit_pos: Option<[f64; 3]> = level
        .and_then(|lvl| lvl.clip(&*host, start, end, clip_block, clip_fluid))
        .and_then(|hit| (!hit.miss).then_some(hit.location))
        .or_else(|| host.raycast(max_dist as f32).map(|h| [h.x, h.y, h.z]));

    let entity_hit = level.and_then(|lvl| {
        lvl.ray_trace_nearest_entity(
            &*host,
            start,
            end,
            0.25,
            Some(cam_entity),
            // `Raycast.traceEntity` skips `Entity.isSpectator()` targets.
            &mut |e| e.spectator(&*host) != Some(true),
        )
        .ok()
        .flatten()
    });

    let (target_pos, target_entity_id) = match (block_hit_pos, entity_hit) {
        (Some(bpos), Some(ehit)) => {
            if distance_sqr_3d(start, bpos) < distance_sqr_3d(start, ehit.location) {
                (bpos, None)
            } else {
                (ehit.location, Some(ehit.entity.id()))
            }
        }
        (Some(bpos), None) => (bpos, None),
        (None, Some(ehit)) => (ehit.location, Some(ehit.entity.id())),
        (None, None) => return,
    };

    let Some(ch) = channels() else {
        return;
    };
    let channel = cfg.active_channel(current_server_address(&*host).as_deref());
    let _ = ch.ping_location_c2s.send(
        host,
        &PingLocationC2S {
            channel,
            pos: target_pos,
            entity_id: target_entity_id,
            sequence,
        },
    );
}

pub fn on_render_gui_post(host: &dyn ClientHost, draw: &mut dyn DrawContext) {
    let cfg = client_config();
    if cfg.ping_distance == 0 {
        return;
    }
    let Some(camera) = host.camera() else {
        return;
    };
    let screen_size = draw.size();
    if screen_size[0] <= 0.0 || screen_size[1] <= 0.0 {
        return;
    }

    let has_arrow_sprite = Identifier::parse(ARROW_SPRITE)
        .ok()
        .and_then(|id| host.gui_sprite(&id))
        .is_some();

    let (playerlist_down, has_custom_ping) = {
        let mut state = lock_state();
        state.last_screen_size = screen_size;
        if state.pings.is_empty() {
            return;
        }
        for ping in &mut state.pings {
            ping.screen_pos = Some(world_to_screen(ping.pos, &camera, screen_size));
            ping.distance = distance_3d(camera.position, ping.pos);
            ping.scale = calculate_ping_scale(ping.distance, cfg.ping_size);
        }
        // Sort back-to-front so closer pings render on top.
        state.pings.sort_by(|a, b| {
            b.distance
                .partial_cmp(&a.distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let held = state
            .key_playerlist
            .and_then(|k| k.is_down(host).ok())
            .unwrap_or(false);
        (held, state.has_custom_ping_texture)
    };

    let ping_texture = has_custom_ping
        .then(|| Identifier::parse(PING_TEXTURE).ok())
        .flatten();

    let safe_tl = [cfg.safe_zone_left as f32, cfg.safe_zone_top as f32];
    let safe_br = [
        (screen_size[0] - cfg.safe_zone_right as f32).max(safe_tl[0] + 1.0),
        (screen_size[1] - cfg.safe_zone_bottom as f32).max(safe_tl[1] + 1.0),
    ];
    let safe_center = [
        (safe_br[0] - safe_tl[0]) * 0.5,
        (safe_br[1] - safe_tl[1]) * 0.5,
    ];

    let label_use_team_color = matches!(
        cfg.team_color_mode,
        TeamColorMode::Full | TeamColorMode::LabelsOnly
    );
    let ping_use_team_color = matches!(
        cfg.team_color_mode,
        TeamColorMode::Full | TeamColorMode::PingOnly
    );
    let show_verbose_player = cfg.player_info_mode == PlayerInfoMode::Always
        || (cfg.player_info_mode == PlayerInfoMode::Hold && playerlist_down);
    let compact_player = cfg.player_info_mode == PlayerInfoMode::Compact;

    let state = lock_state();
    for ping in &state.pings {
        if ping.dimension != camera.dimension {
            continue;
        }
        let Some(sp) = ping.screen_pos else {
            continue;
        };
        let behind = sp.is_behind_camera();
        if behind && !cfg.direction_indicator_visible {
            continue;
        }

        let team_argb = ping.effective_team_color();
        let ping_color = if ping_use_team_color {
            team_argb
        } else {
            WHITE
        };
        let distance_color = if label_use_team_color {
            team_argb
        } else {
            WHITE
        };

        let off_screen = behind || !sp.is_in_bounds([0.0, 0.0], screen_size);

        if cfg.direction_indicator_visible && off_screen {
            let mut dir_x = sp.x - safe_tl[0] - safe_center[0];
            let mut dir_y = sp.y - safe_tl[1] - safe_center[1];
            if behind {
                dir_x = -dir_x;
                dir_y = -dir_y;
            }
            let angle = dir_y.atan2(dir_x);
            let edge = calculate_angle_rect_intersection(angle, safe_tl, safe_br);
            let offset_x = (angle + PI).cos() * 12.0;
            let offset_y = (angle + PI).sin() * 12.0;
            let ping_size = cfg.ping_size as f32 / 100.0;
            let draw_item = cfg.item_icon_visible && ping.item_stack.is_some();

            let _ = draw.draw_gui(&mut |canvas| {
                canvas.scope(
                    &[PoseOp::Translate(edge[0], edge[1])],
                    None,
                    &mut |at_edge| {
                        if !draw_item {
                            at_edge.scope(
                                &[
                                    PoseOp::Scale(ping.scale, ping.scale),
                                    PoseOp::Translate(offset_x, offset_y),
                                ],
                                None,
                                &mut |icon| draw_ping_icon(icon, ping_texture.as_ref(), ping_color),
                            )?;
                        }
                        // Direction arrow
                        at_edge.scope(
                            &[
                                PoseOp::Rotate(angle),
                                PoseOp::Scale(ping_size * 0.25, ping_size * 0.25),
                                PoseOp::Translate(-5.0, 0.0),
                            ],
                            None,
                            &mut |arrow| {
                                if has_arrow_sprite {
                                    arrow.blit_sprite(
                                        ARROW_SPRITE,
                                        [-5.0, -5.0, 10.0, 10.0],
                                        None,
                                        ping_color,
                                        Blend::Textured,
                                    )
                                } else {
                                    arrow.fill([-4.0, -4.0, 8.0, 8.0], ping_color)
                                }
                            },
                        )
                    },
                )
            });

            if let Some(stack) = ping.item_stack.as_ref().filter(|_| cfg.item_icon_visible) {
                let item_size = 16.0 * ping.scale;
                let ix = edge[0] + offset_x * ping.scale - item_size * 0.5;
                let iy = edge[1] + offset_y * ping.scale - item_size * 0.5;
                let _ = draw.item_stack(stack, [ix, iy], item_size);
            }
        }

        if !behind {
            let dist_str = format_meters(host, ping.distance);
            let dist_face = compact_player.then_some(ping.author_uuid);
            let dist_label = Label {
                text: &dist_str,
                width: label_width(draw, &dist_str, dist_face.is_some()),
                y_offset: -1.5,
                player_face: dist_face,
                color: distance_color,
            };
            let name_label = (show_verbose_player && !ping.author_name.is_empty()).then(|| Label {
                text: &ping.author_name,
                width: label_width(draw, &ping.author_name, true),
                y_offset: 1.75,
                player_face: Some(ping.author_uuid),
                color: if label_use_team_color {
                    team_argb
                } else {
                    WHITE
                },
            });
            let draw_item = cfg.item_icon_visible && ping.item_stack.is_some();

            let _ = draw.draw_gui(&mut |canvas| {
                canvas.scope(
                    &[
                        PoseOp::Translate(sp.x, sp.y),
                        PoseOp::Scale(ping.scale, ping.scale),
                    ],
                    None,
                    &mut |at_ping| {
                        draw_label(at_ping, &dist_label)?;
                        if !draw_item {
                            draw_ping_icon(at_ping, ping_texture.as_ref(), ping_color)?;
                        }
                        if let Some(label) = &name_label {
                            draw_label(at_ping, label)?;
                        }
                        Ok(())
                    },
                )
            });

            if let Some(stack) = ping.item_stack.as_ref().filter(|_| cfg.item_icon_visible) {
                let item_size = 16.0 * ping.scale;
                let ix = sp.x - item_size * 0.5;
                let iy = sp.y - item_size * 0.5;
                let _ = draw.item_stack(stack, [ix, iy], item_size);
            }
        }
    }
}

fn format_meters(host: &dyn ClientHost, distance: f64) -> String {
    let formatted = format_distance(distance);
    match host.translate("unit.pingwheel.meters") {
        Some(template) if template.contains("%s") => template.replacen("%s", &formatted, 1),
        _ => format!("{formatted}m"),
    }
}

fn draw_ping_icon(
    canvas: &mut dyn GuiCanvas,
    custom_texture: Option<&Identifier>,
    color: u32,
) -> Result<(), GuiDrawError> {
    match custom_texture {
        // The whole texture, whatever its pixel size: uv 0..1 over a 1x1 divisor.
        Some(texture) => canvas.texture(
            texture,
            [-6.0, -6.0, 12.0, 12.0],
            GuiBlitRegion::new([0.0, 0.0], [1, 1], [1, 1]),
            color,
        ),
        None => canvas.scope(
            &[PoseOp::Rotate(FRAC_PI_4), PoseOp::Translate(-2.5, -2.5)],
            None,
            &mut |diamond| diamond.fill([0.0, 0.0, 5.0, 5.0], color),
        ),
    }
}

/// A ping label: text on a translucent plate, optionally led by the author's head.
struct Label<'a> {
    text: &'a str,
    /// The plate's text width in GUI pixels, including the head.
    width: i32,
    y_offset: f32,
    player_face: Option<u128>,
    color: u32,
}

fn label_width(draw: &dyn DrawContext, text: &str, with_face: bool) -> i32 {
    let extra_w: i32 = if with_face { 10 } else { 0 };
    draw.text_width(text).ceil() as i32 + extra_w
}

fn draw_label(canvas: &mut dyn GuiCanvas, label: &Label<'_>) -> Result<(), GuiDrawError> {
    let extra_w: i32 = if label.player_face.is_some() { 10 } else { 0 };
    let line_h = 9_i32;
    let ox = -(label.width as f32) * 0.5;
    let oy = -(line_h as f32) * 0.5 + (line_h as f32) * label.y_offset;

    canvas.scope(&[PoseOp::Translate(ox, oy)], None, &mut |plate| {
        plate.fill(
            [-2.0, -2.0, (label.width + 3) as f32, (line_h + 2) as f32],
            SHADOW_BLACK,
        )?;
        plate.text(
            &Text::literal(label.text),
            [extra_w as f32, 0.0],
            label.color,
            false,
        )?;
        if let Some(uuid) = label.player_face {
            plate.player_face(uuid, [0.0, 0.0], 8.0, true, false, WHITE)?;
        }
        Ok(())
    })
}

pub fn register_client_commands(commands: &mut Vec<ClientCommandRegistration>) {
    commands.push(Command::new("pingwheel").executes_client(|host, _| {
        send_client_help(host);
        Some(1)
    }));
    commands.push(Command::new("pingwheel help").executes_client(|host, _| {
        send_client_help(host);
        Some(1)
    }));
    commands.push(Command::new("pingwheel config").executes_client(|host, _| {
        let _ = host.mod_screen_open(MOD_ID);
        Some(1)
    }));
    commands.push(
        Command::new("pingwheel channel").executes_client(|host, _| {
            let addr = current_server_address(&*host);
            let ch = client_config().active_channel(addr.as_deref());
            let line = if ch.is_empty() {
                Text::translate("command.pingwheel.channel.get.response.default", Vec::new())
            } else {
                Text::translate(
                    "command.pingwheel.channel.get.response",
                    vec![colored(Text::literal(ch), "yellow")],
                )
            };
            let _ = host.chat_line(line);
            Some(1)
        }),
    );
    commands.push(
        Command::new("pingwheel channel <channel_name:string>").executes_client(
            |host, invocation| {
                let Some(CommandArgumentValue::String(new_channel)) =
                    invocation.argument("channel_name")
                else {
                    return None;
                };
                if new_channel.chars().count() > MAX_CHANNEL_LENGTH {
                    let _ = host.chat_line(colored(
                        Text::translate(
                            "command.pingwheel.channel.set.reject",
                            vec![Text::literal(MAX_CHANNEL_LENGTH.to_string())],
                        ),
                        "red",
                    ));
                    return None;
                }
                let addr = current_server_address(&*host);
                let updated = update_client_config(|c| {
                    c.set_active_channel(addr.as_deref(), new_channel);
                });
                persist_client_config(host.config_dir(), &updated);
                send_channel_update(host);

                let line = if new_channel.is_empty() {
                    Text::translate("command.pingwheel.channel.set.response.default", Vec::new())
                } else {
                    Text::translate(
                        "command.pingwheel.channel.set.response",
                        vec![colored(Text::literal(new_channel.clone()), "yellow")],
                    )
                };
                let _ = host.chat_line(line);
                Some(1)
            },
        ),
    );
}

fn send_client_help(host: &mut dyn ClientHost) {
    let entries = [
        ("/pingwheel config", "command.pingwheel.config.description"),
        (
            "/pingwheel channel",
            "command.pingwheel.channel.get.description",
        ),
        (
            "/pingwheel channel <channel_name>",
            "command.pingwheel.channel.set.description",
        ),
    ];
    for (usage, desc_key) in entries {
        let line = Text::literal(format!("{usage} - "))
            .append(colored(Text::translate(desc_key, Vec::new()), "gray"));
        let _ = host.chat_line(line);
    }
}

fn colored(text: Text, color: &str) -> Text {
    let mut style = TextStyle::default();
    style.color = Some(color.to_owned());
    text.with_style(style)
}

#[cfg(test)]
mod tests {
    #[test]
    fn clip_modes_match_original_raycast() {
        assert!(matches!(
            super::clip_modes(false),
            (ClipBlock::Visual, ClipFluid::None)
        ));
        assert!(matches!(
            super::clip_modes(true),
            (ClipBlock::Outline, ClipFluid::Any)
        ));
    }

    use super::*;

    fn sample_packet(sequence: i32) -> PingLocationS2C {
        PingLocationS2C {
            channel: String::new(),
            pos: [f64::from(sequence), 64.0, 0.0],
            entity_id: None,
            sequence,
            dimension: "minecraft:overworld".to_owned(),
            author: 1,
            author_name: "Player".to_owned(),
            team_color: None,
            item_stack: None,
        }
    }

    #[test]
    fn pending_incoming_caps_and_evicts_oldest() {
        let mut state = ClientState::default();
        let total = MAX_PENDING_INCOMING + 10;
        for seq in 0..total as i32 {
            state.push_pending_incoming(sample_packet(seq));
        }
        assert_eq!(state.pending_incoming.len(), MAX_PENDING_INCOMING);
        assert_eq!(state.pending_incoming.front().map(|p| p.sequence), Some(10));
        assert_eq!(
            state.pending_incoming.back().map(|p| p.sequence),
            Some(total as i32 - 1)
        );
    }

    #[test]
    fn drain_incoming_batch_is_bounded_per_tick() {
        let mut state = ClientState::default();
        let total = MAX_INCOMING_PER_TICK + 5;
        for seq in 0..total as i32 {
            state.push_pending_incoming(sample_packet(seq));
        }

        let first = state.drain_incoming_batch();
        assert_eq!(first.len(), MAX_INCOMING_PER_TICK);
        assert_eq!(first.first().map(|p| p.sequence), Some(0));
        assert_eq!(
            first.last().map(|p| p.sequence),
            Some(MAX_INCOMING_PER_TICK as i32 - 1)
        );
        assert_eq!(state.pending_incoming.len(), 5);

        let second = state.drain_incoming_batch();
        assert_eq!(second.len(), 5);
        assert_eq!(
            second.first().map(|p| p.sequence),
            Some(MAX_INCOMING_PER_TICK as i32)
        );
        assert!(state.pending_incoming.is_empty());
    }

    #[test]
    fn active_pings_caps_and_evicts_oldest_even_after_sort() {
        let mut state = ClientState::default();
        for seq in 0..MAX_ACTIVE_PINGS as i32 {
            state.upsert_ping(sample_packet(seq));
        }
        assert_eq!(state.pings.len(), MAX_ACTIVE_PINGS);

        // Simulate render sorting pings by distance (reversing insertion order).
        for (idx, ping) in state.pings.iter_mut().enumerate() {
            ping.distance = idx as f64;
        }
        state.pings.sort_by(|a, b| {
            b.distance
                .partial_cmp(&a.distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Updating an existing (author, sequence) ping replaces in place without evicting.
        let mut replacement = sample_packet(5);
        replacement.pos = [99.0, 64.0, 99.0];
        state.upsert_ping(replacement);
        assert_eq!(state.pings.len(), MAX_ACTIVE_PINGS);
        assert!(state.pings.iter().any(|p| p.sequence == 0));

        // Adding a brand-new ping evicts the oldest (sequence 0), not the farthest.
        state.upsert_ping(sample_packet(MAX_ACTIVE_PINGS as i32));
        assert_eq!(state.pings.len(), MAX_ACTIVE_PINGS);
        assert!(!state.pings.iter().any(|p| p.sequence == 0));
        assert!(
            state
                .pings
                .iter()
                .any(|p| p.sequence == MAX_ACTIVE_PINGS as i32)
        );
    }

    #[test]
    fn active_sounds_queue_caps_and_evicts_oldest() {
        let mut queue = VecDeque::new();
        for i in 0..MAX_ACTIVE_SOUNDS {
            assert_eq!(push_bounded(&mut queue, MAX_ACTIVE_SOUNDS, i), None);
        }
        assert_eq!(queue.len(), MAX_ACTIVE_SOUNDS);
        assert_eq!(
            push_bounded(&mut queue, MAX_ACTIVE_SOUNDS, MAX_ACTIVE_SOUNDS),
            Some(0)
        );
        assert_eq!(queue.len(), MAX_ACTIVE_SOUNDS);
        assert_eq!(queue.front().copied(), Some(1));
        assert_eq!(queue.back().copied(), Some(MAX_ACTIVE_SOUNDS));
    }
}
