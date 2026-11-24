# Easy Magic

A Sunrise 26.3 port of [Easy Magic](https://github.com/Fuzss/easymagic) by Fuzs (MPL-2.0).

## Features

- **Persistent enchanting table inventory**: items, lapis lazuli (or `#easymagic:enchanting_catalysts`), and optional reroll catalysts (`#easymagic:reroll_catalysts`) stay in the enchanting table when you close the screen and drop when the table is broken.
- **Enchantment rerolling**: click the reroll button inside the enchanting screen to roll a new set of enchantments for a configurable catalyst and experience point (or level) cost, or free when set to `0`.
- **Dedicated reroll catalyst slot**: enable `dedicated_reroll_catalyst` in `easymagic-server.toml` to unlock a third slot for `#easymagic:reroll_catalysts` (amethyst shards by default).
- **Configurable enchantment clues**: show `NONE`, `SINGLE` (vanilla default), or `ALL` resulting enchantments on each offer tooltip, plus optional enchantment description lines from language packs.
- **Lenient bookshelves**: blocks without a full collision shape (such as torches, carpets, and snow layers) do not block bookshelves behind them from supplying enchanting power or particles.
- **Chiseled bookshelf enchanting power**: chiseled bookshelves contribute 1 enchanting power for every 3 stored books (`NONE`, `FACING` by default, or `ALL`), with enchanting particles.
- **Chiseled bookshelf book tooltips**: hovering a book in a chiseled bookshelf displays its full item tooltip on the HUD (`DISABLED`, `CROUCHING`, or `ENABLED`, with configurable X/Y offsets).
- **Hopper & comparator support**: hoppers can insert items and catalysts into enchanting tables and extract enchanted items from underneath; redstone comparators read the catalyst slot (0–3).
- **Data pack tags**:
  - `easymagic:enchanting_catalysts` (item tag, default `minecraft:lapis_lazuli`)
  - `easymagic:reroll_catalysts` (item tag, default `minecraft:amethyst_shard`)
  - `easymagic:unaltered_enchanting_tables` (block tag, empty by default)

## Differences from the original

- Items on an enchanting table are not drawn on the table in the world (the `FLAT` and `FLOATING` settings), because the mod API can't yet draw for a vanilla block entity ([sunrise-org/sunrise#5531](https://github.com/sunrise-org/sunrise/issues/5531)).
- With `keep_enchantment_screen_book`, the screen draws a still book where vanilla's opens and turns its pages, because the mod API can't yet draw an animated vanilla model in a screen ([sunrise-org/sunrise#5532](https://github.com/sunrise-org/sunrise/issues/5532)). The reroll click region and its tooltip work as in the original.

## Configuration

Server settings live in `config/easymagic-server.toml` and sync automatically to connected clients. Client settings live in `config/easymagic-client.toml` and in **Options → Mods → Easy Magic**.

## Building

```sh
cargo sunrise-mod check
cargo sunrise-mod build
cargo test
```

## Licence

MPL-2.0, matching the original project. See `LICENSE` and `NOTICE`.
