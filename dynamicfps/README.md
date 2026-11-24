# Dynamic FPS

A Sunrise 26.3 client-only port of [Dynamic FPS](https://github.com/juliand665/Dynamic-FPS) (3.11.10) by juliand665 and LostLuma (MIT).

## What it does

- **Per-state frame rate caps**: lowers the window's frame rate when **Unfocused** (1 FPS default), **Invisible / Minimized** (0 / 1 FPS default), **Idle** (10 FPS default), **On Battery** (unlimited default), or **Hovered** (60 FPS default). A lower frame rate cap is never raised, and one frame renders immediately when entering a low-FPS state so HUD text appears without delay.
- **Volume multipliers and smooth transitions**: scales Master and per-`SoundSource` (`music`, `records`, `weather`, `blocks`, `hostile`, `neutral`, `players`, `ambient`, `voice`, `ui`) volumes per state, cancels newly started sounds at 0%, and smoothly fades volume at configurable upward and downward speeds.
- **Toggle keybindings and HUD banner**: registers **Force Unfocused Mode (Toggle)** and **Disable Dynamic FPS (Toggle)** in **Options → Controls**, drawing a centered status banner on the HUD while active.
- **Idle detection**: switches to the **Idle** (`abandoned`) state after a configurable timeout without keyboard, mouse, cursor, camera, or screen activity (always, or only while on battery).
- **Ignore initial click**: discards accidental mouse clicks during the first 10 ms after refocusing the window (**Disabled**, **In World**, or **Constant**).
- **Battery integration**: reads system batteries on Linux (`/sys/class/power_supply`, skipping peripheral/device batteries; mock battery mode available on all platforms), switches to **On Battery** when discharging, draws a corner HUD battery icon and percentage, and displays charging, discharging, and low-battery toast notifications.
- **Toast suppression**: blocks incoming toasts in states with **Show Toasts** off and holds Dynamic FPS's own battery toasts until a state with toasts enabled is active.
- **F3 debug entry**: registers `dynamicfps:power_state` in debug options to show the active power state and target frame rate.
- **Options screen and hot-reloaded config**: core settings are editable in **Options → Mods → Dynamic FPS**; full per-source volume multipliers live in `config/dynamicfps.toml` and hot-reload on save.

Client-side only: works in single player and on any multiplayer server.

## Differences from the original

- Pointer hover over an unfocused window (`Hovered`) and non-minimized window occlusion (`Invisible` while occluded) are not detected yet (gap: [sunrise-org/sunrise#5549](https://github.com/sunrise-org/sunrise/issues/5549)).
- The frame rate hook can only lower the engine's cap and ignores values below 1, so a target of `0` FPS (default for Invisible) renders at 1 FPS instead of skipping frames completely, and **Uncap Menu FPS** / bypassing vanilla's built-in idle throttle cannot raise the cap (gap: [sunrise-org/sunrise#5550](https://github.com/sunrise-org/sunrise/issues/5550)).
- Volume multipliers apply to newly started sounds and ticking sounds; already-playing non-ticking channels (such as active music or jukebox records) are not updated or paused on state transitions (gap: [sunrise-org/sunrise#5551](https://github.com/sunrise-org/sunrise/issues/5551)).
- When **Show Toasts** is off for a state, incoming toasts are cancelled instead of paused until focus returns; Dynamic FPS's own battery toasts are delayed as in the original (gap: [sunrise-org/sunrise#5552](https://github.com/sunrise-org/sunrise/issues/5552)).
- Per-state VSync and **Graphics Options** (`Reduced` / `Minimal`) are preserved in `config/dynamicfps.toml` but cannot override video options at runtime yet (gap: [sunrise-org/sunrise#5553](https://github.com/sunrise-org/sunrise/issues/5553)).
- F3 debug status is registered as its own `dynamicfps:power_state` entry instead of appending inline to vanilla's FPS line (gap: [sunrise-org/sunrise#5554](https://github.com/sunrise-org/sunrise/issues/5554)).
- World rendering behind an unfinished resource-reload overlay is not skipped (gap: [sunrise-org/sunrise#5555](https://github.com/sunrise-org/sunrise/issues/5555)).
- In-game configuration uses Sunrise's flat **Options → Mods → Dynamic FPS** page rather than a tabbed Cloth Config screen; per-category sound multipliers are edited in `config/dynamicfps.toml` (gap: [sunrise-org/sunrise#1217](https://github.com/sunrise-org/sunrise/issues/1217)).
- `run_garbage_collector` does not apply because Sunrise is native Rust with no JVM garbage collector.
- Real hardware battery polling uses Linux `/sys/class/power_supply`; on other platforms, **Use Mock Battery Data** is available.

## Configuration

- In-game settings: **Options → Mods → Dynamic FPS**
- Full config file (including per-`SoundSource` volume multipliers per state): `config/dynamicfps.toml` (hot-reloaded automatically when edited)

## Building

```sh
cargo sunrise-mod check
cargo sunrise-mod build
CARGO_TARGET_DIR=target cargo test
cargo clippy --all-targets -- -D warnings
```

## Licence

MIT, matching the original project. See `LICENSE` and `NOTICE`.
