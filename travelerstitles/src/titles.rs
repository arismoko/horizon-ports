//! The title rules: timers, fade, recently shown entries, translation keys and layout.
//! Pure logic with no host access, so it is unit-tested.

use std::collections::VecDeque;

/// One title renderer's options, read from the settings each time they are needed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    pub enabled: bool,
    pub fade_in: i32,
    pub display: i32,
    pub fade_out: i32,
    pub color: u32,
    pub size: f32,
    pub shadow: bool,
    pub x: i32,
    pub y: i32,
    pub centered: bool,
}

impl Style {
    /// Ticks from the title's start to its end.
    pub fn total_ticks(&self) -> i32 {
        self.fade_in
            .max(0)
            .saturating_add(self.display.max(0))
            .saturating_add(self.fade_out.max(0))
    }
}

/// One title: what is shown, how long is left, and what was shown lately.
#[derive(Clone, Debug, Default)]
pub struct Title {
    /// The last title shown (kept after it fades, like the original's `displayedTitle`).
    pub displayed: Option<String>,
    /// `0xRRGGBB` of the shown title.
    pub color: u32,
    /// Ticks left of the current title; 0 when none is showing.
    pub timer: i32,
    /// Ticks until another title may start.
    pub cooldown: i32,
    recent: VecDeque<String>,
}

impl Title {
    /// One client tick: the title and the cooldown each count down.
    pub fn tick(&mut self) {
        if self.timer > 0 {
            self.timer -= 1;
        }
        if self.cooldown > 0 {
            self.cooldown -= 1;
        }
    }

    pub fn display(&mut self, text: String, color: u32, style: &Style) {
        self.displayed = Some(text);
        self.color = color;
        self.timer = style.total_ticks();
    }

    pub fn clear_timer(&mut self) {
        self.timer = 0;
    }

    pub fn is_showing(&self) -> bool {
        self.timer > 0
    }

    /// `TitleRenderer.addRecentEntry`: the oldest entry goes when the list is full; a list of
    /// size 0 keeps nothing.
    pub fn add_recent(&mut self, max: usize, entry: String) {
        if self.recent.len() >= max && !self.recent.is_empty() {
            self.recent.pop_front();
        }
        if max > 0 {
            self.recent.push_back(entry);
        }
    }

    pub fn recently_shown(&self, entry: &str) -> bool {
        self.recent.iter().any(|e| e == entry)
    }

    pub fn clear_recent(&mut self) {
        self.recent.clear();
    }

    /// Forget everything: timer, recent entries and the last shown title.
    #[cfg(test)]
    pub fn reset(&mut self) {
        self.clear_timer();
        self.recent.clear();
        self.displayed = None;
    }
}

/// `Util.makeDescriptionId(prefix, id)`: `prefix.namespace.path` with `/` in the path as `.`.
/// None when `id` is not `namespace:path`.
pub fn description_id(prefix: &str, id: &str) -> Option<String> {
    let (namespace, path) = id.split_once(':')?;
    if namespace.is_empty() || path.is_empty() {
        return None;
    }
    Some(format!("{prefix}.{namespace}.{}", path.replace('/', ".")))
}

/// A colour entry of a language file or config (`Long.parseLong(text, 16)`), as `0xRRGGBB`.
/// Anything unreadable is white.
pub fn parse_color(text: &str) -> u32 {
    let text = text.trim();
    let digits = text.strip_prefix('+').unwrap_or(text);
    match u64::from_str_radix(digits, 16) {
        Ok(value) if !digits.is_empty() && !digits.starts_with('-') => (value & 0xFF_FFFF) as u32,
        _ => 0xFF_FFFF,
    }
}

/// The title's opacity, 0 to 255 (`TitleRenderer.renderText`): it fades in while more than
/// `fade_out + display` ticks are left, holds, then fades out over the last `fade_out`.
pub fn opacity(timer: i32, partial: f32, style: &Style) -> i32 {
    let age = timer as f32 - partial;
    let mut opacity = 255;
    let out = style.fade_out.max(0);
    let held = style.display.max(0);
    let fade_in = style.fade_in.max(0);
    if timer > out.saturating_add(held) {
        let elapsed = (fade_in as f32 + held as f32 + out as f32) - age;
        opacity = (elapsed * 255.0 / (fade_in.max(1) as f32)) as i32;
    }
    if timer <= out {
        opacity = (age * 255.0 / (out.max(1) as f32)) as i32;
    }
    opacity.clamp(0, 255)
}

/// Where the title's top-left corner is on the screen, in GUI pixels. The original scales
/// the pose, so the offsets and the half-width are scaled with the text.
pub fn origin(style: &Style, text_width: f32, screen: [f32; 2]) -> [f32; 2] {
    let width = text_width as i32;
    let (base_x, base_y, x) = if style.centered {
        (screen[0] / 2.0, screen[1] / 2.0, style.x - width / 2)
    } else {
        (0.0, 0.0, style.x)
    };
    [
        base_x + style.size * x as f32,
        base_y + style.size * style.y as f32,
    ]
}

/// The title's text backdrop (`TitleRenderer.drawBackdrop`) as `[x, y, width, height]` in
/// screen pixels: the text's box with two pixels of padding, scaled with the text.
pub fn backdrop_rect(at: [f32; 2], text_width: f32, size: f32) -> [f32; 4] {
    const LINE_HEIGHT: f32 = 9.0;
    const PADDING: f32 = 2.0;
    [
        at[0] - PADDING * size,
        at[1] - PADDING * size,
        (text_width + 2.0 * PADDING) * size,
        (LINE_HEIGHT + 2.0 * PADDING) * size,
    ]
}

