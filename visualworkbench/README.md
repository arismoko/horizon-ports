# Visual Workbench

A Sunrise 26.3 port of [Visual Workbench](https://github.com/Fuzss/visualworkbench) by Fuzs (MPL-2.0).

## What it does

- **Items stay put**: whatever you lay out in a crafting table stays in its grid when you close it, is saved per block position with the world, and drops when the table is broken, blown up or replaced. Two players at one table see the same grid.
- **Result slot**: the result follows the grid through the game's own crafting matcher, so data-pack recipes work. Taking it uses one of each ingredient and puts what a recipe leaves behind (a bucket, a bottle) back in its cell; shift-clicking the result crafts as many as fit your inventory.
- **Hoppers**: a hopper (or any item pipe) can fill the nine cells from any side, spreading items evenly as a crafter does, and never takes items out.
- **Comparators**: a redstone comparator reads how many of the nine cells are filled (0 to 9).
- **Server option** `convert_vanilla_workbench_when_interacting` (`config/visualworkbench-server.toml`, default `true`): crafting tables are handled by this mod when used; set it to `false` and they are vanilla. Blocks in the `#visualworkbench:unaltered_workbenches` block tag (the original's default: fletching and smithing tables) are left vanilla as well.
- **Client options** (Options → Mods → Visual Workbench): ingredient rendering (`None`, `Flat`, `Floating`), result rendering, and who the ingredients turn towards (`Closest Player`, `Crafting Player`, `Never`). They are registered as in the original but do nothing yet (see below).

## Differences from the original

- Items on a crafting table are not drawn on the table in the world: no flat or floating ingredients, no floating result, no turning towards the player, so the three client options have no effect yet, because the mod API can't draw for a vanilla block with no block entity (gap: TBD-world-render).
- The crafting screen has no recipe book, and recipes cannot be placed into the grid from outside it (the original keeps vanilla's `CraftingScreen`, recipe book and recipe-viewer transfer), because a menu with a persistent grid can't bind the game's recipe book yet (gap: TBD-recipe-book).
- The original replaces every `CraftingTableBlock` with its own block carrying a block entity; here the table stays vanilla's `minecraft:crafting_table` with its contents kept in the world's saved data, so crafting tables of other mods are not converted and the block has no block entity (gap: TBD-table-block-entity).
- Taking a result earns the `crafted` statistic and unlocks the recipe, but does not run the advancement trigger or the item's post-crafting step vanilla's result slot runs (gap: TBD-recipe-crafted-trigger).
- The original's JEI and recipe-viewer integration is not part of this port.

## Configuration

`config/visualworkbench-server.toml` (server) and Options → Mods → Visual Workbench (client). The server option is read on the server only; a client needs no copy.

## Building

```sh
cargo sunrise-mod check
cargo sunrise-mod build
cargo test
```

## Licence

MPL-2.0, matching the original project's code. See `LICENSE` and `NOTICE`. No image of the original is included; the icon is new.
