// Things' Bater Wucket (BucketItemMixin.preserverBaterWucket and its cauldron behaviour).
// Injected as `crate::game::things`; the rest of Things is src/lib.rs.
use super::*;
use core::ops::ControlFlow;
use std::cell::Cell;

thread_local! {
    static WUCKET: Cell<bool> = const { Cell::new(false) };
}

/// The Bater Wucket in the main hand: the water bucket's use (placing water, filling a cauldron),
/// then the wucket back, still full.
pub(crate) fn use_wucket(game: &mut Game<'_>, stack: &ItemStack, allow: bool) -> ControlFlow<Result<horizon_client::first_person::UseResult>> {
    if WUCKET.get() || &*stack.item != "things:bater_wucket" {
        return ControlFlow::Continue(());
    }
    let Some(original) = game.held().cloned().filter(|h| h.item == stack.item) else {
        return ControlFlow::Continue(());
    };
    let mut twin = original.clone();
    twin.item = std::sync::Arc::from("minecraft:water_bucket");
    *game.held_mut() = Some(twin.clone());
    WUCKET.set(true);
    let result = game.use_tool(&twin, allow);
    WUCKET.set(false);
    *game.held_mut() = Some(original);
    ControlFlow::Break(result)
}
