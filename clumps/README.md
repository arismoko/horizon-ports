# Clumps

A Sunrise 26.3 server-only port of [Clumps](https://github.com/jaredlll08/Clumps) (26.3) by Jaredlll08 (MIT).

## What it does

- **Unrestricted XP orb merging**: allows any two positive-value experience orbs in merge range to combine regardless of XP value or entity ID modulo 40, both on spawn (`tryMergeToExisting`) and during periodic scans (`scanForMerges`).
- **Single-pickup summed orbs**: writes the exact combined XP total (`into_value * into_count + from_value * from_count`) to the surviving orb with `count = 1`, resetting age on spawn merges and keeping the minimum age on resident merges. Orbs save and reload cleanly in vanilla NBT (`Value` short, `Count = 1`).
- **Overflow-safe clumping**: cancels merges whose combined XP would exceed vanilla's signed 16-bit `Value` limit (32,767 XP per orb) so excess orbs form a second clump without losing experience.
- **No 2-tick pickup cooldown**: clears `Player.takeXpDelay` to `0` on cooldown changes and at the end of each server tick for all online players (singleplayer, LAN host, and multiplayer guests), preserving vanilla pickup sounds, pickup particles, Mending repair (with `xp_repair_ratio`), and `pickup_xp` cancellation.

Server-side only: works in single player and on servers without requiring clients to install it.

## Differences from the original

- **Multiple touching clumps in the same tick:** Removing `takeXpDelay` lets players collect orbs on every tick (20/s instead of 10/s), and any single clump is collected completely in one tick. When multiple separate clumps touch a player at once, Sunrise collects one orb per tick, as vanilla's `Player.aiStep` does, rather than all touching orbs in the same tick.
- **Per-source-orb Mending distribution:** Mending repairs with the clump's full XP total in one `repairPlayerItems` run (respecting `xp_repair_ratio`, firing `player_item_mend`, and spilling leftover XP to other damaged Mending items), rather than re-rolling a random damaged Mending item for each original source orb merged into the clump, because merged orbs keep only their sum.
- **Event API shape:** `ValueEvent` and `RepairEvent` are typed Sunrise mod events (`clumps:value`, `clumps:repair`) that dependent mods subscribe to with `#[subscribe]`, not Java classes. The pickup is taken over to run Mending through the engine, so the vanilla pickup sound and particles for a clump are not replayed.

## Building

```sh
cargo sunrise-mod check
cargo sunrise-mod build
CARGO_TARGET_DIR=target cargo test
cargo clippy --all-targets -- -D warnings
```

## Licence

MIT, matching the original project. See `LICENSE` and `NOTICE`.
