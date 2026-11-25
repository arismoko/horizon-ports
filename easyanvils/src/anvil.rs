//! Core anvil calculation engine, enchantment catalog, and anvil block-state helpers
//! matching Easy Anvils (`ModAnvilMenu`, `ItemInteractionHandler`, `PriorWorkPenalty`).

use crate::config::{RenameAndRepairCost, ServerConfig};
use crate::formatting::{component_to_string, string_to_component, visible_length};
use mod_api::{
    BlockKey, BlockState, ComponentId, Compound, Host, Identifier, ItemResource, ItemStack,
    JavaString, PackType, RegistryEntry, Tag,
};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::{PoisonError, RwLock};

pub const UNALTERED_ANVILS_TAG: &str = "easyanvils:unaltered_anvils";

pub const ANVIL_BLOCKS: [&str; 3] = [
    "minecraft:anvil",
    "minecraft:chipped_anvil",
    "minecraft:damaged_anvil",
];

pub fn is_anvil_block_id(id: &str) -> bool {
    ANVIL_BLOCKS.contains(&id)
}

pub fn is_anvil_state(host: &dyn Host, state: BlockState) -> bool {
    state.block().id(host).is_some_and(is_anvil_block_id)
}

pub fn is_unaltered_anvil(host: &dyn Host, block: BlockKey) -> bool {
    let Some(block_id) = block.id(host) else {
        return false;
    };
    let (Ok(reg), Ok(id)) = (
        Identifier::parse("minecraft:block"),
        Identifier::parse(block_id),
    ) else {
        return false;
    };
    host.registry_entry_in_tag(&RegistryEntry::new(reg, id), UNALTERED_ANVILS_TAG)
        .unwrap_or(false)
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

/// Returns the anvil state repaired by one stage (`damaged` -> `chipped` -> `anvil`), preserving `facing`.
pub fn repaired_anvil_state(host: &dyn Host, state: BlockState) -> Option<BlockState> {
    let block_id = state.block().id(host)?;
    let target = match block_id {
        "minecraft:damaged_anvil" => "minecraft:chipped_anvil",
        "minecraft:chipped_anvil" => "minecraft:anvil",
        _ => return None,
    };
    let serialized = host.block_state_string(state)?;
    let facing = parse_block_state_property(&serialized, "facing").unwrap_or("north");
    host.block_state_id(&format!("{target}[facing={facing}]"))
}

/// Returns the anvil state damaged by one stage (`anvil` -> `chipped` -> `damaged`), or `None` if it breaks.
pub fn damaged_anvil_state(host: &dyn Host, state: BlockState) -> Option<BlockState> {
    let block_id = state.block().id(host)?;
    let target = match block_id {
        "minecraft:anvil" => "minecraft:chipped_anvil",
        "minecraft:chipped_anvil" => "minecraft:damaged_anvil",
        _ => return None,
    };
    let serialized = host.block_state_string(state)?;
    let facing = parse_block_state_property(&serialized, "facing").unwrap_or("north");
    host.block_state_id(&format!("{target}[facing={facing}]"))
}

/// Vanilla `AnvilMenu.calculateIncreasedRepairCost(old)`.
pub fn calculate_increased_repair_cost(old_repair_cost: i32) -> i32 {
    ((i64::from(old_repair_cost.max(0)) * 2) + 1).clamp(0, i64::from(i32::MAX)) as i32
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
    pub max_level: i32,
    pub anvil_cost: i32,
    pub supported_items: Option<ItemSetSpec>,
}

impl EnchantmentDef {
    fn can_enchant(&self, host: &dyn Host, item_id: &str) -> bool {
        self.supported_items
            .as_ref()
            .is_some_and(|spec| spec.matches(host, item_id))
    }
}

#[derive(Clone, Debug, Default)]
pub struct EnchantmentCatalog {
    pub defs: HashMap<String, EnchantmentDef>,
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
    let mut defs = HashMap::new();
    let files = host.list_resources(PackType::ServerData, "enchantment", ".json");
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
        let max_level = json
            .get("max_level")
            .and_then(Value::as_i64)
            .map(|v| (v as i32).max(1))
            .unwrap_or(1);
        let anvil_cost = json
            .get("anvil_cost")
            .and_then(Value::as_i64)
            .map(|v| (v as i32).max(0))
            .unwrap_or(1);
        let supported_items = json.get("supported_items").and_then(ItemSetSpec::from_json);
        defs.insert(
            ench_id,
            EnchantmentDef {
                max_level,
                anvil_cost,
                supported_items,
            },
        );
    }
    EnchantmentCatalog { defs }
}

