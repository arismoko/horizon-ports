# SafeSpawn for Sunrise

A player joining a world for the first time while standing on water, lava, magma, fire, wool or leaves is moved to the nearest safe ground within 128 blocks, and their spawn point with them.

- Safe ground is the surface of a column standing on a solid full block that is not on the blocked list, with room for a player above.
- The search walks square rings outward from the player, loading the chunks it crosses.
- Each player is checked once, on their first join; the mark is kept in their saved data.
- The blocked list is `config/safespawn/blocked_blocks.txt`: one block id or `#tag` per line.

A port of SafeSpawn 1.0.0 by Tokimi, CozyStudios to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), written in Rust on Sunrise's mod API, under the original's licence (CC0-1.0, in `LICENSE`; see `NOTICE`).

Differences from the original:

- The settings are a plain list file; the original's own settings file format and defaults were not available to compare.
