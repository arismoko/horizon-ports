// BonemealCallback: claim the blocks Universal Bone Meal grows (isValidBonemealTarget).
fn ubm_valid(value: Option<bool>, level: &GameLevel<'_>, pos: BlockPos, state: BlockStateId) -> Option<bool> {
    if value.is_none() && crate::game::universalbonemeal::is_valid(level, pos, state) {
        return Some(true);
    }
    value
}
