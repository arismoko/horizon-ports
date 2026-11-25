//! The pure rules of the stacked bars: which icon every heart or armour point shows, where the
//! row count goes and how far chat moves. Nothing here reads the game.

/// One sprite to draw: a GUI sprite id, its rectangle, an optional `(sprite w, sprite h, u, v)`
/// crop and an ARGB tint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpriteOp {
    pub sprite: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub region: Option<[i32; 4]>,
    pub color: u32,
}

pub const WHITE: u32 = 0xFFFF_FFFF;
/// Tint of the layered rows of the armour bar.
pub const ARMOR_LAYER_TINT: u32 = 0xFFFF_C04D;
/// Tint of the first row of the toughness bar.
pub const TOUGHNESS_TINT: u32 = 0xFF7F_D4FF;
/// Tint of the layered rows of the toughness bar.
pub const TOUGHNESS_LAYER_TINT: u32 = 0xFF9B_7BFF;

/// The most the "shift the bar" settings move a bar, in icon rows either way.
pub const MAX_ROW_SHIFT: i32 = 5;
/// Height of one icon row.
pub const ROW_HEIGHT: i32 = 10;

/// Java's `Random`, as vanilla's `LegacyRandomSource` runs it, for the low-health shake.
#[derive(Clone, Debug)]
pub struct JavaRandom {
    seed: u64,
}

impl JavaRandom {
    const MASK: u64 = (1 << 48) - 1;

    pub fn new(seed: i64) -> Self {
        Self {
            seed: (seed as u64 ^ 0x5_DEEC_E66D) & Self::MASK,
        }
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.seed = self.seed.wrapping_mul(0x5_DEEC_E66D).wrapping_add(0xB) & Self::MASK;
        (self.seed >> (48 - bits)) as i32
    }

    /// `nextInt(bound)` for a power-of-two `bound` (the only bound the hearts use).
    pub fn next_int_pow2(&mut self, bound: i32) -> i32 {
        ((i64::from(bound) * i64::from(self.next(31))) >> 31) as i32
    }
}

/// What the health bar needs to remember between frames (vanilla's `Hud` fields).
#[derive(Clone, Debug, Default)]
pub struct HealthTracker {
    last_health: i32,
    display_health: i32,
    last_health_time: u64,
    blink_time: i64,
    last_damage_tick: Option<i64>,
}

/// The health state of one frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HealthFrame {
    pub current: i32,
    pub display: i32,
    pub blink: bool,
}

impl HealthTracker {
    /// Vanilla's `extractPlayerHealth` bookkeeping. The client cannot read the player's damage
    /// cooldown, so a drop starts the blink and a rise blinks only within 20 ticks of a drop.
    pub fn update(&mut self, current: i32, tick: i64, millis: u64) -> HealthFrame {
        let blink = self.blink_time > tick && (self.blink_time - tick) / 3 % 2 == 1;
        if current < self.last_health {
            self.last_health_time = millis;
            self.blink_time = tick + 20;
            self.last_damage_tick = Some(tick);
        } else if current > self.last_health
            && self
                .last_damage_tick
                .is_some_and(|t| tick - t < 20 && tick >= t)
        {
            self.last_health_time = millis;
            self.blink_time = tick + 10;
        }
        if millis.saturating_sub(self.last_health_time) > 1000 {
            self.display_health = current;
            self.last_health_time = millis;
        }
        self.last_health = current;
        HealthFrame {
            current,
            display: self.display_health,
            blink,
        }
    }
}

/// The kind of a heart sprite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeartStyle {
    Normal,
    Poisoned,
    Withered,
    Absorbing,
    /// This mod's colour for the second and later rows.
    Layer,
    Container,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeartSprite {
    pub style: HeartStyle,
    pub half: bool,
    pub blinking: bool,
}

