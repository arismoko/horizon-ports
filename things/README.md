# Things

Trinkets and gadgets from Things 0.3.3 by glisco.

Twenty-odd gadgets and trinkets. Wear the trinkets in the slots of the Trinkets mod (G); the tools work on their own.

What it does:

- Worn trinkets: Monocle (night vision), Moss Necklace (regeneration in light), Mining Gloves (haste), Riot Gauntlet (strength), Rabbit Foot Charm (jump boost), Hades Crystal (fire resistance), Socks (speed, and a jump boost toggled with Caps Lock), Arm Extender (more reach), Enchanted Wax Gland (water movement), Shock Absorber (less fall and wall damage), Placebo (potions sometimes fail) and Luck of the Irish (a poisonous potato heals).
- Item Magnet: use it to pull items you look at; it recharges over time.
- Infernal Scepter: shoots a fireball for a fire charge, 64 uses.
- Container Key: sneak-use it on a chest, barrel, furnace or other container to lock it; only a key with the same number opens it again.
- Recall Potion: takes you to your respawn point.
- Bater Wucket: a water bucket that is never emptied; it fills cauldrons too.
- Hardening Catalyst: on an anvil it makes a tool or armour piece unbreakable for 30 levels.
- Gleaming ore, gleaming powder and compound, and glowstone fixtures.

Differences from the original:

- Without the Trinkets mod the trinkets cannot be worn; the rest works.
- Effects are refreshed every half second with two seconds left, so one lingers a moment after the trinket comes off.
- The sock upgrade recipes are not ported; socks are crafted as they come.
- The Almanac is an item only: the Lavender book pages are not ported.
- The Bater Wucket places water from a click on a block, at the end of the server tick; water in the Nether does nothing.

A port of Things by glisco, under the MIT licence.

## Remaining API differences

- The sock upgrade recipes (`SockUpgradeRecipe`, `JumpySocksRecipe`) need custom recipe serializers; the mod API has none, so their recipes are left out.
- The Bater Wucket is a click-on-block rule; it has no client prediction and does not pick water up.
- Effects are `effect give` commands run at the server tick's end; Java gives five ticks every tick.
- Not checked in game: this was written without building.

## Building

With the Sunrise mod tool (`cargo sunrise-mod`, from the Sunrise SDK) and Rust:

```sh
cargo sunrise-mod build    # compiles against the pinned game, packs target/sunrise-mod/things.smod
cargo sunrise-mod run      # builds it and starts the game with it
cargo test
```

`sunrise-mod.toml` pins the Sunrise build it is made for (`sunrise`, a game commit). It lists `trinkets` as an optional dependency: the worn trinkets are read from its `trinkets:worn` player attachment (`registrar.attachment_key::<Compound>("trinkets:worn")`).

## Licence and credits

Things is by glisco (https://github.com/glisco03/things), under the MIT licence; `LICENSE` holds the text and `NOTICE` says where the port comes from. No Java code is included.

Ported to Minecraft 26.3 by lavoide ([lavoide/horizon-ports](https://github.com/lavoide/horizon-ports)), and to Sunrise by arismoko.
