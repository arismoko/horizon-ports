//! Particle Rain configuration model, 16 built-in weather presets, TOML persistence,
//! and Options > Mods settings.

use crate::wind::WindOptions;
use mod_api::{ClientHost, Config, ConfigKind, Registrar, SettingSpec};

pub const MOD_ID: &str = "particlerain";
pub const CONFIG_FILE_NAME: &str = "particlerain.toml";

// Options > Mods setting keys.
pub const SET_PARTICLE_AMOUNT: &str = "particle_amount";
pub const SET_PARTICLE_STORM_DENSITY: &str = "particle_storm_density";
pub const SET_PARTICLE_DISTANCE: &str = "particle_distance";
pub const SET_SURFACE_RANGE: &str = "surface_range";
pub const SET_RAIN_VOLUME: &str = "rain_volume";
pub const SET_SNOW_VOLUME: &str = "snow_volume";
pub const SET_WIND_VOLUME: &str = "wind_volume";
pub const SET_WIND_STRENGTH: &str = "wind_strength";
pub const SET_GUST_FREQUENCY: &str = "gust_frequency";
pub const SET_MODULATION_SPEED: &str = "modulation_speed";
pub const SET_Y_LEVEL_ADJUSTMENT: &str = "y_level_adjustment";
pub const SET_WATER_TINT: &str = "water_tint";
pub const SET_TINT_MIX: &str = "tint_mix";
pub const SET_CROSS_BIOME_BORDER: &str = "cross_biome_border";
pub const SET_USE_HEIGHTMAP_TEMP: &str = "use_heightmap_temp";
pub const SET_DO_SPAWN_HEIGHT_LIMIT: &str = "do_spawn_height_limit";
pub const SET_SPAWN_HEIGHT_LIMIT: &str = "spawn_height_limit";
pub const SET_RIPPLE_USE_PACK_RES: &str = "ripple_use_pack_res";
pub const SET_RIPPLE_RES: &str = "ripple_res";
pub const SET_ENABLE_RAIN: &str = "enable_rain";
pub const SET_ENABLE_SNOW: &str = "enable_snow";
pub const SET_ENABLE_DUST: &str = "enable_dust";
pub const SET_ENABLE_SPLATTER: &str = "enable_splatter";
pub const SET_ENABLE_RIPPLE: &str = "enable_ripple";
pub const SET_ENABLE_SMOKE: &str = "enable_smoke";
pub const SET_ENABLE_STREAK: &str = "enable_streak";
pub const SET_ENABLE_SHRUB: &str = "enable_shrub";
pub const SET_ENABLE_FOG: &str = "enable_fog";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precipitation {
    None,
    Rain,
    Snow,
}

impl Precipitation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::Rain => "RAIN",
            Self::Snow => "SNOW",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "NONE" => Some(Self::None),
            "RAIN" => Some(Self::Rain),
            "SNOW" => Some(Self::Snow),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherCondition {
    DuringWeather,
    OnlyDuringNormalWeather,
    OnlyDuringStormyWeather,
    AfterWeather,
    Clear,
    Always,
}

impl WeatherCondition {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DuringWeather => "DURING_WEATHER",
            Self::OnlyDuringNormalWeather => "ONLY_DURING_NORMAL_WEATHER",
            Self::OnlyDuringStormyWeather => "ONLY_DURING_STORMY_WEATHER",
            Self::AfterWeather => "AFTER_WEATHER",
            Self::Clear => "CLEAR",
            Self::Always => "ALWAYS",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "DURING_WEATHER" => Some(Self::DuringWeather),
            "ONLY_DURING_NORMAL_WEATHER" => Some(Self::OnlyDuringNormalWeather),
            "ONLY_DURING_STORMY_WEATHER" => Some(Self::OnlyDuringStormyWeather),
            "AFTER_WEATHER" => Some(Self::AfterWeather),
            "CLEAR" => Some(Self::Clear),
            "ALWAYS" => Some(Self::Always),
            _ => None,
        }
    }

    pub fn is_current(self, rain_level: f32, thunder_level: f32, after_weather_ticks: i32) -> bool {
        match self {
            Self::DuringWeather => rain_level > 0.0,
            Self::OnlyDuringNormalWeather => rain_level > 0.0 && thunder_level <= 0.0,
            Self::OnlyDuringStormyWeather => thunder_level > 0.0 && rain_level > 0.9,
            Self::AfterWeather => after_weather_ticks > 0,
            Self::Clear => rain_level <= 0.0,
            Self::Always => true,
        }
    }

    pub fn weather_level(self, rain_level: f32, thunder_level: f32) -> f32 {
        match self {
            Self::DuringWeather | Self::OnlyDuringNormalWeather => rain_level,
            Self::OnlyDuringStormyWeather => thunder_level,
            Self::AfterWeather | Self::Clear | Self::Always => 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnPos {
    Sky,
    BlockSides,
    BlockBottom,
    BlockTop,
    WorldSurface,
}

impl SpawnPos {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sky => "SKY",
            Self::BlockSides => "BLOCK_SIDES",
            Self::BlockBottom => "BLOCK_BOTTOM",
            Self::BlockTop => "BLOCK_TOP",
            Self::WorldSurface => "WORLD_SURFACE",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "SKY" => Some(Self::Sky),
            "BLOCK_SIDES" => Some(Self::BlockSides),
            "BLOCK_BOTTOM" => Some(Self::BlockBottom),
            "BLOCK_TOP" => Some(Self::BlockTop),
            "WORLD_SURFACE" => Some(Self::WorldSurface),
            _ => None,
        }
    }

    pub fn render_distance(self, perf: &PerformanceOptions) -> i32 {
        match self {
            Self::WorldSurface => perf.surface_range,
            _ => perf.particle_distance,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleRenderType {
    Opaque,
    Translucent,
    Blended,
}

impl ParticleRenderType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Opaque => "OPAQUE",
            Self::Translucent => "TRANSLUCENT",
            Self::Blended => "BLENDED",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "OPAQUE" => Some(Self::Opaque),
            "TRANSLUCENT" => Some(Self::Translucent),
            "BLENDED" => Some(Self::Blended),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TintType {
    Water,
    Fog,
    Map,
    Custom,
    None,
}

impl TintType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Water => "WATER",
            Self::Fog => "FOG",
            Self::Map => "MAP",
            Self::Custom => "CUSTOM",
            Self::None => "NONE",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "WATER" => Some(Self::Water),
            "FOG" => Some(Self::Fog),
            "MAP" => Some(Self::Map),
            "CUSTOM" => Some(Self::Custom),
            "NONE" => Some(Self::None),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RotationType {
    CopyCamera,
    RelativeVelocity,
    WorldVelocity,
    Vertical,
    Horizontal,
}

impl RotationType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CopyCamera => "COPY_CAMERA",
            Self::RelativeVelocity => "RELATIVE_VELOCITY",
            Self::WorldVelocity => "WORLD_VELOCITY",
            Self::Vertical => "VERTICAL",
            Self::Horizontal => "HORIZONTAL",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "COPY_CAMERA" => Some(Self::CopyCamera),
            "RELATIVE_VELOCITY" => Some(Self::RelativeVelocity),
            "WORLD_VELOCITY" => Some(Self::WorldVelocity),
            "VERTICAL" => Some(Self::Vertical),
            "HORIZONTAL" => Some(Self::Horizontal),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleStyle {
    Custom,
    Registered,
    BlockModel,
}

impl ParticleStyle {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Custom => "CUSTOM",
            Self::Registered => "REGISTERED",
            Self::BlockModel => "BLOCK_MODEL",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "CUSTOM" => Some(Self::Custom),
            "REGISTERED" => Some(Self::Registered),
            "BLOCK_MODEL" => Some(Self::BlockModel),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FadeType {
    InAndOut,
    Out,
    Distance,
}

impl FadeType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InAndOut => "IN_AND_OUT",
            Self::Out => "OUT",
            Self::Distance => "DISTANCE",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "IN_AND_OUT" => Some(Self::InAndOut),
            "OUT" => Some(Self::Out),
            "DISTANCE" => Some(Self::Distance),
            _ => None,
        }
    }
}

/// Tag-and-ID whitelist or blacklist matching `Whitelist.java`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryFilter {
    pub is_whitelist: bool,
    pub entries: Vec<String>,
}

