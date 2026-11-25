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
    ClientHost, ClipBlock, ClipFluid, Command, CrosshairTarget, DrawContext, Flow, GuiDraw,
    Identifier, ItemStack, KeyConflictContext, KeyRegistration, ModKeyMappingRef, PackType,
    Registrar, SoundPlay, SoundRef, SoundSource, SoundState, Text, VanillaKeyMappingRef,
};
use std::borrow::Cow;
use std::f32::consts::{FRAC_PI_4, PI};
use std::sync::{Mutex, MutexGuard, OnceLock};

pub const KEY_PING_LOCATION: &str = "ping_location";
pub const KEY_OPEN_SETTINGS: &str = "open_settings";

const SHADOW_BLACK: u32 = 0x4000_0000;
const ARROW_SPRITE: &str = "pingwheel:arrow";
const PING_SPRITE: &str = "pingwheel:ping";
const PING_SOUND: &str = "pingwheel:ping";

struct PingView {
    pos: [f64; 3],
    entity_id: Option<u32>,
    author_uuid: u128,
    author_name: String,
    team_color: Option<u32>,
    item_stack: Option<ItemStack>,
    sequence: i32,
    dimension: String,
    spawn_time: Option<i64>,
    age: i64,
    screen_pos: Option<ScreenPos>,
    distance: f64,
    scale: f32,
}

