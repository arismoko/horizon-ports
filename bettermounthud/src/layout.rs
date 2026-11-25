//! Pure HUD geometry for the riding HUD: where each status row goes and which sprites it uses.
//! All coordinates are GUI pixels, as the HUD's draw context takes them.

/// Hearts a mount shows: its max health halved and rounded, at most 30.
/// A non-positive or NaN max health shows none.
pub fn vehicle_max_hearts(max_health: f32) -> i32 {
    (((max_health + 0.5) as i32) / 2).clamp(0, 30)
}

/// Rows of ten that `hearts` fill.
pub fn rows_of_ten(hearts: i32) -> i32 {
    hearts / 10 + i32::from(hearts % 10 > 0)
}

/// Left edge of a row's first (rightmost) icon.
fn right_icon_x(width: i32, index: i32) -> i32 {
    width / 2 + 91 - index * 8 - 9
}

/// Top of the first row of the right status column: one row above the bottom of the HUD's
/// status area.
fn first_row_top(height: i32) -> i32 {
    height - 39
}

/// Which sprite a mount heart uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeartFill {
    Empty,
    Half,
    Full,
}

/// One mount heart, drawn as its container with `fill` over it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Heart {
    pub x: i32,
    pub y: i32,
    pub fill: HeartFill,
}

/// The mount's hearts. The first row sits where the hunger bar is when `above_food` is false
/// and one row higher when it is true; further rows go up.
pub fn vehicle_hearts(
    width: i32,
    height: i32,
    hearts: i32,
    health: f32,
    above_food: bool,
) -> Vec<Heart> {
    let current = f64::from(health).ceil() as i32;
    let mut y = first_row_top(height) - if above_food { 10 } else { 0 };
    let mut remaining = hearts.max(0);
    let mut base = 0;
    let mut out = Vec::new();
    while remaining > 0 {
        let row = remaining.min(10);
        remaining -= row;
        for i in 0..row {
            let at = i * 2 + 1 + base;
            let fill = if at < current {
                HeartFill::Full
            } else if at == current {
                HeartFill::Half
            } else {
                HeartFill::Empty
            };
            out.push(Heart {
                x: right_icon_x(width, i),
                y,
                fill,
            });
        }
        y -= 10;
        base += 20;
    }
    out
}

/// One hunger icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shank {
    pub x: i32,
    pub y: i32,
    pub fill: HeartFill,
}

/// The ten hunger icons. When saturation is empty the icons shake every `food * 3 + 1` ticks.
pub fn food_icons(width: i32, height: i32, food: i32, saturation: f32, tick: i32) -> Vec<Shank> {
    let top = first_row_top(height);
    let shaking =
        saturation <= 0.0 && tick.rem_euclid(food.saturating_mul(3).saturating_add(1)) == 0;
    (0..10)
        .map(|n| {
            let jitter = if shaking {
                (tick.wrapping_mul(7).wrapping_add(n * 5)).rem_euclid(3) - 1
            } else {
                0
            };
            let at = n * 2 + 1;
            let fill = if at < food {
                HeartFill::Full
            } else if at == food {
                HeartFill::Half
            } else {
                HeartFill::Empty
            };
            Shank {
                x: right_icon_x(width, n),
                y: top + jitter,
                fill,
            }
        })
        .collect()
}

/// What one air bubble slot shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bubble {
    Full,
    Bursting,
    Empty,
}

/// One drawn bubble.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AirIcon {
    pub x: i32,
    pub y: i32,
    pub kind: Bubble,
    /// The 1-based slot, which the pop sound tracks.
    pub slot: i32,
}

