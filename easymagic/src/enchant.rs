use crate::config::{ChiseledBookshelfPower, EnchantmentHint, ServerConfig};
use crate::payloads::EnchantClue;
use mod_api::{
    BlockHit, BlockKey, BlockShapeKind, BlockState, Host, Identifier, ItemKey, ItemResource,
    ItemStack, LevelRef, PackType, RegistryEntry, Tag,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{PoisonError, RwLock};

pub const ENCHANTING_CATALYSTS_TAG: &str = "easymagic:enchanting_catalysts";
pub const REROLL_CATALYSTS_TAG: &str = "easymagic:reroll_catalysts";
pub const UNALTERED_ENCHANTING_TABLES_TAG: &str = "easymagic:unaltered_enchanting_tables";
pub const ENCHANTMENT_POWER_TRANSMITTER_TAG: &str = "minecraft:enchantment_power_transmitter";
pub const IN_ENCHANTING_TABLE_TAG: &str = "minecraft:in_enchanting_table";
pub const CURSE_ENCHANTMENT_TAG: &str = "minecraft:curse";

/// Parses a single property value from `host.block_state_string(state)`, e.g.
/// `"minecraft:chiseled_bookshelf[facing=north,slot_0_occupied=true]"`.
pub fn block_state_property(host: &dyn Host, state: BlockState, prop: &str) -> Option<String> {
    let serialized = host.block_state_string(state)?;
    parse_block_state_property(&serialized, prop).map(str::to_owned)
}

pub fn parse_block_state_property<'a>(serialized: &'a str, prop: &str) -> Option<&'a str> {
    let inner = serialized.split_once('[')?.1.strip_suffix(']')?;
    for pair in inner.split(',') {
        if let Some((k, v)) = pair.split_once('=')
            && k == prop
        {
            return Some(v);
        }
    }
    None
}

/// Vanilla `EnchantingTableBlock.BOOKSHELF_OFFSETS`: 32 offsets around the table.
pub const BOOKSHELF_OFFSETS: [[i32; 3]; 32] = {
    let mut offsets = [[0i32; 3]; 32];
    let mut idx = 0;
    let mut dx = -2i32;
    while dx <= 2 {
        let mut dz = -2i32;
        while dz <= 2 {
            if dx.abs() == 2 || dz.abs() == 2 {
                let mut dy = 0i32;
                while dy <= 1 {
                    offsets[idx] = [dx, dy, dz];
                    idx += 1;
                    dy += 1;
                }
            }
            dz += 1;
        }
        dx += 1;
    }
    offsets
};

/// Exact Java `java.util.Random` / `LegacyRandomSource` LCG.
#[derive(Clone, Debug)]
pub struct LegacyRandom {
    state: u64,
}

impl LegacyRandom {
    const MULTIPLIER: u64 = 25_214_903_917;
    const INCREMENT: u64 = 11;
    const MASK: u64 = (1u64 << 48) - 1;

    pub fn new(seed: i64) -> Self {
        let mut rng = Self { state: 0 };
        rng.set_seed(seed);
        rng
    }

    pub fn set_seed(&mut self, seed: i64) {
        self.state = ((seed as u64) ^ Self::MULTIPLIER) & Self::MASK;
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.state = self
            .state
            .wrapping_mul(Self::MULTIPLIER)
            .wrapping_add(Self::INCREMENT)
            & Self::MASK;
        (self.state >> (48 - bits)) as i32
    }

    pub fn next_int_bound(&mut self, bound: i32) -> i32 {
        debug_assert!(bound > 0);
        if (bound & (bound - 1)) == 0 {
            return (((bound as i64) * (self.next(31) as i64)) >> 31) as i32;
        }
        loop {
            let bits = self.next(31);
            let val = bits % bound;
            if bits - val + (bound - 1) >= 0 {
                return val;
            }
        }
    }

    pub fn next_float(&mut self) -> f32 {
        self.next(24) as f32 / (1u32 << 24) as f32
    }
}

/// Vanilla `Player.getXpNeededForNextLevel(level)`.
/// Matches vanilla `Player.getXpNeededForNextLevel`, widened to `i64` so high `i32` levels never
/// overflow.
pub fn xp_needed_for_next_level(level: i32) -> i64 {
    let l = i64::from(level);
    if l >= 30 {
        112 + (l - 30) * 9
    } else if l >= 15 {
        37 + (l - 15) * 5
    } else {
        7 + l * 2
    }
}

/// Exact total experience points corresponding to `level` (progress = 0), matching
/// `PlayerExperienceHelper.getExperienceFromLevels` without `i32` or `f32` overflow.
pub fn experience_from_levels(level: i32) -> i128 {
    if level <= 0 {
        return 0;
    }
    let l = i128::from(level);
    if l < 17 {
        l * l + 6 * l
    } else if l < 32 {
        (5 * l * l - 81 * l + 720) / 2
    } else {
        (9 * l * l - 325 * l + 4440) / 2
    }
}

/// Exact total experience points for a player from their level and progress bar, matching
/// `PlayerExperienceHelper.getTotalExperience` without `i32` overflow.
pub fn total_experience(level: i32, progress: f32) -> i128 {
    if level < 0 {
        return 0;
    }
    experience_from_levels(level)
        + (xp_needed_for_next_level(level) as f64 * f64::from(progress)) as i128
}

