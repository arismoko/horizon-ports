//! Pure Comforts gameplay rules, configuration parsing, sleep time math,
//! hammock placement geometry, client sync encoding, and conditional recipe matching.

use std::collections::BTreeMap;

use mod_api::{Compound, Direction, JavaString, Tag};

pub const MOD_ID: &str = "comforts";

/// Vanilla `MapColor.WOOL` id (used by sleeping bags, hammocks, and rope & nail in Comforts).
pub const MAP_COLOR_WOOL: u8 = 3;

/// Ticks in one vanilla Overworld day-night cycle.
pub const DAY_TICKS: i64 = 24_000;

/// Minimum ticks slept before sleeping bag break chance and debuff effects trigger.
pub const SLEEP_EFFECT_THRESHOLD_TICKS: i64 = 500;

/// Vanilla dusk/nap wake marker default tick (`comforts:wake_up_from_nap`).
pub const DEFAULT_NAP_WAKE_TICK: i64 = 11_999;

/// Vanilla dawn wake marker default tick (`minecraft:wake_up_from_sleep`).
pub const DEFAULT_SLEEP_WAKE_TICK: i64 = 0;

/// Sixteen vanilla dye colour names in Comforts registry order.
pub const COLORS: [&str; 16] = [
    "white",
    "orange",
    "magenta",
    "light_blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "light_gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BedKind {
    SleepingBag,
    Hammock,
}

impl BedKind {
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::SleepingBag => 1,
            Self::Hammock => 2,
        }
    }

    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::SleepingBag),
            2 => Some(Self::Hammock),
            _ => None,
        }
    }

    /// Sleeping surface height above the block bottom, in blocks:
    /// sleeping bag is 3/16 (`0.1875`), hammock is 1/16 (`0.0625`).
    pub const fn sleep_surface_y(self) -> f64 {
        match self {
            Self::SleepingBag => 3.0 / 16.0,
            Self::Hammock => 1.0 / 16.0,
        }
    }

    pub const fn occupied_key(self) -> &'static str {
        match self {
            Self::SleepingBag => "item.comforts.sleeping_bag.occupied",
            Self::Hammock => "item.comforts.hammock.occupied",
        }
    }

    pub const fn too_far_key(self) -> &'static str {
        match self {
            Self::SleepingBag => "item.comforts.sleeping_bag.too_far_away",
            Self::Hammock => "item.comforts.hammock.too_far_away",
        }
    }
}

/// Configured time-of-day usage rule (`ComfortsConstants.TimeUse`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeUse {
    None,
    Day,
    Night,
    DayOrNight,
    UseWithoutSleeping,
}

impl TimeUse {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "NONE" => Some(Self::None),
            "DAY" => Some(Self::Day),
            "NIGHT" => Some(Self::Night),
            "DAY_OR_NIGHT" => Some(Self::DayOrNight),
            "USE_WITHOUT_SLEEPING" => Some(Self::UseWithoutSleeping),
            _ => None,
        }
    }

    pub const fn as_u8(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Day => 1,
            Self::Night => 2,
            Self::DayOrNight => 3,
            Self::UseWithoutSleeping => 4,
        }
    }

    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Day),
            2 => Some(Self::Night),
            3 => Some(Self::DayOrNight),
            4 => Some(Self::UseWithoutSleeping),
            _ => None,
        }
    }

    /// Translation key shown when sleeping is disallowed by this `TimeUse`.
    pub const fn deny_message_key(self) -> &'static str {
        match self {
            Self::None => "item.comforts.no_sleep",
            Self::Day => "item.comforts.hammock.no_sleep",
            Self::Night => "block.minecraft.bed.no_sleep",
            Self::DayOrNight => "item.comforts.hammock.no_sleep.2",
            Self::UseWithoutSleeping => "item.comforts.no_sleep",
        }
    }

    pub const fn allows_day_skip(self) -> bool {
        matches!(self, Self::Day | Self::DayOrNight)
    }
}

/// A parsed status effect from `sleepingBagEffects` (`effect;duration_secs;power`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectSpec {
    pub id: String,
    pub duration_ticks: i32,
    pub amplifier: i32,
}

impl EffectSpec {
    /// Parse `effect;duration(secs);power`, clamping duration to `1..=1600` seconds
    /// and power to `1..=4` (amplifier `0..=3`), matching upstream `ComfortsEvents.initializeEffects`.
    pub fn parse(entry: &str) -> Option<Self> {
        let mut parts = entry.split(';');
        let id = parts.next()?.trim();
        let secs_raw = parts.next()?.trim();
        let power_raw = parts.next()?.trim();
        if id.is_empty() || parts.next().is_some() {
            return None;
        }
        let secs: i32 = secs_raw.parse().ok()?;
        let power: i32 = power_raw.parse().ok()?;
        let duration_ticks = secs.clamp(1, 1600) * 20;
        let amplifier = power.clamp(1, 4) - 1;
        let id = if id.contains(':') {
            id.to_owned()
        } else {
            format!("minecraft:{id}")
        };
        Some(Self {
            id,
            duration_ticks,
            amplifier,
        })
    }
}

/// Default `comforts-common.toml`.
pub const DEFAULT_COMMON: &str = "\
# Comforts common configuration

# If enabled, the default hammock recipes will be available.
enableHammockRecipes = true

# If enabled, the default sleeping bag recipes will be available.
enableSleepingBagRecipes = true
";

/// Common configuration (`ComfortsConfig.COMMON`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommonConfig {
    pub enable_hammock_recipes: bool,
    pub enable_sleeping_bag_recipes: bool,
}

