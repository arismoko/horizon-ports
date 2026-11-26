# Dynamic FPS

A Sunrise 26.3 client-only port of [Dynamic FPS](https://github.com/juliand665/Dynamic-FPS) (3.11.10) by juliand665 and LostLuma (MIT).

## What it does

- **Per-state frame rate caps**: lowers the window's frame rate when **Unfocused** (1 FPS default), **Invisible / Minimized** (0 / 1 FPS default), **Idle** (10 FPS default), **On Battery** (unlimited default), or **Hovered** (60 FPS default). A lower frame rate cap is never raised, and one frame renders immediately when entering a low-FPS state so HUD text appears without delay.
- **Volume multipliers and smooth transitions**: scales Master and per-`SoundSource` (`music`, `records`, `weather`, `blocks`, `hostile`, `neutral`, `players`, `ambient`, `voice`, `ui`) volumes per state, cancels newly started sounds at 0%, scales sounds already playing (music, records) live, pauses every sound once Master reaches 0%, and smoothly fades volume at configurable upward and downward speeds.
- **Toggle keybindings and HUD banner**: registers **Force Unfocused Mode (Toggle)** and **Disable Dynamic FPS (Toggle)** in **Options → Controls**, drawing a centered status banner on the HUD while active.
- **Idle detection**: switches to the **Idle** (`abandoned`) state after a configurable timeout without keyboard, mouse, cursor, camera, or screen activity (always, or only while on battery).
- **Ignore initial click**: discards accidental mouse clicks during the first 10 ms after refocusing the window (**Disabled**, **In World**, or **Constant**).
- **Battery integration**: reads system batteries on Linux (`/sys/class/power_supply`, skipping peripheral/device batteries; mock battery mode available on all platforms), switches to **On Battery** when discharging, draws a corner HUD battery icon and percentage, and displays charging, discharging, and low-battery toast notifications.
- **Held toasts**: in states with **Show Toasts** off, incoming toasts (and Dynamic FPS's own battery toasts) wait in the toast queue and show once a state with toasts enabled is active.
- **Hover and occlusion detection**: an unfocused window under the pointer uses the **Hovered** state, and a fully hidden (occluded) window uses **Invisible**, as the window system reports them (some systems report only minimizing).
- **Per-state VSync and graphics options**: each state can force VSync on or off and apply a **Graphics Options** preset (`Reduced`: fast clouds; `Minimal`: no clouds and no view bobbing) for the session only; `options.txt` is never written.
- **F3 FPS line**: appends the active power state and target frame rate to the end of F3's frame rate line while not focused.
- **Config screen and hot-reloaded config**: **Mods → Dynamic FPS → Config** opens Dynamic FPS's own paged screen (General, Unfocused, Minimized, Idle, Hovered, Battery, Battery HUD) with toggles, choices and sliders; full per-source volume multipliers live in `config/dynamicfps.toml` and hot-reload on save.

Client-side only: works in single player and on any multiplayer server.

## Differences from the original

- The frame rate hook can only lower the engine's cap and ignores values below 1, so a target of `0` FPS (default for Invisible) renders at 1 FPS instead of skipping frames completely, and **Uncap Menu FPS** / bypassing vanilla's built-in idle throttle cannot raise the cap (gap: [sunrise-org/sunrise#5550](https://github.com/sunrise-org/sunrise/issues/5550)).
- World rendering behind an unfinished resource-reload overlay is not skipped (gap: [sunrise-org/sunrise#5555](https://github.com/sunrise-org/sunrise/issues/5555)).
- Per-category sound multipliers beyond Master are edited in `config/dynamicfps.toml`, not on the config screen.
- `run_garbage_collector` does not apply because Sunrise is native Rust with no JVM garbage collector.
- Real hardware battery polling uses Linux `/sys/class/power_supply`; on other platforms, **Use Mock Battery Data** is available.

## Configuration

- In-game settings: **Mods → Dynamic FPS → Config** (click `<` / `>` or press Left / Right to change page; click, drag or scroll a slider)
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
