//! mousewheelie: Mouse Wheelie 1.16.3 (Siphalor, Apache-2.0) for Horizon, with Homestead's config.
//! In the inventory and the chest-like screens (chests, barrels, ender chests, shulker boxes, hoppers,
//! dispensers), furnaces and crafting tables:
//! - The wheel over a slot moves its items towards where the wheel turns (directional scrolling): up
//! from the inventory into the container, down from the container into the inventory (in the
//! inventory screen, hotbar <-> main inventory). One item; Shift the whole stack; Ctrl every stack
//! of the item; Space moves the matching kinds (deposit) or tops up partial stacks (restock). The
//! wheel up on armour in the inventory screen puts it on.
//! - Ctrl+click moves every stack of the item, Ctrl+Shift+click everything in that part; Alt+click
//! drops the stack (Alt+Ctrl every stack of the item, Alt+Ctrl+Shift the whole part); Space+click
//! deposits. Dragging with Alt, Shift or Ctrl held does the same on each slot crossed.
//! - The middle button sorts the part under the cursor: creative order, Shift by quantity, Ctrl by
//! name. The hotbar is its own part.
//! - Pick block while holding a tool or weapon picks the best tool for the block (or a weapon for an
//! entity) from the inventory.
//!
//! Everything is ordinary slot clicks (the game's own `tweak_click`), so it runs on any server.
use mod_api::{self, Hooks};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {}
