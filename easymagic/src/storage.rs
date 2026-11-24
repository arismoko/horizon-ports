use crate::config::{MOD_ID, server_config};
use crate::enchant::{is_enchanting_catalyst, is_reroll_catalyst, is_stack_enchanted_or_book};
use mod_api::{
    AttachmentCodec, AttachmentError, AttachmentStore, AttachmentType, BlockSide, Compound,
    Direction, Holder, Host, ItemResource, ItemStack, JavaString, LevelRef, Overanswered,
    ResourceHandler, ServerHost, Tag, TransactionContext, TransactionError,
};

pub const ATTACH_TABLE_CONTENTS: &str = "easymagic:table_contents";
pub const ATTACH_PLAYER_SEED: &str = "easymagic:enchantment_seed";

/// Serialized contents of an enchanting table's 3 slots (`ItemStack::to_nbt` compounds).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableContents {
    pub slots: [Option<Compound>; 3],
}

impl TableContents {
    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    pub fn to_stacks(&self, host: &dyn Host) -> [Option<ItemStack>; 3] {
        [
            self.slots[0]
                .as_ref()
                .and_then(|c| ItemStack::from_nbt(host, c)),
            self.slots[1]
                .as_ref()
                .and_then(|c| ItemStack::from_nbt(host, c)),
            self.slots[2]
                .as_ref()
                .and_then(|c| ItemStack::from_nbt(host, c)),
        ]
    }

    pub fn from_stacks(host: &dyn Host, stacks: &[Option<ItemStack>; 3]) -> Self {
        Self {
            slots: [
                stacks[0].as_ref().and_then(|s| s.to_nbt(host)),
                stacks[1].as_ref().and_then(|s| s.to_nbt(host)),
                stacks[2].as_ref().and_then(|s| s.to_nbt(host)),
            ],
        }
    }

    pub fn slot_count_from_nbt(&self, slot: usize) -> i32 {
        let Some(compound) = self.slots.get(slot).and_then(Option::as_ref) else {
            return 0;
        };
        match compound.get(&JavaString::from("count")) {
            Some(Tag::Int(v)) => (*v).max(0),
            Some(Tag::Byte(v)) => i32::from(*v).max(0),
            Some(Tag::Short(v)) => i32::from(*v).max(0),
            _ => 1,
        }
    }
}

fn encode_contents(value: &TableContents) -> Result<Compound, AttachmentError> {
    let mut out = Compound::new();
    const KEYS: [&str; 3] = ["slot_0", "slot_1", "slot_2"];
    for (i, key) in KEYS.iter().enumerate() {
        if let Some(comp) = &value.slots[i] {
            out.insert(JavaString::from(*key), Tag::Compound(comp.clone()));
        }
    }
    Ok(out)
}

