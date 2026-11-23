//! The filter view of an upgrade (its filter screen): the listed items over pages, the allow or block list, what
//! they match on (item, mod or tags), whether durability and components count too, and the tag list. An upgrade
//! with several filters (hopper, automatic cooking) opens one view per filter, named by the `key` the filter is kept
//! under; the plain upgrades' filter has the empty key and the upgrade's own settings (void overflow, magnet
//! pickups) beside it.
//!
//! Controls in the control row, from the Back cell: tags or page, allow or block, primary match, the upgrade's own
//! two, durability, components (or any/all tags).

use mod_api::{Host, ItemStack, PlayerRef, ServerHost};

use crate::contents::{self, Contents};
use crate::filter::{Direction, Filter, Primary};
use crate::hopper;
use crate::menu::{Outcome, Session, View, button, filler, inventory_slot};
use crate::places;
use crate::upgrades::{self, Kind, Upgrade};

/// How many slots the filter under `key` has (the settings' counts).
pub fn count(u: &Upgrade, key: &str) -> usize {
    let t = crate::options::tune();
    match (u.kind, key) {
        (Kind::Hopper, hopper::INPUT) => t.hopper_filters[usize::from(u.advanced)][0],
        (Kind::Hopper, hopper::OUTPUT) => t.hopper_filters[usize::from(u.advanced)][1],
        (Kind::Cooking(k), "inputFilter") => t.cook_filters[usize::from(k.recipe)][0],
        (Kind::Cooking(k), "fuelFilter") => t.cook_filters[usize::from(k.recipe)][1],
        _ => u.filters,
    }
}

fn read(host: &dyn Host, up: &ItemStack, u: &Upgrade, key: &str) -> Filter {
    let default = key == "fuelFilter" || (key.is_empty() && u.allow_default);
    Filter::read(host, up, key, count(u, key), default)
}

/// The view `Back` returns to.
fn parent(u: &Upgrade, slot: usize) -> View {
    match u.kind {
        Kind::Cooking(_) => View::Cooking { slot },
        Kind::Hopper => View::Hopper { slot },
        _ => View::Main { page: 0 },
    }
}

/// The upgrades whose filters can match on what the storage holds (ContentsFilterLogic).
fn contents_kind(u: &Upgrade) -> bool {
    matches!(u.kind, Kind::Hopper | Kind::Filter)
}

/// Whether the upgrade has the list and match controls (a refill's list is only its items).
fn has_match(u: &Upgrade) -> bool {
    u.kind != Kind::Refill
}

fn list_button(host: &dyn Host, u: &Upgrade, f: &Filter) -> Option<ItemStack> {
    if contents_kind(u) {
        let what = if u.kind == Kind::Filter {
            "filter"
        } else {
            "input"
        };
        return crate::hopper_view::type_button(host, f, what, u.kind != Kind::Filter);
    }
    if f.allow {
        button(
            host,
            "minecraft:white_wool",
            "Allow list",
            &["Only listed items match", "Click: switch to a block list"],
        )
    } else {
        button(
            host,
            "minecraft:black_wool",
            "Block list",
            &[
                "Listed items don't match; an empty list matches everything",
                "Click: switch to an allow list",
            ],
        )
    }
}

fn primary_button(host: &dyn Host, f: &Filter) -> Option<ItemStack> {
    match f.primary {
        Primary::Item => button(
            host,
            "minecraft:paper",
            "Match by item",
            &["Click: match by mod"],
        ),
        Primary::Mod => button(
            host,
            "minecraft:book",
            "Match by mod",
            &["Click: match by tags"],
        ),
        Primary::Tags => button(
            host,
            "minecraft:name_tag",
            "Match by tags",
            &["Click: match by item"],
        ),
    }
}

fn toggle(host: &dyn Host, on: bool, what: &str, how: &str) -> Option<ItemStack> {
    button(
        host,
        if on {
            "minecraft:lime_dye"
        } else {
            "minecraft:gray_dye"
        },
        &format!("{what}: {}", if on { "on" } else { "off" }),
        &[how],
    )
}

