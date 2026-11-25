//! Falling Leaves configuration model, defaults, TOML persistence, and Options > Mods settings.

use mod_api::{
    ClientHost, ConfigKind, ConfigListener, ConfigSpec, Registrar, SettingKey, SettingSpec,
    SettingValue,
};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock};

pub const MOD_ID: &str = "fallingleaves";
pub const CONFIG_FILE_NAME: &str = "fallingleaves-client.toml";

/// Vanilla 26.3 base falling-leaf chance (`TintedParticleLeavesBlock`: 1%).
pub const DEFAULT_SPAWN_CHANCE: f32 = 0.01;

// Options > Mods setting keys.
pub const SET_ENABLED: &str = "enabled";
pub const SET_ALWAYS_USE_VANILLA_PARTICLES: &str = "always_use_vanilla_particles";
pub const SET_USE_VANILLA_TEXTURES: &str = "use_vanilla_textures";
pub const SET_LEAF_SIZE: &str = "leaf_size";
pub const SET_LEAF_LIFESPAN: &str = "leaf_lifespan";
pub const SET_LEAF_SPAWN_RATE: &str = "leaf_spawn_rate";
pub const SET_CONIFER_LEAF_SPAWN_RATE: &str = "conifer_leaf_spawn_rate";
pub const SET_SNOWFLAKE_SPAWN_RATE: &str = "snowflake_spawn_rate";
pub const SET_DROP_FROM_PLAYER_PLACED: &str = "drop_from_player_placed_blocks";
pub const SET_LEAVES_ON_BLOCK_HIT: &str = "leaves_on_block_hit";
pub const SET_MIN_FREE_SPACE_BELOW: &str = "minimum_free_space_below";
pub const SET_WIND_ENABLED: &str = "wind_enabled";
pub const SET_STARTING_SPAWN_RADIUS: &str = "starting_spawn_radius";
pub const SET_DECAY_SPAWN_RATE_FACTOR: &str = "decay_spawn_rate_factor";
pub const SET_MAX_DECAY_LEAVES: &str = "max_decay_leaves";

/// Which particle implementation a leaf block uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleImplementation {
    Vanilla,
    Regular,
    Conifer,
    Cherry,
}

impl ParticleImplementation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vanilla => "VANILLA",
            Self::Regular => "REGULAR",
            Self::Conifer => "CONIFER",
            Self::Cherry => "CHERRY",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "VANILLA" => Some(Self::Vanilla),
            "REGULAR" => Some(Self::Regular),
            "CONIFER" => Some(Self::Conifer),
            "CHERRY" => Some(Self::Cherry),
            _ => None,
        }
    }
}

/// Per-leaf-block settings matching `LeafSettingsEntry`.
#[derive(Clone, Debug, PartialEq)]
pub struct LeafSettingsEntry {
    pub particle_implementation: ParticleImplementation,
    pub spawn_rate_factor: f32,
    pub is_conifer_block: bool,
    pub spawn_breaking_leaves: bool,
}

impl LeafSettingsEntry {
    pub fn default_for(block_id: &str) -> Self {
        Self {
            particle_implementation: default_implementation(block_id),
            spawn_rate_factor: default_spawn_rate_factor(block_id),
            is_conifer_block: is_default_conifer(block_id),
            spawn_breaking_leaves: true,
        }
    }

    pub fn effective_implementation(&self, config: &FallingLeavesConfig) -> ParticleImplementation {
        if config.always_use_vanilla_particles {
            ParticleImplementation::Vanilla
        } else {
            self.particle_implementation
        }
    }

    pub fn spawn_chance(&self, config: &FallingLeavesConfig) -> f32 {
        let base = if self.is_conifer_block {
            config.base_conifer_leaf_spawn_chance()
        } else {
            config.base_leaf_spawn_chance()
        };
        (self.spawn_rate_factor * base).max(0.0)
    }
}

/// Parsed entry from `leaf_spawners`, e.g. `minecraft:bamboo[leaves=large]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeafSpawnerRule {
    pub block_id: String,
    pub properties: Vec<(String, String)>,
}

