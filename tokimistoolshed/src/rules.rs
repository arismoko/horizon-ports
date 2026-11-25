//! The toolshed's rules with no game in them: which block an area tool takes, the 3x3 in the
//! plane of the face that was hit, the lumber axe's tree, the copper bucket's wear and the
//! abacus's counts. `lib.rs` reads the world and calls these.

/// The tools whose break or use reaches past the block they were aimed at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Excavator,
    Hammer,
    Scythe,
    LumberAxe,
    Clippers,
}

pub const NS: &str = "tokimistoolshed:";
pub const COPPER_BUCKET: &str = "tokimistoolshed:copper_bucket";
pub const COPPER_MILK_BUCKET: &str = "tokimistoolshed:copper_milk_bucket";
pub const ABACUS: &str = "tokimistoolshed:abacus";

/// `CopperBucketItem`'s `maxDamage`: sixteen uses.
pub const BUCKET_USES: u32 = 16;
/// `LumberAxeItem.MAX_TREE_SIZE` and `MIN_LEAVES_REQUIRED`.
pub const MAX_TREE: usize = 256;
pub const MIN_LEAVES: usize = 3;
/// `AbacusHudRenderer.MAX_AXIS_LENGTH`.
pub const MAX_AXIS: i32 = 128;
/// The y an unbound abacus keeps (`AbacusItem`'s `boundPosY` of -999).
pub const UNBOUND: i32 = -999;
/// The face of a block that was not hit first (`LastBreakData.getFace`): north.
pub const NORTH: [i32; 3] = [0, 0, -1];

/// The area tool an item id names.
pub fn kind(item: &str) -> Option<Kind> {
    let name = item.strip_prefix(NS)?;
    Some(if name == "clippers" {
        Kind::Clippers
    } else if name.ends_with("_excavator") {
        Kind::Excavator
    } else if name.ends_with("_hammer") {
        Kind::Hammer
    } else if name.ends_with("_scythe") {
        Kind::Scythe
    } else if name.ends_with("_lumber_axe") {
        Kind::LumberAxe
    } else {
        return None;
    })
}

/// The block tag a kind's own blocks are in (`None`: the kind has its own test).
pub fn mineable_tag(kind: Kind) -> Option<&'static str> {
    match kind {
        Kind::Excavator => Some("minecraft:mineable/shovel"),
        Kind::Hammer => Some("minecraft:mineable/pickaxe"),
        Kind::Scythe => Some("minecraft:mineable/hoe"),
        Kind::LumberAxe => Some("minecraft:logs"),
        Kind::Clippers => None,
    }
}

/// Vegetation the scythe and the clippers take besides what their tags list. Stands for
/// `instanceof PlantBlock` (VegetationBlock), which the mod API cannot ask.
pub const VEGETATION: [&str; 5] = [
    "minecraft:short_grass",
    "minecraft:tall_grass",
    "minecraft:fern",
    "minecraft:large_fern",
    "minecraft:dead_bush",
];

/// Tags whose blocks are vegetation too.
pub const VEGETATION_TAGS: [&str; 3] =
    ["minecraft:flowers", "minecraft:saplings", "minecraft:crops"];

const CLIPPABLE: [&str; 7] = [
    "minecraft:cobweb",
    "minecraft:moss_block",
    "minecraft:moss_carpet",
    "minecraft:vine",
    "minecraft:glow_lichen",
    "minecraft:hanging_roots",
    "minecraft:nether_sprouts",
];

/// `ClippersItem.isClippable`: leaves, wool, a plant that is not a crop, and the listed blocks.
/// `in_tag` answers whether the block is in a block tag.
pub fn clippable(name: &str, in_tag: &dyn Fn(&str) -> bool) -> bool {
    in_tag("minecraft:leaves")
        || in_tag("minecraft:wool")
        || CLIPPABLE.contains(&name)
        || VEGETATION.contains(&name)
        || (VEGETATION_TAGS
            .iter()
            .filter(|t| **t != "minecraft:crops")
            .any(|t| in_tag(t)))
}

