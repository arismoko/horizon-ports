//! sophisticatedcore: Sophisticated Core 1.2.7.15.166 (P3pp3rF1y, Salandora; Fabric port; GPL-3.0) and the rules of
//! Sophisticated Backpacks 3.23.4.5.110 (P3pp3rF1y, Ridanisaurus, Salandora; GPL-3.0), from the decompiled jars
//! (tools/dl/sophisticatedcore/src, tools/dl/sophisticatedbackpacks/src). data/ and assets/ are the core jar's
//! content (mods/sophisticatedcore); the backpacks' items, blocks, models and recipes are the content package
//! sophisticatedbackpacks (tools/sophisticatedbackpacks_port.py). All of it is the mod API: no game module, no mixin.
//!
//! As Farmer's Delight serves its add-ons, the core serves the storage items listed in `storage_items` (the six
//! backpacks now; Sophisticated Storage's later). The parts:
//! - `contents`: a storage's slots and upgrades live in its stack (custom_data `inventory` / `upgradeInventory`,
//!   the jar's keys), so they stay with the item when it is dropped, placed, worn or crafted into the next tier;
//! - `menu`: the storage screen as a mod chest menu (pages, sort, upgrade slots, upgrade settings, the crafting
//!   grid), opened from the hand, by the Open Backpack key (B; the trinkets back slot first, then the chest slot,
//!   then the inventory), or on a placed backpack;
//! - `ticking`: what upgrades do while carried (pickup, magnet, void, feeding, compacting, refill) and the
//!   restock/deposit sneak-click on an inventory block;
//! - here: placing a backpack (sneak + use on a block), picking it back up (sneak + empty hand), its block entity
//!   (one slot holding the backpack, spilled when broken), the chest loot injections (Homestead's
//!   `chestLootEnabled = true`) and the crafting kinds (`crafting`).
//!
//! Not ported: the jar's own screen (tabs, sorting buttons, the memory and no-sort slot settings), the anvil
//! upgrade (it sits in its slot and does nothing), inception in a backpack placed as a block.
// Menus and inventories here are addressed by slot number, which these loops keep explicit.
#![allow(clippy::needless_range_loop, clippy::type_complexity)]

mod compression;
mod contents;
mod cooking;
mod crafting;
mod everlasting;
mod filter;
mod filter_view;
mod fluid_view;
mod fluids;
mod hopper;
mod hopper_view;
mod inception;
mod infinity;
mod jukebox;
mod liquid_caps;
mod menu;
mod mobs;
mod options;
mod paintbrush;
mod places;
mod render;
mod station;
mod storage;
mod storage_items;
mod swapper;
mod table;
mod ticking;
mod tool;
mod upgrades;
mod worn;
mod xp_bucket;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use mod_api::serde_json::json;
use mod_api::{
    AttackEntityEvent, BlockEntityOnLoadEvent, BlockEntityPart, BlockPart, BlockSide, BlockState,
    BlockUpdateFlags, BreakBlockEvent, BuildCreativeModeTabContentsEvent,
    CanFitInsideContainerItemsEvent, ClientHost, DataLoadContext, EntityJoinLevelEvent,
    EquipmentSlot, FinalizeSpawnEvent, Flow, Hand, Hooks, Host, HostSide, IPayloadContext,
    IPayloadHandler, ITEMS, Identifier, InputKeyEvent, InteractionResult, ItemPart,
    ItemPickedUpPostEvent, ItemResource, ItemStack, KeyAction, KeyRegistration,
    LeftClickBlockEvent, LevelSide, LivingDropsEvent, LivingRef, LoopContext, LootTableLoadEvent,
    MenuClickedEvent, MenuClosedEvent, MenuPart, MenuReplace, ModContainer, MouseButtonPostEvent,
    OnBlockPlacedByEvent, PayloadBytes, PayloadHandleError, PayloadId, Registrar,
    RegistrationContext, ReloadContext, ResourceHandler, RightClickBlockEvent, RightClickItemEvent,
    ServerHost, ServerTickEvent, ServerTickPhase, SoundSource, SpawnReason, StateForPlacementEvent,
    TagsUpdatedEvent, TooltipEvent, UseItemOnEvent, UseOnEvent, UseWithoutItemEvent,
};

use contents::Contents;
use menu::{Outcome, Session};
use places::Loc;
use storage_items::Family;

/// The crafting kinds (their ids must appear here: tools/next_content_fix.py keeps recipes of a type the owning
/// package's lib.rs names).
pub const UPGRADE_NEXT_TIER: &str = "sophisticatedcore:upgrade_next_tier";
pub const STORAGE_DYE: &str = "sophisticatedcore:storage_dye";

const OPEN: &str = "sophisticatedcore:open";
const KEY: &str = "open_backpack";
const ACT: &str = "sophisticatedcore:action";

/// The keys besides Open Backpack (`KeybindHandler`): id, label, the default key, the code sent to the server.
/// The original binds Sort to the middle mouse button, Inventory Interaction to C, and the first two upgrade toggles to
/// Z and X; the rest are unbound. (Tool Swap needs the tool swapper upgrade, which has no rules here.)
const ACTIONS: [(&str, &str, &str, u8); 7] = [
    (
        "sort",
        "keybind.sophisticatedbackpacks.sort",
        "key.mouse.middle",
        SORT,
    ),
    (
        "inventory_interaction",
        "keybind.sophisticatedbackpacks.inventory_interaction",
        "key.keyboard.c",
        INTERACT,
    ),
    (
        "toggle_upgrade_1",
        "keybind.sophisticatedbackpacks.toggle_upgrade_1",
        "key.keyboard.z",
        TOGGLE,
    ),
    (
        "toggle_upgrade_2",
        "keybind.sophisticatedbackpacks.toggle_upgrade_2",
        "key.keyboard.x",
        TOGGLE + 1,
    ),
    (
        "toggle_upgrade_3",
        "keybind.sophisticatedbackpacks.toggle_upgrade_3",
        "key.keyboard.unknown",
        TOGGLE + 2,
    ),
    (
        "toggle_upgrade_4",
        "keybind.sophisticatedbackpacks.toggle_upgrade_4",
        "key.keyboard.unknown",
        TOGGLE + 3,
    ),
    (
        "toggle_upgrade_5",
        "keybind.sophisticatedbackpacks.toggle_upgrade_5",
        "key.keyboard.unknown",
        TOGGLE + 4,
    ),
];
/// `BACKPACK_CHECK_COOLDOWN`.
const BACKPACK_CHECK: u64 = 40;
/// `canBePlacedInContainerItems` as last read.
static IN_CONTAINER_ITEMS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
const SORT: u8 = 1;
const INTERACT: u8 = 2;
const TOGGLE: u8 = 10;

/// `GLFW`'s mouse button `button` as vanilla names it.
fn mouse_name(button: i32) -> String {
    match button {
        0 => "key.mouse.left".into(),
        1 => "key.mouse.right".into(),
        2 => "key.mouse.middle".into(),
        n => format!("key.mouse.{}", n + 1),
    }
}

/// The vanilla chests the jar's loot modifiers add its `inject/chests/*` tables to.
const LOOT: [&str; 8] = [
    "abandoned_mineshaft",
    "bastion_treasure",
    "desert_pyramid",
    "end_city_treasure",
    "nether_bridge",
    "shipwreck_treasure",
    "simple_dungeon",
    "woodland_mansion",
];

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

enum Job {
    Open(u128, Loc),
    Key(u128),
    /// A key's action (`ACTIONS`' code).
    Action(u128, u8),
    /// A decoration table was used: the player, its level and position.
    Table(u128, String, [i32; 3]),
}

#[derive(Default)]
struct Shared {
    jobs: Mutex<Vec<Job>>,
    sessions: Mutex<HashMap<u128, Session>>,
    /// The decoration tables players have open.
    tables: table::Sessions,
    /// The tick a player last placed or interacted with a backpack (the client's use packet follows the sneak-use).
    used: Mutex<HashMap<u128, u64>>,
    ticks: AtomicU64,
    /// Placed storages whose upgrades work on their own.
    placed: storage::Placed,
    /// A player's last limited barrel deposit (tick, position): a second within 10 ticks deposits everything.
    deposits: Mutex<HashMap<u128, (u64, [i32; 3])>>,
    /// Who wears a backpack, as the clients were told.
    told: worn::Told,
}

impl Shared {
    fn queue(&self, job: Job) {
        lock(&self.jobs).push(job);
    }

    fn tick(&self) -> u64 {
        self.ticks.load(Ordering::Relaxed)
    }

    fn mark_used(&self, uuid: u128) {
        lock(&self.used).insert(uuid, self.tick());
    }

    fn just_used(&self, uuid: u128) -> bool {
        lock(&self.used)
            .get(&uuid)
            .is_some_and(|t| self.tick().saturating_sub(*t) <= 2)
    }

