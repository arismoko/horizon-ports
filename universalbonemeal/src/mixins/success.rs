// isBonemealSuccess for a block Universal Bone Meal claimed (the server's turn only).
fn ubm_success(value: bool, level: &mut GameLevel<'_>, pos: BlockPos, state: BlockStateId) -> bool {
    if !level.native_authoritative {
        return value;
    }
    crate::game::universalbonemeal::is_success(level, pos, state).unwrap_or(value)
}
