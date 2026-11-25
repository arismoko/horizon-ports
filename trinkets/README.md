# Trinkets

Extra equipment slots for trinkets, worn from a menu on G.

Extra equipment slots for the trinkets other mods add: face, hat, necklace, back, cape, charm, glove, two rings, belt, wearable lantern, key, quiver, shoes and a second charm.

What it does:

- Press G (Open Trinkets in Controls) to open the Trinkets menu: a slot for each trinket place, each showing its picture while empty. Click an item in your inventory to wear it in the first empty slot that takes it (or swap with the first that does); click a worn item to take it off.
- Right-click a trinket in your hand to wear it in the first empty slot that takes it. Backpacks and wireless terminals keep their own use; an elytra still goes in the chest slot.
- An item's tooltip lists the trinket slots it fits, and the key that opens the menu.
- What you wear is saved with you, kept through death, and shown to your client. When you die without keepInventory your worn trinkets drop where you died (items with Curse of Vanishing are destroyed).
- Other mods read the worn stacks from the `trinkets:worn` player attachment: Sophisticated Backpacks' back slot, Elytra Slot, Things, Radiant Gear and others.

Differences from the original:

- A worn Totem of Undying does not save you from death: the game has no hook for an item other than a held one to answer death protection (NeoForge's LivingUseTotemEvent).
- The menu is a two-row chest menu with the slot pictures as placeholder items, not the original's screen beside your inventory.
- Right-click wearing is done by the server only; the client does not predict it.
- Dropped trinkets are not put in a grave by a grave mod: no grave mod is ported.


## Package API for other mods

- The worn stacks are the player attachment `trinkets:worn` (a compound of slot id to item NBT, for example `chest/back` or `chest/cape`; synced to the client and kept through death). A mod reads it with `registrar.attachment_key::<Compound>("trinkets:worn")` in `Hooks::register`, after declaring `trinkets` as a dependency (`AttachmentError::Undeclared` otherwise). Sophisticated Backpacks' back slot (`sophisticatedcore`) does this.
- The slot ids are `head/face`, `head/hat`, `chest/necklace`, `chest/back`, `chest/cape`, `chest/charm`, `hand/glove`, `hand/ring`, `offhand/ring`, `legs/belt`, `legs/lantern`, `legs/key`, `legs/quiver`, `feet/shoes` and `charm/charm`.

## Remaining API differences

- Worn Totem of Undying: no hook answers death protection for anything but a held totem (NeoForge `LivingUseTotemEvent`, vanilla `LivingEntity.checkTotemDeathProtection`). The earlier port patched the game for it; here it is left out. Not filed as an issue yet.
- The slot menu is a two-row chest menu with placeholder items; the original draws its slots in a screen beside the inventory. Clicking is by `menu_clicked`.
- Right-click wearing is server-only (the client does not predict it).
- Death drop uses `living_death` and the `minecraft:keep_inventory` game rule; a grave mod taking the items first (universalgraves in the earlier pack) is not ported.
- The Open Trinkets key listens to keyboard bindings only (a mouse button binding does not open the menu).
- The vanishing-curse check reads `minecraft:enchantments` in the stack's components.
- Not checked in game: this was written without building.

## Building

With the Sunrise mod tool (`cargo sunrise-mod`, from the Sunrise SDK) and Rust:

```sh
cargo sunrise-mod build    # compiles against the pinned game, packs target/sunrise-mod/trinkets.smod
cargo sunrise-mod run      # builds it and starts the game with it
cargo test
```

`sunrise-mod.toml` pins the Sunrise build it is made for (`sunrise`, a game commit). Sunrise builds native mods from source on the player's computer, against the exact game build.

## Licence and credits

Trinkets is by Emi (Emily Rose Ploszaj) and C4 (https://github.com/emilyploszaj/trinkets), under the MIT licence; `LICENSE` holds the text and `NOTICE` says where the port comes from. No Java code is included.

Ported to Minecraft 26.3 by lavoide ([lavoide/horizon-ports](https://github.com/lavoide/horizon-ports)), and to Sunrise by arismoko.
