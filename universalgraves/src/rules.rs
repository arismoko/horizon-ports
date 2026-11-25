//! The rules of a grave that need no game: who may open it, when it spills, where it goes and what it saves.
use mod_api::{Compound, Tag};

/// Game ticks in a second.
pub const TICKS: i64 = 20;
/// `placement.max_distance_from_source_location`: how far from where the player died a grave may stand.
pub const MAX_DISTANCE: i32 = 8;

/// The two timers, from the settings. 0 means the timer is off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timers {
    /// `protection.non_owner_protection_time`: ticks only the owner may take the grave.
    pub protection: i64,
    /// Ticks until the grave spills its items where it stands.
    pub expiry: i64,
}

impl Timers {
    pub fn from_seconds(protection: f64, expiry: f64) -> Self {
        let ticks = |s: f64| (s.max(0.0) * TICKS as f64).round() as i64;
        Self {
            protection: ticks(protection),
            expiry: ticks(expiry),
        }
    }
}

/// `GraveBlock.onUse` / `onBreak`: the owner, anyone in creative, and anyone once the protection is over.
pub fn may_take(
    timers: Timers,
    owner: u128,
    who: u128,
    creative: bool,
    created: i64,
    now: i64,
) -> bool {
    creative || owner == who || now.saturating_sub(created) >= timers.protection
}

/// Whether the grave has outlived its expiry.
pub fn expired(timers: Timers, created: i64, now: i64) -> bool {
    timers.expiry > 0 && now.saturating_sub(created) >= timers.expiry
}

/// The offsets from the death position to try, nearest first (the position itself, then outwards), within `max`
/// blocks along each axis and `max` blocks in a straight line.
pub fn offsets(max: i32) -> Vec<[i32; 3]> {
    let mut all = Vec::new();
    for dx in -max..=max {
        for dy in -max..=max {
            for dz in -max..=max {
                let d2 = dx * dx + dy * dy + dz * dz;
                if d2 <= max * max {
                    all.push((d2, dy.abs(), [dx, dy, dz]));
                }
            }
        }
    }
    all.sort();
    all.into_iter().map(|(_, _, o)| o).collect()
}

/// Whether a grave may replace the block with this id (air and water, as a skull would stand there).
pub fn replaceable(id: &str) -> bool {
    matches!(
        id,
        "minecraft:air"
            | "minecraft:cave_air"
            | "minecraft:void_air"
            | "minecraft:water"
            | "minecraft:short_grass"
            | "minecraft:tall_grass"
            | "minecraft:fern"
            | "minecraft:snow"
    )
}

/// What a grave saves besides its items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meta {
    pub owner: u128,
    pub name: String,
    /// The level's game time when it was made.
    pub created: i64,
}

const ITEMS: &str = "items";

/// The grave's saved form: its meta and its already encoded stacks.
pub fn write(meta: &Meta, stacks: Vec<Compound>) -> Compound {
    let mut c = Compound::new();
    c.insert(
        "owner".into(),
        Tag::String(format!("{:032x}", meta.owner).as_str().into()),
    );
    c.insert("name".into(), Tag::String(meta.name.as_str().into()));
    c.insert("created".into(), Tag::Long(meta.created));
    c.insert(
        ITEMS.into(),
        Tag::List(stacks.into_iter().map(Tag::Compound).collect()),
    );
    c
}

/// The meta and the encoded stacks of a saved grave; `None` when it holds no owner (an empty state).
pub fn read(c: &Compound) -> Option<(Meta, Vec<Compound>)> {
    let Some(Tag::String(owner)) = c.get(&"owner".into()) else {
        return None;
    };
    let owner = u128::from_str_radix(&owner.to_string_lossy(), 16).ok()?;
    let name = match c.get(&"name".into()) {
        Some(Tag::String(s)) => s.to_string_lossy(),
        _ => String::new(),
    };
    let created = match c.get(&"created".into()) {
        Some(Tag::Long(v)) => *v,
        _ => 0,
    };
    let stacks = match c.get(&ITEMS.into()) {
        Some(Tag::List(l)) => l
            .iter()
            .filter_map(|t| match t {
                Tag::Compound(c) => Some(c.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    Some((
        Meta {
            owner,
            name,
            created,
        },
        stacks,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Timers = Timers {
        protection: 900 * TICKS,
        expiry: 0,
    };

    #[test]
    fn the_owner_always_takes_it() {
        assert!(may_take(T, 1, 1, false, 100, 100));
    }

    #[test]
    fn a_stranger_waits_out_the_protection() {
        assert!(!may_take(T, 1, 2, false, 100, 100 + 900 * TICKS - 1));
        assert!(may_take(T, 1, 2, false, 100, 100 + 900 * TICKS));
    }

    #[test]
    fn creative_ignores_the_protection() {
        assert!(may_take(T, 1, 2, true, 100, 100));
    }

    #[test]
    fn no_protection_opens_it_to_anyone() {
        let none = Timers::from_seconds(0.0, 0.0);
        assert!(may_take(none, 1, 2, false, 5, 5));
    }

    #[test]
    fn a_clock_that_ran_back_does_not_open_it() {
        assert!(!may_take(T, 1, 2, false, 5_000, 10));
    }

    #[test]
    fn expiry_zero_never_expires() {
        assert!(!expired(T, 0, i64::MAX));
    }

    #[test]
    fn expiry_comes_at_its_tick() {
        let t = Timers::from_seconds(900.0, 60.0);
        assert!(!expired(t, 10, 10 + 1199));
        assert!(expired(t, 10, 10 + 1200));
    }

    #[test]
    fn seconds_become_ticks_and_negative_is_off() {
        assert_eq!(
            Timers::from_seconds(900.0, -3.0),
            Timers {
                protection: 18_000,
                expiry: 0
            }
        );
    }

    #[test]
    fn offsets_start_at_the_death_and_stay_in_range() {
        let o = offsets(MAX_DISTANCE);
        assert_eq!(o[0], [0, 0, 0]);
        assert!(o.iter().all(|[x, y, z]| x * x + y * y + z * z <= 64));
        assert!(o.contains(&[8, 0, 0]));
        assert!(!o.contains(&[8, 1, 0]));
        let d = |v: &[i32; 3]| v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
        assert!(o.windows(2).all(|w| d(&w[0]) <= d(&w[1])));
    }

    #[test]
    fn only_free_blocks_are_replaced() {
        assert!(replaceable("minecraft:air"));
        assert!(replaceable("minecraft:water"));
        assert!(!replaceable("minecraft:stone"));
        assert!(!replaceable("universalgraves:grave"));
        assert!(!replaceable("minecraft:chest"));
    }

    #[test]
    fn a_grave_round_trips() {
        let meta = Meta {
            owner: 0x0123_4567_89ab_cdef_0011_2233_4455_6677,
            name: "Alex".into(),
            created: 123_456,
        };
        let mut stack = Compound::new();
        stack.insert("id".into(), Tag::String("minecraft:stick".into()));
        let saved = write(&meta, vec![stack.clone(), stack.clone()]);
        let (back, stacks) = read(&saved).unwrap();
        assert_eq!(back, meta);
        assert_eq!(stacks, vec![stack.clone(), stack]);
    }

    #[test]
    fn an_empty_state_is_no_grave() {
        assert!(read(&Compound::new()).is_none());
    }

    #[test]
    fn an_owner_of_leading_zeros_survives() {
        let meta = Meta {
            owner: 5,
            name: String::new(),
            created: 0,
        };
        assert_eq!(read(&write(&meta, Vec::new())).unwrap().0.owner, 5);
    }
}
