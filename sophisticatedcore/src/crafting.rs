//! The core's crafting kinds:
//! - `sophisticatedcore:upgrade_next_tier` (UpgradeNextTierRecipe, and the backpacks' BackpackUpgradeRecipe, which
//!   tools/sophisticatedbackpacks_port.py writes as this kind): a shaped recipe whose result keeps the components
//!   of its `copy` ingredient, so a backpack crafted into the next tier keeps its contents, upgrades and colours,
//!   and an upgrade its settings. Ingredients are lists of item ids (the generator writes tags out).
//! - `sophisticatedcore:storage_dye` (StorageDyeRecipeBase): a storage item and dyes anywhere in the grid; dyes in
//!   its column or left of it colour the cloth, in its column or right of it the trim (ColorHelper.calculateColor).
//!
//! Recipe callbacks get no host, so the item ids recipes may name are given keys at registration (`ITEM_IDS`).
use std::collections::HashMap;

use mod_api::serde_json::Value;
use mod_api::{CraftingGrid, CraftingRecipeCodec, CraftingRecipeKind, ItemKey, ItemStack, Registrar, Tag};

use crate::contents::{Nbt, custom_data, get_int, put, set_custom_data};

/// Vanilla items the core's recipes use (tools/sophisticatedbackpacks_port.py prints none outside this list).
pub const VANILLA_IDS: [&str; 31] = [
    "minecraft:black_stained_glass", "minecraft:blue_stained_glass", "minecraft:brown_stained_glass", "minecraft:bucket",
    "minecraft:copper_ingot", "minecraft:cyan_stained_glass", "minecraft:diamond", "minecraft:dispenser",
    "minecraft:ender_pearl", "minecraft:glass", "minecraft:gold_ingot", "minecraft:gray_stained_glass",
    "minecraft:green_stained_glass", "minecraft:hopper", "minecraft:iron_ingot", "minecraft:lapis_lazuli",
    "minecraft:light_blue_stained_glass", "minecraft:light_gray_stained_glass", "minecraft:lime_stained_glass",
    "minecraft:magenta_stained_glass", "minecraft:orange_stained_glass", "minecraft:pink_stained_glass",
    "minecraft:piston", "minecraft:purple_stained_glass", "minecraft:red_stained_glass", "minecraft:redstone",
    "minecraft:redstone_block", "minecraft:sticky_piston", "minecraft:tinted_glass", "minecraft:white_stained_glass",
    "minecraft:yellow_stained_glass",
];

/// Sophisticated Backpacks' items (ModItems, without the Chipped upgrades).
pub const BACKPACK_ITEM_IDS: [&str; 57] = [
    "backpack", "copper_backpack", "iron_backpack", "gold_backpack", "diamond_backpack", "netherite_backpack",
    "pickup_upgrade", "advanced_pickup_upgrade", "filter_upgrade", "advanced_filter_upgrade", "magnet_upgrade",
    "advanced_magnet_upgrade", "feeding_upgrade", "advanced_feeding_upgrade", "compacting_upgrade",
    "advanced_compacting_upgrade", "void_upgrade", "advanced_void_upgrade", "restock_upgrade",
    "advanced_restock_upgrade", "deposit_upgrade", "advanced_deposit_upgrade", "refill_upgrade",
    "advanced_refill_upgrade", "inception_upgrade", "everlasting_upgrade", "smelting_upgrade", "auto_smelting_upgrade",
    "smoking_upgrade", "auto_smoking_upgrade", "blasting_upgrade", "auto_blasting_upgrade", "crafting_upgrade",
    "stonecutter_upgrade", "stack_upgrade_starter_tier", "stack_upgrade_tier_1", "stack_upgrade_tier_2",
    "stack_upgrade_tier_3", "stack_upgrade_tier_4", "stack_downgrade_tier_1", "stack_downgrade_tier_2",
    "stack_downgrade_tier_3", "stack_upgrade_omega_tier", "jukebox_upgrade", "advanced_jukebox_upgrade",
    "tool_swapper_upgrade", "advanced_tool_swapper_upgrade", "tank_upgrade", "battery_upgrade", "pump_upgrade",
    "advanced_pump_upgrade", "xp_pump_upgrade", "anvil_upgrade", "smithing_upgrade", "infinity_upgrade",
    "survival_infinity_upgrade", "upgrade_base",
];

