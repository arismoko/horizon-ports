//! yungsbridges: YUNG's Bridges 4.0.3 (YungNickYoung, LGPLv3). The bridges are game-side
//! (runtime.rs and the mixins). This listener changes nothing: the game reaches the level tick
//! hook point, where the tick mixin runs, only when a mod listens to `level_tick_post`.
use mod_api::{Hooks, HostSide, LevelTickPostEvent};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn level_tick_post(&self, _host: HostSide<'_>, _event: &mut LevelTickPostEvent) {}
}
