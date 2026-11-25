//! Formatting code (`§`) utilities matching Easy Anvils' `FormattedStringUtil` and
//! `FormattedStringDecomposer`.

use mod_api::text::{TextContent, TextStyle};
use mod_api::{Host, Text};

/// All 22 Minecraft legacy chat formatting codes in `ChatFormatting.values()` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormattingCode {
    pub code: char,
    pub name: &'static str,
    pub color: Option<&'static str>,
}

pub const ALL_FORMATTING_CODES: [FormattingCode; 22] = [
    FormattingCode {
        code: '0',
        name: "black",
        color: Some("black"),
    },
    FormattingCode {
        code: '1',
        name: "dark_blue",
        color: Some("dark_blue"),
    },
    FormattingCode {
        code: '2',
        name: "dark_green",
        color: Some("dark_green"),
    },
    FormattingCode {
        code: '3',
        name: "dark_aqua",
        color: Some("dark_aqua"),
    },
    FormattingCode {
        code: '4',
        name: "dark_red",
        color: Some("dark_red"),
    },
    FormattingCode {
        code: '5',
        name: "dark_purple",
        color: Some("dark_purple"),
    },
    FormattingCode {
        code: '6',
        name: "gold",
        color: Some("gold"),
    },
    FormattingCode {
        code: '7',
        name: "gray",
        color: Some("gray"),
    },
    FormattingCode {
        code: '8',
        name: "dark_gray",
        color: Some("dark_gray"),
    },
    FormattingCode {
        code: '9',
        name: "blue",
        color: Some("blue"),
    },
    FormattingCode {
        code: 'a',
        name: "green",
        color: Some("green"),
    },
    FormattingCode {
        code: 'b',
        name: "aqua",
        color: Some("aqua"),
    },
    FormattingCode {
        code: 'c',
        name: "red",
        color: Some("red"),
    },
    FormattingCode {
        code: 'd',
        name: "light_purple",
        color: Some("light_purple"),
    },
    FormattingCode {
        code: 'e',
        name: "yellow",
        color: Some("yellow"),
    },
    FormattingCode {
        code: 'f',
        name: "white",
        color: Some("white"),
    },
    FormattingCode {
        code: 'k',
        name: "obfuscated",
        color: None,
    },
    FormattingCode {
        code: 'l',
        name: "bold",
        color: None,
    },
    FormattingCode {
        code: 'm',
        name: "strikethrough",
        color: None,
    },
    FormattingCode {
        code: 'n',
        name: "underline",
        color: None,
    },
    FormattingCode {
        code: 'o',
        name: "italic",
        color: None,
    },
    FormattingCode {
        code: 'r',
        name: "reset",
        color: None,
    },
];

pub fn lookup_code(c: char) -> Option<FormattingCode> {
    let lower = c.to_ascii_lowercase();
    ALL_FORMATTING_CODES
        .iter()
        .copied()
        .find(|f| f.code == lower)
}

pub fn color_name_to_code(color: &str) -> Option<char> {
    let lower = color.to_ascii_lowercase();
    ALL_FORMATTING_CODES
        .iter()
        .find(|f| f.color == Some(lower.as_str()))
        .map(|f| f.code)
}

/// Vanilla `StringUtil.isAllowedChatCharacter`, optionally allowing `§` when formatting is enabled.
pub fn is_allowed_chat_char(c: char, allow_formatting: bool) -> bool {
    let code = c as u32;
    if allow_formatting && c == '§' {
        return true;
    }
    c != '§' && code >= 32 && code != 127
}

/// Filters text to allowed chat characters (`FormattedStringUtil.filterText`).
pub fn filter_text(text: &str, allow_formatting: bool) -> String {
    text.chars()
        .filter(|&c| is_allowed_chat_char(c, allow_formatting))
        .collect()
}

/// Visible character length excluding valid `§x` formatting pairs (`FormattedStringUtil.stringLength`).
pub fn visible_length(text: &str, allow_formatting: bool) -> usize {
    if !allow_formatting {
        return text.chars().count();
    }
    let mut count = 0usize;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '§'
            && let Some(&next) = chars.peek()
        {
            let _ = chars.next();
            let _ = next;
            continue;
        }
        count += 1;
    }
    count
}

/// Returns the plain visible string with all `§x` pairs stripped.
#[cfg(test)]
fn strip_formatting(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '§' && chars.peek().is_some() {
            let _ = chars.next();
            continue;
        }
        out.push(c);
    }
    out
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct FlatStyle {
    color: Option<&'static str>,
    bold: bool,
    italic: bool,
    underlined: bool,
    strikethrough: bool,
    obfuscated: bool,
}

