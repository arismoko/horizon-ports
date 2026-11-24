//! YUNG's Bridges: stone and wooden bridges across rivers.
//!
//! The original adds one feature to river biomes (`bridge_list`, a biome modifier in data): it
//! tries its 22 bridges in random order, and each looks for a crossing in the chunk
//! (BridgePlacement: solid banks open to the sky at both ends and water between), passes a
//! rarity roll, reseeds the random from the position (RngInitializerPlacement), places its
//! template at sea level (BridgeFeature) and runs twelve processors over it: legs down to the
//! river bed, stone and wood variation, broken parts and lanterns. Here that is one feature type,
//! `yungsbridges:bridge_list`; the templates are decoded into `bridges.rs` by
//! `tools/gen_bridges.py`.
mod bridges;
mod random;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{PoisonError, RwLock};

use bridges::{BRIDGES, TEMPLATES};
use mod_api::{
    self, BiomeKey, BlockKey, BlockState, BlockUpdateFlags, FeatureContext, FeatureError,
    FeatureType, HeightmapType, Hooks, LevelLoadEvent, LevelRef, Registrar, RegistrationContext,
    ReloadContext, ServerHost, ServerTickEvent, TagsUpdateCause, TagsUpdatedEvent,
};
use random::{Xoroshiro, position_seed};

/// A block as a template names it: id and properties.
type State = (&'static str, Props);

/// A block's properties, as a template names them.
type Props = &'static [(&'static str, &'static str)];

/// One entry of bridge_list: BridgePlacement's numbers, the axis and the rarity.
pub(crate) struct Bridge {
    template: usize,
    length: i32,
    width: i32,
    min_water: i32,
    max_water: i32,
    width_offset: i32,
    solid_needed: i32,
    z_axis: bool,
    rarity: i32,
}

/// A structure template: its palette unrotated and rotated counterclockwise, and its blocks.
pub(crate) struct Template {
    #[allow(dead_code)]
    id: &'static str,
    #[allow(dead_code)]
    size: [i32; 3],
    palette: &'static [State],
    rotated: &'static [State],
    blocks: &'static [([i32; 3], usize)],
}

/// Level.getSeaLevel() of the overworld, where the rivers are; a feature can't read it.
const SEA_LEVEL: i32 = 63;

/// The wood of the biome variants (ITemplateFeatureProcessor's get<Kind>BiomeVariant), by the
/// biome at the bridge's corner, the first of the original's collection tags it is in; oak
/// otherwise.
const WOODS: [(&str, &str); 4] = [
    ("yungsbridges:collections/badlands", "dark_oak"),
    ("yungsbridges:collections/taiga", "spruce"),
    ("yungsbridges:collections/jungle", "jungle"),
    ("yungsbridges:collections/savanna", "acacia"),
];

/// The wood of each biome that isn't oak's, by the world's current biome keys and tags. A feature
/// runs without a host, so the tags are read here when a level opens and after `/reload`.
static BIOME_WOOD: RwLock<Option<HashMap<BiomeKey, &'static str>>> = RwLock::new(None);

/// `/reload` rebuilt the tags and the biome keys; the next server tick reads them again.
static STALE: AtomicBool = AtomicBool::new(false);

fn read_biome_woods(host: &dyn ServerHost, level: LevelRef) {
    let mut woods = HashMap::new();
    for id in host
        .registry_entries("minecraft:worldgen/biome")
        .unwrap_or_default()
    {
        let id = id.to_string();
        let wood = WOODS
            .iter()
            .find(|(tag, _)| level.biome_in_tag(host, &id, tag) == Some(true));
        if let (Some((_, wood)), Some(key)) = (wood, host.biome_key(&id)) {
            woods.insert(key, *wood);
        }
    }
    *BIOME_WOOD.write().unwrap_or_else(PoisonError::into_inner) = Some(woods);
}

fn wood_of(biome: Option<BiomeKey>) -> &'static str {
    let woods = BIOME_WOOD.read().unwrap_or_else(PoisonError::into_inner);
    biome
        .and_then(|key| woods.as_ref()?.get(&key).copied())
        .unwrap_or("oak")
}

