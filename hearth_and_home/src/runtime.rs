// Hearth & Home 2.0.3 (Starfish Studios) for Horizon: the block rules of the content mod mods/hearth_and_home
// (tools/hearth_and_home_port.py), from the decompiled `com.starfish_studios.hearth_and_home.block` classes:
//
// - ChimneyBlock: `top` while no chimney stands on it, lit when placed (out when placed in water), and a
//   chimney under another takes its `lit`;
// - LatticeBlock: faces away from the placer, or along a lattice clicked from the side (not while sneaking);
//   `horizontal` single/left/middle/right from the lattices beside it;
// - ColumnBlock: `axis` from the clicked face, `type` none/bottom/middle/top from the same column along it;
// - RotatingSlabBlock (the tile slabs): a slab that faces away from the placer;
// - HNHBlocks.registerFlammables: ignite 5, burn 20 for the wooden blocks, thatch, bamboo and shoji.
//
// The chimneys' smoke and their shovel and igniter clicks are in src/lib.rs (mod API). Waterlogging isn't
// ported: the lattices and chimneys hold no water here.
//
// Injected as `crate::game::hearth_and_home` (native-next/hearth_and_home, a `[[native.module]]`); the level
// helpers follow native-next/beachparty's.
use super::*;

const NS: &str = "hearth_and_home:";

fn short(name: &str) -> Option<&str> {
    name.strip_prefix(NS)
}
fn step(dir: &str) -> (i32, i32) {
    match dir {
        "north" => (0, -1),
        "south" => (0, 1),
        "west" => (-1, 0),
        _ => (1, 0),
    }
}
fn opposite(dir: &str) -> &'static str {
    match dir {
        "north" => "south",
        "south" => "north",
        "west" => "east",
        _ => "west",
    }
}
fn clockwise(dir: &str) -> &'static str {
    match dir {
        "north" => "east",
        "east" => "south",
        "south" => "west",
        _ => "north",
    }
}
fn counter_clockwise(dir: &str) -> &'static str {
    opposite(clockwise(dir))
}
/// `Direction.fromYRot`: the placer's horizontal facing.
fn horizontal(yaw: f32) -> &'static str {
    match ((yaw / 90.0 + 0.5).floor() as i32) & 3 {
        0 => "south",
        1 => "west",
        2 => "north",
        _ => "east",
    }
}
fn dir_name(d: Direction) -> &'static str {
    match d {
        Direction::North => "north",
        Direction::South => "south",
        Direction::West => "west",
        Direction::East => "east",
        Direction::Up => "up",
        Direction::Down => "down",
    }
}
fn axis_of(d: Direction) -> &'static str {
    match d {
        Direction::West | Direction::East => "x",
        Direction::Up | Direction::Down => "y",
        Direction::North | Direction::South => "z",
    }
}

fn lname<'b>(b: &'b Behaviours, state: BlockStateId) -> &'b str {
    b.table.name(state)
}
fn ltext(b: &Behaviours, state: BlockStateId, k: &str) -> Option<String> {
    let key = b.table.key_of(k)?;
    Some(b.table.text(state, key).to_string())
}
fn lwith_text(b: &Behaviours, state: BlockStateId, k: &str, v: &str) -> Option<BlockStateId> {
    b.table.with_text(state, b.table.key_of(k)?, v)
}
fn off(p: BlockPos, dx: i32, dy: i32, dz: i32) -> BlockPos {
    BlockPos::new(p.x + dx, p.y + dy, p.z + dz)
}
fn off_dir(p: BlockPos, dir: &str) -> BlockPos {
    let (dx, dz) = step(dir);
    off(p, dx, 0, dz)
}

fn is_chimney(n: &str) -> bool {
    n.ends_with("_chimney")
}
fn is_lattice(n: &str) -> bool {
    n.ends_with("_lattice")
}
fn is_column(n: &str) -> bool {
    n.ends_with("_column") || n == "shoji"
}
fn is_tile_slab(n: &str) -> bool {
    n.ends_with("tile_slab")
}

/// The state at `p`, or `changed` when that is the neighbour being updated.
fn read(level: &GameLevel<'_>, p: BlockPos, changed: Option<(BlockPos, BlockStateId)>) -> BlockStateId {
    match changed {
        Some((cp, cs)) if cp == p => cs,
        _ => level.block_state(p),
    }
}

