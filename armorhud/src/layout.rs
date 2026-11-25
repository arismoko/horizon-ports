//! The widget's pure geometry and rules: no host calls, so they are unit tested.

use crate::config::{
    Anchor, ArmorHudConfig, DurabilityDisplay, OffhandBehavior, Orientation, Side, Style,
    WidgetShown,
};

/// Distance between two slots.
pub const STEP: i32 = 20;
/// Width and height of one slot's background.
pub const SIZE: i32 = 22;
/// Distance from the screen centre to the hotbar's edge plus the gap vanilla leaves.
pub const HOTBAR_OFFSET: i32 = 98;
pub const OFFHAND_OFFSET: i32 = 29;
pub const ATTACK_INDICATOR_OFFSET: i32 = 23;
pub const WARNING_SIZE: i32 = 8;
/// `Font.lineHeight`.
pub const LINE_HEIGHT: i32 = 9;
/// Height of the hotbar's sprite, and of the offhand's.
const HOTBAR_SPRITE: (i32, i32) = (182, 22);
const OFFHAND_SPRITE: (i32, i32) = (29, 24);

pub const HOTBAR_SPRITE_ID: &str = "hud/hotbar";
pub const OFFHAND_SPRITE_ID: &str = "hud/hotbar_offhand_left";
pub const EMPTY_SLOT_SPRITES: [&str; 4] = [
    "container/slot/helmet",
    "container/slot/chestplate",
    "container/slot/leggings",
    "container/slot/boots",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// What the widget needs to know about the player and the screen.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub gui_width: i32,
    pub gui_height: i32,
    /// Whether the player's main arm is the widget's preferred side.
    pub main_arm_on_side: bool,
    pub offhand_filled: bool,
    /// Whether the hotbar attack indicator is showing a charge.
    pub attack_indicator_showing: bool,
}

/// Whether a stack's durability is at or under the warning thresholds.
pub fn should_show_warning(
    damageable: bool,
    damage: u32,
    max_damage: Option<u32>,
    cfg: &ArmorHudConfig,
) -> bool {
    let Some(max) = max_damage.filter(|m| *m > 0) else {
        return false;
    };
    if !damageable {
        return false;
    }
    let fraction = 1.0 - f64::from(damage) / f64::from(max);
    fraction <= cfg.min_durability_percentage
        || i64::from(max) - i64::from(damage) <= i64::from(cfg.min_durability_value)
}

/// The text under or beside a piece when the durability display is not the bar.
pub fn durability_text(display: DurabilityDisplay, damage: u32, max_damage: u32) -> String {
    match display {
        DurabilityDisplay::Bar => String::new(),
        DurabilityDisplay::Numeric => max_damage.saturating_sub(damage).to_string(),
        DurabilityDisplay::Percentage => {
            if damage == 0 || max_damage == 0 {
                String::new()
            } else {
                let left = 1.0 - f64::from(damage) / f64::from(max_damage);
                format!("{}%", (left * 100.0).floor().clamp(0.0, 100.0) as i32)
            }
        }
    }
}

/// An item's bar colour from its damage, as the vanilla durability bar computes it: hue from
/// green to red.
pub fn bar_color(damage: u32, max_damage: u32) -> u32 {
    let left = if max_damage == 0 {
        0.0
    } else {
        ((f64::from(max_damage) - f64::from(damage)) / f64::from(max_damage)).max(0.0)
    };
    hsv_to_rgb(left / 3.0)
}

/// `hsv` of full saturation and value, `hue` in `0..=1`, as `0xRRGGBB`.
fn hsv_to_rgb(hue: f64) -> u32 {
    let h = (hue.rem_euclid(1.0)) * 6.0;
    let f = h - h.floor();
    let q = 1.0 - f;
    let (r, g, b) = match h.floor() as i32 {
        0 => (1.0, f, 0.0),
        1 => (q, 1.0, 0.0),
        2 => (0.0, 1.0, f),
        3 => (0.0, q, 1.0),
        4 => (f, 0.0, 1.0),
        _ => (1.0, 0.0, q),
    };
    let byte = |v: f64| ((v * 255.0 + 0.5).floor() as u32).min(255);
    (byte(r) << 16) | (byte(g) << 8) | byte(b)
}

