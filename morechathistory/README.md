# More Chat History for Sunrise

Keeps up to 16,384 chat messages instead of vanilla's 100. A port of [More Chat History](https://github.com/JackFredMods/MoreChatHistory) 2.0.0 by JackFred to Sunrise's typed mod API. Client side only, no mixins.

## What it does

- Raises the received-message history, the wrapped display-line history and the sent-message history (`ChatComponent.MAX_CHAT_HISTORY`) from 100 to 16,384, through `set_chat_history_limit` (sunrise-org/sunrise#5598).
- The cap is a setting: Options → Mods → More Chat History, from 100 to 16,384, default 16,384. A changed value applies on the next client tick.

## Differences from the original

- The cap is adjustable; the original's is fixed at 16,384.
- The saved command history (`command_history.txt`, 50 commands) keeps its own cap, as in the original.

## Building

`cargo sunrise-mod` builds it against the pinned SDK. Licence: CC0-1.0.
