//! invisibleframes: a mixin bundle. As Invisible Frames 1.1.0 (Roundaround): sneak and use an item
//! frame to toggle its invisibility (the item in it stays drawn); a hit the frame accepts (a
//! player's or a projectile's) makes it visible again as it knocks the item out.
use mod_api::{self, Hooks};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {}
