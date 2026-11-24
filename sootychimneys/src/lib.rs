//! Sooty Chimneys for Sunrise: seven chimneys, each clean and dirty, that smoke in the wind,
//! get sooty while they work, and are scraped clean with an axe.
//!
//! The pure parts (kinds, wind, smoke, the config file, the scraping recipe) are in `model.rs`;
//! this file binds them to the game.

mod model;

use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use mod_api::{
    Block, BlockEntityClientTickEvent, BlockEntityClientTicksEvent, BlockEntityPart, BlockKey,
    BlockMetadata, BlockPart, BlockProperties, BlockShapes, BlockState, BlockStateProperty,
    BlocksEvent, BuildCreativeModeTabContentsEvent, ClientHost, Config as ModConfig, ConfigContext,
    ConfigKind, ConfigLoadingEvent, ConfigReloadingEvent, CreativeTabPlacement,
    CreativeTabVisibility, EquipmentSlot, Flow, Hand, Hooks, Host, HostBlockStates, HostSide,
    InteractionResult, ItemKey, ItemStack, LegacyRandomSource, LevelHost, LevelRef,
    LevelTickPostEvent, LevelWriteHost, LivingRef, NeighborChangedEvent, NoteInstrument,
    ParticleKey, ParticleOptions, RandomSource, RandomTickEvent, RandomTicks, RegisterConfigsEvent,
    Registrar, RegistrationContext, ServerHost, SoundSource, StatType, TagKey, Text,
    UseItemOnEvent, UseWithoutItemEvent,
};
use model::{
    CLEAN_HARDNESS, KINDS, RESISTANCE, ScrapingRecipe, Settings, TimeOfDay, Weather, Wind,
    WindState,
};

const MOD: &str = "sootychimneys";
const RECIPE_TYPE: &str = "sootychimneys:soot_scraping";
const SOOT_SCRAPED: &str = "sootychimneys:soot_scraped";
const SMOKE_BLOCKING: &str = "sootychimneys:smoke_blocking";
const SMOKE_BOOSTING: &str = "sootychimneys:smoke_boosting";

/// The registry keys the mod asks about, looked up once at registration.
struct Keys {
    /// Each kind's clean and dirty block, in `KINDS` order.
    blocks: [[BlockKey; 2]; 7],
    items: [[ItemKey; 2]; 7],
    scrapers: TagKey,
    cosy_smoke: ParticleKey,
    signal_smoke: ParticleKey,
    smoke: ParticleKey,
    large_smoke: ParticleKey,
}

impl Keys {
    /// The kind and dirtiness of a chimney block (`instanceof ChimneyBlock`).
    fn chimney(&self, block: BlockKey) -> Option<(usize, bool)> {
        self.blocks.iter().enumerate().find_map(|(kind, pair)| {
            pair.iter()
                .position(|key| *key == block)
                .map(|dirty| (kind, dirty == 1))
        })
    }
}

static KEYS: OnceLock<Keys> = OnceLock::new();

fn keys() -> &'static Keys {
    KEYS.get()
        .expect("the mod's keys are looked up at registration")
}

static SETTINGS: LazyLock<Mutex<Settings>> = LazyLock::new(|| Mutex::new(Settings::default()));
/// The one wind (`Wind.wind`), moved after every level's tick on either side.
static WIND: Mutex<Wind> = Mutex::new(Wind::STILL);
/// Draws the original takes from the client level's random, which no client hook lends: the
/// chimneys' smoke and the client levels' wind. They are cosmetic and seeded from the clock.
static CLIENT_RANDOM: LazyLock<Mutex<LegacyRandomSource>> =
    LazyLock::new(|| Mutex::new(LegacyRandomSource::new(clock_seed())));
/// `RandomOffset.RANDOM`, the original's own `java.util.Random`.
static OFFSETS: LazyLock<Mutex<LegacyRandomSource>> =
    LazyLock::new(|| Mutex::new(LegacyRandomSource::new(clock_seed() ^ 0x5DEE_CE66D)));

