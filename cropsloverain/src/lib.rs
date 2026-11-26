//! Crops Love Rain for Sunrise: crops, saplings, sugar cane, cocoa, sweet berries, stems and
//! bamboo grow an extra step now and then while it rains on them (`CropsLoveRain` and its
//! growth mixins in the original).
//!
//! | Moment | Where | What it does |
//! |---|---|---|
//! | `random_tick` | [`Rain`] | rolls and applies the block's extra growth inside the tick, and stops the game's own growth when the debug switch says so |
//! | `crop_grow_pre` | this module | rolls a bamboo stalk that is about to grow and queues its writes |
//! | `level_tick_post` | [`apply_queued`] | applies the queued bamboo writes and bone meal |
//! | `register` (config listener: loading, reloading) | this module | `config/cropsloverain-common.toml` and its settings page |
//! | `setting_changed` | this module | a setting changed in the game goes to the live config and the file |

pub mod rules;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use mod_api::{
    BlockKey, BlockPart, BlockUpdateFlags, ConfigEvent, ConfigKind, ConfigSnapshot, ConfigSpec,
    CropGrowPreEvent, Flow, Hooks, Host, HostSide, LevelRandomHost, LevelRef, LevelTickPostEvent,
    LevelWriteHost, RandomTickEvent, Registrar, RegistrationContext, ServerHost,
    SettingChangedEvent, SettingKey, SettingSpec, SettingValue, TagKey, TriState,
};
use rules::{Config, CropType, Leaves};

const MOD_ID: &str = "cropsloverain";

static SETTINGS: Mutex<Config> = Mutex::new(Config {
    use_rain_growth_speed: true,
    rain_growth_speed: 10,
    use_individual_speeds: false,
    bamboo: 10,
    cocoa: 10,
    crops: 10,
    sapling: 10,
    sugar_cane: 10,
    sweet_berry: 10,
    separate_stem_speed: false,
    melon: 10,
    pumpkin: 10,
    debug_mode: false,
    halt_regular_growth: false,
});

/// The `minecraft:dirt` block tag, resolved once at registration (`BlockState.is(BlockTags.DIRT)`).
static DIRT: OnceLock<TagKey<BlockKey>> = OnceLock::new();

fn settings() -> Config {
    *SETTINGS.lock().unwrap_or_else(|e| e.into_inner())
}

/// How many writes one level may keep queued between its random ticks and the end of the tick.
const MAX_QUEUED: usize = 65_536;

/// A write the growth needs the server host for.
enum Edit {
    Set {
        pos: [i32; 3],
        text: String,
        flags: BlockUpdateFlags,
    },
    /// `LevelRef::bone_meal`, which only the server host runs.
    BoneMeal([i32; 3]),
}

/// The queued writes: their dimension and the write.
type Queue = Vec<(&'static str, Edit)>;

fn queue() -> MutexGuard<'static, Queue> {
    static QUEUE: Mutex<Queue> = Mutex::new(Vec::new());
    QUEUE
        .lock()
        .expect("the growth queue lock is never held across a panic")
}

fn enqueue(dimension: &'static str, edits: Vec<Edit>) {
    let mut queue = queue();
    for edit in edits {
        if queue.len() < MAX_QUEUED {
            queue.push((dimension, edit));
        }
    }
}

/// A block that grows faster in the rain: its random tick rolls and applies the extra growth,
/// and under the debug switch (`debugMode` and `haltRegularGrowth`) takes the game's own growth away.
pub struct Rain {
    /// A bamboo sapling grows on every random tick, rain or not, as in the original.
    always: bool,
}

#[mod_api::sunrise_part]
impl BlockPart for Rain {
    fn random_tick(&self, level: &mut dyn LevelWriteHost, event: &mut RandomTickEvent) -> Flow {
        let config = settings();
        let p = event.pos;
        let pos = [p.x, p.y, p.z];
        if self.always || (config.use_rain_growth_speed && level.is_raining() == Some(true)) {
            let mut grower = Grower {
                level: &mut *level,
                config,
                edits: Vec::new(),
            };
            grower.grow(pos);
            let mut queued = Vec::new();
            for edit in grower.edits {
                match edit {
                    Edit::Set { pos, text, flags } => {
                        if let Some(state) = level.block_state_id(&text) {
                            level.set_block(pos, state, flags);
                        }
                    }
                    edit => queued.push(edit),
                }
            }
            enqueue(event.dimension.key().id(), queued);
        }
        if config.halts_regular_growth() {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }
}

/// The block of a state's text.
fn block_of(text: &str) -> &str {
    text.split('[').next().unwrap_or(text)
}

/// A property of a state's text.
fn prop<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let props = text.strip_suffix(']')?.split_once('[')?.1;
    props.split(',').find_map(|pair| {
        pair.split_once('=')
            .filter(|(k, _)| *k == key)
            .map(|(_, v)| v)
    })
}

