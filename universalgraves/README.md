# Universal Graves

Your items wait in a grave where you died.

A port of [Universal Graves](https://github.com/Patbox/UniversalGraves) by Patbox to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), written in Rust on Sunrise's mod API.

What it does:

- A player who dies without keepInventory leaves a grave instead of item drops: a grave block on the nearest free block within 8 of where they died, holding every stack. You are told where it is.
- Right-click the grave to take everything back: the stacks go into your inventory and what doesn't fit lands at your feet. Breaking it spills everything where it stands.
- Only the owner may take it for the first 15 minutes; after that, anyone may. Creative players may always. Others are told the grave isn't theirs.
- The grave can't be blown up, so its items are not lost to an explosion. Block JSONs have no push-reaction field, so pistons can still move it; its block entity moves with it.
- Settings (Options > Mods): the seconds only the owner may open a grave (default 900) and the seconds until a grave spills by itself (default 0, never).
- A grave with no free block within reach drops its items where you died, as the game would have.
- Graves are saved with the world, in the grave block's own data.

Differences from the original:

- The grave is a small gravestone block, not a player-head skull with the owner's name and a hologram, and has no screen: right-click takes everything at once.
- Stacks come back through your inventory in the order the game dropped them, not into the slots they came from, and worn armour and the off hand go to the inventory rather than their slots.
- The experience you carried is not held in the grave: it drops as orbs where you died, as in the game. The mod API can't give a player experience points directly.
- No death compass, no `/graves` command, no teleporting to a grave, no unlock costs, no grave limit per player, and no Trinkets slots.
- A grave removed by a command or an unusual mod loses its items; blasts can't remove it.

A port of Universal Graves 3.0.3 by Patbox, under the original's licence (LGPL-3.0-only, in `LICENSE`; see `NOTICE`).

## API differences

- Experience cannot be put into a grave or given back: no mod API call gives a player experience points, and the grave is made after the death's experience orbs are spawned.
- `living_drops` lists a dying player's inventory without slot numbers, so stacks return by `give_stack` order rather than to their original slots.
- A mod block entity cannot hear its own removal by command or by an unknown cause as a general drop point, so the grave is made unbreakable by explosions instead of spilling on every removal; the block is broken or taken only through the mod's hooks (`break_block`, `right_click_block`, expiry).
- A grave's expiry is checked once a second for graves made or loaded since the server started.
