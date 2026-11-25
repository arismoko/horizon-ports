//! Pick-up notification entries, combining/merging rules, expiry, layout math, and client-side diffing.

use crate::config::{ClientConfig, CombineEntries};
use mod_api::text::TextStyle;
use mod_api::{ClientHost, ItemKey, ItemStack, JavaString, Tag, Text};
use std::collections::HashMap;

pub const ELEMENT_HEIGHT: i32 = 18;
pub const TEXT_ITEM_MARGIN: i32 = 4;
pub const ITEM_STACK_POP_TIME: i32 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemRarity {
    Common,
    Uncommon,
    Rare,
    Epic,
}

impl ItemRarity {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "uncommon" => Self::Uncommon,
            "rare" => Self::Rare,
            "epic" => Self::Epic,
            _ => Self::Common,
        }
    }

    /// Vanilla `ItemStack.getRarity()` enchantment rarity upgrade.
    pub fn with_enchantment(self, enchanted: bool) -> Self {
        if !enchanted {
            return self;
        }
        match self {
            Self::Common | Self::Uncommon => Self::Rare,
            Self::Rare | Self::Epic => Self::Epic,
        }
    }

    pub fn style_color(self) -> &'static str {
        match self {
            Self::Common => "white",
            Self::Uncommon => "yellow",
            Self::Rare => "aqua",
            Self::Epic => "light_purple",
        }
    }
}

#[derive(Clone, Debug)]
pub enum EntryKind {
    Item {
        stack: ItemStack,
        item_key: ItemKey,
        hover_name: Text,
        base_name: Text,
        enchanted: bool,
        pop_time: i32,
        inventory_count: u32,
    },
    Experience {
        name: Text,
        tick_count: i32,
    },
}

#[derive(Clone, Debug)]
pub struct DisplayEntry {
    pub kind: EntryKind,
    pub amount: u32,
    pub rarity: ItemRarity,
    pub remaining_ticks: i32,
}

impl DisplayEntry {
    pub fn new_item(
        stack: ItemStack,
        amount: u32,
        hover_name: Text,
        base_name: Text,
        rarity: ItemRarity,
        enchanted: bool,
        inventory_count: u32,
        display_time: i32,
    ) -> Self {
        let item_key = stack.item();
        Self {
            kind: EntryKind::Item {
                stack,
                item_key,
                hover_name,
                base_name,
                enchanted,
                pop_time: ITEM_STACK_POP_TIME,
                inventory_count,
            },
            amount,
            rarity,
            remaining_ticks: display_time,
        }
    }

    pub fn new_experience(name: Text, amount: u32, tick_count: i32, display_time: i32) -> Self {
        Self {
            kind: EntryKind::Experience { name, tick_count },
            amount,
            rarity: ItemRarity::Uncommon,
            remaining_ticks: display_time,
        }
    }

    pub fn may_discard(&self) -> bool {
        self.remaining_ticks <= 0
    }

    pub fn tick(&mut self) {
        if self.remaining_ticks > 0 {
            self.remaining_ticks -= 1;
        }
        match &mut self.kind {
            EntryKind::Item { pop_time, .. } => {
                if *pop_time > 0 {
                    *pop_time -= 1;
                }
            }
            EntryKind::Experience { tick_count, .. } => {
                *tick_count = tick_count.wrapping_add(1);
            }
        }
    }

    pub fn can_combine_with(&self, other: &Self, mode: CombineEntries) -> bool {
        if mode == CombineEntries::Never {
            return false;
        }
        match (&self.kind, &other.kind) {
            (
                EntryKind::Item {
                    item_key: k1,
                    hover_name: n1,
                    enchanted: e1,
                    ..
                },
                EntryKind::Item {
                    item_key: k2,
                    hover_name: n2,
                    enchanted: e2,
                    ..
                },
            ) => {
                if k1 != k2 || self.rarity != other.rarity || e1 != e2 {
                    return false;
                }
                match mode {
                    CombineEntries::Always => true,
                    CombineEntries::ExcludeNamed => n1 == n2,
                    CombineEntries::Never => false,
                }
            }
            (EntryKind::Experience { .. }, EntryKind::Experience { .. }) => true,
            _ => false,
        }
    }

