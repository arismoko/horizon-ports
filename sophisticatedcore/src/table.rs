//! The decoration table (DecorationTableBlockEntity, DecorationHelper, DecorationTableMenu): dyes a backpack's main
//! and accent colours. The backpack, the red, green and blue dyes sit in the block entity's four slots; the chosen
//! colours and the dye parts left over from earlier work are its state. A colour is paid in dye parts, split over
//! the three dyes by its red, green and blue shares, and one dye item is worth 24 parts.
use std::collections::HashMap;

use mod_api::{
    AttachmentCodec, AttachmentType, BlockEntityPart, BlockEntityPartState, BlockEntityRef,
    BlockPart, Compound, Host, HostTransaction, ItemStack, LevelRef, ServerHost, Tag,
};

use crate::contents::{self, get_int, put};
use crate::crafting;
use crate::menu::{button, filler};
use crate::places;
use crate::storage_items::{self, Family};

/// The parts one dye item is worth (`DecorationHelper.BLOCK_TOTAL_PARTS`).
pub const DYE_PARTS: i32 = 24;
/// The parts a new main colour costs, and a new accent colour (`MAIN_COLOR_PARTS`, `ACCENT_COLOR_PARTS`).
pub const MAIN_PARTS: i32 = 18;
pub const ACCENT_PARTS: i32 = 6;
/// A backpack's colours when it has none of its own (the backpack dye recipe's defaults).
pub const MAIN_DEFAULT: i32 = 13_394_234;
pub const ACCENT_DEFAULT: i32 = 6_434_330;
/// "Unchanged": no colour chosen.
pub const UNSET: i32 = -1;

/// The block entity's slots: the backpack, then the red, green and blue dye.
pub const SLOTS: usize = 4;
const DYE_TAGS: [&str; 3] = ["c:dyes/red", "c:dyes/green", "c:dyes/blue"];
const DYE_ITEMS: [&str; 3] = [
    "minecraft:red_dye",
    "minecraft:green_dye",
    "minecraft:blue_dye",
];
const CHANNELS: [&str; 3] = ["Red", "Green", "Blue"];

/// The menu's cells: three rows over the player's inventory.
pub const ROWS: u8 = 3;
const OWN: usize = 27;
const CELL_STORAGE: usize = 0;
const CELL_RESULT: usize = 2;
const CELL_DYES: usize = 4;
const CELL_INFO: usize = 8;
const CELL_MAIN: usize = 9;
const CELL_ACCENT: usize = 18;

/// The table state: the chosen colours and the parts left in each dye.
pub static STATE: AttachmentType<Compound> = AttachmentType {
    namespace: "sophisticatedstorage",
    name: "decoration_table",
    default: Compound::new,
    codec: Some(AttachmentCodec {
        encode: |c| Ok(c.clone()),
        decode: |c| Ok(c.clone()),
    }),
    copy_on_death: false,
    sync: false,
};

/// The block entity part that keeps [`STATE`].
pub struct TableEntity;

#[mod_api::sunrise_part]
impl BlockPart for TableEntity {}

#[mod_api::sunrise_part]
impl BlockEntityPart for TableEntity {}

impl BlockEntityPartState for TableEntity {
    type Data = Compound;
    const DATA: &'static AttachmentType<Compound> = &STATE;
}

/// What a player has open: the table's place and its window.
#[derive(Clone, Debug)]
pub struct Session {
    pub window: i32,
    pub dim: String,
    pub pos: [i32; 3],
}

/// The table's chosen colours and leftover parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State {
    pub main: i32,
    pub accent: i32,
    /// Parts left in the red, green and blue dye.
    pub parts: [i32; 3],
}

impl Default for State {
    fn default() -> Self {
        Self {
            main: UNSET,
            accent: UNSET,
            parts: [0; 3],
        }
    }
}

