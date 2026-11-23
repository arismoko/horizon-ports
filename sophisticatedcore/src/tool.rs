//! The storage tool (StorageToolItem): one item with a mode. Use it on a storage to act on it by the mode, use it in
//! the air to move to the next mode (sneaking: to clear the controller it holds, or else go back a mode).
//!
//! The mode and the controller position live in the tool's custom data (`mode`, `controllerPos` as a block position's
//! long); the flags it toggles live in the storage's own stack (`locked`, `showLock`, `showTier`, `showUpgrades`,
//! `showCounts`, `showFillLevels`).
use mod_api::{Compound, ItemStack, LevelRef, ServerHost, Tag};

use crate::contents::{Nbt, custom_data, get_bool, get_str, put, set_custom_data};
use crate::storage::{self, kind_of_block};
use crate::storage_items::Family;

pub const TOOL: &str = "sophisticatedstorage:storage_tool";
pub const CONTROLLER: &str = "sophisticatedstorage:controller";

const MODE: &str = "mode";
const CONTROLLER_POS: &str = "controllerPos";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Link,
    Lock,
    CountDisplay,
    LockDisplay,
    TierDisplay,
    UpgradesDisplay,
    FillLevelDisplay,
}

pub const MODES: [Mode; 7] = [
    Mode::Link,
    Mode::Lock,
    Mode::CountDisplay,
    Mode::LockDisplay,
    Mode::TierDisplay,
    Mode::UpgradesDisplay,
    Mode::FillLevelDisplay,
];

impl Mode {
    /// The name the tool keeps (the enum constant's).
    pub fn name(self) -> &'static str {
        match self {
            Self::Link => "LINK",
            Self::Lock => "LOCK",
            Self::CountDisplay => "COUNT_DISPLAY",
            Self::LockDisplay => "LOCK_DISPLAY",
            Self::TierDisplay => "TIER_DISPLAY",
            Self::UpgradesDisplay => "UPGRADES_DISPLAY",
            Self::FillLevelDisplay => "FILL_LEVEL_DISPLAY",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        MODES.into_iter().find(|m| m.name() == name)
    }

    fn index(self) -> usize {
        MODES.iter().position(|m| *m == self).unwrap_or(0)
    }

    pub fn next(self) -> Self {
        MODES[(self.index() + 1) % MODES.len()]
    }

    pub fn previous(self) -> Self {
        MODES[(self.index() + MODES.len() - 1) % MODES.len()]
    }

    /// The flag it toggles on a storage, its default, and whether a storage of `family` has it.
    fn flag(self, family: Family) -> Option<(&'static str, bool)> {
        let barrel = family == Family::LimitedBarrel;
        match self {
            Self::Link => None,
            Self::Lock => Some(("locked", false)),
            Self::LockDisplay => Some(("showLock", true)),
            Self::TierDisplay => Some(("showTier", true)),
            Self::UpgradesDisplay => Some(("showUpgrades", false)),
            Self::CountDisplay => barrel.then_some(("showCounts", true)),
            Self::FillLevelDisplay => barrel.then_some(("showFillLevels", false)),
        }
    }

    /// The overlay line for the mode (`getOverlayMessage`).
    pub fn message(self, link: Option<[i32; 3]>) -> String {
        match (self, link) {
            (Self::Link, Some([x, y, z])) => format!("Linking to the controller at {x}, {y}, {z}"),
            (Self::Link, None) => "Unlinking from controllers".into(),
            (Self::Lock, _) => "Toggling lock".into(),
            (Self::LockDisplay, _) => "Toggling lock display".into(),
            (Self::CountDisplay, _) => "Toggling count display".into(),
            (Self::TierDisplay, _) => "Toggling tier display".into(),
            (Self::UpgradesDisplay, _) => "Toggling upgrades display".into(),
            (Self::FillLevelDisplay, _) => "Toggling fill level display".into(),
        }
    }
}

pub fn mode(tool: &ItemStack) -> Mode {
    get_str(&custom_data(tool), MODE)
        .and_then(|n| Mode::parse(&n))
        .unwrap_or(Mode::Link)
}

pub fn set_mode(tool: &mut ItemStack, m: Mode) {
    let mut d = custom_data(tool);
    put(&mut d, MODE, Tag::String(m.name().into()));
    set_custom_data(tool, d);
}

/// `BlockPos.asLong`: x in the top 26 bits, z in the next 26, y in the low 12.
pub fn pack(pos: [i32; 3]) -> i64 {
    ((i64::from(pos[0]) & 0x3FF_FFFF) << 38)
        | ((i64::from(pos[2]) & 0x3FF_FFFF) << 12)
        | (i64::from(pos[1]) & 0xFFF)
}

