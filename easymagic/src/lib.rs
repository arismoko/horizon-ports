//! easymagic: a mixin bundle. Easy Magic 8.0.1 (Fuzs) with Homestead's config
//! (easymagic-server.toml: max_enchanting_power 15, reroll_enchantments, lenient_bookshelves,
//! reroll_catalyst_cost 1, reroll_experience_points_cost 5, enchantment_hint SINGLE,
//! chiseled_bookshelf_enchanting_power FACING, dedicated_reroll_catalyst false):
//! - a re-roll button on the enchanting screen: 1 lapis and 5 experience points (free in
//! creative) give the player a new enchantment seed, so new offers;
//! - the table keeps its item and lapis when the screen closes, and drops them when it's broken;
//! - a block between the table and a bookshelf only blocks it when it's a full block (torches,
//! carpets, slabs don't);
//! - a chiseled bookshelf facing the table gives one power per three books.
//! max_enchanting_power 15 and the single hint are vanilla's already. Not ported: the item drawn
//! on the table, hoppers and comparators, the hint list option.
use mod_api::{self, Hooks};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {}