    fn open(&self, host: &mut dyn ServerHost, uuid: u128, loc: Loc) {
        let replace = if lock(&self.sessions).contains_key(&uuid) {
            MenuReplace::InPlace
        } else {
            MenuReplace::CloseFirst
        };
        if let Some(prev) = lock(&self.sessions).remove(&uuid) {
            self.closed_block(host, &prev.loc);
        }
        if let Some(s) = menu::open(host, uuid, loc, replace) {
            let placed = match &s.loc {
                Loc::Block { dim, pos } => host.level(dim).map(|l| (l, *pos)),
                _ => None,
            };
            match (placed, s.kind.family) {
                (Some((level, pos)), Family::Backpack) => {
                    let _ = (level, pos);
                    if let Some(p) = host.player(uuid) {
                        let _ = p.play_sound(
                            host,
                            "minecraft:item.armor.equip_leather",
                            SoundSource::Players,
                            0.5,
                            1.0,
                        );
                    }
                }
                (Some((level, pos)), family) => {
                    self.placed.note(level, pos);
                    let at = pos.map(|v| f64::from(v) + 0.5);
                    let sound = match family {
                        Family::Chest => "minecraft:block.chest.open",
                        Family::ShulkerBox => "minecraft:block.shulker_box.open",
                        _ => "minecraft:block.barrel.open",
                    };
                    level.play_sound(host, sound, at, SoundSource::Blocks, 0.5, 1.0, None);
                    if family == Family::Barrel {
                        storage::change_props(host, level, pos, &[("open", "true")]);
                    }
                }
                (None, _) => {
                    if let Some(p) = host.player(uuid) {
                        let _ = p.play_sound(
                            host,
                            "minecraft:item.armor.equip_leather",
                            SoundSource::Players,
                            0.5,
                            1.0,
                        );
                    }
                }
            }
            lock(&self.sessions).insert(uuid, s);
        }
    }

    /// A menu on a placed storage closed: a barrel shuts when nobody else has it open.
    fn closed_block(&self, host: &mut dyn ServerHost, loc: &Loc) {
        let Loc::Block { dim, pos } = loc else { return };
        if lock(&self.sessions).values().any(|s| &s.loc == loc) {
            return;
        }
        let Some(level) = host.level(dim) else { return };
        let Some(kind) = storage::kind_of_block(&storage::block_id(&*host, level, *pos)) else {
            return;
        };
        let at = pos.map(|v| f64::from(v) + 0.5);
        let sound = match kind.family {
            Family::Chest => "minecraft:block.chest.close",
            Family::ShulkerBox => "minecraft:block.shulker_box.close",
            _ => "minecraft:block.barrel.close",
        };
        level.play_sound(host, sound, at, SoundSource::Blocks, 0.5, 1.0, None);
        if kind.family == Family::Barrel {
            storage::change_props(host, level, *pos, &[("open", "false")]);
        }
    }

    /// The Open Backpack key (KeybindHandler / BackpackOpenMessage): the trinkets back slot, the chest slot, then
    /// the hands and the inventory.
    fn open_key(&self, host: &mut dyn ServerHost, uuid: u128) {
        let mut candidates = vec![Loc::Worn(places::BACK.into()), Loc::Inv(38), Loc::Inv(40)];
        if let Some(p) = host.player(uuid)
            && let Some(sel) = p.selected_slot(&*host)
        {
            candidates.insert(1, Loc::Inv(sel as u32));
        }
        candidates.extend((0..36).map(Loc::Inv));
        for loc in candidates {
            if places::storage_at(host, uuid, &loc)
                .is_some_and(|(_, k)| k.family == Family::Backpack)
            {
                self.open(host, uuid, loc);
                return;
            }
        }
        places::message(host, uuid, "No backpack worn or in your inventory");
    }

    /// The backpack a key acts on: the open-backpack order (worn, held, then the inventory).
    fn key_backpack(&self, host: &mut dyn ServerHost, uuid: u128) -> Option<Loc> {
        let mut candidates = vec![Loc::Worn(places::BACK.into()), Loc::Inv(38), Loc::Inv(40)];
        if let Some(sel) = host.player(uuid).and_then(|p| p.selected_slot(&*host)) {
            candidates.insert(1, Loc::Inv(sel as u32));
        }
        candidates.extend((0..36).map(Loc::Inv));
        candidates.into_iter().find(|loc| {
            places::storage_at(host, uuid, loc).is_some_and(|(_, k)| k.family == Family::Backpack)
        })
    }

    /// A key's action: sort the open storage, run the inventory interaction upgrades on the block looked at, or
    /// switch the upgrade in a slot on or off.
    fn action(&self, host: &mut dyn ServerHost, uuid: u128, code: u8) {
        match code {
            SORT => {
                let Some(mut s) = lock(&self.sessions).get(&uuid).cloned() else {
                    return;
                };
                let Some(loc) = places::find(host, uuid, &s.loc, s.id) else {
                    return;
                };
                s.loc = loc;
                let Some((mut st, kind)) = places::storage_at(host, uuid, &s.loc) else {
                    return;
                };
                let mut c = Contents::read(&*host, &st, kind);
                menu::sort(&*host, kind, &mut c);
                c.write(&*host, &mut st, kind);
                places::write(host, uuid, &s.loc, Some(st));
                if self.refresh(host, uuid, &mut s)
                    && let Some(cur) = lock(&self.sessions).get_mut(&uuid)
                    && cur.window == s.window
                {
                    *cur = s;
                }
            }
            INTERACT => {
                let Some(loc) = self.key_backpack(host, uuid) else {
                    return;
                };
                let Some(player) = host.player(uuid) else {
                    return;
                };
                let Some(level) = player.entity().level(&*host) else {
                    return;
                };
                let Some(hit) = player.entity().pick(&*host, 5.0).filter(|h| !h.miss) else {
                    return;
                };
                let face = match hit.face {
                    mod_api::BlockFace::Down => [0, -1, 0],
                    mod_api::BlockFace::Up => [0, 1, 0],
                    mod_api::BlockFace::North => [0, 0, -1],
                    mod_api::BlockFace::South => [0, 0, 1],
                    mod_api::BlockFace::West => [-1, 0, 0],
                    _ => [1, 0, 0],
                };
                ticking::interact(host, uuid, &loc, level, hit.block, face);
            }
            n if (TOGGLE..TOGGLE + 5).contains(&n) => {
                let slot = usize::from(n - TOGGLE);
                let Some(loc) = self.key_backpack(host, uuid) else {
                    return;
                };
                let Some((mut st, kind)) = places::storage_at(host, uuid, &loc) else {
                    return;
                };
                let mut c = Contents::read(&*host, &st, kind);
                let Some(Some(up)) = c.upgrades.get_mut(slot) else {
                    places::message(host, uuid, "No upgrade in that slot");
                    return;
                };
                let on = !upgrades::flag(up, "enabled", true);
                upgrades::set_flag(up, "enabled", on);
                c.write(&*host, &mut st, kind);
                places::write(host, uuid, &loc, Some(st));
                places::message(
                    host,
                    uuid,
                    if on {
                        "Upgrade switched on"
                    } else {
                        "Upgrade switched off"
                    },
                );
            }
            _ => {}
        }
    }

    /// `CommonEventHandler.onWorldTick`: a player carrying more backpacks than allowed is slowed by the excess.
    fn too_many(&self, host: &mut dyn ServerHost, players: &[u128], opts: &options::Options) {
        let Some(slowness) = host.effect_key("minecraft:slowness") else {
            return;
        };
        for &uuid in players {
            let Some(player) = host.player(uuid) else {
                continue;
            };
            let mut count = places::inventory(host, player)
                .iter()
                .flatten()
                .filter(|st| {
                    storage_items::by_item(&contents::item_id(&*host, st))
                        .is_some_and(|k| k.family == Family::Backpack)
                })
                .count();
            if places::storage_at(host, uuid, &Loc::Worn(places::BACK.into()))
                .is_some_and(|(_, k)| k.family == Family::Backpack)
            {
                count += 1;
            }
            if let Some(amplifier) =
                options::slowness_amplifier(count, opts.max_backpacks, opts.slowness_ratio)
            {
                let effect = mod_api::ActiveEffect::new(
                    slowness,
                    (BACKPACK_CHECK * 2) as i32,
                    amplifier,
                    false,
                    false,
                    false,
                );
                mod_api::LivingRef::new(player.entity()).add_effect(host, &effect);
            }
        }
    }

    /// Shows the session's view again (the client predicted a pickup the server didn't make).
    fn refresh(&self, host: &mut dyn ServerHost, uuid: u128, s: &mut Session) -> bool {
        let Some(loc) = places::find(host, uuid, &s.loc, s.id) else {
            return false;
        };
        s.loc = loc;
        let Some((st, kind)) = places::storage_at(host, uuid, &s.loc) else {
            return false;
        };
        let c = Contents::read(&*host, &st, kind);
        if let menu::View::Main { page } = s.view
            && page >= s.pages()
        {
            s.view = menu::View::Main { page: 0 };
        }
        let items = menu::render(host, s, &c);
        host.menu_set_items(uuid, s.window, items).is_ok()
    }

    fn close(&self, host: &mut dyn ServerHost, uuid: u128) {
        let prev = lock(&self.sessions).remove(&uuid);
        host.close_menu(uuid);
        if let Some(prev) = prev {
            self.closed_block(host, &prev.loc);
        }
    }

    /// DecorationTableBlock.use: opens the table's menu.
    fn open_table(&self, host: &mut dyn ServerHost, uuid: u128, dim: &str, pos: [i32; 3]) {
        let Some((_, be)) = table::entity(&*host, dim, pos) else {
            return;
        };
        let items = table::render(host, be);
        let replace =
            if lock(&self.sessions).contains_key(&uuid) || lock(&self.tables).contains_key(&uuid) {
                MenuReplace::InPlace
            } else {
                MenuReplace::CloseFirst
            };
        if let Some(prev) = lock(&self.sessions).remove(&uuid) {
            self.closed_block(host, &prev.loc);
        }
        let title = mod_api::Text::literal("Decoration Table");
        if let Ok(Some(window)) = host.open_menu(uuid, table::ROWS, title, items, replace) {
            lock(&self.tables).insert(
                uuid,
                table::Session {
                    window,
                    dim: dim.to_string(),
                    pos,
                },
            );
        }
    }

