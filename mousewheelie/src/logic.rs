//! The rules of Mouse Wheelie that need no game: which slots an action reaches, the click
//! sequences it becomes, and the sort plan. Everything here is plain data, so it is unit-tested.

use mod_api::MenuSlotKind;

/// A part of the open menu the tweaks treat as one inventory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Hotbar,
    Main,
    Armor,
    Container,
}

/// What a slot holds, reduced to what the rules compare.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Equal for stacks of one kind (same item and components), assigned by the caller.
    pub kind: usize,
    pub count: u32,
    pub max: u32,
    /// The item id, e.g. `minecraft:stone`.
    pub name: String,
}

/// One click of the plan, in menu slot order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    Pick { slot: usize, right: bool },
    Quick(usize),
    Throw { slot: usize, whole: bool },
}

/// The modifier keys held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub space: bool,
}

/// How far an action reaches from the slot under the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    One,
    Stack,
    Kind,
    Part,
    Matching,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortMode {
    /// By item id (the creative tab order itself is not readable).
    Creative,
    Quantity,
    Name,
}

/// The menus whose slots the tweaks touch: the inventory and the chest-like screens, furnaces and
/// crafting tables of the original's list.
pub fn supported_menu(menu: Option<&str>) -> bool {
    let Some(menu) = menu else {
        return true;
    };
    matches!(
        menu,
        "minecraft:generic_9x1"
            | "minecraft:generic_9x2"
            | "minecraft:generic_9x3"
            | "minecraft:generic_9x4"
            | "minecraft:generic_9x5"
            | "minecraft:generic_9x6"
            | "minecraft:generic_3x3"
            | "minecraft:shulker_box"
            | "minecraft:hopper"
            | "minecraft:furnace"
            | "minecraft:blast_furnace"
            | "minecraft:smoker"
            | "minecraft:crafting"
    )
}

/// Which part a slot belongs to. `player_screen` is the inventory screen (no container slots), where
/// the hotbar is its own part; elsewhere it joins the main inventory unless `split_hotbar`.
pub fn scope_of(kind: MenuSlotKind, player_screen: bool, split_hotbar: bool) -> Option<Scope> {
    match kind {
        MenuSlotKind::Player(0..=8) => Some(if player_screen || split_hotbar {
            Scope::Hotbar
        } else {
            Scope::Main
        }),
        MenuSlotKind::Player(9..=35) => Some(Scope::Main),
        MenuSlotKind::Player(36..=39) if player_screen => Some(Scope::Armor),
        MenuSlotKind::Storage | MenuSlotKind::Input => Some(Scope::Container),
        _ => None,
    }
}

/// The part items go to when the wheel turns on a slot of `source`.
pub fn target_scope(source: Scope, player_screen: bool) -> Option<Scope> {
    match (source, player_screen) {
        (Scope::Container, _) => Some(Scope::Main),
        (Scope::Main, false) => Some(Scope::Container),
        (Scope::Main, true) => Some(Scope::Hotbar),
        (Scope::Hotbar, true) => Some(Scope::Main),
        _ => None,
    }
}

/// Whether the wheel direction moves from `source`: up from the player's inventory into the
/// container (the hotbar into the main inventory on the inventory screen), down the other way.
pub fn direction_fits(source: Scope, player_screen: bool, up: bool) -> bool {
    match (source, player_screen) {
        (Scope::Main, false) => up,
        (Scope::Container, _) => !up,
        (Scope::Hotbar, true) => up,
        (Scope::Main, true) => !up,
        _ => false,
    }
}

pub fn scroll_reach(m: Mods) -> Reach {
    if m.space {
        Reach::Matching
    } else if m.ctrl {
        Reach::Kind
    } else if m.shift {
        Reach::Stack
    } else {
        Reach::One
    }
}

/// What a press with modifiers does: `(drop, reach)`, `None` for a plain press.
pub fn click_reach(m: Mods) -> Option<(bool, Reach)> {
    if m.alt {
        let reach = if m.ctrl && m.shift {
            Reach::Part
        } else if m.ctrl {
            Reach::Kind
        } else {
            Reach::Stack
        };
        return Some((true, reach));
    }
    if m.space {
        Some((false, Reach::Matching))
    } else if m.ctrl && m.shift {
        Some((false, Reach::Part))
    } else if m.ctrl {
        Some((false, Reach::Kind))
    } else if m.shift {
        Some((false, Reach::Stack))
    } else {
        None
    }
}

