//! Falling Trees for Sunrise: chopping a tree's stem takes the whole tree down, as Falling Trees
//! 0.13.2 (PandaMods) does.
//!
//! The finders and the fall's arithmetic are in [`trees`] (generic trees, cactus and bamboo,
//! chorus plants, giant mushrooms). This file is the glue: the options as mod settings, the
//! chop (`break_block`), the felling at the end of the server tick, the falling tree itself (a
//! general entity that carries the tree's blocks in a synced attachment and drops the loot at
//! the stump when its lifetime ends), its renderer and its sounds.
//!
//! Sneaking is the player's `is_shift_key_down`; the held tool is the player's main hand.

pub mod trees;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use mod_api::{
    self, AttachmentCodec, AttachmentType, BlockEntityDrawContext, BlockUpdateFlags,
    BreakBlockEvent, ClientHost, ClientTickPostEvent, Compound, EntityPart, EntityRef,
    EntityRenderState, EntityRenderer, EntitySpawn, EquipmentSlot, Flow, GameMode,
    GeneralEntityCategory, GeneralEntityTickEvent, GeneralSavePolicy, Hand, Hooks, Host, HostSide,
    ItemStack, LevelRef, LivingRef, PlayerBreakSpeedEvent, PlayerRef, RegisterEntityRenderersEvent,
    Registrar, RegistrationContext, ServerHost, ServerStoppedEvent, ServerTickEvent,
    ServerTickPhase, SettingSpec, SoundSource, SpawnBody, Tag,
};

use trees::{Algorithm, Block, Kind, Tree};

const MOD_ID: &str = "fallingtrees";
const TREE: &str = "fallingtrees:tree";

// ---------------------------------------------------------------------------- options

/// The option keys, as `CommonConfig`, `ClientConfig` and `TreeConfigs` name them.
mod key {
    pub const INVERT_CROUCH: &str = "invert_crouch_mining";
    pub const DISABLE_CROUCH: &str = "disable_crouch_mining";
    pub const NO_EXTRA_DAMAGE: &str = "disable_extra_tool_damage";
    pub const NO_EXTRA_EXHAUSTION: &str = "disable_extra_food_exhaustion";
    pub const LIFETIME: &str = "tree_lifetime_length";
    pub const NO_DYNAMIC_SPEED: &str = "disable_dynamic_mining_speed";
    pub const SPEED_MULTIPLICATION: &str = "mining_speed_multiplication";
    pub const MAX_SPEED_MULTIPLICATION: &str = "max_mining_speed_multiplication";
    pub const MAX_LEAVES_RADIUS: &str = "max_leaves_radius";
    pub const MAX_LOGS: &str = "max_log_amount";
    pub const IGNORE_PERSISTENT: &str = "ignore_persistent_leaves";
    pub const FALL_LENGTH: &str = "fall_animation_length";
    pub const BOUNCE_HEIGHT: &str = "bounce_angle_height";
    pub const BOUNCE_LENGTH: &str = "bounce_animation_length";
    pub const SOUND_ENABLED: &str = "sounds_enabled";
    pub const SOUND_START: &str = "sound_start_volume";
    pub const SOUND_END: &str = "sound_end_volume";
    /// `enabled`, per kind.
    pub const ENABLED: [&str; 4] = [
        "generic_tree_enabled",
        "vertical_tree_enabled",
        "chorus_tree_enabled",
        "mushroom_tree_enabled",
    ];
    /// `requireTool`, per kind.
    pub const REQUIRE_TOOL: [&str; 4] = [
        "generic_tree_require_tool",
        "vertical_tree_require_tool",
        "chorus_tree_require_tool",
        "mushroom_tree_require_tool",
    ];
}

fn kind_index(kind: Kind) -> usize {
    match kind {
        Kind::Generic => 0,
        Kind::Vertical => 1,
        Kind::Chorus => 2,
        Kind::Mushroom => 3,
    }
}

fn flag(host: &dyn Host, key: &str, default: bool) -> bool {
    host.setting(MOD_ID, key)
        .map_or(default, |text| text.trim() == "true")
}