/// The backdrop colour: the game's text-background colour (`Options.getBackgroundColor`)
/// with its alpha scaled by the title's fade (`opacity`, 0 to 255). Fully transparent
/// when the player's Text Background option is limited to chat.
pub fn backdrop_argb(background: u32, opacity: i32) -> u32 {
    let alpha = (background >> 24) * opacity.clamp(0, 255) as u32 / 255;
    (alpha << 24) | (background & 0xFF_FFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style() -> Style {
        Style {
            enabled: true,
            fade_in: 10,
            display: 50,
            fade_out: 10,
            color: 0xFF_FFFF,
            size: 2.0,
            shadow: true,
            x: 0,
            y: -33,
            centered: true,
        }
    }

    #[test]
    fn description_ids_follow_vanilla() {
        assert_eq!(
            description_id("biome", "minecraft:plains").as_deref(),
            Some("biome.minecraft.plains")
        );
        assert_eq!(
            description_id("travelerstitles.biome", "terralith:cave/deep_caves").as_deref(),
            Some("travelerstitles.biome.terralith.cave.deep_caves")
        );
        assert_eq!(description_id("biome", "plains"), None);
        assert_eq!(description_id("biome", ":x"), None);
    }

    #[test]
    fn colours_parse_as_hex_and_fall_back_to_white() {
        assert_eq!(parse_color("750909"), 0x75_0909);
        assert_eq!(parse_color(" c2b740 "), 0xC2_B740);
        assert_eq!(parse_color("zz"), 0xFF_FFFF);
        assert_eq!(parse_color(""), 0xFF_FFFF);
        assert_eq!(parse_color("-1"), 0xFF_FFFF);
        assert_eq!(parse_color("ffffffffffffffffffff"), 0xFF_FFFF);
        assert_eq!(parse_color("12345678"), 0x34_5678);
    }

    #[test]
    fn opacity_fades_in_holds_and_fades_out() {
        let s = style();
        // 70 ticks in all: fade-in while more than 60 are left.
        assert_eq!(opacity(70, 0.0, &s), 0);
        assert_eq!(opacity(65, 0.0, &s), 127);
        assert_eq!(opacity(60, 0.0, &s), 255);
        assert_eq!(opacity(30, 0.0, &s), 255);
        assert_eq!(opacity(10, 0.0, &s), 255);
        assert_eq!(opacity(5, 0.0, &s), 127);
        assert_eq!(opacity(1, 0.5, &s), 12);
        assert_eq!(opacity(1, 5.0, &s), 0);
    }

    #[test]
    fn zero_fades_never_divide_by_zero() {
        let mut s = style();
        s.fade_in = 0;
        s.fade_out = 0;
        assert_eq!(opacity(50, 0.0, &s), 255);
        assert_eq!(opacity(1, 0.0, &s), 255);
        s.display = -5;
        assert_eq!(s.total_ticks(), 0);
    }

    #[test]
    fn timers_count_down_and_stop_at_zero() {
        let s = style();
        let mut t = Title::default();
        t.display("Plains".into(), 0xFF_FFFF, &s);
        assert_eq!(t.timer, 70);
        assert!(t.is_showing());
        t.cooldown = 1;
        t.tick();
        assert_eq!((t.timer, t.cooldown), (69, 0));
        t.tick();
        assert_eq!(t.cooldown, 0);
        t.clear_timer();
        assert!(!t.is_showing());
        assert_eq!(t.displayed.as_deref(), Some("Plains"));
        t.reset();
        assert_eq!(t.displayed, None);
    }

    #[test]
    fn the_recent_list_keeps_the_newest_entries() {
        let mut t = Title::default();
        t.add_recent(2, "a".into());
        t.add_recent(2, "b".into());
        t.add_recent(2, "c".into());
        assert!(!t.recently_shown("a"));
        assert!(t.recently_shown("b") && t.recently_shown("c"));
        t.add_recent(0, "d".into());
        assert!(!t.recently_shown("d"));
        // A list shrunk by the settings still drops one entry per add, as the original does.
        assert!(t.recently_shown("c"));
        t.clear_recent();
        assert!(!t.recently_shown("c"));
    }

    #[test]
    fn origin_scales_offsets_about_the_screen_centre() {
        let s = style();
        // width 40 -> x = -20, scaled 2x -> -40 from the centre; y = -33 * 2.
        assert_eq!(origin(&s, 40.0, [320.0, 240.0]), [120.0, 54.0]);
        let mut left = style();
        left.centered = false;
        left.x = 5;
        assert_eq!(origin(&left, 40.0, [320.0, 240.0]), [10.0, -66.0]);
    }

    #[test]
    fn the_backdrop_pads_the_text_box_and_scales_with_it() {
        assert_eq!(
            backdrop_rect([100.0, 50.0], 40.0, 2.0),
            [96.0, 46.0, 88.0, 26.0]
        );
    }

    #[test]
    fn the_backdrop_alpha_follows_the_fade_and_keeps_the_colour() {
        assert_eq!(backdrop_argb(0x40_000000, 255), 0x40_000000);
        assert_eq!(backdrop_argb(0x40_102030, 0), 0x00_102030);
        assert_eq!(backdrop_argb(0xFF_000000, 128), 0x80_000000);
        assert_eq!(backdrop_argb(0x00_000000, 255), 0);
        assert_eq!(backdrop_argb(0x40_000000, 999), 0x40_000000);
    }
}