/// DyeColor's 1.20 texture colours, by dye item.
pub const DYES: [(&str, u32); 16] = [
    ("minecraft:white_dye", 0xF9FFFE), ("minecraft:orange_dye", 0xF9801D), ("minecraft:magenta_dye", 0xC74EBD),
    ("minecraft:light_blue_dye", 0x3AB3DA), ("minecraft:yellow_dye", 0xFED83D), ("minecraft:lime_dye", 0x80C71F),
    ("minecraft:pink_dye", 0xF38BAA), ("minecraft:gray_dye", 0x474F52), ("minecraft:light_gray_dye", 0x9D9D97),
    ("minecraft:cyan_dye", 0x169C9C), ("minecraft:purple_dye", 0x8932B8), ("minecraft:blue_dye", 0x3C44AA),
    ("minecraft:brown_dye", 0x835432), ("minecraft:green_dye", 0x5E7C16), ("minecraft:red_dye", 0xB02E26),
    ("minecraft:black_dye", 0x1D1D21),
];

/// Vanilla items Sophisticated Storage's core-kind recipes use (tools/sophisticatedstorage_port.py prints them).
pub const STORAGE_VANILLA_IDS: [&str; 22] = [
    "minecraft:black_shulker_box", "minecraft:blue_shulker_box", "minecraft:brown_shulker_box", "minecraft:cyan_shulker_box",
    "minecraft:dropper", "minecraft:gray_shulker_box", "minecraft:green_shulker_box", "minecraft:light_blue_shulker_box",
    "minecraft:light_gray_shulker_box", "minecraft:lime_shulker_box", "minecraft:magenta_shulker_box", "minecraft:netherite_ingot",
    "minecraft:orange_shulker_box", "minecraft:pink_shulker_box", "minecraft:purple_shulker_box", "minecraft:red_shulker_box",
    "minecraft:redstone_torch", "minecraft:shulker_box", "minecraft:shulker_shell", "minecraft:white_shulker_box",
    "minecraft:yellow_shulker_box", "minecraft:chest",
];

/// Sophisticated Storage's items besides its storages (ModItems, without the Chipped upgrades).
pub const STORAGE_ITEM_IDS: [&str; 61] = [
    "pickup_upgrade", "advanced_pickup_upgrade", "filter_upgrade", "advanced_filter_upgrade", "magnet_upgrade",
    "advanced_magnet_upgrade", "feeding_upgrade", "advanced_feeding_upgrade", "compacting_upgrade", "advanced_compacting_upgrade",
    "void_upgrade", "advanced_void_upgrade", "smelting_upgrade", "auto_smelting_upgrade", "smoking_upgrade", "auto_smoking_upgrade",
    "blasting_upgrade", "auto_blasting_upgrade", "crafting_upgrade", "stonecutter_upgrade", "stack_upgrade_tier_1",
    "stack_upgrade_tier_1_plus", "stack_upgrade_tier_2", "stack_upgrade_tier_3", "stack_upgrade_tier_4", "stack_upgrade_tier_5",
    "stack_downgrade_tier_1", "stack_downgrade_tier_2", "stack_downgrade_tier_3", "stack_upgrade_omega_tier", "jukebox_upgrade",
    "advanced_jukebox_upgrade", "pump_upgrade", "advanced_pump_upgrade", "xp_pump_upgrade", "compression_upgrade",
    "hopper_upgrade", "advanced_hopper_upgrade", "infinity_upgrade", "survival_infinity_upgrade", "basic_tier_upgrade",
    "basic_to_copper_tier_upgrade", "basic_to_iron_tier_upgrade", "basic_to_gold_tier_upgrade", "basic_to_diamond_tier_upgrade",
    "basic_to_netherite_tier_upgrade", "copper_to_iron_tier_upgrade", "copper_to_gold_tier_upgrade",
    "copper_to_diamond_tier_upgrade", "copper_to_netherite_tier_upgrade", "iron_to_gold_tier_upgrade",
    "iron_to_diamond_tier_upgrade", "iron_to_netherite_tier_upgrade", "gold_to_diamond_tier_upgrade",
    "gold_to_netherite_tier_upgrade", "diamond_to_netherite_tier_upgrade", "upgrade_base", "packing_tape", "storage_tool",
    "debug_tool", "paintbrush",
];

