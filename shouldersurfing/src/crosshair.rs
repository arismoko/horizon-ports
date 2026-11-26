//! Crosshair rules (`CrosshairRenderer`, the adaptive crosshair, the map overlay layout).

use crate::config::{CrosshairType, Lists, Visibility};
use crate::pattern::PatternList;

/// `CrosshairVisibility.isVisible`.
pub fn visible(rule: Visibility, aiming: bool, target_in_range: bool) -> bool {
    match rule {
        Visibility::Always => true,
        Visibility::Never => false,
        Visibility::WhenAiming => aiming,
        Visibility::WhenInRange => target_in_range,
        Visibility::WhenAimingOrInRange => aiming || target_in_range,
    }
}

/// Whether the crosshair follows the point the player acts on (dynamic) rather than staying at
/// the screen centre (static).
pub fn follows_target(kind: CrosshairType, adaptive_active: bool) -> bool {
    match kind {
        CrosshairType::Dynamic | CrosshairType::DynamicWith1pp => true,
        CrosshairType::Adaptive => adaptive_active,
        CrosshairType::Static | CrosshairType::StaticWith1pp => false,
    }
}

/// Which items make the adaptive crosshair dynamic, compiled from the lists.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Adaptive {
    hold_items: PatternList,
    use_items: PatternList,
    hold_animations: Vec<String>,
    use_animations: Vec<String>,
    hold_components: PatternList,
    use_components: PatternList,
    hold_default_components: Vec<String>,
    use_default_components: Vec<String>,
}

/// What is known about a stack when it is matched.
#[derive(Clone, Copy)]
pub struct StackFacts<'a> {
    pub id: &'a str,
    pub animation: &'a str,
    /// Component names the stack itself changes.
    pub components: &'a [String],
    /// Whether the item has the component by default (asked for the listed names).
    pub default_has: &'a dyn Fn(&str) -> bool,
}

#[cfg(test)]
fn no_default(_: &str) -> bool {
    false
}

#[cfg(test)]
impl StackFacts<'_> {
    pub fn new(id: &str) -> StackFacts<'_> {
        StackFacts {
            id,
            animation: "none",
            components: &[],
            default_has: &no_default,
        }
    }
}

impl Adaptive {
    pub fn new(l: &Lists) -> Self {
        Self {
            hold_items: PatternList::new(&l.hold_items),
            use_items: PatternList::new(&l.use_items),
            hold_animations: l.hold_item_animations.clone(),
            use_animations: l.use_item_animations.clone(),
            hold_components: PatternList::new(&l.hold_item_components),
            use_components: PatternList::new(&l.use_item_components),
            hold_default_components: l.hold_default_components.clone(),
            use_default_components: l.use_default_components.clone(),
        }
    }

    fn matches(
        items: &PatternList,
        animations: &[String],
        components: &PatternList,
        defaults: &[String],
        s: &StackFacts<'_>,
    ) -> bool {
        items.matches(s.id)
            || animations.iter().any(|a| a == s.animation)
            || s.components.iter().any(|c| components.matches(c))
            || defaults.iter().any(|c| (s.default_has)(c))
    }

    /// An item held in a hand.
    pub fn holding(&self, s: &StackFacts<'_>) -> bool {
        Self::matches(
            &self.hold_items,
            &self.hold_animations,
            &self.hold_components,
            &self.hold_default_components,
            s,
        )
    }

    /// An item being used.
    pub fn using(&self, s: &StackFacts<'_>) -> bool {
        Self::matches(
            &self.use_items,
            &self.use_animations,
            &self.use_components,
            &self.use_default_components,
            s,
        )
    }
}

/// `UseAnimation` as the names the lists use.
pub fn animation_name(a: mod_api::UseAnimation) -> &'static str {
    use mod_api::UseAnimation as A;
    match a {
        A::None => "none",
        A::Eat => "eat",
        A::Drink => "drink",
        A::Block => "block",
        A::Bow => "bow",
        A::Trident => "trident",
        A::Crossbow => "crossbow",
        A::Spyglass => "spyglass",
        A::TootHorn => "toot_horn",
        A::Brush => "brush",
        A::Bundle => "bundle",
        A::Spear => "spear",
        #[allow(unreachable_patterns)]
        _ => "none",
    }
}

/// The obstruction indicator rule: the world point the camera ray reaches is not where the
/// player's own ray ends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Obstruction {
    pub enabled: bool,
    pub only_when_aiming: bool,
    pub min_screen_distance: f64,
    pub max_world_distance: f64,
}

