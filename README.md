# arismoko's Sunrise ports

Mods ported to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), each rewritten in Rust on Sunrise's mod API. Each folder holds one port with its own `sunrise-mod.toml`, the original's licence and a `NOTICE` saying where it comes from; each port is under its original's licence. No Java code is copied: the behaviour is written anew from the original.

Most of these were first ported to Minecraft 26.3 by lavoide, for the Homestead modpack, and have been moved to Sunrise and kept up to date by arismoko.

## Install

Open the Sunrise launcher's **Discover** page and search for the mod's name. Only listed ports are there: each is a signed, reviewed release built for the game build you play, and the launcher builds it on your computer when you install it.

## Build one

Each folder is a crate on Sunrise's mod API. With `cargo sunrise-mod` (from the Sunrise SDK) installed:

```sh
cd carryon
cargo sunrise-mod build    # compiles against the game build pinned in sunrise-mod.toml, packs target/sunrise-mod/<mod>.smod
cargo sunrise-mod run      # builds it and starts the game with it
cargo test
```

Releases are tagged `<folder>/v<version>`; the shared Sunrise release workflow builds and signs each one, and the registry lists it once it is checked and reviewed.

## Ports

"Listed" ports are on the registry. "Ported, not listed" ports build on Sunrise but are held back until the game issues they wait on are fixed. Ports marked "Blocked" or "Not ported yet" still hold an earlier port's source in an older mod format: they do not build with `cargo sunrise-mod` until they are ported.