type Keys = HashMap<String, ItemKey>;

pub fn keys(r: &mut dyn Registrar, with_backpacks: bool, with_storage: bool) -> Keys {
    let mut m = Keys::new();
    let mut ids: Vec<String> = VANILLA_IDS.iter().map(|s| s.to_string()).collect();
    ids.extend(DYES.iter().map(|d| d.0.to_string()));
    if with_backpacks {
        ids.extend(BACKPACK_ITEM_IDS.iter().map(|s| format!("sophisticatedbackpacks:{s}")));
    }
    if with_storage {
        ids.extend(STORAGE_VANILLA_IDS.iter().map(|s| s.to_string()));
        ids.extend(STORAGE_ITEM_IDS.iter().map(|s| format!("sophisticatedstorage:{s}")));
        ids.extend(crate::storage_items::STORAGE.iter().map(|s| s.id.to_string()));
    }
    for id in ids {
        let k = r.item_key(&id);
        m.insert(id, k);
    }
    m
}

// ---- upgrade_next_tier ----

#[derive(Clone, Debug, mod_api::Wire)]
pub struct Entry {
    pub symbol: String,
    pub items: Vec<String>,
}

/// A shaped recipe (`pattern`, `key`, `copy` the kept symbol) or, when `ingredients` is set, a shapeless one
/// (`copy_index` the kept ingredient, -1 none): StorageTierUpgradeShapelessRecipe, ShulkerBoxFromVanillaShapelessRecipe.
#[derive(Clone, Debug, mod_api::Wire)]
pub struct Shaped {
    pub pattern: Vec<String>,
    pub key: Vec<Entry>,
    pub result: String,
    pub count: u32,
    pub copy: String,
    pub ingredients: Vec<Entry>,
    pub copy_index: i32,
}

pub struct UpgradeNextTier {
    pub keys: Keys,
}

/// The pattern with its empty outer rows and columns cut off.
fn trim_pattern(p: &[String]) -> Vec<Vec<char>> {
    let rows: Vec<Vec<char>> = p.iter().map(|r| r.chars().collect()).collect();
    let w = rows.iter().map(Vec::len).max().unwrap_or(0);
    let filled = |r: usize, c: usize| rows[r].get(c).is_some_and(|ch| *ch != ' ');
    let used_rows: Vec<usize> = (0..rows.len()).filter(|&r| (0..w).any(|c| filled(r, c))).collect();
    let used_cols: Vec<usize> = (0..w).filter(|&c| (0..rows.len()).any(|r| filled(r, c))).collect();
    let (Some(&r0), Some(&r1), Some(&c0), Some(&c1)) = (used_rows.first(), used_rows.last(), used_cols.first(), used_cols.last()) else {
        return Vec::new();
    };
    (r0..=r1).map(|r| (c0..=c1).map(|c| rows[r].get(c).copied().unwrap_or(' ')).collect()).collect()
}

