//! Conditional crafting recipe serializers for Comforts (`comforts:conditional_shaped`
//! and `comforts:conditional_shapeless`).
//!
//! Each recipe JSON is an ordinary vanilla shaped/shapeless recipe plus a `"condition"`
//! field (`"sleeping_bag"`, `"hammock"`, `"hammock_and_ropes"`, or `"always"`). Recipes
//! evaluate `CommonConfig` (`enableSleepingBagRecipes`, `enableHammockRecipes`) and
//! `#c:ropes` tag non-emptiness dynamically against the current resource generation.

use std::collections::BTreeMap;

use mod_api::{CraftingGrid, CraftingRecipeKind, ItemKey, ItemStack, LoopContext, TagKey};
use serde_json::Value;

use crate::rules::{
    RecipeCondition, RecipeIngredient, ShapedConditionalRecipe, ShapelessConditionalRecipe,
    trim_shaped_pattern,
};
use crate::{common_config, keys};

pub const SHAPED_ID: &str = "comforts:conditional_shaped";
pub const SHAPELESS_ID: &str = "comforts:conditional_shapeless";

/// Pre-bound recipe data: stores resolved `ItemKey` / `TagKey` handles so `matches`
/// and `assemble` do zero string lookups against the host.
#[derive(Clone, Debug)]
pub struct BoundIngredient {
    pub item_key: Option<ItemKey>,
    pub tag_key: Option<TagKey>,
    pub choice_keys: Vec<ItemKey>,
}

impl BoundIngredient {
    fn bind(raw: RecipeIngredient) -> Self {
        let k = keys();
        match &raw {
            RecipeIngredient::Item(id) => Self {
                item_key: k.lookup_item(id),
                tag_key: None,
                choice_keys: Vec::new(),
            },
            RecipeIngredient::Tag(tag) => Self {
                item_key: None,
                tag_key: k.lookup_tag(tag),
                choice_keys: Vec::new(),
            },
            RecipeIngredient::AnyOf(choices) => Self {
                item_key: None,
                tag_key: None,
                choice_keys: choices.iter().filter_map(|id| k.lookup_item(id)).collect(),
            },
        }
    }

    fn matches(&self, context: &dyn LoopContext, item: ItemKey) -> bool {
        if let Some(expected) = self.item_key {
            return expected == item;
        }
        if let Some(tag) = self.tag_key {
            return context.is_in_tag(item, tag) == Some(true);
        }
        if !self.choice_keys.is_empty() {
            return self.choice_keys.contains(&item);
        }
        false
    }
}

#[derive(Clone, Debug)]
pub struct BoundShapedRecipe {
    pub raw: ShapedConditionalRecipe,
    pub cells: Vec<Option<BoundIngredient>>,
    pub result: Option<ItemKey>,
}

impl BoundShapedRecipe {
    fn matches(&self, context: &dyn LoopContext, grid: &CraftingGrid) -> bool {
        let width = grid.width as usize;
        let height = grid.height as usize;
        if self.raw.width == 0
            || self.raw.height == 0
            || width < self.raw.width
            || height < self.raw.height
        {
            return false;
        }
        for oy in 0..=(height - self.raw.height) {
            for ox in 0..=(width - self.raw.width) {
                if self.matches_at(context, width, height, &grid.cells, ox, oy, false)
                    || self.matches_at(context, width, height, &grid.cells, ox, oy, true)
                {
                    return true;
                }
            }
        }
        false
    }

    #[allow(clippy::too_many_arguments)]
    fn matches_at(
        &self,
        context: &dyn LoopContext,
        width: usize,
        height: usize,
        cells: &[Option<ItemStack>],
        ox: usize,
        oy: usize,
        mirror: bool,
    ) -> bool {
        for y in 0..height {
            for x in 0..width {
                let grid_item = cells[y * width + x].as_ref().map(ItemStack::item);
                let expected =
                    if x >= ox && x < ox + self.raw.width && y >= oy && y < oy + self.raw.height {
                        let rx = if mirror {
                            self.raw.width - 1 - (x - ox)
                        } else {
                            x - ox
                        };
                        let ry = y - oy;
                        self.cells[ry * self.raw.width + rx].as_ref()
                    } else {
                        None
                    };
                match (expected, grid_item) {
                    (None, None) => {}
                    (Some(ing), Some(item)) if ing.matches(context, item) => {}
                    _ => return false,
                }
            }
        }
        true
    }
}

#[derive(Clone, Debug)]
pub struct BoundShapelessRecipe {
    pub raw: ShapelessConditionalRecipe,
    pub ingredients: Vec<BoundIngredient>,
    pub result: Option<ItemKey>,
}

