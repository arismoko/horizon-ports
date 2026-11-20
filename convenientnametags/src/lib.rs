//! convenientnametags: a mixin bundle. As Convenient Name Tags 1.1.0 (mim1q) with Homestead's
//! config:
//! - a named mob that dies drops a name tag with its name (dropNameTagsOnDeath);
//! - sneak and use shears on a named mob: the name comes off as a name tag, the shears take 1
//! damage, and the mob's own interaction doesn't run (enableNameTagShearing);
//! - naming a named mob drops its old name as a tag; a tag with the same name isn't used up
//! (dropNameTagsOnNameChange).
//! The rename screen (use a name tag in the air) is not ported. The recipe is in mods/homestead.
use mod_api::{self, Hooks};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {}
