# Pick Up Notifier

A Sunrise 26.3 port of [Pick Up Notifier](https://github.com/Fuzss/pickupnotifier) (26.3.0) by Fuzs (MPL-2.0).

## What it does

- **HUD pick-up feed**: displays a configurable notification list whenever you collect items, experience orbs, or shot arrows, rendered on a dedicated HUD layer (`pickupnotifier:pick_up_entries`) above `minecraft:effects` and respecting F1 HUD hiding.
- **Entry combining & lifecycle**: combines repeated pick-ups according to **Combine Entries** (`ALWAYS`, `NEVER`, or `EXCLUDE_NAMED`), respects configurable **Display Time** (with `0` keeping entries until space is needed), enforces **Max Height**, and clears entries on logout, respawn, or dimension change.
- **Smooth animations**: supports **Move Out** (`NONE`, `HORIZONTAL`, `VERTICAL`, `ALL`) over **Move Out Time** ticks, **Fade Out** opacity transitions, and item pick-up pop scaling.
- **Full display customization**: 9 screen anchor positions (`TOP_LEFT` through `BOTTOM_RIGHT`), X/Y border offsets, `display_scale` (1–24), item rarity colors (or custom default color + `ignore_rarity`), `CHAT` (matching vanilla's text background opacity) / `TOOLTIP` (vanilla 9-slice tooltip frame) / `NONE` backgrounds, `OFF` / `SPRITE` (shortened `K`/`M`/`B` counts) / `TEXT` (`xN` / `Nx`) / `BOTH` count modes, `display_single_count`, `display_item_name`, `draw_sprite`, and total `inventory_count`.
- **Filtering & creative toggle**: hide specific items, `#tags`, `namespace:*` wildcards, or `!exclusions` via `hidden_items` in `config/pickupnotifier-client.toml`; optionally disable logging in creative mode (`disable_in_creative`).
- **Single-player, modded-server, and vanilla-server support**: uses optional typed server payloads (`pickupnotifier:take_item`, `pickupnotifier:take_xp`) in single player and on servers with Pick Up Notifier installed (including `partial_pick_ups` and `backpack_integration` via low-priority `item_picked_up`), and automatically falls back to client-side inventory and experience tracking on unmodded servers or when `client_only = true`.
- **In-game settings & hot-reloaded configs**: all client options are available in **Options → Mods → Pick Up Notifier** and synced with `config/pickupnotifier-client.toml`; server options live in `config/pickupnotifier-server.toml`.

Client-side only (`side = "client"`): joins any vanilla or modded multiplayer server without requiring the server to install it.

## Differences from the original

- Item sprites cannot be drawn with partial alpha transparency on the current mod API: text, backgrounds, sprite count overlays, and experience orbs fade smoothly, while item icons stay opaque until the entry fades out. The pop-in squash/stretch animation matches the vanilla hotbar (gap: sunrise-org/sunrise#5594).
- On remote servers without Pick Up Notifier installed (or with `client_only = true`), the mod API exposes the `minecraft:take_item_entity` packet id without its decoded entity/amount fields or client-level entity lookup, so client-only mode falls back to local inventory and experience diffing; this cannot distinguish individual XP orb counts when `experience_value = false`, XP fully absorbed by Mending, or creative-mode pick-ups (gap: sunrise-org/sunrise#5595).

## Configuration

- In-game settings: **Options → Mods → Pick Up Notifier**
- Client config file (including `hidden_items` list): `config/pickupnotifier-client.toml` (hot-reloaded automatically when edited)
- Server config file: `config/pickupnotifier-server.toml`

## Building

```sh
cargo sunrise-mod check
cargo sunrise-mod build
CARGO_TARGET_DIR=target cargo test
cargo clippy --all-targets -- -D warnings
```

## Licence

MPL-2.0, matching the original project. See `LICENSE` and `NOTICE`.
