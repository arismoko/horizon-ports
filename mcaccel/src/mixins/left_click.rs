// LeftClickPumpkinEvent: shears turn a pumpkin of the cycle back a face, once the game allows the dig.
fn mcaccel_left_click(game: &mut Game<'_>, at: [i32; 3], side: [i32; 3]) {
    if let Err(e) = crate::game::mcaccel::left_click(game, at, side) {
        eprintln!("[mcaccel] left click: {e}");
    }
}
