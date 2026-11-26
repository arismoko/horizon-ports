//! Leaf color calculation, room-below collision checks, snowy tree detection,
//! and particle spawning helpers matching Falling Leaves' `LeafUtil`.

use crate::config::{FallingLeavesConfig, LeafSettingsEntry, ParticleImplementation};
use crate::particles::LeafParticleData;
use mod_api::{
    AtlasSpriteError, BlockState, ClientHost, ColorResolver, Direction, Heightmap, LevelRef,
    ParticleKey, ParticleOptions, ParticleType, RandomSource, SpriteRef, TagKey,
};
use std::cell::Cell;
use std::collections::HashMap;
use std::ops::ControlFlow;
use std::sync::{Mutex, OnceLock};

thread_local! {
    static OWN_VANILLA_SPAWN: Cell<bool> = const { Cell::new(false) };
}

/// Whether this mod is itself adding a vanilla-typed leaf particle right now, which
/// `client_particle_add` must not cancel.
pub fn own_vanilla_spawn_active() -> bool {
    OWN_VANILLA_SPAWN.with(Cell::get)
}

/// The `#minecraft:leaves` tag key resolved during `Hooks::register`.
pub static LEAVES_TAG: OnceLock<TagKey> = OnceLock::new();

/// Vanilla biomes whose `VillagerType.byBiome` is `VillagerType.SNOW`.
const SNOWY_VILLAGER_BIOMES: &[&str] = &[
    "minecraft:snowy_plains",
    "minecraft:ice_spikes",
    "minecraft:snowy_taiga",
    "minecraft:snowy_slopes",
    "minecraft:jagged_peaks",
    "minecraft:frozen_peaks",
    "minecraft:grove",
    "minecraft:frozen_river",
    "minecraft:frozen_ocean",
    "minecraft:deep_frozen_ocean",
    "minecraft:snowy_beach",
];

/// Handles to our registered custom particle types and vanilla leaf/lava particle keys.
#[derive(Clone, Copy)]
pub struct ParticleHandles {
    pub falling_leaf: ParticleType<LeafParticleData>,
    pub falling_leaf_vanilla: ParticleType<LeafParticleData>,
    pub falling_leaf_conifer: ParticleType<LeafParticleData>,
    pub falling_cherry: ParticleType<LeafParticleData>,
    pub falling_snow: ParticleType<LeafParticleData>,
    pub vanilla_lava: ParticleKey,
    pub vanilla_tinted_leaves: ParticleKey,
    pub vanilla_cherry_leaves: ParticleKey,
    pub vanilla_pale_oak_leaves: ParticleKey,
    pub vanilla_red_poplar_leaves: ParticleKey,
    pub vanilla_orange_poplar_leaves: ParticleKey,
    pub vanilla_yellow_poplar_leaves: ParticleKey,
}

/// Chebyshev (`L_infinity`) distance between two block positions, matching
/// `LeafUtil.getMaximumDistance`.
pub fn chebyshev_distance(a: [i32; 3], b: [i32; 3]) -> i32 {
    (a[0] - b[0])
        .abs()
        .max((a[1] - b[1]).abs())
        .max((a[2] - b[2]).abs())
}

/// Extract a property value from a serialized block state string
/// (`minecraft:oak_leaves[distance=7,persistent=false,waterlogged=false]`).
pub fn parse_block_state_property<'a>(serialized: &'a str, key: &str) -> Option<&'a str> {
    let open = serialized.find('[')?;
    let close = serialized.rfind(']')?;
    if close <= open {
        return None;
    }
    for entry in serialized[open + 1..close].split(',') {
        if let Some((k, v)) = entry.split_once('=')
            && k == key
        {
            return Some(v);
        }
    }
    None
}

pub fn is_persistent_leaf(serialized: &str) -> bool {
    parse_block_state_property(serialized, "persistent") == Some("true")
}

/// Vanilla decaying leaves: `persistent=false` and `distance=7` (`LeavesBlock.isRandomlyTicking`).
pub fn is_decaying_leaf(serialized: &str) -> bool {
    parse_block_state_property(serialized, "persistent") == Some("false")
        && parse_block_state_property(serialized, "distance") == Some("7")
}

pub fn is_snowy_biome(biome_id: &str) -> bool {
    SNOWY_VILLAGER_BIOMES.contains(&biome_id)
}

