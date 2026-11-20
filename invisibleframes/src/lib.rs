//! invisibleframes: a mixin bundle. As Invisible Frames 1.1.0 (Roundaround): sneak and use an item
//! frame to toggle its invisibility (the item in it stays drawn); hitting a frame makes it visible
//! again before the hit does what it always does.
use mod_api::{self, Hooks};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {}