impl State {
    pub fn read(c: &Compound) -> Self {
        let parts = |k| get_int(c, k).unwrap_or(0).max(0);
        Self {
            main: get_int(c, "mainColor").unwrap_or(UNSET),
            accent: get_int(c, "accentColor").unwrap_or(UNSET),
            parts: [parts("red"), parts("green"), parts("blue")],
        }
    }

    pub fn write(&self) -> Compound {
        let mut c = Compound::new();
        put(&mut c, "mainColor", Tag::Int(self.main));
        put(&mut c, "accentColor", Tag::Int(self.accent));
        for (k, v) in ["red", "green", "blue"].into_iter().zip(self.parts) {
            put(&mut c, k, Tag::Int(v));
        }
        c
    }
}

/// DecorationHelper.calculateRGBPartsNeeded: `total` parts split over red, green and blue by the colour's shares,
/// the whole parts first and the rest to the largest remainders.
pub fn rgb_parts(color: i32, total: i32) -> [i32; 3] {
    let channel = |shift: u32| ((color >> shift) & 255) as f32 / 255.0;
    let mut ratios = [channel(16), channel(8), channel(0)];
    let sum = ratios[0] + ratios[1] + ratios[2];
    for r in &mut ratios {
        *r /= sum;
    }
    let mut result = [0i32; 3];
    let mut remainders = [0f64; 3];
    for i in 0..3 {
        // A black colour has no shares (NaN): nothing is whole and the parts go round the three evenly.
        let scaled = f64::from(ratios[i]) * f64::from(total);
        result[i] = if scaled.is_nan() { 0 } else { scaled as i32 };
        remainders[i] = if scaled.is_nan() {
            0.0
        } else {
            scaled - f64::from(result[i])
        };
    }
    let left = total - result.iter().sum::<i32>();
    let mut order = [0usize, 1, 2];
    order.sort_by(|&a, &b| remainders[b].total_cmp(&remainders[a]));
    for i in 0..left.max(0) as usize {
        result[order[i % 3]] += 1;
    }
    result
}

/// DecorationHelper.getDyePartsNeeded: the parts of each dye a change of `set` (main, accent) costs over what the
/// backpack `have`s. A colour left unchosen or already there costs nothing.
pub fn parts_needed(set: (i32, i32), have: (i32, i32)) -> [i32; 3] {
    let mut need = [0; 3];
    for (set, have, parts) in [(set.0, have.0, MAIN_PARTS), (set.1, have.1, ACCENT_PARTS)] {
        if set != UNSET && set != have {
            for (n, p) in need.iter_mut().zip(rgb_parts(set, parts)) {
                *n += p;
            }
        }
    }
    need
}

/// What paying `need` takes: the parts left in each dye afterwards and the dye items used. None when the dyes in
/// the slots (`have`, with the parts already left over) fall short.
pub fn spend(need: [i32; 3], left: [i32; 3], have: [u32; 3]) -> Option<([i32; 3], [u32; 3])> {
    let mut parts = left;
    let mut used = [0u32; 3];
    for c in 0..3 {
        if need[c] <= left[c] {
            parts[c] = left[c] - need[c];
            continue;
        }
        let short = need[c] - left[c];
        let items = ((short + DYE_PARTS - 1) / DYE_PARTS) as u32;
        if items > have[c] {
            return None;
        }
        used[c] = items;
        parts[c] = items as i32 * DYE_PARTS - short;
    }
    Some((parts, used))
}

/// The dye channel (0 red, 1 green, 2 blue) an item is a dye of.
pub fn dye_channel(host: &dyn Host, stack: &ItemStack) -> Option<usize> {
    let tags = host.tags().item_tags(stack.item()).unwrap_or_default();
    let id = contents::item_id(host, stack);
    (0..3).find(|&c| tags.iter().any(|t| t.to_string() == DYE_TAGS[c]) || id == DYE_ITEMS[c])
}

