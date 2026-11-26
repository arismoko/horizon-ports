//! Overflowing Bars settings: their keys, the Options > Mods registration and one read of all of
//! them from the client host.

use crate::logic::MAX_ROW_SHIFT;
use mod_api::{ClientHost, Registrar, SettingKey, SettingSpec};
use std::collections::HashMap;
use std::sync::{LazyLock, PoisonError, RwLock};

/// The keys `register_settings` was issued, by setting name; rebuilt on each registration.
static KEYS: LazyLock<RwLock<HashMap<String, SettingKey>>> = LazyLock::new(RwLock::default);

/// The dye colours' translation keys, in vanilla's order.
const DYE_KEYS: [&str; 16] = [
    "color.minecraft.white",
    "color.minecraft.orange",
    "color.minecraft.magenta",
    "color.minecraft.light_blue",
    "color.minecraft.yellow",
    "color.minecraft.lime",
    "color.minecraft.pink",
    "color.minecraft.gray",
    "color.minecraft.light_gray",
    "color.minecraft.cyan",
    "color.minecraft.purple",
    "color.minecraft.blue",
    "color.minecraft.brown",
    "color.minecraft.green",
    "color.minecraft.red",
    "color.minecraft.black",
];

/// The settings every icon bar shares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IconRow {
    pub allow_layers: bool,
    pub allow_count: bool,
    pub colorize_first_row: bool,
    pub inverse_coloring: bool,
    /// Icon rows to shift the bar by, -5 to 5.
    pub row_shift: i32,
    /// Skip empty points (armour and toughness only).
    pub skip_empty: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub health: IconRow,
    pub armor: IconRow,
    pub toughness: IconRow,
    pub move_chat_above_armor: bool,
    pub toughness_bar: bool,
    pub toughness_left: bool,
    pub count_color: u32,
    pub count_force_font: bool,
    pub count_full_rows_only: bool,
    pub count_always: bool,
    pub count_x: bool,
}

impl Default for Config {
    fn default() -> Self {
        let row = |skip_empty| IconRow {
            allow_layers: true,
            allow_count: true,
            colorize_first_row: false,
            inverse_coloring: false,
            row_shift: 0,
            skip_empty,
        };
        Self {
            health: row(false),
            armor: row(true),
            toughness: row(true),
            move_chat_above_armor: true,
            toughness_bar: true,
            toughness_left: false,
            count_color: 0,
            count_force_font: false,
            count_full_rows_only: false,
            count_always: false,
            count_x: true,
        }
    }
}

/// A shift read from a slider: whole rows, clamped, never a panic on NaN.
fn shift(value: f64) -> i32 {
    if value.is_finite() {
        value
            .round()
            .clamp(-f64::from(MAX_ROW_SHIFT), f64::from(MAX_ROW_SHIFT)) as i32
    } else {
        0
    }
}

impl Config {
    /// The player's settings, the defaults for any the host does not answer.
    pub fn read(host: &dyn ClientHost) -> Self {
        let d = Self::default();
        let keys = KEYS.read().unwrap_or_else(PoisonError::into_inner);
        let flag = |name: &str, default: bool| {
            keys.get(name)
                .and_then(|key| host.setting_bool(*key).ok())
                .unwrap_or(default)
        };
        let number = |name: &str| {
            keys.get(name)
                .and_then(|key| host.setting_f64(*key).ok())
                .unwrap_or(0.0)
        };
        let row = |prefix: &str, d: IconRow| IconRow {
            allow_layers: flag(&format!("{prefix}_allow_layers"), d.allow_layers),
            allow_count: flag(&format!("{prefix}_allow_count"), d.allow_count),
            colorize_first_row: flag(
                &format!("{prefix}_colorize_first_row"),
                d.colorize_first_row,
            ),
            inverse_coloring: flag(&format!("{prefix}_inverse_coloring"), d.inverse_coloring),
            row_shift: shift(number(&format!("{prefix}_row_shift"))),
            skip_empty: flag(&format!("{prefix}_skip_empty"), d.skip_empty),
        };
        Self {
            health: row("health", d.health),
            armor: row("armor", d.armor),
            toughness: row("toughness", d.toughness),
            move_chat_above_armor: flag("armor_move_chat", d.move_chat_above_armor),
            toughness_bar: flag("toughness_bar", d.toughness_bar),
            toughness_left: flag("toughness_left_side", d.toughness_left),
            count_color: keys
                .get("count_color")
                .and_then(|key| host.setting_choice(*key).ok())
                .unwrap_or(d.count_color),
            count_force_font: flag("count_force_font", d.count_force_font),
            count_full_rows_only: flag("count_full_rows_only", d.count_full_rows_only),
            count_always: flag("count_always", d.count_always),
            count_x: flag("count_x", d.count_x),
        }
    }
}