/// Whether `kind` takes the block `name` (the block `postMine` starts from, and each one of the
/// 3x3 after it).
pub fn takes(kind: Kind, name: &str, in_tag: &dyn Fn(&str) -> bool) -> bool {
    match kind {
        Kind::Clippers => clippable(name, in_tag),
        Kind::Scythe => {
            in_tag("minecraft:mineable/hoe")
                || VEGETATION.contains(&name)
                || VEGETATION_TAGS.iter().any(|t| in_tag(t))
        }
        _ => mineable_tag(kind).is_some_and(in_tag),
    }
}

/// `HammerItem.getOffsetPos`: the block `a` and `b` away from `origin` in the plane of `face`.
pub fn offset(origin: [i32; 3], face: [i32; 3], a: i32, b: i32) -> [i32; 3] {
    match face {
        [0, _, 0] => [origin[0] + a, origin[1], origin[2] + b],
        [0, 0, _] => [origin[0] + a, origin[1] + b, origin[2]],
        _ => [origin[0], origin[1] + b, origin[2] + a],
    }
}

/// The eight blocks around `origin` in the plane of `face`.
pub fn around(origin: [i32; 3], face: [i32; 3]) -> Vec<[i32; 3]> {
    let mut out = Vec::with_capacity(8);
    for a in -1..=1 {
        for b in -1..=1 {
            if a != 0 || b != 0 {
                out.push(offset(origin, face, a, b));
            }
        }
    }
    out
}

/// What the lumber axe's tree search asks of one block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Probe {
    /// The same log as the broken one.
    pub same_log: bool,
    /// `LumberAxeItem.isNaturalBlock`: air, fluid, a log, leaves, dirt, sand, snow, a beehive,
    /// fire, a plant or something replaceable.
    pub natural: bool,
    /// Leaves that are not persistent.
    pub leaf: bool,
}

pub const SIDES: [[i32; 3]; 6] = [
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];

pub fn add(p: [i32; 3], d: [i32; 3]) -> [i32; 3] {
    [p[0] + d[0], p[1] + d[1], p[2] + d[2]]
}

fn neighbours26(p: [i32; 3]) -> impl Iterator<Item = [i32; 3]> {
    (-1..=1).flat_map(move |dx| {
        (-1..=1).flat_map(move |dy| {
            (-1..=1).filter_map(move |dz| {
                (dx != 0 || dy != 0 || dz != 0).then_some([p[0] + dx, p[1] + dy, p[2] + dz])
            })
        })
    })
}

/// `LumberAxeItem.detectTree`: the logs of the broken one's kind joined to it at or above it,
/// none beside a block a player could have put there, at most [`MAX_TREE`], and at least
/// [`MIN_LEAVES`] non-persistent leaves by the top two layers. Empty when it is not a tree.
pub fn detect(start: [i32; 3], read: &dyn Fn([i32; 3]) -> Probe) -> Vec<[i32; 3]> {
    use std::collections::{HashSet, VecDeque};
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();
    for n in neighbours26(start) {
        if n[1] >= start[1] && read(n).same_log {
            queue.push_back(n);
            visited.insert(n);
        }
    }
    visited.insert(start);
    let mut logs = Vec::new();
    while let Some(cur) = queue.pop_front() {
        if !read(cur).same_log {
            continue;
        }
        if SIDES.iter().any(|d| !read(add(cur, *d)).natural) {
            return Vec::new();
        }
        logs.push(cur);
        if logs.len() > MAX_TREE {
            return Vec::new();
        }
        for n in neighbours26(cur) {
            if n[1] >= start[1] && visited.insert(n) && read(n).same_log {
                queue.push_back(n);
            }
        }
    }
    let Some(top) = logs.iter().map(|p| p[1]).max() else {
        return Vec::new();
    };
    let mut leaves = 0;
    for p in logs.iter().filter(|p| p[1] >= top - 1) {
        for d in SIDES {
            if read(add(*p, d)).leaf {
                leaves += 1;
                if leaves >= MIN_LEAVES {
                    return logs;
                }
            }
        }
    }
    Vec::new()
}

/// The wear a copper bucket has after one more use: `None` when that wears it out.
pub fn worn(damage: u32) -> Option<u32> {
    let next = damage + 1;
    (next < BUCKET_USES).then_some(next)
}