fn resource_to_enchantment_id(res: &Identifier) -> Option<String> {
    let raw = res.to_string();
    let (ns, path) = raw.split_once(':')?;
    let stem = path.strip_prefix("enchantment/")?.strip_suffix(".json")?;
    Some(format!("{ns}:{stem}"))
}

fn item_in_tag_id(host: &dyn Host, item_id: &str, tag: &str) -> bool {
    let (Ok(reg), Ok(id)) = (
        Identifier::parse("minecraft:item"),
        Identifier::parse(item_id),
    ) else {
        return false;
    };
    host.registry_entry_in_tag(&RegistryEntry::new(reg, id), tag)
        .unwrap_or(false)
}

fn effective_max_damage(host: &dyn Host, stack: &ItemStack) -> u32 {
    if let Some(comp) = ComponentId::new("minecraft:max_damage")
        && let Some(Tag::Int(v)) = ItemResource::of(stack).value(host, &comp)
        && v > 0
    {
        return v as u32;
    }
    host.max_damage(stack.item()).unwrap_or(0)
}

fn read_repair_cost(host: &dyn Host, stack: &ItemStack) -> i32 {
    let Some(comp) = ComponentId::new("minecraft:repair_cost") else {
        return 0;
    };
    match ItemResource::of(stack).value(host, &comp) {
        Some(Tag::Int(v)) => v.max(0),
        _ => 0,
    }
}

fn set_repair_cost(stack: &mut ItemStack, cost: i32) {
    let mut patch = Compound::new();
    patch.insert(
        JavaString::from("minecraft:repair_cost"),
        Tag::Int(cost.max(0)),
    );
    let _ = stack.apply_components(&patch);
}

fn has_stored_enchantments(host: &dyn Host, stack: &ItemStack) -> bool {
    let Some(comp) = ComponentId::new("minecraft:stored_enchantments") else {
        return false;
    };
    ItemResource::of(stack).value(host, &comp).is_some()
}

fn enchantment_component_name(host: &dyn Host, stack: &ItemStack) -> &'static str {
    if stack.item().id(host) == Some("minecraft:enchanted_book") {
        "minecraft:stored_enchantments"
    } else {
        "minecraft:enchantments"
    }
}

/// Vanilla `EnchantmentHelper.canStoreEnchantments(ItemStack)`: true when the stack has its
/// enchantment component (`minecraft:stored_enchantments` for enchanted books, otherwise
/// `minecraft:enchantments`), including the item's default prototype component unless removed
/// by a `!` component patch.
fn can_store_enchantments(host: &dyn Host, stack: &ItemStack) -> bool {
    let comp_name = enchantment_component_name(host, stack);
    ComponentId::new(comp_name).is_some_and(|c| ItemResource::of(stack).value(host, &c).is_some())
}

fn read_enchantments(host: &dyn Host, stack: &ItemStack) -> BTreeMap<String, i32> {
    let comp_name = enchantment_component_name(host, stack);
    let Some(comp) = ComponentId::new(comp_name) else {
        return BTreeMap::new();
    };
    let mut out = BTreeMap::new();
    if let Some(Tag::Compound(map)) = ItemResource::of(stack).value(host, &comp) {
        // 26.3 / 1.21.5+ stores enchantments as flat `{ "minecraft:sharpness": 5 }` or `{ levels: { ... } }`.
        let entries = match map.get(&JavaString::from("levels")) {
            Some(Tag::Compound(inner)) => inner,
            _ => &map,
        };
        for (k, v) in entries.iter() {
            let lvl = match v {
                Tag::Int(n) => *n,
                Tag::Short(n) => i32::from(*n),
                Tag::Byte(n) => i32::from(*n),
                _ => continue,
            };
            if lvl > 0 {
                out.insert(k.to_string_lossy(), lvl);
            }
        }
    }
    out
}

