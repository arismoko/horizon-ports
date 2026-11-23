//! The hopper upgrades' view (their upgrade-tab screen): the six sides' modes (the advanced upgrade), and an input
//! and an output filter, each with its type (allow list, block list, or what the inventory already holds) and a
//! button that opens the filter itself (`filter_view`: its items, what it matches on). Clicking an item in the
//! inventory adds it to the input filter, right-clicking to the output filter.
//!
//! Controls are numbered from 0 and laid out from the top row; past the first page row they continue in the
//! control row's free cells, as in the fluid views. Eleven fit even the smallest storage.

use mod_api::{Host, ItemStack, PlayerRef, ServerHost};

use crate::contents::{self, Contents};
use crate::filter::{Filter, Primary};
use crate::fluid_view::{cell, control};
use crate::hopper::{self, INPUT, Io, Mode, OUTPUT, SIDES};
use crate::menu::{Outcome, Session, View, button, filler, inventory_slot};
use crate::places::{self, Loc};
use crate::upgrades::{self, Kind};

/// Where each control group starts, in control numbers.
#[derive(Debug, PartialEq, Eq)]
pub struct Layout {
    pub sides: Option<usize>,
    pub in_type: usize,
    pub in_open: usize,
    pub out_type: usize,
    pub out_open: usize,
}

/// The advanced upgrade has the sides; both have each filter's type and its opener.
pub fn layout(advanced: bool) -> Layout {
    let first = if advanced { 6 } else { 0 };
    Layout {
        sides: advanced.then_some(0),
        in_type: first,
        in_open: first + 1,
        out_type: first + 2,
        out_open: first + 3,
    }
}

/// The storage's facing for a session on a placed block.
fn session_facing(host: &dyn ServerHost, s: &Session) -> String {
    let Loc::Block { dim, pos } = &s.loc else {
        return "north".to_owned();
    };
    host.level(dim)
        .map(|l| hopper::facing(host, l, *pos))
        .unwrap_or_else(|| "north".to_owned())
}

fn io_button(host: &dyn Host, side: &str, dir: &str, io: Io) -> Option<ItemStack> {
    let (id, what) = match io {
        Io::Push => ("minecraft:orange_dye", "Pushes out"),
        Io::Pull => ("minecraft:light_blue_dye", "Pulls in"),
        Io::PushPull => ("minecraft:lime_dye", "Pushes out and pulls in"),
        Io::Off => ("minecraft:gray_dye", "Off"),
    };
    button(
        host,
        id,
        &format!("{side}: {what}"),
        &[&format!("The inventory to the {dir}"), "Click: next mode"],
    )
}

pub(crate) fn type_button(
    host: &dyn Host,
    f: &Filter,
    what: &str,
    input: bool,
) -> Option<ItemStack> {
    match hopper::mode(f) {
        Mode::Allow => button(
            host,
            "minecraft:white_wool",
            &format!("Allow list ({what})"),
            &["Only listed items match", "Click: switch to a block list"],
        ),
        Mode::Block => button(
            host,
            "minecraft:black_wool",
            &format!("Block list ({what})"),
            &[
                "Listed items don't match; an empty list matches everything",
                "Click: match what the inventory already holds",
            ],
        ),
        Mode::Storage => button(
            host,
            "minecraft:chest",
            &format!("Contents ({what})"),
            &[
                if input {
                    "Only items this storage already holds match"
                } else {
                    "Only items the target inventory already holds match"
                },
                "Click: switch to an allow list",
            ],
        ),
    }
}

fn open_button(host: &dyn Host, f: &Filter, input: bool) -> Option<ItemStack> {
    let what = if input { "input" } else { "output" };
    let shown = match f.primary {
        Primary::Item => "by item",
        Primary::Mod => "by mod",
        Primary::Tags => "by tags",
    };
    button(
        host,
        "minecraft:writable_book",
        &format!("Edit the {what} filter"),
        &[&format!("Matches {shown}"), "Click: items, match and tags"],
    )
}

