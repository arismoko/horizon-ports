# Better Days for Sunrise

Set how long day and night last, and let sleeping change the speed of time.

A port of [Better Days](https://github.com/wendall911/BetterDays) 4.1.1.7 by wendall911 to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), under the original's licence (LGPL-3.0, in `LICENSE`, `COPYING` and `COPYING.LESSER`; see `NOTICE`). One native mod on Sunrise's mod API.

## What it does

- Sets the overworld clock's speed every tick from your settings: separate day and night ratios, lengths in real minutes, 12 real hours, or a day length by season and latitude, with the day beginning at `dayStart` and the night at `nightStart`.
- Interpolated time: a list of `tick,speed` pairs with a smooth curve between them.
- Sleeping speeds time up with the share of players in bed (a tunable curve between `sleepSpeedMin` and `sleepSpeedMax`, a fixed speed when all sleep, and a minimum share to start). Vanilla's night skip is held back while they sleep; at sunrise the morning notice goes out and everyone wakes up.
- Hides vanilla's "players sleeping" line above the hotbar in the overworld while the sleep feature is on.
- Notices when someone gets into or out of bed and when the morning comes, with `${player}`, `${totalPlayers}`, `${sleepingPlayers}` and `${sleepingPercentage}` and `§` colour codes, sent as a chat line or above the hotbar, to everyone, the dimension or the sleepers.
- A clock on the in-bed screen, placed by `clockAlignment`, `clockScale` and `clockMargin`.
- `config/betterdays-common.toml` and `config/betterdays-client.toml`, reloaded live.

## Differences from the original

- The weather, potion, hunger and block entity effects are not in this version (sunrise-org/sunrise#5281, sunrise-org/sunrise#5282).
- The season mode uses the early-spring day, as the original does without a seasons mod.
- There is no in-game config screen (sunrise-org/sunrise#1217).
