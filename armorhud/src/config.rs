//! Armor HUD settings: the option enums, their Options > Mods registration and the read of
//! a snapshot from the client host.

use mod_api::{ClientHost, Registrar, SettingKey, SettingSpec, SettingValue};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

pub const MOD_ID: &str = "armorhud";

pub const SET_ENABLED: &str = "enabled";
pub const SET_ANCHOR: &str = "anchor";
pub const SET_SIDE: &str = "side";
pub const SET_OFFSET_X: &str = "offset_x";
pub const SET_OFFSET_Y: &str = "offset_y";
pub const SET_STYLE: &str = "style";
pub const SET_ORIENTATION: &str = "orientation";
pub const SET_WIDGET_SHOWN: &str = "widget_shown";
pub const SET_OFFHAND_BEHAVIOR: &str = "offhand_slot_behavior";
pub const SET_DURABILITY_DISPLAY: &str = "durability_display";
pub const SET_OFFHAND_DURABILITY: &str = "offhand_durability";
pub const SET_MAINHAND_DURABILITY: &str = "mainhand_durability";
pub const SET_REVERSED: &str = "reversed";
pub const SET_ICONS_SHOWN: &str = "icons_shown";
pub const SET_WARNING_SHOWN: &str = "warning_shown";
pub const SET_PLAY_BREAK_SOUND: &str = "play_break_sound";
pub const SET_MIN_DURABILITY_VALUE: &str = "min_durability_value";
pub const SET_MIN_DURABILITY_PERCENT: &str = "min_durability_percentage";
pub const SET_WARNING_BOB: &str = "warning_bob_intensity";

/// Largest offset the settings screen offers, in GUI pixels either way.
pub const OFFSET_LIMIT: f64 = 1000.0;
/// Largest warning bob the settings screen offers.
pub const BOB_LIMIT: f64 = 10.0;
/// Largest durability value threshold the settings screen offers.
pub const MIN_DURABILITY_LIMIT: f64 = 1000.0;

macro_rules! choice_enum {
    ($(#[$m:meta])* $name:ident { $($variant:ident),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name { $($variant),+ }
        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
            /// The variant of option `index`, else `fallback`.
            pub fn from_choice(index: u32, fallback: Self) -> Self {
                Self::ALL.get(index as usize).copied().unwrap_or(fallback)
            }
            pub fn to_choice(self) -> u32 {
                Self::ALL.iter().position(|v| *v == self).unwrap_or(0) as u32
            }
        }
    };
}

choice_enum!(
    /// Where the widget is attached.
    Anchor { TopCenter, Top, VerticalCenter, Bottom, Hotbar }
);
choice_enum!(
    /// The screen side the widget prefers.
    Side { Left, Right }
);
choice_enum!(
    /// How the slot background is drawn.
    Style { Hotbar, RoundedCorners, Rounded, None }
);
choice_enum!(
    /// The direction the slots run in.
    Orientation { Horizontal, Vertical }
);
choice_enum!(
    /// When the slots are shown.
    WidgetShown { Always, IfAnyPresent, NotEmpty, DamagedPieces }
);
choice_enum!(
    /// How the widget reacts to the offhand slot and the hotbar attack indicator.
    OffhandBehavior { AlwaysIgnore, Adhere, AlwaysLeaveSpace }
);
choice_enum!(
    /// How a worn piece's durability is shown.
    DurabilityDisplay { Bar, Numeric, Percentage }
);

impl Anchor {
    pub fn is_top(self) -> bool {
        matches!(self, Self::Top | Self::TopCenter)
    }
}

