# Mouse Wheelie

Scroll items between slots and inventories, move and drop with modifier clicks, sort with the middle button, and refill the hotbar. A port of [Mouse Wheelie](https://github.com/Siphalor/mouse-wheelie) 1.16.3 by Siphalor (Apache-2.0, see `LICENSE` and `NOTICE`) to Sunrise Dev 1 (`bdad86d2d398ff9293002d2dc08702822aea7265`).

Pure typed mod API, no `[[native.mixin]]` rows. Written without compiling: `cargo sunrise-mod check`, `cargo test` and clippy are still to run.

## Features

- Wheel over a slot moves items toward the other inventory (`screen_mouse_scrolled_pre`), one item, Shift the stack, Ctrl every stack of the kind, Space the matching kinds.
- Shift/Ctrl/Alt/Space clicks and drags (`screen_mouse_button_pressed_pre`, `screen_mouse_dragged_pre`, `screen_mouse_button_released_pre`).
- Middle-click sort by item, Shift quantity, Ctrl name.
- Hotbar refill from the main inventory when the selected stack empties (`client_tick_post`).
- Clicks are `MenuRef::click` batches (`Pickup`, `QuickMove`, `Throw`, `Swap`); slot layout comes from `MenuRef::slot_kinds`, `slot_boxes` and `slot`.

## Blocked or different

- **Tool picking on pick block**: no client hook for the pick-block key action (`player_pick_item` is server-side after the pick). Not ported.
- **Creative sort order**: the creative tab order is not readable; items sort by id.
- **Screen hooks have a read-only host**, so `MenuRef::click` (needs `&mut dyn ClientHost`) is called from `client_tick_post`; actions land one tick late.
- **Hovered slot**: only `screen_hovered_stack` exists, so the slot index is found from `slot_boxes` and the pointer position. It is unavailable while the recipe book covers the inventory.
- **Creative inventory screen**: `MenuRef::click` refuses it, so the tweaks do not run there.
- `screen_key_pressed_pre` is not used: no feature needs to intercept a key (Space is read with `is_key_down`).
- The brief's `screen_mouse_clicked_pre` is `screen_mouse_button_pressed_pre` at Dev 1.

## Layout

- `src/logic.rs`: slot scopes, modifier tables, reach, click plans and the sort planner, unit-tested.
- `src/lib.rs`: hooks, settings and the click queue.
