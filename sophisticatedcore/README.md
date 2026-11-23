# Sophisticated Backpacks and Storage for Sunrise

Tiered backpacks, chests, barrels and shulker boxes whose contents travel with them, with upgrades that work while you carry them.

A port of [Sophisticated Backpacks](https://github.com/Salandora/SophisticatedBackpacks), [Sophisticated Storage](https://github.com/Salandora/SophisticatedStorage) and [Sophisticated Core](https://github.com/Salandora/SophisticatedCore) by P3pp3rF1y, Ridanisaurus and Salandora to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), in one package, written in Rust on Sunrise's mod API.

## What it does

- Six backpack tiers (leather to netherite) that keep their contents and upgrades in the item: open one from your hand or with the Open Backpack key (B), or place it with sneak + use and pick it back up with an empty hand.
- Sophisticated Storage's tiered chests, barrels (with limited one-to-four-slot barrels) and shulker boxes in every wood, which keep their contents when broken or packed; chests placed side by side join.
- A paged storage screen with sorting, upgrade slots, upgrade settings and a crafting grid.
- Upgrades that work while carried or placed: pickup, magnet, void, feeding, compacting, refill, restock and deposit, hopper, smelting, smoking and blasting (manual and automatic, with a furnace view), and stack upgrades that raise each slot's limit.
- Tier upgrades crafted in place keep the contents; dye backpacks and storage in the crafting grid.
- Backpacks in vanilla chest loot, as in the original (a setting turns it off).
- Keys in Controls: Open Backpack (B), Sort (middle mouse button, in a storage screen), Inventory Interaction (C, restock and deposit on the block you look at), and switches for the upgrades in slots 1 to 5 (Z and X for the first two): a switched-off upgrade does nothing.
- Settings (Options > Mods): backpack slowness for carrying too many, only the worn backpack's upgrades, chest loot, backpacks in container items, chests and barrels dropping packed, the drop count that forces packing, one wood variant in the creative tab, the magnet ranges, the button click sound and where the sort button sits.
- Monsters (zombies, skeletons, creepers and the other listed kinds) may spawn wearing a backpack, with armour and extra health; a player's kill may drop it filled with loot. Chance, loot, health, armour, drop chance and looting bonus are settings.
- Per-tier settings for slot counts, cooking speed and fuel efficiency, automatic cooking filter sizes, upgrade slots, limited-barrel multipliers and the most upgrades of each kind, and text files in `config/sophisticatedcore/` for items a backpack or shulker box refuses, blocks the interaction key skips, items that never stack, and which monsters wear a backpack. All are read while the game runs.

## Differences from the original

- The original's own screen (tabs, memory and no-sort slots) is a chest menu here; its sort and page buttons sit at the two ends of the bottom control row, and the sort button position setting swaps them or hides sort.
- The tool swapper, stonecutter, anvil, smithing, inception, everlasting and infinity upgrades sit in their slot and do nothing yet; the filter upgrade and the storage network blocks do not work yet.
- A cooking upgrade that is switched off or taken out keeps no paused progress: its burn and cook clocks run on. In an automatic one, right-click an inventory item to add it to the matching filter.
- Worn backpacks are drawn on the body in their default colours; dyed and spawn-egg colours are not drawn. A monster's potion effects are given when it joins the level; a creeper bearer loses its beneficial effects when it explodes.
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