/// Compute `(new_level, new_points_in_level, new_total)` after subtracting `cost` experience
/// points, stepping backward from `(level, progress)` as vanilla `Player.giveExperiencePoints`
/// does for negative amounts.
pub fn subtract_experience_points(
    level: i32,
    progress: f32,
    total: i32,
    cost: i32,
) -> (i32, i32, i32) {
    if cost <= 0 {
        let pts = if level >= 0 {
            (xp_needed_for_next_level(level) as f64 * f64::from(progress)) as i64
        } else {
            0
        };
        return (level, i32::try_from(pts.max(0)).unwrap_or(i32::MAX), total);
    }
    if level < 0 {
        return (0, 0, 0);
    }
    let mut lvl = level;
    let mut remaining_cost = i64::from(cost);
    let mut pts_in_level =
        ((xp_needed_for_next_level(lvl) as f64 * f64::from(progress)) as i64).max(0);
    let mut new_total = total.saturating_sub(cost).max(0);
    while remaining_cost > pts_in_level {
        remaining_cost -= pts_in_level;
        if lvl == 0 {
            return (0, 0, 0);
        }
        lvl -= 1;
        pts_in_level = xp_needed_for_next_level(lvl);
    }
    pts_in_level -= remaining_cost;
    if lvl == 0 && pts_in_level == 0 {
        new_total = 0;
    }
    let new_pts = i32::try_from(pts_in_level).unwrap_or(i32::MAX);
    (lvl, new_pts, new_total)
}

#[derive(Clone, Debug)]
pub struct CostCurve {
    pub base: i32,
    pub per_level_above_first: i32,
}

impl CostCurve {
    pub fn calculate(&self, level: u32) -> i32 {
        self.base + self.per_level_above_first * (level.saturating_sub(1) as i32)
    }
}

#[derive(Clone, Debug)]
pub enum ItemSetSpec {
    Tag(String),
    Items(Vec<String>),
}

impl ItemSetSpec {
    fn from_json(value: &Value) -> Option<Self> {
        match value {
            Value::String(s) => {
                if let Some(tag) = s.strip_prefix('#') {
                    Some(Self::Tag(tag.to_owned()))
                } else {
                    Some(Self::Items(vec![s.clone()]))
                }
            }
            Value::Array(arr) => {
                let items = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect::<Vec<_>>();
                Some(Self::Items(items))
            }
            _ => None,
        }
    }

    fn matches(&self, host: &dyn Host, item_id: &str) -> bool {
        match self {
            Self::Tag(tag) => item_in_tag_id(host, item_id, tag),
            Self::Items(list) => list.iter().any(|entry| entry == item_id),
        }
    }
}

#[derive(Clone, Debug)]
pub struct EnchantmentDef {
    pub id: String,
    pub weight: u32,
    pub max_level: u32,
    pub min_cost: CostCurve,
    pub max_cost: CostCurve,
    pub supported_items: Option<ItemSetSpec>,
    pub primary_items: Option<ItemSetSpec>,
    pub curse: bool,
    pub in_enchanting_table: bool,
}

#[derive(Clone, Debug, Default)]
pub struct EnchantmentCatalog {
    pub defs: Vec<EnchantmentDef>,
    pub tooltip_order: HashMap<String, usize>,
}

static CATALOG: RwLock<Option<EnchantmentCatalog>> = RwLock::new(None);

pub fn invalidate_catalog() {
    *CATALOG.write().unwrap_or_else(PoisonError::into_inner) = None;
}

pub fn catalog(host: &dyn Host) -> EnchantmentCatalog {
    if let Some(cached) = CATALOG
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
    {
        return cached.clone();
    }
    let built = build_catalog(host);
    *CATALOG.write().unwrap_or_else(PoisonError::into_inner) = Some(built.clone());
    built
}

fn build_catalog(host: &dyn Host) -> EnchantmentCatalog {
    let mut defs = Vec::new();
    let files = host.list_resources(PackType::ServerData, "enchantment", ".json");
    let ench_reg = Identifier::parse("minecraft:enchantment").ok();

    for res_id in files {
        let Some(bytes) = host.resource(PackType::ServerData, &res_id) else {
            continue;
        };
        let Ok(json) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        let Some(ench_id) = resource_to_enchantment_id(&res_id) else {
            continue;
        };
        let weight = json
            .get("weight")
            .and_then(Value::as_u64)
            .map(|v| v as u32)
            .unwrap_or(1)
            .max(1);
        let max_level = json
            .get("max_level")
            .and_then(Value::as_u64)
            .map(|v| v as u32)
            .unwrap_or(1)
            .max(1);
        let min_cost = parse_cost_curve(json.get("min_cost")).unwrap_or(CostCurve {
            base: 1,
            per_level_above_first: 10,
        });
        let max_cost = parse_cost_curve(json.get("max_cost")).unwrap_or(CostCurve {
            base: 50,
            per_level_above_first: 10,
        });
        let supported_items = json.get("supported_items").and_then(ItemSetSpec::from_json);
        let primary_items = json.get("primary_items").and_then(ItemSetSpec::from_json);

        let entry = ench_reg.as_ref().and_then(|reg| {
            let id = Identifier::parse(&ench_id).ok()?;
            Some(RegistryEntry::new(reg.clone(), id))
        });

        let in_table = entry
            .as_ref()
            .and_then(|e| host.registry_entry_in_tag(e, IN_ENCHANTING_TABLE_TAG))
            .unwrap_or(false);
        let curse = entry
            .as_ref()
            .and_then(|e| host.registry_entry_in_tag(e, CURSE_ENCHANTMENT_TAG))
            .unwrap_or(false);

        defs.push(EnchantmentDef {
            id: ench_id,
            weight,
            max_level,
            min_cost,
            max_cost,
            supported_items,
            primary_items,
            curse,
            in_enchanting_table: in_table,
        });
    }

    // Sort deterministically by registry ID if server resources came in arbitrary order
    defs.sort_by(|a, b| a.id.cmp(&b.id));

    let tooltip_order = load_tooltip_order(host);
    EnchantmentCatalog {
        defs,
        tooltip_order,
    }
}