impl Side {
    pub fn opposite(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

impl WidgetShown {
    pub fn draws_empty_slots(self) -> bool {
        matches!(self, Self::Always | Self::IfAnyPresent)
    }
}

/// One read of every setting.
#[derive(Clone, Debug, PartialEq)]
pub struct ArmorHudConfig {
    pub enabled: bool,
    pub anchor: Anchor,
    pub side: Side,
    pub offset_x: i32,
    pub offset_y: i32,
    pub style: Style,
    pub orientation: Orientation,
    pub widget_shown: WidgetShown,
    pub offhand_behavior: OffhandBehavior,
    pub durability_display: DurabilityDisplay,
    pub offhand_durability: bool,
    pub mainhand_durability: bool,
    pub reversed: bool,
    pub icons_shown: bool,
    pub warning_shown: bool,
    pub play_break_sound: bool,
    pub min_durability_value: i32,
    pub min_durability_percentage: f64,
    pub warning_bob: i32,
}

impl Default for ArmorHudConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            anchor: Anchor::Hotbar,
            side: Side::Left,
            offset_x: 0,
            offset_y: 0,
            style: Style::Hotbar,
            orientation: Orientation::Horizontal,
            widget_shown: WidgetShown::NotEmpty,
            offhand_behavior: OffhandBehavior::Adhere,
            durability_display: DurabilityDisplay::Bar,
            offhand_durability: true,
            mainhand_durability: false,
            reversed: false,
            icons_shown: true,
            warning_shown: true,
            play_break_sound: true,
            min_durability_value: 20,
            min_durability_percentage: 0.1,
            warning_bob: 3,
        }
    }
}

/// A whole number from a range setting, clamped to `limit` either way.
fn whole(value: f64, limit: f64) -> i32 {
    if value.is_finite() {
        value.round().clamp(-limit, limit) as i32
    } else {
        0
    }
}

impl ArmorHudConfig {
    /// The player's settings, the defaults for any the host does not answer.
    pub fn read(host: &dyn ClientHost) -> Self {
        let d = Self::default();
        let flag = |name: &str, default: bool| {
            key(name)
                .and_then(|key| host.setting_bool(key).ok())
                .unwrap_or(default)
        };
        let number = |name: &str, default: f64| {
            key(name)
                .and_then(|key| host.setting_f64(key).ok())
                .unwrap_or(default)
        };
        let index = |name: &str, default: u32| {
            key(name)
                .and_then(|key| host.setting_choice(key).ok())
                .unwrap_or(default)
        };
        Self {
            enabled: flag(SET_ENABLED, d.enabled),
            anchor: Anchor::from_choice(index(SET_ANCHOR, d.anchor.to_choice()), d.anchor),
            side: Side::from_choice(index(SET_SIDE, d.side.to_choice()), d.side),
            offset_x: whole(number(SET_OFFSET_X, 0.0), OFFSET_LIMIT),
            offset_y: whole(number(SET_OFFSET_Y, 0.0), OFFSET_LIMIT),
            style: Style::from_choice(index(SET_STYLE, d.style.to_choice()), d.style),
            orientation: Orientation::from_choice(
                index(SET_ORIENTATION, d.orientation.to_choice()),
                d.orientation,
            ),
            widget_shown: WidgetShown::from_choice(
                index(SET_WIDGET_SHOWN, d.widget_shown.to_choice()),
                d.widget_shown,
            ),
            offhand_behavior: OffhandBehavior::from_choice(
                index(SET_OFFHAND_BEHAVIOR, d.offhand_behavior.to_choice()),
                d.offhand_behavior,
            ),
            durability_display: DurabilityDisplay::from_choice(
                index(SET_DURABILITY_DISPLAY, d.durability_display.to_choice()),
                d.durability_display,
            ),
            offhand_durability: flag(SET_OFFHAND_DURABILITY, d.offhand_durability),
            mainhand_durability: flag(SET_MAINHAND_DURABILITY, d.mainhand_durability),
            reversed: flag(SET_REVERSED, d.reversed),
            icons_shown: flag(SET_ICONS_SHOWN, d.icons_shown),
            warning_shown: flag(SET_WARNING_SHOWN, d.warning_shown),
            play_break_sound: flag(SET_PLAY_BREAK_SOUND, d.play_break_sound),
            min_durability_value: whole(
                number(SET_MIN_DURABILITY_VALUE, f64::from(d.min_durability_value)),
                MIN_DURABILITY_LIMIT,
            )
            .max(0),
            min_durability_percentage: number(
                SET_MIN_DURABILITY_PERCENT,
                d.min_durability_percentage,
            )
            .clamp(0.0, 1.0),
            warning_bob: whole(number(SET_WARNING_BOB, f64::from(d.warning_bob)), BOB_LIMIT).max(0),
        }
    }
}

