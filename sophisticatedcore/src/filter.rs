//! The filter every filtering upgrade carries (FilterLogic and its contents variant): a list of items under an
//! allow or block list, matched by item, by mod or by tags, optionally also by durability and by components.
//! It is kept in the upgrade's `custom_data` under the jar's keys (`filters`, `isAllowList`, `primaryMatch`,
//! `matchDurability`, `matchNbt`, `tags`, `matchAnyTag`), at the top for the plain upgrades and in a compound of
//! its own (`inputFilter`, `fuelFilter`, `outputFilter`) for the ones with several.

use mod_api::{Compound, Host, ItemStack, Tag};

use crate::contents::{
    self, Nbt, custom_data, get_bool, get_str, put, read_items, set_custom_data, write_items,
};
use crate::upgrades::{self, Kind};

/// What a stack is first compared on (PrimaryMatch).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Primary {
    Item,
    Mod,
    Tags,
}

impl Primary {
    /// PrimaryMatch.next.
    pub fn next(self) -> Primary {
        match self {
            Primary::Item => Primary::Mod,
            Primary::Mod => Primary::Tags,
            Primary::Tags => Primary::Item,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Primary::Item => "item",
            Primary::Mod => "mod",
            Primary::Tags => "tags",
        }
    }

    fn from_name(s: Option<&str>) -> Primary {
        match s {
            Some("mod") => Primary::Mod,
            Some("tags") => Primary::Tags,
            _ => Primary::Item,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Filter {
    pub items: Vec<Option<ItemStack>>,
    pub allow: bool,
    pub primary: Primary,
    /// Also match the filter item's damage.
    pub durability: bool,
    /// Also match the filter item's components (NBT in the original).
    pub components: bool,
    /// The tags a tag match looks for, as `namespace:path`.
    pub tags: Vec<String>,
    /// A tag match needs any one of the tags (otherwise all of them).
    pub any_tag: bool,
    /// The fuel filter: an empty allow list lets everything through.
    pub empty_allows_all: bool,
    /// A hopper upgrade's contents mode (`filterByStorage`): items match by what the inventory already holds.
    pub by_storage: bool,
}

fn string_list(c: &Compound, key: &str) -> Vec<String> {
    let Some(Tag::List(l)) = c.at(key) else {
        return Vec::new();
    };
    l.iter()
        .filter_map(|t| match t {
            Tag::String(s) => Some(s.to_string_lossy()),
            _ => None,
        })
        .collect()
}

/// A tag match: with `any`, one of `chosen` is among the stack's tags; otherwise every one of them is (so an empty
/// list matches, as `allTagsMatch` does).
pub fn tags_hit(any: bool, chosen: &[String], have: &[String]) -> bool {
    if any {
        chosen.iter().any(|t| have.contains(t))
    } else {
        chosen.iter().all(|t| have.contains(t))
    }
}

impl Filter {
    /// The filter under `key` (the upgrade's top when `key` is empty), `n` slots wide.
    pub fn read(host: &dyn Host, up: &ItemStack, key: &str, n: usize, allow_default: bool) -> Self {
        let d = custom_data(up);
        let sub = if key.is_empty() {
            d
        } else {
            match d.at(key) {
                Some(Tag::Compound(c)) => c.clone(),
                _ => Compound::new(),
            }
        };
        Self {
            items: read_items(host, sub.at("filters"), n),
            allow: get_bool(&sub, "isAllowList").unwrap_or(allow_default),
            primary: Primary::from_name(get_str(&sub, "primaryMatch").as_deref()),
            durability: get_bool(&sub, "matchDurability").unwrap_or(false),
            components: get_bool(&sub, "matchNbt").unwrap_or(false),
            tags: string_list(&sub, "tags"),
            any_tag: get_bool(&sub, "matchAnyTag").unwrap_or(true),
            empty_allows_all: key == "fuelFilter",
            by_storage: get_bool(&sub, "filterByStorage").unwrap_or(false),
        }
    }

    pub fn write(&self, host: &dyn Host, up: &mut ItemStack, key: &str) {
        let mut d = custom_data(up);
        let mut sub = if key.is_empty() {
            std::mem::take(&mut d)
        } else {
            match d.at(key) {
                Some(Tag::Compound(c)) => c.clone(),
                _ => Compound::new(),
            }
        };
        put(&mut sub, "filters", write_items(host, &self.items));
        put(&mut sub, "isAllowList", Tag::Byte(i8::from(self.allow)));
        put(
            &mut sub,
            "primaryMatch",
            Tag::String(self.primary.name().into()),
        );
        put(
            &mut sub,
            "matchDurability",
            Tag::Byte(i8::from(self.durability)),
        );
        put(&mut sub, "matchNbt", Tag::Byte(i8::from(self.components)));
        put(
            &mut sub,
            "tags",
            Tag::List(
                self.tags
                    .iter()
                    .map(|t| Tag::String(t.as_str().into()))
                    .collect(),
            ),
        );
        put(&mut sub, "matchAnyTag", Tag::Byte(i8::from(self.any_tag)));
        if self.by_storage {
            put(&mut sub, "filterByStorage", Tag::Byte(1));
        } else {
            sub.del("filterByStorage");
        }
        if key.is_empty() {
            d = sub;
        } else {
            put(&mut d, key, Tag::Compound(sub));
        }
        set_custom_data(up, d);
    }

    /// `stackMatchesFilter`: whether `stack` is what the listed `filter` item names, by the primary match and,
    /// when asked, by damage and components. A tag match has no item to compare, so those fall back to the item.
    fn stack_matches(&self, host: &dyn Host, filter: &ItemStack, stack: &ItemStack) -> bool {
        let ok = match self.primary {
            Primary::Mod => namespace(host, filter) == namespace(host, stack),
            _ => filter.same_item(stack),
        };
        ok && (!self.durability || filter.damage() == stack.damage())
            && (!self.components || filter.components() == stack.components())
    }

    pub fn tags_of(host: &dyn Host, stack: &ItemStack) -> Vec<String> {
        host.tags()
            .item_tags(stack.item())
            .unwrap_or_default()
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    /// FilterLogic.matchesFilter: an allow list matches what it lists (an empty one matches nothing, unless it is
    /// a fuel filter), a block list what it doesn't list (an empty one matches everything); a tags match does the
    /// same over the tags. The stand-in of an unknown item never matches.
    pub fn matches(&self, host: &dyn Host, stack: &ItemStack) -> bool {
        if contents::is_unknown(stack) {
            return false;
        }
        decide(
            self.allow,
            self.primary == Primary::Tags,
            self.empty_allows_all,
            self.items.iter().all(Option::is_none),
            || tags_hit(self.any_tag, &self.tags, &Self::tags_of(host, stack)),
            || {
                self.items
                    .iter()
                    .flatten()
                    .any(|x| self.stack_matches(host, x, stack))
            },
        )
    }

    /// ContentsFilterLogic.matchesFilter: in contents mode a stack matches when it matches one `held` in the
    /// inventory the filter watches; otherwise the list decides.
    pub fn matches_contents<'a>(
        &self,
        host: &dyn Host,
        stack: &ItemStack,
        held: impl IntoIterator<Item = &'a ItemStack>,
    ) -> bool {
        if !self.by_storage {
            return self.matches(host, stack);
        }
        if contents::is_unknown(stack) {
            return false;
        }
        held.into_iter().any(|x| self.stack_matches(host, x, stack))
    }
}

fn namespace(host: &dyn Host, s: &ItemStack) -> String {
    contents::item_id(host, s)
        .split(':')
        .next()
        .unwrap_or("")
        .to_owned()
}

/// Which way a filter upgrade filters (`direction`): what goes into the storage, what comes out, or both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Both,
    Input,
    Output,
}

impl Direction {
    /// Direction.next.
    pub fn next(self) -> Direction {
        match self {
            Direction::Both => Direction::Input,
            Direction::Input => Direction::Output,
            Direction::Output => Direction::Both,
        }
    }

    /// Whether it filters what goes in (`input`) or what comes out.
    pub fn applies(self, input: bool) -> bool {
        match self {
            Direction::Both => true,
            Direction::Input => input,
            Direction::Output => !input,
        }
    }

    pub fn read(up: &ItemStack) -> Direction {
        match get_str(&custom_data(up), "direction").as_deref() {
            Some("input") => Direction::Input,
            Some("output") => Direction::Output,
            _ => Direction::Both,
        }
    }

    pub fn write(self, up: &mut ItemStack) {
        let mut d = custom_data(up);
        let name = match self {
            Direction::Both => "both",
            Direction::Input => "input",
            Direction::Output => "output",
        };
        put(&mut d, "direction", Tag::String(name.into()));
        set_custom_data(up, d);
    }
}

/// InventoryIOHandler: whether the storage's outside (hoppers, pipes) may put `stack` in (`input`) or take it out.
/// With no filter upgrade facing that way everything passes; otherwise it must match one of them. `held` is what
/// the storage holds, for the filters in contents mode.
pub fn io_allowed<'a>(
    host: &dyn Host,
    upgrades: &[Option<ItemStack>],
    stack: &ItemStack,
    held: impl Iterator<Item = &'a ItemStack> + Clone,
    input: bool,
) -> bool {
    let mut any = false;
    for up in upgrades.iter().flatten() {
        let Some(u) = upgrades::of(host, up).filter(|u| u.kind == Kind::Filter) else {
            continue;
        };
        if !Direction::read(up).applies(input) {
            continue;
        }
        any = true;
        let f = Filter::read(host, up, "", u.filters, u.allow_default);
        if f.matches_contents(host, stack, held.clone()) {
            return true;
        }
    }
    !any
}

/// The verdict of the filter's list or tags: `listed` says whether the stack matches some listed item, `tagged`
/// whether it passes the tag match.
fn decide(
    allow: bool,
    by_tags: bool,
    empty_allows_all: bool,
    empty: bool,
    tagged: impl FnOnce() -> bool,
    listed: impl FnOnce() -> bool,
) -> bool {
    match (allow, by_tags) {
        (true, true) => tagged(),
        (false, true) => !tagged(),
        (true, false) => (empty && empty_allows_all) || listed(),
        (false, false) => empty || !listed(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn any_and_all_tags() {
        let have = tags(&["minecraft:logs", "minecraft:planks"]);
        assert!(tags_hit(true, &tags(&["minecraft:logs", "c:ores"]), &have));
        assert!(!tags_hit(
            false,
            &tags(&["minecraft:logs", "c:ores"]),
            &have
        ));
        assert!(tags_hit(
            false,
            &tags(&["minecraft:logs", "minecraft:planks"]),
            &have
        ));
        assert!(!tags_hit(true, &tags(&["c:ores"]), &have));
        // No tags chosen: "all" holds, "any" doesn't.
        assert!(tags_hit(false, &[], &have));
        assert!(!tags_hit(true, &[], &have));
    }

    #[test]
    fn lists_and_tags_decide_as_the_original() {
        let go = |allow, by_tags, fuel, empty, tagged, listed| {
            decide(allow, by_tags, fuel, empty, || tagged, || listed)
        };
        // Allow list of items: only what's listed; an empty one nothing, but a fuel filter everything.
        assert!(go(true, false, false, false, false, true));
        assert!(!go(true, false, false, false, false, false));
        assert!(!go(true, false, false, true, false, false));
        assert!(go(true, false, true, true, false, false));
        // Block list of items: what isn't listed; an empty one everything.
        assert!(!go(false, false, false, false, false, true));
        assert!(go(false, false, false, false, false, false));
        assert!(go(false, false, false, true, false, false));
        // Tags ignore the list.
        assert!(go(true, true, false, true, true, false));
        assert!(!go(false, true, false, true, true, true));
        assert!(go(false, true, false, false, false, true));
    }

    #[test]
    fn a_direction_covers_its_ways() {
        assert!(Direction::Both.applies(true) && Direction::Both.applies(false));
        assert!(Direction::Input.applies(true) && !Direction::Input.applies(false));
        assert!(!Direction::Output.applies(true) && Direction::Output.applies(false));
        assert_eq!(Direction::Both.next(), Direction::Input);
        assert_eq!(Direction::Input.next(), Direction::Output);
        assert_eq!(Direction::Output.next(), Direction::Both);
    }

    #[test]
    fn the_primary_match_cycles_item_mod_tags() {
        let mut p = Primary::Item;
        let mut seen = Vec::new();
        for _ in 0..3 {
            p = p.next();
            seen.push(p);
        }
        assert_eq!(seen, [Primary::Mod, Primary::Tags, Primary::Item]);
        assert_eq!(Primary::from_name(Some("tags")), Primary::Tags);
        assert_eq!(Primary::from_name(None), Primary::Item);
    }
}
