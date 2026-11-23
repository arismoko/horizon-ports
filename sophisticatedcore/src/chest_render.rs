//! The chests' and shulker boxes' own drawing (ChestRenderer, ShulkerBoxRenderer, LockRenderer): their paint, their
//! lock and their tier.
//!
//! A chest's or shulker box's block model is empty (only its particle texture); everything is drawn here with the
//! models' own geometry and texture rectangles: the wood, the paint over it, then the tier, which is simply left out
//! when hidden. The lid swings with the open state the server syncs, as `ChestRenderer` and `ShulkerBoxRenderer`
//! move it. A packed storage keeps its static model.
use mod_api::{BlockEntityDrawContext, CubeFace};

use crate::render::{model_turn, north_based};

/// Which half of a double chest a chest block is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Join {
    Single,
    Left,
    Right,
}

impl Join {
    pub fn parse(s: &str) -> Self {
        match s {
            "left" => Self::Left,
            "right" => Self::Right,
            _ => Self::Single,
        }
    }

    /// The textures' name prefix.
    fn prefix(self) -> &'static str {
        match self {
            Self::Single => "",
            Self::Left => "left_",
            Self::Right => "right_",
        }
    }

    /// Where the lock sits across the front, in 16ths of the block: a double chest's lock is on its seam.
    pub fn lock_x(self) -> f32 {
        match self {
            Self::Single => 8.0,
            Self::Left => 0.0,
            Self::Right => 16.0,
        }
    }
}

/// A box of a model with the texture rectangle of each face, in the order down, up, north, south, west, east.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Part {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub uv: [[f32; 4]; 6],
}

/// A box laid out as the chest textures are: `depth` its side's width on the texture, `width` its front's divided by
/// four, `down` and `side` the rows of the top and the sides.
fn part(
    from: [f32; 3],
    to: [f32; 3],
    depth: f32,
    width: f32,
    down: (f32, f32),
    side: (f32, f32),
) -> Part {
    let (a, b) = (depth, width);
    Part {
        from,
        to,
        uv: [
            [a, down.0, a + b, down.1],
            [a + b, down.1, a + 2.0 * b, down.0],
            [a + b, side.0, a, side.1],
            [2.0 * a + 2.0 * b, side.0, 2.0 * a + b, side.1],
            [a, side.0, 0.0, side.1],
            [2.0 * a + b, side.0, a + b, side.1],
        ],
    }
}

/// The three boxes of a chest, as `template/chest_*` has them with the front to the south: body, lid and latch.
pub fn chest_parts(join: Join) -> [Part; 3] {
    let (x0, x1) = match join {
        Join::Single => (1.0, 15.0),
        Join::Left => (0.0, 15.0),
        Join::Right => (1.0, 16.0),
    };
    let w = (x1 - x0) / 4.0;
    let (l0, l1) = match join {
        Join::Single => (7.0, 9.0),
        Join::Left => (0.0, 1.0),
        Join::Right => (15.0, 16.0),
    };
    [
        part(
            [x0, 0.0, 1.0],
            [x1, 10.0, 15.0],
            3.5,
            w,
            (4.75, 8.25),
            (10.75, 8.25),
        ),
        part(
            [x0, 9.0, 1.0],
            [x1, 14.0, 15.0],
            3.5,
            w,
            (0.0, 3.5),
            (4.75, 3.5),
        ),
        part(
            [l0, 7.0, 15.0],
            [l1, 11.0, 16.0],
            0.25,
            (l1 - l0) / 4.0,
            (0.0, 0.25),
            (1.25, 0.25),
        ),
    ]
}