fn number(host: &dyn Host, key: &str, default: f32) -> f32 {
    host.setting(MOD_ID, key)
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(default)
}

/// The options a chop reads, as they stand when it starts.
struct Options {
    algorithm: Algorithm,
    no_extra_damage: bool,
    no_extra_exhaustion: bool,
    speed_multiplication: f32,
    max_speed_multiplication: f32,
}

impl Options {
    fn read(host: &dyn Host) -> Self {
        let defaults = Algorithm::default();
        Self {
            algorithm: Algorithm {
                max_leaves_radius: number(
                    host,
                    key::MAX_LEAVES_RADIUS,
                    defaults.max_leaves_radius as f32,
                ) as u32,
                max_log_amount: number(host, key::MAX_LOGS, defaults.max_log_amount as f32)
                    as usize,
                ignore_persistent_leaves: flag(
                    host,
                    key::IGNORE_PERSISTENT,
                    defaults.ignore_persistent_leaves,
                ),
            },
            no_extra_damage: flag(host, key::NO_EXTRA_DAMAGE, false),
            no_extra_exhaustion: flag(host, key::NO_EXTRA_EXHAUSTION, false),
            speed_multiplication: number(host, key::SPEED_MULTIPLICATION, 0.5),
            max_speed_multiplication: number(host, key::MAX_SPEED_MULTIPLICATION, 16.0),
        }
    }
}

/// `#minecraft:axes` by name.
pub fn is_axe(item: &str) -> bool {
    item.ends_with("_axe") && !item.ends_with("_pickaxe")
}

// ---------------------------------------------------------------------------- the tree's data

/// One block of a falling tree: its offset from the chopped block, whether it is a stem, and its
/// state as text (`minecraft:oak_log[axis=y]`).
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub offset: [i32; 3],
    pub stem: bool,
    pub state: String,
}

/// What the falling tree entity carries to clients: the blocks and the way it falls.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Falling {
    pub pieces: Vec<Piece>,
    /// The horizontal offset it falls toward.
    pub direction: [i32; 2],
}

impl Falling {
    fn to_nbt(&self) -> Compound {
        let mut out = Compound::new();
        out.insert("dx".into(), Tag::Int(self.direction[0]));
        out.insert("dz".into(), Tag::Int(self.direction[1]));
        let list = self
            .pieces
            .iter()
            .map(|p| {
                let text = format!(
                    "{},{},{},{},{}",
                    p.offset[0],
                    p.offset[1],
                    p.offset[2],
                    u8::from(p.stem),
                    p.state
                );
                Tag::String(text.as_str().into())
            })
            .collect();
        out.insert("blocks".into(), Tag::List(list));
        out
    }

    fn from_nbt(saved: &Compound) -> Option<Self> {
        let (Some(Tag::Int(dx)), Some(Tag::Int(dz)), Some(Tag::List(list))) = (
            saved.get(&"dx".into()),
            saved.get(&"dz".into()),
            saved.get(&"blocks".into()),
        ) else {
            return None;
        };
        let mut pieces = Vec::with_capacity(list.len());
        for tag in list {
            let Tag::String(text) = tag else { return None };
            let text = text.to_string().ok()?;
            let mut parts = text.splitn(5, ',');
            let offset = [
                parts.next()?.parse().ok()?,
                parts.next()?.parse().ok()?,
                parts.next()?.parse().ok()?,
            ];
            let stem = parts.next()? == "1";
            pieces.push(Piece {
                offset,
                stem,
                state: parts.next()?.to_owned(),
            });
        }
        Some(Self {
            pieces,
            direction: [*dx, *dz],
        })
    }
}

static FALLING: AttachmentType<Falling> = AttachmentType {
    namespace: MOD_ID,
    name: "tree",
    default: Falling::default,
    codec: Some(AttachmentCodec {
        encode: |value| Ok(value.to_nbt()),
        decode: |saved| Falling::from_nbt(saved).ok_or(mod_api::AttachmentError::Decode),
    }),
    copy_on_death: false,
    sync: true,
};

