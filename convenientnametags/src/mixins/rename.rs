// NameTagItemMixin.useOnEntity: the same name (text and style) passes (the tag stays); a
// different one drops the old name, style and all, as a tag before the mob is renamed.
fn rename(game: &mut Game<'_>, index: usize) -> core::ops::ControlFlow<bool> {
    let Some(held) = game
        .held()
        .filter(|h| &*h.item == "minecraft:name_tag" && h.custom_name().is_some())
        .cloned()
    else {
        return core::ops::ControlFlow::Continue(());
    };
    let m = &game.mobs[index];
    if m.custom_name.is_none() {
        return core::ops::ControlFlow::Continue(());
    }
    if crate::game::creatures::convenientnametags::same_name(m, &held) {
        return core::ops::ControlFlow::Break(false);
    }
    if m.alive() {
        let m = m.clone();
        crate::game::creatures::convenientnametags::drop_tag(game, &m);
    }
    core::ops::ControlFlow::Continue(())
}
