// Before the anvil hooks are asked: a result the 40-level cap dropped comes back.
fn toocheap_uncap(game: &mut super::Game<'_>, value: &mut crate::menu::AnvilUpdate) {
    crate::game::toocheap::uncap(game, value);
}
