use crate::config::{ServerConfig, server_config, synced_server_config};
use crate::enchant::{
    apply_enchantments_to_stack, catalog, compute_offers, compute_scaled_enchanting_power,
    is_enchanting_catalyst, is_reroll_catalyst, is_stack_enchantable,
    is_unaltered_enchanting_table, roll_row_enchantments, subtract_experience_points,
    total_experience,
};
use crate::payloads::{CluesPayload, ShelfItemPayload, ShelfQueryPayload, channels};
use crate::storage::{
    TableContents, advance_player_seed, comparator_signal, drop_and_clear_table_contents,
    get_or_init_player_seed, read_table_contents, read_table_contents_level, write_table_contents,
};
use mod_api::wire::Writer;
use mod_api::{
    AnalogOutputEvent, AnimateTickEvent, BlockPart, Flow, GameMode, HasAnalogOutputEvent, Host,
    HostSide, HostTransaction, IPayloadContext, ITEMS, InteractionResult, ItemPlace, ItemResource,
    ItemStack, LevelHost, LevelRandomHost, LevelRef, LevelSide, LoopContext, MenuButtonEvent,
    MenuButtonValidEvent, MenuClickedEvent, MenuContainer, MenuKey, MenuPart, MenuRef,
    MenuRemovedEvent, MenuReplace, MenuSlotChangedEvent, MenuSlotRulesEvent, MenuStillValidEvent,
    ModMenuOpen, NeighbourUpdate, OnBlockExplodedEvent, OnBlockStateChangeEvent,
    OnDestroyedByPlayerEvent, PlayerContainerOpenEvent, PlayerRef, ServerHost, SoundSource,
    StatType, Text, UseWithoutItemEvent,
};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};

pub const MENU_ID: &str = "easymagic:enchanting";
pub const REROLL_BUTTON_ID: i32 = 4;

fn find_menu_key(host: &dyn Host) -> Option<MenuKey> {
    host.menu_keys()
        .into_iter()
        .find(|k| k.id(host).is_some_and(|n| n.as_str() == MENU_ID))
}

/// The player's current menu when it is the open window `window`.
fn find_menu(host: &dyn ServerHost, uuid: u128, window: i32) -> Option<MenuRef> {
    let menu = host.player(uuid)?.container_menu(host)?;
    (menu.container_id(host) == Some(window)).then_some(menu)
}

fn notify_comparator(host: &mut dyn ServerHost, level: LevelRef, pos: [i32; 3]) {
    if let Some(state) = level.block_state(&*host, pos) {
        let _ = level.update_neighbors(host, pos, state, NeighbourUpdate::OutputSignal);
    }
}

#[derive(Clone, Debug)]
struct ViewerEntry {
    window: i32,
    dimension: String,
    pos: [i32; 3],
    last_power: i32,
    still_valid: bool,
}

/// An enchanting menu requested by a table click, completed once the engine installs it.
struct PendingOpen {
    dimension: String,
    pos: [i32; 3],
    stacks: [Option<ItemStack>; 3],
    scaled_power: i32,
    clues: CluesPayload,
}

static PENDING_OPENS: LazyLock<Mutex<HashMap<u128, PendingOpen>>> = LazyLock::new(Mutex::default);

type TableKey = (String, [i32; 3]);

static VIEWERS: LazyLock<Mutex<HashMap<u128, ViewerEntry>>> = LazyLock::new(Mutex::default);
/// Notification hints for tables touched by capability transactions, keyed by `(dim, pos)` with
/// the pre-mutation comparator signal baseline so aborted transactions produce no side effects.
static DIRTY_TABLES: LazyLock<Mutex<HashMap<TableKey, i32>>> = LazyLock::new(Mutex::default);
static PENDING_DROPS: LazyLock<Mutex<Vec<TableKey>>> = LazyLock::new(Mutex::default);

pub fn reset_server_state() {
    PENDING_OPENS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    VIEWERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    DIRTY_TABLES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    PENDING_DROPS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
}

pub fn mark_table_dirty(dimension: &str, pos: [i32; 3], prev_signal: i32) {
    DIRTY_TABLES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry((dimension.to_owned(), pos))
        .or_insert(prev_signal);
}

