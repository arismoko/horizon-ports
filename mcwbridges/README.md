# Macaw's Bridges

A Sunrise 26.3 port of Macaw's Bridges (3.1.2) by Sketch Macaw and Sketch Peachy.

## External original jar

Macaw's Bridges is All Rights Reserved. **None of the original mod's assets or data are included in this repository or package.** Instead, `sunrise-mod.toml` declares the exact original 26.3 NeoForge jar under `[[jar]]`:

- Textures, models, blockstates, item definitions, translations, recipes, loot tables, tags, and recipe advancements load directly from the player's own copy of the original jar (which the launcher fetches from Modrinth).
- This mod supplies only the Sunrise block/item/creative-tab declarations (`data/mcwbridges/sunrise/`) and the Rust behaviour on Sunrise's typed mod API (`src/`).

## What it does

- **157 blocks and 158 items** matching the 3.1.2 NeoForge release, organised in the **Macaw's Bridges** creative tab:
  - **Wooden log bridges** (12 woods including pale oak and poplar, plus Asian red and glass bridges)
  - **Rope bridges** (12 woods) and **bamboo / dry bamboo bridges**
  - **Stone & balustrade bridges** (17 stone materials × 2 styles)
  - **Iron bridge**
  - **Rail bridges** (12 woods) with right-click redstone torch powering (signal 10) and torch drop on empty-hand removal or block break
  - **Bridge stairs** (46 variants across iron, log, rope, stone, bamboo, Asian red, and glass)
  - **Bridge piers / supports** (34 variants, waterloggable, stack upward on right-click)
  - **Bridge torch & bridge lantern** (light level 15, auto-orienting on bridges and stairs, smoke/flame particles on torches)
  - **Pliers** (and vanilla shears) to cycle bridge connection/entrance shapes
- **Auto-connecting bridge geometry**: bridges connect to matching neighbours, extend forward when right-clicked with the same bridge item, refuse placement directly atop another bridge/stair, and cycle entrance shapes with Pliers or Shears (preserving toggled rope-bridge ends across neighbour updates).
- **Furnace fuel & tooltips**: wooden bridge blocks burn for 300 ticks, and bridge, light, and pliers items carry their translated tooltip lines.

## Differences from the original

- **Mirroring rope bridges in structure templates (`Bridge_Block_Rope.mirror`)**: Sunrise's typed mod API exposes `BlockPart::rotate` (`IBlockExtension.rotate`) for structure template placement, but has no `BlockPart::mirror` hook for custom `BlockBehaviour.mirror` overrides (gap: pending issue). Structure rotation of rope bridges, rail bridges, torches, and lanterns is implemented via `BlockPart::rotate`.
- **Same-tick `onPlace` connection refresh after `set_block`**: in vanilla, `Bridge_Block.onPlace` and `Bridge_Block_Rope.onPlace` immediately recompute `connection` whenever a block is placed via `setBlock` (including when extending a bridge by right-clicking an existing bridge). Our right-click extension computes the neighbour connection directly before calling `set_block`, and normal player/dispenser placement uses `state_for_placement`; external `/setblock` placement without neighbour updates relies on `update_shape` because Sunrise has no writable `BlockPart::on_place` hook (gap: pending issue).

## Building

```sh
cargo sunrise-mod use-jar /path/to/mcw-bridges-3.1.2-mc26.3neoforge.jar
cargo sunrise-mod check
cargo sunrise-mod build
cargo test
```

## Licence

All code, declarations, and the icon in this repository are MIT licensed (see `LICENSE` and `NOTICE`). No All-Rights-Reserved files from the original mod are included.
