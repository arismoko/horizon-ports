// The death drops go into a grave when one can be made.
fn ug_bury(game: &mut Game<'_>, dying: &crate::game::creatures::DyingDrops<'_>) -> core::ops::ControlFlow<()> {
    if crate::game::universalgraves::bury(game, dying.entity) {
        core::ops::ControlFlow::Break(())
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