/// The slots of one menu, as the rules read them.
pub struct View<'a> {
    pub entries: &'a [Option<Entry>],
    pub scopes: &'a [Option<Scope>],
    pub player_screen: bool,
}

/// The slots an action on `slot` reaches (non-empty ones, in menu order).
pub fn select(view: &View<'_>, slot: usize, reach: Reach) -> Vec<usize> {
    let (Some(Some(here)), Some(Some(source))) = (view.entries.get(slot), view.scopes.get(slot))
    else {
        return Vec::new();
    };
    let in_source = |i: usize| view.scopes[i] == Some(*source) && view.entries[i].is_some();
    match reach {
        Reach::One | Reach::Stack => vec![slot],
        Reach::Kind => (0..view.entries.len())
            .filter(|&i| {
                in_source(i)
                    && view.entries[i]
                        .as_ref()
                        .is_some_and(|e| e.kind == here.kind)
            })
            .collect(),
        Reach::Part => (0..view.entries.len()).filter(|&i| in_source(i)).collect(),
        Reach::Matching => {
            let Some(target) = target_scope(*source, view.player_screen) else {
                return Vec::new();
            };
            // Moving up deposits kinds the target already has; moving down tops up partial stacks.
            let partial_only =
                *source == Scope::Container || (view.player_screen && *source == Scope::Main);
            (0..view.entries.len())
                .filter(|&i| in_source(i))
                .filter(|&i| {
                    let kind = view.entries[i].as_ref().map(|e| e.kind);
                    (0..view.entries.len()).any(|t| {
                        view.scopes[t] == Some(target)
                            && view.entries[t].as_ref().is_some_and(|e| {
                                Some(e.kind) == kind && (!partial_only || e.count < e.max)
                            })
                    })
                })
                .collect()
        }
    }
}

/// One item sent where a shift-click sends the stack: lift the stack, put one back, send it, put
/// the rest back.
pub fn move_one(slot: usize, count: u32) -> Vec<Click> {
    if count <= 1 {
        return vec![Click::Quick(slot)];
    }
    vec![
        Click::Pick { slot, right: false },
        Click::Pick { slot, right: true },
        Click::Quick(slot),
        Click::Pick { slot, right: false },
    ]
}

/// The clicks for an action: quick moves (or drops) of every reached slot.
pub fn clicks_for(view: &View<'_>, slot: usize, reach: Reach, drop: bool) -> Vec<Click> {
    if reach == Reach::One && !drop {
        let Some(Some(entry)) = view.entries.get(slot) else {
            return Vec::new();
        };
        return move_one(slot, entry.count);
    }
    select(view, slot, reach)
        .into_iter()
        .map(|i| {
            if drop {
                Click::Throw {
                    slot: i,
                    whole: true,
                }
            } else {
                Click::Quick(i)
            }
        })
        .collect()
}

/// The clicks for one wheel step on `slot`.
pub fn scroll_clicks(view: &View<'_>, slot: usize, mods: Mods, up: bool) -> Vec<Click> {
    let Some(Some(source)) = view.scopes.get(slot) else {
        return Vec::new();
    };
    if !direction_fits(*source, view.player_screen, up) {
        return Vec::new();
    }
    clicks_for(view, slot, scroll_reach(mods), false)
}

/// Whether a refill should run: the stack in the selected hotbar slot is gone and nothing explains it
/// but use or breakage.
pub fn should_refill(
    had_stack: bool,
    same_slot: bool,
    now_empty: bool,
    drop_key_down: bool,
    playing: bool,
) -> bool {
    had_stack && same_slot && now_empty && !drop_key_down && playing
}

// ---------------------------------------------------------------------------------------------
// Sorting

type Cell = Option<(usize, u32)>;

/// What a left click on `pos` does with `carried`, as the game does it.
fn left_click(state: &mut [Cell], carried: &mut Cell, max: &dyn Fn(usize) -> u32, pos: usize) {
    match (state[pos], *carried) {
        (s, None) => {
            *carried = s;
            state[pos] = None;
        }
        (None, c) => {
            state[pos] = c;
            *carried = None;
        }
        (Some((sk, sc)), Some((ck, cc))) if sk == ck => {
            let total = sc + cc;
            let keep = total.min(max(sk));
            state[pos] = Some((sk, keep));
            *carried = (total > keep).then_some((ck, total - keep));
        }
        (Some(s), Some(c)) => {
            state[pos] = Some(c);
            *carried = Some(s);
        }
    }
}

