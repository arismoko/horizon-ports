use crate::config::MOD_ID;
use mod_api::{
    AttachmentCodec, AttachmentError, AttachmentStore, AttachmentType, BlockSide, Compound, Holder,
    Host, ItemResource, ItemStack, JavaString, LevelRef, Overanswered, ResourceHandler, ServerHost,
    Tag, TransactionContext, TransactionError,
};

pub const ATTACH_TABLE_CONTENTS: &str = "visualworkbench:table_contents";

/// The nine crafting grid cells of a table.
pub const GRID: usize = 9;

/// Serialized contents of a crafting table's grid (`ItemStack::to_nbt` compounds).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableContents {
    pub slots: [Option<Compound>; GRID],
}

impl TableContents {
    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    /// How many grid cells hold something: the table's comparator signal.
    pub fn filled_cells(&self) -> i32 {
        self.slots.iter().filter(|slot| slot.is_some()).count() as i32
    }

    pub fn to_stacks(&self, host: &dyn Host) -> [Option<ItemStack>; GRID] {
        std::array::from_fn(|i| {
            self.slots[i]
                .as_ref()
                .and_then(|c| ItemStack::from_nbt(host, c))
        })
    }

    pub fn from_stacks(host: &dyn Host, stacks: &[Option<ItemStack>; GRID]) -> Self {
        Self {
            slots: std::array::from_fn(|i| stacks[i].as_ref().and_then(|s| s.to_nbt(host))),
        }
    }
}

fn slot_key(index: usize) -> JavaString {
    JavaString::from(format!("slot_{index}").as_str())
}

fn encode_contents(value: &TableContents) -> Result<Compound, AttachmentError> {
    let mut out = Compound::new();
    for (i, slot) in value.slots.iter().enumerate() {
        if let Some(comp) = slot {
            out.insert(slot_key(i), Tag::Compound(comp.clone()));
        }
    }
    Ok(out)
}

fn decode_contents(saved: &Compound) -> Result<TableContents, AttachmentError> {
    let mut slots: [Option<Compound>; GRID] = Default::default();
    for (i, slot) in slots.iter_mut().enumerate() {
        if let Some(Tag::Compound(comp)) = saved.get(&slot_key(i)) {
            *slot = Some(comp.clone());
        }
    }
    Ok(TableContents { slots })
}

pub const TABLE_CONTENTS: AttachmentType<TableContents> = AttachmentType {
    namespace: MOD_ID,
    name: "table_contents",
    default: TableContents::default,
    codec: Some(AttachmentCodec {
        encode: encode_contents,
        decode: decode_contents,
    }),
    copy_on_death: false,
    sync: false,
};