impl FlatStyle {
    fn apply_code(&mut self, c: char) {
        match c.to_ascii_lowercase() {
            'r' => *self = Self::default(),
            'k' => self.obfuscated = true,
            'l' => self.bold = true,
            'm' => self.strikethrough = true,
            'n' => self.underlined = true,
            'o' => self.italic = true,
            other => {
                if let Some(fc) = lookup_code(other)
                    && let Some(name) = fc.color
                {
                    *self = Self {
                        color: Some(name),
                        ..Self::default()
                    };
                }
            }
        }
    }

    fn to_text_style(&self, explicit_reset: bool) -> TextStyle {
        let mut style = TextStyle::default();
        if let Some(c) = self.color {
            style.color = Some(c.to_owned());
        }
        if explicit_reset {
            style.bold = Some(self.bold);
            style.italic = Some(self.italic);
            style.underlined = Some(self.underlined);
            style.strikethrough = Some(self.strikethrough);
            style.obfuscated = Some(self.obfuscated);
        } else {
            if self.bold {
                style.bold = Some(true);
            }
            if self.italic {
                style.italic = Some(true);
            }
            if self.underlined {
                style.underlined = Some(true);
            }
            if self.strikethrough {
                style.strikethrough = Some(true);
            }
            if self.obfuscated {
                style.obfuscated = Some(true);
            }
        }
        style
    }
}

/// Converts a rename string into a `Text` component for `minecraft:custom_name`
/// (`FormattedStringUtil.getAsComponent`).
///
/// When `allow_formatting` is `true`, parses `§` codes and explicitly sets non-active decorations
/// to `false` (matching `FormattedStringUtil.EMPTY`) so vanilla's default custom-name italics do
/// not override the player's formatting.
pub fn string_to_component(text: &str, allow_formatting: bool) -> Text {
    if !allow_formatting {
        return Text::literal(text);
    }

    let mut segments: Vec<(FlatStyle, String)> = Vec::new();
    let mut style = FlatStyle::default();
    let mut buf = String::new();
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '§'
            && let Some(&code) = chars.peek()
        {
            let _ = chars.next();
            if !buf.is_empty() {
                segments.push((style.clone(), std::mem::take(&mut buf)));
            }
            style.apply_code(code);
            continue;
        }
        buf.push(c);
    }
    if !buf.is_empty() {
        segments.push((style, buf));
    }

    if segments.is_empty() {
        return Text::literal("");
    }
    if segments.len() == 1 {
        let (st, s) = segments.remove(0);
        return Text::literal(s).with_style(st.to_text_style(true));
    }

    let mut root = Text::literal("").with_style(FlatStyle::default().to_text_style(true));
    for (st, s) in segments {
        root = root.append(Text::literal(s).with_style(st.to_text_style(true)));
    }
    root
}

/// Builds a `Text` for rendering inside `FormattableEditBox`: keeps `§x` codes visible in default
/// style while applying each code's formatting to subsequent characters
/// (`FormattedStringDecomposer.iterateFormatted`).
pub fn editable_preview_component(text: &str, allow_formatting: bool) -> Text {
    if !allow_formatting || text.is_empty() {
        return Text::literal(text);
    }

    let mut root = Text::literal("");
    let mut style = FlatStyle::default();
    let mut buf = String::new();
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '§' {
            if !buf.is_empty() {
                root = root.append(
                    Text::literal(std::mem::take(&mut buf)).with_style(style.to_text_style(false)),
                );
            }
            let mut code_str = String::from('§');
            if let Some(&next) = chars.peek() {
                let _ = chars.next();
                code_str.push(next);
                style.apply_code(next);
            }
            root = root.append(Text::literal(code_str));
            continue;
        }
        buf.push(c);
    }
    if !buf.is_empty() {
        root = root.append(Text::literal(buf).with_style(style.to_text_style(false)));
    }
    root
}

/// Serializes a `Text` component back into a string with `§` codes (`FormattedStringUtil.getAsString`).
pub fn component_to_string(host: &dyn Host, text: &Text, allow_formatting: bool) -> String {
    if !allow_formatting {
        return host.text_string(text);
    }
    let mut out = String::new();
    let mut emitted_style = FlatStyle::default();
    collect_component_legacy(
        host,
        text,
        &FlatStyle::default(),
        &mut emitted_style,
        &mut out,
    );
    out
}

