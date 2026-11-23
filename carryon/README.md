# Carry On for Sunrise

Hold the carry key and right-click with empty hands to pick up chests, other block entities and small mobs, and carry them.

A port of [Carry On](https://github.com/Tschipp/CarryOn) by Tschipp, Purplicious_Cow and cy4n to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), written in Rust on Sunrise's mod API.

## What it does

- Hold the carry key (left shift unless you rebind it in Controls) and right-click a chest, furnace or any other block entity with both hands empty to pick it up with its contents; right-click a block face to set it down, placed as the game would place it: facing the way you look, chests standing alone, water kept.
- Pick up animals and other small mobs the same way and set them down, or stack them on another mob.
- Carrying slows you by weight: heavier block entities and bigger mobs slow you more.
- You can't break, hit or switch items while carrying; if you die without keepInventory, what you carried is set down where you died.
- Others see what you carry: blocks in your hands, mobs over your head as their spawn egg.
- Settings (Options > Mods) hold the original's switches and numbers: reach (scaled by your reach attribute), mob size limits (relative to your scale), slowness multipliers and heavy options, stack limit, baby pickup, hostile mobs, any-block pickup, unbreakable blocks, hitting while carrying, setting down when hit, the whitelist switches and whether carried blocks face you.
- The lists are text files in `config/carryon/` (forbidden and allowed blocks and mobs, forbidden and allowed stacking, placement state exceptions), written with the original's defaults on first run; entries are ids, ids with `*`, or `#tags`.
- `/carryon debug` shows what you carry, `/carryon clear` drops it from your hands, and operators can `clear` or `place` for chosen players.

## Differences from the original

- Players can't be picked up, the arms are not raised while carrying, custom pickup scripts and conditions are not supported, and there are no model overrides.
- A carried mob is shown over the hotbar in first person rather than in your arms, and as its spawn egg on other players.
- Each side reads its own lists; the server's decide what can be picked up.

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
