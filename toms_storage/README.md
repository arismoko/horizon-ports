# Tom's Simple Storage for Sunrise

Join chests into one network and reach all of it from a storage or crafting terminal, over cables, trims and wireless terminals.

A port of [Tom's Simple Storage](https://github.com/tom5454/Toms-Storage) 1.7.1 by tom5454 to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), written on Sunrise's typed mod API. The content is the original's; the rules are Rust in `src/lib.rs`. It uses no `[[native.mixin]]` rows. Under the original's licence (MIT, in `LICENSE`; see `NOTICE`).

## What it does

- **Network**: an inventory connector joins the inventories touching it, or touching those through trims, within 16 blocks (double chests count once). Cable connectors bring the inventory they face into the connector their cables reach (up to 2048 cables). The connector, the cable connectors and the inventory proxy answer the item capability, so hoppers and pipes reach the whole network; the proxy passes on the inventory it faces.
- **Storage terminal**: items by page with their stored amount, sorting by amount, name or mod, search typed straight into the screen (`@mod` for a mod), take a stack (right click: half), put in a stack (right click: one).
- **Crafting terminal**: the storage screen plus a 3x3 grid refilled from the network and crafted from; the grid goes back to the network when you close it.
- **Wireless terminal** (16 blocks, the terminal you look at) and **advanced wireless terminal** (bound by sneaking on a terminal, 64 blocks).
- **Placement**: terminals against the clicked block, cable connectors and six-way blocks facing it.

All item transfers are typed transactions (`ResourceHandler<ItemResource>`, `TransactionContext`): a move that cannot finish rolls back. The terminals are vanilla chest menus opened with `open_chest_menu` and handled through a `MenuPart`.

## Differences from the original

- The terminal is a six-row chest screen the mod fills, not the original's own screen: no search history, recipe-viewer fill or terminal sizes, and no cursor stack.
- The inventory hopper, level emitter, open crate, item filters and filtered connector rules, inventory link channels, the beacon range bonus and paint kits exist as items and blocks but have no behaviour yet.
- A cable connector with no connector on its cables still offers an empty item capability.

## Building

`cargo sunrise-mod check` and `cargo sunrise-mod build`; `cargo test` runs the unit tests of the pure rules (directions, layouts, search, placement).
