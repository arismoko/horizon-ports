# Enchantment Descriptions

A Sunrise 26.3 client-only port of [Enchantment Descriptions](https://github.com/Darkhax-Minecraft/Enchantment-Descriptions) (26.3.0) by Darkhax (LGPL-2.1).

## What it does

- **Description under each enchantment**: an item's tooltip gets a translated description line right under each enchantment's own line, for enchanted books (stored enchantments) and for enchanted gear (applied enchantments).
- **Description keys**: for enchantment `namespace:name` the language is asked for `enchantment.namespace.name.desc`, then `.description`, then `.info`, each also with the level on the end (`.desc.3`), as the original does; so any mod or resource pack can describe its own enchantments.
- **Translations**: the original's 25 language files are carried (see `NOTICE`), with the hint line `enchdesc.activate.message`.
- **Settings** (Options → Mods → Enchantment Descriptions): **Show Descriptions** (`enabled`), **Only On Enchanted Books** (`only_on_books`), **Only In Enchanting Table** (`only_in_enchanting_table`), **Hold Shift To Show** (`require_keybind`), **Show Shift Hint** (`display_activate_text`) and **Description Colour** (dark gray by default).
- **Config file** `config/enchdesc.toml`: `prefix` and `suffix` (text around each description, plain or a JSON text component), `activate_text` (the hint line), `bold`, `italic`, `underlined`, `strikethrough`, `obfuscated` and `font` for the description text. It hot-reloads.
- While **Hold Shift To Show** is on and Shift is not held, the hint line "Hold shift to view enchantment descriptions." is added at the end of the tooltip.

Client-side only: works in single player and on any multiplayer server.

## Differences from the original

- Descriptions are placed by finding each enchantment's own tooltip line by its translation key, so an enchantment whose line is not that key (a data pack enchantment with a custom description key) gets no description (gap: TBD-description-key).
- **Only In Enchanting Table** recognises the screen by its title (`container.enchant`), so a renamed enchanting table is not recognised (gap: TBD-enchanting-screen).
- Prefix, suffix, hint text and text styling are set in `config/enchdesc.toml`, not on a settings screen with text fields (gap: TBD-text-settings).
- The original's setting names are the options' keys; the on/off options and colour live in Options → Mods, the rest in `config/enchdesc.toml`, where the original has one config file.
- Enchantments without a description are not logged.

## Building

```sh
cargo sunrise-mod check
cargo sunrise-mod build
CARGO_TARGET_DIR=target cargo test
cargo clippy --all-targets -- -D warnings
```

## Licence

LGPL-2.1-only, matching the original project. See `LICENSE` and `NOTICE`.
