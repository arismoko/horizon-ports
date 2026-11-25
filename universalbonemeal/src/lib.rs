//! Universal Bone Meal by Fuzs for Sunrise: bone meal also grows cactus, sugar cane, vines,
//! nether wart, chorus, lily pads, dead bushes, small flowers, mushrooms on mycelium, ferns and
//! berry bushes on podzol, coral, spore blossoms, and turns dirt beside grass or mycelium into it.
//! Ripe melon and pumpkin stems grow their fruit.
//!
//! The original keeps its rules in a data map (`universalbonemeal:bonemealables`) and two data
//! registries of behaviours and predicates. Here its default data is the code: [`behavior`] is the
//! map, [`valid`] the predicates and [`perform`] the behaviours, with each random draw made from
//! the level's random in the original's order. Data packs can't change these rules yet
//! (sunrise-org/sunrise#5368).
use std::collections::HashMap;
use std::ops::ControlFlow;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use mod_api::{
    BlockClass, BlockKey, BlockShapeKind, BlockState, BlockUpdateFlags, BonemealEvent, Flow, Hooks,
    Host, HostBlockStates, HostSide, IPayloadContext, ItemResource, ItemStack, LegacyRandomSource,
    LevelRef, LightLayer, ParticleKey, ParticleOptions, PayloadId, RandomSource, Registrar,
    RegistrationContext, ServerHost, TagKey, ToClient,
};

/// Blocks bone meal never works on, whatever another mod says (`DENY`).
const RESISTANT: &str = "universalbonemeal:fertilizer_resistant_plants";
/// `BlockFlags`: `UPDATE_CLIENTS`.
const CLIENTS: i32 = 2;
/// `setBlockAndUpdate`: `UPDATE_NEIGHBORS | UPDATE_CLIENTS`.
const UPDATE: i32 = 3;
/// `VineBehavior.placeBlock`: `UPDATE_CLIENTS | UPDATE_KNOWN_SHAPE`.
const VINE_FLAGS: i32 = 18;
/// `PlayerSet.nearPosition`: the growth particles reach players this close.
const PARTICLE_RANGE: f64 = 64.0;
/// `DirectionProperty` order of `Plane.HORIZONTAL`, with each face's vine property.
const HORIZONTAL: [([i32; 3], &str); 4] = [
    ([0, 0, -1], "north"),
    ([1, 0, 0], "east"),
    ([0, 0, 1], "south"),
    ([-1, 0, 0], "west"),
];
/// `PipeBlock.PROPERTY_BY_DIRECTION` in `Direction` order; a direction's opposite is its index
/// with the lowest bit flipped.
const DIRECTIONS: [([i32; 3], &str); 6] = [
    ([0, -1, 0], "down"),
    ([0, 1, 0], "up"),
    ([0, 0, -1], "north"),
    ([0, 0, 1], "south"),
    ([-1, 0, 0], "west"),
    ([1, 0, 0], "east"),
];
/// `chorus_plant.json`'s `search_range`.
const CHORUS_SEARCH: i32 = 128;
/// `vine.json`'s `max_height` (`growing_plant`, downwards).
const VINE_MAX_HEIGHT: i32 = 128;
/// `podzol.json`'s `ground_state`, in its order, with each entry's weight.
const PODZOL_GROUND: [(&str, i32); 6] = [
    ("minecraft:sweet_berry_bush[age=3]", 2),
    ("minecraft:sweet_berry_bush[age=2]", 4),
    ("minecraft:sweet_berry_bush[age=1]", 8),
    ("minecraft:sweet_berry_bush", 12),
    ("minecraft:fern", 120),
    ("minecraft:dead_bush", 1),
];

/// `ParticleTypes.HAPPY_VILLAGER`, requested once when the mod registers.
static HAPPY_VILLAGER: OnceLock<ParticleKey> = OnceLock::new();
static GROWTH: OnceLock<ToClient<Growth>> = OnceLock::new();

/// Every tag the mod reads, by ID, resolved once when the mod registers.
const TAG_IDS: [&str; 10] = [
    RESISTANT,
    "minecraft:dirt",
    "minecraft:small_flowers",
    "minecraft:overrides_mushroom_light_requirement",
    "minecraft:supports_dry_vegetation",
    "minecraft:supports_lily_pad",
    "minecraft:supports_melon_stem_fruit",
    "minecraft:supports_pumpkin_stem_fruit",
    "minecraft:supports_vegetation",
    "minecraft:supports_wither_rose",
];
static TAGS: OnceLock<HashMap<&'static str, TagKey>> = OnceLock::new();

/// `ClientboundGrowthParticlesMessage`: bone meal worked on a block that has no bone meal
/// behaviour of its own, so the client draws the growth particles vanilla would have.
#[derive(Clone, Debug, mod_api::Wire)]
struct Growth {
    pos: [i32; 3],
}

/// What a copied plant needs below it to survive (`canSurvive`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Soil {
    /// `LilyPadBlock`: water, or a block that supports lily pads, with no fluid above it.
    LilyPad,
    /// `VegetationBlock` and its kinds: a block in this tag below.
    Tag(&'static str),
}