impl LeafSpawnerRule {
    pub fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Some(open) = trimmed.find('[') {
            let close = trimmed.rfind(']')?;
            if close <= open {
                return None;
            }
            let block_id = normalize_id(&trimmed[..open]);
            if block_id.is_empty() {
                return None;
            }
            let props_str = &trimmed[open + 1..close];
            let mut properties = Vec::new();
            for pair in props_str.split(',') {
                let pair = pair.trim();
                if pair.is_empty() {
                    continue;
                }
                let (k, v) = pair.split_once('=')?;
                let k = k.trim();
                let v = v.trim();
                if !k.is_empty() && !v.is_empty() {
                    properties.push((k.to_owned(), v.to_owned()));
                }
            }
            Some(Self {
                block_id,
                properties,
            })
        } else {
            let block_id = normalize_id(trimmed);
            if block_id.is_empty() {
                None
            } else {
                Some(Self {
                    block_id,
                    properties: Vec::new(),
                })
            }
        }
    }

    pub fn matches_serialized(&self, block_id: &str, serialized_state: &str) -> bool {
        if self.block_id != block_id {
            return false;
        }
        for (key, expected) in &self.properties {
            if crate::leaf_util::parse_block_state_property(serialized_state, key)
                != Some(expected.as_str())
            {
                return false;
            }
        }
        true
    }
}

fn normalize_id(id: &str) -> String {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        String::new()
    } else if trimmed.contains(':') {
        trimmed.to_owned()
    } else {
        format!("minecraft:{trimmed}")
    }
}

/// Default conifer leaves list matching `ConfigDefaults.isConifer`.
pub fn is_default_conifer(block_id: &str) -> bool {
    matches!(
        block_id,
        "bewitchment:cypress_leaves"
            | "bewitchment:dragons_blood_leaves"
            | "bewitchment:juniper_leaves"
            | "biomemakeover:swamp_cypress_leaves"
            | "byg:araucaria_leaves"
            | "byg:blue_spruce_leaves"
            | "byg:cypress_leaves"
            | "byg:fir_leaves"
            | "byg:orange_spruce_leaves"
            | "byg:pine_leaves"
            | "byg:red_spruce_leaves"
            | "byg:redwood_leaves"
            | "byg:yellow_spruce_leaves"
            | "minecraft:spruce_leaves"
            | "terrestria:cypress_leaves"
            | "terrestria:hemlock_leaves"
            | "terrestria:redwood_leaves"
            | "traverse:fir_leaves"
            | "woods_and_mires:pine_leaves"
    )
}

/// Default particle implementation matching `ConfigDefaults.getImplementation`.
pub fn default_implementation(block_id: &str) -> ParticleImplementation {
    if is_default_conifer(block_id) {
        return ParticleImplementation::Conifer;
    }
    // Untinted vanilla particle leaf blocks default to Vanilla in 2.0.8.
    if is_vanilla_untinted_leaves(block_id) {
        return ParticleImplementation::Vanilla;
    }
    ParticleImplementation::Regular
}

pub fn is_vanilla_untinted_leaves(block_id: &str) -> bool {
    matches!(
        block_id,
        "minecraft:cherry_leaves"
            | "minecraft:pale_oak_leaves"
            | "minecraft:red_poplar_leaves"
            | "minecraft:orange_poplar_leaves"
            | "minecraft:yellow_poplar_leaves"
    )
}

/// Default spawn rate multiplier matching `ConfigDefaults.spawnRateFactor`.
pub fn default_spawn_rate_factor(block_id: &str) -> f32 {
    match block_id {
        // Shrubs and large-leaved trees
        "byg:palm_leaves"
        | "minecraft:jungle_leaves"
        | "promenade:palm_leaves"
        | "terrestria:japanese_maple_shrub_leaves"
        | "terrestria:jungle_palm_leaves"
        | "terrestria:yucca_palm_leaves" => 0.0,
        // Vanilla 26.3 leaf particle chances relative to 0.01
        "minecraft:cherry_leaves" => 10.0,
        "minecraft:pale_oak_leaves" => 2.0,
        _ => 1.0,
    }
}

/// Full Falling Leaves client configuration.
#[derive(Clone, Debug, PartialEq)]
pub struct FallingLeavesConfig {
    pub enabled: bool,
    pub always_use_vanilla_particles: bool,
    pub use_vanilla_textures: bool,
    pub leaf_size: i32,
    pub leaf_lifespan: i32,
    pub leaf_spawn_rate: i32,
    pub conifer_leaf_spawn_rate: i32,
    pub snowflake_spawn_rate: i32,
    pub drop_from_player_placed_blocks: bool,
    pub leaves_on_block_hit: bool,
    pub minimum_free_space_below: i32,
    pub wind_enabled: bool,
    pub windless_dimensions: Vec<String>,
    pub leaf_spawners: Vec<String>,
    pub parsed_spawners: Vec<LeafSpawnerRule>,
    pub starting_spawn_radius: i32,
    pub decay_spawn_rate_factor: f32,
    pub max_decay_leaves: i32,
    pub leaf_settings: BTreeMap<String, LeafSettingsEntry>,
}

