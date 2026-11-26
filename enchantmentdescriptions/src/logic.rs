//! The mod's pure rules: description keys, tooltip line matching, the screen test and the
//! `config/enchantmentdescriptions.toml` reader. Nothing here touches the game, and no input panics.

/// The key suffixes tried for a description, in the original's order (`desc`, `description`,
/// `info`); each is also tried with the level appended (`.desc.3`).
pub const KEY_TYPES: [&str; 3] = ["desc", "description", "info"];

/// The translation key of the name of enchantment `id` (`minecraft:sharpness`), and the base of its description keys: `enchantment.minecraft.sharpness`.
pub fn name_key(id: &str) -> Option<String> {
    let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
    if namespace.is_empty() || path.is_empty() {
        return None;
    }
    Some(format!("enchantment.{namespace}.{path}"))
}

/// The Options > Mods section (a translation key) a setting is listed under.
pub fn category_of(name: &str) -> Option<&'static str> {
    match name {
        "enabled" | "only_on_books" | "only_in_enchanting_table" => {
            Some("enchdesc.category.display")
        }
        "require_keybind" | "display_activate_text" => Some("enchdesc.category.hint"),
        "color" => Some("enchdesc.category.style"),
        _ => None,
    }
}

/// A translation template with its `%s`, `%1$s` and `%%` filled from `arguments`; a missing
/// argument leaves nothing.
pub fn fill_template(template: &str, arguments: &[String]) -> String {
    let mut out = String::new();
    let mut next = 0;
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.peek().copied() {
            Some('%') => {
                chars.next();
                out.push('%');
            }
            Some('s') => {
                chars.next();
                if let Some(arg) = arguments.get(next) {
                    out.push_str(arg);
                }
                next += 1;
            }
            Some(d) if d.is_ascii_digit() => {
                let mut digits = String::new();
                while let Some(d) = chars.peek().copied().filter(char::is_ascii_digit) {
                    digits.push(d);
                    chars.next();
                }
                if chars.peek() == Some(&'$') {
                    chars.next();
                    if chars.peek() == Some(&'s') {
                        chars.next();
                    }
                    let index = digits.parse::<usize>().ok().and_then(|n| n.checked_sub(1));
                    if let Some(arg) = index.and_then(|i| arguments.get(i)) {
                        out.push_str(arg);
                    }
                } else {
                    out.push('%');
                    out.push_str(&digits);
                }
            }
            _ => out.push('%'),
        }
    }
    out
}

/// The first description key the language defines for `base` at `level`: for each of
/// [`KEY_TYPES`], `base.type`, then `base.type.level`.
pub fn description_key(base: &str, level: u32, has: impl Fn(&str) -> bool) -> Option<String> {
    for kind in KEY_TYPES {
        let key = format!("{base}.{kind}");
        if has(&key) {
            return Some(key);
        }
        let leveled = format!("{key}.{level}");
        if has(&leveled) {
            return Some(leveled);
        }
    }
    None
}

/// `Enchantment.getFullname`: the level's text follows the name unless both the level and the
/// enchantment's maximum level are one.
pub fn full_name(
    mut name: String,
    level: u32,
    max_level: u32,
    level_text: impl FnOnce(u32) -> String,
) -> String {
    if level != 1 || max_level != 1 {
        name.push(' ');
        name.push_str(&level_text(level));
    }
    name
}

/// For each wanted line text in order, the index of the first tooltip line with exactly that
/// text that no earlier wanted text took: `(wanted index, line index)`. `line_keys` holds each
/// line's text, `None` for a line that could not be read.
pub fn match_lines(line_keys: &[Option<String>], wanted: &[String]) -> Vec<(usize, usize)> {
    let mut used = vec![false; line_keys.len()];
    let mut matches = Vec::new();
    for (w, key) in wanted.iter().enumerate() {
        let found = line_keys
            .iter()
            .enumerate()
            .find(|(i, line)| !used[*i] && line.as_deref() == Some(key.as_str()));
        if let Some((i, _)) = found {
            used[i] = true;
            matches.push((w, i));
        }
    }
    matches
}

/// Whether a menu type id is the enchanting table's.
pub fn is_enchanting_menu(menu: Option<&str>) -> bool {
    menu == Some("minecraft:enchantment")
}

/// Whether a config string is a JSON text component (a string, object or list) rather than
/// plain text.
pub fn looks_like_json(text: &str) -> bool {
    matches!(text.trim_start().chars().next(), Some('{' | '[' | '"'))
}

/// What `config/enchantmentdescriptions.toml` holds: the parts of the original's config a settings screen
/// cannot express.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FileConfig {
    /// Text before each description (the original's `prefix`).
    pub prefix: String,
    /// Text after each description (the original's `suffix`).
    pub suffix: String,
    /// The hint shown while Shift is required and not held; empty keeps the translated default.
    pub activate_text: String,
    /// The description style's decorations (the original's `style`); unset keeps them inherited.
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underlined: Option<bool>,
    pub strikethrough: Option<bool>,
    pub obfuscated: Option<bool>,
    /// The description style's font id; empty keeps the default font.
    pub font: String,
}