/// One entry of the original's `bonemealables` data map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Behavior {
    /// `growing_plant` upwards, one or two blocks, mostly cactus and sometimes a flower.
    Cactus,
    /// `growing_plant` upwards, one or two blocks of young sugar cane.
    SugarCane,
    /// `vine`: up to five vines below the lowest one.
    Vine,
    /// `crop_growth`: one more age, three times in four.
    NetherWart,
    /// `random_tick` two to five times, on a ripe stem with room for its fruit on this soil tag.
    Stem(&'static str),
    /// `random_tick` once.
    ChorusFlower,
    /// `chorus_plant`: a random tick on every flower the plant reaches.
    ChorusPlant,
    /// `random_neighbor_spread`: copies of the block around it.
    Spread { width: i32, most: i32, soil: Soil },
    /// `vegetation_scatter`: red and brown mushrooms.
    Mycelium,
    /// `vegetation_patch`: ferns, large ferns, sweet berry bushes and dead bushes.
    Podzol,
    /// `neighbor_conversion`: becomes the grass or mycelium next to it.
    Dirt,
    /// `placed_feature`: a coral tree, coral claw or coral mushroom, two times in five.
    Coral(&'static str),
    /// `pop_resource`: drops another spore blossom.
    SporeBlossom,
}

impl Behavior {
    /// The entry's `replace`: it applies even to a block with bone meal behaviour of its own.
    fn replaces(self) -> bool {
        matches!(self, Self::Stem(_))
    }
}

/// The original's `bonemealables` data map: a block's entry by its ID, or else by its tags.
fn behavior(id: &str, in_tag: impl Fn(&str) -> bool) -> Option<Behavior> {
    let spread = |width, most, soil| Behavior::Spread { width, most, soil };
    let entry = match id {
        "minecraft:cactus" => Behavior::Cactus,
        "minecraft:sugar_cane" => Behavior::SugarCane,
        "minecraft:vine" => Behavior::Vine,
        "minecraft:nether_wart" => Behavior::NetherWart,
        "minecraft:melon_stem" => Behavior::Stem("minecraft:supports_melon_stem_fruit"),
        "minecraft:pumpkin_stem" => Behavior::Stem("minecraft:supports_pumpkin_stem_fruit"),
        "minecraft:chorus_flower" => Behavior::ChorusFlower,
        "minecraft:chorus_plant" => Behavior::ChorusPlant,
        "minecraft:lily_pad" => spread(4, 3, Soil::LilyPad),
        "minecraft:dead_bush" => spread(4, 2, Soil::Tag("minecraft:supports_dry_vegetation")),
        "minecraft:mycelium" => Behavior::Mycelium,
        "minecraft:podzol" => Behavior::Podzol,
        "minecraft:dirt_path" => Behavior::Dirt,
        "minecraft:brain_coral" => Behavior::Coral("universalbonemeal:brain_coral"),
        "minecraft:bubble_coral" => Behavior::Coral("universalbonemeal:bubble_coral"),
        "minecraft:fire_coral" => Behavior::Coral("universalbonemeal:fire_coral"),
        "minecraft:horn_coral" => Behavior::Coral("universalbonemeal:horn_coral"),
        "minecraft:tube_coral" => Behavior::Coral("universalbonemeal:tube_coral"),
        "minecraft:spore_blossom" => Behavior::SporeBlossom,
        _ if in_tag("minecraft:dirt") => Behavior::Dirt,
        _ if in_tag("minecraft:small_flowers") => {
            let soil = if id == "minecraft:wither_rose" {
                "minecraft:supports_wither_rose"
            } else {
                "minecraft:supports_vegetation"
            };
            spread(3, 1, Soil::Tag(soil))
        }
        _ => return None,
    };
    Some(entry)
}

/// `GrowingPlantTraverser.getMaxHeightAtPosition` with `uniform(12, 16)`: the column's height
/// limit, the same at a position in every world (`WorldgenRandom.seedSlimeChunk(x, z, 0,
/// 987234911)`).
fn max_height(x: i32, z: i32) -> i32 {
    let seed = (x.wrapping_mul(x).wrapping_mul(4_987_142) as i64)
        .wrapping_add(x.wrapping_mul(5_947_611) as i64)
        .wrapping_add((z.wrapping_mul(z) as i64).wrapping_mul(4_392_871))
        .wrapping_add(z.wrapping_mul(389_711) as i64)
        ^ 987_234_911;
    12 + LegacyRandomSource::new(seed).next_int_bound(5)
}

/// `NetherVines.getBlocksToGrowWhenBonemealed`.
fn vine_count(r: &mut dyn RandomSource) -> i32 {
    let mut chance = 1.0;
    let mut count = 0;
    while r.next_double() < chance {
        chance *= 0.826;
        count += 1;
    }
    count
}

/// One step of a random walk: `(nextInt(3) - 1, (nextInt(3) - 1) * nextInt(3) / 2, nextInt(3) - 1)`.
fn step(r: &mut dyn RandomSource) -> [i32; 3] {
    let x = r.next_int_bound(3) - 1;
    let y = r.next_int_bound(3) - 1;
    let y = y * r.next_int_bound(3) / 2;
    [x, y, r.next_int_bound(3) - 1]
}

/// `podzol.json`'s weighted `ground_state` for a draw of `nextInt(147)`.
fn podzol_ground(selection: i32) -> &'static str {
    let mut left = selection;
    for (state, weight) in PODZOL_GROUND {
        if left < weight {
            return state;
        }
        left -= weight;
    }
    PODZOL_GROUND[PODZOL_GROUND.len() - 1].0
}