/// Blocks that are not air, not in `#minecraft:leaves` or `#minecraft:replaceable`, and still
/// don't occlude (BlockState.canOcclude false): the ones a river bank can hold.
const SEE_THROUGH: &[&str] = &[
    "minecraft:water",
    "minecraft:lava",
    "minecraft:bubble_column",
    "minecraft:ice",
    "minecraft:frosted_ice",
    "minecraft:sugar_cane",
    "minecraft:kelp",
    "minecraft:kelp_plant",
    "minecraft:seagrass",
    "minecraft:tall_seagrass",
    "minecraft:lily_pad",
    "minecraft:snow",
    "minecraft:dirt_path",
    "minecraft:farmland",
    "minecraft:mud",
    "minecraft:soul_sand",
    "minecraft:cactus",
    "minecraft:bamboo",
];

/// Every block the bridges place, resolved once at registration.
const OUTPUTS: &[&str] = &[
    "minecraft:air",
    "minecraft:stone_bricks",
    "minecraft:mossy_stone_bricks",
    "minecraft:cracked_stone_bricks",
    "minecraft:chiseled_stone_bricks",
    "minecraft:stone_brick_slab",
    "minecraft:mossy_stone_brick_slab",
    "minecraft:stone_brick_wall",
    "minecraft:mossy_stone_brick_wall",
    "minecraft:stone_brick_stairs",
    "minecraft:mossy_stone_brick_stairs",
    "minecraft:cobblestone",
    "minecraft:mossy_cobblestone",
    "minecraft:cobblestone_slab",
    "minecraft:cobblestone_stairs",
    "minecraft:cobblestone_wall",
    "minecraft:andesite",
    "minecraft:andesite_slab",
    "minecraft:andesite_stairs",
    "minecraft:polished_andesite",
    "minecraft:polished_andesite_slab",
    "minecraft:smooth_stone_slab",
    "minecraft:lantern",
    "minecraft:torch",
    "minecraft:wall_torch",
    "minecraft:oak_log",
    "minecraft:oak_planks",
    "minecraft:oak_slab",
    "minecraft:oak_stairs",
    "minecraft:oak_fence",
    "minecraft:dark_oak_log",
    "minecraft:dark_oak_planks",
    "minecraft:dark_oak_slab",
    "minecraft:dark_oak_stairs",
    "minecraft:dark_oak_fence",
    "minecraft:spruce_log",
    "minecraft:spruce_planks",
    "minecraft:spruce_slab",
    "minecraft:spruce_stairs",
    "minecraft:spruce_fence",
    "minecraft:jungle_log",
    "minecraft:jungle_planks",
    "minecraft:jungle_slab",
    "minecraft:jungle_stairs",
    "minecraft:jungle_fence",
    "minecraft:acacia_log",
    "minecraft:acacia_planks",
    "minecraft:acacia_slab",
    "minecraft:acacia_stairs",
    "minecraft:acacia_fence",
];

#[derive(Default)]
struct Mod;

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, registrar: &mut dyn Registrar, _: &RegistrationContext) {
        let mut keys = HashMap::new();
        let names = TEMPLATES
            .iter()
            .flat_map(|t| t.palette.iter().map(|(name, _)| *name))
            .chain(OUTPUTS.iter().copied());
        for name in names {
            keys.entry(name)
                .or_insert_with(|| registrar.block_key(name));
        }
        registrar.register_feature_type("bridge_list", Vec::new(), BridgeList { keys });
    }

    /// A world opened (its biome keys and tags are new): read the biome woods before its chunks.
    fn level_load(&self, host: &mut dyn ServerHost, event: &mut LevelLoadEvent<'_>) {
        if let Some(level) = host.level(event.dimension) {
            read_biome_woods(host, level);
        }
    }

    fn tags_updated(&self, _: &dyn ReloadContext, event: &mut TagsUpdatedEvent) {
        if let TagsUpdateCause::ServerDataLoad { reload: true } = event.cause {
            STALE.store(true, Ordering::Relaxed);
        }
    }

    fn server_tick(&self, host: &mut dyn ServerHost, _: &mut ServerTickEvent<'_>) {
        if STALE.swap(false, Ordering::Relaxed)
            && let Some(level) = host.level("minecraft:overworld")
        {
            read_biome_woods(host, level);
        }
    }
}