impl Default for FallingLeavesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            always_use_vanilla_particles: false,
            use_vanilla_textures: false,
            leaf_size: 5,
            leaf_lifespan: 200,
            leaf_spawn_rate: 20,
            conifer_leaf_spawn_rate: 0,
            snowflake_spawn_rate: 30,
            drop_from_player_placed_blocks: true,
            leaves_on_block_hit: true,
            minimum_free_space_below: 1,
            wind_enabled: true,
            windless_dimensions: vec![
                "minecraft:the_nether".to_owned(),
                "minecraft:the_end".to_owned(),
            ],
            leaf_spawners: Vec::new(),
            parsed_spawners: Vec::new(),
            starting_spawn_radius: 0,
            decay_spawn_rate_factor: 2.6,
            max_decay_leaves: 9,
            leaf_settings: BTreeMap::new(),
        }
    }
}

impl FallingLeavesConfig {
    pub fn leaf_quad_size(&self) -> f32 {
        self.leaf_size.clamp(1, 10) as f32 / 50.0
    }

    pub fn base_leaf_spawn_chance(&self) -> f32 {
        (self.leaf_spawn_rate.clamp(0, 40) as f32 / 20.0) * DEFAULT_SPAWN_CHANCE
    }

    pub fn base_conifer_leaf_spawn_chance(&self) -> f32 {
        (self.conifer_leaf_spawn_rate.clamp(0, 40) as f32 / 20.0) * DEFAULT_SPAWN_CHANCE
    }

    pub fn snowflake_spawn_chance(&self) -> f32 {
        (self.snowflake_spawn_rate.clamp(0, 40) as f32 / 20.0) * DEFAULT_SPAWN_CHANCE
    }

    pub fn settings_for(&self, block_id: &str) -> LeafSettingsEntry {
        self.leaf_settings
            .get(block_id)
            .cloned()
            .unwrap_or_else(|| LeafSettingsEntry::default_for(block_id))
    }

    pub fn rebuild_spawners(&mut self) {
        self.parsed_spawners = self
            .leaf_spawners
            .iter()
            .filter_map(|s| LeafSpawnerRule::parse(s))
            .collect();
    }

    pub fn clamp_all(&mut self) {
        self.leaf_size = self.leaf_size.clamp(1, 10);
        self.leaf_lifespan = self.leaf_lifespan.clamp(100, 600);
        self.leaf_spawn_rate = self.leaf_spawn_rate.clamp(0, 40);
        self.conifer_leaf_spawn_rate = self.conifer_leaf_spawn_rate.clamp(0, 40);
        self.snowflake_spawn_rate = self.snowflake_spawn_rate.clamp(0, 40);
        self.minimum_free_space_below = self.minimum_free_space_below.clamp(1, 20);
        self.starting_spawn_radius = self.starting_spawn_radius.clamp(0, 23);
        if !self.decay_spawn_rate_factor.is_finite() || self.decay_spawn_rate_factor < 0.0 {
            self.decay_spawn_rate_factor = 2.6;
        }
        self.max_decay_leaves = self.max_decay_leaves.clamp(0, 30);
        for entry in self.leaf_settings.values_mut() {
            if !entry.spawn_rate_factor.is_finite() || entry.spawn_rate_factor < 0.0 {
                entry.spawn_rate_factor = 1.0;
            }
        }
        self.rebuild_spawners();
    }

