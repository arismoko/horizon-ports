//! Armor HUD settings: the option enums, their Options > Mods registration and the read of
//! a snapshot from the client host.

use mod_api::{ClientHost, Registrar, SettingSpec};

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
        let flag = |key: &str, default: bool| host.setting_bool(key).unwrap_or(default);
        let number = |key: &str, default: f64| host.setting_f64(key).unwrap_or(default);
        Self {
            enabled: flag(SET_ENABLED, d.enabled),
            anchor: Anchor::from_choice(
                host.setting_choice(SET_ANCHOR)
                    .unwrap_or(d.anchor.to_choice()),
                d.anchor,
            ),
            side: Side::from_choice(
                host.setting_choice(SET_SIDE).unwrap_or(d.side.to_choice()),
                d.side,
            ),
            offset_x: whole(number(SET_OFFSET_X, 0.0), OFFSET_LIMIT),
            offset_y: whole(number(SET_OFFSET_Y, 0.0), OFFSET_LIMIT),
            style: Style::from_choice(
                host.setting_choice(SET_STYLE)
                    .unwrap_or(d.style.to_choice()),
                d.style,
            ),
            orientation: Orientation::from_choice(
                host.setting_choice(SET_ORIENTATION)
                    .unwrap_or(d.orientation.to_choice()),
                d.orientation,
            ),
            widget_shown: WidgetShown::from_choice(
                host.setting_choice(SET_WIDGET_SHOWN)
                    .unwrap_or(d.widget_shown.to_choice()),
                d.widget_shown,
            ),
            offhand_behavior: OffhandBehavior::from_choice(
                host.setting_choice(SET_OFFHAND_BEHAVIOR)
                    .unwrap_or(d.offhand_behavior.to_choice()),
                d.offhand_behavior,
            ),
            durability_display: DurabilityDisplay::from_choice(
                host.setting_choice(SET_DURABILITY_DISPLAY)
                    .unwrap_or(d.durability_display.to_choice()),
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

fn choice<S: Into<String>>(
    key: &str,
    label: &str,
    options: impl IntoIterator<Item = S>,
    default: u32,
) -> SettingSpec {
    SettingSpec::choice(key, label, options, default as usize)
}

/// Register every setting on Options > Mods > Armor HUD, in the original screen's order
/// (style, position, warning, then the rest).
pub fn register_settings(r: &mut dyn Registrar) {
    let d = ArmorHudConfig::default();
    let specs = [
        SettingSpec::toggle(SET_ENABLED, "armorhud.option.enabled", d.enabled)
            .tooltip("armorhud.tooltip.enabled"),
        choice(
            SET_STYLE,
            "armorhud.option.style",
            [
                "armorhud.option.hotbar",
                "armorhud.option.roundedCorners",
                "armorhud.option.rounded",
                "armorhud.option.none",
            ],
            d.style.to_choice(),
        )
        .tooltip("armorhud.tooltip.style"),
        choice(
            SET_ORIENTATION,
            "armorhud.option.orientation",
            ["armorhud.option.horizontal", "armorhud.option.vertical"],
            d.orientation.to_choice(),
        ),
        choice(
            SET_WIDGET_SHOWN,
            "armorhud.option.widgetShown",
            [
                "armorhud.option.always",
                "armorhud.option.ifAnyPresent",
                "armorhud.option.notEmpty",
                "armorhud.option.damagedPieces",
            ],
            d.widget_shown.to_choice(),
        )
        .tooltip("armorhud.tooltip.widgetShown"),
        SettingSpec::toggle(SET_REVERSED, "armorhud.option.reversed", d.reversed),
        SettingSpec::toggle(SET_ICONS_SHOWN, "armorhud.option.showIcons", d.icons_shown)
            .tooltip("armorhud.tooltip.showIcons"),
        choice(
            SET_DURABILITY_DISPLAY,
            "armorhud.option.durabilityDisplay",
            [
                "armorhud.option.bar",
                "armorhud.option.numeric",
                "armorhud.option.percentage",
            ],
            d.durability_display.to_choice(),
        ),
        SettingSpec::toggle(
            SET_OFFHAND_DURABILITY,
            "armorhud.option.offhandDura",
            d.offhand_durability,
        ),
        SettingSpec::toggle(
            SET_MAINHAND_DURABILITY,
            "armorhud.option.mainHandDura",
            d.mainhand_durability,
        )
        .tooltip("armorhud.tooltip.mainHandDura"),
        choice(
            SET_ANCHOR,
            "armorhud.option.anchor",
            [
                "armorhud.option.topCenter",
                "armorhud.option.top",
                "armorhud.option.verticalCenter",
                "armorhud.option.bottom",
                "armorhud.option.hotbar",
            ],
            d.anchor.to_choice(),
        )
        .tooltip("armorhud.tooltip.anchor"),
        choice(
            SET_SIDE,
            "armorhud.option.side",
            ["options.mainHand.left", "options.mainHand.right"],
            d.side.to_choice(),
        )
        .tooltip("armorhud.tooltip.side"),
        SettingSpec::range(
            SET_OFFSET_X,
            "armorhud.option.offsetX",
            -OFFSET_LIMIT,
            OFFSET_LIMIT,
            1.0,
            0.0,
        )
        .tooltip("armorhud.tooltip.offsetX"),
        SettingSpec::range(
            SET_OFFSET_Y,
            "armorhud.option.offsetY",
            -OFFSET_LIMIT,
            OFFSET_LIMIT,
            1.0,
            0.0,
        )
        .tooltip("armorhud.tooltip.offsetY"),
        choice(
            SET_OFFHAND_BEHAVIOR,
            "armorhud.option.offhandSlotBehavior",
            [
                "armorhud.option.alwaysIgnore",
                "armorhud.option.adhere",
                "armorhud.option.alwaysLeaveSpace",
            ],
            d.offhand_behavior.to_choice(),
        )
        .tooltip("armorhud.tooltip.offhandSlotBehavior"),
        SettingSpec::toggle(
            SET_WARNING_SHOWN,
            "armorhud.option.showWarning",
            d.warning_shown,
        )
        .tooltip("armorhud.tooltip.showWarning"),
        SettingSpec::range(
            SET_MIN_DURABILITY_VALUE,
            "armorhud.option.minDuraValue",
            0.0,
            MIN_DURABILITY_LIMIT,
            1.0,
            f64::from(d.min_durability_value),
        )
        .tooltip("armorhud.tooltip.minDuraValue"),
        SettingSpec::range(
            SET_MIN_DURABILITY_PERCENT,
            "armorhud.option.minDuraPercent",
            0.0,
            1.0,
            0.01,
            d.min_durability_percentage,
        )
        .tooltip("armorhud.tooltip.minDuraPercent"),
        SettingSpec::range(
            SET_WARNING_BOB,
            "armorhud.option.iconBobIntensity",
            0.0,
            BOB_LIMIT,
            1.0,
            f64::from(d.warning_bob),
        )
        .tooltip("armorhud.tooltip.iconBobIntensity"),
        SettingSpec::toggle(
            SET_PLAY_BREAK_SOUND,
            "armorhud.option.playBreakSound",
            d.play_break_sound,
        )
        .tooltip("armorhud.tooltip.playBreakSound"),
    ];
    for spec in specs {
        let _ = r.register_setting(spec);
    }
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
    fn option_label_counts_match_the_enums() {
        assert_eq!(Anchor::ALL.len(), 5);
        assert_eq!(Style::ALL.len(), 4);
        assert_eq!(WidgetShown::ALL.len(), 4);
        assert_eq!(OffhandBehavior::ALL.len(), 3);
        assert_eq!(DurabilityDisplay::ALL.len(), 3);
    }
}
