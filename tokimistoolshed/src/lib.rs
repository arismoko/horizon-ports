//! tokimistoolshed: Tokimi's Toolshed 1.8 (Cozy Studios). The items are this package's data/ and
//! assets/; what the tools do is game-side (runtime.rs and the mixins), since it acts inside the
//! game's own breaking, using and loot code.
use mod_api::Hooks;

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {}
