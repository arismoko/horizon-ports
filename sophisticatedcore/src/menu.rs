//! The storage menu (StorageContainerMenuBase and its screen), as a mod chest menu (`open_menu`): rows of the
//! storage's slots by page, then a control row: sort, the upgrade slots, the page button. Right-click upgrades
//! open their settings (filters, allow/block, match by item or mod, the kind's own switches) or the crafting
//! grid in the same window.
//!
//! A mod menu's cursor and player slots can't be written for the player who hosts the world (see
//! notes/ports/toms_storage.md), so there is no cursor: a click on a stored stack moves it to the inventory (a
//! stack, half with the right button), a click on an inventory stack puts it in (one with the right button),
//! shift-click on an upgrade in the inventory puts it in an upgrade slot.
use mod_api::{
    Compound, Host, Identifier, ItemStack, MenuClickedEvent, MenuReplace, PlayerRef, RecipeInput,
    ServerHost, Tag, Text,
};

use crate::contents::{self, Contents};
use crate::jukebox::{self, Deck, Repeat, Source};
use crate::places::{self, Loc};
use crate::storage_items::{self, Family, StorageItem};
use crate::upgrades::{self, Kind};

#[derive(Clone, Debug, PartialEq)]
pub enum View {
    Main {
        page: usize,
    },
    Settings {
        slot: usize,
    },
    Crafting {
        slot: usize,
        selected: Option<usize>,
    },
    Jukebox {
        slot: usize,
    },
}

#[derive(Clone, Debug)]
pub struct Session {
    pub window: i32,
    pub loc: Loc,
    pub id: [i32; 4],
    pub kind: &'static StorageItem,
    /// The storage's slot count (twice the tier's for a double chest).
    pub slots: usize,
    pub view: View,
    pub rows: u8,
}

impl Session {
    /// Rows of storage slots shown at once (at most five; the sixth is the control row).
    pub fn page_rows(&self) -> usize {
        self.rows as usize - 1
    }
    pub fn page_size(&self) -> usize {
        self.page_rows() * 9
    }
    pub fn pages(&self) -> usize {
        self.slots.div_ceil(self.page_size()).max(1)
    }
    pub fn control(&self) -> usize {
        self.page_size()
    }
    pub fn own_slots(&self) -> usize {
        self.rows as usize * 9
    }
}

pub fn rows_for(slots: usize) -> u8 {
    (slots.div_ceil(9).clamp(1, 5) + 1) as u8
}

/// What a click on the stand-in of an unknown item says.
const UNKNOWN_MSG: &str = "This item's mod isn't installed: it stays here, saved as it was";

/// The crafting view's grid cells and result slot.
const GRID: [usize; 9] = [0, 1, 2, 9, 10, 11, 18, 19, 20];
const RESULT: usize = 14;

fn item(host: &dyn Host, id: &str) -> Option<ItemStack> {
    let key = host.item_key(&Identifier::parse(id).ok()?)?;
    ItemStack::new(key, 1)
}

/// A display item: `id` named `name`, with `lore` lines.
pub fn button(host: &dyn Host, id: &str, name: &str, lore: &[&str]) -> Option<ItemStack> {
    let mut st = item(host, id)?;
    let mut patch = Compound::new();
    patch.insert(
        "minecraft:custom_name".into(),
        Tag::compound([("text", Tag::String(name.into())), ("italic", Tag::Byte(0))]),
    );
    if !lore.is_empty() {
        patch.insert(
            "minecraft:lore".into(),
            Tag::List(
                lore.iter()
                    .map(|l| {
                        Tag::compound([
                            ("text", Tag::String((*l).into())),
                            ("color", Tag::String("gray".into())),
                            ("italic", Tag::Byte(0)),
                        ])
                    })
                    .collect(),
            ),
        );
    }
    let _ = st.apply_components(&patch);
    Some(st)
}

fn filler(host: &dyn Host) -> Option<ItemStack> {
    button(host, "minecraft:black_stained_glass_pane", " ", &[])
}

/// A stored stack as the menu shows it: at most a stack of it, with the stored count in its lore when more.
fn shown(host: &dyn Host, st: &ItemStack) -> Option<ItemStack> {
    let max = host.max_stack_size(st.item()).unwrap_or(64).max(1);
    if st.count() <= max {
        return Some(st.clone());
    }
    let mut s = contents::with_count(st, max)?;
    let mut patch = Compound::new();
    let lore = format!("Stored: {}", st.count());
    patch.insert(
        "minecraft:lore".into(),
        Tag::List(vec![Tag::compound([
            ("text", Tag::String(lore.as_str().into())),
            ("color", Tag::String("gold".into())),
            ("italic", Tag::Byte(0)),
        ])]),
    );
    let _ = s.apply_components(&patch);
    Some(s)
}

fn kind_has_view(u: &upgrades::Upgrade) -> bool {
    u.filters > 0 || matches!(u.kind, Kind::Crafting | Kind::Jukebox)
}