    pub fn merged_with(mut self, old: &Self, display_time: i32) -> Self {
        self.amount = self.amount.saturating_add(old.amount);
        self.remaining_ticks = display_time;
        match (&mut self.kind, &old.kind) {
            (EntryKind::Item { pop_time, .. }, _) => {
                *pop_time = ITEM_STACK_POP_TIME;
            }
            (
                EntryKind::Experience { tick_count, .. },
                EntryKind::Experience {
                    tick_count: old_ticks,
                    ..
                },
            ) => {
                *tick_count = *old_ticks;
            }
            _ => {}
        }
        self
    }

    pub fn relative_remaining_ticks(&self, partial_tick: f32, move_time: i32) -> f32 {
        if move_time <= 0 {
            return 1.0;
        }
        ((self.remaining_ticks as f32 - partial_tick) / move_time as f32).clamp(0.0, 1.0)
    }

    pub fn build_text(&self, config: &ClientConfig) -> Text {
        let reverse = config.display.screen_corner.is_right();
        let mut parts: Vec<Text> = Vec::with_capacity(3);

        if let EntryKind::Item {
            inventory_count, ..
        } = &self.kind
            && config.display.inventory_count
        {
            parts.push(Text::literal(format!("({inventory_count})")));
        }

        if config.display.display_item_name {
            let name = match &self.kind {
                EntryKind::Item {
                    hover_name,
                    base_name,
                    ..
                } => {
                    if config.behavior.combine_entries == CombineEntries::Always {
                        base_name.clone()
                    } else {
                        hover_name.clone()
                    }
                }
                EntryKind::Experience { name, .. } => name.clone(),
            };
            parts.push(name);
        }

        if config.display.display_amount.is_text()
            && (self.amount > 1 || (self.amount == 1 && config.display.display_single_count))
        {
            let count_str = if reverse {
                format!("{}x", self.amount)
            } else {
                format!("x{}", self.amount)
            };
            parts.push(Text::literal(count_str));
        }

        if reverse {
            parts.reverse();
        }

        let color_name = if !config.display.ignore_rarity && self.rarity != ItemRarity::Common {
            self.rarity.style_color()
        } else {
            config.display.default_text_color.style_name()
        };

        let mut style = TextStyle::default();
        style.color = Some(color_name.to_owned());
        let mut root = Text::literal("").with_style(style);

        for (i, part) in parts.into_iter().enumerate() {
            if i > 0 {
                root = root.append(Text::literal(" "));
            }
            root = root.append(part);
        }

        root
    }

    pub fn entry_width(&self, host: &dyn ClientHost, config: &ClientConfig) -> i32 {
        let text = self.build_text(config);
        let text_width = host.text_width(&text);
        if config.display.draw_sprite {
            text_width + if text_width == 0 { 0 } else { TEXT_ITEM_MARGIN } + 16
        } else {
            text_width
        }
    }
}

/// Ordered entry list (oldest first, newest last), mirroring Java's `LinkedHashMap`.
#[derive(Default)]
pub struct EntryCollector {
    pub entries: Vec<DisplayEntry>,
}

impl EntryCollector {
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn push(&mut self, mut entry: DisplayEntry, combine: CombineEntries, max_size: usize) {
        let display_time = entry.remaining_ticks;
        if let Some(pos) = self
            .entries
            .iter()
            .position(|existing| entry.can_combine_with(existing, combine))
        {
            let old = self.entries.remove(pos);
            entry = entry.merged_with(&old, display_time);
        }

        let limit = max_size.max(1);
        while !self.entries.is_empty() && self.entries.len() >= limit {
            self.entries.remove(0);
        }

        self.entries.push(entry);
    }

    pub fn tick(&mut self, display_time: i32) {
        if self.entries.is_empty() {
            return;
        }
        for entry in &mut self.entries {
            entry.tick();
        }
        if display_time != 0 {
            self.entries.retain(|e| !e.may_discard());
        }
    }
}

/// Computes `max_size` matching `AddEntriesHandler.addEntry`.
pub fn max_entries_for_height(gui_height: f32, display_scale: f32, max_height: f64) -> usize {
    if display_scale <= 0.0 || !display_scale.is_finite() || max_height <= 0.0 {
        return 1;
    }
    let scaled_height = (gui_height / display_scale) as i32;
    let raw = ((scaled_height as f64) * max_height / ELEMENT_HEIGHT as f64) as i32 - 1;
    raw.max(1) as usize
}

