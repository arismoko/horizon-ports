# Ping Wheel

A Sunrise 26.3 port of [Ping Wheel](https://github.com/LukenSkyne/Minecraft-Ping-Wheel) (1.12.2, 26.3 branch) by Luken (LukenSkyne) and contributor RXJpaw (MIT).

## What it does

- **Ping locations and entities**: press **Ping Location** (`Mouse 5` by default) to mark the block or entity you are looking at up to 1,024 blocks away. When bound to the same key as **Pick Item**, it only pings on a miss (or on an entity outside Creative mode).
- **Correction period and quick removal**: re-pinging within the configurable **Correction Period** (1.0s default) moves your previous ping instead of creating a new one; pinging while aiming at an existing ping removes it.
- **HUD markers and distance scaling**: projects pings onto the HUD with formatted distance labels, distance-based scaling, back-to-front depth sorting, and per-dimension filtering.
- **Dropped item icons and entity tracking**: pings on moving entities follow the top of the entity's bounding box; pings on dropped items show the item's icon when **Item Icons** is enabled.
- **Player info and team colors**: display the pinging player's head and name in **Hold** (hold `Tab` / Player List), **Always**, **Compact** (head beside distance), or **Disabled** mode, with configurable **Team Color** tinting (**Full**, **Ping Only**, **Labels Only**, **Disabled**).
- **Custom ping icon**: a resource pack can replace the marker icon with `assets/pingwheel/textures/ping.png`, drawn directly as a standalone texture.
- **Off-screen direction indicators**: pings outside your field of view or behind the camera clamp to the configurable screen safe zone with a rotated directional arrow pointing toward them.
- **Directional ping audio**: plays `pingwheel:ping` as a 3D directional sound whose position tracks the direction from the player toward the ping as you move.
- **Channels, vanilla teams, and server controls**: isolate pings by named channel (saved per server address) or vanilla scoreboard team (`auto`, `disabled`, `global`, `team_only`), toggle player tracking, and enforce a configurable token-bucket rate limit on the server.
- **Settings screen and Options → Mods**: configure all options in-game via the **Open Settings** keybind, `/pingwheel config`, **Options → Mods → Ping Wheel**, or `config/pingwheel-client.toml` and `config/pingwheel-server.toml`.
- **9 languages**: includes all 9 upstream translations (`de_de`, `en_us`, `es_ar`, `fr_fr`, `pl_pl`, `tr_tr`, `uk_ua`, `zh_cn`, `zh_tw`).

## Differences from the original

- Block raycasting uses `ClipBlock::Collider` when standing and `ClipBlock::Outline` when sneaking without fluid clipping, because `ClipContext.Block.VISUAL` and `ClipContext.Fluid.ANY` are not exposed on `LevelRef::clip` yet (gap: sunrise-org/sunrise#4677, sunrise-org/sunrise#4630).
- The server administration command is `/pingwheel-server` instead of `/pingwheel:server`, because Sunrise's command literal parser does not allow `:` in command root names yet (gap: sunrise-org/sunrise#4585).
- Client-side entity raycasting does not locally filter out spectator entities because `EntityRef::spectator` is server-only (gap: sunrise-org/sunrise#5613).
- Optional third-party integrations for FTB Teams, Simple Voice Chat, and Distant Horizons are omitted; vanilla scoreboard teams work in full.

## Commands

- Client: `/pingwheel help`, `/pingwheel config`, `/pingwheel channel`, `/pingwheel channel <channel_name>`
- Server (operator level 2): `/pingwheel-server help`, `/pingwheel-server default_channel [mode]`, `/pingwheel-server player_tracking [true|false]`, `/pingwheel-server regen_time [ms]`, `/pingwheel-server rate_limit [limit]`

## Configuration

- In-game settings screen: **Open Settings** keybind, `/pingwheel config`, or **Options → Mods → Ping Wheel**
- Client config file: `config/pingwheel-client.toml` (includes per-server channels and safe-zone / raycast options)
- Server config file: `config/pingwheel-server.toml`

## Building

```sh
cargo sunrise-mod check
cargo sunrise-mod build
CARGO_TARGET_DIR=target cargo test
cargo clippy --all-targets -- -D warnings
```

## Licence

MIT, matching the original project. See `LICENSE` and `NOTICE`.
