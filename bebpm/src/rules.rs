//! The pure rules: settings, scan offsets, the line between table and shelf, and what blocks it.

pub const KEY_X: &str = "radius_x";
pub const KEY_Z: &str = "radius_z";
pub const KEY_UP: &str = "radius_up";
pub const KEY_DOWN: &str = "radius_down";
pub const KEY_LOS: &str = "line_of_sight";
pub const DEFAULT_RADIUS: i32 = 10;
pub const MAX_RADIUS: i32 = 16;

/// Vanilla's ring reaches 2 blocks out sideways, 1 up and 0 down; the box never shrinks below it.
const MIN_SIDE: i32 = 2;
const MIN_UP: i32 = 1;
const MIN_DOWN: i32 = 0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineOfSight {
    /// Every shelf in the box counts.
    Off,
    /// Full-collision blocks between table and shelf block it, unless `#bebpm:power_permeable`.
    #[default]
    DecorOnly,
    /// Every block between must transmit (`#minecraft:enchantment_power_transmitter`).
    On,
}

impl LineOfSight {
    pub fn index(self) -> u32 {
        match self {
            Self::Off => 0,
            Self::DecorOnly => 1,
            Self::On => 2,
        }
    }

    /// A stored choice: its index, or its name.
    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim().trim_matches('"');
        if let Ok(index) = raw.parse::<u32>() {
            return match index {
                0 => Some(Self::Off),
                1 => Some(Self::DecorOnly),
                2 => Some(Self::On),
                _ => None,
            };
        }
        let name = raw.to_ascii_lowercase();
        let name = name.rsplit('.').next().unwrap_or("");
        match name {
            "off" => Some(Self::Off),
            "decor_only" => Some(Self::DecorOnly),
            "on" => Some(Self::On),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub x: i32,
    pub z: i32,
    pub up: i32,
    pub down: i32,
    pub los: LineOfSight,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            x: DEFAULT_RADIUS,
            z: DEFAULT_RADIUS,
            up: DEFAULT_RADIUS,
            down: DEFAULT_RADIUS,
            los: LineOfSight::default(),
        }
    }
}

fn radius(raw: Option<String>, min: i32) -> i32 {
    let value = raw
        .and_then(|raw| raw.trim().trim_matches('"').parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .map_or(DEFAULT_RADIUS, |v| v.round() as i32);
    value.clamp(min, MAX_RADIUS)
}

impl Config {
    pub fn from_settings(read: impl Fn(&str) -> Option<String>) -> Self {
        Self {
            x: radius(read(KEY_X), MIN_SIDE),
            z: radius(read(KEY_Z), MIN_SIDE),
            up: radius(read(KEY_UP), MIN_UP),
            down: radius(read(KEY_DOWN), MIN_DOWN),
            los: read(KEY_LOS)
                .and_then(|raw| LineOfSight::parse(&raw))
                .unwrap_or_default(),
        }
    }

    /// Whether `offset` is inside the box (and not the table itself).
    pub fn contains(&self, offset: [i32; 3]) -> bool {
        offset != [0, 0, 0]
            && offset[0].abs() <= self.x
            && offset[2].abs() <= self.z
            && (-self.down..=self.up).contains(&offset[1])
    }

    /// Every offset of the box, for `add_enchanting_table_offsets`.
    pub fn offsets(&self) -> Vec<[i32; 3]> {
        let mut out = Vec::new();
        for x in -self.x..=self.x {
            for z in -self.z..=self.z {
                for y in -self.down..=self.up {
                    if [x, y, z] != [0, 0, 0] {
                        out.push([x, y, z]);
                    }
                }
            }
        }
        out
    }
}

/// What the line test needs to know about one block between the table and the shelf.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    pub transmitter: bool,
    pub permeable: bool,
    pub full: bool,
}

/// The block offsets from the table along the straight line to `offset`, both ends left out.
pub fn line_between(offset: [i32; 3]) -> Vec<[i32; 3]> {
    let n = offset.iter().map(|v| v.abs()).max().unwrap_or(0);
    let mut out: Vec<[i32; 3]> = Vec::new();
    for i in 1..n {
        let step = offset.map(|v| (f64::from(v) * f64::from(i) / f64::from(n)).round() as i32);
        if out.last() != Some(&step) {
            out.push(step);
        }
    }
    out
}

