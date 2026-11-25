# Sooty Chimneys for Sunrise

Seven chimneys that smoke in the wind, get sooty while they work, and are scraped clean with an axe.

A port of [Sooty Chimneys](https://github.com/mortuusars/sootychimneys) 1.3.5 by mortuusars to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), written in Rust on Sunrise's mod API, under the original's licence (MIT, in `LICENSE`; see `NOTICE`).

## Differences from the original

- A chimney placed from an item is set up at once, but one set by a command or other code becomes stacked, lit or unlit only when a neighbour next changes (sunrise-org/sunrise#5302).
- The wind doesn't bend campfire smoke yet; the `WindAffectsCampfire` setting is kept but has no effect (sunrise-org/sunrise#5382).
- The optional JEI and Create integrations aren't part of this port.