    /// A click in a decoration table's menu; false when the window isn't one.
    fn table_click(&self, host: &mut dyn ServerHost, ev: &MenuClickedEvent) -> bool {
        let uuid = ev.uuid;
        let Some(s) = lock(&self.tables)
            .get(&uuid)
            .filter(|s| s.window == ev.window)
            .cloned()
        else {
            return false;
        };
        let Some((_, be)) = table::entity(&*host, &s.dim, s.pos) else {
            lock(&self.tables).remove(&uuid);
            host.close_menu(uuid);
            return true;
        };
        if matches!(ev.mode, 0 | 1)
            && let Some(msg) = table::click(host, uuid, &s, ev.slot, ev.button == 1)
        {
            places::message(host, uuid, msg);
        }
        let items = table::render(host, be);
        let _ = host.menu_set_items(uuid, s.window, items);
        true
    }

    /// A click in one of the storage menus; false when the window isn't one.
    fn click(&self, host: &mut dyn ServerHost, ev: &MenuClickedEvent) -> bool {
        if self.table_click(host, ev) {
            return true;
        }
        let uuid = ev.uuid;
        let Some(mut s) = lock(&self.sessions)
            .get(&uuid)
            .filter(|s| s.window == ev.window)
            .cloned()
        else {
            return false;
        };
        let Some(loc) = places::find(host, uuid, &s.loc, s.id) else {
            self.close(host, uuid);
            return true;
        };
        s.loc = loc;
        let Some((st, kind)) = places::storage_at(host, uuid, &s.loc) else {
            self.close(host, uuid);
            return true;
        };
        let c = Contents::read(&*host, &st, kind);
        let (outcome, msg) = menu::click(host, uuid, &s, &c, ev);
        let base = s.control() as i64;
        let button_hit =
            (base..base + 9).contains(&i64::from(ev.slot)) && !matches!(outcome, Outcome::Nothing);
        let changed = match outcome {
            Outcome::Changed(c2) => Some(c2),
            Outcome::ChangedView(c2, v) => {
                s.view = v;
                Some(c2)
            }
            Outcome::View(v) => {
                s.view = v;
                None
            }
            Outcome::Close => {
                self.close(host, uuid);
                return true;
            }
            Outcome::Nothing => None,
        };
        if let Some(c2) = changed
            && let Some(mut fresh) = places::read(host, uuid, &s.loc)
        {
            c2.write(&*host, &mut fresh, kind);
            if !places::write(host, uuid, &s.loc, Some(fresh)) {
                eprintln!(
                    "[sophisticatedcore] the storage at {:?} wasn't written",
                    s.loc
                );
            }
        }
        if button_hit
            && options::tune().button_sound
            && let Some(p) = host.player(uuid)
        {
            let _ = p.play_sound(
                host,
                "minecraft:ui.button.click",
                SoundSource::Master,
                0.25,
                1.0,
            );
        }
        if let Some(m) = msg {
            places::message(host, uuid, m);
        }
        if !self.refresh(host, uuid, &mut s) {
            self.close(host, uuid);
            return true;
        }
        if let Some(cur) = lock(&self.sessions).get_mut(&uuid)
            && cur.window == s.window
        {
            *cur = s;
        }
        true
    }

    /// Re-renders the open menus showing storages that `ids` name (an upgrade changed them).
    fn refresh_ids(&self, host: &mut dyn ServerHost, ids: &[[i32; 4]]) {
        if ids.is_empty() {
            return;
        }
        let open: Vec<(u128, Session)> = lock(&self.sessions)
            .iter()
            .filter(|(_, s)| ids.contains(&s.id))
            .map(|(u, s)| (*u, s.clone()))
            .collect();
        for (uuid, mut s) in open {
            self.refresh(host, uuid, &mut s);
        }
    }
}

struct OpenHandler(Arc<Shared>);

impl IPayloadHandler for OpenHandler {
    fn handle(
        &self,
        _payload: PayloadBytes,
        context: &mut dyn IPayloadContext,
    ) -> Result<(), PayloadHandleError> {
        if let Ok(p) = context.player() {
            self.0.queue(Job::Key(p.uuid()));
        }
        Ok(())
    }
}

struct ViewHandler;

impl IPayloadHandler for ViewHandler {
    fn handle(
        &self,
        payload: PayloadBytes,
        _context: &mut dyn IPayloadContext,
    ) -> Result<(), PayloadHandleError> {
        worn::heard(&payload.0);
        Ok(())
    }
}

struct ActionHandler(Arc<Shared>);

impl IPayloadHandler for ActionHandler {
    fn handle(
        &self,
        payload: PayloadBytes,
        context: &mut dyn IPayloadContext,
    ) -> Result<(), PayloadHandleError> {
        if let (Ok(p), Some(code)) = (context.player(), payload.0.first()) {
            self.0.queue(Job::Action(p.uuid(), *code));
        }
        Ok(())
    }
}

/// The hand's inventory slot.
fn hand_slot(host: &dyn mod_api::Host, uuid: u128, hand: Hand) -> Option<u32> {
    match hand {
        Hand::Main => host.player(uuid)?.selected_slot(host).map(|s| s as u32),
        _ => Some(40),
    }
}

/// BackpackItem.useOn's placement: on the clicked face (or in the clicked block when it can be replaced), facing
/// the player; the block entity keeps the backpack. Creative keeps the hand's backpack and places a copy with new
/// contents id (BackpackWrapper.cloneBackpack).
fn place(
    host: &mut dyn ServerHost,
    uuid: u128,
    hand: Hand,
    pos: [i32; 3],
    face: [i32; 3],
    stack: &ItemStack,
) -> bool {
    let Some(kind) = storage_items::by_item(&contents::item_id(&*host, stack)) else {
        return false;
    };
    let Some(block) = kind.block else {
        return false;
    };
    let Some(player) = host.player(uuid) else {
        return false;
    };
    let Some(dim) = player.dimension(&*host).map(|d| d.to_string()) else {
        return false;
    };
    let Some(level) = host.level(&dim) else {
        return false;
    };
    const REPLACEABLE: [&str; 9] = [
        "minecraft:air",
        "minecraft:cave_air",
        "minecraft:void_air",
        "minecraft:short_grass",
        "minecraft:fern",
        "minecraft:snow",
        "minecraft:dead_bush",
        "minecraft:vine",
        "minecraft:water",
    ];
    let id_at = |host: &dyn mod_api::Host, p: [i32; 3]| {
        level
            .block(host, p)
            .and_then(|k| k.id(host))
            .unwrap_or_default()
            .to_owned()
    };
    let target = if REPLACEABLE.contains(&id_at(&*host, pos).as_str()) {
        pos
    } else {
        [pos[0] + face[0], pos[1] + face[1], pos[2] + face[2]]
    };
    if !REPLACEABLE.contains(&id_at(&*host, target).as_str()) {
        return false;
    }
    // The player's horizontal facing from yaw (0 south, 90 west, 180 north, 270 east); the backpack faces back.
    let yaw = player.entity().rotation(&*host).map_or(0.0, |r| r[0]);
    let quarter = ((f64::from(yaw) / 90.0).round() as i64).rem_euclid(4);
    let facing = ["north", "east", "south", "west"][quarter as usize];
    let Some(state) = host.block_state_id(&format!("{block}[facing={facing}]")) else {
        return false;
    };
    let flags = BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS;
    if !matches!(level.set_block(host, target, state, flags), Ok(true)) {
        return false;
    }
    let mut placed = contents::with_count(stack, 1).unwrap_or(stack.clone());
    let creative = places::creative(&*host, uuid);
    if creative {
        let mut d = contents::custom_data(&placed);
        contents::put(
            &mut d,
            "contentsUuid",
            mod_api::Tag::IntArray(contents::new_uuid().to_vec()),
        );
        contents::set_custom_data(&mut placed, d);
    } else {
        contents::ensure_uuid(&mut placed);
    }
    if !places::write(
        host,
        uuid,
        &Loc::Block {
            dim: dim.clone(),
            pos: target,
        },
        Some(placed),
    ) {
        eprintln!(
            "[sophisticatedcore] the placed backpack's block entity took nothing at {target:?}"
        );
    }
    if !creative && let Some(slot) = hand_slot(&*host, uuid, hand) {
        places::set_slot(host, player, slot, stack.clone().shrink(1));
    }
    let at = target.map(|v| f64::from(v) + 0.5);
    level.play_sound(
        host,
        "minecraft:block.wool.place",
        at,
        SoundSource::Blocks,
        1.0,
        0.8,
        None,
    );
    true
}