impl BoundShapelessRecipe {
    fn matches(&self, context: &dyn LoopContext, grid: &CraftingGrid) -> bool {
        let items: Vec<ItemKey> = grid
            .cells
            .iter()
            .filter_map(|slot| slot.as_ref().map(ItemStack::item))
            .collect();
        if items.len() != self.ingredients.len() {
            return false;
        }
        let mut used = vec![false; items.len()];
        self.backtrack(context, 0, &items, &mut used)
    }

    fn backtrack(
        &self,
        context: &dyn LoopContext,
        idx: usize,
        items: &[ItemKey],
        used: &mut [bool],
    ) -> bool {
        if idx == self.ingredients.len() {
            return true;
        }
        for (i, &item) in items.iter().enumerate() {
            if !used[i] && self.ingredients[idx].matches(context, item) {
                used[i] = true;
                if self.backtrack(context, idx + 1, items, used) {
                    return true;
                }
                used[i] = false;
            }
        }
        false
    }
}

pub fn parse_shaped_recipe(json: &str) -> Result<ShapedConditionalRecipe, String> {
    let obj: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let condition = parse_condition(&obj)?;
    let pattern_arr = obj
        .get("pattern")
        .and_then(Value::as_array)
        .ok_or("missing pattern array")?;
    let mut rows = Vec::with_capacity(pattern_arr.len());
    for row in pattern_arr {
        rows.push(row.as_str().ok_or("pattern row must be string")?.to_owned());
    }
    let key_obj = obj
        .get("key")
        .and_then(Value::as_object)
        .ok_or("missing key object")?;
    let mut map = BTreeMap::new();
    for (k, v) in key_obj {
        let mut chars = k.chars();
        let ch = chars.next().ok_or("empty pattern key")?;
        if chars.next().is_some() || ch == ' ' {
            return Err(format!("invalid pattern key '{k}'"));
        }
        map.insert(ch, parse_ingredient(v)?);
    }
    let (width, height, cells) = trim_shaped_pattern(&rows, &map)?;
    let (result_item, result_count) = parse_result(&obj)?;
    Ok(ShapedConditionalRecipe {
        condition,
        width,
        height,
        cells,
        result_item,
        result_count,
        raw_json: json.to_owned(),
    })
}

pub fn parse_shapeless_recipe(json: &str) -> Result<ShapelessConditionalRecipe, String> {
    let obj: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let condition = parse_condition(&obj)?;
    let list = obj
        .get("ingredients")
        .and_then(Value::as_array)
        .ok_or("missing ingredients array")?;
    let mut ingredients = Vec::with_capacity(list.len());
    for entry in list {
        ingredients.push(parse_ingredient(entry)?);
    }
    if ingredients.is_empty() {
        return Err("shapeless recipe has no ingredients".into());
    }
    let (result_item, result_count) = parse_result(&obj)?;
    Ok(ShapelessConditionalRecipe {
        condition,
        ingredients,
        result_item,
        result_count,
        raw_json: json.to_owned(),
    })
}

/// Shaped conditional crafting recipe serializer.
pub struct ConditionalShapedKind;

impl CraftingRecipeKind for ConditionalShapedKind {
    type Value = BoundShapedRecipe;

    fn decode(&self, json: &str) -> Result<Self::Value, String> {
        let raw = parse_shaped_recipe(json)?;
        let bound_cells = raw
            .cells
            .iter()
            .map(|cell| cell.clone().map(BoundIngredient::bind))
            .collect();
        let result = keys().lookup_item(&raw.result_item);
        Ok(BoundShapedRecipe {
            raw,
            cells: bound_cells,
            result,
        })
    }

    fn write(&self, value: &Self::Value) -> Result<String, String> {
        Ok(value.raw.raw_json.clone())
    }

    fn matches(&self, context: &dyn LoopContext, value: &Self::Value, grid: CraftingGrid) -> bool {
        condition_holds(context, value.raw.condition) && value.matches(context, &grid)
    }

    fn assemble(
        &self,
        context: &dyn LoopContext,
        value: &Self::Value,
        grid: CraftingGrid,
    ) -> Option<ItemStack> {
        if !self.matches(context, value, grid) {
            return None;
        }
        build_result(value.result?, value.raw.result_count)
    }
}

/// Shapeless conditional crafting recipe serializer.
pub struct ConditionalShapelessKind;

impl CraftingRecipeKind for ConditionalShapelessKind {
    type Value = BoundShapelessRecipe;

