//! Falling Trees for Horizon: chopping a tree's log with an axe takes the whole tree down, as
//! Falling Trees 0.13.2 (PandaMods) does with Homestead's config (generic trees, axes only,
//! 256 logs at most, leaves within 10, persistent leaves left alone, sneak to chop one log).
//!
//! The tree, as `GenericTree.gatherTreeData` finds it: the logs reachable from the chopped one
//! through the 3x3 squares on its level and the level above (never down), then for each log the
//! leaves whose `distance` matches their path from it (so another tree's leaves don't count), and
//! the vines, cocoa and bee nests touching any of them. No leaves: not a tree.
//!
//! `break_block` finds the tree and lets the chopped log break as vanilla does; at the end of
//! the server tick every other block is destroyed with the held axe as its tool
//! (`Level.destroyBlock`, attachments, then leaves, then logs, so nothing pops twice) and its
//! drops land at the stump. The axe takes one more point of wear per extra log
//! (`hurtAndBreak`, so Unbreaking applies) and the player one more 0.005 of exhaustion, as
//! the original charges `logs.size` of each for the whole tree.
//!
//! Sneaking is the player's `is_shift_key_down`; the held axe is the player's main hand.
//!
//! Not ported: the falling animation and its sounds, the 4 s before the drops land, slower
//! mining for bigger trees (no mining-speed hook), and the cactus, bamboo and chorus "trees"
//! (vanilla already breaks those upwards). Axes are matched by name.
use std::collections::{HashSet, VecDeque};
use std::sync::Mutex;

use mod_api::{
    BreakBlockEvent, EquipmentSlot, Flow, GameMode, Hand, Hooks, Host, ItemStack, LevelRef,
    LivingRef, PlayerRef, ServerHost, ServerTickEvent, ServerTickPhase,
};

pub const MAX_LOGS: usize = 256;
pub const MAX_LEAVES_RADIUS: u32 = 10;

/// `#minecraft:axes` by name.
pub fn is_axe(item: &str) -> bool {
    item.ends_with("_axe") && !item.ends_with("_pickaxe")
}

/// What the finder needs to know about a block.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Block {
    pub name: String,
    /// In `#minecraft:logs`.
    pub log: bool,
    /// In `#minecraft:leaves`.
    pub leaves: bool,
    /// The leaves' `distance`, if the block has one.
    pub distance: Option<u32>,
    pub persistent: bool,
}

/// A tree to take down, apart from the chopped log.
#[derive(Debug, Default, PartialEq)]
pub struct Tree {
    pub logs: Vec<[i32; 3]>,
    pub leaves: Vec<[i32; 3]>,
    /// Vines, cocoa and bee nests.
    pub attached: Vec<[i32; 3]>,
}

const SIDES: [[i32; 3]; 6] = [
    [1, 0, 0],
    [-1, 0, 0],
    [0, 1, 0],
    [0, -1, 0],
    [0, 0, 1],
    [0, 0, -1],
];

