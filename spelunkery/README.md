# Spelunkery

Ores in granite, andesite, diorite and tuff, rough gems and raw nuggets, cave mushrooms, rock salt, nephrite, glowsticks, rope ladders and mining tools.

A port of [Spelunkery](https://github.com/AstralOrdana/Spelunkery) 0.3.16 by Ordana and Keybounce to Sunrise (Dev 1), under the original's licence (LGPL-3.0-only, in `LICENSE`; see `NOTICE`).

- `data/spelunkery/sunrise/`: block, item, fluid and creative tab declarations. The rest of `data/` and `assets/` are the original's loot tables, recipes, tags, world generation, models, textures and sounds in 26.3's formats.
- `src/lib.rs`: the rules, as `BlockPart`s on every block and `ItemPart`s on the glowsticks, depth gauge and item magnet, plus the parachute and thrown-glowstick ticks.
- `src/classes.rs`, `src/vanilla_loot.rs`: each block's original Java class and the vanilla loot tables the original replaced.

Not ported yet: the water channels and sluices, compression blast miner, mineomite, nephrite diode, fountain and siphon, sulfur geyser, portal fluid cauldron, falling sulfur and saltpeter, dust bunnies, and several tools (compass, tuning forks, hammer and chisel, handheld compactor, nephrite charm). They exist as blocks and items without behaviour.

Build with `cargo sunrise-mod check`.
