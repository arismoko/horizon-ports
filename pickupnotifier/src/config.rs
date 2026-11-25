//! Client and server configuration, TOML persistence, item filters, and Options -> Mods settings.

use mod_api::{ClientHost, Config, ConfigKind, Identifier, ItemKey, Registrar, SettingSpec};

pub const MOD_ID: &str = "pickupnotifier";
pub const CLIENT_CONFIG_FILE: &str = "pickupnotifier-client.toml";
pub const SERVER_CONFIG_FILE: &str = "pickupnotifier-server.toml";

// Setting keys registered in Options -> Mods -> Pick Up Notifier.
pub const SET_CLIENT_ONLY: &str = "client_only";
pub const SET_INCLUDE_ITEMS: &str = "include_items";
pub const SET_INCLUDE_EXPERIENCE: &str = "include_experience";
pub const SET_INCLUDE_ARROWS: &str = "include_arrows";
pub const SET_EXPERIENCE_VALUE: &str = "experience_value";
pub const SET_DISABLE_IN_CREATIVE: &str = "disable_in_creative";
pub const SET_COMBINE_ENTRIES: &str = "combine_entries";
pub const SET_DISPLAY_TIME: &str = "display_time";
pub const SET_MOVE_OUT: &str = "move_out";
pub const SET_MOVE_TIME: &str = "move_time";
pub const SET_FADE_OUT: &str = "fade_out";
pub const SET_DRAW_SPRITE: &str = "draw_sprite";
pub const SET_DEFAULT_TEXT_COLOR: &str = "default_text_color";
pub const SET_IGNORE_RARITY: &str = "ignore_rarity";
pub const SET_SCREEN_CORNER: &str = "screen_corner";
pub const SET_OFFSET_X: &str = "offset_x";
pub const SET_OFFSET_Y: &str = "offset_y";
pub const SET_MAX_HEIGHT: &str = "max_height";
pub const SET_DISPLAY_SCALE: &str = "display_scale";
pub const SET_DISPLAY_AMOUNT: &str = "display_amount";
pub const SET_INVENTORY_COUNT: &str = "inventory_count";
pub const SET_DISPLAY_SINGLE_COUNT: &str = "display_single_count";
pub const SET_ENTRY_BACKGROUND: &str = "entry_background";
pub const SET_DISPLAY_ITEM_NAME: &str = "display_item_name";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombineEntries {
    Always,
    Never,
    ExcludeNamed,
}

impl CombineEntries {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Always => "ALWAYS",
            Self::Never => "NEVER",
            Self::ExcludeNamed => "EXCLUDE_NAMED",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "ALWAYS" => Some(Self::Always),
            "NEVER" => Some(Self::Never),
            "EXCLUDE_NAMED" => Some(Self::ExcludeNamed),
            _ => None,
        }
    }

    pub fn to_choice(self) -> u32 {
        match self {
            Self::Always => 0,
            Self::Never => 1,
            Self::ExcludeNamed => 2,
        }
    }

    pub fn from_choice(idx: u32) -> Self {
        match idx {
            0 => Self::Always,
            1 => Self::Never,
            _ => Self::ExcludeNamed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovementDirection {
    None,
    Horizontal,
    Vertical,
    All,
}

impl MovementDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::Horizontal => "HORIZONTAL",
            Self::Vertical => "VERTICAL",
            Self::All => "ALL",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "NONE" => Some(Self::None),
            "HORIZONTAL" => Some(Self::Horizontal),
            "VERTICAL" => Some(Self::Vertical),
            "ALL" => Some(Self::All),
            _ => None,
        }
    }

    pub fn to_choice(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Horizontal => 1,
            Self::Vertical => 2,
            Self::All => 3,
        }
    }

    pub fn from_choice(idx: u32) -> Self {
        match idx {
            0 => Self::None,
            1 => Self::Horizontal,
            2 => Self::Vertical,
            _ => Self::All,
        }
    }

    pub fn moves_horizontal(self, anchor: AnchorPoint) -> bool {
        match self {
            Self::None | Self::Vertical => false,
            Self::Horizontal => !anchor.is_horizontal_center(),
            Self::All => anchor != AnchorPoint::Center,
        }
    }

    pub fn moves_vertical(self, anchor: AnchorPoint) -> bool {
        match self {
            Self::None | Self::Horizontal => false,
            Self::Vertical => !anchor.is_vertical_center(),
            Self::All => anchor != AnchorPoint::Center,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayAmount {
    Off,
    Sprite,
    Text,
    Both,
}

impl DisplayAmount {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Sprite => "SPRITE",
            Self::Text => "TEXT",
            Self::Both => "BOTH",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "OFF" => Some(Self::Off),
            "SPRITE" => Some(Self::Sprite),
            "TEXT" => Some(Self::Text),
            "BOTH" => Some(Self::Both),
            _ => None,
        }
    }

    pub fn to_choice(self) -> u32 {
        match self {
            Self::Off => 0,
            Self::Sprite => 1,
            Self::Text => 2,
            Self::Both => 3,
        }
    }

    pub fn from_choice(idx: u32) -> Self {
        match idx {
            0 => Self::Off,
            1 => Self::Sprite,
            2 => Self::Text,
            _ => Self::Both,
        }
    }

    pub fn is_sprite(self) -> bool {
        matches!(self, Self::Sprite | Self::Both)
    }

    pub fn is_text(self) -> bool {
        matches!(self, Self::Text | Self::Both)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryBackground {
    None,
    Chat,
    Tooltip,
}

impl EntryBackground {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::Chat => "CHAT",
            Self::Tooltip => "TOOLTIP",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "NONE" => Some(Self::None),
            "CHAT" => Some(Self::Chat),
            "TOOLTIP" => Some(Self::Tooltip),
            _ => None,
        }
    }

    pub fn to_choice(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Chat => 1,
            Self::Tooltip => 2,
        }
    }

    pub fn from_choice(idx: u32) -> Self {
        match idx {
            0 => Self::None,
            1 => Self::Chat,
            _ => Self::Tooltip,
        }
    }
}