/// The grid with its empty outer rows and columns cut off: (width, height, cells).
fn trim_grid(g: &CraftingGrid) -> (usize, usize, Vec<Option<ItemStack>>) {
    let (w, h) = (g.width as usize, g.height as usize);
    let at = |x: usize, y: usize| g.cells.get(y * w + x).cloned().flatten();
    let rows: Vec<usize> = (0..h).filter(|&y| (0..w).any(|x| at(x, y).is_some())).collect();
    let cols: Vec<usize> = (0..w).filter(|&x| (0..h).any(|y| at(x, y).is_some())).collect();
    let (Some(&y0), Some(&y1), Some(&x0), Some(&x1)) = (rows.first(), rows.last(), cols.first(), cols.last()) else {
        return (0, 0, Vec::new());
    };
    let mut cells = Vec::new();
    for y in y0..=y1 {
        for x in x0..=x1 {
            cells.push(at(x, y));
        }
    }
    (x1 - x0 + 1, y1 - y0 + 1, cells)
}

impl UpgradeNextTier {
    fn is(&self, items: &[String], s: &ItemStack) -> bool {
        items.iter().any(|id| self.keys.get(id).is_some_and(|k| *k == s.item()))
    }

    /// A shapeless match: each grid stack taken by one ingredient; the stack the kept ingredient took.
    fn shapeless(&self, v: &Shaped, grid: &CraftingGrid) -> Option<Option<ItemStack>> {
        let stacks: Vec<&ItemStack> = grid.cells.iter().flatten().collect();
        if stacks.len() != v.ingredients.len() {
            return None;
        }
        let mut used = vec![false; stacks.len()];
        let mut kept = None;
        for (i, e) in v.ingredients.iter().enumerate() {
            let j = (0..stacks.len()).find(|&j| !used[j] && self.is(&e.items, stacks[j]))?;
            used[j] = true;
            if i as i32 == v.copy_index {
                kept = Some(stacks[j].clone());
            }
        }
        Some(kept)
    }

    fn accepts(&self, v: &Shaped, symbol: char, stack: &Option<ItemStack>) -> bool {
        if symbol == ' ' {
            return stack.is_none();
        }
        let Some(s) = stack else { return false };
        let Some(e) = v.key.iter().find(|e| e.symbol.starts_with(symbol)) else { return false };
        e.items.iter().any(|id| self.keys.get(id).is_some_and(|k| *k == s.item()))
    }

    /// The grid cell of each pattern cell, if the grid matches (plain or mirrored).
    fn layout(&self, v: &Shaped, grid: &CraftingGrid) -> Option<(Vec<Vec<char>>, Vec<Option<ItemStack>>, bool)> {
        let p = trim_pattern(&v.pattern);
        let (w, h, cells) = trim_grid(grid);
        if p.is_empty() || p.len() != h || p[0].len() != w {
            return None;
        }
        for mirrored in [false, true] {
            let ok = (0..h).all(|y| {
                (0..w).all(|x| {
                    let px = if mirrored { w - 1 - x } else { x };
                    self.accepts(v, p[y][px], &cells[y * w + x])
                })
            });
            if ok {
                return Some((p, cells, mirrored));
            }
        }
        None
    }
}

impl CraftingRecipeKind for UpgradeNextTier {
    type Value = Shaped;

    fn codec(&self) -> Option<CraftingRecipeCodec<Shaped>> {
        Some(CraftingRecipeCodec::wire())
    }

