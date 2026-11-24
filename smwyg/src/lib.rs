//! Show Me What You Got by Apace for Sunrise: in an inventory, Left Ctrl and the chat key over an
//! item open the chat with the item's name in it. Sending the line shows everyone the item's
//! hoverable name in its place, as vanilla's `ItemStack.getDisplayName` shows it, and an item
//! hover in chat draws the item beside its name.
//!
//! The client remembers the shared stack and, on send, tells the server where its name sits in
//! the line (`smwyg:share`). The server keeps that for the player's next chat message and swaps
//! the hoverable name in as the message's content.
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, MutexGuard, OnceLock, PoisonError};

use mod_api::text::{TextHover, TextStyle};
use mod_api::{
    ClientChatEvent, ClientHost, ClientTickPreEvent, ClientTooltipComponent, ComponentHoverEvent,
    Compound, DrawContext, Hooks, Host, IPayloadContext, ItemRarity, ItemResource, ItemStack,
    JavaString, PayloadId, PlayerLoggedOutEvent, RegisterClientTooltipComponentFactoriesEvent,
    Registrar, RegistrationContext, RenderTooltipGatherComponentsEvent, ScreenClosingEvent,
    ServerChatEvent, ServerHost, ServerStartedEvent, Tag, Text, ToServer, TooltipComponent,
    TooltipComponentData, TooltipElement, TooltipFont,
};

const CHANNEL: &str = "smwyg:share";
/// A stack whose custom data sets this is never drawn beside its hover name.
const HIDE_STACK: &str = "smwyg:hide_stack";
/// The original's gap between the icon and the name.
const GAP: i32 = 3;
/// `ItemStackTooltipComponent`: a slot's size.
const ICON: i32 = 18;
/// `ClientTextTooltip.getHeight`.
const LINE: i32 = 10;

/// Where the shared stack's name sits in the sent line (byte offsets), and the stack as vanilla's
/// item NBT escape carries it.
#[derive(Clone, Debug, mod_api::Wire)]
struct Share {
    start: u32,
    end: u32,
    id: String,
    count: u32,
    components: Option<Tag>,
}

/// The stack the share key took, and the name it put in the chat box.
struct Pending {
    name: String,
    stack: ItemStack,
}

static TOKEN: OnceLock<ToServer<Share>> = OnceLock::new();
/// Left Ctrl and the chat key were down at the last tick (the original's `sharedStack`).
static HELD: AtomicBool = AtomicBool::new(false);
static SHARING: Mutex<Option<Pending>> = Mutex::new(None);
/// The item hover the chat drew this frame, for its tooltip to find.
static HOVERED: Mutex<Option<(String, u32, Option<Tag>)>> = Mutex::new(None);
/// Each player's share, kept for their next chat message.
static SHARES: LazyLock<Mutex<HashMap<u128, Share>>> = LazyLock::new(Default::default);

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// An empty component patch carries nothing: compare it as none.
fn patch(components: Option<Tag>) -> Option<Tag> {
    components.filter(|tag| !matches!(tag, Tag::Compound(c) if c.is_empty()))
}

/// The stack's id, count and component patch, as a `show_item` hover carries them.
fn hover_parts(host: &dyn Host, stack: &ItemStack) -> Option<(String, u32, Option<Tag>)> {
    let nbt = stack.to_nbt(host)?;
    let Some(Tag::String(id)) = nbt.get(&JavaString::from("id")) else {
        return None;
    };
    let components = nbt.get(&JavaString::from("components")).cloned();
    Some((id.to_string_lossy(), stack.count(), patch(components)))
}

/// `ItemStack.getDisplayName`: the hover name in square brackets, italic when renamed, in the
/// rarity's colour, showing the stack on hover.
fn display_name(host: &dyn Host, stack: &ItemStack) -> Option<Text> {
    let mut name = Text::literal("").append(host.hover_name(stack).ok()?);
    let renamed = stack.components().is_some_and(|c| {
        c.as_nbt()
            .contains_key(&JavaString::from("minecraft:custom_name"))
    });
    if renamed {
        let mut style = TextStyle::default();
        style.italic = Some(true);
        name = name.with_style(style);
    }
    let color = match ItemResource::of(stack).rarity(host) {
        ItemRarity::Uncommon => "yellow",
        ItemRarity::Rare => "aqua",
        ItemRarity::Epic => "light_purple",
        _ => "white",
    };
    let (id, count, components) = hover_parts(host, stack)?;
    let mut style = TextStyle::default();
    style.color = Some(color.to_owned());
    style.hover = Some(TextHover::Item {
        id,
        count,
        components,
    });
    Some(Text::translate("chat.square_brackets", vec![name]).with_style(style))
}