    /// Serialize to `config/fallingleaves-client.toml`.
    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        out.push_str("# Falling Leaves client configuration.\n");
        out.push_str(
            "# Scalar options are also editable in Options -> Mods -> Falling Leaves.\n\n",
        );
        out.push_str(&format!("enabled = {}\n", self.enabled));
        out.push_str(&format!(
            "always_use_vanilla_particles = {}\n",
            self.always_use_vanilla_particles
        ));
        out.push_str(&format!(
            "use_vanilla_textures = {}\n",
            self.use_vanilla_textures
        ));
        out.push_str(&format!("leaf_size = {}\n", self.leaf_size));
        out.push_str(&format!("leaf_lifespan = {}\n", self.leaf_lifespan));
        out.push_str(&format!("leaf_spawn_rate = {}\n", self.leaf_spawn_rate));
        out.push_str(&format!(
            "conifer_leaf_spawn_rate = {}\n",
            self.conifer_leaf_spawn_rate
        ));
        out.push_str(&format!(
            "snowflake_spawn_rate = {}\n",
            self.snowflake_spawn_rate
        ));
        out.push_str(&format!(
            "drop_from_player_placed_blocks = {}\n",
            self.drop_from_player_placed_blocks
        ));
        out.push_str(&format!(
            "leaves_on_block_hit = {}\n",
            self.leaves_on_block_hit
        ));
        out.push_str(&format!(
            "minimum_free_space_below = {}\n",
            self.minimum_free_space_below
        ));
        out.push_str(&format!("wind_enabled = {}\n", self.wind_enabled));
        out.push_str(&format!(
            "windless_dimensions = {}\n",
            format_string_array(&self.windless_dimensions)
        ));
        out.push_str(&format!(
            "leaf_spawners = {}\n",
            format_string_array(&self.leaf_spawners)
        ));
        out.push_str(&format!(
            "starting_spawn_radius = {}\n",
            self.starting_spawn_radius
        ));
        out.push_str(&format!(
            "decay_spawn_rate_factor = {:.2}\n",
            self.decay_spawn_rate_factor
        ));
        out.push_str(&format!("max_decay_leaves = {}\n", self.max_decay_leaves));

