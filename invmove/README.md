# InvMove for Sunrise

Walk, jump, sneak and sprint while an inventory or other screen is open. A port of [InvMove](https://github.com/PieKing1215/InvMove) 0.9.6 by PieKing1215 to Sunrise's typed mod API. Client side only.

## What it does

- Movement keys keep working inside inventories, containers, the creative menu, trades, books, advancements and any other screen you allow.
- Sneak modes (Off, Maintain, Pressed, Maintain While Pressed) for allowed screens, disallowed screens and flying; optional dismounting; toggle-sneak and toggle-sprint are honoured.
- Typing in a text box stops movement; the recipe book can too.
- Per-screen movement and background switches for 23 vanilla screens, a default for unrecognized screens, a "toggle movement" key (unbound by default), and a switch per movement key.
- Hides the dark background tint behind screens, with pause-screen modes.
- Space/Enter bound to jump no longer clicks the focused button.

## Differences from the original

Each is a limit of the mod API at the pinned Sunrise commit.

- Text box focus is guessed from typing (gap: sunrise-org/sunrise#5655).
- Other gameplay keys only update their key state inside screens (vanilla and NeoForge have no such moment).
- The settings page is flat; lists live in `config/invmove.toml` (gap: sunrise-org/sunrise#1217, fixed on main, in the next Dev build).
- Space/Enter is cancelled for buttons whenever jump is allowed and bound to it (gap: sunrise-org/sunrise#5656).
- Not every screen's background can be hidden (gap: sunrise-org/sunrise#5657).
- The addon module API is not ported (gap: sunrise-org/sunrise#5137).
- The Debug Display option is omitted: it is a no-op on 26.x in the original.

## Configuration

Options → Mods → InvMove holds the switches. `config/invmove.toml` holds `[keys] extra = ["key.attack"]` and the per-screen tables `[unrecognized_movement]` and `[unrecognized_background]`, which fill in as you open screens. It reloads when saved.

## Building

```
cargo sunrise-mod build
cargo test
```

## Licence

LGPL-3.0-only; see `LICENSE`, `COPYING` and `COPYING.LESSER`, and `NOTICE`.