fn add(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// The tree whose log at `start` is chopped, or None if it isn't one (no leaves, too big).
pub fn find_tree(start: [i32; 3], read: &dyn Fn([i32; 3]) -> Block) -> Option<Tree> {
    // gatherLogs: breadth-first through dx, dz in -1..=1, dy in 0..=1.
    let mut logs = Vec::new();
    let mut seen = HashSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some(pos) = queue.pop_front() {
        if !read(pos).log {
            continue;
        }
        logs.push(pos);
        if logs.len() > MAX_LOGS {
            return None;
        }
        for dy in 0..=1 {
            for dx in -1..=1 {
                for dz in -1..=1 {
                    let next = add(pos, [dx, dy, dz]);
                    if seen.insert(next) {
                        queue.push_back(next);
                    }
                }
            }
        }
    }
    if logs.is_empty() {
        return None;
    }
    // gatherLeavesAroundLog, for each log; a leaf counts where its distance is its depth.
    let mut leaves = HashSet::new();
    for &log in &logs {
        let mut visited = HashSet::new();
        let mut queue: VecDeque<([i32; 3], u32)> =
            SIDES.iter().map(|&d| (add(log, d), 1)).collect();
        while let Some((pos, depth)) = queue.pop_front() {
            if depth > MAX_LEAVES_RADIUS || visited.contains(&pos) {
                continue;
            }
            let b = read(pos);
            if b.distance.unwrap_or(0) != depth {
                continue;
            }
            visited.insert(pos);
            if b.leaves && !b.persistent {
                leaves.insert(pos);
                for d in SIDES {
                    let next = add(pos, d);
                    if !visited.contains(&next) {
                        queue.push_back((next, depth + 1));
                    }
                }
            }
        }
    }
    if leaves.is_empty() {
        return None;
    }
    // gatherAdjacentBlocks: vines (and the vines hanging below them), cocoa, bee nests.
    let mut attached = HashSet::new();
    for &pos in logs.iter().chain(leaves.iter()) {
        for d in SIDES {
            let mut next = add(pos, d);
            match read(next).name.as_str() {
                "minecraft:vine" => {
                    while read(next).name == "minecraft:vine" && attached.insert(next) {
                        next = add(next, [0, -1, 0]);
                    }
                }
                "minecraft:cocoa" | "minecraft:bee_nest" => {
                    attached.insert(next);
                }
                _ => {}
            }
        }
    }
    let mut leaves: Vec<_> = leaves.into_iter().collect();
    let mut attached: Vec<_> = attached.into_iter().collect();
    leaves.sort();
    attached.sort();
    logs.retain(|&p| p != start);
    Some(Tree {
        logs,
        leaves,
        attached,
    })
}

/// One felled tree, waiting for the end of the server tick.
pub struct Felling {
    pub level: LevelRef,
    pub player: PlayerRef,
    pub origin: [i32; 3],
    pub tree: Tree,
    /// The axe, as it was when the log was chopped: the tool every block's loot rolls with.
    pub axe: ItemStack,
    /// Survival: the axe wears and the player tires.
    pub survival: bool,
}

impl Felling {
    /// Destroys every block of the tree, attachments first and logs last, its drops landing at
    /// the stump. A block that doesn't break (already gone, unloaded) is skipped. Then the axe
    /// wears and the player tires, for the logs actually felled.
    fn run(&self, host: &mut dyn ServerHost) {
        let mut logs = 0;
        let blocks = self
            .tree
            .attached
            .iter()
            .chain(&self.tree.leaves)
            .map(|b| (b, false));
        for (&pos, is_log) in blocks.chain(self.tree.logs.iter().map(|b| (b, true))) {
            let Ok(Some(drops)) = self
                .level
                .destroy_block(host, pos, true, Some(&self.axe), true)
            else {
                continue;
            };
            for stack in drops {
                let _ = self.level.pop_resource(host, self.origin, stack);
            }
            logs += i32::from(is_log);
        }
        if self.survival && logs > 0 {
            let living = LivingRef::new(self.player.entity());
            let _ = living.hurt_and_break(host, EquipmentSlot::MainHand, logs);
            self.player.add_exhaustion(host, 0.005 * logs as f32);
        }
    }
}

#[derive(Default)]
struct Mod {
    queue: Mutex<Vec<Felling>>,
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

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn break_block(&self, host: &mut dyn ServerHost, event: &mut BreakBlockEvent<'_>) -> Flow {
        if !host.on("fallingtrees") {
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
        if player.entity().is_shift_key_down(host) == Some(true) {
            return Flow::Continue;
        }
        let Some((item, axe)) = LivingRef::new(player.entity())
            .with_hand_item(host, Hand::Main, |held| {
                Some((held.item().id(host)?.to_owned(), held.project()?))
            })
            .flatten()
        else {
            return Flow::Continue;
        };
        if !is_axe(&item) {
            return Flow::Continue;
        }
        let Some(level) = player.level(host) else {
            return Flow::Continue;
        };
        let read = |p: [i32; 3]| read_block(host, level, p);
        let Some(tree) = find_tree(event.pos, &read) else {
            return Flow::Continue;
        };
        let survival = matches!(mode, Some(GameMode::Survival));
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        queue.push(Felling {
            level,
            player,
            origin: event.pos,
            tree,
            axe,
            survival,
        });
        Flow::Continue
    }

    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        if !matches!(event.phase, ServerTickPhase::ServerTickEnd { .. }) {
            return;
        }
        let pending = std::mem::take(&mut *self.queue.lock().unwrap_or_else(|e| e.into_inner()));
        for felling in pending {
            felling.run(host);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn world(blocks: &[([i32; 3], &str, Option<u32>)]) -> HashMap<[i32; 3], Block> {
        blocks
            .iter()
            .map(|&(p, n, d)| (p, block(n, d, false)))
            .collect()
    }

    /// A block as the game reads it, its tags those vanilla gives the names used here.
    fn block(name: &str, distance: Option<u32>, persistent: bool) -> Block {
        Block {
            name: name.into(),
            log: name.ends_with("_log") || name.ends_with("_wood"),
            leaves: name.ends_with("_leaves"),
            distance,
            persistent,
        }
    }

    fn reader(w: &HashMap<[i32; 3], Block>) -> impl Fn([i32; 3]) -> Block + '_ {
        |p| {
            w.get(&p).cloned().unwrap_or(Block {
                name: "minecraft:air".into(),
                ..Block::default()
            })
        }
    }

    #[test]
    fn axes_by_name() {
        assert!(is_axe("minecraft:iron_axe") && !is_axe("minecraft:iron_pickaxe"));
    }

    #[test]
    fn a_small_tree_falls_whole() {
        let w = world(&[
            ([0, 0, 0], "minecraft:oak_log", None),
            ([0, 1, 0], "minecraft:oak_log", None),
            ([0, 2, 0], "minecraft:oak_log", None),
            ([1, 2, 0], "minecraft:oak_leaves", Some(1)),
            ([2, 2, 0], "minecraft:oak_leaves", Some(2)),
            ([0, 3, 0], "minecraft:oak_leaves", Some(1)),
            ([1, 1, 0], "minecraft:cocoa", None),
            ([2, 1, 0], "minecraft:vine", None),
            ([2, 0, 0], "minecraft:vine", None),
        ]);
        let t = find_tree([0, 0, 0], &reader(&w)).unwrap();
        assert_eq!(t.logs, vec![[0, 1, 0], [0, 2, 0]]);
        assert_eq!(t.leaves, vec![[0, 3, 0], [1, 2, 0], [2, 2, 0]]);
        assert_eq!(t.attached, vec![[1, 1, 0], [2, 0, 0], [2, 1, 0]]);
    }

    #[test]
    fn logs_below_and_other_trees_leaves_stay() {
        let w = world(&[
            ([0, -1, 0], "minecraft:oak_log", None),
            ([0, 0, 0], "minecraft:oak_log", None),
            ([0, 1, 0], "minecraft:oak_leaves", Some(1)),
            // Distance 1 from another log, two steps from ours: not ours.
            ([1, 1, 0], "minecraft:oak_leaves", Some(1)),
        ]);
        let t = find_tree([0, 0, 0], &reader(&w)).unwrap();
        assert!(t.logs.is_empty());
        assert_eq!(t.leaves, vec![[0, 1, 0]]);
    }

    #[test]
    fn no_leaves_or_persistent_leaves_is_no_tree() {
        let mut w = world(&[
            ([0, 0, 0], "minecraft:oak_log", None),
            ([0, 1, 0], "minecraft:oak_log", None),
        ]);
        assert_eq!(find_tree([0, 0, 0], &reader(&w)), None);
        w.insert([0, 2, 0], block("minecraft:oak_leaves", Some(1), true));
        assert_eq!(find_tree([0, 0, 0], &reader(&w)), None);
    }

    #[test]
    fn too_big_is_no_tree() {
        let mut w = world(&[([0, 300, 0], "minecraft:oak_leaves", Some(1))]);
        for y in 0..300 {
            w.insert([0, y, 0], block("minecraft:oak_log", None, false));
        }
        assert_eq!(find_tree([0, 0, 0], &reader(&w)), None);
    }
}
