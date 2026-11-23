//! The tree finders and the fall's arithmetic: pure, testable by `cargo test` alone.
//!
//! Falling Trees 0.13.2 (`trees/GenericTree`, `VerticalTree`, `ChorusTree`, `mushroom/*`,
//! `client/render/TreeRenderer`). A finder reads the world through a closure and answers the
//! blocks that come down with the chopped one.

use std::collections::{HashSet, VecDeque};

/// What a finder needs to know about a block.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Block {
    pub name: String,
    /// In the generic tree's log filter (`#minecraft:logs`).
    pub log: bool,
    /// In the generic tree's leaves filter (`#minecraft:leaves`).
    pub leaves: bool,
    /// In the vertical tree's block filter.
    pub vertical: bool,
    /// The leaves' `distance`, if the block has one.
    pub distance: Option<u32>,
    pub persistent: bool,
}

/// `TreeConfigs`' kinds of tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Generic,
    Vertical,
    Chorus,
    Mushroom,
}

/// `GenericTreeConfig.Algorithm`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Algorithm {
    pub max_leaves_radius: u32,
    pub max_log_amount: usize,
    pub ignore_persistent_leaves: bool,
}

impl Default for Algorithm {
    fn default() -> Self {
        Self {
            max_leaves_radius: 7,
            max_log_amount: 256,
            ignore_persistent_leaves: true,
        }
    }
}

/// A tree to take down.
#[derive(Debug, Default, PartialEq)]
pub struct Tree {
    /// The stems, the chopped block first.
    pub logs: Vec<[i32; 3]>,
    pub leaves: Vec<[i32; 3]>,
    /// Vines, cocoa and bee nests; chorus flowers; mushroom caps.
    pub attached: Vec<[i32; 3]>,
}

