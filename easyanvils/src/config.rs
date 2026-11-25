use mod_api::{Config, ConfigKind, Registrar, SettingSpec, Wire};
use std::sync::{PoisonError, RwLock};

pub const MOD_ID: &str = "easyanvils";

pub const SETTING_RENDER_ANVIL_CONTENTS: &str = "render_anvil_contents";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Wire)]
pub enum PriorWorkPenalty {
    None,
    Vanilla,
    Limited,
}

impl PriorWorkPenalty {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "NONE" => Some(Self::None),
            "VANILLA" => Some(Self::Vanilla),
            "LIMITED" => Some(Self::Limited),
            _ => None,
        }
    }

    /// Maps an item's raw `minecraft:repair_cost` to the effective prior-work penalty
    /// (`PriorWorkPenalty.operator`).
    pub fn apply(self, item_repair_cost: i32, max_increase: i32) -> i32 {
        match self {
            Self::None => 0,
            Self::Vanilla => item_repair_cost.max(0),
            Self::Limited => {
                let repairs = repair_cost_to_repairs(item_repair_cost);
                limited_repair_cost(repairs, max_increase.max(1))
            }
        }
    }
}

/// Vanilla `PriorWorkPenalty.repairCostToRepairs`.
pub fn repair_cost_to_repairs(mut item_repair_cost: i32) -> i32 {
    if item_repair_cost <= 0 {
        return 0;
    }
    item_repair_cost = item_repair_cost.saturating_add(1);
    let mut prior_repairs = 0;
    while item_repair_cost >= 2 {
        item_repair_cost /= 2;
        prior_repairs += 1;
    }
    prior_repairs
}

/// `PriorWorkPenalty.limitedRepairCost`.
pub fn limited_repair_cost(prior_repairs: i32, max_increase: i32) -> i32 {
    let cap = max_increase.max(1);
    let mut item_repair_cost: i32 = 0;
    for _ in 0..prior_repairs.max(0) {
        let step = item_repair_cost.saturating_add(1).min(cap);
        item_repair_cost = item_repair_cost.saturating_add(step);
    }
    item_repair_cost
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Wire)]
pub enum RenameAndRepairCost {
    Vanilla,
    Fixed,
    Limited,
}

impl RenameAndRepairCost {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "VANILLA" => Some(Self::Vanilla),
            "FIXED" => Some(Self::Fixed),
            "LIMITED" => Some(Self::Limited),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Wire)]
pub enum FreeRenames {
    Never,
    AllItems,
    NameTagsOnly,
}

impl FreeRenames {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "NEVER" => Some(Self::Never),
            "ALL_ITEMS" => Some(Self::AllItems),
            "NAME_TAGS_ONLY" => Some(Self::NameTagsOnly),
            _ => None,
        }
    }

    pub fn is_free_for_item_id(self, item_id: &str) -> bool {
        match self {
            Self::Never => false,
            Self::AllItems => true,
            Self::NameTagsOnly => item_id == "minecraft:name_tag",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Wire)]
pub struct ServerConfig {
    pub convert_vanilla_anvil_when_interacting: bool,
    // [prior_work_penalty]
    pub prior_work_penalty: PriorWorkPenalty,
    pub maximum_prior_work_penalty_increase: i32,
    pub rename_and_repair_costs: RenameAndRepairCost,
    pub penalty_free_renames_and_repairs: bool,
    pub penalty_free_enchants_for_books: bool,
    // [costs]
    pub too_expensive_limit: i32,
    pub free_renames: FreeRenames,
    pub halved_book_costs: bool,
    pub repair_with_material_unit_cost: i32,
    pub repair_with_material_restored_durability: f64,
    pub repair_with_other_item_cost: i32,
    pub repair_with_other_item_bonus_durability: f64,
    // [miscellaneous]
    pub anvil_repairing: bool,
    pub anvil_break_chance: f64,
    pub risk_free_anvil_renaming: bool,
    pub renaming_supports_formatting: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            convert_vanilla_anvil_when_interacting: true,
            prior_work_penalty: PriorWorkPenalty::Limited,
            maximum_prior_work_penalty_increase: 4,
            rename_and_repair_costs: RenameAndRepairCost::Fixed,
            penalty_free_renames_and_repairs: true,
            penalty_free_enchants_for_books: true,
            too_expensive_limit: -1,
            free_renames: FreeRenames::AllItems,
            halved_book_costs: true,
            repair_with_material_unit_cost: 1,
            repair_with_material_restored_durability: 0.25,
            repair_with_other_item_cost: 2,
            repair_with_other_item_bonus_durability: 0.12,
            anvil_repairing: true,
            anvil_break_chance: 0.05,
            risk_free_anvil_renaming: true,
            renaming_supports_formatting: true,
        }
    }
}