fn decode_contents(saved: &Compound) -> Result<TableContents, AttachmentError> {
    let mut slots: [Option<Compound>; 3] = [None, None, None];
    const KEYS: [&str; 3] = ["slot_0", "slot_1", "slot_2"];
    for (i, key) in KEYS.iter().enumerate() {
        if let Some(Tag::Compound(comp)) = saved.get(&JavaString::from(*key)) {
            slots[i] = Some(comp.clone());
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

fn encode_seed(seed: &i32) -> Result<Compound, AttachmentError> {
    let mut out = Compound::new();
    out.insert(JavaString::from("seed"), Tag::Int(*seed));
    Ok(out)
}

fn decode_seed(saved: &Compound) -> Result<i32, AttachmentError> {
    match saved.get(&JavaString::from("seed")) {
        Some(Tag::Int(v)) => Ok(*v),
        _ => Ok(0),
    }
}

pub const PLAYER_SEED: AttachmentType<i32> = AttachmentType {
    namespace: MOD_ID,
    name: "enchantment_seed",
    default: || 0,
    codec: Some(AttachmentCodec {
        encode: encode_seed,
        decode: decode_seed,
    }),
    copy_on_death: true,
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
    let stacks = contents.to_stacks(&*host);
    for stack in stacks.into_iter().flatten() {
        let _ = level.pop_resource(host, pos, stack);
    }
}

/// Reads (or initializes with a deterministic non-zero hash) a player's enchantment seed.
pub fn get_or_init_player_seed(host: &mut dyn ServerHost, player: u128) -> i32 {
    if let Ok(Some(seed)) = PLAYER_SEED.get_existing(host, Holder::Player(player))
        && seed != 0
    {
        return seed;
    }
    let seed = next_random_seed(player, 0);
    let _ = PLAYER_SEED.set(host, Holder::Player(player), seed);
    seed
}

pub fn advance_player_seed(host: &mut dyn ServerHost, player: u128, prev: i32) -> i32 {
    let next = next_random_seed(player, prev);
    let _ = PLAYER_SEED.set(host, Holder::Player(player), next);
    next
}

fn next_random_seed(player: u128, prev: i32) -> i32 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut x = (player as u64)
        ^ ((player >> 64) as u64).rotate_left(17)
        ^ (prev as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ n.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    let candidate = (x as i32) ^ ((x >> 32) as i32);
    if candidate == 0 { 1 } else { candidate }
}

/// Redstone comparator signal for an enchanting table: `min(slot1.count, 3)`, matching
/// `EnchantingTableContainerBlock.getAnalogOutputSignal`.
pub fn comparator_signal(contents: &TableContents) -> i32 {
    contents.slot_count_from_nbt(1).min(3)
}

/// Hopper/automation `ITEMS` capability for `minecraft:enchanting_table`, matching
/// `EnchantingTableContainerBlockEntity` (`WorldlyContainer`).
pub struct EnchantingTableItemHandler {
    side: BlockSide,
}

impl EnchantingTableItemHandler {
    pub fn new(side: BlockSide) -> Self {
        Self { side }
    }

    fn exposed_slots(&self) -> Vec<usize> {
        let dedicated = server_config().dedicated_reroll_catalyst_active();
        match self.side.side {
            Some(Direction::Up) | Some(Direction::Down) => vec![0],
            Some(_) => {
                if dedicated {
                    vec![1, 2]
                } else {
                    vec![1]
                }
            }
            None => {
                if dedicated {
                    vec![0, 1, 2]
                } else {
                    vec![0, 1]
                }
            }
        }
    }

    fn physical_slot(&self, index: usize) -> Option<usize> {
        self.exposed_slots().get(index).copied()
    }

    fn dimension_name(&self, host: &dyn ServerHost) -> Option<String> {
        self.side
            .level
            .dimension()
            .id(host)
            .map(str::to_owned)
            .or_else(|| Some(host.dimension().to_owned()))
    }

    fn slot_accepts(&self, host: &dyn Host, slot: usize, resource: &ItemResource) -> bool {
        match slot {
            0 => true,
            1 => is_enchanting_catalyst(host, resource.item()),
            2 => {
                server_config().dedicated_reroll_catalyst_active()
                    && is_reroll_catalyst(host, resource.item())
            }
            _ => false,
        }
    }

    fn can_insert_from_face(&self, host: &dyn Host, slot: usize, resource: &ItemResource) -> bool {
        if self.side.side == Some(Direction::Down) {
            return false;
        }
        self.slot_accepts(host, slot, resource)
    }

    fn can_extract_from_face(&self, host: &dyn Host, slot: usize, stack: &ItemStack) -> bool {
        match self.side.side {
            Some(Direction::Down) => slot == 0 && is_stack_enchanted_or_book(host, stack),
            None => true,
            Some(_) => false,
        }
    }
}

impl ResourceHandler<ItemResource> for EnchantingTableItemHandler {
    fn size(&self, _host: &mut dyn ServerHost) -> usize {
        self.exposed_slots().len()
    }

    fn resource(&self, host: &mut dyn ServerHost, index: usize) -> Option<ItemResource> {
        let slot = self.physical_slot(index)?;
        let dim = self.dimension_name(host)?;
        let contents = read_table_contents(host, &dim, self.side.pos);
        let stack = contents.to_stacks(&*host)[slot].clone()?;
        Some(ItemResource::of(&stack))
    }

    fn amount(&self, host: &mut dyn ServerHost, index: usize) -> u32 {
        let Some(slot) = self.physical_slot(index) else {
            return 0;
        };
        let Some(dim) = self.dimension_name(host) else {
            return 0;
        };
        let contents = read_table_contents(host, &dim, self.side.pos);
        contents.to_stacks(&*host)[slot]
            .as_ref()
            .map_or(0, ItemStack::count)
    }

    fn capacity(
        &self,
        host: &mut dyn ServerHost,
        index: usize,
        resource: Option<&ItemResource>,
    ) -> u32 {
        let Some(slot) = self.physical_slot(index) else {
            return 0;
        };
        let slot_max = if slot == 0 { 1 } else { 64 };
        match resource {
            Some(res) => {
                if !self.slot_accepts(&*host, slot, res) {
                    return 0;
                }
                let item_max = host.max_stack_size(res.item()).unwrap_or(64);
                slot_max.min(item_max)
            }
            None => slot_max,
        }
    }

    fn is_valid(&self, host: &mut dyn ServerHost, index: usize, resource: &ItemResource) -> bool {
        let Some(slot) = self.physical_slot(index) else {
            return false;
        };
        self.slot_accepts(&*host, slot, resource)
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
        if amount == 0 {
            return Ok(0);
        }
        let Some(slot) = self.physical_slot(index) else {
            return Ok(0);
        };
        if !self.can_insert_from_face(&*host, slot, resource) {
            return Ok(0);
        }
        let Some(dim) = self.dimension_name(host) else {
            return Ok(0);
        };
        if host.dimension() != dim {
            return Ok(0);
        }

        let mut contents = read_table_contents(host, &dim, self.side.pos);
        let prev_signal = comparator_signal(&contents);
        let mut stacks = contents.to_stacks(&*host);
        let max_cap = self.capacity(host, index, Some(resource));
        if max_cap == 0 {
            return Ok(0);
        }

        let moved = match &mut stacks[slot] {
            Some(existing) => {
                if ItemResource::of(existing) != *resource {
                    return Ok(0);
                }
                let room = max_cap.saturating_sub(existing.count());
                let to_add = amount.min(room);
                if to_add == 0 {
                    return Ok(0);
                }
                if existing.grow(to_add).is_err() {
                    return Ok(0);
                }
                to_add
            }
            None => {
                let to_add = amount.min(max_cap);
                if to_add == 0 {
                    return Ok(0);
                }
                let Some(new_stack) = resource.to_stack(to_add) else {
                    return Ok(0);
                };
                stacks[slot] = Some(new_stack);
                to_add
            }
        };

        contents = TableContents::from_stacks(&*host, &stacks);
        write_table_contents(host, &dim, self.side.pos, &contents);
        // Comparator and viewer sync run post-commit in on_server_tick_post so an aborted
        // transaction never publishes notifications or side effects.
        crate::server::mark_table_dirty(&dim, self.side.pos, prev_signal);
        Ok(moved)
    }

    fn extract_at(
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
        if amount == 0 {
            return Ok(0);
        }
        let Some(slot) = self.physical_slot(index) else {
            return Ok(0);
        };
        let Some(dim) = self.dimension_name(host) else {
            return Ok(0);
        };
        if host.dimension() != dim {
            return Ok(0);
        }

        let contents = read_table_contents(host, &dim, self.side.pos);
        let prev_signal = comparator_signal(&contents);
        let mut stacks = contents.to_stacks(&*host);
        let Some(existing) = stacks[slot].clone() else {
            return Ok(0);
        };
        if ItemResource::of(&existing) != *resource {
            return Ok(0);
        }
        if !self.can_extract_from_face(&*host, slot, &existing) {
            return Ok(0);
        }

        let taken = amount.min(existing.count());
        if taken == 0 {
            return Ok(0);
        }
        stacks[slot] = existing.shrink(taken);

        let updated = TableContents::from_stacks(&*host, &stacks);
        write_table_contents(host, &dim, self.side.pos, &updated);
        crate::server::mark_table_dirty(&dim, self.side.pos, prev_signal);
        Ok(taken)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparator_signal_caps_at_three() {
        let empty = TableContents::default();
        assert_eq!(comparator_signal(&empty), 0);

        let mut c1 = Compound::new();
        c1.insert(JavaString::from("count"), Tag::Int(2));
        let t1 = TableContents {
            slots: [None, Some(c1), None],
        };
        assert_eq!(comparator_signal(&t1), 2);

        let mut c2 = Compound::new();
        c2.insert(JavaString::from("count"), Tag::Int(64));
        let t2 = TableContents {
            slots: [None, Some(c2), None],
        };
        assert_eq!(comparator_signal(&t2), 3);
    }

    #[test]
    fn table_contents_codec_round_trips() {
        let mut slot0 = Compound::new();
        slot0.insert(
            JavaString::from("id"),
            Tag::String(JavaString::from("minecraft:diamond_sword")),
        );
        slot0.insert(JavaString::from("count"), Tag::Int(1));

        let orig = TableContents {
            slots: [Some(slot0), None, None],
        };
        let encoded = encode_contents(&orig).unwrap();
        let decoded = decode_contents(&encoded).unwrap();
        assert_eq!(decoded, orig);
    }
}
