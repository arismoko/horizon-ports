# Shoulder Surfing Reloaded

A Sunrise 26.3 client-only port of [Shoulder Surfing Reloaded](https://github.com/Exopandora/ShoulderSurfing) (5.3.1) by Exopandora and contributors (MIT).

## What it does

- **Shoulder-surfing perspective**: F5 cycles first person, third person back, third person front and shoulder surfing (or replaces the back view with shoulder surfing), skipping the perspectives you disable; keys can also toggle or enter a perspective directly, and the last perspective can be remembered as the default.
- **Offset camera**: the camera sits over the player's shoulder at a configurable x, y and z offset that eases in and out, with limits, per-situation multipliers and modifiers (sprinting, aiming, fall flying, climbing, riding), centring when looking down, the entity-scale and camera-distance attribute, and a collision trace so it never goes through blocks. Keys adjust the offset, swap the shoulder and cycle presets.
- **Decoupled camera**: the camera turns on its own, the player walks relative to it and turns to face the way it walks; holding **Free Look** looks around without turning the player. The player turns with the camera while attacking, using or interacting, as configured.
- **Crosshair**: static, dynamic, adaptive (dynamic while a bow, trident, crossbow or thrown item is held or used) and 1PP variants, per-perspective visibility, and an obstruction indicator and obstructed crosshair when something stands between you and the target.
- **Object picker**: what you hit is found with the camera's ray and limited to your reach, ignoring blocks matching the non-collidable list and invisible entities matching the unpickable list.
- **Hiding the player** when the camera is inside the head or looks up, an optional **FOV override**, and a held-map overlay.
- **Options**: 92 settings in **Options → Mods → Shoulder Surfing Reloaded**, 19 key mappings under **Shoulder Surfing** in **Controls**, and list options in `config/shouldersurfing-client.toml`.

Client-side only: works in single player and on any multiplayer server. Server checks stay vanilla's: what you hit is limited to the interaction range.

## Differences from the original

- The player is not drawn semi-transparent as the camera closes in; it is hidden once the original's transparency would have reached its 15% floor, when the camera is inside the head, when looking up past the hide angle and while aiming if enabled (gap: sunrise-org/sunrise#5333).
- Camera sway and camera drag (which roll or lag the camera) are not ported: the camera roll angle is available and the roll other mods set is kept, but the port has no sway or drag option.
- The view bobbing mode (inherit, on, off) is not ported because bobbing cannot be switched per perspective (gap: sunrise-org/sunrise#5553).
- The crosshair is redrawn by the mod in third person with the vanilla crosshair sprite; the attack-strength indicator is not drawn there (gap: sunrise-org/sunrise#5691).
- The map overlay shows the held map as an item icon at the original's position and size, not as the full map with the checkerboard backdrop and decorations, and it does not move down for status effect icons (gap: sunrise-org/sunrise#5692).
- Settings use Sunrise's flat **Options → Mods → Shoulder Surfing Reloaded** page, which has no tabs or list editor; the list options (offset presets, non-collidable blocks, invisible entities that cannot be picked and the adaptive crosshair item lists) are edited in `config/shouldersurfing-client.toml` (gap: sunrise-org/sunrise#1217).
- Not ported yet (possible on the mod API): temporary first person in constrained spaces, turning the camera with the player, ignoring block pillars, orienting the camera on teleport, the minecart and boat camera rules, and the F3 `Camera:` line.
- The original's plugin API and its Create, Curios, Cobblemon and Jade integrations do not apply to a native Sunrise mod.
- The perspective is kept as a camera type plus the mod's own shoulder-surfing flag, so the shoulder view is shown as the third-person-back view by anything else that reads the camera type.

## Configuration

- In-game settings: **Options → Mods → Shoulder Surfing Reloaded**; key mappings in **Options → Controls**
- List options (`camera.offset.presets`, `lists`): `config/shouldersurfing-client.toml` (hot-reloaded when edited). Item, block and entity entries are regular expressions matched against the whole id.

## Building

```sh
cargo sunrise-mod check
cargo sunrise-mod build
CARGO_TARGET_DIR=target cargo test
cargo clippy --all-targets -- -D warnings
```

## Licence

MIT, matching the original project. See `LICENSE` and `NOTICE`.
