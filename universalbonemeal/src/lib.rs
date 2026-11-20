//! universalbonemeal: Universal Bone Meal 8.0.1 (Fuzs, MPL-2.0). The work is game-side
//! (runtime.rs and the mixins on GameLevel's bone meal methods); this crate has no hooks.
use mod_api::{self, Hooks};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {}
