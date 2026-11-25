use mod_api::{ClientHost, MenuScreen, MenuScreenLayout, MenuScreenOpen, ScreenSlot};

/// Where each of the menu's ten slots sits on the panel: the result first, then the grid row by
/// row, at the places vanilla's crafting screen has them.
pub fn slot_positions() -> [[f32; 2]; 10] {
    std::array::from_fn(|i| {
        if i == 0 {
            [124.0, 35.0]
        } else {
            let cell = (i - 1) as f32;
            [
                30.0 + (cell % 3.0).floor() * 18.0,
                17.0 + (cell / 3.0).floor() * 18.0,
            ]
        }
    })
}

/// Builds the screen of `visualworkbench:crafting` when the server opens it: a crafting table's
/// layout on the game's default panel, with an arrow from the grid to the result.
pub fn build_crafting_screen(
    _host: &dyn ClientHost,
    _open: MenuScreenOpen<'_>,
) -> Option<MenuScreen> {
    #[allow(clippy::needless_update)]
    let slots = slot_positions()
        .into_iter()
        .map(|position| {
            Some(ScreenSlot {
                position,
                ..Default::default()
            })
        })
        .collect();
    let layout = MenuScreenLayout {
        width: 176.0,
        height: 166.0,
        slots,
        inventory_x: 8.0,
        inventory_y: 84.0,
        ..Default::default()
    };
    Some(MenuScreen::new(layout).with_draw(|_host, frame, ctx| {
        let [ox, oy] = frame.origin;
        let arrow = 0xFF8B_8B8B;
        ctx.fill([ox + 90.0, oy + 39.0, 18.0, 8.0], arrow);
        for step in 0..8 {
            let step = step as f32;
            ctx.fill(
                [ox + 106.0 + step, oy + 35.0 + step, 1.0, 16.0 - 2.0 * step],
                arrow,
            );
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_follow_the_vanilla_layout() {
        let positions = slot_positions();
        assert_eq!(positions[0], [124.0, 35.0]);
        assert_eq!(positions[1], [30.0, 17.0]);
        assert_eq!(positions[3], [66.0, 17.0]);
        assert_eq!(positions[4], [30.0, 35.0]);
        assert_eq!(positions[9], [66.0, 53.0]);
    }
}