fn resource_to_enchantment_id(res: &Identifier) -> Option<String> {
    let raw = res.to_string();
    let (ns, path) = raw.split_once(':')?;
    let stem = path.strip_prefix("enchantment/")?.strip_suffix(".json")?;
    Some(format!("{ns}:{stem}"))
}

fn parse_cost_curve(value: Option<&Value>) -> Option<CostCurve> {
    let obj = value?.as_object()?;
    let base = obj.get("base")?.as_i64()? as i32;
    let per_level_above_first = obj.get("per_level_above_first")?.as_i64()? as i32;
    Some(CostCurve {
        base,
        per_level_above_first,
    })
}

fn load_tooltip_order(host: &dyn Host) -> HashMap<String, usize> {
    let Ok(tag_id) = Identifier::parse("minecraft:tags/enchantment/tooltip_order.json") else {
        return HashMap::new();
    };
    let mut list: Vec<String> = Vec::new();
    for bytes in host.resource_stack(PackType::ServerData, &tag_id) {
        let Ok(json) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        if json.get("replace").and_then(Value::as_bool) == Some(true) {
            list.clear();
        }
        if let Some(values) = json.get("values").and_then(Value::as_array) {
            for val in values {
                let id_str = val
                    .as_str()
                    .or_else(|| val.get("id").and_then(Value::as_str));
                if let Some(entry) = id_str
                    && !entry.starts_with('#')
                {
                    let canonical = if entry.contains(':') {
                        entry.to_owned()
                    } else {
                        format!("minecraft:{entry}")
                    };
                    if !list.contains(&canonical) {
                        list.push(canonical);
                    }
                }
            }
        }
    }
    list.into_iter()
        .enumerate()
        .map(|(i, id)| (id, i))
        .collect()
}

pub fn item_in_tag_id(host: &dyn Host, item_id: &str, tag: &str) -> bool {
    let (Ok(reg), Ok(id)) = (
        Identifier::parse("minecraft:item"),
        Identifier::parse(item_id),
    ) else {
        return false;
    };
    host.registry_entry_in_tag(&RegistryEntry::new(reg, id), tag)
        .unwrap_or(false)
}

pub fn item_in_tag(host: &dyn Host, item: ItemKey, tag: &str) -> bool {
    let Some(item_id) = item.id(host) else {
        return false;
    };
    item_in_tag_id(host, item_id, tag)
}

pub fn is_enchanting_catalyst(host: &dyn Host, item: ItemKey) -> bool {
    item_in_tag(host, item, ENCHANTING_CATALYSTS_TAG)
}

pub fn is_reroll_catalyst(host: &dyn Host, item: ItemKey) -> bool {
    item_in_tag(host, item, REROLL_CATALYSTS_TAG)
}

/// Matches vanilla `ItemStack.isEnchanted() || stack.is(Items.ENCHANTED_BOOK)`.
pub fn is_stack_enchanted_or_book(host: &dyn Host, stack: &ItemStack) -> bool {
    if stack.item().id(host) == Some("minecraft:enchanted_book") {
        return true;
    }
    let Some(comp_id) = mod_api::ComponentId::new("minecraft:enchantments") else {
        return false;
    };
    matches!(
        ItemResource::of(stack).value(host, &comp_id),
        Some(Tag::Compound(levels)) if !levels.is_empty()
    )
}

/// Reads the effective `minecraft:enchantable` value (vanilla `Enchantable.value`) for `stack`.
pub fn item_enchantability(host: &dyn Host, stack: &ItemStack) -> i32 {
    let resource = ItemResource::of(stack);
    let Some(comp_id) = mod_api::ComponentId::new("minecraft:enchantable") else {
        return 0;
    };
    match resource.value(host, &comp_id) {
        Some(Tag::Compound(map)) => match map.get(&mod_api::JavaString::from("value")) {
            Some(Tag::Int(v)) => *v,
            Some(Tag::Short(v)) => i32::from(*v),
            Some(Tag::Byte(v)) => i32::from(*v),
            _ => 0,
        },
        Some(Tag::Int(v)) => v,
        _ => 0,
    }
}

/// Vanilla `ItemStack.isEnchantable`: has `minecraft:enchantable` > 0 and no existing enchantments.
pub fn is_stack_enchantable(host: &dyn Host, stack: &ItemStack) -> bool {
    let resource = ItemResource::of(stack);
    resource.enchantable(host) && item_enchantability(host, stack) > 0
}

/// Vanilla `EnchantmentHelper.getEnchantmentCost(random, slot, bookcase_count, enchantability)`.
pub fn get_enchantment_cost(
    random: &mut LegacyRandom,
    slot: usize,
    bookcase_count: i32,
    enchantability: i32,
) -> i32 {
    if enchantability <= 0 {
        return 0;
    }
    let books = bookcase_count.clamp(0, 15);
    let j = random.next_int_bound(8) + 1 + (books >> 1) + random.next_int_bound(books + 1);
    match slot {
        0 => (j / 3).max(1),
        1 => j * 2 / 3 + 1,
        _ => j.max(books * 2),
    }
}

