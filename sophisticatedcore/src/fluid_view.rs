//! The tank, battery, pump and XP pump upgrades' views (their upgrade-tab screens): the container slots, the
//! tank's and the battery's level, the pump's modes and fluid filter, the XP pump's levels and buttons.
//!
//! Controls are numbered from 0 and laid out from the top row; past the first page row they continue in the
//! control row's free cells (the ones between Back and the upgrade), so a one-row storage shows seven more.

use mod_api::{
    FluidResource, Host, HostTransaction, ItemPlace, ItemStack, PlayerRef, ResourceHandler,
    ServerHost,
};

use crate::contents::{self, Contents};
use crate::fluids::{self, Limits, Tank, XP_FLUID};
use crate::menu::{Outcome, Session, View, button, filler, inventory_slot};
use crate::places;
use crate::upgrades::{self, Kind};

/// The menu cell of control `n`: the cells below `base`, then those after the Back cell.
pub(crate) fn cell(base: usize, n: usize) -> usize {
    if n < base { n } else { n + 1 }
}

/// The control at menu cell `i`, none for Back, the upgrade cell and beyond.
pub(crate) fn control(base: usize, i: usize) -> Option<usize> {
    match i {
        _ if i < base => Some(i),
        _ if i == base || i > base + 7 => None,
        _ => Some(i - 1),
    }
}

/// The first of the pump's fluid filter controls.
const FILTERS: usize = 5;

/// The most a level target or a level count goes to.
const MAX_LEVEL: i32 = 100;

fn fluid_item(host: &dyn Host, id: &str, name: &str, lore: &[&str]) -> Option<ItemStack> {
    let bucket = id
        .split_once(':')
        .map(|(ns, path)| format!("{ns}:{path}_bucket"));
    let shown = bucket
        .filter(|b| crate::menu::item(host, b).is_some())
        .unwrap_or_else(|| "minecraft:bucket".to_owned());
    button(host, &shown, name, lore)
}