fn clear_table_dirty(dimension: &str, pos: [i32; 3]) {
    DIRTY_TABLES
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

/// Replaces the 3 owned container slots of an open `easymagic:enchanting` menu.
fn set_menu_owned_slots(
    host: &mut dyn ServerHost,
    menu: MenuRef,
    desired: &[Option<ItemStack>; 3],
) {
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

fn read_menu_owned_slots(host: &dyn Host, menu: MenuRef) -> [Option<ItemStack>; 3] {
    [
        menu.slot(host, 0).flatten(),
        menu.slot(host, 1).flatten(),
        menu.slot(host, 2).flatten(),
    ]
}

#[allow(clippy::too_many_arguments)]
fn send_clues_and_data(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    menu: MenuRef,
    window: i32,
    level: LevelRef,
    pos: [i32; 3],
    slot0: Option<&ItemStack>,
    seed: i32,
    cfg: &ServerConfig,
) -> i32 {
    let scaled_power = compute_scaled_enchanting_power(&*host, level, pos, cfg);
    let (costs, rows) = compute_offers(&*host, slot0, seed, scaled_power, cfg.enchantment_hint);
    let _ = menu.set_data_slot(host, 0, costs[0]);
    let _ = menu.set_data_slot(host, 1, costs[1]);
    let _ = menu.set_data_slot(host, 2, costs[2]);
    let _ = menu.set_data_slot(host, 3, seed);
    if let Some(ch) = channels() {
        let payload = CluesPayload {
            window,
            costs,
            seed,
            rows,
        };
        let _ = ch.clues.send(host, player, &payload);
    }
    scaled_power
}

fn sync_viewers_for_table(
    host: &mut dyn ServerHost,
    dimension: &str,
    pos: [i32; 3],
    stacks: &[Option<ItemStack>; 3],
    exclude_player: Option<u128>,
) {
    let Some(level) = host.level(dimension) else {
        return;
    };
    let cfg = server_config();
    for (viewer_uuid, window) in viewers_at(dimension, pos) {
        let Some(player) = host.player(viewer_uuid) else {
            continue;
        };
        let Some(menu) = find_menu(&*host, viewer_uuid, window) else {
            continue;
        };
        if exclude_player != Some(viewer_uuid) {
            set_menu_owned_slots(host, menu, stacks);
        }
        let seed = get_or_init_player_seed(host, viewer_uuid);
        let power = send_clues_and_data(
            host,
            player,
            menu,
            window,
            level,
            pos,
            stacks[0].as_ref(),
            seed,
            &cfg,
        );
        if let Some(entry) = VIEWERS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(&viewer_uuid)
        {
            entry.last_power = power;
        }
    }
}

/// Fills a freshly installed `easymagic:enchanting` menu with the table's contents and
/// registers its viewer (`player_container_open`).
pub fn finish_menu_open(host: &mut dyn ServerHost, event: &PlayerContainerOpenEvent<'_>) {
    if event.menu_type != MENU_ID {
        return;
    }
    let Some(pending) = PENDING_OPENS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&event.player)
    else {
        return;
    };
    let (Some(menu), Some(player)) = (event.menu, host.player(event.player)) else {
        return;
    };
    let Some(window) = menu.container_id(&*host) else {
        return;
    };
    set_menu_owned_slots(host, menu, &pending.stacks);
    VIEWERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(
            event.player,
            ViewerEntry {
                window,
                dimension: pending.dimension,
                pos: pending.pos,
                last_power: pending.scaled_power,
                still_valid: true,
            },
        );
    if let Some(ch) = channels() {
        let mut payload = pending.clues;
        payload.window = window;
        let _ = ch.clues.send(host, player, &payload);
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

/// `BlockPart` attached to `minecraft:enchanting_table`.
pub struct EnchantingTablePart;

#[mod_api::sunrise_part]
impl BlockPart for EnchantingTablePart {
    fn use_without_item(
        &self,
        side: HostSide<'_>,
        event: &mut UseWithoutItemEvent<'_>,
    ) -> Option<InteractionResult> {
        // The menu opens on the server; a client defers to the template's prediction.
        let HostSide::Server(host) = side else {
            return None;
        };
        let cfg = server_config();
        if !cfg.convert_vanilla_enchanting_table_when_interacting {
            return None;
        }
        if is_unaltered_enchanting_table(&*host, event.state.block()) {
            return None;
        }
        let mkey = find_menu_key(&*host)?;
        let player_uuid = event.click.player;
        let pos = event.click.pos;
        let dim = event.click.dimension.key().id().to_owned();
        let player = host.player(player_uuid)?;
        let level = host.level(&dim)?;

        let contents = read_table_contents(host, &dim, pos);
        let stacks = contents.to_stacks(&*host);
        let seed = get_or_init_player_seed(host, player_uuid);
        let scaled_power = compute_scaled_enchanting_power(&*host, level, pos, &cfg);
        let (costs, rows) = compute_offers(
            &*host,
            stacks[0].as_ref(),
            seed,
            scaled_power,
            cfg.enchantment_hint,
        );

        let title = host
            .block_entity(level, pos)
            .and_then(|be| be.name(&*host))
            .filter(|name| !name.is_empty())
            .map(Text::literal)
            .unwrap_or_else(|| Text::translate("container.enchant", Vec::new()));

        let initial_clues = CluesPayload {
            window: 0,
            costs,
            seed,
            rows,
        };
        let mut writer = Writer::new();
        writer.put(&initial_clues);

        let mut open = ModMenuOpen::new(mkey, title);
        open.extra = writer.into_bytes();
        open.data = vec![costs[0], costs[1], costs[2], seed];
        open.pos = Some(pos);
        open.replace = MenuReplace::CloseFirst;

        // The open can be staged until commit, so the live menu is filled in
        // `finish_menu_open` when the engine announces it.
        PENDING_OPENS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                player_uuid,
                PendingOpen {
                    dimension: dim,
                    pos,
                    stacks,
                    scaled_power,
                    clues: initial_clues,
                },
            );
        if !matches!(player.open_mod_menu(host, open), Ok(Some(_))) {
            PENDING_OPENS
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&player_uuid);
            return None;
        }

        Some(InteractionResult::Success)
    }

    fn has_analog_output(&self, _level: &dyn LevelHost, event: &mut HasAnalogOutputEvent) {
        event.output = true;
    }

    fn analog_output(&self, level: &mut dyn LevelRandomHost, event: &mut AnalogOutputEvent) {
        let pos = [event.pos.x, event.pos.y, event.pos.z];
        let contents = read_table_contents_level(level, pos);
        event.output = comparator_signal(&contents);
    }

    fn on_destroyed_by_player(
        &self,
        host: &mut dyn ServerHost,
        event: &mut OnDestroyedByPlayerEvent<'_>,
    ) -> Option<bool> {
        let dim = host.dimension().to_owned();
        invalidate_viewers_at(&dim, event.pos);
        clear_table_dirty(&dim, event.pos);
        if let Some(level) = host.level(&dim) {
            drop_and_clear_table_contents(host, level, &dim, event.pos);
        }
        None
    }

    fn on_block_exploded(&self, host: &mut dyn ServerHost, event: &mut OnBlockExplodedEvent<'_>) {
        let pos = [event.pos.x, event.pos.y, event.pos.z];
        let dim = host.dimension().to_owned();
        invalidate_viewers_at(&dim, pos);
        clear_table_dirty(&dim, pos);
        if let Some(level) = host.level(&dim) {
            drop_and_clear_table_contents(host, level, &dim, pos);
        }
    }

    fn on_block_state_change(&self, level: LevelSide, event: &mut OnBlockStateChangeEvent) {
        if !matches!(level, LevelSide::Server(_)) {
            return;
        }
        if event.new.block() == event.old.block() {
            return;
        }
        let pos = [event.pos.x, event.pos.y, event.pos.z];
        let dim = event.dimension.key().id().to_owned();
        invalidate_viewers_at(&dim, pos);
        clear_table_dirty(&dim, pos);
        let mut drops = PENDING_DROPS.lock().unwrap_or_else(PoisonError::into_inner);
        let key = (dim, pos);
        if !drops.contains(&key) {
            drops.push(key);
        }
    }

    fn animate_tick(&self, host: &mut dyn mod_api::ClientHost, event: &mut AnimateTickEvent<'_>) {
        crate::client::animate_enchanting_table(host, event);
    }
}

/// `MenuPart` attached to `easymagic:enchanting`.
pub struct EnchantingMenuPart;

#[mod_api::sunrise_part]
impl MenuPart for EnchantingMenuPart {
    fn menu_removed(&self, _host: &mut dyn ServerHost, event: &mut MenuRemovedEvent<'_>) {
        let mut map = VIEWERS.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(entry) = map.get(&event.uuid)
            && entry.window == event.window
        {
            map.remove(&event.uuid);
        }
    }

    fn menu_clicked(&self, host: &mut dyn ServerHost, event: &mut MenuClickedEvent<'_>) -> Flow {
        // Shift-click (ClickType::QUICK_MOVE == 1) from player inventory: route catalysts directly
        // to slots 1/2 instead of letting slot 0 capture a single catalyst item.
        if event.mode != 1 || event.slot < 3 {
            return Flow::Continue;
        }
        let Some(menu) = find_menu(&*host, event.uuid, event.window) else {
            return Flow::Continue;
        };
        let src_slot = event.slot as usize;
        let Some(stack) = menu.slot(&*host, src_slot).flatten() else {
            return Flow::Continue;
        };
        let cfg = server_config();
        let target_slot = if is_enchanting_catalyst(&*host, stack.item()) {
            Some(1usize)
        } else if cfg.dedicated_reroll_catalyst_active() && is_reroll_catalyst(&*host, stack.item())
        {
            Some(2usize)
        } else {
            None
        };
        let Some(dst_slot) = target_slot else {
            return Flow::Continue;
        };

        let resource = ItemResource::of(&stack);
        let amount = stack.count();
        let moved = host
            .transaction(|host, tx| {
                let src = ItemPlace::MenuSlot {
                    menu,
                    slot: src_slot,
                };
                let dst = ItemPlace::MenuSlot {
                    menu,
                    slot: dst_slot,
                };
                let taken = src.extract(host, tx, &resource, amount).unwrap_or(0);
                if taken == 0 {
                    return Ok::<u32, ()>(0);
                }
                let put = dst.insert(host, tx, &resource, taken).unwrap_or(0);
                if put < taken {
                    let _ = src.insert(host, tx, &resource, taken - put);
                }
                Ok(put)
            })
            .unwrap_or(0);

        if moved > 0 {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    fn menu_slot_rules(&self, host: &dyn Host, event: &mut MenuSlotRulesEvent<'_>) {
        if event.container != MenuContainer::Main {
            return;
        }
        // Runs on both sides: use the synced server config so client slot prediction matches
        // dedicated_reroll_catalyst on remote servers.
        let cfg = synced_server_config();
        match event.slot {
            0 => {
                event.max_stack = 1;
            }
            1 => {
                event.may_place = is_enchanting_catalyst(host, event.stack.item());
            }
            2 => {
                event.may_place = cfg.dedicated_reroll_catalyst_active()
                    && is_reroll_catalyst(host, event.stack.item());
            }
            _ => {}
        }
    }

    fn menu_button_valid(&self, _host: &dyn Host, event: &mut MenuButtonValidEvent<'_>) -> Flow {
        match event.id {
            0 | 1 | 2 | REROLL_BUTTON_ID => Flow::Cancel,
            _ => Flow::Continue,
        }
    }

    fn menu_button(&self, event: &mut MenuButtonEvent<'_>) {
        let player_uuid = event.context.player;
        let button_id = event.context.id;
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
        let Some(menu) = find_menu(&*host, player_uuid, viewer.window) else {
            return;
        };

        let cfg = server_config();
        let mut stacks = read_menu_owned_slots(&*host, menu);
        let creative = player.game_mode(&*host) == Some(GameMode::Creative);

        if button_id == REROLL_BUTTON_ID {
            if !cfg.reroll_enchantments {
                return;
            }
            let Some(item_stack) = stacks[0].as_ref() else {
                return;
            };
            if !is_stack_enchantable(&*host, item_stack) {
                return;
            }
            let catalyst_slot = if cfg.dedicated_reroll_catalyst_active() {
                2
            } else {
                1
            };
            let catalyst_cost = cfg.reroll_catalyst_cost.max(0) as u32;
            let xp_cost = cfg.reroll_experience_points_cost.max(0);
            let catalyst_have = stacks[catalyst_slot].as_ref().map_or(0, ItemStack::count);
            let (exp_level, exp_progress, exp_total) = player
                .experience(&*host)
                .map_or((0, 0.0, 0), |e| (e.level, e.progress, e.total));
            let enough_xp = if cfg.rerolling_takes_enchantment_levels {
                exp_level >= xp_cost
            } else {
                total_experience(exp_level, exp_progress) >= i128::from(xp_cost)
            };

            if !creative && (catalyst_have < catalyst_cost || !enough_xp) {
                return;
            }

            let prev_seed = get_or_init_player_seed(host, player_uuid);
            let _new_seed = advance_player_seed(host, player_uuid, prev_seed);

            if !creative {
                if catalyst_cost > 0
                    && let Some(cat_stack) = stacks[catalyst_slot].take()
                {
                    stacks[catalyst_slot] = cat_stack.shrink(catalyst_cost);
                }
                if xp_cost > 0 {
                    if cfg.rerolling_takes_enchantment_levels {
                        let _ = player.give_levels(host, -xp_cost);
                    } else {
                        let (new_lvl, new_pts, new_total) =
                            subtract_experience_points(exp_level, exp_progress, exp_total, xp_cost);
                        let _ = player.set_experience_levels(host, new_lvl);
                        let _ = player.set_experience_points(host, new_pts);
                        let _ = player.set_total_experience(host, new_total);
                    }
                }
            }

            set_menu_owned_slots(host, menu, &stacks);
            let updated = TableContents::from_stacks(&*host, &stacks);
            write_table_contents(host, &viewer.dimension, viewer.pos, &updated);
            clear_table_dirty(&viewer.dimension, viewer.pos);
            if catalyst_slot == 1 {
                notify_comparator(host, level, viewer.pos);
            }
            sync_viewers_for_table(
                host,
                &viewer.dimension,
                viewer.pos,
                &stacks,
                Some(player_uuid),
            );
            return;
        }

        if (0..=2).contains(&button_id) {
            let slot_idx = button_id as usize;
            let Some(item_stack) = stacks[0].clone() else {
                return;
            };
            if !is_stack_enchantable(&*host, &item_stack) {
                return;
            }
            let seed = get_or_init_player_seed(host, player_uuid);
            let scaled_power = compute_scaled_enchanting_power(&*host, level, viewer.pos, &cfg);
            let (costs, _) = compute_offers(
                &*host,
                Some(&item_stack),
                seed,
                scaled_power,
                cfg.enchantment_hint,
            );
            let required_level = costs[slot_idx];
            if required_level <= 0 {
                return;
            }
            let cat = catalog(&*host);
            let chosen =
                roll_row_enchantments(&*host, &cat, &item_stack, seed, slot_idx, required_level);
            if chosen.is_empty() {
                return;
            }

            let cost_count = (slot_idx + 1) as u32;
            let lapis_have = stacks[1].as_ref().map_or(0, ItemStack::count);
            let player_level = player.experience(&*host).map_or(0, |e| e.level);
            if !creative && (lapis_have < cost_count || player_level < required_level) {
                return;
            }

            let Some(enchanted) = apply_enchantments_to_stack(&*host, &item_stack, &chosen) else {
                return;
            };

            stacks[0] = Some(enchanted);
            if !creative {
                let _ = player.give_levels(host, -(cost_count as i32));
                if let Some(lapis_stack) = stacks[1].take() {
                    stacks[1] = lapis_stack.shrink(cost_count);
                }
            }

            let _next_seed = advance_player_seed(host, player_uuid, seed);
            let _ = player.award_stat(host, StatType::Custom, "minecraft:enchant_item", 1);
            let pitch = 0.9 + ((seed.unsigned_abs() % 20) as f32) * 0.01;
            let _ = level.play_sound(
                host,
                "minecraft:block.enchantment_table.use",
                [
                    f64::from(viewer.pos[0]) + 0.5,
                    f64::from(viewer.pos[1]) + 0.5,
                    f64::from(viewer.pos[2]) + 0.5,
                ],
                SoundSource::Blocks,
                1.0,
                pitch,
                None,
            );

            set_menu_owned_slots(host, menu, &stacks);
            let updated = TableContents::from_stacks(&*host, &stacks);
            write_table_contents(host, &viewer.dimension, viewer.pos, &updated);
            clear_table_dirty(&viewer.dimension, viewer.pos);
            notify_comparator(host, level, viewer.pos);
            sync_viewers_for_table(
                host,
                &viewer.dimension,
                viewer.pos,
                &stacks,
                Some(player_uuid),
            );
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
        let stacks = [
            event.context.slot(MenuContainer::Main, 0),
            event.context.slot(MenuContainer::Main, 1),
            event.context.slot(MenuContainer::Main, 2),
        ];
        let contents = TableContents::from_stacks(&*host, &stacks);
        write_table_contents(host, &viewer.dimension, viewer.pos, &contents);
        clear_table_dirty(&viewer.dimension, viewer.pos);

        let slot1_changed = event
            .changed
            .iter()
            .any(|c| c.container == MenuContainer::Main && c.slot == 1);
        if slot1_changed && let Some(level) = host.level(&viewer.dimension) {
            notify_comparator(host, level, viewer.pos);
        }

        sync_viewers_for_table(
            host,
            &viewer.dimension,
            viewer.pos,
            &stacks,
            Some(player_uuid),
        );
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

    // 1. Drain any pending drops for this dimension (e.g. /setblock replacement)
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
        let still_table = level
            .block_state(&*host, pos)
            .and_then(|s| s.block().id(&*host))
            == Some("minecraft:enchanting_table");
        if !still_table {
            drop_and_clear_table_contents(host, level, &active_dim, pos);
        }
    }

    // 2. Drain hopper-dirtied tables in this dimension (post-commit) and publish comparator
    //    updates and viewer syncs only where the committed attachment state actually changed.
    let dirty: Vec<([i32; 3], i32)> = {
        let mut guard = DIRTY_TABLES.lock().unwrap_or_else(PoisonError::into_inner);
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
        let still_table = level
            .block_state(&*host, pos)
            .and_then(|s| s.block().id(&*host))
            == Some("minecraft:enchanting_table");
        if !still_table {
            continue;
        }
        let contents = read_table_contents(host, &active_dim, pos);
        if comparator_signal(&contents) != prev_signal {
            notify_comparator(host, level, pos);
        }
        let table_viewers = viewers_at(&active_dim, pos);
        if !table_viewers.is_empty() {
            let needs_sync = table_viewers.iter().any(|&(uuid, window)| {
                find_menu(&*host, uuid, window).is_some_and(|menu| {
                    let cur = read_menu_owned_slots(&*host, menu);
                    TableContents::from_stacks(&*host, &cur) != contents
                })
            });
            if needs_sync {
                let stacks = contents.to_stacks(&*host);
                sync_viewers_for_table(host, &active_dim, pos, &stacks, None);
            }
        }
    }

    // 3. For active viewers in this dimension (empty most ticks), detect broken tables or bookshelf changes
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

    let cfg = server_config();
    for (uuid, entry) in active {
        if !entry.still_valid {
            continue;
        }
        let still_table = level
            .block_state(&*host, entry.pos)
            .and_then(|s| s.block().id(&*host))
            == Some("minecraft:enchanting_table");
        if !still_table {
            invalidate_viewers_at(&entry.dimension, entry.pos);
            continue;
        }
        let new_power = compute_scaled_enchanting_power(&*host, level, entry.pos, &cfg);
        if new_power != entry.last_power
            && let (Some(player), Some(menu)) =
                (host.player(uuid), find_menu(&*host, uuid, entry.window))
        {
            let stacks = read_menu_owned_slots(&*host, menu);
            let seed = get_or_init_player_seed(host, uuid);
            let power = send_clues_and_data(
                host,
                player,
                menu,
                entry.window,
                level,
                entry.pos,
                stacks[0].as_ref(),
                seed,
                &cfg,
            );
            if let Some(v) = VIEWERS
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get_mut(&uuid)
            {
                v.last_power = power;
            }
        }
    }
}

/// Server handler for `easymagic:shelf_query`: returns the book stack stored in a chiseled
/// bookshelf slot so the client can display its full tooltip on the HUD.
pub fn handle_shelf_query(
    query: ShelfQueryPayload,
    ctx: &mut dyn IPayloadContext,
) -> Result<(), String> {
    if query.slot >= 6 {
        return Ok(());
    }
    let Ok(player) = ctx.player() else {
        return Ok(());
    };
    let Ok(host) = ctx.server_host() else {
        return Ok(());
    };
    let Some(level) = player.level(&*host) else {
        return Ok(());
    };

    // Distance guard: only answer queries within normal reach (<= 16 blocks).
    let Some(ppos) = player.entity().position(&*host) else {
        return Ok(());
    };
    let dx = ppos[0] - (f64::from(query.pos[0]) + 0.5);
    let dy = ppos[1] - (f64::from(query.pos[1]) + 0.5);
    let dz = ppos[2] - (f64::from(query.pos[2]) + 0.5);
    if dx * dx + dy * dy + dz * dz > 256.0 {
        return Ok(());
    }

    let is_shelf = level
        .block_state(&*host, query.pos)
        .and_then(|s| s.block().id(&*host))
        == Some("minecraft:chiseled_bookshelf");
    let stack = if is_shelf {
        level
            .capability(host, &ITEMS, query.pos, None)
            .and_then(|cap| {
                let slot = query.slot as usize;
                let res = cap.resource(host, slot)?;
                let amt = cap.amount(host, slot).max(1);
                res.to_stack(amt)
            })
    } else {
        None
    };

    let reply = ShelfItemPayload {
        pos: query.pos,
        slot: query.slot,
        stack,
    };
    if let Some(ch) = channels() {
        let _ = ch.shelf_item.reply(ctx, &reply);
    }
    Ok(())
}