/// `Block.isShapeFullBlock`: the boxes fill the unit cube exactly, nothing outside it.
fn is_full_block(boxes: &[[f64; 6]]) -> bool {
    if boxes
        .iter()
        .any(|b| b[0] < 0.0 || b[1] < 0.0 || b[2] < 0.0 || b[3] > 1.0 || b[4] > 1.0 || b[5] > 1.0)
    {
        return false;
    }
    let edges = |axis: usize| {
        let mut at: Vec<f64> = [0.0, 1.0]
            .into_iter()
            .chain(boxes.iter().flat_map(|b| [b[axis], b[axis + 3]]))
            .collect();
        at.sort_by(f64::total_cmp);
        at.dedup();
        at
    };
    let (xs, ys, zs) = (edges(0), edges(1), edges(2));
    let mid = |at: &[f64]| {
        at.windows(2)
            .map(|w| (w[0] + w[1]) / 2.0)
            .collect::<Vec<_>>()
    };
    let (xs, ys, zs) = (mid(&xs), mid(&ys), mid(&zs));
    xs.iter().all(|&x| {
        ys.iter().all(|&y| {
            zs.iter().all(|&z| {
                boxes.iter().any(|b| {
                    b[0] <= x && x <= b[3] && b[1] <= y && y <= b[4] && b[2] <= z && z <= b[5]
                })
            })
        })
    })
}

/// The order a Java `HashSet<BlockPos>` holding `positions` (added in this order) iterates them:
/// by bucket of `Vec3i.hashCode`, and within a bucket in the order they were added.
fn hash_set_order(positions: Vec<[i32; 3]>) -> Vec<[i32; 3]> {
    let mut unique: Vec<[i32; 3]> = Vec::with_capacity(positions.len());
    for pos in positions {
        if !unique.contains(&pos) {
            unique.push(pos);
        }
    }
    let mut capacity = 16_usize;
    while unique.len() > capacity / 4 * 3 {
        capacity *= 2;
    }
    let bucket = |[x, y, z]: [i32; 3]| {
        let hash = y
            .wrapping_add(z.wrapping_mul(31))
            .wrapping_mul(31)
            .wrapping_add(x) as u32;
        (hash ^ (hash >> 16)) as usize & (capacity - 1)
    };
    unique.sort_by_key(|&pos| bucket(pos));
    unique
}

fn offset(pos: [i32; 3], by: [i32; 3]) -> [i32; 3] {
    [pos[0] + by[0], pos[1] + by[1], pos[2] + by[2]]
}

fn above(pos: [i32; 3], dy: i32) -> [i32; 3] {
    [pos[0], pos[1] + dy, pos[2]]
}

/// Reads of one level, as the original's predicates make them.
#[derive(Clone, Copy)]
struct Reads<'a> {
    host: &'a dyn Host,
    level: LevelRef,
    client: bool,
}

impl<'a> Reads<'a> {
    fn server(host: &'a dyn Host, level: LevelRef) -> Self {
        Reads {
            host,
            level,
            client: false,
        }
    }

    fn state(self, pos: [i32; 3]) -> Option<BlockState> {
        self.level.block_state(self.host, pos)
    }

