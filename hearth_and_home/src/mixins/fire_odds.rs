fn hh_fire_odds(level: &GameLevel<'_>, state: BlockStateId, odds: &mut Option<(i32, i32)>) {
    if odds.is_none() {
        *odds = crate::game::hearth_and_home::fire_odds(level, state);
    }
}