fn clock_seed() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_nanos() as i64)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

fn settings() -> Settings {
    *lock(&SETTINGS)
}

fn flags() -> mod_api::BlockUpdateFlags {
    mod_api::BlockUpdateFlags::NEIGHBORS | mod_api::BlockUpdateFlags::CLIENTS
}

fn flag(states: &dyn BlockMetadata, state: BlockState, name: &str) -> bool {
    matches!(state.property(states, name), Ok(Some("true")))
}

fn text(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn offset_by(pos: [i32; 3], dy: i32) -> [i32; 3] {
    [pos[0], pos[1] + dy, pos[2]]
}

/// `shouldEmitSmoke`: lit, open, nothing stacked on it, and nothing smoke-blocking above.
fn emits_smoke(
    states: &dyn BlockMetadata,
    state: BlockState,
    blocked_above: impl FnOnce() -> bool,
) -> bool {
    flag(states, state, "lit")
        && !flag(states, state, "blocked")
        && !flag(states, state, "stacked")
        && !blocked_above()
}

/// `spawnSootParticles` on the server: a cloud of large smoke where a dirty chimney was.
fn soot_cloud(level: &mut dyn LevelWriteHost, pos: [i32; 3]) {
    let large_smoke = ParticleOptions::simple(keys().large_smoke);
    let centre = pos.map(|c| f64::from(c) + 0.5);
    let range = f64::from(0.8_f32);
    let mut i = 0;
    // The bound is drawn again before every particle, as the original's loop condition is.
    while level
        .with_random(|random| random.next_int_range(12, 20))
        .is_some_and(|bound| i < bound)
    {
        let at = {
            let mut offsets = lock(&OFFSETS);
            centre.map(|c| model::offset(&mut *offsets, c, range))
        };
        level.send_particles(&large_smoke, None, at, [0.0; 3], 0.0, 1);
        i += 1;
    }
}

/// A chimney block: one of the seven kinds, clean or dirty (`ChimneyBlock`).
struct Chimney {
    kind: usize,
    dirty: bool,
}

#[mod_api::sunrise_part]
impl BlockPart for Chimney {
    /// `updateState`: stacked under another chimney, lit unless powered or over an unlit
    /// chimney. A dirty chimney with one put on top comes clean in a cloud of soot.
    /// Placing a chimney doesn't run this (`onPlace`, sunrise-org/sunrise#5302).
    fn neighbor_changed(&self, level: &mut dyn LevelWriteHost, event: &mut NeighborChangedEvent) {
        let keys = keys();
        let pos = [event.pos.x, event.pos.y, event.pos.z];
        let stacked = level
            .block_state(offset_by(pos, 1))
            .is_some_and(|above| keys.chimney(above.block()).is_some());
        let mut state = event.state;
        let cleaned = stacked && self.dirty;
        if cleaned {
            let Some(clean) = level.block_state_default(keys.blocks[self.kind][0]) else {
                return;
            };
            state = clean;
            soot_cloud(level, pos);
        }
        let unlit_below = level.block_state(offset_by(pos, -1)).is_some_and(|below| {
            keys.chimney(below.block()).is_some() && !flag(&level.block_states(), below, "lit")
        });
        let lit = !level.has_neighbor_signal(pos).unwrap_or(false) && !unlit_below;
        let next = {
            let states = level.block_states();
            state
                .with(&states, "lit", text(lit))
                .and_then(|state| state.with(&states, "stacked", text(stacked)))
        };
        if let Ok(next) = next {
            level.set_block(pos, next, flags());
        }
    }

    /// `randomTick`: a working clean chimney gets dirty with the configured chance.
    fn random_tick(&self, level: &mut dyn LevelWriteHost, event: &mut RandomTickEvent) -> Flow {
        if self.dirty {
            return Flow::Continue;
        }
        let pos = [event.pos.x, event.pos.y, event.pos.z];
        let reader: &dyn LevelHost = &*level;
        let emits = emits_smoke(&reader.block_states(), event.state, || {
            reader.block_state(offset_by(pos, 1)).is_some_and(|above| {
                reader.block_in_tag(above.block(), SMOKE_BLOCKING) == Some(true)
            })
        });
        let dirty_chance = settings().dirty_chance;
        if emits
            && level
                .with_random(|random| random.next_double())
                .is_some_and(|roll| roll < dirty_chance)
            && let Some(dirty) = level.block_state_default(keys().blocks[self.kind][1])
        {
            level.set_block(pos, dirty, flags());
        }
        Flow::Cancel
    }

    /// `useWithoutItem`: an empty main hand opens or blocks the chimney.
    fn use_without_item(
        &self,
        host: &mut dyn ServerHost,
        event: &mut UseWithoutItemEvent<'_>,
    ) -> Option<InteractionResult> {
        let click = event.click;
        if click.stack.is_some() {
            return None;
        }
        let level = host.level(click.dimension.key().id())?;
        let blocked = !flag(&host.block_states(), event.state, "blocked");
        let next = event
            .state
            .with(&host.block_states(), "blocked", text(blocked))
            .ok()?;
        if !level
            .set_block(host, click.pos, next, flags())
            .unwrap_or(false)
        {
            return Some(InteractionResult::Success);
        }
        let pitch = 0.85 + level.with_random(host, |random| random.next_float())? * 0.05;
        let sound = if blocked {
            "minecraft:block.lantern.fall"
        } else {
            "minecraft:block.lantern.hit"
        };
        let centre = click.pos.map(|c| f64::from(c) + 0.5);
        level.play_sound(host, sound, centre, SoundSource::Blocks, 0.8, pitch, None);
        let origin = KINDS[self.kind].smoke.origin;
        // `pos.getX() + origin.x()` is float arithmetic.
        let at: [f64; 3] = std::array::from_fn(|i| {
            f64::from(click.pos[i] as f32 + origin[i]) - if i == 1 { 0.1 } else { 0.0 }
        });
        let smoke = ParticleOptions::simple(keys().smoke);
        let mut i = 0;
        while level
            .with_random(host, |random| random.next_int_bound(5))
            .is_some_and(|bound| i < bound)
        {
            let spread = level.with_random(host, |random| {
                std::array::from_fn(|_| random.next_gaussian() * 0.1)
            })?;
            level.send_particles(host, &smoke, None, at, spread, 0.0, 1);
            i += 1;
        }
        let key = if blocked {
            "message.sootychimneys.blocked"
        } else {
            "message.sootychimneys.open"
        };
        if let Some(player) = host.player(click.player) {
            let _ = player.action_bar(host, Text::translate(key, Vec::new()));
        }
        Some(InteractionResult::Success)
    }

    /// `useItemOn`: a soot scraper cleans a dirty chimney and scrapes off the recipe's results.
    /// The scraping player's own sound, soot and swing are drawn by the game's client
    /// (sunrise-org/sunrise#5063).
    fn use_item_on(
        &self,
        host: &mut dyn ServerHost,
        event: &mut UseItemOnEvent<'_>,
    ) -> Option<InteractionResult> {
        let keys = keys();
        let click = event.click;
        let stack = click.stack?;
        if !self.dirty || host.tags().is_in_tag(stack.item(), keys.scrapers) != Some(true) {
            return None;
        }
        let level = host.level(click.dimension.key().id())?;
        let player = host.player(click.player)?;
        let _ = player.trigger_item_used_on_block(host, click.pos, stack);
        let clean = {
            let states = host.block_states();
            let mut clean = host.block_state_default(keys.blocks[self.kind][0])?;
            for name in ["lit", "blocked", "stacked"] {
                let value = flag(&states, event.state, name);
                clean = clean.with(&states, name, text(value)).ok()?;
            }
            clean
        };
        let _ = level.set_block(host, click.pos, clean, flags());
        player.award_stat(host, StatType::Custom, SOOT_SCRAPED, 1);
        let dirty_item = ItemStack::of(keys.items[self.kind][1]);
        let recipe = host
            .recipes_of_type(RECIPE_TYPE)
            .iter()
            .filter_map(|recipe| ScrapingRecipe::parse(&recipe.json))
            .find(|recipe| {
                host.recipe_ingredient(&recipe.chimney)
                    .ok()
                    .flatten()
                    .is_some_and(|ingredient| ingredient.test(&*host, &dirty_item))
            });
        if let Some(recipe) = recipe {
            let counts = level.with_random(host, |random| {
                recipe
                    .results
                    .iter()
                    .map(|result| model::roll(result.count, result.chance, random))
                    .collect::<Vec<_>>()
            })?;
            let stacks: Vec<ItemStack> = recipe
                .results
                .iter()
                .zip(counts)
                .filter(|(_, count)| *count > 0)
                .filter_map(|(result, count)| {
                    let mut stack = host.recipe_result(&result.item).ok().flatten()?;
                    stack.set_count(count).ok()?;
                    Some(stack)
                })
                .collect();
            let face = click.hit.face;
            // A `Vector3f`: float arithmetic.
            let at: [f64; 3] = std::array::from_fn(|i| {
                let lift = if i == 1 { 0.6_f32 } else { 0.5 };
                f64::from(click.pos[i] as f32 + lift + face[i] as f32 * 0.65)
            });
            for stack in stacks {
                let Some((spot, parts)) =
                    level.with_random(host, |random| model::scatter(at, stack.count(), random))
                else {
                    break;
                };
                for (count, motion) in parts {
                    let mut part = stack.clone();
                    if part.set_count(count).is_ok() {
                        let _ = level.spawn_item_with_motion(
                            host, spot[0], spot[1], spot[2], part, motion, 0,
                        );
                    }
                }
            }
        }
        let slot = match click.hand {
            Hand::Main => EquipmentSlot::MainHand,
            Hand::Off => EquipmentSlot::OffHand,
        };
        let _ = LivingRef::new(player.entity()).hurt_and_break(host, slot, 1);
        let centre = click.pos.map(|c| f64::from(c) + 0.5);
        level.play_sound(
            host,
            "minecraft:item.axe.scrape",
            centre,
            SoundSource::Blocks,
            1.0,
            1.0,
            Some(player),
        );
        Some(InteractionResult::Success)
    }
}

/// The chimney's block entity, which only smokes (`ChimneyBlockEntity`).
struct Smoke;

/// The block entity's blocks behave as their [`Chimney`] parts say.
#[mod_api::sunrise_part]
impl BlockPart for Smoke {}

#[mod_api::sunrise_part]
impl BlockEntityPart for Smoke {
    fn block_entity_client_ticks(
        &self,
        _: &dyn ClientHost,
        event: &mut BlockEntityClientTicksEvent<'_>,
    ) {
        event.ticks = true;
    }

    /// `particleTick`: a working chimney puffs smoke, signal smoke over a stacked chimney or a
    /// smoke-boosting block.
    fn block_entity_client_tick(&self, event: &mut BlockEntityClientTickEvent<'_>) {
        let settings = settings();
        let scope = &mut event.subject;
        let Some((level, pos, state)) =
            scope.with_view(|view| (view.level(), view.position(), view.state()))
        else {
            return;
        };
        if lock(&CLIENT_RANDOM).next_double() >= settings.smoke_flow {
            return;
        }
        let keys = keys();
        let Some((kind, _)) = keys.chimney(state.block()) else {
            return;
        };
        let host = scope.host();
        let signal = {
            let reader: &dyn Host = &*host;
            let states = reader.block_states();
            let emits = emits_smoke(&states, state, || {
                level
                    .block_state(reader, offset_by(pos, 1))
                    .is_some_and(|above| {
                        reader.block_in_tag(above.block(), SMOKE_BLOCKING) == Some(true)
                    })
            });
            if !emits {
                return;
            }
            level
                .block_state(reader, offset_by(pos, -1))
                .is_some_and(|below| {
                    (keys.chimney(below.block()).is_some() && flag(&states, below, "stacked"))
                        || reader.block_in_tag(below.block(), SMOKE_BOOSTING) == Some(true)
                })
        };
        let wind = *lock(&WIND);
        let centre = pos.map(|c| f64::from(c) + 0.5);
        let puffs = model::emit(
            &KINDS[kind].smoke,
            centre,
            &wind,
            &settings,
            &mut *lock(&CLIENT_RANDOM),
            &mut *lock(&OFFSETS),
        );
        let options = ParticleOptions::simple(if signal {
            keys.signal_smoke
        } else {
            keys.cosy_smoke
        });
        for puff in puffs {
            host.add_particle(&options, puff.at, puff.velocity);
        }
    }
}

/// `Wind.getWindState`'s inputs for a level. The time of day can't be read yet
/// (sunrise-org/sunrise#5385), so every hour counts as day.
fn wind_state(host: &dyn Host, level: LevelRef) -> Option<WindState> {
    let day = (level.game_time(host)? / 24000) as i32;
    let (rain, thunder) = level.weather(host)?;
    Some(model::wind_state(
        day,
        TimeOfDay::Day,
        Weather::of(rain, thunder),
    ))
}

fn apply(mod_id: &str, kind: ConfigKind, text: &str) {
    if mod_id == MOD && kind == ConfigKind::Common {
        *lock(&SETTINGS) = Settings::parse(text);
    }
}

fn sixteenths(boxes: &[[f64; 6]]) -> Vec<[f64; 6]> {
    boxes.iter().map(|b| b.map(|v| v / 16.0)).collect()
}

#[derive(Default)]
struct Mod;

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        let ids: Vec<[String; 2]> = KINDS
            .iter()
            .map(|kind| [kind.name.to_owned(), format!("dirty_{}", kind.name)])
            .collect();
        let blocks =
            std::array::from_fn(|i| ids[i].clone().map(|id| r.block_key(&format!("{MOD}:{id}"))));
        let items =
            std::array::from_fn(|i| ids[i].clone().map(|id| r.item_key(&format!("{MOD}:{id}"))));
        let names: Vec<&str> = ids.iter().flatten().map(String::as_str).collect();
        let kind = r.block_entity_type("chimney").blocks(&names).build();
        r.block_entity_part(&kind, Smoke)
            .expect("the chimney's block entity type is the mod's own");
        for (i, pair) in blocks.iter().enumerate() {
            r.block_part(
                pair[0],
                Chimney {
                    kind: i,
                    dirty: false,
                },
            );
            r.block_part(
                pair[1],
                Chimney {
                    kind: i,
                    dirty: true,
                },
            );
        }
        r.register_recipe_type(RECIPE_TYPE);
        r.register_raw_recipe_serializer(RECIPE_TYPE, RECIPE_TYPE);
        // A refused declaration refuses the mod at load, with its reason.
        let _ = r.register_custom_stat(SOOT_SCRAPED);
        let _ = KEYS.set(Keys {
            blocks,
            items,
            scrapers: r.tag_key("sootychimneys:soot_scrapers"),
            cosy_smoke: r.particle_key("minecraft:campfire_cosy_smoke"),
            signal_smoke: r.particle_key("minecraft:campfire_signal_smoke"),
            smoke: r.particle_key("minecraft:smoke"),
            large_smoke: r.particle_key("minecraft:large_smoke"),
        });
    }

    /// The fourteen chimneys: `Properties.of()` with each kind's colour and sounds,
    /// `strength(2, 2)` then `destroyTime`, tools required; clean ones tick randomly.
    fn blocks(&self, _: &RegistrationContext, event: &mut BlocksEvent<'_>) {
        for kind in &KINDS {
            for dirty in [false, true] {
                let id = if dirty {
                    format!("{MOD}:dirty_{}", kind.name)
                } else {
                    format!("{MOD}:{}", kind.name)
                };
                let mut block = Block::new(id, "minecraft:stone").with_item(true);
                block.properties = BlockProperties {
                    hardness: Some(if dirty {
                        kind.dirty_hardness
                    } else {
                        CLEAN_HARDNESS
                    }),
                    explosion_resistance: Some(RESISTANCE),
                    requires_tool: Some(true),
                    map_color: Some(kind.map_color),
                    sounds: Some(kind.sounds.into()),
                    random_ticks: Some(RandomTicks::All(!dirty)),
                    instrument: Some(NoteInstrument::Harp),
                    ..Default::default()
                };
                block.state_properties = ["lit", "blocked", "stacked"]
                    .map(|name| (name.to_owned(), BlockStateProperty::Boolean))
                    .into();
                block.default_state = [("lit", "true"), ("blocked", "false"), ("stacked", "false")]
                    .map(|(name, value)| (name.to_owned(), value.to_owned()))
                    .into();
                let shapes = BlockShapes {
                    properties: vec!["stacked".into()],
                    values: BTreeMap::from([
                        ("true".to_owned(), sixteenths(kind.stacked)),
                        ("false".to_owned(), sixteenths(kind.regular)),
                    ]),
                    default: Some(sixteenths(kind.regular)),
                };
                block.shapes = Some(shapes.clone());
                block.collision_shapes = Some(shapes);
                // A refused declaration is reported at startup with its reason.
                let _ = event.register(block);
            }
        }
    }

    fn register_configs(&self, _: &dyn ConfigContext, event: &mut RegisterConfigsEvent<'_>) {
        event.configs.push(
            ModConfig::new(MOD, ConfigKind::Common, model::DEFAULT_COMMON)
                .file("sootychimneys-local.toml"),
        );
    }

    fn config_loading(&self, _: &dyn ConfigContext, event: &mut ConfigLoadingEvent<'_>) {
        apply(
            event.config.mod_id.as_str(),
            event.config.kind,
            &event.config.text,
        );
    }

    fn config_reloading(&self, _: HostSide, event: &mut ConfigReloadingEvent<'_>) {
        apply(
            event.config.mod_id.as_str(),
            event.config.kind,
            &event.config.text,
        );
    }

    /// `Wind.update` after each level's tick, on both sides, when the wind is on.
    fn level_tick_post(&self, host: HostSide, event: &mut LevelTickPostEvent) {
        if !settings().wind {
            return;
        }
        let level = event.level;
        match host {
            HostSide::Server(host) => {
                let Some(state) = wind_state(&*host, level) else {
                    return;
                };
                level.with_random(host, |random| lock(&WIND).update(state, random));
            }
            HostSide::Client(host) => {
                let Some(state) = wind_state(&*host, level) else {
                    return;
                };
                lock(&WIND).update(state, &mut *lock(&CLIENT_RANDOM));
            }
        }
    }

    /// The chimneys go at the end of Functional Blocks, each clean one before its dirty one.
    fn build_creative_mode_tab_contents(
        &self,
        host: HostSide,
        event: &mut BuildCreativeModeTabContentsEvent<'_>,
    ) {
        if event.tab != "minecraft:functional_blocks" {
            return;
        }
        let host: &dyn Host = match &host {
            HostSide::Server(host) => &**host,
            HostSide::Client(host) => &**host,
        };
        for item in keys().items.iter().flatten() {
            let _ = event.edit.insert(
                host,
                CreativeTabPlacement::Last,
                ItemStack::of(*item),
                CreativeTabVisibility::Both,
            );
        }
    }
}
