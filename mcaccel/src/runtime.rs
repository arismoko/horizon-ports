// Minecraft Accelerated 1.0.0 (pumpkinsAccelerated, mari, CC0-1.0): the shears cycle of carved
// faces, the mod's pumpkins as golem heads, and the worn face's blur. The blocks are the
// content mod mods/mcaccel.
//
// Injected as `crate::game::mcaccel`. The shears' left click and the enderman's stare are the mod
// crate's hooks (src/lib.rs), whose face table (`mod_mcaccel::step`) this uses too.
use super::*;

/// The block after (`forward`) or before `name` in its cycle (the mod crate's table).
pub(crate) fn step(name: &str, forward: bool) -> Option<String> {
    mod_mcaccel::step(name, forward)
}

fn name_at(game: &Game<'_>, p: [i32; 3]) -> std::sync::Arc<str> {
    game.res.facts().get(game.world.block(p[0], p[1], p[2])).name.clone()
}

/// The clicked side, or the player's opposite on the top and bottom (both handlers' blockDir).
fn facing(game: &Game<'_>, side: [i32; 3]) -> &'static str {
    let (dx, dz) = if side[1] != 0 {
        match ((game.yaw / 90.0 + 0.5).floor() as i32).rem_euclid(4) {
            0 => (0, -1),
            1 => (1, 0),
            2 => (0, 1),
            _ => (-1, 0),
        }
    } else {
        (side[0], side[2])
    };
    match (dx, dz) {
        (0, -1) => "north",
        (0, 1) => "south",
        (-1, 0) => "west",
        _ => "east",
    }
}

fn state_of(game: &Game<'_>, name: &str, facing: &str) -> Option<horizon_world::registry::BlockStateId> {
    let reg = game.pack.assets.registry;
    let d = reg.default_state(name)?;
    let mut st = reg.state(d)?.clone();
    st.properties.insert("facing".into(), facing.into());
    reg.resolve(&st).ok()
}

/// Turns the pumpkin at `at` to `next`, as setBlockState does it: onPlace builds a golem.
fn turn(game: &mut Game<'_>, at: [i32; 3], next: &str, side: [i32; 3]) -> horizon_client::Result<bool> {
    let Some(state) = state_of(game, next, facing(game, side)) else {
        return Ok(false);
    };
    let center = at.map(|c| f64::from(c) + 0.5);
    game.audio.play("minecraft:block.pumpkin.carve", Some(center), 1.0, 1.0);
    game.set_golem_head(at, state)?;
    Ok(true)
}

/// CarvedPumpkinModifier.onUse with shears: the next face, one point of wear, the shear game
/// event and the used statistic. None when it isn't shears on a pumpkin of the cycle.
pub(crate) fn shears_use(
    game: &mut Game<'_>,
    at: [i32; 3],
    side: [i32; 3],
    held: Option<&ItemStack>,
) -> Option<horizon_client::Result<()>> {
    let shears = held.filter(|s| &*s.item == "minecraft:shears")?.clone();
    let next = step(&name_at(game, at), true)?;
    Some((|| {
        if !turn(game, at, &next, side)? {
            return Ok(());
        }
        game.damage_held(1);
        game.post_block_game_event("minecraft:shear", at, game.sculk_actor(), None);
        game.award_stat("used", "minecraft:shears", 1);
        game.trigger_block("minecraft:item_used_on_block", at, &shears);
        Ok(())
    })())
}

/// BlockInit.PUMPKIN_BLOCKS: every one of the mod's 28 blocks heads a golem.
pub(crate) fn is_head(game: &Game<'_>, at: [i32; 3]) -> bool {
    let name = name_at(game, at);
    name.starts_with("mcaccel:carved_pumpkin_")
        || name.starts_with("mcaccel:jack_o_lantern_")
        || name.starts_with("mcaccel:soul_jack_o_lantern_")
}
