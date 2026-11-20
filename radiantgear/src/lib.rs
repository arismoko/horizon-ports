//! radiantgear: Radiant Gear 2.1.6 (Illusive Soulworks, LGPL-3.0-or-later). Worn trinkets that glow
//! light up with Horizon Extras' dynamic lights. It reads native/trinkets' `trinkets:worn` player
//! attachment and needs none of its code. ITEM_LIGHTS (radiantgear_data.rs) is written by
//! tools/trinkets_port.py.
use mod_api::{self, Hooks};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {}