/// The upgrade's own two buttons beside its filter (cells 4 and 5 of the control row).
fn own(host: &dyn Host, u: &Upgrade, up: &ItemStack) -> [Option<ItemStack>; 2] {
    match u.kind {
        Kind::Void => [
            if upgrades::void_overflow(u, up) {
                button(
                    host,
                    "minecraft:lava_bucket",
                    "Void overflow",
                    &[
                        "Matching items are voided only when they don't fit",
                        "Click: void every matching item",
                    ],
                )
            } else {
                button(
                    host,
                    "minecraft:bucket",
                    "Void all matching",
                    &[
                        "Matching items are voided as they are picked up",
                        "Click: void only what doesn't fit",
                    ],
                )
            },
            None,
        ],
        Kind::Magnet => [
            if upgrades::flag(up, "pickupItems", true) {
                button(
                    host,
                    "minecraft:hopper",
                    "Picks up items",
                    &["Click: don't pick up items"],
                )
            } else {
                button(
                    host,
                    "minecraft:barrier",
                    "Doesn't pick up items",
                    &["Click: pick up items"],
                )
            },
            if upgrades::flag(up, "pickupXp", true) {
                button(
                    host,
                    "minecraft:experience_bottle",
                    "Pulls experience",
                    &["Click: don't pull experience orbs"],
                )
            } else {
                button(
                    host,
                    "minecraft:glass_bottle",
                    "Doesn't pull experience",
                    &["Click: pull experience orbs"],
                )
            },
        ],
        Kind::Filter => [
            match Direction::read(up) {
                Direction::Both => button(
                    host,
                    "minecraft:comparator",
                    "Filters in and out",
                    &[
                        "Applies to what goes in and what comes out",
                        "Click: only what goes in",
                    ],
                ),
                Direction::Input => button(
                    host,
                    "minecraft:hopper",
                    "Filters what goes in",
                    &[
                        "Hoppers and pipes putting items in",
                        "Click: only what comes out",
                    ],
                ),
                Direction::Output => button(
                    host,
                    "minecraft:dropper",
                    "Filters what comes out",
                    &["Hoppers and pipes taking items out", "Click: in and out"],
                ),
            },
            None,
        ],
        Kind::ToolSwapper if u.advanced => [
            match crate::swapper::Mode::read(up) {
                crate::swapper::Mode::Any => button(
                    host,
                    "minecraft:iron_pickaxe",
                    "Swaps any tool",
                    &[
                        "Swaps in a better tool for what you mine",
                        "Click: only when holding a tool",
                    ],
                ),
                crate::swapper::Mode::OnlyTools => button(
                    host,
                    "minecraft:iron_axe",
                    "Swaps tools only",
                    &["Not when holding a sword", "Click: don't swap"],
                ),
                crate::swapper::Mode::NoSwap => button(
                    host,
                    "minecraft:barrier",
                    "Doesn't swap for blocks",
                    &["Click: swap any tool"],
                ),
            },
            toggle(
                host,
                crate::swapper::swaps_weapons(up),
                "Swaps weapons",
                "Click: swap in a sword or axe when attacking",
            ),
        ],
        _ => [None, None],
    }
}

fn pages(n: usize, per: usize) -> usize {
    n.div_ceil(per.max(1)).max(1)
}

fn page_button(host: &dyn Host, page: usize, of: usize) -> Option<ItemStack> {
    button(
        host,
        "minecraft:paper",
        &format!("Page {} of {}", page + 1, of),
        &["Click: next page", "Right-click: previous page"],
    )
}

fn flip(page: usize, of: usize, right: bool) -> usize {
    if right {
        (page + of - 1) % of
    } else {
        (page + 1) % of
    }
}

fn tag_item(host: &dyn Host, tag: &str, chosen: bool) -> Option<ItemStack> {
    if chosen {
        button(
            host,
            "minecraft:name_tag",
            tag,
            &["In the filter", "Click: remove it"],
        )
    } else {
        button(
            host,
            "minecraft:paper",
            tag,
            &["Click: add it to the filter"],
        )
    }
}

