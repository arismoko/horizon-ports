//! Crops Love Rain for Sunrise: crops, saplings, sugar cane, cocoa, sweet berries, stems and
//! bamboo grow an extra step now and then while it rains on them (`CropsLoveRain` and its
//! growth mixins in the original).
//!
//! | Moment | Where | What it does |
//! |---|---|---|
//! | `random_tick` | [`Rain`] | remembers the block that ticked, and stops the game's own growth when the debug switch says so |
//! | `crop_grow_pre` | this module | remembers a bamboo stalk that is about to grow |
//! | `level_tick_post` | [`grow_queued`] | rolls each remembered block's extra growth and applies it |
//! | `register_configs`, `config_loading`, `config_reloading` | this module | `config/cropsloverain-common.toml` |

pub mod rules;

use std::sync::{Mutex, MutexGuard};

use mod_api::{
    BlockPart, BlockUpdateFlags, Config as ModConfig, ConfigContext, ConfigKind,
    ConfigLoadingEvent, CropGrowPreEvent, Flow, Hooks, HostBlockStates, HostSide, LevelRandomHost,
    LevelRef, LevelTickPostEvent, LevelWriteHost, LightLayer, RandomTickEvent,
    RegisterConfigsEvent, Registrar, RegistrationContext, ServerHost, TriState,
};
use rules::{Config, CropType, Leaves};

const MOD: &str = "cropsloverain";

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

fn settings() -> Config {
    *SETTINGS.lock().unwrap_or_else(|e| e.into_inner())
}

/// How many positions one level may keep queued between its random ticks and the end of the tick.
const MAX_QUEUED: usize = 65_536;

/// A remembered block: its dimension, its position, and whether it grows whatever the weather.
type Queue = Vec<(&'static str, [i32; 3], bool)>;

fn queue() -> MutexGuard<'static, Queue> {
    static QUEUE: Mutex<Queue> = Mutex::new(Vec::new());
    QUEUE
        .lock()
        .expect("the growth queue lock is never held across a panic")
}

fn enqueue(dimension: &'static str, pos: [i32; 3], always: bool) {
    let mut queue = queue();
    if queue.len() < MAX_QUEUED {
        queue.push((dimension, pos, always));
    }
}

/// A block that grows faster in the rain: it remembers the random tick, and under the debug
/// switch (`debugMode` and `haltRegularGrowth`) takes the game's own growth away.
pub struct Rain {
    /// A bamboo sapling grows on every random tick, rain or not, as in the original.
    always: bool,
}

