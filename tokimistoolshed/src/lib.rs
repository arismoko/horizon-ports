//! tokimistoolshed: Tokimi's Toolshed 1.8 (Cozy Studios). The items are this package's data/ and
//! assets/; what the tools do is game-side (runtime.rs and the mixins), since it acts inside the
//! game's own breaking, using and loot code.
//!
//! Here: whether Falling Trees (native-next/fallingtrees) is loaded, read once at registration.
//! With it, Falling Trees fells the tree a lumber axe chops (on Java its BREAK event takes the
//! whole tree for any axe first); without it, the lumber axe fells the tree itself.
//!
//! `player_tick_post` does nothing itself: the game only runs its player-tick route (where the
//! `abacus` mixin draws the Abacus HUD) when some mod listens to the hook, so this one does.
use std::sync::atomic::{AtomicBool, Ordering};

use mod_api::{Hooks, HostSide, PlayerTickPostEvent, RegistrationContext, Registrar};

static FALLING_TREES: AtomicBool = AtomicBool::new(false);

/// Whether the fallingtrees package is loaded (runtime.rs asks before the lumber axe fells).
pub fn falling_trees() -> bool {
    FALLING_TREES.load(Ordering::Relaxed)
}

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, _r: &mut dyn Registrar, cx: &RegistrationContext) {
        FALLING_TREES.store(cx.mods().get("fallingtrees").is_some(), Ordering::Relaxed);
    }

    /// Subscribes the player tick the `abacus` mixin (native_player_tick_post) runs on.
    fn player_tick_post(&self, _host: HostSide<'_>, _event: &mut PlayerTickPostEvent) {}
}
