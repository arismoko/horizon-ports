// The dying player whose grave was just made: their experience drop is about to be decided.
fn ug_xp_arm(game: &Game<'_>, entity: u32, kind: &str) {
    crate::game::universalgraves::xp_arm(game, entity, kind);
}
