// Grind Enchantments 3.1.4 (mschae23) for Horizon: DisenchantOperation and MoveOperation with
// Homestead's config (allow_removing_curses true, consume_enchanted_item false, the cost
// functions below). Injected as `crate::game::stations::grindenchantments`: the operations with the
// game's item and enchantment types, the grindstone dispatcher halves and the screen's cost line.
use super::{Game, Kind, argb};
use horizon_client::{enchant::Enchantments, hud::HudBatch, item_data, items::ItemStack};

const BOOK: &str = "minecraft:book";
const ENCHANTED_BOOK: &str = "minecraft:enchanted_book";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// The enchanted item is in this input slot, the book in the other.
    Disenchant(usize),
    /// Slot 0 an enchanted book, slot 1 a book or an enchanted book.
    Move,
}

fn is(stack: Option<&ItemStack>, item: &str) -> bool {
    stack.is_some_and(|s| &*s.item == item)
}

/// `ItemStack.hasEnchantments` (1.20.1): the `Enchantments` list, which an enchanted book lacks.
fn enchanted(stack: Option<&ItemStack>) -> bool {
    stack.is_some_and(|s| !s.enchantments().is_empty())
}

/// `isDisenchantOperation`, then `isMoveOperation`.
pub fn op(input: Option<&ItemStack>, additional: Option<&ItemStack>) -> Option<Op> {
    if enchanted(input) && is(additional, BOOK) {
        return Some(Op::Disenchant(0));
    }
    if enchanted(additional) && is(input, BOOK) {
        return Some(Op::Disenchant(1));
    }
    if is(input, ENCHANTED_BOOK) && (is(additional, ENCHANTED_BOOK) || is(additional, BOOK)) {
        return Some(Op::Move);
    }
    None
}

fn slots<'a>(op: Op, input: Option<&'a ItemStack>, additional: Option<&'a ItemStack>) -> (Option<&'a ItemStack>, Option<&'a ItemStack>) {
    match op {
        Op::Disenchant(1) => (additional, input),
        _ => (input, additional),
    }
}

fn enchanted_book(levels: item_data::Levels) -> ItemStack {
    let mut book = ItemStack::new(ENCHANTED_BOOK, 1);
    book.set_crafting_enchantments(levels);
    book.tidy();
    book
}

/// `onUpdateResult`: Some(result) when one of the operations applies (None inside: no result).
pub fn result(e: &Enchantments, input: Option<&ItemStack>, additional: Option<&ItemStack>) -> Option<Option<ItemStack>> {
    let op = op(input, additional)?;
    let (from, to) = slots(op, input, additional);
    let (from, to) = (from?, to?);
    Some(match op {
        // transferEnchantmentsToBook.
        Op::Disenchant(_) => {
            let levels = from.enchantments().to_vec();
            (!levels.is_empty()).then(|| enchanted_book(levels))
        }
        Op::Move => {
            let list = from.stored_enchantments();
            if list.len() < 2 {
                return Some(None);
            }
            let (id, level) = list[0].clone();
            if &*to.item == ENCHANTED_BOOK {
                let mut level = level;
                if let Some(&(_, has)) = to.stored_enchantments().iter().find(|(x, _)| *x == id) {
                    let max = e.get(&id).map_or(1, |d| d.max_level);
                    if has != level || level == max {
                        return Some(None);
                    }
                    level = has + 1;
                }
                let mut out = to.clone();
                let mut stored = out.stored_enchantments().to_vec();
                item_data::upgrade(&mut stored, &id, level);
                out.set_crafting_enchantments(stored);
                Some(out)
            } else {
                Some(enchanted_book(vec![(id, level)]))
            }
        }
    })
}

/// `getLevelCost`: `ceil` of the operation's cost function.
pub fn cost(e: &Enchantments, input: Option<&ItemStack>, additional: Option<&ItemStack>) -> Option<i32> {
    let op = op(input, additional)?;
    let from = slots(op, input, additional).0?;
    let raw = match op {
        // transform(count_min_power, factor 0.3, offset 8).
        Op::Disenchant(_) => {
            let power: f64 =
                from.enchantments().iter().map(|(id, l)| f64::from(e.get(id).map_or(0, |d| d.min_cost.calculate(*l)))).sum();
            power * 0.3 + 8.0
        }
        // transform(first_enchantment(count_levels(normal 3, treasure 8)), factor 0.5, offset 0.5).
        Op::Move => {
            let first = from.stored_enchantments().first().map_or(1.0, |(id, l)| {
                let factor = if e.in_tag("minecraft:treasure", id) { 8.0 } else { 3.0 };
                f64::from(*l) * factor
            });
            first * 0.5 + 0.5
        }
    };
    Some(raw.ceil() as i32)
}