    fn decode(&self, json: &str) -> Result<Self::Value, String> {
        let raw = parse_shapeless_recipe(json)?;
        let bound = raw
            .ingredients
            .iter()
            .cloned()
            .map(BoundIngredient::bind)
            .collect();
        let result = keys().lookup_item(&raw.result_item);
        Ok(BoundShapelessRecipe {
            raw,
            ingredients: bound,
            result,
        })
    }

    fn write(&self, value: &Self::Value) -> Result<String, String> {
        Ok(value.raw.raw_json.clone())
    }

    fn matches(&self, context: &dyn LoopContext, value: &Self::Value, grid: CraftingGrid) -> bool {
        condition_holds(context, value.raw.condition) && value.matches(context, &grid)
    }

    fn assemble(
        &self,
        context: &dyn LoopContext,
        value: &Self::Value,
        grid: CraftingGrid,
    ) -> Option<ItemStack> {
        if !self.matches(context, value, grid) {
            return None;
        }
        build_result(value.result?, value.raw.result_count)
    }
}

fn condition_holds(context: &dyn LoopContext, condition: RecipeCondition) -> bool {
    let common = common_config();
    let has_ropes = if condition == RecipeCondition::HammockAndRopes {
        context
            .items_in_tag(keys().tag_ropes)
            .is_some_and(|items| !items.is_empty())
    } else {
        true
    };
    condition.is_satisfied(common, has_ropes)
}

fn build_result(key: ItemKey, count: u8) -> Option<ItemStack> {
    let mut stack = ItemStack::of(key);
    stack.set_count(u32::from(count)).ok()?;
    Some(stack)
}

fn parse_condition(obj: &Value) -> Result<RecipeCondition, String> {
    match obj.get("condition").and_then(Value::as_str) {
        Some(raw) => {
            RecipeCondition::parse(raw).ok_or_else(|| format!("unknown recipe condition '{raw}'"))
        }
        None => Ok(RecipeCondition::Always),
    }
}

fn parse_ingredient(val: &Value) -> Result<RecipeIngredient, String> {
    match val {
        Value::String(s) => {
            if let Some(tag) = s.strip_prefix('#') {
                Ok(RecipeIngredient::Tag(tag.to_owned()))
            } else {
                Ok(RecipeIngredient::Item(s.clone()))
            }
        }
        Value::Array(arr) => {
            let mut choices = Vec::with_capacity(arr.len());
            for item in arr {
                let s = item
                    .as_str()
                    .ok_or("ingredient list entry must be string")?;
                choices.push(s.to_owned());
            }
            if choices.is_empty() {
                return Err("empty ingredient choice list".into());
            }
            Ok(RecipeIngredient::AnyOf(choices))
        }
        _ => Err("ingredient must be string or array of strings".into()),
    }
}

fn parse_result(obj: &Value) -> Result<(String, u8), String> {
    let res = obj.get("result").ok_or("missing result object")?;
    let id = res
        .get("id")
        .and_then(Value::as_str)
        .ok_or("missing result.id")?
        .to_owned();
    let count = res
        .get("count")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .clamp(1, 99) as u8;
    Ok((id, count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_data_pack_recipes_parse_cleanly() {
        let recipe_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("data")
            .join("comforts")
            .join("recipe");
        let entries: Vec<_> = std::fs::read_dir(&recipe_dir)
            .expect("comforts recipe dir exists")
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
            .collect();
        assert_eq!(
            entries.len(),
            66,
            "expected all 66 upstream comforts recipes"
        );

        let mut shaped_count = 0;
        let mut shapeless_count = 0;
        for path in entries {
            let text = std::fs::read_to_string(&path).unwrap();
            let v: Value = serde_json::from_str(&text).unwrap();
            match v["type"].as_str().unwrap() {
                "comforts:conditional_shaped" => {
                    let parsed = parse_shaped_recipe(&text)
                        .unwrap_or_else(|e| panic!("failed shaped {}: {e}", path.display()));
                    assert!(parsed.result_count >= 1);
                    assert!(parsed.result_item.starts_with("comforts:"));
                    shaped_count += 1;
                }
                "comforts:conditional_shapeless" => {
                    let parsed = parse_shapeless_recipe(&text)
                        .unwrap_or_else(|e| panic!("failed shapeless {}: {e}", path.display()));
                    assert!(parsed.result_count >= 1);
                    assert!(parsed.result_item.starts_with("comforts:"));
                    shapeless_count += 1;
                }
                other => panic!("unexpected recipe type {other} in {}", path.display()),
            }
        }
        assert_eq!(shaped_count, 33);
        assert_eq!(shapeless_count, 33);
    }
}
