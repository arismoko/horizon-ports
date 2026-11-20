// MWClient.triggerPick: pick block while holding a tool or weapon picks the best one instead.
fn mw_pick(game: &mut Game<'_>, key: InteractionKey) -> ::core::ops::ControlFlow<InteractionDecision> {
    let button = if key == InteractionKey::PickBlock { 2 } else { -1 };
    match crate::game::mousewheelie::pick(game, button) {
        ::core::ops::ControlFlow::Break(swing) => ::core::ops::ControlFlow::Break(InteractionDecision {
            canceled: true,
            swing: swing.unwrap_or(false),
        }),
        ::core::ops::ControlFlow::Continue(()) => ::core::ops::ControlFlow::Continue(()),
    }
}