/// Binomial(n, 0.5) sample using `random.next_boolean()`, matching `Leaves.java` and `LeafTickMixin.java`.
pub fn binomial_half(n: i32, random: &mut dyn RandomSource) -> i32 {
    let mut count = 0;
    for _ in 0..n.max(0) {
        if random.next_boolean() {
            count += 1;
        }
    }
    count
}

/// Compute the effective ambient spawn chance for a leaf block at `pos`.
pub fn modified_spawn_chance(
    config: &FallingLeavesConfig,
    player_block_pos: Option<[i32; 3]>,
    pos: [i32; 3],
    serialized_state: &str,
    is_leaf_block: bool,
    settings: &LeafSettingsEntry,
) -> f32 {
    if config.starting_spawn_radius > 0
        && let Some(player_pos) = player_block_pos
        && chebyshev_distance(player_pos, pos) < config.starting_spawn_radius
    {
        return 0.0;
    }

    if !config.drop_from_player_placed_blocks && is_persistent_leaf(serialized_state) {
        return 0.0;
    }

    let mut chance = settings.spawn_chance(config);

    if config.decay_spawn_rate_factor != 1.0 && is_leaf_block && is_decaying_leaf(serialized_state)
    {
        chance *= config.decay_spawn_rate_factor;
    }

    chance
}

/// Unpack `0xRRGGBB` into normalized `[r, g, b]` in `0.0..=1.0`.
pub fn unpack_rgb(rgb: u32) -> [f32; 3] {
    [
        ((rgb >> 16) & 0xFF) as f32 / 255.0,
        ((rgb >> 8) & 0xFF) as f32 / 255.0,
        (rgb & 0xFF) as f32 / 255.0,
    ]
}

/// Fallback texture brightness / base RGB for vanilla leaf blocks, used only when the
/// resource pack's sprite pixels cannot be read (`atlas_sprite_pixels`).
/// Tinted leaves use a greyscale leaf texture (~0.72 average brightness) multiplied by
/// the biome foliage tint; untinted leaves carry their own fixed leaf RGB.
pub fn base_texture_rgb(block_id: &str, is_tinted: bool) -> [f32; 3] {
    match block_id {
        "minecraft:cherry_leaves" => [0.95, 0.72, 0.80],
        "minecraft:pale_oak_leaves" => [0.49, 0.52, 0.49],
        "minecraft:azalea_leaves" => [0.44, 0.57, 0.18],
        "minecraft:flowering_azalea_leaves" => [0.56, 0.52, 0.36],
        "minecraft:red_poplar_leaves" => [0.76, 0.25, 0.19],
        "minecraft:orange_poplar_leaves" => [0.85, 0.47, 0.16],
        "minecraft:yellow_poplar_leaves" => [0.88, 0.75, 0.22],
        _ if is_tinted => [0.72, 0.72, 0.72],
        _ => [0.45, 0.65, 0.28],
    }
}

/// Average RGB of the visible pixels (alpha above zero) of an RGBA8 sprite, normalized to
/// `0.0..=1.0`. `None` when the buffer holds no visible pixel. Matches the original's
/// average over the sprite's non-transparent pixels.
pub fn average_opaque_rgb(rgba: &[u8]) -> Option<[f32; 3]> {
    let mut sum = [0u64; 3];
    let mut count = 0u64;
    for px in rgba.as_chunks::<4>().0 {
        if px[3] == 0 {
            continue;
        }
        sum[0] += u64::from(px[0]);
        sum[1] += u64::from(px[1]);
        sum[2] += u64::from(px[2]);
        count += 1;
    }
    if count == 0 {
        return None;
    }
    let scale = 255.0 * count as f64;
    Some([
        (sum[0] as f64 / scale) as f32,
        (sum[1] as f64 / scale) as f32,
        (sum[2] as f64 / scale) as f32,
    ])
}

/// Averages already computed from the active resource pack's blocks atlas, keyed by sprite
/// reference and dropped whenever `block_sprites_version` changes (a resource reload).
#[derive(Default)]
struct SpriteAverages {
    version: u64,
    by_sprite: HashMap<(u32, u32), Option<[f32; 3]>>,
}

static SPRITE_AVERAGES: OnceLock<Mutex<SpriteAverages>> = OnceLock::new();

