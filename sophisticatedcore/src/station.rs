//! The stonecutter and smithing upgrades (StonecutterUpgradeWrapper, SmithingUpgradeWrapper): a stonecutter and a
//! smithing table kept in the upgrade. Their items stay in the upgrade stack (`input`, `craftingInventory`), the
//! stonecutter's chosen recipe too (`recipeId`).
use mod_api::{Host, Identifier, ItemStack, PlayerRef, Recipe, RecipeInput, ServerHost, Tag};

use crate::contents::{
    self, Contents, Nbt, custom_data, get_str, put, read_items, set_custom_data, write_items,
};
use crate::menu::{Outcome, Session, View, button, filler, inventory_slot};
use crate::places;
use crate::upgrades::Kind;

/// The stonecutter view's cells: its input, the result, and the recipe grid (five columns, three rows a page).
const S_INPUT: usize = 10;
const S_RESULT: usize = 12;
const S_PER_PAGE: usize = 15;

/// The smithing view's cells: template, base, addition, and the result.
const M_CELLS: [usize; 3] = [10, 11, 12];
const M_RESULT: usize = 14;

/// The recipe cell `k` of a stonecutter page: columns 4 to 8 of the three rows.
fn recipe_cell(k: usize) -> usize {
    (k / 5) * 9 + 4 + k % 5
}

/// The recipe grid's cell `cell` (None when it is not one).
fn recipe_at(cell: usize) -> Option<usize> {
    let (row, col) = (cell / 9, cell % 9);
    (row < 3 && (4..9).contains(&col)).then(|| row * 5 + col - 4)
}

/// The recipes of `page`, `per` to a page.
pub fn page_of<T>(all: &[T], page: usize, per: usize) -> &[T] {
    let from = (page * per).min(all.len());
    &all[from..(from + per).min(all.len())]
}

/// The chosen recipe: the stored id if the input still offers it.
pub fn chosen(ids: &[&str], stored: Option<&str>) -> Option<usize> {
    let stored = stored?;
    ids.iter().position(|i| *i == stored)
}

fn set(out: &mut [Option<ItemStack>], at: usize, v: Option<ItemStack>) {
    if let Some(cell) = out.get_mut(at) {
        *cell = v;
    }
}

fn input(host: &dyn Host, up: &ItemStack) -> Option<ItemStack> {
    read_items(host, custom_data(up).at("input"), 1)
        .into_iter()
        .next()
        .flatten()
}

fn set_input(host: &dyn Host, up: &mut ItemStack, st: Option<ItemStack>) {
    let mut d = custom_data(up);
    put(&mut d, "input", write_items(host, &[st]));
    set_custom_data(up, d);
}

fn recipe_id(up: &ItemStack) -> Option<String> {
    get_str(&custom_data(up), "recipeId")
}

/// The stonecutting recipes `st` is an input of, in file order.
fn recipes_for(host: &dyn ServerHost, st: &ItemStack) -> Vec<Recipe> {
    host.recipes_of_type("minecraft:stonecutting")
        .iter()
        .filter(|r| {
            r.ingredients.first().is_some_and(|i| i.test(host, st)) && !r.results.is_empty()
        })
        .cloned()
        .collect()
}

fn smithing(host: &dyn Host, up: &ItemStack) -> Vec<Option<ItemStack>> {
    read_items(host, custom_data(up).at("craftingInventory"), 3)
}

fn set_smithing(host: &dyn Host, up: &mut ItemStack, g: &[Option<ItemStack>]) {
    let mut d = custom_data(up);
    put(&mut d, "craftingInventory", write_items(host, g));
    set_custom_data(up, d);
}

fn smithing_result(host: &dyn ServerHost, g: &[Option<ItemStack>]) -> Option<mod_api::RecipeMatch> {
    if g.iter().all(Option::is_none) {
        return None;
    }
    let level = host.level("minecraft:overworld")?;
    let input = RecipeInput::Smithing {
        template: g[0].clone(),
        base: g[1].clone(),
        addition: g[2].clone(),
    };
    level
        .recipe_for(host, &Identifier::parse("minecraft:smithing").ok()?, &input)
        .ok()
        .flatten()
}

