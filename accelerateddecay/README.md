# Accelerated Decay for Sunrise

Leaves decay quickly after a tree is cut.

A port of [Accelerated Decay](https://github.com/ErrorMikey/AcceleratedDecay) (26.3.0.1) by ErrorMikey to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), written in Rust on Sunrise's mod API.

## What it does

- Watches server block state updates on leaves (`#minecraft:leaves`). When an existing leaf updates to `distance=7` and `persistent=false` (no longer supported by a log), its position is added to that dimension's decay queue.
- Newly placed or bone-mealed tree leaves are left alone because the previous block was not already in `#minecraft:leaves`, and player-placed leaves (`persistent=true`) never decay.
- At the end of each server level tick, up to 5 queued leaves are removed in order, dropping their normal vanilla loot (respecting `minecraft:block_drops`), keeping fluids for waterlogged leaves, and emitting `minecraft:block_destroy`.
- Only the first queued position taken in a tick plays break sound and particles, keeping client sound and particle load low while a canopy clears.
- Each dimension's remaining decay queue is saved with the level (`accelerateddecay:break_data`), so leaves finish decaying after a save and reload.
- Works with any vanilla or modded tree using `#minecraft:leaves` and `#minecraft:logs`, and does not fire player block-break events.

Install it on the server (or in single player); clients do not need the mod to join.

## Differences from the original

- A queued position whose chunk is currently unloaded is skipped rather than force-loading the chunk; vanilla random ticks handle it once the chunk is visited again.
- After the first leaf of a tick, the others drop their loot without running block-drop listeners or experience, since the mod API can't yet drop a block's resources without its break effect ([sunrise-org/sunrise#5526](https://github.com/sunrise-org/sunrise/issues/5526)). Leaves give no experience, so only a mod listening to block drops would notice.

## Building

With the Sunrise mod tool (`cargo sunrise-mod`, from the Sunrise SDK) and Rust:

```sh
cargo sunrise-mod check
cargo sunrise-mod build    # target/sunrise-mod/accelerateddecay.smod
```

`sunrise-mod.toml` pins the Sunrise build this version is made for. The mod is server-side only.

## Licence and credits

Accelerated Decay is by ErrorMikey, under the GNU General Public License v3.0 only; `LICENSE` holds its text. This port contains no code or assets from the original and is distributed under the same licence. The icon is the port's own.
