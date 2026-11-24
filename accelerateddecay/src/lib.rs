//! Accelerated Decay for Sunrise: unsupported leaves decay quickly after their logs are removed,
//! five blocks per tick per dimension, with the queue persisted across reloads, as Accelerated
//! Decay 26.3.0.1 (ErrorMikey) does.
//!
//! Server-side `block_changed` observes `Level.setBlock` transitions where both `old` and `new`
//! are in `#minecraft:leaves` (preventing newly placed or bone-mealed trees from decaying) and
//! `new` has `persistent=false` and `distance=7`. Positions are staged in memory until
//! `level_tick_post`, which merges them into the level's persistent `accelerateddecay:break_data`
//! attachment and breaks up to 5 still-decaying leaves per tick, playing break sound and
//! particles only for the first leaf broken in that tick.
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;

use mod_api::{
    AttachmentCodec, AttachmentError, AttachmentType, BlockChangedEvent, Compound,
    GameEventContext, Holder, Hooks, HostBlockStates, HostSide, JavaString, LevelHost, LevelRef,
    LevelTickPostEvent, LevelUnloadEvent, Registrar, RegistrationContext, ServerHost, Tag,
};

/// Block tag identifying leaves eligible for accelerated decay.
pub const LEAVES_TAG: &str = "minecraft:leaves";
/// Distance value vanilla sets on a leaf with no supporting log within range.
pub const DECAY_DISTANCE: &str = "7";
/// Maximum queued leaf positions processed per level per server tick.
pub const MAX_BREAKS_PER_TICK: usize = 5;
/// Persistent level attachment identifier holding pending decay targets.
pub const BREAK_DATA_ID: &str = "accelerateddecay:break_data";
const BREAK_TARGETS_KEY: &str = "breakTargets";

/// Whether a leaf block with these `persistent` and `distance` property values is ready to decay.
pub fn is_decayable_leaf(persistent: Option<&str>, distance: Option<&str>) -> bool {
    persistent == Some("false") && distance == Some(DECAY_DISTANCE)
}

/// Whether a `Level.setBlock` transition should enqueue the block for accelerated decay.
///
/// Requiring `old_is_leaves` matches Accelerated Decay's fix for bone-mealed and newly generated
/// trees, where leaves are initially placed into non-leaf blocks with default `distance=7`.
pub fn should_queue_transition(
    old_is_leaves: bool,
    new_is_leaves: bool,
    new_persistent: Option<&str>,
    new_distance: Option<&str>,
) -> bool {
    old_is_leaves && new_is_leaves && is_decayable_leaf(new_persistent, new_distance)
}

/// FIFO queue of leaf positions waiting to break in one dimension, deduplicated in O(1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BreakQueue {
    targets: VecDeque<[i32; 3]>,
    queued: HashSet<[i32; 3]>,
}

impl BreakQueue {
    /// Append `pos` to the back of the queue if it is not already queued.
    pub fn push(&mut self, pos: [i32; 3]) -> bool {
        if self.queued.insert(pos) {
            self.targets.push_back(pos);
            true
        } else {
            false
        }
    }

    /// Append every position in `positions`, keeping FIFO order and skipping duplicates.
    pub fn extend<I: IntoIterator<Item = [i32; 3]>>(&mut self, positions: I) {
        for pos in positions {
            self.push(pos);
        }
    }

    /// Pop the next queued position.
    pub fn pop_front(&mut self) -> Option<[i32; 3]> {
        let pos = self.targets.pop_front()?;
        self.queued.remove(&pos);
        Some(pos)
    }

    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    pub fn len(&self) -> usize {
        self.targets.len()
    }
}

/// Encode `queue` as a flat `[x0, y0, z0, ...]` `IntArray` under `breakTargets`.
pub fn encode_queue(queue: &BreakQueue) -> Result<Compound, AttachmentError> {
    let mut coords = Vec::with_capacity(queue.targets.len() * 3);
    for &[x, y, z] in &queue.targets {
        coords.push(x);
        coords.push(y);
        coords.push(z);
    }
    let mut compound = Compound::new();
    compound.insert(JavaString::from(BREAK_TARGETS_KEY), Tag::IntArray(coords));
    Ok(compound)
}

/// Decode a saved `BreakQueue` from its level attachment compound.
pub fn decode_queue(compound: &Compound) -> Result<BreakQueue, AttachmentError> {
    let key = JavaString::from(BREAK_TARGETS_KEY);
    let Some(tag) = compound.get(&key) else {
        return Ok(BreakQueue::default());
    };
    let Tag::IntArray(coords) = tag else {
        return Err(AttachmentError::Decode);
    };
    let (chunks, rem) = coords.as_chunks::<3>();
    if !rem.is_empty() {
        return Err(AttachmentError::Decode);
    }
    let mut queue = BreakQueue::default();
    for &pos in chunks {
        queue.push(pos);
    }
    Ok(queue)
}