/// `onTakeResult`'s new inputs (slot 0, slot 1).
pub fn new_inputs(input: Option<&ItemStack>, additional: Option<&ItemStack>) -> Option<[Option<ItemStack>; 2]> {
    let op = op(input, additional)?;
    let (from, to) = slots(op, input, additional);
    let (from, to) = (from?, to?);
    let one_less = |s: &ItemStack| (s.count > 1).then(|| ItemStack { count: s.count - 1, ..s.clone() });
    Some(match op {
        Op::Disenchant(slot) => {
            // grind: every enchantment off (curses too), repair cost 0.
            let mut item = from.clone();
            item.set_crafting_enchantments(Vec::new());
            item.data_mut().repair_cost = 0;
            item.tidy();
            let book = one_less(to);
            if slot == 0 { [Some(item), book] } else { [book, Some(item)] }
        }
        Op::Move => {
            let rest = from.stored_enchantments()[1..].to_vec();
            let book = if &*to.item == ENCHANTED_BOOK { None } else { one_less(to) };
            [Some(enchanted_book(rest)), book]
        }
    })
}

/// `api_grindstone_place`'s head: the operation's result, no experience, and its level cost noted
/// for the mod's `grindstone_place` hook (which hides it from a player who can't pay).
pub(crate) fn place(game: &mut Game<'_>, player: u128, value: &mut crate::menu::GrindstonePlace) {
    let e = std::sync::Arc::clone(game.item_enchantments());
    let (a, b) = (value.top.as_ref(), value.bottom.as_ref());
    let Some(result) = result(&e, a, b) else {
        mod_grindenchantments::note_cost(player, None);
        return;
    };
    let cost = result.as_ref().and_then(|_| cost(&e, a, b));
    mod_grindenchantments::note_cost(player, cost);
    value.output = result;
    value.xp = 0;
}

/// `api_grindstone_take`'s head: the inputs as the operation leaves them, no experience; the cost
/// noted for the `grindstone_take` hook, which takes the levels.
pub(crate) fn take(game: &mut Game<'_>, player: u128, value: &mut crate::menu::GrindstoneTake) {
    let e = std::sync::Arc::clone(game.item_enchantments());
    let (a, b) = (value.top.as_ref(), value.bottom.as_ref());
    let (Some(cost), Some([new_a, new_b])) = (cost(&e, a, b), new_inputs(a, b)) else {
        mod_grindenchantments::note_taken(player, None);
        return;
    };
    mod_grindenchantments::note_taken(player, Some(cost));
    value.new_top = new_a;
    value.new_bottom = new_b;
    value.xp = 0;
}

thread_local! {
    static DRAWING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// GrindstoneScreenMixin: the station drawn as always (draw_station re-entered behind a guard),
/// then "Enchantment Cost: N" where the anvil puts its cost, green, or red when the player can't
/// pay.
pub(crate) fn draw(game: &mut Game<'_>, batch: &mut HudBatch) -> Option<bool> {
    if DRAWING.with(|d| d.get()) {
        return None;
    }
    DRAWING.with(|d| d.set(true));
    let shown = game.draw_station(batch);
    DRAWING.with(|d| d.set(false));
    let e = std::sync::Arc::clone(game.item_enchantments());
    let cost = game.station.as_ref().and_then(|s| {
        if s.kind != Kind::Grindstone || s.result.is_none() {
            return None;
        }
        cost(&e, s.inputs[0].as_ref(), s.inputs[1].as_ref())
    });
    let Some(cost) = cost.filter(|_| shown) else {
        return Some(shown);
    };
    let color = if game.creative || game.experience.level >= cost { argb(-8323296) } else { argb(-40864) };
    let line = game.translate_with_fallback("container.repair.cost", "Enchantment Cost: %1$s").replace("%1$s", &cost.to_string());
    let (ox, oy) = game.inventory_origin(batch);
    let atlas = &game.gui.atlas;
    let tw = HudBatch::text_width(atlas, &line);
    let tx = 176.0 - 8.0 - tw - 2.0;
    batch.fill(atlas, ox + tx - 2.0, oy + 67.0, 176.0 - 8.0 - (tx - 2.0), 12.0, argb(1325400064));
    batch.text(atlas, &line, ox + tx, oy + 69.0, color, true);
    Some(shown)
}