/// The keys the host minted for this mod's settings at registration.
static KEYS: LazyLock<Mutex<HashMap<&'static str, SettingKey>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn keys() -> MutexGuard<'static, HashMap<&'static str, SettingKey>> {
    KEYS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The registered key of setting `name`, if the host accepted it.
pub fn key(name: &str) -> Option<SettingKey> {
    keys().get(name).copied()
}

/// Write setting `name` through the host, which validates it against the registered bounds.
pub fn write(host: &mut dyn ClientHost, name: &str, value: SettingValue) {
    if let Some(key) = key(name) {
        let _ = host.set_setting(key, value);
    }
}

/// The tab of the settings screen a setting sits on (the original's Style, Position and
/// Warning tabs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Style,
    Position,
    Warning,
}

impl Tab {
    pub const ALL: [Self; 3] = [Self::Style, Self::Position, Self::Warning];

    /// The translation key of the tab's title.
    pub fn label(self) -> &'static str {
        match self {
            Self::Style => "armorhud.config.style",
            Self::Position => "armorhud.config.position",
            Self::Warning => "armorhud.config.warning",
        }
    }
}

/// What a setting holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Toggle {
        default: bool,
    },
    Choice {
        /// Translation keys of the options, in index order.
        options: &'static [&'static str],
        default: u32,
    },
    Range {
        min: f64,
        max: f64,
        step: f64,
        default: f64,
    },
}

/// One setting, shared by the Options > Mods page and the settings screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Row {
    pub name: &'static str,
    /// Translation key of the label.
    pub label: &'static str,
    /// Translation key of the tooltip.
    pub tooltip: Option<&'static str>,
    pub tab: Tab,
    pub kind: Kind,
}

impl Row {
    fn spec(&self) -> SettingSpec {
        let spec = match self.kind {
            Kind::Toggle { default } => SettingSpec::toggle(self.name, self.label, default),
            Kind::Choice { options, default } => SettingSpec::choice(
                self.name,
                self.label,
                options.iter().copied(),
                default as usize,
            ),
            Kind::Range {
                min,
                max,
                step,
                default,
            } => SettingSpec::range(self.name, self.label, min, max, step, default),
        };
        match self.tooltip {
            Some(tooltip) => spec.tooltip(tooltip),
            None => spec,
        }
    }
}

const STYLE_OPTIONS: &[&str] = &[
    "armorhud.option.hotbar",
    "armorhud.option.roundedCorners",
    "armorhud.option.rounded",
    "armorhud.option.none",
];
const ORIENTATION_OPTIONS: &[&str] = &["armorhud.option.horizontal", "armorhud.option.vertical"];
const WIDGET_SHOWN_OPTIONS: &[&str] = &[
    "armorhud.option.always",
    "armorhud.option.ifAnyPresent",
    "armorhud.option.notEmpty",
    "armorhud.option.damagedPieces",
];
const DURABILITY_OPTIONS: &[&str] = &[
    "armorhud.option.bar",
    "armorhud.option.numeric",
    "armorhud.option.percentage",
];
const ANCHOR_OPTIONS: &[&str] = &[
    "armorhud.option.topCenter",
    "armorhud.option.top",
    "armorhud.option.verticalCenter",
    "armorhud.option.bottom",
    "armorhud.option.hotbar",
];
const SIDE_OPTIONS: &[&str] = &["options.mainHand.left", "options.mainHand.right"];
const OFFHAND_OPTIONS: &[&str] = &[
    "armorhud.option.alwaysIgnore",
    "armorhud.option.adhere",
    "armorhud.option.alwaysLeaveSpace",
];

