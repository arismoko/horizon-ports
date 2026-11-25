//! Pure geometry and 3D Bresenham voxel traversal for reach-around placement.

use crate::config::{BridgingAdjacency, PlacementAxisMode};

/// Threshold below which a cardinal direction is not considered aligned with the view.
pub const DIRECTION_SIMILARITY_THRESHOLD: f64 = 0.1;

/// Epsilon used in voxel stepping and slab-assist height checks (`Path.NEAR_ZERO`).
pub const NEAR_ZERO: f64 = 0.01;

/// Half-extent of a unit block cube for SAT missed-voxel checks (`Path.CUBE_EXTENT`).
pub const CUBE_EXTENT: [f64; 3] = [0.5, 0.5, 0.5];

/// Maximum Bresenham iterations to guarantee termination regardless of inputs.
const MAX_VOXEL_STEPS: usize = 256;

/// Vanilla cardinal directions in `Direction.values()` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Direction {
    /// All six cardinal directions in vanilla's `Direction.values()` order.
    pub const ALL: [Self; 6] = [
        Self::Down,
        Self::Up,
        Self::North,
        Self::South,
        Self::West,
        Self::East,
    ];

    /// Unit normal vector `[dx, dy, dz]`.
    pub const fn normal(self) -> [i32; 3] {
        match self {
            Self::Down => [0, -1, 0],
            Self::Up => [0, 1, 0],
            Self::North => [0, 0, -1],
            Self::South => [0, 0, 1],
            Self::West => [-1, 0, 0],
            Self::East => [1, 0, 0],
        }
    }

    /// Opposite cardinal direction.
    pub const fn opposite(self) -> Self {
        match self {
            Self::Down => Self::Up,
            Self::Up => Self::Down,
            Self::North => Self::South,
            Self::South => Self::North,
            Self::West => Self::East,
            Self::East => Self::West,
        }
    }

    /// Offset `pos` by one block in this direction using saturating arithmetic.
    pub const fn offset(self, pos: [i32; 3]) -> [i32; 3] {
        let n = self.normal();
        [
            pos[0].saturating_add(n[0]),
            pos[1].saturating_add(n[1]),
            pos[2].saturating_add(n[2]),
        ]
    }

    /// Whether this direction is vertical (`Up` or `Down`).
    pub const fn is_vertical(self) -> bool {
        matches!(self, Self::Down | Self::Up)
    }

    /// Whether this direction is horizontal (`North`, `South`, `West`, `East`).
    pub const fn is_horizontal(self) -> bool {
        !self.is_vertical()
    }

    /// Whether this direction is allowed by `mode`.
    pub const fn allowed_by(self, mode: PlacementAxisMode) -> bool {
        match mode {
            PlacementAxisMode::Horizontal => self.is_horizontal(),
            PlacementAxisMode::Vertical => self.is_vertical(),
            PlacementAxisMode::Both => true,
        }
    }

    /// Crosshair indicator alignment (`PlacementAlignment.from`).
    ///
    /// `direction` is the offset from the placement spot to the support block:
    /// support above (`Up`) shows the `down` indicator, support below (`Down`)
    /// shows `up`, and horizontal support shows `horizontal`.
    pub const fn indicator_alignment(self) -> PlacementAlignment {
        match self {
            Self::Up => PlacementAlignment::Down,
            Self::Down => PlacementAlignment::Up,
            Self::North | Self::South | Self::West | Self::East => PlacementAlignment::Horizontal,
        }
    }
}

/// Crosshair directional indicator sprite (`PlacementAlignment`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacementAlignment {
    Up,
    Down,
    Horizontal,
}

impl PlacementAlignment {
    /// GUI atlas sprite identifier under `assets/bridgingmod/textures/gui/sprites/`.
    pub const fn sprite_name(self) -> &'static str {
        match self {
            Self::Up => "bridgingmod:indicator/up",
            Self::Down => "bridgingmod:indicator/down",
            Self::Horizontal => "bridgingmod:indicator/horizontal",
        }
    }
}