/// BackpackBlock.putInPlayersHandAndRemove: a sneak-click with an empty hand takes the backpack.
fn pick_up(host: &mut dyn ServerHost, uuid: u128, dim: &str, pos: [i32; 3]) -> bool {
    let loc = Loc::Block {
        dim: dim.to_string(),
        pos,
    };
    let Some(st) = places::read(host, uuid, &loc) else {
        return false;
    };
    let Some(player) = host.player(uuid) else {
        return false;
    };
    let Some(level) = host.level(dim) else {
        return false;
    };
    if let Some(kind) = storage_items::by_item(&contents::item_id(&*host, &st))
        && infinity::guards(&*host, player, kind, &st)
    {
        places::message(
            host,
            uuid,
            "Only an operator can pick up a backpack with an infinity upgrade",
        );
        return false;
    }
    if !places::write(host, uuid, &loc, None) {
        return false;
    }
    let Some(air) = host.block_state_id("minecraft:air") else {
        return false;
    };
    let _ = level.set_block(
        host,
        pos,
        air,
        BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS,
    );
    let main = hand_slot(&*host, uuid, Hand::Main);
    let inv = places::inventory(host, player);
    match main {
        Some(slot) if inv[slot as usize].is_none() && places::complete(&*host, player) => {
            places::set_slot(host, player, slot, Some(st));
        }
        _ => places::give_or_drop(host, player, &st),
    }
    let at = pos.map(|v| f64::from(v) + 0.5);
    level.play_sound(
        host,
        "minecraft:item.armor.equip_leather",
        at,
        SoundSource::Blocks,
        0.5,
        1.0,
        None,
    );
    true
}

/// BackpackItem: sneak-use on a block runs the inventory interaction upgrades or places the backpack; it never
/// goes inside another storage (`canBePlacedInContainerItems = false`); its tooltip.
struct StorageItemPart(Arc<Shared>);

#[mod_api::sunrise_part]
impl ItemPart for StorageItemPart {
    fn use_on(&self, host: HostSide<'_>, event: &mut UseOnEvent<'_>) -> Option<InteractionResult> {
        let HostSide::Server(host) = host else {
            return None;
        };
        let slot = hand_slot(&*host, event.player, event.hand)?;
        if !places::sneaking(&*host, event.player) {
            // BackpackItem.useOn passes and Item.use opens it; Horizon's window doesn't send the item use after a
            // block click, so the click on a block opens it here (the item use that may follow is skipped).
            self.0.mark_used(event.player);
            self.0.queue(Job::Open(event.player, Loc::Inv(slot)));
            return Some(InteractionResult::Success);
        }
        let dim = host.player(event.player)?.dimension(&*host)?.to_string();
        let level = host.level(&dim)?;
        let loc = Loc::Inv(slot);
        if ticking::interact(host, event.player, &loc, level, event.pos, event.face) {
            self.0.mark_used(event.player);
            return Some(InteractionResult::Success);
        }
        let stack = event.stack.clone();
        if place(
            host,
            event.player,
            event.hand,
            event.pos,
            event.face,
            &stack,
        ) {
            self.0.mark_used(event.player);
            return Some(InteractionResult::Success);
        }
        None
    }

    /// `canBePlacedInContainerItems`, read from the settings a tick ago (the context has none).
    fn can_fit_inside_container_items(
        &self,
        _context: &dyn LoopContext,
        event: &mut CanFitInsideContainerItemsEvent<'_>,
    ) {
        event.fits = IN_CONTAINER_ITEMS.load(Ordering::Relaxed);
    }

    fn tooltip(&self, host: &dyn ClientHost, event: &mut TooltipEvent<'_>) {
        let Some(kind) = storage_items::by_item(&contents::item_id(host, event.stack)) else {
            return;
        };
        let c = Contents::read(host, event.stack, kind);
        let used = c.slots.iter().flatten().count();
        event.lines.push((
            mod_api::Text::literal(format!("§7{used} of {} slots used", kind.slot_count())),
            0xFFAAAAAA,
        ));
        for u in c.upgrades.iter().flatten() {
            let name = host
                .hover_name(u)
                .map(|n| host.text_string(&n))
                .unwrap_or_default();
            event
                .lines
                .push((mod_api::Text::literal(format!("§9{name}")), 0xFF5555FF));
        }
        let unknown = c.unknown_count();
        if unknown > 0 {
            event.lines.push((
                mod_api::Text::literal(format!("§c{unknown} stacks of mods not installed (kept)")),
                0xFFFF5555,
            ));
        }
        if used == 0 && c.upgrades.iter().all(Option::is_none) {
            event.lines.push((
                mod_api::Text::literal("§8Use to open; sneak-use on a block to place"),
                0xFF555555,
            ));
        }
    }
}

/// BackpackBlock.use: opens the placed backpack; a sneak-click with an empty hand takes it.
struct StorageBlockPart(Arc<Shared>);

#[mod_api::sunrise_part]
impl BlockPart for StorageBlockPart {
    fn use_item_on(
        &self,
        host: &mut dyn ServerHost,
        event: &mut UseItemOnEvent<'_>,
    ) -> Option<InteractionResult> {
        let click = event.click;
        if !matches!(click.hand, Hand::Main) {
            return Some(InteractionResult::Success);
        }
        // An empty hand goes on to use_without_item (the vanilla TRY_WITH_EMPTY_HAND), where sneaking takes it.
        let empty = host
            .player(click.player)
            .and_then(|p| {
                mod_api::LivingRef::new(p.entity())
                    .with_hand_item(&*host, Hand::Main, |s| s.count())
            })
            .is_none_or(|n| n == 0);
        if empty {
            return None;
        }
        let dim = click.dimension.key().id().to_string();
        self.0.queue(Job::Open(
            click.player,
            Loc::Block {
                dim,
                pos: click.pos,
            },
        ));
        Some(InteractionResult::Success)
    }

    fn use_without_item(
        &self,
        host: &mut dyn ServerHost,
        event: &mut UseWithoutItemEvent<'_>,
    ) -> Option<InteractionResult> {
        let click = event.click;
        if !matches!(click.hand, Hand::Main) {
            return Some(InteractionResult::Success);
        }
        let dim = click.dimension.key().id().to_string();
        if places::sneaking(&*host, click.player) {
            pick_up(host, click.player, &dim, click.pos);
        } else {
            self.0.queue(Job::Open(
                click.player,
                Loc::Block {
                    dim,
                    pos: click.pos,
                },
            ));
        }
        Some(InteractionResult::Success)
    }

    /// BackpackBlock.getExplosionResistance: a placed backpack with the everlasting upgrade resists like bedrock.
    fn explosion_resistance(
        &self,
        level: &dyn mod_api::LevelHost,
        event: &mut mod_api::ExplosionResistanceEvent<'_>,
    ) -> Option<f32> {
        let be = level.block_entity([event.pos.x, event.pos.y, event.pos.z])?;
        let data = level.block_entity_data(be)?;
        everlasting::in_block_entity(&data).then_some(everlasting::BEDROCK_RESISTANCE)
    }

    /// BackpackBlock.canEntityDestroy: nothing destroys a placed backpack with the everlasting upgrade.
    fn can_entity_destroy(
        &self,
        host: &dyn Host,
        event: &mut mod_api::CanEntityDestroyEvent,
    ) -> Option<bool> {
        let level = event.entity.level(host)?;
        let st = storage::read_be(host, level, [event.pos.x, event.pos.y, event.pos.z])?;
        everlasting::has(&st).then_some(false)
    }
}

/// The placed backpack's block entity: the declared container (one slot, no screen) holds the backpack.
struct StorageBlockEntity;

#[mod_api::sunrise_part]
impl BlockPart for StorageBlockEntity {}

#[mod_api::sunrise_part]
impl BlockEntityPart for StorageBlockEntity {}

/// Sophisticated Storage's block items (StorageBlockItem): the placed block's block entity takes the stack; the
/// tooltip.
struct PlacedItemPart(Arc<Shared>);

#[mod_api::sunrise_part]
impl ItemPart for PlacedItemPart {
    fn on_block_placed_by(&self, host: &mut dyn ServerHost, event: &mut OnBlockPlacedByEvent<'_>) {
        if let Some((level, pos)) = storage::placed(host, event.placer, event.pos, event.stack) {
            self.0.placed.note(level, pos);
        }
    }

    fn tooltip(&self, host: &dyn ClientHost, event: &mut TooltipEvent<'_>) {
        let Some(kind) = storage_items::by_item(&contents::item_id(host, event.stack)) else {
            return;
        };
        if kind.family == Family::LimitedBarrel {
            let s = if kind.slot_count() == 1 {
                "slot"
            } else {
                "slots"
            };
            event.lines.push((
                mod_api::Text::literal(format!(
                    "§8{} {s}, {}x base stack size multiplier",
                    kind.slot_count(),
                    kind.base_multiplier()
                )),
                0xFF555555,
            ));
        }
        let c = Contents::read(host, event.stack, kind);
        if storage::is_packed(event.stack) {
            event
                .lines
                .push((mod_api::Text::literal("§6Packed".to_string()), 0xFFFFAA00));
        }
        let used = c.slots.iter().flatten().count();
        if used > 0 {
            event.lines.push((
                mod_api::Text::literal(format!("§7{used} of {} slots used", c.slots.len())),
                0xFFAAAAAA,
            ));
        }
        for u in c.upgrades.iter().flatten() {
            let name = host
                .hover_name(u)
                .map(|n| host.text_string(&n))
                .unwrap_or_default();
            event
                .lines
                .push((mod_api::Text::literal(format!("§9{name}")), 0xFF5555FF));
        }
        let unknown = c.unknown_count();
        if unknown > 0 {
            event.lines.push((
                mod_api::Text::literal(format!("§c{unknown} stacks of mods not installed (kept)")),
                0xFFFF5555,
            ));
        }
    }
}

