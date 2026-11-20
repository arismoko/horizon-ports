// Too Cheap 1.4.0 for Horizon: no "Too Expensive!" cap. Injected as `crate::game::toocheap`.
//
// `stations::anvil_result` (AnvilMenu.createResult, horizon_client_lib: not patchable) drops the
// result when the cost reaches 40 and the player isn't creative. Its `creative` flag also lets
// any enchantment onto any item, which is the only other thing it changes. So when the cap
// dropped the result (no output, cost 40 or more), the result is made again with `creative`, and
// kept only when every enchantment of the right input could go on the left one anyway (or the
// left is a book) and the cost came out the same: then it is exactly the uncapped result.
use super::*;

pub(crate) fn uncap(game: &mut Game<'_>, value: &mut crate::menu::AnvilUpdate) {
    if value.vanilla.output.is_some() || value.vanilla.xp_cost < 40 {
        return;
    }
    let enchantments = std::sync::Arc::clone(game.item_enchantments());
    let items = game.items();
    let left = &value.left;
    if &*left.item != "minecraft:enchanted_book"
        && let Some(right) = value.right.as_ref()
        && right
            .crafting_enchantments()
            .iter()
            .any(|(id, _)| !enchantments.can_enchant(items, id, &left.item))
    {
        return;
    }
    let hover = horizon_client::stations::hover_name(&game.pack.names, left);
    let r = horizon_client::stations::anvil_result(
        &enchantments,
        items,
        Some(left),
        value.right.as_ref(),
        value.name.as_deref(),
        &hover,
        true,
    );
    if r.result.is_none() || r.cost != value.vanilla.xp_cost {
        return;
    }
    let material = i32::try_from(r.repair_item_count_cost).unwrap_or(i32::MAX);
    value.vanilla.output = r.result.clone();
    value.vanilla.material_cost = material;
    if value.output.is_none() {
        value.output = r.result;
        value.xp_cost = r.cost;
        value.material_cost = material;
    }
}

/// The anvil's cost line in place of "Too Expensive!".
pub(crate) fn label(game: &Game<'_>, key: &str) -> Option<String> {
    if key != "container.repair.expensive" {
        return None;
    }
    let station = game.station.as_ref()?;
    Some(horizon_client::lang::get_or("container.repair.cost", "Enchantment Cost: %1$s").replace("%1$s", &station.anvil.cost.to_string()))
}