/// The abacus's line over the hotbar: the blocks counted along each axis that differs from the
/// bound block (at most [`MAX_AXIS`]), `1` when the aimed block is the bound one.
pub fn abacus_line(bound: [i32; 3], aimed: [i32; 3]) -> String {
    let parts: Vec<String> = ["X", "Y", "Z"]
        .iter()
        .zip((0..3).map(|i| (aimed[i] - bound[i]).abs().min(MAX_AXIS)))
        .filter(|(_, v)| *v > 0)
        .map(|(axis, v)| format!("{axis} {}", v + 1))
        .collect();
    if parts.is_empty() {
        "1".to_owned()
    } else {
        parts.join("   ")
    }
}

/// What a use of the abacus on a block does: unbind when it is bound, else bind to the block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AbacusUse {
    Unbind,
    Bind([i32; 3]),
}

pub fn abacus_use(bound_y: Option<i32>, clicked: [i32; 3]) -> AbacusUse {
    if bound_y.is_some_and(|y| y != UNBOUND) {
        AbacusUse::Unbind
    } else {
        AbacusUse::Bind(clicked)
    }
}

pub fn abacus_message(action: &AbacusUse) -> String {
    match action {
        AbacusUse::Unbind => "\u{a7}6Abacus unbound".to_owned(),
        AbacusUse::Bind(p) => format!("\u{a7}6Abacus bound to {}, {}, {}", p[0], p[1], p[2]),
    }
}

