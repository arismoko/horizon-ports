// The Java mod checks the trim material's asset name, "gold": vanilla's `minecraft:gold`, or an inline
// material whose palette is "gold".
fn gold_trim(game: &Game<'_>) -> ::core::ops::ControlFlow<bool> {
    let trimmed = game.armor.iter().flatten().any(|s| {
        s.data().and_then(|d| d.trim.as_ref()).is_some_and(|(material, _)| match material {
            crate::item_data::trim::Holder::Named(name) => &**name == "minecraft:gold",
            crate::item_data::trim::Holder::Direct(m) => &*m.palette_id == "gold",
        })
    });
    if trimmed {
        ::core::ops::ControlFlow::Break(true)
    } else {
        ::core::ops::ControlFlow::Continue(())
    }
}