/// Vanilla `Entity.calculateViewVector(pitch, yaw)` from degrees (`yaw`, `pitch`).
pub fn view_vector(yaw_deg: f32, pitch_deg: f32) -> [f64; 3] {
    let pitch_rad = f64::from(pitch_deg).to_radians();
    let yaw_rad = -f64::from(yaw_deg).to_radians();
    let cos_pitch = pitch_rad.cos();
    [
        yaw_rad.sin() * cos_pitch,
        -pitch_rad.sin(),
        yaw_rad.cos() * cos_pitch,
    ]
}

/// Convert a world-space point into its containing integer block coordinates (`BlockPos.containing`).
pub fn block_pos_containing(p: [f64; 3]) -> [i32; 3] {
    [
        p[0].floor() as i32,
        p[1].floor() as i32,
        p[2].floor() as i32,
    ]
}

/// Euclidean distance between two 3D points.
pub fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// Compute the ordered list of block coordinates along the reach line (`PathTraversalHandler.getViewBlockPath`).
#[allow(clippy::too_many_arguments)]
pub fn compute_view_block_path(
    cam_pos: [f64; 3],
    cam_dir: [f64; 3],
    eye_pos: [f64; 3],
    eye_dir: [f64; 3],
    reach: f64,
    min_distance_pct: f32,
    adjacency: BridgingAdjacency,
    snap_strength: f32,
) -> Vec<[i32; 3]> {
    if !reach.is_finite() || reach <= 0.0 {
        return Vec::new();
    }
    if !cam_pos.iter().all(|v| v.is_finite())
        || !cam_dir.iter().all(|v| v.is_finite())
        || !eye_pos.iter().all(|v| v.is_finite())
        || !eye_dir.iter().all(|v| v.is_finite())
    {
        return Vec::new();
    }

    let world_view_end = [
        eye_pos[0] + eye_dir[0] * reach,
        eye_pos[1] + eye_dir[1] * reach,
        eye_pos[2] + eye_dir[2] * reach,
    ];
    let total_distance = distance(world_view_end, cam_pos);
    let min_frac = f64::from(min_distance_pct.clamp(0.0, 100.0)) / 100.0;

    let far_vec = [
        cam_dir[0] * total_distance,
        cam_dir[1] * total_distance,
        cam_dir[2] * total_distance,
    ];
    let near_vec = [
        far_vec[0] * min_frac,
        far_vec[1] * min_frac,
        far_vec[2] * min_frac,
    ];

    let start_pos = block_pos_containing([
        cam_pos[0] + near_vec[0],
        cam_pos[1] + near_vec[1],
        cam_pos[2] + near_vec[2],
    ]);
    let end_pos = block_pos_containing([
        cam_pos[0] + far_vec[0],
        cam_pos[1] + far_vec[1],
        cam_pos[2] + far_vec[2],
    ]);

    calculate_bresenham_voxels(start_pos, end_pos, adjacency, snap_strength)
}