/// Whether `stack` is a backpack, which the table decorates.
pub fn is_backpack(host: &dyn Host, stack: &ItemStack) -> bool {
    storage_items::by_item(&contents::item_id(host, stack))
        .is_some_and(|k| k.family == Family::Backpack)
}

/// The table's contents, read from its slots and state.
#[derive(Clone, Debug)]
pub struct Table {
    pub slots: [Option<ItemStack>; SLOTS],
    pub state: State,
}

impl Table {
    pub fn read(host: &dyn Host, be: BlockEntityRef) -> Self {
        let slots = std::array::from_fn(|i| be.item_slot(host, i as u32).ok().flatten());
        let state =
            TableEntity::state(host, be).map_or_else(|_| State::default(), |c| State::read(&c));
        Self { slots, state }
    }

    fn dye_counts(&self) -> [u32; 3] {
        std::array::from_fn(|c| self.slots[1 + c].as_ref().map_or(0, ItemStack::count))
    }

    /// The backpack's colours now.
    pub fn current(&self) -> Option<(i32, i32)> {
        let st = self.slots[0].as_ref()?;
        Some(crafting::colors(st, MAIN_DEFAULT, ACCENT_DEFAULT))
    }

    /// The colours the result gets: the chosen ones, else the backpack's own.
    fn target(&self) -> Option<(i32, i32)> {
        let (m, a) = self.current()?;
        Some((
            if self.state.main == UNSET {
                m
            } else {
                self.state.main
            },
            if self.state.accent == UNSET {
                a
            } else {
                self.state.accent
            },
        ))
    }

    /// The parts the chosen colours cost.
    pub fn need(&self) -> [i32; 3] {
        self.current().map_or([0; 3], |cur| {
            parts_needed((self.state.main, self.state.accent), cur)
        })
    }

    /// Whether the dyes in the slots and the parts left over cover the cost.
    pub fn enough(&self) -> bool {
        spend(self.need(), self.state.parts, self.dye_counts()).is_some()
    }

    /// The decorated backpack, if the chosen colours change it (`colorsTransparentOrSameAs`).
    pub fn result(&self, host: &dyn Host) -> Option<ItemStack> {
        let st = self.slots[0].as_ref().filter(|s| is_backpack(host, s))?;
        let cur = self.current()?;
        let target = self.target()?;
        if target == cur {
            return None;
        }
        let mut out = contents::with_count(st, 1)?;
        crafting::set_colors(&mut out, target.0, target.1);
        Some(out)
    }
}

/// The table's block entity at `pos`.
pub fn entity(
    host: &dyn ServerHost,
    dim: &str,
    pos: [i32; 3],
) -> Option<(LevelRef, BlockEntityRef)> {
    let level = host.level(dim)?;
    Some((level, host.block_entity(level, pos)?))
}

fn write_slots(
    host: &mut dyn ServerHost,
    be: BlockEntityRef,
    set: &[(usize, Option<ItemStack>)],
) -> bool {
    let ok = host
        .transaction(|host, tx| {
            for (slot, stack) in set {
                be.set_item_slot(host, tx, *slot as u32, stack.clone())
                    .map_err(|_| ())?;
            }
            Ok::<(), ()>(())
        })
        .is_ok();
    if ok {
        let _ = be.set_changed(host);
    }
    ok
}

fn edit_state(host: &mut dyn ServerHost, be: BlockEntityRef, f: impl FnOnce(&mut State)) {
    let _ = TableEntity::edit_state(host, be, |_, c| {
        let mut s = State::read(c);
        f(&mut s);
        *c = s.write();
    });
    let _ = be.set_changed(host);
}

/// The cells of the menu for the table `be`.
pub fn render(host: &mut dyn ServerHost, be: BlockEntityRef) -> Vec<Option<ItemStack>> {
    let t = Table::read(&*host, be);
    cells(host, &t)
}