/// A state's id and properties from its text form: `a:b[x=1,y=2]`.
pub fn split_state(state: &str) -> (&str, Vec<(&str, &str)>) {
    let Some((id, rest)) = state.split_once('[') else {
        return (state, Vec::new());
    };
    let props = rest
        .trim_end_matches(']')
        .split(',')
        .filter_map(|pair| pair.split_once('='))
        .collect();
    (id, props)
}

// ---------------------------------------------------------------------------- felling

/// One felled tree, waiting for the end of the server tick.
struct Felling {
    level: LevelRef,
    player: PlayerRef,
    origin: [i32; 3],
    kind: Kind,
    tree: Tree,
    /// The tool, as it was when the stem was chopped: what every block's loot rolls with.
    tool: Option<ItemStack>,
    survival: bool,
}

/// What a living tree still owes: its loot, and how long it has fallen.
struct Pending {
    drops: Vec<ItemStack>,
    ticks: i32,
}

type PendingMap = HashMap<u32, Pending>;

fn pending() -> std::sync::MutexGuard<'static, PendingMap> {
    static PENDING: std::sync::OnceLock<Mutex<PendingMap>> = std::sync::OnceLock::new();
    PENDING
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

impl Felling {
    /// `TreeHandler.destroyTree`: the loot and the tool, the wear and the tiredness, the blocks
    /// gone, and the falling tree in their place.
    fn run(&self, host: &mut dyn ServerHost) {
        let blocks: Vec<[i32; 3]> = self.tree.blocks().collect();
        let mut drops = Vec::new();
        let mut pieces = Vec::with_capacity(blocks.len());
        for &pos in &blocks {
            let Some(text) = self
                .level
                .block_state(&*host, pos)
                .and_then(|state| host.block_state_string(state))
            else {
                continue;
            };
            if let Ok(Some(found)) =
                self.level
                    .block_drops(host, pos, self.tool.as_ref(), Some(self.player.entity()))
            {
                drops.extend(found);
            }
            pieces.push(Piece {
                offset: [
                    pos[0] - self.origin[0],
                    pos[1] - self.origin[1],
                    pos[2] - self.origin[2],
                ],
                stem: self.tree.logs.contains(&pos),
                state: text,
            });
        }
        if pieces.is_empty() {
            return;
        }
        let options = Options::read(&*host);
        if self.survival {
            let stems = if self.kind == Kind::Generic {
                self.tree.logs.len()
            } else {
                self.tree.len()
            };
            let wear = if options.no_extra_damage {
                1
            } else {
                stems as i32
            };
            let _ = LivingRef::new(self.player.entity()).hurt_and_break(
                host,
                EquipmentSlot::MainHand,
                wear,
            );
            let tired = if options.no_extra_exhaustion {
                1
            } else {
                stems
            };
            self.player.add_exhaustion(host, 0.005 * tired as f32);
        }
        let player_at = self.player.entity().position(&*host).unwrap_or([0.0; 3]);
        let direction = trees::fall_direction(
            [player_at[0], player_at[2]],
            [self.origin[0], self.origin[2]],
        );
        let Some(air) = host.block_state_id("minecraft:air") else {
            return;
        };
        for &pos in &blocks {
            let _ = self.level.set_block(
                host,
                pos,
                air,
                BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS,
            );
        }
        let at = [
            f64::from(self.origin[0]) + 0.5,
            f64::from(self.origin[1]),
            f64::from(self.origin[2]) + 0.5,
        ];
        let spawn = EntitySpawn::new(SpawnBody::General);
        let Some(tree) = self.level.spawn_entity(host, TREE, at, &spawn) else {
            // The entity could not join: the loot still lands at the stump.
            for stack in drops {
                let _ = self.level.pop_resource(host, self.origin, stack);
            }
            return;
        };
        let _ = FALLING.set(
            host.attachments(),
            mod_api::Holder::Entity(tree.id()),
            Falling { pieces, direction },
        );
        pending().insert(tree.id(), Pending { drops, ticks: 0 });
    }
}

/// The falling tree: its lifetime, its sounds and its loot.
pub struct TreePart;

