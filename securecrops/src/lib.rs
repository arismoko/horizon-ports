//! Secure Crops 2.0.0 (zekk051) for Horizon, with Homestead's `preferred-gamerules.properties`
//! (`secureFarmlandAndCrops` and `secureFarmlandIfEmpty` true, `secureFarmlandBreakCrops` false):
//! nothing tramples farmland, with or without a crop, player or mob. Fall damage still happens.
//!
//! `farmland_trample` (NeoForge's `FarmlandTrampleEvent`) fires on the server before a landing
//! turns farmland into dirt; cancelling it keeps the farmland and its crop. The gamerules aren't
//! read: the pack's values are built in.
use mod_api::{FarmlandTrampleEvent, Flow, Hooks, LoopContext};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn farmland_trample(&self, _context: &dyn LoopContext, event: &mut FarmlandTrampleEvent) -> Flow {
        event.cancelled = true;
        Flow::Cancel
    }
}
