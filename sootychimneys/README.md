# Sooty Chimneys for Sunrise

Seven chimneys that smoke in the wind, get sooty while they work, and are scraped clean with an axe.

A port of [Sooty Chimneys](https://github.com/mortuusars/sootychimneys) 1.3.5 by mortuusars to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), written in Rust on Sunrise's mod API, under the original's licence (MIT, in `LICENSE`; see `NOTICE`).

## Differences from the original

- Placing a chimney doesn't set it up at once: it becomes stacked, lit or unlit when a neighbour next changes (sunrise-org/sunrise#5302).
- The wind doesn't bend campfire smoke yet; the `WindAffectsCampfire` setting is kept but has no effect (sunrise-org/sunrise#5382).
- Breaking a dirty chimney doesn't puff a cloud of soot (sunrise-org/sunrise#5383).
- The wind doesn't change between day and night yet: every hour counts as day (sunrise-org/sunrise#5385).
- Scraping a chimney shows its scrape sound, soot and arm swing to other players, but not to the player who scrapes (sunrise-org/sunrise#5063).
- The optional JEI and Create integrations aren't part of this port.
