use mod_api::{ClientHost, Config, ConfigKind, Registrar, SettingSpec, Wire};
use std::sync::{PoisonError, RwLock};

pub const MOD_ID: &str = "easymagic";

pub const SETTING_KEEP_BOOK: &str = "keep_enchantment_screen_book";
pub const SETTING_CHISELED_TOOLTIP: &str = "chiseled_bookshelf_tooltip";
pub const SETTING_CHISELED_TOOLTIP_X: &str = "chiseled_bookshelf_tooltip_offset_x";
pub const SETTING_CHISELED_TOOLTIP_Y: &str = "chiseled_bookshelf_tooltip_offset_y";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Wire)]
pub enum EnchantmentHint {
    None,
    Single,
    All,
}

impl EnchantmentHint {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "NONE" => Some(Self::None),
            "SINGLE" => Some(Self::Single),
            "ALL" => Some(Self::All),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Wire)]
pub enum ChiseledBookshelfPower {
    None,
    Facing,
    All,
}

impl ChiseledBookshelfPower {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "NONE" => Some(Self::None),
            "FACING" => Some(Self::Facing),
            "ALL" => Some(Self::All),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChiseledBookshelfTooltip {
    Disabled,
    Crouching,
    Enabled,
}

impl ChiseledBookshelfTooltip {
    fn from_choice(index: u32) -> Self {
        match index {
            0 => Self::Disabled,
            1 => Self::Crouching,
            _ => Self::Enabled,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Wire)]
pub struct ServerConfig {
    pub convert_vanilla_enchanting_table_when_interacting: bool,
    pub reroll_enchantments: bool,
    pub reroll_catalyst_cost: i32,
    pub reroll_experience_points_cost: i32,
    pub rerolling_takes_enchantment_levels: bool,
    pub enchantment_hint: EnchantmentHint,
    pub max_enchanting_power: i32,
    pub lenient_bookshelves: bool,
    pub dedicated_reroll_catalyst: bool,
    pub chiseled_bookshelf_enchanting_power: ChiseledBookshelfPower,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            convert_vanilla_enchanting_table_when_interacting: true,
            reroll_enchantments: true,
            reroll_catalyst_cost: 1,
            reroll_experience_points_cost: 5,
            rerolling_takes_enchantment_levels: false,
            enchantment_hint: EnchantmentHint::Single,
            max_enchanting_power: 15,
            lenient_bookshelves: true,
            dedicated_reroll_catalyst: false,
            chiseled_bookshelf_enchanting_power: ChiseledBookshelfPower::Facing,
        }
    }
}

impl ServerConfig {
    pub fn dedicated_reroll_catalyst_active(&self) -> bool {
        self.reroll_enchantments && self.dedicated_reroll_catalyst
    }

    pub fn default_toml() -> &'static str {
        r#"# Easy Magic server configuration
# Leftover vanilla enchanting tables are handled by Easy Magic when interacting.
convert_vanilla_enchanting_table_when_interacting = true
# Choose how many enchantments are shown on the enchanting tooltip: "NONE", "SINGLE", or "ALL".
enchantment_hint = "SINGLE"

[enchanting_power]
# Amount of bookshelves required to perform enchantments at the highest level (0..=50).
max_enchanting_power = 15
# Blocks without a full collision shape (e.g. torches & carpet) do not block bookshelves behind them.
lenient_bookshelves = true
# Do chiseled bookshelves provide enchanting power (1 per 3 books): "NONE", "FACING", or "ALL".
chiseled_bookshelf_enchanting_power = "FACING"

[rerolling]
# Add a button in the enchanting screen to allow for re-rolling enchantments.
reroll_enchantments = true
# Amount of catalyst item taken as a cost for re-rolling enchantments (0..=64).
reroll_catalyst_cost = 1
# Amount of experience points (or levels) taken as a cost for re-rolling enchantments (>= 0).
reroll_experience_points_cost = 5
# Switch re-rolling cost from experience points to full enchantment levels.
rerolling_takes_enchantment_levels = false
# Use items in #easymagic:reroll_catalysts in a dedicated third slot instead of #easymagic:enchanting_catalysts.
dedicated_reroll_catalyst = false
"#
    }

    pub fn parse_toml(source: &str) -> Self {
        let mut cfg = Self::default();
        for (key, value) in parse_flat_toml(source) {
            match key {
                "convert_vanilla_enchanting_table_when_interacting" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.convert_vanilla_enchanting_table_when_interacting = v;
                    }
                }
                "reroll_enchantments" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.reroll_enchantments = v;
                    }
                }
                "reroll_catalyst_cost" => {
                    if let Some(v) = parse_i32(value) {
                        cfg.reroll_catalyst_cost = v.clamp(0, 64);
                    }
                }
                "reroll_experience_points_cost" => {
                    if let Some(v) = parse_i32(value) {
                        cfg.reroll_experience_points_cost = v.max(0);
                    }
                }
                "rerolling_takes_enchantment_levels" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.rerolling_takes_enchantment_levels = v;
                    }
                }
                "enchantment_hint" => {
                    if let Some(v) = EnchantmentHint::parse(unquote(value)) {
                        cfg.enchantment_hint = v;
                    }
                }
                "max_enchanting_power" => {
                    if let Some(v) = parse_i32(value) {
                        cfg.max_enchanting_power = v.clamp(0, 50);
                    }
                }
                "lenient_bookshelves" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.lenient_bookshelves = v;
                    }
                }
                "dedicated_reroll_catalyst" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.dedicated_reroll_catalyst = v;
                    }
                }
                "chiseled_bookshelf_enchanting_power" => {
                    if let Some(v) = ChiseledBookshelfPower::parse(unquote(value)) {
                        cfg.chiseled_bookshelf_enchanting_power = v;
                    }
                }
                _ => {}
            }
        }
        cfg
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientConfig {
    pub keep_enchantment_screen_book: bool,
    pub chiseled_bookshelf_tooltip: ChiseledBookshelfTooltip,
    pub chiseled_bookshelf_tooltip_offset_x: i32,
    pub chiseled_bookshelf_tooltip_offset_y: i32,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            keep_enchantment_screen_book: false,
            chiseled_bookshelf_tooltip: ChiseledBookshelfTooltip::Enabled,
            chiseled_bookshelf_tooltip_offset_x: 0,
            chiseled_bookshelf_tooltip_offset_y: 0,
        }
    }
}

