# InvMove for Sunrise

Walk, jump, sneak and sprint while an inventory or other screen is open. A port of [InvMove](https://github.com/PieKing1215/InvMove) 0.9.6 by PieKing1215 to Sunrise's typed mod API. Client side only.

## What it does

- Movement keys keep working inside inventories, containers, the creative menu, trades, books, advancements and any other screen you allow.
- Sneak modes (Off, Maintain, Pressed, Maintain While Pressed) for allowed screens, disallowed screens and flying; optional dismounting; toggle-sneak and toggle-sprint are honoured.
- Typing in a text box stops movement; the recipe book can too.
- Per-screen movement and background switches for 23 vanilla screens, a default for unrecognized screens, a "toggle movement" key (unbound by default), and a switch per movement key.
- Hides the dark background tint behind screens, with pause-screen modes.
- Space/Enter bound to jump no longer clicks the focused button.
- Its own config screen (Options → Mods → InvMove → Config) with tabs for General, Movement, Keys, Background, per-screen movement and background, and the lists of keys and screens the config file holds.

## Differences from the original

Each is a limit of the mod API at the pinned Sunrise commit.

- Text box focus is guessed from typing (gap: sunrise-org/sunrise#5655).
- Other gameplay keys only update their key state inside screens (vanilla and NeoForge have no such moment).
- Space/Enter is cancelled across the whole screen, not only for focused buttons, whenever jump is allowed and bound to it: the focused widget can't be seen, so every button, and any other use of those keys in that screen, is skipped while jump is bound to Space or Enter (gap: sunrise-org/sunrise#5656).
- Not every screen's background can be hidden (gap: sunrise-org/sunrise#5657).
- Other mods can allow or veto movement in a screen through the typed `invmove:screen_movement` event (`ScreenMovementEvent`, posted on the client; a listener edits `allowed`). InvMove's own config screen is never offered. The original's addon module API is replaced by this event rather than ported as is.
- The Debug Display option is omitted: it is a no-op on 26.x in the original.

## Configuration

Options → Mods → InvMove → Config opens InvMove's own screen with the switches. Press a row to flip it (right click cycles a choice backwards); the Keys tab lists the extra keys, where a press removes one and the field under the rows adds one (a vanilla key name such as `key.attack`); the Unrecognized tab lists the screens recorded as you open them, where a press flips their movement or background choice. `config/invmove.toml` holds the same lists as `[keys] extra = ["key.attack"]` and the tables `[unrecognized_movement]` and `[unrecognized_background]`, and reloads when saved.

## Building

```
cargo sunrise-mod build
cargo test
```

## Licence

LGPL-3.0-only; see `LICENSE`, `COPYING` and `COPYING.LESSER`, and `NOTICE`.