/// The average visible colour of `sprite`'s first frame, read from the blocks atlas on the CPU
/// (`RenderAssets::atlas_sprite_pixels`). `None` when the atlas is not ready, the reference is
/// stale, or the sprite is fully transparent; callers then use the built-in fallback table.
fn sprite_average_rgb(host: &dyn ClientHost, sprite: SpriteRef) -> Option<[f32; 3]> {
    let version = host.block_sprites_version();
    let mut cache = SPRITE_AVERAGES
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if cache.version != version {
        cache.version = version;
        cache.by_sprite.clear();
    }
    let key = (sprite.array, sprite.layer);
    if let Some(hit) = cache.by_sprite.get(&key) {
        return *hit;
    }
    let average = match host.render_assets().atlas_sprite_pixels(sprite) {
        Ok(pixels) => average_opaque_rgb(&pixels.rgba),
        // Not ready (resources still loading): do not cache, ask again next time.
        Err(AtlasSpriteError::NotReady) => return None,
        Err(_) => None,
    };
    cache.by_sprite.insert(key, average);
    average
}

/// Resolve the particle RGB color for `state` at `pos`: the average colour of the block's
/// sprite in the active resource pack (falling back to `base_texture_rgb` when it cannot be
/// read), multiplied by the biome-blended tint for tinted faces.
pub fn leaf_color(host: &dyn ClientHost, state: BlockState, pos: [i32; 3]) -> [f32; 3] {
    let block_id = state.block().id(host).unwrap_or("");

    let sprites = host.block_sprites(&[state]).into_iter().next().flatten();
    let face = sprites.as_ref().and_then(|sprites| {
        sprites
            .sides
            .get(Direction::Down as usize)
            .and_then(|faces| faces.first())
    });
    let tint_index = face.map(|f| f.tint_index).unwrap_or(0);
    let sprite = face
        .map(|f| f.sprite)
        .or_else(|| sprites.as_ref().and_then(|s| s.particle));

    let is_tinted = tint_index >= 0;
    let mut rgb = sprite
        .and_then(|sprite| sprite_average_rgb(host, sprite))
        .unwrap_or_else(|| base_texture_rgb(block_id, is_tinted));

    if is_tinted {
        let tint = host.block_tint_color(state, tint_index, pos).or_else(|| {
            host.biome_tint(pos, ColorResolver::Foliage)
                .ok()
                .map(|argb| argb & 0x00FF_FFFF)
        });
        if let Some(color) = tint {
            let t = unpack_rgb(color);
            rgb[0] *= t[0];
            rgb[1] *= t[1];
            rgb[2] *= t[2];
        }
    }

    rgb
}

/// Check that there is no leaf block directly below and enough collision-free vertical space.
pub fn has_room_for_leaf_particle(
    host: &dyn ClientHost,
    level: LevelRef,
    pos: [i32; 3],
    x: f64,
    y: f64,
    z: f64,
    min_free_space_below: i32,
) -> bool {
    let below = [pos[0], pos[1] - 1, pos[2]];
    if let Some(below_block) = level.block(host, below)
        && let Some(leaves) = LEAVES_TAG.get()
        && below_block
            .is_in_tag(host.tags(), leaves.typed())
            .unwrap_or(false)
    {
        return false;
    }

    let y2 = y - f64::from(min_free_space_below.max(1)) * 0.5;
    let aabb = [x - 0.1, y2, z - 0.1, x + 0.1, y, z + 0.1];
    !level.has_collisions(host, aabb).unwrap_or(true)
}

/// Check whether `pos` is in a snowy biome or under snow/snow_block/powder_snow on the surface.
pub fn is_snowy_location(host: &dyn ClientHost, level: LevelRef, pos: [i32; 3]) -> bool {
    if let Some(biome) = level.client_biome(host, pos)
        && is_snowy_biome(&biome)
    {
        return true;
    }

    let mut surface = None;
    let read = level.visit_surface_heights(
        host,
        &[(pos[0], pos[2])],
        Heightmap::WorldSurface,
        &mut |_, height| {
            surface = height;
            ControlFlow::Break(())
        },
    );
    if read.is_ok()
        && let Some(surface_y) = surface
    {
        let top_pos = [pos[0], surface_y - 1, pos[2]];
        if let Some(top_block) = level.block(host, top_pos)
            && let Some(top_id) = top_block.id(host)
            && matches!(
                top_id,
                "minecraft:snow" | "minecraft:snow_block" | "minecraft:powder_snow"
            )
        {
            return true;
        }
    }

    false
}