pub fn render(
    host: &mut dyn ServerHost,
    s: &Session,
    c: &Contents,
    v: &View,
    out: &mut [Option<ItemStack>],
) {
    let View::Filter { slot: j, key, page } = *v else {
        return;
    };
    let fill = filler(&*host);
    let base = s.control();
    let Some(up) = c.upgrades.get(j).cloned().flatten() else {
        return;
    };
    let Some(u) = upgrades::of(&*host, &up) else {
        return;
    };
    let f = read(&*host, &up, &u, key);
    for cell in out.iter_mut().take(base + 9) {
        *cell = fill.clone();
    }
    let by_tags = has_match(&u) && f.primary == Primary::Tags;
    let of = pages(f.items.len(), base);
    let page = page.min(of - 1);
    if by_tags {
        for (i, t) in f.tags.iter().take(base).enumerate() {
            out[i] = tag_item(&*host, t, true);
        }
    } else {
        for i in 0..base {
            let Some(x) = f.items.get(page * base + i) else {
                break;
            };
            out[i] = match x {
                Some(x) => Some(x.clone()),
                None => button(
                    &*host,
                    "minecraft:white_stained_glass_pane",
                    "Filter slot",
                    &["Click an item in your inventory to add it"],
                ),
            };
        }
    }
    out[base] = button(&*host, "minecraft:arrow", "Back", &[]);
    if has_match(&u) {
        out[base + 1] = if by_tags {
            button(
                &*host,
                "minecraft:writable_book",
                "Edit tags",
                &["Click, or click an item in your inventory, to pick the tags to match"],
            )
        } else if of > 1 {
            page_button(&*host, page, of)
        } else {
            None
        };
        out[base + 2] = list_button(&*host, &u, &f);
        out[base + 3] = primary_button(&*host, &f);
        if by_tags {
            out[base + 7] = toggle(
                &*host,
                f.any_tag,
                "Any tag",
                "Click: switch between needing any tag and every tag",
            );
        } else {
            out[base + 6] = toggle(
                &*host,
                f.durability,
                "Match durability",
                "Click: also compare the damage of the items",
            );
            out[base + 7] = toggle(
                &*host,
                f.components,
                "Match components",
                "Click: also compare the components of the items (enchantments, names, ...)",
            );
        }
    } else if of > 1 {
        out[base + 1] = page_button(&*host, page, of);
    }
    if key.is_empty() {
        let [a, b] = own(&*host, &u, &up);
        out[base + 4] = a;
        out[base + 5] = b;
    }
    out[base + 8] = Some(up);
}