/// Computes the vertical slide-out offset matching `DrawEntriesHandler.getMoveOffset`.
pub fn vertical_move_offset(entries: &[&DisplayEntry], partial_tick: f32, move_time: i32) -> i32 {
    if entries.is_empty() || move_time <= 0 {
        return 0;
    }
    let sum: f64 = entries
        .iter()
        .map(|e| 1.0 - f64::from(e.relative_remaining_ticks(partial_tick, move_time)))
        .sum();
    (sum * ELEMENT_HEIGHT as f64) as i32
}

/// Measures the fraction of `[elem_x, elem_y, elem_w, ELEMENT_HEIGHT]` inside `orig_rect`.
pub fn covered_area_alpha(
    elem_x: i32,
    elem_y: i32,
    elem_w: i32,
    orig_x: i32,
    orig_y: i32,
    orig_w: i32,
    orig_h: i32,
) -> f32 {
    let elem_area = elem_w.saturating_mul(ELEMENT_HEIGHT);
    if elem_area <= 0 {
        return 1.0;
    }
    let x0 = elem_x.max(orig_x);
    let y0 = elem_y.max(orig_y);
    let x1 = (elem_x + elem_w).min(orig_x + orig_w);
    let y1 = (elem_y + ELEMENT_HEIGHT).min(orig_y + orig_h);
    let inter_w = (x1 - x0).max(0);
    let inter_h = (y1 - y0).max(0);
    let inter_area = inter_w.saturating_mul(inter_h);
    (inter_area as f32 / elem_area as f32).clamp(0.0, 1.0)
}

/// Shortens large sprite counts (`1_000` -> `1K`, `1_000_000` -> `1M`, `1_000_000_000` -> `1B`).
pub fn shorten_value(value: u32) -> String {
    if value >= 1_000_000_000 {
        format!("{}B", value / 1_000_000_000)
    } else if value >= 1_000_000 {
        format!("{}M", value / 1_000_000)
    } else if value >= 1_000 {
        format!("{}K", value / 1_000)
    } else {
        value.to_string()
    }
}

/// Vanilla `ExperienceOrb.getIcon()` texture tier (0..=10).
pub fn xp_texture_index(display_count: u32) -> u32 {
    if display_count >= 2477 {
        10
    } else if display_count >= 1237 {
        9
    } else if display_count >= 617 {
        8
    } else if display_count >= 307 {
        7
    } else if display_count >= 149 {
        6
    } else if display_count >= 73 {
        5
    } else if display_count >= 37 {
        4
    } else if display_count >= 17 {
        3
    } else if display_count >= 7 {
        2
    } else if display_count >= 3 {
        1
    } else {
        0
    }
}

/// Vanilla `ExperienceOrbRenderer` oscillating orb tint (packed ARGB `0xAARRGGBB`).
pub fn xp_orb_color(age_in_ticks: f32, alpha: f32) -> u32 {
    let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
    let r = (((age_in_ticks.sin() + 1.0) * 0.5).clamp(0.0, 1.0) * 255.0).round() as u32;
    let g = 255u32;
    let b = (((age_in_ticks + std::f32::consts::PI * 4.0 / 3.0).sin() + 1.0) * 0.1).clamp(0.0, 1.0);
    let b = (b * 255.0).round() as u32;
    (a << 24) | (r << 16) | (g << 8) | b
}

/// Determines whether `stack` has an enchantment glint (`ItemStack.isEnchanted`).
pub fn is_stack_enchanted(host: &dyn ClientHost, stack: &ItemStack) -> bool {
    if let Some(comps) = stack.components() {
        let nbt = comps.as_nbt();
        if let Some(Tag::Byte(b)) =
            nbt.get(&JavaString::from("minecraft:enchantment_glint_override"))
        {
            return *b != 0;
        }
        if let Some(Tag::Compound(c)) = nbt.get(&JavaString::from("minecraft:enchantments"))
            && !c.is_empty()
        {
            return true;
        }
        if let Some(Tag::Compound(c)) = nbt.get(&JavaString::from("minecraft:stored_enchantments"))
            && !c.is_empty()
        {
            return true;
        }
    }
    if let Some(Tag::Byte(b)) =
        host.item_default_component(stack.item(), "minecraft:enchantment_glint_override")
    {
        return b != 0;
    }
    false
}