impl Tree {
    /// Every block of the tree, attachments first, then leaves, then stems.
    pub fn blocks(&self) -> impl Iterator<Item = [i32; 3]> + '_ {
        self.attached
            .iter()
            .chain(&self.leaves)
            .chain(&self.logs)
            .copied()
    }

    pub fn len(&self) -> usize {
        self.logs.len() + self.leaves.len() + self.attached.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

const SIDES: [[i32; 3]; 6] = [
    [1, 0, 0],
    [-1, 0, 0],
    [0, 1, 0],
    [0, -1, 0],
    [0, 0, 1],
    [0, 0, -1],
];
const HORIZONTAL: [[i32; 3]; 4] = [[0, 0, -1], [1, 0, 0], [0, 0, 1], [-1, 0, 0]];

fn add(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// Which kind of tree the block at a spot starts, in `TreeRegistry` order.
pub fn kind_of(block: &Block) -> Option<Kind> {
    if block.log {
        Some(Kind::Generic)
    } else if block.vertical {
        Some(Kind::Vertical)
    } else {
        match block.name.as_str() {
            "minecraft:chorus_plant" => Some(Kind::Chorus),
            "minecraft:mushroom_stem" => Some(Kind::Mushroom),
            _ => None,
        }
    }
}

/// `GenericTree.gatherTreeData`.
pub fn find_generic(
    start: [i32; 3],
    read: &dyn Fn([i32; 3]) -> Block,
    algorithm: Algorithm,
) -> Option<Tree> {
    // gatherLogs: breadth-first through dx, dz in -1..=1, dy in 0..=1.
    let mut logs = Vec::new();
    let mut seen = HashSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some(pos) = queue.pop_front() {
        if !read(pos).log {
            continue;
        }
        logs.push(pos);
        if logs.len() > algorithm.max_log_amount {
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
            if depth > algorithm.max_leaves_radius || visited.contains(&pos) {
                continue;
            }
            let b = read(pos);
            if b.distance.unwrap_or(0) != depth {
                continue;
            }
            visited.insert(pos);
            if b.leaves && !(algorithm.ignore_persistent_leaves && b.persistent) {
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
    // The chopped log first, the rest in the order found.
    Some(Tree {
        logs,
        leaves,
        attached,
    })
}

/// `VerticalTree.gatherBlocks`: the block and the same blocks stacked above it.
pub fn find_vertical(start: [i32; 3], read: &dyn Fn([i32; 3]) -> Block) -> Option<Tree> {
    let name = read(start).name;
    let mut logs = vec![start];
    let mut at = add(start, [0, 1, 0]);
    while read(at).name == name && logs.len() < 4096 {
        logs.push(at);
        at = add(at, [0, 1, 0]);
    }
    Some(Tree {
        logs,
        ..Tree::default()
    })
}

/// `ChorusTree.gatherBlocks`: the plants and the flowers they grow.
pub fn find_chorus(start: [i32; 3], read: &dyn Fn([i32; 3]) -> Block) -> Option<Tree> {
    let is_plant = |pos| read(pos).name == "minecraft:chorus_plant";
    let is_flower = |pos| read(pos).name == "minecraft:chorus_flower";
    let mut tree = Tree::default();
    let mut visited = HashSet::new();
    let mut queue = VecDeque::from([start]);
    while let Some(current) = queue.pop_front() {
        if !visited.insert(current) {
            continue;
        }
        if is_flower(current) {
            tree.attached.push(current);
        } else if is_plant(current) {
            tree.logs.push(current);
            let mut around = Vec::new();
            for d in HORIZONTAL {
                let next = add(current, d);
                if is_plant(add(next, [0, -1, 0])) {
                    continue;
                }
                if is_plant(next) || is_flower(next) {
                    around.push(next);
                }
            }
            let above = add(current, [0, 1, 0]);
            if is_plant(above) || is_flower(above) {
                around.push(above);
            }
            queue.extend(around.into_iter().filter(|p| !visited.contains(p)));
        }
    }
    Some(tree)
}

/// `RedMushroomTree` and `BrownMushroomTree`: the stem, and the cap blocks within six steps.
pub fn find_mushroom(start: [i32; 3], read: &dyn Fn([i32; 3]) -> Block) -> Option<Tree> {
    let mut stems = Vec::new();
    let mut at = start;
    while read(at).name == "minecraft:mushroom_stem" {
        stems.push(at);
        at = add(at, [0, 1, 0]);
    }
    for cap in [
        "minecraft:red_mushroom_block",
        "minecraft:brown_mushroom_block",
    ] {
        let mut caps = HashSet::new();
        for &stem in &stems {
            let mut visited = HashSet::new();
            let mut queue = VecDeque::from([(add(stem, [0, 1, 0]), 1)]);
            while let Some((current, distance)) = queue.pop_front() {
                if distance > 6 || !visited.insert(current) {
                    continue;
                }
                if read(current).name == cap {
                    caps.insert(current);
                    for d in HORIZONTAL {
                        let next = add(current, d);
                        if !visited.contains(&next) {
                            queue.push_back((next, distance + 1));
                        }
                    }
                }
            }
        }
        if !caps.is_empty() {
            let mut attached: Vec<_> = caps.into_iter().collect();
            attached.sort();
            return Some(Tree {
                logs: stems,
                attached,
                ..Tree::default()
            });
        }
    }
    None
}

/// The tree whose block at `start` is chopped, of the kind that block starts.
pub fn find(
    start: [i32; 3],
    read: &dyn Fn([i32; 3]) -> Block,
    algorithm: Algorithm,
) -> Option<(Kind, Tree)> {
    let kind = kind_of(&read(start))?;
    let tree = match kind {
        Kind::Generic => find_generic(start, read, algorithm),
        Kind::Vertical => find_vertical(start, read),
        Kind::Chorus => find_chorus(start, read),
        Kind::Mushroom => find_mushroom(start, read),
    }?;
    Some((kind, tree))
}

/// The dynamic mining speed's divisor for a tree of `blocks` blocks
/// (`miningSpeedModifier`: speed / (min(max, blocks - 1) * multiplication + 1)).
pub fn speed_divisor(blocks: usize, multiplication: f32, max_multiplication: f32) -> f32 {
    max_multiplication.min(blocks as f32 - 1.0) * multiplication + 1.0
}

/// `TreeRenderer`'s fall, in degrees below upright (negative while falling), for a tree that has
/// lived `life` seconds: the cosine bump of the fall plus the sine bump of the bounce.
pub fn fall_angle(life: f32, fall_length: f32, bounce_height: f32, bounce_length: f32) -> f32 {
    let half_pi = std::f32::consts::FRAC_PI_2;
    let time = life * half_pi / fall_length;
    let fall = bump_cos(time) * 90.0;
    let bounce = bump_sin((time - half_pi) / (bounce_length / (fall_length * 2.0))) * bounce_height;
    (fall + bounce) - 90.0
}

fn bump_cos(time: f32) -> f32 {
    time.clamp(-std::f32::consts::PI, std::f32::consts::PI)
        .cos()
        .max(0.0)
}

fn bump_sin(time: f32) -> f32 {
    time.clamp(-std::f32::consts::PI, std::f32::consts::PI)
        .sin()
        .max(0.0)
}

/// The horizontal direction a tree falls, away from the player: `Direction.fromYRot(
/// -atan2(player.x - origin.x, player.z - origin.z)).getOpposite()`, as an offset (dx, dz).
pub fn fall_direction(player: [f64; 2], origin: [i32; 2]) -> [i32; 2] {
    let dx = player[0] - f64::from(origin[0]);
    let dz = player[1] - f64::from(origin[1]);
    let yaw = -dx.atan2(dz).to_degrees();
    // Direction.fromYRot: 2D data value floor(yaw / 90 + 0.5) & 3 over south, west, north, east.
    let index = ((yaw / 90.0 + 0.5).floor() as i64).rem_euclid(4);
    let facing = [[0, 1], [-1, 0], [0, -1], [1, 0]][index as usize];
    [-facing[0], -facing[1]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn block(name: &str, distance: Option<u32>, persistent: bool) -> Block {
        Block {
            name: name.into(),
            log: name.ends_with("_log") || name.ends_with("_wood"),
            leaves: name.ends_with("_leaves"),
            vertical: name == "minecraft:cactus" || name == "minecraft:bamboo",
            distance,
            persistent,
        }
    }

    fn world(blocks: &[([i32; 3], &str, Option<u32>)]) -> HashMap<[i32; 3], Block> {
        blocks
            .iter()
            .map(|&(p, n, d)| (p, block(n, d, false)))
            .collect()
    }

    fn reader(w: &HashMap<[i32; 3], Block>) -> impl Fn([i32; 3]) -> Block + '_ {
        |p| {
            w.get(&p).cloned().unwrap_or(Block {
                name: "minecraft:air".into(),
                ..Block::default()
            })
        }
    }

    fn algorithm() -> Algorithm {
        Algorithm {
            max_leaves_radius: 10,
            ..Algorithm::default()
        }
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
        let (kind, t) = find([0, 0, 0], &reader(&w), algorithm()).unwrap();
        assert_eq!(kind, Kind::Generic);
        assert_eq!(t.logs[0], [0, 0, 0]);
        assert_eq!(t.logs.len(), 3);
        assert_eq!(t.leaves, vec![[0, 3, 0], [1, 2, 0], [2, 2, 0]]);
        assert_eq!(t.attached, vec![[1, 1, 0], [2, 0, 0], [2, 1, 0]]);
    }

    #[test]
    fn leaves_past_the_radius_and_persistent_leaves_stay() {
        let mut w = world(&[
            ([0, 0, 0], "minecraft:oak_log", None),
            ([0, 1, 0], "minecraft:oak_leaves", Some(1)),
            ([0, 2, 0], "minecraft:oak_leaves", Some(2)),
        ]);
        let near = Algorithm {
            max_leaves_radius: 1,
            ..Algorithm::default()
        };
        assert_eq!(
            find_generic([0, 0, 0], &reader(&w), near).unwrap().leaves,
            vec![[0, 1, 0]]
        );
        w.insert([0, 1, 0], block("minecraft:oak_leaves", Some(1), true));
        assert_eq!(find_generic([0, 0, 0], &reader(&w), algorithm()), None);
        let all = Algorithm {
            ignore_persistent_leaves: false,
            ..algorithm()
        };
        assert!(find_generic([0, 0, 0], &reader(&w), all).is_some());
    }

    #[test]
    fn too_many_logs_is_no_tree() {
        let mut w = world(&[([0, 300, 0], "minecraft:oak_leaves", Some(1))]);
        for y in 0..300 {
            w.insert([0, y, 0], block("minecraft:oak_log", None, false));
        }
        assert_eq!(find_generic([0, 0, 0], &reader(&w), algorithm()), None);
        let big = Algorithm {
            max_log_amount: 400,
            ..algorithm()
        };
        assert!(find_generic([0, 0, 0], &reader(&w), big).is_some());
    }

    #[test]
    fn a_cactus_comes_down_from_the_chopped_block_up() {
        let w = world(&[
            ([0, 0, 0], "minecraft:cactus", None),
            ([0, 1, 0], "minecraft:cactus", None),
            ([0, 2, 0], "minecraft:cactus", None),
            ([0, 3, 0], "minecraft:bamboo", None),
        ]);
        let (kind, t) = find([0, 1, 0], &reader(&w), algorithm()).unwrap();
        assert_eq!(kind, Kind::Vertical);
        assert_eq!(t.logs, vec![[0, 1, 0], [0, 2, 0]]);
    }

    #[test]
    fn a_chorus_tree_takes_its_plants_and_flowers() {
        let w = world(&[
            ([0, 0, 0], "minecraft:chorus_plant", None),
            ([0, 1, 0], "minecraft:chorus_plant", None),
            ([1, 1, 0], "minecraft:chorus_plant", None),
            ([0, 2, 0], "minecraft:chorus_flower", None),
            ([1, 2, 0], "minecraft:chorus_flower", None),
        ]);
        let (kind, t) = find([0, 0, 0], &reader(&w), algorithm()).unwrap();
        assert_eq!(kind, Kind::Chorus);
        assert_eq!(t.logs.len(), 3);
        assert_eq!(t.attached.len(), 2);
    }

    #[test]
    fn a_mushroom_needs_a_cap() {
        let mut w = world(&[
            ([0, 0, 0], "minecraft:mushroom_stem", None),
            ([0, 1, 0], "minecraft:mushroom_stem", None),
        ]);
        assert_eq!(find([0, 0, 0], &reader(&w), algorithm()), None);
        w.insert(
            [0, 2, 0],
            block("minecraft:red_mushroom_block", None, false),
        );
        w.insert(
            [1, 2, 0],
            block("minecraft:red_mushroom_block", None, false),
        );
        let (kind, t) = find([0, 0, 0], &reader(&w), algorithm()).unwrap();
        assert_eq!(kind, Kind::Mushroom);
        assert_eq!(t.logs, vec![[0, 0, 0], [0, 1, 0]]);
        assert_eq!(t.attached, vec![[0, 2, 0], [1, 2, 0]]);
    }

    #[test]
    fn bigger_trees_mine_slower_up_to_a_cap() {
        assert_eq!(speed_divisor(1, 0.5, 16.0), 1.0);
        assert_eq!(speed_divisor(5, 0.5, 16.0), 3.0);
        assert_eq!(speed_divisor(200, 0.5, 16.0), 9.0);
    }

    #[test]
    fn the_fall_goes_from_upright_to_flat_and_bounces() {
        // At the start: upright (angle 0). At the fall's length: flat (-90). The bounce lifts it after.
        assert!((fall_angle(0.0, 2.5, 10.0, 1.0)).abs() < 1e-3);
        assert!((fall_angle(2.5, 2.5, 10.0, 1.0) + 90.0).abs() < 1.0);
        let mid_bounce = fall_angle(2.5 + 0.5 * 2.5 / 2.5, 2.5, 10.0, 1.0);
        assert!(mid_bounce > -90.0);
    }

    #[test]
    fn the_tree_falls_away_from_the_player() {
        // Player south of the tree (z larger): the tree falls north.
        assert_eq!(fall_direction([0.5, 5.5], [0, 0]), [0, -1]);
        assert_eq!(fall_direction([0.5, -5.5], [0, 0]), [0, 1]);
        assert_eq!(fall_direction([5.5, 0.5], [0, 0]), [-1, 0]);
        assert_eq!(fall_direction([-5.5, 0.5], [0, 0]), [1, 0]);
    }
}