/// The GUI sprite id of a heart.
pub fn heart_sprite_id(s: HeartSprite) -> String {
    let blink = if s.blinking { "_blinking" } else { "" };
    let half = if s.half { "half" } else { "full" };
    match s.style {
        HeartStyle::Container => format!("hud/heart/container{blink}"),
        HeartStyle::Layer => format!("overflowingbars:hud/heart/layer_{half}{blink}"),
        HeartStyle::Normal => format!("hud/heart/{half}{blink}"),
        HeartStyle::Poisoned => format!("hud/heart/poisoned_{half}{blink}"),
        HeartStyle::Withered => format!("hud/heart/withered_{half}{blink}"),
        HeartStyle::Absorbing => format!("hud/heart/absorbing_{half}{blink}"),
    }
}

/// The heart kind the player's effects give: poisoned over withered over normal.
pub fn base_heart_style(poisoned: bool, withered: bool) -> HeartStyle {
    if poisoned {
        HeartStyle::Poisoned
    } else if withered {
        HeartStyle::Withered
    } else {
        HeartStyle::Normal
    }
}

/// Which heart a slot draws: an effect's kind always wins; otherwise the layer colour goes to
/// layered rows (or to the first row with `inverse`).
pub fn heart_style(base: HeartStyle, absorbing: bool, layer: bool, inverse: bool) -> HeartStyle {
    if base != HeartStyle::Normal {
        base
    } else if layer {
        if absorbing || !inverse {
            HeartStyle::Layer
        } else {
            base
        }
    } else if absorbing {
        HeartStyle::Absorbing
    } else if inverse {
        HeartStyle::Layer
    } else {
        base
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeartInput {
    pub max_health: f32,
    pub frame: HealthFrame,
    pub absorption: i32,
    /// The heart index the regeneration wave is on, or -1.
    pub regen_index: i32,
    pub base: HeartStyle,
    pub inverse: bool,
    pub colorize_first_row: bool,
}

/// `Mth.ceil` for the heart maths.
fn ceil_i32(v: f64) -> i32 {
    if v.is_finite() {
        v.ceil().clamp(-1e9, 1e9) as i32
    } else {
        0
    }
}

/// The regeneration wave position of a tick, or -1 without the effect.
pub fn regen_index(has_regeneration: bool, tick: i64, max_health: f32) -> i32 {
    if !has_regeneration {
        return -1;
    }
    let period = i64::from(ceil_i32(f64::from(max_health.min(20.0)) + 5.0)).max(1);
    i32::try_from(tick.rem_euclid(period)).unwrap_or(-1)
}

/// The sprites of every heart in the stacked layout, from the left bottom corner `(x, y)`.
pub fn plan_hearts(input: &HeartInput, x: i32, y: i32, tick: i64) -> Vec<SpriteOp> {
    let HeartInput {
        max_health,
        frame,
        absorption,
        regen_index,
        base,
        inverse,
        colorize_first_row,
    } = *input;
    let (current, display, blink) = (frame.current, frame.display, frame.blink);
    let normal = 10.min(ceil_i32(f64::from(max_health) / 2.0));
    let max_abs_hearts = 20 - normal;
    let abs_hearts = max_abs_hearts.min(ceil_i32(f64::from(absorption) / 2.0));
    let mut random = JavaRandom::new(tick.wrapping_mul(312_871));
    let shaking = i64::from(current) + i64::from(absorption) <= 4;
    let mut ops = Vec::new();
    let mut blit = |style: HeartStyle, px: i32, py: i32, blinking: bool, half: bool| {
        ops.push(SpriteOp {
            sprite: heart_sprite_id(HeartSprite {
                style,
                half,
                blinking,
            }),
            x: px,
            y: py,
            w: 9,
            h: 9,
            region: None,
            color: WHITE,
        });
    };
    for heart in 0..normal + abs_hearts {
        let px = x + (heart % 10) * 8;
        let mut py = y - (heart / 10) * 10;
        if shaking {
            py += random.next_int_pow2(2);
        }
        if heart < normal && regen_index == heart {
            py -= 2;
        }
        blit(HeartStyle::Container, px, py, blink, false);
        if heart >= normal {
            let abs_slot = heart * 2 - normal * 2;
            if abs_slot < absorption {
                let max_abs = max_abs_hearts * 2;
                let half = abs_slot + 1 == absorption % max_abs;
                let layer = absorption > max_abs && abs_slot < (absorption - 1) % max_abs + 1;
                if half && layer {
                    blit(
                        heart_style(base, true, false, inverse),
                        px,
                        py,
                        false,
                        false,
                    );
                }
                blit(heart_style(base, true, layer, inverse), px, py, false, half);
            }
        }
        if blink && heart * 2 < 20.min(display) {
            let top = (display - 1) % 20 + 1;
            let half = heart * 2 + 1 == top;
            let layer = display > 20 && heart * 2 < top;
            if half && layer {
                blit(
                    heart_style(base, false, false, inverse),
                    px,
                    py,
                    true,
                    false,
                );
            }
            let colored = layer || (colorize_first_row && heart * 2 < top);
            blit(
                heart_style(base, false, colored, inverse),
                px,
                py,
                true,
                half,
            );
        }
        if heart * 2 < 20.min(current) {
            let top = (current - 1) % 20 + 1;
            let half = heart * 2 + 1 == top;
            let layer = current > 20 && heart * 2 < top;
            if half && layer {
                blit(
                    heart_style(base, false, false, inverse),
                    px,
                    py,
                    false,
                    false,
                );
            }
            let colored = layer || (colorize_first_row && heart * 2 < top);
            blit(
                heart_style(base, false, colored, inverse),
                px,
                py,
                false,
                half,
            );
        }
    }
    ops
}

/// What one armour point of a bar shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArmorCell {
    /// A whole point of the newest row.
    LayerFull,
    /// The newest row's half point over a whole point of the row below.
    Mixed,
    /// The newest row's half point (the first row, with colouring of it on).
    LayerHalf,
    Full,
    Half,
    Empty,
    /// Nothing: the point is empty and empty points are skipped.
    Skip,
}

/// The points of the newest row: 0 when the bar has a single uncoloured row.
pub fn last_row_points(value: i32, colorize_first_row: bool, match_vanilla: bool) -> i32 {
    if !match_vanilla && (colorize_first_row || value > 20) {
        (value - 1) % 20 + 1
    } else {
        0
    }
}

/// The icon of point `index` (0 to 9) of a bar of `value` points.
pub fn armor_cell(index: i32, value: i32, last_row: i32, skip_empty: bool) -> ArmorCell {
    let odd = index * 2 + 1;
    if odd < last_row {
        ArmorCell::LayerFull
    } else if odd == last_row {
        if value > 20 {
            ArmorCell::Mixed
        } else {
            ArmorCell::LayerHalf
        }
    } else if odd < value {
        ArmorCell::Full
    } else if odd == value {
        ArmorCell::Half
    } else if !skip_empty {
        ArmorCell::Empty
    } else {
        ArmorCell::Skip
    }
}

/// The two tints of an armour-like bar: the ordinary rows' and the newest row's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub old: u32,
    pub new: u32,
}