    fn id(self, state: BlockState) -> &'a str {
        state.block().id(self.host).unwrap_or("")
    }

    fn tagged(self, state: BlockState, tag: &str) -> bool {
        tag_key(tag).is_some_and(|key| state.block().is_in_tag(self.host.tags(), key) == Some(true))
    }

    /// `BlockState.isAir` of a state: air, cave air and void air.
    fn state_is_air(self, state: BlockState) -> bool {
        self.host.block_states().is_air(state) == Some(true)
    }

    /// `BlockState.isAir` of the block at `pos`.
    fn is_air(self, pos: [i32; 3]) -> bool {
        self.state(pos)
            .is_some_and(|state| self.state_is_air(state))
    }

    fn property(self, state: BlockState, name: &str) -> Option<String> {
        let states = self.host.block_states();
        state
            .property(&states, name)
            .ok()
            .flatten()
            .map(str::to_owned)
    }

    fn int(self, state: BlockState, name: &str) -> Option<i32> {
        self.property(state, name)?.parse().ok()
    }

    /// `LazyBlockProperty.getMaxValue`: the property's highest value.
    fn max(self, state: BlockState, name: &str) -> Option<i32> {
        let states = self.host.block_states();
        let properties = states.definition(state.block())?;
        let property = properties.iter().find(|property| property.name == name)?;
        property
            .values
            .iter()
            .filter_map(|value| value.parse().ok())
            .max()
    }

    fn shape(self, pos: [i32; 3], kind: BlockShapeKind) -> Option<Vec<[f64; 6]>> {
        let state = self.state(pos)?;
        let mut boxes = Vec::new();
        let _ = self.level
            .visit_block_shape(self.host, state, pos, kind, &mut |b| {
                boxes.push(b);
                ControlFlow::Continue(())
            })?;
        Some(boxes)
    }

    /// `BlockState.isCollisionShapeFullBlock`.
    fn full_collision(self, pos: [i32; 3]) -> bool {
        self.shape(pos, BlockShapeKind::Collision)
            .is_some_and(|boxes| is_full_block(&boxes))
    }

    fn fluid_empty(self, pos: [i32; 3]) -> bool {
        self.level
            .fluid_state(self.host, pos)
            .is_none_or(|fluid| fluid.is_empty())
    }

    fn fluid_in_tag(self, pos: [i32; 3], tag: &str) -> bool {
        self.level
            .fluid_state(self.host, pos)
            .filter(|fluid| !fluid.is_empty())
            .zip(tag_key(tag))
            .is_some_and(|(fluid, key)| fluid.fluid.is_in_tag(self.host.tags(), key) == Some(true))
    }

    /// `BlockState.propagatesSkylightDown`, the block's own answer.
    fn propagates_skylight_down(self, pos: [i32; 3]) -> bool {
        self.state(pos).is_some_and(|state| {
            self.host.block_states().propagates_skylight_down(state) == Some(true)
        })
    }

    /// `GrowingPlantTraverser.getTopConnectedBlock`: the last block of `block` going `dy` from
    /// `pos`.
    fn column_end(self, pos: [i32; 3], dy: i32, block: BlockKey) -> [i32; 3] {
        let mut at = pos;
        while self.state(above(at, dy)).map(BlockState::block) == Some(block) {
            at = above(at, dy);
        }
        at
    }

    /// `canSurvive` of a copied plant at `pos` (sunrise-org/sunrise#3596).
    fn survives(self, soil: Soil, pos: [i32; 3]) -> bool {
        let below = above(pos, -1);
        let Some(ground) = self.state(below) else {
            return false;
        };
        match soil {
            Soil::LilyPad => {
                (self.fluid_in_tag(below, "minecraft:supports_lily_pad")
                    || self.tagged(ground, "minecraft:supports_lily_pad"))
                    && self.fluid_empty(pos)
            }
            Soil::Tag(tag) => self.tagged(ground, tag),
        }
    }

    /// `MushroomBlock.canSurvive`.
    fn mushroom_survives(self, pos: [i32; 3]) -> bool {
        let Some(ground) = self.state(above(pos, -1)) else {
            return false;
        };
        if self.tagged(ground, "minecraft:overrides_mushroom_light_requirement") {
            return true;
        }
        let light = |layer| self.level.brightness(self.host, layer, pos).unwrap_or(0);
        light(LightLayer::Block).max(light(LightLayer::Sky)) < 13
            && self.host.block_states().is_solid_render(ground) == Some(true)
    }

    /// `SpreadSourcesCollector.findSpreadSources` for grass and mycelium one block around `pos`:
    /// each block found once, in the order `BlockPos.betweenClosed` meets them.
    fn spread_sources(self, pos: [i32; 3]) -> Vec<BlockKey> {
        let mut found = Vec::new();
        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let Some(state) = self.state(offset(pos, [dx, dy, dz])) else {
                        continue;
                    };
                    let source = matches!(
                        self.id(state),
                        "minecraft:grass_block" | "minecraft:mycelium"
                    );
                    if source && !found.contains(&state.block()) {
                        found.push(state.block());
                    }
                }
            }
        }
        found
    }

    /// `ChorusPlantTraverser.getFlowerPositions`, in the order the original visits them.
    fn chorus_flowers(self, start: [i32; 3]) -> Vec<[i32; 3]> {
        let mut found = Vec::new();
        let mut seen = HashMap::new();
        self.chorus_visit(start, 0, CHORUS_SEARCH, &mut seen, &mut found);
        hash_set_order(found)
    }

    /// One step of the original's walk up a chorus plant. A plant reached again from the same
    /// side with no more depth left finds nothing new, so it is not walked again: a closed loop
    /// of plants would otherwise be walked once per path through it.
    fn chorus_visit(
        self,
        pos: [i32; 3],
        from: usize,
        depth: i32,
        seen: &mut HashMap<([i32; 3], usize), i32>,
        found: &mut Vec<[i32; 3]>,
    ) {
        let Some(state) = self.state(pos) else {
            return;
        };
        match self.id(state) {
            "minecraft:chorus_plant" if depth > 0 => {
                if seen.get(&(pos, from)).is_some_and(|&left| left >= depth) {
                    return;
                }
                seen.insert((pos, from), depth);
                for (index, (by, face)) in DIRECTIONS.iter().enumerate() {
                    if index == 0 || index == from {
                        continue;
                    }
                    if self.property(state, face).as_deref() == Some("true") {
                        self.chorus_visit(offset(pos, *by), index ^ 1, depth - 1, seen, found);
                    }
                }
            }
            "minecraft:chorus_flower" => found.push(pos),
            _ => {}
        }
    }

    fn coral_biome(self, pos: [i32; 3]) -> bool {
        let biome = if self.client {
            self.level.client_biome(self.host, pos)
        } else {
            self.level.biome_at(self.host, pos)
        };
        biome.is_some_and(|biome| {
            self.level
                .biome_in_tag(self.host, &biome, "minecraft:produces_corals_from_bonemeal")
                == Some(true)
        })
    }
}

/// The entry's predicate (`when`), or true where it has none.
fn valid(w: Reads<'_>, entry: Behavior, pos: [i32; 3], state: BlockState) -> bool {
    match entry {
        Behavior::Cactus | Behavior::SugarCane => {
            let top = w.column_end(pos, 1, state.block());
            let bottom = w.column_end(pos, -1, state.block());
            (top[1] - bottom[1]).abs() < max_height(pos[0], pos[2]) && w.is_air(above(top, 1))
        }
        Behavior::Vine => {
            let top = w.column_end(pos, 1, state.block());
            let bottom = w.column_end(pos, -1, state.block());
            (top[1] - bottom[1]).abs() < VINE_MAX_HEIGHT
                && w.is_air(above(bottom, -1))
                && HORIZONTAL
                    .iter()
                    .any(|(_, face)| w.property(state, face).as_deref() == Some("true"))
        }
        Behavior::NetherWart | Behavior::ChorusFlower => {
            matches!((w.int(state, "age"), w.max(state, "age")), (Some(age), Some(max)) if age < max)
        }
        Behavior::Stem(soil) => {
            w.int(state, "age").is_some_and(|age| age >= 7)
                && HORIZONTAL.iter().any(|(by, _)| {
                    let fruit = offset(pos, *by);
                    w.is_air(fruit)
                        && w.state(above(fruit, -1))
                            .is_some_and(|ground| w.tagged(ground, soil))
                })
        }
        Behavior::ChorusPlant => w.chorus_flowers(pos).into_iter().any(|flower| {
            w.state(flower)
                .filter(|state| w.id(*state) == "minecraft:chorus_flower")
                .and_then(|state| w.int(state, "age"))
                .is_some_and(|age| age < 5)
        }),
        Behavior::Spread { .. } | Behavior::SporeBlossom => true,
        Behavior::Mycelium | Behavior::Podzol => w.is_air(above(pos, 1)),
        Behavior::Dirt => {
            w.propagates_skylight_down(above(pos, 1)) && !w.spread_sources(pos).is_empty()
        }
        Behavior::Coral(_) => w.coral_biome(pos),
    }
}

