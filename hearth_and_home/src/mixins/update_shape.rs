fn hh_update_shape(level: &GameLevel<'_>, pos: BlockPos, state: BlockStateId, direction: Direction, neighbour_pos: BlockPos, neighbour: BlockStateId, new: &mut Option<BlockStateId>) {
    if new.is_none() && level.native_authoritative {
        *new = crate::game::hearth_and_home::update_shape(level, pos, state, direction, neighbour_pos, neighbour);
    }
}
