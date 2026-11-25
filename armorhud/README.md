# Armor HUD

A Sunrise 26.3 client-only port of [uku's Armor HUD](https://github.com/uku3lig/armor-hud) (0.14.0) by uku, based on BerdinskiyBear's Armor HUD (MIT).

## What it does

- **Armour widget**: draws the helmet, chestplate, leggings and boots as items with their durability bars in a vanilla-style slot strip, in the hotbar's own texture or rounded styles (**Hotbar**, **Rounded Corners**, **Rounded**, **None**).
- **Placement**: anchor to the **Hotbar**, **Top Center**, **Top**, **Vertical Center** or **Bottom**, on the **Left** or **Right**, with X and Y offsets, **Horizontal** or **Vertical** orientation and a **Reversed** order. Beside the hotbar it makes room for the offhand slot (**Always Ignore**, **Adhere**, **Always Leave Space**).
- **Show Widget**: **Always**, **If Any Present**, **Not Empty** or **Damaged Pieces**, with the vanilla empty-slot icons in empty slots.
- **Durability display**: a **Bar**, a **Numeric** value or a **Percentage** beside each piece, and optionally the offhand's and the held item's durability on the hotbar.
- **Low-durability warning**: a small warning icon, with adjustable jitter, at any piece at or under a durability value or percentage you choose, and a breaking sound (with its own subtitle) when a worn piece wears down to the warning.
- **Toggle key**: bind **Toggle Armor Hud** in **Options → Controls**; it shows a message when used.
- **Settings**: everything is in **Options → Mods → Armor HUD**.

Client-side only: works in single player and on any multiplayer server.

## Differences from the original

- The hotbar attack indicator is not read, so beside the hotbar the widget does not make room for it while it charges (gap: TBD-attack-indicator).
- Boss bars, effect icons and subtitles are not moved out of the widget's way: the original's **Push Bossbars**, **Push Effect Icons** and **Push Subtitles** options are left out, and vanilla keeps their placement. A HUD layer wrapper's pose does not reach the layer it wraps, and the game paints subtitles in a later pass (gap: TBD-layer-offset). NeoForge lets a `RenderGuiLayerEvent.Pre` listener translate the `GuiGraphics` pose for the layer.
- The widget shows the local player's armour only, not that of the player you spectate (gap: TBD-spectated-armor).
- Settings use Sunrise's flat **Options → Mods → Armor HUD** page rather than the original's tabbed Style, Position and Warning screen, are kept by Sunrise rather than in a JSON file, and offsets and thresholds have fixed ranges (gap: TBD-settings-sections).
- The original's compatibility hooks for other HUD mods (opacity, screen safe area) have no counterpart; the English translations are the only ones the original ships.

A port of uku's Armor HUD by uku (uku3lig), based on BerdinskiyBear's Armor HUD, under the MIT licence.