/// `BlockPos.of(long)`.
pub fn unpack(v: i64) -> [i32; 3] {
    [
        (v >> 38) as i32,
        ((v << 52) >> 52) as i32,
        ((v << 26) >> 38) as i32,
    ]
}

fn get_long(c: &Compound, k: &str) -> Option<i64> {
    match c.at(k)? {
        Tag::Long(v) => Some(*v),
        _ => None,
    }
}

/// The controller the tool holds.
pub fn controller_link(tool: &ItemStack) -> Option<[i32; 3]> {
    get_long(&custom_data(tool), CONTROLLER_POS).map(unpack)
}

pub fn set_controller_link(tool: &mut ItemStack, pos: Option<[i32; 3]>) {
    let mut d = custom_data(tool);
    match pos {
        Some(p) => put(&mut d, CONTROLLER_POS, Tag::Long(pack(p))),
        None => {
            d.del(CONTROLLER_POS);
        }
    }
    set_custom_data(tool, d);
}

/// What using the tool in the air does to `tool` (`use`, and the mode cycle): the change, and the overlay line.
pub fn use_in_air(tool: &mut ItemStack, sneaking: bool) -> String {
    if sneaking && controller_link(tool).is_some() {
        set_controller_link(tool, None);
    } else {
        let m = mode(tool);
        set_mode(tool, if sneaking { m.previous() } else { m.next() });
    }
    mode(tool).message(controller_link(tool))
}

/// What `use_on` did: whether the click was the tool's, and what the tool now holds.
pub enum Used {
    /// Not for the tool: the click goes on as it would.
    Passed,
    /// Handled; the tool may have changed.
    Done(Option<[i32; 3]>),
}

/// StorageToolItem.onItemUseFirst on the block at `pos`.
pub fn use_on(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    block: &str,
    tool: &ItemStack,
) -> Used {
    let m = mode(tool);
    if m == Mode::Link {
        if block == CONTROLLER {
            return Used::Done(Some(pos));
        }
        if kind_of_block(block).is_none() {
            return Used::Passed;
        }
        let Some(mut st) = storage::read_be(&*host, level, pos) else {
            return Used::Passed;
        };
        let mut d = custom_data(&st);
        match controller_link(tool) {
            Some(c) => put(&mut d, CONTROLLER_POS, Tag::Long(pack(c))),
            None => {
                d.del(CONTROLLER_POS);
            }
        }
        set_custom_data(&mut st, d);
        storage::write_be(host, level, pos, Some(st));
        return Used::Done(controller_link(tool));
    }
    let Some(kind) = kind_of_block(block) else {
        return Used::Passed;
    };
    let Some((key, default)) = m.flag(kind.family) else {
        return Used::Passed;
    };
    let Some(mut st) = storage::read_be(&*host, level, pos) else {
        return Used::Passed;
    };
    let mut d = custom_data(&st);
    let now = get_bool(&d, key).unwrap_or(default);
    put(&mut d, key, Tag::Byte(i8::from(!now)));
    set_custom_data(&mut st, d);
    storage::write_be(host, level, pos, Some(st));
    Used::Done(controller_link(tool))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_cycle_both_ways() {
        assert_eq!(Mode::Link.next(), Mode::Lock);
        assert_eq!(Mode::FillLevelDisplay.next(), Mode::Link);
        assert_eq!(Mode::Link.previous(), Mode::FillLevelDisplay);
        assert_eq!(Mode::parse("TIER_DISPLAY"), Some(Mode::TierDisplay));
        assert_eq!(Mode::parse("nope"), None);
    }

    #[test]
    fn a_position_round_trips_as_a_long() {
        for p in [
            [0, 0, 0],
            [100, 64, -200],
            [-30_000_000, -64, 29_999_999],
            [1, 319, 1],
        ] {
            assert_eq!(unpack(pack(p)), p);
        }
        // BlockPos.asLong(1, 2, 3): x << 38 | z << 12 | y.
        assert_eq!(pack([1, 2, 3]), (1 << 38) | (3 << 12) | 2);
    }

    #[test]
    fn only_barrels_have_count_and_fill_displays() {
        assert!(Mode::CountDisplay.flag(Family::Chest).is_none());
        assert_eq!(
            Mode::CountDisplay.flag(Family::LimitedBarrel),
            Some(("showCounts", true))
        );
        assert_eq!(
            Mode::UpgradesDisplay.flag(Family::Chest),
            Some(("showUpgrades", false))
        );
        assert!(Mode::Link.flag(Family::Chest).is_none());
    }
}
