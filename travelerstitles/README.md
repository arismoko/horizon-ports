# Traveler's Titles

A Sunrise 26.3 client-only port of [Traveler's Titles](https://github.com/YUNG-GANG/Travelers-Titles) (26.1.2 branch, 6.1.0) by YUNG (LGPL-3.0-only).

## What it does

- **Biome titles**: a large title with the biome's name fades in and out when you enter a new biome. A cooldown (80 ticks) and a list of the 5 most recent biomes stop spam when you move between the same few biomes; the list is cleared when you change dimensions. The name is looked up as `travelerstitles.biome.<namespace>.<path>` first, then vanilla's `biome.<namespace>.<path>`, and its colour as the same key plus `.color`. Biomes with no name in the language are skipped.
- **Dimension titles**: a title with the dimension's name (`travelerstitles.<namespace>.<path>`, `???` when unknown) and colour when you enter a dimension, once per dimension type until you leave it.
- **Skylight rule**: biome titles only show when you can see the sky in dimensions that have a sky (so caves do not spam titles), except in `minecraft:lush_caves`, `minecraft:dripstone_caves` and the biomes of the `travelerstitles:is_underground` tag. The same rule is available for dimension titles (off by default).
- **Appearance**: fade-in, display and fade-out times in ticks, colour, scale, text shadow, X and Y offsets, and centring on the screen. Titles are hidden while the F3 debug overlay is open.
- **Sounds**: separate entry sounds for biomes and dimensions with their own volume and pitch. The dimension sound replaces the biome sound when both start together.
- **Blacklists**: biomes and dimensions that never show a title (`minecraft:the_end`, `minecraft:river` and `minecraft:frozen_river` by default).
- **Commands**: `/biometitle <biome>` and `/dimensiontitle <dimension>` show a title as a preview, and `/tt_reload` clears the running titles and recent lists.
- **Translations**: the original's English, Japanese, Russian, Ukrainian and Chinese files, with about 470 dimension and modded-biome names and colours.
- **Settings**: everything but the two blacklists is on **Options → Mods → Traveler's Titles**; the blacklists are in `config/travelerstitles.toml` and reload when saved.

Client-side only: works in single player and on any multiplayer server.

## Differences from the original

- Waystone titles (the title for the nearest named waystone, its sound, cache, range, cooldown and "override biome title" options) are not available: they need the list of waystones another mod sends to the client, and mods cannot yet listen to another mod's events from the original's side (gap: sunrise-org/sunrise#5137).
- The biome and dimension blacklists are edited in `config/travelerstitles.toml` because Options → Mods has no text or list setting (gap: sunrise-org/sunrise#1174). A mod's own config screen (`register_config_screen`) would replace the whole settings page rather than add to it, so it is not used for the lists alone. Everything else is on the Options → Mods page, not a separate config file, and the option labels there are English only.
- The optional text backdrop (drawn only when the player's Text Background option is not limited to chat) is not drawn, because mods cannot read that option (gap: sunrise-org/sunrise#5653).
- `/biometitle`, `/dimensiontitle` and `/tt_reload` are client commands with plain `namespace:path` arguments: they need no operator permission, give no suggestions and take no biome tags (gap: sunrise-org/sunrise#5654). `/tt_reload` does not re-read settings, which always apply at once, and the blacklist file reloads on save.

## Configuration

- In-game settings: **Options → Mods → Traveler's Titles**
- Blacklists: `config/travelerstitles.toml` (`blacklisted_biomes`, `blacklisted_dimensions`)

## Building

```sh
cargo sunrise-mod check
cargo sunrise-mod build
CARGO_TARGET_DIR=target cargo test
cargo clippy --all-targets -- -D warnings
```

## Licence

LGPL-3.0-only, matching the original project. See `LICENSE`, `COPYING`, `COPYING.LESSER` and `NOTICE`.