/// What the menu shows for the session's view.
pub fn render(host: &mut dyn ServerHost, s: &Session, c: &Contents) -> Vec<Option<ItemStack>> {
    let mut out: Vec<Option<ItemStack>> = vec![None; s.own_slots()];
    let fill = filler(&*host);
    let base = s.control();
    match &s.view {
        View::Main { page } => {
            for i in 0..s.page_size() {
                let idx = page * s.page_size() + i;
                out[i] = if idx < s.slots {
                    c.slots[idx].as_ref().and_then(|st| shown(&*host, st))
                } else {
                    fill.clone()
                };
            }
            out[base] = button(
                &*host,
                "minecraft:hopper",
                "Sort",
                &["Merges and sorts the contents by item"],
            );
            for j in 0..7 {
                out[base + 1 + j] = if j < c.upgrades.len() {
                    match &c.upgrades[j] {
                        Some(u) => Some(u.clone()),
                        None => button(
                            &*host,
                            "minecraft:light_gray_stained_glass_pane",
                            "Upgrade slot",
                            &["Shift-click an upgrade in your inventory to put it here"],
                        ),
                    }
                } else {
                    fill.clone()
                };
            }
            out[base + 8] = if s.pages() > 1 {
                let name = format!("Page {} of {}", page + 1, s.pages());
                button(
                    &*host,
                    "minecraft:paper",
                    &name,
                    &["Click: next page", "Right-click: previous page"],
                )
            } else {
                fill.clone()
            };
        }
        View::Settings { slot } => {
            let Some(up) = c.upgrades.get(*slot).cloned().flatten() else {
                return out;
            };
            let Some(u) = upgrades::of(&*host, &up) else {
                return out;
            };
            let f = upgrades::filters(&*host, &up, u.filters);
            for (i, x) in f.iter().enumerate() {
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
            for i in f.len()..base {
                out[i] = fill.clone();
            }
            for i in base..base + 9 {
                out[i] = fill.clone();
            }
            out[base] = button(&*host, "minecraft:arrow", "Back", &[]);
            if u.kind != Kind::Refill {
                out[base + 2] = if upgrades::allow_list(&up, &u) {
                    button(
                        &*host,
                        "minecraft:white_wool",
                        "Allow list",
                        &["Only listed items match", "Click: switch to a block list"],
                    )
                } else {
                    button(
                        &*host,
                        "minecraft:black_wool",
                        "Block list",
                        &[
                            "Listed items don't match; an empty list matches everything",
                            "Click: switch to an allow list",
                        ],
                    )
                };
                out[base + 3] = if upgrades::match_mod(&up) {
                    button(
                        &*host,
                        "minecraft:book",
                        "Match by mod",
                        &["Click: match by item"],
                    )
                } else {
                    button(
                        &*host,
                        "minecraft:paper",
                        "Match by item",
                        &["Click: match by mod"],
                    )
                };
            }
            match u.kind {
                Kind::Void => {
                    out[base + 4] = if upgrades::void_overflow(&u, &up) {
                        button(
                            &*host,
                            "minecraft:lava_bucket",
                            "Void overflow",
                            &[
                                "Matching items are voided only when they don't fit",
                                "Click: void every matching item",
                            ],
                        )
                    } else {
                        button(
                            &*host,
                            "minecraft:bucket",
                            "Void all matching",
                            &[
                                "Matching items are voided as they are picked up",
                                "Click: void only what doesn't fit",
                            ],
                        )
                    };
                }
                Kind::Magnet => {
                    out[base + 4] = if upgrades::flag(&up, "pickupItems", true) {
                        button(
                            &*host,
                            "minecraft:hopper",
                            "Picks up items",
                            &["Click: don't pick up items"],
                        )
                    } else {
                        button(
                            &*host,
                            "minecraft:barrier",
                            "Doesn't pick up items",
                            &["Click: pick up items"],
                        )
                    };
                    out[base + 5] = if upgrades::flag(&up, "pickupXp", true) {
                        button(
                            &*host,
                            "minecraft:experience_bottle",
                            "Pulls experience",
                            &["Click: don't pull experience orbs"],
                        )
                    } else {
                        button(
                            &*host,
                            "minecraft:glass_bottle",
                            "Doesn't pull experience",
                            &["Click: pull experience orbs"],
                        )
                    };
                }
                _ => {}
            }
            out[base + 8] = Some(up.clone());
        }
        View::Jukebox { slot } => {
            let Some(up) = c.upgrades.get(*slot).cloned().flatten() else {
                return out;
            };
            let Some(u) = upgrades::of(&*host, &up) else {
                return out;
            };
            let deck = Deck::read(&*host, &up, upgrades::disc_slots(&u));
            for i in 0..base {
                out[i] = fill.clone();
            }
            for (i, d) in deck.discs.iter().enumerate() {
                out[i] = match d {
                    Some(d) => Some(d.clone()),
                    None => button(
                        &*host,
                        "minecraft:white_stained_glass_pane",
                        "Disc slot",
                        &["Click a music disc in your inventory to add it"],
                    ),
                };
            }
            for i in base..base + 9 {
                out[i] = fill.clone();
            }
            out[base] = button(&*host, "minecraft:arrow", "Back", &[]);
            out[base + 2] = if deck.playing {
                button(&*host, "minecraft:red_dye", "Stop", &["Stops the music"])
            } else {
                button(
                    &*host,
                    "minecraft:lime_dye",
                    "Play",
                    &["Plays the discs in the slots"],
                )
            };
            out[base + 3] = button(
                &*host,
                "minecraft:spectral_arrow",
                "Previous",
                &["The disc before this one"],
            );
            out[base + 4] = button(&*host, "minecraft:arrow", "Next", &["The next disc"]);
            out[base + 5] = if deck.shuffle {
                button(
                    &*host,
                    "minecraft:ender_pearl",
                    "Shuffle on",
                    &["Click: play in order"],
                )
            } else {
                button(
                    &*host,
                    "minecraft:snowball",
                    "Shuffle off",
                    &["Click: shuffle the discs"],
                )
            };
            let (name, lore) = match deck.repeat {
                Repeat::No => ("Repeat off", "Click: repeat the disc"),
                Repeat::One => ("Repeat one", "Click: repeat all discs"),
                Repeat::All => ("Repeat all", "Click: don't repeat"),
            };
            out[base + 6] = button(&*host, "minecraft:repeater", name, &[lore]);
            out[base + 8] = Some(up.clone());
        }
        View::Crafting { slot, selected } => {
            let Some(up) = c.upgrades.get(*slot).cloned().flatten() else {
                return out;
            };
            let grid = upgrades::grid(&*host, &up);
            for i in 0..base {
                out[i] = fill.clone();
            }
            for (k, &cell) in GRID.iter().enumerate() {
                out[cell] = match &grid[k] {
                    Some(st) => Some(st.clone()),
                    None if *selected == Some(k) => button(
                        &*host,
                        "minecraft:lime_stained_glass_pane",
                        "Selected",
                        &["Click an item in your inventory to put one here"],
                    ),
                    None => button(
                        &*host,
                        "minecraft:white_stained_glass_pane",
                        "Empty",
                        &["Click to select, then click an item in your inventory"],
                    ),
                };
            }
            out[12] = button(&*host, "minecraft:gray_stained_glass_pane", " ", &[]);
            out[13] = button(
                &*host,
                "minecraft:arrow",
                "Craft",
                &[
                    "Click the result to craft one",
                    "Shift-click: craft as many as the grid allows",
                ],
            );
            out[RESULT] = craft_result(host, s, &grid).map(|m| m.output);
            for i in base..base + 9 {
                out[i] = fill.clone();
            }
            out[base] = button(&*host, "minecraft:arrow", "Back", &[]);
            out[base + 4] = button(
                &*host,
                "minecraft:crafting_table",
                "Crafting",
                &["Empty grid slots are refilled from the backpack"],
            );
            out[base + 8] = button(
                &*host,
                "minecraft:barrier",
                "Clear grid",
                &["Puts the grid's items back in your inventory"],
            );
        }
    }
    out
}

fn player_level(host: &dyn ServerHost, s: &Session, uuid: u128) -> Option<mod_api::LevelRef> {
    let dim = match &s.loc {
        Loc::Block { dim, .. } => dim.clone(),
        _ => host.player(uuid)?.dimension(host)?.to_string(),
    };
    host.level(&dim)
}

thread_local! {
    static CRAFT_PLAYER: std::cell::Cell<u128> = const { std::cell::Cell::new(0) };
}

fn craft_result(
    host: &dyn ServerHost,
    s: &Session,
    grid: &[Option<ItemStack>],
) -> Option<mod_api::RecipeMatch> {
    if grid.iter().all(Option::is_none) {
        return None;
    }
    let uuid = CRAFT_PLAYER.with(|c| c.get());
    let level = player_level(host, s, uuid)?;
    let input = RecipeInput::Crafting {
        width: 3,
        height: 3,
        slots: grid.to_vec(),
    };
    level
        .recipe_for(host, &Identifier::parse("minecraft:crafting").ok()?, &input)
        .ok()
        .flatten()
}

/// The inventory slot a click past the menu's own slots is on (27 main rows first, then the hotbar).
fn inventory_slot(s: &Session, slot: i32) -> Option<u32> {
    let rel = usize::try_from(slot).ok()?.checked_sub(s.own_slots())?;
    match rel {
        0..27 => Some(rel as u32 + 9),
        27..36 => Some(rel as u32 - 27),
        _ => None,
    }
}

/// Whether `stack` may go in a storage of `kind` (the disallowed lists and the container rules of
/// [`refuses_container`]); chests and barrels take anything.
pub fn may_store(host: &dyn Host, kind: &StorageItem, stack: &ItemStack) -> bool {
    if !matches!(kind.family, Family::Backpack | Family::ShulkerBox) {
        return true;
    }
    let id = contents::item_id(host, stack);
    let lists = crate::options::Lists::get(host);
    let opts = crate::options::Options::read(host);
    let backpack = kind.family == Family::Backpack;
    let refused = if backpack {
        &lists.backpack_disallowed
    } else {
        &lists.shulker_disallowed
    };
    if crate::options::listed(refused, &id) {
        return false;
    }
    !refuses_container(
        &id,
        storage_items::by_item(&id).map(|o| o.family),
        backpack,
        if backpack {
            opts.backpack_containers_denied
        } else {
            opts.storage_containers_denied
        },
        opts.in_container_items,
    )
}

/// The container rules of `BackpackInventoryHandler.isAllowed` and `StackStorageWrapper.isAllowedInStorage`: a
/// backpack never goes in a backpack (no inception upgrade), no shulker box goes in a shulker box, and with
/// `containerItemsDisallowed` on, nothing that cannot fit inside container items goes in: shulker boxes always,
/// backpacks unless `canBePlacedInContainerItems`.
fn refuses_container(
    id: &str,
    family: Option<Family>,
    into_backpack: bool,
    denied: bool,
    backpacks_fit: bool,
) -> bool {
    let shulker = id.ends_with("shulker_box") || family == Some(Family::ShulkerBox);
    let backpack = family == Some(Family::Backpack);
    if into_backpack {
        backpack || (denied && shulker)
    } else {
        shulker || (denied && backpack && !backpacks_fit)
    }
}

/// Can `u` go into upgrade slot `j`? (UpgradeHandler: slot free, the group's limit, a downgrade only when every
/// stack still fits.)
pub fn may_add_upgrade(
    host: &dyn Host,
    kind: &StorageItem,
    c: &Contents,
    j: usize,
    up: &ItemStack,
) -> Result<(), &'static str> {
    let Some(u) = upgrades::of(host, up) else {
        return Err("That isn't an upgrade");
    };
    if c.upgrades.get(j).is_none_or(Option::is_some) {
        return Err("That upgrade slot is taken");
    }
    let id = contents::item_id(host, up);
    if let Some((group, max)) = upgrades::group(&id, kind.family) {
        let n = c
            .upgrades
            .iter()
            .flatten()
            .filter(|x| {
                upgrades::group(&contents::item_id(host, x), kind.family)
                    .is_some_and(|g| g.0 == group)
            })
            .count();
        if n >= max {
            return Err("No more upgrades of this kind fit");
        }
    }
    if let Kind::Stack(m) = u.kind {
        let after = upgrades::multiplier(host, kind, &c.upgrades) * m;
        if c.slots
            .iter()
            .flatten()
            .any(|st| st.count() > upgrades::limit(host, st, after))
        {
            return Err("Too many items in a slot for this downgrade");
        }
    }
    Ok(())
}