/// Whether the line to `offset` is clear in `mode`. `cell` answers for a block offset from the
/// table; `None` (an unreadable block) blocks, except with the line of sight off.
pub fn line_clear(
    mode: LineOfSight,
    offset: [i32; 3],
    cell: impl Fn([i32; 3]) -> Option<Cell>,
) -> bool {
    if mode == LineOfSight::Off {
        return true;
    }
    line_between(offset).into_iter().all(|step| {
        cell(step).is_some_and(|c| match mode {
            LineOfSight::Off => true,
            LineOfSight::DecorOnly => !c.full || c.permeable,
            LineOfSight::On => c.transmitter,
        })
    })
}

/// `BlockState.isCollisionShapeFullBlock`: the boxes fill the unit cube exactly, nothing outside.
pub fn is_full_block(boxes: &[[f64; 6]]) -> bool {
    if boxes
        .iter()
        .any(|b| b[0] < 0.0 || b[1] < 0.0 || b[2] < 0.0 || b[3] > 1.0 || b[4] > 1.0 || b[5] > 1.0)
    {
        return false;
    }
    let edges = |axis: usize| {
        let mut at: Vec<f64> = [0.0, 1.0]
            .into_iter()
            .chain(boxes.iter().flat_map(|b| [b[axis], b[axis + 3]]))
            .collect();
        at.sort_by(f64::total_cmp);
        at.dedup();
        at.windows(2)
            .map(|w| (w[0] + w[1]) / 2.0)
            .collect::<Vec<_>>()
    };
    let (xs, ys, zs) = (edges(0), edges(1), edges(2));
    xs.iter().all(|&x| {
        ys.iter().all(|&y| {
            zs.iter().all(|&z| {
                boxes.iter().any(|b| {
                    b[0] <= x && x <= b[3] && b[1] <= y && y <= b[4] && b[2] <= z && z <= b[5]
                })
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(pairs: &'static [(&'static str, &'static str)]) -> Config {
        Config::from_settings(|key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_owned())
        })
    }

    #[test]
    fn defaults_are_radius_ten() {
        let config = settings(&[]);
        assert_eq!(config, Config::default());
        assert_eq!(config.offsets().len(), 21 * 21 * 21 - 1);
    }

    #[test]
    fn radii_clamp_between_vanillas_ring_and_sixteen() {
        let config = settings(&[
            (KEY_X, "0"),
            (KEY_Z, "99"),
            (KEY_UP, "0.0"),
            (KEY_DOWN, "-4"),
        ]);
        assert_eq!((config.x, config.z, config.up, config.down), (2, 16, 1, 0));
        let junk = settings(&[(KEY_X, "abc"), (KEY_Z, "NaN")]);
        assert_eq!((junk.x, junk.z), (10, 10));
    }

    #[test]
    fn box_covers_vanillas_offsets_and_skips_the_table() {
        let config = settings(&[(KEY_X, "0"), (KEY_Z, "0"), (KEY_UP, "0"), (KEY_DOWN, "0")]);
        let offsets = config.offsets();
        for x in -2_i32..=2 {
            for z in -2_i32..=2 {
                if x.abs() == 2 || z.abs() == 2 {
                    for y in 0..=1 {
                        assert!(offsets.contains(&[x, y, z]));
                        assert!(config.contains([x, y, z]));
                    }
                }
            }
        }
        assert!(!offsets.contains(&[0, 0, 0]));
        assert!(!config.contains([0, 0, 0]));
        assert!(!config.contains([3, 0, 0]));
        assert!(!config.contains([2, -1, 2]));
    }

    #[test]
    fn box_extends_by_axis() {
        let config = settings(&[(KEY_X, "5"), (KEY_Z, "3"), (KEY_UP, "4"), (KEY_DOWN, "6")]);
        assert!(config.contains([5, 4, 3]));
        assert!(config.contains([-5, -6, -3]));
        assert!(!config.contains([6, 0, 0]));
        assert!(!config.contains([0, 5, 0]));
        assert!(!config.contains([0, -7, 0]));
        assert!(!config.contains([0, 0, 4]));
    }

    #[test]
    fn line_of_sight_parses_index_or_name() {
        assert_eq!(LineOfSight::parse("0"), Some(LineOfSight::Off));
        assert_eq!(LineOfSight::parse("\"2\""), Some(LineOfSight::On));
        assert_eq!(
            LineOfSight::parse("DECOR_ONLY"),
            Some(LineOfSight::DecorOnly)
        );
        assert_eq!(
            LineOfSight::parse("setting.bebpm.line_of_sight.on"),
            Some(LineOfSight::On)
        );
        assert_eq!(LineOfSight::parse("3"), None);
        assert_eq!(LineOfSight::parse("maybe"), None);
        assert_eq!(settings(&[(KEY_LOS, "1")]).los, LineOfSight::DecorOnly);
        assert_eq!(settings(&[(KEY_LOS, "x")]).los, LineOfSight::default());
        for mode in [LineOfSight::Off, LineOfSight::DecorOnly, LineOfSight::On] {
            assert_eq!(LineOfSight::parse(&mode.index().to_string()), Some(mode));
        }
    }

    #[test]
    fn line_leaves_out_both_ends() {
        assert!(line_between([1, 0, 0]).is_empty());
        assert_eq!(line_between([3, 0, 0]), vec![[1, 0, 0], [2, 0, 0]]);
        assert_eq!(line_between([0, -3, 0]), vec![[0, -1, 0], [0, -2, 0]]);
        let diagonal = line_between([4, 2, -4]);
        assert_eq!(diagonal.len(), 3);
        assert_eq!(diagonal[1], [2, 1, -2]);
    }

    fn cells(blocking: [i32; 3], cell: Cell) -> impl Fn([i32; 3]) -> Option<Cell> {
        move |at| {
            Some(if at == blocking {
                cell
            } else {
                Cell {
                    transmitter: true,
                    ..Cell::default()
                }
            })
        }
    }

    #[test]
    fn off_ignores_everything_between() {
        assert!(line_clear(LineOfSight::Off, [4, 0, 0], |_| None));
    }

    #[test]
    fn decor_only_blocks_on_full_non_permeable_blocks() {
        let stone = Cell {
            full: true,
            ..Cell::default()
        };
        let glass = Cell {
            full: true,
            permeable: true,
            ..Cell::default()
        };
        let fence = Cell::default();
        let mode = LineOfSight::DecorOnly;
        assert!(!line_clear(mode, [4, 0, 0], cells([2, 0, 0], stone)));
        assert!(line_clear(mode, [4, 0, 0], cells([2, 0, 0], glass)));
        assert!(line_clear(mode, [4, 0, 0], cells([2, 0, 0], fence)));
        assert!(!line_clear(mode, [4, 0, 0], |_| None));
        // Not on the line.
        assert!(line_clear(mode, [4, 0, 0], cells([0, 1, 0], stone)));
    }

    #[test]
    fn on_needs_every_block_to_transmit() {
        let solid = Cell::default();
        let open = Cell {
            transmitter: true,
            ..Cell::default()
        };
        assert!(line_clear(
            LineOfSight::On,
            [4, 0, 0],
            cells([2, 0, 0], open)
        ));
        assert!(!line_clear(
            LineOfSight::On,
            [4, 0, 0],
            cells([2, 0, 0], solid)
        ));
        assert!(!line_clear(LineOfSight::On, [4, 0, 0], |_| None));
        // Next to the table nothing lies between.
        assert!(line_clear(LineOfSight::On, [1, 0, 0], |_| None));
    }

    #[test]
    fn full_block_detection() {
        assert!(is_full_block(&[[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]]));
        assert!(!is_full_block(&[]));
        assert!(!is_full_block(&[[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]]));
        assert!(is_full_block(&[
            [0.0, 0.0, 0.0, 1.0, 0.5, 1.0],
            [0.0, 0.5, 0.0, 1.0, 1.0, 1.0]
        ]));
        assert!(!is_full_block(&[[0.0, 0.0, 0.0, 1.0, 1.5, 1.0]]));
    }
}