/// Whether `name` is a cow the copper bucket milks (`CowMilkMixin`: a cow or a mooshroom).
pub fn milkable(entity_type: &str) -> bool {
    matches!(entity_type, "minecraft:cow" | "minecraft:mooshroom")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(set: &'static [&'static str]) -> impl Fn(&str) -> bool {
        move |t| set.contains(&t)
    }

    #[test]
    fn items_name_their_kind() {
        assert_eq!(kind("tokimistoolshed:diamond_hammer"), Some(Kind::Hammer));
        assert_eq!(
            kind("tokimistoolshed:wooden_excavator"),
            Some(Kind::Excavator)
        );
        assert_eq!(kind("tokimistoolshed:iron_scythe"), Some(Kind::Scythe));
        assert_eq!(
            kind("tokimistoolshed:obsidian_lumber_axe"),
            Some(Kind::LumberAxe)
        );
        assert_eq!(kind("tokimistoolshed:clippers"), Some(Kind::Clippers));
        assert_eq!(kind("tokimistoolshed:trowel"), None);
        assert_eq!(kind("minecraft:iron_hammer"), None);
    }

    #[test]
    fn the_nine_are_in_the_plane_of_the_face() {
        let up = around([10, 64, 10], [0, 1, 0]);
        assert_eq!(up.len(), 8);
        assert!(up.iter().all(|p| p[1] == 64));
        let north = around([10, 64, 10], [0, 0, -1]);
        assert!(north.iter().all(|p| p[2] == 10));
        assert!(north.contains(&[9, 63, 10]) && north.contains(&[11, 65, 10]));
        let east = around([10, 64, 10], [1, 0, 0]);
        assert!(east.iter().all(|p| p[0] == 10));
        assert!(east.contains(&[10, 63, 9]) && east.contains(&[10, 65, 11]));
        assert!(!up.contains(&[10, 64, 10]));
    }

    #[test]
    fn each_kind_takes_its_own_blocks() {
        let pick = tags(&["minecraft:mineable/pickaxe"]);
        assert!(takes(Kind::Hammer, "minecraft:stone", &pick));
        assert!(!takes(Kind::Excavator, "minecraft:stone", &pick));
        let leaves = tags(&["minecraft:leaves"]);
        assert!(takes(Kind::Clippers, "minecraft:oak_leaves", &leaves));
        assert!(takes(Kind::Clippers, "minecraft:cobweb", &tags(&[])));
        assert!(!takes(Kind::Clippers, "minecraft:stone", &tags(&[])));
        // A crop is for the scythe, not the clippers.
        let crops = tags(&["minecraft:crops"]);
        assert!(takes(Kind::Scythe, "minecraft:wheat", &crops));
        assert!(!takes(Kind::Clippers, "minecraft:wheat", &crops));
        assert!(takes(Kind::Scythe, "minecraft:short_grass", &tags(&[])));
    }

    fn tree(logs: &[[i32; 3]], leaves: &[[i32; 3]]) -> impl Fn([i32; 3]) -> Probe {
        let logs = logs.to_vec();
        let leaves = leaves.to_vec();
        move |p| Probe {
            same_log: logs.contains(&p),
            natural: p[1] >= 0,
            leaf: leaves.contains(&p),
        }
    }

    #[test]
    fn a_tree_with_leaves_is_found_from_above_the_start() {
        // The broken log is [0,0,0]; the trunk goes up to y=4, leaves beside the top.
        let logs: Vec<[i32; 3]> = (1..=4).map(|y| [0, y, 0]).collect();
        let leaves = [[1, 4, 0], [-1, 4, 0], [0, 4, 1], [0, 3, -1]];
        let found = detect([0, 0, 0], &tree(&logs, &leaves));
        assert_eq!(found.len(), 4);
    }

    #[test]
    fn few_leaves_or_a_player_block_beside_it_is_not_a_tree() {
        let logs: Vec<[i32; 3]> = (1..=4).map(|y| [0, y, 0]).collect();
        assert!(detect([0, 0, 0], &tree(&logs, &[[1, 4, 0], [-1, 4, 0]])).is_empty());
        let leaves = [[1, 4, 0], [-1, 4, 0], [0, 4, 1]];
        let read = |p: [i32; 3]| {
            let mut probe = tree(&logs, &leaves)(p);
            if p == [1, 2, 0] {
                probe.natural = false;
            }
            probe
        };
        assert!(detect([0, 0, 0], &read).is_empty());
    }

    #[test]
    fn logs_below_the_start_are_left() {
        let logs = [[0, -1, 0], [0, 1, 0], [0, 2, 0]];
        let leaves = [[1, 2, 0], [-1, 2, 0], [0, 2, 1]];
        let found = detect([0, 0, 0], &tree(&logs, &leaves));
        assert_eq!(found, vec![[0, 1, 0], [0, 2, 0]]);
    }

    #[test]
    fn a_tree_over_the_size_limit_is_refused() {
        let logs: Vec<[i32; 3]> = (1..=(MAX_TREE as i32 + 2)).map(|y| [0, y, 0]).collect();
        let top = logs.last().copied().unwrap();
        let leaves = [
            [top[0] + 1, top[1], top[2]],
            [top[0] - 1, top[1], top[2]],
            [top[0], top[1], top[2] + 1],
        ];
        assert!(detect([0, 0, 0], &tree(&logs, &leaves)).is_empty());
    }

    #[test]
    fn the_bucket_wears_out_on_its_sixteenth_use() {
        assert_eq!(worn(0), Some(1));
        assert_eq!(worn(14), Some(15));
        assert_eq!(worn(15), None);
    }

    #[test]
    fn the_abacus_counts_each_differing_axis() {
        assert_eq!(abacus_line([0, 0, 0], [0, 0, 0]), "1");
        assert_eq!(abacus_line([0, 0, 0], [3, 0, -4]), "X 4   Z 5");
        assert_eq!(abacus_line([0, 0, 0], [0, 500, 0]), "Y 129");
    }

    #[test]
    fn the_abacus_binds_then_unbinds() {
        assert_eq!(abacus_use(None, [1, 2, 3]), AbacusUse::Bind([1, 2, 3]));
        assert_eq!(
            abacus_use(Some(UNBOUND), [1, 2, 3]),
            AbacusUse::Bind([1, 2, 3])
        );
        assert_eq!(abacus_use(Some(70), [1, 2, 3]), AbacusUse::Unbind);
        assert_eq!(
            abacus_message(&AbacusUse::Bind([1, 2, 3])),
            "\u{a7}6Abacus bound to 1, 2, 3"
        );
    }

    #[test]
    fn only_cows_are_milked() {
        assert!(milkable("minecraft:cow") && milkable("minecraft:mooshroom"));
        assert!(!milkable("minecraft:goat"));
    }
}