impl ServerConfig {
    pub fn default_toml() -> &'static str {
        r#"# Easy Anvils server configuration
# Leftover vanilla anvils are handled by Easy Anvils when interacting.
convert_vanilla_anvil_when_interacting = true

[prior_work_penalty]
# Controls how working an item in the anvil multiple times affects future costs: "LIMITED", "VANILLA", or "NONE".
prior_work_penalty = "LIMITED"
# Value to use when prior_work_penalty is "LIMITED" (>= 1).
maximum_prior_work_penalty_increase = 4
# How prior work penalty affects renames and repairs: "FIXED", "LIMITED", or "VANILLA".
rename_and_repair_costs = "FIXED"
# Prevents prior work penalty from increasing when an item is only renamed or repaired.
penalty_free_renames_and_repairs = true
# Prevents prior work penalty from increasing when combining two enchanted books.
penalty_free_enchants_for_books = true

[costs]
# Max enchantment level cost allowed in an anvil. Set to -1 to disable the limit (vanilla is 40).
too_expensive_limit = -1
# Free renaming in anvils: "ALL_ITEMS", "NAME_TAGS_ONLY", or "NEVER".
free_renames = "ALL_ITEMS"
# Costs for applying enchantments from enchanted books are halved.
halved_book_costs = true
# Additional level cost for each valid repair material unit (>= 0).
repair_with_material_unit_cost = 1
# Restored fraction of full durability per repair material unit (0.0..=1.0).
repair_with_material_restored_durability = 0.25
# Additional level cost for combining an item with another damaged item of the same kind (>= 0).
repair_with_other_item_cost = 2
# Bonus durability fraction when combining two items of the same kind (0.0..=1.0).
repair_with_other_item_bonus_durability = 0.12

[miscellaneous]
# Allow repairing an anvil by one damage stage with an iron block (right-click or dispenser).
anvil_repairing = true
# Chance the anvil takes a stage of damage on use in survival mode (0.0..=1.0; vanilla is 0.12).
anvil_break_chance = 0.05
# Solely renaming items in an anvil will never cause the anvil to break.
risk_free_anvil_renaming = true
# The naming field in anvils supports formatting codes (§) for custom text colours and styles.
renaming_supports_formatting = true
"#
    }

    pub fn parse_toml(source: &str) -> Self {
        let mut cfg = Self::default();
        for (key, value) in parse_flat_toml(source) {
            match key {
                "convert_vanilla_anvil_when_interacting" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.convert_vanilla_anvil_when_interacting = v;
                    }
                }
                "prior_work_penalty" => {
                    if let Some(v) = PriorWorkPenalty::parse(unquote(value)) {
                        cfg.prior_work_penalty = v;
                    }
                }
                "maximum_prior_work_penalty_increase" => {
                    if let Some(v) = parse_i32(value) {
                        cfg.maximum_prior_work_penalty_increase = v.max(1);
                    }
                }
                "rename_and_repair_costs" => {
                    if let Some(v) = RenameAndRepairCost::parse(unquote(value)) {
                        cfg.rename_and_repair_costs = v;
                    }
                }
                "penalty_free_renames_and_repairs" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.penalty_free_renames_and_repairs = v;
                    }
                }
                "penalty_free_enchants_for_books" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.penalty_free_enchants_for_books = v;
                    }
                }
                "too_expensive_limit" => {
                    if let Some(v) = parse_i32(value) {
                        cfg.too_expensive_limit = v.max(-1);
                    }
                }
                "free_renames" => {
                    if let Some(v) = FreeRenames::parse(unquote(value)) {
                        cfg.free_renames = v;
                    }
                }
                "halved_book_costs" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.halved_book_costs = v;
                    }
                }
                "repair_with_material_unit_cost" => {
                    if let Some(v) = parse_i32(value) {
                        cfg.repair_with_material_unit_cost = v.max(0);
                    }
                }
                "repair_with_material_restored_durability" => {
                    if let Some(v) = parse_f64(value) {
                        cfg.repair_with_material_restored_durability = v.clamp(0.0, 1.0);
                    }
                }
                "repair_with_other_item_cost" => {
                    if let Some(v) = parse_i32(value) {
                        cfg.repair_with_other_item_cost = v.max(0);
                    }
                }
                "repair_with_other_item_bonus_durability" => {
                    if let Some(v) = parse_f64(value) {
                        cfg.repair_with_other_item_bonus_durability = v.clamp(0.0, 1.0);
                    }
                }
                "anvil_repairing" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.anvil_repairing = v;
                    }
                }
                "anvil_break_chance" => {
                    if let Some(v) = parse_f64(value) {
                        cfg.anvil_break_chance = v.clamp(0.0, 1.0);
                    }
                }
                "risk_free_anvil_renaming" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.risk_free_anvil_renaming = v;
                    }
                }
                "renaming_supports_formatting" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.renaming_supports_formatting = v;
                    }
                }
                _ => {}
            }
        }
        cfg
    }
}

