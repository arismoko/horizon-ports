# Better Ping Display

A Sunrise 26.3 client-only port of [Better Ping Display](https://github.com/vladmarica/better-ping-display-fabric) (1.2.0) by Vlad Marica (Quintinity) (MIT).

## What it does

- **Numeric ping in the player list**: renders each player's ping in milliseconds (formatted with `%dms` by default) in the Tab player list, expanding each slot by 45 pixels so the readout fits cleanly beside player names and scoreboard objectives.
- **Auto-colouring by latency**: smoothly interpolates ping text colour from green (`0 ms`, `#00E676`) through yellow (`150 ms`, `#D6CD30`) to red (`300+ ms`, `#E53935`), and uses grey (`#535353`) when latency is unknown (`< 0 ms`).
- **Custom static colour**: disable **Auto-color Ping Text** to render all ping values in a chosen RGB colour (default `#A0A0A0`), configurable in **Options → Mods → Better Ping Display** or `config/betterpingdisplay.toml`.
- **Optional vanilla ping bars**: enable **Render Ping Bars** (default off) to display the vanilla signal-strength bars beside the numeric ping text.
- **Custom format string**: configure `ping_text_format_string` in `config/betterpingdisplay.toml` (any format string containing `%d`, hot-reloaded on save).
- **Vanilla Tab list fidelity**: keeps headers, footers, multi-column layout (up to 80 players), player heads, team-formatted names, spectator styling, `list` scoreboard integer scores and animated blinking hearts, F1 / screen / singleplayer visibility rules, and narration.

Client-side only: works on any multiplayer server and in LAN worlds.

## Differences from the original

- Because the mod API does not yet expose a per-entry player-list ping render hook, this port wraps and replaces the `minecraft:tab_list` HUD layer; `ClientTabList` does not expose `online_mode` or per-player hat visibility, so player heads are always drawn with the hat layer enabled (gap: TBD-tab-list-ping-hook).
- **Ping Text Format String** is edited in `config/betterpingdisplay.toml` (hot-reloaded on save) rather than in **Options → Mods → Better Ping Display**, and **Ping Text Color** stays interactive while **Auto-color Ping Text** is on, because Sunrise's mod settings UI does not yet have a string input or conditional row disabling (gap: TBD-mod-settings-string).

## Configuration

- In-game settings: **Options → Mods → Better Ping Display** (`Auto-color Ping Text`, `Render Ping Bars`, `Ping Text Color`)
- Full config file (including `ping_text_format_string`): `config/betterpingdisplay.toml` (hot-reloaded automatically when edited)

## Building

```sh
cargo sunrise-mod check
cargo sunrise-mod build
CARGO_TARGET_DIR=target cargo test
cargo clippy --all-targets -- -D warnings
```

## Licence

MIT, matching the original project. See `LICENSE` and `NOTICE`.
