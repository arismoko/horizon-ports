//! craftingpad: a mixin bundle. As Crafting Pad 1.0.12 (StrikerRocker): using the crafting pad
//! opens a 3x3 crafting grid wherever the player is. The item itself (texture, recipe: a
//! crafting table over a stick) is in the Homestead dev mod, `mods/homestead`, namespace
//! `craftingpad`.
use mod_api::{self, Hooks};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {}