impl Palette {
    /// `inverse` gives the ordinary tint to the layers and the layer tint to the front row.
    pub fn new(base: u32, layer: u32, inverse: bool) -> Self {
        if inverse {
            Self {
                old: layer,
                new: base,
            }
        } else {
            Self {
                old: base,
                new: layer,
            }
        }
    }
}

const ARMOR_EMPTY: &str = "hud/armor_empty";
const ARMOR_FULL: &str = "hud/armor_full";

/// The sprites of an armour-like bar of `value` points. A bar growing from the left starts at
/// `x`; a `mirrored` one starts at `x` and grows to the left.
pub fn plan_armor(
    x: i32,
    y: i32,
    value: i32,
    palette: Palette,
    mirrored: bool,
    last_row: i32,
    skip_empty: bool,
) -> Vec<SpriteOp> {
    let mut ops = Vec::new();
    if value <= 0 {
        return ops;
    }
    let op = |px: i32, w: i32, region: Option<[i32; 4]>, sprite: &str, color: u32| SpriteOp {
        sprite: sprite.to_string(),
        x: px,
        y,
        w,
        h: 9,
        region,
        color,
    };
    for index in 0..10 {
        let px = if mirrored {
            x - index * 8 - 9
        } else {
            x + index * 8
        };
        let full = |color| op(px, 9, None, ARMOR_FULL, color);
        let empty = op(px, 9, None, ARMOR_EMPTY, WHITE);
        // The half a bar fills first: the left five columns, or the right five when mirrored.
        let half = |color| {
            if mirrored {
                op(px + 4, 5, Some([9, 9, 4, 0]), ARMOR_FULL, color)
            } else {
                op(px, 5, Some([9, 9, 0, 0]), ARMOR_FULL, color)
            }
        };
        match armor_cell(index, value, last_row, skip_empty) {
            ArmorCell::LayerFull => ops.push(full(palette.new)),
            ArmorCell::Mixed => {
                ops.push(full(palette.old));
                ops.push(half(palette.new));
            }
            ArmorCell::LayerHalf => {
                ops.push(empty);
                ops.push(half(palette.new));
            }
            ArmorCell::Full => ops.push(full(palette.old)),
            ArmorCell::Half => {
                ops.push(empty);
                ops.push(half(palette.old));
            }
            ArmorCell::Empty => ops.push(empty),
            ArmorCell::Skip => {}
        }
    }
    ops
}