/// Nine-point screen anchor matching PuzzlesLib's `AnchorPoint`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorPoint {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl AnchorPoint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TopLeft => "TOP_LEFT",
            Self::TopCenter => "TOP_CENTER",
            Self::TopRight => "TOP_RIGHT",
            Self::CenterLeft => "CENTER_LEFT",
            Self::Center => "CENTER",
            Self::CenterRight => "CENTER_RIGHT",
            Self::BottomLeft => "BOTTOM_LEFT",
            Self::BottomCenter => "BOTTOM_CENTER",
            Self::BottomRight => "BOTTOM_RIGHT",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "TOP_LEFT" => Some(Self::TopLeft),
            "TOP_CENTER" => Some(Self::TopCenter),
            "TOP_RIGHT" => Some(Self::TopRight),
            "CENTER_LEFT" => Some(Self::CenterLeft),
            "CENTER" => Some(Self::Center),
            "CENTER_RIGHT" => Some(Self::CenterRight),
            "BOTTOM_LEFT" => Some(Self::BottomLeft),
            "BOTTOM_CENTER" => Some(Self::BottomCenter),
            "BOTTOM_RIGHT" => Some(Self::BottomRight),
            _ => None,
        }
    }

    pub fn to_choice(self) -> u32 {
        match self {
            Self::TopLeft => 0,
            Self::TopCenter => 1,
            Self::TopRight => 2,
            Self::CenterLeft => 3,
            Self::Center => 4,
            Self::CenterRight => 5,
            Self::BottomLeft => 6,
            Self::BottomCenter => 7,
            Self::BottomRight => 8,
        }
    }

    pub fn from_choice(idx: u32) -> Self {
        match idx {
            0 => Self::TopLeft,
            1 => Self::TopCenter,
            2 => Self::TopRight,
            3 => Self::CenterLeft,
            4 => Self::Center,
            5 => Self::CenterRight,
            6 => Self::BottomLeft,
            7 => Self::BottomCenter,
            _ => Self::BottomRight,
        }
    }

    pub fn is_left(self) -> bool {
        matches!(self, Self::TopLeft | Self::CenterLeft | Self::BottomLeft)
    }

    pub fn is_right(self) -> bool {
        matches!(self, Self::TopRight | Self::CenterRight | Self::BottomRight)
    }

    pub fn is_horizontal_center(self) -> bool {
        matches!(self, Self::TopCenter | Self::Center | Self::BottomCenter)
    }

    pub fn is_top(self) -> bool {
        matches!(self, Self::TopLeft | Self::TopCenter | Self::TopRight)
    }

    pub fn is_bottom(self) -> bool {
        matches!(
            self,
            Self::BottomLeft | Self::BottomCenter | Self::BottomRight
        )
    }

    pub fn is_vertical_center(self) -> bool {
        matches!(self, Self::CenterLeft | Self::Center | Self::CenterRight)
    }

    /// Outward horizontal direction normal (`-1` on left edge, `+1` on right edge, `0` centered).
    pub fn normal_x(self) -> i32 {
        if self.is_left() {
            -1
        } else if self.is_right() {
            1
        } else {
            0
        }
    }

    pub fn pos_x(self, area_width: i32, element_width: i32, offset_x: i32) -> i32 {
        if self.is_left() {
            offset_x
        } else if self.is_right() {
            area_width - element_width - offset_x
        } else {
            (area_width - element_width) / 2
        }
    }

    pub fn pos_y(self, area_height: i32, element_height: i32, offset_y: i32) -> i32 {
        if self.is_top() {
            offset_y
        } else if self.is_bottom() {
            area_height - element_height - offset_y
        } else {
            (area_height - element_height) / 2
        }
    }
}

/// The 16 legacy `ChatFormatting` colors accepted by `default_text_color`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextColorChoice {
    Black,
    DarkBlue,
    DarkGreen,
    DarkAqua,
    DarkRed,
    DarkPurple,
    Gold,
    Gray,
    DarkGray,
    Blue,
    Green,
    Aqua,
    Red,
    LightPurple,
    Yellow,
    White,
}

impl TextColorChoice {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Black => "BLACK",
            Self::DarkBlue => "DARK_BLUE",
            Self::DarkGreen => "DARK_GREEN",
            Self::DarkAqua => "DARK_AQUA",
            Self::DarkRed => "DARK_RED",
            Self::DarkPurple => "DARK_PURPLE",
            Self::Gold => "GOLD",
            Self::Gray => "GRAY",
            Self::DarkGray => "DARK_GRAY",
            Self::Blue => "BLUE",
            Self::Green => "GREEN",
            Self::Aqua => "AQUA",
            Self::Red => "RED",
            Self::LightPurple => "LIGHT_PURPLE",
            Self::Yellow => "YELLOW",
            Self::White => "WHITE",
        }
    }

    pub fn style_name(self) -> &'static str {
        match self {
            Self::Black => "black",
            Self::DarkBlue => "dark_blue",
            Self::DarkGreen => "dark_green",
            Self::DarkAqua => "dark_aqua",
            Self::DarkRed => "dark_red",
            Self::DarkPurple => "dark_purple",
            Self::Gold => "gold",
            Self::Gray => "gray",
            Self::DarkGray => "dark_gray",
            Self::Blue => "blue",
            Self::Green => "green",
            Self::Aqua => "aqua",
            Self::Red => "red",
            Self::LightPurple => "light_purple",
            Self::Yellow => "yellow",
            Self::White => "white",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "BLACK" => Some(Self::Black),
            "DARK_BLUE" => Some(Self::DarkBlue),
            "DARK_GREEN" => Some(Self::DarkGreen),
            "DARK_AQUA" => Some(Self::DarkAqua),
            "DARK_RED" => Some(Self::DarkRed),
            "DARK_PURPLE" => Some(Self::DarkPurple),
            "GOLD" => Some(Self::Gold),
            "GRAY" => Some(Self::Gray),
            "DARK_GRAY" => Some(Self::DarkGray),
            "BLUE" => Some(Self::Blue),
            "GREEN" => Some(Self::Green),
            "AQUA" => Some(Self::Aqua),
            "RED" => Some(Self::Red),
            "LIGHT_PURPLE" => Some(Self::LightPurple),
            "YELLOW" => Some(Self::Yellow),
            "WHITE" => Some(Self::White),
            _ => None,
        }
    }

    pub fn to_choice(self) -> u32 {
        match self {
            Self::Black => 0,
            Self::DarkBlue => 1,
            Self::DarkGreen => 2,
            Self::DarkAqua => 3,
            Self::DarkRed => 4,
            Self::DarkPurple => 5,
            Self::Gold => 6,
            Self::Gray => 7,
            Self::DarkGray => 8,
            Self::Blue => 9,
            Self::Green => 10,
            Self::Aqua => 11,
            Self::Red => 12,
            Self::LightPurple => 13,
            Self::Yellow => 14,
            Self::White => 15,
        }
    }

    pub fn from_choice(idx: u32) -> Self {
        match idx {
            0 => Self::Black,
            1 => Self::DarkBlue,
            2 => Self::DarkGreen,
            3 => Self::DarkAqua,
            4 => Self::DarkRed,
            5 => Self::DarkPurple,
            6 => Self::Gold,
            7 => Self::Gray,
            8 => Self::DarkGray,
            9 => Self::Blue,
            10 => Self::Green,
            11 => Self::Aqua,
            12 => Self::Red,
            13 => Self::LightPurple,
            14 => Self::Yellow,
            _ => Self::White,
        }
    }
}