/// The left clicks (positions in `entries`) that merge partial stacks and order the stacks by
/// `mode`, empty slots last. Carried must be empty when they run.
pub fn plan_sort(entries: &[Option<Entry>], mode: SortMode) -> Vec<usize> {
    let mut max_of = std::collections::HashMap::new();
    for e in entries.iter().flatten() {
        max_of.insert(e.kind, e.max.max(1));
    }
    let max = |kind: usize| max_of.get(&kind).copied().unwrap_or(1);
    let mut name_of = std::collections::HashMap::new();
    for e in entries.iter().flatten() {
        name_of.entry(e.kind).or_insert_with(|| e.name.clone());
    }
    let mut state: Vec<Cell> = entries
        .iter()
        .map(|e| e.as_ref().map(|e| (e.kind, e.count)))
        .collect();
    let mut carried: Cell = None;
    let mut plan = Vec::new();
    let click = |state: &mut Vec<Cell>, carried: &mut Cell, plan: &mut Vec<usize>, pos: usize| {
        left_click(state, carried, &max, pos);
        plan.push(pos);
    };

    // Merge: fill the earliest stacks of each kind from the latest.
    let mut kinds: Vec<usize> = max_of.keys().copied().collect();
    kinds.sort_unstable();
    for kind in kinds {
        if max(kind) <= 1 {
            continue;
        }
        let positions: Vec<usize> = (0..state.len())
            .filter(|&p| state[p].is_some_and(|(k, _)| k == kind))
            .collect();
        let (mut i, mut j) = (0, positions.len().saturating_sub(1));
        while i < j {
            let (pi, pj) = (positions[i], positions[j]);
            match (state[pi], state[pj]) {
                (Some((_, c)), _) if c >= max(kind) => i += 1,
                (_, None) => j -= 1,
                _ => {
                    click(&mut state, &mut carried, &mut plan, pj);
                    click(&mut state, &mut carried, &mut plan, pi);
                    if carried.is_some() {
                        click(&mut state, &mut carried, &mut plan, pj);
                    }
                }
            }
        }
    }

    // Order: the kinds in sort order, each kind's stacks together, empty slots last.
    let mut totals = std::collections::HashMap::new();
    for (k, c) in state.iter().flatten() {
        *totals.entry(*k).or_insert(0u32) += c;
    }
    let mut stacks: Vec<(usize, u32)> = state.iter().flatten().copied().collect();
    let path = |kind: usize| {
        let name = name_of.get(&kind).map(String::as_str).unwrap_or("");
        name.split_once(':').map_or(name, |(_, p)| p).to_owned()
    };
    stacks.sort_by(|a, b| {
        let key = |s: &(usize, u32)| match mode {
            SortMode::Creative => (0u32, name_of.get(&s.0).cloned().unwrap_or_default()),
            SortMode::Quantity => (
                u32::MAX - totals[&s.0],
                name_of.get(&s.0).cloned().unwrap_or_default(),
            ),
            SortMode::Name => (0u32, path(s.0)),
        };
        key(a).cmp(&key(b)).then(a.0.cmp(&b.0)).then(b.1.cmp(&a.1))
    });
    for (i, want) in stacks.iter().map(|s| s.0).enumerate() {
        if state[i].is_some_and(|(k, _)| k == want) {
            continue;
        }
        let Some(j) = (i + 1..state.len()).find(|&j| state[j].is_some_and(|(k, _)| k == want))
        else {
            continue;
        };
        if state[i].is_some() {
            click(&mut state, &mut carried, &mut plan, i);
        }
        click(&mut state, &mut carried, &mut plan, j);
        click(&mut state, &mut carried, &mut plan, i);
        if carried.is_some() {
            click(&mut state, &mut carried, &mut plan, j);
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: usize, count: u32, max: u32, name: &str) -> Option<Entry> {
        Some(Entry {
            kind,
            count,
            max,
            name: name.to_owned(),
        })
    }

    fn replay(entries: &[Option<Entry>], plan: &[usize]) -> Vec<Cell> {
        let mut max_of = std::collections::HashMap::new();
        for e in entries.iter().flatten() {
            max_of.insert(e.kind, e.max);
        }
        let mut state: Vec<Cell> = entries
            .iter()
            .map(|e| e.as_ref().map(|e| (e.kind, e.count)))
            .collect();
        let mut carried = None;
        for &p in plan {
            left_click(&mut state, &mut carried, &|k| max_of[&k], p);
        }
        assert_eq!(carried, None, "the cursor ends empty");
        state
    }

    fn total(state: &[Cell]) -> u32 {
        state.iter().flatten().map(|s| s.1).sum()
    }

    #[test]
    fn sort_merges_and_orders_by_name() {
        let entries = vec![
            entry(2, 10, 64, "minecraft:stone"),
            None,
            entry(1, 5, 64, "minecraft:apple"),
            entry(2, 60, 64, "minecraft:stone"),
            entry(1, 7, 64, "minecraft:apple"),
        ];
        let state = replay(&entries, &plan_sort(&entries, SortMode::Name));
        assert_eq!(total(&state), 82);
        assert_eq!(
            state,
            vec![Some((1, 12)), Some((2, 64)), Some((2, 6)), None, None]
        );
    }

    #[test]
    fn sort_by_quantity_puts_the_largest_total_first() {
        let entries = vec![
            entry(1, 3, 64, "minecraft:apple"),
            entry(2, 40, 64, "minecraft:stone"),
            entry(3, 9, 64, "minecraft:dirt"),
        ];
        let state = replay(&entries, &plan_sort(&entries, SortMode::Quantity));
        assert_eq!(state, vec![Some((2, 40)), Some((3, 9)), Some((1, 3))]);
    }

    #[test]
    fn sort_keeps_unstackables_and_pushes_gaps_last() {
        let entries = vec![
            None,
            entry(5, 1, 1, "minecraft:stick"),
            None,
            entry(4, 1, 1, "minecraft:bow"),
        ];
        let state = replay(&entries, &plan_sort(&entries, SortMode::Creative));
        assert_eq!(state, vec![Some((4, 1)), Some((5, 1)), None, None]);
    }

    #[test]
    fn sorted_input_needs_no_clicks() {
        let entries = vec![entry(1, 64, 64, "a:b"), entry(2, 1, 64, "a:c"), None];
        assert!(plan_sort(&entries, SortMode::Creative).is_empty());
    }

    #[test]
    fn scopes_follow_the_screen() {
        assert_eq!(
            scope_of(MenuSlotKind::Player(3), true, false),
            Some(Scope::Hotbar)
        );
        assert_eq!(
            scope_of(MenuSlotKind::Player(3), false, false),
            Some(Scope::Main)
        );
        assert_eq!(
            scope_of(MenuSlotKind::Player(3), false, true),
            Some(Scope::Hotbar)
        );
        assert_eq!(
            scope_of(MenuSlotKind::Player(37), true, false),
            Some(Scope::Armor)
        );
        assert_eq!(scope_of(MenuSlotKind::Player(37), false, false), None);
        assert_eq!(
            scope_of(MenuSlotKind::Storage, false, false),
            Some(Scope::Container)
        );
        assert_eq!(scope_of(MenuSlotKind::Result, true, false), None);
    }

    #[test]
    fn wheel_direction_picks_the_source() {
        assert!(direction_fits(Scope::Main, false, true));
        assert!(!direction_fits(Scope::Main, false, false));
        assert!(direction_fits(Scope::Container, false, false));
        assert!(direction_fits(Scope::Hotbar, true, true));
        assert!(!direction_fits(Scope::Armor, true, true));
    }

    #[test]
    fn modifier_tables() {
        let m = |shift, ctrl, alt, space| Mods {
            shift,
            ctrl,
            alt,
            space,
        };
        assert_eq!(scroll_reach(m(false, false, false, false)), Reach::One);
        assert_eq!(scroll_reach(m(true, false, false, false)), Reach::Stack);
        assert_eq!(scroll_reach(m(true, true, false, false)), Reach::Kind);
        assert_eq!(scroll_reach(m(false, false, false, true)), Reach::Matching);
        assert_eq!(click_reach(m(false, false, false, false)), None);
        assert_eq!(
            click_reach(m(true, false, false, false)),
            Some((false, Reach::Stack))
        );
        assert_eq!(
            click_reach(m(true, true, false, false)),
            Some((false, Reach::Part))
        );
        assert_eq!(
            click_reach(m(false, true, true, false)),
            Some((true, Reach::Kind))
        );
        assert_eq!(
            click_reach(m(true, true, true, false)),
            Some((true, Reach::Part))
        );
        assert_eq!(
            click_reach(m(false, false, true, false)),
            Some((true, Reach::Stack))
        );
    }

    fn chest_view<'a>(entries: &'a [Option<Entry>], scopes: &'a [Option<Scope>]) -> View<'a> {
        View {
            entries,
            scopes,
            player_screen: false,
        }
    }

    #[test]
    fn wheel_moves_one_item_or_the_stack() {
        // slot 0 container, slots 1..3 player main.
        let entries = vec![
            entry(1, 8, 64, "a:b"),
            entry(1, 3, 64, "a:b"),
            entry(2, 1, 64, "a:c"),
        ];
        let scopes = vec![Some(Scope::Container), Some(Scope::Main), Some(Scope::Main)];
        let view = chest_view(&entries, &scopes);
        let none = Mods::default();
        assert_eq!(scroll_clicks(&view, 1, none, true), move_one(1, 3));
        assert!(scroll_clicks(&view, 1, none, false).is_empty());
        assert_eq!(scroll_clicks(&view, 2, none, true), vec![Click::Quick(2)]);
        assert_eq!(
            scroll_clicks(
                &view,
                1,
                Mods {
                    shift: true,
                    ..none
                },
                true
            ),
            vec![Click::Quick(1)]
        );
        assert_eq!(scroll_clicks(&view, 0, none, false), move_one(0, 8));
    }

    #[test]
    fn ctrl_reaches_every_stack_of_the_kind_in_the_part() {
        let entries = vec![
            entry(1, 8, 64, "a:b"),
            entry(1, 3, 64, "a:b"),
            entry(2, 1, 64, "a:c"),
            entry(1, 2, 64, "a:b"),
        ];
        let scopes = vec![
            Some(Scope::Container),
            Some(Scope::Main),
            Some(Scope::Main),
            Some(Scope::Main),
        ];
        let view = chest_view(&entries, &scopes);
        assert_eq!(select(&view, 1, Reach::Kind), vec![1, 3]);
        assert_eq!(select(&view, 1, Reach::Part), vec![1, 2, 3]);
        assert_eq!(
            clicks_for(&view, 1, Reach::Kind, true),
            vec![
                Click::Throw {
                    slot: 1,
                    whole: true
                },
                Click::Throw {
                    slot: 3,
                    whole: true
                }
            ]
        );
    }

    #[test]
    fn deposit_and_restock_match_kinds_in_the_other_part() {
        let entries = vec![
            entry(1, 8, 64, "a:b"),
            entry(3, 8, 64, "a:d"),
            entry(1, 3, 64, "a:b"),
            entry(2, 1, 64, "a:c"),
            entry(3, 64, 64, "a:d"),
        ];
        let scopes = vec![
            Some(Scope::Container),
            Some(Scope::Container),
            Some(Scope::Main),
            Some(Scope::Main),
            Some(Scope::Main),
        ];
        let view = chest_view(&entries, &scopes);
        // Deposit from the player's part: kinds 1 and 3 are in the chest.
        assert_eq!(select(&view, 2, Reach::Matching), vec![2, 4]);
        // Restock from the chest: kind 1 has a partial stack in the inventory, kind 3 only a full one.
        assert_eq!(select(&view, 0, Reach::Matching), vec![0]);
    }

    #[test]
    fn refill_only_after_a_plain_loss() {
        assert!(should_refill(true, true, true, false, true));
        assert!(!should_refill(true, true, true, true, true));
        assert!(!should_refill(true, false, true, false, true));
        assert!(!should_refill(false, true, true, false, true));
        assert!(!should_refill(true, true, false, false, true));
        assert!(!should_refill(true, true, true, false, false));
    }

    #[test]
    fn supported_menus() {
        assert!(supported_menu(None));
        assert!(supported_menu(Some("minecraft:generic_9x3")));
        assert!(!supported_menu(Some("minecraft:anvil")));
    }
}