| Port | What it does | Original | Licence | State |
|---|---|---|---|---|
| [rightclickharvest](rightclickharvest/) | Right-click a grown crop to harvest and replant it. | [Right Click Harvest](https://github.com/JamCoreModding/right-click-harvest) by Jamalam | MIT | Listed |
| [cropsloverain](cropsloverain/) | Crops grow faster in the rain. | [Crops Love Rain](https://github.com/ThePoultryMan/Crops-Love_Rain) by ThePoultryMan | MIT | Listed |
| [preferredgamerules](preferredgamerules/) | New worlds start with your preferred game rules. | [Preferred Gamerules](https://github.com/Estecka/mc-Preferred-Gamerules) by Estecka | MIT | Listed |
| [securecrops](securecrops/) | Farmland isn't trampled by players who land softly. | [Secure Crops](https://github.com/zekk051/secure-crops) by zekk051 | MIT | Listed |
| [invisibleframes](invisibleframes/) | Sneak-use an item frame to make it invisible. | [Invisible Frames](https://github.com/Roundaround/mc-fabric-invisible-frames) by Roundaround | MIT | Ported, not listed: waits on [#5357](https://github.com/sunrise-org/sunrise/issues/5357), [#5358](https://github.com/sunrise-org/sunrise/issues/5358) |
| [craftingpad](craftingpad/) | A portable crafting table. | [Crafting Pad](https://github.com/StrikerRockers-Mods/CraftingPad) by StrikerRocker, Giselbaer | MIT | Blocked on [#4592](https://github.com/sunrise-org/sunrise/issues/4592) |
| [toocheap](toocheap/) | Anvil repairs never become Too Expensive. | [Too Cheap!](https://github.com/the-sh4d0w/too-cheap) by Sh4d0w | MIT | Blocked on [#5375](https://github.com/sunrise-org/sunrise/issues/5375) |
| [sootychimneys](sootychimneys/) | Chimneys that smoke, with wind, soot and scraping. | [Sooty Chimneys](https://github.com/mortuusars/sootychimneys) by mortuusars | MIT | Ported, not listed: waits on [#1244](https://github.com/sunrise-org/sunrise/issues/1244), [#5302](https://github.com/sunrise-org/sunrise/issues/5302), [#5382](https://github.com/sunrise-org/sunrise/issues/5382), [#5383](https://github.com/sunrise-org/sunrise/issues/5383), [#5385](https://github.com/sunrise-org/sunrise/issues/5385), [#5063](https://github.com/sunrise-org/sunrise/issues/5063) |
| [things](things/) | Trinkets and gadgets from Things (subset). | [Things](https://github.com/glisco03/things) by glisco | MIT | Not ported yet |
| [trinkets](trinkets/) | Extra equipment slots for trinkets, in a menu on G. | [Trinkets](https://github.com/emilyploszaj/trinkets) by Emi, C4 | MIT | Not ported yet |
| [trinketlantern](trinketlantern/) | Wearable lanterns for the trinket lantern slot. | [Wearable lanterns](https://github.com/AbsentAbigail/WearableLantern) by tsuki, Absent Abigail | MIT | Not ported yet |
| [convenientnametags](convenientnametags/) | Name tags drop on death, can be sheared off and renamed. | [Convenient Name Tags](https://www.curseforge.com/projects/662763) by Mim1q | Apache-2.0 | Blocked on [#5380](https://github.com/sunrise-org/sunrise/issues/5380) |
| [bebpm](bebpm/) | Enchanting tables count bookshelves further away. | [BEBPM](https://www.curseforge.com/projects/1462757) by CozyStudios | CC0-1.0 | Blocked on [#5374](https://github.com/sunrise-org/sunrise/issues/5374) |
| [safespawn](safespawn/) | New players spawn somewhere safe. | [SafeSpawn](https://github.com/CozyCord/SafeSpawn) by Tokimi, CozyStudios | CC0-1.0 | Listed |
| [obsidianequipmentrework](obsidianequipmentrework/) | Obsidian tools and armour. | [Obsidian Equipment Reworked](https://github.com/CozyCord/Obsidian-Equipment-Reworked) by Tokimi, CozyStudios | CC0-1.0 | Ported, not listed: waits on [#4991](https://github.com/sunrise-org/sunrise/issues/4991), [#5308](https://github.com/sunrise-org/sunrise/issues/5308), [#5359](https://github.com/sunrise-org/sunrise/issues/5359), [#5362](https://github.com/sunrise-org/sunrise/issues/5362), [#1209](https://github.com/sunrise-org/sunrise/issues/1209) |
| [mcaccel](mcaccel/) | Pumpkins Accelerated: carved pumpkin faces and more. | [Minecraft Accelerated](https://github.com/Cookieglue/PumpkinsAccelerated) by Mari | CC0-1.0 | Listed |
| [tokimistoolshed](tokimistoolshed/) | Hammers, excavators, scythes, copper buckets and other tools. | [Tokimi's Toolshed](https://github.com/CozyCord/TokimisToolshed) by Tokimi, CozyStudios | CC0-1.0 | Not ported yet |
| [smwyg](smwyg/) | Ctrl+T over an item puts it in chat, with its tooltip on hover. | [Show Me What You Got](https://github.com/apace100/show-me-what-you-got) by Apace | MIT | Ported, not listed: waits on [#5377](https://github.com/sunrise-org/sunrise/issues/5377) |
| [bridgingmod](bridgingmod/) | Right-click past an edge to bridge blocks out, Bedrock style. | [Bridging Mod](https://github.com/CloudG360/BridgingMod) by CG360 | MIT | Not ported yet |
| [mousewheelie](mousewheelie/) | Scroll items between inventories, sort with the middle button, pick the right tool. | [Mouse Wheelie](https://github.com/Siphalor/mouse-wheelie) by Siphalor | Apache-2.0 | Not ported yet |
| [emojiful](emojiful/) | :short_name: in chat shows the emoji (Twemoji), with completions as you type. | [Emojiful](https://github.com/InnovativeOnlineIndustries/Emojiful) by HorizonStudio | MIT | Blocked on [#382](https://github.com/sunrise-org/sunrise/issues/382), [#5294](https://github.com/sunrise-org/sunrise/issues/5294) |
| [yungsbridges](yungsbridges/) | Bridges over rivers. | [YUNG's Bridges](https://github.com/yungnickyoung/YUNGs-Bridges) by YUNGNICKYOUNG | LGPL-3.0-only | Listed |
| [betterdays](betterdays/) | Slower or faster days and nights. | [BetterDays](https://github.com/wendall911/BetterDays) by wendall911 | LGPL-3.0-or-later | Listed |
| [comforts](comforts/) | Sleeping bags and hammocks. | [Comforts](https://github.com/illusivesoulworks/comforts) by Illusive Soulworks | LGPL-3.0-or-later | Not ported yet |
| [elytraslot](elytraslot/) | Glide with an elytra worn in the trinket cape slot. | [Elytra Slot](https://github.com/illusivesoulworks/elytraslot) by Illusive Soulworks | LGPL-3.0-or-later | Not ported yet |
| [radiantgear](radiantgear/) | Worn trinkets that glow give dynamic light. | [Radiant Gear](https://github.com/illusivesoulworks/radiantgear) by Illusive Soulworks | LGPL-3.0-or-later | Not ported yet |
| [universalgraves](universalgraves/) | Your items wait in a grave where you died. | [Universal Graves](https://github.com/Patbox/UniversalGraves) by Patbox | LGPL-3.0-only | Not ported yet |
| [grindenchantments](grindenchantments/) | Grindstones move enchantments to books. | [Grind Enchantments](https://github.com/mschae23/grind-enchantments) by mschae23 | LGPL-3.0-or-later | Blocked on [#5391](https://github.com/sunrise-org/sunrise/issues/5391), [#5392](https://github.com/sunrise-org/sunrise/issues/5392), [#5393](https://github.com/sunrise-org/sunrise/issues/5393) |
| [respectmytrims](respectmytrims/) | Piglins respect gold armour trims. | [Respect My Trims](https://github.com/tnoctua/respectmytrims) by N (tnoctua) | LGPL-2.1-only | Listed |
| [blockrunner](blockrunner/) | Walk faster on path blocks. | [Block Runner](https://github.com/Fuzss/blockrunner) by Fuzs | MPL-2.0 | Listed |
| [easymagic](easymagic/) | Enchanting tables keep their items and can re-roll. | [Easy Magic](https://github.com/Fuzss/easymagic) by Fuzs | MPL-2.0 | Not ported yet |
| [armorstatues](armorstatues/) | Sneak and use an armor stand with an empty hand to pose, style and move it. | [Armor Statues](https://github.com/Fuzss/armorstatues) by Fuzs | MPL-2.0 | Being ported |
| [universalbonemeal](universalbonemeal/) | Bone meal grows cactus, sugar cane, vines, flowers, coral, chorus and more. | [Universal Bone Meal](https://github.com/Fuzss/universalbonemeal-forge) by Fuzs | MPL-2.0 | Listed |
| [accelerateddecay](accelerateddecay/) | Leaves decay quickly after a tree is cut. | [Accelerated Decay](https://github.com/errormikey/accelerated-decay) by ErrorMikey | GPL-3.0-only | Not ported yet |
| [betterthanmending](betterthanmending/) | Sneak-use to repair mending gear with experience. | [BetterThanMending](https://github.com/legobmw99/BetterThanMending/tree/fabric) by legobmw99 | GPL-3.0-only | Listed |
| [fallingtrees](fallingtrees/) | Trees fall when their base is cut. | [Panda's Falling Tree's](https://github.com/PandaMods-Dev/Pandas-Falling-Trees) by The Panda Oliver | GPL-3.0-only | Listed |
| [carryon](carryon/) | Pick up chests with their contents, and mobs, and carry them. | [Carry On](https://github.com/Tschipp/CarryOn) by Tschipp | LGPL-3.0 | Listed |
| [toms_storage](toms_storage/) | Join chests into one network and reach it all from a storage terminal. | [Tom's Simple Storage](https://github.com/tom5454/Toms-Storage) by tom5454 | MIT | Not ported yet |
| [sophisticatedcore](sophisticatedcore/) | Sophisticated Backpacks and Sophisticated Storage in one package. | [Sophisticated Core](https://github.com/Salandora/SophisticatedCore), Backpacks and Storage (Fabric ports by Salandora; originals by P3pp3rF1y) | GPL-3.0 | Ported, not listed: in-game check pending; gaps [#5050](https://github.com/sunrise-org/sunrise/issues/5050), [#4592](https://github.com/sunrise-org/sunrise/issues/4592), [#5099](https://github.com/sunrise-org/sunrise/issues/5099), [#5110](https://github.com/sunrise-org/sunrise/issues/5110) |
| [clutter](clutter/) | Furniture you can sit on, candles and chandeliers, lamps, curtains that open, cupboards, copper, silver and onyx. | [Clutter](https://github.com/EmilsGithub/Clutter-Mod-Repository) by EmilSG | MIT | Not ported yet |
| [spelunkery](spelunkery/) | Ores in granite, andesite, diorite and tuff, rough gems and raw nuggets, cave mushrooms, rock salt, glowsticks, rope ladders and mining tools. | [Spelunkery](https://github.com/AstralOrdana/Spelunkery) by Ordana and Keybounce | LGPL-3.0-only | Not ported yet |
| [mythicupgrades](mythicupgrades/) | Eight gems and necoium with their ores and crystal geodes, and armour sets and tools that each have their own effect. | [Mythic Upgrades](https://github.com/TriQue06/mythic-upgrades-multiloader/tree/1.20.1) by TriQue | MIT | Not ported yet |

Bugs and requests go in this repository's issues.
