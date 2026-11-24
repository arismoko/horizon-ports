//! Pure state rules for Macaw's Bridges blocks.
//!
//! Every rule here mirrors the original Java classes (`Bridge_Block`,
//! `Bridge_Block_Rope`, `Bridge_Stairs`, `Bridge_Torch`, `Rail_Bridge`) without
//! touching the host, so every branch is directly unit-tested.

use mod_api::{Direction, StructureRotation};

/// Horizontal cardinal direction (N/E/S/W).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Horizontal {
    North,
    East,
    South,
    West,
}

impl Horizontal {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "north" => Some(Self::North),
            "east" => Some(Self::East),
            "south" => Some(Self::South),
            "west" => Some(Self::West),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::North => "north",
            Self::East => "east",
            Self::South => "south",
            Self::West => "west",
        }
    }

    pub fn from_direction(dir: Direction) -> Self {
        match dir {
            Direction::East => Self::East,
            Direction::South => Self::South,
            Direction::West => Self::West,
            _ => Self::North,
        }
    }

    /// Vanilla `Direction.fromYRot(yaw)`: 0 = south, 90 = west, 180 = north, 270 = east.
    pub fn from_yaw(yaw: f32) -> Self {
        let quadrant = ((yaw * 4.0 / 360.0) + 0.5).floor() as i32 & 3;
        match quadrant {
            0 => Self::South,
            1 => Self::West,
            2 => Self::North,
            _ => Self::East,
        }
    }

    pub fn clockwise(self) -> Self {
        match self {
            Self::North => Self::East,
            Self::East => Self::South,
            Self::South => Self::West,
            Self::West => Self::North,
        }
    }

    pub fn counterclockwise(self) -> Self {
        match self {
            Self::North => Self::West,
            Self::East => Self::North,
            Self::South => Self::East,
            Self::West => Self::South,
        }
    }

    pub fn rotate(self, rot: StructureRotation) -> Self {
        match rot {
            StructureRotation::Clockwise90 => self.clockwise(),
            StructureRotation::Clockwise180 => self.clockwise().clockwise(),
            StructureRotation::Counterclockwise90 => self.counterclockwise(),
            _ => self,
        }
    }

    pub fn step(self, pos: [i32; 3]) -> [i32; 3] {
        match self {
            Self::North => [pos[0], pos[1], pos[2] - 1],
            Self::East => [pos[0] + 1, pos[1], pos[2]],
            Self::South => [pos[0], pos[1], pos[2] + 1],
            Self::West => [pos[0] - 1, pos[1], pos[2]],
        }
    }

    /// Collapse N/S to `"north"` and E/W to `"east"` (`Bridge_Block_Rope.FACING_TD`).
    pub fn rope_axis(self) -> Self {
        match self {
            Self::North | Self::South => Self::North,
            Self::East | Self::West => Self::East,
        }
    }
}

/// Extract a property value from a serialized block state string (`mod:block[a=1,b=2]`).
pub fn property<'a>(state: &'a str, key: &str) -> Option<&'a str> {
    let props = state.strip_suffix(']')?.split_once('[')?.1;
    props.split(',').find_map(|pair| {
        pair.split_once('=')
            .filter(|(k, _)| *k == key)
            .map(|(_, v)| v)
    })
}

/// `Bridge_Block.getConnectionStatus`.
pub fn bridge_connection(north: bool, east: bool, south: bool, west: bool) -> &'static str {
    match (north, east, south, west) {
        (false, false, false, false) => "middle_ns",
        (true, false, true, false) => "middle_ew",
        (false, true, false, true) => "middle_ns",
        (false, false, true, false) => "middle_ew",
        (true, false, false, false) => "middle_ew",
        (false, false, false, true) => "middle_ns",
        (false, true, false, false) => "middle_ns",
        (false, false, true, true) => "corner_sw",
        (false, true, true, false) => "corner_se",
        (true, false, false, true) => "corner_nw",
        (true, true, false, false) => "corner_ne",
        (false, true, true, true) => "side_n",
        (true, false, true, true) => "side_e",
        (true, true, false, true) => "side_s",
        (true, true, true, false) => "side_w",
        (true, true, true, true) => "base",
    }
}

/// `Bridge_Block.ConnectionStatus` enum cycle order (`state.cycle(CONNECTION)`).
const BRIDGE_CYCLE: [&str; 11] = [
    "base",
    "middle_ns",
    "middle_ew",
    "corner_ne",
    "corner_nw",
    "corner_se",
    "corner_sw",
    "side_n",
    "side_e",
    "side_s",
    "side_w",
];