fn hidden(components: Option<&Tag>) -> bool {
    let Some(Tag::Compound(patch)) = components else {
        return false;
    };
    let Some(Tag::Compound(data)) = patch.get(&JavaString::from("minecraft:custom_data")) else {
        return false;
    };
    matches!(data.get(&JavaString::from(HIDE_STACK)), Some(Tag::Byte(b)) if *b != 0)
}

/// The first tooltip line with the item drawn before it.
struct Row {
    stack: ItemStack,
    text: Text,
    argb: u32,
}

impl TooltipComponentData for Row {}

impl ClientTooltipComponent for Row {
    fn height(&self, _: &dyn TooltipFont) -> i32 {
        ICON.max(LINE)
    }

    fn width(&self, font: &dyn TooltipFont) -> i32 {
        ICON + GAP + font.width(&self.text).unwrap_or(0)
    }

    fn draw_text(&self, font: &dyn TooltipFont, x: i32, y: i32, draw: &mut dyn DrawContext) {
        let at = [
            (x + ICON + GAP) as f32,
            (y + (self.height(font) - LINE) / 2) as f32,
        ];
        let _ = draw.text_with_font(font.id(), &self.text, at, self.argb, true);
    }

    fn draw_image(
        &self,
        font: &dyn TooltipFont,
        x: i32,
        y: i32,
        _: i32,
        _: i32,
        draw: &mut dyn DrawContext,
    ) {
        let count = self.stack.count();
        let label = (count > 1).then(|| count.to_string());
        let at = [x as f32, (y + (self.height(font) - ICON) / 2) as f32];
        draw.gui_draw_item_with(&self.stack, at, Some(label.as_deref().unwrap_or("")));
    }
}

fn on_share(share: Share, cx: &mut dyn IPayloadContext) -> Result<(), String> {
    let player = cx.player().map_err(|e| format!("{e:?}"))?;
    lock(&SHARES).insert(player.uuid(), share);
    Ok(())
}

