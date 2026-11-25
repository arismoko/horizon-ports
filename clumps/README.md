# Clumps

A Sunrise 26.3 server-only port of [Clumps](https://github.com/jaredlll08/Clumps) (26.3) by Jaredlll08 (MIT).

## What it does

- **Unrestricted XP orb merging**: allows any two positive-value experience orbs in merge range to combine regardless of XP value or entity ID modulo 40, both on spawn (`tryMergeToExisting`) and during periodic scans (`scanForMerges`).
- **Single-pickup summed orbs**: writes the exact combined XP total (`into_value * into_count + from_value * from_count`) to the surviving orb with `count = 1`, resetting age on spawn merges and keeping the minimum age on resident merges. Orbs save and reload cleanly in vanilla NBT (`Value` short, `Count = 1`).
- **Overflow-safe clumping**: cancels merges whose combined XP would exceed vanilla's signed 16-bit `Value` limit (32,767 XP per orb) so excess orbs form a second clump without losing experience.
- **No 2-tick pickup cooldown**: clears `Player.takeXpDelay` to `0` on cooldown changes and at the end of each server tick for all online players (singleplayer, LAN host, and multiplayer guests), preserving vanilla pickup sounds, pickup particles, Mending repair (with `xp_repair_ratio`), and `pickup_xp` cancellation.

Server-side only: works in single player and on servers without requiring clients to install it.

## Differences from the original

- **Multiple touching clumps in the same tick**: clearing `takeXpDelay` removes the 2-tick delay so players collect orbs on every tick (20/s instead of 10/s), and any single clump is collected completely in one tick. When multiple separate clumps touch a player simultaneously, Sunrise collects one orb per tick rather than all touching orbs in the same tick; taking over pickup in `pickup_xp` to collect additional touching orbs cannot run vanilla Mending repair (`ExperienceOrb.repairPlayerItems`) for the XP a mod awards (gap: sunrise-org/sunrise#5593).
- **Per-source-orb Mending distribution**: Mending repairs damaged items using the clump's combined XP total (respecting `xp_repair_ratio` and overflowing remaining XP to other damaged Mending items), rather than re-rolling a random damaged Mending item for each original source orb in `clumpedMap`, because no mod can run vanilla Mending repair for the XP it awards; similarly, a single orb caps at vanilla's 32,767-XP NBT short limit before starting a second clump (gap: sunrise-org/sunrise#5593).
- **Third-party `ValueEvent` and `RepairEvent` API**: Clumps' mod-to-mod `ValueEvent` and `RepairEvent` events are not exposed because Sunrise Dev `bdad86d` does not yet have a mod-to-mod event bus (gap: sunrise-org/sunrise#5137).

## Building

```sh
cargo sunrise-mod check
cargo sunrise-mod build
CARGO_TARGET_DIR=target cargo test
cargo clippy --all-targets -- -D warnings
```

## Licence

MIT, matching the original project. See `LICENSE` and `NOTICE`.
