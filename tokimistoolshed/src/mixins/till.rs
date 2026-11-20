// A scythe tills the 3x3.
fn tokimistoolshed_till(game: &mut Game<'_>, stack: &ItemStack, hit: &Hit) -> ::core::ops::ControlFlow<Result<bool>> {
    crate::game::tokimistoolshed::till(game, stack, hit)
}
