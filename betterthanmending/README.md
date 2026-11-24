# Better Than Mending for Sunrise

Sneak and use a damaged item that has Mending to repair it with your own experience.

- With more than 2 points of experience, sneaking and using the item repairs it instead of using it.
- Each use spends 20 points when you have at least 30 and the item lacks at least 40 durability, otherwise 2 points.
- The item gets back what Mending gives for those points: 40 or 4 durability.

A port of [Better Than Mending](https://github.com/legobmw99/BetterThanMending) by legobmw99 to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), written in Rust on Sunrise's mod API, under the original's licence (GPL-3.0-only, in `LICENSE`; see `NOTICE`).

Differences from the original:

- The durability per point is vanilla Mending's (2). The original asks the item's enchantments, so a data pack that changes Mending's rate changes the repair there: [sunrise#5350](https://github.com/sunrise-org/sunrise/issues/5350).
