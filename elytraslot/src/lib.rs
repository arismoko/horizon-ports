//! elytraslot: Elytra Slot 6.4.4 (Illusive Soulworks, LGPL-3.0-or-later). An elytra worn in a trinket
//! slot (native/trinkets' cape slot) glides, wears and draws its wings. It reads native/trinkets'
//! `trinkets:worn` player attachment and needs none of its code.
use mod_api::{self, Hooks};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {}
