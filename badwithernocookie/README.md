# Bad Wither No Cookie - Reloaded

A Sunrise 26.3 port of [Bad Wither No Cookie - Reloaded](https://gitlab.com/kreezxil/BadWitherNoCookie) (4.0.5) by Kreezxil and Eleksploded (MIT).

## What it does

- **Configurable broadcast & loud sound toggles**: silences Wither spawn and death (`silenceWither`, on), wandering trader and llama ambient sounds (`silenceTrader`, on), Ender Dragon death (`silenceDragon`, on), thunder (`silenceLightning`, on), and explosion/primed TNT/creeper/wind charge sounds (`silenceExplosion`, off).
- **Custom sound list (`silenceUs`)**: silence any additional sound by path (e.g. `entity.sheep.step`) or full namespaced id (`othermod:entity.thing.hurt`), while respecting other mods' sound cancellations and replacements.
- **Book of Silence (`bwncr:silence`)**: craft a book surrounded by 8 wool and apply it at an anvil to headwear (`#bwncr:silence_headwear`) to silence all `entity.*` sounds and sounds in the hostile, neutral, weather, and ambient categories for the wearer.
- **88 Mob Silence books & 37 Environment Silence books**: craft a Book of Silence with a mob's drop or an environment block/item to silence one mob or one block/environment sound group; stack multiple Silence enchantments on one helmet.
- **Silence All (`bwncr:silence_all`)**: surround a Book of Silence with 8 diamonds to silence every sound in every category for the wearer, including music, jukebox records, and UI.
- **Gray item tooltips**: displays explanatory tooltips on Silence books and headwear carrying Silence enchantments.
- **Operator & client commands**: `/bad_wither_no_cookie_reloaded` and `/bwncr` (permission level 2 on servers: `show`, `<option> [true|false]`, `toggle <option>`, `all <true|false>`, `silenceUs [list|add|remove|clear]`, `listen`, `listen mute <sound>`), plus client-side `/bwncr listen` and `/bwncr listen mute <sound>`.
- **Optional server sync**: servers broadcast active settings over the optional `bwncr:settings` channel so modded clients apply server rules while vanilla clients can still join.
- **Translations & configuration**: includes `en_us`, `es_ar`, `pt_br`, and `ru_ru` translations, hot-reloaded `config/badwithernocookie-common.toml`, and toggle controls in **Options → Mods → Bad Wither No Cookie - Reloaded**.

Works on the client, on the server, or both.

## Differences from the original

- Server-side silencing of global level events (1023 Wither spawn, 1028 Ender Dragon death, 1038 End Portal spawn) and sound-only level events (1000–1054) for vanilla clients without the mod is not available yet; clients with the mod installed silence all of them (gap: TBD-server-level-event-sound-filter).
- Per-player server-side sound filtering (so Silence books worn on headwear silence sounds for vanilla clients without affecting other players) is not available yet because server level-sound hooks cancel level-wide; clients with the mod installed filter per wearer (gap: TBD-per-player-server-sound-filter).
- Clearing only the `playSound` flag on server explosion packets (`ClientboundExplodePacket`) for vanilla clients when `silenceExplosion` is enabled is not available yet; clients with the mod installed silence the explosion sound while keeping knockback and particles (gap: TBD-explosion-packet-sound-toggle).
- The `silenceUs` string list cannot be edited in **Options → Mods** (which shows the 6 boolean toggles); edit `silenceUs` with `/bwncr silenceUs`, `/bwncr listen mute <sound>`, or `config/badwithernocookie-common.toml` (gap: TBD-config-string-list-setting).
- The config file is named `config/badwithernocookie-common.toml` (matching the Sunrise mod id) rather than `config/bwncr-common.toml`, and no separate `.example.cf` file is written.

## Configuration

- In-game toggles: **Options → Mods → Bad Wither No Cookie - Reloaded**
- Full config file: `config/badwithernocookie-common.toml` (hot-reloaded automatically when edited)

## Building

```sh
cargo sunrise-mod check
cargo sunrise-mod build
CARGO_TARGET_DIR=target cargo test
cargo clippy --all-targets -- -D warnings
```

## Licence

MIT, matching the original project. See `LICENSE` and `NOTICE`.