impl Default for CommonConfig {
    fn default() -> Self {
        Self {
            enable_hammock_recipes: true,
            enable_sleeping_bag_recipes: true,
        }
    }
}

impl CommonConfig {
    pub fn parse(text: &str) -> Self {
        let mut cfg = Self::default();
        for (key, value) in parse_toml_kv(text) {
            match key {
                "enableHammockRecipes" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.enable_hammock_recipes = v;
                    }
                }
                "enableSleepingBagRecipes" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.enable_sleeping_bag_recipes = v;
                    }
                }
                _ => {}
            }
        }
        cfg
    }
}

/// Default `comforts-server.toml`.
pub const DEFAULT_SERVER: &str = "\
# Comforts server configuration

# If enabled, players automatically attempt to use sleeping bags when placed.
autoUse = true

# If enabled, players cannot sleep again for a period of time after sleeping.
restrictSleeping = false

# If restrictSleeping is true, this value determines the length of wait time
# (larger numbers sleep sooner). Range: 1.0 .. 20.0
restMultiplier = 2.0

# The time of day that hammocks can be used.
# Allowed values: NONE, DAY, NIGHT, DAY_OR_NIGHT, USE_WITHOUT_SLEEPING
hammockUse = \"DAY\"

# The time of day that sleeping bags can be used.
# Allowed values: NONE, DAY, NIGHT, DAY_OR_NIGHT, USE_WITHOUT_SLEEPING
sleepingBagUse = \"NIGHT\"

# What percentage of players must sleep to skip the day.
# 0 skips with 1 player, 100 requires all players, < 0 uses players_sleeping_percentage.
daySleepingPercentage = -1

# The amount of time, in ticks, to add or remove after sleeping through a night (-2000 .. 2000).
dayWakeTimeOffset = 0

# The amount of time, in ticks, to add or remove after sleeping through a day (-2000 .. 2000).
nightWakeTimeOffset = 0

# If enabled, attempting to sleep in hammocks stops phantoms from spawning.
hammocksStopPhantoms = true

# If enabled, attempting to sleep in sleeping bags stops phantoms from spawning.
sleepingBagsStopPhantoms = true

# The percentage chance (0 .. 100) that a sleeping bag will break upon use.
sleepingBagBreakChance = 0

# The value multiplied by a player's luck and subtracted from sleepingBagBreakChance (-1.0 .. 1.0).
sleepingBagBreakChanceLuckMultiplier = 0.0

# Status effects to apply to players after using a sleeping bag.
# Format: \"effect;duration(secs);power\"
sleepingBagEffects = []
";

/// Server configuration (`ComfortsConfig.SERVER`).
#[derive(Clone, Debug, PartialEq)]
pub struct ServerConfig {
    pub auto_use: bool,
    pub restrict_sleeping: bool,
    pub rest_multiplier: f64,
    pub hammock_use: TimeUse,
    pub sleeping_bag_use: TimeUse,
    pub day_sleeping_percentage: i32,
    pub day_wake_time_offset: i32,
    pub night_wake_time_offset: i32,
    pub hammocks_stop_phantoms: bool,
    pub sleeping_bags_stop_phantoms: bool,
    pub sleeping_bag_break_chance: i32,
    pub sleeping_bag_break_chance_luck_multiplier: f64,
    pub sleeping_bag_effects: Vec<EffectSpec>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            auto_use: true,
            restrict_sleeping: false,
            rest_multiplier: 2.0,
            hammock_use: TimeUse::Day,
            sleeping_bag_use: TimeUse::Night,
            day_sleeping_percentage: -1,
            day_wake_time_offset: 0,
            night_wake_time_offset: 0,
            hammocks_stop_phantoms: true,
            sleeping_bags_stop_phantoms: true,
            sleeping_bag_break_chance: 0,
            sleeping_bag_break_chance_luck_multiplier: 0.0,
            sleeping_bag_effects: Vec::new(),
        }
    }
}

impl ServerConfig {
    pub fn parse(text: &str) -> Self {
        let mut cfg = Self::default();
        for (key, value) in parse_toml_kv(text) {
            match key {
                "autoUse" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.auto_use = v;
                    }
                }
                "restrictSleeping" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.restrict_sleeping = v;
                    }
                }
                "restMultiplier" => {
                    if let Some(v) = parse_f64(value) {
                        cfg.rest_multiplier = v.clamp(1.0, 20.0);
                    }
                }
                "hammockUse" => {
                    if let Some(v) = TimeUse::parse(unquote(value)) {
                        cfg.hammock_use = v;
                    }
                }
                "sleepingBagUse" => {
                    if let Some(v) = TimeUse::parse(unquote(value)) {
                        cfg.sleeping_bag_use = v;
                    }
                }
                "daySleepingPercentage" => {
                    if let Some(v) = parse_i32(value) {
                        cfg.day_sleeping_percentage = v.clamp(-1, 100);
                    }
                }
                "dayWakeTimeOffset" => {
                    if let Some(v) = parse_i32(value) {
                        cfg.day_wake_time_offset = v.clamp(-2000, 2000);
                    }
                }
                "nightWakeTimeOffset" => {
                    if let Some(v) = parse_i32(value) {
                        cfg.night_wake_time_offset = v.clamp(-2000, 2000);
                    }
                }
                "hammocksStopPhantoms" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.hammocks_stop_phantoms = v;
                    }
                }
                "sleepingBagsStopPhantoms" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.sleeping_bags_stop_phantoms = v;
                    }
                }
                "sleepingBagBreakChance" => {
                    if let Some(v) = parse_i32(value) {
                        cfg.sleeping_bag_break_chance = v.clamp(0, 100);
                    }
                }
                "sleepingBagBreakChanceLuckMultiplier" => {
                    if let Some(v) = parse_f64(value) {
                        cfg.sleeping_bag_break_chance_luck_multiplier = v.clamp(-1.0, 1.0);
                    }
                }
                "sleepingBagEffects" => {
                    cfg.sleeping_bag_effects = parse_string_list(value)
                        .into_iter()
                        .filter_map(|entry| EffectSpec::parse(&entry))
                        .collect();
                }
                _ => {}
            }
        }
        cfg
    }

    pub const fn time_use_for(&self, kind: BedKind) -> TimeUse {
        match kind {
            BedKind::SleepingBag => self.sleeping_bag_use,
            BedKind::Hammock => self.hammock_use,
        }
    }

    pub const fn stops_phantoms(&self, kind: BedKind) -> bool {
        match kind {
            BedKind::SleepingBag => self.sleeping_bags_stop_phantoms,
            BedKind::Hammock => self.hammocks_stop_phantoms,
        }
    }

    pub const fn client_sync(&self) -> ClientSyncConfig {
        ClientSyncConfig {
            auto_use: self.auto_use,
            sleeping_bag_use: self.sleeping_bag_use,
            hammock_use: self.hammock_use,
        }
    }
}