/// Vanilla `EnchantmentHelper.selectEnchantment` over pre-filtered candidate definitions.
pub fn select_enchantment<'a, F>(
    random: &mut LegacyRandom,
    enchantability: i32,
    level: i32,
    candidates: &[&'a EnchantmentDef],
    compatible: F,
) -> Vec<(&'a EnchantmentDef, u32)>
where
    F: Fn(&str, &str) -> bool,
{
    let mut chosen: Vec<(&'a EnchantmentDef, u32)> = Vec::new();
    if enchantability <= 0 || level <= 0 {
        return chosen;
    }
    let mut modified_level = level
        + 1
        + random.next_int_bound(enchantability / 4 + 1)
        + random.next_int_bound(enchantability / 4 + 1);
    let f = (random.next_float() + random.next_float() - 1.0) * 0.15;
    modified_level = ((modified_level as f32 * (1.0 + f)).round() as i32).max(1);

    let mut available: Vec<(&'a EnchantmentDef, u32)> = Vec::new();
    for &def in candidates {
        for ench_level in (1..=def.max_level).rev() {
            let min_c = def.min_cost.calculate(ench_level);
            let max_c = def.max_cost.calculate(ench_level);
            if modified_level >= min_c && modified_level <= max_c {
                available.push((def, ench_level));
                break;
            }
        }
    }

    if let Some(first) = weighted_random(random, &available) {
        chosen.push(first);
        while random.next_int_bound(50) <= modified_level {
            if let Some(&(last_def, _)) = chosen.last() {
                available.retain(|&(cand, _)| {
                    cand.id != last_def.id && compatible(&last_def.id, &cand.id)
                });
            }
            if available.is_empty() {
                break;
            }
            if let Some(next) = weighted_random(random, &available) {
                chosen.push(next);
            }
            modified_level /= 2;
        }
    }

    chosen
}

fn weighted_random<'a>(
    random: &mut LegacyRandom,
    entries: &[(&'a EnchantmentDef, u32)],
) -> Option<(&'a EnchantmentDef, u32)> {
    let total: i32 = entries
        .iter()
        .map(|(def, _)| def.weight.max(1) as i32)
        .sum();
    if total <= 0 {
        return None;
    }
    let mut roll = random.next_int_bound(total);
    for &(def, lvl) in entries {
        roll -= def.weight.max(1) as i32;
        if roll < 0 {
            return Some((def, lvl));
        }
    }
    None
}

/// Vanilla `EnchantmentMenu.getEnchantmentList` for a single row (`slot` in `0..3`).
pub fn roll_row_enchantments(
    host: &dyn Host,
    cat: &EnchantmentCatalog,
    stack: &ItemStack,
    seed: i32,
    slot: usize,
    cost: i32,
) -> Vec<EnchantClue> {
    if cost <= 0 {
        return Vec::new();
    }
    let enchantability = item_enchantability(host, stack);
    if enchantability <= 0 {
        return Vec::new();
    }
    let item_key = stack.item();
    let item_id = item_key.id(host).unwrap_or("");
    let is_book = item_id == "minecraft:book";

    let candidates: Vec<&EnchantmentDef> = cat
        .defs
        .iter()
        .filter(|def| {
            if !def.in_enchanting_table {
                return false;
            }
            if is_book {
                return true;
            }
            if let Some(primary) = &def.primary_items {
                primary.matches(host, item_id)
            } else if let Some(supported) = &def.supported_items {
                supported.matches(host, item_id)
            } else {
                false
            }
        })
        .collect();

    let mut rng = LegacyRandom::new(seed.wrapping_add(slot as i32) as i64);
    let mut list = select_enchantment(&mut rng, enchantability, cost, &candidates, |a, b| {
        host.enchantments_compatible(a, b)
    });
    if is_book && list.len() > 1 {
        let drop_idx = rng.next_int_bound(list.len() as i32) as usize;
        list.remove(drop_idx);
    }

    list.into_iter()
        .map(|(def, level)| EnchantClue {
            id: def.id.clone(),
            level,
            curse: def.curse,
        })
        .collect()
}

/// Computes costs `[i32; 3]` and visible clue lists `[Vec<EnchantClue>; 3]` matching
/// `ModEnchantmentMenu.updateLevelsAndClues`.
pub fn compute_offers(
    host: &dyn Host,
    stack: Option<&ItemStack>,
    seed: i32,
    scaled_power: i32,
    hint: EnchantmentHint,
) -> ([i32; 3], [Vec<EnchantClue>; 3]) {
    let Some(stack) = stack else {
        return ([0; 3], [Vec::new(), Vec::new(), Vec::new()]);
    };
    if !is_stack_enchantable(host, stack) {
        return ([0; 3], [Vec::new(), Vec::new(), Vec::new()]);
    }
    let enchantability = item_enchantability(host, stack);
    let cat = catalog(host);
    let mut rng = LegacyRandom::new(seed as i64);
    let mut costs = [0i32; 3];
    let mut clues = [Vec::new(), Vec::new(), Vec::new()];

    for slot in 0..3 {
        let mut cost = get_enchantment_cost(&mut rng, slot, scaled_power, enchantability);
        if cost < (slot as i32) + 1 {
            cost = 0;
        }
        let full = roll_row_enchantments(host, &cat, stack, seed, slot, cost);
        if full.is_empty() {
            cost = 0;
        }
        costs[slot] = cost;
        clues[slot] = match hint {
            EnchantmentHint::None => Vec::new(),
            EnchantmentHint::Single => {
                if full.is_empty() {
                    Vec::new()
                } else {
                    let pick = rng.next_int_bound(full.len() as i32) as usize;
                    vec![full[pick].clone()]
                }
            }
            EnchantmentHint::All => sort_clues_by_tooltip_order(full, &cat.tooltip_order),
        };
    }

    (costs, clues)
}

pub fn sort_clues_by_tooltip_order(
    mut clues: Vec<EnchantClue>,
    order: &HashMap<String, usize>,
) -> Vec<EnchantClue> {
    clues.sort_by_key(|c| order.get(&c.id).copied().unwrap_or(usize::MAX));
    clues
}

/// Whether `boxes` represents a full 1×1×1 solid collision cube (blocking lenient bookshelves).
pub fn is_full_collision_block(boxes: &[[f64; 6]]) -> bool {
    boxes.iter().any(|b| {
        b[0] <= 0.0 && b[1] <= 0.0 && b[2] <= 0.0 && b[3] >= 1.0 && b[4] >= 1.0 && b[5] >= 1.0
    })
}

/// Whether a chiseled bookshelf at `pos + offset` with `facing` faces the enchanting table,
/// matching `ChiseledBookshelfHelper.findValidBooks`.
pub fn chiseled_bookshelf_faces_table(facing: &str, offset: [i32; 3]) -> bool {
    match facing {
        // Facing north (-Z) -> opposite is south (+Z) -> offset.z must be +2
        "north" => offset[2] == 2,
        // Facing south (+Z) -> opposite is north (-Z) -> offset.z must be -2
        "south" => offset[2] == -2,
        // Facing west (-X) -> opposite is east (+X) -> offset.x must be +2
        "west" => offset[0] == 2,
        // Facing east (+X) -> opposite is west (-X) -> offset.x must be -2
        "east" => offset[0] == -2,
        _ => false,
    }
}

/// Count occupied book slots (`0..=6`) from `minecraft:chiseled_bookshelf` block state properties.
pub fn chiseled_bookshelf_book_count(host: &dyn Host, state: BlockState) -> i32 {
    let Some(serialized) = host.block_state_string(state) else {
        return 0;
    };
    const SLOTS: [&str; 6] = [
        "slot_0_occupied",
        "slot_1_occupied",
        "slot_2_occupied",
        "slot_3_occupied",
        "slot_4_occupied",
        "slot_5_occupied",
    ];
    let mut count = 0;
    for prop in SLOTS {
        if parse_block_state_property(&serialized, prop) == Some("true") {
            count += 1;
        }
    }
    count
}

/// Checks whether the middle block between the table and `pos + offset` allows enchanting power.
pub fn is_valid_transmitter(
    host: &dyn Host,
    level: LevelRef,
    pos: [i32; 3],
    offset: [i32; 3],
    lenient: bool,
) -> bool {
    let mid = [
        pos[0] + offset[0] / 2,
        pos[1] + offset[1],
        pos[2] + offset[2] / 2,
    ];
    let Some(mid_state) = level.block_state(host, mid) else {
        return false;
    };
    if host
        .block_in_tag(mid_state.block(), ENCHANTMENT_POWER_TRANSMITTER_TAG)
        .unwrap_or(false)
    {
        return true;
    }
    if lenient {
        let Some(boxes) = level.block_shape(host, mid_state, mid, BlockShapeKind::Collision) else {
            return false;
        };
        return !is_full_collision_block(&boxes);
    }
    false
}

/// Counts valid books in a chiseled bookshelf at `pos + offset` according to `mode`.
pub fn valid_chiseled_books_at(
    host: &dyn Host,
    level: LevelRef,
    pos: [i32; 3],
    offset: [i32; 3],
    mode: ChiseledBookshelfPower,
) -> i32 {
    if mode == ChiseledBookshelfPower::None {
        return 0;
    }
    let shelf_pos = [pos[0] + offset[0], pos[1] + offset[1], pos[2] + offset[2]];
    let Some(state) = level.block_state(host, shelf_pos) else {
        return 0;
    };
    if state.block().id(host) != Some("minecraft:chiseled_bookshelf") {
        return 0;
    }
    if mode == ChiseledBookshelfPower::Facing {
        let Some(facing) = block_state_property(host, state, "facing") else {
            return 0;
        };
        if !chiseled_bookshelf_faces_table(&facing, offset) {
            return 0;
        }
    }
    chiseled_bookshelf_book_count(host, state)
}

/// Computes effective scaled enchanting power (`0..=15`) at `pos` in `level`, matching
/// `ModEnchantmentMenu.getEnchantingPower` and `max_enchanting_power` scaling.
pub fn compute_scaled_enchanting_power(
    host: &dyn Host,
    level: LevelRef,
    pos: [i32; 3],
    cfg: &ServerConfig,
) -> i32 {
    if cfg.max_enchanting_power <= 0 {
        return 15;
    }
    let mut regular_power = 0.0_f32;
    let mut chiseled_books = 0i32;

    for offset in BOOKSHELF_OFFSETS {
        if !is_valid_transmitter(host, level, pos, offset, cfg.lenient_bookshelves) {
            continue;
        }
        let shelf_pos = [pos[0] + offset[0], pos[1] + offset[1], pos[2] + offset[2]];
        if let Some(shelf_state) = level.block_state(host, shelf_pos)
            && let Ok(bonus) = level.get_enchant_power_bonus(host, shelf_pos, shelf_state, pos)
            && bonus.is_finite()
            && bonus > 0.0
        {
            regular_power += bonus;
        }
        chiseled_books += valid_chiseled_books_at(
            host,
            level,
            pos,
            offset,
            cfg.chiseled_bookshelf_enchanting_power,
        );
    }

    let raw_power = (regular_power as i32) + chiseled_books / 3;
    (raw_power * 15) / cfg.max_enchanting_power
}

/// Checks if a bookshelf at `pos + offset` should emit enchanting particles (for `animate_tick`).
pub fn is_valid_particle_bookshelf(
    host: &dyn Host,
    level: LevelRef,
    pos: [i32; 3],
    offset: [i32; 3],
    cfg: &ServerConfig,
) -> bool {
    if !is_valid_transmitter(host, level, pos, offset, cfg.lenient_bookshelves) {
        return false;
    }
    let shelf_pos = [pos[0] + offset[0], pos[1] + offset[1], pos[2] + offset[2]];
    if let Some(shelf_state) = level.block_state(host, shelf_pos)
        && let Ok(bonus) = level.get_enchant_power_bonus(host, shelf_pos, shelf_state, pos)
        && bonus.is_finite()
        && bonus > 0.0
    {
        return true;
    }
    valid_chiseled_books_at(
        host,
        level,
        pos,
        offset,
        cfg.chiseled_bookshelf_enchanting_power,
    ) > 0
}

/// Vanilla `ChiseledBookShelfBlock.getHitSlot(BlockHitResult, Direction)`: returns slot `0..6`.
pub fn chiseled_bookshelf_hit_slot(
    block_pos: [i32; 3],
    hit: &BlockHit,
    facing: &str,
) -> Option<u8> {
    let expected_face = match facing {
        "north" => [0, 0, -1],
        "south" => [0, 0, 1],
        "west" => [-1, 0, 0],
        "east" => [1, 0, 0],
        _ => return None,
    };
    if hit.face != expected_face {
        return None;
    }
    let rx = (hit.point[0] - f64::from(block_pos[0])) as f32;
    let ry = (hit.point[1] - f64::from(block_pos[1])) as f32;
    let rz = (hit.point[2] - f64::from(block_pos[2])) as f32;
    let u = match facing {
        "north" => 1.0 - rx,
        "south" => rx,
        "west" => rz,
        "east" => 1.0 - rz,
        _ => return None,
    };
    let row = if ry >= 0.5 { 0u8 } else { 1u8 };
    let col = ((u * 3.0).floor() as i32).clamp(0, 2) as u8;
    Some(row * 3 + col)
}

/// Checks whether a block is marked in `#easymagic:unaltered_enchanting_tables`.
pub fn is_unaltered_enchanting_table(host: &dyn Host, block: BlockKey) -> bool {
    host.block_in_tag(block, UNALTERED_ENCHANTING_TABLES_TAG)
        .unwrap_or(false)
}

/// Vanilla `EnchantmentNames` word list for Standard Galactic Alphabet enchanting runes.
const SGA_WORDS: &[&str] = &[
    "the",
    "elder",
    "scrolls",
    "klaatu",
    "berata",
    "niktu",
    "xyzzy",
    "bless",
    "curse",
    "light",
    "darkness",
    "fire",
    "air",
    "earth",
    "water",
    "hot",
    "dry",
    "cold",
    "wet",
    "ignite",
    "snuff",
    "embiggen",
    "twist",
    "shorten",
    "stretch",
    "fiddle",
    "destroy",
    "imbue",
    "galvanize",
    "enchant",
    "free",
    "limited",
    "range",
    "of",
    "towards",
    "inside",
    "sphere",
    "cube",
    "self",
    "other",
    "ball",
    "mental",
    "physical",
    "grow",
    "shrink",
    "demon",
    "elemental",
    "spirit",
    "animal",
    "creature",
    "beast",
    "humanoid",
    "undead",
    "fresh",
    "stale",
    "phnglui",
    "mglwnafh",
    "cthulhu",
    "rlyeh",
    "wgahnagl",
    "fhtagn",
    "baguette",
];

/// Generates the Standard Galactic Alphabet rune string for a row, seeded identically to vanilla
/// `EnchantmentNames`.
pub fn generate_rune_phrase(seed: i32, row: usize) -> String {
    let mut rng = LegacyRandom::new(seed as i64);
    let mut phrase = String::new();
    for r in 0..=row {
        phrase.clear();
        let word_count = rng.next_int_bound(2) + 3;
        for w in 0..word_count {
            if w > 0 {
                phrase.push(' ');
            }
            let idx = rng.next_int_bound(SGA_WORDS.len() as i32) as usize;
            phrase.push_str(SGA_WORDS[idx]);
        }
        if r == row {
            break;
        }
    }
    phrase
}

/// Formats an enchantment ID like `"minecraft:sharpness"` into its translation key
/// `"enchantment.minecraft.sharpness"`.
pub fn enchantment_translation_key(id: &str) -> String {
    if let Some((ns, path)) = id.split_once(':') {
        format!("enchantment.{ns}.{path}")
    } else {
        format!("enchantment.minecraft.{id}")
    }
}

/// Applies the rolled enchantments onto `stack`: converts `minecraft:book` into
/// `minecraft:enchanted_book` with `minecraft:stored_enchantments`, otherwise merges into
/// `minecraft:enchantments`, keeping the higher level if an enchantment already exists.
pub fn apply_enchantments_to_stack(
    host: &dyn Host,
    stack: &ItemStack,
    enchantments: &[EnchantClue],
) -> Option<ItemStack> {
    let is_book = stack.item().id(host) == Some("minecraft:book");
    let mut result = if is_book {
        let book_id = Identifier::parse("minecraft:enchanted_book").ok()?;
        let enchanted_book_key = host.item_key(&book_id)?;
        let mut book = ItemStack::new(enchanted_book_key, stack.count())?;
        if let Some(patch) = stack.components() {
            let _ = book.apply_components(patch.as_nbt());
        }
        book
    } else {
        stack.clone()
    };

    let component_name = if is_book {
        "minecraft:stored_enchantments"
    } else {
        "minecraft:enchantments"
    };

    let mut levels_map = mod_api::Compound::new();
    if let Some(comp_id) = mod_api::ComponentId::new(component_name)
        && let Some(Tag::Compound(existing)) = ItemResource::of(&result).value(host, &comp_id)
    {
        for (k, v) in existing.iter() {
            levels_map.insert(k.clone(), v.clone());
        }
    }

    for ench in enchantments {
        let key = mod_api::JavaString::from(ench.id.as_str());
        let prev = match levels_map.get(&key) {
            Some(Tag::Int(v)) => (*v).max(0) as u32,
            _ => 0,
        };
        let next = prev.max(ench.level);
        levels_map.insert(key, Tag::Int(next as i32));
    }

    let mut patch = mod_api::Compound::new();
    patch.insert(
        mod_api::JavaString::from(component_name),
        Tag::Compound(levels_map),
    );
    result.apply_components(&patch).ok()?;
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_random_matches_java_util_random() {
        // Known java.util.Random(42) outputs: nextInt(100) = 30, 63, 48
        let mut rng = LegacyRandom::new(42);
        assert_eq!(rng.next_int_bound(100), 30);
        assert_eq!(rng.next_int_bound(100), 63);
        assert_eq!(rng.next_int_bound(100), 48);
        // Power-of-two fast path: java.util.Random(0).nextInt(8) = 5
        let mut rng0 = LegacyRandom::new(0);
        assert_eq!(rng0.next_int_bound(8), 5);
    }

    #[test]
    fn enchantment_cost_matches_vanilla_bounds() {
        let mut rng = LegacyRandom::new(12345);
        assert_eq!(get_enchantment_cost(&mut rng, 0, 15, 0), 0);

        let mut rng = LegacyRandom::new(12345);
        let c0 = get_enchantment_cost(&mut rng, 0, 15, 10);
        let c1 = get_enchantment_cost(&mut rng, 1, 15, 10);
        let c2 = get_enchantment_cost(&mut rng, 2, 15, 10);
        assert!((1..=8).contains(&c0));
        assert!(c1 >= 1);
        assert_eq!(c2, 30);
    }

    #[test]
    fn experience_conversion_and_subtraction_round_trip() {
        assert_eq!(experience_from_levels(0), 0);
        assert_eq!(experience_from_levels(1), 7);
        assert_eq!(experience_from_levels(16), 352);
        assert_eq!(experience_from_levels(17), 394);
        assert_eq!(experience_from_levels(30), 1395);
        assert_eq!(experience_from_levels(31), 1507);
        assert_eq!(experience_from_levels(32), 1628);

        assert_eq!(total_experience(30, 0.0), 1395);
        let (lvl, pts, tot) = subtract_experience_points(30, 0.0, 1395, 5);
        assert_eq!(tot, 1390);
        assert_eq!(lvl, 29);
        assert_eq!(i64::from(pts), xp_needed_for_next_level(29) - 5);

        // High levels must not overflow i32 or loop from 0.
        assert_eq!(
            experience_from_levels(100_000),
            (9 * 100_000_i128 * 100_000 - 325 * 100_000 + 4440) / 2
        );
        let (hi_lvl, hi_pts, hi_tot) = subtract_experience_points(100_000, 0.0, 50_000, 5);
        assert_eq!(hi_lvl, 99_999);
        assert_eq!(i64::from(hi_pts), xp_needed_for_next_level(99_999) - 5);
        assert_eq!(hi_tot, 49_995);

        // Near/at i32::MAX must not panic or overflow.
        assert_eq!(
            xp_needed_for_next_level(i32::MAX),
            112 + (i64::from(i32::MAX) - 30) * 9
        );
        assert!(total_experience(i32::MAX, 0.0) > i128::from(i32::MAX));
        let (max_lvl, _max_pts, max_tot) = subtract_experience_points(i32::MAX, 0.0, 10, 5);
        assert_eq!(max_lvl, i32::MAX - 1);
        assert_eq!(max_tot, 5);

        // Underflowing below level 0 zeroes level, points, and total.
        assert_eq!(subtract_experience_points(0, 0.0, 10, 5), (0, 0, 0));
        assert_eq!(subtract_experience_points(1, 0.0, 7, 7), (0, 0, 0));
    }

    #[test]
    fn bookshelf_offsets_has_32_unique_entries() {
        assert_eq!(BOOKSHELF_OFFSETS.len(), 32);
        for [dx, dy, dz] in BOOKSHELF_OFFSETS {
            assert!(dx.abs() == 2 || dz.abs() == 2);
            assert!((0..=1).contains(&dy));
        }
    }

    #[test]
    fn chiseled_bookshelf_facing_check_matches_fuzs() {
        assert!(chiseled_bookshelf_faces_table("north", [0, 0, 2]));
        assert!(chiseled_bookshelf_faces_table("north", [2, 1, 2]));
        assert!(!chiseled_bookshelf_faces_table("north", [0, 0, -2]));
        assert!(!chiseled_bookshelf_faces_table("north", [2, 0, 0]));
        assert!(chiseled_bookshelf_faces_table("south", [0, 0, -2]));
        assert!(chiseled_bookshelf_faces_table("east", [-2, 0, 1]));
        assert!(chiseled_bookshelf_faces_table("west", [2, 1, -1]));
    }

    #[test]
    fn full_collision_block_detects_unit_cube_only() {
        assert!(is_full_collision_block(&[[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]]));
        // Empty (air / torch / flower)
        assert!(!is_full_collision_block(&[]));
        // Carpet (1/16 height)
        assert!(!is_full_collision_block(&[[
            0.0, 0.0, 0.0, 1.0, 0.0625, 1.0
        ]]));
        // Slab
        assert!(!is_full_collision_block(&[[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]]));
    }

    #[test]
    fn select_enchantment_filters_incompatible_and_picks_highest_level() {
        let sharp = EnchantmentDef {
            id: "minecraft:sharpness".into(),
            weight: 10,
            max_level: 5,
            min_cost: CostCurve {
                base: 1,
                per_level_above_first: 11,
            },
            max_cost: CostCurve {
                base: 21,
                per_level_above_first: 11,
            },
            supported_items: None,
            primary_items: None,
            curse: false,
            in_enchanting_table: true,
        };
        let smite = EnchantmentDef {
            id: "minecraft:smite".into(),
            weight: 5,
            max_level: 5,
            min_cost: CostCurve {
                base: 5,
                per_level_above_first: 8,
            },
            max_cost: CostCurve {
                base: 25,
                per_level_above_first: 8,
            },
            supported_items: None,
            primary_items: None,
            curse: false,
            in_enchanting_table: true,
        };
        let unbreaking = EnchantmentDef {
            id: "minecraft:unbreaking".into(),
            weight: 5,
            max_level: 3,
            min_cost: CostCurve {
                base: 5,
                per_level_above_first: 8,
            },
            max_cost: CostCurve {
                base: 55,
                per_level_above_first: 8,
            },
            supported_items: None,
            primary_items: None,
            curse: false,
            in_enchanting_table: true,
        };
        let candidates = [&sharp, &smite, &unbreaking];
        for seed in 0..20 {
            let mut rng = LegacyRandom::new(seed);
            let rolled = select_enchantment(&mut rng, 15, 30, &candidates, |a, b| {
                !((a == "minecraft:sharpness" && b == "minecraft:smite")
                    || (a == "minecraft:smite" && b == "minecraft:sharpness"))
            });
            assert!(!rolled.is_empty());
            let has_sharp = rolled.iter().any(|(d, _)| d.id == "minecraft:sharpness");
            let has_smite = rolled.iter().any(|(d, _)| d.id == "minecraft:smite");
            assert!(!(has_sharp && has_smite));
        }
    }

    #[test]
    fn chiseled_hit_slot_maps_six_slots() {
        let hit_top_left = BlockHit::new([10.1, 64.75, 21.0], [0, 0, 1], false);
        assert_eq!(
            chiseled_bookshelf_hit_slot([10, 64, 20], &hit_top_left, "south"),
            Some(0)
        );
        let hit_bottom_right = BlockHit::new([10.9, 64.25, 21.0], [0, 0, 1], false);
        assert_eq!(
            chiseled_bookshelf_hit_slot([10, 64, 20], &hit_bottom_right, "south"),
            Some(5)
        );
        // Wrong face returns None
        assert_eq!(
            chiseled_bookshelf_hit_slot([10, 64, 20], &hit_top_left, "north"),
            None
        );
    }

    #[test]
    fn block_state_property_parses_serialized_properties() {
        let s =
            "minecraft:chiseled_bookshelf[facing=east,slot_0_occupied=true,slot_1_occupied=false]";
        assert_eq!(parse_block_state_property(s, "facing"), Some("east"));
        assert_eq!(
            parse_block_state_property(s, "slot_0_occupied"),
            Some("true")
        );
        assert_eq!(
            parse_block_state_property(s, "slot_1_occupied"),
            Some("false")
        );
        assert_eq!(parse_block_state_property(s, "missing"), None);
    }

    #[test]
    fn resource_to_enchantment_id_strips_prefix_and_suffix() {
        let id1 = Identifier::parse("minecraft:enchantment/sharpness.json").unwrap();
        assert_eq!(
            resource_to_enchantment_id(&id1).as_deref(),
            Some("minecraft:sharpness")
        );
        let id2 = Identifier::parse("custom:enchantment/frost/bite.json").unwrap();
        assert_eq!(
            resource_to_enchantment_id(&id2).as_deref(),
            Some("custom:frost/bite")
        );
    }

    #[test]
    fn rune_phrase_is_deterministic_per_seed_and_row() {
        let a = generate_rune_phrase(12345, 0);
        let b = generate_rune_phrase(12345, 0);
        let c = generate_rune_phrase(12345, 1);
        assert_eq!(a, b);
        assert!(!a.is_empty());
        assert_ne!(a, c);
    }
}