/// StorageToolItem: a click acts on the storage by the tool's mode; the air changes the mode.
struct ToolItem;

#[mod_api::sunrise_part]
impl ItemPart for ToolItem {
    fn use_on(&self, host: HostSide<'_>, event: &mut UseOnEvent<'_>) -> Option<InteractionResult> {
        let HostSide::Server(host) = host else {
            return None;
        };
        let dim = host.player(event.player)?.dimension(&*host)?.to_string();
        let level = host.level(&dim)?;
        let block = storage::block_id(&*host, level, event.pos);
        match tool::use_on(host, level, event.pos, &block, event.stack) {
            tool::Used::Passed => None,
            tool::Used::Done(link) => {
                if tool::mode(event.stack) == tool::Mode::Link && block == tool::CONTROLLER {
                    let slot = hand_slot(&*host, event.player, event.hand)?;
                    let mut t = event.stack.clone();
                    tool::set_controller_link(&mut t, link);
                    let player = host.player(event.player)?;
                    places::set_slot(host, player, slot, Some(t));
                    places::message(host, event.player, &tool::mode(event.stack).message(link));
                }
                Some(InteractionResult::Success)
            }
        }
    }

    fn use_item(
        &self,
        host: HostSide<'_>,
        event: &mut mod_api::UseItemEvent<'_>,
    ) -> Option<InteractionResult> {
        let HostSide::Server(host) = host else {
            return None;
        };
        let slot = hand_slot(&*host, event.player, event.hand)?;
        let mut t = event.stack.clone();
        let line = tool::use_in_air(&mut t, places::sneaking(&*host, event.player));
        let player = host.player(event.player)?;
        places::set_slot(host, player, slot, Some(t));
        places::message(host, event.player, &line);
        Some(InteractionResult::Success)
    }
}

/// PaintbrushItem: a click on a storage paints it with the brush's colours.
struct PaintbrushItem;

#[mod_api::sunrise_part]
impl ItemPart for PaintbrushItem {
    fn use_on(&self, host: HostSide<'_>, event: &mut UseOnEvent<'_>) -> Option<InteractionResult> {
        let HostSide::Server(host) = host else {
            return None;
        };
        let player = host.player(event.player)?;
        let dim = player.dimension(&*host)?.to_string();
        let level = host.level(&dim)?;
        let block = storage::block_id(&*host, level, event.pos);
        let slot = hand_slot(&*host, event.player, event.hand)?;
        paintbrush::use_on(host, player, slot, event.stack, level, event.pos, &block)
            .then_some(InteractionResult::Success)
    }
}

/// DecorationTableBlock: using it opens the table's menu.
struct TableBlock(Arc<Shared>);

#[mod_api::sunrise_part]
impl BlockPart for TableBlock {
    fn use_item_on(
        &self,
        host: &mut dyn ServerHost,
        event: &mut UseItemOnEvent<'_>,
    ) -> Option<InteractionResult> {
        if places::sneaking(&*host, event.click.player) {
            return None;
        }
        self.use_without_item_at(event.click)
    }

    fn use_without_item(
        &self,
        _host: &mut dyn ServerHost,
        event: &mut UseWithoutItemEvent<'_>,
    ) -> Option<InteractionResult> {
        self.use_without_item_at(event.click)
    }
}

impl TableBlock {
    fn use_without_item_at(&self, click: &mod_api::BlockClick<'_>) -> Option<InteractionResult> {
        if !matches!(click.hand, Hand::Main) {
            return Some(InteractionResult::Success);
        }
        let dim = click.dimension.key().id().to_string();
        self.0.queue(Job::Table(click.player, dim, click.pos));
        Some(InteractionResult::Success)
    }
}

/// StorageBlockBase: placement facing, opening, the limited barrels' front.
struct StorageBlock(Arc<Shared>);

#[mod_api::sunrise_part]
impl BlockPart for StorageBlock {
    fn state_for_placement(
        &self,
        level: LevelSide<'_>,
        event: &mut StateForPlacementEvent<'_>,
    ) -> Option<Option<BlockState>> {
        let (LevelSide::Server(level) | LevelSide::Client(level)) = level;
        let block = level.state_block(event.state)?.to_string();
        let kind = storage::kind_of_block(&block)?;
        let ctx = event.context;
        let facing = storage::placement_facing(kind.family, ctx.yaw, ctx.pitch, ctx.face);
        let current = level.state_string(event.state)?;
        let rest = current
            .split_once('[')
            .map(|(_, r)| r.trim_end_matches(']').to_string())
            .unwrap_or_default();
        let mut props: Vec<String> = rest
            .split(',')
            .filter(|kv| !kv.is_empty() && !kv.starts_with("facing="))
            .map(str::to_string)
            .collect();
        props.push(format!("facing={facing}"));
        level
            .block_state_id(&format!("{block}[{}]", props.join(",")))
            .map(Some)
    }

    fn use_item_on(
        &self,
        host: &mut dyn ServerHost,
        event: &mut UseItemOnEvent<'_>,
    ) -> Option<InteractionResult> {
        let click = event.click;
        if !matches!(click.hand, Hand::Main) {
            return Some(InteractionResult::Success);
        }
        let empty = click.stack.is_none_or(|s| s.count() == 0);
        if empty || places::sneaking(&*host, click.player) {
            return None;
        }
        self.0.use_storage(host, click, true)
    }

    fn use_without_item(
        &self,
        host: &mut dyn ServerHost,
        event: &mut UseWithoutItemEvent<'_>,
    ) -> Option<InteractionResult> {
        let click = event.click;
        if !matches!(click.hand, Hand::Main) {
            return Some(InteractionResult::Success);
        }
        self.0.use_storage(host, click, false)
    }
}

/// The storages' menus are vanilla chest menus the mod fills (`open_menu`); their clicks are the mod's.
struct StorageMenu(Arc<Shared>);

#[mod_api::sunrise_part]
impl MenuPart for StorageMenu {
    fn menu_clicked(&self, host: &mut dyn ServerHost, event: &mut MenuClickedEvent<'_>) -> Flow {
        if self.0.click(host, event) {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }
}

/// A placed storage's block entity: one slot holding the storage's stack; noted when it loads, so its upgrades work.
struct StorageEntity(Arc<Shared>);

#[mod_api::sunrise_part]
impl BlockPart for StorageEntity {}

#[mod_api::sunrise_part]
impl BlockEntityPart for StorageEntity {
    fn block_entity_update_tag(
        &self,
        event: &mut mod_api::BlockEntityUpdateTagEvent<'_>,
    ) -> Option<mod_api::Compound> {
        render::update_tag(event.entity)
    }

    fn block_entity_submit(
        &self,
        host: &dyn ClientHost,
        event: &mut mod_api::BlockEntitySubmitEvent<'_>,
    ) {
        render::submit(
            host,
            event.state,
            event.block,
            event.entity,
            &mut *event.draw,
        );
    }

