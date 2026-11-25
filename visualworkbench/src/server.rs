use crate::config::server_config;
use crate::storage::{
    GRID, TableContents, drop_and_clear_table_contents, read_table_contents,
    read_table_contents_level, write_table_contents,
};
use mod_api::{
    AnalogOutputEvent, BlockPart, Flow, HasAnalogOutputEvent, Host, HostTransaction, Identifier,
    InteractionResult, ItemPlace, ItemResource, ItemStack, LevelHost, LevelRandomHost, LevelRef,
    LevelSide, LoopContext, MenuClickedEvent, MenuClosedEvent, MenuContainer, MenuKey, MenuPart,
    MenuRef, MenuReplace, MenuSlotChangedEvent, MenuSlotRulesEvent, MenuSlotTakeEvent,
    MenuSlotWrite, MenuStillValidEvent, ModMenuOpen, NeighbourUpdate, OnBlockExplodedEvent,
    OnBlockStateChangeEvent, OnDestroyedByPlayerEvent, RecipeInput, RecipeMatch, ServerHost,
    StatType, Text, TransactionContext, UseWithoutItemEvent,
};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};

pub const MENU_ID: &str = "visualworkbench:crafting";
const TABLE_BLOCK: &str = "minecraft:crafting_table";
const UNALTERED_TAG: &str = "visualworkbench:unaltered_workbenches";
/// The menu's result slot; the grid is slots 1 to 9, as in vanilla's `CraftingMenu`.
const RESULT_SLOT: usize = 0;
/// A stack never holds more than 99 (`DataComponents.MAX_STACK_SIZE`'s ceiling), so a quick craft
/// cannot go on longer than that.
const MAX_QUICK_CRAFTS: usize = 99;
/// Vanilla's `Inventory.INVENTORY_SIZE` without armour and off hand: what an item can be added to.
const INVENTORY_SLOTS: i32 = 36;

#[derive(Clone, Debug)]
struct ViewerEntry {
    window: i32,
    dimension: String,
    pos: [i32; 3],
    still_valid: bool,
}

type TableKey = (String, [i32; 3]);

static VIEWERS: LazyLock<Mutex<HashMap<u128, ViewerEntry>>> = LazyLock::new(Mutex::default);
/// Tables touched by capability transactions, with the comparator signal from before.
static DIRTY_TABLES: LazyLock<Mutex<HashMap<TableKey, i32>>> = LazyLock::new(Mutex::default);
static PENDING_DROPS: LazyLock<Mutex<Vec<TableKey>>> = LazyLock::new(Mutex::default);