/// Parsed entry from `hidden_items`, supporting PuzzlesLib's `ConfigDataSet` syntax:
/// exact item id (`minecraft:cobblestone`), item tag (`#minecraft:flowers`),
/// namespace wildcard (`modid:*`), and `!` negation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HiddenItemRule {
    Exact(String),
    Tag(String),
    Namespace(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HiddenItemFilter {
    pub raw: Vec<String>,
    pub include: Vec<HiddenItemRule>,
    pub exclude: Vec<HiddenItemRule>,
}

impl HiddenItemFilter {
    pub fn from_raw(entries: Vec<String>) -> Self {
        let mut include = Vec::new();
        let mut exclude = Vec::new();
        for entry in &entries {
            let trimmed = entry.trim();
            if trimmed.is_empty() {
                continue;
            }
            let (negated, rest) = match trimmed.strip_prefix('!') {
                Some(r) => (true, r.trim()),
                None => (false, trimmed),
            };
            if rest.is_empty() {
                continue;
            }
            let rule = if let Some(tag) = rest.strip_prefix('#') {
                HiddenItemRule::Tag(normalize_id(tag))
            } else if let Some(ns) = rest.strip_suffix(":*") {
                HiddenItemRule::Namespace(ns.to_ascii_lowercase())
            } else if rest == "*" {
                HiddenItemRule::Namespace("*".into())
            } else {
                HiddenItemRule::Exact(normalize_id(rest))
            };
            if negated {
                exclude.push(rule);
            } else {
                include.push(rule);
            }
        }
        Self {
            raw: entries,
            include,
            exclude,
        }
    }

    pub fn is_hidden(&self, host: &dyn ClientHost, item: ItemKey) -> bool {
        if self.include.is_empty() {
            return false;
        }
        let Some(item_id) = item.id(host) else {
            return false;
        };
        let included = self
            .include
            .iter()
            .any(|rule| rule_matches(rule, host, item, item_id));
        if !included {
            return false;
        }
        !self
            .exclude
            .iter()
            .any(|rule| rule_matches(rule, host, item, item_id))
    }

    #[cfg(test)]
    pub fn is_hidden_pure(&self, item_id: &str, tags: &[&str]) -> bool {
        if self.include.is_empty() {
            return false;
        }
        let matches = |rule: &HiddenItemRule| match rule {
            HiddenItemRule::Exact(id) => id == item_id,
            HiddenItemRule::Tag(tag) => tags.iter().any(|t| *t == tag),
            HiddenItemRule::Namespace(ns) => {
                ns == "*"
                    || item_id
                        .split_once(':')
                        .is_some_and(|(item_ns, _)| item_ns == ns)
            }
        };
        self.include.iter().any(matches) && !self.exclude.iter().any(matches)
    }
}

fn normalize_id(raw: &str) -> String {
    let lower = raw.trim().to_ascii_lowercase();
    if lower.contains(':') {
        lower
    } else {
        format!("minecraft:{lower}")
    }
}

fn rule_matches(
    rule: &HiddenItemRule,
    host: &dyn ClientHost,
    item: ItemKey,
    item_id: &str,
) -> bool {
    match rule {
        HiddenItemRule::Exact(id) => id == item_id,
        HiddenItemRule::Namespace(ns) => {
            ns == "*"
                || item_id
                    .split_once(':')
                    .is_some_and(|(item_ns, _)| item_ns == ns)
        }
        HiddenItemRule::Tag(tag) => {
            let Ok(id) = Identifier::parse(tag) else {
                return false;
            };
            let tags = host.tags();
            let Some(key) = tags.key(&id) else {
                return false;
            };
            tags.is_in_tag(item, key).unwrap_or(false)
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneralConfig {
    pub client_only: bool,
    pub include_items: bool,
    pub include_experience: bool,
    pub include_arrows: bool,
    pub experience_value: bool,
    pub disable_in_creative: bool,
    pub hidden_items: HiddenItemFilter,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            client_only: false,
            include_items: true,
            include_experience: true,
            include_arrows: true,
            experience_value: true,
            disable_in_creative: false,
            hidden_items: HiddenItemFilter::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BehaviorConfig {
    pub combine_entries: CombineEntries,
    pub display_time: i32,
    pub move_out: MovementDirection,
    pub move_time: i32,
    pub fade_out: bool,
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            combine_entries: CombineEntries::ExcludeNamed,
            display_time: 80,
            move_out: MovementDirection::Vertical,
            move_time: 20,
            fade_out: true,
        }
    }
}

impl BehaviorConfig {
    pub fn effective_move_time(&self) -> i32 {
        self.move_time.min(self.display_time)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DisplayConfig {
    pub draw_sprite: bool,
    pub default_text_color: TextColorChoice,
    pub ignore_rarity: bool,
    pub screen_corner: AnchorPoint,
    pub offset_x: i32,
    pub offset_y: i32,
    pub max_height: f64,
    pub display_scale: i32,
    pub display_amount: DisplayAmount,
    pub inventory_count: bool,
    pub display_single_count: bool,
    pub entry_background: EntryBackground,
    pub display_item_name: bool,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            draw_sprite: true,
            default_text_color: TextColorChoice::White,
            ignore_rarity: false,
            screen_corner: AnchorPoint::BottomRight,
            offset_x: 8,
            offset_y: 4,
            max_height: 0.5,
            display_scale: 4,
            display_amount: DisplayAmount::Text,
            inventory_count: false,
            display_single_count: true,
            entry_background: EntryBackground::Chat,
            display_item_name: true,
        }
    }
}

impl DisplayConfig {
    pub fn scale(&self) -> f32 {
        self.display_scale.clamp(1, 24) as f32 / 6.0
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClientConfig {
    pub general: GeneralConfig,
    pub behavior: BehaviorConfig,
    pub display: DisplayConfig,
}

impl ClientConfig {
    pub fn to_toml(&self) -> String {
        let hidden = self
            .general
            .hidden_items
            .raw
            .iter()
            .map(|s| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            r##"# Pick Up Notifier client configuration

[general]
# Force client-side inventory/XP tracking instead of server pick-up payloads.
client_only = {}
# Show item entities the player has collected in the pick-up notifications.
include_items = {}
# Show experience orbs the player has collected in the pick-up notifications.
include_experience = {}
# Show shot arrows the player has collected in the pick-up notifications.
include_arrows = {}
# Show the value of experience points collected instead of the amount of individual orbs.
experience_value = {}
# Prevent items from being added to the pick-up log when in creative mode.
disable_in_creative = {}
# Hide specific items, tags ("#minecraft:flowers"), namespaces ("modid:*"), or "!exclusions".
hidden_items = [{}]

[behavior]
# Combine entries of the same type: "ALWAYS", "NEVER", or "EXCLUDE_NAMED".
combine_entries = "{}"
# Amount of ticks each entry will be shown for. Set to 0 to keep entries until space is needed.
display_time = {}
# Direction outdated entries slide off screen: "NONE", "HORIZONTAL", "VERTICAL", or "ALL".
move_out = "{}"
# Amount of ticks it takes for an entry to move out of the screen.
move_time = {}
# Make outdated entry names slowly fade away instead of simply vanishing.
fade_out = {}

[display]
# Show a small sprite next to the name of each entry.
draw_sprite = {}
# Color for the entry name text (16 vanilla chat colors, e.g. "WHITE", "GOLD").
default_text_color = "{}"
# Ignore rarity when determining item name color.
ignore_rarity = {}
# Screen anchor: "TOP_LEFT", "TOP_CENTER", "TOP_RIGHT", "CENTER_LEFT", "CENTER", "CENTER_RIGHT", "BOTTOM_LEFT", "BOTTOM_CENTER", "BOTTOM_RIGHT".
screen_corner = "{}"
# Offset on x-axis from screen border.
offset_x = {}
# Offset on y-axis from screen border.
offset_y = {}
# Percentage of relative screen height entries are allowed to fill at max (0.0..=1.0).
max_height = {:.2}
# Scale of entries (1..=24, divided by 6).
display_scale = {}
# Where to display the amount picked up: "OFF", "SPRITE", "TEXT", or "BOTH".
display_amount = "{}"
# Add the total amount of an item in your inventory to the entry.
inventory_count = {}
# Should the picked up amount be shown when it's just a single item.
display_single_count = {}
# Background behind entries: "NONE", "CHAT", or "TOOLTIP".
entry_background = "{}"
# Add the name of the item to the entry.
display_item_name = {}
"##,
            self.general.client_only,
            self.general.include_items,
            self.general.include_experience,
            self.general.include_arrows,
            self.general.experience_value,
            self.general.disable_in_creative,
            hidden,
            self.behavior.combine_entries.as_str(),
            self.behavior.display_time,
            self.behavior.move_out.as_str(),
            self.behavior.move_time,
            self.behavior.fade_out,
            self.display.draw_sprite,
            self.display.default_text_color.as_str(),
            self.display.ignore_rarity,
            self.display.screen_corner.as_str(),
            self.display.offset_x,
            self.display.offset_y,
            self.display.max_height,
            self.display.display_scale,
            self.display.display_amount.as_str(),
            self.display.inventory_count,
            self.display.display_single_count,
            self.display.entry_background.as_str(),
            self.display.display_item_name,
        )
    }

    pub fn parse_toml(source: &str) -> Self {
        let mut cfg = Self::default();
        for (key, value) in parse_flat_toml(source) {
            match key {
                "client_only" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.general.client_only = v;
                    }
                }
                "include_items" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.general.include_items = v;
                    }
                }
                "include_experience" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.general.include_experience = v;
                    }
                }
                "include_arrows" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.general.include_arrows = v;
                    }
                }
                "experience_value" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.general.experience_value = v;
                    }
                }
                "disable_in_creative" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.general.disable_in_creative = v;
                    }
                }
                "hidden_items" => {
                    if let Some(list) = parse_string_list(value) {
                        cfg.general.hidden_items = HiddenItemFilter::from_raw(list);
                    }
                }
                "combine_entries" => {
                    if let Some(v) = CombineEntries::parse(unquote(value)) {
                        cfg.behavior.combine_entries = v;
                    }
                }
                "display_time" => {
                    if let Some(v) = parse_i32(value) {
                        cfg.behavior.display_time = v.max(0);
                    }
                }
                "move_out" => {
                    if let Some(v) = MovementDirection::parse(unquote(value)) {
                        cfg.behavior.move_out = v;
                    }
                }
                "move_time" => {
                    if let Some(v) = parse_i32(value) {
                        cfg.behavior.move_time = v.max(0);
                    }
                }
                "fade_out" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.behavior.fade_out = v;
                    }
                }
                "draw_sprite" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.display.draw_sprite = v;
                    }
                }
                "default_text_color" => {
                    if let Some(v) = TextColorChoice::parse(unquote(value)) {
                        cfg.display.default_text_color = v;
                    }
                }
                "ignore_rarity" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.display.ignore_rarity = v;
                    }
                }
                "screen_corner" => {
                    if let Some(v) = AnchorPoint::parse(unquote(value)) {
                        cfg.display.screen_corner = v;
                    }
                }
                "offset_x" => {
                    if let Some(v) = parse_i32(value) {
                        cfg.display.offset_x = v.max(0);
                    }
                }
                "offset_y" => {
                    if let Some(v) = parse_i32(value) {
                        cfg.display.offset_y = v.max(0);
                    }
                }
                "max_height" => {
                    if let Some(v) = parse_f64(value) {
                        cfg.display.max_height = v.clamp(0.0, 1.0);
                    }
                }
                "display_scale" => {
                    if let Some(v) = parse_i32(value) {
                        cfg.display.display_scale = v.clamp(1, 24);
                    }
                }
                "display_amount" => {
                    if let Some(v) = DisplayAmount::parse(unquote(value)) {
                        cfg.display.display_amount = v;
                    }
                }
                "inventory_count" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.display.inventory_count = v;
                    }
                }
                "display_single_count" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.display.display_single_count = v;
                    }
                }
                "entry_background" => {
                    if let Some(v) = EntryBackground::parse(unquote(value)) {
                        cfg.display.entry_background = v;
                    }
                }
                "display_item_name" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.display.display_item_name = v;
                    }
                }
                _ => {}
            }
        }
        cfg
    }

    pub fn write_to_host_settings(&self, host: &dyn ClientHost) {
        let set_bool =
            |k: &str, v: bool| host.set_setting(MOD_ID, k, if v { "true" } else { "false" });
        let set_u32 = |k: &str, v: u32| host.set_setting(MOD_ID, k, &v.to_string());
        let set_f64 = |k: &str, v: f64| host.set_setting(MOD_ID, k, &format!("{v}"));

        set_bool(SET_CLIENT_ONLY, self.general.client_only);
        set_bool(SET_INCLUDE_ITEMS, self.general.include_items);
        set_bool(SET_INCLUDE_EXPERIENCE, self.general.include_experience);
        set_bool(SET_INCLUDE_ARROWS, self.general.include_arrows);
        set_bool(SET_EXPERIENCE_VALUE, self.general.experience_value);
        set_bool(SET_DISABLE_IN_CREATIVE, self.general.disable_in_creative);

        set_u32(
            SET_COMBINE_ENTRIES,
            self.behavior.combine_entries.to_choice(),
        );
        set_f64(
            SET_DISPLAY_TIME,
            f64::from(self.behavior.display_time.min(600)),
        );
        set_u32(SET_MOVE_OUT, self.behavior.move_out.to_choice());
        set_f64(SET_MOVE_TIME, f64::from(self.behavior.move_time.min(200)));
        set_bool(SET_FADE_OUT, self.behavior.fade_out);

        set_bool(SET_DRAW_SPRITE, self.display.draw_sprite);
        set_u32(
            SET_DEFAULT_TEXT_COLOR,
            self.display.default_text_color.to_choice(),
        );
        set_bool(SET_IGNORE_RARITY, self.display.ignore_rarity);
        set_u32(SET_SCREEN_CORNER, self.display.screen_corner.to_choice());
        set_f64(SET_OFFSET_X, f64::from(self.display.offset_x.min(200)));
        set_f64(SET_OFFSET_Y, f64::from(self.display.offset_y.min(200)));
        set_f64(SET_MAX_HEIGHT, (self.display.max_height * 100.0).round());
        set_f64(SET_DISPLAY_SCALE, f64::from(self.display.display_scale));
        set_u32(SET_DISPLAY_AMOUNT, self.display.display_amount.to_choice());
        set_bool(SET_INVENTORY_COUNT, self.display.inventory_count);
        set_bool(SET_DISPLAY_SINGLE_COUNT, self.display.display_single_count);
        set_u32(
            SET_ENTRY_BACKGROUND,
            self.display.entry_background.to_choice(),
        );
        set_bool(SET_DISPLAY_ITEM_NAME, self.display.display_item_name);
    }

    pub fn apply_setting(&mut self, host: &dyn ClientHost, key: &str) -> bool {
        match key {
            SET_CLIENT_ONLY => {
                if let Some(v) = host.setting_bool(key) {
                    self.general.client_only = v;
                    return true;
                }
            }
            SET_INCLUDE_ITEMS => {
                if let Some(v) = host.setting_bool(key) {
                    self.general.include_items = v;
                    return true;
                }
            }
            SET_INCLUDE_EXPERIENCE => {
                if let Some(v) = host.setting_bool(key) {
                    self.general.include_experience = v;
                    return true;
                }
            }
            SET_INCLUDE_ARROWS => {
                if let Some(v) = host.setting_bool(key) {
                    self.general.include_arrows = v;
                    return true;
                }
            }
            SET_EXPERIENCE_VALUE => {
                if let Some(v) = host.setting_bool(key) {
                    self.general.experience_value = v;
                    return true;
                }
            }
            SET_DISABLE_IN_CREATIVE => {
                if let Some(v) = host.setting_bool(key) {
                    self.general.disable_in_creative = v;
                    return true;
                }
            }
            SET_COMBINE_ENTRIES => {
                if let Some(v) = host.setting_choice(key) {
                    self.behavior.combine_entries = CombineEntries::from_choice(v);
                    return true;
                }
            }
            SET_DISPLAY_TIME => {
                if let Some(v) = host.setting_f64(key) {
                    self.behavior.display_time = (v.round() as i32).max(0);
                    return true;
                }
            }
            SET_MOVE_OUT => {
                if let Some(v) = host.setting_choice(key) {
                    self.behavior.move_out = MovementDirection::from_choice(v);
                    return true;
                }
            }
            SET_MOVE_TIME => {
                if let Some(v) = host.setting_f64(key) {
                    self.behavior.move_time = (v.round() as i32).max(0);
                    return true;
                }
            }
            SET_FADE_OUT => {
                if let Some(v) = host.setting_bool(key) {
                    self.behavior.fade_out = v;
                    return true;
                }
            }
            SET_DRAW_SPRITE => {
                if let Some(v) = host.setting_bool(key) {
                    self.display.draw_sprite = v;
                    return true;
                }
            }
            SET_DEFAULT_TEXT_COLOR => {
                if let Some(v) = host.setting_choice(key) {
                    self.display.default_text_color = TextColorChoice::from_choice(v);
                    return true;
                }
            }
            SET_IGNORE_RARITY => {
                if let Some(v) = host.setting_bool(key) {
                    self.display.ignore_rarity = v;
                    return true;
                }
            }
            SET_SCREEN_CORNER => {
                if let Some(v) = host.setting_choice(key) {
                    self.display.screen_corner = AnchorPoint::from_choice(v);
                    return true;
                }
            }
            SET_OFFSET_X => {
                if let Some(v) = host.setting_f64(key) {
                    self.display.offset_x = (v.round() as i32).max(0);
                    return true;
                }
            }
            SET_OFFSET_Y => {
                if let Some(v) = host.setting_f64(key) {
                    self.display.offset_y = (v.round() as i32).max(0);
                    return true;
                }
            }
            SET_MAX_HEIGHT => {
                if let Some(v) = host.setting_f64(key) {
                    self.display.max_height = (v / 100.0).clamp(0.0, 1.0);
                    return true;
                }
            }
            SET_DISPLAY_SCALE => {
                if let Some(v) = host.setting_f64(key) {
                    self.display.display_scale = (v.round() as i32).clamp(1, 24);
                    return true;
                }
            }
            SET_DISPLAY_AMOUNT => {
                if let Some(v) = host.setting_choice(key) {
                    self.display.display_amount = DisplayAmount::from_choice(v);
                    return true;
                }
            }
            SET_INVENTORY_COUNT => {
                if let Some(v) = host.setting_bool(key) {
                    self.display.inventory_count = v;
                    return true;
                }
            }
            SET_DISPLAY_SINGLE_COUNT => {
                if let Some(v) = host.setting_bool(key) {
                    self.display.display_single_count = v;
                    return true;
                }
            }
            SET_ENTRY_BACKGROUND => {
                if let Some(v) = host.setting_choice(key) {
                    self.display.entry_background = EntryBackground::from_choice(v);
                    return true;
                }
            }
            SET_DISPLAY_ITEM_NAME => {
                if let Some(v) = host.setting_bool(key) {
                    self.display.display_item_name = v;
                    return true;
                }
            }
            _ => {}
        }
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerConfig {
    pub partial_pick_ups: bool,
    pub backpack_integration: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            partial_pick_ups: true,
            backpack_integration: true,
        }
    }
}