/// 3D Bresenham voxel line from `start_pos` to `end_pos`, optionally inserting
/// diagonal edge/corner voxels that the line segment intersects (`Path.calculateBresenhamVoxels`).
pub fn calculate_bresenham_voxels(
    start_pos: [i32; 3],
    end_pos: [i32; 3],
    adjacency: BridgingAdjacency,
    snap_strength: f32,
) -> Vec<[i32; 3]> {
    let mut points = Vec::with_capacity(16);
    points.push(start_pos);

    let delta = [
        end_pos[0].saturating_sub(start_pos[0]),
        end_pos[1].saturating_sub(start_pos[1]),
        end_pos[2].saturating_sub(start_pos[2]),
    ];

    let dx = delta[0].saturating_abs();
    let dy = delta[1].saturating_abs();
    let dz = delta[2].saturating_abs();

    let x_step = if delta[0] > 0 { 1 } else { -1 };
    let y_step = if delta[1] > 0 { 1 } else { -1 };
    let z_step = if delta[2] > 0 { 1 } else { -1 };

    let mut cur = start_pos;
    let extent_mult = f64::from(snap_strength.clamp(0.0, 1.0));

    if dx >= dy && dx >= dz {
        let mut p1 = 2 * dy - dx;
        let mut p2 = 2 * dz - dx;
        let mut steps = 0;

        while cur[0] != end_pos[0] && steps < MAX_VOXEL_STEPS {
            steps += 1;
            cur[0] = cur[0].saturating_add(x_step);
            if p1 >= 0 {
                cur[1] = cur[1].saturating_add(y_step);
                p1 -= 2 * dx;
            }
            if p2 >= 0 {
                cur[2] = cur[2].saturating_add(z_step);
                p2 -= 2 * dx;
            }
            p1 += 2 * dy;
            p2 += 2 * dz;

            if adjacency != BridgingAdjacency::Disabled {
                let missed = calculate_missed_points(
                    &points,
                    cur,
                    start_pos,
                    end_pos,
                    extent_mult,
                    adjacency,
                );
                points.extend(missed);
            }
            points.push(cur);
        }
    } else if dy >= dx && dy >= dz {
        let mut p1 = 2 * dx - dy;
        let mut p2 = 2 * dz - dy;
        let mut steps = 0;

        while cur[1] != end_pos[1] && steps < MAX_VOXEL_STEPS {
            steps += 1;
            cur[1] = cur[1].saturating_add(y_step);
            if p1 >= 0 {
                cur[0] = cur[0].saturating_add(x_step);
                p1 -= 2 * dy;
            }
            if p2 >= 0 {
                cur[2] = cur[2].saturating_add(z_step);
                p2 -= 2 * dy;
            }
            p1 += 2 * dx;
            p2 += 2 * dz;

            if adjacency != BridgingAdjacency::Disabled {
                let missed = calculate_missed_points(
                    &points,
                    cur,
                    start_pos,
                    end_pos,
                    extent_mult,
                    adjacency,
                );
                points.extend(missed);
            }
            points.push(cur);
        }
    } else {
        let mut p1 = 2 * dy - dz;
        let mut p2 = 2 * dx - dz;
        let mut steps = 0;

        while cur[2] != end_pos[2] && steps < MAX_VOXEL_STEPS {
            steps += 1;
            cur[2] = cur[2].saturating_add(z_step);
            if p1 >= 0 {
                cur[1] = cur[1].saturating_add(y_step);
                p1 -= 2 * dz;
            }
            if p2 >= 0 {
                cur[0] = cur[0].saturating_add(x_step);
                p2 -= 2 * dz;
            }
            p1 += 2 * dy;
            p2 += 2 * dx;

            if adjacency != BridgingAdjacency::Disabled {
                let missed = calculate_missed_points(
                    &points,
                    cur,
                    start_pos,
                    end_pos,
                    extent_mult,
                    adjacency,
                );
                points.extend(missed);
            }
            points.push(cur);
        }
    }

    points
}

