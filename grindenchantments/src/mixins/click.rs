// A click on a guest's grindstone: the result's cost checked against the levels now, before the
// click can move it.
fn ge_click(game: &mut Game<'_>, conn: u32, p: &play::ContainerClick) {
    crate::game::lan_stations::grindenchantments::revalidate(game, conn, p.container);
}