impl ServerConfig {
    pub fn default_toml() -> &'static str {
        r#"# Pick Up Notifier server configuration
# Collect partial pick-up entries (when there isn't enough room in your inventory) in the log.
partial_pick_ups = true
# Show entries for items picked up that don't go to the player's inventory (reserved for backpack integration).
backpack_integration = true
"#
    }

    pub fn parse_toml(source: &str) -> Self {
        let mut cfg = Self::default();
        for (key, value) in parse_flat_toml(source) {
            match key {
                "partial_pick_ups" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.partial_pick_ups = v;
                    }
                }
                "backpack_integration" => {
                    if let Some(v) = parse_bool(value) {
                        cfg.backpack_integration = v;
                    }
                }
                _ => {}
            }
        }
        cfg
    }
}

pub fn register_mod_configs(configs: &mut Vec<Config>) {
    let client_toml = ClientConfig::default().to_toml();
    configs.push(Config::new(MOD_ID, ConfigKind::Client, &client_toml).file(CLIENT_CONFIG_FILE));
    configs.push(
        Config::new(MOD_ID, ConfigKind::Server, ServerConfig::default_toml())
            .file(SERVER_CONFIG_FILE),
    );
}

pub fn register_settings(r: &mut dyn Registrar) {
    let specs = [
        SettingSpec::toggle(
            SET_INCLUDE_ITEMS,
            "pickupnotifier.setting.include_items",
            true,
        )
        .tooltip("pickupnotifier.setting.include_items.tooltip"),
        SettingSpec::toggle(
            SET_INCLUDE_EXPERIENCE,
            "pickupnotifier.setting.include_experience",
            true,
        )
        .tooltip("pickupnotifier.setting.include_experience.tooltip"),
        SettingSpec::toggle(
            SET_INCLUDE_ARROWS,
            "pickupnotifier.setting.include_arrows",
            true,
        )
        .tooltip("pickupnotifier.setting.include_arrows.tooltip"),
        SettingSpec::toggle(
            SET_EXPERIENCE_VALUE,
            "pickupnotifier.setting.experience_value",
            true,
        )
        .tooltip("pickupnotifier.setting.experience_value.tooltip"),
        SettingSpec::toggle(
            SET_DISABLE_IN_CREATIVE,
            "pickupnotifier.setting.disable_in_creative",
            false,
        )
        .tooltip("pickupnotifier.setting.disable_in_creative.tooltip"),
        SettingSpec::toggle(SET_CLIENT_ONLY, "pickupnotifier.setting.client_only", false)
            .tooltip("pickupnotifier.setting.client_only.tooltip"),
        SettingSpec::choice(
            SET_COMBINE_ENTRIES,
            "pickupnotifier.setting.combine_entries",
            [
                "pickupnotifier.setting.combine_entries.always",
                "pickupnotifier.setting.combine_entries.never",
                "pickupnotifier.setting.combine_entries.exclude_named",
            ],
            2,
        )
        .tooltip("pickupnotifier.setting.combine_entries.tooltip"),
        SettingSpec::range(
            SET_DISPLAY_TIME,
            "pickupnotifier.setting.display_time",
            0.0,
            600.0,
            5.0,
            80.0,
        )
        .tooltip("pickupnotifier.setting.display_time.tooltip"),
        SettingSpec::choice(
            SET_MOVE_OUT,
            "pickupnotifier.setting.move_out",
            [
                "pickupnotifier.setting.move_out.none",
                "pickupnotifier.setting.move_out.horizontal",
                "pickupnotifier.setting.move_out.vertical",
                "pickupnotifier.setting.move_out.all",
            ],
            2,
        )
        .tooltip("pickupnotifier.setting.move_out.tooltip"),
        SettingSpec::range(
            SET_MOVE_TIME,
            "pickupnotifier.setting.move_time",
            0.0,
            200.0,
            5.0,
            20.0,
        )
        .tooltip("pickupnotifier.setting.move_time.tooltip"),
        SettingSpec::toggle(SET_FADE_OUT, "pickupnotifier.setting.fade_out", true)
            .tooltip("pickupnotifier.setting.fade_out.tooltip"),
        SettingSpec::toggle(SET_DRAW_SPRITE, "pickupnotifier.setting.draw_sprite", true)
            .tooltip("pickupnotifier.setting.draw_sprite.tooltip"),
        SettingSpec::choice(
            SET_DEFAULT_TEXT_COLOR,
            "pickupnotifier.setting.default_text_color",
            [
                "pickupnotifier.setting.color.black",
                "pickupnotifier.setting.color.dark_blue",
                "pickupnotifier.setting.color.dark_green",
                "pickupnotifier.setting.color.dark_aqua",
                "pickupnotifier.setting.color.dark_red",
                "pickupnotifier.setting.color.dark_purple",
                "pickupnotifier.setting.color.gold",
                "pickupnotifier.setting.color.gray",
                "pickupnotifier.setting.color.dark_gray",
                "pickupnotifier.setting.color.blue",
                "pickupnotifier.setting.color.green",
                "pickupnotifier.setting.color.aqua",
                "pickupnotifier.setting.color.red",
                "pickupnotifier.setting.color.light_purple",
                "pickupnotifier.setting.color.yellow",
                "pickupnotifier.setting.color.white",
            ],
            15,
        )
        .tooltip("pickupnotifier.setting.default_text_color.tooltip"),
        SettingSpec::toggle(
            SET_IGNORE_RARITY,
            "pickupnotifier.setting.ignore_rarity",
            false,
        )
        .tooltip("pickupnotifier.setting.ignore_rarity.tooltip"),
        SettingSpec::choice(
            SET_SCREEN_CORNER,
            "pickupnotifier.setting.screen_corner",
            [
                "pickupnotifier.setting.corner.top_left",
                "pickupnotifier.setting.corner.top_center",
                "pickupnotifier.setting.corner.top_right",
                "pickupnotifier.setting.corner.center_left",
                "pickupnotifier.setting.corner.center",
                "pickupnotifier.setting.corner.center_right",
                "pickupnotifier.setting.corner.bottom_left",
                "pickupnotifier.setting.corner.bottom_center",
                "pickupnotifier.setting.corner.bottom_right",
            ],
            8,
        )
        .tooltip("pickupnotifier.setting.screen_corner.tooltip"),
        SettingSpec::range(
            SET_OFFSET_X,
            "pickupnotifier.setting.offset_x",
            0.0,
            200.0,
            1.0,
            8.0,
        )
        .tooltip("pickupnotifier.setting.offset_x.tooltip"),
        SettingSpec::range(
            SET_OFFSET_Y,
            "pickupnotifier.setting.offset_y",
            0.0,
            200.0,
            1.0,
            4.0,
        )
        .tooltip("pickupnotifier.setting.offset_y.tooltip"),
        SettingSpec::range(
            SET_MAX_HEIGHT,
            "pickupnotifier.setting.max_height",
            0.0,
            100.0,
            5.0,
            50.0,
        )
        .tooltip("pickupnotifier.setting.max_height.tooltip"),
        SettingSpec::range(
            SET_DISPLAY_SCALE,
            "pickupnotifier.setting.display_scale",
            1.0,
            24.0,
            1.0,
            4.0,
        )
        .tooltip("pickupnotifier.setting.display_scale.tooltip"),
        SettingSpec::choice(
            SET_DISPLAY_AMOUNT,
            "pickupnotifier.setting.display_amount",
            [
                "pickupnotifier.setting.display_amount.off",
                "pickupnotifier.setting.display_amount.sprite",
                "pickupnotifier.setting.display_amount.text",
                "pickupnotifier.setting.display_amount.both",
            ],
            2,
        )
        .tooltip("pickupnotifier.setting.display_amount.tooltip"),
        SettingSpec::toggle(
            SET_INVENTORY_COUNT,
            "pickupnotifier.setting.inventory_count",
            false,
        )
        .tooltip("pickupnotifier.setting.inventory_count.tooltip"),
        SettingSpec::toggle(
            SET_DISPLAY_SINGLE_COUNT,
            "pickupnotifier.setting.display_single_count",
            true,
        )
        .tooltip("pickupnotifier.setting.display_single_count.tooltip"),
        SettingSpec::choice(
            SET_ENTRY_BACKGROUND,
            "pickupnotifier.setting.entry_background",
            [
                "pickupnotifier.setting.entry_background.none",
                "pickupnotifier.setting.entry_background.chat",
                "pickupnotifier.setting.entry_background.tooltip_bg",
            ],
            1,
        )
        .tooltip("pickupnotifier.setting.entry_background.tooltip"),
        SettingSpec::toggle(
            SET_DISPLAY_ITEM_NAME,
            "pickupnotifier.setting.display_item_name",
            true,
        )
        .tooltip("pickupnotifier.setting.display_item_name.tooltip"),
    ];

    for spec in specs {
        let _ = r.register_setting(spec);
    }
}

