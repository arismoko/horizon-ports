//! Pure geometry and decisions behind Cut Through: whether a segment is blocked by
//! collision shapes, and whether an entity beats the block the vanilla pick chose.

/// Most cells a segment walk visits. An entity candidate is at most a reach away
/// (a few blocks), so this only bounds malformed input.
const MAX_CELLS: usize = 512;

/// The distance along `from -> to` at which the segment first touches a box given in
/// block-local coordinates of the cell `cell` (`[min x, min y, min z, max x, max y, max z]`).
/// A segment that starts inside the box touches it at 0. None for a miss or non-finite input.
pub fn segment_box_distance(
    from: [f64; 3],
    to: [f64; 3],
    cell: [i32; 3],
    local: [f64; 6],
) -> Option<f64> {
    let delta = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let length = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
    if !length.is_finite() || local.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let mut t_min = 0.0_f64;
    let mut t_max = 1.0_f64;
    for axis in 0..3 {
        let lo = f64::from(cell[axis]) + local[axis];
        let hi = f64::from(cell[axis]) + local[axis + 3];
        if delta[axis].abs() < 1.0e-12 {
            if from[axis] < lo || from[axis] > hi {
                return None;
            }
        } else {
            let a = (lo - from[axis]) / delta[axis];
            let b = (hi - from[axis]) / delta[axis];
            let (near, far) = if a < b { (a, b) } else { (b, a) };
            t_min = t_min.max(near);
            t_max = t_max.min(far);
            if t_min > t_max {
                return None;
            }
        }
    }
    Some(t_min * length)
}

/// The block cells a segment passes through, in order from `from` (voxel traversal).
/// Empty for non-finite input; capped at [`MAX_CELLS`].
pub fn segment_cells(from: [f64; 3], to: [f64; 3]) -> Vec<[i32; 3]> {
    if from.iter().chain(to.iter()).any(|v| !v.is_finite()) {
        return Vec::new();
    }
    // Segments this far out cannot be a pick; the cap would cut them anyway.
    if from.iter().chain(to.iter()).any(|v| v.abs() > 3.0e7) {
        return Vec::new();
    }
    let delta = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let mut cell = [
        from[0].floor() as i32,
        from[1].floor() as i32,
        from[2].floor() as i32,
    ];
    let end = [
        to[0].floor() as i32,
        to[1].floor() as i32,
        to[2].floor() as i32,
    ];
    let mut step = [0i32; 3];
    let mut t_next = [f64::INFINITY; 3];
    let mut t_step = [f64::INFINITY; 3];
    for axis in 0..3 {
        if delta[axis] > 0.0 {
            step[axis] = 1;
            t_next[axis] = (f64::from(cell[axis]) + 1.0 - from[axis]) / delta[axis];
            t_step[axis] = 1.0 / delta[axis];
        } else if delta[axis] < 0.0 {
            step[axis] = -1;
            t_next[axis] = (from[axis] - f64::from(cell[axis])) / -delta[axis];
            t_step[axis] = 1.0 / -delta[axis];
        }
    }
    let mut cells = vec![cell];
    while cell != end && cells.len() < MAX_CELLS {
        let axis = if t_next[0] <= t_next[1] && t_next[0] <= t_next[2] {
            0
        } else if t_next[1] <= t_next[2] {
            1
        } else {
            2
        };
        if step[axis] == 0 || t_next[axis] > 1.0 {
            break;
        }
        cell[axis] += step[axis];
        t_next[axis] += t_step[axis];
        cells.push(cell);
    }
    cells
}

/// The nearest distance at which a collision shape blocks the segment `from -> to`,
/// reading each visited cell's shape through `boxes` (like `ClipContext.Block.COLLIDER`).
pub fn collider_distance(
    from: [f64; 3],
    to: [f64; 3],
    mut boxes: impl FnMut([i32; 3]) -> Vec<[f64; 6]>,
) -> Option<f64> {
    let mut nearest: Option<f64> = None;
    for cell in segment_cells(from, to) {
        for local in boxes(cell) {
            if let Some(d) = segment_box_distance(from, to, cell, local) {
                nearest = Some(nearest.map_or(d, |n| n.min(d)));
            }
        }
    }
    nearest
}

