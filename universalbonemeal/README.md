# Universal Bone Meal for Sunrise

Bone meal grows cactus, sugar cane, vines, nether wart, ripe melon and pumpkin stems, chorus, lily pads, dead bushes, small flowers, mushrooms on mycelium, ferns and berry bushes on podzol, coral in warm oceans and spore blossoms, and turns dirt beside grass or mycelium into it. Blocks in `#universalbonemeal:fertilizer_resistant_plants` never take bone meal.

A port of [Universal Bone Meal](https://github.com/Fuzss/universal-bone-meal) 26.3.1 by Fuzs to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), written in Rust on Sunrise's mod API, under the original's licence (MPL-2.0, in `LICENSE`; see `NOTICE`).

## Differences from the original

- Only the spread rules (lily pad, dead bush, small flowers) are data, in the `universalbonemeal:spread_rule` registry (`data/<namespace>/universalbonemeal/spread_rule/<name>.json`): `blocks` (a block, list or `#tag`), `width`, `most` and an optional `placeable` block predicate. The rest of the original's `bonemealables` data map and its behaviour registries stay code, and the registry is not sent to clients, so a client doesn't predict the spread.
- A copied plant or mushroom survives where the game's `canSurvive` for it says, so another mod's plant with its own rule is judged by it.
