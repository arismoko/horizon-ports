//! Show Me What You Got 1.1.1 (Apace, MIT), the server's half: a `smwyg:share` message from a
//! player becomes the chat line `<name> text [item] text` for everyone (`chat.type.text`, as
//! ShowMeWhatYouGot's ServerMessageDecoratorEvent decorates it), written with `tellraw @a`.
//!
//! The channel is an optional serverbound play payload (`PayloadRegistrar::play_to_server`; a raw
//! channel can't be sent on from the client: `UnknownPayload`), declared in `register`; the
//! window's half (`crate::game::smwyg`, runtime.rs) sends on it with the game's
//! `send_payload_to_server`.
use std::sync::Arc;

use mod_api::{
    Hooks, IPayloadContext, IPayloadHandler, PayloadBytes, PayloadHandleError, PayloadId,
    Registrar, RegistrationContext,
};

#[derive(Default)]
struct Mod;

/// The longest message accepted, in bytes (a chat line plus an item's components).
const MAX: usize = 32 * 1024;

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
        let Ok(mod_api::serde_json::Value::Object(map)) =
            mod_api::serde_json::from_slice::<mod_api::serde_json::Value>(&payload.0)
        else {
            return Ok(());
        };
        let Some(body @ mod_api::serde_json::Value::Array(_)) = map.get("body") else {
            return Ok(());
        };
        // Only plain text and the item's hover go on: the sender is a client, and `tellraw` would
        // resolve selectors, scores and NBT for it and pass on click actions.
        let Some(body) = clean(body, 0) else {
            return Ok(());
        };
        let player = context.player().ok();
        let Ok(host) = context.server_host() else {
            return Ok(());
        };
        // The server's own name for the player, else the one the window sent.
        let name = player
            .and_then(|p| p.name(&*host))
            .or_else(|| map.get("name").and_then(|n| n.as_str()).map(str::to_string))
            .unwrap_or_default();
        let line = mod_api::serde_json::json!({
            "translate": "chat.type.text",
            "with": [{ "text": name }, body],
        });
        if !host.commands().run_command(&format!("tellraw @a {line}")) {
            eprintln!("[smwyg] tellraw failed");
        }
        Ok(())
    }
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
