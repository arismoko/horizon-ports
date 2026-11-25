# Convenient Name Tags

Named mobs drop their name tag on death, shears take a name off as a tag, and a name tag isn't wasted on a mob that already has its name.

A port of [Convenient Name Tags](https://www.curseforge.com/projects/662763) 1.1.0 by Mim1q, under the original's licence (Apache-2.0, in `LICENSE`; see `NOTICE`). Written for Sunrise's typed mod API (no `[[native.mixin]]` rows), pinned to Sunrise `bdad86d2d398ff9293002d2dc08702822aea7265`.

## What it does

- A named mob that dies drops a name tag carrying its whole name (colour and style included) beside its other drops. Players are left out.
- Sneak and use shears on a named mob: the name comes off as a name tag, the shears take 1 damage and the mob's own interaction (shearing a sheep) does not run.
- Use a name tag on a mob that already has that exact name: the tag is not used up.
- Use a name tag with a different name on a named mob: the old name drops as a tag, then the mob is renamed as in vanilla.

Install it on the server; clients need nothing (`side = "server"`).

## Differences from the original

- **No rename screen.** The original opens a text screen when you sneak and use a name tag, to rename the tag in your hand. This port does not have it. Dev 1 gives a mod a client text field only by building a whole mod screen (a `ScreenWidget` set, a registered mod screen and a client-to-server payload) and that was not written; nothing in Dev 1 is missing for it (a text-input screen is possible, as `armorstatues` does), so this is unfinished work, not an API gap. Rename a tag in an anvil.

## Tests

`src/rules.rs` unit-tests the click decisions.