pub fn may_remove_upgrade(
    host: &dyn Host,
    kind: &StorageItem,
    c: &Contents,
    j: usize,
) -> Result<(), &'static str> {
    let Some(up) = c.upgrades.get(j).cloned().flatten() else {
        return Err("Empty");
    };
    if let Some(upgrades::Upgrade {
        kind: Kind::Stack(m),
        ..
    }) = upgrades::of(host, &up)
    {
        let after = upgrades::multiplier(host, kind, &c.upgrades) / m;
        if c.slots
            .iter()
            .flatten()
            .any(|st| st.count() > upgrades::limit(host, st, after))
        {
            return Err("Take some items out first: they wouldn't fit without this upgrade");
        }
    }
    Ok(())
}

/// Merges equal stacks and sorts by item id, then by count (the jar's sort by name, close enough).
pub fn sort(host: &dyn Host, kind: &StorageItem, c: &mut Contents) {
    if kind.family == Family::LimitedBarrel {
        return;
    }
    let m = upgrades::multiplier(host, kind, &c.upgrades);
    // Stand-ins of unknown items stay in their slots.
    let mut all: Vec<ItemStack> = c
        .slots
        .iter_mut()
        .filter(|s| s.as_ref().is_some_and(|s| !contents::is_unknown(s)))
        .filter_map(Option::take)
        .collect();
    all.sort_by_key(|s| (contents::item_id(host, s), std::cmp::Reverse(s.count())));
    for st in all {
        let lim = upgrades::limit(host, &st, m);
        let n = st.count();
        let went = contents::insert(&mut c.slots, &st, n, lim);
        debug_assert_eq!(went, n);
    }
}

