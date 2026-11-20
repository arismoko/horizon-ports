//! Too Cheap 1.4.0 for Horizon, with Homestead's config (`maxLevelCost` 2147483647, every cost
//! option vanilla): the anvil has no "Too Expensive!" cap at 40 levels.
//!
//! The work is game-side (runtime.rs and its mixins): the game asks the anvil hooks only when a
//! mod listens to `anvil_update`, and its dispatcher is where the dropped result comes back. This
//! listener changes nothing itself.
use mod_api::{AnvilUpdateEvent, Flow, HostSide, Hooks};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn anvil_update(&self, _host: HostSide<'_>, _event: &mut AnvilUpdateEvent<'_>) -> Flow {
        Flow::Continue
    }
}