fn cells(host: &mut dyn ServerHost, t: &Table) -> Vec<Option<ItemStack>> {
    let mut out: Vec<Option<ItemStack>> = vec![filler(&*host); OWN];
    let hint = |what: &str, id: &str| {
        button(
            &*host,
            id,
            what,
            &["Click an item in your inventory to put it here"],
        )
    };
    out[CELL_STORAGE] = t.slots[0]
        .clone()
        .or_else(|| hint("Backpack", "minecraft:white_stained_glass_pane"));
    out[CELL_STORAGE + 1] = button(&*host, "minecraft:arrow", "Decorate", &[]);
    let result = t.result(&*host);
    out[CELL_RESULT] = match &result {
        Some(r) if t.enough() => Some(r.clone()),
        Some(_) => button(
            &*host,
            "minecraft:barrier",
            "Not enough dye",
            &["Put the missing dye in its slot"],
        ),
        None => button(
            &*host,
            "minecraft:gray_stained_glass_pane",
            "Nothing to decorate",
            &[],
        ),
    };
    let ids = [
        "minecraft:red_stained_glass_pane",
        "minecraft:green_stained_glass_pane",
        "minecraft:blue_stained_glass_pane",
    ];
    for c in 0..3 {
        out[CELL_DYES + c] = t.slots[1 + c]
            .clone()
            .or_else(|| hint(&format!("{} dye", CHANNELS[c]), ids[c]));
    }
    let need = t.need();
    let lines: Vec<String> = (0..3)
        .map(|c| {
            format!(
                "{}: {} parts needed, {} left over",
                CHANNELS[c], need[c], t.state.parts[c]
            )
        })
        .collect();
    let lore: Vec<&str> = lines
        .iter()
        .map(String::as_str)
        .chain(["A dye is worth 24 parts"])
        .collect();
    out[CELL_INFO] = button(&*host, "minecraft:paper", "Dye needed", &lore);
    for (row, (name, set, have)) in [
        ("Main colour", t.state.main, t.current().map(|c| c.0)),
        ("Accent colour", t.state.accent, t.current().map(|c| c.1)),
    ]
    .into_iter()
    .enumerate()
    {
        let base = if row == 0 { CELL_MAIN } else { CELL_ACCENT };
        let shown = if set == UNSET { have } else { Some(set) };
        let label = match (set, shown) {
            (UNSET, Some(c)) => format!("{name}: unchanged (#{:06X})", c & 0xFF_FFFF),
            (UNSET, None) => format!("{name}: unchanged"),
            (c, _) => format!("{name}: #{:06X}", c & 0xFF_FFFF),
        };
        out[base] = button(
            &*host,
            nearest_dye(shown.unwrap_or(0x80_8080)),
            &label,
            &["Click: leave it unchanged"],
        );
        for c in 0..3 {
            let both = [
                (format!("{} down", CHANNELS[c]), "minecraft:red_dye"),
                (format!("{} up", CHANNELS[c]), "minecraft:lime_dye"),
            ];
            for (k, (n, id)) in both.iter().enumerate() {
                let value = shown.map_or(0, |s| channel_of(s, c));
                let line = format!("Now {value}");
                out[base + 1 + c * 2 + k] = button(
                    &*host,
                    id,
                    n,
                    &[&line, "Click: 16 steps", "Right-click: 1 step"],
                );
            }
        }
    }
    out
}

/// The dye item whose colour is nearest `rgb`, for the colour's swatch.
fn nearest_dye(rgb: i32) -> &'static str {
    crafting::DYES
        .iter()
        .min_by_key(|(_, c)| {
            (0..3)
                .map(|ch| {
                    let d = channel_of(*c as i32, ch) - channel_of(rgb, ch);
                    d * d
                })
                .sum::<i32>()
        })
        .map_or("minecraft:white_dye", |d| d.0)
}

/// Red (0), green (1) or blue (2) of `rgb`.
pub fn channel_of(rgb: i32, channel: usize) -> i32 {
    (rgb >> (16 - 8 * channel)) & 255
}

