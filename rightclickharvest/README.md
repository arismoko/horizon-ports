# Right Click Harvest for Sunrise

Right-click a grown crop to harvest and replant it.

A port of [Right Click Harvest](https://github.com/JamCoreModding/right-click-harvest) by Jamalam to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), under the original's licence (MIT, in `LICENSE`; see `NOTICE`). One native mod on Sunrise's mod API.

## What it does

- Right-click a fully grown crop (wheat, carrots, potatoes, beetroots, torchflowers, cocoa, nether wart, and crops from other mods) to harvest it with the drops a break would give, one seed or crop short (the block's pick-block item, so modded crops too), and replant it at its first age.
- Right-click the second block of a sugar cane or cactus column to harvest it; the rest of the column falls as usual.
- A hoe in the hand also harvests the crops around: tiers set by the `rightclickharvest:low_tier_hoes`, `mid_tier_hoes` and `high_tier_hoes` item tags (the four neighbours, the square around, the wider diamond), and `rightclickharvest:radius_harvest_blacklist` keeps cocoa, cane and cactus out of it.
- Sneaking, the off hand and spectators never harvest; `rightclickharvest:blacklist` lists blocks that never do.
- A hoe in use loses durability, a harvest awards the mined and used statistics, and spawn-protected ground is left alone.
- `config/rightclickharvest-common.toml` holds the settings: `requireHoe` (crops need a hoe, except `rightclickharvest:hoe_never_required` blocks; a bare hand is told once), `harvestInRadius`, `hungerLevel` (none, low, normal, high: how much exhaustion a harvest adds, and no harvest with an empty food bar) and `experienceType` (none, cost or reward 0 to 2 points).
- Translations in the eleven languages the original ships.

## Differences from the original

- A hoe is an item in `#minecraft:hoes` or one of the three tier tags; the original's extra test for a block-transformer component that tills is not read, as the mod API does not expose that component's contents ([sunrise-org/sunrise#5099](https://github.com/sunrise-org/sunrise/issues/5099)).
- Other mods' claim protection cannot be asked through the loader's break and place events: the mod API has no host-level break or place veto to call ([sunrise-org/sunrise#5288](https://github.com/sunrise-org/sunrise/issues/5288)). Instead the mod posts a typed `rightclickharvest:before_harvest` event that a claim mod cancels (see below); spawn protection and build permission are still checked.
- The original's `AfterHarvest` event is the typed `rightclickharvest:after_harvest` event ([sunrise-org/sunrise#5137](https://github.com/sunrise-org/sunrise/issues/5137)).
- The settings are also under Options > Mods, in three sections (Requirements, Harvesting, Costs); a change there is written to the config file ([sunrise-org/sunrise#1174](https://github.com/sunrise-org/sunrise/issues/1174)). There is no config screen with links as in the original. Only the English text of the section names is shipped.

## Events for other mods

A mod that depends on this crate listens with `#[subscribe]` on the server:

- `BeforeHarvestEvent` (`rightclickharvest:before_harvest`, cancellable): the player, dimension, block position, block id and whether it is replanted. Cancelling leaves the block as it is.
- `AfterHarvestEvent` (`rightclickharvest:after_harvest`): the same, once the harvest is done.

The replant item of a crop comes from the block's pick-block stack (`PlayerRef::clone_item_stack`, [sunrise-org/sunrise#5289](https://github.com/sunrise-org/sunrise/issues/5289)), so modded crops keep back their own seed.

## Building

With the Sunrise mod tool (`cargo sunrise-mod`, from the Sunrise SDK) and Rust:

```sh
cargo sunrise-mod build    # compiles against the pinned game, packs target/sunrise-mod/rightclickharvest.smod
cargo sunrise-mod run      # builds it and starts the game with it
cargo test
```

`sunrise-mod.toml` pins the Sunrise build it is made for (`sunrise`, a game commit). Sunrise builds native mods from source on the player's computer, against the exact game build.

## Licence and credits

Right Click Harvest is by Jamalam. This port is distributed under the original's licence, the **MIT licence**; `LICENSE` holds the text and `NOTICE` says where the port comes from.

Ported to Minecraft 26.3 by lavoide ([lavoide/horizon-ports](https://github.com/lavoide/horizon-ports)), and to Sunrise by arismoko.