pub fn bridge_cycle(current: &str) -> &'static str {
    let idx = BRIDGE_CYCLE
        .iter()
        .position(|&s| s == current)
        .map_or(0, |i| (i + 1) % BRIDGE_CYCLE.len());
    BRIDGE_CYCLE[idx]
}

/// `Bridge_Block_Rope.getConnectionStatus`.
pub fn rope_connection(north: bool, east: bool, south: bool, west: bool) -> &'static str {
    match (north, east, south, west) {
        (false, false, false, false) => "middle_ns",
        (true, false, true, false) => "middle_ew",
        (false, true, false, true) => "middle_ns",
        (false, false, true, false) => "middle_end_s",
        (true, false, false, false) => "middle_end_n",
        (false, false, false, true) => "middle_end_w",
        (false, true, false, false) => "middle_end_e",
        (false, false, true, true) => "corner_sw",
        (false, true, true, false) => "corner_se",
        (true, false, false, true) => "corner_nw",
        (true, true, false, false) => "corner_ne",
        (false, true, true, true) => "side_n",
        (true, false, true, true) => "side_e",
        (true, true, false, true) => "side_s",
        (true, true, true, false) => "side_w",
        (true, true, true, true) => "base",
    }
}

/// Whether `Bridge_Block_Rope.updateShape` freezes this connection against neighbour updates.
pub fn rope_connection_is_locked(connection: &str) -> bool {
    matches!(
        connection,
        "base_toggled"
            | "end_n_left"
            | "end_n_right"
            | "end_e_left"
            | "end_e_right"
            | "end_s_left"
            | "end_s_right"
            | "end_w_left"
            | "end_w_right"
    )
}

/// `Bridge_Block_Rope.useItemOn` pliers/shears state transition table.
pub fn rope_cycle(connection: &str) -> &'static str {
    match connection {
        "side_n" | "side_e" | "side_s" | "side_w" => "base_toggled",
        "corner_sw" => "end_n_left",
        "end_n_left" => "end_s_right",
        "end_s_right" => "corner_nw",
        "corner_nw" => "end_s_left",
        "end_s_left" => "end_n_right",
        "end_n_right" => "corner_ne",
        "corner_ne" => "end_e_left",
        "end_e_left" => "end_w_right",
        "end_w_right" => "corner_se",
        "corner_se" => "end_w_left",
        "end_w_left" => "end_e_right",
        "end_e_right" => "base_toggled",
        "base_toggled" => "middle_ew",
        "middle_ew" => "middle_ns",
        "middle_ns" => "corner_sw",
        "base" => "base_toggled",
        // `middle_end_*` fall through to default `ConnectionStatus.BASE` in original switch.
        _ => "base",
    }
}

/// `Bridge_Block_Rope.rotate`: note that in the original Java, `rotate` checks
/// `originalDirection` (not `rotatedDirection`), collapsing N/S -> N and E/W -> E.
pub fn rope_rotate(facing: Horizontal, _rot: StructureRotation) -> Horizontal {
    facing.rope_axis()
}

/// `Bridge_Stairs.getConnectionStatus`.
pub fn stair_connection(
    facing: Horizontal,
    north: bool,
    east: bool,
    south: bool,
    west: bool,
) -> &'static str {
    match facing {
        Horizontal::North => match (north, south) {
            (true, true) => "double",
            (_, true) => "right",
            (true, false) => "left",
            (false, false) => "base",
        },
        Horizontal::South => match (north, south) {
            (true, true) => "double",
            (true, false) => "right",
            (false, true) => "left",
            (false, false) => "base",
        },
        Horizontal::East => match (east, west) {
            (true, true) => "double",
            (_, true) => "right",
            (true, false) => "left",
            (false, false) => "base",
        },
        Horizontal::West => match (east, west) {
            (true, true) => "double",
            (true, false) => "right",
            (false, true) => "left",
            (false, false) => "base",
        },
    }
}

/// `Bridge_Torch.handleBridgeBlockPlacement`: returns `None` when placement is refused.
pub fn torch_facing_on_bridge(connection: &str) -> Option<Horizontal> {
    match connection {
        "base" => None,
        "side_e" => Some(Horizontal::South),
        "side_n" => Some(Horizontal::East),
        "side_s" => Some(Horizontal::West),
        "side_w" => Some(Horizontal::North),
        "middle_ew" => Some(Horizontal::North),
        "middle_ns" => Some(Horizontal::East),
        "corner_ne" => Some(Horizontal::North),
        "corner_nw" => Some(Horizontal::West),
        "corner_se" => Some(Horizontal::North),
        "corner_sw" => Some(Horizontal::East),
        _ => Some(Horizontal::North),
    }
}