fn bar(amount: u32, cap: u32) -> String {
    format!("{amount} / {cap} mB")
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
    for c in out.iter_mut().take(base + 9) {
        *c = fill.clone();
    }
    let l = fluids::limits(&*host, s.kind, c);
    let put = |out: &mut [Option<ItemStack>], at: usize, st: Option<ItemStack>| {
        let at = cell(base, at);
        if at < base + 8 {
            out[at] = st;
        }
    };
    match u.kind {
        Kind::Tank | Kind::Battery => {
            let tank = u.kind == Kind::Tank;
            let slots = fluids::read_slots(&*host, &up);
            let names = if tank {
                ("Fill from container", "Fill container")
            } else {
                ("Charge from item", "Charge item")
            };
            let hints = if tank {
                [
                    "Click a container with fluid in your inventory to put it here",
                    "Click a container in your inventory to put it here to be filled",
                ]
            } else {
                [
                    "Click a battery in your inventory to put it here",
                    "Click a battery in your inventory to put it here to be charged",
                ]
            };
            for (i, name) in [names.0, names.1].into_iter().enumerate() {
                let st = slots[i].clone().or_else(|| {
                    button(
                        &*host,
                        if i == 0 {
                            "minecraft:white_stained_glass_pane"
                        } else {
                            "minecraft:gray_stained_glass_pane"
                        },
                        name,
                        &[hints[i]],
                    )
                });
                put(out, i, st);
            }
            let info = if tank {
                let t = fluids::read_tank(&up);
                match &t.fluid {
                    Some(f) => fluid_item(
                        &*host,
                        f,
                        f,
                        &[
                            &bar(t.amount, l.tank_cap),
                            &format!("Moves {} mB at a time", l.tank_io),
                        ],
                    ),
                    None => button(
                        &*host,
                        "minecraft:bucket",
                        "Empty",
                        &[
                            &bar(0, l.tank_cap),
                            &format!("Moves {} mB at a time", l.tank_io),
                        ],
                    ),
                }
            } else {
                let e = fluids::read_energy(&up);
                button(
                    &*host,
                    "minecraft:redstone",
                    "Energy",
                    &[
                        &format!("{e} / {} FE", l.battery_cap),
                        &format!("Moves {} FE at a time", l.battery_io),
                    ],
                )
            };
            put(out, 4, info);
        }
        Kind::Pump => {
            let set = fluids::read_pump(&up, u.advanced);
            put(
                out,
                0,
                if set.input {
                    button(
                        &*host,
                        "minecraft:hopper",
                        "Pumps in",
                        &["Click: pump out of the tank instead"],
                    )
                } else {
                    button(
                        &*host,
                        "minecraft:dropper",
                        "Pumps out",
                        &["Click: pump into the tank instead"],
                    )
                },
            );
            put(
                out,
                1,
                button(
                    &*host,
                    if set.hand {
                        "minecraft:lime_dye"
                    } else {
                        "minecraft:gray_dye"
                    },
                    if set.hand {
                        "Fills and empties held containers"
                    } else {
                        "Leaves held containers alone"
                    },
                    &["Click: switch"],
                ),
            );
            put(
                out,
                2,
                button(
                    &*host,
                    if set.world {
                        "minecraft:lime_dye"
                    } else {
                        "minecraft:gray_dye"
                    },
                    if set.world {
                        "Works on the world's fluids"
                    } else {
                        "Leaves the world alone"
                    },
                    &["Click: switch"],
                ),
            );
            put(
                out,
                4,
                match fluids::read_tank(&up_of_tank(&*host, c)) {
                    Tank {
                        fluid: Some(f),
                        amount,
                    } => fluid_item(&*host, &f, &f, &[&bar(amount, l.tank_cap)]),
                    _ => button(
                        &*host,
                        "minecraft:bucket",
                        "The tank is empty",
                        &["A pump needs a tank upgrade in the same storage"],
                    ),
                },
            );
            let filters =
                fluids::read_pump_filters(&*host, &up, crate::options::tune().liquids.pump_filters);
            for (i, f) in filters.iter().enumerate() {
                let st = match f {
                    Some(f) => fluid_item(&*host, f, f, &["Click: remove"]),
                    None => button(
                        &*host,
                        "minecraft:white_stained_glass_pane",
                        "Fluid filter",
                        &["Right-click a container with fluid in your inventory to add it"],
                    ),
                };
                put(out, FILTERS + i, st);
            }
        }
        Kind::XpPump => {
            let x = fluids::read_xp(&up);
            let names = [
                "Takes experience in",
                "Gives experience out",
                "Keeps the level",
                "Off",
            ];
            put(
                out,
                0,
                button(
                    &*host,
                    "minecraft:experience_bottle",
                    names[x.direction % 4],
                    &["Click: next mode"],
                ),
            );
            put(
                out,
                1,
                button(
                    &*host,
                    "minecraft:enchanted_book",
                    &format!("Level {}", x.level),
                    &["Left-click: up, right-click: down", "Shift: five at a time"],
                ),
            );
            put(
                out,
                2,
                button(
                    &*host,
                    if x.mend {
                        "minecraft:anvil"
                    } else {
                        "minecraft:barrier"
                    },
                    if x.mend {
                        "Mends your items"
                    } else {
                        "Doesn't mend"
                    },
                    &["Click: switch"],
                ),
            );
            let t = fluids::read_tank(&up_of_tank(&*host, c));
            let points = if t.fluid.as_deref() == Some(XP_FLUID) {
                t.amount / fluids::MB_PER_POINT
            } else {
                0
            };
            put(
                out,
                4,
                button(
                    &*host,
                    "minecraft:glow_ink_sac",
                    &format!("{points} points in the tank"),
                    &[&bar(t.amount, l.tank_cap)],
                ),
            );
            let rows = [
                (9, x.store, "Levels to store", "Store levels", "Store all"),
                (12, x.take, "Levels to give", "Take levels", "Take all"),
            ];
            for (at, v, count, one, all) in rows {
                put(
                    out,
                    at,
                    button(
                        &*host,
                        "minecraft:paper",
                        &format!("{count}: {v}"),
                        &["Left-click: up, right-click: down", "Shift: five at a time"],
                    ),
                );
                put(
                    out,
                    at + 1,
                    button(&*host, "minecraft:lime_dye", one, &["Click"]),
                );
                put(
                    out,
                    at + 2,
                    button(&*host, "minecraft:emerald", all, &["Click"]),
                );
            }
        }
        _ => {}
    }
    out[base] = button(&*host, "minecraft:arrow", "Back", &[]);
    out[base + 8] = Some(up);
}