/// The number a bar's row count shows, or None when none shows.
pub fn row_count_number(
    value: i32,
    max_row: i32,
    always: bool,
    full_rows_only: bool,
) -> Option<i32> {
    if value <= 0 || max_row <= 0 {
        return None;
    }
    let rows = value as f32 / max_row as f32;
    if !always && rows <= 1.0 {
        return None;
    }
    Some(if full_rows_only {
        rows.floor() as i32
    } else {
        rows.ceil() as i32
    })
}

/// The decimal digits of `n`, least significant first.
pub fn digits_lsb_first(mut n: i32) -> Vec<i32> {
    let mut out = Vec::new();
    while n > 0 {
        out.push(n % 10);
        n /= 10;
    }
    out
}

/// Each glyph is five rows of three bits, the top bit the left pixel.
const GLYPHS: [[u8; 5]; 11] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b010, 0b010, 0b010],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
    [0b000, 0b101, 0b010, 0b101, 0b000],
];
/// The index of the "x" glyph.
pub const GLYPH_X: usize = 10;

/// The filled rectangles `[x0, y0, x1, y1]` of glyph `glyph` with its top-left at `(x, y)`.
pub fn glyph_fills(glyph: usize, x: i32, y: i32) -> Vec<[i32; 4]> {
    let Some(rows) = GLYPHS.get(glyph) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (row, bits) in rows.iter().enumerate() {
        let py = y + row as i32;
        let mut col = 0;
        while col < 3 {
            if bits & (0b100 >> col) == 0 {
                col += 1;
                continue;
            }
            let start = col;
            while col < 3 && bits & (0b100 >> col) != 0 {
                col += 1;
            }
            out.push([x + start, py, x + col, py + 1]);
        }
    }
    out
}