/// The slots to show, each with the armour slot (`0` head to `3` feet) it came from, in drawing
/// order. `pieces` are head, chest, legs, feet; `warns` says whether a present piece warns.
pub fn visible_slots<T>(
    shown: WidgetShown,
    reversed: bool,
    pieces: Vec<Option<T>>,
    warns: impl Fn(&T) -> bool,
) -> Vec<(usize, Option<T>)> {
    let any_present = pieces.iter().any(Option::is_some);
    let mut out: Vec<(usize, Option<T>)> = pieces
        .into_iter()
        .enumerate()
        .filter(|(_, piece)| match shown {
            WidgetShown::Always => true,
            WidgetShown::IfAnyPresent => any_present,
            WidgetShown::NotEmpty => piece.is_some(),
            WidgetShown::DamagedPieces => piece.as_ref().is_some_and(&warns),
        })
        .collect();
    if reversed {
        out.reverse();
    }
    out
}

/// The widget's box, without the warning and the text. None when there is no slot.
pub fn widget_rect(cfg: &ArmorHudConfig, frame: &Frame, slots: usize) -> Option<Rect> {
    if slots == 0 {
        return None;
    }
    let count = i32::try_from(slots).ok()?;
    let anchored_left_of_hotbar = cfg.anchor == Anchor::Hotbar && cfg.side == Side::Left;
    let flipped =
        anchored_left_of_hotbar || (cfg.anchor != Anchor::Hotbar && cfg.side == Side::Right);
    let (side_mul, side_offset_mul) = if flipped { (-1, -1) } else { (1, 0) };

    let added = match cfg.offhand_behavior {
        OffhandBehavior::AlwaysIgnore => 0,
        OffhandBehavior::AlwaysLeaveSpace => {
            if frame.main_arm_on_side {
                ATTACK_INDICATOR_OFFSET
            } else {
                OFFHAND_OFFSET
            }
        }
        OffhandBehavior::Adhere => {
            if frame.main_arm_on_side {
                if frame.attack_indicator_showing {
                    ATTACK_INDICATOR_OFFSET
                } else {
                    0
                }
            } else if frame.offhand_filled {
                OFFHAND_OFFSET
            } else {
                0
            }
        }
    };

    let texture_width = SIZE + (count - 1) * STEP;
    let vertical = cfg.orientation == Orientation::Vertical;
    let (w, h) = if vertical {
        (SIZE, texture_width)
    } else {
        (texture_width, SIZE)
    };

    let x = cfg.offset_x * side_mul
        + match cfg.anchor {
            Anchor::TopCenter => (frame.gui_width - w) / 2,
            Anchor::Top | Anchor::VerticalCenter | Anchor::Bottom => {
                (w - frame.gui_width) * side_offset_mul
            }
            Anchor::Hotbar => {
                frame.gui_width / 2 + (HOTBAR_OFFSET + added) * side_mul + w * side_offset_mul
            }
        };
    let y = match cfg.anchor {
        Anchor::Bottom | Anchor::Hotbar => frame.gui_height - h - cfg.offset_y,
        Anchor::Top | Anchor::TopCenter => cfg.offset_y,
        Anchor::VerticalCenter => (frame.gui_height - h) / 2 + cfg.offset_y,
    };
    Some(Rect { x, y, w, h })
}

/// The box including the warning icon and the durability text, which other layers make room for.
pub fn effective_rect(cfg: &ArmorHudConfig, rect: Rect) -> Rect {
    let mut rect = rect;
    if cfg.orientation == Orientation::Horizontal {
        let mut extra = 0;
        if cfg.warning_shown {
            extra += WARNING_SIZE + 2 + cfg.warning_bob / 2;
        }
        if cfg.durability_display != DurabilityDisplay::Bar {
            extra += LINE_HEIGHT;
        }
        rect.h += extra;
        if !cfg.anchor.is_top() {
            rect.y -= extra;
        }
    }
    rect
}

/// How far each pushed layer moves, in GUI pixels downwards (up when negative).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pushes {
    pub effects: i32,
    pub bossbars: i32,
    pub subtitles: i32,
}