pub fn reset_server_state() {
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

fn find_menu_key(host: &dyn Host) -> Option<MenuKey> {
    host.menu_keys()
        .into_iter()
        .find(|k| k.id(host).is_some_and(|n| n.as_str() == MENU_ID))
}

fn is_crafting_table(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> bool {
    level
        .block_state(host, pos)
        .and_then(|s| s.block().id(host))
        == Some(TABLE_BLOCK)
}

fn notify_comparator(host: &mut dyn ServerHost, level: LevelRef, pos: [i32; 3]) {
    if let Some(state) = level.block_state(&*host, pos) {
        let _ = level.update_neighbors(host, pos, state, NeighbourUpdate::OutputSignal);
    }
}

fn read_menu_grid(host: &dyn Host, menu: MenuRef) -> [Option<ItemStack>; GRID] {
    std::array::from_fn(|i| menu.slot(host, i + 1).flatten())
}

fn same_stack(a: Option<&ItemStack>, b: Option<&ItemStack>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.count() == b.count() && ItemResource::of(a) == ItemResource::of(b),
        _ => false,
    }
}

/// Puts `desired` in the grid cells of an open menu, writing only the cells that differ.
fn set_menu_grid(host: &mut dyn ServerHost, menu: MenuRef, desired: &[Option<ItemStack>; GRID]) {
    let current = read_menu_grid(&*host, menu);
    let _ = host.transaction(|host, tx| {
        for (i, target) in desired.iter().enumerate() {
            if same_stack(current[i].as_ref(), target.as_ref()) {
                continue;
            }
            let place = ItemPlace::MenuSlot { menu, slot: i + 1 };
            if let Some(res) = place.resource(host) {
                let amount = place.amount(host);
                if amount > 0 {
                    let _ = place.extract(host, tx, &res, amount);
                }
            }
            if let Some(stack) = target {
                let _ = place.insert(host, tx, &ItemResource::of(stack), stack.count());
            }
        }
        Ok::<(), ()>(())
    });
}

/// The recipe the grid makes, matched by the game's own crafting matcher.
fn match_recipe(
    host: &dyn Host,
    level: LevelRef,
    grid: &[Option<ItemStack>; GRID],
) -> Option<RecipeMatch> {
    if grid.iter().all(Option::is_none) {
        return None;
    }
    let kind = Identifier::parse("minecraft:crafting").ok()?;
    let input = RecipeInput::Crafting {
        width: 3,
        height: 3,
        slots: grid.to_vec(),
    };
    level.recipe_for(host, &kind, &input).ok().flatten()
}

/// `ResultSlot.onTake` without the cursor: uses one of each ingredient and puts what a recipe
/// leaves behind (a bucket, a bottle) into the cell, or else into the player's inventory.
fn consume_craft(
    host: &mut dyn ServerHost,
    tx: &TransactionContext,
    menu: MenuRef,
    player: u128,
    grid: &[Option<ItemStack>; GRID],
    made: &RecipeMatch,
) -> Result<(), ()> {
    for (i, cell) in grid.iter().enumerate() {
        let Some(stack) = cell else { continue };
        let place = ItemPlace::MenuSlot { menu, slot: i + 1 };
        let taken = place
            .extract(host, tx, &ItemResource::of(stack), 1)
            .map_err(|_| ())?;
        if taken != 1 {
            return Err(());
        }
        let Some(Some(left)) = made.remainders.get(i) else {
            continue;
        };
        let res = ItemResource::of(left);
        let put = place.insert(host, tx, &res, left.count()).map_err(|_| ())?;
        if put < left.count() {
            ItemPlace::PlayerSlot { player, slot: 0 }
                .insert(host, tx, &res, left.count() - put)
                .map_err(|_| ())?;
        }
    }
    Ok(())
}

/// The crafting statistic and the recipe unlock a taken result earns (`ResultSlot.onTake`).
fn award_craft(host: &mut dyn ServerHost, player: u128, made: &RecipeMatch) {
    let Some(player) = host.player(player) else {
        return;
    };
    let id = made.output.item().id(&*host).map(str::to_owned);
    if let Some(id) = id {
        let _ = player.award_stat(host, StatType::Crafted, &id, made.output.count() as i32);
    }
    let recipe = made.recipe.to_string();
    let _ = player.award_recipes(host, &[recipe.as_str()]);
}

/// How many of `kind` fit an inventory's cells (`Inventory.add` without creative or loot rules).
pub fn room_for<K: PartialEq>(cells: &[Option<(K, u32)>], kind: &K, max_stack: u32) -> u32 {
    cells
        .iter()
        .map(|cell| match cell {
            None => max_stack,
            Some((held, count)) if held == kind => max_stack.saturating_sub(*count),
            Some(_) => 0,
        })
        .sum()
}

fn inventory_room(host: &mut dyn ServerHost, player: u128, res: &ItemResource) -> u32 {
    let max = host.max_stack_size(res.item()).unwrap_or(64);
    let cells: Vec<Option<(ItemResource, u32)>> = (0..INVENTORY_SLOTS)
        .map(|slot| {
            let place = ItemPlace::PlayerSlot { player, slot };
            place.resource(host).map(|held| (held, place.amount(host)))
        })
        .collect();
    room_for(&cells, res, max)
}

/// Shift-clicking the result: craft again and again into the inventory while the same item comes
/// out and fits (`CraftingMenu.quickMoveStack`'s loop).
fn quick_craft(host: &mut dyn ServerHost, uuid: u128, window: i32) {
    let Some(viewer) = viewer_of(uuid) else {
        return;
    };
    let (Some(menu), Some(level)) = (host.menu(uuid, window), host.level(&viewer.dimension)) else {
        return;
    };
    let mut first: Option<ItemResource> = None;
    for _ in 0..MAX_QUICK_CRAFTS {
        let grid = read_menu_grid(&*host, menu);
        let Some(made) = match_recipe(&*host, level, &grid) else {
            return;
        };
        let res = ItemResource::of(&made.output);
        if first.as_ref().is_some_and(|first| *first != res) {
            return;
        }
        let count = made.output.count();
        if inventory_room(host, uuid, &res) < count {
            return;
        }
        let crafted = host.transaction(|host, tx| {
            let put = ItemPlace::PlayerSlot {
                player: uuid,
                slot: 0,
            }
            .insert(host, tx, &res, count)
            .map_err(|_| ())?;
            if put < count {
                return Err(());
            }
            consume_craft(host, tx, menu, uuid, &grid, &made)
        });
        if crafted.is_err() {
            return;
        }
        award_craft(host, uuid, &made);
        first = Some(res);
    }
}

fn sync_viewers_for_table(
    host: &mut dyn ServerHost,
    dimension: &str,
    pos: [i32; 3],
    stacks: &[Option<ItemStack>; GRID],
    exclude_player: Option<u128>,
) {
    for (viewer_uuid, window) in viewers_at(dimension, pos) {
        if exclude_player == Some(viewer_uuid) {
            continue;
        }
        if let Some(menu) = host.menu(viewer_uuid, window) {
            set_menu_grid(host, menu, stacks);
        }
    }
}

/// `BlockPart` attached to `minecraft:crafting_table`.
pub struct CraftingTablePart;

#[mod_api::sunrise_part]
impl BlockPart for CraftingTablePart {
    fn use_without_item(
        &self,
        host: &mut dyn ServerHost,
        event: &mut UseWithoutItemEvent<'_>,
    ) -> Option<InteractionResult> {
        if !server_config().convert_vanilla_workbench_when_interacting {
            return None;
        }
        if host
            .block_in_tag(event.state.block(), UNALTERED_TAG)
            .unwrap_or(false)
        {
            return None;
        }
        let menu_key = find_menu_key(&*host)?;
        let uuid = event.click.player;
        let pos = event.click.pos;
        let dim = event.click.dimension.key().id().to_owned();
        let player = host.player(uuid)?;

        let stacks = read_table_contents(host, &dim, pos).to_stacks(&*host);

        let mut open =
            ModMenuOpen::new(menu_key, Text::translate("container.crafting", Vec::new()));
        open.pos = Some(pos);
        open.replace = MenuReplace::CloseFirst;
        let Ok(Some(window)) = player.open_mod_menu(host, open) else {
            return None;
        };

        VIEWERS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                uuid,
                ViewerEntry {
                    window,
                    dimension: dim,
                    pos,
                    still_valid: true,
                },
            );
        if let Some(menu) = host.menu(uuid, window) {
            set_menu_grid(host, menu, &stacks);
        }
        let _ = player.award_stat(
            host,
            StatType::Custom,
            "minecraft:interact_with_crafting_table",
            1,
        );
        Some(InteractionResult::Success)
    }

    fn has_analog_output(&self, _level: &dyn LevelHost, event: &mut HasAnalogOutputEvent) {
        event.output = true;
    }

    fn analog_output(&self, level: &mut dyn LevelRandomHost, event: &mut AnalogOutputEvent) {
        let pos = [event.pos.x, event.pos.y, event.pos.z];
        event.output = read_table_contents_level(level, pos).filled_cells();
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
        if !matches!(level, LevelSide::Server(_)) || event.new.block() == event.old.block() {
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
}

/// `MenuPart` attached to `visualworkbench:crafting`.
pub struct CraftingMenuPart;

#[mod_api::sunrise_part]
impl MenuPart for CraftingMenuPart {
    fn menu_clicked(&self, host: &mut dyn ServerHost, event: &mut MenuClickedEvent<'_>) -> Flow {
        // A shift-click (ClickType::QUICK_MOVE is 1) on the result crafts as many as fit.
        if event.mode != 1 || event.slot != RESULT_SLOT as i32 {
            return Flow::Continue;
        }
        quick_craft(host, event.uuid, event.window);
        Flow::Cancel
    }

    fn menu_slot_rules(&self, _host: &dyn Host, event: &mut MenuSlotRulesEvent<'_>) {
        // The result slot takes nothing from the player (`ResultSlot.mayPlace`).
        if event.container == MenuContainer::Main && event.slot == RESULT_SLOT {
            event.may_place = false;
        }
    }

    fn menu_slot_take(&self, event: &mut MenuSlotTakeEvent<'_>) {
        if event.slot != RESULT_SLOT {
            return;
        }
        let uuid = event.player;
        let _ = event.subject.transaction(|scope, tx| {
            let host = scope.host();
            let viewer = viewer_of(uuid).ok_or(())?;
            let menu = host
                .player(uuid)
                .and_then(|p| p.container_menu(&*host))
                .ok_or(())?;
            let level = host.level(&viewer.dimension).ok_or(())?;
            let grid = read_menu_grid(&*host, menu);
            let made = match_recipe(&*host, level, &grid).ok_or(())?;
            consume_craft(host, tx, menu, uuid, &grid, &made)?;
            award_craft(host, uuid, &made);
            Ok::<(), ()>(())
        });
    }

    fn menu_slot_changed(&self, host: &mut dyn ServerHost, event: &mut MenuSlotChangedEvent<'_>) {
        let Some(uuid) = event.context.player else {
            return;
        };
        let Some(viewer) = viewer_of(uuid) else {
            return;
        };
        if !viewer.still_valid {
            return;
        }
        let grid: [Option<ItemStack>; GRID] =
            std::array::from_fn(|i| event.context.slot(MenuContainer::Main, i + 1));

        // The result follows the grid.
        let result = host
            .level(&viewer.dimension)
            .and_then(|level| match_recipe(&*host, level, &grid))
            .map(|made| made.output);
        event
            .outputs
            .push(MenuSlotWrite::new(MenuContainer::Main, RESULT_SLOT, result));

        // The table keeps the grid.
        let previous = read_table_contents(host, &viewer.dimension, viewer.pos).filled_cells();
        let contents = TableContents::from_stacks(&*host, &grid);
        write_table_contents(host, &viewer.dimension, viewer.pos, &contents);
        clear_table_dirty(&viewer.dimension, viewer.pos);
        if contents.filled_cells() != previous
            && let Some(level) = host.level(&viewer.dimension)
        {
            notify_comparator(host, level, viewer.pos);
        }
        sync_viewers_for_table(host, &viewer.dimension, viewer.pos, &grid, Some(uuid));
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
        // `Container.stillValidBlockEntity`: the block interaction range (4.5) plus the 4.0 margin.
        if dx * dx + dy * dy + dz * dz > 72.25 {
            event.valid = false;
        }
    }
}

pub fn on_menu_closed(host: &mut dyn ServerHost, event: &mut MenuClosedEvent<'_>) {
    let removed = {
        let mut map = VIEWERS.lock().unwrap_or_else(PoisonError::into_inner);
        match map.get(&event.uuid) {
            Some(entry) if entry.window == event.window => map.remove(&event.uuid),
            _ => None,
        }
    };
    let Some(entry) = removed else {
        return;
    };
    // Keep what the grid held when the menu went; a table that was broken has dropped it already.
    if !entry.still_valid || event.owned_slots.len() < GRID + 1 {
        return;
    }
    let Some(level) = host.level(&entry.dimension) else {
        return;
    };
    if !is_crafting_table(&*host, level, entry.pos) {
        return;
    }
    let grid: [Option<ItemStack>; GRID] = std::array::from_fn(|i| event.owned_slots[i + 1].clone());
    let contents = TableContents::from_stacks(&*host, &grid);
    write_table_contents(host, &entry.dimension, entry.pos, &contents);
}

pub fn on_server_tick_post(host: &mut dyn ServerHost, level: LevelRef) {
    let active_dim = level
        .dimension()
        .id(&*host)
        .unwrap_or_else(|| host.dimension())
        .to_owned();

    // Contents of tables that were replaced (`/setblock`, pistons, fluids) drop here.
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
        if !is_crafting_table(&*host, level, pos) {
            drop_and_clear_table_contents(host, level, &active_dim, pos);
        }
    }

    // Tables a hopper touched, after the commit: comparator output and open menus follow.
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
        if !is_crafting_table(&*host, level, pos) {
            continue;
        }
        let contents = read_table_contents(host, &active_dim, pos);
        if contents.filled_cells() != prev_signal {
            notify_comparator(host, level, pos);
        }
        let stacks = contents.to_stacks(&*host);
        sync_viewers_for_table(host, &active_dim, pos, &stacks, None);
    }

    // Viewers of a table that is gone have their menu closed (`menu_still_valid`).
    let active: Vec<ViewerEntry> = VIEWERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .values()
        .filter(|e| e.dimension == active_dim && e.still_valid)
        .cloned()
        .collect();
    for entry in active {
        if !is_crafting_table(&*host, level, entry.pos) {
            invalidate_viewers_at(&entry.dimension, entry.pos);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_counts_empty_cells_and_matching_stacks() {
        let cells = vec![None, Some(("planks", 60)), Some(("stone", 10)), None];
        assert_eq!(room_for(&cells, &"planks", 64), 132);
        assert_eq!(room_for(&cells, &"stone", 16), 38);
    }

    #[test]
    fn a_full_inventory_has_no_room() {
        let cells = vec![Some(("stone", 64)), Some(("dirt", 1))];
        assert_eq!(room_for(&cells, &"stone", 64), 0);
    }
}