/// Spawn `count` leaf particles according to `settings` and `config`.
#[allow(clippy::too_many_arguments)]
pub fn spawn_leaf_particles(
    host: &mut dyn ClientHost,
    handles: &ParticleHandles,
    config: &FallingLeavesConfig,
    level: LevelRef,
    pos: [i32; 3],
    state: BlockState,
    count: i32,
    spawn_inside_block: bool,
    settings: &LeafSettingsEntry,
    random: &mut dyn RandomSource,
) {
    if count <= 0 {
        return;
    }

    let implementation = settings.effective_implementation(config);
    let block_id = state.block().id(&*host).unwrap_or("").to_owned();

    if implementation == ParticleImplementation::Vanilla {
        for _ in 0..count {
            let x = f64::from(pos[0]) + random.next_double();
            let z = f64::from(pos[2]) + random.next_double();
            let y = if spawn_inside_block {
                f64::from(pos[1]) + random.next_double()
            } else {
                f64::from(pos[1]) - 0.05
            };
            let opts = vanilla_leaf_options(&*host, handles, &block_id, state, pos);
            OWN_VANILLA_SPAWN.with(|flag| flag.set(true));
            let _ = host.add_particle(&opts, [x, y, z], [0.0, 0.0, 0.0]);
            OWN_VANILLA_SPAWN.with(|flag| flag.set(false));
        }
        return;
    }

    let (r, g, b) = if implementation == ParticleImplementation::Cherry {
        (1.0, 1.0, 1.0)
    } else {
        let c = leaf_color(&*host, state, pos);
        (c[0], c[1], c[2])
    };

    let data = LeafParticleData {
        r,
        g,
        b,
        size: config.leaf_quad_size(),
        lifespan: config.leaf_lifespan,
    };

    let opts = match implementation {
        ParticleImplementation::Cherry => handles.falling_cherry.options(data),
        ParticleImplementation::Conifer => handles.falling_leaf_conifer.options(data),
        ParticleImplementation::Regular => {
            if config.use_vanilla_textures {
                handles.falling_leaf_vanilla.options(data)
            } else {
                handles.falling_leaf.options(data)
            }
        }
        ParticleImplementation::Vanilla => unreachable!(),
    };

    spawn_custom_particles(
        host,
        config,
        level,
        pos,
        count,
        spawn_inside_block,
        &opts,
        random,
    );
}

fn vanilla_leaf_options(
    host: &dyn ClientHost,
    handles: &ParticleHandles,
    block_id: &str,
    state: BlockState,
    pos: [i32; 3],
) -> ParticleOptions {
    match block_id {
        "minecraft:cherry_leaves" => ParticleOptions::simple(handles.vanilla_cherry_leaves),
        "minecraft:pale_oak_leaves" => ParticleOptions::simple(handles.vanilla_pale_oak_leaves),
        "minecraft:red_poplar_leaves" => ParticleOptions::simple(handles.vanilla_red_poplar_leaves),
        "minecraft:orange_poplar_leaves" => {
            ParticleOptions::simple(handles.vanilla_orange_poplar_leaves)
        }
        "minecraft:yellow_poplar_leaves" => {
            ParticleOptions::simple(handles.vanilla_yellow_poplar_leaves)
        }
        "minecraft:azalea_leaves" | "minecraft:flowering_azalea_leaves" => {
            let rgb = unpack_rgb(0x0070_922D);
            ParticleOptions::colored(handles.vanilla_tinted_leaves, [rgb[0], rgb[1], rgb[2], 1.0])
        }
        _ => {
            let rgb = host
                .block_tint_color(state, 0, pos)
                .or_else(|| {
                    host.biome_tint(pos, ColorResolver::Foliage)
                        .ok()
                        .map(|argb| argb & 0x00FF_FFFF)
                })
                .map(unpack_rgb)
                .unwrap_or([0.45, 0.65, 0.28]);
            ParticleOptions::colored(handles.vanilla_tinted_leaves, [rgb[0], rgb[1], rgb[2], 1.0])
        }
    }
}

/// Spawn `count` snowflake particles if `pos` is in a snowy biome or snow-covered tree.
#[allow(clippy::too_many_arguments)]
pub fn spawn_snow_particles(
    host: &mut dyn ClientHost,
    handles: &ParticleHandles,
    config: &FallingLeavesConfig,
    level: LevelRef,
    pos: [i32; 3],
    count: i32,
    spawn_inside_block: bool,
    random: &mut dyn RandomSource,
) {
    if count <= 0 || !is_snowy_location(&*host, level, pos) {
        return;
    }
    let data = LeafParticleData {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        size: config.leaf_quad_size(),
        lifespan: config.leaf_lifespan,
    };
    let opts = handles.falling_snow.options(data);
    spawn_custom_particles(
        host,
        config,
        level,
        pos,
        count,
        spawn_inside_block,
        &opts,
        random,
    );
}