fn icon_row_specs(prefix: &str, d: IconRow, specs: &mut Vec<SettingSpec>) {
    let toggle = |name: &str, default: bool| {
        let key = format!("{prefix}_{name}");
        SettingSpec::toggle(
            key.clone(),
            format!("overflowingbars.option.{key}"),
            default,
        )
        .tooltip(format!("overflowingbars.tooltip.{key}"))
    };
    specs.push(toggle("allow_layers", d.allow_layers));
    specs.push(toggle("allow_count", d.allow_count));
    specs.push(toggle("colorize_first_row", d.colorize_first_row));
    specs.push(toggle("inverse_coloring", d.inverse_coloring));
    let key = format!("{prefix}_row_shift");
    specs.push(
        SettingSpec::range(
            key.clone(),
            format!("overflowingbars.option.{key}"),
            -f64::from(MAX_ROW_SHIFT),
            f64::from(MAX_ROW_SHIFT),
            1.0,
            0.0,
        )
        .tooltip(format!("overflowingbars.tooltip.{key}")),
    );
}

/// The Options > Mods section a setting is listed under (a translation key).
fn category_of(name: &str) -> Option<&'static str> {
    let prefix = name.split('_').next()?;
    match prefix {
        "health" => Some("overflowingbars.category.health"),
        "armor" => Some("overflowingbars.category.armor"),
        "toughness" => Some("overflowingbars.category.toughness"),
        "count" => Some("overflowingbars.category.count"),
        _ => None,
    }
}

/// Register every setting on Options > Mods > Overflowing Bars.
pub fn register_settings(r: &mut dyn Registrar) {
    let d = Config::default();
    let toggle = |key: &str, default: bool| {
        SettingSpec::toggle(key, format!("overflowingbars.option.{key}"), default)
            .tooltip(format!("overflowingbars.tooltip.{key}"))
    };
    let mut specs = Vec::new();
    icon_row_specs("health", d.health, &mut specs);
    icon_row_specs("armor", d.armor, &mut specs);
    specs.push(toggle("armor_skip_empty", d.armor.skip_empty));
    specs.push(toggle("armor_move_chat", d.move_chat_above_armor));
    specs.push(toggle("toughness_bar", d.toughness_bar));
    icon_row_specs("toughness", d.toughness, &mut specs);
    specs.push(toggle("toughness_skip_empty", d.toughness.skip_empty));
    specs.push(toggle("toughness_left_side", d.toughness_left));
    specs.push(
        SettingSpec::choice(
            "count_color",
            "overflowingbars.option.count_color",
            DYE_KEYS,
            d.count_color as usize,
        )
        .tooltip("overflowingbars.tooltip.count_color"),
    );
    specs.push(toggle("count_force_font", d.count_force_font));
    specs.push(toggle("count_full_rows_only", d.count_full_rows_only));
    specs.push(toggle("count_always", d.count_always));
    specs.push(toggle("count_x", d.count_x));
    let mut keys = KEYS.write().unwrap_or_else(PoisonError::into_inner);
    keys.clear();
    for spec in specs {
        let name = spec.key.clone();
        if let Ok(key) = r.register_setting(spec) {
            if let Some(category) = category_of(&name) {
                let _ = r.categorize_setting(&name, category);
            }
            keys.insert(name, key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_originals() {
        let d = Config::default();
        assert!(d.health.allow_layers && d.armor.allow_layers && d.toughness.allow_layers);
        assert!(!d.health.skip_empty && d.armor.skip_empty && d.toughness.skip_empty);
        assert!(d.move_chat_above_armor && d.toughness_bar && !d.toughness_left);
        assert!(d.count_x && !d.count_always && !d.count_force_font && !d.count_full_rows_only);
        assert_eq!(d.count_color, 0);
    }

    #[test]
    fn shifts_are_clamped_and_never_panic() {
        assert_eq!(shift(f64::NAN), 0);
        assert_eq!(shift(f64::INFINITY), 0);
        assert_eq!(shift(99.0), 5);
        assert_eq!(shift(-99.0), -5);
        assert_eq!(shift(2.4), 2);
    }

    #[test]
    fn every_setting_lands_in_a_category_with_a_label() {
        assert_eq!(
            category_of("health_row_shift"),
            Some("overflowingbars.category.health")
        );
        assert_eq!(
            category_of("armor_move_chat"),
            Some("overflowingbars.category.armor")
        );
        assert_eq!(
            category_of("toughness_left_side"),
            Some("overflowingbars.category.toughness")
        );
        assert_eq!(
            category_of("count_x"),
            Some("overflowingbars.category.count")
        );
        assert_eq!(category_of("unknown"), None);
    }

    #[test]
    fn there_is_one_dye_label_per_dye_colour() {
        assert_eq!(DYE_KEYS.len(), crate::logic::DYE_COLORS.len());
    }
}