/// The first tank upgrade's stack of a storage (an empty tank's when it has none).
fn up_of_tank(host: &dyn Host, c: &Contents) -> ItemStack {
    fluids::tank_index(host, c)
        .and_then(|i| c.upgrades[i].clone())
        .or_else(|| {
            c.upgrades.iter().flatten().next().cloned().map(|mut u| {
                fluids::write_tank(&mut u, &Tank::default());
                u
            })
        })
        .unwrap_or_else(|| crate::menu::item(host, "minecraft:stone").expect("stone exists"))
}

/// The fluid a container in `place` holds, if any.
fn held_fluid(host: &mut dyn ServerHost, st: &ItemStack) -> Option<String> {
    let mut found = None;
    let _ = host.scratch_stack(st.clone(), |host, _, place| {
        let cap = place.capability(host, &mod_api::ITEM_FLUIDS).ok_or(())?;
        for i in 0..cap.size(host) {
            if cap.amount(host, i) > 0
                && let Some(r) = cap.resource(host, i)
            {
                found = fluid_id(&*host, &r);
                break;
            }
        }
        Err::<(), ()>(())
    });
    found
}

fn fluid_id(host: &dyn Host, r: &FluidResource) -> Option<String> {
    r.to_stack(1)
        .key()
        .and_then(|k| k.id(host).map(str::to_owned))
}

/// Whether the player's slot `inv` holds an item with a fluid (or energy) capability.
fn has_capability(host: &mut dyn ServerHost, player: PlayerRef, inv: u32, energy: bool) -> bool {
    let place = ItemPlace::PlayerSlot {
        player: player.uuid(),
        slot: inv as i32,
    };
    if energy {
        place.capability(host, &mod_api::ITEM_ENERGY).is_some()
    } else {
        place.capability(host, &mod_api::ITEM_FLUIDS).is_some()
    }
}

fn step(v: i32, right: bool, shift: bool, min: i32) -> i32 {
    let d = if shift { 5 } else { 1 };
    (if right { v - d } else { v + d }).clamp(min, MAX_LEVEL)
}

