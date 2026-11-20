//! universalgraves: a mixin bundle. Universal Graves 3.0.3 (Patbox), lite, with Homestead's
//! config (protection 900 s, no expiry, experience percent_points 100, no grave limit):
//! - a player who dies (keep_inventory off) leaves a grave instead of drops: a skeleton skull on
//! the nearest free block within 8, holding every stack with its slot and the experience
//! vanilla would drop; the player is told where;
//! - a right click on the grave by its owner, by anyone after 15 minutes, or in creative gives
//! the stacks back to the slots they came from and the experience (quick pickup);
//! - breaking it (same rule) drops everything there; others are told the grave isn't theirs;
//! - a grave whose skull is gone otherwise (explosion, piston, command) drops its contents, and
//! the owner hears of it.
//! Graves are saved with the level (a level attachment, universal_graves:graves).
//! Not ported: the grave screen, the grave model and its text, the death compass, /graves and
//! teleporting, unlock costs.
use mod_api::{self, Hooks};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {}