/// A row count's glyphs: `(glyph, x, y)` for each, left to right, from the bar edge `(x, y)`
/// as the original lays them out. `left` puts the number to the left of the edge.
pub fn row_count_glyphs(
    number: i32,
    show_x: bool,
    left: bool,
    x: i32,
    y: i32,
) -> Vec<(usize, i32, i32)> {
    let digits = digits_lsb_first(number);
    let mut px = x;
    if left {
        px -= if show_x { 7 } else { 3 };
    } else {
        px += 4 * digits.len() as i32;
    }
    let mut out: Vec<(usize, i32, i32)> = digits
        .iter()
        .enumerate()
        .map(|(i, d)| (*d as usize, px - 4 * i as i32, y + 2))
        .collect();
    out.reverse();
    if show_x {
        out.push((GLYPH_X, px + 4, y + 2));
    }
    out
}

/// The 16 dye colours' texture-diffuse colours, in vanilla's order.
pub const DYE_COLORS: [u32; 16] = [
    0xF9FFFE, 0xF9801D, 0xC74EBD, 0x3AB3DA, 0xFED83D, 0x80C71F, 0xF38BAA, 0x474F52, 0x9D9D97,
    0x169C9C, 0x8932B8, 0x3C44AA, 0x835432, 0x5E7C16, 0xB02E26, 0x1D1D21,
];

/// The opaque ARGB of dye `index`, white when the index is out of range.
pub fn dye_argb(index: u32) -> u32 {
    0xFF00_0000
        | DYE_COLORS
            .get(index as usize)
            .copied()
            .unwrap_or(DYE_COLORS[0])
}

/// `ChatOffsetHelper.twoHealthRows`.
pub fn two_health_rows(max_health: f32, absorption: f32) -> bool {
    absorption > 0.0 && max_health + absorption > 20.0
}

/// How far chat lifts above the bars, in GUI pixels (`ChatOffsetHelper.getChatOffsetY`).
pub fn chat_offset(
    two_rows: bool,
    armor_row: bool,
    toughness_row: bool,
    toughness_left: bool,
) -> i32 {
    let mut rows = 0;
    if two_rows {
        rows += 1;
    }
    if armor_row {
        rows += 1;
    }
    if toughness_row && (toughness_left || rows == 0) {
        rows += 1;
    }
    rows * ROW_HEIGHT
}