/// `Bridge_Torch.handleBridgeBlockRopePlacement`: returns `None` when placement is refused.
pub fn torch_facing_on_rope(connection: &str) -> Option<Horizontal> {
    match connection {
        "base" | "base_toggled" => None,
        "side_e" => Some(Horizontal::South),
        "side_n" => Some(Horizontal::East),
        "side_s" => Some(Horizontal::West),
        "side_w" => Some(Horizontal::North),
        "middle_ew" => Some(Horizontal::North),
        "middle_ns" => Some(Horizontal::East),
        "corner_ne" => Some(Horizontal::North),
        "corner_nw" => Some(Horizontal::West),
        "corner_se" => Some(Horizontal::North),
        "corner_sw" => Some(Horizontal::East),
        "middle_end_n" | "middle_end_s" => Some(Horizontal::North),
        "middle_end_e" | "middle_end_w" => Some(Horizontal::East),
        "end_e_left" | "end_e_right" => Some(Horizontal::North),
        "end_n_left" | "end_n_right" => Some(Horizontal::South),
        "end_s_left" => Some(Horizontal::West),
        "end_s_right" => Some(Horizontal::East),
        "end_w_left" => Some(Horizontal::East),
        "end_w_right" => Some(Horizontal::West),
        _ => Some(Horizontal::North),
    }
}

/// `Bridge_Torch.handleBridgeStairsPlacement`: returns `None` on `double`.
pub fn torch_facing_on_stair(connection: &str, stair_facing: Horizontal) -> Option<Horizontal> {
    if connection == "double" {
        return None;
    }
    if connection == "left" {
        Some(stair_facing.counterclockwise())
    } else {
        Some(stair_facing.clockwise())
    }
}