/// Subset of server configuration needed by clients (auto-use tooltips & F5 perspective toggle).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClientSyncConfig {
    pub auto_use: bool,
    pub sleeping_bag_use: TimeUse,
    pub hammock_use: TimeUse,
}

impl Default for ClientSyncConfig {
    fn default() -> Self {
        ServerConfig::default().client_sync()
    }
}

impl ClientSyncConfig {
    pub const fn encode(self) -> [u8; 3] {
        [
            if self.auto_use { 1 } else { 0 },
            self.sleeping_bag_use.as_u8(),
            self.hammock_use.as_u8(),
        ]
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 3 {
            return None;
        }
        let auto_use = match bytes[0] {
            0 => false,
            1 => true,
            _ => return None,
        };
        let sleeping_bag_use = TimeUse::from_u8(bytes[1])?;
        let hammock_use = TimeUse::from_u8(bytes[2])?;
        Some(Self {
            auto_use,
            sleeping_bag_use,
            hammock_use,
        })
    }

    pub const fn time_use_for(self, kind: BedKind) -> TimeUse {
        match kind {
            BedKind::SleepingBag => self.sleeping_bag_use,
            BedKind::Hammock => self.hammock_use,
        }
    }
}

/// Per-player persisted sleep state (`comforts:sleep_data` attachment, copied on death).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SleepData {
    pub sleep_time: i64,
    pub wake_time: i64,
    pub tired_time: i64,
    pub auto_sleep_pos: Option<[i32; 3]>,
    pub auto_sleep_custom_name: Option<Tag>,
    pub saved_time_since_rest: Option<i32>,
    pub sleeping_kind: Option<BedKind>,
}

impl SleepData {
    pub fn to_compound(&self) -> Compound {
        let mut map = Compound::new();
        map.insert("sleepTime".into(), Tag::Long(self.sleep_time));
        map.insert("wakeTime".into(), Tag::Long(self.wake_time));
        map.insert("tiredTime".into(), Tag::Long(self.tired_time));
        if let Some([x, y, z]) = self.auto_sleep_pos {
            map.insert("autoSleepPos".into(), Tag::IntArray(vec![x, y, z]));
        }
        if let Some(name) = &self.auto_sleep_custom_name {
            map.insert("autoSleepCustomName".into(), name.clone());
        }
        if let Some(rest) = self.saved_time_since_rest {
            map.insert("savedTimeSinceRest".into(), Tag::Int(rest));
        }
        if let Some(kind) = self.sleeping_kind {
            map.insert("sleepingKind".into(), Tag::Byte(kind.as_u8() as i8));
        }
        map
    }

    pub fn from_compound(map: &Compound) -> Self {
        let get_i64 = |key: &str| match map.get(&JavaString::from(key)) {
            Some(Tag::Long(v)) => *v,
            Some(Tag::Int(v)) => i64::from(*v),
            _ => 0,
        };
        let auto_sleep_pos = match map.get(&JavaString::from("autoSleepPos")) {
            Some(Tag::IntArray(coords)) if coords.len() == 3 => {
                Some([coords[0], coords[1], coords[2]])
            }
            _ => None,
        };
        let auto_sleep_custom_name = map.get(&JavaString::from("autoSleepCustomName")).cloned();
        let saved_time_since_rest = match map.get(&JavaString::from("savedTimeSinceRest")) {
            Some(Tag::Int(v)) => Some(*v),
            _ => None,
        };
        let sleeping_kind = match map.get(&JavaString::from("sleepingKind")) {
            Some(Tag::Byte(v)) => BedKind::from_u8(*v as u8),
            _ => None,
        };
        Self {
            sleep_time: get_i64("sleepTime"),
            wake_time: get_i64("wakeTime"),
            tired_time: get_i64("tiredTime"),
            auto_sleep_pos,
            auto_sleep_custom_name,
            saved_time_since_rest,
            sleeping_kind,
        }
    }
}

/// Whether the Overworld clock time is considered bright/daytime (`Level.isBrightOutside`).
/// In vanilla Overworld, sky darkening is below 4 for day ticks `0..12542` (or `0..12010` in rain).
/// Matches vanilla's `Level.isDarkOutside` / `isBrightOutside` approximation when sky darkening is
/// computed from day time and weather.
pub fn is_bright_outside(clock_time: i64, is_raining: bool, is_thundering: bool) -> bool {
    if is_thundering {
        return false;
    }
    let tod = clock_time.rem_euclid(DAY_TICKS);
    if is_raining {
        !(12_010..23_992).contains(&tod)
    } else {
        !(12_542..23_460).contains(&tod)
    }
}