/// The two boxes of a shulker box with its lid up: the upper (lid and top) and the lower.
pub fn shulker_parts() -> [Part; 2] {
    [
        Part {
            from: [0.0, 4.0, 0.0],
            to: [16.0, 16.0, 16.0],
            uv: [
                [8.0, 4.0, 12.0, 0.0],
                [4.0, 0.0, 8.0, 4.0],
                [12.0, 4.0, 16.0, 7.0],
                [4.0, 4.0, 8.0, 7.0],
                [0.0, 4.0, 4.0, 7.0],
                [8.0, 4.0, 12.0, 7.0],
            ],
        },
        Part {
            from: [0.0, 0.0, 0.0],
            to: [16.0, 8.0, 16.0],
            uv: [
                [8.0, 11.0, 12.0, 7.0],
                [4.0, 7.0, 8.0, 11.0],
                [12.0, 11.0, 16.0, 13.0],
                [4.0, 11.0, 8.0, 13.0],
                [0.0, 11.0, 4.0, 13.0],
                [8.0, 11.0, 12.0, 13.0],
            ],
        },
    ]
}

/// The blockstate's turn of a chest with its front `facing`, in degrees about y (south is the model's own).
pub fn chest_turn(facing: &str) -> f32 {
    match facing {
        "west" => 90.0,
        "north" => 180.0,
        "east" => 270.0,
        _ => 0.0,
    }
}

/// What the client knows of a chest or shulker box to draw it.
pub struct Look<'a> {
    pub main: Option<i32>,
    pub accent: Option<i32>,
    pub locked: bool,
    pub show_lock: bool,
    pub show_tier: bool,
    /// The player holds the storage tool in a mode that shows the lock, or the tier.
    pub tool_shows_lock: bool,
    pub tool_shows_tier: bool,
    /// The sprite of the wood, or of the plain shulker box, under the paint.
    pub base: &'a str,
    /// The two tint sprites.
    pub tint_main: &'a str,
    pub tint_accent: &'a str,
    /// The tier's own sprite.
    pub tier: &'a str,
    /// How far the lid is open, 0 to 1 (a chest's eased: `1 - (1 - t)^3`).
    pub lid: f32,
}

impl Look<'_> {
    fn tier_visible(&self) -> bool {
        self.show_tier || self.tool_shows_tier
    }

    fn lock_visible(&self) -> bool {
        self.locked && (self.show_lock || self.tool_shows_lock)
    }
}

/// The layers, bottom to top: (sprite, tint, growth in texels). Each grows more than the one under it so none
/// fights another over a face; the tier is last, as the original draws it, and absent when hidden.
fn layers<'a>(look: &Look<'a>) -> Vec<(&'a str, u32, f32)> {
    let mut v = vec![(look.base, 0xFF_FFFF, 0.0)];
    if let Some(c) = look.main {
        v.push((look.tint_main, c as u32 & 0xFF_FFFF, 0.02));
    }
    if let Some(c) = look.accent {
        v.push((look.tint_accent, c as u32 & 0xFF_FFFF, 0.04));
    }
    if look.tier_visible() {
        v.push((look.tier, 0xFF_FFFF, 0.06));
    }
    v
}

/// Draws `part` in every layer, the part's own box grown by the layer's growth.
fn draw_part(draw: &mut dyn BlockEntityDrawContext, part: &Part, layers: &[(&str, u32, f32)]) {
    for &(sprite, tint, grow) in layers {
        let faces = part.uv.map(|uv| Some(CubeFace::new(sprite).with_uv(uv)));
        draw.cube(
            part.from.map(|v| v - grow),
            part.to.map(|v| v + grow),
            faces,
            tint,
        );
    }
}

/// A chest lid's turn about its hinge (the back edge at the lid's foot): degrees about x, front up.
fn lid_degrees(lid: f32) -> f32 {
    -lid * 90.0
}

const CHEST_HINGE: [f32; 3] = [0.5, 9.0 / 16.0, 1.0 / 16.0];

