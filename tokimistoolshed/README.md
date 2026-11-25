# Tokimi's Toolshed

A Sunrise 26.3 port of [Tokimi's Toolshed](https://github.com/CozyCord/TokimisToolshed) 1.8 by Tokimi, CozyStudios, under the original's licence (CC0-1.0, in `LICENSE`; see `NOTICE`).

Items, recipes, tags, models and textures are the original's, declared in `data/tokimistoolshed/sunrise/`. What the tools do is Rust in `src/`, on Sunrise's typed mod API (pinned to Dev 1, `bdad86d`), with no `[[native.mixin]]` row. The rules with no game in them are in `src/rules.rs` and are unit tested.

## What it does

| Tool | Moment | Behaviour |
|---|---|---|
| Hammer, excavator | `left_click_block`, `player_destroyed_block` | After a survival break (not sneaking) breaks the 3x3 in the plane of the face hit, each block through the player's own break (`PlayerRef::destroy_block`); the hammer only takes what it can harvest |
| Scythe | `player_destroyed_block`, `right_click_block` | Breaks the horizontal 3x3 of crops and plants; after a till that works, tills the eight around from above (`LevelRef::block_transform`) |
| Clippers | `player_destroyed_block` | Breaks the 3x3 of leaves, wool, webs, moss, vines and plants |
| Lumber axe | `player_destroyed_block` | Fells the tree (`LumberAxeItem.detectTree` rules); drops at the stump; one use of wear per log. With Falling Trees installed its break cancel comes first, as on Java |
| Copper bucket | `item_interact_mob` | Milks a cow or mooshroom |
| Copper milk bucket | `living_entity_use_item_finish` | Back as a copper bucket with a use of wear (sixteen uses) |
| Abacus | `right_click_block`, `client_tick_post` | Binds or unbinds; the HUD line counts each axis between the bound and the aimed block |

## Omitted when standalone

The original's optional Obsidian Equipment Rework recipes (the four obsidian tools from `obsidianequipmentrework:obsidian_ingot` and their four smelting recipes back to the ingot), with their recipe advancements, are not shipped. Without that mod the ingot is not registered, so the recipes could not load and the client disconnected on `recipe_book_add`. The obsidian tool items stay registered.

## Not ported (missing or blocked on the mod API)

- **Copper bucket fill, empty and cauldrons** (`CopperBucketItem.use`, `CopperBucketBehaviors`): a data item's template carries components only; `Item-id-specific behaviour does not follow a template`. The fluid helpers exist (`LevelRef::clip` with fluids, `level_pickup_fluid`, `level_place_fluid` in a transaction) but there is no hook for a mod item that swaps itself for a filled twin the way `BucketItem.use` does, no `ItemStack` return for `right_click_item` that carries a result stack, and no powder-snow or cauldron entry for a mod bucket. The filled copper buckets are items only.
- **Trowel and chisels** (`TrowelItem`, `ChiselItem`): need `BlockItem.place` with a chosen hotbar stack (the placement checks, the sound, the consumption). `player_place_block` writes a state with none of those checks, so a faithful port waits for a use-a-stack-on-a-block primitive.
- **Clippers rolling loot as shears** (`ShearsLootMixin`): loot-table conditions that name `minecraft:shears` do not see another item; the original's `#minecraft:shears` tag entry is kept.
- **`instanceof PlantBlock`**: the mod API cannot ask a block's class; vegetation is read from block tags and a list of grasses.
- **The hammer/excavator/scythe `block_break_speed`**: not needed, the tools' `minecraft:tool` component carries their speed.
- **Break sound and statistic when the worn-out milk bucket breaks**: the mod API gives the finish hook only a replaced result.
- **Better Combat weapon attributes** (`data/tokimistoolshed/weapon_attributes`) are kept as data; Sunrise has no consumer.
