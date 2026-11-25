# Cut Through

A Sunrise 26.3 client-only port of [Cut Through](https://github.com/Fuzss/cut-through) (26.3.0) by Fuzs (MPL-2.0).

## What it does

- **Attacks and use pass through plants**: blocks without a collision shape (tall grass, flowers, ferns, vines, torches and the like) no longer stop the crosshair. A mob standing behind one is targeted instead, so you can swing at it without breaking the plant. Solid blocks still block the pick, and reach and line of sight follow vanilla's rules (the server checks are unchanged).
- **Target Alive Only** (default on): intended to ignore mobs that already died and only remain for their death animation, and to suspend block mining for 5 ticks after hitting a mob, so a held attack does not break the plant once the mob is gone.
- Client-side only: works in single player and on any multiplayer server.

## Differences from the original

- Targeting alive entities only cannot be enforced: the mod API reads liveness only on the server and for the local player, so a mob in its death animation can still be targeted and absorb hits. **Target Alive Only** therefore only controls the mining delay below (gap: TBD-alive-targets).
- After you attack a mob, vanilla's block-mining delay is not reset by this mod; instead held attacks on a block are cancelled for 5 ticks after an entity attack, which has the same effect but also suppresses a swing (gap: TBD-destroy-delay).
- Collision shapes along the pick ray are read with an empty collision context, so blocks whose shape depends on the player (such as powder snow or scaffolding) can differ from the original's `ClipContext` (gap: TBD-collision-context).
- Settings are one toggle in **Options → Mods → Cut Through** and `config/cutthrough-client.toml`; the original's Puzzles Lib config screen and `cutthrough-client.toml` layout are not reproduced. The original ships no translations, so only `en_us` is provided.

## Configuration

- In-game setting: **Options → Mods → Cut Through**
- Config file: `config/cutthrough-client.toml` (hot-reloaded when edited)

## Building

```sh
cargo sunrise-mod check
cargo sunrise-mod build
CARGO_TARGET_DIR=target cargo test
cargo clippy --all-targets -- -D warnings
```

## Licence

MPL-2.0, matching the original project. The original's assets are All Rights Reserved and are not included. See `LICENSE` and `NOTICE`.