fn merge_style(parent: &FlatStyle, own: &TextStyle) -> FlatStyle {
    let mut out = parent.clone();
    if let Some(color_str) = own.color.as_deref()
        && let Some(code) = color_name_to_code(color_str)
        && let Some(fc) = lookup_code(code)
        && let Some(name) = fc.color
    {
        out.color = Some(name);
    }
    if let Some(v) = own.bold {
        out.bold = v;
    }
    if let Some(v) = own.italic {
        out.italic = v;
    }
    if let Some(v) = own.underlined {
        out.underlined = v;
    }
    if let Some(v) = own.strikethrough {
        out.strikethrough = v;
    }
    if let Some(v) = own.obfuscated {
        out.obfuscated = v;
    }
    out
}

fn push_style_transition(out: &mut String, cur: &mut FlatStyle, target: &FlatStyle) {
    if cur == target {
        return;
    }
    let need_reset = (cur.bold && !target.bold)
        || (cur.italic && !target.italic)
        || (cur.underlined && !target.underlined)
        || (cur.strikethrough && !target.strikethrough)
        || (cur.obfuscated && !target.obfuscated)
        || (cur.color.is_some() && target.color.is_none());

    if need_reset {
        out.push('§');
        out.push('r');
        *cur = FlatStyle::default();
    }
    if cur.color != target.color
        && let Some(color) = target.color
        && let Some(code) = color_name_to_code(color)
    {
        out.push('§');
        out.push(code);
        *cur = FlatStyle {
            color: Some(color),
            ..FlatStyle::default()
        };
    }
    if !cur.bold && target.bold {
        out.push_str("§l");
        cur.bold = true;
    }
    if !cur.italic && target.italic {
        out.push_str("§o");
        cur.italic = true;
    }
    if !cur.underlined && target.underlined {
        out.push_str("§n");
        cur.underlined = true;
    }
    if !cur.strikethrough && target.strikethrough {
        out.push_str("§m");
        cur.strikethrough = true;
    }
    if !cur.obfuscated && target.obfuscated {
        out.push_str("§k");
        cur.obfuscated = true;
    }
}

fn collect_component_legacy(
    host: &dyn Host,
    text: &Text,
    parent: &FlatStyle,
    emitted: &mut FlatStyle,
    out: &mut String,
) {
    let effective = merge_style(parent, &text.style);
    let piece = match &text.content {
        TextContent::Literal(s) => s.clone(),
        other => host.text_string(&Text::new(other.clone())),
    };
    if !piece.is_empty() {
        push_style_transition(out, emitted, &effective);
        out.push_str(&piece);
    }
    for child in &text.children {
        collect_component_legacy(host, child, &effective, emitted, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_length_ignores_formatting_pairs() {
        assert_eq!(visible_length("§6§lExcalibur", true), 9);
        assert_eq!(visible_length("§6§lExcalibur", false), 13);
        assert_eq!(visible_length("§6§r", true), 0);
        assert_eq!(strip_formatting("§aHello §lWorld"), "Hello World");
    }

    #[test]
    fn filter_text_respects_formatting_flag() {
        assert_eq!(filter_text("§6Sword\n\x7f!", true), "§6Sword!");
        assert_eq!(filter_text("§6Sword\n\x7f!", false), "6Sword!");
    }

    #[test]
    fn string_to_component_sets_explicit_non_italic_and_color() {
        let comp = string_to_component("§6§lBlade", true);
        assert_eq!(comp.content, TextContent::Literal("Blade".to_owned()));
        assert_eq!(comp.style.color.as_deref(), Some("gold"));
        assert_eq!(comp.style.bold, Some(true));
        assert_eq!(comp.style.italic, Some(false));
    }

    #[test]
    fn string_to_component_splits_multiple_styled_segments() {
        let comp = string_to_component("§cFire §9Ice", true);
        assert_eq!(comp.children.len(), 2);
        assert_eq!(
            comp.children[0].content,
            TextContent::Literal("Fire ".to_owned())
        );
        assert_eq!(comp.children[0].style.color.as_deref(), Some("red"));
        assert_eq!(
            comp.children[1].content,
            TextContent::Literal("Ice".to_owned())
        );
        assert_eq!(comp.children[1].style.color.as_deref(), Some("blue"));
    }

    #[test]
    fn editable_preview_keeps_codes_visible() {
        let comp = editable_preview_component("§6Hi", true);
        assert_eq!(comp.children.len(), 2);
        assert_eq!(
            comp.children[0].content,
            TextContent::Literal("§6".to_owned())
        );
        assert_eq!(comp.children[0].style.color, None);
        assert_eq!(
            comp.children[1].content,
            TextContent::Literal("Hi".to_owned())
        );
        assert_eq!(comp.children[1].style.color.as_deref(), Some("gold"));
    }
}
