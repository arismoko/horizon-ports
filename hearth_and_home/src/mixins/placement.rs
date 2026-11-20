fn hh_placement(level: &GameLevel<'_>, state: BlockStateId, context: &PlacementContext, fluid: FluidState, answer: &mut Option<Option<BlockStateId>>) {
    if answer.is_none() && level.native_authoritative {
        *answer = crate::game::hearth_and_home::placement(level, state, context, fluid);
    }
}
