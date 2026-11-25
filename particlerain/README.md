# Particle Rain

A Sunrise 26.3 port of [Particle Rain](https://github.com/PigCart/particle-rain) by PigCart (MIT).

## What it does

- **16 built-in weather particle effects**: rain, heavy rain, snow, heavy snow, desert dust, skittering dust, rain splashes (`minecraft:rain`), water ripples, rain steam (`minecraft:smoke` on hot blocks), rain streaks sliding down walls and dripping off edges, tumbling 3D desert shrubs (`minecraft:dead_bush`), horizontal and vertical wet-biome fog, after-rain fog, and desert dust haze.
- **Resource-pack-aware weather sprites**: splits vanilla or resource-pack `textures/environment/rain.png` and `snow.png` into individual and merged heavy particle sprites with alpha boosting and optional water-tint desaturation, and generates concentric Bresenham-circle ripple sprites at configurable or pack-matched resolution.
- **Dynamic wind simulation**: sine-wave wind gusts with configurable strength, gust frequency, modulation speed, storm wind multiplier, and altitude scaling above Y 50.
- **Full particle orientation and fading**: supports camera-facing, relative-velocity, world-velocity, vertical, and horizontal quads with backface flipping, distance/in-and-out/out fading, pitch-angle fading, distance/FOV scaling, sprite animation, and block bouncing.
- **Biome and block tinting**: tints rain, ripples, and streaks with a configurable blend of biome water and fog colors; tints fog by biome fog color and rain level; tints desert dust by ground block map color.
- **Weather sounds**: scales or mutes vanilla rain sounds (`rainVolume`) and plays custom snow and desert sandstorm ambient sounds (`snowVolume`, `windVolume`), including muffled overhead variants when sheltered.
- **Commands and F3 debug entries**: adds `/particlerain`, `/particlerain debug`, and `/particlerain biometags`, plus `particlerain:debug` and `particlerain:biome_tags` F3 debug entries.
- **Settings, hot-reloaded config, and translations**: core controls live in **Options → Mods → Particle Rain**; full per-preset configuration and block/biome filters live in `config/particlerain.toml` and reload automatically. Includes all 8 translations from the original mod (`en_us`, `es_mx`, `ko_kr`, `pl_pl`, `ru_ru`, `uk_ua`, `zh_cn`, `zh_tw`).

## Differences from the original

- The package declares `side = "both"` instead of `side = "client"`, so the port must be installed on the server as well as the client, because Sunrise's `Registrar::register_particle_type` requires `side = "both"` (gap: sunrise-org/sunrise#5611).
- Vanilla rain and snow weather columns cannot be suppressed yet, so they continue to render alongside Particle Rain's custom particles (`renderDefaultRain` / `renderDefaultSnow` are preserved in config) (gap: sunrise-org/sunrise#968).
- Vanilla rain splash particles cannot be suppressed yet (`doDefaultSplashing` is preserved in config) (gap: sunrise-org/sunrise#5614).
- Biome water tinting applies to Particle Rain's own rain, ripple, and streak particles, but cannot recolor vanilla `minecraft:rain`, `minecraft:splash`, or water-drip particles on creation (gap: sunrise-org/sunrise#5615).
- Active shader packs cannot be detected from `ClientHost` yet, so `dontTintWithShaders` does not automatically disable water tinting while a shader pack is active; toggle **Tint water particles with biome colors** manually if needed (gap: sunrise-org/sunrise#5371).
- When **Limit spawn height** is enabled with **Spawn height limit** set to `0` (Automatic), the Overworld default cloud height (`192`) is used instead of querying a custom dimension's cloud height (gap: sunrise-org/sunrise#5385).
- Fog particles configured with the `BLENDED` render type draw on Sunrise's standard translucent particle layer rather than a custom blended particle pipeline (gap: sunrise-org/sunrise#5616).
- In-game configuration uses Sunrise's flat **Options → Mods → Particle Rain** page rather than a tabbed per-particle GUI editor; individual particle presets and whitelists/blacklists are edited in `config/particlerain.toml` (gap: [sunrise-org/sunrise#1217](https://github.com/sunrise-org/sunrise/issues/1217)).

## Configuration

- In-game settings: **Options → Mods → Particle Rain**
- Full config file (including all 16 particle presets, custom presets, and block/biome filters): `config/particlerain.toml` (hot-reloaded automatically when edited)

## Commands

- `/particlerain` — prints where to configure Particle Rain and lists subcommands
- `/particlerain debug` — prints live particle counts, weather levels, wind multiplier, and surface heights in chat
- `/particlerain biometags` — lists the current biome and its matching tags in chat

## Building

```sh
cargo sunrise-mod check
cargo sunrise-mod build
CARGO_TARGET_DIR=target cargo test
cargo clippy --all-targets -- -D warnings
```

## Licence

MIT, matching the original project. See `LICENSE` and `NOTICE`.