/// What bone meal does to the block in the event.
enum Decision {
    /// The block is fertilizer resistant: nothing happens and no meal is used.
    Resist,
    /// The entry applies and its predicate holds.
    Grow(Behavior),
    /// Vanilla and other mods decide.
    Pass,
}

fn decide(w: Reads<'_>, event: &BonemealEvent<'_>, pos: [i32; 3]) -> Decision {
    let state = event.state;
    if w.tagged(state, RESISTANT) {
        return Decision::Resist;
    }
    let id = state.block().id(w.host).unwrap_or("");
    let Some(entry) = behavior(id, |tag| w.tagged(state, tag)) else {
        return Decision::Pass;
    };
    if bonemealable(w.host, event) && !entry.replaces() {
        return Decision::Pass;
    }
    if valid(w, entry, pos, state) {
        Decision::Grow(entry)
    } else {
        Decision::Pass
    }
}

/// `state.getBlock() instanceof BonemealableBlock`: the block's class, or its own answer that it
/// is a valid target here.
fn bonemealable(host: &dyn Host, event: &BonemealEvent<'_>) -> bool {
    event.valid_bonemeal_target
        || matches!(
            host.block_states()
                .block_class(event.state, BlockClass::Bonemealable),
            Ok(Some(true))
        )
}

fn set(host: &mut dyn ServerHost, level: LevelRef, pos: [i32; 3], state: BlockState, flags: i32) {
    let _ = level.set_block(host, pos, state, BlockUpdateFlags::from_bits_retain(flags));
}

/// `BoneMealBehavior.isBonemealSuccess`.
fn succeeds(host: &mut dyn ServerHost, level: LevelRef, entry: Behavior) -> bool {
    match entry {
        Behavior::Coral(_) => level
            .with_random(host, |r| r.next_float() < 0.4)
            .unwrap_or(false),
        _ => true,
    }
}