/// Whether an entity hit `entity_distance` away is picked over the block ahead of it:
/// only when nothing with a collision shape lies at or before the entity.
pub fn entity_wins(entity_distance: f64, collider: Option<f64>) -> bool {
    entity_distance.is_finite() && collider.is_none_or(|c| entity_distance < c)
}

/// Ticks that block mining stays suspended after an entity attack (`MultiPlayerGameMode.destroyDelay`).
pub const DESTROY_DELAY_TICKS: u64 = 5;

/// Whether a held attack on a block is still inside the delay that follows an entity attack.
pub fn mining_suspended(now: u64, last_entity_attack: Option<u64>) -> bool {
    last_entity_attack.is_some_and(|at| now.saturating_sub(at) < DESTROY_DELAY_TICKS)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: [f64; 6] = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];

    #[test]
    fn box_hit_distance() {
        let d = segment_box_distance([0.5, 0.5, -2.0], [0.5, 0.5, 3.0], [0, 0, 0], FULL).unwrap();
        assert!((d - 2.0).abs() < 1e-9);
    }

    #[test]
    fn box_miss_and_short_segment() {
        assert!(segment_box_distance([5.5, 0.5, -2.0], [5.5, 0.5, 3.0], [0, 0, 0], FULL).is_none());
        assert!(
            segment_box_distance([0.5, 0.5, -2.0], [0.5, 0.5, -0.5], [0, 0, 0], FULL).is_none()
        );
    }

    #[test]
    fn starting_inside_hits_at_zero() {
        let d = segment_box_distance([0.5, 0.5, 0.5], [0.5, 0.5, 4.0], [0, 0, 0], FULL).unwrap();
        assert_eq!(d, 0.0);
    }

    #[test]
    fn non_finite_input_is_a_miss() {
        assert!(segment_box_distance([f64::NAN; 3], [1.0; 3], [0; 3], FULL).is_none());
        assert!(segment_box_distance([0.0; 3], [1.0; 3], [0; 3], [f64::NAN; 6]).is_none());
        assert!(segment_cells([f64::INFINITY, 0.0, 0.0], [0.0; 3]).is_empty());
        assert!(segment_cells([1.0e300, 0.0, 0.0], [0.0; 3]).is_empty());
    }

    #[test]
    fn cells_along_axis_and_diagonal() {
        assert_eq!(
            segment_cells([0.5, 0.5, 0.5], [3.5, 0.5, 0.5]),
            vec![[0, 0, 0], [1, 0, 0], [2, 0, 0], [3, 0, 0]]
        );
        assert_eq!(
            segment_cells([0.5, 0.5, 0.5], [-1.5, 0.5, 0.5]),
            vec![[0, 0, 0], [-1, 0, 0], [-2, 0, 0]]
        );
        let diagonal = segment_cells([0.1, 0.1, 0.5], [1.9, 1.9, 0.5]);
        assert_eq!(diagonal.first(), Some(&[0, 0, 0]));
        assert_eq!(diagonal.last(), Some(&[1, 1, 0]));
    }

    #[test]
    fn cells_are_capped() {
        assert!(segment_cells([0.0; 3], [2.0e6, 0.0, 0.0]).len() <= MAX_CELLS);
    }

    #[test]
    fn plant_does_not_block_but_wall_does() {
        // A plant at x=1 has no collision shape; a wall at x=3 does.
        let wall = |cell: [i32; 3]| {
            if cell == [3, 0, 0] {
                vec![FULL]
            } else {
                Vec::new()
            }
        };
        let from = [0.5, 0.5, 0.5];
        let d = collider_distance(from, [5.5, 0.5, 0.5], wall).unwrap();
        assert!((d - 2.5).abs() < 1e-9);
        assert!(entity_wins(2.0, Some(d)));
        assert!(!entity_wins(3.0, Some(d)));
        assert_eq!(collider_distance(from, [2.5, 0.5, 0.5], wall), None);
    }

    #[test]
    fn entity_wins_without_collider_but_not_when_equal() {
        assert!(entity_wins(4.0, None));
        assert!(!entity_wins(2.0, Some(2.0)));
        assert!(!entity_wins(f64::NAN, None));
    }

    #[test]
    fn mining_delay_window() {
        assert!(!mining_suspended(10, None));
        assert!(mining_suspended(10, Some(10)));
        assert!(mining_suspended(14, Some(10)));
        assert!(!mining_suspended(15, Some(10)));
        assert!(mining_suspended(3, Some(10)));
    }
}