impl Obstruction {
    pub fn shows(&self, aiming: bool, screen_distance: f64, world_distance: f64) -> bool {
        self.enabled
            && (aiming || !self.only_when_aiming)
            && screen_distance >= self.min_screen_distance
            && (self.max_world_distance <= 0.0 || world_distance <= self.max_world_distance)
    }
}

/// What the crosshair attack indicator shows (`Hud.extractCrosshair`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AttackIndicator {
    #[default]
    Hidden,
    /// The full-charge icon, drawn when the target is a living entity in reach.
    Full,
    /// The charging bar, `0..=16` pixels of the progress sprite.
    Progress(i32),
}

/// `Hud.extractCrosshair`'s indicator rules. `mode` is the attack indicator option (1 is
/// crosshair); `target_living` is `Some(alive)` when the crosshair is on a living entity.
pub fn attack_indicator(
    mode: u8,
    scale: f32,
    full_charge_delay: f32,
    target_living: Option<bool>,
) -> AttackIndicator {
    if mode != 1 {
        return AttackIndicator::Hidden;
    }
    if scale >= 1.0 {
        return match target_living {
            Some(alive) if full_charge_delay > 5.0 && alive => AttackIndicator::Full,
            _ => AttackIndicator::Hidden,
        };
    }
    AttackIndicator::Progress(((scale.max(0.0) * 17.0) as i32).min(16))
}

/// A map drawn at `x, y`, `size` GUI pixels wide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapRect {
    pub x: i32,
    pub y: i32,
    pub size: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapLayout {
    pub scale_percent: i32,
    pub anchor_top: bool,
    pub inset_top: i32,
    pub inset_right: i32,
    pub inset_bottom: i32,
    pub inset_left: i32,
}

