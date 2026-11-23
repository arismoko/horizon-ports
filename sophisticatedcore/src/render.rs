//! The storage blocks' own drawing (StorageRenderer, LimitedBarrelRenderer, DisplayItemRenderer): the paint of a
//! decorated barrel, and on a limited barrel's front the items it holds, their counts, its upgrades and its fill levels.
//!
//! The server sends what the client shows (`update_tag`): the two colours, the display flags, and a limited barrel's
//! slots and upgrades as the storage item keeps them. The client reads that in `submit` and draws it in the frame of
//! the barrel's front: its face is the north face, the origin its lower corner (`front`).
use mod_api::HostBlockStates;
use mod_api::{
    BlockEntityDrawContext, BlockState, ClientHost, Compound, CubeFace, Host, ItemDisplayContext,
    ItemStack, ModBlockEntity, Tag, Text,
};

use crate::contents::{
    INVENTORY, Nbt, UPGRADES, custom_data, get_bool, get_int, get_long, put, read_items,
};
use crate::places;
use crate::storage;
use crate::storage_items::{self, Family};
use crate::upgrades;

const MAIN: &str = "mainColor";
const ACCENT: &str = "accentColor";
const SLOTS: &str = "slots";
/// The controller a storage is linked to, as a packed position (`i64::MIN` for none).
pub const LINKED_TO: &str = "linkedTo";
const UPGRADE_ITEMS: &str = "upgrades";
/// `DyeColor.WHITE.getTextColor()`, the count's colour before any slot colour.
const COUNT_COLOR: u32 = 0xF9FFFE;
const SINGLE_FONT: f32 = 1.0 / 48.0;
const MULTIPLE_FONT: f32 = 1.0 / 96.0;
const SMALL_ITEM_SCALE: f32 = 0.5;
const BIG_FLAT_SCALE: f32 = 0.5;
const SMALL_FLAT_SCALE: f32 = 0.25;
const UPGRADE_SCALE: f32 = 0.125;
/// A limited barrel has at most four slots.
const LIMITED_MAX_SLOTS: usize = 4;

/// What the server tells clients of the storage in `entity`'s slot 0 (`getUpdateTag`): the colours, the display
/// flags, the upgrades, and the slots of a stack that holds few enough to be a limited barrel's.
pub fn update_tag(entity: &ModBlockEntity) -> Option<Compound> {
    let stack = entity.items.first()?.as_ref()?;
    let data = custom_data(stack);
    let mut tag = Compound::default();
    // Every field is always sent: the client's copy only replaces the fields it is told, so one that went away
    // must say so.
    for k in [MAIN, ACCENT] {
        put(&mut tag, k, Tag::Int(get_int(&data, k).unwrap_or(-1)));
    }
    put(
        &mut tag,
        LINKED_TO,
        Tag::Long(get_long(&data, "controllerPos").unwrap_or(i64::MIN)),
    );
    put(
        &mut tag,
        storage::PACKED,
        Tag::Byte(i8::from(get_bool(&data, storage::PACKED) == Some(true))),
    );
    let empty = Tag::List(Vec::new());
    put(
        &mut tag,
        UPGRADE_ITEMS,
        data.at(UPGRADES).cloned().unwrap_or_else(|| empty.clone()),
    );
    let slots = match data.at(INVENTORY) {
        Some(Tag::List(list)) if list.len() > LIMITED_MAX_SLOTS => empty,
        Some(inv) => inv.clone(),
        None => empty,
    };
    put(&mut tag, SLOTS, slots);
    put(
        &mut tag,
        "double",
        Tag::Byte(i8::from(
            get_bool(&data, crate::contents::DOUBLE) == Some(true),
        )),
    );
    for (k, default) in [
        ("locked", false),
        ("showLock", true),
        ("showTier", true),
        ("showCounts", true),
        ("showUpgrades", false),
        ("showFillLevels", false),
    ] {
        let on = get_bool(&data, k).unwrap_or(default);
        put(&mut tag, k, Tag::Byte(i8::from(on)));
    }
    Some(tag)
}

/// How a block's own `facing` turns the model: x, then y, in the blockstate's degrees (clockwise).
pub fn model_turn(facing: &str) -> (f32, f32) {
    match facing {
        "down" => (180.0, 0.0),
        "north" => (90.0, 0.0),
        "south" => (90.0, 180.0),
        "west" => (90.0, 270.0),
        "east" => (90.0, 90.0),
        _ => (0.0, 0.0),
    }
}

