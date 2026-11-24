# Minecraft Accelerated for Sunrise

Shears carve nine new faces into carved pumpkins, jack o'lanterns and soul jack o'lanterns:

- Right-click a carved pumpkin or jack o'lantern with shears to carve the next face; after the ninth it goes back to the plain one, and a soul jack o'lantern back to its first face. Left-click with shears in the main hand to go back a face.
- A carved face over a torch crafts its jack o'lantern, and over a soul torch its soul jack o'lantern. A plain soul jack o'lantern is carved back from the first face.
- Wear a carved face on your head and the view blurs through that face, and endermen don't notice you looking at them, as with a carved pumpkin.

The blocks come right after the carved pumpkin in the Natural Blocks tab. Install it on the server and the clients.

A port of [Minecraft Accelerated](https://github.com/Cookieglue/PumpkinsAccelerated) 1.0.0 by Mari to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), written in Rust on Sunrise's mod API, under the original's licence (CC0-1.0, in `LICENSE`; see `NOTICE`).

## Differences from the original

- A carved face or its lantern on top of a golem's body doesn't build the golem (sunrise-org/sunrise#5397).
- A snow golem doesn't wear or drop the face it was built with (sunrise-org/sunrise#5398).
- Mobs can spawn on the jack o'lanterns, as on vanilla's; the original's can't be spawned on (sunrise-org/sunrise#5399).
- Shears used on a plain soul jack o'lantern do nothing; the original plays the carving sound and then crashes the server.