        if !self.leaf_settings.is_empty() {
            out.push('\n');
            for (block_id, entry) in &self.leaf_settings {
                out.push_str(&format!("[leaf_settings.\"{}\"]\n", escape_toml(block_id)));
                out.push_str(&format!(
                    "particle_implementation = \"{}\"\n",
                    entry.particle_implementation.as_str()
                ));
                out.push_str(&format!(
                    "spawn_rate_factor = {:.2}\n",
                    entry.spawn_rate_factor
                ));
                out.push_str(&format!("is_conifer_block = {}\n", entry.is_conifer_block));
                out.push_str(&format!(
                    "spawn_breaking_leaves = {}\n\n",
                    entry.spawn_breaking_leaves
                ));
            }
        }
        out
    }

    /// Parse `config/fallingleaves-client.toml`, keeping defaults for missing or invalid fields.
    pub fn parse_toml(text: &str) -> Self {
        let mut cfg = Self::default();
        let mut current_leaf_block: Option<String> = None;

        for raw_line in text.lines() {
            let line = strip_toml_comment(raw_line).trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                let header = line[1..line.len() - 1].trim();
                if let Some(rest) = header.strip_prefix("leaf_settings.") {
                    let block_id = normalize_id(unquote_toml(rest.trim()));
                    if !block_id.is_empty() {
                        cfg.leaf_settings
                            .entry(block_id.clone())
                            .or_insert_with(|| LeafSettingsEntry::default_for(&block_id));
                        current_leaf_block = Some(block_id);
                    } else {
                        current_leaf_block = None;
                    }
                } else {
                    current_leaf_block = None;
                }
                continue;
            }
            let Some((raw_key, raw_val)) = line.split_once('=') else {
                continue;
            };
            let key = raw_key.trim();
            let val = raw_val.trim();

            if let Some(block_id) = &current_leaf_block {
                if let Some(entry) = cfg.leaf_settings.get_mut(block_id) {
                    match key {
                        "particle_implementation" => {
                            if let Some(imp) = ParticleImplementation::parse(unquote_toml(val)) {
                                entry.particle_implementation = imp;
                            }
                        }
                        "spawn_rate_factor" => {
                            if let Ok(v) = val.parse::<f32>()
                                && v.is_finite()
                                && v >= 0.0
                            {
                                entry.spawn_rate_factor = v;
                            }
                        }
                        "is_conifer_block" => {
                            if let Ok(v) = val.parse::<bool>() {
                                entry.is_conifer_block = v;
                            }
                        }
                        "spawn_breaking_leaves" => {
                            if let Ok(v) = val.parse::<bool>() {
                                entry.spawn_breaking_leaves = v;
                            }
                        }
                        _ => {}
                    }
                }
                continue;
            }

            match key {
                "enabled" => {
                    if let Ok(v) = val.parse::<bool>() {
                        cfg.enabled = v;
                    }
                }
                "always_use_vanilla_particles" => {
                    if let Ok(v) = val.parse::<bool>() {
                        cfg.always_use_vanilla_particles = v;
                    }
                }
                "use_vanilla_textures" => {
                    if let Ok(v) = val.parse::<bool>() {
                        cfg.use_vanilla_textures = v;
                    }
                }
                "leaf_size" => {
                    if let Ok(v) = val.parse::<i32>() {
                        cfg.leaf_size = v;
                    }
                }
                "leaf_lifespan" => {
                    if let Ok(v) = val.parse::<i32>() {
                        cfg.leaf_lifespan = v;
                    }
                }
                "leaf_spawn_rate" => {
                    if let Ok(v) = val.parse::<i32>() {
                        cfg.leaf_spawn_rate = v;
                    }
                }
                "conifer_leaf_spawn_rate" => {
                    if let Ok(v) = val.parse::<i32>() {
                        cfg.conifer_leaf_spawn_rate = v;
                    }
                }
                "snowflake_spawn_rate" => {
                    if let Ok(v) = val.parse::<i32>() {
                        cfg.snowflake_spawn_rate = v;
                    }
                }
                "drop_from_player_placed_blocks" => {
                    if let Ok(v) = val.parse::<bool>() {
                        cfg.drop_from_player_placed_blocks = v;
                    }
                }
                "leaves_on_block_hit" => {
                    if let Ok(v) = val.parse::<bool>() {
                        cfg.leaves_on_block_hit = v;
                    }
                }
                "minimum_free_space_below" => {
                    if let Ok(v) = val.parse::<i32>() {
                        cfg.minimum_free_space_below = v;
                    }
                }
                "wind_enabled" => {
                    if let Ok(v) = val.parse::<bool>() {
                        cfg.wind_enabled = v;
                    }
                }
                "windless_dimensions" => {
                    if let Some(items) = parse_string_array(val) {
                        cfg.windless_dimensions =
                            items.into_iter().map(|s| normalize_id(&s)).collect();
                    }
                }
                "leaf_spawners" => {
                    if let Some(items) = parse_string_array(val) {
                        cfg.leaf_spawners = items;
                    }
                }
                "starting_spawn_radius" => {
                    if let Ok(v) = val.parse::<i32>() {
                        cfg.starting_spawn_radius = v;
                    }
                }
                "decay_spawn_rate_factor" => {
                    if let Ok(v) = val.parse::<f32>()
                        && v.is_finite()
                        && v >= 0.0
                    {
                        cfg.decay_spawn_rate_factor = v;
                    }
                }
                "max_decay_leaves" => {
                    if let Ok(v) = val.parse::<i32>() {
                        cfg.max_decay_leaves = v;
                    }
                }
                _ => {}
            }
        }

        cfg.clamp_all();
        cfg
    }

    pub fn write_to_host_settings(&self, host: &mut dyn ClientHost) {
        let Some(keys) = SETTING_KEYS.get() else {
            return;
        };
        let mut set = |k: &str, v: SettingValue| {
            if let Some(key) = keys.get(k) {
                let _ = host.set_setting(*key, v);
            }
        };
        set(SET_ENABLED, SettingValue::Toggle(self.enabled));
        set(
            SET_ALWAYS_USE_VANILLA_PARTICLES,
            SettingValue::Toggle(self.always_use_vanilla_particles),
        );
        set(
            SET_USE_VANILLA_TEXTURES,
            SettingValue::Toggle(self.use_vanilla_textures),
        );
        set(
            SET_LEAF_SIZE,
            SettingValue::Range(f64::from(self.leaf_size)),
        );
        set(
            SET_LEAF_LIFESPAN,
            SettingValue::Range(f64::from(self.leaf_lifespan)),
        );
        set(
            SET_LEAF_SPAWN_RATE,
            SettingValue::Range(f64::from(self.leaf_spawn_rate)),
        );
        set(
            SET_CONIFER_LEAF_SPAWN_RATE,
            SettingValue::Range(f64::from(self.conifer_leaf_spawn_rate)),
        );
        set(
            SET_SNOWFLAKE_SPAWN_RATE,
            SettingValue::Range(f64::from(self.snowflake_spawn_rate)),
        );
        set(
            SET_DROP_FROM_PLAYER_PLACED,
            SettingValue::Toggle(self.drop_from_player_placed_blocks),
        );
        set(
            SET_LEAVES_ON_BLOCK_HIT,
            SettingValue::Toggle(self.leaves_on_block_hit),
        );
        set(
            SET_MIN_FREE_SPACE_BELOW,
            SettingValue::Range(f64::from(self.minimum_free_space_below)),
        );
        set(SET_WIND_ENABLED, SettingValue::Toggle(self.wind_enabled));
        set(
            SET_STARTING_SPAWN_RADIUS,
            SettingValue::Range(f64::from(self.starting_spawn_radius)),
        );
        set(
            SET_DECAY_SPAWN_RATE_FACTOR,
            SettingValue::Range(f64::from(self.decay_spawn_rate_factor)),
        );
        set(
            SET_MAX_DECAY_LEAVES,
            SettingValue::Range(f64::from(self.max_decay_leaves)),
        );
    }

    /// Applies a changed setting's `value` to its config field and reports whether the field
    /// changed.
    pub fn apply_single_setting(&mut self, key: &str, value: SettingValue) -> bool {
        let toggle = match value {
            SettingValue::Toggle(v) => Some(v),
            _ => None,
        };
        let range = match value {
            SettingValue::Range(v) if v.is_finite() => Some(v),
            _ => None,
        };
        let rounded = |v: f64, min: i32, max: i32| (v.round() as i32).clamp(min, max);
        match key {
            SET_ENABLED => toggle.is_some_and(|v| store_changed(&mut self.enabled, v)),
            SET_ALWAYS_USE_VANILLA_PARTICLES => {
                toggle.is_some_and(|v| store_changed(&mut self.always_use_vanilla_particles, v))
            }
            SET_USE_VANILLA_TEXTURES => {
                toggle.is_some_and(|v| store_changed(&mut self.use_vanilla_textures, v))
            }
            SET_LEAF_SIZE => {
                range.is_some_and(|v| store_changed(&mut self.leaf_size, rounded(v, 1, 10)))
            }
            SET_LEAF_LIFESPAN => {
                range.is_some_and(|v| store_changed(&mut self.leaf_lifespan, rounded(v, 100, 600)))
            }
            SET_LEAF_SPAWN_RATE => {
                range.is_some_and(|v| store_changed(&mut self.leaf_spawn_rate, rounded(v, 0, 40)))
            }
            SET_CONIFER_LEAF_SPAWN_RATE => range.is_some_and(|v| {
                store_changed(&mut self.conifer_leaf_spawn_rate, rounded(v, 0, 40))
            }),
            SET_SNOWFLAKE_SPAWN_RATE => range
                .is_some_and(|v| store_changed(&mut self.snowflake_spawn_rate, rounded(v, 0, 40))),
            SET_DROP_FROM_PLAYER_PLACED => {
                toggle.is_some_and(|v| store_changed(&mut self.drop_from_player_placed_blocks, v))
            }
            SET_LEAVES_ON_BLOCK_HIT => {
                toggle.is_some_and(|v| store_changed(&mut self.leaves_on_block_hit, v))
            }
            SET_MIN_FREE_SPACE_BELOW => range.is_some_and(|v| {
                store_changed(&mut self.minimum_free_space_below, rounded(v, 1, 20))
            }),
            SET_WIND_ENABLED => toggle.is_some_and(|v| store_changed(&mut self.wind_enabled, v)),
            SET_STARTING_SPAWN_RADIUS => range
                .is_some_and(|v| store_changed(&mut self.starting_spawn_radius, rounded(v, 0, 23))),
            SET_DECAY_SPAWN_RATE_FACTOR => range.is_some_and(|v| {
                replace(
                    &mut self.decay_spawn_rate_factor,
                    (v as f32).clamp(0.0, 10.0),
                )
            }),
            SET_MAX_DECAY_LEAVES => {
                range.is_some_and(|v| store_changed(&mut self.max_decay_leaves, rounded(v, 0, 30)))
            }
            _ => false,
        }
    }
}

