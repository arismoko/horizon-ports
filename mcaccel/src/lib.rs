//! Minecraft Accelerated 1.0.0 (pumpkinsAccelerated, mari, CC0-1.0) for Horizon: shears carve
//! nine faces into carved pumpkins, jack o'lanterns and soul jack o'lanterns (the blocks are this
//! package's content), the mod's pumpkins head golems, a worn carved face blurs the view as a
//! carved pumpkin does and hides the wearer's stare from endermen.
//!
//! Game-side (runtime.rs, mixins): shears' use (the next face), shears' left click
//! (LeftClickPumpkinEvent: back a face, gated by the game's own dig permission), golem heads, the
//! blur. Here:
//! - `enderman_anger` (EndermanEntityMixin): a player wearing one of the mod's carved faces isn't
//!   staring.
use mod_api::{EndermanAngerEvent, Flow, Hooks, Host};

pub const FACES: [&str; 9] = ["x", "smile", "derp", "check", "creeper", "herobrine", "evil", "heart", "shout"];

/// The three cycles, vanilla's block first: CarvedPumpkinModifier.PUMPKIN_CHANGE_MAP goes forward
/// through them and LeftClickPumpkinEvent's reversed map backward.
fn cycles() -> [Vec<String>; 3] {
    let cycle = |first: &str, prefix: &str| {
        let mut c = vec![first.to_owned()];
        c.extend(FACES.iter().map(|f| format!("mcaccel:{prefix}_{f}")));
        c
    };
    [
        cycle("minecraft:carved_pumpkin", "carved_pumpkin"),
        cycle("minecraft:jack_o_lantern", "jack_o_lantern"),
        cycle("mcaccel:soul_jack_o_lantern_normal", "soul_jack_o_lantern"),
    ]
}

/// The block after (`forward`) or before `name` in its cycle.
pub fn step(name: &str, forward: bool) -> Option<String> {
    for (i, cycle) in cycles().iter().enumerate() {
        let Some(at) = cycle.iter().position(|n| n == name) else {
            continue;
        };
        if forward {
            // The soul cycle's forward map leaves its plain lantern out (shout goes back to x):
            // shears on a plain soul jack o'lantern do nothing (on Java they throw).
            if i == 2 {
                return (at != 0).then(|| cycle[at % 9 + 1].clone());
            }
            return Some(cycle[(at + 1) % cycle.len()].clone());
        }
        return Some(cycle[(at + cycle.len() - 1) % cycle.len()].clone());
    }
    None
}

/// The equipment slot numbers of `PlayerRef::inventory_slots`: the head.
const HEAD: u32 = 39;

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn enderman_anger(&self, host: &dyn Host, event: &mut EndermanAngerEvent) -> Flow {
        let worn = event
            .player
            .inventory_slots(host)
            .unwrap_or_default()
            .into_iter()
            .find(|(n, _)| *n == HEAD)
            .map(|(_, s)| s.item().id(host));
        if worn.is_some_and(|h| h.starts_with("mcaccel:carved_pumpkin_")) {
            event.cancelled = true;
            return Flow::Cancel;
        }
        Flow::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::step;

    #[test]
    fn faces_cycle_both_ways() {
        assert_eq!(step("minecraft:carved_pumpkin", true).as_deref(), Some("mcaccel:carved_pumpkin_x"));
        assert_eq!(step("mcaccel:carved_pumpkin_shout", true).as_deref(), Some("minecraft:carved_pumpkin"));
        assert_eq!(step("minecraft:jack_o_lantern", false).as_deref(), Some("mcaccel:jack_o_lantern_shout"));
        assert_eq!(step("mcaccel:soul_jack_o_lantern_shout", true).as_deref(), Some("mcaccel:soul_jack_o_lantern_x"));
        assert_eq!(step("mcaccel:soul_jack_o_lantern_normal", true), None);
        assert_eq!(step("mcaccel:soul_jack_o_lantern_x", false).as_deref(), Some("mcaccel:soul_jack_o_lantern_normal"));
        assert_eq!(step("minecraft:pumpkin", true), None);
    }
}
