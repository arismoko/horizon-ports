# Falling Leaves

A Sunrise 26.3 port of [Falling Leaves](https://github.com/Fourmisain/fallingleaves) (2.0.8) by Fourmisain, BrekiTomasson, and RandomMcSomethin (MIT).

## What it does

- **Wind-driven falling leaf particles**: adds custom falling leaf particles (`regular`, `conifer`, `cherry`, and `vanilla`-textured variants) with per-kind gravity, rain weight under open sky, accelerating spin, water floating/buoyancy/flow push, lava burning (spawning a lava splash), ground collision and settling, and a 16-tick fade-out.
- **Smooth-noise wind simulation**: transitions smoothly between **Calm**, **Windy**, and **Stormy** states every 6 minutes and on weather changes (rain and thunder), with configurable `wind_enabled` and `windless_dimensions` (defaults to `minecraft:the_nether` and `minecraft:the_end`).
- **Snowflakes from snowy trees**: drops snowflake particles from leaf blocks in snowy biomes or beneath surface snow / snow blocks / powder snow.
- **Block-hit and leaf-decay bursts**: hitting a leaf block drops 0–3 leaves and 0–6 snowflakes; decaying leaf blocks release a burst of leaves and snowflakes in both singleplayer and multiplayer.
- **Spawn controls & custom spawners**: supports leaf size, lifespan, regular/conifer/snowflake spawn rates, `drop_from_player_placed_blocks`, `minimum_free_space_below` collision check, `starting_spawn_radius`, `decay_spawn_rate_factor`, per-block overrides (`[leaf_settings."<block_id>"]`), and custom `leaf_spawners` block states (e.g. `minecraft:bamboo[leaves=large]`).
- **Options screen, hot-reloaded config, and translations**: 15 scalar settings live in **Options → Mods → Falling Leaves** with all 12 original translations (`de_de`, `en_us`, `es_ar`, `es_mx`, `ko_kr`, `pt_br`, `ru_ru`, `tr_tr`, `uk_ua`, `vi_vn`, `zh_cn`, `zh_tw`); full config including per-block overrides lives in `config/fallingleaves-client.toml`.
- **F3 debug entry**: registers `fallingleaves:wind` in debug options to show the live wind state, velocity, and dimension.

## Differences from the original

- Vanilla's built-in `FallingParticlesLeavesBlock` particles (`tinted_leaves`, `cherry_leaves`, `pale_oak_leaves`, poplar leaves) cannot be cancelled or suppressed from `BlockPart::animate_tick`, so they continue to emit alongside Falling Leaves' custom wind-driven particles (gap: sunrise-org/sunrise#5382).
- Needs installing on the server as well as the client (`side = "both"` instead of `side = "client"`, with no `registerParticles = false` client-only mode) because a client-only mod cannot register particle types via `Registrar::register_particle_type` (gap: sunrise-org/sunrise#5611).
- Leaf particle colors use exact biome-blended `block_tint_color` / `biome_tint` plus vanilla leaf base colors rather than averaging stitched `TextureAtlasSprite` pixels (`NativeImage`), so resource packs or modded untinted leaves with custom colors use a fallback base color (gap: sunrise-org/sunrise#4601).
- Resource packs cannot supply arbitrary unregistered per-block sprite sets under `assets/fallingleaves/particles/block/<ns>/<path>.json` (gap: sunrise-org/sunrise#5612).
- Per-block `leaf_settings` maps and `windless_dimensions` / `leaf_spawners` lists are edited in `config/fallingleaves-client.toml` rather than in-game because Sunrise's Options → Mods screen is flat (gap: sunrise-org/sunrise#1174).
- Fabric Seasons and Serene Seasons compatibility multipliers are omitted because those mods are not present on Sunrise.

## Configuration

- In-game settings: **Options → Mods → Falling Leaves**
- Full config file (including `[leaf_settings."<block_id>"]`, `windless_dimensions`, and `leaf_spawners`): `config/fallingleaves-client.toml` (hot-reloaded automatically when edited)

## Building

```sh
cargo sunrise-mod check
cargo sunrise-mod build
CARGO_TARGET_DIR=target cargo test
cargo clippy --all-targets -- -D warnings
```

## Licence

MIT, matching the original project. See `LICENSE` and `NOTICE`.