#[mod_api::sunrise_part]
impl EntityPart for TreePart {
    /// `TreeEntity.tick`: count up, sound the fall and the impact, and at the end of the
    /// lifetime drop the loot at the stump and go.
    fn general_entity_tick(&self, event: &mut GeneralEntityTickEvent<'_>) {
        let Some((entity, at)) = event
            .subject
            .with_view(|view| (view.handle(), view.position()))
        else {
            return;
        };
        let host = event.subject.host();
        let lifetime = (number(&*host, key::LIFETIME, 4.0) * 20.0) as i32;
        let fall = (number(&*host, key::FALL_LENGTH, 2.5) * 20.0) as i32;
        let (ticks, drops) = {
            let mut all = pending();
            let Some(this) = all.get_mut(&entity.id()) else {
                drop(all);
                entity.discard(host);
                return;
            };
            this.ticks += 1;
            let drops = (this.ticks >= lifetime).then(|| std::mem::take(&mut this.drops));
            (this.ticks, drops)
        };
        let Some(level) = entity.level(&*host) else {
            return;
        };
        if flag(&*host, key::SOUND_ENABLED, true) {
            let (start, end) = (
                number(&*host, key::SOUND_START, 0.7),
                number(&*host, key::SOUND_END, 0.7),
            );
            if ticks == 1 {
                level.play_sound(
                    host,
                    "fallingtrees:tree_fall",
                    at,
                    SoundSource::Blocks,
                    start,
                    1.0,
                    None,
                );
            }
            if ticks == fall - 5 {
                level.play_sound(
                    host,
                    "fallingtrees:tree_impact",
                    at,
                    SoundSource::Blocks,
                    end,
                    1.0,
                    None,
                );
            }
        }
        if let Some(drops) = drops {
            let mut seed = u64::from(entity.id()) ^ (at[0].to_bits());
            let mut next = move || {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                (seed >> 11) as f64 / (1u64 << 53) as f64
            };
            for stack in drops {
                let motion = [next() * 0.2 - 0.1, 0.25, next() * 0.2 - 0.1];
                let _ = level.spawn_item_with_motion(
                    host,
                    at[0],
                    at[1] + 0.125,
                    at[2],
                    stack,
                    motion,
                    10,
                );
            }
            pending().remove(&entity.id());
            entity.discard(host);
        }
    }
}

// ---------------------------------------------------------------------------- the renderer

/// The client's animation options, read once a tick (the renderer has no host).
static FALL_LENGTH: AtomicU32 = AtomicU32::new(0x4020_0000); // 2.5
static BOUNCE_HEIGHT: AtomicU32 = AtomicU32::new(0x4120_0000); // 10.0
static BOUNCE_LENGTH: AtomicU32 = AtomicU32::new(0x3F80_0000); // 1.0

fn load(cell: &AtomicU32) -> f32 {
    f32::from_bits(cell.load(Ordering::Relaxed))
}

/// The renderer has no host: block keys it has not met are asked for here and resolved by the
/// next client tick, so a tree's blocks show a frame after they arrive.
fn wanted() -> std::sync::MutexGuard<'static, std::collections::HashSet<String>> {
    static WANTED: std::sync::OnceLock<Mutex<std::collections::HashSet<String>>> =
        std::sync::OnceLock::new();
    WANTED
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

fn block_keys() -> std::sync::MutexGuard<'static, HashMap<String, mod_api::BlockKey>> {
    static KEYS: std::sync::OnceLock<Mutex<HashMap<String, mod_api::BlockKey>>> =
        std::sync::OnceLock::new();
    KEYS.get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// `TreeRenderer`: the blocks, turned about the stump's far edge by the fall.
struct TreeRenderer;

