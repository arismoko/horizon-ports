// Grind Enchantments for Horizon, the server's side of a take: a guest's click on the grindstone
// re-checks the operation's level cost against the guest's levels now, before the menu engine runs
// the click. Injected as `crate::game::lan_stations::grindenchantments`.
//
// The result is made (and hidden from a player who can't pay) when the inputs change, but the
// levels can drop after that, and the `grindstone_take` hook that charges them runs after the
// engine has already moved the result. So the click first takes away a result its player can't
// pay for (`grindstone_placed` with a cancel, the engine's "no result"); a click it lets through
// moves the result and charges the levels in the same `lan_station_op`, at the levels checked here.
use super::*;

/// `lan_station_click`'s head for `conn`'s window `window`.
pub(crate) fn revalidate(game: &mut Game<'_>, conn: u32, window: i32) {
    let Some(host) = game.lan.as_ref() else {
        return;
    };
    let Some(g) = host.hub.guests.iter().find(|g| g.conn == conn) else {
        return;
    };
    let (creative, level) = (g.creative(), g.xp_level);
    let e = std::sync::Arc::clone(game.item_enchantments());
    let Some(gs) = game
        .lan
        .as_mut()
        .and_then(|host| host.menus.get_mut(&conn))
        .and_then(|p| p.station.as_mut())
        .filter(|s| s.window == window && s.kind == StationKind::Grindstone)
    else {
        return;
    };
    if gs.menu.slot(2).is_none() {
        return;
    }
    let cost = crate::game::stations::grindenchantments::cost(&e, gs.menu.slot(0), gs.menu.slot(1));
    if let Some(cost) = cost
        && !creative
        && level < cost
    {
        let place = gs.menu.grindstone_place();
        gs.menu.grindstone_placed(place, true);
    }
}
