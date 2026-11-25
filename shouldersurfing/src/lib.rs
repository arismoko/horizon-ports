#![deny(unsafe_code)]
//! Shoulder Surfing Reloaded for Sunrise: an over-the-shoulder third-person camera with a
//! decoupled view, a crosshair that follows what the player acts on, and the original's options.

mod camera;
mod config;
mod crosshair;
mod math;
mod pattern;
mod persp;
mod pick;

use crate::camera::{OffsetCfg, OffsetCtx, Offsets, Rotation};
use crate::config::{
    CrosshairType, K, Lists, MOD_ID, Origin, TurningMode, Values, Visibility, register_mod_configs,
    register_settings, write_bool, write_choice, write_f64,
};
use crate::crosshair::{Adaptive, MapLayout, Obstruction, StackFacts};
use crate::math::V3;
use crate::pattern::PatternList;
use crate::persp::{Enabled, Perspective};
use mod_api::client::hud_layout::Blend;
use mod_api::{
    BlockFace, BlockHit, BlockPos, CalculatePlayerTurnEvent, CameraDistanceEvent, CameraPoseEvent,
    CameraType, CameraTypeCycleEvent, ClientHost, ClientPickEvent, ClientTickPreEvent, ClipBlock,
    ClipFluid, ComputeFovEvent, ConfigContext, ConfigKind, ConfigLoadingEvent,
    ConfigReloadingEvent, CrosshairTarget, DrawContext, EntityRef, Flow, GuiDraw, Hand, Hooks,
    HostSide, HudMode, ItemStack, KeyConflictContext, KeyRegistration, LevelRef, ModKeyMappingRef,
    MovementInputUpdateEvent, RegisterConfigsEvent, RegisterGuiLayersEvent, Registrar,
    RegistrationContext, RenderFramePreEvent, RenderGuiPostEvent, RenderPlayerPreEvent,
    SettingChangedEvent, VanillaKeyMappingRef,
};
use std::borrow::Cow;
use std::sync::{LazyLock, Mutex, MutexGuard};

/// `(id, default key)` of the original's key mappings, in the order of `InputHandler`.
const KEYS: [(&str, &str); 19] = [
    ("adjust_camera_left", "key.keyboard.left"),
    ("adjust_camera_right", "key.keyboard.right"),
    ("adjust_camera_in", "key.keyboard.up"),
    ("adjust_camera_out", "key.keyboard.down"),
    ("adjust_camera_up", "key.keyboard.page.up"),
    ("adjust_camera_down", "key.keyboard.page.down"),
    ("swap_shoulder", "key.keyboard.u"),
    ("toggle_first_person", "key.keyboard.unknown"),
    ("toggle_third_person_front", "key.keyboard.unknown"),
    ("toggle_third_person_back", "key.keyboard.unknown"),
    ("free_look", "key.keyboard.left.alt"),
    ("toggle_camera_coupling", "key.keyboard.unknown"),
    ("toggle_x_offset_presets", "key.keyboard.unknown"),
    ("toggle_y_offset_presets", "key.keyboard.unknown"),
    ("toggle_z_offset_presets", "key.keyboard.unknown"),
    ("enter_first_person", "key.keyboard.unknown"),
    ("enter_third_person_front", "key.keyboard.unknown"),
    ("enter_third_person_back", "key.keyboard.unknown"),
    ("enter_shoulder_surfing", "key.keyboard.unknown"),
];

/// The keys that act while held; the rest act once per press.
const HELD: [&str; 7] = [
    "adjust_camera_left",
    "adjust_camera_right",
    "adjust_camera_in",
    "adjust_camera_out",
    "adjust_camera_up",
    "adjust_camera_down",
    "free_look",
];

const MAX_SKIPPED_BLOCKS: usize = 16;
const VANILLA_FOOD_COMPONENT: &str = "minecraft:food";
const FILLED_MAP: &str = "minecraft:filled_map";
const OFFHAND_SLOT: u32 = 40;
const SPRITE_CROSSHAIR: &str = "minecraft:hud/crosshair";
const SPRITE_OBSTRUCTION_INDICATOR: &str = "shouldersurfing:hud/obstruction_indicator";
const SPRITE_OBSTRUCTED_CROSSHAIR: &str = "shouldersurfing:hud/obstructed_crosshair";
const SPRITE_OBSTRUCTED_CROSS: &str = "shouldersurfing:hud/obstructed_crosshair_cross";

struct Runtime {
    values: Values,
    lists: Lists,
    adaptive: Adaptive,
    non_collidable: PatternList,
    unpickable: PatternList,
    synced: bool,
    surfing: bool,
    default_applied: bool,
    rot: Rotation,
    rot_init: bool,
    offsets: Offsets,
    /// The player's rotation when a mouse turn was reported: it is put back once the turn has
    /// moved the camera instead.
    snapshot: Option<[f32; 2]>,
    want_yaw: Option<f32>,
    /// Ticks the player keeps turning with the camera after an action.
    lock: i32,
    aiming: bool,
    free_look: bool,
    keys: Vec<(&'static str, ModKeyMappingRef)>,
    attack: Option<VanillaKeyMappingRef>,
    use_key: Option<VanillaKeyMappingRef>,
    pick_key: Option<VanillaKeyMappingRef>,
    entity_scale: f64,
    distance_attribute: f64,
    cam_pos: Option<V3>,
    cam_rot: Rotation,
    cam_distance: f64,
    render_offset: V3,
    /// Where the player's own ray ends, for the dynamic crosshair.
    player_point: Option<V3>,
    /// Where the player's ray towards the camera's hit ends, for the obstruction indicator.
    obstruction_point: Option<V3>,
    climbing: bool,
}

impl Runtime {
    fn new() -> Self {
        let lists = Lists::default();
        Self {
            values: Values::default(),
            adaptive: Adaptive::new(&lists),
            non_collidable: PatternList::new(&lists.non_collidable_blocks),
            unpickable: PatternList::new(&lists.unpickable_invisible_entities),
            lists,
            synced: false,
            surfing: false,
            default_applied: false,
            rot: Rotation::default(),
            rot_init: false,
            offsets: Offsets::new([0.0; 3]),
            snapshot: None,
            want_yaw: None,
            lock: 0,
            aiming: false,
            free_look: false,
            keys: Vec::new(),
            attack: None,
            use_key: None,
            pick_key: None,
            entity_scale: 1.0,
            distance_attribute: camera::VANILLA_CAMERA_DISTANCE,
            cam_pos: None,
            cam_rot: Rotation::default(),
            cam_distance: 0.0,
            render_offset: [0.0; 3],
            player_point: None,
            obstruction_point: None,
            climbing: false,
        }
    }