impl EntityRenderer for TreeRenderer {
    fn submit(
        &self,
        entity: &EntityRenderState,
        draw: &mut dyn BlockEntityDrawContext,
    ) -> Result<(), String> {
        let Some(attachments) = entity.attachments.as_ref() else {
            return Ok(());
        };
        let Some(tree) = attachments.get(&FALLING).map_err(|e| format!("{e:?}"))? else {
            return Ok(());
        };
        let [fx, fz] = tree.direction;
        let life = entity.age / 20.0;
        let angle = trees::fall_angle(
            life,
            load(&FALL_LENGTH),
            load(&BOUNCE_HEIGHT),
            load(&BOUNCE_LENGTH),
        );
        // The stems that lie along the fall from the stump push the hinge out by their count.
        let mut reach = 0;
        while tree
            .pieces
            .iter()
            .any(|p| p.stem && p.offset == [fx * (reach + 1), 0, fz * (reach + 1)])
        {
            reach += 1;
        }
        let hinge = 0.5 + reach as f32;
        let axis = [fz as f32, 0.0, -(fx as f32)];
        draw.push();
        draw.rotate_about(axis, -angle, [fx as f32 * hinge, 0.0, fz as f32 * hinge]);
        for piece in &tree.pieces {
            let (id, props) = split_state(&piece.state);
            let Some(block) = block_keys().get(id).copied() else {
                wanted().insert(id.to_owned());
                continue;
            };
            draw.push();
            draw.translate([
                piece.offset[0] as f32 - 0.5,
                piece.offset[1] as f32,
                piece.offset[2] as f32 - 0.5,
            ]);
            draw.block(block, &props);
            draw.pop();
        }
        draw.pop();
        Ok(())
    }
}

// ---------------------------------------------------------------------------- the chop

/// The tool in the player's main hand, by name.
fn held(host: &dyn Host, player: EntityRef) -> Option<String> {
    LivingRef::new(player)
        .with_hand_item(host, Hand::Main, |held| {
            held.item().id(host).map(str::to_owned)
        })
        .flatten()
}

/// Whether `player` would fell a tree by breaking `pos` now, and the tree: not sneaking (or
/// sneaking, with the option inverted), the kind enabled, a tool where the kind needs one.
fn chop(host: &dyn Host, player: EntityRef, pos: [i32; 3]) -> Option<(Kind, Tree, Options)> {
    let sneaking = player.is_shift_key_down(host) == Some(true);
    let invert = flag(host, key::INVERT_CROUCH, false);
    if !flag(host, key::DISABLE_CROUCH, false) && sneaking != invert {
        return None;
    }
    let level = player.level(host)?;
    let options = Options::read(host);
    let (kind, tree) = trees::find(pos, &|p| read_block(host, level, p), options.algorithm)?;
    if !flag(host, key::ENABLED[kind_index(kind)], true) {
        return None;
    }
    if flag(host, key::REQUIRE_TOOL[kind_index(kind)], false)
        && !held(host, player).is_some_and(|item| is_axe(&item))
    {
        return None;
    }
    Some((kind, tree, options))
}

