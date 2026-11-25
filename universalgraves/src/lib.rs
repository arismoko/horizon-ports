//! Universal Graves 3.0.3 (Patbox; LGPL-3.0-only) on Sunrise's mod API, with no mixin and no game module.
//!
//! A player who dies without keepInventory leaves a grave instead of drops: `living_drops` takes the death's
//! item list and holds it until the server tick, which stands a grave block (`universalgraves:grave`) on the
//! nearest free block within 8 of where the player died and saves the stacks in its block entity's typed state.
//! The owner, anyone in creative, and anyone once the protection time is over, take the stacks back with a right
//! click (they go into the inventory, what doesn't fit lands at the player's feet) or by breaking the grave (they
//! spill where it stands). A grave past its expiry spills by itself. The block can't be blown up.
mod rules;

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use mod_api::{
    AttachmentCodec, AttachmentDescriptor, AttachmentKey, BlockEntityOnLoadEvent, BlockEntityPart,
    BlockEntityPartState, BlockPart, BlockUpdateFlags, BreakBlockEvent, Compound, Flow, GameMode,
    Hand, Hooks, Host, HostSide, InteractionResult, ItemStack, LevelRef, LivingDropsEvent,
    PlayerRef, Registrar, RegistrationContext, RightClickBlockEvent, ServerHost, ServerTickEvent,
    ServerTickPhase, SettingKey, SettingSpec, SettingValue, Text, edit_part_state, read_state,
};

use rules::{Meta, Timers};

const GRAVE: &str = "universalgraves:grave";
const STATE_ID: &str = "universalgraves:grave";
const PROTECTION: &str = "protection_seconds";
const EXPIRY: &str = "expiry_seconds";
/// The defaults: protected for 900 s, no expiry.
const PROTECTION_DEFAULT: f64 = 900.0;
const EXPIRY_DEFAULT: f64 = 0.0;
/// How often the graves are checked for expiry, in ticks.
const EXPIRY_CHECK: u64 = 20;

/// The key the registration pass issued for the grave's state.
static STATE: RwLock<Option<AttachmentKey<Compound>>> = RwLock::new(None);
/// The keys the registration pass issued for the two settings.
static SETTINGS: RwLock<Option<(SettingKey, SettingKey)>> = RwLock::new(None);

fn state_key() -> Option<AttachmentKey<Compound>> {
    STATE.read().unwrap_or_else(|e| e.into_inner()).clone()
}

fn timers(host: &dyn Host) -> Timers {
    let seconds = |key: SettingKey, default: f64| match host.setting(key) {
        Ok(SettingValue::Range(v)) => v,
        _ => default,
    };
    match *SETTINGS.read().unwrap_or_else(|e| e.into_inner()) {
        Some((p, e)) => {
            Timers::from_seconds(seconds(p, PROTECTION_DEFAULT), seconds(e, EXPIRY_DEFAULT))
        }
        None => Timers::from_seconds(PROTECTION_DEFAULT, EXPIRY_DEFAULT),
    }
}

/// What a dead player's grave is to hold, until the server tick makes it.
struct Pending {
    dim: String,
    at: [i32; 3],
    meta: Meta,
    stacks: Vec<ItemStack>,
}

#[derive(Default)]
struct Shared {
    pending: Mutex<Vec<Pending>>,
    /// The graves seen (made, loaded): the ones checked for expiry.
    graves: Mutex<HashSet<(LevelRef, [i32; 3])>>,
    ticks: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The grave block entity: one typed `Compound` state, never synced.
struct GraveEntity(AttachmentKey<Compound>, Arc<Shared>);

#[mod_api::sunrise_part]
impl BlockPart for GraveEntity {}

#[mod_api::sunrise_part]
impl BlockEntityPart for GraveEntity {
    fn block_entity_on_load(&self, event: &mut BlockEntityOnLoadEvent<'_>) {
        let mod_api::BlockEntitySideScope::Server(scope) = &mut event.subject else {
            return;
        };
        if let Some((level, pos)) = scope.with_view(|view| (view.level(), view.position())) {
            lock(&self.1.graves).insert((level, pos));
        }
    }
}

impl BlockEntityPartState for GraveEntity {
    type Data = Compound;