/// Stores `new` in `slot` and reports whether it differs from the old value.
fn store_changed<T: PartialEq>(slot: &mut T, new: T) -> bool {
    let changed = *slot != new;
    *slot = new;
    changed
}

/// The registered Options > Mods setting keys by their key name.
static SETTING_KEYS: OnceLock<HashMap<String, SettingKey>> = OnceLock::new();

/// Register `fallingleaves-client.toml` with its lifecycle listener.
pub fn register_config(r: &mut dyn Registrar, listener: Arc<ConfigListener>) {
    let default_toml = FallingLeavesConfig::default().to_toml();
    let spec = ConfigSpec::new(ConfigKind::Client, default_toml).file(CONFIG_FILE_NAME);
    if let Err(err) = r.register_config(spec, Some(listener)) {
        eprintln!("fallingleaves: config registration failed: {err}");
    }
}

/// Register all scalar settings on Sunrise's Options -> Mods -> Falling Leaves page,
/// reusing the original mod's translation keys so all 12 bundled locales apply automatically.
pub fn register_settings(r: &mut dyn Registrar) {
    let specs = [
        SettingSpec::toggle(
            SET_ENABLED,
            "text.autoconfig.fallingleaves2.option.enabled",
            true,
        ),
        SettingSpec::toggle(
            SET_ALWAYS_USE_VANILLA_PARTICLES,
            "text.autoconfig.fallingleaves2.option.alwaysUseVanillaParticles",
            false,
        )
        .tooltip("text.autoconfig.fallingleaves2.option.alwaysUseVanillaParticles.@Tooltip"),
        SettingSpec::toggle(
            SET_USE_VANILLA_TEXTURES,
            "text.autoconfig.fallingleaves2.option.useVanillaTextures",
            false,
        )
        .tooltip("text.autoconfig.fallingleaves2.option.useVanillaTextures.@Tooltip"),
        SettingSpec::range(
            SET_LEAF_SIZE,
            "text.autoconfig.fallingleaves2.option.leafSize",
            1.0,
            10.0,
            1.0,
            5.0,
        ),
        SettingSpec::range(
            SET_LEAF_LIFESPAN,
            "text.autoconfig.fallingleaves2.option.leafLifespan",
            100.0,
            600.0,
            10.0,
            200.0,
        )
        .tooltip("text.autoconfig.fallingleaves2.option.leafLifespan.@Tooltip"),
        SettingSpec::range(
            SET_LEAF_SPAWN_RATE,
            "text.autoconfig.fallingleaves2.option.leafSpawnRate",
            0.0,
            40.0,
            1.0,
            20.0,
        )
        .tooltip("text.autoconfig.fallingleaves2.option.leafSpawnRate.@Tooltip"),
        SettingSpec::range(
            SET_CONIFER_LEAF_SPAWN_RATE,
            "text.autoconfig.fallingleaves2.option.coniferLeafSpawnRate",
            0.0,
            40.0,
            1.0,
            0.0,
        )
        .tooltip("text.autoconfig.fallingleaves2.option.coniferLeafSpawnRate.@Tooltip"),
        SettingSpec::range(
            SET_SNOWFLAKE_SPAWN_RATE,
            "text.autoconfig.fallingleaves2.option.snowflakeSpawnRate",
            0.0,
            40.0,
            1.0,
            30.0,
        )
        .tooltip("text.autoconfig.fallingleaves2.option.snowflakeSpawnRate.@Tooltip"),
        SettingSpec::toggle(
            SET_DROP_FROM_PLAYER_PLACED,
            "text.autoconfig.fallingleaves2.option.dropFromPlayerPlacedBlocks",
            true,
        ),
        SettingSpec::toggle(
            SET_LEAVES_ON_BLOCK_HIT,
            "text.autoconfig.fallingleaves2.option.leavesOnBlockHit",
            true,
        ),
        SettingSpec::range(
            SET_MIN_FREE_SPACE_BELOW,
            "text.autoconfig.fallingleaves2.option.minimumFreeSpaceBelow",
            1.0,
            20.0,
            1.0,
            1.0,
        )
        .tooltip("text.autoconfig.fallingleaves2.option.minimumFreeSpaceBelow.@Tooltip"),
        SettingSpec::toggle(
            SET_WIND_ENABLED,
            "text.autoconfig.fallingleaves2.option.windEnabled",
            true,
        )
        .tooltip("text.autoconfig.fallingleaves2.option.windEnabled.@Tooltip"),
        SettingSpec::range(
            SET_STARTING_SPAWN_RADIUS,
            "text.autoconfig.fallingleaves2.option.startingSpawnRadius",
            0.0,
            23.0,
            1.0,
            0.0,
        )
        .tooltip("text.autoconfig.fallingleaves2.option.startingSpawnRadius.@Tooltip"),
        SettingSpec::range(
            SET_DECAY_SPAWN_RATE_FACTOR,
            "text.autoconfig.fallingleaves2.option.decaySpawnRateFactor",
            0.0,
            10.0,
            0.1,
            2.6,
        )
        .tooltip("text.autoconfig.fallingleaves2.option.decaySpawnRateFactor.@Tooltip"),
        SettingSpec::range(
            SET_MAX_DECAY_LEAVES,
            "text.autoconfig.fallingleaves2.option.maxDecayLeaves",
            0.0,
            30.0,
            1.0,
            9.0,
        )
        .tooltip("text.autoconfig.fallingleaves2.option.maxDecayLeaves.@Tooltip[0]"),
    ];

    let mut keys = HashMap::new();
    for spec in specs {
        let name = spec.key.clone();
        if let Ok(key) = r.register_setting(spec) {
            keys.insert(name, key);
        }
    }
    let _ = SETTING_KEYS.set(keys);
}

