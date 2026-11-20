// performBonemeal for a block Universal Bone Meal claimed (the server's turn only).
fn ubm_perform(level: &mut GameLevel<'_>, ticks: &mut Ticks, pos: BlockPos, state: BlockStateId) {
    if level.native_authoritative {
        crate::game::universalbonemeal::perform(level, ticks, pos, state);
    }
}