/// Whether `time_use` allows sleeping right now, given the dimension's `bed_can_sleep` rule
/// (`"always"`, `"when_dark"`, `"never"`), daytime flag, and thunder state.
pub fn can_sleep_for_time(
    time_use: TimeUse,
    dim_bed_can_sleep: &str,
    bright_outside: bool,
    is_thundering: bool,
) -> bool {
    match time_use {
        TimeUse::None => false,
        TimeUse::UseWithoutSleeping => true,
        TimeUse::Night => match dim_bed_can_sleep {
            "always" => true,
            "never" => false,
            _ => !bright_outside || is_thundering,
        },
        TimeUse::Day => match dim_bed_can_sleep {
            "always" => true,
            "never" => false,
            _ => bright_outside && !is_thundering,
        },
        TimeUse::DayOrNight => dim_bed_can_sleep != "never",
    }
}

/// Resolve the next tick matching `target_in_period` strictly after `current_time`,
/// mirroring vanilla `ClockTimeMarker.resolveTimeToMoveTo`.
pub fn resolve_next_marker_time(current_time: i64, period: i64, target_in_period: i64) -> i64 {
    if period <= 0 {
        return current_time;
    }
    let rem = current_time.rem_euclid(period);
    let target = target_in_period.rem_euclid(period);
    let mut delta = (target - rem).rem_euclid(period);
    if delta == 0 {
        delta = period;
    }
    current_time.saturating_add(delta)
}

/// Compute the target world clock time when waking up (`ComfortsEvents.getWakeTime`).
/// `marker_tick` is the clock's resolved marker tick if known (from `level.clock_marker_time`),
/// otherwise falls back to `11999` (nap) or `0` (night sleep).
pub fn compute_wake_time(
    current_time: i64,
    wake_from_nap: bool,
    marker_tick: Option<i64>,
    day_wake_offset: i32,
    night_wake_offset: i32,
) -> i64 {
    let target_tick = marker_tick.unwrap_or(if wake_from_nap {
        DEFAULT_NAP_WAKE_TICK
    } else {
        DEFAULT_SLEEP_WAKE_TICK
    });
    let base = resolve_next_marker_time(current_time, DAY_TICKS, target_tick);
    let offset = if wake_from_nap {
        i64::from(night_wake_offset)
    } else {
        i64::from(day_wake_offset)
    };
    base.saturating_add(offset).max(0)
}

/// Compute `tired_time` after sleeping (`ComfortsEvents.onWakeUp`):
/// `wake_time + (time_slept / rest_multiplier)`.
pub fn compute_tired_time(wake_time: i64, time_slept: i64, rest_multiplier: f64) -> i64 {
    if time_slept <= 0 || rest_multiplier <= 0.0 || !rest_multiplier.is_finite() {
        return wake_time;
    }
    let wait = (time_slept as f64 / rest_multiplier) as i64;
    wake_time.saturating_add(wait.max(0))
}

/// Compute effective sleeping bag break probability (`0.0..=1.0`) given config percentage (`0..=100`),
/// player's `minecraft:luck` attribute value, and `sleepingBagBreakChanceLuckMultiplier`.
pub fn compute_break_chance(break_chance_pct: i32, luck: f64, luck_multiplier: f64) -> f64 {
    let mut chance = f64::from(break_chance_pct.clamp(0, 100)) / 100.0;
    if luck_multiplier > 0.0 && luck.is_finite() {
        chance -= luck_multiplier * luck;
    }
    chance.clamp(0.0, 1.0)
}

/// `SleepStatus.sleepersNeeded` for a given `active` player count and percentage.
/// Matches `Math.max(1, Mth.ceil((float) (active * pct) / 100.0F))`.
pub fn sleepers_needed(active: i64, percentage: i32) -> i64 {
    if active <= 0 {
        return 1;
    }
    let pct = i64::from(percentage.max(0));
    ((active * pct + 99) / 100).max(1)
}

/// Horizontal direction offset `[dx, 0, dz]`.
pub const fn horizontal_step(dir: Direction) -> [i32; 3] {
    match dir {
        Direction::North => [0, 0, -1],
        Direction::South => [0, 0, 1],
        Direction::West => [-1, 0, 0],
        Direction::East => [1, 0, 0],
        Direction::Up => [0, 1, 0],
        Direction::Down => [0, -1, 0],
    }
}

pub fn offset(pos: [i32; 3], step: [i32; 3], scale: i32) -> [i32; 3] {
    [
        pos[0] + step[0] * scale,
        pos[1] + step[1] * scale,
        pos[2] + step[2] * scale,
    ]
}

pub fn parse_direction(name: &str) -> Option<Direction> {
    match name {
        "north" => Some(Direction::North),
        "south" => Some(Direction::South),
        "west" => Some(Direction::West),
        "east" => Some(Direction::East),
        "up" => Some(Direction::Up),
        "down" => Some(Direction::Down),
        _ => None,
    }
}

pub const fn direction_name(dir: Direction) -> &'static str {
    match dir {
        Direction::North => "north",
        Direction::South => "south",
        Direction::West => "west",
        Direction::East => "east",
        Direction::Up => "up",
        Direction::Down => "down",
    }
}