/// `BoneMealBehavior.performBonemeal`.
fn perform(
    host: &mut dyn ServerHost,
    level: LevelRef,
    entry: Behavior,
    pos: [i32; 3],
    state: BlockState,
) {
    let min_y = level
        .dimension_shape(&*host)
        .map_or(i32::MIN, |shape| shape.min_y);
    match entry {
        Behavior::Cactus | Behavior::SugarCane => {
            let block = state.block();
            let head = Reads::server(&*host, level).column_end(pos, 1, block);
            let Some(count) = level.with_random(host, |r| 1 + r.next_int_bound(2)) else {
                return;
            };
            let mut at = above(head, 1);
            for _ in 0..count {
                if !Reads::server(&*host, level).is_air(at) {
                    break;
                }
                let grown = if entry == Behavior::Cactus {
                    let flower = level.with_random(host, |r| r.next_int_bound(10) >= 9);
                    match flower {
                        Some(true) => "minecraft:cactus_flower",
                        Some(false) => "minecraft:cactus",
                        None => break,
                    }
                } else {
                    "minecraft:sugar_cane[age=0]"
                };
                let Some(grown) = host.block_state_id(grown) else {
                    break;
                };
                set(host, level, at, grown, UPDATE);
                if grown.block() != block {
                    break;
                }
                at = above(at, 1);
            }
            let head_state = Reads::server(&*host, level).state(head);
            if let Some(young) = head_state.and_then(|head_state| {
                let states = host.block_states();
                head_state.with(&states, "age", "0").ok()
            }) {
                set(host, level, head, young, UPDATE);
            }
        }
        Behavior::Vine => {
            let block = state.block();
            let bottom = Reads::server(&*host, level).column_end(pos, -1, block);
            let Some(count) = level.with_random(host, |r| vine_count(r).min(5)) else {
                return;
            };
            let mut at = above(bottom, -1);
            for _ in 0..count {
                if !Reads::server(&*host, level).is_air(at) {
                    break;
                }
                let Some(source) = Reads::server(&*host, level).state(above(at, 1)) else {
                    break;
                };
                let states = host.block_states();
                let grown = source.with(&states, "up", "false").unwrap_or(source);
                set(host, level, at, grown, VINE_FLAGS);
                if grown.block() != block {
                    break;
                }
                at = above(at, -1);
            }
        }
        Behavior::NetherWart => {
            let w = Reads::server(&*host, level);
            let (Some(age), Some(max)) = (w.int(state, "age"), w.max(state, "age")) else {
                return;
            };
            let Some(more) = level.with_random(host, |r| i32::from(r.next_int_bound(4) >= 1))
            else {
                return;
            };
            let states = host.block_states();
            if let Ok(grown) = state.with(&states, "age", &(age + more).min(max).to_string()) {
                set(host, level, pos, grown, CLIENTS);
            }
        }
        Behavior::Stem(_) | Behavior::ChorusFlower => {
            let attempts = if entry == Behavior::ChorusFlower {
                Some(1)
            } else {
                level.with_random(host, |r| 2 + r.next_int_bound(4))
            };
            for _ in 0..attempts.unwrap_or(0) {
                if Reads::server(&*host, level).state(pos) != Some(state) {
                    break;
                }
                level.block_random_tick(host, pos);
            }
        }
        Behavior::ChorusPlant => {
            for flower in Reads::server(&*host, level).chorus_flowers(pos) {
                level.block_random_tick(host, flower);
            }
        }
        Behavior::Spread { width, most, soil } => {
            let mut successes = 0;
            for spread in (0..(width + 1) * 16).rev() {
                if spread_once(host, level, pos, spread, soil, min_y) {
                    successes += 1;
                    if successes >= most {
                        return;
                    }
                }
            }
        }
        Behavior::Mycelium => {
            let Some(shape) = level.dimension_shape(&*host) else {
                return;
            };
            let max_y = shape.min_y + shape.height - 1;
            let origin = above(pos, 1);
            if origin[1] < min_y + 1 || origin[1] + 1 >= max_y {
                return;
            }
            for _ in 0..9 {
                let Some((by, red)) = level.with_random(host, |r| {
                    let x = r.next_int_bound(3) - r.next_int_bound(3);
                    let y = r.next_int_bound(1) - r.next_int_bound(1);
                    let z = r.next_int_bound(3) - r.next_int_bound(3);
                    ([x, y, z], r.next_int_bound(2) < 1)
                }) else {
                    return;
                };
                let at = offset(origin, by);
                let w = Reads::server(&*host, level);
                if w.is_air(at) && at[1] > min_y && w.mushroom_survives(at) {
                    let id = if red {
                        "minecraft:red_mushroom"
                    } else {
                        "minecraft:brown_mushroom"
                    };
                    if let Some(mushroom) = host.block_state_id(id) {
                        set(host, level, at, mushroom, CLIENTS);
                    }
                }
            }
        }
        Behavior::Podzol => {
            for attempt in 0..128 {
                let Some(at) = podzol_walk(host, level, pos, attempt / 16) else {
                    continue;
                };
                let w = Reads::server(&*host, level);
                let Some(found) = w.state(at) else {
                    continue;
                };
                if w.id(found) == "minecraft:fern"
                    && level.with_random(host, |r| r.next_int_bound(10) == 0) == Some(true)
                {
                    let lower = host.block_state_id("minecraft:large_fern[half=lower]");
                    let upper = host.block_state_id("minecraft:large_fern[half=upper]");
                    if let (Some(lower), Some(upper)) = (lower, upper) {
                        set(host, level, at, lower, CLIENTS);
                        set(host, level, above(at, 1), upper, CLIENTS);
                    }
                }
                if Reads::server(&*host, level).state_is_air(found)
                    && level.with_random(host, |r| r.next_int_bound(5) == 0) == Some(true)
                    && Reads::server(&*host, level).is_air(at)
                    && at[1] > min_y
                {
                    let Some(pick) = level.with_random(host, |r| r.next_int_bound(147)) else {
                        return;
                    };
                    if let Some(ground) = host.block_state_id(podzol_ground(pick)) {
                        set(host, level, at, ground, CLIENTS);
                    }
                }
            }
        }
        Behavior::Dirt => {
            let sources = Reads::server(&*host, level).spread_sources(pos);
            let Ok(len) = i32::try_from(sources.len()) else {
                return;
            };
            if len == 0 {
                return;
            }
            let Some(index) = level.with_random(host, |r| r.next_int_bound(len)) else {
                return;
            };
            let Some(grown) = usize::try_from(index)
                .ok()
                .and_then(|index| sources.get(index))
                .and_then(|block| host.block_state_default(*block))
            else {
                return;
            };
            set(host, level, pos, grown, UPDATE);
        }
        Behavior::Coral(feature) => {
            level.remove_block(host, pos, false);
            if level.place_placed_feature(host, feature, pos) != Ok(true) {
                set(host, level, pos, state, UPDATE);
            }
        }
        Behavior::SporeBlossom => {
            let center = [
                f64::from(pos[0]) + 0.5,
                f64::from(pos[1]) + 0.5,
                f64::from(pos[2]) + 0.5,
            ];
            let table = "universalbonemeal:grow/spore_blossom";
            if let Ok(Some(drops)) = level.roll_loot(host, table, center, None, None) {
                for drop in drops {
                    let _ = level.pop_resource(host, pos, drop);
                }
            }
        }
    }
}

/// `RandomNeighborSpreadBehavior.placeBlockAtOffset`: walk `steps / 16` steps from `origin`,
/// each to a place the copied block survives and with no full block, then place it there.
fn spread_once(
    host: &mut dyn ServerHost,
    level: LevelRef,
    origin: [i32; 3],
    steps: i32,
    soil: Soil,
    min_y: i32,
) -> bool {
    let mut at = origin;
    let mut copy = None;
    for _ in 0..steps / 16 {
        let Some(by) = level.with_random(host, step) else {
            return false;
        };
        at = offset(at, by);
        let w = Reads::server(&*host, level);
        copy = w.state(origin);
        if copy.is_none() || !w.survives(soil, at) || w.full_collision(at) {
            return false;
        }
    }
    let w = Reads::server(&*host, level);
    match copy {
        Some(copy) if w.is_air(at) && at[1] > min_y => {
            set(host, level, at, copy, CLIENTS);
            true
        }
        _ => false,
    }
}

