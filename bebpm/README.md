# BEBPM

Better Enchantment Bookshelf Placement: enchanting tables count bookshelves further away.

A port of [BEBPM](https://www.curseforge.com/projects/1462757) by CozyStudios, under the original's licence (CC0-1.0, in `LICENSE`; see `NOTICE`).

## What it does

- The table scans a box around it instead of vanilla's ring. The default radius is 10 in every direction; X, Z, up and down are each set from 0 to 16 in Options → Mods (never below vanilla's ring: 2 sideways, 1 up). The menu still caps the power at 15 bookshelves.
- Line of sight, in three modes:
  - **Off**: every power-giving block in the box counts.
  - **Decor only** (default): a full-collision block between the table and the shelf blocks it, unless it is in the `#bebpm:power_permeable` tag (leaves, fences, walls, doors, trapdoors, glass, iron bars; override the tag with a data pack).
  - **On**: every block on the line must be in `#minecraft:enchantment_power_transmitter`.
- The table's particles come from every shelf that counts.

Install on both client and server: the server decides the offer, the client draws the particles. Settings and the scan box are fixed at startup, so a change needs a restart.

## Differences from the original

- Settings are in Options → Mods rather than a Cloth Config screen and `bebpm.json`; the original's list of extra blocking blocks is the `#bebpm:power_permeable` tag here,.
- The line of sight walks the blocks from table to shelf in steps along the line; the original's exact sampling was not available, so edge cases at diagonals may differ.
