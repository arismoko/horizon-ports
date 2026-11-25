# Mythic Upgrades for Sunrise

Eight gems and necoium with their ores and crystal geodes, and armour sets and tools that each have their own effect.

- **Ores and geodes**: gem ores drop experience like vanilla ores, and budding crystals grow their own buds, from small to large to a cluster, on a random side.
- **Armour sets**: each worn piece adds levels to its gem's effect: Damage Deflection (sapphire), Arcane Aura (ametrine), Topaz Reaction, Blood Thirst (ruby), Miasma (peridot), Ice Shield (aquamarine), Static Field (citrine) and Jade Aura.
- **Tools**: each gem's sword, pickaxe, axe, shovel and hoe has its own effect.
- **Necoium Share**: holders within 30 blocks share their gem effects.
- **Smithing and loot**: gear is made at the smithing table with the Mythic upgrade template, which can be found in end city treasure.

A port of [Mythic Upgrades](https://github.com/TriQue06/mythic-upgrades-multiloader/tree/1.20.1) 5.1.1 by TriQue to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), written in Rust on Sunrise's mod API (no native rows and no mixins), under the original's licence (MIT, in `LICENSE`; see `NOTICE`). The 26.3 carry is by lavoide.

## Layout

- `src/lib.rs`: the rules (ore experience, budding growth, armour and tool effects, end city loot).
- `src/classes.rs`: the block list the rules attach to.
- `data/mythicupgrades/sunrise/`: blocks, items, effects and creative tabs; `data/` otherwise holds recipes, loot tables, tags, trims and worldgen.

## Differences from the original

- Hits of the mod's own damage types fall back to vanilla `magic` or `generic` damage when the host refuses a content damage type.
- Effects the hooks cannot read from the host are cached per tick, so a reaction can lag a tick behind the original.
- Not yet checked in game on Dev 1.