pub fn pushes(cfg: &ArmorHudConfig, rect: Option<Rect>) -> Pushes {
    let Some(rect) = rect else {
        return Pushes::default();
    };
    if !cfg.enabled {
        return Pushes::default();
    }
    let eff = effective_rect(cfg, rect);
    Pushes {
        effects: if cfg.push_effects && cfg.anchor == Anchor::Top && cfg.side == Side::Right {
            eff.y + eff.h
        } else {
            0
        },
        bossbars: if cfg.push_bossbars && cfg.anchor == Anchor::TopCenter {
            eff.y + eff.h
        } else {
            0
        },
        // The subtitles sit about 25 pixels above the bottom of the screen.
        subtitles: if cfg.push_subtitles && cfg.anchor == Anchor::Bottom && cfg.side == Side::Right
        {
            -(eff.h - 25).max(0)
        } else {
            0
        },
    }
}

/// One slice of the slot background, drawn in the widget's own frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slice {
    pub sprite: &'static str,
    /// Source `[sprite width, sprite height, u, v]`.
    pub region: [i32; 4],
    /// Destination `[x, y, w, h]`.
    pub dst: [i32; 4],
}

fn hotbar_slice(u: i32, x: i32, w: i32) -> Slice {
    Slice {
        sprite: HOTBAR_SPRITE_ID,
        region: [HOTBAR_SPRITE.0, HOTBAR_SPRITE.1, u, 0],
        dst: [x, 0, w, SIZE],
    }
}

fn offhand_slice(u: i32, x: i32, w: i32) -> Slice {
    Slice {
        sprite: OFFHAND_SPRITE_ID,
        region: [OFFHAND_SPRITE.0, OFFHAND_SPRITE.1, u, 1],
        dst: [x, 0, w, SIZE],
    }
}

/// The slot background for `slots` slots, in the frame whose origin is the widget's corner.
pub fn background(style: Style, slots: usize) -> Vec<Slice> {
    let count = i32::try_from(slots).unwrap_or(0);
    if count <= 0 {
        return Vec::new();
    }
    let width = SIZE + (count - 1) * STEP;
    match style {
        Style::None => Vec::new(),
        Style::Hotbar => vec![
            hotbar_slice(0, 0, width - 3),
            hotbar_slice(HOTBAR_SPRITE.0 - 3, width - 3, 3),
        ],
        Style::RoundedCorners => {
            if count > 1 {
                vec![
                    offhand_slice(0, 0, 3),
                    hotbar_slice(3, 3, width - 6),
                    offhand_slice(SIZE - 3, width - 3, 3),
                ]
            } else {
                vec![offhand_slice(0, 0, SIZE)]
            }
        }
        Style::Rounded => {
            if count > 1 {
                let border = (SIZE - STEP) / 2;
                let mut out = vec![offhand_slice(0, 0, SIZE - border)];
                for i in 1..count - 1 {
                    out.push(offhand_slice(border, border + i * STEP, STEP));
                }
                out.push(offhand_slice(1, width - STEP - border, SIZE - border));
                out
            } else {
                vec![offhand_slice(0, 0, SIZE)]
            }
        }
    }
}

/// Text to draw: `at` is where `text` is placed (centred on it when `centered`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub centered: bool,
    pub color: u32,
}

/// What one slot draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotPlan {
    pub armor_slot: usize,
    /// Top left of the slot background; the item goes 3 pixels in.
    pub x: i32,
    pub y: i32,
    pub empty_icon: bool,
    pub label: Option<Label>,
    /// Top left of the warning icon.
    pub warning: Option<(i32, i32)>,
}

/// What a worn piece shows, to plan its slot.
#[derive(Clone, Debug)]
pub struct PieceFacts {
    pub damageable: bool,
    pub damage: u32,
    pub max_damage: Option<u32>,
    pub bar_color: u32,
}