/// Reads stored table contents from the transactional block entity attachment.
pub fn read_table_contents(
    host: &mut dyn ServerHost,
    dimension: &str,
    pos: [i32; 3],
) -> TableContents {
    if host.dimension() != dimension {
        return TableContents::default();
    }
    TABLE_CONTENTS
        .get_existing(host, Holder::BlockEntity(pos))
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Reads stored table contents from a `LevelRandomHost` (used by `analog_output`).
pub fn read_table_contents_level<S: AttachmentStore + ?Sized>(
    store: &mut S,
    pos: [i32; 3],
) -> TableContents {
    TABLE_CONTENTS
        .get_existing(store, Holder::BlockEntity(pos))
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Writes stored table contents to the transactional block entity attachment.
pub fn write_table_contents(
    host: &mut dyn ServerHost,
    dimension: &str,
    pos: [i32; 3],
    contents: &TableContents,
) {
    if host.dimension() != dimension {
        return;
    }
    if contents.is_empty() {
        let _ = TABLE_CONTENTS.remove(host, Holder::BlockEntity(pos));
    } else {
        let _ = TABLE_CONTENTS.set(host, Holder::BlockEntity(pos), contents.clone());
    }
}

/// Clears stored contents and drops all items at `pos` in `level`.
pub fn drop_and_clear_table_contents(
    host: &mut dyn ServerHost,
    level: LevelRef,
    dimension: &str,
    pos: [i32; 3],
) {
    if host.dimension() != dimension {
        return;
    }
    let contents = TABLE_CONTENTS
        .get_existing(host, Holder::BlockEntity(pos))
        .ok()
        .flatten()
        .unwrap_or_default();
    let _ = TABLE_CONTENTS.remove(host, Holder::BlockEntity(pos));
    if contents.is_empty() {
        return;
    }
    for stack in contents.to_stacks(&*host).into_iter().flatten() {
        let _ = level.pop_resource(host, pos, stack);
    }
}

/// `CraftingTableBlockEntity.smallerStackExist` (itself `CrafterBlockEntity`'s): whether a cell
/// after `from` holds the same kind of item with fewer than `size` of them. `cells` lists each
/// cell's item and count in grid order.
pub fn smaller_stack_exists<K: PartialEq>(
    cells: &[Option<(K, u32)>],
    size: u32,
    kind: &K,
    from: Option<usize>,
) -> bool {
    let start = from.map_or(0, |slot| slot + 1);
    cells
        .iter()
        .skip(start)
        .flatten()
        .any(|(other, count)| *count < size && other == kind)
}

/// `CraftingTableBlockEntity.canPlaceItem`: hoppers (and the like) spread items over the grid
/// evenly, so a stack only goes into a cell when no later cell holds a smaller one of the kind.
pub fn can_place_in_cell<K: PartialEq>(
    cells: &[Option<(K, u32)>],
    slot: usize,
    kind: &K,
    max_stack: u32,
) -> bool {
    match cells.get(slot).and_then(Option::as_ref) {
        None => !smaller_stack_exists(cells, max_stack, kind, None),
        Some((held, count)) => !smaller_stack_exists(cells, *count, held, Some(slot)),
    }
}

/// Hopper and automation `ITEMS` capability for `minecraft:crafting_table`: all nine cells take
/// items by the spreading rule from any face, and nothing comes out (`canTakeItem` is false).
pub struct CraftingTableItemHandler {
    side: BlockSide,
}

impl CraftingTableItemHandler {
    pub fn new(side: BlockSide) -> Self {
        Self { side }
    }

    fn dimension_name(&self, host: &dyn ServerHost) -> String {
        self.side
            .level
            .dimension()
            .id(host)
            .map(str::to_owned)
            .unwrap_or_else(|| host.dimension().to_owned())
    }

    fn cells(&self, host: &mut dyn ServerHost) -> [Option<ItemStack>; GRID] {
        let dim = self.dimension_name(host);
        read_table_contents(host, &dim, self.side.pos).to_stacks(&*host)
    }
}

fn cell_view(stacks: &[Option<ItemStack>; GRID]) -> Vec<Option<(ItemResource, u32)>> {
    stacks
        .iter()
        .map(|s| s.as_ref().map(|s| (ItemResource::of(s), s.count())))
        .collect()
}

impl ResourceHandler<ItemResource> for CraftingTableItemHandler {
    fn size(&self, _host: &mut dyn ServerHost) -> usize {
        GRID
    }

    fn resource(&self, host: &mut dyn ServerHost, index: usize) -> Option<ItemResource> {
        self.cells(host).get(index)?.as_ref().map(ItemResource::of)
    }

    fn amount(&self, host: &mut dyn ServerHost, index: usize) -> u32 {
        self.cells(host)
            .get(index)
            .and_then(Option::as_ref)
            .map_or(0, ItemStack::count)
    }

    fn capacity(
        &self,
        host: &mut dyn ServerHost,
        _index: usize,
        resource: Option<&ItemResource>,
    ) -> u32 {
        resource.map_or(64, |res| host.max_stack_size(res.item()).unwrap_or(64))
    }

    fn is_valid(&self, host: &mut dyn ServerHost, index: usize, resource: &ItemResource) -> bool {
        if index >= GRID {
            return false;
        }
        let max = host.max_stack_size(resource.item()).unwrap_or(64);
        let cells = cell_view(&self.cells(host));
        can_place_in_cell(&cells, index, resource, max)
    }

    fn insert_at(
        &self,
        host: &mut dyn ServerHost,
        tx: &TransactionContext,
        index: usize,
        resource: &ItemResource,
        amount: u32,
    ) -> Result<u32, TransactionError<Overanswered>> {
        if !host.transaction_matches(Some(tx)) {
            return Err(TransactionError::InvalidContext);
        }
        if amount == 0 || index >= GRID {
            return Ok(0);
        }
        let dim = self.dimension_name(host);
        if host.dimension() != dim {
            return Ok(0);
        }
        let mut stacks = self.cells(host);
        let max = host.max_stack_size(resource.item()).unwrap_or(64);
        if !can_place_in_cell(&cell_view(&stacks), index, resource, max) {
            return Ok(0);
        }
        let prev_signal = TableContents::from_stacks(&*host, &stacks).filled_cells();
        let moved = match &mut stacks[index] {
            Some(existing) => {
                if ItemResource::of(existing) != *resource {
                    return Ok(0);
                }
                let to_add = amount.min(max.saturating_sub(existing.count()));
                if to_add == 0 || existing.grow(to_add).is_err() {
                    return Ok(0);
                }
                to_add
            }
            None => {
                let to_add = amount.min(max);
                let Some(new_stack) = resource.to_stack(to_add) else {
                    return Ok(0);
                };
                stacks[index] = Some(new_stack);
                to_add
            }
        };
        let contents = TableContents::from_stacks(&*host, &stacks);
        write_table_contents(host, &dim, self.side.pos, &contents);
        // Comparator and viewer sync run after the commit, in the level tick, so an aborted
        // transaction never publishes anything.
        crate::server::mark_table_dirty(&dim, self.side.pos, prev_signal);
        Ok(moved)
    }

    fn extract_at(
        &self,
        host: &mut dyn ServerHost,
        tx: &TransactionContext,
        _index: usize,
        _resource: &ItemResource,
        _amount: u32,
    ) -> Result<u32, TransactionError<Overanswered>> {
        if !host.transaction_matches(Some(tx)) {
            return Err(TransactionError::InvalidContext);
        }
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(counts: [u32; 4]) -> Vec<Option<(&'static str, u32)>> {
        counts
            .iter()
            .map(|&c| (c > 0).then_some(("planks", c)))
            .collect()
    }

    #[test]
    fn smaller_stack_must_be_after_the_cell_and_of_the_same_kind() {
        let grid = cells([5, 0, 2, 7]);
        assert!(smaller_stack_exists(&grid, 5, &"planks", Some(0)));
        assert!(!smaller_stack_exists(&grid, 5, &"planks", Some(2)));
        assert!(!smaller_stack_exists(&grid, 5, &"stone", None));
        assert!(smaller_stack_exists(&grid, 64, &"planks", None));
    }

    #[test]
    fn items_spread_over_the_grid_by_the_crafter_rule() {
        let grid = cells([3, 0, 2, 0]);
        // Cell 0 holds 3 and cell 2 holds fewer: hold back until cell 2 catches up.
        assert!(!can_place_in_cell(&grid, 0, &"planks", 64));
        assert!(can_place_in_cell(&grid, 2, &"planks", 64));
        // An empty cell takes a kind only when no smaller stack of it exists after it.
        assert!(!can_place_in_cell(&grid, 1, &"planks", 64));
        assert!(can_place_in_cell(&grid, 1, &"stone", 64));
        // A full-size stack has nothing smaller than itself after it.
        let full = cells([64, 0, 64, 0]);
        assert!(can_place_in_cell(&full, 0, &"planks", 64));
        assert!(can_place_in_cell(&full, 1, &"planks", 64));
    }

    #[test]
    fn comparator_counts_filled_cells() {
        let mut contents = TableContents::default();
        assert_eq!(contents.filled_cells(), 0);
        contents.slots[0] = Some(Compound::new());
        contents.slots[8] = Some(Compound::new());
        assert_eq!(contents.filled_cells(), 2);
    }

    #[test]
    fn contents_codec_round_trips() {
        let mut slot = Compound::new();
        slot.insert(
            JavaString::from("id"),
            Tag::String(JavaString::from("minecraft:stick")),
        );
        slot.insert(JavaString::from("count"), Tag::Int(4));
        let mut contents = TableContents::default();
        contents.slots[4] = Some(slot);
        let decoded = decode_contents(&encode_contents(&contents).unwrap()).unwrap();
        assert_eq!(decoded, contents);
    }
}
