//! Show Me What You Got 1.1.1 (Apace, MIT), the server's half: a player's chat line that holds a
//! shared item becomes `<name> text [item] text` for everyone, the item's `[name]` hoverable (as
//! ShowMeWhatYouGot's ServerMessageDecoratorEvent decorates it).
//!
//! The line stays the player's own chat message: the window's half first sends the decorated body
//! on `smwyg:share`, then the line goes as ordinary signed chat through the game's whole chat path
//! (hidden chat, the chat filter and mutes, every mod's `server_chat` and its cancel, the spam
//! limit). This mod's `server_chat` swaps the decorated body in as the message's unsigned content
//! (`withUnsignedContent`; the signature still covers what was typed) only when the line from that
//! same player reaches it untouched and the body shows nothing the typed line didn't hold. A line
//! the game or a mod refuses never reaches it, and a share payload alone shows nothing.
//!
//! The channel is an optional serverbound play payload (`PayloadRegistrar::play_to_server`; a raw
//! channel can't be sent on from the client: `UnknownPayload`), declared in `register`; the
//! window's half (`crate::game::smwyg`, runtime.rs) sends on it with the game's
//! `send_payload_to_server`.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use mod_api::{
    Flow, Hooks, IPayloadContext, IPayloadHandler, PayloadBytes, PayloadHandleError, PayloadId,
    Registrar, RegistrationContext, ServerChatEvent, ServerHost, text::TextContent,
};

#[derive(Default)]
struct Mod;

/// The longest message accepted, in bytes (a chat line plus an item's components).
const MAX: usize = 32 * 1024;

/// Each player's announced share, by UUID: (the chat line it belongs to, the decorated body).
/// Taken by that player's next chat line, which uses it only if it is that line.
static PENDING: Mutex<Option<HashMap<u128, (String, mod_api::serde_json::Value)>>> = Mutex::new(None);

struct Share;

impl IPayloadHandler for Share {
    fn handle(
        &self,
        payload: PayloadBytes,
        context: &mut dyn IPayloadContext,
    ) -> Result<(), PayloadHandleError> {
        if payload.0.len() > MAX {
            return Ok(());
        }
        // Only a share from a player the server knows: its UUID keys the chat line it decorates.
        let Ok(player) = context.player() else {
            return Ok(());
        };
        let Ok(host) = context.server_host() else {
            return Ok(());
        };
        let uuid = player.uuid();
        let Ok(mod_api::serde_json::Value::Object(map)) =
            mod_api::serde_json::from_slice::<mod_api::serde_json::Value>(&payload.0)
        else {
            return Ok(());
        };
        let Some(text) = map.get("text").and_then(|t| t.as_str()) else {
            return Ok(());
        };
        let Some(body @ mod_api::serde_json::Value::Array(_)) = map.get("body") else {
            return Ok(());
        };
        // Only plain text and the item's hover go on: the sender is a client, and its body must
        // not resolve selectors, scores and NBT or carry click actions.
        let Some(body) = clean(body, 0) else {
            return Ok(());
        };
        if !shows_only(&body, text) {
            return Ok(());
        }
        PENDING
            .lock()
            .unwrap()
            .get_or_insert_with(HashMap::new)
            .insert(uuid, (text.to_string(), body));
        Ok(())
    }
}

