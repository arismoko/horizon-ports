# Overflowing Bars

A Sunrise 26.3 client-only port of [Overflowing Bars](https://github.com/Fuzss/overflowing-bars) (26.3.0) by Fuzs (MPL-2.0).

## What it does

- **Stacked health**: hearts past ten stack into a second and further rows in a layer colour instead of climbing the screen, with absorption stacking the same way, the vanilla blink, regeneration wave and low-health shake kept, and poisoned and withered hearts, and the hardcore heart sprites in a hardcore world.
- **Stacked armor and toughness**: armor points past 20 stack in a layer colour, and armor toughness gets its own bar on the right of the hotbar (or the left, above armor) that stacks the same way.
- **Row count**: a small number beside each bar shows how many rows it holds, in a built-in pixel font or the game font, in any of the 16 dye colours, with or without the "x", for full rows only, or always.
- **Per-bar options**: for health, armor and toughness: **Allow Layers**, **Row Count**, **Colorize First Row**, **Inverse Coloring** and **Row Shift** (-5 to 5 rows); **Skip Empty Points** for armor and toughness; **Armor Toughness Bar**, **Toughness Bar On Left Side** and **Move Chat Above Armor**.
- **Settings**: everything is in **Options → Mods → Overflowing Bars**.

Client-side only: works in single player and on any multiplayer server.

## Differences from the original

- Frozen hearts (powder snow) are not drawn: the client cannot read the local player's frozen ticks, so only poisoned and withered hearts replace the normal ones (gap: sunrise-org/sunrise#5693).
- The blink after taking or regaining health is approximated from changes in health: the client cannot read the player's damage cooldown (gap: sunrise-org/sunrise#5694).
- The bars show the local player only, not the player you spectate (gap: sunrise-org/sunrise#5498).
- A mount's heart rows are not stacked or read: the right-hand toughness bar sits above vanilla's right-hand column, so it clears a mount's hearts, but the mount's own hearts are not drawn in layers (gap: sunrise-org/sunrise#5498).
- The toughness bar reads the height of vanilla's right-hand status column, but other layers (air bubbles, other mods' bars) do not move to make room for it and the bar does not read other mods' bar heights, because a layer cannot publish the height it takes (gap: sunrise-org/sunrise#5640).
- Settings are on Sunrise's flat **Options → Mods → Overflowing Bars** page instead of `overflowingbars-client.toml` and apply at once, including the three **Allow Layers** options that need a restart in the original (gap: [sunrise-org/sunrise#1217](https://github.com/sunrise-org/sunrise/issues/1217)).
- The icons are tinted vanilla armor icons and original layered hearts, not the original's textures (All Rights Reserved, not carried), so colours differ; the row count digits are drawn from rectangles in an original 3x5 font. The original ships no translations; the English text is written for this port.

A port of Overflowing Bars by Fuzs, under the MPL-2.0 licence.