/// `rgb` with one channel moved by `delta` (kept within 0 to 255).
pub fn stepped(rgb: i32, channel: usize, delta: i32) -> i32 {
    let shift = 16 - 8 * channel;
    let v = (channel_of(rgb, channel) + delta).clamp(0, 255);
    (rgb & !(255 << shift) & 0xFF_FFFF) | (v << shift)
}

/// A click on the menu's cell `slot`: what the table did, and a message for the player.
pub fn click(
    host: &mut dyn ServerHost,
    uuid: u128,
    s: &Session,
    slot: i32,
    right: bool,
) -> Option<&'static str> {
    let player = host.player(uuid)?;
    let (_, be) = entity(&*host, &s.dim, s.pos)?;
    let t = Table::read(&*host, be);
    let Ok(slot) = usize::try_from(slot) else {
        return None;
    };
    if slot >= OWN {
        // The player's inventory: the stack goes where it belongs.
        let rel = slot - OWN;
        let inv = match rel {
            0..27 => rel as u32 + 9,
            27..36 => rel as u32 - 27,
            _ => return None,
        };
        let st = places::inventory(host, player)[inv as usize].clone()?;
        let target = if is_backpack(&*host, &st) {
            0
        } else if let Some(c) = dye_channel(&*host, &st) {
            1 + c
        } else {
            return Some("The table takes a backpack and red, green and blue dye");
        };
        let mut cell = t.slots[target].clone();
        let rest = match &mut cell {
            None => {
                cell = Some(st.clone());
                None
            }
            Some(have) if target > 0 && contents::same(have, &st) => {
                let max = host.max_stack_size(st.item()).unwrap_or(64).max(1);
                let moved = max.saturating_sub(have.count()).min(st.count());
                *have = contents::with_count(have, have.count() + moved)?;
                (moved < st.count())
                    .then(|| contents::with_count(&st, st.count() - moved))
                    .flatten()
            }
            Some(_) => return Some("That slot is taken"),
        };
        if rest.as_ref().is_some_and(|r| r.count() == st.count()) {
            return Some("That slot is full");
        }
        places::set_slot(host, player, inv, rest);
        write_slots(host, be, &[(target, cell)]);
        return None;
    }
    match slot {
        CELL_STORAGE | 4..=6 => {
            let at = if slot == CELL_STORAGE {
                0
            } else {
                slot - CELL_DYES + 1
            };
            if let Some(st) = t.slots[at].clone() {
                places::give_or_drop(host, player, &st);
                write_slots(host, be, &[(at, None)]);
            }
        }
        CELL_RESULT => {
            let result = t.result(&*host)?;
            let Some((parts, used)) = spend(t.need(), t.state.parts, t.dye_counts()) else {
                return Some("Not enough dye");
            };
            let dyes: Vec<(usize, Option<ItemStack>)> = (0..3)
                .map(|c| {
                    let left = t.slots[1 + c]
                        .as_ref()
                        .and_then(|d| contents::with_count(d, d.count() - used[c]));
                    (1 + c, left)
                })
                .chain([(0, None)])
                .collect();
            places::give_or_drop(host, player, &result);
            write_slots(host, be, &dyes);
            edit_state(host, be, |st| st.parts = parts);
        }
        CELL_MAIN | CELL_ACCENT => {
            edit_state(host, be, |st| {
                if slot == CELL_MAIN {
                    st.main = UNSET;
                } else {
                    st.accent = UNSET;
                }
            });
        }
        _ => {
            let (row, k) = if (CELL_MAIN + 1..CELL_MAIN + 7).contains(&slot) {
                (0, slot - CELL_MAIN - 1)
            } else if (CELL_ACCENT + 1..CELL_ACCENT + 7).contains(&slot) {
                (1, slot - CELL_ACCENT - 1)
            } else {
                return None;
            };
            let (channel, up) = (k / 2, k % 2 == 1);
            let step = if right { 1 } else { 16 };
            let delta = if up { step } else { -step };
            let from = t.current().map(|c| if row == 0 { c.0 } else { c.1 });
            edit_state(host, be, |st| {
                let chosen = if row == 0 {
                    &mut st.main
                } else {
                    &mut st.accent
                };
                let base = if *chosen == UNSET {
                    from.unwrap_or(0x80_8080)
                } else {
                    *chosen
                };
                *chosen = stepped(base & 0xFF_FFFF, channel, delta);
            });
        }
    }
    None
}

