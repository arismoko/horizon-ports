# Block Runner for Sunrise

Run faster on path blocks.

A port of [Block Runner](https://github.com/Fuzss/blockrunner) by Fuzs to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), written in Rust on Sunrise's mod API. The 26.3 carry is by lavoide; this port moves it to Sunrise and rebuilds it on the original's data map.

## What it does

Players and mobs move faster on some blocks:

| Blocks | Speed |
|---|---|
| Dirt path | ×1.35 |
| Stone bricks (`#minecraft:stone_bricks`) | ×1.15 |

The speed is a movement-speed modifier, `blockrunner:block_speed`, added when an entity steps onto such a block or lands on it, and kept through a jump. Flying in creative or gliding with an elytra drops it. A block with a speed moves at its own speed regardless of soul sand's or honey's slowness.

An item for a block with a speed shows it in its tooltip ("Speed Multiplier: 1.35").

## Giving blocks a speed

The speeds come from the data map `blockrunner:block_speeds` on blocks, as in the original. The port ships six block tags, each given a speed by that map:

| Tag | Speed |
|---|---|
| `blockrunner:very_slow_blocks` | 0.45 |
| `blockrunner:slow_blocks` | 0.65 |
| `blockrunner:slightly_slow_blocks` | 0.85 |
| `blockrunner:slightly_quick_blocks` | 1.15 |
| `blockrunner:quick_blocks` | 1.35 |
| `blockrunner:very_quick_blocks` | 1.55 |

A data pack adds a block to a tag (`data/blockrunner/tags/block/slow_blocks.json`), or gives any block its own speed, from 0 to 8, in `data/blockrunner/data_maps/block/block_speeds.json`:

```json
{ "values": { "minecraft:gravel": { "speed": 0.8 } } }
```

`/reload` applies a change, and the server sends the new speeds to its players.

## Settings

In the mod's settings screen:

- **Block Speed Tooltip** (on): show a block's speed in its item tooltip.
- **Steady Field of View** (on): keep the field of view from widening while running on a faster block.

## Differences from the original

- On the client, the speed reaches the player after the server's next movement-speed update, where the original also sets it on the player's own side at once. Sunrise lets a mod change the player's attributes only on the server ([sunrise-org/sunrise#5343](https://github.com/sunrise-org/sunrise/issues/5343)).
- With Steady Field of View on, the port takes the field-of-view effect scale from the value the game passes to mods, as the game gives mods no separate scale ([sunrise-org/sunrise#5344](https://github.com/sunrise-org/sunrise/issues/5344)). The result is the original's, except on the rare tick where the field of view before scaling is exactly 1.0, when the block speed's widening stays.
- The server sends its speed table to players through the mod's own channel, since Sunrise doesn't yet send data maps to clients ([sunrise-org/sunrise#1097](https://github.com/sunrise-org/sunrise/issues/1097)).
- A block with a speed ignores the speed factor of soul sand and honey blocks only. A block from another mod with its own speed factor keeps slowing or speeding the entity on top of its block speed, since a mod can't change the speed factor of every block ([sunrise-org/sunrise#5345](https://github.com/sunrise-org/sunrise/issues/5345)).

## Building

With the Sunrise mod tool (`cargo sunrise-mod`, from the Sunrise SDK) and Rust:

```sh
cargo sunrise-mod check
cargo sunrise-mod build    # target/sunrise-mod/blockrunner.smod
```

`sunrise-mod.toml` pins the Sunrise build this version is made for. The mod is needed on both the server and the client.

## Licence and credits

Block Runner is by Fuzs, under the Mozilla Public License 2.0; `LICENSE` holds its text. This port contains no code or assets from the original and is distributed under the same licence. The icon is the port's own.