/// `LatticeBlock.getConnection`: a lattice clockwise of the facing (facing the same way or clockwise) is
/// on the left, one counter-clockwise (the same way or counter-clockwise) on the right.
fn lattice_type(level: &GameLevel<'_>, pos: BlockPos, state: BlockStateId, changed: Option<(BlockPos, BlockStateId)>) -> &'static str {
    let b = level.behaviours;
    let facing = ltext(b, state, "facing").unwrap_or_else(|| "north".into());
    let side = |dir: &str, turned: &str| {
        let s = read(level, off_dir(pos, dir), changed);
        short(lname(b, s)).is_some_and(is_lattice)
            && ltext(b, s, "facing").is_some_and(|f| f == facing || f == turned)
    };
    let l = side(clockwise(&facing), clockwise(&facing));
    let r = side(counter_clockwise(&facing), counter_clockwise(&facing));
    match (l, r) {
        (true, true) => "middle",
        (false, true) => "left",
        (true, false) => "right",
        _ => "single",
    }
}

/// `ColumnBlock.getType`: the same block with the same axis above (along the axis) and below.
fn column_type(level: &GameLevel<'_>, pos: BlockPos, state: BlockStateId, changed: Option<(BlockPos, BlockStateId)>) -> &'static str {
    let b = level.behaviours;
    let axis = ltext(b, state, "axis").unwrap_or_else(|| "y".into());
    let (dx, dy, dz) = match axis.as_str() {
        "x" => (1, 0, 0),
        "z" => (0, 0, 1),
        _ => (0, 1, 0),
    };
    let same = |p: BlockPos| {
        let s = read(level, p, changed);
        lname(b, s) == lname(b, state) && ltext(b, s, "axis").as_deref() == Some(axis.as_str())
    };
    match (same(off(pos, dx, dy, dz)), same(off(pos, -dx, -dy, -dz))) {
        (true, false) => "bottom",
        (false, true) => "top",
        (true, true) => "middle",
        _ => "none",
    }
}

/// Whether the position holds a water source (the placement's fluid).
fn water_at(level: &GameLevel<'_>, p: BlockPos) -> bool {
    let b = level.behaviours;
    let s = level.block_state(p);
    lname(b, s) == "minecraft:water" && ltext(b, s, "level").as_deref() == Some("0")
}

/// `getStateForPlacement`.
pub(crate) fn placement(level: &GameLevel<'_>, state: BlockStateId, context: &PlacementContext, _fluid: FluidState) -> Option<Option<BlockStateId>> {
    let b = level.behaviours;
    let n = short(lname(b, state))?;
    let p = context.pos;
    let facing = opposite(horizontal(context.yaw));
    if is_chimney(n) {
        let above = level.block_state(off(p, 0, 1, 0));
        let top = !short(lname(b, above)).is_some_and(is_chimney);
        let s = lwith_text(b, state, "top", if top { "true" } else { "false" })?;
        return Some(lwith_text(b, s, "lit", if water_at(level, p) { "false" } else { "true" }));
    }
    if is_lattice(n) {
        let mut f = facing;
        // Clicking a lattice's side (not its front or back) while standing: along that lattice.
        let face = dir_name(context.face);
        let clicked = level.block_state(context.hit_pos);
        if !context.sneaking && short(lname(b, clicked)).is_some_and(is_lattice)
            && let Some(cf) = ltext(b, clicked, "facing")
            && cf != face && opposite(&cf) != face
        {
            f = match cf.as_str() {
                "north" => "north",
                "south" => "south",
                "west" => "west",
                _ => "east",
            };
        }
        let s = lwith_text(b, state, "facing", f)?;
        return Some(lwith_text(b, s, "horizontal", lattice_type(level, p, s, None)));
    }
    if is_column(n) {
        let s = lwith_text(b, state, "axis", axis_of(context.face))?;
        return Some(lwith_text(b, s, "type", column_type(level, p, s, None)));
    }
    if is_tile_slab(n) {
        // SlabBlock.getStateForPlacement, then the facing (set again when a second slab makes it double).
        let here = level.block_state(p);
        let s = if lname(b, here) == lname(b, state) {
            lwith_text(b, lwith_text(b, here, "type", "double")?, "waterlogged", "false")?
        } else {
            let upper = match context.face {
                Direction::Down => true,
                Direction::Up => false,
                _ => context.hit.y - f64::from(p.y) > 0.5,
            };
            let s = lwith_text(b, state, "type", if upper { "top" } else { "bottom" })?;
            lwith_text(b, s, "waterlogged", if water_at(level, p) { "true" } else { "false" })?
        };
        return Some(lwith_text(b, s, "facing", facing));
    }
    None
}

/// `updateShape` / `neighborChanged`.
pub(crate) fn update_shape(level: &GameLevel<'_>, pos: BlockPos, state: BlockStateId, direction: Direction, neighbour_pos: BlockPos, neighbour: BlockStateId) -> Option<BlockStateId> {
    let b = level.behaviours;
    let n = short(lname(b, state))?;
    let changed = Some((neighbour_pos, neighbour));
    if is_chimney(n) {
        if direction != Direction::Up {
            return None;
        }
        let above_chimney = short(lname(b, neighbour)).is_some_and(is_chimney);
        let mut s = lwith_text(b, state, "top", if above_chimney { "false" } else { "true" })?;
        if above_chimney {
            let lit = ltext(b, neighbour, "lit").unwrap_or_else(|| "false".into());
            s = lwith_text(b, s, "lit", &lit)?;
        }
        return Some(s);
    }
    if is_lattice(n) {
        if matches!(direction, Direction::Up | Direction::Down) {
            return None;
        }
        return lwith_text(b, state, "horizontal", lattice_type(level, pos, state, changed));
    }
    if is_column(n) {
        let axis = ltext(b, state, "axis").unwrap_or_else(|| "y".into());
        if axis_of(direction) != axis {
            return None;
        }
        return lwith_text(b, state, "type", column_type(level, pos, state, changed));
    }
    None
}

/// HNHBlocks.registerFlammables: (ignite 5, burn 20).
pub(crate) fn fire_odds(level: &GameLevel<'_>, state: BlockStateId) -> Option<(i32, i32)> {
    let n = short(lname(level.behaviours, state))?;
    let wooden = ["oak", "spruce", "birch", "jungle", "acacia", "dark_oak", "mangrove", "cherry", "bamboo"]
        .iter()
        .any(|w| n.strip_prefix(w).and_then(|r| r.strip_prefix('_')).is_some_and(|r| {
            matches!(r, "parquet" | "trim" | "vertical_trim" | "sanded_wood" | "lattice")
        }));
    let other = matches!(n, "thatch" | "thatch_slab" | "thatch_stairs" | "woven_bamboo" | "bamboo_mat" | "shoji" | "shoji_panel");
    (wooden || other).then_some((5, 20))
}
