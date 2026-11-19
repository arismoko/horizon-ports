//! Block Runner 8.0.4 (Fuzs) for Horizon, with Homestead's config: entities run faster on
//! `minecraft:dirt_path` (x1.35) and `#minecraft:stone_bricks` (x1.15; the mod's own block tags
//! are empty in the pack).
//!
//! A `BlockPart` on those blocks edits `block_speed_factor` (`Block.getSpeedFactor`), which every
//! living entity's move reads for the block at its feet (a path: 15/16 tall, the feet stand in it)
//! or, when that gives 1.0, the block below (stone bricks). It runs on both sides and on the
//! client's replayed ticks, so the player's own prediction agrees with the server, LAN guests too.
//!
//! The Java mod multiplies the `movement_speed` attribute. A speed factor multiplies the velocity
//! after each move instead (`Entity.move`), with the ground's slipperiness (0.6 x 0.91 = 0.546 on
//! these blocks), so it compounds: each tick moves `v`, then `v = (v·f·s) + a`, and the steady speed
//! is `a / (1 − f·s)` against vanilla's `a / (1 − s)`. `factor_for` picks the `f` that gives the
//! configured ratio on ground of that slipperiness. (Measured in game, walking: dirt 4.39 blocks/s,
//! stone bricks and dirt path see the notes.)
use mod_api::{BlockPart, BlockSpeedFactorEvent, Hooks, LoopContext, Registrar, RegistrationContext};

/// Homestead's Block Runner config: block, speed multiplier.
pub const BLOCKS: &[(&str, f64)] = &[
    ("minecraft:dirt_path", 1.35),
    // `#minecraft:stone_bricks` in 26.3.
    ("minecraft:stone_bricks", 1.15),
    ("minecraft:mossy_stone_bricks", 1.15),
    ("minecraft:cracked_stone_bricks", 1.15),
    ("minecraft:chiseled_stone_bricks", 1.15),
];

/// Slipperiness times air drag on these blocks (`Block.getFriction` 0.6 x 0.91).
const SLIP: f64 = 0.6 * 0.91;

/// The speed factor that makes the steady ground speed `ratio` times vanilla's.
pub fn factor_for(ratio: f64) -> f32 {
    // ratio = (1 − s) / (1 − f·s)  =>  f = (1 − (1 − s) / ratio) / s
    ((1.0 - (1.0 - SLIP) / ratio) / SLIP) as f32
}

struct Fast(f32);

#[mod_api::horizon_part]
impl BlockPart for Fast {
    fn block_speed_factor(&self, _: &dyn LoopContext, event: &mut BlockSpeedFactorEvent) {
        event.factor = self.0;
    }
}

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        for &(block, ratio) in BLOCKS {
            let key = r.block_key(block);
            r.block_part(key, Fast(factor_for(ratio)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steady(f: f64) -> f64 {
        1.0 / (1.0 - f * SLIP)
    }

    #[test]
    fn factors_give_the_configured_ratio() {
        for ratio in [1.35, 1.15] {
            let f = factor_for(ratio) as f64;
            assert!((steady(f) / steady(1.0) - ratio).abs() < 1e-6, "{ratio}");
        }
        assert!((factor_for(1.0) - 1.0).abs() < 1e-6);
        // Below 1 / SLIP: the speed stays bounded.
        assert!((factor_for(1.35) as f64) * SLIP < 1.0);
    }
}