/// The text of a state with one property set.
fn with(text: &str, key: &str, value: &str) -> Option<String> {
    let (block, props) = text.strip_suffix(']')?.split_once('[')?;
    let mut found = false;
    let props: Vec<String> = props
        .split(',')
        .map(|pair| match pair.split_once('=') {
            Some((k, _)) if k == key => {
                found = true;
                format!("{key}={value}")
            }
            _ => pair.to_owned(),
        })
        .collect();
    found.then(|| format!("{block}[{}]", props.join(",")))
}

/// The level of one growth, with the rules' draws behind it. It reads the level and collects
/// the writes, which the caller applies where it can.
struct Grower<'a> {
    level: &'a mut dyn LevelRandomHost,
    config: Config,
    edits: Vec<Edit>,
}

impl Grower<'_> {
    fn text(&self, pos: [i32; 3]) -> Option<String> {
        let state = self.level.block_state(pos)?;
        self.level.state_string(state)
    }

    /// `CropsLoveRain.shouldGrowExtra` for the block at `pos`.
    fn roll(&mut self, kind: CropType, pos: [i32; 3]) -> bool {
        let raining_at = self.level.is_raining_at(pos);
        let raining = self.level.is_raining().unwrap_or(false);
        let level = &mut *self.level;
        rules::should_grow_extra(&self.config, kind, raining_at, raining, |bound| {
            level
                .with_random(|random| random.next_int_bound(bound))
                .unwrap_or(1)
        })
    }

    fn set(&mut self, pos: [i32; 3], text: &str, flags: BlockUpdateFlags) {
        self.edits.push(Edit::Set {
            pos,
            text: text.to_owned(),
            flags,
        });
    }

    /// `getRawBrightness(pos, darken) >= 9`.
    fn lit(&self, pos: [i32; 3], sky_darken: i32) -> bool {
        self.level.raw_brightness(pos, sky_darken) >= 9
    }

    fn max_age(&self, pos: [i32; 3]) -> Option<i32> {
        let key = self.level.block_state(pos)?.block();
        self.level
            .block_states()
            .definition(key)?
            .iter()
            .find(|property| property.name == "age")?
            .values
            .iter()
            .filter_map(|value| value.parse::<i32>().ok())
            .max()
    }

    fn is_air(&self, pos: [i32; 3]) -> bool {
        self.text(pos).is_some_and(|t| {
            matches!(
                t.as_str(),
                "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
            )
        })
    }

    /// One age more, up to `limit`, for the block at `pos`.
    fn age_up(&mut self, pos: [i32; 3], text: &str, age: i32, limit: i32) {
        if age < limit
            && let Some(next) = with(text, "age", &(age + 1).to_string())
        {
            self.set(pos, &next, BlockUpdateFlags::CLIENTS);
        }
    }

    fn grow(&mut self, pos: [i32; 3]) {
        let Some(text) = self.text(pos) else {
            return;
        };
        let block = block_of(&text);
        let age = prop(&text, "age").and_then(|v| v.parse::<i32>().ok());
        match block {
            "minecraft:cocoa" => {
                let age = age.unwrap_or(2);
                if age < 2 && self.roll(CropType::Cocoa, pos) {
                    self.age_up(pos, &text, age, 2);
                }
            }
            "minecraft:sweet_berry_bush" => {
                let age = age.unwrap_or(3);
                let above = [pos[0], pos[1] + 1, pos[2]];
                if age < 3 && self.lit(above, 0) && self.roll(CropType::SweetBerries, pos) {
                    self.age_up(pos, &text, age, 3);
                }
            }
            "minecraft:sugar_cane" => self.grow_cane(pos, &text),
            "minecraft:bamboo" => self.grow_bamboo(pos, &text),
            "minecraft:bamboo_sapling" => {
                let above = [pos[0], pos[1] + 1, pos[2]];
                if self.is_air(above) && self.lit(above, 0) {
                    self.set(
                        above,
                        "minecraft:bamboo[age=0,leaves=small,stage=0]",
                        BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS,
                    );
                }
            }
            "minecraft:melon_stem" | "minecraft:pumpkin_stem" => {
                let melon = block == "minecraft:melon_stem";
                self.grow_stem(pos, &text, melon);
            }
            "minecraft:pitcher_crop" | "minecraft:mangrove_propagule" => {}
            _ if block.ends_with("_sapling") && prop(&text, "stage").is_some() => {
                self.grow_sapling(pos, &text);
            }
            _ => {
                if let (Some(age), Some(max)) = (age, self.max_age(pos))
                    && self.lit(pos, 0)
                    && self.roll(CropType::Crop, pos)
                {
                    self.age_up(pos, &text, age, max);
                }
            }
        }
    }

    /// `SugarCaneGrowthSpeedup`: a new cane on top, this one back to age 0.
    fn grow_cane(&mut self, pos: [i32; 3], text: &str) {
        let above = [pos[0], pos[1] + 1, pos[2]];
        if !self.is_air(above) {
            return;
        }
        let mut height = 1;
        while self
            .text([pos[0], pos[1] - height, pos[2]])
            .is_some_and(|t| block_of(&t) == "minecraft:sugar_cane")
        {
            height += 1;
        }
        if height >= 3 || !self.roll(CropType::SugarCane, pos) {
            return;
        }
        self.set(
            above,
            "minecraft:sugar_cane[age=0]",
            BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS,
        );
        if let Some(reset) = with(text, "age", "0") {
            self.set(pos, &reset, BlockUpdateFlags::NONE);
        }
    }

    /// `SaplingGrowthSpeedup`: `SaplingBlock.advanceTree`, a stage first, then the tree.
    fn grow_sapling(&mut self, pos: [i32; 3], text: &str) {
        let darken = self.level.sky_darken().unwrap_or(0);
        let above = [pos[0], pos[1] + 1, pos[2]];
        if !self.lit(above, darken) || !self.roll(CropType::Sapling, pos) {
            return;
        }
        if prop(text, "stage") == Some("0") {
            if let Some(next) = with(text, "stage", "1") {
                self.set(pos, &next, BlockUpdateFlags::CLIENTS);
            }
        } else {
            self.edits.push(Edit::BoneMeal(pos));
        }
    }

    /// `StemBlockGrowthSpeedup`: an extra roll that makes the stem take its growth step.
    fn grow_stem(&mut self, pos: [i32; 3], text: &str, melon: bool) {
        if !self.lit(pos, 0) {
            return;
        }
        let kind = rules::stem_type(&self.config, melon);
        if !self.roll(kind, pos) {
            return;
        }
        let age = prop(text, "age").and_then(|v| v.parse::<i32>().ok());
        let Some(age) = age else {
            return;
        };
        if age < 7 {
            self.age_up(pos, text, age, 7);
            return;
        }
        let direction = self
            .level
            .with_random(|random| random.next_int_bound(4))
            .unwrap_or(0);
        let (name, dx, dz) = [
            ("north", 0, -1),
            ("east", 1, 0),
            ("south", 0, 1),
            ("west", -1, 0),
        ][direction as usize % 4];
        let target = [pos[0] + dx, pos[1], pos[2] + dz];
        let ground = [target[0], target[1] - 1, target[2]];
        let farmland = self.text(ground).is_some_and(|t| {
            block_of(&t) == "minecraft:farmland"
                || DIRT.get().is_some_and(|&tag| {
                    self.level
                        .block_state(ground)
                        .and_then(|state| state.block().is_in_tag(self.level.tags(), tag))
                        .unwrap_or(false)
                })
        });
        if !self.is_air(target) || !farmland {
            return;
        }
        let (fruit, attached) = if melon {
            ("minecraft:melon", "minecraft:attached_melon_stem")
        } else {
            ("minecraft:pumpkin", "minecraft:attached_pumpkin_stem")
        };
        self.set(
            target,
            fruit,
            BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS,
        );
        self.set(
            pos,
            &format!("{attached}[facing={name}]"),
            BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS,
        );
    }

    /// `bamboo.GrowthSpeedup` and `BambooStalkBlock.growBamboo`.
    fn grow_bamboo(&mut self, pos: [i32; 3], text: &str) {
        let above = [pos[0], pos[1] + 1, pos[2]];
        if prop(text, "stage") != Some("0") || !self.is_air(above) || !self.lit(above, 0) {
            return;
        }
        if !self.roll(CropType::Bamboo, pos) {
            return;
        }
        let mut below_count = 0;
        while below_count < 16
            && self
                .text([pos[0], pos[1] - 1 - below_count, pos[2]])
                .is_some_and(|t| block_of(&t) == "minecraft:bamboo")
        {
            below_count += 1;
        }
        let height = below_count + 1;
        if height >= 16 {
            return;
        }
        let below_pos = [pos[0], pos[1] - 1, pos[2]];
        let below2_pos = [pos[0], pos[1] - 2, pos[2]];
        let below = self
            .text(below_pos)
            .filter(|t| block_of(t) == "minecraft:bamboo");
        let below2 = self
            .text(below2_pos)
            .filter(|t| block_of(t) == "minecraft:bamboo");
        let below_leaves = below
            .as_deref()
            .map(|t| Leaves::of(prop(t, "leaves").unwrap_or("none")));
        let unit = self
            .level
            .with_random(|random| random.next_float())
            .unwrap_or(1.0);
        let own_age = u8::from(prop(text, "age") == Some("1"));
        let plan = rules::grow_bamboo(height, own_age, below_leaves, below2.is_some(), unit);
        if plan.thin_below {
            if let Some(t) = below.as_deref().and_then(|t| with(t, "leaves", "small")) {
                self.set(
                    below_pos,
                    &t,
                    BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS,
                );
            }
            if let Some(t) = below2.as_deref().and_then(|t| with(t, "leaves", "none")) {
                self.set(
                    below2_pos,
                    &t,
                    BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS,
                );
            }
        }
        self.set(
            above,
            &format!(
                "minecraft:bamboo[age={},leaves={},stage={}]",
                plan.age,
                plan.leaves.name(),
                plan.stage
            ),
            BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS,
        );
    }
}

