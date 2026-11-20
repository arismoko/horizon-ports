// ShearsItemMixin.useOnEntity: sneaking with shears on a named mob takes the name off as a tag
// and damages the shears by 1; the mob's own interaction (shearing a sheep...) doesn't run.
// Hooked where Player.interactOn asks the mods (`native_entity_interact`), before the entity's
// own interaction: the answer `Success` ends the click there.
fn shear_name(
    game: &mut Game<'_>,
    entity: i32,
    result: &mut Option<mod_api::InteractionResult>,
) -> ::core::ops::ControlFlow<()> {
    let sneaking = game.player.as_ref().is_some_and(|p| p.shift_key_down);
    if !sneaking || !game.held().is_some_and(|h| &*h.item == "minecraft:shears") {
        return ::core::ops::ControlFlow::Continue(());
    }
    let Some(index) = game.mobs.iter().position(|m| m.id as i32 == entity) else {
        return ::core::ops::ControlFlow::Continue(());
    };
    let m = &game.mobs[index];
    if !m.alive() {
        return ::core::ops::ControlFlow::Continue(());
    }
    let Some(name) = m.custom_name.clone() else {
        return ::core::ops::ControlFlow::Continue(());
    };
    let m = m.clone();
    crate::game::creatures::convenientnametags::drop_tag(game, &m, &name);
    game.mobs[index].custom_name = None;
    game.mobs[index].custom_name_text = None;
    // The save keeps the loaded tag's `CustomName` unless the mob writes a new one
    // (entity_nbt::mob_to_nbt), so it goes from there too or it comes back on the next load.
    if let Some(horizon_core::nbt::Tag::Compound(raw)) = game.persist.raw.get_mut(&m.id) {
        raw.remove(&horizon_core::nbt::JavaString::from("CustomName"));
    }
    game.damage_held_tool();
    *result = Some(mod_api::InteractionResult::Success);
    ::core::ops::ControlFlow::Break(())
}
