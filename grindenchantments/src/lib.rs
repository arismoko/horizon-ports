//! Grind Enchantments 3.1.4 (mschae23) for Horizon, with Homestead's config: disenchant (an
//! enchanted item and a book give an enchanted book with all its enchantments, curses included;
//! the item is kept, unenchanted) and move (the first enchantment of an enchanted book with two or
//! more moves onto a book or an enchanted book). Each costs levels; taking the result gives no
//! experience.
//!
//! The operations need the game's enchantment data, so they are game-side (runtime.rs, on the
//! grindstone hook dispatchers). This crate:
//! - lets a book into the grindstone's inputs (a `MenuPart`'s `menu_slot_rules`, both sides);
//! - `grindstone_place`: no result for a player who can't pay its levels (GrindstoneScreen's
//!   canTakeResult, decided when the result is made rather than when it is taken);
//! - `grindstone_take`: takes the levels (`Player.giveExperienceLevels(-cost)`).
use std::collections::HashMap;
use std::sync::Mutex;

use mod_api::{
    Flow, GameMode, GrindstonePlaceEvent, GrindstoneTakeEvent, Hooks, Host, ItemStack, MenuContainer, MenuPart,
    MenuSlotRulesEvent, Registrar, RegistrationContext, ServerHost,
};

/// The level cost of each player's grindstone operation, by the runtime's mixins: (made, taken).
static COSTS: Mutex<Option<HashMap<u128, (Option<i32>, Option<i32>)>>> = Mutex::new(None);

fn costs<R>(f: impl FnOnce(&mut HashMap<u128, (Option<i32>, Option<i32>)>) -> R) -> R {
    f(COSTS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new))
}

/// The runtime made a result for `player` costing `cost` levels (None: not an operation of this mod).
pub fn note_cost(player: u128, cost: Option<i32>) {
    costs(|c| c.entry(player).or_default().0 = cost);
}

/// The runtime applied a take for `player` costing `cost` levels.
pub fn note_taken(player: u128, cost: Option<i32>) {
    costs(|c| c.entry(player).or_default().1 = cost);
}

struct Books;

#[mod_api::horizon_part]
impl MenuPart for Books {
    /// GrindstoneMenu's input slots' canInsert: a book goes in either.
    fn menu_slot_rules(&self, host: &dyn Host, event: &mut MenuSlotRulesEvent<'_>) {
        if matches!(event.container, MenuContainer::Main) && event.slot < 2 && is_book(host, event.stack) {
            event.may_place = true;
        }
    }
}

fn is_book(host: &dyn Host, stack: &ItemStack) -> bool {
    stack.item().id(host) == "minecraft:book"
}

fn creative(host: &dyn ServerHost, uuid: u128) -> bool {
    host.player(uuid).and_then(|p| p.game_mode(host)) == Some(GameMode::Creative)
}

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        r.menu_part("minecraft:grindstone", Books);
    }

    fn grindstone_place(&self, host: &mut dyn ServerHost, event: &mut GrindstonePlaceEvent<'_>) -> Flow {
        let Some(cost) = costs(|c| c.get(&event.player).and_then(|e| e.0)) else { return Flow::Continue };
        if creative(&*host, event.player) {
            return Flow::Continue;
        }
        let level = host.player(event.player).and_then(|p| p.experience(&*host)).map_or(0, |x| x.level);
        if level >= cost {
            return Flow::Continue;
        }
        event.cancelled = true;
        Flow::Cancel
    }

    fn grindstone_take(&self, host: &mut dyn ServerHost, event: &mut GrindstoneTakeEvent<'_>) -> Flow {
        let Some(cost) = costs(|c| c.get_mut(&event.player).and_then(|e| e.1.take())) else { return Flow::Continue };
        if !creative(&*host, event.player)
            && let Some(player) = host.player(event.player)
        {
            player.give_levels(host, -cost);
        }
        Flow::Continue
    }
}