pub fn render(
    host: &mut dyn ServerHost,
    s: &Session,
    c: &Contents,
    j: usize,
    out: &mut [Option<ItemStack>],
) {
    let fill = filler(&*host);
    let base = s.control();
    let Some(up) = c.upgrades.get(j).cloned().flatten() else {
        return;
    };
    let Some(u) = upgrades::of(&*host, &up) else {
        return;
    };
    for cell in out.iter_mut().take(base + 9) {
        *cell = fill.clone();
    }
    let (input, output) = hopper::filters(&*host, &up, u.advanced);
    let l = layout(u.advanced);
    let mut put = |n: usize, st: Option<ItemStack>| {
        let at = cell(base, n);
        if at < base + 8 {
            out[at] = st;
        }
    };
    if let Some(first) = l.sides {
        let facing = session_facing(&*host, s);
        let dirs = hopper::read_dirs(&up, u.advanced, &facing);
        for (i, side) in SIDES.into_iter().enumerate() {
            let dir = hopper::side_dir(side, &facing);
            put(
                first + i,
                io_button(&*host, side.label(), dir, dirs.io(dir)),
            );
        }
    }
    put(l.in_type, type_button(&*host, &input, "input", true));
    put(l.in_open, open_button(&*host, &input, true));
    put(l.out_type, type_button(&*host, &output, "output", false));
    put(l.out_open, open_button(&*host, &output, false));
    out[base] = button(&*host, "minecraft:arrow", "Back", &[]);
    out[base + 8] = Some(up);
}

pub fn click(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    s: &Session,
    c: &Contents,
    slot: i32,
    j: usize,
    right: bool,
) -> (Outcome, Option<&'static str>) {
    let mut c = c.clone();
    let base = s.control();
    let Some(mut up) = c.upgrades.get(j).cloned().flatten() else {
        return (Outcome::View(View::Main { page: 0 }), None);
    };
    let Some(u) = upgrades::of(&*host, &up).filter(|u| u.kind == Kind::Hopper) else {
        return (Outcome::View(View::Main { page: 0 }), None);
    };
    let (mut input, mut output) = hopper::filters(&*host, &up, u.advanced);
    if let Some(inv) = inventory_slot(s, slot) {
        let invs = places::inventory(host, player);
        let Some(st) = invs[inv as usize].clone() else {
            return (Outcome::Nothing, None);
        };
        let (f, key) = if right {
            (&mut output, OUTPUT)
        } else {
            (&mut input, INPUT)
        };
        if f.items.iter().flatten().any(|x| x.same_item(&st)) {
            return (Outcome::Nothing, Some("Already in the filter"));
        }
        let Some(free) = f.items.iter().position(Option::is_none) else {
            return (Outcome::Nothing, Some("The filter is full"));
        };
        f.items[free] = contents::with_count(&st, 1);
        f.write(&*host, &mut up, key);
    } else {
        let Ok(raw) = usize::try_from(slot) else {
            return (Outcome::Nothing, None);
        };
        if raw == base {
            return (Outcome::View(View::Main { page: 0 }), None);
        }
        let Some(i) = control(base, raw) else {
            return (Outcome::Nothing, None);
        };
        let l = layout(u.advanced);
        if let Some(first) = l.sides.filter(|f| (*f..*f + 6).contains(&i)) {
            let facing = session_facing(&*host, s);
            let dir = hopper::side_dir(SIDES[i - first], &facing);
            let mut dirs = hopper::read_dirs(&up, u.advanced, &facing);
            let next = dirs.io(dir).next();
            dirs.set_io(dir, next);
            hopper::write_dirs(&mut up, &dirs);
        } else if i == l.in_type {
            hopper::cycle(&mut input);
            input.write(&*host, &mut up, INPUT);
        } else if i == l.out_type {
            hopper::cycle(&mut output);
            output.write(&*host, &mut up, OUTPUT);
        } else if i == l.in_open || i == l.out_open {
            let key = if i == l.in_open { INPUT } else { OUTPUT };
            return (
                Outcome::View(View::Filter {
                    slot: j,
                    key,
                    page: 0,
                }),
                None,
            );
        } else {
            return (Outcome::Nothing, None);
        }
    }
    c.upgrades[j] = Some(up);
    (Outcome::Changed(c), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_follow_one_another() {
        assert_eq!(
            layout(true),
            Layout {
                sides: Some(0),
                in_type: 6,
                in_open: 7,
                out_type: 8,
                out_open: 9,
            }
        );
        let b = layout(false);
        assert_eq!((b.sides, b.in_type, b.out_open), (None, 0, 3));
    }
}