fn write_enchantments(
    host: &dyn Host,
    stack: &mut ItemStack,
    enchantments: &BTreeMap<String, i32>,
) {
    if enchantments.is_empty() {
        return;
    }
    let comp_name = enchantment_component_name(host, stack);
    let mut map = Compound::new();
    for (id, &lvl) in enchantments {
        if lvl > 0 {
            map.insert(JavaString::from(id.as_str()), Tag::Int(lvl));
        }
    }
    let mut patch = Compound::new();
    patch.insert(JavaString::from(comp_name), Tag::Compound(map));
    let _ = stack.apply_components(&patch);
}

/// Output of `compute_anvil_result`.
#[derive(Clone, Debug)]
pub struct AnvilResult {
    pub output: Option<ItemStack>,
    /// Level cost: `>= 0` normal, `-1` sentinel for "Too Expensive!" (stacked enchanted item).
    pub cost: i32,
    pub repair_item_count_cost: u32,
    pub only_renaming: bool,
}

impl AnvilResult {
    pub fn empty(cost: i32) -> Self {
        Self {
            output: None,
            cost,
            repair_item_count_cost: 0,
            only_renaming: false,
        }
    }
}

/// Computes the anvil output stack, level cost, material count cost, and `only_renaming` flag,
/// matching `ModAnvilMenu.createAnvilResult` in Easy Anvils.
///
/// `item_name == None` means the player has not edited the name box (keep the input's existing name).
/// `item_name == Some("")` means the player cleared the box (removes `CUSTOM_NAME` if present).
pub fn compute_anvil_result(
    host: &dyn Host,
    primary: Option<&ItemStack>,
    secondary: Option<&ItemStack>,
    item_name: Option<&str>,
    creative: bool,
    cfg: &ServerConfig,
) -> AnvilResult {
    let Some(primary) = primary else {
        return AnvilResult::empty(0);
    };
    if !can_store_enchantments(host, primary) {
        return AnvilResult::empty(0);
    }

    let primary_id = primary.item().id(host).unwrap_or("");
    let mut output = primary.clone();
    let mut left_enchantments = read_enchantments(host, &output);
    let left_repair_cost = read_repair_cost(host, primary);
    let right_repair_cost = secondary.map_or(0, |s| read_repair_cost(host, s));
    let raw_base_repair_cost = left_repair_cost.saturating_add(right_repair_cost);
    let base_repair_cost = cfg.prior_work_penalty.apply(
        raw_base_repair_cost,
        cfg.maximum_prior_work_penalty_increase,
    );

    let mut repair_item_count_cost: u32 = 0;
    let mut is_book = false;
    let mut repair_op_cost: i32 = 0;
    let mut enchant_op_cost: i32 = 0;
    let mut rename_op_cost: i32 = 0;

    if let Some(secondary) = secondary {
        is_book = has_stored_enchantments(host, secondary);
        let output_res = ItemResource::of(&output);
        let secondary_res = ItemResource::of(secondary);
        let output_damageable = output_res.damageable(host);

        if output_damageable && output_res.repairable(host, &secondary_res) {
            let max_dmg = effective_max_damage(host, &output);
            let mut cur_dmg = output.damage();
            let unit_restore =
                (f64::from(max_dmg) * cfg.repair_with_material_restored_durability).floor() as u32;
            let mut step = cur_dmg.min(unit_restore);
            if step == 0 {
                return AnvilResult::empty(0);
            }
            let mut materials_used: u32 = 0;
            while step > 0 && materials_used < secondary.count() {
                cur_dmg = cur_dmg.saturating_sub(step);
                output.set_damage(cur_dmg);
                repair_op_cost = repair_op_cost.saturating_add(cfg.repair_with_material_unit_cost);
                materials_used += 1;
                step = cur_dmg.min(unit_restore);
            }
            repair_item_count_cost = materials_used;
        } else {
            if !is_book && (!output.same_item(secondary) || !output_damageable) {
                return AnvilResult::empty(0);
            }

            if output_damageable && !is_book {
                let max_dmg = effective_max_damage(host, &output);
                let left_rem = max_dmg.saturating_sub(primary.damage());
                let right_rem = max_dmg.saturating_sub(secondary.damage());
                let bonus = (f64::from(max_dmg) * cfg.repair_with_other_item_bonus_durability)
                    .floor() as u32;
                let total_rem = left_rem.saturating_add(right_rem).saturating_add(bonus);
                let new_dmg = max_dmg.saturating_sub(total_rem);
                if new_dmg < output.damage() {
                    output.set_damage(new_dmg);
                    repair_op_cost = repair_op_cost.saturating_add(cfg.repair_with_other_item_cost);
                }
            }

            let right_enchantments = read_enchantments(host, secondary);
            let cat = catalog(host);
            let mut any_compatible = false;
            let mut any_incompatible = false;

            for (right_id, right_lvl) in &right_enchantments {
                let left_lvl = left_enchantments.get(right_id).copied().unwrap_or(0);
                let mut combined = if left_lvl == *right_lvl {
                    right_lvl.saturating_add(1)
                } else {
                    left_lvl.max(*right_lvl)
                };

                let def = cat.defs.get(right_id);
                let mut compatible_with_item = def.is_some_and(|d| d.can_enchant(host, primary_id));
                if creative || primary_id == "minecraft:enchanted_book" {
                    compatible_with_item = true;
                }

                for left_id in left_enchantments.keys() {
                    if left_id != right_id && !host.enchantments_compatible(right_id, left_id) {
                        compatible_with_item = false;
                        enchant_op_cost = enchant_op_cost.saturating_add(1);
                    }
                }

                if !compatible_with_item {
                    // Allow using items with only incompatible enchantments as repair material
                    if repair_op_cost > 0 {
                        continue;
                    }
                    any_incompatible = true;
                } else {
                    any_compatible = true;
                    let max_allowed = def.map_or(1, |d| d.max_level);
                    if combined > max_allowed {
                        combined = max_allowed;
                    }
                    // Preserve levels already above max_level
                    let highest_input = left_lvl.max(*right_lvl);
                    if highest_input > combined {
                        combined = highest_input;
                    }

                    let mut anvil_cost = def.map_or(1, |d| d.anvil_cost);
                    if is_book && cfg.halved_book_costs {
                        anvil_cost = (anvil_cost / 2).max(1);
                    }

                    left_enchantments.insert(right_id.clone(), combined);
                    if left_lvl != 0 || left_lvl != combined {
                        enchant_op_cost =
                            enchant_op_cost.saturating_add(anvil_cost.saturating_mul(combined));
                    }

                    if primary.count() > 1 && !creative {
                        return AnvilResult::empty(-1);
                    }
                }
            }

            if any_incompatible && !any_compatible {
                return AnvilResult::empty(0);
            }
        }
    }

    let mut has_renamed = false;
    if let Some(raw_name) = item_name {
        let trimmed = raw_name.trim();
        if visible_length(trimmed, cfg.renaming_supports_formatting) == 0 {
            if primary.custom_name(host).ok().flatten().is_some() {
                rename_op_cost = if cfg.free_renames.is_free_for_item_id(primary_id) {
                    0
                } else {
                    1
                };
                has_renamed = true;
                let _ = output.set_custom_name(host, None);
            }
        } else {
            let current_name = host
                .hover_name(primary)
                .map(|t| component_to_string(host, &t, cfg.renaming_supports_formatting))
                .unwrap_or_default();
            if trimmed != current_name {
                rename_op_cost = if cfg.free_renames.is_free_for_item_id(primary_id) {
                    0
                } else {
                    1
                };
                has_renamed = true;
                let comp = string_to_component(trimmed, cfg.renaming_supports_formatting);
                let _ = output.set_custom_name(host, Some(&comp));
            }
        }
    }

    let all_ops_cost = enchant_op_cost
        .saturating_add(repair_op_cost)
        .saturating_add(rename_op_cost);

    let (mut final_cost, mut has_output) = compute_final_cost_and_cap(
        base_repair_cost,
        enchant_op_cost,
        all_ops_cost,
        has_renamed,
        creative,
        cfg,
    );

    let only_renaming = has_renamed && all_ops_cost == rename_op_cost;

    if !has_output {
        return AnvilResult {
            output: None,
            cost: final_cost,
            repair_item_count_cost,
            only_renaming,
        };
    }

    let mut output_repair_cost = read_repair_cost(host, &output);
    if secondary.is_some() && output_repair_cost < right_repair_cost {
        output_repair_cost = right_repair_cost;
    }

    if all_ops_cost > 0 {
        let should_increase = (enchant_op_cost > 0
            && (!is_book
                || primary_id != "minecraft:enchanted_book"
                || !cfg.penalty_free_enchants_for_books))
            || !cfg.penalty_free_renames_and_repairs;
        if should_increase {
            output_repair_cost = calculate_increased_repair_cost(output_repair_cost);
        }
    }

    if output_repair_cost > 0 {
        set_repair_cost(&mut output, output_repair_cost);
    }

    write_enchantments(host, &mut output, &left_enchantments);

    // Guard against negative cost leaking out of cap math
    if final_cost < 0 {
        final_cost = 0;
        has_output = false;
    }

    AnvilResult {
        output: has_output.then_some(output),
        cost: final_cost,
        repair_item_count_cost,
        only_renaming,
    }
}