/// `yungsbridges:bridge_list`: MultipleAttemptSingleRandomFeature over the 22 bridges.
struct BridgeList {
    keys: HashMap<&'static str, BlockKey>,
}

impl FeatureType for BridgeList {
    fn place(
        &self,
        context: &mut dyn FeatureContext,
        origin: [i32; 3],
    ) -> Result<bool, FeatureError> {
        let mut left: Vec<usize> = (0..BRIDGES.len()).collect();
        while !left.is_empty() {
            let i = context.random().next_int_bound(left.len() as i32) as usize;
            let bridge = &BRIDGES[left[i]];
            if let Some(at) = crossing(context, bridge, origin)
                && context.random().next_float() < 1.0 / bridge.rarity as f32
            {
                // RngInitializerPlacement: the random reseeded from the position.
                let a = context.random().next_long() | 1;
                let b = context.random().next_long() | 1;
                let mut random = Xoroshiro::new(position_seed(at[0], at[2], a, b));
                let mut placer = Placer {
                    context,
                    keys: &self.keys,
                    random: &mut random,
                    chunk: [origin[0] >> 4, origin[2] >> 4],
                };
                placer.bridge(bridge, at)?;
                return Ok(true);
            }
            left.remove(i);
        }
        Ok(false)
    }
}

fn name(context: &dyn FeatureContext, state: BlockState) -> &str {
    context.metadata().name(state.block()).unwrap_or("")
}

/// BlockState.canOcclude, for what generation leaves at a river.
fn occludes(context: &mut dyn FeatureContext, p: [i32; 3]) -> bool {
    let Some(state) = context.block(p) else {
        return false;
    };
    if context.is_air(state) || SEE_THROUGH.contains(&name(context, state)) {
        return false;
    }
    context.block_in_tag(p, "minecraft:leaves") != Some(true)
        && context.block_in_tag(p, "minecraft:replaceable") != Some(true)
}

/// BlockState.liquid: water, lava and bubble columns.
fn liquid(context: &mut dyn FeatureContext, p: [i32; 3]) -> bool {
    context.block(p).is_some_and(|s| {
        matches!(
            name(context, s),
            "minecraft:water" | "minecraft:lava" | "minecraft:bubble_column"
        )
    })
}

/// BridgePlacement.getPositions: the first crossing in the chunk at `origin`, scanning across
/// then along the bridge's axis: banks at both ends (sea level - 1, occluding, nothing above
/// sea level), `solid_needed` such blocks across each end, and water under the span.
fn crossing(context: &mut dyn FeatureContext, b: &Bridge, origin: [i32; 3]) -> Option<[i32; 3]> {
    let sea = SEA_LEVEL - 1;
    let along = |p: [i32; 3], minor: i32, major: i32| {
        if b.z_axis {
            [p[0] + minor, sea, p[2] + major]
        } else {
            [p[0] + major, sea, p[2] + minor]
        }
    };
    let low = |context: &mut dyn FeatureContext, p: [i32; 3]| {
        context
            .height(HeightmapType::WorldSurface, p[0], p[2])
            .is_some_and(|h| h <= sea + 1)
    };
    let bank =
        |context: &mut dyn FeatureContext, p: [i32; 3]| occludes(context, p) && low(context, p);
    let solid_run = |context: &mut dyn FeatureContext, end: [i32; 3]| {
        let mut n = 1;
        for direction in [-1, 1] {
            for d in 1..=b.width / 2 {
                if !bank(context, along(end, direction * d, 0)) {
                    break;
                }
                n += 1;
            }
        }
        n
    };
    let half = b.width / 2;
    for minor in (half + b.width_offset + 1)..(16 - half - b.width_offset) {
        for major in 0..16 {
            let start = along(origin, minor, major);
            let end = along(start, 0, b.length + 1);
            if !(occludes(context, start)
                && occludes(context, end)
                && low(context, start)
                && low(context, end))
            {
                continue;
            }
            if solid_run(context, start) < b.solid_needed
                || solid_run(context, end) < b.solid_needed
            {
                continue;
            }
            let water = (-half..=half)
                .all(|m| (b.min_water..=b.max_water).all(|w| liquid(context, along(start, m, w))));
            if water {
                return Some(if b.z_axis {
                    [start[0] - half - b.width_offset, sea, start[2] + 1]
                } else {
                    [start[0] + 1, sea, start[2] + half + b.width_offset]
                });
            }
        }
    }
    None
}

