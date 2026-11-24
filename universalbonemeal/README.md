# Universal Bone Meal for Sunrise

Bone meal grows cactus, sugar cane, vines, nether wart, ripe melon and pumpkin stems, chorus, lily pads, dead bushes, small flowers, mushrooms on mycelium, ferns and berry bushes on podzol, coral in warm oceans and spore blossoms, and turns dirt beside grass or mycelium into it. Blocks in `#universalbonemeal:fertilizer_resistant_plants` never take bone meal.

A port of [Universal Bone Meal](https://github.com/Fuzss/universal-bone-meal) 26.3.1 by Fuzs to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), written in Rust on Sunrise's mod API, under the original's licence (MPL-2.0, in `LICENSE`; see `NOTICE`).

## Differences from the original

- Data packs can't change the growth rules: the original's `universalbonemeal:bonemealables` data map and its behaviour and predicate registries are the port's code (sunrise-org/sunrise#5368).
- Copied plants and grown mushrooms follow vanilla's survival rules for their kind; another mod's plant with a rule of its own is judged by the vanilla one (sunrise-org/sunrise#3596).
- Sky light over dirt is judged from the block's shape and fluid, air from `#minecraft:air`, and another mod's bone-mealable block is left to it only when it is a valid target (sunrise-org/sunrise#5364).
- A damageable fertiliser wears without Unbreaking (sunrise-org/sunrise#5369).
