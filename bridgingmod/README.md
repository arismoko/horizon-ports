# Bridging Mod

A Sunrise 26.3 client-only port of [Bridging Mod](https://github.com/CloudG360/BridgingMod) (2.7.0) by CG360 (MIT).

## What it does

- **Reach-around block placement**: when your crosshair aims past an edge into open air while holding a placeable block, traces along your view line to find the nearest valid adjacent placement spot (in front, above, or below) and lets you right-click to place Bedrock-style.
- **Slab and trapdoor alignment**: aligns horizontally bridged slabs and trapdoors to your standing Y fraction (bottom or top half), and combines vertically bridged slabs into double slabs.
- **Crosshair directional indicator**: draws inverted-blend directional arrows (`up`, `down`, `horizontal`) around the crosshair whenever a reach-around target is ready, shifting down automatically when the F3 debug overlay is open.
- **World block outlines & F3 debug view**: highlights the reach-around target with a configurable RGB color and opacity, optionally highlights normal non-bridging placement spots, and supports separate F3 debug outline toggles plus a full view-path trace visualizer.
- **Axis, crouch, and item controls**: configure horizontal/vertical/both bridging axes, a separate axis override while crouching, crouch-only bridging, minimum bridging distance, snap strength and adjacency, perspective lock (copy camera perspective vs. head eyeline), skip torch bridging, and non-solid block replacement (tall grass, water, snow layers).
- **Toggle keybind**: registers **Toggle Reacharound Placement** (default `,`) in **Options → Controls**, persisting the toggle and showing an action-bar confirmation (`ON` / `OFF`).
- **Options screen & hot-reloaded config**: all settings are editable in **Options → Mods → Bridging Mod** and saved in `config/bridgingmod.toml` with automatic hot-reload.
- **Translations**: includes all 10 language files from the original mod (`en_us`, `es_ar`, `es_mx`, `it_it`, `pt_br`, `ru_ru`, `tr_tr`, `uk_ua`, `zh_cn`, `zh_tw`).

Client-side only: works in single player and on any multiplayer server using standard block-placement packets.

## Differences from the original

- **Post-bridging delay below 4 ticks (`delayPostBridging` < 4)**: values from `4` to `20` ticks are enforced, but shortening the cooldown below vanilla's hardcoded 4-tick right-click delay (such as `3` ticks) is not possible yet (gap: TBD-post-bridging-delay).
- **Skip torch bridging with offhand blocks**: when `skipTorchBridging` is enabled and you hold a torch in your main hand while holding blocks in your offhand, bridging is suppressed rather than skipping the main hand to place from the offhand, because the mod API cannot invoke per-hand client block placement directly (gap: TBD-client-use-item-on). Holding a non-block item (such as food, a tool, or a weapon) in your main hand and blocks in your offhand bridges normally.
- **Configuration UI layout**: in-game settings use Sunrise's flat **Options → Mods → Bridging Mod** page with separate RGB color and opacity controls rather than YACL's tabbed category screen with embedded preview images and an RGBA picker (gap: TBD-config-tabs).
- Third-party mod integrations from the original (Dynamic Crosshair, Dank Storage, Sable) are omitted because those mods are not on Sunrise; `perspectiveLock = "let_bridging_mod_decide"` still checks whether `freelook` is loaded.

## Configuration

- In-game settings: **Options → Mods → Bridging Mod**
- Config file: `config/bridgingmod.toml` (hot-reloaded automatically when edited)

## Building

```sh
cargo sunrise-mod check
cargo sunrise-mod build
CARGO_TARGET_DIR=target cargo test
cargo clippy --all-targets -- -D warnings
```

## Licence

MIT, matching the original project. See `LICENSE` and `NOTICE`.