/// A chest: body, lid and latch in every layer, the lid and latch swung open, then the lock if it is the main half.
pub fn draw_chest(
    draw: &mut dyn BlockEntityDrawContext,
    facing: &str,
    join: Join,
    look: &Look<'_>,
    main_half: bool,
) {
    let layers = layers(look);
    let [body, lid, latch] = chest_parts(join);
    draw.push();
    draw.rotate_about([0.0, 1.0, 0.0], -chest_turn(facing), [0.5, 0.5, 0.5]);
    draw_part(draw, &body, &layers);
    draw.push();
    draw.rotate_about([1.0, 0.0, 0.0], lid_degrees(look.lid), CHEST_HINGE);
    draw_part(draw, &lid, &layers);
    draw_part(draw, &latch, &layers);
    if main_half && look.lock_visible() {
        let x = join.lock_x() / 16.0;
        let h = 0.5 / 16.0;
        let (y0, y1, z) = (13.0 / 16.0, 14.0 / 16.0, 1.001);
        draw.quad(
            LOCK,
            [
                [x - h, y1, z],
                [x - h, y0, z],
                [x + h, y0, z],
                [x + h, y1, z],
            ],
            [0.0, 0.0, 16.0, 16.0],
            0xFF_FFFF,
        );
    }
    draw.pop();
    draw.pop();
}

/// The lid's lift and spin at `lid` 0 to 1: half a block along the opening, three quarters of a turn.
fn shulker_lid(lid: f32) -> (f32, f32) {
    (0.5 * lid, 270.0 * lid)
}

/// A shulker box: body and lid in every layer, the lid lifted and spun, then its lock on the face it opens to.
pub fn draw_shulker(draw: &mut dyn BlockEntityDrawContext, facing: &str, look: &Look<'_>) {
    let layers = layers(look);
    let [lid, body] = shulker_parts();
    let (lift, spin) = shulker_lid(look.lid);
    let (x, y) = model_turn(facing);
    draw.push();
    draw.rotate_about([0.0, 1.0, 0.0], -y, [0.5, 0.5, 0.5]);
    draw.rotate_about([1.0, 0.0, 0.0], -x, [0.5, 0.5, 0.5]);
    draw_part(draw, &body, &layers);
    draw.push();
    draw.translate([0.0, lift, 0.0]);
    draw.rotate_about([0.0, 1.0, 0.0], -spin, [0.5, 0.5, 0.5]);
    draw_part(draw, &lid, &layers);
    draw.pop();
    draw.pop();
    if look.lock_visible() {
        draw.push();
        if let Some((axis, deg)) = north_based(facing) {
            draw.rotate_about(axis, deg, [0.5, 0.5, 0.5]);
        }
        // In this frame the opening direction is north (-z) and the lid's spin is about z.
        draw.translate([0.0, 0.0, -lift]);
        draw.rotate_about([0.0, 0.0, 1.0], spin, [0.5, 0.5, 0.5]);
        let (x0, x1) = (0.5 - 0.5 / 16.0, 0.5 + 0.5 / 16.0);
        let (y0, y1, z) = (15.0 / 16.0, 1.0, -0.001);
        draw.quad(
            LOCK,
            [[x1, y1, z], [x1, y0, z], [x0, y0, z], [x0, y1, z]],
            [0.0, 0.0, 16.0, 16.0],
            0xFF_FFFF,
        );
        draw.pop();
    }
}

/// How far a lid has moved after `ticks`, toward open or shut, at a tenth per tick (`ChestLidController`,
/// `ShulkerBoxBlockEntity.updateAnimation`).
pub fn lid_step(progress: f32, open: bool, ticks: f32) -> f32 {
    let to = if open { 1.0 } else { 0.0 };
    let by = 0.1 * ticks.max(0.0);
    if progress < to {
        (progress + by).min(to)
    } else {
        (progress - by).max(to)
    }
}

/// The chest lid's ease (`ChestRenderer`: `1 - (1 - t)^3`).
pub fn chest_ease(t: f32) -> f32 {
    let u = 1.0 - t;
    1.0 - u * u * u
}

