// The result taken: the inputs as the operation leaves them, before the grindstone_take hooks.
fn ge_take(game: &mut super::Game<'_>, player: u128, value: &mut crate::menu::GrindstoneTake) {
    crate::game::stations::grindenchantments::take(game, player, value);
}