#[allow(clippy::too_many_arguments)]
fn spawn_custom_particles(
    host: &mut dyn ClientHost,
    config: &FallingLeavesConfig,
    level: LevelRef,
    pos: [i32; 3],
    count: i32,
    spawn_inside_block: bool,
    opts: &ParticleOptions,
    random: &mut dyn RandomSource,
) {
    for _ in 0..count {
        let x = f64::from(pos[0]) + random.next_double();
        let z = f64::from(pos[2]) + random.next_double();
        let y = if spawn_inside_block {
            f64::from(pos[1]) + random.next_double()
        } else {
            let y = f64::from(pos[1]) - 0.1;
            if !has_room_for_leaf_particle(
                &*host,
                level,
                pos,
                x,
                y,
                z,
                config.minimum_free_space_below,
            ) {
                continue;
            }
            y
        };
        let _ = host.add_particle(opts, [x, y, z], [0.0, 0.0, 0.0]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_block_state_properties_and_decay_state() {
        let s = "minecraft:oak_leaves[distance=7,persistent=false,waterlogged=false]";
        assert_eq!(parse_block_state_property(s, "distance"), Some("7"));
        assert_eq!(parse_block_state_property(s, "persistent"), Some("false"));
        assert!(!is_persistent_leaf(s));
        assert!(is_decaying_leaf(s));

        let placed = "minecraft:oak_leaves[distance=7,persistent=true,waterlogged=false]";
        assert!(is_persistent_leaf(placed));
        assert!(!is_decaying_leaf(placed));
    }

    #[test]
    fn average_opaque_rgb_skips_transparent_pixels() {
        let rgba = [
            255, 0, 0, 255, // red, visible
            0, 0, 255, 255, // blue, visible
            9, 9, 9, 0, // transparent: ignored
        ];
        let avg = average_opaque_rgb(&rgba).expect("visible pixels");
        assert!((avg[0] - 0.5).abs() < 1e-6);
        assert!(avg[1].abs() < 1e-6);
        assert!((avg[2] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn average_opaque_rgb_none_when_fully_transparent_or_empty() {
        assert_eq!(average_opaque_rgb(&[]), None);
        assert_eq!(average_opaque_rgb(&[1, 2, 3, 0, 4, 5, 6, 0]), None);
    }

    #[test]
    fn chebyshev_distance_matches_max_axis_delta() {
        assert_eq!(chebyshev_distance([0, 64, 0], [3, 60, -5]), 5);
        assert_eq!(chebyshev_distance([10, 10, 10], [10, 10, 10]), 0);
    }

    #[test]
    fn modified_spawn_chance_respects_radius_persistent_and_decay() {
        let cfg = FallingLeavesConfig {
            starting_spawn_radius: 5,
            drop_from_player_placed_blocks: false,
            decay_spawn_rate_factor: 2.6,
            ..FallingLeavesConfig::default()
        };
        let settings = LeafSettingsEntry::default_for("minecraft:oak_leaves");

        // Inside minimal radius -> 0
        assert_eq!(
            modified_spawn_chance(
                &cfg,
                Some([0, 64, 0]),
                [3, 64, 2],
                "minecraft:oak_leaves[distance=1,persistent=false]",
                true,
                &settings
            ),
            0.0
        );

        // Player-placed when disabled -> 0
        assert_eq!(
            modified_spawn_chance(
                &cfg,
                Some([0, 64, 0]),
                [10, 64, 0],
                "minecraft:oak_leaves[distance=7,persistent=true]",
                true,
                &settings
            ),
            0.0
        );

        // Decaying leaf outside radius -> 0.01 * 2.6
        let decay_chance = modified_spawn_chance(
            &cfg,
            Some([0, 64, 0]),
            [10, 64, 0],
            "minecraft:oak_leaves[distance=7,persistent=false]",
            true,
            &settings,
        );
        assert!((decay_chance - 0.026).abs() < 1e-6);
    }

    #[test]
    fn snowy_biome_detection() {
        assert!(is_snowy_biome("minecraft:snowy_taiga"));
        assert!(is_snowy_biome("minecraft:grove"));
        assert!(!is_snowy_biome("minecraft:forest"));
    }
}