    fn block_entity_on_load(&self, event: &mut BlockEntityOnLoadEvent<'_>) {
        let mod_api::BlockEntitySideScope::Server(scope) = &mut event.subject else {
            return;
        };
        if let Some((level, pos)) = scope.with_view(|view| (view.level(), view.position())) {
            self.0.placed.note(level, pos);
        }
    }
}

impl Shared {
    /// StorageBlockBase.use: opens the storage (a double chest's main half); a packed storage doesn't open; a limited
    /// barrel's front takes the held stack (twice in 10 ticks: every stack of that item) instead.
    fn use_storage(
        &self,
        host: &mut dyn ServerHost,
        click: &mod_api::BlockClick<'_>,
        holding: bool,
    ) -> Option<InteractionResult> {
        let dim = click.dimension.key().id().to_string();
        let level = host.level(&dim)?;
        let pos = click.pos;
        let kind = storage::kind_of_block(&storage::block_id(&*host, level, pos))?;
        let main = storage::main_pos(&*host, level, pos);
        if storage::read_be(&*host, level, main).is_some_and(|s| storage::is_packed(&s)) {
            places::message(
                host,
                click.player,
                "Packed: break it to take it with its contents",
            );
            return Some(InteractionResult::Success);
        }
        if kind.family == Family::LimitedBarrel && !places::sneaking(&*host, click.player) {
            let facing = level
                .block_property(&*host, pos, "facing")
                .unwrap_or_default();
            if click.hit.face == storage::dir_vec(&facing) {
                let player = host.player(click.player)?;
                let slot = storage::limited_slot(kind, &facing, pos, click.hit.point);
                let now = self.tick();
                let again = lock(&self.deposits)
                    .get(&click.player)
                    .is_some_and(|(t, p)| *p == pos && now.saturating_sub(*t) < 10);
                lock(&self.deposits).insert(click.player, (now, pos));
                if holding || again {
                    storage::limited_deposit(host, player, level, pos, slot, again);
                }
                return Some(InteractionResult::Success);
            }
        }
        self.queue(Job::Open(
            click.player,
            storage::menu_loc(&*host, &dim, level, pos),
        ));
        Some(InteractionResult::Success)
    }
}

#[derive(Default)]
struct Mod {
    shared: Arc<Shared>,
}

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, cx: &RegistrationContext) {
        let has = |ns: &str| cx.mods().iter().any(|p| p.namespaces().any(|n| n == ns));
        let backpacks = has("sophisticatedbackpacks");
        let storage_loaded = has("sophisticatedstorage");
        // Experience, as the original's `xp_still`: a tank fluid with no block or flowing form; it travels in
        // the `xp_bucket` item.
        let xp = r.declare_fluid("xp_still");
        let empty_bucket = r.item_key("minecraft:bucket");
        r.item_capability(
            &mod_api::ITEM_FLUIDS,
            "sophisticatedcore:xp_bucket",
            move |_, place| {
                Some(Box::new(xp_bucket::XpBucket {
                    place: place.clone(),
                    xp,
                    empty: empty_bucket,
                })
                    as Box<dyn ResourceHandler<mod_api::FluidResource>>)
            },
        );
        let _ = everlasting::SPARKLE.set(r.particle_key("minecraft:happy_villager"));
        let keys = crafting::keys(r, backpacks, storage_loaded);
        r.register_crafting_recipe_serializer(
            UPGRADE_NEXT_TIER,
            crafting::UpgradeNextTier { keys: keys.clone() },
        );
        r.register_crafting_recipe_serializer(STORAGE_DYE, crafting::StorageDye { keys });
        let mut blocks = Vec::new();
        let mut tiers = Vec::new();
        for s in storage_items::BACKPACKS
            .iter()
            .filter(|s| has(storage_items::namespace(s)))
        {
            let item = r.item_key(s.id);
            r.item_part(item, StorageItemPart(self.shared.clone()));
            let kind: &'static storage_items::StorageItem = s;
            r.item_capability(&mod_api::ITEM_ENERGY, s.id, move |_, place| {
                Some(Box::new(liquid_caps::Backed::item(kind, place.clone()))
                    as Box<dyn mod_api::EnergyHandler>)
            });
            r.item_capability(&mod_api::ITEM_FLUIDS, s.id, move |_, place| {
                options::tune().liquids.item_fluids.then(|| {
                    Box::new(liquid_caps::Backed::item(kind, place.clone()))
                        as Box<dyn ResourceHandler<mod_api::FluidResource>>
                })
            });
            if let Some(b) = s.block {
                let key = r.block_key(b);
                r.block_capability(
                    &mod_api::FLUIDS,
                    &key,
                    move |_, at: BlockSide, _, _, offer| {
                        offer.offer(|| {
                            Box::new(liquid_caps::Backed::block(kind, at.level, at.pos))
                                as Box<dyn ResourceHandler<mod_api::FluidResource>>
                        });
                    },
                );
                r.block_capability(
                    &mod_api::ENERGY,
                    &key,
                    move |_, at: BlockSide, _, _, offer| {
                        offer.offer(|| {
                            Box::new(liquid_caps::Backed::block(kind, at.level, at.pos))
                                as Box<dyn mod_api::EnergyHandler>
                        });
                    },
                );
                r.block_part(key, StorageBlockPart(self.shared.clone()));
                r.block_container(
                    &key,
                    ModContainer {
                        slots: 1,
                        ..Default::default()
                    },
                );
                blocks.push(b);
                tiers.push((s.tier, key));
            }
        }
        worn::set_blocks(tiers);
        if !blocks.is_empty() {
            let kind = r.block_entity_type("backpack").blocks(&blocks).build();
            if let Err(e) = r.block_entity_part(&kind, StorageBlockEntity) {
                eprintln!("[sophisticatedcore] backpack block entity part refused: {e:?}");
            }
        }
        let mut storage_blocks = Vec::new();
        for s in storage_items::STORAGE
            .iter()
            .filter(|s| has(storage_items::namespace(s)))
        {
            let item = r.item_key(s.id);
            r.item_part(item, PlacedItemPart(self.shared.clone()));
            let key = r.block_key(s.id);
            r.block_part(key, StorageBlock(self.shared.clone()));
            r.block_container(
                &key,
                ModContainer {
                    slots: 1,
                    ..Default::default()
                },
            );
            let kind: &'static storage_items::StorageItem = s;
            r.block_capability_before_game(&ITEMS, &key, move |_, at: BlockSide, _, _, offer| {
                offer.offer(|| {
                    Box::new(storage::Handler::new(at.level, at.pos, kind))
                        as Box<dyn ResourceHandler<ItemResource>>
                });
            });
            storage_blocks.push(s.id);
        }
        if storage_loaded {
            let tool = r.item_key(tool::TOOL);
            r.item_part(tool, ToolItem);
            let brush = r.item_key(paintbrush::ID);
            r.item_part(brush, PaintbrushItem);
            let key = r.block_key("sophisticatedstorage:decoration_table");
            r.block_part(key, TableBlock(self.shared.clone()));
            r.block_container(
                &key,
                ModContainer {
                    slots: table::SLOTS,
                    ..Default::default()
                },
            );
            let _ = r.register_attachment_type("sophisticatedstorage:decoration_table", false);
            let kind = r
                .block_entity_type("decoration_table")
                .blocks(&["sophisticatedstorage:decoration_table"])
                .build();
            if let Err(e) = r.block_entity_part_with_state(&kind, table::TableEntity) {
                eprintln!("[sophisticatedcore] decoration table block entity part refused: {e:?}");
            }
        }
        if !storage_blocks.is_empty() {
            let kind = r
                .block_entity_type("storage")
                .blocks(&storage_blocks)
                .build();
            if let Err(e) = r.block_entity_part(&kind, StorageEntity(self.shared.clone())) {
                eprintln!("[sophisticatedcore] storage block entity part refused: {e:?}");
            }
        }
        // The original's default, B (unbound in vanilla 26.3).
        r.register_key_mapping(
            KeyRegistration::new(KEY, "key.keyboard.b")
                .label("keybind.sophisticatedbackpacks.open_backpack"),
        );
        for (name, kind) in options::SPECS {
            let label = format!("sophisticatedcore.config.{name}");
            let spec = match kind {
                options::Kind::Toggle(default) => {
                    mod_api::SettingSpec::toggle(*name, &label, *default)
                }
                options::Kind::Range(min, max, step, default) => {
                    mod_api::SettingSpec::range(*name, &label, *min, *max, *step, *default)
                }
            };
            let _ = r.register_setting(spec);
        }
        let _ = r.register_setting(mod_api::SettingSpec::choice(
            options::key::SORT_POSITION,
            "sophisticatedcore.config.sort_buttons_position",
            options::SORT_POSITIONS
                .map(|p| format!("sophisticatedcore.config.sort_buttons_position.{p}")),
            0,
        ));
        for (name, kind) in options::tier_specs() {
            let label = format!("sophisticatedcore.config.{name}");
            if let options::Kind::Range(min, max, step, default) = kind {
                let _ = r.register_setting(mod_api::SettingSpec::range(
                    &name, &label, min, max, step, default,
                ));
            }
        }
        for (id, label, key, _) in ACTIONS.iter() {
            r.register_key_mapping(KeyRegistration::new(id, key).label(label));
        }
        for rows in 1..=6 {
            r.menu_part(
                &format!("minecraft:generic_9x{rows}"),
                StorageMenu(self.shared.clone()),
            );
        }
        r.payload_handlers()
            .registrar("1")
            .optional()
            .play_to_server(
                PayloadId(OPEN.into()),
                Arc::new(OpenHandler(self.shared.clone())),
            )
            .play_to_server(
                PayloadId(ACT.into()),
                Arc::new(ActionHandler(self.shared.clone())),
            )
            .play_to_client(PayloadId(worn::VIEW.into()), Some(Arc::new(ViewHandler)));
    }

    /// `EntityBackpackAdditionHandler.addBackpack`: a new monster of a listed kind may wear a backpack, with armour
    /// and more health.
    fn finalize_spawn(&self, host: &dyn Host, event: &mut FinalizeSpawnEvent<'_>) -> Flow {
        if matches!(event.reason, SpawnReason::Conversion | SpawnReason::Event)
            || mobs::loot_table(host, event.kind).is_none()
        {
            return Flow::Continue;
        }
        let opts = options::Options::read(host);
        let Some(wearer) = mobs::roll(
            event.kind,
            event.pos,
            event.dimension,
            event.difficulty.effective,
            opts.mob_chance,
            opts.mob_armour,
        ) else {
            return Flow::Continue;
        };
        let (mob, edits) = (event.mob, event.edits);
        let Some(mut pack) = mobs::backpack(host, wearer.tier) else {
            return Flow::Continue;
        };
        let jukebox = opts.mob_jukebox
            && mobs::add_jukebox(
                host,
                &mut pack,
                &[
                    event.pos[0].to_bits(),
                    event.pos[1].to_bits(),
                    event.pos[2].to_bits(),
                ],
                &options::Lists::get(host).disc_block,
            );
        if jukebox {
            let _ = edits.add_tag(mob, mobs::JUKEBOX_TAG);
        }
        let _ = edits.set_equipment(mob, EquipmentSlot::Chest, Some(pack));
        for (slot, id) in [
            EquipmentSlot::Head,
            EquipmentSlot::Legs,
            EquipmentSlot::Feet,
        ]
        .into_iter()
        .zip(&wearer.armour)
        {
            if let Some(stack) = id.as_deref().and_then(|id| mobs::armour_stack(host, id)) {
                let _ = edits.set_equipment(mob, slot, Some(stack));
            }
        }
        if opts.mob_health
            && let Ok(id) = Identifier::parse("minecraft:max_health")
            && let Ok(base) = mob.attribute_base(host, &id)
            && let Some(bonus) =
                mobs::health_bonus(base, wearer.min_difficulty).and_then(mobs::health_modifier)
        {
            let _ = edits.add_attribute_modifier(mob, &id, bonus);
        }
        let difficulty = if opts.mob_loot { wearer.difficulty } else { 0 };
        if opts.mob_potions {
            let _ = edits.add_tag(mob, &format!("{}{}", mobs::EFFECTS_TAG, wearer.difficulty));
        }
        let _ = edits.add_tag(mob, &format!("{}{}", mobs::TIER_TAG, wearer.tier));
        let _ = edits.add_tag(mob, &format!("{}{difficulty}", mobs::DIFFICULTY_TAG));
        Flow::Continue
    }

    /// `CreeperMixin` / `removeBeneficialEffects`: a bearer creeper's blast leaves it without its beneficial effects
    /// (vanilla's lingering cloud is made from what is left, after `Level.explode`).
    fn explosion_start(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::ExplosionStartEvent<'_>,
    ) -> Flow {
        let Some(id) = event.explosion.source else {
            return Flow::Continue;
        };
        let Some(entity) = host.entity(id) else {
            return Flow::Continue;
        };
        if entity
            .kind(&*host)
            .and_then(|k| k.id(&*host))
            .map(|n| n.to_string())
            .as_deref()
            != Some("minecraft:creeper")
            || !mobs::is_bearer(&*host, entity)
        {
            return Flow::Continue;
        }
        let living = LivingRef::new(entity);
        for effect in living.effects(&*host).unwrap_or_default() {
            if effect.id.category(&*host) == Some(mod_api::EffectCategory::Beneficial) {
                living.remove_effect(host, effect.id);
            }
        }
        Flow::Continue
    }

    /// `applyPotions`: a bearer new to a level is given its effects (they need the live mob, which the spawn hook
    /// doesn't have).
    fn entity_join_level(
        &self,
        host: &mut dyn ServerHost,
        event: &mut EntityJoinLevelEvent<'_>,
    ) -> Flow {
        let Some(entity) = host.entity(event.entity) else {
            return Flow::Continue;
        };
        if entity
            .tags(&*host)
            .is_some_and(|t| t.contains(mobs::JUKEBOX_TAG))
        {
            jukebox::mob_starts(host, event.entity, u64::from(event.entity));
        }
        // EverlastingBackpackItemEntity: a dropped backpack with the upgrade can't be hurt.
        if host
            .item_entity(entity)
            .and_then(|i| i.with_stack(&*host, |v| v.project()).flatten())
            .is_some_and(|s| everlasting::has(&s))
        {
            entity.set_invulnerable(host, true);
            entity.add_tag(host, everlasting::TAG);
        }
        if event.loaded_from_disk {
            return Flow::Continue;
        }
        let Some((difficulty, min_difficulty)) = mobs::effects_owed(&*host, entity) else {
            return Flow::Continue;
        };
        let living = LivingRef::new(entity);
        for (id, amplifier) in mobs::effects(event.entity, event.pos, difficulty, min_difficulty) {
            if let Some(key) = host.effect_key(id) {
                living.add_effect(
                    host,
                    &mod_api::ActiveEffect::new(
                        key,
                        mobs::EFFECT_TICKS,
                        amplifier,
                        false,
                        true,
                        true,
                    ),
                );
            }
        }
        entity.add_tag(host, mobs::EFFECTS_DONE_TAG);
        Flow::Continue
    }

    /// `handleBackpackDrop`: a monster that spawned with a backpack drops it, filled with loot, to a player's kill by
    /// the drop chance (plus looting); otherwise the backpack is lost.
    fn living_drops(&self, host: &mut dyn ServerHost, event: &mut LivingDropsEvent<'_>) -> Flow {
        let Some(entity) = host.entity(event.entity) else {
            return Flow::Continue;
        };
        let Some((tier, difficulty)) = mobs::tags_of(&*host, entity) else {
            return Flow::Continue;
        };
        event
            .drops
            .retain(|d| !mobs::is_backpack(&contents::item_id(&*host, &d.stack)));
        let killer = host
            .players()
            .server_players()
            .into_iter()
            .find(|p| i64::from(p.entity().id()) == i64::from(event.source.causing));
        let (Some(killer), Some(pos)) = (killer, entity.position(&*host)) else {
            return Flow::Continue;
        };
        let opts = options::Options::read(&*host);
        let looting = LivingRef::new(killer.entity())
            .with_hand_item(&*host, Hand::Main, |held| {
                held.project().map_or(0, |s| mobs::looting_level(&s))
            })
            .unwrap_or(0);
        if mobs::drops(
            tier,
            looting,
            opts.drop_chance,
            opts.looting_chance,
            mobs::drop_roll(event.entity, pos),
        ) && let Some(drop) =
            mobs::dropped_backpack(host, entity, event.kind, tier, difficulty, pos)
        {
            event.drops.push(drop);
        }
        Flow::Continue
    }

    /// The layer that draws a worn backpack on its bearer.
    fn register_entity_layers(
        &self,
        _: &RegistrationContext,
        event: &mut mod_api::RegisterEntityLayersEvent<'_>,
    ) {
        worn::register(event.layers);
    }

    /// A client that starts tracking a bearer is told it wears a backpack.
    fn player_start_tracking(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::PlayerStartTrackingEvent<'_>,
    ) {
        let tier = if event.kind == "minecraft:player" {
            self.shared.told.of(event.entity)
        } else {
            worn::mob_tier(&*host, event.entity)
        };
        // Always told: a client's note of a dead bearer must not outlive its entity id.
        if let Some(p) = host.player(event.player) {
            let _ = p.send_payload(
                host,
                &PayloadId(worn::VIEW.into()),
                worn::encode(event.entity, tier),
            );
        }
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _: &mut mod_api::ClientTickPostEvent) {
        IN_CONTAINER_ITEMS.store(
            options::Options::read(&*host).in_container_items,
            Ordering::Relaxed,
        );
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        if N.fetch_add(1, Ordering::Relaxed).is_multiple_of(20) {
            options::refresh(&*host);
        }
    }

    /// SBPLootInjectModifier: the jar's chest tables join the vanilla chests'.
    fn loot_table_load(&self, _context: &dyn DataLoadContext, event: &mut LootTableLoadEvent<'_>) {
        let Some(chest) = event.id.strip_prefix("minecraft:chests/") else {
            return;
        };
        if _context.setting(options::MOD, options::key::CHEST_LOOT) == Some("false") {
            return;
        }
        if !LOOT.contains(&chest) {
            return;
        }
        let pool = json!({"rolls": 1, "entries": [{"type": "minecraft:loot_table", "value": format!("sophisticatedbackpacks:inject/chests/{chest}")}]});
        match event.table.get_mut("pools").and_then(|p| p.as_array_mut()) {
            Some(pools) => pools.push(pool),
            None => event.table["pools"] = json!([pool]),
        }
    }

    /// The recipes were rebuilt: what compression looked up is asked again.
    fn tags_updated(&self, _ctx: &dyn ReloadContext, _event: &mut TagsUpdatedEvent) {
        compression::forget();
    }

    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        if !matches!(event.phase, ServerTickPhase::ServerTickEnd { .. }) {
            return;
        }
        let n = self.shared.ticks.fetch_add(1, Ordering::Relaxed) + 1;
        if n % 20 == 1 {
            options::refresh(&*host);
        }
        let opts = options::Options::read(&*host);
        IN_CONTAINER_ITEMS.store(opts.in_container_items, Ordering::Relaxed);
        let jobs = std::mem::take(&mut *lock(&self.shared.jobs));
        for job in jobs {
            match job {
                Job::Open(uuid, loc) => self.shared.open(host, uuid, loc),
                Job::Key(uuid) => self.shared.open_key(host, uuid),
                Job::Action(uuid, code) => self.shared.action(host, uuid, code),
                Job::Table(uuid, dim, pos) => self.shared.open_table(host, uuid, &dim, pos),
            }
        }
        let players: Vec<u128> = host
            .players()
            .server_players()
            .iter()
            .map(|p| p.uuid())
            .collect();
        if opts.slowness && n.is_multiple_of(BACKPACK_CHECK) {
            self.shared.too_many(host, &players, &opts);
        }
        if n.is_multiple_of(4) {
            self.shared.told.sync(host, &players);
        }
        for uuid in players {
            let ids = ticking::tick(host, uuid, n);
            self.shared.refresh_ids(host, &ids);
        }
        storage::tick(host, &self.shared.placed, n);
        jukebox::tick(host, n);
    }

    /// StorageTierUpgradeItem.onItemUseFirst and the packing tape: before the block's own use (a vanilla chest would
    /// open).
    fn right_click_block(&self, host: HostSide<'_>, event: &mut RightClickBlockEvent<'_>) -> Flow {
        let click = event.click;
        let HostSide::Server(host) = host else {
            return Flow::Continue;
        };
        if !matches!(click.hand, Hand::Main) {
            return Flow::Continue;
        }
        let Some(stack) = click.stack.filter(|s| s.count() > 0) else {
            return Flow::Continue;
        };
        let held = contents::item_id(&*host, stack);
        if !held.starts_with("sophisticatedstorage:") {
            return Flow::Continue;
        }
        let dim = click.dimension.key().id().to_string();
        let Some(level) = host.level(&dim) else {
            return Flow::Continue;
        };
        if !storage::takes_click(&held, &storage::block_id(&*host, level, click.pos)) {
            return Flow::Continue;
        }
        let (Some(player), Some(slot)) = (
            host.player(click.player),
            hand_slot(&*host, click.player, Hand::Main),
        ) else {
            return Flow::Continue;
        };
        let stack = stack.clone();
        if storage::use_tool(host, player, slot, &stack, level, click.pos) {
            self.shared.placed.note(level, click.pos);
            *event.cancellation_result = InteractionResult::Success;
            event.cancelled = true;
            return Flow::Cancel;
        }
        Flow::Continue
    }

    /// A player breaks a placed storage: its contents spill (or stay, packed / shulker box) before it drops.
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
        if let Some(kind) = storage::kind_of_block(&storage::block_id(&*host, level, event.pos)) {
            if let Some(st) = storage::read_be(&*host, level, event.pos)
                && infinity::guards(&*host, player, kind, &st)
            {
                let _ = player.action_bar(
                    host,
                    mod_api::Text::literal(
                        "Only an operator can break a backpack with an infinity upgrade",
                    ),
                );
                return Flow::Cancel;
            }
            storage::broken(host, player, level, event.pos);
        }
        Flow::Continue
    }

    /// This mod follows its dropped everlasting backpacks' ticks.
    fn entity_tick_listened(
        &self,
        _host: HostSide<'_>,
        event: &mut mod_api::EntityTickListenedEvent,
    ) {
        event.listened = true;
    }

    /// EverlastingBackpackItemEntity.tick: a dropped everlasting backpack rises out of water and lava,
    /// then hangs where it surfaced.
    fn entity_tick_pre(&self, host: HostSide<'_>, event: &mut mod_api::EntityTickPreEvent<'_>) {
        let HostSide::Server(host) = host else {
            return;
        };
        for (&id, &kind) in event.entities.iter().zip(event.kinds) {
            if kind == "minecraft:item"
                && let Some(entity) = host.entity(id)
            {
                everlasting::float(host, entity);
            }
        }
    }

    /// EverlastingBackpackItemEntity: the dropped backpack never despawns; each expiry gives it more life.
    fn item_expire(&self, _host: &mut dyn ServerHost, event: &mut mod_api::ItemExpireEvent<'_>) {
        if everlasting::has(event.stack) {
            event.extra_life = everlasting::EXTRA_LIFE;
        }
    }

    /// ToolSwapperUpgradeWrapper.onAttackEntity: the best weapon the backpack holds goes into the hand.
    fn attack_entity(&self, host: &mut dyn ServerHost, _event: &mut AttackEntityEvent<'_>) -> Flow {
        if let Some(player) = host.players().acting_player() {
            swapper::on_attack(host, player);
        }
        Flow::Continue
    }

    /// LimitedBarrelBlock.attack: a left click on the front takes one of the slot's item (a stack when sneaking).
    fn left_click_block(&self, host: &mut dyn ServerHost, event: &mut LeftClickBlockEvent) -> Flow {
        let Some(player) = host.players().acting_player() else {
            return Flow::Continue;
        };
        let Some(dim) = player.dimension(&*host).map(|d| d.to_string()) else {
            return Flow::Continue;
        };
        let Some(level) = host.level(&dim) else {
            return Flow::Continue;
        };
        swapper::on_block_click(host, player, level, event.pos);
        let Some(kind) = storage::kind_of_block(&storage::block_id(&*host, level, event.pos))
        else {
            return Flow::Continue;
        };
        if kind.family != Family::LimitedBarrel {
            return Flow::Continue;
        }
        let facing = level
            .block_property(&*host, event.pos, "facing")
            .unwrap_or_default();
        if event.face != storage::dir_vec(&facing) {
            return Flow::Continue;
        }
        let point = storage::look_point(&*host, player, event.pos, &facing)
            .unwrap_or(event.pos.map(|v| f64::from(v) + 0.5));
        let slot = storage::limited_slot(kind, &facing, event.pos, point);
        let sneak = places::sneaking(&*host, player.uuid());
        storage::limited_take(host, player, level, event.pos, slot, sneak);
        if places::creative(&*host, player.uuid()) {
            event.cancelled = true;
            return Flow::Cancel;
        }
        Flow::Continue
    }

    /// The jar's creative tab lists every wood of a chest or barrel (WoodStorageBlockBase.addCreativeTabItems); the
    /// tab's json leaves them out, so they come here, after its shulker boxes, blocks and upgrades. (The dyed variants
    /// the jar lists too are left out: a creative tab entry cannot carry its colours here.)
    fn build_creative_mode_tab_contents(
        &self,
        host: HostSide<'_>,
        event: &mut BuildCreativeModeTabContentsEvent<'_>,
    ) {
        if event.tab != "sophisticatedstorage:main" {
            return;
        }
        let h: &dyn Host = match &host {
            HostSide::Server(host) => &**host,
            HostSide::Client(host) => &**host,
        };
        for kind in storage_items::STORAGE
            .iter()
            .filter(|k| storage_items::wooden(k))
        {
            let Some(st) = storage::new_stack(h, kind.id) else {
                continue;
            };
            let woods = if options::Options::read(h).single_wood {
                &storage_items::WOODS[..1]
            } else {
                &storage_items::WOODS[..]
            };
            for w in woods.iter().copied() {
                let mut s = st.clone();
                storage::set_wood(&mut s, kind, w);
                let placed = event.edit.insert(
                    h,
                    mod_api::CreativeTabPlacement::Last,
                    s,
                    mod_api::CreativeTabVisibility::Both,
                );
                if let Err(e) = placed {
                    eprintln!("[sophisticatedcore] creative tab entry refused: {e:?}");
                }
            }
        }
    }

    /// BackpackItem.use: opens the backpack in hand.
    fn right_click_item(&self, host: HostSide<'_>, event: &mut RightClickItemEvent<'_>) -> Flow {
        let click = event.click;
        let HostSide::Server(host) = host else {
            return Flow::Continue;
        };
        if storage_items::by_item(&contents::item_id(&*host, click.stack))
            .is_none_or(|k| k.family != Family::Backpack)
        {
            return Flow::Continue;
        }
        if !self.shared.just_used(click.player)
            && let Some(slot) = hand_slot(&*host, click.player, click.hand)
        {
            self.shared.queue(Job::Open(click.player, Loc::Inv(slot)));
        }
        *event.cancellation_result = InteractionResult::Success;
        event.cancelled = true;
        Flow::Cancel
    }

    fn menu_closed(&self, host: &mut dyn ServerHost, event: &mut MenuClosedEvent) {
        let prev = {
            let mut sessions = lock(&self.shared.sessions);
            if sessions
                .get(&event.uuid)
                .is_some_and(|s| s.window == event.window)
            {
                sessions.remove(&event.uuid)
            } else {
                None
            }
        };
        {
            let mut tables = lock(&self.shared.tables);
            if tables
                .get(&event.uuid)
                .is_some_and(|s| s.window == event.window)
            {
                tables.remove(&event.uuid);
            }
        }
        if let Some(prev) = prev {
            self.shared.closed_block(host, &prev.loc);
        }
    }

    /// What vanilla's pickup put in the inventory goes on into a backpack with a pickup upgrade.
    fn item_picked_up_post(
        &self,
        host: &mut dyn ServerHost,
        event: &mut ItemPickedUpPostEvent<'_>,
    ) {
        let left = event.remaining.as_ref().map_or(0, |s| s.count());
        let Some(picked) =
            contents::with_count(event.original, event.original.count().saturating_sub(left))
        else {
            return;
        };
        let ids = ticking::picked_up(host, event.player, &picked);
        self.shared.refresh_ids(host, &ids);
    }

    /// The Open Backpack key, while playing or in the player's own inventory, and the other keys while playing or
    /// in a screen.
    fn input_key(&self, host: &mut dyn ClientHost, event: &mut InputKeyEvent<'_>) {
        if matches!(event.action, KeyAction::Press) {
            press(host, event.key);
        }
    }

    fn mouse_button_post(&self, host: &mut dyn ClientHost, event: &mut MouseButtonPostEvent) {
        if event.pressed {
            press(host, &mouse_name(event.button));
        }
    }
}

/// What a pressed key (or mouse button), by vanilla's name, does: it matches a mapping of this mod's.
fn press(host: &mut dyn ClientHost, name: &str) {
    let bound = |host: &dyn ClientHost, id: &str| {
        host.mod_key_mapping("sophisticatedcore", id)
            .ok()
            .and_then(|m| m.binding(host).ok().map(|(key, _)| key == name))
            == Some(true)
    };
    if bound(&*host, KEY) {
        let inventory = host
            .observed_screen_id()
            .is_some_and(|s| s.id == "inventory")
            && host.screen_title().is_none();
        if (host.playing() || inventory)
            && let Err(e) =
                host.send_payload_to_server(&PayloadId(OPEN.into()), PayloadBytes(Vec::new()))
        {
            eprintln!("[sophisticatedcore] open: {e:?}");
        }
        return;
    }
    for (id, _, _, code) in ACTIONS {
        if bound(&*host, id) {
            let _ = host.send_payload_to_server(&PayloadId(ACT.into()), PayloadBytes(vec![code]));
            return;
        }
    }
}
