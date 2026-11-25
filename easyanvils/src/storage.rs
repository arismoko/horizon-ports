//! Persistent anvil slot storage in a transactional chunk attachment (`Holder::Chunk`),
//! comparator signal math, and `ITEMS` hopper capability matching `AnvilBlockEntity`.

use crate::config::MOD_ID;
use mod_api::{
    AttachmentCodec, AttachmentError, AttachmentStore, AttachmentType, BlockSide, Compound,
    Direction, Holder, Host, ItemResource, ItemStack, JavaString, LevelRef, Overanswered,
    ResourceHandler, ServerHost, Tag, TransactionContext, TransactionError,
};
use std::collections::BTreeMap;

pub const ATTACH_ANVIL_CONTENTS: &str = "easyanvils:anvil_contents";

/// Serialized 2-slot input contents of one anvil (`ItemStack::to_nbt` compounds),
/// along with each slot's effective max stack size for redstone comparator calculation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnvilSlots {
    pub slots: [Option<Compound>; 2],
    pub max_stacks: [u32; 2],
}

impl AnvilSlots {
    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    pub fn to_stacks(&self, host: &dyn Host) -> [Option<ItemStack>; 2] {
        [
            self.slots[0]
                .as_ref()
                .and_then(|c| ItemStack::from_nbt(host, c)),
            self.slots[1]
                .as_ref()
                .and_then(|c| ItemStack::from_nbt(host, c)),
        ]
    }

    pub fn from_stacks(host: &dyn Host, stacks: &[Option<ItemStack>; 2]) -> Self {
        Self {
            slots: [
                stacks[0].as_ref().and_then(|s| s.to_nbt(host)),
                stacks[1].as_ref().and_then(|s| s.to_nbt(host)),
            ],
            max_stacks: [
                stacks[0]
                    .as_ref()
                    .map_or(64, |s| host.max_stack_size(s.item()).unwrap_or(64)),
                stacks[1]
                    .as_ref()
                    .map_or(64, |s| host.max_stack_size(s.item()).unwrap_or(64)),
            ],
        }
    }

    pub fn slot_count_from_nbt(&self, slot: usize) -> u32 {
        let Some(compound) = self.slots.get(slot).and_then(Option::as_ref) else {
            return 0;
        };
        match compound.get(&JavaString::from("count")) {
            Some(Tag::Int(v)) => (*v).max(0) as u32,
            Some(Tag::Byte(v)) => i32::from(*v).max(0) as u32,
            Some(Tag::Short(v)) => i32::from(*v).max(0) as u32,
            _ => 1,
        }
    }
}

/// All persistent anvils stored inside a single chunk.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChunkAnvils {
    pub entries: BTreeMap<[i32; 3], AnvilSlots>,
}

fn encode_chunk_anvils(value: &ChunkAnvils) -> Result<Compound, AttachmentError> {
    let mut list = Vec::with_capacity(value.entries.len());
    for (pos, slots) in &value.entries {
        if slots.is_empty() {
            continue;
        }
        let mut entry = Compound::new();
        entry.insert(JavaString::from("x"), Tag::Int(pos[0]));
        entry.insert(JavaString::from("y"), Tag::Int(pos[1]));
        entry.insert(JavaString::from("z"), Tag::Int(pos[2]));
        if let Some(s0) = &slots.slots[0] {
            entry.insert(JavaString::from("slot_0"), Tag::Compound(s0.clone()));
            entry.insert(
                JavaString::from("slot_0_max"),
                Tag::Int(slots.max_stacks[0] as i32),
            );
        }
        if let Some(s1) = &slots.slots[1] {
            entry.insert(JavaString::from("slot_1"), Tag::Compound(s1.clone()));
            entry.insert(
                JavaString::from("slot_1_max"),
                Tag::Int(slots.max_stacks[1] as i32),
            );
        }
        list.push(Tag::Compound(entry));
    }
    let mut out = Compound::new();
    out.insert(JavaString::from("anvils"), Tag::List(list));
    Ok(out)
}