    fn decode(&self, json: &str) -> Result<Shaped, String> {
        let d: Value = mod_api::serde_json::from_str(json).map_err(|e| e.to_string())?;
        let ids = |v: &Value| -> Result<Vec<String>, String> {
            let items: Vec<String> = match v {
                Value::String(s) => vec![s.clone()],
                Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
                _ => return Err("bad ingredient".into()),
            };
            if let Some(id) = items.iter().find(|id| !self.keys.contains_key(*id)) {
                return Err(format!("unknown item {id}"));
            }
            Ok(items)
        };
        if let Some(list) = d["ingredients"].as_array() {
            let mut ingredients = Vec::new();
            for (i, v) in list.iter().enumerate() {
                ingredients.push(Entry { symbol: i.to_string(), items: ids(v)? });
            }
            let result = d["result"]["id"].as_str().ok_or("no result")?.to_string();
            if !self.keys.contains_key(&result) {
                return Err(format!("unknown result {result}"));
            }
            let count = d["result"]["count"].as_u64().unwrap_or(1) as u32;
            let copy_index = d["copy_index"].as_i64().unwrap_or(-1) as i32;
            return Ok(Shaped { pattern: Vec::new(), key: Vec::new(), result, count, copy: String::new(), ingredients, copy_index });
        }
        let pattern: Vec<String> = d["pattern"].as_array().ok_or("no pattern")?.iter().filter_map(|r| r.as_str().map(String::from)).collect();
        let mut key = Vec::new();
        for (k, v) in d["key"].as_object().ok_or("no key")? {
            let items: Vec<String> = match v {
                Value::String(s) => vec![s.clone()],
                Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
                _ => return Err(format!("bad ingredient {k}")),
            };
            for id in &items {
                if !self.keys.contains_key(id) {
                    return Err(format!("unknown item {id}"));
                }
            }
            key.push(Entry { symbol: k.clone(), items });
        }
        let result = d["result"]["id"].as_str().ok_or("no result")?.to_string();
        if !self.keys.contains_key(&result) {
            return Err(format!("unknown result {result}"));
        }
        let count = d["result"]["count"].as_u64().unwrap_or(1) as u32;
        let copy = d["copy"].as_str().unwrap_or("").to_string();
        Ok(Shaped { pattern, key, result, count, copy, ingredients: Vec::new(), copy_index: -1 })
    }

    fn write(&self, v: &Shaped) -> Result<String, String> {
        if !v.ingredients.is_empty() {
            let ings: Vec<Value> = v.ingredients.iter().map(|e| Value::from(e.items.clone())).collect();
            return Ok(mod_api::serde_json::json!({"ingredients": ings, "result": {"id": v.result, "count": v.count}, "copy_index": v.copy_index}).to_string());
        }
        let key: mod_api::serde_json::Map<String, Value> = v.key.iter().map(|e| (e.symbol.clone(), Value::from(e.items.clone()))).collect();
        Ok(mod_api::serde_json::json!({"pattern": v.pattern, "key": key, "result": {"id": v.result, "count": v.count}, "copy": v.copy}).to_string())
    }

    fn matches(&self, v: &Shaped, grid: CraftingGrid) -> bool {
        if !v.ingredients.is_empty() {
            return self.shapeless(v, &grid).is_some();
        }
        self.layout(v, &grid).is_some()
    }

    fn assemble(&self, v: &Shaped, grid: CraftingGrid) -> Option<ItemStack> {
        if !v.ingredients.is_empty() {
            let kept = self.shapeless(v, &grid)?;
            let mut out = ItemStack::new(*self.keys.get(&v.result)?, v.count.max(1))?;
            if let Some(src) = kept
                && let Some(c) = src.components()
            {
                let _ = out.apply_components(c.as_nbt());
                renamed(&mut out, &v.result);
            }
            return Some(out);
        }
        let (p, cells, mirrored) = self.layout(v, &grid)?;
        let mut out = ItemStack::new(*self.keys.get(&v.result)?, v.count.max(1))?;
        if let Some(sym) = v.copy.chars().next() {
            let w = p[0].len();
            let src = cells.iter().enumerate().find_map(|(i, c)| {
                let (x, y) = (i % w, i / w);
                let px = if mirrored { w - 1 - x } else { x };
                (p[y][px] == sym).then(|| c.clone()).flatten()
            });
            if let Some(src) = src
                && let Some(c) = src.components()
            {
                let _ = out.apply_components(c.as_nbt());
                renamed(&mut out, &v.result);
            }
        }
        Some(out)
    }
}

