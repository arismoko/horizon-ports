# Secure Crops for Sunrise

Three game rules, all off by default, that keep farmland from being trampled:

- `securecrops:secure_farmland_and_crops`: farmland with a crop on it stays, and so does the crop.
- `securecrops:secure_farmland_break_crops`: farmland with a crop on it stays, and the crop breaks and drops its items.
- `securecrops:secure_farmland_if_empty`: farmland with nothing on it stays.

A port of [Secure Crops](https://github.com/zekk051/secure-crops) 2.0.1 by zekk051 to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), written in Rust on Sunrise's mod API, under the original's licence (MIT, in `LICENSE`; see `NOTICE`).

## Differences from the original

- Rule ids follow 26.3's namespaced form instead of `secureFarmlandAndCrops` and the like.