/// The air row for a rider whose mount shows `mount_hearts` hearts, above the mount's rows and
/// the hunger row. `None` when nothing is drawn (above water with full air).
/// The second value is the bursting slot's `(volume, pitch)` when a pop sounds.
pub fn air_icons(
    width: i32,
    height: i32,
    mount_hearts: i32,
    air: i32,
    max_air: i32,
    under_water: bool,
    tick: i32,
) -> Option<Vec<AirIcon>> {
    let max_air = max_air.max(1);
    let air = air.clamp(0, max_air);
    if !under_water && air >= max_air {
        return None;
    }
    let y = first_row_top(height) - 10 - rows_of_ten(mount_hearts) * 10;
    let bubble = |offset: i32| {
        ((f64::from(air) + f64::from(offset)) * 10.0 / f64::from(max_air)).ceil() as i32
    };
    let full = bubble(-2);
    let popping = bubble(0);
    let delay = i32::from(air != 0 && under_water);
    let empty = 10 - bubble(delay);
    let mut out = Vec::new();
    for slot in 1..=10 {
        let x = right_icon_x(width, slot - 1);
        if slot <= full {
            out.push(AirIcon {
                x,
                y,
                kind: Bubble::Full,
                slot,
            });
        } else if full != popping && slot == popping && under_water {
            out.push(AirIcon {
                x,
                y,
                kind: Bubble::Bursting,
                slot,
            });
        } else if slot > 10 - empty {
            let wobble = if empty == 10 && tick % 2 == 0 {
                (tick / 2).rem_euclid(2)
            } else {
                0
            };
            out.push(AirIcon {
                x,
                y: y + wobble,
                kind: Bubble::Empty,
                slot,
            });
        }
    }
    Some(out)
}

/// The pop sound's `(volume, pitch)` for the number of empty bubbles when one bursts.
pub fn pop_sound(air: i32, max_air: i32) -> (f32, f32) {
    let max_air = max_air.max(1);
    let air = air.clamp(0, max_air);
    let empty = 10
        - ((f64::from(air) + f64::from(i32::from(air != 0))) * 10.0 / f64::from(max_air)).ceil()
            as i32;
    (
        0.5 + 0.1 * (empty - 3 + 1).max(0) as f32,
        1.0 + 0.1 * (empty - 5 + 1).max(0) as f32,
    )
}

/// The experience bar: its left edge, top and filled width for `progress` (0 to 1).
pub fn xp_bar(width: i32, height: i32, progress: f32) -> (i32, i32, i32) {
    let filled = (progress.clamp(0.0, 1.0) * 183.0) as i32;
    (width / 2 - 91, height - 32 + 3, filled.min(182))
}