    fn set_lists(&mut self, lists: Lists) {
        self.adaptive = Adaptive::new(&lists);
        self.non_collidable = PatternList::new(&lists.non_collidable_blocks);
        self.unpickable = PatternList::new(&lists.unpickable_invisible_entities);
        self.lists = lists;
    }

    fn sync(&mut self, host: &dyn ClientHost) {
        if !self.synced {
            self.synced = true;
            self.values.refresh(host);
        }
    }

    fn enabled(&self) -> Enabled {
        Enabled {
            first_person: self.values.b(K::FirstPersonEnabled),
            third_person_back: self.values.b(K::ThirdPersonBackEnabled),
            third_person_front: self.values.b(K::ThirdPersonFrontEnabled),
            replace_third_person_back: self.values.b(K::ReplaceDefaultPerspective),
        }
    }

    /// Shoulder surfing is on while the game shows the back view and our flag is set.
    fn is_surfing(&self, host: &dyn ClientHost) -> bool {
        self.surfing && host.camera_type() == CameraType::ThirdPersonBack
    }

    /// The camera turns on its own, apart from the player's view.
    fn decoupled(&self) -> bool {
        self.surfing && (self.values.b(K::DecoupledCamera) || self.free_look)
    }

    fn set_perspective(&mut self, host: &dyn ClientHost, p: Perspective) {
        let was = self.surfing;
        self.surfing = p == Perspective::ShoulderSurfing;
        if self.surfing && !was {
            self.rot_init = false;
            self.offsets = Offsets::new([0.0; 3]);
        }
        host.set_camera_type(p.camera_type());
    }

    fn remember(&mut self, host: &dyn ClientHost, p: Perspective) {
        if self.values.b(K::RememberLastPerspective) {
            write_choice(host, &mut self.values, K::DefaultPerspective, p.index());
        }
    }

    fn current_perspective(&self, host: &dyn ClientHost) -> Perspective {
        Perspective::current(host.camera_type(), self.surfing)
    }

    fn pick_ranges(&self, host: &dyn ClientHost) -> (f64, f64) {
        let living = host.local_player().and_then(|p| host.living(p.entity()));
        let read = |id: &str| living.and_then(|l| l.attribute_value(host, id));
        (
            pick::range_or(
                read("minecraft:block_interaction_range"),
                pick::DEFAULT_BLOCK_RANGE,
            ),
            pick::range_or(
                read("minecraft:entity_interaction_range"),
                pick::DEFAULT_ENTITY_RANGE,
            ),
        )
    }

    /// How far the camera's own ray looks.
    fn camera_ray_length(&self, ranges: (f64, f64)) -> f64 {
        if self.values.b(K::UseCustomRaytraceDistance) {
            self.values.f(K::CustomRaytraceDistance)
        } else {
            ranges.0.max(ranges.1) + self.values.f(K::MaxOffsetZ).abs()
        }
    }

    fn entity_pickable(&self, host: &dyn ClientHost, e: EntityRef) -> bool {
        if e.is_invisible(host) != Some(true) {
            return true;
        }
        let id = e
            .kind(host)
            .and_then(|k| k.id(host))
            .map(|n| n.to_string())
            .unwrap_or_default();
        !self.unpickable.matches(&id)
    }
}

static RT: LazyLock<Mutex<Runtime>> = LazyLock::new(|| Mutex::new(Runtime::new()));

fn rt() -> MutexGuard<'static, Runtime> {
    RT.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone, Copy)]
enum Found {
    Block {
        pos: [i32; 3],
        face: BlockFace,
        location: V3,
        inside: bool,
    },
    Entity {
        entity: EntityRef,
        location: V3,
    },
}

impl Found {
    fn location(&self) -> V3 {
        match self {
            Found::Block { location, .. } | Found::Entity { location, .. } => *location,
        }
    }
}

fn face_vector(face: BlockFace) -> [i32; 3] {
    match face {
        BlockFace::Down => [0, -1, 0],
        BlockFace::Up => [0, 1, 0],
        BlockFace::North => [0, 0, -1],
        BlockFace::South => [0, 0, 1],
        BlockFace::West => [-1, 0, 0],
        BlockFace::East => [1, 0, 0],
        #[allow(unreachable_patterns)]
        _ => [0, 1, 0],
    }
}

