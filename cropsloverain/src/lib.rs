//! Crops Love Rain for Horizon: crops and sugar cane grow faster while it rains on them, as the
//! Fabric mod `crops_love_rain` 1.4.0 does with Homestead's config (`rainGrowthSpeed = 3`).
//!
//! The Java mod, at the start of a crop's random tick: if it's lit (raw brightness >= 9), not
//! fully grown and `level.isRainingAt(pos)`, then with chance 1/3 it adds one age stage.
//! Sugar cane the same, while the cane is less than 3 tall: +1 age, or at age 15 a new cane on
//! top.
//!
//! Horizon's `crop_grow_pre` hook (NeoForge's `CropGrowEvent.Pre`) fires where one of these is
//! about to roll its own growth on a random tick: when it rains on it and it is lit, one roll in
//! `RAIN_GROWTH_SPEED` answers `True`, which grows it one stage now (a new cane on top for sugar
//! cane at age 15, as vanilla's own growth does). Vanilla's roll is still drawn, so a rained-on
//! crop grows with chance 1/3 + 2/3 of vanilla's per tick, where the Java mod could add its stage
//! on top of vanilla's in the same tick.
use std::sync::Mutex;

use mod_api::{CropGrowPreEvent, Hooks, LevelRandomHost, TriState};

/// Homestead's `rainGrowthSpeed`: one extra stage with chance 1 in this.
pub const RAIN_GROWTH_SPEED: u64 = 3;

/// The vanilla `CropBlock`s and their last age.
pub fn crop_max_age(block: &str) -> Option<u32> {
    match block {
        "minecraft:wheat" | "minecraft:carrots" | "minecraft:potatoes" => Some(7),
        "minecraft:beetroots" => Some(3),
        _ => None,
    }
}

/// What to do with a rained-on block: the new `age`, if it grows.
#[derive(Debug, PartialEq)]
pub enum Growth {
    Age(u32),
    /// Sugar cane at age 15: a new cane on top, this one back to age 0.
    NewCane,
}

/// The growth for `block` at `age` (with `height` canes in its column counting itself, for
/// sugar cane), before the 1-in-`RAIN_GROWTH_SPEED` roll.
pub fn growth(block: &str, age: u32, height: u32) -> Option<Growth> {
    if let Some(max) = crop_max_age(block) {
        return (age < max).then_some(Growth::Age(age + 1));
    }
    if block == "minecraft:sugar_cane" && height < 3 {
        return Some(if age >= 15 { Growth::NewCane } else { Growth::Age(age + 1) });
    }
    None
}

/// A small xorshift: the roll only needs to be fair, not vanilla's stream.
fn roll(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

struct State {
    seed: u64,
}

struct Mod {
    state: Mutex<State>,
}

impl Default for Mod {
    fn default() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15)
            | 1;
        Mod { state: Mutex::new(State { seed }) }
    }
}

impl Mod {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn crop_grow_pre(&self, level: &mut dyn LevelRandomHost, event: &mut CropGrowPreEvent<'_>) {
        if *event.result != TriState::Default {
            return;
        }
        let Some(state) = level.state_string(event.state) else { return };
        let block = state.split('[').next().unwrap_or("");
        if crop_max_age(block).is_none() && block != "minecraft:sugar_cane" {
            return;
        }
        let p = [event.pos.x, event.pos.y, event.pos.z];
        // The Java mod: lit (raw brightness >= 9) and `level.isRainingAt(pos)`.
        if level.raw_brightness(p, 0) < 9 || !level.is_raining_at(p) {
            return;
        }
        if roll(&mut self.lock().seed) % RAIN_GROWTH_SPEED == 0 {
            *event.result = TriState::True;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crops_grow_one_stage_until_grown() {
        assert_eq!(growth("minecraft:wheat", 3, 1), Some(Growth::Age(4)));
        assert_eq!(growth("minecraft:wheat", 7, 1), None);
        assert_eq!(growth("minecraft:beetroots", 2, 1), Some(Growth::Age(3)));
        assert_eq!(growth("minecraft:beetroots", 3, 1), None);
    }

    #[test]
    fn other_aged_blocks_are_not_crops() {
        assert_eq!(growth("minecraft:melon_stem", 3, 1), None);
        assert_eq!(growth("minecraft:nether_wart", 1, 1), None);
        assert_eq!(growth("minecraft:cactus", 1, 1), None);
    }

    #[test]
    fn sugar_cane_ages_then_grows_up_to_three_tall() {
        assert_eq!(growth("minecraft:sugar_cane", 4, 1), Some(Growth::Age(5)));
        assert_eq!(growth("minecraft:sugar_cane", 15, 2), Some(Growth::NewCane));
        assert_eq!(growth("minecraft:sugar_cane", 15, 3), None);
    }

    #[test]
    fn the_roll_hits_about_one_in_three() {
        let mut seed = 12345u64;
        let hits = (0..30_000).filter(|_| roll(&mut seed) % RAIN_GROWTH_SPEED == 0).count();
        assert!((9_000..11_000).contains(&hits), "{hits}");
    }
}
