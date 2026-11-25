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
    Compound, Host, Identifier, ItemStack, MenuClickedEvent, MenuRef, MenuReplace, PlayerRef,
    RecipeInput, ServerHost, Tag, Text,
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
    /// One filter of an upgrade (see `filter_view`): `key` names which, empty for the upgrade's own.
    Filter {
        slot: usize,
        key: &'static str,
        page: usize,
    },
    /// The tags of a filter, with the item whose tags are offered.
    Tags {
        slot: usize,
        key: &'static str,
        pick: Option<ItemStack>,
        page: usize,
    },
    Crafting {
        slot: usize,
        selected: Option<usize>,
    },
    Jukebox {
        slot: usize,
    },
    /// A cooking upgrade's furnace (and, for the automatic ones, its two filters).
    Cooking {
        slot: usize,
    },
    /// A hopper upgrade's sides and filters.
    Hopper {
        slot: usize,
    },
    /// A stonecutter upgrade's input and recipes.
    Stonecutter {
        slot: usize,
        page: usize,
    },
    /// A smithing upgrade's three slots.
    Smithing {
        slot: usize,
        selected: Option<usize>,
    },
    /// A tank, battery, pump or XP pump upgrade's screen.
    Fluid {
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
    /// A double chest's other half, which shows the lid open too.
    pub also: Option<[i32; 3]>,
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

pub(crate) fn item(host: &dyn Host, id: &str) -> Option<ItemStack> {
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

pub(crate) fn filler(host: &dyn Host) -> Option<ItemStack> {
    button(host, "minecraft:black_stained_glass_pane", " ", &[])
}

/// A stored stack as the menu shows it: at most a stack of it, with the stored count in its lore when more.
fn shown(host: &dyn Host, st: &ItemStack, notes: &[&str]) -> Option<ItemStack> {
    let max = host.max_stack_size(st.item()).unwrap_or(64).max(1);
    if st.count() <= max && notes.is_empty() {
        return Some(st.clone());
    }
    let mut s = contents::with_count(st, st.count().min(max))?;
    let mut lines = Vec::new();
    if st.count() > max {
        lines.push(("gold", format!("Stored: {}", st.count())));
    }
    lines.extend(notes.iter().map(|n| ("aqua", (*n).to_string())));
    let mut patch = Compound::new();
    patch.insert(
        "minecraft:lore".into(),
        Tag::List(
            lines
                .iter()
                .map(|(color, text)| {
                    Tag::compound([
                        ("text", Tag::String(text.as_str().into())),
                        ("color", Tag::String((*color).into())),
                        ("italic", Tag::Byte(0)),
                    ])
                })
                .collect(),
        ),
    );
    let _ = s.apply_components(&patch);
    Some(s)
}

fn kind_has_view(u: &upgrades::Upgrade) -> bool {
    u.filters > 0
        || matches!(
            u.kind,
            Kind::Crafting
                | Kind::Stonecutter
                | Kind::Smithing
                | Kind::Jukebox
                | Kind::Cooking(_)
                | Kind::Hopper
                | Kind::Tank
                | Kind::Battery
                | Kind::Pump
                | Kind::XpPump
                | Kind::Inception
        )
}

/// The control-row cells of the sort button and the page button, by the `sort_buttons_position`
/// setting (an index into `options::SORT_POSITIONS`): the title line puts sort first and the page
/// button last, the two below-upgrades positions swap them, and hidden has no sort button.
fn control_cells(base: usize, position: usize) -> (Option<usize>, usize) {
    match position {
        1 | 2 => (Some(base + 8), base),
        3 => (None, base + 8),
        _ => (Some(base), base + 8),
    }
}

/// What the menu shows for the session's view.
pub fn render(host: &mut dyn ServerHost, s: &Session, c: &Contents) -> Vec<Option<ItemStack>> {
    let mut out: Vec<Option<ItemStack>> = vec![None; s.own_slots()];
    let fill = filler(&*host);
    let base = s.control();
    match &s.view {
        View::Main { page } => {
            let visible = c.visible(&*host, s.kind);
            for i in 0..s.page_size() {
                let idx = page * s.page_size() + i;
                out[i] = if idx < s.slots {
                    match (&visible[idx], c.memory.get(idx).and_then(Option::as_ref)) {
                        (Some(st), memo) => {
                            let mut notes = Vec::new();
                            if memo.is_some() {
                                notes.push("Memory slot");
                            }
                            if c.no_sort.contains(&(idx as i32)) {
                                notes.push("Not sorted");
                            }
                            shown(&*host, st, &notes)
                        }
                        (None, Some(m)) => button(
                            &*host,
                            &contents::item_id(&*host, m),
                            "Memory slot",
                            &[
                                "Only this item goes in this slot",
                                "Shift-right-click to forget it",
                            ],
                        ),
                        (None, None) if c.no_sort.contains(&(idx as i32)) => button(
                            &*host,
                            "minecraft:lime_stained_glass_pane",
                            "Not sorted",
                            &["Press Q over the slot to sort it again"],
                        ),
                        (None, None) => None,
                    }
                } else {
                    fill.clone()
                };
            }
            let (sort_at, page_at) = control_cells(base, crate::options::tune().sort_position);
            out[base] = fill.clone();
            out[base + 8] = fill.clone();
            if let Some(at) = sort_at {
                out[at] = button(
                    &*host,
                    "minecraft:hopper",
                    "Sort",
                    &[
                        "Merges and sorts the contents by item",
                        "Right-click: memory slots match components too (or not)",
                        "Q over a slot: sort leaves it alone (or not)",
                    ],
                );
            }
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
            if s.pages() > 1 {
                let name = format!("Page {} of {}", page + 1, s.pages());
                out[page_at] = button(
                    &*host,
                    "minecraft:paper",
                    &name,
                    &["Click: next page", "Right-click: previous page"],
                );
            }
        }
        View::Filter { .. } => crate::filter_view::render(host, s, c, &s.view, &mut out),
        View::Tags { .. } => crate::filter_view::render_tags(host, s, c, &s.view, &mut out),
        View::Cooking { slot } => render_cooking(host, s, c, *slot, &mut out),
        View::Fluid { slot } => crate::fluid_view::render(host, s, c, *slot, &mut out),
        View::Stonecutter { slot, page } => {
            crate::station::render_stonecutter(host, s, c, *slot, *page, &mut out)
        }
        View::Smithing { slot, selected } => {
            crate::station::render_smithing(host, s, c, (*slot, *selected), &mut out)
        }
        View::Hopper { slot } => crate::hopper_view::render(host, s, c, *slot, &mut out),
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
pub(crate) fn inventory_slot(s: &Session, slot: i32) -> Option<u32> {
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

/// [`may_store`] for a storage that holds `upgrades`: a backpack with an inception upgrade also takes backpacks that
/// have none (`BackpackInventoryHandler.isAllowed`).
pub fn may_store_in(
    host: &dyn Host,
    kind: &StorageItem,
    upgrades: &[Option<ItemStack>],
    stack: &ItemStack,
) -> bool {
    let id = contents::item_id(host, stack);
    let inner = match storage_items::by_item(&id) {
        Some(o) if kind.family == Family::Backpack && o.family == Family::Backpack => o,
        _ => return may_store(host, kind, stack),
    };
    let has = |ups: &[Option<ItemStack>]| {
        ups.iter()
            .flatten()
            .any(|u| upgrades::of(host, u).is_some_and(|k| k.kind == Kind::Inception))
    };
    let listed = crate::options::listed(&crate::options::Lists::get(host).backpack_disallowed, &id);
    nests(
        has(upgrades),
        has(&Contents::read(host, stack, inner).upgrades),
        listed,
    )
}

/// Whether a backpack goes into another: the outer one needs the inception upgrade, the inner one must not have it.
fn nests(outer_inception: bool, inner_inception: bool, listed: bool) -> bool {
    outer_inception && !inner_inception && !listed
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
    crate::infinity::may_add(host, c, matches!(u.kind, Kind::Infinity(_)))?;
    if u.kind == Kind::Compression {
        crate::compression::may_add(host, kind, c)?;
    }
    if u.kind == Kind::Compacting
        && c.upgrades
            .iter()
            .flatten()
            .any(|x| upgrades::of(host, x).is_some_and(|o| o.kind == Kind::Compression))
    {
        return Err("There's already a Compression upgrade in another upgrade slot");
    }
    if u.kind == Kind::Everlasting
        && c.upgrades
            .iter()
            .flatten()
            .any(|x| upgrades::of(host, x).is_some_and(|o| o.kind == Kind::Everlasting))
    {
        return Err("There's already an Everlasting upgrade in another upgrade slot");
    }
    if u.kind == Kind::ToolSwapper
        && c.upgrades
            .iter()
            .flatten()
            .any(|x| upgrades::of(host, x).is_some_and(|o| o.kind == Kind::ToolSwapper))
    {
        return Err("There's already a Tool Swapper upgrade in another upgrade slot");
    }
    if matches!(u.kind, Kind::Tank | Kind::Battery)
        && c.upgrades
            .iter()
            .flatten()
            .any(|x| upgrades::of(host, x).is_some_and(|o| o.kind == u.kind))
    {
        return Err("Only one of these fits in a storage");
    }
    if let Kind::Stack(m) = u.kind {
        let after = upgrades::multiplier(host, kind, &c.upgrades) * m;
        let mut ups = c.upgrades.clone();
        ups[j] = Some(up.clone());
        if !liquids_fit(host, kind, c.slots.len(), &ups) {
            return Err("The tank or battery holds too much for this downgrade");
        }
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

/// Whether a tank's fluid and a battery's energy fit the capacity `ups` give them (IStackableContentsUpgrade).
fn liquids_fit(
    host: &dyn Host,
    kind: &StorageItem,
    slots: usize,
    ups: &[Option<ItemStack>],
) -> bool {
    let l = crate::fluids::limits_with(host, kind, slots, ups);
    ups.iter()
        .flatten()
        .all(|u| match upgrades::of(host, u).map(|u| u.kind) {
            Some(Kind::Tank) => crate::fluids::read_tank(u).amount <= l.tank_cap,
            Some(Kind::Battery) => crate::fluids::read_energy(u) <= l.battery_cap,
            _ => true,
        })
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
        let mut ups = c.upgrades.clone();
        ups[j] = None;
        if !liquids_fit(host, kind, c.slots.len(), &ups) {
            return Err("The tank or battery holds too much to lose this upgrade");
        }
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
    if kind.family == Family::LimitedBarrel || crate::infinity::level(host, &c.upgrades).is_some() {
        return;
    }
    let m = upgrades::multiplier(host, kind, &c.upgrades);
    let banks = crate::compression::Banks::of(host, kind, c);
    // Stand-ins of unknown items stay in their slots; a compression range keeps its slots; so do memory slots
    // (the original leaves them out of the sort).
    let free: Vec<bool> = (0..c.slots.len())
        .filter(|i| !banks.covers(*i))
        .map(|i| !c.sort_skips(i))
        .collect();
    let modes: Vec<u8> = free
        .iter()
        .map(|&f| {
            if f {
                contents::ACCEPTS
            } else {
                contents::REFUSES
            }
        })
        .collect();
    let mut all: Vec<ItemStack> = Vec::new();
    c.plain(&banks, |slots| {
        all = slots
            .iter_mut()
            .zip(&free)
            .filter(|(s, f)| **f && s.as_ref().is_some_and(|s| !contents::is_unknown(s)))
            .filter_map(|(s, _)| s.take())
            .collect();
        0
    });
    all.sort_by_key(|s| (contents::item_id(host, s), std::cmp::Reverse(s.count())));
    for st in all {
        let lim = upgrades::limit(host, &st, m);
        let n = st.count();
        let went = c.plain(&banks, |slots| {
            contents::insert_with(slots, &st, n, lim, true, Some(&modes))
        });
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
    if ev.mode == 4
        && let View::Main { page } = s.view
        && let Ok(i) = usize::try_from(ev.slot)
        && i < s.control()
    {
        // The drop key over a stored slot: the sort leaves it alone, or sorts it again.
        let mut c = c.clone();
        if c.toggle_no_sort(page * s.page_size() + i) {
            return (Outcome::Changed(c), None);
        }
        return (Outcome::Nothing, None);
    }
    if !matches!(ev.mode, 0 | 1) {
        return (Outcome::Nothing, None);
    }
    let shift = ev.mode == 1;
    let right = ev.button == 1;
    CRAFT_PLAYER.with(|cp| cp.set(uuid));
    let view = s.view.clone();
    match view {
        View::Main { page } => click_main(host, player, s, c, ev.slot, page, shift, right),
        View::Filter { .. } => crate::filter_view::click(host, player, s, c, ev.slot, &view, right),
        View::Tags { .. } => {
            crate::filter_view::click_tags(host, player, s, c, ev.slot, &view, right)
        }
        View::Jukebox { slot } => click_jukebox(host, player, s, c, ev.slot, slot, uuid),
        View::Cooking { slot } => click_cooking(host, player, s, c, ev.slot, slot, right),
        View::Hopper { slot } => {
            crate::hopper_view::click(host, player, s, c, ev.slot, slot, right)
        }
        View::Fluid { slot } => {
            crate::fluid_view::click(host, player, s, c, ev.slot, slot, right, shift)
        }
        View::Crafting { slot, selected } => {
            click_crafting(host, player, s, c, ev.slot, slot, selected, shift)
        }
        View::Stonecutter { slot, page } => crate::station::click_stonecutter(
            host,
            player,
            s,
            c,
            ev.slot,
            (slot, page),
            (right, shift),
        ),
        View::Smithing { slot, selected } => {
            crate::station::click_smithing(host, player, s, c, ev.slot, (slot, selected), shift)
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
    if let Some(inv) = inventory_slot(s, slot) {
        // An inventory stack into the storage.
        let invs = places::inventory(host, player);
        let Some(st) = invs[inv as usize].clone() else {
            return (Outcome::Nothing, None);
        };
        if !may_store_in(&*host, s.kind, &c.upgrades, &st) {
            return (Outcome::Nothing, Some("That can't go in here"));
        }
        if shift
            && upgrades::of(&*host, &st).is_some()
            && let Some(j) = (0..c.upgrades.len()).find(|&j| c.upgrades[j].is_none())
        {
            let mut one = contents::with_count(&st, 1).unwrap_or(st.clone());
            match may_add_upgrade(&*host, s.kind, &c, j, &one) {
                Ok(()) => {
                    crate::compression::on_added(&*host, &c, &mut one);
                    c.upgrades[j] = Some(one);
                    places::set_slot(host, player, inv, st.clone().shrink(1));
                    return (Outcome::Changed(c), None);
                }
                Err(e) => return (Outcome::Nothing, Some(e)),
            }
        }
        let want = if right { 1 } else { st.count() };
        let went = if let Some(need) = crate::infinity::level(&*host, &c.upgrades) {
            // Only an empty slot takes a stack, and only from a player with the upgrade's permission.
            if !crate::infinity::permits(player.permission_level(&*host), need) {
                return (
                    Outcome::Nothing,
                    Some("You can't put items into an infinity storage"),
                );
            }
            if crate::infinity::put(&mut c.slots, &st) {
                want
            } else {
                0
            }
        } else {
            c.add(&*host, s.kind, &st, want)
        };
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
        let idx = page * s.page_size() + i;
        if shift && right {
            // Keeps the slot for what it holds (MemorySettingsCategory.selectSlot), or forgets it.
            let holds = c.shown_at(&*host, s.kind, idx);
            if !c.toggle_memory(idx, holds.as_ref()) {
                return (Outcome::Nothing, Some("Put the item in the slot first"));
            }
            return (Outcome::Changed(c), None);
        }
        // A stored stack to the inventory.
        let Some(st) = c.visible(&*host, s.kind).get(idx).cloned().flatten() else {
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
        c.take_from(&*host, s.kind, idx, gone);
        return (Outcome::Changed(c), None);
    }
    let (sort_at, page_at) = control_cells(base, crate::options::tune().sort_position);
    if Some(i) == sort_at {
        if right {
            c.exact = !c.exact;
            let msg = if c.exact {
                "Memory slots now match components too"
            } else {
                "Memory slots now match the item only"
            };
            return (Outcome::Changed(c), Some(msg));
        }
        sort(&*host, s.kind, &mut c);
        return (Outcome::Changed(c), None);
    }
    if i == page_at {
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
                Kind::Stonecutter | Kind::Smithing => {
                    crate::station::view_of(u.kind, j).unwrap_or(View::Main { page: 0 })
                }
                Kind::Jukebox => View::Jukebox { slot: j },
                Kind::Cooking(_) => View::Cooking { slot: j },
                Kind::Hopper => View::Hopper { slot: j },
                Kind::Tank | Kind::Battery | Kind::Pump | Kind::XpPump => View::Fluid { slot: j },
                _ => View::Filter {
                    slot: j,
                    key: "",
                    page: 0,
                },
            };
            return (Outcome::View(view), None);
        }
        if !crate::infinity::allowed(&*host, player, &c.upgrades) {
            return (
                Outcome::Nothing,
                Some("Only an operator can remove the infinity upgrade"),
            );
        }
        if let Err(e) = may_remove_upgrade(&*host, s.kind, &c, j) {
            return (Outcome::Nothing, Some(e));
        }
        let mut back = up.clone();
        crate::compression::cleaned(&mut back);
        if places::give(host, player, &back) > 0 {
            return (Outcome::Nothing, Some("Your inventory is full"));
        }
        c.upgrades[j] = None;
        return (Outcome::Changed(c), None);
    }
    (Outcome::Nothing, None)
}

fn render_cooking(
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
    let Some(Kind::Cooking(k)) = upgrades::of(&*host, &up).map(|u| u.kind) else {
        return;
    };
    for cell in out.iter_mut().take(base + 9) {
        *cell = fill.clone();
    }
    let cook = crate::cooking::read(&*host, &up);
    let labels = [
        ("minecraft:white_stained_glass_pane", "Input"),
        ("minecraft:black_stained_glass_pane", "Fuel"),
        ("minecraft:gray_stained_glass_pane", "Output"),
    ];
    for (i, (id, name)) in labels.iter().enumerate() {
        out[i] = match &cook.slots[i] {
            Some(st) => Some(st.clone()),
            None => button(
                &*host,
                id,
                name,
                &[if i == 2 {
                    "Smelted items collect here"
                } else {
                    "Click an item in your inventory to put it here"
                }],
            ),
        };
    }
    let level = host
        .level("minecraft:overworld")
        .and_then(|l| l.game_time(&*host))
        .unwrap_or(0);
    let burning = cook.burning(level) && cook.burn_total > 0;
    out[4] = if burning {
        let left = (cook.burn_finish - level).max(0);
        button(
            &*host,
            "minecraft:blaze_powder",
            "Burning",
            &[&format!(
                "{} of {} ticks of fuel left",
                left, cook.burn_total
            )],
        )
    } else {
        button(&*host, "minecraft:gunpowder", "Not burning", &[])
    };
    if k.auto {
        let (fi, ff) = crate::cooking::filters(&*host, &up, k);
        for (at, f, mode) in [(6usize, &fi, "Input filter"), (7, &ff, "Fuel filter")] {
            let name = format!(
                "{mode}: {}",
                if f.allow { "allow list" } else { "block list" }
            );
            out[at] = button(
                &*host,
                if f.allow {
                    "minecraft:lime_dye"
                } else {
                    "minecraft:red_dye"
                },
                &name,
                &["Click: edit the filter"],
            );
        }
    }
    out[base] = button(&*host, "minecraft:arrow", "Back", &[]);
    out[base + 8] = Some(up);
}

/// A click in a cooking upgrade's view: items in and out of the furnace's slots, the automatic upgrades' filters.
fn click_cooking(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    s: &Session,
    c: &Contents,
    slot: i32,
    j: usize,
    right: bool,
) -> (Outcome, Option<&'static str>) {
    use crate::cooking::{Env, FUEL, INPUT, OUTPUT, Real};
    let mut c = c.clone();
    let base = s.control();
    let Some(mut up) = c.upgrades.get(j).cloned().flatten() else {
        return (Outcome::View(View::Main { page: 0 }), None);
    };
    let Some(Kind::Cooking(k)) = upgrades::of(&*host, &up).map(|u| u.kind) else {
        return (Outcome::View(View::Main { page: 0 }), None);
    };
    let mut cook = crate::cooking::read(&*host, &up);
    let (mut fi, mut ff) = crate::cooking::filters(&*host, &up, k);
    let Some(level) = host.level("minecraft:overworld") else {
        return (Outcome::Nothing, None);
    };
    if let Some(inv) = inventory_slot(s, slot) {
        let invs = places::inventory(host, player);
        let Some(st) = invs[inv as usize].clone() else {
            return (Outcome::Nothing, None);
        };
        let env = Real {
            host: &*host,
            level,
            kind: k,
        };
        let is_input = env.recipe(&st).is_some();
        let is_fuel = env.burn(&st) > 0;
        if right && k.auto {
            let (f, what) = if is_input {
                (&mut fi, "input")
            } else if is_fuel {
                (&mut ff, "fuel")
            } else {
                return (Outcome::Nothing, Some("It can't be cooked or burnt"));
            };
            if f.items.iter().flatten().any(|x| x.same_item(&st)) {
                return (Outcome::Nothing, Some("Already in the filter"));
            }
            let Some(free) = f.items.iter().position(Option::is_none) else {
                return (Outcome::Nothing, Some("The filter is full"));
            };
            f.items[free] = contents::with_count(&st, 1);
            let key = if what == "input" {
                "inputFilter"
            } else {
                "fuelFilter"
            };
            f.write(&*host, &mut up, key);
        } else {
            let (target, ok) = if is_input && (!k.auto || fi.matches(&*host, &st)) {
                (INPUT, true)
            } else if is_fuel && (!k.auto || ff.matches(&*host, &st)) {
                (FUEL, true)
            } else {
                (INPUT, false)
            };
            if !ok {
                return (Outcome::Nothing, Some("It can't be cooked or burnt here"));
            }
            let max = env.max(&st);
            let held = cook.slots[target].clone();
            let moved = match &held {
                None => st.count().min(max),
                Some(h) if contents::same(h, &st) => st.count().min(max.saturating_sub(h.count())),
                Some(_) => 0,
            };
            if moved == 0 {
                return (Outcome::Nothing, Some("That slot is full"));
            }
            let total = held.as_ref().map_or(0, |h| h.count()) + moved;
            cook.slots[target] = contents::with_count(&st, total);
            places::set_slot(host, player, inv, st.clone().shrink(moved));
            crate::cooking::write(&*host, &mut up, &cook);
        }
    } else {
        let Ok(i) = usize::try_from(slot) else {
            return (Outcome::Nothing, None);
        };
        if i <= OUTPUT {
            let Some(st) = cook.slots[i].take() else {
                return (Outcome::Nothing, None);
            };
            places::give_or_drop(host, player, &st);
            crate::cooking::write(&*host, &mut up, &cook);
        } else if k.auto && (6..=7).contains(&i) {
            let key = if i == 6 { "inputFilter" } else { "fuelFilter" };
            return (
                Outcome::View(View::Filter {
                    slot: j,
                    key,
                    page: 0,
                }),
                None,
            );
        } else if i == base {
            return (Outcome::View(View::Main { page: 0 }), None);
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
                    && c.remove(&*host, s.kind, was, 1) == 1
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
        also: None,
    };
    let items = render(host, &s, &c);
    let title = title(&*host, &st, kind);
    let player = host.player(uuid)?;
    match player.open_chest_menu(host, rows, Text::literal(title), items, replace) {
        Ok(menu) => {
            let Some(window) = menu.container_id(&*host) else {
                let _ = menu.close_container(host);
                return None;
            };
            s.window = window;
            Some(s)
        }
        Err(e) => {
            eprintln!("[sophisticatedcore] open_chest_menu: {e:?}");
            None
        }
    }
}

/// The player's open menu, when it is the window `window`.
pub fn live(host: &dyn ServerHost, uuid: u128, window: i32) -> Option<MenuRef> {
    let menu = host.player(uuid)?.container_menu(host)?;
    (menu.container_id(host) == Some(window)).then_some(menu)
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
    #[test]
    fn control_cells_follow_the_position() {
        assert_eq!(control_cells(27, 0), (Some(27), 35));
        assert_eq!(control_cells(27, 1), (Some(35), 27));
        assert_eq!(control_cells(27, 2), (Some(35), 27));
        assert_eq!(control_cells(27, 3), (None, 35));
    }

    use super::*;

    #[test]
    fn a_backpack_nests_only_with_an_inception_upgrade_outside_and_none_inside() {
        assert!(nests(true, false, false));
        assert!(!nests(false, false, false));
        assert!(!nests(true, true, false));
        assert!(!nests(true, false, true));
    }

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
            also: None,
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
