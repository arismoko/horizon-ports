#![deny(unsafe_code)]
//! BEBPM (Better Enchantment Bookshelf Placement, CozyStudios, CC0-1.0) for Sunrise.
//!
//! The enchanting table scans a configurable box around it instead of vanilla's ring (default
//! radius 10, up to 16 in X, Z, up and down). The box is registered as extra scan offsets, and
//! `is_valid_book_shelf` decides which scanned positions count, with the original's three
//! line-of-sight modes.
mod rules;

use std::ops::ControlFlow;
use std::sync::OnceLock;

use mod_api::{
    BlockShapeKind, Hooks, Host, IsValidBookShelfEvent, LevelRef, Registrar, RegistrationContext,
    SettingKey, SettingSpec, TagKey,
};

use crate::rules::{Cell, Config, LineOfSight};

const MOD_ID: &str = "bebpm";
const TRANSMITTER_TAG: &str = "minecraft:enchantment_power_transmitter";
const PERMEABLE_TAG: &str = "bebpm:power_permeable";

/// The settings as they were when registration began; offsets are fixed at startup, so the
/// line-of-sight test reads the same values and a change takes a restart.
static CONFIG: OnceLock<Config> = OnceLock::new();
static TAGS: OnceLock<(TagKey, TagKey)> = OnceLock::new();

#[derive(Default)]
struct Mod;

fn read_setting(cx: &RegistrationContext, key: &str) -> Option<String> {
    cx.setting(MOD_ID, key).ok().flatten()
}

fn register_setting(r: &mut dyn Registrar, spec: SettingSpec) -> Option<SettingKey> {
    r.register_setting(spec).ok()
}

fn range_spec(key: &str, default: i32) -> SettingSpec {
    SettingSpec::range(
        key,
        format!("setting.bebpm.{key}"),
        0.0,
        f64::from(rules::MAX_RADIUS),
        1.0,
        f64::from(default),
    )
    .tooltip(format!("setting.bebpm.{key}.tooltip"))
}

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, cx: &RegistrationContext) {
        for key in [rules::KEY_X, rules::KEY_Z, rules::KEY_UP, rules::KEY_DOWN] {
            let _ = register_setting(r, range_spec(key, rules::DEFAULT_RADIUS));
        }
        let _ = register_setting(
            r,
            SettingSpec::choice(
                rules::KEY_LOS,
                "setting.bebpm.line_of_sight",
                [
                    "setting.bebpm.line_of_sight.off",
                    "setting.bebpm.line_of_sight.decor_only",
                    "setting.bebpm.line_of_sight.on",
                ],
                LineOfSight::default().index(),
            )
            .tooltip("setting.bebpm.line_of_sight.tooltip"),
        );

        let config = Config::from_settings(|key| read_setting(cx, key));
        r.add_enchanting_table_offsets(&config.offsets());
        let _ = CONFIG.set(config);
        let _ = TAGS.set((r.tag_key(TRANSMITTER_TAG), r.tag_key(PERMEABLE_TAG)));
    }

    fn is_valid_book_shelf(&self, host: &dyn Host, event: &mut IsValidBookShelfEvent) {
        let Some(config) = CONFIG.get() else { return };
        let table = [event.table_pos.x, event.table_pos.y, event.table_pos.z];
        let offset = [event.offset.x, event.offset.y, event.offset.z];
        event.valid = is_valid(host, event.level, config, table, offset);
    }
}

fn is_valid(
    host: &dyn Host,
    level: LevelRef,
    config: &Config,
    table: [i32; 3],
    offset: [i32; 3],
) -> bool {
    if !config.contains(offset) {
        return false;
    }
    let shelf = [
        table[0] + offset[0],
        table[1] + offset[1],
        table[2] + offset[2],
    ];
    let Some(state) = level.block_state(host, shelf) else {
        return false;
    };
    let powered = level
        .get_enchant_power_bonus(host, shelf, state, table)
        .is_ok_and(|bonus| bonus.is_finite() && bonus > 0.0);
    if !powered {
        return false;
    }
    let Some(&(transmitter, permeable)) = TAGS.get() else {
        return false;
    };
    rules::line_clear(config.los, offset, |step| {
        let pos = [table[0] + step[0], table[1] + step[1], table[2] + step[2]];
        let state = level.block_state(host, pos)?;
        let in_tag = |tag| state.block().is_in_tag(host.tags(), tag) == Some(true);
        let mut full = None;
        if config.los == LineOfSight::DecorOnly {
            let mut boxes = Vec::new();
            level.visit_block_shape(host, state, pos, BlockShapeKind::Collision, &mut |b| {
                boxes.push(b);
                ControlFlow::Continue(())
            })?;
            full = Some(rules::is_full_block(&boxes));
        }
        Some(Cell {
            transmitter: in_tag(transmitter),
            permeable: in_tag(permeable),
            full: full.unwrap_or(false),
        })
    })
}