fn read_block(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> Block {
    let key = level.block(host, pos);
    let in_tag = |tag| key.and_then(|k| host.block_in_tag(k, tag)) == Some(true);
    Block {
        name: key.and_then(|k| k.id(host)).unwrap_or_default().to_owned(),
        log: in_tag("minecraft:logs"),
        leaves: in_tag("minecraft:leaves"),
        distance: level
            .block_property(host, pos, "distance")
            .and_then(|d| d.parse().ok()),
        persistent: level.block_property(host, pos, "persistent").as_deref() == Some("true"),
    }
}

/// The log a player is mining and its speed divisor (None: not a tree).
type MinedLog = ([i32; 3], Option<f32>);

#[derive(Default)]
struct Mod {
    queue: Mutex<Vec<Felling>>,
    /// TreeHandler's TREE_SPEED_CACHES: per side and player, the block being mined and its
    /// divisor (None: not a tree), found again when the player moves to another block.
    speeds: Mutex<HashMap<(bool, u32), MinedLog>>,
}

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        let toggle =
            |key: &str, label: &str, default: bool| SettingSpec::toggle(key, label, default);
        let range = |key: &str, label: &str, min: f64, max: f64, step: f64, default: f64| {
            SettingSpec::range(key, label, min, max, step, default)
        };
        let label = |key: &str| format!("fallingtrees.config.{key}");
        let specs = [
            toggle(key::INVERT_CROUCH, &label(key::INVERT_CROUCH), false),
            toggle(key::DISABLE_CROUCH, &label(key::DISABLE_CROUCH), false),
            toggle(key::NO_EXTRA_DAMAGE, &label(key::NO_EXTRA_DAMAGE), false),
            toggle(
                key::NO_EXTRA_EXHAUSTION,
                &label(key::NO_EXTRA_EXHAUSTION),
                false,
            ),
            range(key::LIFETIME, &label(key::LIFETIME), 1.0, 30.0, 0.5, 4.0),
            toggle(key::NO_DYNAMIC_SPEED, &label(key::NO_DYNAMIC_SPEED), false),
            range(
                key::SPEED_MULTIPLICATION,
                &label(key::SPEED_MULTIPLICATION),
                0.0,
                5.0,
                0.1,
                0.5,
            ),
            range(
                key::MAX_SPEED_MULTIPLICATION,
                &label(key::MAX_SPEED_MULTIPLICATION),
                1.0,
                64.0,
                1.0,
                16.0,
            ),
            range(
                key::MAX_LEAVES_RADIUS,
                &label(key::MAX_LEAVES_RADIUS),
                1.0,
                16.0,
                1.0,
                7.0,
            ),
            range(
                key::MAX_LOGS,
                &label(key::MAX_LOGS),
                1.0,
                2048.0,
                1.0,
                256.0,
            ),
            toggle(key::IGNORE_PERSISTENT, &label(key::IGNORE_PERSISTENT), true),
            range(
                key::FALL_LENGTH,
                &label(key::FALL_LENGTH),
                0.5,
                10.0,
                0.1,
                2.5,
            ),
            range(
                key::BOUNCE_HEIGHT,
                &label(key::BOUNCE_HEIGHT),
                0.0,
                45.0,
                1.0,
                10.0,
            ),
            range(
                key::BOUNCE_LENGTH,
                &label(key::BOUNCE_LENGTH),
                0.0,
                5.0,
                0.1,
                1.0,
            ),
            toggle(key::SOUND_ENABLED, &label(key::SOUND_ENABLED), true),
            range(
                key::SOUND_START,
                &label(key::SOUND_START),
                0.0,
                1.0,
                0.05,
                0.7,
            ),
            range(key::SOUND_END, &label(key::SOUND_END), 0.0, 1.0, 0.05, 0.7),
        ];
        for spec in specs {
            let _ = r.register_setting(spec);
        }
        for index in 0..4 {
            let _ = r.register_setting(toggle(
                key::ENABLED[index],
                &label(key::ENABLED[index]),
                true,
            ));
            let _ = r.register_setting(toggle(
                key::REQUIRE_TOOL[index],
                &label(key::REQUIRE_TOOL[index]),
                false,
            ));
        }
        let _ = r.register_attachment_type("fallingtrees:tree", true);
        let _ = r.declare_general_entity(
            "tree",
            GeneralEntityCategory::Misc,
            1.0,
            1.0,
            None,
            GeneralSavePolicy::NoSave,
            false,
            false,
            None,
            10,
            20,
            1,
            &[],
        );
        r.entity_part(TREE, TreePart);
    }

    fn register_entity_renderers(
        &self,
        _: &RegistrationContext,
        event: &mut RegisterEntityRenderersEvent<'_>,
    ) {
        event
            .renderers
            .register(TREE.to_owned(), Arc::new(TreeRenderer));
    }

    /// The animation options for the renderer.
    fn client_tick_post(&self, host: &mut dyn ClientHost, _: &mut ClientTickPostEvent) {
        let asked: Vec<String> = wanted().drain().collect();
        for id in asked {
            if let Some(found) = host.block_key(&id) {
                block_keys().insert(id, found);
            }
        }
        for (cell, key) in [
            (&FALL_LENGTH, key::FALL_LENGTH),
            (&BOUNCE_HEIGHT, key::BOUNCE_HEIGHT),
            (&BOUNCE_LENGTH, key::BOUNCE_LENGTH),
        ] {
            if let Some(value) = host.setting_f64(key) {
                cell.store((value as f32).to_bits(), Ordering::Relaxed);
            }
        }
    }

    fn break_block(&self, host: &mut dyn ServerHost, event: &mut BreakBlockEvent<'_>) -> Flow {
        if !host.on(MOD_ID) {
            return Flow::Continue;
        }
        let Some(player) = host.players().acting_player() else {
            return Flow::Continue;
        };
        let host: &dyn Host = &*host;
        let mode = player.game_mode(host);
        if !matches!(mode, Some(GameMode::Survival | GameMode::Creative)) {
            return Flow::Continue;
        }
        let Some((kind, tree, _)) = chop(host, player.entity(), event.pos) else {
            return Flow::Continue;
        };
        let tool = LivingRef::new(player.entity())
            .with_hand_item(host, Hand::Main, |held| held.project())
            .flatten();
        let Some(level) = player.level(host) else {
            return Flow::Continue;
        };
        let survival = matches!(mode, Some(GameMode::Survival));
        self.queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Felling {
                level,
                player,
                origin: event.pos,
                kind,
                tree,
                tool,
                survival,
            });
        // `EventResult.interruptFalse`: the break is the tree's now.
        Flow::Cancel
    }

    fn player_break_speed(&self, host: HostSide, event: &mut PlayerBreakSpeedEvent) -> Flow {
        let (server, host): (bool, &dyn Host) = match &host {
            HostSide::Server(host) => (true, &**host),
            HostSide::Client(host) => (false, &**host),
        };
        if !host.on(MOD_ID) || flag(host, key::NO_DYNAMIC_SPEED, false) {
            return Flow::Continue;
        }
        let mut speeds = self.speeds.lock().unwrap_or_else(|e| e.into_inner());
        let slot = (server, event.player);
        let divisor = match speeds.get(&slot) {
            Some(&(pos, divisor)) if pos == event.pos => divisor,
            _ => {
                let divisor = host
                    .entity(event.player)
                    .and_then(|player| chop(host, player, event.pos))
                    .map(|(_, tree, options)| {
                        trees::speed_divisor(
                            tree.len() + usize::from(tree.logs.is_empty()),
                            options.speed_multiplication,
                            options.max_speed_multiplication,
                        )
                    });
                speeds.insert(slot, (event.pos, divisor));
                divisor
            }
        };
        if let Some(divisor) = divisor {
            event.new_speed /= divisor;
        }
        Flow::Continue
    }

    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        if !matches!(event.phase, ServerTickPhase::ServerTickEnd { .. }) {
            return;
        }
        let queued = std::mem::take(&mut *self.queue.lock().unwrap_or_else(|e| e.into_inner()));
        for felling in queued {
            felling.run(host);
        }
    }

    /// A world closed: what trees were still falling in it are gone.
    fn server_stopped(&self, _: &mut dyn ServerHost, _: &mut ServerStoppedEvent) {
        pending().clear();
        self.queue.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axes_by_name() {
        assert!(is_axe("minecraft:iron_axe") && !is_axe("minecraft:iron_pickaxe"));
    }

    #[test]
    fn a_state_splits_into_block_and_properties() {
        assert_eq!(
            split_state("minecraft:oak_log[axis=y]"),
            ("minecraft:oak_log", vec![("axis", "y")])
        );
        assert_eq!(
            split_state("minecraft:vine[east=true,north=false]"),
            ("minecraft:vine", vec![("east", "true"), ("north", "false")])
        );
        assert_eq!(
            split_state("minecraft:cactus"),
            ("minecraft:cactus", vec![])
        );
    }

    #[test]
    fn the_falling_tree_round_trips_through_its_attachment() {
        let tree = Falling {
            pieces: vec![
                Piece {
                    offset: [0, 0, 0],
                    stem: true,
                    state: "minecraft:oak_log[axis=y]".into(),
                },
                Piece {
                    offset: [1, 3, -1],
                    stem: false,
                    state: "minecraft:oak_leaves[distance=1,persistent=false]".into(),
                },
            ],
            direction: [0, -1],
        };
        assert_eq!(Falling::from_nbt(&tree.to_nbt()), Some(tree));
    }
}