/// The first block on the ray that the picker does not ignore (`ObjectPicker`'s
/// non-collidable block list).
fn ray_block(
    host: &dyn ClientHost,
    level: LevelRef,
    rt: &Runtime,
    from: V3,
    to: V3,
) -> Option<Found> {
    let total = math::length(math::sub(to, from));
    let unit = pick::normalized(math::sub(to, from))?;
    let mut start = from;
    for _ in 0..MAX_SKIPPED_BLOCKS {
        let hit = level.clip(host, start, to, ClipBlock::Outline, ClipFluid::None)?;
        if hit.miss {
            return None;
        }
        let id = level
            .block(host, hit.block)
            .and_then(|k| k.id(host))
            .unwrap_or_default();
        if !rt.non_collidable.matches(id) {
            return Some(Found::Block {
                pos: hit.block,
                face: hit.face,
                location: hit.location,
                inside: hit.inside,
            });
        }
        let exit = pick::cell_exit(start, unit, hit.block);
        start = math::add(start, math::scale(unit, exit + 1e-4));
        if math::length(math::sub(start, from)) >= total {
            return None;
        }
    }
    None
}

fn ray_entity(
    host: &dyn ClientHost,
    level: LevelRef,
    rt: &Runtime,
    from: V3,
    to: V3,
    exclude: EntityRef,
) -> Option<Found> {
    let hit = level
        .ray_trace_nearest_entity(host, from, to, 0.0, Some(exclude), &mut |e: EntityRef| {
            rt.entity_pickable(host, e)
        })
        .ok()
        .flatten()?;
    Some(Found::Entity {
        entity: hit.entity,
        location: hit.location,
    })
}

/// The nearest block or entity on the ray from `from` towards `towards`, `len` long.
fn ray_any(
    host: &dyn ClientHost,
    level: LevelRef,
    rt: &Runtime,
    from: V3,
    towards: V3,
    len: f64,
    exclude: EntityRef,
) -> Option<Found> {
    let to = pick::ray_end(from, towards, len)?;
    let block = ray_block(host, level, rt, from, to);
    let entity = ray_entity(host, level, rt, from, to, exclude);
    let tag = |f: Option<Found>| f.map(|f| (f, f.location()));
    pick::nearest(from, tag(block), tag(entity))
}

fn target_of(host: &dyn ClientHost, level: LevelRef, found: Found) -> Option<CrosshairTarget> {
    match found {
        Found::Block {
            pos,
            face,
            location,
            inside,
        } => Some(CrosshairTarget::Block {
            pos: BlockPos::new(pos[0], pos[1], pos[2]),
            state: level.block_state(host, pos)?,
            hit: BlockHit::new(location, face_vector(face), inside),
        }),
        Found::Entity { entity, location } => Some(CrosshairTarget::Entity {
            entity,
            point: location,
        }),
    }
}

/// The stacks in the main hand and the off hand.
fn hand_stacks(host: &dyn ClientHost) -> (Option<ItemStack>, Option<ItemStack>) {
    let Some(player) = host.local_player() else {
        return (None, None);
    };
    let slot = player.selected_slot(host).unwrap_or(0) as u32;
    player
        .with_inventory(host, |inv| {
            (
                inv.get(slot).and_then(|v| v.project()),
                inv.get(OFFHAND_SLOT).and_then(|v| v.project()),
            )
        })
        .unwrap_or((None, None))
}