/// Given a `rope_and_nail` at `rope_pos` facing `dir`, compute:
/// - `foot_pos`: `rope_pos + dir`
/// - `head_pos`: `rope_pos + 2 * dir`
/// - `partner_rope_pos`: `rope_pos + 3 * dir`
///
/// Matches `HammockItem.useOn` / `HammockBlock.hasPartneredRopes`.
pub fn hammock_layout_from_rope(
    rope_pos: [i32; 3],
    dir: Direction,
) -> ([i32; 3], [i32; 3], [i32; 3]) {
    let step = horizontal_step(dir);
    (
        offset(rope_pos, step, 1),
        offset(rope_pos, step, 2),
        offset(rope_pos, step, 3),
    )
}

/// Given one half of a hammock (`pos`, `facing`, `is_head`), return:
/// - `partner_half_pos`
/// - `own_rope_pos` (the rope directly behind this half)
/// - `partner_rope_pos` (the rope directly behind the other half)
pub fn hammock_positions(
    pos: [i32; 3],
    facing: Direction,
    is_head: bool,
) -> ([i32; 3], [i32; 3], [i32; 3]) {
    let step = horizontal_step(facing);
    if is_head {
        let foot = offset(pos, step, -1);
        let head_rope = offset(pos, step, 1);
        let foot_rope = offset(pos, step, -2);
        (foot, head_rope, foot_rope)
    } else {
        let head = offset(pos, step, 1);
        let foot_rope = offset(pos, step, -1);
        let head_rope = offset(pos, step, 2);
        (head, foot_rope, head_rope)
    }
}

/// Clockwise 90° horizontal rotation (`Direction.getClockWise`).
pub const fn clock_wise(dir: Direction) -> Direction {
    match dir {
        Direction::North => Direction::East,
        Direction::East => Direction::South,
        Direction::South => Direction::West,
        Direction::West => Direction::North,
        other => other,
    }
}

/// Vanilla `AbstractBedBlock.bedStandUpOffsets(bedFacing, sideFacing)` candidate offsets
/// relative to `head` (first side, then opposite side, then around bed).
pub fn stand_up_offsets(bed_facing: Direction) -> [[i32; 2]; 10] {
    let [bx, _, bz] = horizontal_step(bed_facing);
    let side = clock_wise(bed_facing);
    let [sx, _, sz] = horizontal_step(side);
    [
        [sx, sz],
        [-bx, -bz],
        [-bx + sx, -bz + sz],
        [-bx * 2, -bz * 2],
        [-bx * 2 + sx, -bz * 2 + sz],
        [bx, bz],
        [bx + sx, bz + sz],
        [-sx, -sz],
        [-bx - sx, -bz - sz],
        [-bx * 2 - sx, -bz * 2 - sz],
    ]
}

/// Yaw in degrees facing from `from` toward the centre of block `target`.
pub fn look_at_yaw(from: [f64; 3], target: [i32; 3]) -> f32 {
    let dx = f64::from(target[0]) + 0.5 - from[0];
    let dz = f64::from(target[2]) + 0.5 - from[2];
    (dz.atan2(dx).to_degrees() - 90.0) as f32
}

/// Vanilla `ServerPlayer.isWithinRange` / `isReachableBedBlock` check (`|dx| <= 3, |dy| <= 2, |dz| <= 3`).
pub fn within_bed_reach(player_pos: [f64; 3], head: [i32; 3], foot: [i32; 3]) -> bool {
    let check = |p: [i32; 3]| {
        (player_pos[0] - f64::from(p[0])).abs() <= 3.0
            && (player_pos[1] - f64::from(p[1])).abs() <= 2.0
            && (player_pos[2] - f64::from(p[2])).abs() <= 3.0
    };
    check(head) || check(foot)
}

/// Conditional crafting recipe model shared by `comforts:conditional_shaped` and
/// `comforts:conditional_shapeless`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecipeCondition {
    Always,
    SleepingBag,
    Hammock,
    HammockAndRopes,
}

impl RecipeCondition {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "always" => Some(Self::Always),
            "sleeping_bag" => Some(Self::SleepingBag),
            "hammock" => Some(Self::Hammock),
            "hammock_and_ropes" => Some(Self::HammockAndRopes),
            _ => None,
        }
    }

    pub fn is_satisfied(self, common: CommonConfig, has_ropes_tag: bool) -> bool {
        match self {
            Self::Always => true,
            Self::SleepingBag => common.enable_sleeping_bag_recipes,
            Self::Hammock => common.enable_hammock_recipes,
            Self::HammockAndRopes => common.enable_hammock_recipes && has_ropes_tag,
        }
    }
}

/// One ingredient choice in a conditional recipe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecipeIngredient {
    Item(String),
    Tag(String),
    AnyOf(Vec<String>),
}