impl Default for EntryFilter {
    fn default() -> Self {
        Self {
            is_whitelist: false,
            entries: Vec::new(),
        }
    }
}

impl EntryFilter {
    pub fn whitelist(entries: &[&str]) -> Self {
        Self {
            is_whitelist: true,
            entries: entries.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    pub fn blacklist(entries: &[&str]) -> Self {
        Self {
            is_whitelist: false,
            entries: entries.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    /// Evaluate filter given an exact ID matcher and a `#tag` matcher.
    pub fn matches<FId, FTag>(&self, mut id_match: FId, mut tag_match: FTag) -> bool
    where
        FId: FnMut(&str) -> bool,
        FTag: FnMut(&str) -> bool,
    {
        if self.entries.is_empty() {
            return !self.is_whitelist;
        }
        let found = self.entries.iter().any(|entry| {
            if let Some(tag) = entry.strip_prefix('#') {
                tag_match(tag)
            } else {
                id_match(entry)
            }
        });
        if self.is_whitelist { found } else { !found }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PerformanceOptions {
    pub particle_amount: i32,
    pub particle_storm_density: f32,
    pub particle_distance: i32,
    pub surface_range: i32,
}

impl Default for PerformanceOptions {
    fn default() -> Self {
        Self {
            particle_amount: 1500,
            particle_storm_density: 1.5,
            particle_distance: 16,
            surface_range: 100,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SoundOptions {
    pub rain_volume: f32,
    pub snow_volume: f32,
    pub wind_volume: f32,
}

impl Default for SoundOptions {
    fn default() -> Self {
        Self {
            rain_volume: 0.2,
            snow_volume: 0.1,
            wind_volume: 0.1,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompatOptions {
    pub render_default_rain: bool,
    pub render_default_snow: bool,
    pub do_default_splashing: bool,
    pub water_tint: bool,
    pub tint_mix: f32,
    pub dont_tint_with_shaders: bool,
    pub cross_biome_border: bool,
    pub use_heightmap_temp: bool,
    pub weather_ignore_blocks: EntryFilter,
    pub do_spawn_height_limit: bool,
    pub spawn_height_limit: i32,
    pub ripple_res: u32,
    pub ripple_use_pack_res: bool,
}

impl Default for CompatOptions {
    fn default() -> Self {
        Self {
            render_default_rain: false,
            render_default_snow: false,
            do_default_splashing: false,
            water_tint: true,
            tint_mix: 0.6,
            dont_tint_with_shaders: true,
            cross_biome_border: false,
            use_heightmap_temp: true,
            weather_ignore_blocks: EntryFilter::whitelist(&[
                "#minecraft:fences",
                "#minecraft:all_signs",
                "minecraft:barrier",
            ]),
            do_spawn_height_limit: false,
            spawn_height_limit: 0,
            ripple_res: 16,
            ripple_use_pack_res: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StreakOptions {
    pub opacity: f32,
    pub size: f32,
}

impl Default for StreakOptions {
    fn default() -> Self {
        Self {
            opacity: 0.9,
            size: 0.5,
        }
    }
}

/// Complete definition of one weather particle effect (`ParticleData.java`).
#[derive(Clone, Debug, PartialEq)]
pub struct ParticlePreset {
    pub id: String,
    pub enabled: bool,
    pub style: ParticleStyle,
    pub registered_particle_id: String,
    pub block_id: String,
    pub density: f32,
    pub weather: WeatherCondition,
    pub precipitation: Vec<Precipitation>,
    pub biome_list: EntryFilter,
    pub block_list: EntryFilter,
    pub needs_sky_access: bool,
    pub spawn_pos: SpawnPos,
    pub wind_strength: f32,
    pub storm_wind_strength: f32,
    pub gravity: f32,
    pub rotation_amount: f32,
    pub bounciness: f32,
    pub lifetime: i32,
    pub opacity: f32,
    pub fade_type: FadeType,
    pub size: f32,
    pub distance_scaling: f32,
    pub render_type: ParticleRenderType,
    pub animate_sprites: bool,
    pub sprite_locations: Vec<String>,
    pub tint_type: TintType,
    pub custom_tint: [u8; 3],
    pub rotation_type: RotationType,
}

impl ParticlePreset {
    fn base(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            enabled: true,
            style: ParticleStyle::Custom,
            registered_particle_id: "minecraft:cloud".into(),
            block_id: "minecraft:dead_bush".into(),
            density: 1.0,
            weather: WeatherCondition::DuringWeather,
            precipitation: vec![Precipitation::Rain],
            biome_list: EntryFilter::default(),
            block_list: EntryFilter::default(),
            needs_sky_access: true,
            spawn_pos: SpawnPos::Sky,
            wind_strength: 0.1,
            storm_wind_strength: 0.5,
            gravity: 0.1,
            rotation_amount: 0.0,
            bounciness: 0.0,
            lifetime: 3000,
            opacity: 1.0,
            fade_type: FadeType::Distance,
            size: 0.5,
            distance_scaling: 0.0,
            render_type: ParticleRenderType::Translucent,
            animate_sprites: false,
            sprite_locations: vec!["particlerain:new_custom_particle".into()],
            tint_type: TintType::None,
            custom_tint: [0, 0, 0],
            rotation_type: RotationType::CopyCamera,
        }
    }
}

/// All 16 built-in presets from Particle Rain's `assets/particlerain/particles.json`.
pub fn default_presets() -> Vec<ParticlePreset> {
    let numbered = |prefix: &str, count: usize| -> Vec<String> {
        (0..count).map(|i| format!("{prefix}{i}")).collect()
    };
    let sand_blocks = || {
        EntryFilter::whitelist(&[
            "#minecraft:camel_sand_step_sound_blocks",
            "#minecraft:sand",
            "#minecraft:terracotta",
            "#c:sandstone_blocks",
            "minecraft:coarse_dirt",
        ])
    };
    let overworld_biomes = || EntryFilter::whitelist(&["#c:in_overworld"]);
    let fog_blocks =
        || EntryFilter::whitelist(&["#minecraft:dirt", "#minecraft:substrate_overworld"]);
    let after_rain_biomes =
        || EntryFilter::blacklist(&["minecraft:plains", "minecraft:meadow", "#c:is_windswept"]);
    let wet_biomes = || EntryFilter::whitelist(&["#c:is_wet", "#c:is_spooky", "#c:climate_wet"]);

    vec![
        ParticlePreset {
            gravity: 0.9,
            wind_strength: 0.4,
            storm_wind_strength: 0.8,
            size: 0.75,
            sprite_locations: numbered("particlerain:rain_", 16),
            tint_type: TintType::Water,
            rotation_type: RotationType::RelativeVelocity,
            ..ParticlePreset::base("rain")
        },
        ParticlePreset {
            gravity: 1.0,
            weather: WeatherCondition::OnlyDuringStormyWeather,
            wind_strength: 0.4,
            storm_wind_strength: 0.8,
            size: 0.75,
            sprite_locations: numbered("particlerain:heavy_rain_", 4),
            tint_type: TintType::Water,
            rotation_type: RotationType::RelativeVelocity,
            ..ParticlePreset::base("heavy_rain")
        },
        ParticlePreset {
            density: 0.5,
            precipitation: vec![Precipitation::Snow],
            gravity: 0.05,
            wind_strength: 0.01,
            storm_wind_strength: 0.1,
            rotation_amount: 0.4,
            size: 0.75,
            sprite_locations: numbered("particlerain:snow_", 16),
            ..ParticlePreset::base("snow")
        },
        ParticlePreset {
            density: 0.4,
            precipitation: vec![Precipitation::Snow],
            weather: WeatherCondition::OnlyDuringStormyWeather,
            gravity: 0.1,
            wind_strength: 0.01,
            storm_wind_strength: 0.1,
            rotation_amount: 0.4,
            size: 0.75,
            sprite_locations: numbered("particlerain:heavy_snow_", 4),
            ..ParticlePreset::base("heavy_snow")
        },
        ParticlePreset {
            precipitation: vec![Precipitation::None],
            block_list: sand_blocks(),
            biome_list: overworld_biomes(),
            gravity: 0.1,
            wind_strength: 0.7,
            storm_wind_strength: 1.0,
            rotation_amount: 0.1,
            size: 0.25,
            sprite_locations: numbered("minecraft:generic_", 3),
            tint_type: TintType::Map,
            ..ParticlePreset::base("dust")
        },
        ParticlePreset {
            density: 0.2,
            precipitation: vec![Precipitation::None],
            block_list: sand_blocks(),
            biome_list: overworld_biomes(),
            spawn_pos: SpawnPos::BlockTop,
            gravity: 0.5,
            wind_strength: 0.8,
            storm_wind_strength: 1.0,
            bounciness: 0.15,
            rotation_amount: 1.0,
            lifetime: 60,
            size: 0.2,
            render_type: ParticleRenderType::Opaque,
            sprite_locations: numbered("minecraft:generic_", 3),
            tint_type: TintType::Map,
            ..ParticlePreset::base("skittering_dust")
        },
        ParticlePreset {
            style: ParticleStyle::Registered,
            registered_particle_id: "minecraft:rain".into(),
            block_list: EntryFilter::blacklist(&["minecraft:lava"]),
            spawn_pos: SpawnPos::BlockTop,
            wind_strength: 0.1,
            storm_wind_strength: 0.2,
            ..ParticlePreset::base("splatter")
        },
        ParticlePreset {
            style: ParticleStyle::Custom,
            gravity: 0.0,
            wind_strength: 0.0,
            storm_wind_strength: 0.0,
            block_list: EntryFilter::whitelist(&["minecraft:water", "puddleflood:puddle"]),
            spawn_pos: SpawnPos::BlockTop,
            animate_sprites: true,
            sprite_locations: numbered("particlerain:ripple_", 8),
            rotation_type: RotationType::Horizontal,
            render_type: ParticleRenderType::Translucent,
            size: 0.25,
            fade_type: FadeType::Out,
            lifetime: 7,
            tint_type: TintType::Water,
            ..ParticlePreset::base("ripple")
        },
        ParticlePreset {
            style: ParticleStyle::Registered,
            registered_particle_id: "minecraft:smoke".into(),
            block_list: EntryFilter::whitelist(&[
                "#minecraft:strider_warm_blocks",
                "#minecraft:infiniburn_overworld",
            ]),
            needs_sky_access: true,
            spawn_pos: SpawnPos::BlockTop,
            ..ParticlePreset::base("smoke")
        },
        ParticlePreset {
            style: ParticleStyle::Registered,
            registered_particle_id: "particlerain:streak".into(),
            density: 0.2,
            block_list: EntryFilter::whitelist(&[
                "#minecraft:impermeable",
                "#c:glass_panes",
                "#minecraft:mineable/pickaxe",
            ]),
            spawn_pos: SpawnPos::BlockSides,
            ..ParticlePreset::base("streak")
        },
        ParticlePreset {
            style: ParticleStyle::BlockModel,
            block_id: "minecraft:dead_bush".into(),
            density: 0.01,
            bounciness: 0.15,
            gravity: 0.5,
            wind_strength: 0.4,
            storm_wind_strength: 0.5,
            rotation_amount: 1.0,
            size: 1.0,
            precipitation: vec![Precipitation::None],
            block_list: EntryFilter::whitelist(&[
                "#minecraft:camel_sand_step_sound_blocks",
                "#minecraft:sand",
                "#minecraft:terracotta",
                "minecraft:coarse_dirt",
            ]),
            biome_list: overworld_biomes(),
            spawn_pos: SpawnPos::WorldSurface,
            ..ParticlePreset::base("shrub")
        },
        ParticlePreset {
            density: 0.5,
            gravity: 0.0,
            wind_strength: 0.0,
            storm_wind_strength: 0.0,
            weather: WeatherCondition::AfterWeather,
            biome_list: after_rain_biomes(),
            block_list: fog_blocks(),
            spawn_pos: SpawnPos::WorldSurface,
            sprite_locations: vec!["particlerain:fog_translucent".into()],
            rotation_type: RotationType::Horizontal,
            render_type: ParticleRenderType::Blended,
            size: 2.0,
            distance_scaling: 0.1,
            fade_type: FadeType::InAndOut,
            lifetime: 250,
            tint_type: TintType::Fog,
            ..ParticlePreset::base("fog_after_rain")
        },
        ParticlePreset {
            density: 0.5,
            gravity: 0.0,
            wind_strength: 0.0,
            storm_wind_strength: 0.0,
            weather: WeatherCondition::AfterWeather,
            biome_list: after_rain_biomes(),
            block_list: fog_blocks(),
            spawn_pos: SpawnPos::WorldSurface,
            sprite_locations: vec!["particlerain:horizontal_fog".into()],
            rotation_type: RotationType::Vertical,
            render_type: ParticleRenderType::Blended,
            size: 2.0,
            distance_scaling: 0.1,
            fade_type: FadeType::InAndOut,
            lifetime: 250,
            tint_type: TintType::Fog,
            ..ParticlePreset::base("vertical_fog_after_rain")
        },
        ParticlePreset {
            density: 0.5,
            gravity: 0.0,
            wind_strength: 0.0,
            storm_wind_strength: 0.0,
            weather: WeatherCondition::Always,
            biome_list: wet_biomes(),
            block_list: fog_blocks(),
            spawn_pos: SpawnPos::WorldSurface,
            sprite_locations: vec!["particlerain:fog_translucent".into()],
            rotation_type: RotationType::Horizontal,
            render_type: ParticleRenderType::Blended,
            size: 2.0,
            distance_scaling: 0.1,
            fade_type: FadeType::InAndOut,
            lifetime: 250,
            tint_type: TintType::Fog,
            ..ParticlePreset::base("fog")
        },
        ParticlePreset {
            density: 0.5,
            gravity: 0.0,
            wind_strength: 0.0,
            storm_wind_strength: 0.0,
            weather: WeatherCondition::Always,
            biome_list: wet_biomes(),
            block_list: fog_blocks(),
            spawn_pos: SpawnPos::WorldSurface,
            sprite_locations: vec!["particlerain:horizontal_fog".into()],
            rotation_type: RotationType::Vertical,
            render_type: ParticleRenderType::Blended,
            size: 2.0,
            distance_scaling: 0.1,
            fade_type: FadeType::InAndOut,
            lifetime: 250,
            tint_type: TintType::Fog,
            ..ParticlePreset::base("vertical_fog")
        },
        ParticlePreset {
            lifetime: 100,
            gravity: 0.1,
            wind_strength: 0.5,
            storm_wind_strength: 0.8,
            rotation_amount: 0.1,
            opacity: 0.4,
            size: 0.4,
            distance_scaling: 1.0,
            sprite_locations: vec!["particlerain:fog_dithered".into()],
            rotation_type: RotationType::CopyCamera,
            precipitation: vec![Precipitation::None],
            block_list: sand_blocks(),
            biome_list: overworld_biomes(),
            spawn_pos: SpawnPos::WorldSurface,
            bounciness: 0.15,
            tint_type: TintType::Map,
            ..ParticlePreset::base("dust_haze")
        },
    ]
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParticleRainConfig {
    pub perf: PerformanceOptions,
    pub sound: SoundOptions,
    pub wind: WindOptions,
    pub compat: CompatOptions,
    pub streak: StreakOptions,
    pub particles: Vec<ParticlePreset>,
}

impl Default for ParticleRainConfig {
    fn default() -> Self {
        Self {
            perf: PerformanceOptions::default(),
            sound: SoundOptions::default(),
            wind: WindOptions::default(),
            compat: CompatOptions::default(),
            streak: StreakOptions::default(),
            particles: default_presets(),
        }
    }
}

impl ParticleRainConfig {
    pub fn preset_enabled(&self, id: &str) -> bool {
        self.particles
            .iter()
            .find(|p| p.id == id)
            .is_some_and(|p| p.enabled)
    }

    pub fn set_preset_enabled(&mut self, ids: &[&str], enabled: bool) {
        for p in &mut self.particles {
            if ids.iter().any(|id| *id == p.id) {
                p.enabled = enabled;
            }
        }
    }

    pub fn to_toml(&self) -> String {
        let mut out = String::from("# Particle Rain configuration (hot-reloaded on save)\n\n");
        out.push_str("[perf]\n");
        out.push_str(&format!(
            "particle_amount = {}\n",
            self.perf.particle_amount
        ));
        out.push_str(&format!(
            "particle_storm_density = {:.3}\n",
            self.perf.particle_storm_density
        ));
        out.push_str(&format!(
            "particle_distance = {}\n",
            self.perf.particle_distance
        ));
        out.push_str(&format!("surface_range = {}\n\n", self.perf.surface_range));

        out.push_str("[sound]\n");
        out.push_str(&format!("rain_volume = {:.3}\n", self.sound.rain_volume));
        out.push_str(&format!("snow_volume = {:.3}\n", self.sound.snow_volume));
        out.push_str(&format!("wind_volume = {:.3}\n\n", self.sound.wind_volume));

        out.push_str("[wind]\n");
        out.push_str(&format!("strength = {:.4}\n", self.wind.strength));
        out.push_str(&format!(
            "gust_frequency = {:.4}\n",
            self.wind.gust_frequency
        ));
        out.push_str(&format!(
            "modulation_speed = {:.4}\n",
            self.wind.modulation_speed
        ));
        out.push_str(&format!(
            "y_level_adjustment = {}\n\n",
            self.wind.y_level_adjustment
        ));

        out.push_str("[compat]\n");
        out.push_str(&format!(
            "render_default_rain = {}\n",
            self.compat.render_default_rain
        ));
        out.push_str(&format!(
            "render_default_snow = {}\n",
            self.compat.render_default_snow
        ));
        out.push_str(&format!(
            "do_default_splashing = {}\n",
            self.compat.do_default_splashing
        ));
        out.push_str(&format!("water_tint = {}\n", self.compat.water_tint));
        out.push_str(&format!("tint_mix = {:.3}\n", self.compat.tint_mix));
        out.push_str(&format!(
            "dont_tint_with_shaders = {}\n",
            self.compat.dont_tint_with_shaders
        ));
        out.push_str(&format!(
            "cross_biome_border = {}\n",
            self.compat.cross_biome_border
        ));
        out.push_str(&format!(
            "use_heightmap_temp = {}\n",
            self.compat.use_heightmap_temp
        ));
        out.push_str(&format!(
            "weather_ignore_blocks_whitelist = {}\n",
            self.compat.weather_ignore_blocks.is_whitelist
        ));
        out.push_str(&format!(
            "weather_ignore_blocks = {}\n",
            fmt_str_list(&self.compat.weather_ignore_blocks.entries)
        ));
        out.push_str(&format!(
            "do_spawn_height_limit = {}\n",
            self.compat.do_spawn_height_limit
        ));
        out.push_str(&format!(
            "spawn_height_limit = {}\n",
            self.compat.spawn_height_limit
        ));
        out.push_str(&format!("ripple_res = {}\n", self.compat.ripple_res));
        out.push_str(&format!(
            "ripple_use_pack_res = {}\n\n",
            self.compat.ripple_use_pack_res
        ));

        out.push_str("[streak]\n");
        out.push_str(&format!("opacity = {:.3}\n", self.streak.opacity));
        out.push_str(&format!("size = {:.3}\n\n", self.streak.size));

        for p in &self.particles {
            out.push_str(&format!("[particles.{}]\n", p.id));
            out.push_str(&format!("enabled = {}\n", p.enabled));
            out.push_str(&format!("style = \"{}\"\n", p.style.as_str()));
            if p.style == ParticleStyle::Registered {
                out.push_str(&format!(
                    "registered_particle_id = \"{}\"\n",
                    escape_toml(&p.registered_particle_id)
                ));
            }
            if p.style == ParticleStyle::BlockModel {
                out.push_str(&format!("block_id = \"{}\"\n", escape_toml(&p.block_id)));
            }
            out.push_str(&format!("density = {:.4}\n", p.density));
            out.push_str(&format!("weather = \"{}\"\n", p.weather.as_str()));
            let precs: Vec<String> = p
                .precipitation
                .iter()
                .map(|pr| pr.as_str().to_owned())
                .collect();
            out.push_str(&format!("precipitation = {}\n", fmt_str_list(&precs)));
            out.push_str(&format!("spawn_pos = \"{}\"\n", p.spawn_pos.as_str()));
            out.push_str(&format!("needs_sky_access = {}\n", p.needs_sky_access));
            out.push_str(&format!("wind_strength = {:.3}\n", p.wind_strength));
            out.push_str(&format!(
                "storm_wind_strength = {:.3}\n",
                p.storm_wind_strength
            ));
            out.push_str(&format!("gravity = {:.3}\n", p.gravity));
            out.push_str(&format!("rotation_amount = {:.3}\n", p.rotation_amount));
            out.push_str(&format!("bounciness = {:.3}\n", p.bounciness));
            out.push_str(&format!("lifetime = {}\n", p.lifetime));
            out.push_str(&format!("opacity = {:.3}\n", p.opacity));
            out.push_str(&format!("fade_type = \"{}\"\n", p.fade_type.as_str()));
            out.push_str(&format!("size = {:.3}\n", p.size));
            out.push_str(&format!("distance_scaling = {:.3}\n", p.distance_scaling));
            out.push_str(&format!("render_type = \"{}\"\n", p.render_type.as_str()));
            out.push_str(&format!("animate_sprites = {}\n", p.animate_sprites));
            out.push_str(&format!("tint_type = \"{}\"\n", p.tint_type.as_str()));
            out.push_str(&format!(
                "rotation_type = \"{}\"\n",
                p.rotation_type.as_str()
            ));
            if !p.biome_list.entries.is_empty() || p.biome_list.is_whitelist {
                out.push_str(&format!(
                    "biome_whitelist = {}\n",
                    p.biome_list.is_whitelist
                ));
                out.push_str(&format!(
                    "biome_entries = {}\n",
                    fmt_str_list(&p.biome_list.entries)
                ));
            }
            if !p.block_list.entries.is_empty() || p.block_list.is_whitelist {
                out.push_str(&format!(
                    "block_whitelist = {}\n",
                    p.block_list.is_whitelist
                ));
                out.push_str(&format!(
                    "block_entries = {}\n",
                    fmt_str_list(&p.block_list.entries)
                ));
            }
            out.push('\n');
        }
        out
    }

    pub fn parse_toml(text: &str) -> Self {
        let mut cfg = Self::default();
        let mut section = String::new();

        for raw_line in text.lines() {
            let line = strip_comment(raw_line).trim();
            if line.is_empty() {
                continue;
            }
            if let Some(sec) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                section = sec.trim().to_owned();
                if let Some(id) = section.strip_prefix("particles.")
                    && !id.is_empty()
                    && !cfg.particles.iter().any(|p| p.id == id)
                {
                    cfg.particles.push(ParticlePreset::base(id));
                }
                continue;
            }
            let Some((key, val)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let val = val.trim();

            match section.as_str() {
                "perf" => match key {
                    "particle_amount" => {
                        if let Ok(v) = val.parse::<i32>() {
                            cfg.perf.particle_amount = v.clamp(0, 10000);
                        }
                    }
                    "particle_storm_density" => {
                        if let Ok(v) = val.parse::<f32>()
                            && v.is_finite()
                        {
                            cfg.perf.particle_storm_density = v.clamp(0.1, 10.0);
                        }
                    }
                    "particle_distance" => {
                        if let Ok(v) = val.parse::<i32>() {
                            cfg.perf.particle_distance = v.clamp(4, 64);
                        }
                    }
                    "surface_range" => {
                        if let Ok(v) = val.parse::<i32>() {
                            cfg.perf.surface_range = v.clamp(8, 256);
                        }
                    }
                    _ => {}
                },
                "sound" => match key {
                    "rain_volume" => {
                        if let Ok(v) = val.parse::<f32>()
                            && v.is_finite()
                        {
                            cfg.sound.rain_volume = v.clamp(0.0, 2.0);
                        }
                    }
                    "snow_volume" => {
                        if let Ok(v) = val.parse::<f32>()
                            && v.is_finite()
                        {
                            cfg.sound.snow_volume = v.clamp(0.0, 2.0);
                        }
                    }
                    "wind_volume" => {
                        if let Ok(v) = val.parse::<f32>()
                            && v.is_finite()
                        {
                            cfg.sound.wind_volume = v.clamp(0.0, 2.0);
                        }
                    }
                    _ => {}
                },
                "wind" => match key {
                    "strength" => {
                        if let Ok(v) = val.parse::<f32>()
                            && v.is_finite()
                        {
                            cfg.wind.strength = v.clamp(0.0, 1.0);
                        }
                    }
                    "gust_frequency" => {
                        if let Ok(v) = val.parse::<f32>()
                            && v.is_finite()
                        {
                            cfg.wind.gust_frequency = v.clamp(0.0, 5.0);
                        }
                    }
                    "modulation_speed" => {
                        if let Ok(v) = val.parse::<f32>()
                            && v.is_finite()
                        {
                            cfg.wind.modulation_speed = v.clamp(0.0, 5.0);
                        }
                    }
                    "y_level_adjustment" => {
                        if let Ok(v) = val.parse::<bool>() {
                            cfg.wind.y_level_adjustment = v;
                        }
                    }
                    _ => {}
                },
                "compat" => match key {
                    "render_default_rain" => {
                        if let Ok(v) = val.parse::<bool>() {
                            cfg.compat.render_default_rain = v;
                        }
                    }
                    "render_default_snow" => {
                        if let Ok(v) = val.parse::<bool>() {
                            cfg.compat.render_default_snow = v;
                        }
                    }
                    "do_default_splashing" => {
                        if let Ok(v) = val.parse::<bool>() {
                            cfg.compat.do_default_splashing = v;
                        }
                    }
                    "water_tint" => {
                        if let Ok(v) = val.parse::<bool>() {
                            cfg.compat.water_tint = v;
                        }
                    }
                    "tint_mix" => {
                        if let Ok(v) = val.parse::<f32>()
                            && v.is_finite()
                        {
                            cfg.compat.tint_mix = v.clamp(0.0, 1.0);
                        }
                    }
                    "dont_tint_with_shaders" => {
                        if let Ok(v) = val.parse::<bool>() {
                            cfg.compat.dont_tint_with_shaders = v;
                        }
                    }
                    "cross_biome_border" => {
                        if let Ok(v) = val.parse::<bool>() {
                            cfg.compat.cross_biome_border = v;
                        }
                    }
                    "use_heightmap_temp" => {
                        if let Ok(v) = val.parse::<bool>() {
                            cfg.compat.use_heightmap_temp = v;
                        }
                    }
                    "weather_ignore_blocks_whitelist" => {
                        if let Ok(v) = val.parse::<bool>() {
                            cfg.compat.weather_ignore_blocks.is_whitelist = v;
                        }
                    }
                    "weather_ignore_blocks" => {
                        if let Some(list) = parse_str_list(val) {
                            cfg.compat.weather_ignore_blocks.entries = list;
                        }
                    }
                    "do_spawn_height_limit" => {
                        if let Ok(v) = val.parse::<bool>() {
                            cfg.compat.do_spawn_height_limit = v;
                        }
                    }
                    "spawn_height_limit" => {
                        if let Ok(v) = val.parse::<i32>() {
                            cfg.compat.spawn_height_limit = v.clamp(0, 512);
                        }
                    }
                    "ripple_res" => {
                        if let Ok(v) = val.parse::<u32>() {
                            cfg.compat.ripple_res = v.clamp(4, 256);
                        }
                    }
                    "ripple_use_pack_res" => {
                        if let Ok(v) = val.parse::<bool>() {
                            cfg.compat.ripple_use_pack_res = v;
                        }
                    }
                    _ => {}
                },
                "streak" => match key {
                    "opacity" => {
                        if let Ok(v) = val.parse::<f32>()
                            && v.is_finite()
                        {
                            cfg.streak.opacity = v.clamp(0.0, 1.0);
                        }
                    }
                    "size" => {
                        if let Ok(v) = val.parse::<f32>()
                            && v.is_finite()
                        {
                            cfg.streak.size = v.clamp(0.05, 4.0);
                        }
                    }
                    _ => {}
                },
                other => {
                    if let Some(id) = other.strip_prefix("particles.")
                        && let Some(p) = cfg.particles.iter_mut().find(|p| p.id == id)
                    {
                        apply_preset_key(p, key, val);
                    }
                }
            }
        }
        cfg
    }

    pub fn write_to_host_settings(&self, host: &dyn ClientHost) {
        let set_bool =
            |k: &str, v: bool| host.set_setting(MOD_ID, k, if v { "true" } else { "false" });
        let set_f64 = |k: &str, v: f64| host.set_setting(MOD_ID, k, &format!("{v}"));

        set_f64(SET_PARTICLE_AMOUNT, self.perf.particle_amount as f64);
        set_f64(
            SET_PARTICLE_STORM_DENSITY,
            self.perf.particle_storm_density as f64,
        );
        set_f64(SET_PARTICLE_DISTANCE, self.perf.particle_distance as f64);
        set_f64(SET_SURFACE_RANGE, self.perf.surface_range as f64);
        set_f64(SET_RAIN_VOLUME, self.sound.rain_volume as f64);
        set_f64(SET_SNOW_VOLUME, self.sound.snow_volume as f64);
        set_f64(SET_WIND_VOLUME, self.sound.wind_volume as f64);
        set_f64(SET_WIND_STRENGTH, self.wind.strength as f64);
        set_f64(SET_GUST_FREQUENCY, self.wind.gust_frequency as f64);
        set_f64(SET_MODULATION_SPEED, self.wind.modulation_speed as f64);
        set_bool(SET_Y_LEVEL_ADJUSTMENT, self.wind.y_level_adjustment);
        set_bool(SET_WATER_TINT, self.compat.water_tint);
        set_f64(SET_TINT_MIX, self.compat.tint_mix as f64);
        set_bool(SET_CROSS_BIOME_BORDER, self.compat.cross_biome_border);
        set_bool(SET_USE_HEIGHTMAP_TEMP, self.compat.use_heightmap_temp);
        set_bool(SET_DO_SPAWN_HEIGHT_LIMIT, self.compat.do_spawn_height_limit);
        set_f64(
            SET_SPAWN_HEIGHT_LIMIT,
            self.compat.spawn_height_limit as f64,
        );
        set_bool(SET_RIPPLE_USE_PACK_RES, self.compat.ripple_use_pack_res);
        set_f64(SET_RIPPLE_RES, self.compat.ripple_res as f64);
        set_bool(SET_ENABLE_RAIN, self.preset_enabled("rain"));
        set_bool(SET_ENABLE_SNOW, self.preset_enabled("snow"));
        set_bool(SET_ENABLE_DUST, self.preset_enabled("dust"));
        set_bool(SET_ENABLE_SPLATTER, self.preset_enabled("splatter"));
        set_bool(SET_ENABLE_RIPPLE, self.preset_enabled("ripple"));
        set_bool(SET_ENABLE_SMOKE, self.preset_enabled("smoke"));
        set_bool(SET_ENABLE_STREAK, self.preset_enabled("streak"));
        set_bool(SET_ENABLE_SHRUB, self.preset_enabled("shrub"));
        set_bool(SET_ENABLE_FOG, self.preset_enabled("fog"));
    }

    /// Apply one Options > Mods setting change. Returns `true` if live particles should be cleared.
    pub fn apply_setting_change(&mut self, host: &dyn ClientHost, key: &str) -> bool {
        let mut clear = false;
        match key {
            SET_PARTICLE_AMOUNT => {
                if let Some(v) = host.setting_f64(key) {
                    self.perf.particle_amount = (v.round() as i32).clamp(0, 10000);
                    clear = true;
                }
            }
            SET_PARTICLE_STORM_DENSITY => {
                if let Some(v) = host.setting_f64(key) {
                    self.perf.particle_storm_density = (v as f32).clamp(0.1, 10.0);
                }
            }
            SET_PARTICLE_DISTANCE => {
                if let Some(v) = host.setting_f64(key) {
                    self.perf.particle_distance = (v.round() as i32).clamp(4, 64);
                }
            }
            SET_SURFACE_RANGE => {
                if let Some(v) = host.setting_f64(key) {
                    self.perf.surface_range = (v.round() as i32).clamp(8, 256);
                }
            }
            SET_RAIN_VOLUME => {
                if let Some(v) = host.setting_f64(key) {
                    self.sound.rain_volume = (v as f32).clamp(0.0, 2.0);
                }
            }
            SET_SNOW_VOLUME => {
                if let Some(v) = host.setting_f64(key) {
                    self.sound.snow_volume = (v as f32).clamp(0.0, 2.0);
                }
            }
            SET_WIND_VOLUME => {
                if let Some(v) = host.setting_f64(key) {
                    self.sound.wind_volume = (v as f32).clamp(0.0, 2.0);
                }
            }
            SET_WIND_STRENGTH => {
                if let Some(v) = host.setting_f64(key) {
                    self.wind.strength = (v as f32).clamp(0.0, 1.0);
                }
            }
            SET_GUST_FREQUENCY => {
                if let Some(v) = host.setting_f64(key) {
                    self.wind.gust_frequency = (v as f32).clamp(0.0, 5.0);
                }
            }
            SET_MODULATION_SPEED => {
                if let Some(v) = host.setting_f64(key) {
                    self.wind.modulation_speed = (v as f32).clamp(0.0, 5.0);
                }
            }
            SET_Y_LEVEL_ADJUSTMENT => {
                if let Some(v) = host.setting_bool(key) {
                    self.wind.y_level_adjustment = v;
                }
            }
            SET_WATER_TINT => {
                if let Some(v) = host.setting_bool(key) {
                    self.compat.water_tint = v;
                }
            }
            SET_TINT_MIX => {
                if let Some(v) = host.setting_f64(key) {
                    self.compat.tint_mix = (v as f32).clamp(0.0, 1.0);
                }
            }
            SET_CROSS_BIOME_BORDER => {
                if let Some(v) = host.setting_bool(key) {
                    self.compat.cross_biome_border = v;
                }
            }
            SET_USE_HEIGHTMAP_TEMP => {
                if let Some(v) = host.setting_bool(key) {
                    self.compat.use_heightmap_temp = v;
                }
            }
            SET_DO_SPAWN_HEIGHT_LIMIT => {
                if let Some(v) = host.setting_bool(key) {
                    self.compat.do_spawn_height_limit = v;
                }
            }
            SET_SPAWN_HEIGHT_LIMIT => {
                if let Some(v) = host.setting_f64(key) {
                    self.compat.spawn_height_limit = (v.round() as i32).clamp(0, 512);
                }
            }
            SET_RIPPLE_USE_PACK_RES => {
                if let Some(v) = host.setting_bool(key) {
                    self.compat.ripple_use_pack_res = v;
                }
            }
            SET_RIPPLE_RES => {
                if let Some(v) = host.setting_f64(key) {
                    self.compat.ripple_res = (v.round() as u32).clamp(4, 256);
                }
            }
            SET_ENABLE_RAIN => {
                if let Some(v) = host.setting_bool(key) {
                    self.set_preset_enabled(&["rain", "heavy_rain"], v);
                }
            }
            SET_ENABLE_SNOW => {
                if let Some(v) = host.setting_bool(key) {
                    self.set_preset_enabled(&["snow", "heavy_snow"], v);
                }
            }
            SET_ENABLE_DUST => {
                if let Some(v) = host.setting_bool(key) {
                    self.set_preset_enabled(&["dust", "skittering_dust", "dust_haze"], v);
                }
            }
            SET_ENABLE_SPLATTER => {
                if let Some(v) = host.setting_bool(key) {
                    self.set_preset_enabled(&["splatter"], v);
                }
            }
            SET_ENABLE_RIPPLE => {
                if let Some(v) = host.setting_bool(key) {
                    self.set_preset_enabled(&["ripple"], v);
                }
            }
            SET_ENABLE_SMOKE => {
                if let Some(v) = host.setting_bool(key) {
                    self.set_preset_enabled(&["smoke"], v);
                }
            }
            SET_ENABLE_STREAK => {
                if let Some(v) = host.setting_bool(key) {
                    self.set_preset_enabled(&["streak"], v);
                }
            }
            SET_ENABLE_SHRUB => {
                if let Some(v) = host.setting_bool(key) {
                    self.set_preset_enabled(&["shrub"], v);
                }
            }
            SET_ENABLE_FOG => {
                if let Some(v) = host.setting_bool(key) {
                    self.set_preset_enabled(
                        &[
                            "fog",
                            "vertical_fog",
                            "fog_after_rain",
                            "vertical_fog_after_rain",
                        ],
                        v,
                    );
                }
            }
            _ => {}
        }
        clear
    }
}

fn apply_preset_key(p: &mut ParticlePreset, key: &str, val: &str) {
    match key {
        "enabled" => {
            if let Ok(v) = val.parse::<bool>() {
                p.enabled = v;
            }
        }
        "style" => {
            if let Some(v) = parse_str(val).and_then(|s| ParticleStyle::parse(&s)) {
                p.style = v;
            }
        }
        "registered_particle_id" => {
            if let Some(s) = parse_str(val) {
                p.registered_particle_id = s;
            }
        }
        "block_id" => {
            if let Some(s) = parse_str(val) {
                p.block_id = s;
            }
        }
        "density" => {
            if let Ok(v) = val.parse::<f32>()
                && v.is_finite()
            {
                p.density = v.clamp(0.0, 1.0);
            }
        }
        "weather" => {
            if let Some(v) = parse_str(val).and_then(|s| WeatherCondition::parse(&s)) {
                p.weather = v;
            }
        }
        "precipitation" => {
            if let Some(list) = parse_str_list(val) {
                let parsed: Vec<Precipitation> = list
                    .iter()
                    .filter_map(|s| Precipitation::parse(s))
                    .collect();
                if !parsed.is_empty() {
                    p.precipitation = parsed;
                }
            }
        }
        "spawn_pos" => {
            if let Some(v) = parse_str(val).and_then(|s| SpawnPos::parse(&s)) {
                p.spawn_pos = v;
            }
        }
        "needs_sky_access" => {
            if let Ok(v) = val.parse::<bool>() {
                p.needs_sky_access = v;
            }
        }
        "wind_strength" => {
            if let Ok(v) = val.parse::<f32>()
                && v.is_finite()
            {
                p.wind_strength = v.clamp(0.0, 5.0);
            }
        }
        "storm_wind_strength" => {
            if let Ok(v) = val.parse::<f32>()
                && v.is_finite()
            {
                p.storm_wind_strength = v.clamp(0.0, 5.0);
            }
        }
        "gravity" => {
            if let Ok(v) = val.parse::<f32>()
                && v.is_finite()
            {
                p.gravity = v.clamp(-2.0, 5.0);
            }
        }
        "rotation_amount" => {
            if let Ok(v) = val.parse::<f32>()
                && v.is_finite()
            {
                p.rotation_amount = v.clamp(0.0, 10.0);
            }
        }
        "bounciness" => {
            if let Ok(v) = val.parse::<f32>()
                && v.is_finite()
            {
                p.bounciness = v.clamp(0.0, 2.0);
            }
        }
        "lifetime" => {
            if let Ok(v) = val.parse::<i32>() {
                p.lifetime = v.clamp(1, 20000);
            }
        }
        "opacity" => {
            if let Ok(v) = val.parse::<f32>()
                && v.is_finite()
            {
                p.opacity = v.clamp(0.0, 1.0);
            }
        }
        "fade_type" => {
            if let Some(v) = parse_str(val).and_then(|s| FadeType::parse(&s)) {
                p.fade_type = v;
            }
        }
        "size" => {
            if let Ok(v) = val.parse::<f32>()
                && v.is_finite()
            {
                p.size = v.clamp(0.01, 16.0);
            }
        }
        "distance_scaling" => {
            if let Ok(v) = val.parse::<f32>()
                && v.is_finite()
            {
                p.distance_scaling = v.clamp(0.0, 2.0);
            }
        }
        "render_type" => {
            if let Some(v) = parse_str(val).and_then(|s| ParticleRenderType::parse(&s)) {
                p.render_type = v;
            }
        }
        "animate_sprites" => {
            if let Ok(v) = val.parse::<bool>() {
                p.animate_sprites = v;
            }
        }
        "sprite_locations" => {
            if let Some(list) = parse_str_list(val)
                && !list.is_empty()
            {
                p.sprite_locations = list;
            }
        }
        "tint_type" => {
            if let Some(v) = parse_str(val).and_then(|s| TintType::parse(&s)) {
                p.tint_type = v;
            }
        }
        "rotation_type" => {
            if let Some(v) = parse_str(val).and_then(|s| RotationType::parse(&s)) {
                p.rotation_type = v;
            }
        }
        "biome_whitelist" => {
            if let Ok(v) = val.parse::<bool>() {
                p.biome_list.is_whitelist = v;
            }
        }
        "biome_entries" => {
            if let Some(list) = parse_str_list(val) {
                p.biome_list.entries = list;
            }
        }
        "block_whitelist" => {
            if let Ok(v) = val.parse::<bool>() {
                p.block_list.is_whitelist = v;
            }
        }
        "block_entries" => {
            if let Some(list) = parse_str_list(val) {
                p.block_list.entries = list;
            }
        }
        _ => {}
    }
}

pub fn register_mod_configs(configs: &mut Vec<Config>) {
    let default_toml = ParticleRainConfig::default().to_toml();
    configs.push(Config::new(MOD_ID, ConfigKind::Client, &default_toml).file(CONFIG_FILE_NAME));
}

/// Register in-game settings on Sunrise's Options > Mods > Particle Rain page,
/// reusing the original mod's translation keys (translated in all 8 languages).
pub fn register_settings(r: &mut dyn Registrar) {
    let specs = [
        SettingSpec::range(
            SET_PARTICLE_AMOUNT,
            "particlerain.particleAmount",
            0.0,
            5000.0,
            50.0,
            1500.0,
        )
        .tooltip("particlerain.particleAmount.description"),
        SettingSpec::range(
            SET_PARTICLE_STORM_DENSITY,
            "particlerain.particleStormDensity",
            0.5,
            5.0,
            0.1,
            1.5,
        )
        .tooltip("particlerain.particleStormDensity.description"),
        SettingSpec::range(
            SET_PARTICLE_DISTANCE,
            "particlerain.particleDistance",
            4.0,
            48.0,
            1.0,
            16.0,
        )
        .tooltip("particlerain.particleDistance.description"),
        SettingSpec::range(
            SET_SURFACE_RANGE,
            "particlerain.surfaceRange",
            16.0,
            200.0,
            4.0,
            100.0,
        )
        .tooltip("particlerain.surfaceRange.description"),
        SettingSpec::range(
            SET_RAIN_VOLUME,
            "particlerain.rainVolume",
            0.0,
            1.0,
            0.05,
            0.2,
        ),
        SettingSpec::range(
            SET_SNOW_VOLUME,
            "particlerain.snowVolume",
            0.0,
            1.0,
            0.05,
            0.1,
        ),
        SettingSpec::range(
            SET_WIND_VOLUME,
            "particlerain.windVolume",
            0.0,
            1.0,
            0.05,
            0.1,
        ),
        SettingSpec::range(
            SET_WIND_STRENGTH,
            "particlerain.strength",
            0.0,
            0.2,
            0.005,
            0.03,
        ),
        SettingSpec::range(
            SET_GUST_FREQUENCY,
            "particlerain.gustFrequency",
            0.01,
            1.0,
            0.01,
            0.15,
        )
        .tooltip("particlerain.gustFrequency.description"),
        SettingSpec::range(
            SET_MODULATION_SPEED,
            "particlerain.modulationSpeed",
            0.01,
            1.0,
            0.01,
            0.1,
        )
        .tooltip("particlerain.modulationSpeed.description"),
        SettingSpec::toggle(
            SET_Y_LEVEL_ADJUSTMENT,
            "particlerain.yLevelAdjustment",
            true,
        )
        .tooltip("particlerain.yLevelAdjustment.description"),
        SettingSpec::toggle(SET_WATER_TINT, "particlerain.waterTint", true)
            .tooltip("particlerain.waterTint.description"),
        SettingSpec::range(SET_TINT_MIX, "particlerain.tintMix", 0.0, 1.0, 0.05, 0.6)
            .tooltip("particlerain.tintMix.description"),
        SettingSpec::toggle(
            SET_CROSS_BIOME_BORDER,
            "particlerain.crossBiomeBorder",
            false,
        )
        .tooltip("particlerain.crossBiomeBorder.description"),
        SettingSpec::toggle(
            SET_USE_HEIGHTMAP_TEMP,
            "particlerain.useHeightmapTemp",
            true,
        )
        .tooltip("particlerain.useHeightmapTemp.description"),
        SettingSpec::toggle(
            SET_DO_SPAWN_HEIGHT_LIMIT,
            "particlerain.doSpawnHeightLimit",
            false,
        )
        .tooltip("particlerain.doSpawnHeightLimit.description"),
        SettingSpec::range(
            SET_SPAWN_HEIGHT_LIMIT,
            "particlerain.spawnHeightLimit",
            0.0,
            320.0,
            4.0,
            0.0,
        )
        .tooltip("particlerain.spawnHeightLimit.description"),
        SettingSpec::toggle(
            SET_RIPPLE_USE_PACK_RES,
            "particlerain.rippleUsePackRes",
            true,
        )
        .tooltip("particlerain.rippleUsePackRes.description"),
        SettingSpec::range(
            SET_RIPPLE_RES,
            "particlerain.rippleRes",
            4.0,
            128.0,
            4.0,
            16.0,
        ),
        SettingSpec::toggle(SET_ENABLE_RAIN, "particlerain.rain", true),
        SettingSpec::toggle(SET_ENABLE_SNOW, "particlerain.snow", true),
        SettingSpec::toggle(SET_ENABLE_DUST, "particlerain.dust", true),
        SettingSpec::toggle(SET_ENABLE_SPLATTER, "particlerain.splatter", true),
        SettingSpec::toggle(SET_ENABLE_RIPPLE, "particlerain.ripple", true),
        SettingSpec::toggle(SET_ENABLE_SMOKE, "particlerain.smoke", true),
        SettingSpec::toggle(SET_ENABLE_STREAK, "particlerain.streak", true),
        SettingSpec::toggle(SET_ENABLE_SHRUB, "particlerain.shrub", true),
        SettingSpec::toggle(SET_ENABLE_FOG, "particlerain.fog", true),
    ];

    for spec in specs {
        let _ = r.register_setting(spec);
    }
}

fn strip_comment(line: &str) -> &str {
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

fn escape_toml(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn fmt_str_list(items: &[String]) -> String {
    let inner: Vec<String> = items
        .iter()
        .map(|s| format!("\"{}\"", escape_toml(s)))
        .collect();
    format!("[{}]", inner.join(", "))
}

fn parse_str(val: &str) -> Option<String> {
    let inner = val.strip_prefix('"')?.strip_suffix('"')?;
    Some(inner.replace("\\\"", "\"").replace("\\\\", "\\"))
}

fn parse_str_list(val: &str) -> Option<Vec<String>> {
    let inner = val.strip_prefix('[')?.strip_suffix(']')?.trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    for part in inner.split(',') {
        let trimmed = part.trim();
        if let Some(s) = parse_str(trimmed) {
            out.push(s);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_all_16_presets() {
        let cfg = ParticleRainConfig::default();
        assert_eq!(cfg.particles.len(), 16);
        assert!(cfg.preset_enabled("rain"));
        assert!(cfg.preset_enabled("dust_haze"));
        assert!(cfg.preset_enabled("shrub"));
    }

    #[test]
    fn toml_round_trip_preserves_settings() {
        let mut cfg = ParticleRainConfig::default();
        cfg.perf.particle_amount = 2400;
        cfg.sound.rain_volume = 0.45;
        cfg.wind.y_level_adjustment = false;
        cfg.compat.do_spawn_height_limit = true;
        cfg.compat.spawn_height_limit = 210;
        cfg.set_preset_enabled(&["shrub"], false);

        let serialized = cfg.to_toml();
        let parsed = ParticleRainConfig::parse_toml(&serialized);
        assert_eq!(parsed.perf.particle_amount, 2400);
        assert!((parsed.sound.rain_volume - 0.45).abs() < 1e-3);
        assert!(!parsed.wind.y_level_adjustment);
        assert!(parsed.compat.do_spawn_height_limit);
        assert_eq!(parsed.compat.spawn_height_limit, 210);
        assert!(!parsed.preset_enabled("shrub"));
        assert!(parsed.preset_enabled("rain"));
    }

    #[test]
    fn entry_filter_handles_whitelist_and_blacklist() {
        let wl = EntryFilter::whitelist(&["#minecraft:sand", "minecraft:coarse_dirt"]);
        assert!(wl.matches(|id| id == "minecraft:coarse_dirt", |_| false));
        assert!(wl.matches(|_| false, |tag| tag == "minecraft:sand"));
        assert!(!wl.matches(|id| id == "minecraft:stone", |_| false));

        let bl = EntryFilter::blacklist(&["minecraft:lava"]);
        assert!(!bl.matches(|id| id == "minecraft:lava", |_| false));
        assert!(bl.matches(|id| id == "minecraft:water", |_| false));

        let empty_bl = EntryFilter::default();
        assert!(empty_bl.matches(|_| false, |_| false));
    }
}
