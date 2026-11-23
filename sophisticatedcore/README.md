# Sophisticated Backpacks and Storage for Sunrise

Tiered backpacks, chests, barrels and shulker boxes whose contents travel with them, with upgrades that work while you carry them.

A port of [Sophisticated Backpacks](https://github.com/Salandora/SophisticatedBackpacks), [Sophisticated Storage](https://github.com/Salandora/SophisticatedStorage) and [Sophisticated Core](https://github.com/Salandora/SophisticatedCore) by P3pp3rF1y, Ridanisaurus and Salandora to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), in one package, written in Rust on Sunrise's mod API.

## What it does

- Six backpack tiers (leather to netherite) that keep their contents and upgrades in the item: open one from your hand or with the Open Backpack key (B), or place it with sneak + use and pick it back up with an empty hand.
- Sophisticated Storage's tiered chests, barrels (with limited one-to-four-slot barrels) and shulker boxes in every wood, which keep their contents when broken or packed; chests placed side by side join.
- A paged storage screen with sorting, upgrade slots, upgrade settings and a crafting grid.
- Upgrades that work while carried or placed: pickup, magnet, void, feeding, compacting, refill, restock and deposit, hopper, and stack upgrades that raise each slot's limit.
- Tier upgrades crafted in place keep the contents; dye backpacks and storage in the crafting grid.
- Backpacks in vanilla chest loot, as in the original (a setting turns it off).
- Keys in Controls: Open Backpack (B), Sort (middle mouse button, in a storage screen), Inventory Interaction (C, restock and deposit on the block you look at), and switches for the upgrades in slots 1 to 5 (Z and X for the first two): a switched-off upgrade does nothing.
- Settings (Options > Mods): backpack slowness for carrying too many, only the worn backpack's upgrades, chest loot, backpacks in container items, chests and barrels dropping packed, the drop count that forces packing, one wood variant in the creative tab, and the magnet ranges.

## Differences from the original

- The original's own screen (tabs, sort buttons, memory and no-sort slots) is a chest menu here.
- The furnace, jukebox, tank, battery, pump, XP pump, tool swapper, stonecutter, anvil, smithing, inception, everlasting and infinity upgrades sit in their slot and do nothing yet; the filter upgrade and the storage network blocks do not work yet.
- Backpacks are not drawn on the player's back, and mobs don't spawn wearing them.

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
