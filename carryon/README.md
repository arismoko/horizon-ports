# Carry On for Sunrise

Sneak and right-click with empty hands to pick up chests, other block entities and small mobs, and carry them.

A port of [Carry On](https://github.com/Tschipp/CarryOn) by Tschipp, Purplicious_Cow and cy4n to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), written in Rust on Sunrise's mod API.

## What it does

- Sneak and right-click a chest, furnace or any other block entity with both hands empty to pick it up with its contents; right-click a block face to set it down, facing the way you look.
- Pick up animals and other small mobs the same way (at most 1.5 blocks wide and 2.5 high, no hostile mobs in survival) and set them down, or stack them on another mob, up to ten high.
- Carrying slows you by weight: heavier block entities and bigger mobs slow you more, in creative too.
- You can't break, hit or switch items while carrying; if you die without keepInventory, what you carried is set down where you died.
- Others see what you carry: blocks in your hands, mobs over your head as their spawn egg.
- The original's default block and mob blacklists, block and entity tags included (`#c:relocation_not_supported`, `#c:capturing_not_supported` and the others).

## Differences from the original

- Players can't be picked up, the arms are not raised while carrying, and custom pickup scripts and conditions are not supported.
- A carried mob is shown over the hotbar in first person rather than in your arms.
- The settings are the original's defaults; there is no config file yet.

## Building

With the Sunrise mod tool (`cargo sunrise-mod`, from the Sunrise SDK) and Rust:

```sh
cargo sunrise-mod build    # compiles against the pinned game, packs target/sunrise-mod/carryon.smod
cargo sunrise-mod run      # builds it and starts the game with it
cargo test
```

`sunrise-mod.toml` pins the Sunrise Dev build `dev-2026-11-22-7cfdf81` (game commit `7cfdf81463`). Sunrise builds native mods from source on the player's computer, against the exact game build.

## Licence and credits

Carry On is by Tschipp, Purplicious_Cow and cy4n (https://github.com/Tschipp/CarryOn). This port is distributed under the original's licence, the **GNU LGPL v3.0 only**; `LICENSE` holds the text and `NOTICE` says where the port comes from. No Java code is included: the rules are written anew in Rust.

Ported to Minecraft 26.3 by lavoide ([lavoide/horizon-ports](https://github.com/lavoide/horizon-ports)), and to Sunrise by arismoko.