const LOCK: &str = "sophisticatedstorage:block/lock";

/// The sprites of a chest's wood and its two paint layers.
pub fn chest_sprites(join: Join, wood: &str) -> (String, String, String) {
    let p = join.prefix();
    (
        format!("sophisticatedstorage:block/entity/chest/{p}{wood}"),
        format!("sophisticatedstorage:block/entity/chest/{p}tintable_main"),
        format!("sophisticatedstorage:block/entity/chest/{p}tintable_accent"),
    )
}

/// The sprite of a chest's tier layer: `block` is the storage's block id (`iron_chest`, `chest` for wood).
pub fn chest_tier_sprite(join: Join, block: &str) -> String {
    let name = block.rsplit(':').next().unwrap_or(block);
    let tier = name.strip_suffix("_chest").unwrap_or("wood");
    format!(
        "sophisticatedstorage:block/entity/chest/{}{tier}_tier",
        join.prefix()
    )
}

/// The sprite of a shulker box's tier layer (`shulker_box` is the plain one).
pub fn shulker_tier_sprite(block: &str) -> String {
    let name = block.rsplit(':').next().unwrap_or(block);
    let tier = name.strip_suffix("_shulker_box").unwrap_or("base");
    format!("sophisticatedstorage:block/entity/shulker_box/{tier}_tier")
}

