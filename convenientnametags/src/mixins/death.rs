// removeNameAndNameTag on death: a named mob's name drops as a tag.
fn named_death(game: &mut Game<'_>, m: &Mob) {
    crate::game::creatures::convenientnametags::drop_tag(game, m);
}
