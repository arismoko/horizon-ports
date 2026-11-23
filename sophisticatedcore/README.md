# Sophisticated Backpacks and Storage for Sunrise

Tiered backpacks, chests, barrels and shulker boxes whose contents travel with them, with upgrades that work while you carry them.

A port of [Sophisticated Backpacks](https://github.com/Salandora/SophisticatedBackpacks), [Sophisticated Storage](https://github.com/Salandora/SophisticatedStorage) and [Sophisticated Core](https://github.com/Salandora/SophisticatedCore) by P3pp3rF1y, Ridanisaurus and Salandora to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), in one package, written in Rust on Sunrise's mod API.

## What it does

- Six backpack tiers (leather to netherite) that keep their contents and upgrades in the item: open one from your hand or with the Open Backpack key (B), or place it with sneak + use and pick it back up with an empty hand.
- Sophisticated Storage's tiered chests, barrels (with limited one-to-four-slot barrels) and shulker boxes in every wood, which keep their contents when broken or packed; chests placed side by side join.
- A paged storage screen with sorting, upgrade slots, upgrade settings and a crafting grid.
- Upgrades that work while carried or placed: pickup, magnet, void, feeding, compacting, tool swapper (the best tool for the block you start mining, or sword for an attack, from the backpack), compression (a range of slots that holds a material and everything it compresses to, 3 to 9 slots), refill, restock and deposit, hopper (sides and input and output filters), smelting, smoking and blasting (manual and automatic, with a furnace view), and stack upgrades that raise each slot's limit.
- The filter upgrade limits what hoppers and pipes may put into a placed storage and take out of it (in, out or both; allow list, block list or what the storage already holds).
- Every filter (pickup, void, feeding, hopper, cooking and the rest) is an allow or block list matched by item, by mod or by tags (any or all of the chosen ones), and optionally by durability and components too.
- The decoration table dyes the main and accent colours of a backpack, a storage item or a paintbrush, paid in dye: put the item and red, green and blue dye in its slots, pick the colours, and take the result. A colour costs dye parts (18 for the main colour, 6 for the accent colour) split over red, green and blue by its shares; a dye is worth 24 parts and what a dye gives beyond the cost stays in the table for next time. A paintbrush is dyed for free.
- Tier upgrades crafted in place keep the contents; dye backpacks and storage in the crafting grid.
- Backpacks in vanilla chest loot, as in the original (a setting turns it off).
- Keys in Controls: Open Backpack (B), Sort (middle mouse button, in a storage screen), Inventory Interaction (C, restock and deposit on the block you look at), and switches for the upgrades in slots 1 to 5 (Z and X for the first two): a switched-off upgrade does nothing.
- Settings (Options > Mods): backpack slowness for carrying too many, only the worn backpack's upgrades, chest loot, backpacks in container items, chests and barrels dropping packed, the drop count that forces packing, one wood variant in the creative tab, the magnet ranges, the button click sound and where the sort button sits.
- Monsters (zombies, skeletons, creepers and the other listed kinds) may spawn wearing a backpack, with armour and extra health; a player's kill may drop it filled with loot. Chance, loot, health, armour, drop chance and looting bonus are settings.
- Per-tier settings for slot counts, cooking speed and fuel efficiency, automatic cooking filter sizes, upgrade slots, limited-barrel multipliers and the most upgrades of each kind, and text files in `config/sophisticatedcore/` for items a backpack or shulker box refuses, blocks the interaction key skips, items that never stack, what compressed items break into, and which monsters wear a backpack. The settings apply while the game runs; the text files are read once, when the game starts.

## Differences from the original

- The original's own screen (tabs, memory and no-sort slots) is a chest menu here; its sort and page buttons sit at the two ends of the bottom control row, and the sort button position setting swaps them or hides sort.
- The storage network blocks do not work yet.
- The decoration table's menu is a chest menu: a colour is set with a down and up button for each of its red, green and blue (left-click 16 steps, right-click 1) instead of the original's colour picker. Its material and trim slots (wood, barrel and shulker parts) and leather armour are not done: a placed storage is drawn untinted and without its materials, so there is nothing to show for them yet.
- The hopper upgrade's view lists the sides (advanced only) and, for each filter, its type and a button that opens the filter; left-click an inventory item to add it to the input filter, right-click for the output filter. The contents filter does not look at memory slots (not ported).
- The compression upgrade weighs one crafting recipe per grid where the original weighs every matching one, and a slot's shown count stops at 2147483647. It cannot go in beside a compacting upgrade, as in the original.
- The stonecutter upgrade holds an input stack and lists its stonecutting recipes, five a row and three rows a page; the smithing upgrade holds template, base and addition and shows the smithing result. Both are chest-menu views, not the original's slot tabs, and take whole stacks from your inventory.
- A dropped backpack with the everlasting upgrade can't be hurt, never despawns and rises out of water and lava; the placed one resists explosions like bedrock and no mob breaks it. It rises from the world's floor instead of falling out of it, and sparkles now and then. One per storage.
- Infinity upgrades show each filled slot as 2147483647 and never run out; only the storage menu fills an empty slot (with a stack of one), pipes and automation insert nothing, and sorting does nothing. Only players with the upgrade's permission level (2 for the admin one) can fill a slot or remove the upgrade, or break or pick up the placed backpack. No other upgrade fits beside it.
- With an inception upgrade, a backpack takes backpacks that have none; they pool their inventories with it (the upgrade's button picks whose comes first, as in the original) and the upgrades in them run (a setting turns each off). The menu shows the outer backpack's own slots only, as the original does. A backpack placed as a block does not pool or run the ones inside it.
- The anvil upgrade does nothing yet: it waits on sunrise#4592 (opening a real anvil from an item, with vanilla's result, cost and naming).
- The advanced tool swapper does not swap tools when you right-click a block or entity (stripping, shearing, flattening): the game gives mods no way to ask whether a tool works on a block (sunrise#5099). Weapons are ranked by their main-hand attack damage modifiers and the item tags `minecraft:swords` and `minecraft:axes`.
- A cooking upgrade that is switched off or taken out keeps no paused progress: its burn and cook clocks run on. In an automatic one, right-click an inventory item to add it to the matching filter.
- Worn backpacks are drawn on the body in their default colours; dyed and spawn-egg colours are not drawn. A monster's potion effects are given when it joins the level; a creeper bearer loses its beneficial effects when it explodes.
- A bucket of experience holds 1000 mB of the experience fluid and gives it up whole into a pipe or tank; an empty vanilla bucket cannot take experience from a tank yet (the game's bucket handler only knows water and lava; sunrise#5070).
- Backpacks now take vanilla shulker boxes, and shulker boxes take backpacks, as the original does; the container-item switches in settings refuse them again.
- The jukebox upgrade plays the vanilla discs only (modded and data-pack discs wait for an API to read a jukebox song); its shuffle plays each disc once per round, and "previous" goes to the disc in the slot before.

## Building

With the Sunrise mod tool (`cargo sunrise-mod`, from the Sunrise SDK) and Rust:

```sh
cargo sunrise-mod build    # compiles against the pinned game, packs target/sunrise-mod/sophisticatedcore.smod
cargo sunrise-mod run      # builds it and starts the game with it
cargo test
```

`sunrise-mod.toml` pins the Sunrise Dev build `dev-2026-11-22-7cfdf81` (game commit `7cfdf81463`). Sunrise builds native mods from source on the player's computer, against the exact game build.

## Licence and credits

Sophisticated Backpacks, Sophisticated Storage and Sophisticated Core is by P3pp3rF1y, Ridanisaurus and Salandora (https://github.com/Salandora/SophisticatedBackpacks). This port is distributed under the originals' licence, the **GNU GPL v3.0**; `LICENSE` holds the text and `NOTICE` says where the port comes from. No Java code is included: the rules are written anew in Rust, and the textures, models and data are the original mods'.

Ported to Minecraft 26.3 by lavoide ([lavoide/horizon-ports](https://github.com/lavoide/horizon-ports)), and to Sunrise by arismoko.
