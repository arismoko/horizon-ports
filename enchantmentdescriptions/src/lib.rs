#![deny(unsafe_code)]

//! Enchantment Descriptions for Sunrise: a translated description line under each enchantment
//! in an item's tooltip, for enchanted books and (optionally) enchanted gear.
//!
//! Like the original, a description is the language's text for
//! `enchantment.<namespace>.<name>.desc` (or `.description`, or `.info`, each also with the
//! level appended), so any mod or resource pack can describe its own enchantments. The line goes
//! right under the enchantment's own line, which the tooltip hook finds by its text.
//! Client only.

mod logic;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use logic::{
    DEFAULT_TOML, FileConfig, description_key, full_name, is_enchanting_menu, looks_like_json,
    match_lines, name_key, parse_config,
};
use mod_api::text::{TextContent, TextStyle};
use mod_api::{
    ClientHost, ComponentId, ConfigEvent, ConfigKind, ConfigSnapshot, ConfigSpec, Hooks, Host,
    ItemResource, ItemTooltipEvent, Registrar, RegistrationContext, ScreenClosingEvent,
    ScreenRenderPostEvent, SettingKey, SettingSpec, Tag, Text,
};

const CONFIG_FILE: &str = "enchantmentdescriptions.toml";
const ACTIVATE_KEY: &str = "enchdesc.activate.message";
const ENCHANTED_BOOK: &str = "minecraft:enchanted_book";
const COMPONENTS: [&str; 2] = ["minecraft:enchantments", "minecraft:stored_enchantments"];
/// `ChatFormatting.DARK_GRAY`, the original's default colour for descriptions and the hint.
const DARK_GRAY: u32 = 0x55_55_55;

const ENABLED: &str = "enabled";
const ONLY_ON_BOOKS: &str = "only_on_books";
const ONLY_IN_TABLE: &str = "only_in_enchanting_table";
const REQUIRE_KEYBIND: &str = "require_keybind";
const DISPLAY_ACTIVATE_TEXT: &str = "display_activate_text";
const COLOR: &str = "color";

/// The toggles' keys and their defaults (the original's `Config` defaults).
const TOGGLES: [(&str, bool); 5] = [
    (ENABLED, true),
    (ONLY_ON_BOOKS, false),
    (ONLY_IN_TABLE, false),
    (REQUIRE_KEYBIND, false),
    (DISPLAY_ACTIVATE_TEXT, true),
];

static FILE: Mutex<Option<FileConfig>> = Mutex::new(None);
/// Whether the open screen is an enchanting table's, kept from the screen hooks for the
/// tooltip hook.
static ENCHANTING_SCREEN: AtomicBool = AtomicBool::new(false);

fn file_config() -> FileConfig {
    let guard: MutexGuard<'_, Option<FileConfig>> =
        FILE.lock().unwrap_or_else(PoisonError::into_inner);
    guard.clone().unwrap_or_default()
}

