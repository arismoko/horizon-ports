# Comforts for Sunrise

A pure typed-API port of [Comforts](https://github.com/illusivesoulworks/comforts) (26.x, LGPL-3.0-or-later) by Illusive Soulworks to Sunrise (Minecraft Java 26.3, Dev 1 (`dev-2026-11-25-bdad86d`)), adding sleeping bags, hammocks, and rope & nail without changing your respawn point.

## Features

- **16 Sleeping Bags (`comforts:sleeping_bag_<color>`):**
  - Crafted from 3 matching wool in a horizontal row, or by dyeing `comforts:sleeping_bag_white`.
  - Auto-use enabled by default: right-click a surface to place, immediately sleep, and pick the bag back up on wake-up (preserving custom names); sneak right-click to place without auto-sleeping.
  - Supports waterlogging, Nether/End bed explosions, optional break chance with Luck multiplier, optional wake-up status effects (`sleepingBagEffects`), and phantom timer toggle.
  - Never overwrites the player's respawn point.
- **16 Hammocks (`comforts:hammock_<color>`) & Rope and Nail (`comforts:rope_and_nail`):**
  - Place two `comforts:rope_and_nail` blocks facing each other across 2 empty blocks (4 blocks apart wall-to-wall) on sturdy horizontal faces, then right-click a rope with a hammock cloth to hang it.
  - Sleeps through the day to skip to dusk (`comforts:wake_up_from_nap` timeline marker), with optional `nightWakeTimeOffset`, waterlogging, and custom name preservation.
  - Breaking either rope or either hammock half cleanly removes the hammock and resets the partner rope's `supporting` state.
- **Configuration (`comforts-common.toml` and `comforts-server.toml`):**
  - Common: `enableSleepingBagRecipes`, `enableHammockRecipes` (enforced dynamically via `comforts:conditional_shaped` and `comforts:conditional_shapeless` crafting recipe serializers, including `#c:ropes` conditional rope & nail recipes).
  - Server: `autoUse`, `restrictSleeping`, `restMultiplier`, `hammockUse`, `sleepingBagUse` (`NIGHT`, `DAY`, `DAY_OR_NIGHT`, `USE_WITHOUT_SLEEPING`, including F5 perspective toggle while resting in `USE_WITHOUT_SLEEPING`), `daySleepingPercentage`, `dayWakeTimeOffset`, `nightWakeTimeOffset`, `sleepingBagsStopPhantoms`, `hammocksStopPhantoms`, `sleepingBagBreakChance`, `sleepingBagBreakChanceLuckMultiplier`, `sleepingBagEffects`.
  - Server settings relevant to client UX (`autoUse`, `sleepingBagUse`, `hammockUse`) sync over `comforts:client_sync` on login and config reload.
- **Assets & Localization:** All 16 colours of sleeping bags and hammocks, rope & nail (supporting and unsupporting models), block & item tags, timeline wake marker (`data/comforts/timeline/sleep_schedule.json`), loot tables, 66 conditional recipes, and all 17 upstream language files.

## Known Differences on Dev 1 (`dev-2026-11-25-bdad86d`)

- **Engine bed identification (`IBlockExtension.isBed`):** Player sleep on sleeping bags and hammocks runs through Comforts' sleep lifecycle and `can_continue_sleeping`, while engine-level bed checks (`Game::is_bed`, `LivingRef::start_sleeping`, `bed_wake_pose`) still match only `*_bed` block ids, so villagers do not claim sleeping bags or hammocks on their own (gap: sunrise-org/sunrise#5557).
- **Daytime sleeping percentage & live status line:** `daySleepingPercentage` (when set, `>= 0`) replaces `players_sleeping_percentage` for daytime naps through the `sleepers_needed` hook, above or below the vanilla value. The daytime action-bar line (`comforts.skipping_day`) is shown live on every sleep count change through the `sleep_status_announce` hook, in place of vanilla's line.
- **Cat bed AI & hammock jump tweak:** Sleeping bags and hammocks report `PathType::Walkable`, while cat lie-on-bed / sit-on-bed / relax-on-owner goals and the hammock `WalkNodeEvaluator` jump tweak await cat AI hooks (gap: sunrise-org/sunrise#5559).
- **Remote sleeper pose offset & auto-sleep arm swing:** Local sleepers are positioned directly onto the lowered sleeping bag (`0.1875`) and hammock (`0.0625`) surfaces; remote sleeper vertical model offset (`-0.375` / `-0.5`) and clearing the auto-sleep arm swing animation await client player-render and arm-swing hooks (gap: sunrise-org/sunrise#5560).
- **Block item display names:** Sleeping bags, hammocks, and rope & nail show raw `block.comforts.<id>` names in-game because Sunrise gives a declared block's item the block's translation key while upstream Comforts' language files define `item.comforts.<id>` keys (gap: sunrise-org/sunrise#5574).

## Credits & Licence

- Original mod by **Illusive Soulworks** (`LGPL-3.0-or-later`).
- Block models and initial port assets credit **lavoide**.
- Sunrise port by **arismoko**.
- Licensed under `LGPL-3.0-or-later` (see `COPYING`, `COPYING.LESSER`, `LICENSE`, `NOTICE`).
