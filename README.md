# arismoko's Sunrise ports

The mods arismoko maintains or is building for [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), each rewritten in Rust on Sunrise's mod API. Each folder holds one port with its own `sunrise-mod.toml`, the original's licence and a `NOTICE` saying where it comes from; each port is under its original's licence. No Java code is copied: the behaviour is written anew from the original.

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

## Mods

"Listed" mods are on the registry. "Ported, not listed" mods build on Sunrise but are held back until the game issues they wait on are fixed. "Being ported" mods are being rewritten now; until their port lands, their folders still hold the earlier source, which does not build with `cargo sunrise-mod`.

| Mod | What it does | Original | Licence | State |
|---|---|---|---|---|
| [betterdays](betterdays/) | Slower or faster days and nights. | [BetterDays](https://github.com/wendall911/BetterDays) by wendall911 | LGPL-3.0-or-later | Listed |
| [betterthanmending](betterthanmending/) | Sneak-use to repair mending gear with experience. | [BetterThanMending](https://github.com/legobmw99/BetterThanMending/tree/fabric) by legobmw99 | GPL-3.0-only | Listed |
| [blockrunner](blockrunner/) | Walk faster on path blocks. | [Block Runner](https://github.com/Fuzss/blockrunner) by Fuzs | MPL-2.0 | Listed |
| [carryon](carryon/) | Pick up chests with their contents, and mobs, and carry them. | [Carry On](https://github.com/Tschipp/CarryOn) by Tschipp | LGPL-3.0-only | Listed |
| [cropsloverain](cropsloverain/) | Crops grow faster in the rain. | [Crops Love Rain](https://codeberg.org/ThePoultryMan/Crops-Love-Rain) by ThePoultryMan | MIT | Listed |
| [fallingtrees](fallingtrees/) | Trees fall when their base is cut. | [Panda's Falling Tree's](https://github.com/PandaMods-Dev/Pandas-Falling-Trees) by The Panda Oliver | GPL-3.0-only | Listed |
| [mcaccel](mcaccel/) | Pumpkins Accelerated: carved pumpkin faces and more. | [Minecraft Accelerated](https://github.com/Cookieglue/PumpkinsAccelerated) by Mari | CC0-1.0 | Listed |
| [preferredgamerules](preferredgamerules/) | New worlds start with your preferred game rules. | [Preferred Gamerules](https://github.com/Estecka/mc-Preferred-Gamerules) by Estecka | MIT | Listed |
| [respectmytrims](respectmytrims/) | Piglins respect gold armour trims. | [Respect My Trims](https://github.com/tnoctua/respectmytrims) by N (tnoctua) | LGPL-2.1-only | Listed |
| [rightclickharvest](rightclickharvest/) | Right-click a grown crop to harvest and replant it. | [Right Click Harvest](https://github.com/JamCoreModding/right-click-harvest) by Jamalam | MIT | Listed |
| [safespawn](safespawn/) | New players spawn somewhere safe. | SafeSpawn by Tokimi, CozyStudios | CC0-1.0 | Listed |
| [securecrops](securecrops/) | Farmland isn't trampled by players who land softly. | [Secure Crops](https://github.com/zekk051/secure-crops) by zekk051 | MIT | Listed |
| [toms_storage](toms_storage/) | Join chests into one network and reach it all from a storage or crafting terminal. | [Tom's Simple Storage](https://github.com/tom5454/Toms-Storage) by tom5454 | MIT | Being ported |
| [universalbonemeal](universalbonemeal/) | Bone meal grows cactus, sugar cane, vines, flowers, coral, chorus and more. | [Universal Bone Meal](https://github.com/Fuzss/universalbonemeal-forge) by Fuzs | MPL-2.0 | Listed |
| [yungsbridges](yungsbridges/) | Bridges over rivers. | [YUNG's Bridges](https://github.com/yungnickyoung/YUNGs-Bridges) by YUNGNICKYOUNG | LGPL-3.0-only | Listed |
| [invisibleframes](invisibleframes/) | Sneak-use an item frame to make it invisible. | [Invisible Frames](https://github.com/Roundaround/mc-fabric-invisible-frames) by Roundaround | MIT | Ported, not listed: waits on [#5357](https://github.com/sunrise-org/sunrise/issues/5357), [#5358](https://github.com/sunrise-org/sunrise/issues/5358) (fixed on main, in the next Dev build) |
| [obsidianequipmentrework](obsidianequipmentrework/) | Obsidian tools and armour. | [Obsidian Equipment Reworked](https://github.com/CozyCord/Obsidian-Equipment-Reworked) by Tokimi, CozyStudios | CC0-1.0 | Ported, not listed: waits on [#4991](https://github.com/sunrise-org/sunrise/issues/4991), [#5308](https://github.com/sunrise-org/sunrise/issues/5308), [#5359](https://github.com/sunrise-org/sunrise/issues/5359), [#5362](https://github.com/sunrise-org/sunrise/issues/5362), [#1209](https://github.com/sunrise-org/sunrise/issues/1209) |
| [shouldersurfing](shouldersurfing/) | An over-the-shoulder third-person camera with a decoupled view and a crosshair that follows what you act on. | [Shoulder Surfing Reloaded](https://github.com/Exopandora/ShoulderSurfing) by Exopandora | MIT | Ported, not listed: in-game check pending; gaps #5333, #5553, #5691, #5692 |
| [smwyg](smwyg/) | Ctrl+T over an item puts it in chat, with its tooltip on hover. | [Show Me What You Got](https://github.com/apace100/show-me-what-you-got) by Apace | MIT | Ported, not listed: in-game check on Dev 1 pending |
| [sootychimneys](sootychimneys/) | Chimneys that smoke, with wind, soot and scraping. | [Sooty Chimneys](https://github.com/mortuusars/sootychimneys) by mortuusars | MIT | Ported, not listed: waits on [#1244](https://github.com/sunrise-org/sunrise/issues/1244), [#5302](https://github.com/sunrise-org/sunrise/issues/5302), [#5382](https://github.com/sunrise-org/sunrise/issues/5382), [#5383](https://github.com/sunrise-org/sunrise/issues/5383), [#5385](https://github.com/sunrise-org/sunrise/issues/5385), [#5063](https://github.com/sunrise-org/sunrise/issues/5063) (fixed on main, in the next Dev build) |
| [sophisticatedcore](sophisticatedcore/) | Sophisticated Backpacks and Sophisticated Storage in one package. | [Sophisticated Core](https://github.com/Salandora/SophisticatedCore), Backpacks and Storage (Fabric ports by Salandora; originals by P3pp3rF1y) | GPL-3.0-only | Ported, not listed: in-game check pending; gaps [#5050](https://github.com/sunrise-org/sunrise/issues/5050), [#4592](https://github.com/sunrise-org/sunrise/issues/4592), [#5099](https://github.com/sunrise-org/sunrise/issues/5099), [#5110](https://github.com/sunrise-org/sunrise/issues/5110) |
| [accelerateddecay](accelerateddecay/) | Leaves decay quickly after a tree is cut. | [Accelerated Decay](https://github.com/ErrorMikey/AcceleratedDecay) by ErrorMikey | GPL-3.0-only | Being ported |
| [armorhud](armorhud/) | Shows your worn armour and its durability in a small widget beside the hotbar. | [Armor HUD](https://github.com/uku3lig/armor-hud) by uku, BerdinskiyBear | MIT | Being ported |
| [armorstatues](armorstatues/) | Sneak and use an armor stand with an empty hand to pose, style and move it. | [Armor Statues](https://github.com/Fuzss/armorstatues) by Fuzs | MPL-2.0 | Being ported |
| [easymagic](easymagic/) | Enchanting tables keep their items and can re-roll. | [Easy Magic](https://github.com/Fuzss/easymagic) by Fuzs | MPL-2.0 | Being ported |
| [overflowingbars](overflowingbars/) | Health, armor and armor toughness stack into coloured rows with row counts. | [Overflowing Bars](https://github.com/Fuzss/overflowing-bars) by Fuzs | MPL-2.0 | Being ported |
| [cutthrough](cutthrough/) | Attacks pass through tall grass and flowers to hit mobs behind them. | [Cut Through](https://github.com/Fuzss/cut-through) by Fuzs | MPL-2.0 | Being ported |
| [enchantmentdescriptions](enchantmentdescriptions/) | Enchanted books and gear show what each enchantment does in their tooltips. | [Enchantment Descriptions](https://github.com/Darkhax-Minecraft/Enchantment-Descriptions) by Darkhax | LGPL-2.1-only | Being ported |
| [travelerstitles](travelerstitles/) | Shows a fading title when you enter a new biome or dimension. | [Traveler's Titles](https://github.com/YUNG-GANG/Travelers-Titles) by YUNGNICKYOUNG | LGPL-3.0-only | Being ported |
| [invmove](invmove/) | Walk, jump, sneak and sprint while inventories and other screens are open. | [InvMove](https://github.com/PieKing1215/InvMove) by PieKing1215 | LGPL-3.0-only | Being ported |

Some arismoko ports live in their own repositories, such as the YUNG's and Macaw's ports, Iron Furnaces and Jade; their pages on the registry link to them.

Mods on our radar that aren't being worked on now are in [RADAR.md](RADAR.md), with what each needs before it can be finished. Their earlier sources are on the [`parked`](https://github.com/arismoko/sunrise-ports/tree/parked) branch.

Bugs and requests go in this repository's issues.
| [mousewheelie](mousewheelie/) | Scroll items between inventories, sort with the middle button, refill the hotbar. | [Mouse Wheelie](https://github.com/Siphalor/mouse-wheelie) by Siphalor | Apache-2.0 | Being ported |
| [badwithernocookie](badwithernocookie/) | Wither, dragon and other loud sounds are heard only nearby; Books of Silence quiet chosen sounds for the wearer. | [Bad Wither No Cookie - Reloaded](https://gitlab.com/kreezxil/BadWitherNoCookie) by kreezxil | MIT | Ready to list |
| [bebpm](bebpm/) | Enchanting tables count bookshelves further away. | [BEBPM](https://www.curseforge.com/projects/1462757) by CozyStudios | CC0-1.0 | Ready to list |
| [morechathistory](morechathistory/) | Keeps more chat history than vanilla's 100 lines. | [More Chat History](https://github.com/JackFredMods/MoreChatHistory) by JackFred | CC0-1.0 | Ready to list |