/// Resolves vanilla `ItemStack.getRarity()` from stack overrides, item defaults, and enchantment state.
pub fn resolve_stack_rarity(
    host: &dyn ClientHost,
    stack: &ItemStack,
    enchanted: bool,
) -> ItemRarity {
    let base = stack
        .components()
        .and_then(|c| c.as_nbt().get(&JavaString::from("minecraft:rarity")))
        .and_then(|t| match t {
            Tag::String(s) => Some(ItemRarity::parse(&s.to_string_lossy())),
            _ => None,
        })
        .or_else(|| {
            host.item_default_component(stack.item(), "minecraft:rarity")
                .and_then(|t| match t {
                    Tag::String(s) => Some(ItemRarity::parse(&s.to_string_lossy())),
                    _ => None,
                })
        })
        .unwrap_or(ItemRarity::Common);
    base.with_enchantment(enchanted)
}

/// Counts total items matching `item` (`ItemStack::same_item`) in the local player's inventory.
pub fn count_in_local_inventory(host: &dyn ClientHost, item: ItemKey) -> u32 {
    let Some(player) = host.local_player() else {
        return 0;
    };
    player
        .with_inventory(host, |inv| {
            let mut total = 0u32;
            for (slot, stack) in inv.iter() {
                if slot <= 40 && stack.item() == item {
                    total = total.saturating_add(stack.count());
                }
            }
            total
        })
        .unwrap_or(0)
}

/// Snapshot of the local player's inventory and experience for client-only fallback tracking
/// on servers without Pick Up Notifier installed.
#[derive(Default)]
pub struct ClientFallbackTracker {
    prev_items: HashMap<ItemKey, u32>,
    prev_xp_total: Option<i32>,
    initialized: bool,
    pub pending_take_ticks: u8,
}

impl ClientFallbackTracker {
    pub fn reset(&mut self) {
        self.prev_items.clear();
        self.prev_xp_total = None;
        self.initialized = false;
        self.pending_take_ticks = 0;
    }

    pub fn on_take_packet(&mut self) {
        self.pending_take_ticks = 4;
    }

    /// Updates the snapshot each client tick and returns `(gained_items, gained_xp)` when a
    /// `minecraft:take_item_entity` packet recently arrived and `emit` is true.
    pub fn tick_and_diff(
        &mut self,
        host: &dyn ClientHost,
        emit: bool,
    ) -> (Vec<(ItemStack, u32)>, u32) {
        let Some(player) = host.local_player() else {
            self.reset();
            return (Vec::new(), 0);
        };

        let mut cur_counts: HashMap<ItemKey, u32> = HashMap::new();
        let mut sample_stacks: HashMap<ItemKey, ItemStack> = HashMap::new();
        let _ = player.with_inventory(host, |inv| {
            for (slot, view) in inv.iter() {
                if slot <= 40 {
                    let key = view.item();
                    *cur_counts.entry(key).or_insert(0) += view.count();
                    sample_stacks
                        .entry(key)
                        .or_insert_with(|| view.project().unwrap_or_else(|| ItemStack::of(key)));
                }
            }
        });
        let cur_xp = player.experience(host).map(|e| e.total);

        let mut gained_items = Vec::new();
        let mut gained_xp = 0u32;

        if self.initialized
            && emit
            && self.pending_take_ticks > 0
            && host.observed_screen_id().is_none()
        {
            for (&key, &count) in &cur_counts {
                let prev = self.prev_items.get(&key).copied().unwrap_or(0);
                if count > prev
                    && let Some(stack) = sample_stacks.get(&key)
                {
                    gained_items.push((stack.clone(), count - prev));
                }
            }
            if let (Some(prev), Some(cur)) = (self.prev_xp_total, cur_xp)
                && cur > prev
            {
                gained_xp = (cur - prev) as u32;
            }
        }

        if self.pending_take_ticks > 0 {
            self.pending_take_ticks -= 1;
        }
        self.prev_items = cur_counts;
        self.prev_xp_total = cur_xp;
        self.initialized = true;

        (gained_items, gained_xp)
    }
}