    fn state_key(&self) -> &AttachmentKey<Compound> {
        &self.0
    }
}

/// The grave at `pos`, read from its block entity: its meta and its stacks.
fn read_grave(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> Option<(Meta, Vec<ItemStack>)> {
    let be = host.block_entity(level, pos)?;
    let key = state_key()?;
    let saved = read_state(host, be, &key, |c| c.cloned()).ok()??;
    let (meta, encoded) = rules::read(&saved)?;
    let stacks = encoded
        .iter()
        .filter_map(|c| ItemStack::from_nbt(host, c))
        .collect();
    Some((meta, stacks))
}

fn is_grave(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> bool {
    level.block(host, pos).and_then(|k| k.id(host)) == Some(GRAVE)
}

/// Empties the grave's state, so its stacks are held only by whoever took them.
fn clear_grave(host: &mut dyn ServerHost, level: LevelRef, pos: [i32; 3]) {
    if let Some(be) = host.block_entity(level, pos)
        && let Some(key) = state_key()
    {
        let _ = edit_part_state(host, be, &key, |_, _, c| *c = Compound::new());
    }
}

/// Spills `stacks` where a grave stood.
fn spill(host: &mut dyn ServerHost, level: LevelRef, pos: [i32; 3], stacks: Vec<ItemStack>) {
    for stack in stacks {
        if let Err(e) = level.pop_resource(host, pos, stack) {
            eprintln!("[universalgraves] spill at {pos:?}: {e:?}");
        }
    }
}

/// Gives the player the stacks (into the inventory, the rest at their feet).
fn give(host: &mut dyn ServerHost, level: LevelRef, player: PlayerRef, stacks: Vec<ItemStack>) {
    let feet = player.entity().position(&*host).map_or([0, 0, 0], |p| {
        [
            p[0].floor() as i32,
            p[1].floor() as i32,
            p[2].floor() as i32,
        ]
    });
    for stack in stacks {
        match player.give_stack(host, stack.clone()) {
            Ok(None) => {}
            Ok(Some(rest)) => {
                let _ = level.pop_resource(host, feet, rest);
            }
            Err(e) => {
                eprintln!("[universalgraves] give_stack: {e:?}");
                let _ = level.pop_resource(host, feet, stack);
            }
        }
    }
}

fn tell(host: &mut dyn ServerHost, player: PlayerRef, line: &str) {
    let _ = player.action_bar(host, Text::literal(line));
}

/// Makes the grave of `p`: a free block near where the player died gets the block and the stacks; with none free
/// they drop where the player died, as they would have.
fn place(host: &mut dyn ServerHost, p: Pending) -> Option<(LevelRef, [i32; 3])> {
    let Some(level) = host.level(&p.dim) else {
        eprintln!(
            "[universalgraves] no level {}: {} stacks lost",
            p.dim,
            p.stacks.len()
        );
        return None;
    };
    let reader: &dyn ServerHost = &*host;
    let Some(state) = reader.block_state_id(GRAVE) else {
        eprintln!("[universalgraves] the grave block is not registered");
        spill(host, level, p.at, p.stacks);
        return None;
    };
    let mut meta = p.meta;
    meta.created = level.game_time(reader).unwrap_or(0);
    let mut encoded: Vec<Compound> = Vec::new();
    let mut unsaved: Vec<ItemStack> = Vec::new();
    let mut stacks: Vec<ItemStack> = Vec::new();
    for s in p.stacks {
        match s.to_nbt(reader) {
            Some(c) => {
                encoded.push(c);
                stacks.push(s);
            }
            None => unsaved.push(s),
        }
    }
    let spot = rules::offsets(rules::MAX_DISTANCE)
        .into_iter()
        .find_map(|o| {
            let pos = [p.at[0] + o[0], p.at[1] + o[1], p.at[2] + o[2]];
            let free = level
                .block(reader, pos)
                .and_then(|k| k.id(reader))
                .is_some_and(rules::replaceable);
            free.then_some(pos)
        });
    let flags = BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS;
    let made = spot.filter(|pos| matches!(level.set_block(host, *pos, state, flags), Ok(true)));
    let Some(pos) = made else {
        spill(host, level, p.at, stacks);
        spill(host, level, p.at, unsaved);
        return None;
    };
    let saved = rules::write(&meta, encoded);
    let stored = match (host.block_entity(level, pos), state_key()) {
        (Some(be), Some(key)) => edit_part_state(host, be, &key, |_, _, c| *c = saved).is_ok(),
        _ => false,
    };
    if !stored {
        eprintln!("[universalgraves] grave at {pos:?} has no state: spilling");
        level.remove_block(host, pos, false);
        spill(host, level, pos, stacks);
        spill(host, level, pos, unsaved);
        return None;
    }
    spill(host, level, p.at, unsaved);
    if let Some(owner) = host.player(meta.owner) {
        tell(
            host,
            owner,
            &format!("Your grave is at {} {} {}", pos[0], pos[1], pos[2]),
        );
    }
    Some((level, pos))
}

#[derive(Default)]
struct Mod {
    shared: Arc<Shared>,
}

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        let protection = r.register_setting(SettingSpec::range(
            PROTECTION,
            "universalgraves.config.protection_seconds",
            0.0,
            86_400.0,
            30.0,
            PROTECTION_DEFAULT,
        ));
        let expiry = r.register_setting(SettingSpec::range(
            EXPIRY,
            "universalgraves.config.expiry_seconds",
            0.0,
            604_800.0,
            60.0,
            EXPIRY_DEFAULT,
        ));
        if let (Ok(p), Ok(e)) = (protection, expiry) {
            *SETTINGS.write().unwrap_or_else(|e| e.into_inner()) = Some((p, e));
        }
        let key = match r.register_attachment_type(
            AttachmentDescriptor::new(STATE_ID, Compound::new).serialize(AttachmentCodec {
                encode: |c| Ok(c.clone()),
                decode: |c| Ok(c.clone()),
            }),
        ) {
            Ok(key) => key,
            Err(e) => {
                eprintln!("[universalgraves] attachment {STATE_ID}: {e}");
                return;
            }
        };
        *STATE.write().unwrap_or_else(|e| e.into_inner()) = Some(key.clone());
        let kind = r.block_entity_type("grave").blocks(&[GRAVE]).build();
        if let Err(e) = r.block_entity_part_with_state(&kind, GraveEntity(key, self.shared.clone()))
        {
            eprintln!("[universalgraves] grave block entity part refused: {e:?}");
        }
    }

    /// `PlayerEntity.dropInventory`: a dying player's items wait for the tick that makes their grave.
    fn living_drops(&self, host: &mut dyn ServerHost, event: &mut LivingDropsEvent<'_>) -> Flow {
        let Some(uuid) = event.player.filter(|_| event.kind == "minecraft:player") else {
            return Flow::Continue;
        };
        if event.drops.iter().all(|d| d.stack.count() == 0) {
            return Flow::Continue;
        }
        let Some(player) = host.player(uuid) else {
            return Flow::Continue;
        };
        let Some(dim) = player.dimension(&*host).map(|d| d.to_string()) else {
            return Flow::Continue;
        };
        let Some(at) = host.entity(event.entity).and_then(|e| e.position(&*host)) else {
            return Flow::Continue;
        };
        let name = player.name(&*host).unwrap_or_default().to_owned();
        let stacks: Vec<ItemStack> = event
            .drops
            .drain(..)
            .map(|d| d.stack)
            .filter(|s| s.count() > 0)
            .collect();
        lock(&self.shared.pending).push(Pending {
            dim,
            at: [
                at[0].floor() as i32,
                at[1].floor() as i32,
                at[2].floor() as i32,
            ],
            meta: Meta {
                owner: uuid,
                name,
                created: 0,
            },
            stacks,
        });
        Flow::Continue
    }

    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        if !matches!(event.phase, ServerTickPhase::ServerTickEnd { .. }) {
            return;
        }
        let made = std::mem::take(&mut *lock(&self.shared.pending));
        for p in made {
            if let Some(grave) = place(host, p) {
                lock(&self.shared.graves).insert(grave);
            }
        }
        let n = self.shared.ticks.fetch_add(1, Ordering::Relaxed) + 1;
        if !n.is_multiple_of(EXPIRY_CHECK) {
            return;
        }
        let known: Vec<(LevelRef, [i32; 3])> = lock(&self.shared.graves).iter().copied().collect();
        let rules_now = timers(&*host);
        for (level, pos) in known {
            if !is_grave(&*host, level, pos) {
                lock(&self.shared.graves).remove(&(level, pos));
                continue;
            }
            let now = level.game_time(&*host).unwrap_or(0);
            let Some((meta, stacks)) = read_grave(&*host, level, pos) else {
                continue;
            };
            if rules::expired(rules_now, meta.created, now) {
                clear_grave(host, level, pos);
                level.remove_block(host, pos, false);
                spill(host, level, pos, stacks);
                lock(&self.shared.graves).remove(&(level, pos));
                if let Some(owner) = host.player(meta.owner) {
                    tell(host, owner, "Your grave has crumbled");
                }
            }
        }
    }