fn decode_chunk_anvils(saved: &Compound) -> Result<ChunkAnvils, AttachmentError> {
    let mut entries = BTreeMap::new();
    if let Some(Tag::List(list)) = saved.get(&JavaString::from("anvils")) {
        for item in list {
            let Tag::Compound(entry) = item else {
                continue;
            };
            let (Some(Tag::Int(x)), Some(Tag::Int(y)), Some(Tag::Int(z))) = (
                entry.get(&JavaString::from("x")),
                entry.get(&JavaString::from("y")),
                entry.get(&JavaString::from("z")),
            ) else {
                continue;
            };
            let s0 = match entry.get(&JavaString::from("slot_0")) {
                Some(Tag::Compound(c)) => Some(c.clone()),
                _ => None,
            };
            let s1 = match entry.get(&JavaString::from("slot_1")) {
                Some(Tag::Compound(c)) => Some(c.clone()),
                _ => None,
            };
            let m0 = match entry.get(&JavaString::from("slot_0_max")) {
                Some(Tag::Int(v)) if *v > 0 => *v as u32,
                _ => 64,
            };
            let m1 = match entry.get(&JavaString::from("slot_1_max")) {
                Some(Tag::Int(v)) if *v > 0 => *v as u32,
                _ => 64,
            };
            let slots = AnvilSlots {
                slots: [s0, s1],
                max_stacks: [m0, m1],
            };
            if !slots.is_empty() {
                entries.insert([*x, *y, *z], slots);
            }
        }
    }
    Ok(ChunkAnvils { entries })
}

pub const ANVIL_CONTENTS: AttachmentType<ChunkAnvils> = AttachmentType {
    namespace: MOD_ID,
    name: "anvil_contents",
    default: ChunkAnvils::default,
    codec: Some(AttachmentCodec {
        encode: encode_chunk_anvils,
        decode: decode_chunk_anvils,
    }),
    copy_on_death: false,
    sync: false,
};

pub fn chunk_holder(pos: [i32; 3]) -> Holder {
    Holder::Chunk([pos[0] >> 4, pos[2] >> 4])
}

pub fn read_anvil_slots(host: &mut dyn ServerHost, dimension: &str, pos: [i32; 3]) -> AnvilSlots {
    if host.dimension() != dimension {
        return AnvilSlots::default();
    }
    ANVIL_CONTENTS
        .get_existing(host, chunk_holder(pos))
        .ok()
        .flatten()
        .and_then(|chunk| chunk.entries.get(&pos).cloned())
        .unwrap_or_default()
}

pub fn read_anvil_slots_store<S: AttachmentStore + ?Sized>(
    store: &mut S,
    pos: [i32; 3],
) -> AnvilSlots {
    ANVIL_CONTENTS
        .get_existing(store, chunk_holder(pos))
        .ok()
        .flatten()
        .and_then(|chunk| chunk.entries.get(&pos).cloned())
        .unwrap_or_default()
}

pub fn write_anvil_slots(
    host: &mut dyn ServerHost,
    dimension: &str,
    pos: [i32; 3],
    slots: &AnvilSlots,
) {
    if host.dimension() != dimension {
        return;
    }
    let holder = chunk_holder(pos);
    let mut chunk = ANVIL_CONTENTS
        .get_existing(host, holder)
        .ok()
        .flatten()
        .unwrap_or_default();
    if slots.is_empty() {
        chunk.entries.remove(&pos);
    } else {
        chunk.entries.insert(pos, slots.clone());
    }
    if chunk.entries.is_empty() {
        let _ = ANVIL_CONTENTS.remove(host, holder);
    } else {
        let _ = ANVIL_CONTENTS.set(host, holder, chunk);
    }
}

pub fn drop_and_clear_anvil_slots(
    host: &mut dyn ServerHost,
    level: LevelRef,
    dimension: &str,
    pos: [i32; 3],
) {
    if host.dimension() != dimension {
        return;
    }
    let slots = read_anvil_slots(host, dimension, pos);
    if slots.is_empty() {
        return;
    }
    write_anvil_slots(host, dimension, pos, &AnvilSlots::default());
    let stacks = slots.to_stacks(&*host);
    for stack in stacks.into_iter().flatten() {
        let _ = level.pop_resource(host, pos, stack);
    }
}

/// Vanilla `AbstractContainerMenu.getRedstoneSignalFromContainer` over 2 slots.
pub fn comparator_signal_from_counts(slot_ratios: [Option<(u32, u32)>; 2]) -> i32 {
    let mut non_empty = 0u32;
    let mut fill = 0.0f32;
    for entry in slot_ratios.into_iter().flatten() {
        let (count, max_stack) = entry;
        if count > 0 && max_stack > 0 {
            fill += (count as f32) / (max_stack.min(64) as f32);
            non_empty += 1;
        }
    }
    if non_empty == 0 {
        return 0;
    }
    let average = fill / 2.0;
    ((average * 14.0).floor() as i32) + 1
}