/// Plan every slot. `text_width` measures a label; `bob(n)` answers a number in `0..n`.
pub fn plan_slots(
    cfg: &ArmorHudConfig,
    rect: Rect,
    slots: &[(usize, Option<PieceFacts>)],
    text_width: &dyn Fn(&str) -> i32,
    bob: &mut dyn FnMut(i32) -> i32,
) -> Vec<SlotPlan> {
    let vertical = cfg.orientation == Orientation::Vertical;
    let top = cfg.anchor.is_top();
    // Beside the hotbar the extras go to the hotbar's side; elsewhere to the opposite one.
    let extras_side = if cfg.anchor == Anchor::Hotbar {
        cfg.side
    } else {
        cfg.side.opposite()
    };
    let mut out = Vec::with_capacity(slots.len());
    for (i, (armor_slot, piece)) in slots.iter().enumerate() {
        let step = i32::try_from(i).unwrap_or(0) * STEP;
        let (sx, sy) = if vertical {
            (rect.x, rect.y + step)
        } else {
            (rect.x + step, rect.y)
        };
        let (mut x, mut y) = (sx, sy);
        if top && !vertical {
            y += SIZE;
        } else if extras_side == Side::Right && vertical {
            x += SIZE;
        }

        let mut label = None;
        if cfg.durability_display != DurabilityDisplay::Bar
            && let Some(p) = piece
            && p.damageable
            && let Some(max) = p.max_damage
        {
            let text = durability_text(cfg.durability_display, p.damage, max);
            let color = 0xFF00_0000 | p.bar_color;
            if !vertical {
                if !top {
                    y -= LINE_HEIGHT;
                }
                label = Some(Label {
                    text,
                    x: x + SIZE / 2,
                    y,
                    centered: true,
                    color,
                });
                if top {
                    y += LINE_HEIGHT;
                }
            } else {
                let width = text_width(&text) + 2;
                let text_y = (SIZE - LINE_HEIGHT) / 2;
                if extras_side == Side::Left {
                    x -= width;
                }
                label = Some(Label {
                    text,
                    x: x + 1,
                    y: y + text_y,
                    centered: false,
                    color,
                });
                if extras_side == Side::Right {
                    x += width;
                }
            }
        }

        let mut warning = None;
        if cfg.warning_shown
            && let Some(p) = piece
            && should_show_warning(p.damageable, p.damage, p.max_damage, cfg)
        {
            if cfg.warning_bob != 0 {
                y += bob(cfg.warning_bob) - (cfg.warning_bob + 1) / 2;
            }
            if !vertical {
                if !top {
                    y -= WARNING_SIZE + 2;
                }
                warning = Some((x + (SIZE - WARNING_SIZE) / 2, y + 1));
            } else {
                if extras_side == Side::Left {
                    x -= WARNING_SIZE + 2;
                }
                warning = Some((x + 1, y + (SIZE - WARNING_SIZE) / 2));
            }
        }

        out.push(SlotPlan {
            armor_slot: *armor_slot,
            x: sx,
            y: sy,
            empty_icon: piece.is_none() && cfg.icons_shown && cfg.widget_shown.draws_empty_slots(),
            label,
            warning,
        });
    }
    out
}

/// The hand extras that sit on the hotbar: the offhand's text and warning, the main hand's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandPlan {
    pub label: Option<Label>,
    pub warning: Option<(i32, i32)>,
}

/// The offhand slot's durability text and warning, above its hotbar slot.
pub fn plan_offhand(
    cfg: &ArmorHudConfig,
    frame: &Frame,
    main_arm_right: bool,
    piece: &PieceFacts,
    bob: &mut dyn FnMut(i32) -> i32,
) -> HandPlan {
    let x = if main_arm_right {
        frame.gui_width / 2 - 91 - 29
    } else {
        frame.gui_width / 2 + 91 + 7
    };
    let mut y = frame.gui_height - SIZE;
    let mut label = None;
    if cfg.durability_display != DurabilityDisplay::Bar
        && let Some(max) = piece.max_damage
    {
        y -= LINE_HEIGHT;
        label = Some(Label {
            text: durability_text(cfg.durability_display, piece.damage, max),
            x: x + SIZE / 2,
            y,
            centered: true,
            color: 0xFF00_0000 | piece.bar_color,
        });
    }
    let mut warning = None;
    if cfg.warning_shown
        && should_show_warning(piece.damageable, piece.damage, piece.max_damage, cfg)
    {
        if cfg.warning_bob != 0 {
            y += bob(cfg.warning_bob) - (cfg.warning_bob + 1) / 2;
        }
        y -= WARNING_SIZE + 2;
        warning = Some((x + (SIZE - WARNING_SIZE) / 2, y + 1));
    }
    HandPlan { label, warning }
}