/// `VegetationPatchBehavior.getOffsetPos`: walk `steps` steps from above `pos`, each onto podzol
/// and into no full block.
fn podzol_walk(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    steps: i32,
) -> Option<[i32; 3]> {
    let mut at = above(pos, 1);
    for _ in 0..steps {
        at = offset(at, level.with_random(host, step)?);
        let w = Reads::server(&*host, level);
        let on_podzol = w
            .state(above(at, -1))
            .is_some_and(|ground| w.id(ground) == "minecraft:podzol");
        if !on_podzol || w.full_collision(at) {
            return None;
        }
    }
    Some(at)
}

/// `UseBoneMealHandler.shrinkOrHurtItem`: a damageable item is worn by one as
/// `ItemStack.hurtAndBreak(1, level, null)` does, Unbreaking included; anything else is used up
/// one at a time.
fn use_meal(host: &mut dyn ServerHost, level: LevelRef, stack: &mut Option<ItemStack>) {
    let Some(meal) = stack.take() else {
        return;
    };
    if ItemResource::of(&meal).damageable(&*host) {
        *stack = match meal.hurt_and_break(host, level, 1, None) {
            Ok(worn) => worn,
            Err(_) => Some(meal),
        };
    } else {
        *stack = meal.shrink(1);
    }
}

fn serve(host: &mut dyn ServerHost, event: &mut BonemealEvent<'_>, pos: [i32; 3]) -> Flow {
    let Some(level) = host.level(event.dimension.key().id()) else {
        return Flow::Continue;
    };
    let reads = Reads::server(&*host, level);
    match decide(reads, event, pos) {
        Decision::Pass => Flow::Continue,
        Decision::Resist => {
            event.successful = false;
            event.cancelled = true;
            Flow::Cancel
        }
        Decision::Grow(entry) => {
            let bonemealable_block = bonemealable(&*host, event);
            if succeeds(host, level, entry) {
                perform(host, level, entry, pos, event.state);
            }
            use_meal(host, level, event.stack);
            if !bonemealable_block && let Some(growth) = GROWTH.get() {
                let at = [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])];
                let _ = growth.send_near(host, level, at, PARTICLE_RANGE, None, &Growth { pos });
            }
            event.successful = true;
            event.cancelled = true;
            Flow::Cancel
        }
    }
}

fn predict(host: &dyn Host, event: &mut BonemealEvent<'_>, pos: [i32; 3]) -> Flow {
    let Some(level) = host.level(event.dimension.key().id()) else {
        return Flow::Continue;
    };
    let reads = Reads {
        host,
        level,
        client: true,
    };
    match decide(reads, event, pos) {
        Decision::Pass => Flow::Continue,
        Decision::Resist => {
            event.successful = false;
            event.cancelled = true;
            Flow::Cancel
        }
        Decision::Grow(_) => {
            event.successful = true;
            event.cancelled = true;
            Flow::Cancel
        }
    }
}

/// The client's own random for where particles go, as `level.getRandom()` on a client.
fn particle_random() -> &'static Mutex<LegacyRandomSource> {
    static RANDOM: OnceLock<Mutex<LegacyRandomSource>> = OnceLock::new();
    RANDOM.get_or_init(|| {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |time| time.as_nanos() as i64);
        Mutex::new(LegacyRandomSource::new(seed))
    })
}

/// `ClientboundGrowthParticlesMessage`'s listener: `ParticleUtils.spawnParticles` over a solid
/// block, otherwise `ParticleUtils.spawnParticleInBlock`.
fn on_growth(growth: Growth, cx: &mut dyn IPayloadContext) -> Result<(), String> {
    let player = cx.player().map_err(|e| format!("{e:?}"))?;
    let host = cx.client_host().map_err(|e| format!("{e:?}"))?;
    let Some(key) = HAPPY_VILLAGER.get() else {
        return Ok(());
    };
    let Some(level) = player.entity().level(&*host) else {
        return Ok(());
    };
    let w = Reads {
        host: &*host,
        level,
        client: true,
    };
    let pos = growth.pos;
    let Some(state) = w.state(pos) else {
        return Ok(());
    };
    let (origin, count, width, height, floating) =
        if host.block_states().is_solid_render(state) == Some(true) {
            (above(pos, 1), 45, 3.0, 1.0, false)
        } else if w.state_is_air(state) {
            (pos, 15, 0.5, 1.0, true)
        } else {
            let Some(top) = w
                .shape(pos, BlockShapeKind::Outline)
                .and_then(|boxes| boxes.iter().map(|b| b[4]).max_by(f64::total_cmp))
            else {
                return Ok(());
            };
            (pos, 15, 0.5, top, true)
        };
    let mut particles = Vec::with_capacity(count);
    {
        let mut random = particle_random()
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        for _ in 0..count {
            let velocity = [
                random.next_gaussian() * 0.02,
                random.next_gaussian() * 0.02,
                random.next_gaussian() * 0.02,
            ];
            let start = 0.5 - width;
            let at = [
                f64::from(origin[0]) + start + random.next_double() * width * 2.0,
                f64::from(origin[1]) + random.next_double() * height,
                f64::from(origin[2]) + start + random.next_double() * width * 2.0,
            ];
            particles.push((at, velocity));
        }
    }
    let options = ParticleOptions::simple(*key);
    for (at, velocity) in particles {
        let below = [
            at[0].floor() as i32,
            at[1].floor() as i32 - 1,
            at[2].floor() as i32,
        ];
        let w = Reads {
            host: &*host,
            level,
            client: true,
        };
        if floating || !w.is_air(below) {
            host.add_particle(&options, at, velocity);
        }
    }
    Ok(())
}

fn tag_key(id: &str) -> Option<TagKey> {
    TAGS.get()?.get(id).copied()
}