/// Puts `st` into `slot`'s stack: all of it when empty, what fits when it is the same item. The rest, if any.
fn merge_in(host: &dyn Host, slot: &mut Option<ItemStack>, st: &ItemStack) -> Option<ItemStack> {
    match slot {
        None => {
            *slot = Some(st.clone());
            None
        }
        Some(have) if contents::same(have, st) => {
            let max = host.max_stack_size(st.item()).unwrap_or(64).max(1);
            let room = max.saturating_sub(have.count());
            let moved = room.min(st.count());
            if moved == 0 {
                return Some(st.clone());
            }
            *slot = contents::with_count(have, have.count() + moved);
            st.clone().shrink(moved)
        }
        Some(_) => Some(st.clone()),
    }
}

pub fn render_stonecutter(
    host: &mut dyn ServerHost,
    s: &Session,
    c: &Contents,
    j: usize,
    page: usize,
    out: &mut [Option<ItemStack>],
) {
    let fill = filler(&*host);
    let base = s.control();
    let Some(up) = c.upgrades.get(j).cloned().flatten() else {
        return;
    };
    for cell in out.iter_mut().take(base + 9) {
        *cell = fill.clone();
    }
    let have = input(&*host, &up);
    set(
        out,
        S_INPUT,
        have.clone().or_else(|| {
            button(
                &*host,
                "minecraft:white_stained_glass_pane",
                "Input",
                &["Click an item in your inventory to put it here"],
            )
        }),
    );
    set(
        out,
        S_INPUT + 1,
        button(&*host, "minecraft:arrow", "Stonecutter", &[]),
    );
    let options = have.map(|h| recipes_for(&*host, &h)).unwrap_or_default();
    for (k, r) in page_of(&options, page, S_PER_PAGE).iter().enumerate() {
        set(out, recipe_cell(k), r.results.first().cloned());
    }
    let ids: Vec<&str> = options.iter().map(|r| r.id.as_str()).collect();
    if let Some(at) = chosen(&ids, recipe_id(&up).as_deref()) {
        set(out, S_RESULT, options[at].results.first().cloned());
    } else {
        set(
            out,
            S_RESULT,
            button(
                &*host,
                "minecraft:gray_stained_glass_pane",
                "Pick a recipe",
                &[],
            ),
        );
    }
    let pages = options.len().div_ceil(S_PER_PAGE).max(1);
    if pages > 1 {
        let name = format!("Page {} of {pages}", page + 1);
        set(
            out,
            base + 4,
            button(
                &*host,
                "minecraft:paper",
                &name,
                &["Click: next page", "Right-click: previous page"],
            ),
        );
    }
    set(out, base, button(&*host, "minecraft:arrow", "Back", &[]));
    set(out, base + 8, Some(up));
}