/// `DisplayItemRenderer.getNorthBasedRotation`: the turn from the north face to `dir`'s, as (axis, degrees).
pub fn north_based(dir: &str) -> Option<([f32; 3], f32)> {
    match dir {
        "down" => Some(([1.0, 0.0, 0.0], -90.0)),
        "up" => Some(([1.0, 0.0, 0.0], 90.0)),
        "south" => Some(([0.0, 1.0, 0.0], 180.0)),
        "west" => Some(([0.0, 1.0, 0.0], 90.0)),
        "east" => Some(([0.0, 1.0, 0.0], -90.0)),
        _ => None,
    }
}

pub fn opposite(dir: &str) -> &'static str {
    match dir {
        "down" => "up",
        "up" => "down",
        "north" => "south",
        "south" => "north",
        "west" => "east",
        _ => "west",
    }
}

/// `getDisplayItemIndexFrontOffset(index, count)`: where slot `index` of `count` sits on the front, (x, y) in blocks
/// from the lower corner of the face as the north face is seen from outside's mirror (x grows to the east).
pub fn front_offset(index: usize, count: usize) -> (f32, f32) {
    let c = 0.5f32;
    let half = c / 2.0;
    let top = index == 0;
    match count {
        1 => (0.5, c),
        2 => (0.5, if top { c + half } else { half }),
        3 => {
            let x = if index > 0 {
                0.75 - (index - 1) as f32 * 0.5
            } else {
                0.5
            };
            (x, if top { c + half } else { half })
        }
        4 => (
            if index == 0 || index == 2 {
                c + half
            } else {
                half
            },
            if index <= 1 { c + half } else { half },
        ),
        _ => (0.0, 0.5),
    }
}

/// `CountAbbreviator.abbreviate(count, max_chars)`: the count with thousands separators, or cut to k, m or b.
pub fn abbreviate(count: u32, max_chars: u32) -> String {
    if count == 0 {
        return "0".into();
    }
    let digits = count.ilog10() + 1;
    if digits <= max_chars {
        return grouped(u64::from(count));
    }
    let exp = (digits - max_chars) / 3 + 1;
    let suffix = ["k", "m", "b"][(exp as usize - 1).min(2)];
    let div = 1000u64.pow(exp);
    let whole = digits as i32 - exp as i32 * 3;
    let precision = max_chars as i32 - 1 - whole;
    let number = if whole > 3 || precision == 0 {
        grouped(u64::from(count) / div)
    } else if precision == 2 {
        let v = u64::from(count) * 100 / div;
        format!("{}.{:02}", v / 100, v % 100)
    } else if precision == 1 {
        let v = u64::from(count) * 10 / div;
        format!("{}.{}", v / 10, v % 10)
    } else {
        String::new()
    };
    number + suffix
}