impl PingView {
    fn from_packet(packet: PingLocationS2C) -> Self {
        Self {
            pos: packet.pos,
            entity_id: packet.entity_id,
            author_uuid: packet.author,
            author_name: packet.author_name,
            team_color: packet.team_color,
            item_stack: packet.item_stack,
            sequence: packet.sequence,
            dimension: packet.dimension,
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
    pending_incoming: Vec<PingLocationS2C>,
    active_sounds: Vec<ActiveSound>,
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
    state.pending_incoming.push(packet);
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
            cfg.write_to_host_settings(&*host);
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

    // Drain incoming S2C pings.
    let incoming: Vec<PingLocationS2C> = {
        let mut state = lock_state();
        std::mem::take(&mut state.pending_incoming)
    };
    if !incoming.is_empty() {
        let active_channel = cfg.active_channel(current_server_address(&*host).as_deref());
        let player_pos = host
            .local_player()
            .and_then(|p| p.entity().position(&*host));
        let current_dim = host.dimension();

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
            let view = PingView::from_packet(packet);

            let mut state = lock_state();
            if let Some(existing) = state
                .pings
                .iter_mut()
                .find(|p| p.author_uuid == view.author_uuid && p.sequence == view.sequence)
            {
                *existing = view;
            } else {
                state.pings.push(view);
            }

            if same_dim
                && cfg.ping_volume > 0
                && let Some(ppos) = player_pos
            {
                let sound_pos = directional_sound_pos(ppos, ping_pos);
                let vol = (cfg.ping_volume as f32 / 100.0).clamp(0.0, 1.0);
                let play = SoundPlay::at(PING_SOUND, SoundSource::Master, sound_pos)
                    .with_volume(vol)
                    .with_pitch(1.0);
                if let Some(handle) = host.play_local_sound(&play, 0) {
                    state.active_sounds.push(ActiveSound { handle, ping_pos });
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

fn check_custom_ping_texture(host: &dyn ClientHost) -> bool {
    let Ok(tex_id) = Identifier::parse("pingwheel:textures/ping.png") else {
        return false;
    };
    let Ok(sprite_id) = Identifier::parse(PING_SPRITE) else {
        return false;
    };
    host.resource_stack(PackType::ClientResources, &tex_id)
        .len()
        > 1
        && host.gui_sprite(&sprite_id).is_some()
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
    let clip_block = if hit_translucent {
        ClipBlock::Outline
    } else {
        ClipBlock::Collider
    };

    let dimension = camera.dimension.clone();
    let level = host.level(&dimension);

    let block_hit_pos: Option<[f64; 3]> = level
        .and_then(|lvl| lvl.clip(&*host, start, end, clip_block, ClipFluid::None))
        .and_then(|hit| (!hit.miss).then_some(hit.location))
        .or_else(|| host.raycast(max_dist as f32).map(|h| [h.x, h.y, h.z]));

    let entity_hit = level.and_then(|lvl| {
        lvl.ray_trace_nearest_entity(&*host, start, end, 0.25, Some(cam_entity), &mut |_| true)
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
            dimension,
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

            let mut ops = Vec::new();
            ops.push(GuiDraw::Push(vec![PoseOp::Translate(edge[0], edge[1])]));

            let draw_item = cfg.item_icon_visible && ping.item_stack.is_some();
            if !draw_item {
                ops.push(GuiDraw::Push(vec![
                    PoseOp::Scale(ping.scale, ping.scale),
                    PoseOp::Translate(offset_x, offset_y),
                ]));
                push_ping_icon(&mut ops, has_custom_ping, ping_color);
                ops.push(GuiDraw::Pop);
            }

            // Direction arrow
            ops.push(GuiDraw::Push(vec![
                PoseOp::Rotate(angle),
                PoseOp::Scale(ping_size * 0.25, ping_size * 0.25),
                PoseOp::Translate(-5.0, 0.0),
            ]));
            if has_arrow_sprite {
                ops.push(GuiDraw::Sprite {
                    sprite: Cow::Borrowed(ARROW_SPRITE),
                    x: -5,
                    y: -5,
                    w: 10,
                    h: 10,
                    region: None,
                    color: ping_color,
                    blend: Blend::Textured,
                });
            } else {
                ops.push(GuiDraw::Fill {
                    x0: -4,
                    y0: -4,
                    x1: 4,
                    y1: 4,
                    color: ping_color,
                });
            }
            ops.push(GuiDraw::Pop);
            ops.push(GuiDraw::Pop);
            let _ = draw.gui_paint(&ops);

            if let Some(stack) = ping.item_stack.as_ref().filter(|_| cfg.item_icon_visible) {
                let item_size = 16.0 * ping.scale;
                let ix = edge[0] + offset_x * ping.scale - item_size * 0.5;
                let iy = edge[1] + offset_y * ping.scale - item_size * 0.5;
                let _ = draw.item_stack(stack, [ix, iy], item_size);
            }
        }

        if !behind {
            let dist_str = format_meters(host, ping.distance);
            let mut ops = Vec::new();
            ops.push(GuiDraw::Push(vec![
                PoseOp::Translate(sp.x, sp.y),
                PoseOp::Scale(ping.scale, ping.scale),
            ]));

            push_label(
                &mut ops,
                draw,
                &dist_str,
                -1.5,
                if compact_player {
                    Some(ping.author_uuid)
                } else {
                    None
                },
                distance_color,
            );

            let draw_item = cfg.item_icon_visible && ping.item_stack.is_some();
            if !draw_item {
                push_ping_icon(&mut ops, has_custom_ping, ping_color);
            }

            if show_verbose_player && !ping.author_name.is_empty() {
                let name_color = if label_use_team_color {
                    team_argb
                } else {
                    WHITE
                };
                push_label(
                    &mut ops,
                    draw,
                    &ping.author_name,
                    1.75,
                    Some(ping.author_uuid),
                    name_color,
                );
            }

            ops.push(GuiDraw::Pop);
            let _ = draw.gui_paint(&ops);

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

fn push_ping_icon(ops: &mut Vec<GuiDraw>, has_custom_ping: bool, color: u32) {
    if has_custom_ping {
        ops.push(GuiDraw::Sprite {
            sprite: Cow::Borrowed(PING_SPRITE),
            x: -6,
            y: -6,
            w: 12,
            h: 12,
            region: None,
            color,
            blend: Blend::Textured,
        });
    } else {
        ops.push(GuiDraw::Push(vec![
            PoseOp::Rotate(FRAC_PI_4),
            PoseOp::Translate(-2.5, -2.5),
        ]));
        ops.push(GuiDraw::Fill {
            x0: 0,
            y0: 0,
            x1: 5,
            y1: 5,
            color,
        });
        ops.push(GuiDraw::Pop);
    }
}

fn push_label(
    ops: &mut Vec<GuiDraw>,
    draw: &dyn DrawContext,
    text: &str,
    y_offset: f32,
    player_face: Option<u128>,
    color: u32,
) {
    let extra_w: i32 = if player_face.is_some() { 10 } else { 0 };
    let text_w = draw.text_width(text).ceil() as i32 + extra_w;
    let line_h = 9_i32;
    let ox = -(text_w as f32) * 0.5;
    let oy = -(line_h as f32) * 0.5 + (line_h as f32) * y_offset;

    ops.push(GuiDraw::Push(vec![PoseOp::Translate(ox, oy)]));
    ops.push(GuiDraw::Fill {
        x0: -2,
        y0: -2,
        x1: text_w + 1,
        y1: line_h,
        color: SHADOW_BLACK,
    });
    ops.push(GuiDraw::Text {
        text: Text::literal(text),
        x: extra_w,
        y: 0,
        color,
        shadow: false,
    });
    if let Some(uuid) = player_face {
        ops.push(GuiDraw::PlayerFace {
            uuid,
            x: 0,
            y: 0,
            size: 8,
            hat: true,
            flip: false,
            color: WHITE,
        });
    }
    ops.push(GuiDraw::Pop);
}

pub fn register_client_commands(commands: &mut Vec<Command>) {
    commands.push(Command::new("pingwheel"));
    commands.push(Command::new("pingwheel help"));
    commands.push(Command::new("pingwheel config"));
    commands.push(Command::new("pingwheel channel"));
    commands.push(Command::new("pingwheel channel <channel_name:string>"));
}

pub fn handle_client_command(host: &mut dyn ClientHost, raw_command: &str) -> Flow {
    let trimmed = raw_command.trim();
    if trimmed != "pingwheel" && !trimmed.starts_with("pingwheel ") {
        return Flow::Continue;
    }

    let rest = trimmed.strip_prefix("pingwheel").unwrap_or("").trim();
    if rest.is_empty() || rest == "help" {
        send_client_help(host);
        return Flow::Cancel;
    }

    if rest == "config" {
        let _ = host.mod_screen_open(MOD_ID);
        return Flow::Cancel;
    }

    if rest == "channel" {
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
        return Flow::Cancel;
    }

    if let Some(arg) = rest.strip_prefix("channel ") {
        let new_channel = parse_command_string_arg(arg.trim());
        if new_channel.chars().count() > MAX_CHANNEL_LENGTH {
            let _ = host.chat_line(colored(
                Text::translate(
                    "command.pingwheel.channel.set.reject",
                    vec![Text::literal(MAX_CHANNEL_LENGTH.to_string())],
                ),
                "red",
            ));
            return Flow::Cancel;
        }
        let addr = current_server_address(&*host);
        let updated = update_client_config(|c| {
            c.set_active_channel(addr.as_deref(), &new_channel);
        });
        persist_client_config(&*host, &updated);
        send_channel_update(host);

        let line = if new_channel.is_empty() {
            Text::translate("command.pingwheel.channel.set.response.default", Vec::new())
        } else {
            Text::translate(
                "command.pingwheel.channel.set.response",
                vec![colored(Text::literal(new_channel), "yellow")],
            )
        };
        let _ = host.chat_line(line);
        return Flow::Cancel;
    }

    send_client_help(host);
    Flow::Cancel
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

/// Decodes a Brigadier-style quotable string argument (`""`, `"my channel"`, or `word`).
pub fn parse_command_string_arg(raw: &str) -> String {
    if raw.len() >= 2
        && ((raw.starts_with('"') && raw.ends_with('"'))
            || (raw.starts_with('\'') && raw.ends_with('\'')))
    {
        let quote = raw.as_bytes()[0] as char;
        let inner = &raw[1..raw.len() - 1];
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    if next == quote || next == '\\' {
                        out.push(next);
                    } else {
                        out.push('\\');
                        out.push(next);
                    }
                }
            } else {
                out.push(ch);
            }
        }
        return out;
    }
    raw.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_and_unquoted_channel_args() {
        assert_eq!(parse_command_string_arg("\"\""), "");
        assert_eq!(parse_command_string_arg("\"squad alpha\""), "squad alpha");
        assert_eq!(parse_command_string_arg("'team_1'"), "team_1");
        assert_eq!(parse_command_string_arg("alpha"), "alpha");
        assert_eq!(parse_command_string_arg("\"a\\\"b\""), "a\"b");
    }
}
