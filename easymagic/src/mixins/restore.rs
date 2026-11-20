// A table's kept item and lapis are in its menu when it opens.
fn em_restore(game: &mut Game<'_>, conn: u32) {
    crate::game::lan_stations::easymagic::restore(game, conn);
}