/// The owner-bound keys `register_setting` returned, by the setting's name.
static KEYS: Mutex<Vec<(&'static str, SettingKey)>> = Mutex::new(Vec::new());

fn setting_key(name: &str) -> Option<SettingKey> {
    let keys = KEYS.lock().unwrap_or_else(PoisonError::into_inner);
    keys.iter().find(|(n, _)| *n == name).map(|(_, key)| *key)
}

fn toggle(host: &dyn Host, key: &str, default: bool) -> bool {
    setting_key(key)
        .and_then(|key| host.setting_bool(key).ok())
        .unwrap_or(default)
}

/// Every enchantment on the stack, applied and stored, with its level
/// (`DataComponents.ENCHANTMENTS` and `STORED_ENCHANTMENTS`).
fn enchantments(host: &dyn Host, stack: &ItemResource) -> Vec<(String, u32)> {
    let mut found = Vec::new();
    for component in COMPONENTS {
        let Some(id) = ComponentId::new(component) else {
            continue;
        };
        let Some(Tag::Compound(levels)) = stack.value(host, &id) else {
            continue;
        };
        for (enchantment, level) in levels.iter() {
            if let Tag::Int(level) = level
                && let Ok(level) = u32::try_from(*level)
                && level > 0
            {
                found.push((enchantment.to_string_lossy(), level));
            }
        }
    }
    found
}

/// A line's text with its children's, as the player reads it.
fn plain(text: &Text) -> String {
    let mut out = String::new();
    if let TextContent::Literal(value) = &text.content {
        out.push_str(value);
    }
    for child in &text.children {
        out.push_str(&plain(child));
    }
    out
}

/// A config text: a JSON component when it looks like one, otherwise plain text.
fn component(host: &dyn ClientHost, text: &str) -> Option<Text> {
    if text.is_empty() {
        return None;
    }
    if looks_like_json(text) {
        return host.text_from_json(text).ok().flatten();
    }
    Some(Text::literal(text))
}

fn style(host: &dyn Host, file: &FileConfig) -> TextStyle {
    let mut style = TextStyle::default();
    let color = setting_key(COLOR)
        .and_then(|key| host.setting_color(key).ok())
        .unwrap_or(DARK_GRAY)
        & 0xFF_FF_FF;
    style.color = Some(format!("#{color:06x}"));
    style.bold = file.bold;
    style.italic = file.italic;
    style.underlined = file.underlined;
    style.strikethrough = file.strikethrough;
    style.obfuscated = file.obfuscated;
    if !file.font.is_empty() {
        style.font = Some(file.font.clone());
    }
    style
}

/// The original's `canDisplayDescription`, for a stack that has enchantments.
fn can_display(host: &dyn Host, item_id: Option<&str>) -> bool {
    if !toggle(host, ENABLED, true) {
        return false;
    }
    if toggle(host, ONLY_ON_BOOKS, false) && item_id != Some(ENCHANTED_BOOK) {
        return false;
    }
    if toggle(host, ONLY_IN_TABLE, false) && !ENCHANTING_SCREEN.load(Ordering::Relaxed) {
        return false;
    }
    true
}

/// `Minecraft.hasShiftDown`.
fn shift_down(host: &dyn ClientHost) -> bool {
    host.is_key_down("key.keyboard.left.shift") || host.is_key_down("key.keyboard.right.shift")
}

#[derive(Default)]
struct EnchantmentDescriptions;

#[mod_api::sunrise_mod]
impl Hooks for EnchantmentDescriptions {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        for (key, default) in TOGGLES {
            let label = format!("enchdesc.config.{key}");
            let spec =
                SettingSpec::toggle(key, &label, default).tooltip(format!("{label}.tooltip"));
            if let Ok(setting) = r.register_setting(spec) {
                KEYS.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push((key, setting));
            }
        }
        if let Ok(setting) = r.register_setting(
            SettingSpec::color(COLOR, "enchdesc.config.color", DARK_GRAY)
                .tooltip("enchdesc.config.color.tooltip"),
        ) {
            KEYS.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((COLOR, setting));
        }
        let listener = |event: ConfigEvent<'_>, snapshot: &ConfigSnapshot| -> Result<(), String> {
            if let ConfigEvent::Loading(_) | ConfigEvent::Reloading(_) = event {
                *FILE.lock().unwrap_or_else(PoisonError::into_inner) =
                    Some(parse_config(&snapshot.text));
            }
            Ok(())
        };
        let spec = ConfigSpec::new(ConfigKind::Client, DEFAULT_TOML).file(CONFIG_FILE);
        // A refused (duplicate) declaration leaves the defaults in use.
        let _ = r.register_config(spec, Some(Arc::new(listener)));
    }

    fn screen_render_post(&self, _host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
        ENCHANTING_SCREEN.store(is_enchanting_menu(event.screen.menu), Ordering::Relaxed);
    }

    fn screen_closing(&self, _host: &dyn ClientHost, _event: &mut ScreenClosingEvent<'_>) {
        ENCHANTING_SCREEN.store(false, Ordering::Relaxed);
    }

    fn item_tooltip(&self, host: &dyn ClientHost, event: &mut ItemTooltipEvent<'_>) {
        if !toggle(host, ENABLED, true) {
            return;
        }
        let Some(stack) = event.stack.project() else {
            return;
        };
        let resource = ItemResource::of(&stack);
        let found = enchantments(host, &resource);
        if found.is_empty() || !can_display(host, event.stack.item().id(host)) {
            return;
        }
        let file = file_config();
        if toggle(host, REQUIRE_KEYBIND, false) && !shift_down(host) {
            if toggle(host, DISPLAY_ACTIVATE_TEXT, true) {
                let hint = component(host, &file.activate_text).unwrap_or_else(|| {
                    let mut gray = TextStyle::default();
                    gray.color = Some(format!("#{DARK_GRAY:06x}"));
                    Text::translate(ACTIVATE_KEY, Vec::new()).with_style(gray)
                });
                let _ = event.lines.push(hint, u32::MAX);
            }
            return;
        }

        // The game builds an enchantment's tooltip line as plain text (`Enchantment.getFullname`:
        // the translated name, then the level's text unless both are one), so lines are matched
        // by that text, not by a translation key.
        let line_texts: Vec<Option<String>> = event
            .lines
            .iter()
            .map(|line| line.ok().map(|(text, _)| plain(&text)))
            .collect();
        let infos = host.enchantments();
        let wanted_keys: Vec<String> = found
            .iter()
            .map(|(id, _)| name_key(id).unwrap_or_default())
            .collect();
        let wanted: Vec<String> = found
            .iter()
            .zip(&wanted_keys)
            .map(|((id, level), key)| {
                let max_level = infos
                    .iter()
                    .find(|info| info.id == *id)
                    .map_or(0, |info| info.max_level);
                let name = host.translate(key).unwrap_or_else(|| key.clone());
                full_name(name, *level, max_level, |level| {
                    let key = format!("enchantment.level.{level}");
                    host.translate(&key).unwrap_or(key)
                })
            })
            .collect();
        let mut insertions: Vec<(usize, Text)> = Vec::new();
        for (w, line) in match_lines(&line_texts, &wanted) {
            let (_, level) = &found[w];
            let Some(key) =
                description_key(&wanted_keys[w], *level, |key| host.translate(key).is_some())
            else {
                continue;
            };
            let mut text = Text::literal("");
            if let Some(prefix) = component(host, &file.prefix) {
                text = text.append(prefix);
            }
            text = text.append(Text::translate(key, Vec::new()).with_style(style(host, &file)));
            if let Some(suffix) = component(host, &file.suffix) {
                text = text.append(suffix);
            }
            insertions.push((line + 1, text));
        }
        // From the bottom up, so each insertion leaves the lines above it where they were.
        insertions.sort_by_key(|(index, _)| std::cmp::Reverse(*index));
        for (index, text) in insertions {
            let _ = event.lines.insert(index, text, u32::MAX);
        }
    }
}