fn empty_queue() -> BreakQueue {
    BreakQueue::default()
}

static BREAK_DATA: AttachmentType<BreakQueue> = AttachmentType {
    namespace: "accelerateddecay",
    name: "break_data",
    default: empty_queue,
    codec: Some(AttachmentCodec {
        encode: encode_queue,
        decode: decode_queue,
    }),
    copy_on_death: false,
    sync: false,
};

/// Break a queued leaf if it is loaded and still a non-persistent `distance=7` leaf in
/// `#minecraft:leaves`. Returns `true` when the block was removed.
fn break_decaying_leaf(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    sound_and_particles: bool,
) -> bool {
    let Some(state) = level.block_state(&*host, pos) else {
        return false;
    };
    if host.block_in_tag(state.block(), LEAVES_TAG) != Some(true) {
        return false;
    }
    let decaying = {
        let states = host.block_states();
        let persistent = state.property(&states, "persistent").ok().flatten();
        let distance = state.property(&states, "distance").ok().flatten();
        is_decayable_leaf(persistent, distance)
    };
    if !decaying {
        return false;
    }

    if sound_and_particles {
        let _ = level.block_break_effect(host, pos, state);
    }
    if let Ok(Some(drops)) = level.block_drops(host, pos, None, None) {
        for stack in drops {
            let _ = level.pop_resource(host, pos, stack);
        }
    }
    let destroyed = level.remove_block(host, pos, false);
    if destroyed {
        let _ = level.game_event(
            host,
            "minecraft:block_destroy",
            [
                f64::from(pos[0]) + 0.5,
                f64::from(pos[1]) + 0.5,
                f64::from(pos[2]) + 0.5,
            ],
            GameEventContext {
                affected_state: Some(state),
                ..Default::default()
            },
        );
    }
    destroyed
}

#[derive(Default)]
struct Mod {
    /// Newly decaying leaves staged from read-only `block_changed` until that dimension's
    /// `level_tick_post` merges them into the level's persistent `BREAK_DATA` attachment.
    pending: Mutex<HashMap<String, Vec<[i32; 3]>>>,
}

impl Mod {
    fn stage(&self, dimension: &str, pos: [i32; 3]) {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(list) = pending.get_mut(dimension) {
            list.push(pos);
        } else {
            pending.insert(dimension.to_owned(), vec![pos]);
        }
    }

    fn take_staged(&self, dimension: &str) -> Vec<[i32; 3]> {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        pending
            .get_mut(dimension)
            .map(std::mem::take)
            .unwrap_or_default()
    }