/// What vanilla's own heart layer takes from the left stack: `(rows - 1) * rowHeight + 10`.
pub fn vanilla_health_height(max_health: f32, absorption: i32) -> i32 {
    let rows = ceil_i32((f64::from(max_health) + f64::from(absorption)) / 2.0 / 10.0).max(1);
    let row_height = (10 - (rows - 2)).max(3);
    (rows - 1) * row_height + 10
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(current: i32, absorption: i32) -> HeartInput {
        HeartInput {
            max_health: 20.0,
            frame: HealthFrame {
                current,
                display: current,
                blink: false,
            },
            absorption,
            regen_index: -1,
            base: HeartStyle::Normal,
            inverse: false,
            colorize_first_row: false,
        }
    }

    fn count(ops: &[SpriteOp], id: &str) -> usize {
        ops.iter().filter(|o| o.sprite == id).count()
    }

    #[test]
    fn java_random_matches_the_known_sequence() {
        // new Random(0).nextInt() runs -1155484576, -723955400, 1033096058, -1690734402,
        // -1557280266, so the top bit (nextBoolean, nextInt(2)) runs 1, 1, 0, 1, 1; nextInt(2) is the
        // same top bit.
        let mut r = JavaRandom::new(0);
        let got: Vec<i32> = (0..5).map(|_| r.next_int_pow2(2)).collect();
        assert_eq!(got, vec![1, 1, 0, 1, 1]);
    }

    #[test]
    fn full_health_draws_ten_containers_and_hearts() {
        let ops = plan_hearts(&input(20, 0), 0, 100, 0);
        assert_eq!(count(&ops, "hud/heart/container"), 10);
        assert_eq!(count(&ops, "hud/heart/full"), 10);
        assert_eq!(count(&ops, "hud/heart/half"), 0);
    }

    #[test]
    fn health_beyond_twenty_stacks_in_the_layer_colour() {
        // 25 health on a 20 max: 5 hearts are layered, the half one over a plain full heart.
        let mut i = input(25, 0);
        i.max_health = 25.0;
        let ops = plan_hearts(&i, 0, 0, 0);
        let layer_full = count(&ops, "overflowingbars:hud/heart/layer_full");
        let layer_half = count(&ops, "overflowingbars:hud/heart/layer_half");
        assert_eq!((layer_full, layer_half), (2, 1));
    }

    #[test]
    fn absorption_uses_the_second_row() {
        let ops = plan_hearts(&input(20, 6), 0, 100, 0);
        assert_eq!(count(&ops, "hud/heart/absorbing_full"), 3);
        assert!(ops.iter().any(|o| o.y == 90));
    }

    #[test]
    fn low_health_shake_stays_within_one_pixel() {
        let ops = plan_hearts(&input(3, 0), 0, 100, 7);
        assert!(ops.iter().all(|o| o.y == 100 || o.y == 101));
    }

    #[test]
    fn effect_hearts_override_layers_and_inverse() {
        assert_eq!(
            heart_style(HeartStyle::Poisoned, true, true, true),
            HeartStyle::Poisoned
        );
        assert_eq!(
            heart_style(HeartStyle::Normal, false, false, true),
            HeartStyle::Layer
        );
        assert_eq!(
            heart_style(HeartStyle::Normal, false, true, true),
            HeartStyle::Normal
        );
        assert_eq!(
            heart_style(HeartStyle::Normal, true, false, false),
            HeartStyle::Absorbing
        );
    }

    #[test]
    fn heart_sprite_ids_name_the_vanilla_files() {
        let id = |style, half, blinking| {
            heart_sprite_id(HeartSprite {
                style,
                half,
                blinking,
            })
        };
        assert_eq!(
            id(HeartStyle::Normal, true, true),
            "hud/heart/half_blinking"
        );
        assert_eq!(
            id(HeartStyle::Container, false, false),
            "hud/heart/container"
        );
        assert_eq!(
            id(HeartStyle::Layer, false, false),
            "overflowingbars:hud/heart/layer_full"
        );
    }

    #[test]
    fn regeneration_wave_wraps() {
        assert_eq!(regen_index(false, 5, 20.0), -1);
        assert_eq!(regen_index(true, 25, 20.0), 0);
        assert_eq!(regen_index(true, 3, 20.0), 3);
    }

    #[test]
    fn tracker_blinks_after_a_hit_and_never_panics_on_clock_jumps() {
        let mut t = HealthTracker::default();
        t.update(20, 0, 5000);
        let f = t.update(14, 1, 5050);
        assert!(!f.blink);
        assert_eq!(f.display, 20);
        // 20 ticks of blink time, in steps of three ticks.
        assert!(t.update(14, 4, 5100).blink);
        // A clock that runs backwards is not a panic.
        t.update(14, 5, 1);
        // A rise long after the hit does not blink.
        let mut t = HealthTracker::default();
        t.update(10, 0, 5000);
        t.update(11, 100, 6000);
        assert!(!t.update(11, 101, 6050).blink);
    }

    #[test]
    fn armor_cells_follow_the_original_ladder() {
        assert_eq!(armor_cell(0, 5, 0, true), ArmorCell::Full);
        assert_eq!(armor_cell(2, 5, 0, true), ArmorCell::Half);
        assert_eq!(armor_cell(3, 5, 0, true), ArmorCell::Skip);
        assert_eq!(armor_cell(3, 5, 0, false), ArmorCell::Empty);
        // 25 points: last row has 5, so cell 2 is the mixed one, 0-1 the layer.
        let last = last_row_points(25, false, false);
        assert_eq!(last, 5);
        assert_eq!(armor_cell(1, 25, last, true), ArmorCell::LayerFull);
        assert_eq!(armor_cell(2, 25, last, true), ArmorCell::Mixed);
        assert_eq!(armor_cell(3, 25, last, true), ArmorCell::Full);
        assert_eq!(last_row_points(25, false, true), 0);
        assert_eq!(last_row_points(5, false, false), 0);
        assert_eq!(last_row_points(5, true, false), 5);
    }

    #[test]
    fn inverse_palette_swaps_the_tints() {
        let p = Palette::new(WHITE, ARMOR_LAYER_TINT, true);
        assert_eq!((p.old, p.new), (ARMOR_LAYER_TINT, WHITE));
    }

    #[test]
    fn mirrored_armor_grows_leftwards_and_halves_sit_on_the_right() {
        let p = Palette::new(WHITE, ARMOR_LAYER_TINT, false);
        let ops = plan_armor(100, 0, 3, p, true, 0, true);
        // One full point at x-9 and a half point at x-17 whose crop is the right side.
        assert_eq!(ops[0].x, 91);
        let half = ops.last().unwrap();
        assert_eq!(half.region, Some([9, 9, 4, 0]));
        assert_eq!(half.x, 100 - 8 - 9 + 4);
        assert!(plan_armor(0, 0, 0, p, false, 0, true).is_empty());
    }

    #[test]
    fn row_count_rules() {
        assert_eq!(row_count_number(20, 20, false, false), None);
        assert_eq!(row_count_number(20, 20, true, false), Some(1));
        assert_eq!(row_count_number(45, 20, false, false), Some(3));
        assert_eq!(row_count_number(45, 20, false, true), Some(2));
        assert_eq!(row_count_number(0, 20, true, false), None);
        assert_eq!(row_count_number(5, 0, true, false), None);
    }

    #[test]
    fn digits_and_glyph_layout() {
        assert_eq!(digits_lsb_first(120), vec![0, 2, 1]);
        assert!(digits_lsb_first(0).is_empty());
        let g = row_count_glyphs(12, true, true, 10, 0);
        // Left of the edge by 7, digits right to left four apart, "x" after.
        assert_eq!(g[0], (1, 10 - 7 - 4, 2));
        assert_eq!(g[1], (2, 10 - 7, 2));
        assert_eq!(g[2], (GLYPH_X, 10 - 7 + 4, 2));
        let r = row_count_glyphs(3, false, false, 10, 0);
        assert_eq!(r, vec![(3, 14, 2)]);
    }

    #[test]
    fn glyph_fills_cover_the_bitmap() {
        let eight: i32 = glyph_fills(8, 0, 0)
            .iter()
            .map(|r| (r[2] - r[0]) * (r[3] - r[1]))
            .sum();
        assert_eq!(eight, 13);
        assert!(glyph_fills(99, 0, 0).is_empty());
    }

    #[test]
    fn dye_colours_never_panic() {
        assert_eq!(dye_argb(0), 0xFFF9_FFFE);
        assert_eq!(dye_argb(14), 0xFFB0_2E26);
        assert_eq!(dye_argb(9999), dye_argb(0));
    }

    #[test]
    fn chat_offset_counts_rows() {
        assert_eq!(chat_offset(false, false, false, false), 0);
        assert_eq!(chat_offset(true, true, false, false), 20);
        // A right-side toughness row lifts chat only when nothing else does.
        assert_eq!(chat_offset(false, false, true, false), 10);
        assert_eq!(chat_offset(true, false, true, false), 10);
        assert_eq!(chat_offset(true, true, true, true), 30);
        assert!(two_health_rows(20.0, 1.0));
        assert!(!two_health_rows(20.0, 0.0));
        assert!(!two_health_rows(10.0, 5.0));
    }

    #[test]
    fn vanilla_height_matches_one_and_two_rows() {
        assert_eq!(vanilla_health_height(20.0, 0), 10);
        assert_eq!(vanilla_health_height(20.0, 2), 20);
        assert_eq!(vanilla_health_height(f32::NAN, 0), 10);
    }
}
