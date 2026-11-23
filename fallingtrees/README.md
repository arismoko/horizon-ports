# Falling Trees for Sunrise

Chop the bottom log of a tree with an axe and the whole tree comes down, logs, leaves and all.

A port of [Panda's Falling Trees](https://github.com/PandaMods-Dev/Pandas-Falling-Trees) by The Panda Oliver to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), written in Rust on Sunrise's mod API.

## What it does

- Breaking a stem fells its tree. Four kinds: ordinary trees (stems found by the log tag, leaves by the leaves tag and their distance, plus vines, cocoa and bee nests), cactus and bamboo (from the chopped block up), chorus plants with their flowers, and giant mushrooms with their caps.
- The tree is a falling entity: it tips away from you about its far edge, bounces, and after its lifetime drops the loot at the stump. The loot is each block's own, rolled with your tool (Silk Touch and Fortune count).
- Fall and landing sounds, the fall, bounce and lifetime lengths, the leaf radius, the stem limit, persistent leaves and the volumes are settings.
- The tool wears one point per stem and you tire as if you had chopped each one (each can be cut down to one block's worth in settings). Unbreaking applies.
- Sneak to chop a single block (settings invert or drop this). Each kind can be switched off or made to need an axe.
- Bigger trees take longer to chop: the more blocks, the slower the first one breaks, up to a cap, as the original's dynamic mining speed (settings).
- Works in survival and creative.

## Differences from the original

- Options are Sunrise mod settings, and the block and tool lists are text files in `config/fallingtrees/` (`logs.txt`, `leaves.txt`, `vertical_blocks.txt`, `tools.txt`: ids with `*`, `#tags`, and `!id` to refuse one). The tool list is by item name only: a host can't read an item's tags, so `#minecraft:axes` becomes `*_axe`.
- "One block's tiredness only" charges one block's exhaustion (0.005); the original's flat charge for this option is far larger, which looks like a slip in it.
- The tree's blocks are drawn as plain block models rotated about the stump; the original draws its own baked model.

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
