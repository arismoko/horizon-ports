// The station as always, then the grindstone's cost line.
fn ge_label(game: &mut Game<'_>, batch: &mut HudBatch) -> ::core::ops::ControlFlow<bool> {
    match crate::game::stations::grindenchantments::draw(game, batch) {
        Some(shown) => ::core::ops::ControlFlow::Break(shown),
        None => ::core::ops::ControlFlow::Continue(()),
    }
}
