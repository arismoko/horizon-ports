//! bebpm: a mixin bundle. As Better Enchanting Bookshelf Placement 1.0.0 (Cozy Studios) with
//! Homestead's config (radius 10 in every direction, line of sight OFF): every
//! `#minecraft:enchantment_power_provider` block within 10 blocks of the table counts, with
//! nothing needed in between. The menu still caps the power at 15 bookshelves.
use mod_api::{self, Hooks};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {}