static SERVER_CONFIG: RwLock<ServerConfig> = RwLock::new(ServerConfig {
    convert_vanilla_anvil_when_interacting: true,
    prior_work_penalty: PriorWorkPenalty::Limited,
    maximum_prior_work_penalty_increase: 4,
    rename_and_repair_costs: RenameAndRepairCost::Fixed,
    penalty_free_renames_and_repairs: true,
    penalty_free_enchants_for_books: true,
    too_expensive_limit: -1,
    free_renames: FreeRenames::AllItems,
    halved_book_costs: true,
    repair_with_material_unit_cost: 1,
    repair_with_material_restored_durability: 0.25,
    repair_with_other_item_cost: 2,
    repair_with_other_item_bonus_durability: 0.12,
    anvil_repairing: true,
    anvil_break_chance: 0.05,
    risk_free_anvil_renaming: true,
    renaming_supports_formatting: true,
});

static SYNCED_SERVER_CONFIG: RwLock<ServerConfig> = RwLock::new(ServerConfig {
    convert_vanilla_anvil_when_interacting: true,
    prior_work_penalty: PriorWorkPenalty::Limited,
    maximum_prior_work_penalty_increase: 4,
    rename_and_repair_costs: RenameAndRepairCost::Fixed,
    penalty_free_renames_and_repairs: true,
    penalty_free_enchants_for_books: true,
    too_expensive_limit: -1,
    free_renames: FreeRenames::AllItems,
    halved_book_costs: true,
    repair_with_material_unit_cost: 1,
    repair_with_material_restored_durability: 0.25,
    repair_with_other_item_cost: 2,
    repair_with_other_item_bonus_durability: 0.12,
    anvil_repairing: true,
    anvil_break_chance: 0.05,
    risk_free_anvil_renaming: true,
    renaming_supports_formatting: true,
});

pub fn server_config() -> ServerConfig {
    SERVER_CONFIG
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

pub fn set_server_config(cfg: ServerConfig) {
    *SERVER_CONFIG
        .write()
        .unwrap_or_else(PoisonError::into_inner) = cfg;
}

pub fn synced_server_config() -> ServerConfig {
    SYNCED_SERVER_CONFIG
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

pub fn set_synced_server_config(cfg: ServerConfig) {
    *SYNCED_SERVER_CONFIG
        .write()
        .unwrap_or_else(PoisonError::into_inner) = cfg;
}

pub fn register_mod_configs(configs: &mut Vec<Config>) {
    configs.push(Config::new(
        MOD_ID,
        ConfigKind::Server,
        ServerConfig::default_toml(),
    ));
}

pub fn register_settings(r: &mut dyn Registrar) {
    let _ = r.register_setting(
        SettingSpec::toggle(
            SETTING_RENDER_ANVIL_CONTENTS,
            "easyanvils.setting.render_anvil_contents",
            true,
        )
        .tooltip("easyanvils.setting.render_anvil_contents.tooltip"),
    );
}

fn parse_flat_toml(source: &str) -> Vec<(&str, &str)> {
    let mut pairs = Vec::new();
    for raw_line in source.lines() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() || line.starts_with('[') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            pairs.push((key.trim(), value.trim()));
        }
    }
    pairs
}

