# Obsidian Equipment Reworked for Sunrise

Obsidian tools, armour, a spear, horse armour and nautilus armour, between diamond and netherite.

A port of [Obsidian Equipment Reworked](https://github.com/CozyCord/Obsidian-Equipment-Reworked) 1.2 by Tokimi and CozyStudios to Sunrise (Minecraft 26.3), under the original's licence (CC0-1.0, in `LICENSE`; see `NOTICE`).

## What it adds

- Obsidian Ingot, made by blasting obsidian.
- Obsidian sword, pickaxe, shovel, axe, hoe and spear: 1800 durability, 8.7 mining speed, attack bonus 3.5, enchantability 12. They mine what netherite mines and repair with diamonds.
- Obsidian helmet, chestplate, leggings and boots: armour 3/8/6/3, toughness 2.5, knockback resistance 0.05, enchantability 12. They repair with diamonds.
- Obsidian horse armour and nautilus armour, with armour 15.
- The Obsidian Upgrade smithing template turns diamond gear into obsidian gear with an obsidian ingot. Obsidian gear upgrades to netherite with the netherite template.
- Chest loot:
  - the template in bastions, nether fortresses, mineshafts, stronghold corridors, ancient cities and ruined portals;
  - the horse armour in fortresses, bastion treasure and ruined portals;
  - the nautilus armour in shipwreck supply chests, underwater ruins and buried treasure.
- Creative tab placement after the diamond counterparts.

## Differences from the original

- Diamond gear still upgrades straight to netherite. The original replaces vanilla's netherite smithing recipes and two vanilla translations, and a Sunrise mod can't yet replace a vanilla file: [sunrise#4991](https://github.com/sunrise-org/sunrise/issues/4991).
- On the Obsidian Upgrade template, the smithing screen shows the netherite template's slot hint ("Add Netherite Ingot") and empty-slot icons. Its tooltip is the original's. Tracked as [sunrise#5308](https://github.com/sunrise-org/sunrise/issues/5308).
- Items from the older Obsidian Equipment mod (`obsidianequipment:`) aren't renamed when a world loads: [sunrise#1209](https://github.com/sunrise-org/sunrise/issues/1209).

## Building

`cargo sunrise-mod check` builds the package against the SDK that `sunrise-mod.toml` pins.
