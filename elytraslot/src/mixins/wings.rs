// Elytra Slot: the trinket glider's wings on a player avatar (its own wearer's).
fn elytraslot_wings(
    game: &Game<'_>,
    m: &horizon_client::mobs::Mob,
    chest: Option<&str>,
    camera: glam::DVec3,
    partial: f32,
    light: u32,
    out: &mut Vec<horizon_client::mesher::Vertex>,
) -> core::ops::ControlFlow<()> {
    if crate::game::elytraslot::wings(game, m, chest, camera, partial, light, out) {
        core::ops::ControlFlow::Break(())
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
