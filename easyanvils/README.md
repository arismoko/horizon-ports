# Easy Anvils

A Sunrise 26.3 port of [Easy Anvils](https://github.com/Fuzss/easyanvils) by Fuzs (MPL-2.0).

## What it does

- **Persistent anvil inventory**: items stay inside the anvil when you close its screen, persist across saves, survive damage and repair stages, and drop when the anvil is broken or replaced.
- **No "Too Expensive!" cap (configurable)**: removes the 40-level limit by default (`too_expensive_limit = -1`), or lets you set any custom cap.
- **Configurable prior work penalty**: `LIMITED` (default, caps each penalty increase at 4 levels), `VANILLA`, or `NONE`, plus `FIXED` / `LIMITED` / `VANILLA` rename-and-repair cost handling, penalty-free renames/repairs, and penalty-free book combining.
- **Free renames & tunable repair/book costs**: free renaming for all items by default (`ALL_ITEMS`, `NAME_TAGS_ONLY`, or `NEVER`), halved enchanted book costs, and configurable material/item repair costs and durability bonuses.
- **Iron block anvil repair**: right-click a chipped or damaged anvil with an iron block—or dispense an iron block into one—to repair it by one damage stage, with anvil sound and particles.
- **Reduced break chance & risk-free renaming**: 5% break chance by default (`anvil_break_chance = 0.05`) and zero break chance when only renaming (`risk_free_anvil_renaming = true`).
- **Formatted item renaming**: supports `§` formatting codes directly in the anvil rename field (with live styled preview, 50 visible-character limit, and a top-right `?` formatting guide tooltip).
- **Hopper & comparator support**: hoppers can insert items into anvils from the top or sides (no bottom extraction); redstone comparators read the anvil's fill level.
- **Other mods' anvil recipes**: the custom anvil menu runs every mod's `anvil_update` listeners (`PlayerRef::anvil_update`, NeoForge's `CommonHooks.onAnvilUpdate`), so their recipes, cost changes and repair rules work in it.
- **Data pack tag**: `easyanvils:unaltered_anvils` (block tag) to keep specific anvils on vanilla's anvil menu.

## Differences from the original

- Items stored inside an anvil are not drawn on top of the anvil in the world (`render_anvil_contents`), because vanilla anvils do not expose a mod block-entity renderer and `WorldDrawContext` cannot yet draw an `ItemStack` in world space (gap: sunrise-org/sunrise#5609).
- Unaltered vanilla anvils (when `convert_vanilla_anvil_when_interacting = false` or listed in `#easyanvils:unaltered_anvils`) keep vanilla's 12% break chance, because `anvil_craft_pre` does not expose a break-chance modifier (gap: sunrise-org/sunrise#5500).

## Configuration

Server settings live in `config/easyanvils-server.toml` and sync automatically to connected clients. Client settings live in **Options → Mods → Easy Anvils**.

## Building

```sh
cargo sunrise-mod check
cargo sunrise-mod build
cargo test
```

## Licence

MPL-2.0, matching the original project. See `LICENSE` and `NOTICE`.