#[cfg(test)]
impl RecipeIngredient {
    pub fn matches<F>(&self, item_id: &str, in_tag: &F) -> bool
    where
        F: Fn(&str, &str) -> bool,
    {
        match self {
            Self::Item(expected) => expected == item_id,
            Self::Tag(tag) => in_tag(tag, item_id),
            Self::AnyOf(choices) => choices.iter().any(|c| {
                if let Some(tag) = c.strip_prefix('#') {
                    in_tag(tag, item_id)
                } else {
                    c == item_id
                }
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShapedConditionalRecipe {
    pub condition: RecipeCondition,
    pub width: usize,
    pub height: usize,
    pub cells: Vec<Option<RecipeIngredient>>,
    pub result_item: String,
    pub result_count: u8,
    pub raw_json: String,
}

#[cfg(test)]
impl ShapedConditionalRecipe {
    pub fn matches_grid<F>(
        &self,
        width: usize,
        height: usize,
        grid: &[Option<&str>],
        in_tag: F,
    ) -> bool
    where
        F: Fn(&str, &str) -> bool,
    {
        if self.width == 0 || self.height == 0 || width < self.width || height < self.height {
            return false;
        }
        for oy in 0..=(height - self.height) {
            for ox in 0..=(width - self.width) {
                if self.matches_at((width, height), grid, (ox, oy), false, &in_tag)
                    || self.matches_at((width, height), grid, (ox, oy), true, &in_tag)
                {
                    return true;
                }
            }
        }
        false
    }

    fn matches_at<F>(
        &self,
        (width, height): (usize, usize),
        grid: &[Option<&str>],
        (ox, oy): (usize, usize),
        mirror: bool,
        in_tag: &F,
    ) -> bool
    where
        F: Fn(&str, &str) -> bool,
    {
        for y in 0..height {
            for x in 0..width {
                let grid_item = grid[y * width + x];
                let expected = if x >= ox && x < ox + self.width && y >= oy && y < oy + self.height
                {
                    let rx = if mirror {
                        self.width - 1 - (x - ox)
                    } else {
                        x - ox
                    };
                    let ry = y - oy;
                    self.cells[ry * self.width + rx].as_ref()
                } else {
                    None
                };
                match (expected, grid_item) {
                    (None, None) => {}
                    (Some(ing), Some(id)) if ing.matches(id, in_tag) => {}
                    _ => return false,
                }
            }
        }
        true
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShapelessConditionalRecipe {
    pub condition: RecipeCondition,
    pub ingredients: Vec<RecipeIngredient>,
    pub result_item: String,
    pub result_count: u8,
    pub raw_json: String,
}

#[cfg(test)]
impl ShapelessConditionalRecipe {
    pub fn matches_grid<F>(&self, grid: &[Option<&str>], in_tag: F) -> bool
    where
        F: Fn(&str, &str) -> bool,
    {
        let items: Vec<&str> = grid.iter().flatten().copied().collect();
        if items.len() != self.ingredients.len() {
            return false;
        }
        let mut used = vec![false; items.len()];
        self.backtrack(0, &items, &mut used, &in_tag)
    }

    fn backtrack<F>(&self, idx: usize, items: &[&str], used: &mut [bool], in_tag: &F) -> bool
    where
        F: Fn(&str, &str) -> bool,
    {
        if idx == self.ingredients.len() {
            return true;
        }
        for (i, item) in items.iter().enumerate() {
            if !used[i] && self.ingredients[idx].matches(item, in_tag) {
                used[i] = true;
                if self.backtrack(idx + 1, items, used, in_tag) {
                    return true;
                }
                used[i] = false;
            }
        }
        false
    }
}

/// Build shaped pattern rows trimmed to the minimal bounding box (matching vanilla `ShapedRecipePattern`).
pub fn trim_shaped_pattern(
    rows: &[String],
    keys: &BTreeMap<char, RecipeIngredient>,
) -> Result<(usize, usize, Vec<Option<RecipeIngredient>>), String> {
    if rows.is_empty() {
        return Err("empty pattern".into());
    }
    let row_w = rows[0].chars().count();
    if row_w == 0 || rows.iter().any(|r| r.chars().count() != row_w) {
        return Err("inconsistent pattern row widths".into());
    }
    let mut min_x = row_w;
    let mut max_x = 0usize;
    let mut min_y = rows.len();
    let mut max_y = 0usize;
    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            if ch != ' ' {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }
    if min_x > max_x || min_y > max_y {
        return Err("blank pattern".into());
    }
    let width = max_x - min_x + 1;
    let height = max_y - min_y + 1;
    let mut cells = Vec::with_capacity(width * height);
    for row in &rows[min_y..=max_y] {
        let chars: Vec<char> = row.chars().collect();
        for &ch in &chars[min_x..=max_x] {
            if ch == ' ' {
                cells.push(None);
            } else {
                let ing = keys
                    .get(&ch)
                    .ok_or_else(|| format!("unknown pattern key '{ch}'"))?;
                cells.push(Some(ing.clone()));
            }
        }
    }
    Ok((width, height, cells))
}

// --- Lightweight TOML key/value helpers ---

fn parse_toml_kv(text: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    for raw_line in text.lines() {
        let line = strip_toml_comment(raw_line).trim();
        if line.is_empty() || line.starts_with('[') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            let v = v.trim();
            if !k.is_empty() && !v.is_empty() {
                out.push((k, v));
            }
        }
    }
    out
}

fn strip_toml_comment(line: &str) -> &str {
    let mut in_quotes = false;
    let mut escape = false;
    for (i, ch) in line.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_quotes => escape = true,
            '"' => in_quotes = !in_quotes,
            '#' if !in_quotes => return &line[..i],
            _ => {}
        }
    }
    line
}

fn unquote(v: &str) -> &str {
    v.trim().trim_matches('"')
}

fn parse_bool(v: &str) -> Option<bool> {
    match v.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn parse_i32(v: &str) -> Option<i32> {
    v.trim().parse().ok()
}

fn parse_f64(v: &str) -> Option<f64> {
    v.trim().parse::<f64>().ok().filter(|n| n.is_finite())
}

fn parse_string_list(v: &str) -> Vec<String> {
    let inner = v.trim().strip_prefix('[').and_then(|s| s.strip_suffix(']'));
    let Some(inner) = inner else {
        return Vec::new();
    };
    inner
        .split(',')
        .map(|item| unquote(item).trim())
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_and_server_defaults_round_trip() {
        assert_eq!(CommonConfig::parse(DEFAULT_COMMON), CommonConfig::default());
        assert_eq!(ServerConfig::parse(DEFAULT_SERVER), ServerConfig::default());
    }

    #[test]
    fn server_config_clamps_and_parses_effects() {
        let raw = r#"
autoUse = false
restrictSleeping = true
restMultiplier = 99.0
hammockUse = "DAY_OR_NIGHT"
sleepingBagUse = "USE_WITHOUT_SLEEPING"
daySleepingPercentage = 50
dayWakeTimeOffset = 5000
nightWakeTimeOffset = -5000
hammocksStopPhantoms = false
sleepingBagsStopPhantoms = false
sleepingBagBreakChance = 150
sleepingBagBreakChanceLuckMultiplier = 0.25
sleepingBagEffects = ["minecraft:slowness;30;2", "weakness;9999;10", "bad_entry"]
"#;
        let cfg = ServerConfig::parse(raw);
        assert!(!cfg.auto_use);
        assert!(cfg.restrict_sleeping);
        assert_eq!(cfg.rest_multiplier, 20.0);
        assert_eq!(cfg.hammock_use, TimeUse::DayOrNight);
        assert_eq!(cfg.sleeping_bag_use, TimeUse::UseWithoutSleeping);
        assert_eq!(cfg.day_sleeping_percentage, 50);
        assert_eq!(cfg.day_wake_time_offset, 2000);
        assert_eq!(cfg.night_wake_time_offset, -2000);
        assert!(!cfg.hammocks_stop_phantoms);
        assert!(!cfg.sleeping_bags_stop_phantoms);
        assert_eq!(cfg.sleeping_bag_break_chance, 100);
        assert_eq!(cfg.sleeping_bag_break_chance_luck_multiplier, 0.25);
        assert_eq!(
            cfg.sleeping_bag_effects,
            vec![
                EffectSpec {
                    id: "minecraft:slowness".into(),
                    duration_ticks: 600,
                    amplifier: 1,
                },
                EffectSpec {
                    id: "minecraft:weakness".into(),
                    duration_ticks: 32_000,
                    amplifier: 3,
                },
            ]
        );
    }

    #[test]
    fn client_sync_wire_round_trips_and_rejects_bad_bytes() {
        let sync = ClientSyncConfig {
            auto_use: false,
            sleeping_bag_use: TimeUse::UseWithoutSleeping,
            hammock_use: TimeUse::DayOrNight,
        };
        assert_eq!(ClientSyncConfig::decode(&sync.encode()), Some(sync));
        assert_eq!(ClientSyncConfig::decode(&[0, 1]), None);
        assert_eq!(ClientSyncConfig::decode(&[2, 0, 0]), None);
        assert_eq!(ClientSyncConfig::decode(&[0, 9, 0]), None);
    }

    #[test]
    fn sleep_data_compound_round_trips() {
        let data = SleepData {
            sleep_time: 13_000,
            wake_time: 24_000,
            tired_time: 29_500,
            auto_sleep_pos: Some([10, 64, -20]),
            auto_sleep_custom_name: None,
            saved_time_since_rest: Some(72_000),
            sleeping_kind: Some(BedKind::SleepingBag),
        };
        assert_eq!(SleepData::from_compound(&data.to_compound()), data);
    }

    #[test]
    fn time_use_and_wake_time_math_match_upstream() {
        assert!(can_sleep_for_time(TimeUse::Day, "when_dark", true, false));
        assert!(!can_sleep_for_time(TimeUse::Day, "when_dark", false, false));
        assert!(!can_sleep_for_time(TimeUse::Day, "when_dark", true, true));
        assert!(can_sleep_for_time(
            TimeUse::Night,
            "when_dark",
            false,
            false
        ));
        assert!(can_sleep_for_time(TimeUse::Night, "when_dark", true, true));
        assert!(!can_sleep_for_time(TimeUse::None, "always", true, false));
        assert!(can_sleep_for_time(
            TimeUse::UseWithoutSleeping,
            "never",
            true,
            false
        ));

        // Night -> Dawn (day 1 at 14000 -> 24000)
        assert_eq!(compute_wake_time(14_000, false, None, 0, 0), 24_000);
        // Day -> Dusk nap (day 1 at 1000 -> 11999)
        assert_eq!(compute_wake_time(1_000, true, None, 0, 0), 11_999);
        // Offsets apply
        assert_eq!(compute_wake_time(1_000, true, None, 0, 500), 12_499);
        assert_eq!(compute_wake_time(14_000, false, None, -200, 0), 23_800);
        // Tired time: slept 10000 ticks with restMultiplier 2.0 => wait 5000 ticks
        assert_eq!(compute_tired_time(24_000, 10_000, 2.0), 29_000);
    }

    #[test]
    fn break_chance_and_luck_multiplier_work() {
        assert!((compute_break_chance(50, 0.0, 0.0) - 0.5).abs() < 1e-9);
        assert!((compute_break_chance(50, 2.0, 0.1) - 0.3).abs() < 1e-9);
        assert_eq!(compute_break_chance(10, 5.0, 0.1), 0.0);
        assert_eq!(compute_break_chance(50, -10.0, 0.1), 1.0);
    }

    #[test]
    fn sleepers_needed_matches_ceil_formula() {
        assert_eq!(sleepers_needed(4, 50), 2);
        assert_eq!(sleepers_needed(3, 50), 2);
        assert_eq!(sleepers_needed(4, 0), 1);
        assert_eq!(sleepers_needed(4, 100), 4);
    }

    #[test]
    fn hammock_layout_and_positions_agree() {
        let (foot, head, partner) = hammock_layout_from_rope([0, 64, 0], Direction::East);
        assert_eq!(foot, [1, 64, 0]);
        assert_eq!(head, [2, 64, 0]);
        assert_eq!(partner, [3, 64, 0]);

        let (other, head_rope, foot_rope) = hammock_positions(head, Direction::East, true);
        assert_eq!((other, head_rope, foot_rope), (foot, partner, [0, 64, 0]));
        let (other2, foot_rope2, head_rope2) = hammock_positions(foot, Direction::East, false);
        assert_eq!(
            (other2, foot_rope2, head_rope2),
            (head, [0, 64, 0], partner)
        );
    }

    #[test]
    fn shaped_and_shapeless_recipes_match_and_mirror() {
        let mut keys = BTreeMap::new();
        keys.insert('A', RecipeIngredient::Tag("c:strings".into()));
        keys.insert('X', RecipeIngredient::Tag("c:ingots/iron".into()));
        let (w, h, cells) =
            trim_shaped_pattern(&["AA ".into(), "AX ".into(), "  A".into()], &keys).unwrap();
        let recipe = ShapedConditionalRecipe {
            condition: RecipeCondition::Hammock,
            width: w,
            height: h,
            cells,
            result_item: "comforts:rope_and_nail".into(),
            result_count: 2,
            raw_json: String::new(),
        };
        let in_tag = |tag: &str, item: &str| {
            (tag == "c:strings" && item == "minecraft:string")
                || (tag == "c:ingots/iron" && item == "minecraft:iron_ingot")
        };
        let normal = [
            Some("minecraft:string"),
            Some("minecraft:string"),
            None,
            Some("minecraft:string"),
            Some("minecraft:iron_ingot"),
            None,
            None,
            None,
            Some("minecraft:string"),
        ];
        assert!(recipe.matches_grid(3, 3, &normal, in_tag));

        // Horizontally mirrored
        let mirrored = [
            None,
            Some("minecraft:string"),
            Some("minecraft:string"),
            None,
            Some("minecraft:iron_ingot"),
            Some("minecraft:string"),
            Some("minecraft:string"),
            None,
            None,
        ];
        assert!(recipe.matches_grid(3, 3, &mirrored, in_tag));

        let shapeless = ShapelessConditionalRecipe {
            condition: RecipeCondition::HammockAndRopes,
            ingredients: vec![
                RecipeIngredient::Tag("c:ingots/iron".into()),
                RecipeIngredient::Tag("c:ropes".into()),
            ],
            result_item: "comforts:rope_and_nail".into(),
            result_count: 2,
            raw_json: String::new(),
        };
        let in_tag2 = |tag: &str, item: &str| {
            (tag == "c:ingots/iron" && item == "minecraft:iron_ingot")
                || (tag == "c:ropes" && item == "farmersdelight:rope")
        };
        assert!(shapeless.matches_grid(
            &[
                Some("farmersdelight:rope"),
                None,
                Some("minecraft:iron_ingot"),
                None
            ],
            in_tag2
        ));
        assert!(!shapeless.matches_grid(&[Some("minecraft:iron_ingot")], in_tag2));
    }

    #[test]
    fn sleep_data_compound_round_trips_custom_name() {
        let original = SleepData {
            sleep_time: 13_500,
            wake_time: 24_000,
            tired_time: 45_000,
            auto_sleep_pos: Some([12, 64, -8]),
            auto_sleep_custom_name: Some(Tag::String("Camping Bag".into())),
            saved_time_since_rest: Some(72_000),
            sleeping_kind: Some(BedKind::SleepingBag),
        };
        let encoded = original.to_compound();
        assert_eq!(SleepData::from_compound(&encoded), original);
    }

    #[test]
    fn hammock_layout_all_cardinal_directions() {
        for dir in [
            Direction::North,
            Direction::South,
            Direction::East,
            Direction::West,
        ] {
            let rope = [10, 70, 20];
            let (foot, head, partner) = hammock_layout_from_rope(rope, dir);
            let (other, head_rope, foot_rope) = hammock_positions(head, dir, true);
            assert_eq!(other, foot);
            assert_eq!(head_rope, partner);
            assert_eq!(foot_rope, rope);
            let (other2, foot_rope2, head_rope2) = hammock_positions(foot, dir, false);
            assert_eq!(other2, head);
            assert_eq!(foot_rope2, rope);
            assert_eq!(head_rope2, partner);
        }
    }

    #[test]
    fn time_use_and_dimension_sleep_matrix() {
        // Never-sleep dimensions reject Night/Day/DayOrNight, but UseWithoutSleeping still sits.
        assert!(!can_sleep_for_time(TimeUse::Night, "never", false, false));
        assert!(!can_sleep_for_time(TimeUse::Day, "never", true, false));
        assert!(!can_sleep_for_time(
            TimeUse::DayOrNight,
            "never",
            true,
            false
        ));
        assert!(can_sleep_for_time(
            TimeUse::UseWithoutSleeping,
            "never",
            true,
            false
        ));
        // Always-sleep dimensions allow Night/Day/DayOrNight.
        assert!(can_sleep_for_time(TimeUse::Night, "always", true, false));
        assert!(can_sleep_for_time(TimeUse::Day, "always", false, false));
        // Overworld ("when_dark"): thunderstorm allows Night, blocks Day.
        assert!(can_sleep_for_time(TimeUse::Night, "when_dark", true, true));
        assert!(!can_sleep_for_time(TimeUse::Day, "when_dark", true, true));
        assert!(can_sleep_for_time(TimeUse::Day, "when_dark", true, false));
        assert!(!can_sleep_for_time(TimeUse::Day, "when_dark", false, false));
        assert!(can_sleep_for_time(
            TimeUse::DayOrNight,
            "when_dark",
            true,
            false
        ));
        assert!(can_sleep_for_time(
            TimeUse::DayOrNight,
            "when_dark",
            false,
            false
        ));
    }
}
