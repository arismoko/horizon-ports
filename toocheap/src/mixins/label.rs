// "Too Expensive!" becomes "Enchantment Cost: N" with the anvil's cost.
fn toocheap_label(game: &Game<'_>, key: &str) -> ::core::ops::ControlFlow<String> {
    match crate::game::toocheap::label(game, key) {
        Some(line) => ::core::ops::ControlFlow::Break(line),
        None => ::core::ops::ControlFlow::Continue(()),
    }
}