/// Every setting in the original screen's order (style, position, warning).
pub fn rows() -> Vec<Row> {
    let d = ArmorHudConfig::default();
    let toggle = |name, label, tooltip, tab, default| Row {
        name,
        label,
        tooltip,
        tab,
        kind: Kind::Toggle { default },
    };
    let choice = |name, label, tooltip, tab, options, default: u32| Row {
        name,
        label,
        tooltip,
        tab,
        kind: Kind::Choice { options, default },
    };
    let range = |name, label, tooltip, tab, min, max, step, default| Row {
        name,
        label,
        tooltip,
        tab,
        kind: Kind::Range {
            min,
            max,
            step,
            default,
        },
    };
    vec![
        toggle(
            SET_ENABLED,
            "armorhud.option.enabled",
            Some("armorhud.tooltip.enabled"),
            Tab::Style,
            d.enabled,
        ),
        choice(
            SET_STYLE,
            "armorhud.option.style",
            Some("armorhud.tooltip.style"),
            Tab::Style,
            STYLE_OPTIONS,
            d.style.to_choice(),
        ),
        choice(
            SET_ORIENTATION,
            "armorhud.option.orientation",
            None,
            Tab::Style,
            ORIENTATION_OPTIONS,
            d.orientation.to_choice(),
        ),
        choice(
            SET_WIDGET_SHOWN,
            "armorhud.option.widgetShown",
            Some("armorhud.tooltip.widgetShown"),
            Tab::Style,
            WIDGET_SHOWN_OPTIONS,
            d.widget_shown.to_choice(),
        ),
        toggle(
            SET_REVERSED,
            "armorhud.option.reversed",
            None,
            Tab::Style,
            d.reversed,
        ),
        toggle(
            SET_ICONS_SHOWN,
            "armorhud.option.showIcons",
            Some("armorhud.tooltip.showIcons"),
            Tab::Style,
            d.icons_shown,
        ),
        choice(
            SET_DURABILITY_DISPLAY,
            "armorhud.option.durabilityDisplay",
            None,
            Tab::Style,
            DURABILITY_OPTIONS,
            d.durability_display.to_choice(),
        ),
        toggle(
            SET_OFFHAND_DURABILITY,
            "armorhud.option.offhandDura",
            None,
            Tab::Style,
            d.offhand_durability,
        ),
        toggle(
            SET_MAINHAND_DURABILITY,
            "armorhud.option.mainHandDura",
            Some("armorhud.tooltip.mainHandDura"),
            Tab::Style,
            d.mainhand_durability,
        ),
        choice(
            SET_ANCHOR,
            "armorhud.option.anchor",
            Some("armorhud.tooltip.anchor"),
            Tab::Position,
            ANCHOR_OPTIONS,
            d.anchor.to_choice(),
        ),
        choice(
            SET_SIDE,
            "armorhud.option.side",
            Some("armorhud.tooltip.side"),
            Tab::Position,
            SIDE_OPTIONS,
            d.side.to_choice(),
        ),
        range(
            SET_OFFSET_X,
            "armorhud.option.offsetX",
            Some("armorhud.tooltip.offsetX"),
            Tab::Position,
            -OFFSET_LIMIT,
            OFFSET_LIMIT,
            1.0,
            0.0,
        ),
        range(
            SET_OFFSET_Y,
            "armorhud.option.offsetY",
            Some("armorhud.tooltip.offsetY"),
            Tab::Position,
            -OFFSET_LIMIT,
            OFFSET_LIMIT,
            1.0,
            0.0,
        ),
        choice(
            SET_OFFHAND_BEHAVIOR,
            "armorhud.option.offhandSlotBehavior",
            Some("armorhud.tooltip.offhandSlotBehavior"),
            Tab::Position,
            OFFHAND_OPTIONS,
            d.offhand_behavior.to_choice(),
        ),
        toggle(
            SET_WARNING_SHOWN,
            "armorhud.option.showWarning",
            Some("armorhud.tooltip.showWarning"),
            Tab::Warning,
            d.warning_shown,
        ),
        range(
            SET_MIN_DURABILITY_VALUE,
            "armorhud.option.minDuraValue",
            Some("armorhud.tooltip.minDuraValue"),
            Tab::Warning,
            0.0,
            MIN_DURABILITY_LIMIT,
            1.0,
            f64::from(d.min_durability_value),
        ),
        range(
            SET_MIN_DURABILITY_PERCENT,
            "armorhud.option.minDuraPercent",
            Some("armorhud.tooltip.minDuraPercent"),
            Tab::Warning,
            0.0,
            1.0,
            0.01,
            d.min_durability_percentage,
        ),
        range(
            SET_WARNING_BOB,
            "armorhud.option.iconBobIntensity",
            Some("armorhud.tooltip.iconBobIntensity"),
            Tab::Warning,
            0.0,
            BOB_LIMIT,
            1.0,
            f64::from(d.warning_bob),
        ),
        toggle(
            SET_PLAY_BREAK_SOUND,
            "armorhud.option.playBreakSound",
            Some("armorhud.tooltip.playBreakSound"),
            Tab::Warning,
            d.play_break_sound,
        ),
    ]
}