/// The outcome of a click: what to tell the player.
pub enum Outcome {
    Changed(Contents),
    /// A change that also moves to another view (the crafting grid's selection).
    ChangedView(Contents, View),
    View(View),
    Close,
    Nothing,
}

/// A click in the session's menu (`slot` the menu's index, `button`, `mode` ClickType's id).
pub fn click(
    host: &mut dyn ServerHost,
    uuid: u128,
    s: &Session,
    c: &Contents,
    ev: &MenuClickedEvent,
) -> (Outcome, Option<&'static str>) {
    let Some(player) = host.player(uuid) else {
        return (Outcome::Close, None);
    };
    if !matches!(ev.mode, 0 | 1) {
        return (Outcome::Nothing, None);
    }
    let shift = ev.mode == 1;
    let right = ev.button == 1;
    CRAFT_PLAYER.with(|cp| cp.set(uuid));
    match s.view.clone() {
        View::Main { page } => click_main(host, player, s, c, ev.slot, page, shift, right),
        View::Settings { slot } => click_settings(host, player, s, c, ev.slot, slot),
        View::Jukebox { slot } => click_jukebox(host, player, s, c, ev.slot, slot, uuid),
        View::Crafting { slot, selected } => {
            click_crafting(host, player, s, c, ev.slot, slot, selected, shift)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn click_main(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    s: &Session,
    c: &Contents,
    slot: i32,
    page: usize,
    shift: bool,
    right: bool,
) -> (Outcome, Option<&'static str>) {
    let mut c = c.clone();
    let base = s.control();
    let m = upgrades::multiplier(&*host, s.kind, &c.upgrades);
    if let Some(inv) = inventory_slot(s, slot) {
        // An inventory stack into the storage.
        let invs = places::inventory(host, player);
        let Some(st) = invs[inv as usize].clone() else {
            return (Outcome::Nothing, None);
        };
        if !may_store(&*host, s.kind, &st) {
            return (Outcome::Nothing, Some("That can't go in here"));
        }
        if shift
            && upgrades::of(&*host, &st).is_some()
            && let Some(j) = (0..c.upgrades.len()).find(|&j| c.upgrades[j].is_none())
        {
            let one = contents::with_count(&st, 1).unwrap_or(st.clone());
            match may_add_upgrade(&*host, s.kind, &c, j, &one) {
                Ok(()) => {
                    c.upgrades[j] = Some(one);
                    places::set_slot(host, player, inv, st.clone().shrink(1));
                    return (Outcome::Changed(c), None);
                }
                Err(e) => return (Outcome::Nothing, Some(e)),
            }
        }
        let want = if right { 1 } else { st.count() };
        let lim = upgrades::limit(&*host, &st, m);
        let went = contents::insert(&mut c.slots, &st, want, lim);
        if went == 0 {
            return (Outcome::Nothing, Some("It's full"));
        }
        places::set_slot(host, player, inv, st.clone().shrink(went));
        return (Outcome::Changed(c), None);
    }
    let Ok(i) = usize::try_from(slot) else {
        return (Outcome::Nothing, None);
    };
    if i < base {
        // A stored stack to the inventory.
        let idx = page * s.page_size() + i;
        let Some(st) = c.slots.get(idx).cloned().flatten() else {
            return (Outcome::Nothing, None);
        };
        if contents::is_unknown(&st) {
            return (Outcome::Nothing, Some(UNKNOWN_MSG));
        }
        let max = host.max_stack_size(st.item()).unwrap_or(64).max(1);
        let take = st.count().min(max);
        let take = if right && !shift {
            take.div_ceil(2)
        } else {
            take
        };
        let Some(moving) = contents::with_count(&st, take) else {
            return (Outcome::Nothing, None);
        };
        let left = places::give(host, player, &moving);
        let gone = take - left;
        if gone == 0 {
            return (Outcome::Nothing, Some("Your inventory is full"));
        }
        c.slots[idx] = contents::with_count(&st, st.count() - gone);
        return (Outcome::Changed(c), None);
    }
    if i == base {
        sort(&*host, s.kind, &mut c);
        return (Outcome::Changed(c), None);
    }
    if i == base + 8 {
        let n = s.pages();
        if n > 1 {
            let next = if right {
                (page + n - 1) % n
            } else {
                (page + 1) % n
            };
            return (Outcome::View(View::Main { page: next }), None);
        }
        return (Outcome::Nothing, None);
    }
    if (base + 1..base + 8).contains(&i) {
        let j = i - base - 1;
        let Some(up) = c.upgrades.get(j).cloned().flatten() else {
            return (Outcome::Nothing, None);
        };
        if contents::is_unknown(&up) {
            return (Outcome::Nothing, Some(UNKNOWN_MSG));
        }
        let u = upgrades::of(&*host, &up);
        if !shift && let Some(u) = u.filter(kind_has_view) {
            let view = match u.kind {
                Kind::Crafting => View::Crafting {
                    slot: j,
                    selected: None,
                },
                Kind::Jukebox => View::Jukebox { slot: j },
                _ => View::Settings { slot: j },
            };
            return (Outcome::View(view), None);
        }
        if let Err(e) = may_remove_upgrade(&*host, s.kind, &c, j) {
            return (Outcome::Nothing, Some(e));
        }
        if places::give(host, player, &up) > 0 {
            return (Outcome::Nothing, Some("Your inventory is full"));
        }
        c.upgrades[j] = None;
        return (Outcome::Changed(c), None);
    }
    (Outcome::Nothing, None)
}

fn click_settings(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    s: &Session,
    c: &Contents,
    slot: i32,
    j: usize,
) -> (Outcome, Option<&'static str>) {
    let mut c = c.clone();
    let base = s.control();
    let Some(mut up) = c.upgrades.get(j).cloned().flatten() else {
        return (Outcome::View(View::Main { page: 0 }), None);
    };
    let Some(u) = upgrades::of(&*host, &up) else {
        return (Outcome::View(View::Main { page: 0 }), None);
    };
    let mut f = upgrades::filters(&*host, &up, u.filters);
    if let Some(inv) = inventory_slot(s, slot) {
        let invs = places::inventory(host, player);
        let Some(st) = invs[inv as usize].clone() else {
            return (Outcome::Nothing, None);
        };
        if f.iter().flatten().any(|x| x.same_item(&st)) {
            return (Outcome::Nothing, Some("Already in the filter"));
        }
        let Some(free) = f.iter().position(Option::is_none) else {
            return (Outcome::Nothing, Some("The filter is full"));
        };
        f[free] = contents::with_count(&st, 1);
        upgrades::set_filters(&*host, &mut up, &f);
    } else {
        let Ok(i) = usize::try_from(slot) else {
            return (Outcome::Nothing, None);
        };
        if i < f.len() {
            if f[i].is_none() {
                return (Outcome::Nothing, None);
            }
            f[i] = None;
            upgrades::set_filters(&*host, &mut up, &f);
        } else if i == base {
            return (Outcome::View(View::Main { page: 0 }), None);
        } else if i == base + 2 && u.kind != Kind::Refill {
            let v = !upgrades::allow_list(&up, &u);
            upgrades::set_flag(&mut up, "isAllowList", v);
        } else if i == base + 3 && u.kind != Kind::Refill {
            let v = if upgrades::match_mod(&up) {
                "item"
            } else {
                "mod"
            };
            upgrades::set_str(&mut up, "primaryMatch", v);
        } else if i == base + 4 && u.kind == Kind::Void {
            // `setShouldVoidOverflow`: "void all" is refused when the setting turns it off.
            let v = !upgrades::void_overflow(&u, &up);
            if v || crate::options::tune().void_anything[usize::from(u.advanced)] {
                upgrades::set_flag(&mut up, "shouldVoidOverflow", v);
            }
        } else if i == base + 4 && u.kind == Kind::Magnet {
            let v = !upgrades::flag(&up, "pickupItems", true);
            upgrades::set_flag(&mut up, "pickupItems", v);
        } else if i == base + 5 && u.kind == Kind::Magnet {
            let v = !upgrades::flag(&up, "pickupXp", true);
            upgrades::set_flag(&mut up, "pickupXp", v);
        } else {
            return (Outcome::Nothing, None);
        }
    }
    c.upgrades[j] = Some(up);
    (Outcome::Changed(c), None)
}

/// A click in a jukebox upgrade's view: discs in and out, the player's buttons.
fn click_jukebox(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    s: &Session,
    c: &Contents,
    slot: i32,
    j: usize,
    uuid: u128,
) -> (Outcome, Option<&'static str>) {
    let mut c = c.clone();
    let base = s.control();
    let Some(mut up) = c.upgrades.get(j).cloned().flatten() else {
        return (Outcome::View(View::Main { page: 0 }), None);
    };
    let Some(u) = upgrades::of(&*host, &up) else {
        return (Outcome::View(View::Main { page: 0 }), None);
    };
    let mut deck = Deck::read(&*host, &up, upgrades::disc_slots(&u));
    let Some(id) = s.id.into_iter().any(|x| x != 0).then_some(s.id) else {
        return (Outcome::Nothing, None);
    };
    let source = match &s.loc {
        Loc::Block { dim, pos } => Source::Block {
            dim: dim.clone(),
            pos: *pos,
            slot: j,
        },
        _ => Source::Carried {
            player: uuid,
            slot: j,
        },
    };
    let seed =
        u64::from(id[1] as u32) ^ (deck.discs.len() as u64).wrapping_mul(0x9E37_79B9) ^ slot as u64;
    if let Some(inv) = inventory_slot(s, slot) {
        let invs = places::inventory(host, player);
        let Some(st) = invs[inv as usize].clone() else {
            return (Outcome::Nothing, None);
        };
        if !Deck::valid(&*host, &st) {
            return (Outcome::Nothing, Some("Only music discs go here"));
        }
        let Some(free) = deck.discs.iter().position(Option::is_none) else {
            return (Outcome::Nothing, Some("The disc slots are full"));
        };
        deck.discs[free] = contents::with_count(&st, 1);
        places::set_slot(host, player, inv, st.clone().shrink(1));
    } else {
        let Ok(i) = usize::try_from(slot) else {
            return (Outcome::Nothing, None);
        };
        if i < deck.discs.len() {
            let Some(disc) = deck.discs[i].take() else {
                return (Outcome::Nothing, None);
            };
            // Taking out the disc that plays stops the music (the slot's setChanged).
            if deck.playing && deck.active == Some(i) {
                jukebox::stop(host, id, &mut deck);
            }
            places::give_or_drop(host, player, &disc);
        } else if i == base {
            return (Outcome::View(View::Main { page: 0 }), None);
        } else if i == base + 2 {
            if deck.playing {
                jukebox::stop(host, id, &mut deck);
            } else if !jukebox::play(host, id, &mut deck, source, seed) {
                return (Outcome::Nothing, Some("Put a music disc in first"));
            }
        } else if i == base + 3 || i == base + 4 {
            if deck.playing {
                jukebox::skip(host, id, &mut deck, i == base + 4, seed);
            }
        } else if i == base + 5 {
            deck.shuffle = !deck.shuffle;
            deck.played = deck.active.map_or(0, |a| 1u64 << a);
        } else if i == base + 6 {
            deck.repeat = deck.repeat.next();
        } else {
            return (Outcome::Nothing, None);
        }
    }
    deck.write(&*host, &mut up);
    c.upgrades[j] = Some(up);
    (Outcome::Changed(c), None)
}

#[allow(clippy::too_many_arguments)]
fn click_crafting(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    s: &Session,
    c: &Contents,
    slot: i32,
    j: usize,
    selected: Option<usize>,
    shift: bool,
) -> (Outcome, Option<&'static str>) {
    let mut c = c.clone();
    let base = s.control();
    let Some(mut up) = c.upgrades.get(j).cloned().flatten() else {
        return (Outcome::View(View::Main { page: 0 }), None);
    };
    let mut grid = upgrades::grid(&*host, &up);
    if let Some(inv) = inventory_slot(s, slot) {
        // One of an inventory stack into the selected (or first empty) grid cell.
        let invs = places::inventory(host, player);
        let Some(st) = invs[inv as usize].clone() else {
            return (Outcome::Nothing, None);
        };
        let cell = selected
            .filter(|&k| grid[k].is_none())
            .or_else(|| grid.iter().position(Option::is_none));
        let Some(k) = cell else {
            return (Outcome::Nothing, Some("The grid is full"));
        };
        grid[k] = contents::with_count(&st, 1);
        places::set_slot(host, player, inv, st.clone().shrink(1));
        upgrades::set_grid(&*host, &mut up, &grid);
        c.upgrades[j] = Some(up);
        let next = grid.iter().position(Option::is_none);
        return (
            Outcome::ChangedView(
                c,
                View::Crafting {
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
    if let Some(k) = GRID.iter().position(|&g| g == i) {
        match grid[k].take() {
            Some(st) if contents::is_unknown(&st) => return (Outcome::Nothing, Some(UNKNOWN_MSG)),
            Some(st) => {
                places::give_or_drop(host, player, &st);
                upgrades::set_grid(&*host, &mut up, &grid);
                c.upgrades[j] = Some(up);
                return (Outcome::Changed(c), None);
            }
            None => {
                return (
                    Outcome::View(View::Crafting {
                        slot: j,
                        selected: Some(k),
                    }),
                    None,
                );
            }
        }
    }
    if i == RESULT {
        let times = if shift { 64 } else { 1 };
        let mut crafted = 0;
        let mut first: Option<ItemStack> = None;
        for _ in 0..times {
            let Some(m) = craft_result(&*host, s, &grid) else {
                break;
            };
            if let Some(f) = &first
                && !contents::same(f, &m.output)
            {
                break;
            }
            first.get_or_insert(m.output.clone());
            // Consume one of each ingredient; remainders go back to their cell, else to the player.
            for k in 0..9 {
                if let Some(st) = grid[k].take() {
                    grid[k] = st.shrink(1);
                }
                if let Some(Some(rem)) = m.remainders.get(k) {
                    if grid[k].is_none() {
                        grid[k] = Some(rem.clone());
                    } else {
                        places::give_or_drop(host, player, rem);
                    }
                }
            }
            places::give_or_drop(host, player, &m.output);
            crafted += 1;
            // Refill emptied cells from the storage with what they held.
            let before = upgrades::grid(&*host, &up);
            for k in 0..9 {
                if grid[k].is_none()
                    && let Some(was) = &before[k]
                    && contents::extract(&mut c.slots, was, 1) == 1
                {
                    grid[k] = contents::with_count(was, 1);
                }
            }
            upgrades::set_grid(&*host, &mut up, &grid);
        }
        if crafted == 0 {
            return (Outcome::Nothing, None);
        }
        c.upgrades[j] = Some(up);
        return (Outcome::Changed(c), None);
    }
    if i == base {
        return (Outcome::View(View::Main { page: 0 }), None);
    }
    if i == base + 8 {
        for k in 0..9 {
            if grid[k].as_ref().is_some_and(contents::is_unknown) {
                continue;
            }
            if let Some(st) = grid[k].take() {
                places::give_or_drop(host, player, &st);
            }
        }
        upgrades::set_grid(&*host, &mut up, &grid);
        c.upgrades[j] = Some(up);
        return (Outcome::Changed(c), None);
    }
    (Outcome::Nothing, None)
}

/// Opens the menu for a storage at `loc`: its contents get an id if they have none.
pub fn open(
    host: &mut dyn ServerHost,
    uuid: u128,
    loc: Loc,
    replace: MenuReplace,
) -> Option<Session> {
    let (mut st, kind) = places::storage_at(host, uuid, &loc)?;
    let had = contents::uuid(&st);
    let id = contents::ensure_uuid(&mut st);
    if had.is_none() && !places::write(host, uuid, &loc, Some(st.clone())) {
        return None;
    }
    let c = Contents::read(&*host, &st, kind);
    let slots = c.slots.len();
    let rows = rows_for(slots);
    let mut s = Session {
        window: 0,
        loc,
        id,
        kind,
        slots,
        view: View::Main { page: 0 },
        rows,
    };
    let items = render(host, &s, &c);
    let title = title(&*host, &st, kind);
    match host.open_menu(uuid, rows, Text::literal(title), items, replace) {
        Ok(Some(w)) => {
            s.window = w;
            Some(s)
        }
        Ok(None) => None,
        Err(e) => {
            eprintln!("[sophisticatedcore] open_menu: {e:?}");
            None
        }
    }
}

/// The menu's title: the stack's own name, else the storage's (with its wood: "Oak Chest").
fn title(host: &dyn Host, st: &ItemStack, kind: &StorageItem) -> String {
    let _ = host;
    let wood = contents::get_str(&contents::custom_data(st), "woodType")
        .filter(|_| storage_items::wooden(kind));
    let own = match wood {
        Some(w) if !storage_items::wood_name(&w).is_empty() => {
            match kind.name.strip_prefix("Limited ") {
                Some(rest) => format!("Limited {} {rest}", storage_items::wood_name(&w)),
                None => format!("{} {}", storage_items::wood_name(&w), kind.name),
            }
        }
        _ => kind.name.to_string(),
    };
    let double = contents::slots_of(st, kind) > kind.slot_count();
    let own = if double { format!("Large {own}") } else { own };
    match st
        .components()
        .and_then(|c| contents::Nbt::at(c.as_nbt(), "minecraft:custom_name").cloned())
    {
        Some(Tag::String(s)) => s.to_string_lossy(),
        Some(Tag::Compound(t)) => contents::get_str(&t, "text").unwrap_or(own),
        _ => own,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_rules_follow_the_original() {
        let sh = Some(Family::ShulkerBox);
        let bp = Some(Family::Backpack);
        // A backpack never goes in a backpack; a shulker box goes in one unless the setting denies containers.
        assert!(refuses_container(
            "sophisticatedbackpacks:backpack",
            bp,
            true,
            false,
            false
        ));
        assert!(!refuses_container(
            "minecraft:red_shulker_box",
            None,
            true,
            false,
            false
        ));
        assert!(refuses_container(
            "minecraft:red_shulker_box",
            None,
            true,
            true,
            false
        ));
        assert!(!refuses_container(
            "minecraft:stone",
            None,
            true,
            true,
            false
        ));
        // No shulker box in a shulker box; a backpack goes in unless the setting denies it and it cannot fit.
        assert!(refuses_container(
            "sophisticatedstorage:shulker_box",
            sh,
            false,
            false,
            false
        ));
        assert!(!refuses_container(
            "sophisticatedbackpacks:backpack",
            bp,
            false,
            false,
            false
        ));
        assert!(refuses_container(
            "sophisticatedbackpacks:backpack",
            bp,
            false,
            true,
            false
        ));
        assert!(!refuses_container(
            "sophisticatedbackpacks:backpack",
            bp,
            false,
            true,
            true
        ));
    }

    #[test]
    fn rows() {
        let leather = storage_items::by_item("sophisticatedbackpacks:backpack").unwrap();
        assert_eq!(rows_for(leather.slots), 4);
        let s = Session {
            window: 1,
            loc: Loc::Inv(0),
            id: [0; 4],
            kind: leather,
            slots: 27,
            view: View::Main { page: 0 },
            rows: 4,
        };
        assert_eq!(
            (s.page_size(), s.pages(), s.control(), s.own_slots()),
            (27, 1, 27, 36)
        );
        let gold = storage_items::by_item("sophisticatedbackpacks:gold_backpack").unwrap();
        let s = Session {
            kind: gold,
            slots: gold.slots,
            rows: rows_for(gold.slots),
            ..s
        };
        assert_eq!((s.rows, s.page_size(), s.pages()), (6, 45, 2));
        let nether = storage_items::by_item("sophisticatedbackpacks:netherite_backpack").unwrap();
        let s = Session {
            kind: nether,
            slots: nether.slots,
            rows: rows_for(nether.slots),
            ..s
        };
        assert_eq!(s.pages(), 3);
        assert_eq!(inventory_slot(&s, 54), Some(9));
        assert_eq!(inventory_slot(&s, 54 + 27), Some(0));
    }
}
