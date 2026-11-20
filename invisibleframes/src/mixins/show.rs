// The Java mod's ItemFrame.hurt HEAD, once the hit is accepted: a hit makes the frame visible.
// Hooked on `hit_frame`, which both a player's hit (`attack_frame`) and a projectile's
// (`projectile_hurts_frame`) reach only after the engine's checks (the block in front, the
// interaction permission `may_interact_at`, a fixed frame, `isInvulnerableToBase`). An empty frame
// breaks on the hit, or stays as it was when a mod refuses the break, so only a frame giving up
// its item is shown.
fn show(game: &mut Game<'_>, i: usize) {
    if let Some(f) = game.item_frames.get_mut(i)
        && f.item.is_some()
    {
        f.invisible = false;
    }
}