/// Register every setting on Options > Mods > Armor HUD, in the original screen's order
/// (style, position, warning), and keep the keys the host minted for them.
pub fn register_settings(r: &mut dyn Registrar) {
    let mut minted = HashMap::new();
    for row in rows() {
        if let Ok(key) = r.register_setting(row.spec()) {
            minted.insert(row.name, key);
        }
    }
    *keys() = minted;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_round_trip_and_unknown_indices_fall_back() {
        for (i, a) in Anchor::ALL.iter().enumerate() {
            assert_eq!(a.to_choice(), i as u32);
            assert_eq!(Anchor::from_choice(i as u32, Anchor::Top), *a);
        }
        assert_eq!(Anchor::from_choice(99, Anchor::Bottom), Anchor::Bottom);
        assert_eq!(Style::from_choice(u32::MAX, Style::None), Style::None);
    }

    #[test]
    fn whole_numbers_are_clamped_and_never_panic() {
        assert_eq!(whole(f64::NAN, 10.0), 0);
        assert_eq!(whole(f64::INFINITY, 10.0), 0);
        assert_eq!(whole(1e300, 10.0), 10);
        assert_eq!(whole(-1e300, 10.0), -10);
        assert_eq!(whole(2.5, 10.0), 3);
    }

    #[test]
    fn defaults_are_the_originals() {
        let d = ArmorHudConfig::default();
        assert!(d.enabled && d.offhand_durability && !d.mainhand_durability && !d.reversed);
        assert_eq!(d.min_durability_value, 20);
        assert_eq!(d.warning_bob, 3);
        assert_eq!(d.anchor, Anchor::Hotbar);
    }

    #[test]
    fn rows_have_unique_keys_and_valid_defaults() {
        let rows = rows();
        for (i, row) in rows.iter().enumerate() {
            assert!(rows[i + 1..].iter().all(|other| other.name != row.name));
            match row.kind {
                Kind::Choice { options, default } => assert!((default as usize) < options.len()),
                Kind::Range {
                    min,
                    max,
                    step,
                    default,
                } => assert!(min < max && step > 0.0 && (min..=max).contains(&default)),
                Kind::Toggle { .. } => {}
            }
        }
        for tab in Tab::ALL {
            assert!(rows.iter().any(|row| row.tab == tab));
        }
    }

    #[test]
    fn choice_rows_match_the_enums() {
        let options = |name: &str| match rows().iter().find(|row| row.name == name).map(|r| r.kind)
        {
            Some(Kind::Choice { options, .. }) => options.len(),
            _ => 0,
        };
        assert_eq!(options(SET_ANCHOR), Anchor::ALL.len());
        assert_eq!(options(SET_SIDE), Side::ALL.len());
        assert_eq!(options(SET_STYLE), Style::ALL.len());
        assert_eq!(options(SET_ORIENTATION), Orientation::ALL.len());
        assert_eq!(options(SET_WIDGET_SHOWN), WidgetShown::ALL.len());
        assert_eq!(options(SET_OFFHAND_BEHAVIOR), OffhandBehavior::ALL.len());
        assert_eq!(
            options(SET_DURABILITY_DISPLAY),
            DurabilityDisplay::ALL.len()
        );
    }

    #[test]
    fn option_label_counts_match_the_enums() {
        assert_eq!(Anchor::ALL.len(), 5);
        assert_eq!(Style::ALL.len(), 4);
        assert_eq!(WidgetShown::ALL.len(), 4);
        assert_eq!(OffhandBehavior::ALL.len(), 3);
        assert_eq!(DurabilityDisplay::ALL.len(), 3);
    }
}