fn grouped(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Where a slot's fill bar goes for `slots` slots: (x, y, large), in 16ths of the front.
fn bar_place(index: usize, slots: usize) -> Option<(f32, f32, bool)> {
    let s = 1.0 / 16.0;
    Some(match (slots, index) {
        (1, 0) => (s, s, true),
        (2, 0) => (s, 9.0 / 16.0, false),
        (2, 1) => (s, s, false),
        (3, 0) => (s, 9.0 / 16.0, false),
        (3, 1) => (14.0 / 16.0, s, false),
        (3, 2) => (s, s, false),
        (4, 0) => (14.0 / 16.0, 9.0 / 16.0, false),
        (4, 1) => (s, 9.0 / 16.0, false),
        (4, 2) => (14.0 / 16.0, s, false),
        (4, 3) => (s, s, false),
        _ => return None,
    })
}

/// The quad of a fill bar at `fill` (0..=1): its corners as seen from the front, and its texture rectangle in
/// texels of 16 per side.
pub fn bar_quad(fill: f32, x: f32, y: f32, large: bool) -> ([[f32; 3]; 4], [f32; 4]) {
    let unit = 1.0 / 80.0;
    let bar = if large { 14.0 } else { 6.0 };
    let (x0, y0) = (x + unit, y + unit);
    let (x1, y1) = (x0 + 3.0 * unit, y0 + fill * (bar * 5.0 - 2.0) * unit);
    let min_u = if large { 0.0 } else { 3.0 / 128.0 };
    let max_v = if large { 68.0 / 128.0 } else { 28.0 / 128.0 };
    (
        [[x1, y1, 0.0], [x1, y0, 0.0], [x0, y0, 0.0], [x0, y1, 0.0]],
        [
            min_u * 16.0,
            (1.0 - fill) * max_v * 16.0,
            (min_u + 3.0 / 128.0) * 16.0,
            max_v * 16.0,
        ],
    )
}

/// The fraction of a slot's room taken: `count` of `limit`.
pub fn fill_ratio(count: u32, limit: u32) -> f32 {
    if limit == 0 {
        return 0.0;
    }
    (count as f32 / limit as f32).clamp(0.0, 1.0)
}

const BARREL_SPRITES: [&str; 3] = ["side", "top", "bottom"];

fn sprite(part: &str, layer: &str) -> String {
    format!("sophisticatedstorage:block/barrel_{part}_tintable_{layer}")
}

/// The sprites (side, top, bottom) of a barrel's base layer: a limited barrel's top shows its slots, an open barrel's its
/// lid ajar.
pub fn barrel_base_sprites(wood: &str, open: bool, limited_slots: Option<usize>) -> [String; 3] {
    let top = match (limited_slots, open) {
        (Some(n), _) => format!("limited_{wood}_barrel_{n}_top"),
        (None, true) => format!("{wood}_barrel_top_open"),
        (None, false) => format!("{wood}_barrel_top"),
    };
    [
        format!("sophisticatedstorage:block/{wood}_barrel_side"),
        format!("sophisticatedstorage:block/{top}"),
        format!("sophisticatedstorage:block/{wood}_barrel_bottom"),
    ]
}

/// What is drawn over a barrel's model, in the model's own frame.
struct BarrelLook {
    /// The base layer again, covering a hidden tier: (side, top, bottom).
    cover: Option<[String; 3]>,
    main: Option<i32>,
    accent: Option<i32>,
    /// The lock mark on the top face's edge (`barrel_part/locked`).
    lock: bool,
}

/// The paint, tier cover and lock over a barrel, drawn in the block's own frame turned as its model is.
fn paint(draw: &mut dyn BlockEntityDrawContext, facing: &str, look: &BarrelLook) {
    let (x, y) = model_turn(facing);
    draw.push();
    draw.rotate_about([0.0, 1.0, 0.0], -y, [0.5, 0.5, 0.5]);
    draw.rotate_about([1.0, 0.0, 0.0], -x, [0.5, 0.5, 0.5]);
    if let Some(sprites) = &look.cover {
        barrel_shape(draw, sprites, 0.01, 0xFF_FFFF);
    }
    for (layer, color, grow) in [("main", look.main, 0.02), ("accent", look.accent, 0.04)] {
        let Some(c) = color else {
            continue;
        };
        let sprites = BARREL_SPRITES.map(|p| sprite(p, layer));
        barrel_shape(draw, &sprites, grow, c as u32 & 0xFF_FFFF);
    }
    if look.lock {
        let (x0, x1, z0, z1, y) = (7.5 / 16.0, 8.5 / 16.0, 15.0 / 16.0, 1.0, 16.1 / 16.0);
        draw.quad(
            "sophisticatedstorage:block/lock",
            [[x0, y, z0], [x0, y, z1], [x1, y, z1], [x1, y, z0]],
            [0.0, 0.0, 16.0, 16.0],
            0xFF_FFFF,
        );
    }
    draw.pop();
}

/// One element of the barrel template: its corners in sixteenths, and per face (down, up, north, south, west, east)
/// which sprite (0 side, 1 top, 2 bottom) and uv it shows; the rim's top-trim is the top sprite.
type Element = ([f32; 3], [f32; 3], [Option<(usize, [f32; 4])>; 6]);

/// `block/template/barrel_0`: the core up to 15 sixteenths and a one-sixteenth rim round the top.
const BARREL_ELEMENTS: [Element; 5] = [
    (
        [0.0, 0.0, 0.0],
        [16.0, 15.0, 16.0],
        [
            Some((2, [0.0, 0.0, 16.0, 16.0])),
            Some((1, [0.0, 0.0, 16.0, 16.0])),
            Some((0, [0.0, 1.0, 16.0, 16.0])),
            Some((0, [0.0, 1.0, 16.0, 16.0])),
            Some((0, [0.0, 1.0, 16.0, 16.0])),
            Some((0, [0.0, 1.0, 16.0, 16.0])),
        ],
    ),
    (
        [0.0, 15.0, 0.0],
        [16.0, 16.0, 1.0],
        [
            None,
            Some((1, [0.0, 0.0, 16.0, 1.0])),
            Some((0, [0.0, 0.0, 16.0, 1.0])),
            Some((1, [0.0, 1.0, 16.0, 2.0])),
            Some((0, [0.0, 0.0, 1.0, 1.0])),
            Some((0, [15.0, 0.0, 16.0, 1.0])),
        ],
    ),
    (
        [0.0, 15.0, 15.0],
        [16.0, 16.0, 16.0],
        [
            None,
            Some((1, [0.0, 15.0, 16.0, 16.0])),
            Some((1, [0.0, 14.0, 16.0, 15.0])),
            Some((0, [0.0, 0.0, 16.0, 1.0])),
            Some((0, [15.0, 0.0, 16.0, 1.0])),
            Some((0, [0.0, 0.0, 1.0, 1.0])),
        ],
    ),
    (
        [15.0, 15.0, 1.0],
        [16.0, 16.0, 15.0],
        [
            None,
            Some((1, [15.0, 1.0, 16.0, 15.0])),
            None,
            None,
            Some((1, [14.0, 1.0, 15.0, 15.0])),
            Some((0, [1.0, 0.0, 15.0, 1.0])),
        ],
    ),
    (
        [0.0, 15.0, 1.0],
        [1.0, 16.0, 15.0],
        [
            None,
            Some((1, [0.0, 1.0, 1.0, 15.0])),
            None,
            None,
            Some((0, [1.0, 0.0, 15.0, 1.0])),
            Some((1, [1.0, 1.0, 2.0, 15.0])),
        ],
    ),
];

/// A coordinate in sixteenths with the block's outer faces (0 and 16) pushed out by `grow`.
fn grown(v: f32, grow: f32) -> f32 {
    if v <= 0.0 {
        -grow
    } else if v >= 16.0 {
        16.0 + grow
    } else {
        v
    }
}

/// The barrel's shape (core and rim) in `sprites` (side, top, bottom), every outer face pushed out by `grow`
/// sixteenths, times `tint`.
fn barrel_shape(
    draw: &mut dyn BlockEntityDrawContext,
    sprites: &[String; 3],
    grow: f32,
    tint: u32,
) {
    for (from, to, faces) in &BARREL_ELEMENTS {
        // A face on the block's outside moves out; the core's top (15) and the rim's inner faces stay.
        let (lo, hi) = (from.map(|v| grown(v, grow)), to.map(|v| grown(v, grow)));
        let faces =
            faces.map(|f| f.map(|(sprite, uv)| CubeFace::new(&sprites[sprite]).with_uv(uv)));
        draw.cube(lo, hi, faces, tint);
    }
}

/// A limited barrel's front: the frame of its face, so that x grows along the face and z goes into the block.
fn front(draw: &mut dyn BlockEntityDrawContext, facing: &str) {
    if let Some((axis, deg)) = north_based(facing) {
        draw.rotate_about(axis, deg, [0.5, 0.5, 0.5]);
    }
    draw.translate([0.0, 0.0, 1.0 / 16.0]);
}

/// `submit`: draws the storage at `pos` from what `update_tag` sent.
pub fn submit(
    host: &dyn ClientHost,
    state: BlockState,
    block: &str,
    entity: &ModBlockEntity,
    draw: &mut dyn BlockEntityDrawContext,
) {
    let nbt = &entity.nbt;
    let facing = state
        .property(&host.block_states(), "facing")
        .ok()
        .flatten()
        .unwrap_or("up")
        .to_string();
    let colors = |k: &str| get_int(nbt, k).filter(|v| *v >= 0);
    let (main, accent) = (colors(MAIN), colors(ACCENT));
    let Some(kind) = storage_items::by_block(block) else {
        return;
    };
    let barrel = matches!(kind.family, Family::Barrel | Family::LimitedBarrel);
    draw.level_light();
    if matches!(kind.family, Family::Chest | Family::ShulkerBox) {
        if get_bool(nbt, storage::PACKED) != Some(true) {
            chest_or_shulker(host, state, kind.family, &facing, nbt, (main, accent), draw);
        }
        return;
    }
    if barrel {
        let prop = |k: &str| {
            state
                .property(&host.block_states(), k)
                .ok()
                .flatten()
                .map(str::to_string)
        };
        let flag = |k: &str, d: bool| get_bool(nbt, k).unwrap_or(d);
        let held = held_tool(host);
        let show_tier = flag("showTier", true) || held == Some(crate::tool::Mode::TierDisplay);
        let wood = prop("wood");
        let limited = (kind.family == Family::LimitedBarrel).then(|| kind.slot_count());
        let cover = wood.filter(|_| !show_tier).map(|w| {
            let open = prop("open").as_deref() == Some("true");
            barrel_base_sprites(&w, open, limited)
        });
        let lock = flag("locked", false)
            && (flag("showLock", true)
                || matches!(
                    held,
                    Some(crate::tool::Mode::Lock | crate::tool::Mode::LockDisplay)
                ));
        let look = BarrelLook {
            cover,
            main,
            accent,
            lock,
        };
        if look.cover.is_some() || main.is_some() || accent.is_some() || look.lock {
            paint(draw, &facing, &look);
        }
    }
    if kind.family != Family::LimitedBarrel {
        return;
    }
    let host: &dyn Host = host;
    let slots = kind.slot_count();
    let items = read_items(host, nbt.at(SLOTS), slots);
    let upgrades = read_items(host, nbt.at(UPGRADE_ITEMS), kind.upgrade_count());
    let flag = |k: &str, d: bool| get_bool(nbt, k).unwrap_or(d);
    let mult = upgrades::multiplier(host, kind, &upgrades);

    // Slots with items, in front order.
    draw.push();
    front(draw, &facing);
    for (i, stack) in items.iter().enumerate() {
        let Some(stack) = stack else { continue };
        let (x, y) = front_offset(i, slots);
        let flat = host.item_block(stack.item()).is_none();
        let scale = match (slots == 1, flat) {
            (true, false) => 1.0,
            (true, true) => BIG_FLAT_SCALE,
            (false, false) => SMALL_ITEM_SCALE,
            (false, true) => SMALL_FLAT_SCALE,
        };
        draw.push();
        draw.translate([x, y, 0.0]);
        draw.scale([scale; 3]);
        draw.item(&one(stack), ItemDisplayContext::Fixed, None, i as i32);
        draw.pop();
    }
    if flag("showUpgrades", false) {
        for (i, up) in upgrades.iter().flatten().enumerate() {
            draw.push();
            draw.translate([
                1.0 - i as f32 * 2.0 / 16.0 - 1.0 / 16.0,
                1.0 / 16.0,
                -1.0 / 16.0,
            ]);
            draw.scale([UPGRADE_SCALE; 3]);
            draw.item(up, ItemDisplayContext::Fixed, None, i as i32);
            draw.pop();
        }
    }
    if flag("showFillLevels", false) {
        draw.translate([0.0, 0.0, -0.001]);
        for (i, stack) in items.iter().enumerate() {
            let Some(stack) = stack else { continue };
            let Some((x, y, large)) = bar_place(i, slots) else {
                continue;
            };
            let limit = upgrades::limit(host, stack, mult);
            let (corners, uv) = bar_quad(fill_ratio(stack.count(), limit), x, y, large);
            draw.quad(
                "sophisticatedstorage:block/fill_indicators",
                corners,
                uv,
                0xFFFFFF,
            );
        }
    }
    draw.pop();

    if flag("showCounts", true) {
        counts(draw, &facing, &items, slots);
    }
}

/// The stack in the local player's selected slot.
pub fn held_stack(host: &dyn ClientHost) -> Option<ItemStack> {
    let local = host.local_player()?;
    let host: &dyn Host = host;
    let (inv, _) = places::readable(host, local)?;
    inv.get(local.selected_slot(host)?).cloned().flatten()
}

/// The storage tool in the player's main hand, with its mode.
fn held_tool(host: &dyn ClientHost) -> Option<crate::tool::Mode> {
    let held = held_stack(host)?;
    (crate::contents::item_id(host, &held) == crate::tool::TOOL).then(|| crate::tool::mode(&held))
}

fn chest_or_shulker(
    host: &dyn ClientHost,
    state: BlockState,
    family: Family,
    facing: &str,
    nbt: &Compound,
    paint: (Option<i32>, Option<i32>),
    draw: &mut dyn BlockEntityDrawContext,
) {
    use crate::chest_render as cr;
    use crate::tool::Mode;
    let prop = |k: &str| {
        state
            .property(&host.block_states(), k)
            .ok()
            .flatten()
            .map(str::to_string)
    };
    let flag = |k: &str, d: bool| get_bool(nbt, k).unwrap_or(d);
    let held = held_tool(host);
    let join = cr::Join::parse(prop("type").as_deref().unwrap_or("single"));
    let (base, tint_main, tint_accent) = if family == Family::Chest {
        let wood = prop("wood").unwrap_or_else(|| "oak".into());
        cr::chest_sprites(join, &wood)
    } else {
        (
            cr::SHULKER_BASE.into(),
            cr::SHULKER_MAIN.into(),
            cr::SHULKER_ACCENT.into(),
        )
    };
    let look = cr::Look {
        main: paint.0,
        accent: paint.1,
        locked: flag("locked", false),
        show_lock: flag("showLock", true),
        show_tier: flag("showTier", true),
        tool_shows_lock: matches!(held, Some(Mode::Lock | Mode::LockDisplay)),
        tool_shows_tier: held == Some(Mode::TierDisplay),
        base: &base,
        tint_main: &tint_main,
        tint_accent: &tint_accent,
    };
    if family == Family::Chest {
        let main_half = join == cr::Join::Single || flag("double", false);
        cr::draw_chest(draw, facing, join, &look, main_half);
    } else {
        cr::draw_shulker(draw, facing, &look);
    }
}

/// The stack as one item, for drawing (the count is drawn as text).
fn one(stack: &ItemStack) -> ItemStack {
    let mut s = stack.clone();
    let _ = s.set_count(1);
    s
}

/// The network's debug view (ControllerRenderer): two lines of text on a face of a storage, `order` in the network and
/// its `slots`, in the frame of a barrel's count on that face. `at` is the storage's offset from the block drawn.
pub fn debug_text(
    draw: &mut dyn BlockEntityDrawContext,
    at: [f32; 3],
    face: &str,
    order: usize,
    slots: usize,
) {
    draw.push();
    draw.translate(at);
    if let Some((axis, deg)) = north_based(opposite(face)) {
        draw.rotate_about(axis, deg, [0.5, 0.5, 0.5]);
    }
    draw.translate([1.0, 0.0, 1.0]);
    for (i, line) in [format!("Order: {order}"), format!("Slots: {slots}")]
        .into_iter()
        .enumerate()
    {
        draw.push();
        draw.translate([-0.5, 0.65 - i as f32 * 0.15, 0.001 - 0.75 / 16.0]);
        draw.scale([MULTIPLE_FONT, -MULTIPLE_FONT, MULTIPLE_FONT]);
        let text = Text::literal(line);
        let width = draw.text_width(&text);
        let _ = draw.text(&text, [-width / 2.0, 0.0], COUNT_COLOR, false, None);
        draw.pop();
    }
    draw.pop();
}

fn counts(
    draw: &mut dyn BlockEntityDrawContext,
    facing: &str,
    items: &[Option<ItemStack>],
    slots: usize,
) {
    draw.push();
    if let Some((axis, deg)) = north_based(opposite(facing)) {
        draw.rotate_about(axis, deg, [0.5, 0.5, 0.5]);
    }
    draw.translate([1.0, 0.0, 1.0]);
    let y_offset = -(if slots == 1 { 0.25 } else { 0.11 });
    for (i, stack) in items.iter().enumerate() {
        let Some(stack) = stack else { continue };
        let (x, y) = front_offset(i, slots);
        let scale = if slots == 1 {
            SINGLE_FONT
        } else {
            MULTIPLE_FONT
        };
        draw.push();
        draw.translate([-x, y + y_offset, 0.001 - 0.75 / 16.0]);
        draw.scale([scale, -scale, scale]);
        let text = Text::literal(abbreviate(stack.count(), if slots == 1 { 6 } else { 5 }));
        let width = draw.text_width(&text);
        let _ = draw.text(&text, [-width / 2.0, 0.0], COUNT_COLOR, false, None);
        draw.pop();
    }
    draw.pop();
}

#[cfg(test)]
mod tests {
    #[test]
    fn growing_a_barrel_moves_only_the_outer_faces() {
        assert_eq!(grown(0.0, 0.5), -0.5);
        assert_eq!(grown(16.0, 0.5), 16.5);
        assert_eq!(grown(15.0, 0.5), 15.0);
        assert_eq!(grown(1.0, 0.5), 1.0);
    }

    #[test]
    fn the_barrel_has_a_core_to_fifteen_and_a_rim_to_sixteen() {
        let tops: Vec<f32> = BARREL_ELEMENTS.iter().map(|e| e.1[1]).collect();
        assert_eq!(tops, [15.0, 16.0, 16.0, 16.0, 16.0]);
        // The rim's four bars close the ring: their footprints add up to the 16 by 16 block minus its 14 by 14 middle.
        let area: f32 = BARREL_ELEMENTS[1..]
            .iter()
            .map(|e| (e.1[0] - e.0[0]) * (e.1[2] - e.0[2]))
            .sum();
        assert_eq!(area, 16.0 * 16.0 - 14.0 * 14.0);
    }

    use super::*;

    #[test]
    fn a_barrels_base_sprites_follow_its_wood_lid_and_slots() {
        let p = "sophisticatedstorage:block/";
        assert_eq!(
            barrel_base_sprites("oak", false, None),
            [
                format!("{p}oak_barrel_side"),
                format!("{p}oak_barrel_top"),
                format!("{p}oak_barrel_bottom")
            ]
        );
        assert_eq!(
            barrel_base_sprites("oak", true, None)[1],
            format!("{p}oak_barrel_top_open")
        );
        assert_eq!(
            barrel_base_sprites("spruce", true, Some(3))[1],
            format!("{p}limited_spruce_barrel_3_top")
        );
    }

    #[test]
    fn counts_abbreviate_like_the_original() {
        assert_eq!(abbreviate(7, 6), "7");
        assert_eq!(abbreviate(1234, 5), "1,234");
        assert_eq!(abbreviate(123_456, 6), "123,456");
        assert_eq!(abbreviate(1_234_567, 6), "1,234k");
        assert_eq!(abbreviate(12_345_678, 6), "12,345k");
        assert_eq!(abbreviate(123_456_789, 6), "123.45m");
        assert_eq!(abbreviate(123_456, 5), "123.4k");
        assert_eq!(abbreviate(1_234_567_890, 6), "1,234m");
    }

    #[test]
    fn slots_sit_where_the_original_puts_them() {
        assert_eq!(front_offset(0, 1), (0.5, 0.5));
        assert_eq!(front_offset(0, 2), (0.5, 0.75));
        assert_eq!(front_offset(1, 2), (0.5, 0.25));
        assert_eq!(front_offset(1, 3), (0.75, 0.25));
        assert_eq!(front_offset(2, 3), (0.25, 0.25));
        assert_eq!(front_offset(0, 4), (0.75, 0.75));
        assert_eq!(front_offset(1, 4), (0.25, 0.75));
        assert_eq!(front_offset(2, 4), (0.75, 0.25));
        assert_eq!(front_offset(3, 4), (0.25, 0.25));
    }

    #[test]
    fn a_fill_bar_grows_with_the_fill() {
        let (empty, _) = bar_quad(0.0, 1.0 / 16.0, 1.0 / 16.0, true);
        let (full, uv) = bar_quad(1.0, 1.0 / 16.0, 1.0 / 16.0, true);
        assert_eq!(empty[0][1], empty[2][1]);
        assert!((full[0][1] - full[2][1] - 0.85).abs() < 1e-6);
        assert_eq!(uv[1], 0.0);
        assert_eq!(fill_ratio(32, 64), 0.5);
        assert_eq!(fill_ratio(100, 64), 1.0);
        assert_eq!(fill_ratio(1, 0), 0.0);
    }

    #[test]
    fn a_facing_turns_the_model_as_its_blockstate_does() {
        assert_eq!(model_turn("up"), (0.0, 0.0));
        assert_eq!(model_turn("east"), (90.0, 90.0));
        assert_eq!(opposite("north"), "south");
        assert!(north_based("north").is_none());
    }
}