/// Find edge- or corner-adjacent voxels skipped by a diagonal Bresenham step that
/// intersect the line segment under the scaled box extent (`Path.calculateMissedPoints`).
fn calculate_missed_points(
    points: &[[i32; 3]],
    new_point: [i32; 3],
    line_start: [i32; 3],
    line_end: [i32; 3],
    extent_mult: f64,
    adjacency: BridgingAdjacency,
) -> Vec<[i32; 3]> {
    let Some(&last_point) = points.last() else {
        return Vec::new();
    };

    let pd = [
        new_point[0] - last_point[0],
        new_point[1] - last_point[1],
        new_point[2] - last_point[2],
    ];
    let diff = pd[0].abs() + pd[1].abs() + pd[2].abs();

    if diff <= 1 || diff > 3 {
        return Vec::new();
    }

    let mut review = Vec::with_capacity(6);

    if diff == 2
        && matches!(
            adjacency,
            BridgingAdjacency::Edges | BridgingAdjacency::Corners
        )
    {
        let dirs = [[pd[0], 0, 0], [0, pd[1], 0], [0, 0, pd[2]]];
        for d in dirs {
            if d != [0, 0, 0] {
                review.push([
                    last_point[0] + d[0],
                    last_point[1] + d[1],
                    last_point[2] + d[2],
                ]);
            }
        }
    } else if diff == 3 && matches!(adjacency, BridgingAdjacency::Corners) {
        let dirs = [
            [pd[0], 0, 0],
            [0, pd[1], 0],
            [0, 0, pd[2]],
            [pd[0], pd[1], 0],
            [pd[0], 0, pd[2]],
            [0, pd[1], pd[2]],
        ];
        for d in dirs {
            if d != [0, 0, 0] {
                review.push([
                    last_point[0] + d[0],
                    last_point[1] + d[1],
                    last_point[2] + d[2],
                ]);
            }
        }
    }

    let ext = [
        CUBE_EXTENT[0] * extent_mult,
        CUBE_EXTENT[1] * extent_mult,
        CUBE_EXTENT[2] * extent_mult,
    ];

    review
        .into_iter()
        .filter(|&pos| {
            let ls = [
                f64::from(line_start[0] - pos[0]),
                f64::from(line_start[1] - pos[1]),
                f64::from(line_start[2] - pos[2]),
            ];
            let le = [
                f64::from(line_end[0] - pos[0]),
                f64::from(line_end[1] - pos[1]),
                f64::from(line_end[2] - pos[2]),
            ];
            let mid = [
                (ls[0] + le[0]) * 0.5,
                (ls[1] + le[1]) * 0.5,
                (ls[2] + le[2]) * 0.5,
            ];
            let line = [ls[0] - mid[0], ls[1] - mid[1], ls[2] - mid[2]];
            let line_ext = [line[0].abs(), line[1].abs(), line[2].abs()];

            if mid[0].abs() > ext[0] + line_ext[0] {
                return false;
            }
            if mid[1].abs() > ext[1] + line_ext[1] {
                return false;
            }
            if mid[2].abs() > ext[2] + line_ext[2] {
                return false;
            }

            if (mid[1] * line[2] - mid[2] * line[1]).abs()
                > ext[1] * line_ext[2] + ext[2] * line_ext[1]
            {
                return false;
            }
            if (mid[0] * line[2] - mid[2] * line[0]).abs()
                > ext[0] * line_ext[2] + ext[2] * line_ext[0]
            {
                return false;
            }
            if (mid[0] * line[1] - mid[1] * line[0]).abs()
                > ext[0] * line_ext[1] + ext[1] * line_ext[0]
            {
                return false;
            }

            true
        })
        .collect()
}

/// Sides that a block can be built off of for a given view direction (`PathTraversalHandler.getValidAssistSides`).
pub fn valid_assist_sides(view_dir: [f64; 3]) -> Vec<Direction> {
    let mut sides = Vec::with_capacity(3);
    for dir in Direction::ALL {
        let n = dir.normal();
        let similarity = view_dir[0] * f64::from(n[0])
            + view_dir[1] * f64::from(n[1])
            + view_dir[2] * f64::from(n[2]);
        if similarity >= DIRECTION_SIMILARITY_THRESHOLD {
            sides.push(dir.opposite());
        }
    }
    sides
}

/// Whether the AABB `[min, max]` strictly overlaps the unit block cube at `pos` (`AABB.intersects`).
pub fn aabb_intersects_block(min: [f64; 3], max: [f64; 3], pos: [i32; 3]) -> bool {
    let bx0 = f64::from(pos[0]);
    let by0 = f64::from(pos[1]);
    let bz0 = f64::from(pos[2]);
    let bx1 = bx0 + 1.0;
    let by1 = by0 + 1.0;
    let bz1 = bz0 + 1.0;

    min[0] < bx1 && max[0] > bx0 && min[1] < by1 && max[1] > by0 && min[2] < bz1 && max[2] > bz0
}