/// `MapOverlayRenderer.extractMapOverlay`: the rectangles of the map in the left hand and the one in
/// the right. `effect_rows` is how many rows of status icons sit at the top right.
pub fn map_rects(
    l: &MapLayout,
    gui: [i32; 2],
    left: bool,
    right: bool,
    effect_rows: i32,
) -> (Option<MapRect>, Option<MapRect>) {
    let [w, h] = gui;
    let mut size = (f64::from(h) * f64::from(l.scale_percent) / 100.0).ceil() as i32;
    size = size.max(0);
    let mut y = if l.anchor_top {
        l.inset_top.min(h - size)
    } else {
        h - (l.inset_bottom + size).min(h)
    };
    y = y.max(0);
    let left_rect = left.then(|| MapRect {
        x: l.inset_left.min(w - size).max(0),
        y,
        size,
    });
    let right_rect = right.then(|| {
        if l.anchor_top {
            y += 26 * effect_rows;
        }
        let mut size = size;
        if y + size > h {
            size = h - y;
            size -= if l.anchor_top {
                l.inset_top
            } else {
                l.inset_bottom
            };
        }
        let size = size.max(0);
        MapRect {
            x: w - (l.inset_right + size).min(w),
            y,
            size,
        }
    });
    (left_rect, right_rect)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_rules() {
        assert!(visible(Visibility::Always, false, false));
        assert!(!visible(Visibility::Never, true, true));
        assert!(visible(Visibility::WhenAiming, true, false));
        assert!(!visible(Visibility::WhenAiming, false, true));
        assert!(visible(Visibility::WhenInRange, false, true));
        assert!(visible(Visibility::WhenAimingOrInRange, true, false));
        assert!(!visible(Visibility::WhenAimingOrInRange, false, false));
    }

    #[test]
    fn crosshair_following() {
        assert!(follows_target(CrosshairType::Dynamic, false));
        assert!(!follows_target(CrosshairType::Static, true));
        assert!(follows_target(CrosshairType::Adaptive, true));
        assert!(!follows_target(CrosshairType::Adaptive, false));
    }

    #[test]
    fn adaptive_matches_defaults_of_the_original() {
        let a = Adaptive::new(&Lists::default());
        assert!(a.holding(&StackFacts::new("minecraft:snowball")));
        assert!(!a.holding(&StackFacts::new("minecraft:stick")));
        let bow = StackFacts {
            animation: "bow",
            ..StackFacts::new("minecraft:bow")
        };
        assert!(a.using(&bow));
        assert!(!a.using(&StackFacts::new("minecraft:apple")));
        let charged = vec!["minecraft:charged_projectiles".to_owned()];
        let crossbow = StackFacts {
            components: &charged,
            ..StackFacts::new("minecraft:crossbow")
        };
        assert!(a.holding(&crossbow));
        let has = |c: &str| c == "minecraft:piercing_weapon";
        let spear = StackFacts {
            default_has: &has,
            ..StackFacts::new("minecraft:spear")
        };
        assert!(a.holding(&spear));
    }

    #[test]
    fn attack_indicator_follows_vanilla() {
        use AttackIndicator::*;
        assert_eq!(attack_indicator(0, 0.5, 20.0, None), Hidden);
        assert_eq!(attack_indicator(2, 0.5, 20.0, None), Hidden);
        assert_eq!(attack_indicator(1, 0.5, 20.0, None), Progress(8));
        assert_eq!(attack_indicator(1, 0.0, 20.0, Some(true)), Progress(0));
        assert_eq!(attack_indicator(1, 1.0, 20.0, Some(true)), Full);
        assert_eq!(attack_indicator(1, 1.0, 20.0, Some(false)), Hidden);
        assert_eq!(attack_indicator(1, 1.0, 5.0, Some(true)), Hidden);
        assert_eq!(attack_indicator(1, 1.0, 20.0, None), Hidden);
        assert_eq!(attack_indicator(1, 0.999, 20.0, Some(true)), Progress(16));
    }

    #[test]
    fn animation_names_match_the_lists() {
        assert_eq!(animation_name(mod_api::UseAnimation::Bow), "bow");
        assert_eq!(animation_name(mod_api::UseAnimation::TootHorn), "toot_horn");
        assert_eq!(animation_name(mod_api::UseAnimation::None), "none");
    }

    #[test]
    fn obstruction_rules() {
        let o = Obstruction {
            enabled: true,
            only_when_aiming: true,
            min_screen_distance: 8.0,
            max_world_distance: 20.0,
        };
        assert!(o.shows(true, 10.0, 5.0));
        assert!(!o.shows(false, 10.0, 5.0));
        assert!(!o.shows(true, 3.0, 5.0));
        assert!(!o.shows(true, 10.0, 30.0));
        assert!(
            Obstruction {
                max_world_distance: 0.0,
                ..o
            }
            .shows(true, 10.0, 300.0)
        );
        assert!(
            !Obstruction {
                enabled: false,
                ..o
            }
            .shows(true, 10.0, 5.0)
        );
        assert!(
            Obstruction {
                only_when_aiming: false,
                ..o
            }
            .shows(false, 10.0, 5.0)
        );
    }

    const LAYOUT: MapLayout = MapLayout {
        scale_percent: 40,
        anchor_top: false,
        inset_top: 1,
        inset_right: 1,
        inset_bottom: 1,
        inset_left: 1,
    };

    #[test]
    fn maps_sit_at_the_bottom_corners() {
        let (l, r) = map_rects(&LAYOUT, [400, 200], true, true, 0);
        assert_eq!(
            l,
            Some(MapRect {
                x: 1,
                y: 119,
                size: 80
            })
        );
        assert_eq!(
            r,
            Some(MapRect {
                x: 319,
                y: 119,
                size: 80
            })
        );
        let (l, r) = map_rects(&LAYOUT, [400, 200], false, true, 0);
        assert_eq!(l, None);
        assert!(r.is_some());
    }

    #[test]
    fn top_anchor_leaves_room_for_effects() {
        let top = MapLayout {
            anchor_top: true,
            ..LAYOUT
        };
        let (l, r) = map_rects(&top, [400, 200], true, true, 2);
        assert_eq!(l.map(|m| m.y), Some(1));
        let r = r.unwrap();
        assert_eq!(r.y, 53);
        // 53 + 80 fits in 200, so the size stays.
        assert_eq!(r.size, 80);
        let (_, r) = map_rects(&top, [400, 200], false, true, 5);
        let r = r.unwrap();
        assert_eq!(r.y, 131);
        assert_eq!(r.size, 200 - 131 - 1);
    }

    #[test]
    fn tiny_screens_never_go_negative() {
        let (l, r) = map_rects(&LAYOUT, [0, 0], true, true, 0);
        assert_eq!(
            l,
            Some(MapRect {
                x: 0,
                y: 0,
                size: 0
            })
        );
        assert_eq!(
            r,
            Some(MapRect {
                x: 0,
                y: 0,
                size: 0
            })
        );
        let big = MapLayout {
            scale_percent: 100,
            inset_left: 500,
            ..LAYOUT
        };
        let (l, _) = map_rects(&big, [100, 100], true, false, 0);
        assert!(l.unwrap().x >= 0);
    }
}