pub fn click_stonecutter(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    s: &Session,
    c: &Contents,
    slot: i32,
    (j, page): (usize, usize),
    (right, shift): (bool, bool),
) -> (Outcome, Option<&'static str>) {
    let mut c = c.clone();
    let base = s.control();
    let Some(mut up) = c.upgrades.get(j).cloned().flatten() else {
        return (Outcome::View(View::Main { page: 0 }), None);
    };
    let have = input(&*host, &up);
    if let Some(inv) = inventory_slot(s, slot) {
        let invs = places::inventory(host, player);
        let Some(st) = invs[inv as usize].clone() else {
            return (Outcome::Nothing, None);
        };
        let mut cell = have;
        let rest = merge_in(&*host, &mut cell, &st);
        if rest.as_ref().is_some_and(|r| r.count() == st.count()) {
            return (
                Outcome::Nothing,
                Some("The input is full or holds another item"),
            );
        }
        places::set_slot(host, player, inv, rest);
        set_input(&*host, &mut up, cell);
        c.upgrades[j] = Some(up);
        return (Outcome::Changed(c), None);
    }
    let Ok(i) = usize::try_from(slot) else {
        return (Outcome::Nothing, None);
    };
    if i == base {
        return (Outcome::View(View::Main { page: 0 }), None);
    }
    if i == base + 4 {
        let n = have
            .as_ref()
            .map_or(0, |h| recipes_for(&*host, h).len().div_ceil(S_PER_PAGE));
        let next = if right {
            page.saturating_sub(1)
        } else if page + 1 < n {
            page + 1
        } else {
            0
        };
        return (
            Outcome::View(View::Stonecutter {
                slot: j,
                page: next,
            }),
            None,
        );
    }
    if i == S_INPUT {
        let Some(h) = have else {
            return (Outcome::Nothing, None);
        };
        places::give_or_drop(host, player, &h);
        set_input(&*host, &mut up, None);
        c.upgrades[j] = Some(up);
        return (Outcome::Changed(c), None);
    }
    let Some(h) = have else {
        return (Outcome::Nothing, None);
    };
    let options = recipes_for(&*host, &h);
    if let Some(k) = recipe_at(i) {
        let Some(r) = page_of(&options, page, S_PER_PAGE).get(k) else {
            return (Outcome::Nothing, None);
        };
        let mut d = custom_data(&up);
        put(&mut d, "recipeId", Tag::String(r.id.as_str().into()));
        set_custom_data(&mut up, d);
        c.upgrades[j] = Some(up);
        return (Outcome::Changed(c), None);
    }
    if i == S_RESULT {
        let ids: Vec<&str> = options.iter().map(|r| r.id.as_str()).collect();
        let Some(at) = chosen(&ids, recipe_id(&up).as_deref()) else {
            return (Outcome::Nothing, None);
        };
        let Some(out) = options[at].results.first() else {
            return (Outcome::Nothing, None);
        };
        let times = if shift { h.count() } else { 1 };
        for _ in 0..times {
            places::give_or_drop(host, player, out);
        }
        set_input(&*host, &mut up, h.shrink(times));
        c.upgrades[j] = Some(up);
        return (Outcome::Changed(c), None);
    }
    (Outcome::Nothing, None)
}

pub fn render_smithing(
    host: &mut dyn ServerHost,
    s: &Session,
    c: &Contents,
    (j, selected): (usize, Option<usize>),
    out: &mut [Option<ItemStack>],
) {
    let fill = filler(&*host);
    let base = s.control();
    let Some(up) = c.upgrades.get(j).cloned().flatten() else {
        return;
    };
    for cell in out.iter_mut().take(base + 9) {
        *cell = fill.clone();
    }
    let g = smithing(&*host, &up);
    let labels = [
        ("Template", "minecraft:paper"),
        ("Base", "minecraft:white_stained_glass_pane"),
        ("Addition", "minecraft:light_gray_stained_glass_pane"),
    ];
    for (k, &cell) in M_CELLS.iter().enumerate() {
        let v = g[k].clone().or_else(|| {
            let (name, id) = labels[k];
            let hint = if selected == Some(k) {
                "Click an item in your inventory to put it here"
            } else {
                "Click to select, then click an item in your inventory"
            };
            button(&*host, id, name, &[hint])
        });
        set(out, cell, v);
    }
    set(
        out,
        M_RESULT - 1,
        button(&*host, "minecraft:arrow", "Smithing", &[]),
    );
    set(out, M_RESULT, smithing_result(&*host, &g).map(|m| m.output));
    set(out, base, button(&*host, "minecraft:arrow", "Back", &[]));
    set(
        out,
        base + 8,
        button(
            &*host,
            "minecraft:barrier",
            "Clear",
            &["Puts the items back in your inventory"],
        ),
    );
    set(out, base + 4, Some(up));
}