/// The main hand's durability text above the selected hotbar slot, centred on `x`.
pub fn plan_mainhand(
    cfg: &ArmorHudConfig,
    frame: &Frame,
    selected: i32,
    piece: &PieceFacts,
) -> Option<Label> {
    if cfg.durability_display == DurabilityDisplay::Bar {
        return None;
    }
    let max = piece.max_damage?;
    Some(Label {
        text: durability_text(cfg.durability_display, piece.damage, max),
        x: frame.gui_width / 2 - 91 + selected.clamp(0, 8) * 20 + SIZE / 2,
        y: frame.gui_height - SIZE - 1,
        centered: true,
        color: 0xFF00_0000 | piece.bar_color,
    })
}

/// A tiny generator for the warning icon's jitter.
#[derive(Clone, Debug)]
pub struct Jitter(u64);

impl Jitter {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    /// A number in `0..bound`; `0` for a bound under one.
    pub fn below(&mut self, bound: i32) -> i32 {
        if bound <= 1 {
            return 0;
        }
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        i32::try_from((self.0 >> 33) % u64::try_from(bound).unwrap_or(1)).unwrap_or(0)
    }
}

/// What the break sound watches, per slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchedSlot {
    Armor,
    MainHand,
    OffHand,
}

/// A stack as the break sound remembers it between ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Remembered<K> {
    pub item: Option<K>,
    pub damage: u32,
}

impl<K> Default for Remembered<K> {
    fn default() -> Self {
        Self {
            item: None,
            damage: 0,
        }
    }
}