/// Placing one bridge: the template, then the processors.
struct Placer<'a> {
    context: &'a mut dyn FeatureContext,
    keys: &'a HashMap<&'static str, BlockKey>,
    random: &'a mut Xoroshiro,
    /// The chunk being decorated: writes go to it and its neighbours only, the region vanilla's
    /// decoration may write (WorldGenRegion drops the rest).
    chunk: [i32; 2],
}

impl Placer<'_> {
    fn state(&self, block: &str, props: &[(&str, &str)]) -> Result<BlockState, FeatureError> {
        let key = self
            .keys
            .get(block)
            .copied()
            .ok_or_else(|| FeatureError(format!("{block} was not resolved")))?;
        let metadata = self.context.metadata();
        let mut state = self.context.state(key);
        for (k, v) in props {
            // A property the block doesn't have is left out, as the copy helpers check
            // hasProperty.
            if let Ok(next) = state.with(metadata, k, v) {
                state = next;
            }
        }
        Ok(state)
    }

    fn set(&mut self, p: [i32; 3], state: BlockState) {
        let chunk = [p[0] >> 4, p[2] >> 4];
        if (chunk[0] - self.chunk[0]).abs() <= 1 && (chunk[1] - self.chunk[1]).abs() <= 1 {
            // Level.setBlock(.., 2): to the clients, no neighbour updates.
            self.context.set_block(p, state, BlockUpdateFlags::CLIENTS);
        }
    }

    fn put(
        &mut self,
        p: [i32; 3],
        block: &str,
        props: &[(&str, &str)],
    ) -> Result<(), FeatureError> {
        let state = self.state(block, props)?;
        self.set(p, state);
        Ok(())
    }

    /// BlockStateRandomizer.get: one roll, the first entry whose cumulative share it is under.
    fn pick(&mut self, default: &'static str, entries: &[(&'static str, f32)]) -> &'static str {
        let roll = self.random.next_float();
        let mut total = 0.0;
        for (name, share) in entries {
            total += share;
            if roll < total {
                return name;
            }
        }
        default
    }

    /// BridgeFeature.place: the template with its corner at `at` and sea level, rotated
    /// counterclockwise for an east-west bridge, then the processors in their order.
    fn bridge(&mut self, b: &Bridge, at: [i32; 3]) -> Result<(), FeatureError> {
        let template = &TEMPLATES[b.template];
        let corner = [at[0], SEA_LEVEL, at[2]];
        let palette = if b.z_axis {
            template.palette
        } else {
            template.rotated
        };
        // StructureTemplate's transform about the corner: COUNTERCLOCKWISE_90 takes (x, z) to
        // (z, -x).
        let world = |p: [i32; 3]| {
            if b.z_axis {
                [corner[0] + p[0], corner[1] + p[1], corner[2] + p[2]]
            } else {
                [corner[0] + p[2], corner[1] + p[1], corner[2] - p[0]]
            }
        };
        let blocks: Vec<([i32; 3], State)> = template
            .blocks
            .iter()
            .map(|(p, i)| (world(*p), palette[*i]))
            .collect();
        for &(p, (block, props)) in &blocks {
            let mut state = self.state(block, props)?;
            // A waterloggable block placed into water takes it (LiquidBlockContainer.placeLiquid).
            if props.iter().any(|(k, _)| *k == "waterlogged") && self.water_source(p) {
                state = state
                    .with(self.context.metadata(), "waterlogged", "true")
                    .unwrap_or(state);
            }
            self.set(p, state);
        }
        // The processors' getBiome(cornerPos).
        let wood = wood_of(self.context.biome(corner));
        self.processors(&blocks, wood)
    }

    fn water_source(&mut self, p: [i32; 3]) -> bool {
        let Some(state) = self.context.block(p) else {
            return false;
        };
        let metadata = self.context.metadata();
        match name(self.context, state) {
            "minecraft:water" => state.property(metadata, "level") == Ok(Some("0")),
            _ => state.property(metadata, "waterlogged") == Ok(Some("true")),
        }
    }

    /// ITemplateFeatureProcessor.generatePillarDown: the replacement at `p`, then legs down
    /// through air and liquid to y 0.
    fn pillar(
        &mut self,
        p: [i32; 3],
        mut replacement: impl FnMut(&mut Self) -> Result<BlockState, FeatureError>,
        mut leg: impl FnMut(&mut Self) -> Result<BlockState, FeatureError>,
    ) -> Result<(), FeatureError> {
        let state = replacement(self)?;
        self.set(p, state);
        let mut q = [p[0], p[1] - 1, p[2]];
        while q[1] > 0 {
            let open = self
                .context
                .block(q)
                .is_some_and(|s| self.context.is_air(s))
                || liquid(self.context, q);
            if !open {
                break;
            }
            let state = leg(self)?;
            self.set(q, state);
            q[1] -= 1;
        }
        Ok(())
    }

    fn stone_brick(&mut self) -> &'static str {
        self.pick(
            "minecraft:stone_bricks",
            &[
                ("minecraft:mossy_stone_bricks", 0.5),
                ("minecraft:cracked_stone_bricks", 0.25),
            ],
        )
    }

    fn cobble(&mut self) -> &'static str {
        self.pick(
            "minecraft:cobblestone",
            &[("minecraft:mossy_cobblestone", 0.6)],
        )
    }

    /// The twelve processors of BridgeFeature, in its order, each over the template's blocks of
    /// one kind as placed (template.filterBlocks).
    fn processors(&mut self, blocks: &[([i32; 3], State)], wood: &str) -> Result<(), FeatureError> {
        let of = |block: &str| -> Vec<([i32; 3], Props)> {
            blocks
                .iter()
                .filter(|(_, (n, _))| n.strip_prefix("minecraft:") == Some(block))
                .map(|(p, (_, props))| (*p, *props))
                .collect()
        };
        let log = format!("minecraft:{wood}_log");
        let fence = format!("minecraft:{wood}_fence");
        let planks = format!("minecraft:{wood}_planks");
        let slab = format!("minecraft:{wood}_slab");
        let stairs = format!("minecraft:{wood}_stairs");

        // DynamicLegProcessor.
        for (p, s) in of("yellow_stained_glass") {
            self.pillar(p, |me| me.state(&log, s), |me| me.state(&log, s))?;
        }
        for (p, s) in of("warped_fence") {
            self.pillar(p, |me| me.state(&fence, s), |me| me.state(&fence, s))?;
        }
        for (p, _) in of("pink_stained_glass") {
            self.pillar(
                p,
                |me| {
                    let n = me.stone_brick();
                    me.state(n, &[])
                },
                |me| {
                    let n = me.stone_brick();
                    me.state(n, &[])
                },
            )?;
        }
        for (p, _) in of("light_blue_stained_glass") {
            self.pillar(
                p,
                |me| me.state("minecraft:polished_andesite", &[]),
                |me| me.state("minecraft:polished_andesite", &[]),
            )?;
        }
        for (p, _) in of("magenta_stained_glass") {
            self.pillar(
                p,
                |me| {
                    let n = me.cobble();
                    me.state(n, &[])
                },
                |me| {
                    let n = me.cobble();
                    me.state(n, &[])
                },
            )?;
        }
        for (p, _) in of("gray_stained_glass") {
            self.pillar(
                p,
                |me| me.state("minecraft:polished_andesite", &[]),
                |me| {
                    let n = me.stone_brick();
                    me.state(n, &[])
                },
            )?;
        }
        for (p, s) in of("prismarine_wall") {
            // The leg is getFenceBlockWithState of a wall: the wall's default state.
            self.pillar(
                p,
                |me| me.state("minecraft:cobblestone_wall", s),
                |me| me.state("minecraft:cobblestone_wall", &[]),
            )?;
        }
        // LogBiomeProcessor, StairBiomeProcessor, PlanksBiomeProcessor, SlabBiomeProcessor.
        for (p, s) in of("oak_log") {
            self.put(p, &log, s)?;
        }
        for (p, s) in of("oak_stairs") {
            self.put(p, &stairs, s)?;
        }
        for (p, _) in of("oak_planks") {
            self.put(p, &planks, &[])?;
        }
        for (p, s) in of("oak_slab") {
            self.put(p, &slab, s)?;
        }
        // StoneVariationProcessor.
        for (p, _) in of("stone_bricks") {
            let n = self.stone_brick();
            self.put(p, n, &[])?;
        }
        for (p, s) in of("stone_brick_slab") {
            let n = self.pick(
                "minecraft:stone_brick_slab",
                &[("minecraft:mossy_stone_brick_slab", 0.5)],
            );
            self.put(p, n, s)?;
        }
        for (p, s) in of("stone_brick_wall") {
            let n = self.pick(
                "minecraft:stone_brick_wall",
                &[("minecraft:mossy_stone_brick_wall", 0.5)],
            );
            self.put(p, n, s)?;
        }
        for (p, s) in of("stone_brick_stairs") {
            let n = self.pick(
                "minecraft:stone_brick_stairs",
                &[("minecraft:mossy_stone_brick_stairs", 0.5)],
            );
            self.put(p, n, s)?;
        }
        for (p, _) in of("cobblestone") {
            let n = self.cobble();
            self.put(p, n, &[])?;
        }
        // LanternRotProcessor: half the lanterns and torches go.
        for (p, s) in of("lantern") {
            let n = self.pick("minecraft:air", &[("minecraft:lantern", 0.5)]);
            self.put(p, n, s)?;
        }
        for (p, _) in of("torch") {
            let n = self.pick("minecraft:air", &[("minecraft:torch", 0.5)]);
            self.put(p, n, &[])?;
        }
        for (p, _) in of("wall_torch") {
            let n = self.pick("minecraft:air", &[("minecraft:wall_torch", 0.5)]);
            self.put(p, n, &[])?;
        }
        // OptionalWallProcessor.
        for (p, s) in of("end_stone_brick_wall") {
            let n = self.pick("minecraft:air", &[("minecraft:stone_brick_wall", 0.5)]);
            self.put(p, n, s)?;
        }
        // OptionalBlockProcessor.
        let optional: [(&str, &[(&'static str, f32)]); 5] = [
            (
                "brown_stained_glass",
                &[
                    ("minecraft:stone_bricks", 0.1875),
                    ("minecraft:mossy_stone_bricks", 0.375),
                    ("minecraft:cracked_stone_bricks", 0.1875),
                ],
            ),
            (
                "blue_stained_glass",
                &[("minecraft:chiseled_stone_bricks", 0.75)],
            ),
            ("red_stained_glass", &[("minecraft:cobblestone", 0.75)]),
            ("end_stone_bricks", &[("minecraft:polished_andesite", 0.75)]),
            (
                "dark_prismarine",
                &[
                    ("minecraft:cobblestone", 0.375),
                    ("minecraft:andesite", 0.375),
                ],
            ),
        ];
        for (block, entries) in optional {
            for (p, _) in of(block) {
                let n = self.pick("minecraft:air", entries);
                self.put(p, n, &[])?;
            }
        }
        for (p, s) in of("lime_stained_glass") {
            if self.random.next_float() < 0.5 {
                self.put(p, &log, s)?;
            } else {
                self.put(p, "minecraft:air", &[])?;
            }
        }
        // OptionalSlabProcessor.
        let slabs: [(&str, &[(&'static str, f32)]); 4] = [
            ("purpur_slab", &[("minecraft:stone_brick_slab", 0.75)]),
            (
                "end_stone_brick_slab",
                &[("minecraft:polished_andesite_slab", 0.75)],
            ),
            (
                "dark_prismarine_slab",
                &[
                    ("minecraft:cobblestone_slab", 0.45),
                    ("minecraft:andesite_slab", 0.45),
                ],
            ),
            (
                "prismarine_brick_slab",
                &[("minecraft:smooth_stone_slab", 0.9)],
            ),
        ];
        for (block, entries) in slabs {
            for (p, s) in of(block) {
                let n = self.pick("minecraft:air", entries);
                self.put(p, n, s)?;
            }
        }
        for (p, s) in of("crimson_slab") {
            if self.random.next_float() < 0.5 {
                self.put(p, &slab, s)?;
            } else {
                self.put(p, "minecraft:air", &[])?;
            }
        }
        for (p, s) in of("warped_slab") {
            let n = self.pick("minecraft:air", &[("minecraft:cobblestone_slab", 0.9)]);
            self.put(p, n, s)?;
        }
        // OptionalStairProcessor.
        let stair_kinds: [(&str, &[(&'static str, f32)]); 3] = [
            ("purpur_stairs", &[("minecraft:stone_brick_stairs", 0.75)]),
            (
                "dark_prismarine_stairs",
                &[
                    ("minecraft:cobblestone_stairs", 0.375),
                    ("minecraft:andesite_stairs", 0.375),
                ],
            ),
            ("warped_stairs", &[("minecraft:cobblestone_stairs", 0.75)]),
        ];
        for (block, entries) in stair_kinds {
            for (p, s) in of(block) {
                let n = self.pick("minecraft:air", entries);
                self.put(p, n, s)?;
            }
        }
        for (p, s) in of("crimson_stairs") {
            self.put(p, &stairs, s)?;
        }
        // FenceBiomeProcessor: three in four fences stay (all under an occluding block), in the
        // biome's wood, dropping their north and west arms where nothing occludes; the rest go,
        // unhooking the fences north and west of them.
        for (p, s) in of("oak_fence") {
            let above = [p[0], p[1] + 1, p[2]];
            let north = [p[0], p[1], p[2] - 1];
            let west = [p[0] - 1, p[1], p[2]];
            if self.random.next_float() < 0.75 || occludes(self.context, above) {
                let mut state = self.state(&fence, s)?;
                let (north_open, west_open) = (
                    !occludes(self.context, north),
                    !occludes(self.context, west),
                );
                let metadata = self.context.metadata();
                if north_open {
                    state = state.with(metadata, "north", "false").unwrap_or(state);
                }
                if west_open {
                    state = state.with(metadata, "west", "false").unwrap_or(state);
                }
                self.set(p, state);
            } else {
                self.put(p, "minecraft:air", &[])?;
                self.unhook(north, "south");
                self.unhook(west, "east");
            }
        }
        Ok(())
    }

    /// Sets a fence's `side` arm false where it is true.
    fn unhook(&mut self, p: [i32; 3], side: &str) {
        let Some(state) = self.context.block(p) else {
            return;
        };
        let metadata = self.context.metadata();
        if state.property(metadata, side) == Ok(Some("true"))
            && let Ok(next) = state.with(metadata, side, "false")
        {
            self.set(p, next);
        }
    }
}