/// The file written when `config/enchantmentdescriptions.toml` is missing.
pub const DEFAULT_TOML: &str = r##"# Enchantment Descriptions, extra options.
# The on/off options and the description colour are in Options > Mods > Enchantment Descriptions.
#
# A text value is plain text, or a JSON text component when it starts with { [ or ",
# for example '{"text":"> ","color":"gold"}'.

# Text added before each description. This can be used to add indents and other decorators.
prefix = ""

# Text added after each description.
suffix = ""

# The hint shown when Shift is required and not held. Empty uses the translated default.
activate_text = ""

# Decorations of the description text. Remove the # to set one.
# bold = false
# italic = false
# underlined = false
# strikethrough = false
# obfuscated = false

# The font of the description text, such as "minecraft:uniform". Empty is the default font.
font = ""
"##;

enum Value {
    Text(String),
    Flag(bool),
}

/// Reads `config/enchantmentdescriptions.toml`: `key = "text"`, `key = 'literal'` and `key = true|false`
/// lines, `#` comments. Unknown keys and unreadable lines are skipped.
pub fn parse_config(text: &str) -> FileConfig {
    let mut config = FileConfig::default();
    for line in text.lines() {
        let Some((key, raw)) = line.split_once('=') else {
            continue;
        };
        let Some(value) = parse_value(raw.trim()) else {
            continue;
        };
        match (key.trim(), value) {
            ("prefix", Value::Text(v)) => config.prefix = v,
            ("suffix", Value::Text(v)) => config.suffix = v,
            ("activate_text", Value::Text(v)) => config.activate_text = v,
            ("font", Value::Text(v)) => config.font = v,
            ("bold", Value::Flag(v)) => config.bold = Some(v),
            ("italic", Value::Flag(v)) => config.italic = Some(v),
            ("underlined", Value::Flag(v)) => config.underlined = Some(v),
            ("strikethrough", Value::Flag(v)) => config.strikethrough = Some(v),
            ("obfuscated", Value::Flag(v)) => config.obfuscated = Some(v),
            _ => {}
        }
    }
    config
}

fn parse_value(raw: &str) -> Option<Value> {
    let mut chars = raw.chars();
    let quote = chars.next()?;
    let rest = |after: &str| after.trim().is_empty() || after.trim_start().starts_with('#');
    match quote {
        '\'' => {
            let body = &raw[1..];
            let end = body.find('\'')?;
            rest(&body[end + 1..]).then(|| Value::Text(body[..end].to_owned()))
        }
        '"' => {
            let mut out = String::new();
            let mut chars = raw[1..].char_indices();
            while let Some((i, c)) = chars.next() {
                match c {
                    '"' => {
                        return rest(&raw[1 + i + 1..]).then_some(Value::Text(out));
                    }
                    '\\' => match chars.next()?.1 {
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        'r' => out.push('\r'),
                        '"' => out.push('"'),
                        '\\' => out.push('\\'),
                        'u' => out.push(unicode_escape(&mut chars, 4)?),
                        'U' => out.push(unicode_escape(&mut chars, 8)?),
                        _ => return None,
                    },
                    c => out.push(c),
                }
            }
            None
        }
        _ => {
            let word = raw.split('#').next().unwrap_or("").trim();
            match word {
                "true" => Some(Value::Flag(true)),
                "false" => Some(Value::Flag(false)),
                _ => None,
            }
        }
    }
}