    /// `GraveBlock.onUse`: the stacks go to the player and the grave goes.
    fn right_click_block(&self, host: HostSide<'_>, event: &mut RightClickBlockEvent<'_>) -> Flow {
        let HostSide::Server(host) = host else {
            return Flow::Continue;
        };
        let click = event.click;
        let dim = click.dimension.key().id().to_string();
        let Some(level) = host.level(&dim) else {
            return Flow::Continue;
        };
        if !is_grave(&*host, level, click.pos) {
            return Flow::Continue;
        }
        *event.cancellation_result = InteractionResult::Success;
        event.cancelled = true;
        if !matches!(click.hand, Hand::Main) {
            return Flow::Cancel;
        }
        let Some(player) = host.player(click.player) else {
            return Flow::Cancel;
        };
        let Some((meta, stacks)) = read_grave(&*host, level, click.pos) else {
            return Flow::Cancel;
        };
        if !may_take(&*host, level, player, &meta) {
            tell(host, player, "This grave isn't yours");
            return Flow::Cancel;
        }
        clear_grave(host, level, click.pos);
        level.remove_block(host, click.pos, false);
        lock(&self.shared.graves).remove(&(level, click.pos));
        give(host, level, player, stacks);
        Flow::Cancel
    }

    /// `GraveBlock.onBreak`: whoever may take it breaks it and its stacks spill; others are told it isn't theirs.
    fn break_block(&self, host: &mut dyn ServerHost, event: &mut BreakBlockEvent<'_>) -> Flow {
        let Some(player) = host.players().acting_player() else {
            return Flow::Continue;
        };
        let Some(dim) = player.dimension(&*host).map(|d| d.to_string()) else {
            return Flow::Continue;
        };
        let Some(level) = host.level(&dim) else {
            return Flow::Continue;
        };
        if !is_grave(&*host, level, event.pos) {
            return Flow::Continue;
        }
        let Some((meta, stacks)) = read_grave(&*host, level, event.pos) else {
            return Flow::Continue;
        };
        if !may_take(&*host, level, player, &meta) {
            tell(host, player, "This grave isn't yours");
            return Flow::Cancel;
        }
        clear_grave(host, level, event.pos);
        lock(&self.shared.graves).remove(&(level, event.pos));
        spill(host, level, event.pos, stacks);
        Flow::Continue
    }
}

fn may_take(host: &dyn Host, level: LevelRef, player: PlayerRef, meta: &Meta) -> bool {
    let creative = player.game_mode(host) == Some(GameMode::Creative);
    let now = level.game_time(host).unwrap_or(0);
    rules::may_take(
        timers(host),
        meta.owner,
        player.uuid(),
        creative,
        meta.created,
        now,
    )
}