/// The sessions open on tables, by player.
pub type Sessions = std::sync::Mutex<HashMap<u128, Session>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colour_splits_its_parts_by_its_shares() {
        // Pure red is all red; an even grey is thirds.
        assert_eq!(rgb_parts(0xFF0000, 18), [18, 0, 0]);
        assert_eq!(rgb_parts(0x808080, 18), [6, 6, 6]);
        // 255:128:0 of 24 parts is 15.98, 8.02 and 0: 15 and 8 whole, the last part to the larger remainder.
        assert_eq!(rgb_parts(0xFF8000, 24), [16, 8, 0]);
    }

    #[test]
    fn black_splits_evenly() {
        assert_eq!(rgb_parts(0, 18), [6, 6, 6]);
        assert_eq!(rgb_parts(0, 6), [2, 2, 2]);
    }

    #[test]
    fn only_a_new_colour_costs_parts() {
        let have = (0xAA0000, 0x00AA00);
        assert_eq!(parts_needed((UNSET, UNSET), have), [0; 3]);
        assert_eq!(parts_needed((0xAA0000, UNSET), have), [0; 3]);
        assert_eq!(parts_needed((0x0000FF, UNSET), have), [0, 0, 18]);
        assert_eq!(parts_needed((UNSET, 0xFF0000), have), [6, 0, 0]);
        assert_eq!(parts_needed((0x0000FF, 0xFF0000), have), [6, 0, 18]);
    }

    #[test]
    fn spending_uses_leftover_parts_before_new_dye() {
        // Needing 18 with 5 left over: 13 more is one dye item (24), leaving 11.
        assert_eq!(
            spend([18, 0, 0], [5, 0, 0], [1, 0, 0]),
            Some(([11, 0, 0], [1, 0, 0]))
        );
        // Covered by the leftover alone: no dye used.
        assert_eq!(
            spend([18, 0, 0], [20, 0, 0], [0, 0, 0]),
            Some(([2, 0, 0], [0; 3]))
        );
        // Exactly the leftover: nothing left.
        assert_eq!(
            spend([6, 0, 0], [6, 0, 0], [0, 0, 0]),
            Some(([0; 3], [0; 3]))
        );
    }

    #[test]
    fn spending_needs_enough_dye() {
        assert_eq!(spend([0, 25, 0], [0; 3], [0, 1, 0]), None);
        assert_eq!(
            spend([0, 25, 0], [0; 3], [0, 2, 0]),
            Some(([0, 23, 0], [0, 2, 0]))
        );
        // Nothing needed, nothing in the slots: fine.
        assert_eq!(spend([0; 3], [0; 3], [0; 3]), Some(([0; 3], [0; 3])));
    }

    #[test]
    fn a_channel_steps_within_its_range() {
        assert_eq!(stepped(0x102030, 0, 16), 0x202030);
        assert_eq!(stepped(0x102030, 1, -1), 0x101F30);
        assert_eq!(stepped(0x102030, 2, 1000), 0x1020FF);
        assert_eq!(stepped(0x102030, 0, -1000), 0x002030);
    }

    #[test]
    fn the_state_survives_its_compound() {
        let s = State {
            main: 0x123456,
            accent: UNSET,
            parts: [1, 2, 3],
        };
        assert_eq!(State::read(&s.write()), s);
        assert_eq!(State::read(&Compound::new()), State::default());
    }
}
