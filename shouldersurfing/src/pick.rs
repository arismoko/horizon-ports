//! Pure parts of the object picker (`ObjectPicker`, `PickContext`).

use crate::math::{V3, length, scale, sub};

/// How far along the unit direction `dir` from `origin` the ray leaves the block cell `cell`
/// (the far side of the slab intersection). Used to step past a block the picker ignores.
pub fn cell_exit(origin: V3, dir: V3, cell: [i32; 3]) -> f64 {
    let mut exit = f64::INFINITY;
    for i in 0..3 {
        if dir[i] == 0.0 {
            continue;
        }
        let lo = (f64::from(cell[i]) - origin[i]) / dir[i];
        let hi = (f64::from(cell[i]) + 1.0 - origin[i]) / dir[i];
        exit = exit.min(lo.max(hi));
    }
    if exit.is_finite() { exit.max(0.0) } else { 0.0 }
}

/// The end of a ray from `from` towards `towards`, `len` long; None when the two coincide or
/// anything is not finite.
pub fn ray_end(from: V3, towards: V3, len: f64) -> Option<V3> {
    let d = sub(towards, from);
    let l = length(d);
    if !(l.is_finite() && l > 1e-9 && len.is_finite() && len > 0.0) {
        return None;
    }
    let end = [
        from[0] + d[0] / l * len,
        from[1] + d[1] / l * len,
        from[2] + d[2] / l * len,
    ];
    Some(end)
}

/// The nearer of two candidate hits by distance from `origin`.
pub fn nearest<T: Copy>(origin: V3, a: Option<(T, V3)>, b: Option<(T, V3)>) -> Option<T> {
    let d = |p: V3| length(sub(p, origin));
    match (a, b) {
        (Some((x, px)), Some((y, py))) => Some(if d(px) <= d(py) { x } else { y }),
        (Some((x, _)), None) => Some(x),
        (None, Some((y, _))) => Some(y),
        (None, None) => None,
    }
}

/// Whether `point` is within `range` of `origin` (`Player.isWithinBlockInteractionRange` and
/// the entity counterpart compare the distance to the range).
pub fn within(origin: V3, point: V3, range: f64) -> bool {
    range.is_finite() && length(sub(point, origin)) <= range
}

pub fn normalized(v: V3) -> Option<V3> {
    let l = length(v);
    (l.is_finite() && l > 1e-9).then(|| scale(v, 1.0 / l))
}

/// The vanilla interaction ranges when the attribute cannot be read.
pub const DEFAULT_BLOCK_RANGE: f64 = 4.5;
pub const DEFAULT_ENTITY_RANGE: f64 = 3.0;

/// A readable attribute value, else the default.
pub fn range_or(value: Option<f64>, default: f64) -> f64 {
    value
        .filter(|v| v.is_finite() && *v >= 0.0)
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exits_the_cell_on_the_far_side() {
        // From x = -3 heading +x through the cell at x = 0..1: leaves at 4 units.
        assert_eq!(cell_exit([-3.0, 0.5, 0.5], [1.0, 0.0, 0.0], [0, 0, 0]), 4.0);
        // Heading -x from x = 5 through cell 1..2: leaves at x = 1, 4 units on.
        assert_eq!(cell_exit([5.0, 0.5, 0.5], [-1.0, 0.0, 0.0], [1, 0, 0]), 4.0);
        // The nearest of the three axes decides.
        let d = normalized([1.0, 1.0, 0.0]).unwrap();
        let t = cell_exit([0.5, 0.0, 0.5], d, [0, 0, 0]);
        assert!((t - 0.5 / d[0]).abs() < 1e-9);
        // A ray that has no direction never moves on.
        assert_eq!(cell_exit([0.0; 3], [0.0; 3], [0, 0, 0]), 0.0);
    }

    #[test]
    fn rays_have_the_requested_length() {
        let end = ray_end([0.0; 3], [0.0, 0.0, 10.0], 4.5).unwrap();
        assert_eq!(end, [0.0, 0.0, 4.5]);
        assert!(ray_end([1.0; 3], [1.0; 3], 4.0).is_none());
        assert!(ray_end([0.0; 3], [1.0, 0.0, 0.0], f64::NAN).is_none());
        assert!(ray_end([0.0; 3], [f64::NAN, 0.0, 0.0], 4.0).is_none());
        assert!(ray_end([0.0; 3], [1.0, 0.0, 0.0], 0.0).is_none());
    }

    #[test]
    fn nearest_prefers_the_closer_hit() {
        let o = [0.0; 3];
        assert_eq!(
            nearest(
                o,
                Some(("a", [3.0, 0.0, 0.0])),
                Some(("b", [2.0, 0.0, 0.0]))
            ),
            Some("b")
        );
        assert_eq!(
            nearest(
                o,
                Some(("a", [1.0, 0.0, 0.0])),
                Some(("b", [2.0, 0.0, 0.0]))
            ),
            Some("a")
        );
        assert_eq!(nearest(o, Some(("a", [1.0, 0.0, 0.0])), None), Some("a"));
        assert_eq!(nearest::<u8>(o, None, None), None);
    }

    #[test]
    fn ranges() {
        assert!(within([0.0; 3], [4.5, 0.0, 0.0], 4.5));
        assert!(!within([0.0; 3], [4.6, 0.0, 0.0], 4.5));
        assert!(!within([0.0; 3], [1.0, 0.0, 0.0], f64::NAN));
        assert_eq!(range_or(Some(5.0), 4.5), 5.0);
        assert_eq!(range_or(Some(f64::NAN), 4.5), 4.5);
        assert_eq!(range_or(Some(-1.0), 3.0), 3.0);
        assert_eq!(range_or(None, 3.0), 3.0);
    }
}