#[derive(Default)]
struct Mod;

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        let token = r
            .payload_handlers()
            .registrar("1")
            .optional()
            .play_to_server_typed::<Share>(PayloadId(CHANNEL.into()), on_share);
        let _ = TOKEN.set(token);
    }

    fn register_client_tooltip_component_factories(
        &self,
        _: &RegistrationContext,
        event: &mut RegisterClientTooltipComponentFactoriesEvent<'_>,
    ) {
        event.factories.register::<Row, Row>(|row| Row {
            stack: row.stack.clone(),
            text: row.text.clone(),
            argb: row.argb,
        });
    }

    /// The original's tick: on the tick Left Ctrl and the chat key come down together in a
    /// container screen, with nothing on the cursor, the hovered stack is shared.
    fn client_tick_pre(&self, host: &mut dyn ClientHost, _: &mut ClientTickPreEvent) {
        let view: &dyn ClientHost = &*host;
        let in_container = view
            .observed_screen_id()
            .is_some_and(|screen| matches!(screen.id.as_str(), "inventory" | "creative"));
        if !in_container {
            return;
        }
        let chat = view
            .vanilla_key_mapping("key.chat")
            .ok()
            .and_then(|mapping| mapping.binding(view).ok().map(|key| view.is_key_down(key)));
        let down = view.is_key_down("key.keyboard.left.control") && chat == Some(true);
        if !down || HELD.swap(true, Ordering::Relaxed) {
            HELD.store(down, Ordering::Relaxed);
            return;
        }
        let cursor_empty = view
            .local_player()
            .and_then(|player| player.container_menu(view))
            .and_then(|menu| menu.carried(view))
            .is_some_and(|carried| carried.is_none());
        let Some(stack) = view.screen_hovered_stack().filter(|_| cursor_empty) else {
            return;
        };
        let Some(name) = display_name(view, &stack).map(|text| view.text_string(&text)) else {
            return;
        };
        host.close_screen();
        if host.open_chat(&name).is_ok() {
            *lock(&SHARING) = Some(Pending { name, stack });
        }
    }

    /// Sending the line tells the server where the shared name sits; a command shares nothing.
    fn client_chat(
        &self,
        host: &mut dyn ClientHost,
        event: &mut ClientChatEvent<'_>,
    ) -> mod_api::Flow {
        let Some(pending) = lock(&SHARING).take() else {
            return mod_api::Flow::Continue;
        };
        if event.message.starts_with('/') {
            return mod_api::Flow::Continue;
        }
        let Some(start) = event.message.find(&pending.name) else {
            return mod_api::Flow::Continue;
        };
        let (Some((id, count, components)), Some(token)) =
            (hover_parts(&*host, &pending.stack), TOKEN.get())
        else {
            return mod_api::Flow::Continue;
        };
        let share = Share {
            start: start as u32,
            end: (start + pending.name.len()) as u32,
            id,
            count,
            components,
        };
        let _ = token.send(host, &share);
        mod_api::Flow::Continue
    }

    fn screen_closing(&self, _: &dyn ClientHost, event: &mut ScreenClosingEvent<'_>) {
        if event.screen.id == "chat" {
            lock(&SHARING).take();
        }
    }

    /// Remember the item a chat hover is about to draw, for its tooltip.
    fn component_hover(
        &self,
        _: &dyn ClientHost,
        event: &mut ComponentHoverEvent<'_>,
    ) -> mod_api::Flow {
        *lock(&HOVERED) = match event.hover {
            TextHover::Item {
                id,
                count,
                components,
            } if !hidden(components.as_ref()) => {
                Some((id.clone(), *count, patch(components.clone())))
            }
            _ => None,
        };
        mod_api::Flow::Continue
    }

    /// The hovered item's tooltip gets the item drawn before its first line.
    fn render_tooltip_gather_components(
        &self,
        host: &dyn ClientHost,
        event: &mut RenderTooltipGatherComponentsEvent<'_>,
    ) -> mod_api::Flow {
        let mut hovered = lock(&HOVERED);
        if hovered.is_none() || hover_parts(host, event.stack) != *hovered {
            return mod_api::Flow::Continue;
        }
        hovered.take();
        if let Some(TooltipElement::Text(text, color)) = event.elements.first() {
            let argb = if *color == u32::MAX {
                0xFFFF_FFFF
            } else {
                *color
            };
            let row = Row {
                stack: event.stack.clone(),
                text: text.clone(),
                argb,
            };
            event.elements[0] = TooltipElement::Component(TooltipComponent::custom(row));
        }
        mod_api::Flow::Continue
    }

    fn server_started(&self, _: &mut dyn ServerHost, _: &mut ServerStartedEvent) {
        lock(&SHARES).clear();
    }

    fn player_logged_out(&self, _: &mut dyn ServerHost, event: &mut PlayerLoggedOutEvent<'_>) {
        lock(&SHARES).remove(&event.player);
    }

    /// The player's share replaces the name in their message with the hoverable stack.
    fn server_chat(
        &self,
        host: &mut dyn ServerHost,
        event: &mut ServerChatEvent<'_>,
    ) -> mod_api::Flow {
        let Some(share) = lock(&SHARES).remove(&event.player) else {
            return mod_api::Flow::Continue;
        };
        let raw = event.raw_text;
        let (start, end) = (share.start as usize, share.end as usize);
        let (Some(before), Some(after)) = (raw.get(..start), raw.get(end..)) else {
            return mod_api::Flow::Continue;
        };
        if start > end {
            return mod_api::Flow::Continue;
        }
        let mut nbt = Compound::new();
        nbt.insert("id".into(), Tag::String(share.id.as_str().into()));
        nbt.insert("count".into(), Tag::Int(share.count as i32));
        if let Some(components) = share.components {
            nbt.insert("components".into(), components);
        }
        let host: &dyn Host = &*host;
        let Some(name) = ItemStack::from_nbt(host, &nbt).and_then(|s| display_name(host, &s))
        else {
            return mod_api::Flow::Continue;
        };
        *event.message = Text::literal(before)
            .append(name)
            .append(Text::literal(after));
        mod_api::Flow::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hide_stack_reads_custom_data() {
        let mut data = Compound::new();
        data.insert(HIDE_STACK.into(), Tag::Byte(1));
        let mut components = Compound::new();
        components.insert("minecraft:custom_data".into(), Tag::Compound(data));
        assert!(hidden(Some(&Tag::Compound(components))));
        assert!(!hidden(Some(&Tag::Compound(Compound::new()))));
        assert!(!hidden(None));
    }
}
