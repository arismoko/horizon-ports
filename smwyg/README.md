# Show Me What You Got for Sunrise

In an inventory, hold Left Ctrl and press the chat key over an item: the chat opens with the item's name in it. Send the line and everyone sees the name in its rarity's colour, with the item's tooltip on hover. An item tooltip in chat shows the item beside its name.

- The cursor must be empty, as in the original.
- Players without the mod see the shared item too; only the person sharing needs it on their client, and the server needs it to show the item.

A port of [Show Me What You Got](https://github.com/apace100/show-me-what-you-got) 1.2.0 by Apace to [Sunrise](https://playsunrise.net) (Minecraft Java 26.3), written in Rust on Sunrise's mod API, under the original's licence (MIT, in `LICENSE`; see `NOTICE`).

## Differences from the original

- The item's name in the chat box is ordinary text: it can be edited like any other text, and the chat history recalls the line as text, not as the item (sunrise-org/sunrise#5353).