#[mod_api::sunrise_part]
impl BlockPart for Rain {
    fn random_tick(&self, _level: &mut dyn LevelWriteHost, event: &mut RandomTickEvent) -> Flow {
        let p = event.pos;
        enqueue(event.dimension.key().id(), [p.x, p.y, p.z], self.always);
        if settings().halts_regular_growth() {
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

/// The level of one growth, with the rules' draws behind it.
struct Grower<'a> {
    host: &'a mut dyn ServerHost,
    level: LevelRef,
    config: Config,
}

impl Grower<'_> {
    fn text(&self, pos: [i32; 3]) -> Option<String> {
        let state = self.level.block_state(&*self.host, pos)?;
        self.host.block_state_string(state)
    }

    /// `CropsLoveRain.shouldGrowExtra` for the block at `pos`.
    fn roll(&mut self, kind: CropType, pos: [i32; 3]) -> bool {
        let raining_at = self.level.is_raining_at(&*self.host, pos).unwrap_or(false);
        let raining = self.level.is_raining(&*self.host).unwrap_or(false);
        let level = self.level;
        let host = &mut *self.host;
        rules::should_grow_extra(&self.config, kind, raining_at, raining, |bound| {
            level
                .with_random(host, |random| random.next_int_bound(bound))
                .unwrap_or(1)
        })
    }

    fn set(&mut self, pos: [i32; 3], text: &str, flags: BlockUpdateFlags) {
        if let Some(state) = self.host.block_state_id(text) {
            let _ = self.level.set_block(self.host, pos, state, flags);
        }
    }

    fn lit(&self, pos: [i32; 3], sky_darken: i32) -> bool {
        // `getRawBrightness`: the brighter of the block light and the sky light less the darkening.
        let block = self
            .level
            .brightness(&*self.host, LightLayer::Block, pos)
            .unwrap_or(0);
        let sky = self
            .level
            .brightness(&*self.host, LightLayer::Sky, pos)
            .unwrap_or(0);
        block.max(sky - sky_darken) >= 9
    }

    fn max_age(&self, pos: [i32; 3]) -> Option<i32> {
        let key = self.level.block(&*self.host, pos)?;
        self.host
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
        let darken = self.level.get_sky_darken(&*self.host).unwrap_or(0);
        let above = [pos[0], pos[1] + 1, pos[2]];
        if !self.lit(above, darken) || !self.roll(CropType::Sapling, pos) {
            return;
        }
        if prop(text, "stage") == Some("0") {
            if let Some(next) = with(text, "stage", "1") {
                self.set(pos, &next, BlockUpdateFlags::CLIENTS);
            }
        } else {
            let _ = self.level.bone_meal(self.host, pos);
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
            .with_random(self.host, |random| random.next_int_bound(4))
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
            let block = block_of(&t).to_owned();
            block == "minecraft:farmland"
                || self.level.block(&*self.host, ground).is_some_and(|key| {
                    self.host
                        .block_in_tag(key, "minecraft:dirt")
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
            .with_random(self.host, |random| random.next_float())
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

/// Roll and apply the extra growth of every block remembered for the level that just ticked.
pub fn grow_queued(host: &mut dyn ServerHost, level: LevelRef) {
    let Some(dimension) = level.dimension().id(&*host) else {
        return;
    };
    let config = settings();
    // Only rain makes a roll succeed, so in dry weather, or with the master switch off, only the
    // blocks that grow whatever the weather are looked at.
    let wet = config.use_rain_growth_speed && level.is_raining(&*host).unwrap_or(false);
    let waiting: Vec<[i32; 3]> = {
        let mut queue = queue();
        let (ours, rest): (Queue, Queue) = queue.drain(..).partition(|(d, _, _)| *d == dimension);
        *queue = rest;
        ours.into_iter()
            .filter(|(_, _, always)| wet || *always)
            .map(|(_, pos, _)| pos)
            .collect()
    };
    if waiting.is_empty() {
        return;
    }
    let mut grower = Grower {
        host,
        level,
        config,
    };
    for pos in waiting {
        grower.grow(pos);
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

#[derive(Default)]
struct Mod;

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        declare(r);
    }

    /// A bamboo stalk the game is about to grow: the extra growth follows its own roll.
    fn crop_grow_pre(&self, level: &mut dyn LevelRandomHost, event: &mut CropGrowPreEvent<'_>) {
        let bamboo = level.state_block(event.state) == Some("minecraft:bamboo");
        if !bamboo {
            return;
        }
        let p = event.pos;
        enqueue(event.dimension.key().id(), [p.x, p.y, p.z], false);
        if settings().halts_regular_growth() {
            *event.result = TriState::False;
        }
    }

    fn level_tick_post(&self, host: HostSide<'_>, event: &mut LevelTickPostEvent) {
        if let HostSide::Server(host) = host {
            grow_queued(host, event.level);
        }
    }

    fn register_configs(&self, _: &dyn ConfigContext, event: &mut RegisterConfigsEvent<'_>) {
        event.configs.push(ModConfig::new(
            MOD,
            ConfigKind::Common,
            rules::DEFAULT_CONFIG,
        ));
    }

    fn config_loading(&self, _: &dyn ConfigContext, event: &mut ConfigLoadingEvent<'_>) {
        if event.config.mod_id == MOD {
            *SETTINGS.lock().unwrap_or_else(|e| e.into_inner()) = Config::parse(&event.config.text);
        }
    }

    fn config_reloading(&self, _: HostSide, event: &mut mod_api::ConfigReloadingEvent<'_>) {
        if event.config.mod_id == MOD {
            *SETTINGS.lock().unwrap_or_else(|e| e.into_inner()) = Config::parse(&event.config.text);
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