fn strip_comment(line: &str) -> &str {
    let mut in_quotes = false;
    for (idx, ch) in line.char_indices() {
        if ch == '"' {
            in_quotes = !in_quotes;
        } else if ch == '#' && !in_quotes {
            return &line[..idx];
        }
    }
    line
}

fn unquote(raw: &str) -> &str {
    raw.trim()
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| {
            raw.trim()
                .strip_prefix('\'')
                .and_then(|s| s.strip_suffix('\''))
        })
        .unwrap_or_else(|| raw.trim())
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn parse_i32(raw: &str) -> Option<i32> {
    raw.trim().parse::<i32>().ok()
}

fn parse_f64(raw: &str) -> Option<f64> {
    let v = raw.trim().parse::<f64>().ok()?;
    v.is_finite().then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_server_toml_parses_to_default() {
        let parsed = ServerConfig::parse_toml(ServerConfig::default_toml());
        assert_eq!(parsed, ServerConfig::default());
    }

    #[test]
    fn server_toml_clamps_ranges_and_parses_enums() {
        let raw = r#"
            convert_vanilla_anvil_when_interacting = false
            [prior_work_penalty]
            prior_work_penalty = "none"
            maximum_prior_work_penalty_increase = 0
            rename_and_repair_costs = "LIMITED"
            penalty_free_renames_and_repairs = false
            penalty_free_enchants_for_books = false
            [costs]
            too_expensive_limit = -50
            free_renames = "NAME_TAGS_ONLY"
            halved_book_costs = false
            repair_with_material_unit_cost = -3
            repair_with_material_restored_durability = 2.5
            repair_with_other_item_cost = -1
            repair_with_other_item_bonus_durability = -0.5
            [miscellaneous]
            anvil_repairing = false
            anvil_break_chance = 5.0
            risk_free_anvil_renaming = false
            renaming_supports_formatting = false
        "#;
        let parsed = ServerConfig::parse_toml(raw);
        assert!(!parsed.convert_vanilla_anvil_when_interacting);
        assert_eq!(parsed.prior_work_penalty, PriorWorkPenalty::None);
        assert_eq!(parsed.maximum_prior_work_penalty_increase, 1);
        assert_eq!(parsed.rename_and_repair_costs, RenameAndRepairCost::Limited);
        assert!(!parsed.penalty_free_renames_and_repairs);
        assert!(!parsed.penalty_free_enchants_for_books);
        assert_eq!(parsed.too_expensive_limit, -1);
        assert_eq!(parsed.free_renames, FreeRenames::NameTagsOnly);
        assert!(!parsed.halved_book_costs);
        assert_eq!(parsed.repair_with_material_unit_cost, 0);
        assert_eq!(parsed.repair_with_material_restored_durability, 1.0);
        assert_eq!(parsed.repair_with_other_item_cost, 0);
        assert_eq!(parsed.repair_with_other_item_bonus_durability, 0.0);
        assert!(!parsed.anvil_repairing);
        assert_eq!(parsed.anvil_break_chance, 1.0);
        assert!(!parsed.risk_free_anvil_renaming);
        assert!(!parsed.renaming_supports_formatting);
    }

    #[test]
    fn prior_work_penalty_limited_matches_original() {
        // 0 repairs -> 0, 1 -> 1, 2 -> 3, 3 -> 7, 4 -> 11, 5 -> 15 (cap = 4)
        assert_eq!(PriorWorkPenalty::Limited.apply(0, 4), 0);
        assert_eq!(PriorWorkPenalty::Limited.apply(1, 4), 1);
        assert_eq!(PriorWorkPenalty::Limited.apply(3, 4), 3);
        assert_eq!(PriorWorkPenalty::Limited.apply(7, 4), 7);
        assert_eq!(PriorWorkPenalty::Limited.apply(15, 4), 11);
        assert_eq!(PriorWorkPenalty::Limited.apply(31, 4), 15);
        assert_eq!(PriorWorkPenalty::Vanilla.apply(31, 4), 31);
        assert_eq!(PriorWorkPenalty::None.apply(31, 4), 0);
    }
}