/// Whether the body `["", before, item, after]` shows only what the typed `text` holds, so the chat
/// filter, which reads the typed line, has seen every character the decorated line shows: `before`
/// and `after` are the line's ends, and every literal string in the item's name (its hover aside)
/// is in the part between them. A translated name shows the reader's own language string; only
/// item and block keys, and the `[%s]` wrapper, pass. The item hover (its components) is the item's own, unchecked.
fn shows_only(body: &mod_api::serde_json::Value, text: &str) -> bool {
    use mod_api::serde_json::Value;
    let Some([Value::String(head), Value::String(before), item, Value::String(after)]) =
        body.as_array().map(Vec::as_slice)
    else {
        return false;
    };
    if !head.is_empty() || before.len() + after.len() > text.len() {
        return false;
    }
    let (Some(rest), true) = (text.strip_prefix(before.as_str()), text.ends_with(after.as_str())) else {
        return false;
    };
    let middle = &rest[..rest.len() - after.len()];
    fn literals_in(v: &Value, middle: &str) -> bool {
        match v {
            Value::String(s) => middle.contains(s.as_str()),
            Value::Array(items) => items.iter().all(|i| literals_in(i, middle)),
            Value::Object(map) => map.iter().all(|(k, val)| match k.as_str() {
                "text" | "fallback" => val.as_str().is_some_and(|s| middle.contains(s)),
                // An item or block name's key (a key with no translation shows as itself), or the
                // `[%s]` wrapper the game puts around a shared item's name (the brackets are in the
                // typed line too).
                "translate" => val.as_str().is_some_and(|k| {
                    (k.starts_with("item.") || k.starts_with("block.") || k == "chat.square_brackets")
                        && k.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
                }),
                "with" | "extra" => literals_in(val, middle),
                _ => true,
            }),
            _ => true,
        }
    }
    !middle.is_empty() && literals_in(item, middle)
}

/// A text component from the client, cut down to what a shared line holds: text, translation keys
/// with their arguments, children, style flags and colours, and a `show_item` hover. Selector,
/// score, NBT and keybind contents, click events and insertions are dropped. None when nothing is
/// left or the nesting is deeper than a chat line needs.
fn clean(v: &mod_api::serde_json::Value, depth: u32) -> Option<mod_api::serde_json::Value> {
    use mod_api::serde_json::Value;
    if depth > 8 {
        return None;
    }
    match v {
        Value::String(_) => Some(v.clone()),
        Value::Array(items) => Some(Value::Array(items.iter().filter_map(|i| clean(i, depth + 1)).collect())),
        Value::Object(map) => {
            let mut out = mod_api::serde_json::Map::new();
            for (k, val) in map {
                let kept = match k.as_str() {
                    "text" | "translate" | "fallback" | "color" | "font" => val.is_string().then(|| val.clone()),
                    "bold" | "italic" | "underlined" | "strikethrough" | "obfuscated" => {
                        val.is_boolean().then(|| val.clone())
                    }
                    "with" | "extra" => clean(val, depth + 1).filter(Value::is_array),
                    "hover_event" => val
                        .get("action")
                        .and_then(Value::as_str)
                        .filter(|a| *a == "show_item")
                        .map(|_| val.clone()),
                    _ => None,
                };
                if let Some(kept) = kept {
                    out.insert(k.clone(), kept);
                }
            }
            (out.contains_key("text") || out.contains_key("translate")).then_some(Value::Object(out))
        }
        _ => None,
    }
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    /// The player's chat line, through the game's chat checks and any earlier mod: if it is the
    /// line their share announced and still the plain typed text, show the decorated body.
    fn server_chat(&self, host: &mut dyn ServerHost, event: &mut ServerChatEvent<'_>) -> Flow {
        let pending = PENDING
            .lock()
            .unwrap()
            .as_mut()
            .and_then(|m| m.remove(&event.player));
        let Some((text, body)) = pending else {
            return Flow::Continue;
        };
        if event.cancelled || text != event.raw_text {
            return Flow::Continue;
        }
        // Another mod already changed the message: theirs stands.
        let untouched = event.message.children.is_empty()
            && matches!(&event.message.content, TextContent::Literal(s) if s == event.raw_text);
        if !untouched {
            return Flow::Continue;
        }
        if let Ok(Some(decorated)) = host.text_from_json(&body.to_string()) {
            *event.message = decorated;
        }
        Flow::Continue
    }

    fn register(&self, registrar: &mut dyn Registrar, _: &RegistrationContext) {
        // Optional: a server without this mod is still joined, and the window then sends the
        // plain chat line (the send answers UnsupportedPeer).
        let mut payloads = registrar.payload_handlers();
        payloads
            .registrar("1")
            .optional()
            .play_to_server(PayloadId("smwyg:share".into()), Arc::new(Share));
    }
}