pub fn comparator_signal(slots: &AnvilSlots) -> i32 {
    let ratios = [
        slots.slots[0]
            .as_ref()
            .map(|_| (slots.slot_count_from_nbt(0), slots.max_stacks[0].max(1))),
        slots.slots[1]
            .as_ref()
            .map(|_| (slots.slot_count_from_nbt(1), slots.max_stacks[1].max(1))),
    ];
    comparator_signal_from_counts(ratios)
}

/// Hopper/automation `ITEMS` capability for anvils, matching `AnvilBlockEntity` (`WorldlyContainer`):
/// top and sides can insert into slots 0 and 1; bottom cannot extract.
pub struct AnvilItemHandler {
    side: BlockSide,
}

impl AnvilItemHandler {
    pub fn new(side: BlockSide) -> Self {
        Self { side }
    }

    fn exposed_slots(&self) -> &'static [usize] {
        match self.side.side {
            Some(Direction::Down) => &[],
            _ => &[0, 1],
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
}

impl ResourceHandler<ItemResource> for AnvilItemHandler {
    fn size(&self, _host: &mut dyn ServerHost) -> usize {
        self.exposed_slots().len()
    }

    fn resource(&self, host: &mut dyn ServerHost, index: usize) -> Option<ItemResource> {
        let slot = self.physical_slot(index)?;
        let dim = self.dimension_name(host)?;
        let slots = read_anvil_slots(host, &dim, self.side.pos);
        let stack = slots.to_stacks(&*host)[slot].clone()?;
        Some(ItemResource::of(&stack))
    }

    fn amount(&self, host: &mut dyn ServerHost, index: usize) -> u32 {
        let Some(slot) = self.physical_slot(index) else {
            return 0;
        };
        let Some(dim) = self.dimension_name(host) else {
            return 0;
        };
        let slots = read_anvil_slots(host, &dim, self.side.pos);
        slots.to_stacks(&*host)[slot]
            .as_ref()
            .map_or(0, ItemStack::count)
    }

    fn capacity(
        &self,
        host: &mut dyn ServerHost,
        index: usize,
        resource: Option<&ItemResource>,
    ) -> u32 {
        if self.physical_slot(index).is_none() {
            return 0;
        }
        match resource {
            Some(res) => host.max_stack_size(res.item()).unwrap_or(64).min(64),
            None => 64,
        }
    }

    fn is_valid(&self, _host: &mut dyn ServerHost, index: usize, _resource: &ItemResource) -> bool {
        self.physical_slot(index).is_some()
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
        if amount == 0 || self.side.side == Some(Direction::Down) {
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

        let mut stored = read_anvil_slots(host, &dim, self.side.pos);
        let prev_signal = comparator_signal(&stored);
        let mut stacks = stored.to_stacks(&*host);
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

        stored = AnvilSlots::from_stacks(&*host, &stacks);
        write_anvil_slots(host, &dim, self.side.pos, &stored);
        crate::server::mark_anvil_dirty(&dim, self.side.pos, prev_signal);
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
        // Matches AnvilBlockEntity.canTakeItemThroughFace -> false
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparator_signal_matches_vanilla_container_formula() {
        assert_eq!(comparator_signal_from_counts([None, None]), 0);
        assert_eq!(comparator_signal_from_counts([Some((1, 64)), None]), 1);
        assert_eq!(comparator_signal_from_counts([Some((1, 1)), None]), 8);
        assert_eq!(
            comparator_signal_from_counts([Some((1, 1)), Some((64, 64))]),
            15
        );
    }

    #[test]
    fn chunk_anvils_round_trip_nbt() {
        let mut s0 = Compound::new();
        s0.insert(
            JavaString::from("id"),
            Tag::String("minecraft:diamond_sword".into()),
        );
        let mut chunk = ChunkAnvils::default();
        chunk.entries.insert(
            [10, 64, -5],
            AnvilSlots {
                slots: [Some(s0.clone()), None],
                max_stacks: [1, 64],
            },
        );
        let encoded = encode_chunk_anvils(&chunk).unwrap();
        let decoded = decode_chunk_anvils(&encoded).unwrap();
        assert_eq!(decoded, chunk);
    }
}
