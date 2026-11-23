# Falling Trees for Sunrise

Chop the bottom log of a tree with an axe and the whole tree comes down, logs, leaves and all.

A port of [Panda's Falling Trees](https://github.com/PandaMods-Dev/Pandas-Falling-Trees) by The Panda Oliver to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), written in Rust on Sunrise's mod API.

## What it does

- Breaking a log with an axe fells its tree: every log above it, the leaves that belong to it (not a neighbouring tree's), and the vines, cocoa and bee nests on it.
- Each block drops what it would when mined with your axe (so Silk Touch and Fortune count), and the drops land at the stump.
- The axe wears one point per log and you tire as if you had chopped each log, as in the original; Unbreaking applies.
- Sneak to chop a single log. Persistent (player-placed) leaves are never part of a tree, and a tree of more than 256 logs is left alone.
- Bigger trees take longer to chop: the more logs, the slower the bottom one breaks (up to nine times slower), as the original's dynamic mining speed.
- Works in survival and creative, with any item named as an axe.

## Differences from the original

- The tree does not topple as an animated entity with its sounds: it comes down at once, at the end of the tick.
- Cactus, bamboo and chorus plants are left to vanilla.

## Building

With the Sunrise mod tool (`cargo sunrise-mod`, from the Sunrise SDK) and Rust:

```sh
cargo sunrise-mod build    # compiles against the pinned game, packs target/sunrise-mod/fallingtrees.smod
cargo sunrise-mod run      # builds it and starts the game with it
cargo test
```

`sunrise-mod.toml` pins the Sunrise Dev build `dev-2026-11-22-7cfdf81` (game commit `7cfdf81463`). Sunrise builds native mods from source on the player's computer, against the exact game build.

## Licence and credits

Panda's Falling Trees is by The Panda Oliver (https://github.com/PandaMods-Dev/Pandas-Falling-Trees). This port is distributed under the original's licence, the **GNU GPL v3.0 only**; `LICENSE` holds the text and `NOTICE` says where the port comes from. No Java code is included: the rules are written anew in Rust.

Ported to Minecraft 26.3 by lavoide ([lavoide/horizon-ports](https://github.com/lavoide/horizon-ports)), and to Sunrise by arismoko.
