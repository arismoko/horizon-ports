//! The tool swapper upgrade (ToolSwapperUpgradeWrapper): while a player starts mining a block, the best tool the
//! backpack holds for it goes into the hand and the tool that was there goes into the backpack; while attacking, the
//! best sword (else axe) does. A filter limits what may be swapped in. The advanced upgrade has a settings tab:
//! the swap mode (any held item, only tools, no swapping) and whether weapons are swapped.
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use mod_api::{Host, HostBlockStates, ItemStack, LevelRef, PlayerRef, ServerHost, Tag};

use crate::contents::{self, Nbt, custom_data, get_str, put, set_custom_data};
use crate::filter::Filter;
use crate::places;
use crate::storage_items;
use crate::ticking::{self, Carried};
use crate::upgrades::{self, Kind, Upgrade};

/// `toolSwapMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Any,
    OnlyTools,
    NoSwap,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Any => "name",
            Mode::OnlyTools => "onlyTools",
            Mode::NoSwap => "noSwap",
        }
    }

    pub fn next(self) -> Mode {
        match self {
            Mode::Any => Mode::OnlyTools,
            Mode::OnlyTools => Mode::NoSwap,
            Mode::NoSwap => Mode::Any,
        }
    }

    pub fn read(up: &ItemStack) -> Mode {
        match get_str(&custom_data(up), "toolSwapMode").as_deref() {
            Some("onlyTools") => Mode::OnlyTools,
            Some("noSwap") => Mode::NoSwap,
            _ => Mode::Any,
        }
    }

    pub fn write(self, up: &mut ItemStack) {
        let mut d = custom_data(up);
        put(&mut d, "toolSwapMode", Tag::String(self.name().into()));
        set_custom_data(up, d);
    }
}

/// `shouldSwapWeapon`, on unless the settings turned it off.
pub fn swaps_weapons(up: &ItemStack) -> bool {
    upgrades::flag(up, "shouldSwapWeapon", true)
}

/// An attribute modifier's way of combining (`AttributeModifier.Operation`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Add,
    MultiplyBase,
    MultiplyTotal,
}

/// Attack damage the attribute starts from (`Attributes.ATTACK_DAMAGE`'s default).
const BASE_DAMAGE: f64 = 2.0;

/// The attribute's value with `mods` applied (`AttributeInstance.calculateValue`).
pub fn damage(mods: &[(f64, Op)]) -> f64 {
    let add: f64 = mods.iter().filter(|m| m.1 == Op::Add).map(|m| m.0).sum();
    let base = BASE_DAMAGE + add;
    let mut v = base;
    for m in mods.iter().filter(|m| m.1 == Op::MultiplyBase) {
        v += base * m.0;
    }
    for m in mods.iter().filter(|m| m.1 == Op::MultiplyTotal) {
        v *= 1.0 + m.0;
    }
    v
}

/// The faster tool: the index of the first whose speed beats `best` and every earlier candidate's.
pub fn best_tool(best: f64, speeds: &[Option<f64>]) -> Option<usize> {
    let mut at = None;
    let mut top = best;
    for (i, s) in speeds.iter().enumerate() {
        if let Some(s) = s
            && *s > top
        {
            top = *s;
            at = Some(i);
        }
    }
    at
}

/// What kind of weapon a stack is, with its damage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Weapon {
    Axe(f64),
    Sword(f64),
}

/// The best sword among `weapons` (by index), else the best axe (`tryToSwapInWeapon`); earlier ones win ties.
pub fn best_weapon(weapons: &[Option<Weapon>]) -> Option<usize> {
    let mut sword: Option<(usize, f64)> = None;
    let mut axe: Option<(usize, f64)> = None;
    for (i, w) in weapons.iter().enumerate() {
        match w {
            Some(Weapon::Sword(d)) if sword.is_none_or(|s| *d > s.1) => sword = Some((i, *d)),
            Some(Weapon::Axe(d)) if axe.is_none_or(|a| *d > a.1) => axe = Some((i, *d)),
            _ => {}
        }
    }
    sword.or(axe).map(|s| s.0)
}

// ---- the game's side ----

/// The block each player mined last with a swap considered (`lastMinedBlock`).
static LAST_MINED: LazyLock<Mutex<HashMap<u128, String>>> = LazyLock::new(Mutex::default);

const TOOL_TAGS: [&str; 5] = [
    "minecraft:pickaxes",
    "minecraft:axes",
    "minecraft:shovels",
    "minecraft:hoes",
    "c:tools/shear",
];

fn has_tag(host: &dyn Host, stack: &ItemStack, tags: &[&str]) -> bool {
    let have = Filter::tags_of(host, stack);
    tags.iter().any(|t| have.iter().any(|h| h == t))
}

fn is_sword(host: &dyn Host, stack: &ItemStack) -> bool {
    has_tag(host, stack, &["minecraft:swords"])
}

fn is_tool(host: &dyn Host, stack: &ItemStack) -> bool {
    has_tag(host, stack, &TOOL_TAGS)
}