/// A storage result named after itself, not after the storage its components came from.
fn renamed(out: &mut ItemStack, result: &str) {
    if let Some(s) = crate::storage_items::STORAGE.iter().find(|s| s.id == result) {
        crate::storage::fix_name(out, s);
    }
}

// ---- storage_dye ----

/// `main_key` / `accent_key`: where the colours go (the backpacks' `clothColor` / `borderColor`, the storages'
/// `mainColor` / `accentColor`).
#[derive(Clone, Debug, mod_api::Wire)]
pub struct Dye {
    pub storage: Vec<String>,
    pub main_default: i32,
    pub accent_default: i32,
    pub main_key: String,
    pub accent_key: String,
}

pub struct StorageDye {
    pub keys: Keys,
}

impl StorageDye {
    fn dye_color(&self, s: &ItemStack) -> Option<u32> {
        DYES.iter().find(|(id, _)| self.keys.get(*id).is_some_and(|k| *k == s.item())).map(|d| d.1)
    }

    fn is_storage(&self, v: &Dye, s: &ItemStack) -> bool {
        v.storage.iter().any(|id| self.keys.get(id).is_some_and(|k| *k == s.item()))
    }
}

/// ColorHelper.calculateColor.
pub fn calculate_color(base: i32, default: i32, dyes: &[u32]) -> i32 {
    if dyes.is_empty() {
        return base;
    }
    let mut rgb = [0i64; 3];
    let mut sum_max = 0i64;
    let mut n = 0i64;
    if base != default {
        let (r, g, b) = (i64::from((base >> 16) & 255), i64::from((base >> 8) & 255), i64::from(base & 255));
        sum_max += r.max(g).max(b);
        rgb[0] += r;
        rgb[1] += g;
        rgb[2] += b;
        n += 1;
    }
    for d in dyes {
        let (r, g, b) = (i64::from((d >> 16) & 255), i64::from((d >> 8) & 255), i64::from(d & 255));
        sum_max += r.max(g).max(b);
        rgb[0] += r;
        rgb[1] += g;
        rgb[2] += b;
        n += 1;
    }
    let (mut r, mut g, mut b) = (rgb[0] / n, rgb[1] / n, rgb[2] / n);
    let avg_max = sum_max as f32 / n as f32;
    let max_avg = r.max(g).max(b) as f32;
    if max_avg > 0.0 {
        r = (r as f32 * avg_max / max_avg) as i64;
        g = (g as f32 * avg_max / max_avg) as i64;
        b = (b as f32 * avg_max / max_avg) as i64;
    }
    (((r << 8) + g) << 8 | b) as i32
}

/// The stack's colours (`clothColor`, `borderColor`), or the defaults.
pub fn colors(stack: &ItemStack, main_default: i32, accent_default: i32) -> (i32, i32) {
    let d = custom_data(stack);
    (get_int(&d, "clothColor").unwrap_or(main_default), get_int(&d, "borderColor").unwrap_or(accent_default))
}

/// Writes the colours: the jar's keys, and `minecraft:custom_model_data` colours, which the item model's tints read.
pub fn set_colors(stack: &mut ItemStack, main: i32, accent: i32) {
    let mut d = custom_data(stack);
    put(&mut d, "clothColor", Tag::Int(main));
    put(&mut d, "borderColor", Tag::Int(accent));
    set_custom_data(stack, d);
    let mut patch = mod_api::Compound::new();
    patch.insert("minecraft:custom_model_data".into(), Tag::compound([("colors", Tag::List(vec![Tag::Int(main), Tag::Int(accent)]))]));
    let _ = stack.apply_components(&patch);
}

impl CraftingRecipeKind for StorageDye {
    type Value = Dye;

    fn codec(&self) -> Option<CraftingRecipeCodec<Dye>> {
        Some(CraftingRecipeCodec::wire())
    }