    fn drop_dimension(&self, dimension: &str) {
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(dimension);
    }
}

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        let _ = r.register_attachment_type(BREAK_DATA_ID, BREAK_DATA.sync);
    }

    fn block_changed(&self, level: &dyn LevelHost, event: &mut BlockChangedEvent) {
        let old_is_leaves = level.block_in_tag(event.old.block(), LEAVES_TAG) == Some(true);
        let new_is_leaves = level.block_in_tag(event.new.block(), LEAVES_TAG) == Some(true);
        if !old_is_leaves || !new_is_leaves {
            return;
        }
        let states = level.block_states();
        let persistent = event.new.property(&states, "persistent").ok().flatten();
        let distance = event.new.property(&states, "distance").ok().flatten();
        if should_queue_transition(old_is_leaves, new_is_leaves, persistent, distance) {
            let p = event.pos;
            self.stage(event.dimension.key().id(), [p.x, p.y, p.z]);
        }
    }

    fn level_tick_post(&self, host: HostSide<'_>, event: &mut LevelTickPostEvent) {
        let HostSide::Server(host) = host else {
            return;
        };
        let level = event.level;
        let staged = {
            let Some(dim) = level.dimension().id(&*host) else {
                return;
            };
            self.take_staged(dim)
        };
        if staged.is_empty()
            && !BREAK_DATA
                .has(host.attachments(), Holder::Level)
                .unwrap_or(false)
        {
            return;
        }

        let mut queue = BREAK_DATA
            .get_existing(host.attachments(), Holder::Level)
            .ok()
            .flatten()
            .unwrap_or_default();
        queue.extend(staged);

        let mut played_effect = false;
        for _ in 0..MAX_BREAKS_PER_TICK {
            let Some(pos) = queue.pop_front() else {
                break;
            };
            if break_decaying_leaf(host, level, pos, !played_effect) {
                played_effect = true;
            }
        }

        // Fold any transitions triggered during this tick's breaks so they persist on save.
        if let Some(dim) = level.dimension().id(&*host) {
            let extra = self.take_staged(dim);
            if !extra.is_empty() {
                queue.extend(extra);
            }
        }

        if queue.is_empty() {
            let _ = BREAK_DATA.remove(host.attachments(), Holder::Level);
        } else {
            let _ = BREAK_DATA.set(host.attachments(), Holder::Level, queue);
        }
    }

    fn level_unload(&self, _host: &mut dyn ServerHost, event: &mut LevelUnloadEvent<'_>) {
        self.drop_dimension(event.dimension);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_natural_distance_seven_leaves_decay() {
        assert!(is_decayable_leaf(Some("false"), Some("7")));
        assert!(!is_decayable_leaf(Some("true"), Some("7")));
        assert!(!is_decayable_leaf(Some("false"), Some("6")));
        assert!(!is_decayable_leaf(Some("false"), Some("1")));
        assert!(!is_decayable_leaf(None, Some("7")));
        assert!(!is_decayable_leaf(Some("false"), None));
    }

    #[test]
    fn newly_placed_and_removed_leaves_are_not_queued() {
        // Leaf updating from distance=6 to distance=7 is queued.
        assert!(should_queue_transition(
            true,
            true,
            Some("false"),
            Some("7")
        ));
        // Air -> leaf (e.g. bone-mealed or generated tree) must not be queued.
        assert!(!should_queue_transition(
            false,
            true,
            Some("false"),
            Some("7")
        ));
        // Leaf -> air is ignored.
        assert!(!should_queue_transition(
            true,
            false,
            Some("false"),
            Some("7")
        ));
        // Player-placed persistent leaf is never queued.
        assert!(!should_queue_transition(
            true,
            true,
            Some("true"),
            Some("7")
        ));
        // Still-supported leaf is not queued.
        assert!(!should_queue_transition(
            true,
            true,
            Some("false"),
            Some("5")
        ));
    }

    #[test]
    fn queue_is_fifo_and_deduplicated() {
        let mut q = BreakQueue::default();
        assert!(q.push([1, 64, 2]));
        assert!(q.push([3, 65, 4]));
        assert!(!q.push([1, 64, 2]));
        assert_eq!(q.len(), 2);

        assert_eq!(q.pop_front(), Some([1, 64, 2]));
        // Once popped, the same position can be queued again later.
        assert!(q.push([1, 64, 2]));
        assert_eq!(q.pop_front(), Some([3, 65, 4]));
        assert_eq!(q.pop_front(), Some([1, 64, 2]));
        assert_eq!(q.pop_front(), None);
        assert!(q.is_empty());
    }

    #[test]
    fn queue_round_trips_through_nbt_and_rejects_corrupt_data() {
        let mut q = BreakQueue::default();
        q.extend([[10, 70, -5], [-3, 120, 42], [10, 70, -5], [0, -64, 0]]);
        let saved = encode_queue(&q).expect("encode");
        let loaded = decode_queue(&saved).expect("decode");
        assert_eq!(loaded, q);
        assert_eq!(loaded.len(), 3);

        // Empty compound decodes as an empty queue.
        let empty = decode_queue(&Compound::new()).expect("empty compound");
        assert!(empty.is_empty());

        // Truncated int array is rejected.
        let mut bad_len = Compound::new();
        bad_len.insert(
            JavaString::from(BREAK_TARGETS_KEY),
            Tag::IntArray(vec![1, 2]),
        );
        assert!(matches!(
            decode_queue(&bad_len),
            Err(AttachmentError::Decode)
        ));

        // Wrong NBT tag type is rejected.
        let mut bad_tag = Compound::new();
        bad_tag.insert(JavaString::from(BREAK_TARGETS_KEY), Tag::Int(5));
        assert!(matches!(
            decode_queue(&bad_tag),
            Err(AttachmentError::Decode)
        ));
    }

    #[test]
    fn staging_is_isolated_by_dimension_and_clears_on_unload() {
        let m = Mod::default();
        m.stage("minecraft:overworld", [1, 70, 1]);
        m.stage("minecraft:overworld", [2, 70, 1]);
        m.stage("minecraft:the_nether", [0, 80, 0]);

        assert_eq!(
            m.take_staged("minecraft:overworld"),
            vec![[1, 70, 1], [2, 70, 1]]
        );
        assert!(m.take_staged("minecraft:overworld").is_empty());

        m.drop_dimension("minecraft:the_nether");
        assert!(m.take_staged("minecraft:the_nether").is_empty());
    }

    #[test]
    fn queue_drains_in_five_block_batches() {
        let mut q = BreakQueue::default();
        for x in 0..12 {
            q.push([x, 70, 0]);
        }
        let mut batches = Vec::new();
        while !q.is_empty() {
            let mut batch = Vec::new();
            for _ in 0..MAX_BREAKS_PER_TICK {
                let Some(pos) = q.pop_front() else {
                    break;
                };
                batch.push(pos[0]);
            }
            batches.push(batch);
        }
        assert_eq!(
            batches,
            vec![vec![0, 1, 2, 3, 4], vec![5, 6, 7, 8, 9], vec![10, 11]]
        );
    }
}