/// `Bridge_Torch.animateTick` particle position `[x, y, z]`.
pub fn torch_particle_pos(pos: [i32; 3], facing: Horizontal, lightstate: &str) -> [f64; 3] {
    let x = f64::from(pos[0]);
    let y = f64::from(pos[1]) + if lightstate == "bridge" { 0.5 } else { 0.9 };
    let z = f64::from(pos[2]);
    match facing {
        Horizontal::East => [x + 0.5, y, z + 0.05],
        Horizontal::West => [x + 0.5, y, z + 0.95],
        Horizontal::North => [x + 0.05, y, z + 0.5],
        Horizontal::South => [x + 0.95, y, z + 0.5],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_connection_matches_all_16_neighbor_masks() {
        assert_eq!(bridge_connection(false, false, false, false), "middle_ns");
        assert_eq!(bridge_connection(true, false, true, false), "middle_ew");
        assert_eq!(bridge_connection(false, true, false, true), "middle_ns");
        assert_eq!(bridge_connection(false, false, true, false), "middle_ew");
        assert_eq!(bridge_connection(true, false, false, false), "middle_ew");
        assert_eq!(bridge_connection(false, false, false, true), "middle_ns");
        assert_eq!(bridge_connection(false, true, false, false), "middle_ns");
        assert_eq!(bridge_connection(false, false, true, true), "corner_sw");
        assert_eq!(bridge_connection(false, true, true, false), "corner_se");
        assert_eq!(bridge_connection(true, false, false, true), "corner_nw");
        assert_eq!(bridge_connection(true, true, false, false), "corner_ne");
        assert_eq!(bridge_connection(false, true, true, true), "side_n");
        assert_eq!(bridge_connection(true, false, true, true), "side_e");
        assert_eq!(bridge_connection(true, true, false, true), "side_s");
        assert_eq!(bridge_connection(true, true, true, false), "side_w");
        assert_eq!(bridge_connection(true, true, true, true), "base");
    }

    #[test]
    fn bridge_cycle_wraps_full_enum() {
        let mut cur = "base";
        for expected in [
            "middle_ns",
            "middle_ew",
            "corner_ne",
            "corner_nw",
            "corner_se",
            "corner_sw",
            "side_n",
            "side_e",
            "side_s",
            "side_w",
            "base",
        ] {
            cur = bridge_cycle(cur);
            assert_eq!(cur, expected);
        }
    }

    #[test]
    fn rope_connection_single_neighbors_use_middle_end() {
        assert_eq!(rope_connection(true, false, false, false), "middle_end_n");
        assert_eq!(rope_connection(false, true, false, false), "middle_end_e");
        assert_eq!(rope_connection(false, false, true, false), "middle_end_s");
        assert_eq!(rope_connection(false, false, false, true), "middle_end_w");
        assert_eq!(rope_connection(false, false, false, false), "middle_ns");
        assert_eq!(rope_connection(true, true, true, true), "base");
    }

    #[test]
    fn rope_cycle_and_locks_match_original() {
        assert_eq!(rope_cycle("side_n"), "base_toggled");
        assert_eq!(rope_cycle("base"), "base_toggled");
        assert_eq!(rope_cycle("base_toggled"), "middle_ew");
        assert_eq!(rope_cycle("middle_ew"), "middle_ns");
        assert_eq!(rope_cycle("middle_ns"), "corner_sw");
        assert_eq!(rope_cycle("corner_sw"), "end_n_left");
        assert_eq!(rope_cycle("end_n_left"), "end_s_right");
        assert_eq!(rope_cycle("end_s_right"), "corner_nw");
        assert_eq!(rope_cycle("corner_nw"), "end_s_left");
        assert_eq!(rope_cycle("end_s_left"), "end_n_right");
        assert_eq!(rope_cycle("end_n_right"), "corner_ne");
        assert_eq!(rope_cycle("corner_ne"), "end_e_left");
        assert_eq!(rope_cycle("end_e_left"), "end_w_right");
        assert_eq!(rope_cycle("end_w_right"), "corner_se");
        assert_eq!(rope_cycle("corner_se"), "end_w_left");
        assert_eq!(rope_cycle("end_w_left"), "end_e_right");
        assert_eq!(rope_cycle("end_e_right"), "base_toggled");
        assert_eq!(rope_cycle("middle_end_n"), "base");

        assert!(rope_connection_is_locked("base_toggled"));
        assert!(rope_connection_is_locked("end_n_left"));
        assert!(!rope_connection_is_locked("base"));
        assert!(!rope_connection_is_locked("corner_ne"));
    }

    #[test]
    fn stair_connection_uses_perpendicular_axis() {
        assert_eq!(
            stair_connection(Horizontal::North, true, false, true, false),
            "double"
        );
        assert_eq!(
            stair_connection(Horizontal::North, false, false, true, false),
            "right"
        );
        assert_eq!(
            stair_connection(Horizontal::North, true, false, false, false),
            "left"
        );
        assert_eq!(
            stair_connection(Horizontal::South, true, false, false, false),
            "right"
        );
        assert_eq!(
            stair_connection(Horizontal::East, false, false, false, true),
            "right"
        );
        assert_eq!(
            stair_connection(Horizontal::West, false, true, false, false),
            "right"
        );
        assert_eq!(
            stair_connection(Horizontal::East, true, false, true, false),
            "base"
        );
    }

    #[test]
    fn torch_placement_rules_match_original() {
        assert_eq!(torch_facing_on_bridge("base"), None);
        assert_eq!(torch_facing_on_bridge("side_e"), Some(Horizontal::South));
        assert_eq!(torch_facing_on_bridge("corner_nw"), Some(Horizontal::West));
        assert_eq!(torch_facing_on_rope("base_toggled"), None);
        assert_eq!(torch_facing_on_rope("end_s_right"), Some(Horizontal::East));
        assert_eq!(torch_facing_on_stair("double", Horizontal::North), None);
        assert_eq!(
            torch_facing_on_stair("left", Horizontal::North),
            Some(Horizontal::West)
        );
        assert_eq!(
            torch_facing_on_stair("right", Horizontal::North),
            Some(Horizontal::East)
        );
        assert_eq!(
            torch_facing_on_stair("base", Horizontal::North),
            Some(Horizontal::East)
        );
    }

    #[test]
    fn yaw_and_property_parsing() {
        assert_eq!(Horizontal::from_yaw(0.0), Horizontal::South);
        assert_eq!(Horizontal::from_yaw(90.0), Horizontal::West);
        assert_eq!(Horizontal::from_yaw(180.0), Horizontal::North);
        assert_eq!(Horizontal::from_yaw(270.0), Horizontal::East);
        assert_eq!(Horizontal::from_yaw(-90.0), Horizontal::East);

        let s = "mcwbridges:rope_oak_bridge[connection=middle_ns,facing=east]";
        assert_eq!(property(s, "connection"), Some("middle_ns"));
        assert_eq!(property(s, "facing"), Some("east"));
        assert_eq!(property(s, "missing"), None);
    }

    #[test]
    fn torch_particle_positions_match_original() {
        assert_eq!(
            torch_particle_pos([10, 64, 20], Horizontal::East, "bridge"),
            [10.5, 64.5, 20.05]
        );
        assert_eq!(
            torch_particle_pos([10, 64, 20], Horizontal::West, "stair"),
            [10.5, 64.9, 20.95]
        );
    }
}
