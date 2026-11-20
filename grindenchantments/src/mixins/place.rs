// GrindstoneMenu.createResult with the operations, before the mods' grindstone_place hooks.
fn ge_place(game: &mut super::Game<'_>, player: u128, value: &mut crate::menu::GrindstonePlace) {
    crate::game::stations::grindenchantments::place(game, player, value);
}