fn with_facts<R>(
    host: &dyn ClientHost,
    stack: &ItemStack,
    f: impl FnOnce(&StackFacts<'_>) -> R,
) -> R {
    let item = stack.item();
    let id = item.id(host).unwrap_or_default();
    let animation = crosshair::animation_name(stack.use_animation(host));
    let components: Vec<String> = stack
        .components()
        .map(|c| c.as_nbt().keys().filter_map(|k| k.to_string().ok()).collect())
        .unwrap_or_default();
    let has_default = |c: &str| host.item_default_component(item, c).is_some();
    f(&StackFacts {
        id,
        animation,
        components: &components,
        default_has: &has_default,
    })
}

#[derive(Default)]
struct Painted {
    /// Where the crosshair is drawn, in normalised device coordinates; the centre when None.
    crosshair: Option<[f32; 2]>,
    obstructed: bool,
    indicator: Option<[f32; 2]>,
}

enum Plan {
    Vanilla,
    Nothing,
    Draw(Painted),
}

fn ndc(host: &dyn ClientHost, at: V3) -> Option<[f32; 2]> {
    let p = host.project(at)?;
    (p[0].is_finite() && p[1].is_finite()).then_some([p[0], p[1]])
}

fn gui_size(host: &dyn ClientHost) -> Option<[f32; 2]> {
    let [w, h] = host.window_size()?;
    let scale = host.gui_scale()?.max(1);
    Some([w.div_ceil(scale) as f32, h.div_ceil(scale) as f32])
}

fn plan_crosshair(host: &dyn ClientHost) -> Plan {
    let rt = rt();
    if host.hud_mode() != HudMode::AllVisible {
        return Plan::Vanilla;
    }
    let p = rt.current_perspective(host);
    let vis_key = match p {
        Perspective::FirstPerson => K::VisibilityFirstPerson,
        Perspective::ThirdPersonBack => K::VisibilityThirdPersonBack,
        Perspective::ThirdPersonFront => K::VisibilityThirdPersonFront,
        Perspective::ShoulderSurfing => K::VisibilityShoulderSurfing,
    };
    let rule = Visibility::from_index(rt.values.c(vis_key));
    let in_range = host.crosshair_target().is_some();
    let kind = CrosshairType::from_index(rt.values.c(K::CrosshairType));
    let shoulder = p == Perspective::ShoulderSurfing && rt.is_surfing(host);
    let dynamic = shoulder && crosshair::follows_target(kind, rt.aiming);
    let mut painted = Painted::default();
    if shoulder {
        let size = gui_size(host).unwrap_or([0.0, 0.0]);
        let to_px = |n: [f32; 2]| [n[0] * size[0] / 2.0, -n[1] * size[1] / 2.0];
        if dynamic {
            match rt.player_point.and_then(|at| ndc(host, at)) {
                Some(n) => painted.crosshair = Some(n),
                None => return Plan::Nothing,
            }
        } else if let Some(at) = rt.obstruction_point {
            let eye = host
                .local_player()
                .and_then(|pl| pl.entity().eye_position(host));
            let screen = ndc(host, at);
            let obstruction = Obstruction {
                enabled: rt.values.b(K::ObstructionIndicator),
                only_when_aiming: rt.values.b(K::ObstructionOnlyWhenAiming),
                min_screen_distance: f64::from(rt.values.i(K::ObstructionMinDistance)),
                max_world_distance: rt.values.f(K::ObstructionMaxDistance),
            };
            if let (Some(n), Some(eye)) = (screen, eye) {
                let px = to_px(n);
                let screen_distance = f64::from(px[0]).hypot(f64::from(px[1]));
                let world_distance = math::length(math::sub(at, eye));
                if obstruction.shows(rt.aiming, screen_distance, world_distance) {
                    painted.obstructed = rt.aiming;
                    if rt.aiming {
                        painted.crosshair = Some(n);
                    } else {
                        painted.indicator = Some(n);
                    }
                }
            }
        }
    }
    if !crosshair::visible(rule, rt.aiming, in_range) {
        return Plan::Nothing;
    }
    if p == Perspective::FirstPerson {
        return Plan::Vanilla;
    }
    Plan::Draw(painted)
}

fn sprite_at(name: &'static str, blend: Blend, x: i32, y: i32) -> GuiDraw {
    GuiDraw::Sprite {
        sprite: Cow::Borrowed(name),
        x,
        y,
        w: 15,
        h: 15,
        region: None,
        color: 0xFFFF_FFFF,
        blend,
    }
}

fn paint_crosshair(d: &mut dyn DrawContext, p: &Painted) {
    let [w, h] = d.size();
    let base = [((w as i32) - 15) / 2, ((h as i32) - 15) / 2];
    let at = |n: Option<[f32; 2]>| match n {
        Some(n) => [
            base[0] + (n[0] * w / 2.0).round() as i32,
            base[1] + (-n[1] * h / 2.0).round() as i32,
        ],
        None => base,
    };
    let main = at(p.crosshair);
    let mut ops = vec![sprite_at(
        SPRITE_CROSSHAIR,
        Blend::Crosshair,
        main[0],
        main[1],
    )];
    if p.obstructed {
        ops.push(sprite_at(
            SPRITE_OBSTRUCTED_CROSSHAIR,
            Blend::Crosshair,
            base[0],
            base[1],
        ));
        ops.push(sprite_at(
            SPRITE_OBSTRUCTED_CROSS,
            Blend::Textured,
            base[0],
            base[1],
        ));
    } else if let Some(n) = p.indicator {
        let i = at(Some(n));
        ops.push(sprite_at(
            SPRITE_OBSTRUCTION_INDICATOR,
            Blend::Crosshair,
            i[0],
            i[1],
        ));
    }
    let _ = d.gui_paint(&ops);
}

/// Puts the player's rotation back and gives the turn to the camera.
fn absorb(host: &mut dyn ClientHost, rt: &mut Runtime) {
    let Some(snap) = rt.snapshot.take() else {
        return;
    };
    let Some([yaw, pitch]) = host
        .local_player()
        .and_then(|p| p.entity().rotation(&*host))
    else {
        return;
    };
    rt.rot
        .turn(math::wrap_degrees(yaw - snap[0]), pitch - snap[1]);
    let _ = host.set_local_rotation(snap[0], snap[1]);
}

fn offset_cfg_ctx(rt: &Runtime, host: &dyn ClientHost, pitch: f32) -> (OffsetCfg, OffsetCtx) {
    let player = host.local_player();
    let entity = player.map(|p| p.entity());
    let living = entity.and_then(|e| host.living(e));
    let flag = |v: Option<bool>| v.unwrap_or(false);
    (
        OffsetCfg::from_values(&rt.values),
        OffsetCtx {
            passenger: flag(entity.and_then(|e| e.is_passenger(host))),
            sprinting: flag(entity.and_then(|e| e.is_sprinting(host))),
            aiming: rt.aiming,
            fall_flying: flag(living.and_then(|l| l.is_gliding(host))),
            climbing: rt.climbing,
            pitch,
            scale: rt.entity_scale,
            distance_attribute: rt.distance_attribute,
        },
    )
}

impl Runtime {
    /// Reads the held actions and works out `aiming` and the turn lock.
    fn update_actions(&mut self, host: &dyn ClientHost) {
        let down = |k: Option<VanillaKeyMappingRef>| {
            k.map(|k| k.is_down(host).unwrap_or(false)).unwrap_or(false)
        };
        let has_target = host.crosshair_target().is_some();
        let (main, off) = hand_stacks(host);
        let using = host.local_item_use();
        let use_stack = using.and_then(|u| match u.hand {
            Hand::Main => main.as_ref(),
            _ => off.as_ref(),
        });

        let mut aiming = false;
        if let Some(stack) = use_stack {
            aiming = with_facts(host, stack, |f| self.adaptive.using(f));
        }
        if !aiming {
            aiming = [main.as_ref(), off.as_ref()]
                .into_iter()
                .flatten()
                .any(|s| with_facts(host, s, |f| self.adaptive.holding(f)));
        }
        self.aiming = aiming;

        let fall_flying = host
            .local_player()
            .and_then(|p| host.living(p.entity())?.is_gliding(host))
            .unwrap_or(false);
        let mode = |k| TurningMode::from_index(self.values.c(k)).should_turn(has_target);
        let attacking = down(self.attack) && mode(K::TurnWhenAttacking);
        let using_item = use_stack.is_some_and(|s| {
            let scoping = crosshair::animation_name(s.use_animation(host)) == "spyglass";
            let food = host
                .item_default_component(s.item(), VANILLA_FOOD_COMPONENT)
                .is_some();
            (!food || scoping) && mode(K::TurnWhenUsingItem)
        });
        let interacting =
            down(self.use_key) && use_stack.is_none() && has_target && mode(K::TurnWhenInteracting);
        let picking = down(self.pick_key) && mode(K::TurnWhenPicking);
        if (attacking || using_item || interacting || picking) && !fall_flying {
            self.lock = self.values.i(K::TurningLockTime).max(1);
        } else if self.lock > 0 {
            self.lock -= 1;
        }
    }
}

#[derive(Default)]
struct ShoulderSurfing;

#[mod_api::sunrise_mod]
impl Hooks for ShoulderSurfing {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        register_settings(r);
        for (id, key) in KEYS {
            r.register_key_mapping(
                KeyRegistration::new(id, key)
                    .label(&format!("key.shouldersurfing.{id}"))
                    .category("key.category.shouldersurfing.general")
                    .context(KeyConflictContext::InGame),
            );
        }
    }

    fn register_configs(&self, _ctx: &dyn ConfigContext, event: &mut RegisterConfigsEvent<'_>) {
        register_mod_configs(event.configs);
    }

    fn config_loading(&self, _ctx: &dyn ConfigContext, event: &mut ConfigLoadingEvent<'_>) {
        if event.config.mod_id == MOD_ID && event.config.kind == ConfigKind::Client {
            rt().set_lists(Lists::parse(&event.config.text));
        }
    }

    fn config_reloading(&self, _host: HostSide<'_>, event: &mut ConfigReloadingEvent<'_>) {
        if event.config.mod_id == MOD_ID && event.config.kind == ConfigKind::Client {
            rt().set_lists(Lists::parse(&event.config.text));
        }
    }

    fn setting_changed(&self, host: &mut dyn ClientHost, event: &mut SettingChangedEvent<'_>) {
        if event.mod_id == MOD_ID {
            let mut rt = rt();
            rt.synced = true;
            rt.values.refresh(&*host);
        }
    }

    fn register_gui_layers(
        &self,
        _ctx: &RegistrationContext,
        event: &mut RegisterGuiLayersEvent<'_>,
    ) {
        event.layers.wrap_layer(
            "minecraft:crosshair",
            Box::new(|ctx| {
                let plan = plan_crosshair(ctx.host());
                match plan {
                    Plan::Vanilla => ctx.draw_previous(),
                    Plan::Nothing => {}
                    Plan::Draw(p) => {
                        ctx.with_draw(&mut |d: &mut dyn DrawContext| paint_crosshair(d, &p))
                    }
                }
            }),
        );
    }

    fn camera_type_cycle(&self, host: &mut dyn ClientHost, event: &mut CameraTypeCycleEvent<'_>) {
        if !host.on(MOD_ID) {
            return;
        }
        let mut rt = rt();
        rt.sync(&*host);
        let next = Perspective::current(event.previous, rt.surfing).next(&rt.enabled());
        let was = rt.surfing;
        rt.surfing = next == Perspective::ShoulderSurfing;
        if rt.surfing && !was {
            rt.rot_init = false;
            rt.offsets = Offsets::new([0.0; 3]);
        }
        *event.next = next.camera_type();
        rt.remember(&*host, next);
    }

    fn camera_distance(&self, host: &dyn ClientHost, event: &mut CameraDistanceEvent<'_>) {
        if !host.on(MOD_ID) {
            return;
        }
        let mut rt = rt();
        if let Some(living) = event.entity.living {
            rt.entity_scale = f64::from(living.scale);
            rt.distance_attribute = living.camera_distance;
        }
        if rt.is_surfing(host) && !event.mirrored {
            event.distance = 0.0;
        }
    }

    fn compute_fov(&self, host: &dyn ClientHost, event: &mut ComputeFovEvent) {
        if !host.on(MOD_ID) {
            return;
        }
        let rt = rt();
        if !rt.is_surfing(host) || !rt.values.b(K::FovOverrideEnabled) {
            return;
        }
        let option = host.graphics_options().fov;
        if option.is_finite() && option > 0.0 {
            let scaled = rt.values.f(K::FovOverride) as f32 / option * event.degrees;
            if scaled.is_finite() {
                event.degrees = scaled.clamp(1.0, 179.0);
            }
        }
    }

    fn calculate_player_turn(&self, host: &dyn ClientHost, _event: &mut CalculatePlayerTurnEvent) {
        if !host.on(MOD_ID) {
            return;
        }
        let mut rt = rt();
        if rt.snapshot.is_none() && rt.is_surfing(host) && rt.decoupled() {
            rt.snapshot = host.local_player().and_then(|p| p.entity().rotation(host));
        }
    }

    fn camera_pose(&self, host: &dyn ClientHost, event: &mut CameraPoseEvent<'_>) {
        if !host.on(MOD_ID) {
            return;
        }
        let mut rt = rt();
        if !rt.is_surfing(host) {
            rt.cam_pos = None;
            return;
        }
        if !rt.rot_init || !rt.decoupled() {
            rt.rot = Rotation {
                yaw: event.yaw,
                pitch: event.pitch,
            };
            rt.rot_init = true;
        } else {
            event.yaw = rt.rot.yaw;
            event.pitch = rt.rot.pitch;
        }
        let eye = *event.position;
        let offset = rt.offsets.render(f64::from(event.partial));
        let zoomed = |off: V3| math::camera_displacement(event.yaw, event.pitch, off);
        let full = zoomed(offset);
        let length = math::length(full);
        let mut fraction = 1.0;
        if length > 1e-6
            && let Some(level) = host.level(host.dimension())
        {
            let hits: Vec<Option<f64>> = camera::CORNERS
                .iter()
                .map(|c| {
                    let from = math::add(eye, *c);
                    let to = math::add(from, full);
                    level
                        .clip(host, from, to, ClipBlock::Collider, ClipFluid::None)
                        .filter(|h| !h.miss)
                        .map(|h| math::length(math::sub(h.location, from)) / length)
                })
                .collect();
            fraction = camera::zoom_fraction(&hits);
        }
        let displacement = if rt.values.b(K::DynamicOffsets) {
            math::scale(full, fraction)
        } else {
            // Only the distance behind the player shrinks; the side and height stay.
            let sideways = zoomed([offset[0], offset[1], 0.0]);
            let behind = math::sub(full, sideways);
            math::add(sideways, math::scale(behind, fraction))
        };
        *event.position = math::add(eye, displacement);
        rt.cam_pos = Some(*event.position);
        rt.cam_rot = Rotation {
            yaw: event.yaw,
            pitch: event.pitch,
        };
        rt.cam_distance = math::length(displacement);
        rt.render_offset = offset;
    }

    fn movement_input_update(
        &self,
        host: &dyn ClientHost,
        event: &mut MovementInputUpdateEvent<'_>,
    ) {
        if !host.on(MOD_ID) {
            return;
        }
        let mut rt = rt();
        if !rt.is_surfing(host) || !rt.decoupled() {
            return;
        }
        let mv = event.input.move_vector;
        if mv == [0.0, 0.0] {
            return;
        }
        let Some([yaw, _]) = host.local_player().and_then(|p| p.entity().rotation(host)) else {
            return;
        };
        let camera_yaw = rt.rot.yaw;
        event.input.move_vector = math::rotate_input(mv, math::wrap_degrees(camera_yaw - yaw));
        // Face the way the player walks, as seen from the camera.
        let towards = f64::from(mv[0]).atan2(f64::from(mv[1])).to_degrees() as f32;
        rt.want_yaw = Some(math::wrap_degrees(camera_yaw - towards));
    }

    fn client_tick_pre(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPreEvent) {
        if !host.on(MOD_ID) {
            return;
        }
        let mut rt = rt();
        rt.sync(&*host);
        absorb(host, &mut rt);
        if rt.keys.is_empty() {
            let mut keys = Vec::new();
            for (id, _) in KEYS {
                if let Ok(key) = host.mod_key_mapping(MOD_ID, id) {
                    keys.push((id, key));
                }
            }
            rt.keys = keys;
        }
        if rt.attack.is_none() {
            rt.attack = host.vanilla_key_mapping("key.attack").ok();
            rt.use_key = host.vanilla_key_mapping("key.use").ok();
            rt.pick_key = host.vanilla_key_mapping("key.pickItem").ok();
        }
        let Some(player) = host.local_player() else {
            rt.default_applied = false;
            rt.surfing = false;
            rt.snapshot = None;
            rt.lock = 0;
            return;
        };
        if !rt.default_applied {
            rt.default_applied = true;
            let wanted = Perspective::from_index(rt.values.c(K::DefaultPerspective))
                .or_next_enabled(&rt.enabled());
            if Perspective::current(host.camera_type(), rt.surfing) != wanted {
                rt.set_perspective(&*host, wanted);
            }
        }
        if rt.surfing && host.camera_type() != CameraType::ThirdPersonBack {
            rt.surfing = false;
        }

        // Presses.
        let mut clicked: Vec<&'static str> = Vec::new();
        let keys = rt.keys.clone();
        for (id, key) in &keys {
            if HELD.contains(id) {
                continue;
            }
            while key.consume_click(host).unwrap_or(false) {
                clicked.push(id);
            }
        }
        let screen_open = host.observed_screen_id().is_some();
        let held = |id: &str| {
            !screen_open
                && keys
                    .iter()
                    .any(|(k, key)| *k == id && key.is_down(&*host).unwrap_or(false))
        };
        rt.free_look = rt.surfing && held("free_look");
        if rt.is_surfing(&*host) {
            let step = rt.values.f(K::StepSize);
            let axes = [
                (
                    "adjust_camera_left",
                    0,
                    1.0,
                    K::OffsetX,
                    K::MinOffsetX,
                    K::MaxOffsetX,
                    K::UnlimitedX,
                ),
                (
                    "adjust_camera_right",
                    0,
                    -1.0,
                    K::OffsetX,
                    K::MinOffsetX,
                    K::MaxOffsetX,
                    K::UnlimitedX,
                ),
                (
                    "adjust_camera_up",
                    1,
                    1.0,
                    K::OffsetY,
                    K::MinOffsetY,
                    K::MaxOffsetY,
                    K::UnlimitedY,
                ),
                (
                    "adjust_camera_down",
                    1,
                    -1.0,
                    K::OffsetY,
                    K::MinOffsetY,
                    K::MaxOffsetY,
                    K::UnlimitedY,
                ),
                (
                    "adjust_camera_out",
                    2,
                    1.0,
                    K::OffsetZ,
                    K::MinOffsetZ,
                    K::MaxOffsetZ,
                    K::UnlimitedZ,
                ),
                (
                    "adjust_camera_in",
                    2,
                    -1.0,
                    K::OffsetZ,
                    K::MinOffsetZ,
                    K::MaxOffsetZ,
                    K::UnlimitedZ,
                ),
            ];
            for (id, _, dir, value, min, max, unlimited) in axes {
                if held(id) {
                    let next = camera::adjust_axis(
                        rt.values.f(value),
                        dir,
                        step,
                        rt.values.f(min),
                        rt.values.f(max),
                        rt.values.b(unlimited),
                    );
                    let values = &mut rt.values;
                    write_f64(&*host, values, value, next);
                }
            }
        }
        for id in clicked {
            let current = rt.current_perspective(&*host);
            let toggle = |target: Perspective, rt: &Runtime| {
                if current == target {
                    let back = Perspective::from_index(rt.values.c(K::DefaultPerspective));
                    if back == target {
                        Perspective::ShoulderSurfing
                    } else {
                        back
                    }
                } else {
                    target
                }
            };
            let go = match id {
                "toggle_first_person" => Some(toggle(Perspective::FirstPerson, &rt)),
                "toggle_third_person_front" => Some(toggle(Perspective::ThirdPersonFront, &rt)),
                "toggle_third_person_back" => Some(toggle(Perspective::ThirdPersonBack, &rt)),
                "enter_first_person" => Some(Perspective::FirstPerson),
                "enter_third_person_front" => Some(Perspective::ThirdPersonFront),
                "enter_third_person_back" => Some(Perspective::ThirdPersonBack),
                "enter_shoulder_surfing" => Some(Perspective::ShoulderSurfing),
                _ => None,
            };
            if let Some(p) = go {
                let enabled = rt.enabled();
                rt.set_perspective(&*host, p.or_next_enabled(&enabled));
                rt.remember(&*host, p);
                continue;
            }
            if !rt.is_surfing(&*host) {
                continue;
            }
            match id {
                "swap_shoulder" => {
                    let x = -rt.values.f(K::OffsetX);
                    write_f64(&*host, &mut rt.values, K::OffsetX, x);
                }
                "toggle_camera_coupling" => {
                    let on = !rt.values.b(K::DecoupledCamera);
                    write_bool(&*host, &mut rt.values, K::DecoupledCamera, on);
                }
                "toggle_x_offset_presets"
                | "toggle_y_offset_presets"
                | "toggle_z_offset_presets" => {
                    let (k, presets) = match id {
                        "toggle_x_offset_presets" => {
                            (K::OffsetX, rt.lists.offset_x_presets.clone())
                        }
                        "toggle_y_offset_presets" => {
                            (K::OffsetY, rt.lists.offset_y_presets.clone())
                        }
                        _ => (K::OffsetZ, rt.lists.offset_z_presets.clone()),
                    };
                    if let Some(next) = camera::next_preset(&presets, rt.values.f(k)) {
                        write_f64(&*host, &mut rt.values, k, next);
                    }
                }
                _ => {}
            }
        }

        rt.update_actions(&*host);
        rt.climbing = host
            .living(player.entity())
            .and_then(|l| l.is_climbing(&*host))
            .unwrap_or(false);

        if !rt.is_surfing(&*host) {
            return;
        }
        // Turn the player: with the camera while acting, otherwise to the way it walks.
        let Some([yaw, pitch]) = player.entity().rotation(&*host) else {
            return;
        };
        let speed = rt.values.f(K::TurningSpeed) as f32;
        let mut target = None;
        if rt.lock > 0 && rt.decoupled() {
            let cam_pitch = rt.rot.pitch;
            let follow_pitch =
                rt.values.b(K::PlayerXRotWithCamera) || rt.values.b(K::PlayerXRotToCamera);
            let to_yaw = if rt.values.b(K::PlayerYRotWithCamera) {
                rt.rot.yaw
            } else {
                yaw
            };
            target = Some((to_yaw, if follow_pitch { cam_pitch } else { pitch }));
        } else if let Some(want) = rt.want_yaw {
            target = Some((want, pitch));
        }
        rt.want_yaw = None;
        if let Some((to_yaw, to_pitch)) = target {
            let new_yaw =
                math::approach_degrees(yaw, to_yaw, math::wrap_degrees(to_yaw - yaw).abs() * speed);
            let new_pitch = pitch + (to_pitch - pitch) * speed.clamp(0.0, 1.0);
            let _ = host.set_local_rotation(new_yaw, new_pitch);
        }

        // The camera's own offset moves towards where it should be.
        let (cfg, ctx) = offset_cfg_ctx(&rt, &*host, rt.rot.pitch);
        let target = camera::target_offset(&cfg, &ctx);
        let speed = rt.values.f(K::TransitionSpeed);
        rt.offsets.tick(target, speed);
    }

    fn render_frame_pre(&self, host: &mut dyn ClientHost, _event: &mut RenderFramePreEvent) {
        if !host.on(MOD_ID) {
            return;
        }
        let mut rt = rt();
        absorb(host, &mut rt);
    }

    fn client_pick(&self, host: &mut dyn ClientHost, event: &mut ClientPickEvent<'_>) {
        if !host.on(MOD_ID) {
            return;
        }
        let host = &*host;
        let mut rt = rt();
        let (Some(cam_pos), true) = (rt.cam_pos, rt.is_surfing(host)) else {
            rt.player_point = None;
            rt.obstruction_point = None;
            return;
        };
        let Some(player) = host.local_player() else {
            return;
        };
        let Some(level) = host.level(host.dimension()) else {
            return;
        };
        let me = player.entity();
        let origin = event.origin;
        let ranges = rt.pick_ranges(host);
        let long = rt.camera_ray_length(ranges);
        let player_look = me
            .rotation(host)
            .map(|[y, p]| math::add(origin, math::forward(y, p)));
        let cam_dir = math::forward(rt.cam_rot.yaw, rt.cam_rot.pitch);
        let cam_towards = math::add(cam_pos, cam_dir);
        let end_of = |found: Option<Found>, from: V3, towards: V3| {
            found
                .map(|f| f.location())
                .or_else(|| pick::ray_end(from, towards, long))
        };

        // What the camera looks at, and what the player's own ray reaches.
        let cam_hit = ray_any(host, level, &rt, cam_pos, cam_towards, long, me);
        let cam_point = end_of(cam_hit, cam_pos, cam_towards);
        rt.player_point = player_look.and_then(|look| {
            end_of(
                ray_any(host, level, &rt, origin, look, long, me),
                origin,
                look,
            )
        });
        rt.obstruction_point = cam_point
            .and_then(|p| end_of(ray_any(host, level, &rt, origin, p, long, me), origin, p));

        let towards = if Origin::from_index(rt.values.c(K::PickVector)) == Origin::Camera {
            cam_point
        } else {
            player_look
        };
        let Some(towards) = towards else {
            return;
        };
        let origin_of = |k: K| {
            if Origin::from_index(rt.values.c(k)) == Origin::Camera {
                cam_pos
            } else {
                origin
            }
        };
        let block_from = origin_of(K::BlockPickOrigin);
        let entity_from = origin_of(K::EntityPickOrigin);
        let block = pick::ray_end(block_from, towards, long)
            .and_then(|to| ray_block(host, level, &rt, block_from, to))
            .filter(|f| pick::within(origin, f.location(), ranges.0));
        let entity = pick::ray_end(entity_from, towards, long)
            .and_then(|to| ray_entity(host, level, &rt, entity_from, to, me))
            .filter(|f| pick::within(origin, f.location(), ranges.1));
        let tag = |f: Option<Found>| f.map(|f| (f, f.location()));
        let found = pick::nearest(origin, tag(block), tag(entity));
        *event.result = found.and_then(|f| target_of(host, level, f));
    }

    fn render_player_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut RenderPlayerPreEvent<'_>,
    ) -> Flow {
        if !event.who.local || !host.on(MOD_ID) {
            return Flow::Continue;
        }
        let rt = rt();
        if !rt.is_surfing(host) || rt.cam_pos.is_none() {
            return Flow::Continue;
        }
        let Some(player) = host.local_player() else {
            return Flow::Continue;
        };
        let entity = player.entity();
        let [width, height] = entity.dimensions(host).unwrap_or([0.6, 1.8]);
        let mut hidden = camera::inside_head(rt.cam_distance, width, rt.values.f(K::KeepOutOfHead))
            || camera::looking_up(rt.cam_rot.pitch, rt.values.f(K::HideLookingUpAngle));
        let spectator = player.is_spectator(host).unwrap_or(false);
        if !hidden && !spectator && rt.values.b(K::AdjustTransparency) {
            let eye_height = entity.eye_height(host).unwrap_or(1.62);
            let configured = rt.values.v3(K::OffsetX, K::OffsetY, K::OffsetZ);
            let alpha =
                camera::player_alpha(rt.render_offset, configured, width, height, eye_height);
            hidden = camera::player_hidden(alpha)
                && !(rt.climbing && !rt.values.b(K::TransparentWhenClimbing));
            if rt.aiming && rt.values.b(K::TransparentWhenAiming) {
                hidden = true;
            }
        }
        if hidden {
            event.cancelled = true;
        }
        Flow::Continue
    }

    fn render_gui_post(&self, host: &dyn ClientHost, event: &mut RenderGuiPostEvent<'_>) {
        if !host.on(MOD_ID) || host.hud_mode() != HudMode::AllVisible {
            return;
        }
        let rt = rt();
        if !rt.is_surfing(host) || !rt.values.b(K::MapOverlayEnabled) {
            return;
        }
        let Some(player) = host.local_player() else {
            return;
        };
        let (main, off) = hand_stacks(host);
        let is_map = |s: &Option<ItemStack>| {
            s.as_ref()
                .filter(|s| s.item().id(host) == Some(FILLED_MAP))
                .cloned()
        };
        let (main, off) = (is_map(&main), is_map(&off));
        let left_is_off = player.entity().main_arm(host) != Some(mod_api::MainHand::Left);
        let (left, right) = if left_is_off {
            (off, main)
        } else {
            (main, off)
        };
        let [w, h] = event.draw.size();
        let layout = MapLayout {
            scale_percent: rt.values.i(K::MapOverlayScale),
            anchor_top: rt.values.c(K::MapOverlayAnchor) == 0,
            inset_top: rt.values.i(K::MapInsetTop),
            inset_right: rt.values.i(K::MapInsetRight),
            inset_bottom: rt.values.i(K::MapInsetBottom),
            inset_left: rt.values.i(K::MapInsetLeft),
        };
        let (l, r) = crosshair::map_rects(
            &layout,
            [w as i32, h as i32],
            left.is_some(),
            right.is_some(),
            0,
        );
        for (stack, rect) in [(left, l), (right, r)] {
            if let (Some(stack), Some(rect)) = (stack, rect) {
                let _ =
                    event
                        .draw
                        .item_stack(&stack, [rect.x as f32, rect.y as f32], rect.size as f32);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_table_matches_the_original() {
        assert_eq!(KEYS.len(), 19);
        let mut ids: Vec<&str> = KEYS.iter().map(|k| k.0).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 19);
        for held in HELD {
            assert!(KEYS.iter().any(|k| k.0 == held), "{held}");
        }
        assert!(KEYS.iter().all(|k| k.1.starts_with("key.keyboard.")));
    }

    #[test]
    fn faces_point_the_right_way() {
        assert_eq!(face_vector(BlockFace::Up), [0, 1, 0]);
        assert_eq!(face_vector(BlockFace::North), [0, 0, -1]);
        assert_eq!(face_vector(BlockFace::East), [1, 0, 0]);
    }

    #[test]
    fn runtime_defaults_are_safe() {
        let rt = Runtime::new();
        assert!(!rt.surfing);
        assert!(rt.entity_scale > 0.0);
        assert!(crate::config::CONFIG_FILE_NAME.starts_with("shouldersurfing"));
    }
}