pub const SHULKER_BASE: &str = "sophisticatedstorage:block/entity/shulker_box/no_tint";
pub const SHULKER_MAIN: &str = "sophisticatedstorage:block/entity/shulker_box/tintable_main";
pub const SHULKER_ACCENT: &str = "sophisticatedstorage:block/entity/shulker_box/tintable_accent";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_chest_has_the_template_boxes() {
        let [body, lid, latch] = chest_parts(Join::Single);
        assert_eq!((body.from, body.to), ([1.0, 0.0, 1.0], [15.0, 10.0, 15.0]));
        assert_eq!((lid.from, lid.to), ([1.0, 9.0, 1.0], [15.0, 14.0, 15.0]));
        assert_eq!(
            (latch.from, latch.to),
            ([7.0, 7.0, 15.0], [9.0, 11.0, 16.0])
        );
        // down, up, north, south, west, east, as `template/chest_single` has them.
        assert_eq!(
            body.uv,
            [
                [3.5, 4.75, 7.0, 8.25],
                [7.0, 8.25, 10.5, 4.75],
                [7.0, 10.75, 3.5, 8.25],
                [14.0, 10.75, 10.5, 8.25],
                [3.5, 10.75, 0.0, 8.25],
                [10.5, 10.75, 7.0, 8.25],
            ]
        );
        assert_eq!(lid.uv[3], [14.0, 4.75, 10.5, 3.5]);
        assert_eq!(latch.uv[1], [0.75, 0.25, 1.25, 0.0]);
    }

    #[test]
    fn the_halves_of_a_double_chest_are_wider_by_a_quarter() {
        let [lbody, _, llatch] = chest_parts(Join::Left);
        assert_eq!((lbody.from[0], lbody.to[0]), (0.0, 15.0));
        assert_eq!(lbody.uv[3], [14.5, 10.75, 10.75, 8.25]);
        assert_eq!(lbody.uv[1], [7.25, 8.25, 11.0, 4.75]);
        assert_eq!((llatch.from[0], llatch.to[0]), (0.0, 1.0));
        assert_eq!(llatch.uv[0], [0.25, 0.0, 0.5, 0.25]);
        let [rbody, _, rlatch] = chest_parts(Join::Right);
        assert_eq!((rbody.from[0], rbody.to[0]), (1.0, 16.0));
        assert_eq!((rlatch.from[0], rlatch.to[0]), (15.0, 16.0));
        assert_eq!(rlatch.uv[5], [0.75, 1.25, 0.5, 0.25]);
    }

    #[test]
    fn the_lock_sits_on_a_double_chests_seam() {
        assert_eq!(Join::Single.lock_x(), 8.0);
        assert_eq!(Join::Left.lock_x(), 0.0);
        assert_eq!(Join::Right.lock_x(), 16.0);
        assert_eq!(Join::parse("right"), Join::Right);
        assert_eq!(Join::parse("single"), Join::Single);
    }

    fn look(show_tier: bool, tool: bool) -> Look<'static> {
        Look {
            main: Some(0xFF0000),
            accent: Some(0x00FF00),
            locked: true,
            show_lock: false,
            show_tier,
            tool_shows_lock: false,
            tool_shows_tier: tool,
            base: "base",
            tint_main: "main",
            tint_accent: "accent",
            tier: "tier",
            lid: 0.0,
        }
    }

    #[test]
    fn layers_grow_upward_and_a_hidden_tier_is_not_drawn() {
        fn names(l: Vec<(&str, u32, f32)>) -> Vec<&str> {
            l.into_iter().map(|x| x.0).collect()
        }
        let shown = layers(&look(true, false));
        assert_eq!(names(shown.clone()), ["base", "main", "accent", "tier"]);
        assert!(shown.windows(2).all(|w| w[0].2 < w[1].2));
        assert_eq!(
            names(layers(&look(false, false))),
            ["base", "main", "accent"]
        );
        // Hidden, but shown by the tool in hand.
        assert_eq!(layers(&look(false, true)).len(), 4);
    }

    #[test]
    fn a_lid_moves_a_tenth_a_tick_and_stops_at_the_ends() {
        assert_eq!(lid_step(0.0, true, 3.0), 0.3);
        assert_eq!(lid_step(0.95, true, 3.0), 1.0);
        assert_eq!(lid_step(1.0, false, 4.0), 0.6);
        assert_eq!(lid_step(0.05, false, 1.0), 0.0);
        assert_eq!(lid_step(0.5, true, -2.0), 0.5);
        assert_eq!(chest_ease(0.0), 0.0);
        assert_eq!(chest_ease(1.0), 1.0);
        assert!(chest_ease(0.5) > 0.5);
        assert_eq!(lid_degrees(1.0), -90.0);
        assert_eq!(shulker_lid(1.0), (0.5, 270.0));
    }

    #[test]
    fn tier_sprites_follow_the_block() {
        assert_eq!(
            chest_tier_sprite(Join::Single, "sophisticatedstorage:chest"),
            "sophisticatedstorage:block/entity/chest/wood_tier"
        );
        assert_eq!(
            chest_tier_sprite(Join::Left, "sophisticatedstorage:iron_chest"),
            "sophisticatedstorage:block/entity/chest/left_iron_tier"
        );
        assert_eq!(
            shulker_tier_sprite("sophisticatedstorage:shulker_box"),
            "sophisticatedstorage:block/entity/shulker_box/base_tier"
        );
        assert_eq!(
            shulker_tier_sprite("sophisticatedstorage:gold_shulker_box"),
            "sophisticatedstorage:block/entity/shulker_box/gold_tier"
        );
    }

    #[test]
    fn the_lock_shows_when_locked_and_displayed_or_the_tool_is_held() {
        let mut l = look(true, false);
        assert!(!l.lock_visible());
        l.tool_shows_lock = true;
        assert!(l.lock_visible());
        l.tool_shows_lock = false;
        l.show_lock = true;
        assert!(l.lock_visible());
        l.locked = false;
        assert!(!l.lock_visible());
    }

    #[test]
    fn chests_turn_as_their_blockstates_do() {
        assert_eq!(chest_turn("south"), 0.0);
        assert_eq!(chest_turn("west"), 90.0);
        assert_eq!(chest_turn("north"), 180.0);
        assert_eq!(chest_turn("east"), 270.0);
    }
}