#[derive(Default)]
struct Mod;

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        let _ = TAGS.set(TAG_IDS.into_iter().map(|id| (id, r.tag_key(id))).collect());
        let _ = HAPPY_VILLAGER.set(r.particle_key("minecraft:happy_villager"));
        let growth = r
            .payload_handlers()
            .registrar("1")
            .optional()
            .play_to_client_typed::<Growth>(
                PayloadId("universalbonemeal:growth_particles".into()),
                on_growth,
            );
        let _ = GROWTH.set(growth);
    }

    fn bonemeal(&self, host: HostSide, event: &mut BonemealEvent<'_>) -> Flow {
        let pos = [event.pos.x, event.pos.y, event.pos.z];
        match host {
            HostSide::Server(host) => serve(host, event, pos),
            HostSide::Client(host) => predict(&*host, event, pos),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_map_names_blocks_before_their_tags() {
        let none = |_: &str| false;
        assert_eq!(behavior("minecraft:cactus", none), Some(Behavior::Cactus));
        assert_eq!(
            behavior("minecraft:dead_bush", |_| true),
            Some(Behavior::Spread {
                width: 4,
                most: 2,
                soil: Soil::Tag("minecraft:supports_dry_vegetation")
            })
        );
        assert_eq!(
            behavior("example:loam", |tag| tag == "minecraft:dirt"),
            Some(Behavior::Dirt)
        );
        assert_eq!(
            behavior("example:bloom", |tag| tag == "minecraft:small_flowers"),
            Some(Behavior::Spread {
                width: 3,
                most: 1,
                soil: Soil::Tag("minecraft:supports_vegetation")
            })
        );
        assert_eq!(
            behavior("minecraft:wither_rose", |tag| tag
                == "minecraft:small_flowers"),
            Some(Behavior::Spread {
                width: 3,
                most: 1,
                soil: Soil::Tag("minecraft:supports_wither_rose")
            })
        );
        assert_eq!(behavior("minecraft:stone", none), None);
        assert!(behavior("minecraft:pumpkin_stem", none).is_some_and(Behavior::replaces));
        assert!(!behavior("minecraft:cactus", none).is_some_and(Behavior::replaces));
    }

    /// Expected values from an independent Java `Random` model of
    /// `seedSlimeChunk(x, z, 0, 987234911)` and `nextInt(5) + 12`.
    #[test]
    fn column_height_limits_follow_the_slime_chunk_seed() {
        assert_eq!(max_height(0, 0), 14);
        assert_eq!(max_height(10, -7), 12);
        assert_eq!(max_height(123_456, -98_765), 13);
        assert_eq!(max_height(30_000_000, -30_000_000), 12);
    }

    #[test]
    fn a_walk_step_halves_toward_zero() {
        struct Fixed(Vec<i32>);
        impl RandomSource for Fixed {
            fn next_int(&mut self) -> i32 {
                0
            }
            fn next_int_bound(&mut self, _: i32) -> i32 {
                self.0.remove(0)
            }
            fn next_long(&mut self) -> i64 {
                0
            }
            fn next_boolean(&mut self) -> bool {
                false
            }
            fn next_float(&mut self) -> f32 {
                0.0
            }
            fn next_double(&mut self) -> f64 {
                0.0
            }
            fn next_gaussian(&mut self) -> f64 {
                0.0
            }
        }
        assert_eq!(step(&mut Fixed(vec![0, 0, 1, 2])), [-1, 0, 1]);
        assert_eq!(step(&mut Fixed(vec![1, 0, 2, 1])), [0, -1, 0]);
        assert_eq!(step(&mut Fixed(vec![2, 2, 2, 0])), [1, 1, -1]);
    }

    #[test]
    fn podzol_ground_follows_the_weights_in_order() {
        assert_eq!(podzol_ground(0), "minecraft:sweet_berry_bush[age=3]");
        assert_eq!(podzol_ground(1), "minecraft:sweet_berry_bush[age=3]");
        assert_eq!(podzol_ground(2), "minecraft:sweet_berry_bush[age=2]");
        assert_eq!(podzol_ground(13), "minecraft:sweet_berry_bush[age=1]");
        assert_eq!(podzol_ground(25), "minecraft:sweet_berry_bush");
        assert_eq!(podzol_ground(26), "minecraft:fern");
        assert_eq!(podzol_ground(145), "minecraft:fern");
        assert_eq!(podzol_ground(146), "minecraft:dead_bush");
    }

    #[test]
    fn only_boxes_filling_the_cube_are_a_full_block() {
        assert!(is_full_block(&[[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]]));
        assert!(is_full_block(&[
            [0.0, 0.0, 0.0, 1.0, 0.5, 1.0],
            [0.0, 0.5, 0.0, 1.0, 1.0, 1.0]
        ]));
        assert!(!is_full_block(&[]));
        assert!(!is_full_block(&[[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]]));
        // Two overlapping halves whose volumes add to one still leave a gap.
        assert!(!is_full_block(&[
            [0.0, 0.0, 0.0, 1.0, 0.75, 1.0],
            [0.0, 0.0, 0.0, 1.0, 0.25, 1.0]
        ]));
        // A fence's collision reaches above the block.
        assert!(!is_full_block(&[[0.0, 0.0, 0.0, 1.0, 1.5, 1.0]]));
    }

    #[test]
    fn flowers_come_back_in_java_hash_set_order() {
        // Hashes 1, 0 and 31: buckets 1, 0 and 15 of 16; a repeat keeps its first place.
        let order = hash_set_order(vec![[1, 0, 0], [0, 1, 0], [0, 0, 0], [1, 0, 0]]);
        assert_eq!(order, vec![[0, 0, 0], [1, 0, 0], [0, 1, 0]]);
    }
}
