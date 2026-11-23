# YUNG's Bridges for Sunrise

Stone and wooden bridges that span rivers where the banks allow, some of them crumbling.

A port of [YUNG's Bridges](https://github.com/YUNG-GANG/YUNGs-Bridges) by YUNGNICKYOUNG to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3). The original's bridge feature, crossing search and template processors are Rust here, as one feature type that world generation runs in river biomes.

## What it does

- Eleven bridge designs (stone and wooden, 15 to 31 blocks long, some broken), each north-south or east-west: 22 variants, as in the original.
- Bridges only where a river crosses the chunk with solid banks at both ends and open water between, at the original's rarity.
- Legs down to the river bed, mossy and cracked stone, missing slabs and stairs, and lanterns and torches that are sometimes gone, from the original's processors.
- Rivers from other mods count through the `c:river` biome tag, as in the original.

## Differences from the original

- Wooden parts are always oak: the original picks dark oak, spruce, jungle or acacia by the biome at the bank, and a world-generation feature cannot read the biome yet.

## Building

With the Sunrise mod tool (`cargo sunrise-mod`, from the Sunrise SDK) and Rust:

```sh
cargo sunrise-mod build    # compiles against the pinned game, packs target/sunrise-mod/yungsbridges.smod
cargo sunrise-mod run      # builds it and starts the game with it
cargo test
```

`sunrise-mod.toml` pins the Sunrise Dev build `dev-2026-11-22-7cfdf81` (game commit `7cfdf81463`). Sunrise builds native mods from source on the player's computer, against the exact game build.

## Licence and credits

YUNG's Bridges is by YUNGNICKYOUNG (https://github.com/YUNG-GANG/YUNGs-Bridges). This port is distributed under the original's licence, the **GNU LGPL v3.0 only**; `LICENSE` holds the text and `NOTICE` says where the port comes from. The bridge designs and data are the original's; the code is written anew in Rust.

Ported to Minecraft 26.3 by lavoide ([lavoide/horizon-ports](https://github.com/lavoide/horizon-ports)), and to Sunrise by arismoko.