pub fn click(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    s: &Session,
    c: &Contents,
    slot: i32,
    v: &View,
    right: bool,
) -> (Outcome, Option<&'static str>) {
    let View::Filter { slot: j, key, page } = *v else {
        return (Outcome::Nothing, None);
    };
    let mut c = c.clone();
    let base = s.control();
    let Some(mut up) = c.upgrades.get(j).cloned().flatten() else {
        return (Outcome::View(View::Main { page: 0 }), None);
    };
    let Some(u) = upgrades::of(&*host, &up) else {
        return (Outcome::View(View::Main { page: 0 }), None);
    };
    let mut f = read(&*host, &up, &u, key);
    let by_tags = has_match(&u) && f.primary == Primary::Tags;
    let of = pages(f.items.len(), base);
    let page = page.min(of - 1);
    let here = |page| View::Filter { slot: j, key, page };
    if let Some(inv) = inventory_slot(s, slot) {
        let invs = places::inventory(host, player);
        let Some(st) = invs[inv as usize].clone() else {
            return (Outcome::Nothing, None);
        };
        if by_tags {
            let pick = contents::with_count(&st, 1);
            return (
                Outcome::View(View::Tags {
                    slot: j,
                    key,
                    pick,
                    page: 0,
                }),
                None,
            );
        }
        if f.items.iter().flatten().any(|x| x.same_item(&st)) {
            return (Outcome::Nothing, Some("Already in the filter"));
        }
        let Some(free) = f.items.iter().position(Option::is_none) else {
            return (Outcome::Nothing, Some("The filter is full"));
        };
        f.items[free] = contents::with_count(&st, 1);
    } else {
        let Ok(i) = usize::try_from(slot) else {
            return (Outcome::Nothing, None);
        };
        if i < base {
            if by_tags {
                if i >= f.tags.len() {
                    return (Outcome::Nothing, None);
                }
                f.tags.remove(i);
            } else {
                let at = page * base + i;
                if f.items.get(at).cloned().flatten().is_none() {
                    return (Outcome::Nothing, None);
                }
                f.items[at] = None;
            }
        } else if i == base {
            return (Outcome::View(parent(&u, j)), None);
        } else if i == base + 1 && by_tags && has_match(&u) {
            return (
                Outcome::View(View::Tags {
                    slot: j,
                    key,
                    pick: None,
                    page: 0,
                }),
                None,
            );
        } else if i == base + 1 && !by_tags && of > 1 {
            return (Outcome::View(here(flip(page, of, right))), None);
        } else if i == base + 2 && has_match(&u) {
            if contents_kind(&u) {
                hopper::cycle(&mut f);
            } else {
                f.allow = !f.allow;
            }
        } else if i == base + 3 && has_match(&u) {
            f.primary = f.primary.next();
        } else if i == base + 6 && has_match(&u) && !by_tags {
            f.durability = !f.durability;
        } else if i == base + 7 && has_match(&u) {
            if by_tags {
                f.any_tag = !f.any_tag;
            } else {
                f.components = !f.components;
            }
        } else if i == base + 4 && key.is_empty() && u.kind == Kind::Void {
            // `setShouldVoidOverflow`: "void all" is refused when the setting turns it off.
            let v = !upgrades::void_overflow(&u, &up);
            if v || crate::options::tune().void_anything[usize::from(u.advanced)] {
                upgrades::set_flag(&mut up, "shouldVoidOverflow", v);
            }
            c.upgrades[j] = Some(up);
            return (Outcome::Changed(c), None);
        } else if i == base + 4 && key.is_empty() && u.kind == Kind::Filter {
            Direction::read(&up).next().write(&mut up);
            c.upgrades[j] = Some(up);
            return (Outcome::Changed(c), None);
        } else if i == base + 4 && key.is_empty() && u.kind == Kind::ToolSwapper && u.advanced {
            crate::swapper::Mode::read(&up).next().write(&mut up);
            c.upgrades[j] = Some(up);
            return (Outcome::Changed(c), None);
        } else if i == base + 5 && key.is_empty() && u.kind == Kind::ToolSwapper && u.advanced {
            let v = !crate::swapper::swaps_weapons(&up);
            upgrades::set_flag(&mut up, "shouldSwapWeapon", v);
            c.upgrades[j] = Some(up);
            return (Outcome::Changed(c), None);
        } else if i == base + 4 && key.is_empty() && u.kind == Kind::Magnet {
            let v = !upgrades::flag(&up, "pickupItems", true);
            upgrades::set_flag(&mut up, "pickupItems", v);
            c.upgrades[j] = Some(up);
            return (Outcome::Changed(c), None);
        } else if i == base + 5 && key.is_empty() && u.kind == Kind::Magnet {
            let v = !upgrades::flag(&up, "pickupXp", true);
            upgrades::set_flag(&mut up, "pickupXp", v);
            c.upgrades[j] = Some(up);
            return (Outcome::Changed(c), None);
        } else {
            return (Outcome::Nothing, None);
        }
    }
    f.write(&*host, &mut up, key);
    c.upgrades[j] = Some(up);
    (Outcome::Changed(c), None)
}

/// The tags listed: the filter's own, then the picked item's that aren't in it, each with whether it is chosen.
fn listed(host: &dyn Host, f: &Filter, pick: Option<&ItemStack>) -> Vec<(String, bool)> {
    let mut v: Vec<(String, bool)> = f.tags.iter().map(|t| (t.clone(), true)).collect();
    if let Some(p) = pick {
        for t in Filter::tags_of(host, p) {
            if !f.tags.contains(&t) {
                v.push((t, false));
            }
        }
    }
    v
}