fn strip_comment(line: &str) -> &str {
    let mut in_quotes = false;
    let mut escaped = false;
    for (idx, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_quotes => escaped = true,
            '"' => in_quotes = !in_quotes,
            '#' if !in_quotes => return &line[..idx],
            _ => {}
        }
    }
    line
}

fn parse_flat_toml(source: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    for raw_line in source.lines() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() || line.starts_with('[') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let key = k.trim();
            let val = v.trim();
            if !key.is_empty() && !val.is_empty() {
                out.push((key, val));
            }
        }
    }
    out
}

fn unquote(raw: &str) -> &str {
    let t = raw.trim();
    t.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| t.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(t)
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn parse_i32(raw: &str) -> Option<i32> {
    raw.trim().parse::<i32>().ok()
}

fn parse_f64(raw: &str) -> Option<f64> {
    raw.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

fn parse_string_list(raw: &str) -> Option<Vec<String>> {
    let inner = raw.trim().strip_prefix('[')?.strip_suffix(']')?.trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }
    let mut items = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut escaped = false;
    for ch in inner.chars() {
        if escaped {
            cur.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_quotes => escaped = true,
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                let s = cur.trim();
                if !s.is_empty() {
                    items.push(s.to_owned());
                }
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    let s = cur.trim();
    if !s.is_empty() {
        items.push(s.to_owned());
    }
    Some(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_config_roundtrip() {
        let mut cfg = ClientConfig::default();
        cfg.general.client_only = true;
        cfg.general.include_arrows = false;
        cfg.general.disable_in_creative = true;
        cfg.general.hidden_items = HiddenItemFilter::from_raw(vec![
            "minecraft:cobblestone".into(),
            "#minecraft:flowers".into(),
            "!minecraft:dandelion".into(),
        ]);
        cfg.behavior.combine_entries = CombineEntries::Always;
        cfg.behavior.display_time = 120;
        cfg.behavior.move_out = MovementDirection::All;
        cfg.behavior.move_time = 30;
        cfg.behavior.fade_out = false;
        cfg.display.default_text_color = TextColorChoice::Gold;
        cfg.display.screen_corner = AnchorPoint::TopLeft;
        cfg.display.offset_x = 12;
        cfg.display.offset_y = 6;
        cfg.display.max_height = 0.75;
        cfg.display.display_scale = 6;
        cfg.display.display_amount = DisplayAmount::Both;
        cfg.display.inventory_count = true;
        cfg.display.entry_background = EntryBackground::Tooltip;

        let serialized = cfg.to_toml();
        let parsed = ClientConfig::parse_toml(&serialized);
        assert_eq!(parsed, cfg);
    }

    #[test]
    fn server_config_parses_values() {
        let cfg =
            ServerConfig::parse_toml("partial_pick_ups = false\nbackpack_integration = false\n");
        assert!(!cfg.partial_pick_ups);
        assert!(!cfg.backpack_integration);
    }

    #[test]
    fn hidden_item_filter_supports_exact_tags_wildcards_and_exclusions() {
        let filter = HiddenItemFilter::from_raw(vec![
            "cobblestone".into(),
            "#minecraft:flowers".into(),
            "somemod:*".into(),
            "!minecraft:dandelion".into(),
            "!somemod:special".into(),
        ]);
        assert!(filter.is_hidden_pure("minecraft:cobblestone", &[]));
        assert!(filter.is_hidden_pure("minecraft:poppy", &["minecraft:flowers"]));
        assert!(!filter.is_hidden_pure("minecraft:dandelion", &["minecraft:flowers"]));
        assert!(filter.is_hidden_pure("somemod:junk", &[]));
        assert!(!filter.is_hidden_pure("somemod:special", &[]));
        assert!(!filter.is_hidden_pure("minecraft:diamond", &[]));
    }

    #[test]
    fn move_time_is_clamped_to_display_time() {
        let b = BehaviorConfig {
            display_time: 10,
            move_time: 25,
            ..Default::default()
        };
        assert_eq!(b.effective_move_time(), 10);
    }

    #[test]
    fn movement_direction_respects_center_anchors() {
        assert!(!MovementDirection::Horizontal.moves_horizontal(AnchorPoint::TopCenter));
        assert!(MovementDirection::Horizontal.moves_horizontal(AnchorPoint::TopLeft));
        assert!(!MovementDirection::Vertical.moves_vertical(AnchorPoint::CenterLeft));
        assert!(MovementDirection::Vertical.moves_vertical(AnchorPoint::BottomRight));
        assert!(!MovementDirection::All.moves_horizontal(AnchorPoint::Center));
        assert!(!MovementDirection::All.moves_vertical(AnchorPoint::Center));
        assert!(MovementDirection::All.moves_horizontal(AnchorPoint::TopCenter));
        assert!(MovementDirection::All.moves_vertical(AnchorPoint::CenterRight));
    }

    #[test]
    fn client_config_clamps_out_of_range_values() {
        let parsed = ClientConfig::parse_toml(
            "display_time = -5\nmove_time = -2\noffset_x = -10\noffset_y = -4\nmax_height = 2.5\ndisplay_scale = 99\n",
        );
        assert_eq!(parsed.behavior.display_time, 0);
        assert_eq!(parsed.behavior.move_time, 0);
        assert_eq!(parsed.display.offset_x, 0);
        assert_eq!(parsed.display.offset_y, 0);
        assert_eq!(parsed.display.max_height, 1.0);
        assert_eq!(parsed.display.display_scale, 24);
    }
}
