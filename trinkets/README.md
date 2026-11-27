# Trinkets

Extra equipment slots for trinkets, shown on your inventory screen and in a menu on G.

Extra equipment slots for the trinkets other mods add: face, hat, necklace, back, cape, charm, glove, two rings, belt, wearable lantern, key, quiver, shoes and a second charm.

What it does:

- On your inventory screen, hover an armour slot or the offhand slot: the trinket slots of that group (head, chest, legs, feet, offhand) pop out to the left of it, and fold away when the pointer leaves. Hand and charm slots have a small anchor left of the panel. An empty slot shows its slot-type picture. Click a worn item to take it off; click with an item on the cursor to wear it in that slot (the item is parked in a free inventory slot first, and one of it is worn).
- Press G (Open Trinkets in Controls) to open the Trinkets menu: a slot for each trinket place, each showing its picture while empty. Click an item in your inventory to wear it in the first empty slot that takes it (or swap with the first that does); click a worn item to take it off.
- Right-click a trinket in your hand to wear it in the first empty slot that takes it. Backpacks and wireless terminals keep their own use; an elytra still goes in the chest slot.
- An item's tooltip lists the trinket slots it fits, and the key that opens the menu.
- A Totem of Undying worn in a trinket slot saves you from a killing blow, and is used up.
- What you wear is saved with you, kept through death, and shown to your client. When you die without keepInventory your worn trinkets drop where you died (items with Curse of Vanishing are destroyed).
- Other mods read the worn stacks from the `trinkets:worn` player attachment: Sophisticated Backpacks' back slot, Elytra Slot, Things, Radiant Gear and others.

Differences from the original:

- The inventory slots are drawn and clicked by the mod, they are not real slots of the inventory menu: the pinned API (ba1f24e) has no way to add slots to the player inventory menu (`set_screen_slot` only moves slots of an open mod menu; `MenuPart::menu_slot_rules` only reaches menus with a registered type, and the player's has none). So shift-click, drag, number-key swaps and recipe viewers do not see them. Wearing from the cursor is done by parking the cursor stack in a free inventory slot, so it needs one.
- The groups have no slot type with several copies, so the original's vertical expansion of a column is not there.
- The G menu is a two-row chest menu with the slot pictures as placeholder items.
- Right-click wearing is done by the server only; the client does not predict it.
- Dropped trinkets are not put in a grave by a grave mod: no grave mod is ported.


## Package API for other mods

- The worn stacks are the player attachment `trinkets:worn` (a compound of slot id to item NBT, for example `chest/back` or `chest/cape`; synced to the client and kept through death). A mod reads it with `registrar.attachment_key::<Compound>("trinkets:worn")` in `Hooks::register`, after declaring `trinkets` as a dependency (`AttachmentError::Undeclared` otherwise). Sophisticated Backpacks' back slot (`sophisticatedcore`) does this.
- The slot ids are `head/face`, `head/hat`, `chest/necklace`, `chest/back`, `chest/cape`, `chest/charm`, `hand/glove`, `hand/ring`, `offhand/ring`, `legs/belt`, `legs/lantern`, `legs/key`, `legs/quiver`, `feet/shoes` and `charm/charm`.

## Remaining API differences

- The slot menu is a two-row chest menu with placeholder items; the original draws its slots in a screen beside the inventory. Clicking is by `menu_clicked`.
- Right-click wearing is server-only (the client does not predict it).
- Death drop uses `living_death` and the `minecraft:keep_inventory` game rule; a grave mod taking the items first (universalgraves in the earlier pack) is not ported.
- The Open Trinkets key listens to keyboard bindings only (a mouse button binding does not open the menu).
- The vanishing-curse check reads `minecraft:enchantments` in the stack's components.

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