#[allow(clippy::too_many_arguments)]
pub fn click(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    s: &Session,
    c: &Contents,
    slot: i32,
    j: usize,
    right: bool,
    shift: bool,
) -> (Outcome, Option<&'static str>) {
    let mut c = c.clone();
    let base = s.control();
    let Some(mut up) = c.upgrades.get(j).cloned().flatten() else {
        return (Outcome::View(View::Main { page: 0 }), None);
    };
    let Some(u) = upgrades::of(&*host, &up) else {
        return (Outcome::View(View::Main { page: 0 }), None);
    };
    if let Some(inv) = inventory_slot(s, slot) {
        let invs = places::inventory(host, player);
        let Some(st) = invs[inv as usize].clone() else {
            return (Outcome::Nothing, None);
        };
        match u.kind {
            Kind::Tank | Kind::Battery => {
                let energy = u.kind == Kind::Battery;
                let target = usize::from(right);
                // A battery's output slot takes any item, as the original's does.
                if !(energy && right) && !has_capability(host, player, inv, energy) {
                    return (
                        Outcome::Nothing,
                        Some(if energy {
                            "That item holds no energy"
                        } else {
                            "That item holds no fluid"
                        }),
                    );
                }
                let mut slots = fluids::read_slots(&*host, &up);
                if slots[target].is_some() {
                    return (Outcome::Nothing, Some("That slot is taken"));
                }
                slots[target] = contents::with_count(&st, 1);
                places::set_slot(host, player, inv, st.clone().shrink(1));
                fluids::write_slots(&*host, &mut up, &slots);
            }
            Kind::Pump if right => {
                let Some(f) = held_fluid(host, &st) else {
                    return (Outcome::Nothing, Some("That item holds no fluid"));
                };
                let mut filters = fluids::read_pump_filters(
                    &*host,
                    &up,
                    crate::options::tune().liquids.pump_filters,
                );
                if filters.iter().flatten().any(|x| *x == f) {
                    return (Outcome::Nothing, Some("Already in the filter"));
                }
                let Some(free) = filters.iter().position(Option::is_none) else {
                    return (Outcome::Nothing, Some("The filter is full"));
                };
                filters[free] = Some(f);
                fluids::write_pump_filters(&mut up, &filters);
            }
            _ => return (Outcome::Nothing, None),
        }
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
        match u.kind {
            Kind::Tank | Kind::Battery if i < 2 => {
                let mut slots = fluids::read_slots(&*host, &up);
                let Some(st) = slots[i].take() else {
                    return (Outcome::Nothing, None);
                };
                places::give_or_drop(host, player, &st);
                fluids::write_slots(&*host, &mut up, &slots);
            }
            Kind::Pump => {
                let mut set = fluids::read_pump(&up, u.advanced);
                let n = crate::options::tune().liquids.pump_filters;
                match i {
                    0 => set.input = !set.input,
                    1 => set.hand = !set.hand,
                    2 => set.world = !set.world,
                    FILTERS.. if i - FILTERS < n => {
                        let mut filters = fluids::read_pump_filters(&*host, &up, n);
                        if filters[i - FILTERS].take().is_none() {
                            return (Outcome::Nothing, None);
                        }
                        fluids::write_pump_filters(&mut up, &filters);
                        c.upgrades[j] = Some(up);
                        return (Outcome::Changed(c), None);
                    }
                    _ => return (Outcome::Nothing, None),
                }
                upgrades::set_flag(&mut up, "input", set.input);
                upgrades::set_flag(&mut up, "interactWithHand", set.hand);
                upgrades::set_flag(&mut up, "interactWithWorld", set.world);
            }
            Kind::XpPump => {
                let mut x = fluids::read_xp(&up);
                match i {
                    0 => x.direction = (x.direction + 1) % 4,
                    1 => x.level = step(x.level, right, shift, 0),
                    2 => x.mend = !x.mend,
                    9 => x.store = step(x.store, right, shift, 1),
                    12 => x.take = step(x.take, right, shift, 1),
                    10 | 11 | 13 | 14 => {
                        if let Some(msg) = xp_button(host, player, &mut c, s, &x, i) {
                            return (Outcome::Nothing, Some(msg));
                        }
                        return (Outcome::Changed(c), None);
                    }
                    _ => return (Outcome::Nothing, None),
                }
                fluids::write_xp(&mut up, &x);
            }
            _ => return (Outcome::Nothing, None),
        }
    }
    c.upgrades[j] = Some(up);
    (Outcome::Changed(c), None)
}

/// The XP pump's four buttons, which ignore the tank's rate (the original's `ignoreInOutLimit`). Returns what to
/// tell the player when nothing moved.
fn xp_button(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    c: &mut Contents,
    s: &Session,
    x: &fluids::Xp,
    cell: usize,
) -> Option<&'static str> {
    let Some(ti) = fluids::tank_index(&*host, c) else {
        return Some("This needs a tank upgrade");
    };
    let l: Limits = fluids::limits(&*host, s.kind, c);
    let mut up = c.upgrades[ti].clone()?;
    let mut tank = fluids::read_tank(&up);
    let e = player.experience(&*host)?;
    let moved = match cell {
        10 => fluids::xp_store(
            host,
            player,
            &mut tank,
            l.tank_cap,
            None,
            (e.level - x.store).max(0),
        ),
        11 => fluids::xp_store(host, player, &mut tank, l.tank_cap, None, 0),
        13 => fluids::xp_give(host, player, &mut tank, None, e.level + x.take),
        _ => fluids::xp_give(host, player, &mut tank, None, MAX_LEVEL_ALL),
    };
    if moved == 0 {
        return Some("No experience moved");
    }
    fluids::write_tank(&mut up, &tank);
    c.upgrades[ti] = Some(up);
    None
}

/// `ALL_LEVELS`: the level "give all" stops at.
const MAX_LEVEL_ALL: i32 = 10_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_skip_the_back_cell() {
        // A one-row storage: nine page cells, Back at 9, the upgrade at 17.
        assert_eq!(cell(9, 8), 8);
        assert_eq!(cell(9, 9), 10);
        assert_eq!(cell(9, 15), 16);
        for n in 0..16 {
            assert_eq!(control(9, cell(9, n)), Some(n));
        }
        assert_eq!(control(9, 9), None);
        assert_eq!(control(9, 17), None);
        // Bigger menus keep the same controls where they were.
        assert_eq!(cell(27, 14), 14);
    }
}
