# Crops Love Rain for Sunrise

Crops grow faster in the rain.

A port of [Crops Love Rain](https://codeberg.org/ThePoultryMan/Crops-Love-Rain) 3.2.2 by ThePoultryMan to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), under the original's licence (MIT, in `LICENSE`; see `NOTICE`). One native mod on Sunrise's mod API.

## What it does

- While it rains on a crop (`Level.isRainingAt`) and it is lit, each random tick gives it a 1 in `rain_growth_speed` chance (default 10) of one extra growth step. It covers the `#minecraft:crops` blocks (wheat, carrots, potatoes, beetroots, torchflowers) and other mods' crops in that tag, sugar cane (a new cane on top while the column is under three tall), cocoa, sweet berries, all saplings (a stage first, then the tree), melon and pumpkin stems (an extra step, then the fruit) and bamboo (while it rains in the level).
- A bamboo sapling grows its first stalk on every random tick, as in the original.
- `config/cropsloverain-common.toml` holds the settings: the master switch and chance, optional individual chances for bamboo, cocoa, crops, saplings, sugar cane, sweet berries, melons and pumpkins (0 turns one off), a separate stem speed, and a debug mode with "halt regular growth" that stops the game's own growth so the rain's effect shows alone.
- Each setting is also on the mod's page in Options > Mods, in the file's three sections (General, Individual, Debug); a change there is written to the file.
- English text as the original ships it.

## Differences from the original

- A bamboo stalk's extra growth and a sapling's tree stage are applied at the end of the server tick, not inside the random tick: the hooks that see them cannot write blocks or run bone meal.
- A stem's extra step is rolled beside vanilla's own, where the original turns a failed vanilla roll into growth ([sunrise-org/sunrise#5318](https://github.com/sunrise-org/sunrise/issues/5318)).
- A sapling's tree stage is bone meal's, which succeeds with bone meal's chance ([sunrise-org/sunrise#979](https://github.com/sunrise-org/sunrise/issues/979)).
- The original's own list of crops is the `#minecraft:crops` tag here, so crops of other mods that are in the tag also grow faster.
- The original syncs its config to players; here it is a server-side file, also shown in Options > Mods in a single-player world ([sunrise-org/sunrise#1216](https://github.com/sunrise-org/sunrise/issues/1216)).