/// Apply the writes queued for the level that just ticked.
pub fn apply_queued(host: &mut dyn ServerHost, level: LevelRef) {
    let Some(dimension) = level.dimension().id(&*host) else {
        return;
    };
    let waiting: Vec<Edit> = {
        let mut queue = queue();
        let (ours, rest): (Queue, Queue) = queue.drain(..).partition(|(d, _)| *d == dimension);
        *queue = rest;
        ours.into_iter().map(|(_, edit)| edit).collect()
    };
    for edit in waiting {
        match edit {
            Edit::Set { pos, text, flags } => {
                if let Some(state) = host.block_state_id(&text) {
                    let _ = level.set_block(host, pos, state, flags);
                }
            }
            Edit::BoneMeal(pos) => {
                let _ = level.bone_meal(host, pos);
            }
        }
    }
}

fn declare(r: &mut dyn Registrar) {
    r.block_part_tag("minecraft:crops", Rain { always: false });
    r.block_part_tag("minecraft:saplings", Rain { always: false });
    for id in [
        "minecraft:cocoa",
        "minecraft:sweet_berry_bush",
        "minecraft:sugar_cane",
    ] {
        let block = r.block_key(id);
        r.block_part(block, Rain { always: false });
    }
    let sapling = r.block_key("minecraft:bamboo_sapling");
    r.block_part(sapling, Rain { always: true });
}