/// Pure cost & too-expensive cap calculation matching `ModAnvilMenu.createAnvilResult`.
pub fn compute_final_cost_and_cap(
    base_repair_cost: i32,
    enchant_op_cost: i32,
    all_ops_cost: i32,
    has_renamed: bool,
    creative: bool,
    cfg: &ServerConfig,
) -> (i32, bool) {
    let (mut cost, mut has_output) = if all_ops_cost == 0 {
        (0, has_renamed)
    } else {
        let new_cost =
            if enchant_op_cost == 0 && cfg.rename_and_repair_costs == RenameAndRepairCost::Fixed {
                all_ops_cost
            } else {
                base_repair_cost.saturating_add(all_ops_cost)
            };
        (new_cost.max(0), true)
    };

    let has_no_limit = cfg.too_expensive_limit == -1;
    let max_cost = if has_no_limit {
        40
    } else {
        cfg.too_expensive_limit
    };

    if cost >= max_cost {
        if enchant_op_cost == 0 && cfg.rename_and_repair_costs == RenameAndRepairCost::Limited {
            cost = (max_cost - 1).max(0);
        } else if !has_no_limit && !creative {
            has_output = false;
        }
    }

    (cost, has_output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn increased_repair_cost_doubles_plus_one() {
        assert_eq!(calculate_increased_repair_cost(0), 1);
        assert_eq!(calculate_increased_repair_cost(1), 3);
        assert_eq!(calculate_increased_repair_cost(3), 7);
        assert_eq!(calculate_increased_repair_cost(7), 15);
        assert_eq!(calculate_increased_repair_cost(i32::MAX), i32::MAX);
    }

    #[test]
    fn free_rename_produces_output_at_zero_cost() {
        let cfg = ServerConfig::default();
        let (cost, has_out) = compute_final_cost_and_cap(15, 0, 0, true, false, &cfg);
        assert_eq!(cost, 0);
        assert!(has_out);
    }

    #[test]
    fn fixed_rename_and_repair_ignores_prior_work_penalty() {
        let cfg = ServerConfig::default(); // RenameAndRepairCost::Fixed
        let (cost, has_out) = compute_final_cost_and_cap(20, 0, 2, false, false, &cfg);
        assert_eq!(cost, 2);
        assert!(has_out);
    }

    #[test]
    fn limited_rename_and_repair_caps_below_max_cost() {
        let cfg = ServerConfig {
            rename_and_repair_costs: RenameAndRepairCost::Limited,
            too_expensive_limit: 40,
            ..ServerConfig::default()
        };
        let (cost, has_out) = compute_final_cost_and_cap(50, 0, 2, false, false, &cfg);
        assert_eq!(cost, 39);
        assert!(has_out);
    }

    #[test]
    fn too_expensive_limit_blocks_survival_but_allows_creative() {
        let cfg = ServerConfig {
            too_expensive_limit: 40,
            ..ServerConfig::default()
        };
        let (cost_surv, out_surv) = compute_final_cost_and_cap(30, 15, 15, false, false, &cfg);
        assert_eq!(cost_surv, 45);
        assert!(!out_surv);

        let (cost_crea, out_crea) = compute_final_cost_and_cap(30, 15, 15, false, true, &cfg);
        assert_eq!(cost_crea, 45);
        assert!(out_crea);
    }

    #[test]
    fn unlimited_cap_allows_over_forty_in_survival() {
        let cfg = ServerConfig::default(); // too_expensive_limit = -1
        let (cost, has_out) = compute_final_cost_and_cap(30, 25, 25, false, false, &cfg);
        assert_eq!(cost, 55);
        assert!(has_out);
    }
}
