//! Respect My Trims 1.0.0 (tnoctua): armour with a gold trim makes piglins treat the player as wearing
//! gold. All of it is the mixin in src/mixins/trim.rs.
use mod_api::{self, Hooks};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {}