pub fn click_smithing(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    s: &Session,
    c: &Contents,
    slot: i32,
    (j, selected): (usize, Option<usize>),
    shift: bool,
) -> (Outcome, Option<&'static str>) {
    let mut c = c.clone();
    let base = s.control();
    let Some(mut up) = c.upgrades.get(j).cloned().flatten() else {
        return (Outcome::View(View::Main { page: 0 }), None);
    };
    let mut g = smithing(&*host, &up);
    if let Some(inv) = inventory_slot(s, slot) {
        let invs = places::inventory(host, player);
        let Some(st) = invs[inv as usize].clone() else {
            return (Outcome::Nothing, None);
        };
        let cell = selected
            .filter(|&k| g[k].is_none() || g[k].as_ref().is_some_and(|h| contents::same(h, &st)))
            .or_else(|| g.iter().position(Option::is_none));
        let Some(k) = cell else {
            return (Outcome::Nothing, Some("The slots are full"));
        };
        let rest = merge_in(&*host, &mut g[k], &st);
        if rest.as_ref().is_some_and(|r| r.count() == st.count()) {
            return (Outcome::Nothing, Some("That slot is full"));
        }
        places::set_slot(host, player, inv, rest);
        set_smithing(&*host, &mut up, &g);
        c.upgrades[j] = Some(up);
        let next = g.iter().position(Option::is_none);
        return (
            Outcome::ChangedView(
                c,
                View::Smithing {
                    slot: j,
                    selected: next,
                },
            ),
            None,
        );
    }
    let Ok(i) = usize::try_from(slot) else {
        return (Outcome::Nothing, None);
    };
    if i == base {
        return (Outcome::View(View::Main { page: 0 }), None);
    }
    if let Some(k) = M_CELLS.iter().position(|&m| m == i) {
        return match g[k].take() {
            Some(st) if contents::is_unknown(&st) => {
                g[k] = Some(st);
                (
                    Outcome::Nothing,
                    Some("This item's mod isn't installed: it stays here, saved as it was"),
                )
            }
            Some(st) => {
                places::give_or_drop(host, player, &st);
                set_smithing(&*host, &mut up, &g);
                c.upgrades[j] = Some(up);
                (Outcome::Changed(c), None)
            }
            None => (
                Outcome::View(View::Smithing {
                    slot: j,
                    selected: Some(k),
                }),
                None,
            ),
        };
    }
    if i == M_RESULT {
        let times = if shift { 64 } else { 1 };
        let mut crafted = 0;
        for _ in 0..times {
            let Some(m) = smithing_result(&*host, &g) else {
                break;
            };
            for cell in g.iter_mut() {
                if let Some(st) = cell.take() {
                    *cell = st.shrink(1);
                }
            }
            places::give_or_drop(host, player, &m.output);
            for rem in m.remainders.iter().flatten() {
                places::give_or_drop(host, player, rem);
            }
            crafted += 1;
        }
        if crafted == 0 {
            return (Outcome::Nothing, None);
        }
        set_smithing(&*host, &mut up, &g);
        c.upgrades[j] = Some(up);
        return (Outcome::Changed(c), None);
    }
    if i == base + 8 {
        for cell in g.iter_mut() {
            if cell.as_ref().is_some_and(contents::is_unknown) {
                continue;
            }
            if let Some(st) = cell.take() {
                places::give_or_drop(host, player, &st);
            }
        }
        set_smithing(&*host, &mut up, &g);
        c.upgrades[j] = Some(up);
        return (Outcome::Changed(c), None);
    }
    (Outcome::Nothing, None)
}

/// The kinds that have these views.
pub fn view_of(kind: Kind, slot: usize) -> Option<View> {
    match kind {
        Kind::Stonecutter => Some(View::Stonecutter { slot, page: 0 }),
        Kind::Smithing => Some(View::Smithing {
            slot,
            selected: None,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_cells_run_over_columns_four_to_eight_of_three_rows() {
        assert_eq!(recipe_cell(0), 4);
        assert_eq!(recipe_cell(4), 8);
        assert_eq!(recipe_cell(5), 13);
        assert_eq!(recipe_cell(14), 26);
        for k in 0..S_PER_PAGE {
            assert_eq!(recipe_at(recipe_cell(k)), Some(k));
        }
        assert_eq!(recipe_at(S_INPUT), None);
        assert_eq!(recipe_at(27), None);
    }

    #[test]
    fn pages_slice_the_recipes() {
        let all: Vec<u32> = (0..20).collect();
        assert_eq!(page_of(&all, 0, 15).len(), 15);
        assert_eq!(page_of(&all, 1, 15), &[15, 16, 17, 18, 19]);
        assert!(page_of(&all, 5, 15).is_empty());
    }

    #[test]
    fn the_stored_recipe_counts_only_while_offered() {
        let ids = ["a:x", "a:y"];
        assert_eq!(chosen(&ids, Some("a:y")), Some(1));
        assert_eq!(chosen(&ids, Some("a:z")), None);
        assert_eq!(chosen(&ids, None), None);
    }
}
