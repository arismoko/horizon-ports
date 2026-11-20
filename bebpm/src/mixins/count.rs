// Sum the enchanting power of every block in the box around the table (not the table itself), each
// as the game's own `enchant_power` answers it (the provider tag, or a block part's
// `enchant_power_bonus`), truncated as the game's count is.
fn count(game: &Game<'_>, pos: [i32; 3]) -> ::core::ops::ControlFlow<i32> {
    const R: i32 = 10;
    let table = horizon_core::position::BlockPos::new(pos[0], pos[1], pos[2]);
    let mut power = 0.0f32;
    for x in -R..=R {
        for y in -R..=R {
            for z in -R..=R {
                if x == 0 && y == 0 && z == 0 {
                    continue;
                }
                let at = horizon_core::position::BlockPos::new(pos[0] + x, pos[1] + y, pos[2] + z);
                power += game.enchant_power(game.world.block(at.x, at.y, at.z), at, table);
            }
        }
    }
    ::core::ops::ControlFlow::Break((power as i32).max(0))
}
