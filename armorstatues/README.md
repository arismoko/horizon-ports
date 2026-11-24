# Armor Statues for Sunrise

Pose, style, position, align, and equip armor stands through an in-game six-tab menu.

A port of [Armor Statues](https://github.com/Fuzss/armorstatues) and [Statue Menus](https://github.com/Fuzss/statuemenus) by Fuzs to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), written in Rust on Sunrise's mod API. The 26.3 carry is by lavoide; this port moves it to Sunrise and implements the full six-tab statue screen.

## What it does

Sneak and use any armor stand with an empty hand to open its configuration screen (the armor stand item tooltip shows the bound keys). Players who cannot build or who are in spectator mode cannot open it, and sealed stands (`DisabledSlots` set on all slots) can only be opened in creative mode.

The screen opens to the last tab you used and switches tabs by clicking the left tab bar, scrolling the mouse wheel over the tabs, or pressing hotbar keys `1` through `6`:

- **Rotations**: Six 2D square sliders (X and Y, with Y inverted to match the stand's facing) and six vertical Z sliders for Head, Body, Right Arm, Left Arm, Right Leg, and Left Leg, plus a live stand preview. Hold Shift to lock the vertical axis or Alt to lock the horizontal axis; press Enter or Space on a focused slider to step with arrow keys. Includes clamp toggle (limited vs. unlimited `−180°..180°`), tips, randomize, reset, mirror (swap left/right and negate Y/Z), and pose copy/paste. Arm sliders activate once Show Arms is enabled.
- **Poses**: 29 pose presets (11 Minecraft presets and 18 Vanilla Tweaks presets) in a 2×2 paged grid with interactive stand previews and mouse-wheel or button pagination.
- **Style**: Custom name field (up to 50 characters, debounced 20 ticks, blank clears and toggles `CustomNameVisible`) and checkboxes for Show Arms, Small, Invisible, No Base Plate, Immovable (`NoGravity`), Invulnerable, and Sealed (creative only).
- **Position**: Logarithmic scale slider (`0.0625` to `16`, hold Alt to reveal the reset button), wrapped rotation slider (`−180°` to `180°`, snapping to 1/8 turns), Move By step selector (`1`, `4`, `8`, or `16` pixels), read-only X/Y/Z coordinate boxes rounded to 1/16 block with up/down nudge buttons, and a Focus toggle on every row that hides the rest of the menu while positioning.
- **Alignments**: Align Centered (`+0.5, 0, +0.5`), Align Cornered (`floor`), and four held-item surface alignments (Block on Surface, Floating Item on Surface, Flat Item on Surface, Tool on Surface) that set Invisible and NoGravity, pose the right arm, and translate the stand in local coordinates for normal or small stands.
- **Equipment**: Full 42-slot equipment screen (head, chest, legs, feet, mainhand, offhand, plus player inventory and hotbar) with live stand preview, vanilla empty-slot icons, disabled-slot overlays, Binding Curse protection in survival, relaxed shift-click routing, non-equippable item placement on the head slot, and automatic Show Arms activation when placing an item in either hand.

## Differences from the original

- **External edits while the screen is open**: The client reads an armor stand's initial pose and flags from the server snapshot sent when opening the menu, and tracks its own edits plus server-pushed Show Arms and equipment updates. Edits made to the same stand by another player or command while the screen is open appear when reopening, because Sunrise's client mod API does not expose synced armor stand pose rotations or flag fields.
- **Vanilla Tweaks credits link**: Clicking the `?` credits button on Poses and Alignments plays the click sound and shows the credits tooltip, but does not open a confirmation dialog to `https://vanillatweaks.net/`, because Sunrise's client mod API does not expose `ConfirmLinkScreen`.
- **Client-only mode on unmodded servers**: Requires the mod on both server and client. The original's client-only fallback that queues `/data merge` / `/attribute` commands or Vanilla Tweaks `/trigger` commands on unmodded servers is not included.
- **Vanilla sprites**: Original Statue Menus GUI textures are All Rights Reserved; the screen is drawn with vanilla GUI sprites and widgets.

## Building

With the Sunrise mod tool (`cargo sunrise-mod`, from the Sunrise SDK) and Rust:

```sh
cargo sunrise-mod check
cargo sunrise-mod build    # target/sunrise-mod/armorstatues.smod
```

`sunrise-mod.toml` pins the Sunrise build this version is made for. Needed on both the server and the client.

## Licence and credits

Armor Statues and Statue Menus are by Fuzs, under the Mozilla Public License 2.0; `LICENSE` holds its text. This port contains no code or assets from the originals and is distributed under the same licence. The icon is the port's own.