fn number(t: &Tag) -> Option<f64> {
    match t {
        Tag::Double(v) => Some(f64::from_bits(*v)),
        Tag::Float(v) => Some(f64::from(f32::from_bits(*v))),
        Tag::Int(v) => Some(f64::from(*v)),
        Tag::Long(v) => Some(*v as f64),
        Tag::Short(v) => Some(f64::from(*v)),
        Tag::Byte(v) => Some(f64::from(*v)),
        _ => None,
    }
}

/// The attack damage modifiers `stack` has in the main hand: its own `attribute_modifiers`, else its item's.
fn attack_mods(host: &dyn Host, stack: &ItemStack) -> Vec<(f64, Op)> {
    let own = stack
        .components()
        .and_then(|c| c.as_nbt().at("minecraft:attribute_modifiers").cloned());
    let Some(tag) =
        own.or_else(|| host.item_default_component(stack.item(), "minecraft:attribute_modifiers"))
    else {
        return Vec::new();
    };
    let list = match tag {
        Tag::List(l) => l,
        Tag::Compound(c) => match c.at("modifiers") {
            Some(Tag::List(l)) => l.clone(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    list.iter()
        .filter_map(|e| {
            let Tag::Compound(e) = e else { return None };
            let kind = get_str(e, "type")?;
            if kind.rsplit(':').next() != Some("attack_damage") {
                return None;
            }
            let slot = get_str(e, "slot").unwrap_or_else(|| "any".into());
            if !matches!(slot.as_str(), "any" | "mainhand" | "hand") {
                return None;
            }
            let amount = number(e.at("amount")?)?;
            let op = match get_str(e, "operation").as_deref() {
                Some("add_multiplied_base") => Op::MultiplyBase,
                Some("add_multiplied_total") => Op::MultiplyTotal,
                _ => Op::Add,
            };
            Some((amount, op))
        })
        .collect()
}

fn weapon(host: &dyn Host, stack: &ItemStack) -> Option<Weapon> {
    let mods = attack_mods(host, stack);
    if mods.is_empty() {
        return None;
    }
    let d = damage(&mods);
    if has_tag(host, stack, &["minecraft:axes"]) {
        Some(Weapon::Axe(d))
    } else if is_sword(host, stack) {
        Some(Weapon::Sword(d))
    } else {
        None
    }
}

/// The tool swapper upgrades of a carried storage.
fn swappers(host: &dyn Host, c: &Carried) -> Vec<(ItemStack, Upgrade)> {
    c.ups(host)
        .into_iter()
        .filter(|(_, k)| k.kind == Kind::ToolSwapper)
        .collect()
}

/// Puts `tool` (one of it) in the hand and the hand's stack into the storage, if the storage has room for it.
fn exchange(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    c: &mut Carried,
    hand: u32,
    main: &ItemStack,
    tool: &ItemStack,
) -> bool {
    let Some(one) = contents::with_count(tool, 1) else {
        return false;
    };
    let mut trial = c.contents.clone();
    if trial.remove(&*host, c.kind, &one, 1) != 1
        || trial.add(&*host, c.kind, main, main.count()) != main.count()
    {
        return false;
    }
    if !places::set_slot(host, player, hand, Some(one)) {
        return false;
    }
    c.contents = trial;
    c.contents_changed = true;
    true
}

/// `onBlockClick`: a player starts mining the block at `pos`. Whether an upgrade took it.
fn swap_for_block(
    host: &mut dyn ServerHost,
    at: (PlayerRef, LevelRef, [i32; 3]),
    c: &mut Carried,
    up: &ItemStack,
    k: &Upgrade,
) -> bool {
    let (player, level, pos) = at;
    let uuid = player.uuid();
    let mode = if k.advanced {
        Mode::read(up)
    } else {
        Mode::Any
    };
    if mode == Mode::NoSwap {
        return false;
    }
    let hand = player.selected_slot(&*host).unwrap_or(0) as u32;
    let inv = places::inventory(host, player);
    let Some(main) = inv.get(hand as usize).cloned().flatten() else {
        return false;
    };
    let id = contents::item_id(&*host, &main);
    let sword = is_sword(&*host, &main);
    if storage_items::by_item(&id).is_some()
        || (mode == Mode::OnlyTools && sword)
        || (!sword && !is_tool(&*host, &main))
        || !upgrades::matches(&*host, up, k, &main)
    {
        return false;
    }
    let Some(state) = level.block_state(&*host, pos) else {
        return false;
    };
    let block = crate::storage::block_id(&*host, level, pos);
    let good = |host: &dyn Host, stack: &ItemStack| {
        host.block_states().is_correct_tool_for_drops(stack, state) == Some(true)
            && level
                .block_break_speed(host, pos, Some(stack))
                .is_some_and(|s| s > 1.5)
    };
    let speed = |host: &dyn Host, stack: &ItemStack| {
        f64::from(
            level
                .block_break_speed(host, pos, Some(stack))
                .unwrap_or(1.0),
        )
    };
    let mut main_speed = 0.0;
    let mut last = LAST_MINED.lock().unwrap_or_else(|e| e.into_inner());
    if good(&*host, &main) {
        if last.get(&uuid) == Some(&block) {
            return true;
        }
        main_speed = speed(&*host, &main);
    }
    last.insert(uuid, block);
    drop(last);
    let held: Vec<ItemStack> = c
        .contents
        .visible(&*host, c.kind)
        .into_iter()
        .flatten()
        .collect();
    let speeds: Vec<Option<f64>> = held
        .iter()
        .map(|s| (upgrades::matches(&*host, up, k, s) && good(&*host, s)).then(|| speed(&*host, s)))
        .collect();
    let Some(at) = best_tool(main_speed, &speeds) else {
        return false;
    };
    exchange(host, player, c, hand, &main, &held[at])
}

/// `onAttackEntity`: a player attacks. Whether an upgrade took it.
fn swap_for_attack(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    c: &mut Carried,
    up: &ItemStack,
    k: &Upgrade,
) -> bool {
    if !swaps_weapons(up) {
        return false;
    }
    let hand = player.selected_slot(&*host).unwrap_or(0) as u32;
    let inv = places::inventory(host, player);
    let Some(main) = inv.get(hand as usize).cloned().flatten() else {
        return false;
    };
    if is_sword(&*host, &main) {
        return true;
    }
    if storage_items::by_item(&contents::item_id(&*host, &main)).is_some()
        || !is_tool(&*host, &main)
        || !upgrades::matches(&*host, up, k, &main)
    {
        return false;
    }
    let held: Vec<ItemStack> = c
        .contents
        .visible(&*host, c.kind)
        .into_iter()
        .flatten()
        .filter(|s| upgrades::matches(&*host, up, k, s))
        .collect();
    let mut all = vec![main.clone()];
    all.extend(held);
    let found: Vec<Option<Weapon>> = all.iter().map(|s| weapon(&*host, s)).collect();
    match best_weapon(&found) {
        Some(0) => true,
        Some(i) => exchange(host, player, c, hand, &main, &all[i]),
        None => false,
    }
}

/// A player starts mining the block at `pos`: the first carried tool swapper that swaps does.
pub fn on_block_click(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    level: LevelRef,
    pos: [i32; 3],
) {
    let uuid = player.uuid();
    if places::creative(&*host, uuid) {
        return;
    }
    for mut root in ticking::carried(host, uuid) {
        let swapped = root.visit(&mut |c| {
            swappers(&*host, c)
                .into_iter()
                .any(|(up, k)| swap_for_block(host, (player, level, pos), c, &up, &k))
        });
        if swapped {
            root.flush(host, uuid);
            return;
        }
    }
}

/// A player attacks an entity: the first carried tool swapper that swaps a weapon in does.
pub fn on_attack(host: &mut dyn ServerHost, player: PlayerRef) {
    let uuid = player.uuid();
    if places::creative(&*host, uuid) {
        return;
    }
    for mut root in ticking::carried(host, uuid) {
        let swapped = root.visit(&mut |c| {
            swappers(&*host, c)
                .into_iter()
                .any(|(up, k)| swap_for_attack(host, player, c, &up, &k))
        });
        if swapped {
            root.flush(host, uuid);
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_cycle_and_round_trip_their_names() {
        assert_eq!(Mode::Any.next(), Mode::OnlyTools);
        assert_eq!(Mode::OnlyTools.next(), Mode::NoSwap);
        assert_eq!(Mode::NoSwap.next(), Mode::Any);
        assert_eq!(Mode::Any.name(), "name");
    }

    #[test]
    fn damage_adds_then_scales() {
        assert_eq!(damage(&[]), 2.0);
        assert_eq!(damage(&[(5.0, Op::Add)]), 7.0);
        // 2 + 4 = 6; +50% of that base; then x1.1 over the lot.
        let d = damage(&[
            (4.0, Op::Add),
            (0.5, Op::MultiplyBase),
            (0.1, Op::MultiplyTotal),
        ]);
        assert!((d - 9.9).abs() < 1e-9);
    }

    #[test]
    fn a_tool_is_swapped_in_only_when_faster_than_the_hand() {
        assert_eq!(
            best_tool(0.0, &[None, Some(4.0), Some(8.0), Some(6.0)]),
            Some(2)
        );
        assert_eq!(best_tool(8.0, &[Some(8.0), Some(6.0)]), None);
        assert_eq!(best_tool(0.0, &[None, None]), None);
        // The first of equally fast tools wins.
        assert_eq!(best_tool(1.0, &[Some(4.0), Some(4.0)]), Some(0));
    }

    #[test]
    fn a_sword_beats_an_axe_whatever_their_damage() {
        let w = [
            Some(Weapon::Axe(10.0)),
            Some(Weapon::Sword(6.0)),
            Some(Weapon::Sword(8.0)),
            None,
        ];
        assert_eq!(best_weapon(&w), Some(2));
        assert_eq!(
            best_weapon(&[Some(Weapon::Axe(3.0)), Some(Weapon::Axe(9.0))]),
            Some(1)
        );
        assert_eq!(best_weapon(&[None]), None);
    }
}