/// Top of the experience level number, and its left edge for a text `text_width` wide.
pub fn xp_level_origin(width: i32, height: i32, text_width: i32) -> (i32, i32) {
    ((width - text_width) / 2, height - 24 - 9 - 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_hearts_are_half_the_health_capped_at_thirty() {
        assert_eq!(vehicle_max_hearts(22.0), 11);
        assert_eq!(vehicle_max_hearts(30.0), 15);
        assert_eq!(vehicle_max_hearts(1000.0), 30);
        assert_eq!(vehicle_max_hearts(0.0), 0);
        assert_eq!(vehicle_max_hearts(-5.0), 0);
        assert_eq!(vehicle_max_hearts(f32::NAN), 0);
    }

    #[test]
    fn rows_round_up() {
        assert_eq!(rows_of_ten(0), 0);
        assert_eq!(rows_of_ten(10), 1);
        assert_eq!(rows_of_ten(11), 2);
        assert_eq!(rows_of_ten(30), 3);
    }

    #[test]
    fn mount_hearts_sit_a_row_above_hunger_when_hunger_shows() {
        let (w, h) = (320, 240);
        let low = vehicle_hearts(w, h, 11, 22.0, false);
        let high = vehicle_hearts(w, h, 11, 22.0, true);
        assert_eq!(low.len(), 11);
        assert_eq!(low[0].y, 201);
        assert_eq!(high[0].y, 191);
        assert_eq!(low[0].x, 242);
        assert_eq!(low[10].y, 191);
        assert_eq!(low[10].x, 242);
        assert!(low.iter().all(|heart| heart.fill == HeartFill::Full));
    }

    #[test]
    fn a_hurt_mount_shows_half_and_empty_hearts() {
        let hearts = vehicle_hearts(320, 240, 3, 3.0, true);
        assert_eq!(
            hearts.iter().map(|h| h.fill).collect::<Vec<_>>(),
            [HeartFill::Full, HeartFill::Half, HeartFill::Empty]
        );
        assert!(vehicle_hearts(320, 240, 0, 3.0, true).is_empty());
        assert!(vehicle_hearts(320, 240, -4, 3.0, true).is_empty());
    }

    #[test]
    fn hunger_is_drawn_in_the_first_right_row() {
        let icons = food_icons(320, 240, 19, 5.0, 3);
        assert_eq!(icons.len(), 10);
        assert_eq!(icons[0].y, 201);
        assert_eq!(icons[0].x, 242);
        assert_eq!(icons[9].fill, HeartFill::Half);
        assert_eq!(icons[0].fill, HeartFill::Full);
        assert!(
            food_icons(320, 240, 0, 5.0, 3)
                .iter()
                .all(|i| i.fill == HeartFill::Empty)
        );
    }

    #[test]
    fn hunger_shakes_only_without_saturation() {
        let steady = food_icons(320, 240, 20, 0.0, 1);
        assert!(steady.iter().all(|i| i.y == 201));
        let shaking = food_icons(320, 240, 20, 0.0, 61);
        assert!(shaking.iter().any(|i| i.y != 201));
        assert!(shaking.iter().all(|i| (200..=202).contains(&i.y)));
        let saturated = food_icons(320, 240, 20, 2.0, 61);
        assert!(saturated.iter().all(|i| i.y == 201));
        // A negative food level or a huge one never panics.
        let _ = food_icons(320, 240, i32::MIN, 0.0, 0);
        let _ = food_icons(320, 240, i32::MAX, 0.0, 0);
    }

    #[test]
    fn air_sits_above_the_mount_rows() {
        assert_eq!(air_icons(320, 240, 0, 300, 300, false, 0), None);
        let icons = air_icons(320, 240, 11, 150, 300, true, 0).unwrap();
        assert_eq!(icons[0].y, 240 - 49 - 20);
        let plain = air_icons(320, 240, 10, 150, 300, true, 0).unwrap();
        assert_eq!(plain[0].y, 240 - 49 - 10);
    }

    #[test]
    fn air_bubbles_burst_and_empty_as_the_air_runs_out() {
        let half = air_icons(320, 240, 10, 150, 300, true, 0).unwrap();
        assert_eq!(half.iter().filter(|i| i.kind == Bubble::Full).count(), 5);
        let gone = air_icons(320, 240, 10, 0, 300, true, 1).unwrap();
        assert!(gone.iter().all(|i| i.kind == Bubble::Empty));
        assert_eq!(gone.len(), 10);
        let burst = air_icons(320, 240, 10, 151, 300, true, 0).unwrap();
        assert_eq!(
            burst.iter().filter(|i| i.kind == Bubble::Bursting).count(),
            1
        );
        // Out of water with less than full air, no bubble bursts.
        let dry = air_icons(320, 240, 10, 151, 300, false, 0).unwrap();
        assert!(dry.iter().all(|i| i.kind != Bubble::Bursting));
        // Odd inputs never panic.
        let _ = air_icons(320, 240, 10, i32::MIN, 0, true, 0);
        let _ = air_icons(320, 240, 10, 5, -3, false, i32::MAX);
    }

    #[test]
    fn pop_sound_rises_as_bubbles_empty() {
        let (full_volume, full_pitch) = pop_sound(299, 300);
        let (low_volume, low_pitch) = pop_sound(10, 300);
        assert!(low_volume > full_volume);
        assert!(low_pitch > full_pitch);
        assert_eq!(pop_sound(300, 300), (0.5, 1.0));
    }

    #[test]
    fn experience_bar_fills_with_progress() {
        assert_eq!(xp_bar(320, 240, 0.0), (69, 211, 0));
        assert_eq!(xp_bar(320, 240, 0.5), (69, 211, 91));
        assert_eq!(xp_bar(320, 240, 1.0), (69, 211, 182));
        assert_eq!(xp_bar(320, 240, f32::NAN).2, 0);
        assert_eq!(xp_bar(320, 240, 7.0).2, 182);
    }

    #[test]
    fn experience_level_is_centred_above_the_bar() {
        assert_eq!(xp_level_origin(320, 240, 12), (154, 205));
    }
}