fn unicode_escape(chars: &mut std::str::CharIndices<'_>, digits: usize) -> Option<char> {
    let mut code = 0u32;
    for _ in 0..digits {
        code = code.checked_mul(16)? + chars.next()?.1.to_digit(16)?;
    }
    char::from_u32(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_keys_follow_the_registry_id() {
        assert_eq!(
            name_key("minecraft:sharpness").as_deref(),
            Some("enchantment.minecraft.sharpness")
        );
        assert_eq!(name_key("mod:a/b").as_deref(), Some("enchantment.mod.a/b"));
        assert_eq!(
            name_key("mending").as_deref(),
            Some("enchantment.minecraft.mending")
        );
        assert_eq!(name_key(":x"), None);
        assert_eq!(name_key("x:"), None);
        assert_eq!(name_key(""), None);
    }

    #[test]
    fn description_keys_try_types_in_order_then_levels() {
        let base = "enchantment.minecraft.sharpness";
        let has = |defined: &'static [&'static str]| {
            move |key: &str| {
                let defined: &[&str] = defined;
                defined.contains(&key)
            }
        };
        assert_eq!(description_key(base, 1, has(&[])), None);
        assert_eq!(
            description_key(base, 1, has(&["enchantment.minecraft.sharpness.desc"])).as_deref(),
            Some("enchantment.minecraft.sharpness.desc")
        );
        assert_eq!(
            description_key(
                base,
                2,
                has(&[
                    "enchantment.minecraft.sharpness.description",
                    "enchantment.minecraft.sharpness.desc.2",
                ])
            )
            .as_deref(),
            Some("enchantment.minecraft.sharpness.desc.2")
        );
        assert_eq!(
            description_key(
                base,
                3,
                has(&[
                    "enchantment.minecraft.sharpness.info",
                    "enchantment.minecraft.sharpness.description.3",
                ])
            )
            .as_deref(),
            Some("enchantment.minecraft.sharpness.description.3")
        );
        // A level key for another level does not match.
        assert_eq!(
            description_key(base, 1, has(&["enchantment.minecraft.sharpness.desc.2"])),
            None
        );
    }

    #[test]
    fn lines_match_once_each_and_in_order() {
        let lines = vec![
            Some("item.name".to_owned()),
            Some("Sharpness III".to_owned()),
            None,
            Some("Sharpness III".to_owned()),
            Some("Mending".to_owned()),
        ];
        let wanted = vec![
            "Mending".to_owned(),
            "Sharpness III".to_owned(),
            "Sharpness III".to_owned(),
            "Sharpness III".to_owned(),
            "Unknown".to_owned(),
        ];
        assert_eq!(match_lines(&lines, &wanted), vec![(0, 4), (1, 1), (2, 3)]);
        assert!(match_lines(&[], &wanted).is_empty());
        assert!(match_lines(&lines, &[]).is_empty());
    }

    #[test]
    fn settings_are_categorised() {
        assert_eq!(category_of("enabled"), Some("enchdesc.category.display"));
        assert_eq!(
            category_of("require_keybind"),
            Some("enchdesc.category.hint")
        );
        assert_eq!(category_of("color"), Some("enchdesc.category.style"));
        assert_eq!(category_of("other"), None);
    }

    #[test]
    fn templates_fill_arguments() {
        let args = vec!["A".to_owned(), "B".to_owned()];
        assert_eq!(fill_template("x %s y %s", &args), "x A y B");
        assert_eq!(fill_template("%2$s %1$s", &args), "B A");
        assert_eq!(fill_template("100%% %s %s %s", &args), "100% A B ");
        assert_eq!(fill_template("%", &args), "%");
        assert_eq!(fill_template("%9$s %0$s %12", &args), " %12");
        assert_eq!(fill_template("%é", &[]), "%é");
    }

    #[test]
    fn the_enchanting_menu_is_minecraft_enchantment() {
        assert!(is_enchanting_menu(Some("minecraft:enchantment")));
        assert!(!is_enchanting_menu(Some("minecraft:generic_9x3")));
        assert!(!is_enchanting_menu(None));
    }

    #[test]
    fn full_names_carry_the_level_unless_both_are_one() {
        let level = |n: u32| format!("L{n}");
        assert_eq!(full_name("Mending".into(), 1, 1, level), "Mending");
        assert_eq!(full_name("Sharpness".into(), 1, 5, level), "Sharpness L1");
        assert_eq!(full_name("Sharpness".into(), 3, 5, level), "Sharpness L3");
        assert_eq!(full_name("Odd".into(), 2, 1, level), "Odd L2");
    }

    #[test]
    fn json_text_is_recognised() {
        assert!(looks_like_json(r#"{"text":"a"}"#));
        assert!(looks_like_json("  [\"a\"]"));
        assert!(looks_like_json("\"a\""));
        assert!(!looks_like_json("  - "));
        assert!(!looks_like_json(""));
    }

    #[test]
    fn the_default_file_parses_to_the_default_config() {
        assert_eq!(parse_config(DEFAULT_TOML), FileConfig::default());
    }

    #[test]
    fn config_values_are_read() {
        let config = parse_config(
            r#"
prefix = "  \u2022 "   # indent
suffix = '{"text":"!"}'
activate_text = "Hold \"Shift\""
bold = true
italic = false
font = "minecraft:uniform"
unknown = 3
"#,
        );
        assert_eq!(config.prefix, "  \u{2022} ");
        assert_eq!(config.suffix, r#"{"text":"!"}"#);
        assert_eq!(config.activate_text, "Hold \"Shift\"");
        assert_eq!(config.bold, Some(true));
        assert_eq!(config.italic, Some(false));
        assert_eq!(config.underlined, None);
        assert_eq!(config.font, "minecraft:uniform");
    }

    #[test]
    fn garbage_config_never_panics() {
        for text in [
            "",
            "=",
            "prefix",
            "prefix = ",
            "prefix = \"unterminated",
            "prefix = \"bad \\q escape\"",
            "prefix = \"\\u12\"",
            "prefix = \"\\uD800\"",
            "prefix = \"\\UFFFFFFFF\"",
            "prefix = 'open",
            "prefix = \"é\" trailing",
            "bold = maybe",
            "bold = \"true\"",
            "\u{0}\u{1}=\u{2}",
            "prefix = \"é",
        ] {
            let _ = parse_config(text);
        }
        assert_eq!(parse_config("prefix = \"a\" b").prefix, "");
        assert_eq!(parse_config("bold = true # yes").bold, Some(true));
        assert_eq!(parse_config("prefix = \"é\"").prefix, "é");
    }
}