pub fn render_tags(
    host: &mut dyn ServerHost,
    s: &Session,
    c: &Contents,
    v: &View,
    out: &mut [Option<ItemStack>],
) {
    let View::Tags {
        slot: j,
        key,
        pick,
        page,
    } = v
    else {
        return;
    };
    let (j, key, page, pick) = (*j, *key, *page, pick.as_ref());
    let fill = filler(&*host);
    let base = s.control();
    let Some(up) = c.upgrades.get(j).cloned().flatten() else {
        return;
    };
    let Some(u) = upgrades::of(&*host, &up) else {
        return;
    };
    let f = read(&*host, &up, &u, key);
    for cell in out.iter_mut().take(base + 9) {
        *cell = fill.clone();
    }
    let all = listed(&*host, &f, pick);
    let of = pages(all.len(), base);
    let page = page.min(of - 1);
    for (i, (t, on)) in all.iter().skip(page * base).take(base).enumerate() {
        out[i] = tag_item(&*host, t, *on);
    }
    out[base] = button(&*host, "minecraft:arrow", "Back", &[]);
    out[base + 1] = match pick {
        Some(p) => Some(p.clone()),
        None => button(
            &*host,
            "minecraft:glass_bottle",
            "No item picked",
            &["Click an item in your inventory to list its tags"],
        ),
    };
    if of > 1 {
        out[base + 3] = page_button(&*host, page, of);
    }
    out[base + 7] = toggle(
        &*host,
        f.any_tag,
        "Any tag",
        "Click: switch between needing any tag and every tag",
    );
    out[base + 8] = Some(up);
}

pub fn click_tags(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    s: &Session,
    c: &Contents,
    slot: i32,
    v: &View,
    right: bool,
) -> (Outcome, Option<&'static str>) {
    let View::Tags {
        slot: j,
        key,
        pick,
        page,
    } = v
    else {
        return (Outcome::Nothing, None);
    };
    let (j, key, page, pick) = (*j, *key, *page, pick.clone());
    let mut c = c.clone();
    let base = s.control();
    let Some(mut up) = c.upgrades.get(j).cloned().flatten() else {
        return (Outcome::View(View::Main { page: 0 }), None);
    };
    let Some(u) = upgrades::of(&*host, &up) else {
        return (Outcome::View(View::Main { page: 0 }), None);
    };
    let mut f = read(&*host, &up, &u, key);
    if let Some(inv) = inventory_slot(s, slot) {
        let invs = places::inventory(host, player);
        let Some(st) = invs[inv as usize].clone() else {
            return (Outcome::Nothing, None);
        };
        return (
            Outcome::View(View::Tags {
                slot: j,
                key,
                pick: contents::with_count(&st, 1),
                page: 0,
            }),
            None,
        );
    }
    let Ok(i) = usize::try_from(slot) else {
        return (Outcome::Nothing, None);
    };
    let all = listed(&*host, &f, pick.as_ref());
    let of = pages(all.len(), base);
    let page = page.min(of - 1);
    if i < base {
        let Some((t, on)) = all.get(page * base + i).cloned() else {
            return (Outcome::Nothing, None);
        };
        if on {
            f.tags.retain(|x| *x != t);
        } else {
            f.tags.push(t);
            f.tags.sort();
        }
    } else if i == base {
        return (
            Outcome::View(View::Filter {
                slot: j,
                key,
                page: 0,
            }),
            None,
        );
    } else if i == base + 3 && of > 1 {
        return (
            Outcome::View(View::Tags {
                slot: j,
                key,
                pick,
                page: flip(page, of, right),
            }),
            None,
        );
    } else if i == base + 7 {
        f.any_tag = !f.any_tag;
    } else {
        return (Outcome::Nothing, None);
    }
    f.write(&*host, &mut up, key);
    c.upgrades[j] = Some(up);
    (Outcome::Changed(c), None)
}