/// Ensure a placement hit point stays within `reach` of `origin` so `client_pick` accepts it.
///
/// If `point` is already within `reach`, returns it unchanged. Otherwise moves it to the closest
/// point inside `pos`'s unit cube (preserving `point[1]` when `preserve_y` is true for horizontal
/// slab/trapdoor alignment), and returns `None` if even that point exceeds `reach`.
pub fn clamp_point_within_reach(
    origin: [f64; 3],
    point: [f64; 3],
    pos: [i32; 3],
    reach: f64,
    preserve_y: bool,
) -> Option<[f64; 3]> {
    if !reach.is_finite() || reach <= 0.0 {
        return None;
    }
    let eps_reach = (reach - 1e-4).max(0.0);
    if distance(origin, point) <= eps_reach {
        return Some(point);
    }

    let clamped = [
        origin[0].clamp(f64::from(pos[0]) + 0.01, f64::from(pos[0]) + 0.99),
        if preserve_y {
            point[1]
        } else {
            origin[1].clamp(f64::from(pos[1]) + 0.01, f64::from(pos[1]) + 0.99)
        },
        origin[2].clamp(f64::from(pos[2]) + 0.01, f64::from(pos[2]) + 0.99),
    ];

    if distance(origin, clamped) <= eps_reach {
        Some(clamped)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_vector_cardinal_directions() {
        // Yaw 0, pitch 0 -> South (+Z).
        let south = view_vector(0.0, 0.0);
        assert!(south[0].abs() < 1e-9);
        assert!(south[1].abs() < 1e-9);
        assert!((south[2] - 1.0).abs() < 1e-9);

        // Yaw 90, pitch 0 -> West (-X).
        let west = view_vector(90.0, 0.0);
        assert!((west[0] + 1.0).abs() < 1e-9);
        assert!(west[1].abs() < 1e-9);
        assert!(west[2].abs() < 1e-9);

        // Pitch -90 -> Up (+Y).
        let up = view_vector(0.0, -90.0);
        assert!(up[0].abs() < 1e-9);
        assert!((up[1] - 1.0).abs() < 1e-9);
        assert!(up[2].abs() < 1e-9);
    }

    #[test]
    fn valid_assist_sides_matches_view_opposite() {
        // Looking straight South (+Z) and slightly down (-Y).
        let sides = valid_assist_sides([0.0, -0.5, 0.866]);
        // Down normal dot = +0.5 -> opposite Up; South normal dot = +0.866 -> opposite North.
        assert_eq!(sides, vec![Direction::Up, Direction::North]);
    }

    #[test]
    fn bresenham_straight_line_visits_every_voxel() {
        let path =
            calculate_bresenham_voxels([0, 64, 0], [0, 64, 3], BridgingAdjacency::Corners, 1.0);
        assert_eq!(path, vec![[0, 64, 0], [0, 64, 1], [0, 64, 2], [0, 64, 3]]);
    }

    #[test]
    fn bresenham_diagonal_inserts_adjacent_voxels_when_enabled() {
        let strict =
            calculate_bresenham_voxels([0, 64, 0], [2, 64, 2], BridgingAdjacency::Disabled, 1.0);
        assert_eq!(strict, vec![[0, 64, 0], [1, 64, 1], [2, 64, 2]]);

        let with_edges =
            calculate_bresenham_voxels([0, 64, 0], [2, 64, 2], BridgingAdjacency::Edges, 1.0);
        assert!(with_edges.len() > strict.len());
        assert!(with_edges.contains(&[1, 64, 0]));
        assert!(with_edges.contains(&[0, 64, 1]));
    }

    #[test]
    fn aabb_block_intersection_matches_vanilla_strict_bounds() {
        assert!(aabb_intersects_block(
            [0.2, 64.0, 0.2],
            [0.8, 65.8, 0.8],
            [0, 64, 0]
        ));
        // Touching face only does not intersect.
        assert!(!aabb_intersects_block(
            [1.0, 64.0, 0.0],
            [1.6, 65.8, 0.6],
            [0, 64, 0]
        ));
    }

    #[test]
    fn clamp_point_within_reach_keeps_valid_and_clamps_border() {
        let origin = [0.5, 65.62, 0.5];
        let near = [0.5, 64.5, 2.5];
        assert_eq!(
            clamp_point_within_reach(origin, near, [0, 64, 2], 4.5, false),
            Some(near)
        );

        // Center is 4.55 away, but closest corner of block [0, 65, 5] is ~4.51? No, 4.01 away.
        let far_center = [0.5, 65.5, 5.5];
        let clamped = clamp_point_within_reach(origin, far_center, [0, 65, 5], 4.5, false);
        assert!(clamped.is_some());
        assert!(distance(origin, clamped.unwrap()) <= 4.5);
    }

    #[test]
    fn indicator_alignment_maps_support_direction() {
        assert_eq!(
            Direction::Up.indicator_alignment(),
            PlacementAlignment::Down
        );
        assert_eq!(
            Direction::Down.indicator_alignment(),
            PlacementAlignment::Up
        );
        assert_eq!(
            Direction::North.indicator_alignment(),
            PlacementAlignment::Horizontal
        );
    }
}
