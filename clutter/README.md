# Clutter

Furniture you can sit on, candles and chandeliers, lamps, curtains that open, cupboards, copper, silver and onyx.

A port of [Clutter](https://github.com/EmilsGithub/Clutter-Mod-Repository) 0.6.2 by EmilSG to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), written in Rust on Sunrise's mod API, under the original's licence (MIT, in `LICENSE`; see `NOTICE`). It uses no `[[native.mixin]]` rows: every rule is the typed mod API.

The blocks, items, creative tabs and effects are data in `data/clutter/sunrise/`, with models, textures and translations in `assets/`. The rules are in `src/lib.rs`, going by each block's original Java class (`src/classes.rs`).

## Differences from the original

- The original's mobs, its two TerraBlender biomes, the brick kiln and some smaller features are left out.
- The clicking client predicts empty-handed clicks only; a held item (flint and steel, an axe, honeycomb, shears, coins) is handled by the server, so the same item in both hands can act once per hand.