/// Whether the change from `last` to `current` should play the armour breaking sound.
pub fn should_play_break_sound<K: PartialEq>(
    slot: WatchedSlot,
    last: &Remembered<K>,
    current: &Remembered<K>,
    current_warns: bool,
    cfg: &ArmorHudConfig,
) -> bool {
    if last.damage == current.damage || !current_warns {
        return false;
    }
    // Switching the selected hotbar slot to another item is not a wear change.
    if slot == WatchedSlot::MainHand && last.item != current.item {
        return false;
    }
    match slot {
        WatchedSlot::Armor => true,
        WatchedSlot::MainHand => cfg.mainhand_durability,
        WatchedSlot::OffHand => cfg.offhand_durability,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Frame {
        Frame {
            gui_width: 400,
            gui_height: 240,
            main_arm_on_side: false,
            offhand_filled: false,
            attack_indicator_showing: false,
        }
    }

    #[test]
    fn warning_uses_value_or_percentage() {
        let cfg = ArmorHudConfig::default();
        assert!(should_show_warning(true, 230, Some(250), &cfg));
        assert!(!should_show_warning(true, 100, Some(250), &cfg));
        assert!(!should_show_warning(true, 1000, Some(1500), &cfg));
        // 10% of 2000 is 200, above the 20 value threshold.
        assert!(should_show_warning(true, 1800, Some(2000), &cfg));
        assert!(!should_show_warning(false, 249, Some(250), &cfg));
        assert!(!should_show_warning(true, 0, None, &cfg));
        assert!(!should_show_warning(true, 0, Some(0), &cfg));
        assert!(should_show_warning(true, 999, Some(10), &cfg));
    }

    #[test]
    fn durability_texts() {
        assert_eq!(durability_text(DurabilityDisplay::Numeric, 30, 250), "220");
        assert_eq!(durability_text(DurabilityDisplay::Numeric, 300, 250), "0");
        assert_eq!(durability_text(DurabilityDisplay::Percentage, 0, 250), "");
        assert_eq!(
            durability_text(DurabilityDisplay::Percentage, 125, 250),
            "50%"
        );
        assert_eq!(
            durability_text(DurabilityDisplay::Percentage, 1, 250),
            "99%"
        );
        assert_eq!(durability_text(DurabilityDisplay::Bar, 1, 250), "");
    }

    #[test]
    fn bar_colour_runs_green_to_red() {
        assert_eq!(bar_color(0, 100), 0x00FF00);
        assert_eq!(bar_color(100, 100), 0xFF0000);
        assert_eq!(bar_color(500, 100), 0xFF0000);
        assert_eq!(bar_color(5, 0), 0xFF0000);
        assert_eq!(bar_color(50, 100), 0xFFFF00);
    }

    #[test]
    fn visible_slots_filter_and_reverse() {
        let pieces = vec![Some(1u32), None, Some(3), None];
        let warns = |p: &u32| *p == 3;
        let ids = |v: Vec<(usize, Option<u32>)>| v.into_iter().map(|(i, _)| i).collect::<Vec<_>>();
        assert_eq!(
            ids(visible_slots(
                WidgetShown::Always,
                false,
                pieces.clone(),
                warns
            )),
            [0, 1, 2, 3]
        );
        assert_eq!(
            ids(visible_slots(
                WidgetShown::NotEmpty,
                false,
                pieces.clone(),
                warns
            )),
            [0, 2]
        );
        assert_eq!(
            ids(visible_slots(
                WidgetShown::NotEmpty,
                true,
                pieces.clone(),
                warns
            )),
            [2, 0]
        );
        assert_eq!(
            ids(visible_slots(
                WidgetShown::DamagedPieces,
                false,
                pieces.clone(),
                warns
            )),
            [2]
        );
        assert_eq!(
            ids(visible_slots(
                WidgetShown::IfAnyPresent,
                false,
                pieces,
                warns
            )),
            [0, 1, 2, 3]
        );
        let none: Vec<Option<u32>> = vec![None; 4];
        assert!(visible_slots(WidgetShown::IfAnyPresent, false, none, warns).is_empty());
    }

    #[test]
    fn hotbar_anchor_sits_beside_the_hotbar() {
        let mut cfg = ArmorHudConfig::default();
        let left = widget_rect(&cfg, &frame(), 4).unwrap();
        assert_eq!(left.w, 82);
        assert_eq!(left.x, 200 - 98 - 82);
        assert_eq!(left.y, 240 - 22);
        cfg.side = Side::Right;
        let right = widget_rect(&cfg, &frame(), 4).unwrap();
        assert_eq!(right.x, 200 + 98);
        assert!(widget_rect(&cfg, &frame(), 0).is_none());
    }

    #[test]
    fn offhand_makes_space_on_the_far_side_only() {
        let mut cfg = ArmorHudConfig::default();
        cfg.side = Side::Right;
        let mut f = frame();
        f.offhand_filled = true;
        // Main arm not on the widget's side: the offhand is there.
        assert_eq!(widget_rect(&cfg, &f, 1).unwrap().x, 200 + 98 + 29);
        f.main_arm_on_side = true;
        assert_eq!(widget_rect(&cfg, &f, 1).unwrap().x, 200 + 98);
        f.attack_indicator_showing = true;
        assert_eq!(widget_rect(&cfg, &f, 1).unwrap().x, 200 + 98 + 23);
        cfg.offhand_behavior = OffhandBehavior::AlwaysLeaveSpace;
        f.attack_indicator_showing = false;
        assert_eq!(widget_rect(&cfg, &f, 1).unwrap().x, 200 + 98 + 23);
        cfg.offhand_behavior = OffhandBehavior::AlwaysIgnore;
        assert_eq!(widget_rect(&cfg, &f, 1).unwrap().x, 200 + 98);
    }

    #[test]
    fn other_anchors() {
        let mut cfg = ArmorHudConfig::default();
        cfg.anchor = Anchor::TopCenter;
        let r = widget_rect(&cfg, &frame(), 2).unwrap();
        assert_eq!((r.x, r.y, r.w, r.h), ((400 - 42) / 2, 0, 42, 22));
        cfg.anchor = Anchor::VerticalCenter;
        cfg.orientation = Orientation::Vertical;
        cfg.side = Side::Right;
        cfg.offset_x = 5;
        cfg.offset_y = -4;
        let r = widget_rect(&cfg, &frame(), 2).unwrap();
        assert_eq!((r.w, r.h), (22, 42));
        assert_eq!(r.x, -5 + (22 - 400) * -1);
        assert_eq!(r.y, (240 - 42) / 2 - 4);
        cfg.anchor = Anchor::Bottom;
        assert_eq!(widget_rect(&cfg, &frame(), 2).unwrap().y, 240 - 42 + 4);
    }

    #[test]
    fn effective_rect_grows_away_from_the_edge() {
        let mut cfg = ArmorHudConfig::default();
        cfg.anchor = Anchor::Top;
        cfg.durability_display = DurabilityDisplay::Numeric;
        let rect = Rect {
            x: 0,
            y: 5,
            w: 22,
            h: 22,
        };
        let eff = effective_rect(&cfg, rect);
        assert_eq!((eff.y, eff.h), (5, 22 + 8 + 2 + 1 + 9));
        cfg.anchor = Anchor::Bottom;
        let eff = effective_rect(&cfg, rect);
        assert_eq!((eff.y, eff.h), (5 - 20, 42));
        cfg.orientation = Orientation::Vertical;
        assert_eq!(effective_rect(&cfg, rect), rect);
    }

    #[test]
    fn pushes_follow_anchor_side_and_switches() {
        let mut cfg = ArmorHudConfig::default();
        let rect = Some(Rect {
            x: 0,
            y: 0,
            w: 22,
            h: 22,
        });
        assert_eq!(pushes(&cfg, rect), Pushes::default());
        cfg.anchor = Anchor::TopCenter;
        assert_eq!(pushes(&cfg, rect).bossbars, 22 + 8 + 2 + 1);
        cfg.push_bossbars = false;
        assert_eq!(pushes(&cfg, rect).bossbars, 0);
        cfg.anchor = Anchor::Top;
        cfg.side = Side::Right;
        assert_eq!(pushes(&cfg, rect).effects, 33);
        cfg.anchor = Anchor::Bottom;
        assert_eq!(pushes(&cfg, rect).subtitles, -8);
        assert_eq!(pushes(&cfg, None), Pushes::default());
        cfg.enabled = false;
        assert_eq!(pushes(&cfg, rect), Pushes::default());
    }

    #[test]
    fn backgrounds_cover_the_widget() {
        assert!(background(Style::None, 4).is_empty());
        assert!(background(Style::Hotbar, 0).is_empty());
        let hotbar = background(Style::Hotbar, 4);
        assert_eq!(hotbar[0].dst, [0, 0, 79, 22]);
        assert_eq!(hotbar[1].dst, [79, 0, 3, 22]);
        assert_eq!(hotbar[1].region, [182, 22, 179, 0]);
        assert_eq!(background(Style::RoundedCorners, 1).len(), 1);
        assert_eq!(background(Style::RoundedCorners, 3).len(), 3);
        let rounded = background(Style::Rounded, 4);
        assert_eq!(rounded.len(), 4);
        assert_eq!(rounded[0].dst[2], 21);
        assert_eq!(rounded[3].dst[0], 82 - 20 - 1);
    }

    fn facts(damage: u32, max: u32) -> PieceFacts {
        PieceFacts {
            damageable: true,
            damage,
            max_damage: Some(max),
            bar_color: 0x00FF00,
        }
    }

    #[test]
    fn slot_plan_places_text_and_warning() {
        let mut cfg = ArmorHudConfig::default();
        cfg.durability_display = DurabilityDisplay::Numeric;
        cfg.warning_bob = 0;
        let rect = Rect {
            x: 10,
            y: 100,
            w: 42,
            h: 22,
        };
        let slots = vec![(0, Some(facts(245, 250))), (1, None)];
        let plans = plan_slots(&cfg, rect, &slots, &|t| t.len() as i32 * 6, &mut |_| 0);
        assert_eq!(plans[0].x, 10);
        assert_eq!(plans[1].x, 30);
        let label = plans[0].label.as_ref().unwrap();
        assert_eq!((label.text.as_str(), label.x, label.y), ("5", 21, 100 - 9));
        assert_eq!(plans[0].warning, Some((10 + 7, 100 - 9 - 10 + 1)));
        assert!(!plans[1].empty_icon);
        cfg.widget_shown = WidgetShown::Always;
        let plans = plan_slots(&cfg, rect, &slots, &|_| 0, &mut |_| 0);
        assert!(plans[1].empty_icon && plans[1].label.is_none());
    }

    #[test]
    fn vertical_slot_plan_uses_the_extras_side() {
        let mut cfg = ArmorHudConfig::default();
        cfg.orientation = Orientation::Vertical;
        cfg.durability_display = DurabilityDisplay::Numeric;
        cfg.warning_bob = 0;
        cfg.anchor = Anchor::Top;
        cfg.side = Side::Left; // extras go right
        let rect = Rect {
            x: 0,
            y: 0,
            w: 22,
            h: 22,
        };
        let slots = vec![(0, Some(facts(249, 250)))];
        let p = &plan_slots(&cfg, rect, &slots, &|_| 6, &mut |_| 0)[0];
        let label = p.label.as_ref().unwrap();
        assert_eq!((label.x, label.y), (22 + 1, 6));
        assert_eq!(p.warning, Some((22 + 8 + 1, 7)));
    }

    #[test]
    fn bob_stays_within_the_original_range() {
        let mut cfg = ArmorHudConfig::default();
        cfg.durability_display = DurabilityDisplay::Bar;
        let rect = Rect {
            x: 0,
            y: 100,
            w: 22,
            h: 22,
        };
        let slots = vec![(0, Some(facts(249, 250)))];
        for roll in 0..3 {
            let p = &plan_slots(&cfg, rect, &slots, &|_| 0, &mut |_| roll)[0];
            let y = p.warning.unwrap().1;
            assert!((100 - 10 + 1 - 2..=100 - 10 + 1 + 1).contains(&y));
        }
    }

    #[test]
    fn hand_plans() {
        let mut cfg = ArmorHudConfig::default();
        cfg.durability_display = DurabilityDisplay::Percentage;
        cfg.warning_bob = 0;
        let f = frame();
        let hand = plan_offhand(&cfg, &f, true, &facts(245, 250), &mut |_| 0);
        assert_eq!(hand.label.as_ref().unwrap().x, 200 - 91 - 29 + 11);
        assert_eq!(
            hand.warning,
            Some((200 - 91 - 29 + 7, 240 - 22 - 9 - 10 + 1))
        );
        let left = plan_offhand(&cfg, &f, false, &facts(0, 250), &mut |_| 0);
        assert_eq!(left.label.as_ref().unwrap().text, "");
        assert!(left.warning.is_none());
        let main = plan_mainhand(&cfg, &f, 3, &facts(125, 250)).unwrap();
        assert_eq!((main.x, main.y), (200 - 91 + 60 + 11, 240 - 23));
        assert_eq!(
            plan_mainhand(&cfg, &f, 99, &facts(1, 250)).unwrap().x,
            200 - 91 + 160 + 11
        );
        cfg.durability_display = DurabilityDisplay::Bar;
        assert!(plan_mainhand(&cfg, &f, 3, &facts(1, 250)).is_none());
    }

    #[test]
    fn jitter_is_bounded() {
        let mut j = Jitter::new(0);
        for _ in 0..200 {
            assert!((0..3).contains(&j.below(3)));
        }
        assert_eq!(j.below(0), 0);
        assert_eq!(j.below(-4), 0);
    }

    #[test]
    fn break_sound_rules() {
        let mut cfg = ArmorHudConfig::default();
        let r = |item: u32, damage| Remembered {
            item: Some(item),
            damage,
        };
        assert!(should_play_break_sound(
            WatchedSlot::Armor,
            &r(1, 10),
            &r(1, 11),
            true,
            &cfg
        ));
        assert!(!should_play_break_sound(
            WatchedSlot::Armor,
            &r(1, 10),
            &r(1, 11),
            false,
            &cfg
        ));
        assert!(!should_play_break_sound(
            WatchedSlot::Armor,
            &r(1, 10),
            &r(1, 10),
            true,
            &cfg
        ));
        // Equipping a damaged piece is loud.
        assert!(should_play_break_sound(
            WatchedSlot::Armor,
            &Remembered::default(),
            &r(1, 240),
            true,
            &cfg
        ));
        // A hotbar switch is not.
        assert!(!should_play_break_sound(
            WatchedSlot::MainHand,
            &r(1, 0),
            &r(2, 240),
            true,
            &cfg
        ));
        assert!(!should_play_break_sound(
            WatchedSlot::MainHand,
            &r(1, 5),
            &r(1, 6),
            true,
            &cfg
        ));
        cfg.mainhand_durability = true;
        assert!(should_play_break_sound(
            WatchedSlot::MainHand,
            &r(1, 5),
            &r(1, 6),
            true,
            &cfg
        ));
        cfg.offhand_durability = false;
        assert!(!should_play_break_sound(
            WatchedSlot::OffHand,
            &r(1, 5),
            &r(1, 6),
            true,
            &cfg
        ));
    }
}