fn strip_toml_comment(line: &str) -> &str {
    let mut in_quotes = false;
    let mut escaped = false;
    for (idx, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_quotes => escaped = true,
            '"' => in_quotes = !in_quotes,
            '#' if !in_quotes => return &line[..idx],
            _ => {}
        }
    }
    line
}

fn unquote_toml(s: &str) -> &str {
    s.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(s)
}

fn escape_toml(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn format_string_array(items: &[String]) -> String {
    let mut out = String::from("[");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push('"');
        out.push_str(&escape_toml(item));
        out.push('"');
    }
    out.push(']');
    out
}

fn parse_string_array(raw: &str) -> Option<Vec<String>> {
    let inner = raw.strip_prefix('[')?.strip_suffix(']')?.trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escaped = false;
    let mut had_token = false;

    for ch in inner.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if in_quotes {
            match ch {
                '\\' => escaped = true,
                '"' => in_quotes = false,
                _ => current.push(ch),
            }
        } else {
            match ch {
                '"' => {
                    in_quotes = true;
                    had_token = true;
                }
                ',' => {
                    if had_token || !current.trim().is_empty() {
                        result.push(current.trim().to_owned());
                    }
                    current.clear();
                    had_token = false;
                }
                c if c.is_whitespace() => {}
                other => {
                    current.push(other);
                    had_token = true;
                }
            }
        }
    }
    if had_token || !current.trim().is_empty() {
        result.push(current.trim().to_owned());
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_round_trip_preserves_scalar_and_per_block_settings() {
        let mut cfg = FallingLeavesConfig {
            leaf_size: 8,
            leaf_spawn_rate: 30,
            wind_enabled: false,
            leaf_spawners: vec!["minecraft:bamboo[leaves=large]".to_owned()],
            ..FallingLeavesConfig::default()
        };
        cfg.leaf_settings.insert(
            "minecraft:oak_leaves".to_owned(),
            LeafSettingsEntry {
                particle_implementation: ParticleImplementation::Cherry,
                spawn_rate_factor: 2.5,
                is_conifer_block: false,
                spawn_breaking_leaves: false,
            },
        );
        cfg.clamp_all();

        let text = cfg.to_toml();
        let reparsed = FallingLeavesConfig::parse_toml(&text);
        assert_eq!(cfg, reparsed);
    }

    #[test]
    fn leaf_spawner_rule_parses_block_and_properties() {
        let rule = LeafSpawnerRule::parse("minecraft:bamboo[leaves=large,stage=1]").unwrap();
        assert_eq!(rule.block_id, "minecraft:bamboo");
        assert_eq!(
            rule.properties,
            vec![
                ("leaves".to_owned(), "large".to_owned()),
                ("stage".to_owned(), "1".to_owned()),
            ]
        );
        assert!(rule.matches_serialized(
            "minecraft:bamboo",
            "minecraft:bamboo[age=0,leaves=large,stage=1]"
        ));
        assert!(!rule.matches_serialized(
            "minecraft:bamboo",
            "minecraft:bamboo[age=0,leaves=small,stage=1]"
        ));
    }

    #[test]
    fn defaults_match_original_conifers_and_jungle() {
        assert!(is_default_conifer("minecraft:spruce_leaves"));
        assert_eq!(
            default_implementation("minecraft:spruce_leaves"),
            ParticleImplementation::Conifer
        );
        assert_eq!(default_spawn_rate_factor("minecraft:jungle_leaves"), 0.0);
        assert_eq!(default_spawn_rate_factor("minecraft:cherry_leaves"), 10.0);
        assert_eq!(default_spawn_rate_factor("minecraft:pale_oak_leaves"), 2.0);
    }
}