    fn decode(&self, json: &str) -> Result<Dye, String> {
        let d: Value = mod_api::serde_json::from_str(json).map_err(|e| e.to_string())?;
        let storage: Vec<String> = d["storage"].as_array().ok_or("no storage")?.iter().filter_map(|x| x.as_str().map(String::from)).collect();
        if let Some(id) = storage.iter().find(|id| !self.keys.contains_key(*id)) {
            return Err(format!("unknown item {id}"));
        }
        Ok(Dye {
            storage,
            main_default: d["main_default"].as_i64().unwrap_or(0) as i32,
            accent_default: d["accent_default"].as_i64().unwrap_or(0) as i32,
            main_key: d["main_key"].as_str().unwrap_or("clothColor").to_string(),
            accent_key: d["accent_key"].as_str().unwrap_or("borderColor").to_string(),
        })
    }

    fn write(&self, v: &Dye) -> Result<String, String> {
        Ok(mod_api::serde_json::json!({"storage": v.storage, "main_default": v.main_default, "accent_default": v.accent_default,
            "main_key": v.main_key, "accent_key": v.accent_key}).to_string())
    }

    fn matches(&self, v: &Dye, grid: CraftingGrid) -> bool {
        let mut storage = 0;
        let mut dyes = 0;
        for s in grid.cells.iter().flatten() {
            if self.is_storage(v, s) {
                storage += 1;
            } else if self.dye_color(s).is_some() {
                dyes += 1;
            } else {
                return false;
            }
        }
        storage == 1 && dyes > 0
    }

    fn assemble(&self, v: &Dye, grid: CraftingGrid) -> Option<ItemStack> {
        let w = grid.width as usize;
        let mut storage = None;
        let mut column_dyes: Vec<(usize, u32)> = Vec::new();
        for (i, c) in grid.cells.iter().enumerate() {
            let Some(s) = c else { continue };
            if self.is_storage(v, s) {
                storage = Some((i % w, s.clone()));
            } else {
                column_dyes.push((i % w, self.dye_color(s)?));
            }
        }
        let (col, mut out) = storage?;
        out.set_count(1).ok()?;
        let main: Vec<u32> = column_dyes.iter().filter(|(c, _)| *c <= col).map(|d| d.1).collect();
        let trim: Vec<u32> = column_dyes.iter().filter(|(c, _)| *c >= col).map(|d| d.1).collect();
        if v.main_key != "clothColor" {
            // A storage: its own keys; StorageBlockItem keeps a colour only when dyed (no default colour).
            let mut d = custom_data(&out);
            let get = |d: &mod_api::Compound, k: &str| get_int(d, k).unwrap_or(-1);
            let (m, a) = (get(&d, &v.main_key), get(&d, &v.accent_key));
            if !main.is_empty() {
                put(&mut d, &v.main_key, Tag::Int(calculate_color(m, -1, &main)));
            }
            if !trim.is_empty() {
                put(&mut d, &v.accent_key, Tag::Int(calculate_color(a, -1, &trim)));
            }
            set_custom_data(&mut out, d);
            return Some(out);
        }
        let (m, a) = colors(&out, v.main_default, v.accent_default);
        set_colors(&mut out, calculate_color(m, v.main_default, &main), calculate_color(a, v.accent_default, &trim));
        Some(out)
    }
}

/// Whether a custom_data compound already says something about colours.
pub fn _has_colors(stack: &ItemStack) -> bool {
    custom_data(stack).has("clothColor")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn red_dye_on_a_default_backpack() {
        // One red dye on the default cloth gives the dye's colour (the default isn't averaged in).
        assert_eq!(calculate_color(13394234, 13394234, &[0xB02E26]), 0xB02E26);
        assert_eq!(calculate_color(5, 13394234, &[]), 5);
    }

    #[test]
    fn patterns_trim() {
        let p = trim_pattern(&[" I ".into(), "IBI".into(), " I ".into()]);
        assert_eq!(p.len(), 3);
        let p = trim_pattern(&["   ".into(), " B ".into(), "   ".into()]);
        assert_eq!(p, vec![vec!['B']]);
    }
}