static SERVER_CONFIG: RwLock<ServerConfig> = RwLock::new(ServerConfig {
    convert_vanilla_enchanting_table_when_interacting: true,
    reroll_enchantments: true,
    reroll_catalyst_cost: 1,
    reroll_experience_points_cost: 5,
    rerolling_takes_enchantment_levels: false,
    enchantment_hint: EnchantmentHint::Single,
    max_enchanting_power: 15,
    lenient_bookshelves: true,
    dedicated_reroll_catalyst: false,
    chiseled_bookshelf_enchanting_power: ChiseledBookshelfPower::Facing,
});

static SYNCED_SERVER_CONFIG: RwLock<ServerConfig> = RwLock::new(ServerConfig {
    convert_vanilla_enchanting_table_when_interacting: true,
    reroll_enchantments: true,
    reroll_catalyst_cost: 1,
    reroll_experience_points_cost: 5,
    rerolling_takes_enchantment_levels: false,
    enchantment_hint: EnchantmentHint::Single,
    max_enchanting_power: 15,
    lenient_bookshelves: true,
    dedicated_reroll_catalyst: false,
    chiseled_bookshelf_enchanting_power: ChiseledBookshelfPower::Facing,
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

/// Effective client config read from Options -> Mods -> Easy Magic settings.
pub fn effective_client_config(host: &dyn ClientHost) -> ClientConfig {
    let mut cfg = ClientConfig::default();
    if let Some(v) = host.setting_bool(SETTING_KEEP_BOOK) {
        cfg.keep_enchantment_screen_book = v;
    }
    if let Some(v) = host.setting_choice(SETTING_CHISELED_TOOLTIP) {
        cfg.chiseled_bookshelf_tooltip = ChiseledBookshelfTooltip::from_choice(v);
    }
    if let Some(v) = host.setting_f64(SETTING_CHISELED_TOOLTIP_X) {
        cfg.chiseled_bookshelf_tooltip_offset_x = v.round() as i32;
    }
    if let Some(v) = host.setting_f64(SETTING_CHISELED_TOOLTIP_Y) {
        cfg.chiseled_bookshelf_tooltip_offset_y = v.round() as i32;
    }
    cfg
}

pub fn keep_enchantment_screen_book(client: &ClientConfig, server: &ServerConfig) -> bool {
    client.keep_enchantment_screen_book || !server.reroll_enchantments
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
        SettingSpec::toggle(SETTING_KEEP_BOOK, "easymagic.setting.keep_book", false)
            .tooltip("easymagic.setting.keep_book.tooltip"),
    );
    let _ = r.register_setting(
        SettingSpec::choice(
            SETTING_CHISELED_TOOLTIP,
            "easymagic.setting.chiseled_tooltip",
            [
                "easymagic.setting.chiseled_tooltip.disabled",
                "easymagic.setting.chiseled_tooltip.crouching",
                "easymagic.setting.chiseled_tooltip.enabled",
            ],
            2,
        )
        .tooltip("easymagic.setting.chiseled_tooltip.tooltip"),
    );
    let _ = r.register_setting(
        SettingSpec::range(
            SETTING_CHISELED_TOOLTIP_X,
            "easymagic.setting.chiseled_tooltip_x",
            -200.0,
            200.0,
            1.0,
            0.0,
        )
        .tooltip("easymagic.setting.chiseled_tooltip_x.tooltip"),
    );
    let _ = r.register_setting(
        SettingSpec::range(
            SETTING_CHISELED_TOOLTIP_Y,
            "easymagic.setting.chiseled_tooltip_y",
            -200.0,
            200.0,
            1.0,
            0.0,
        )
        .tooltip("easymagic.setting.chiseled_tooltip_y.tooltip"),
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
            enchantment_hint = "ALL"
            [enchanting_power]
            max_enchanting_power = 999
            lenient_bookshelves = false
            chiseled_bookshelf_enchanting_power = "all"
            [rerolling]
            reroll_catalyst_cost = -5
            reroll_experience_points_cost = -10
            rerolling_takes_enchantment_levels = true
            dedicated_reroll_catalyst = true
        "#;
        let parsed = ServerConfig::parse_toml(raw);
        assert_eq!(parsed.enchantment_hint, EnchantmentHint::All);
        assert_eq!(parsed.max_enchanting_power, 50);
        assert!(!parsed.lenient_bookshelves);
        assert_eq!(
            parsed.chiseled_bookshelf_enchanting_power,
            ChiseledBookshelfPower::All
        );
        assert_eq!(parsed.reroll_catalyst_cost, 0);
        assert_eq!(parsed.reroll_experience_points_cost, 0);
        assert!(parsed.rerolling_takes_enchantment_levels);
        assert!(parsed.dedicated_reroll_catalyst_active());
    }
}