/// The handles of the registered settings, by the file's key.
static SETTING_KEYS: OnceLock<Vec<(&'static str, SettingKey)>> = OnceLock::new();

/// Whether the settings were given the file's values since the game started.
static SYNCED: AtomicBool = AtomicBool::new(false);

/// The config file `register` declares (`ConfigKind::Common`).
const CONFIG_FILE: &str = "cropsloverain-common.toml";

/// Registers the settings on Options > Mods, each under its file's section.
fn register_settings(r: &mut dyn Registrar) {
    let defaults = Config::default();
    let mut keys = Vec::new();
    for (name, section, is_flag) in rules::SETTINGS {
        let label = format!("cropsloverain.configuration.{name}");
        let tooltip = format!("{label}.tooltip");
        let mut config = defaults;
        let spec = if is_flag {
            SettingSpec::toggle(name, &label, config.flag(name).copied().unwrap_or(false))
        } else {
            let default = f64::from(config.number(name).copied().unwrap_or(0));
            SettingSpec::range(name, &label, 0.0, 100.0, 1.0, default)
        };
        let has_tooltip = matches!(
            name,
            "rain_growth_speed"
                | "use_individual_speeds"
                | "separate_stem_speed"
                | "halt_regular_growth"
        );
        let spec = if has_tooltip {
            spec.tooltip(tooltip)
        } else {
            spec
        };
        match r.register_setting(spec) {
            Ok(key) => {
                let category = format!("cropsloverain.configuration.{section}");
                if let Err(error) = r.categorize_setting(name, &category) {
                    eprintln!("cropsloverain: setting {name} not categorized: {error}");
                }
                keys.push((name, key));
            }
            Err(error) => eprintln!("cropsloverain: setting {name} not registered: {error}"),
        }
    }
    let _ = SETTING_KEYS.set(keys);
}

/// Shows the config's values on the settings page. A changed value notifies `setting_changed`,
/// which finds the live config already equal and writes nothing.
fn mirror_settings(host: &mut dyn ServerHost, mut config: Config) {
    let Some(keys) = SETTING_KEYS.get() else {
        return;
    };
    for (name, key) in keys {
        let value = match config.flag(name) {
            Some(flag) => SettingValue::Toggle(*flag),
            None => SettingValue::Range(f64::from(config.number(name).copied().unwrap_or(0))),
        };
        let _ = host.set_setting(*key, value);
    }
}

#[derive(Default)]
struct Mod;

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        declare(r);
        register_settings(r);
        let _ = DIRT.set(r.tag_key("minecraft:dirt").typed());
        let spec = ConfigSpec::new(ConfigKind::Common, rules::DEFAULT_CONFIG);
        let listener = |event: ConfigEvent<'_>, snapshot: &ConfigSnapshot| -> Result<(), String> {
            if let ConfigEvent::Loading(_) | ConfigEvent::Reloading(_) = event {
                let parsed = Config::parse(&snapshot.text);
                *SETTINGS.lock().unwrap_or_else(|e| e.into_inner()) = parsed;
                // Written with the lock released: a changed value notifies `setting_changed`.
                if let ConfigEvent::Reloading(HostSide::Server(host)) = event {
                    mirror_settings(host, parsed);
                }
            }
            Ok(())
        };
        // A refused declaration leaves the built-in defaults in force.
        let _ = r.register_config(spec, Some(Arc::new(listener)));
    }

    /// A bamboo stalk the game is about to grow: the extra growth follows its own roll, and its
    /// writes wait for the end of the tick (the hook's level view cannot write).
    fn crop_grow_pre(&self, level: &mut dyn LevelRandomHost, event: &mut CropGrowPreEvent<'_>) {
        let bamboo = level.state_block(event.state) == Some("minecraft:bamboo");
        if !bamboo {
            return;
        }
        let p = event.pos;
        let config = settings();
        let mut grower = Grower {
            level,
            config,
            edits: Vec::new(),
        };
        grower.grow([p.x, p.y, p.z]);
        enqueue(event.dimension.key().id(), grower.edits);
        if config.halts_regular_growth() {
            *event.result = TriState::False;
        }
    }

    fn level_tick_post(&self, host: HostSide<'_>, event: &mut LevelTickPostEvent) {
        if let HostSide::Server(host) = host {
            if !SYNCED.swap(true, Ordering::Relaxed) {
                mirror_settings(host, settings());
            }
            apply_queued(host, event.level);
        }
    }

    fn setting_changed(&self, host: &dyn Host, event: &mut SettingChangedEvent<'_>) {
        if event.mod_id != MOD_ID {
            return;
        }
        let changed = {
            let mut config = SETTINGS.lock().unwrap_or_else(|e| e.into_inner());
            match event.value {
                SettingValue::Toggle(on) => config
                    .flag(event.key)
                    .is_some_and(|slot| std::mem::replace(slot, on) != on),
                SettingValue::Range(number) => config.number(event.key).is_some_and(|slot| {
                    std::mem::replace(slot, number.round() as i32) != number.round() as i32
                }),
                _ => false,
            }
        };
        if changed {
            let path = host.config_dir().join(CONFIG_FILE);
            let _ = std::fs::write(path, settings().to_text());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_text_reads_and_sets_a_property() {
        let text = "minecraft:wheat[age=3]";
        assert_eq!(block_of(text), "minecraft:wheat");
        assert_eq!(prop(text, "age"), Some("3"));
        assert_eq!(
            with(text, "age", "4").as_deref(),
            Some("minecraft:wheat[age=4]")
        );
        assert_eq!(with("minecraft:stone", "age", "4"), None);
        assert_eq!(prop("minecraft:air", "age"), None);
    }
}