pub fn is_arrow_item_id(id: &str) -> bool {
    matches!(
        id,
        "minecraft:arrow" | "minecraft:spectral_arrow" | "minecraft:tipped_arrow"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorten_value_matches_original() {
        assert_eq!(shorten_value(1), "1");
        assert_eq!(shorten_value(999), "999");
        assert_eq!(shorten_value(1_000), "1K");
        assert_eq!(shorten_value(12_345), "12K");
        assert_eq!(shorten_value(1_000_000), "1M");
        assert_eq!(shorten_value(2_500_000_000), "2B");
    }

    #[test]
    fn xp_texture_tiers_match_vanilla() {
        assert_eq!(xp_texture_index(0), 0);
        assert_eq!(xp_texture_index(2), 0);
        assert_eq!(xp_texture_index(3), 1);
        assert_eq!(xp_texture_index(7), 2);
        assert_eq!(xp_texture_index(17), 3);
        assert_eq!(xp_texture_index(37), 4);
        assert_eq!(xp_texture_index(73), 5);
        assert_eq!(xp_texture_index(149), 6);
        assert_eq!(xp_texture_index(307), 7);
        assert_eq!(xp_texture_index(617), 8);
        assert_eq!(xp_texture_index(1237), 9);
        assert_eq!(xp_texture_index(2477), 10);
    }

    #[test]
    fn rarity_enchantment_upgrade_matches_vanilla() {
        assert_eq!(
            ItemRarity::Common.with_enchantment(false),
            ItemRarity::Common
        );
        assert_eq!(ItemRarity::Common.with_enchantment(true), ItemRarity::Rare);
        assert_eq!(
            ItemRarity::Uncommon.with_enchantment(true),
            ItemRarity::Rare
        );
        assert_eq!(ItemRarity::Rare.with_enchantment(true), ItemRarity::Epic);
        assert_eq!(ItemRarity::Epic.with_enchantment(true), ItemRarity::Epic);
    }

    #[test]
    fn experience_entries_combine_and_expire() {
        let mut collector = EntryCollector::default();
        collector.push(
            DisplayEntry::new_experience(Text::literal("Experience Orb"), 5, 10, 80),
            CombineEntries::ExcludeNamed,
            10,
        );
        collector.push(
            DisplayEntry::new_experience(Text::literal("Experience Orb"), 7, 25, 80),
            CombineEntries::ExcludeNamed,
            10,
        );
        assert_eq!(collector.entries.len(), 1);
        assert_eq!(collector.entries[0].amount, 12);
        // Preserves original orb tick_count on merge.
        match &collector.entries[0].kind {
            EntryKind::Experience { tick_count, .. } => assert_eq!(*tick_count, 10),
            _ => panic!("expected experience entry"),
        }

        // Tick down to expiry.
        for _ in 0..80 {
            collector.tick(80);
        }
        assert!(collector.entries.is_empty());
    }

    #[test]
    fn display_time_zero_never_expires_by_time() {
        let mut collector = EntryCollector::default();
        collector.push(
            DisplayEntry::new_experience(Text::literal("Experience Orb"), 3, 0, 0),
            CombineEntries::Never,
            2,
        );
        for _ in 0..100 {
            collector.tick(0);
        }
        assert_eq!(collector.entries.len(), 1);

        // Evicts oldest only when exceeding max_size.
        collector.push(
            DisplayEntry::new_experience(Text::literal("Experience Orb"), 4, 0, 0),
            CombineEntries::Never,
            2,
        );
        collector.push(
            DisplayEntry::new_experience(Text::literal("Experience Orb"), 5, 0, 0),
            CombineEntries::Never,
            2,
        );
        assert_eq!(collector.entries.len(), 2);
        assert_eq!(collector.entries[0].amount, 4);
        assert_eq!(collector.entries[1].amount, 5);
    }

    #[test]
    fn covered_area_alpha_computes_intersection_ratio() {
        // Fully inside -> 1.0
        assert!((covered_area_alpha(10, 10, 50, 0, 0, 100, 100) - 1.0).abs() < 1e-6);
        // Half outside horizontally -> 0.5
        assert!((covered_area_alpha(75, 10, 50, 0, 0, 100, 100) - 0.5).abs() < 1e-6);
        // Completely outside -> 0.0
        assert!((covered_area_alpha(120, 10, 50, 0, 0, 100, 100) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn xp_orb_color_packs_alpha_and_green() {
        let color = xp_orb_color(0.0, 0.5);
        let a = (color >> 24) & 0xFF;
        let g = (color >> 8) & 0xFF;
        assert_eq!(a, 128);
        assert_eq!(g, 255);
    }
}
