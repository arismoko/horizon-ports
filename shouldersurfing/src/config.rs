//! Options of Shoulder Surfing Reloaded: the scalar ones are Sunrise settings (Options > Mods),
//! the list ones live in `config/shouldersurfing-client.toml`.

use mod_api::{
    ClientHost, ConfigEvent, ConfigKind, ConfigListener, ConfigSnapshot, ConfigSpec, Registrar,
    SettingKey, SettingSpec, SettingValue,
};
use std::sync::{Arc, RwLock};

pub const MOD_ID: &str = "shouldersurfing";
pub const CONFIG_FILE_NAME: &str = "shouldersurfing-client.toml";
const LABEL_PREFIX: &str = "shouldersurfing.configuration.";

/// What an option is, with its default.
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Bool(bool),
    Float {
        min: f64,
        max: f64,
        step: f64,
        def: f64,
    },
    Int {
        min: i32,
        max: i32,
        def: i32,
    },
    /// `group` names the enum for the labels of its values.
    Choice {
        group: &'static str,
        opts: &'static [&'static str],
        def: usize,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct Opt {
    /// The setting key (at most 64 bytes).
    pub key: &'static str,
    /// The original's config path when it differs from the key, else empty.
    pub lang: &'static str,
    pub kind: Kind,
}

impl Opt {
    pub fn label_key(&self) -> String {
        let path = if self.lang.is_empty() {
            self.key
        } else {
            self.lang
        };
        format!("{LABEL_PREFIX}{path}")
    }

    pub fn default_value(&self) -> f64 {
        match self.kind {
            Kind::Bool(b) => f64::from(u8::from(b)),
            Kind::Float { def, .. } => def,
            Kind::Int { def, .. } => f64::from(def),
            Kind::Choice { def, .. } => def as f64,
        }
    }
}

const fn float(min: f64, max: f64, step: f64, def: f64) -> Kind {
    Kind::Float {
        min,
        max,
        step,
        def,
    }
}

const fn int(min: i32, max: i32, def: i32) -> Kind {
    Kind::Int { min, max, def }
}

const fn choice(group: &'static str, opts: &'static [&'static str], def: usize) -> Kind {
    Kind::Choice { group, opts, def }
}

const MODES: &[&str] = &["always", "never", "requires_target"];
const ORIGINS: &[&str] = &["player", "camera"];
const VISIBILITY: &[&str] = &[
    "always",
    "never",
    "when_aiming",
    "when_in_range",
    "when_aiming_or_in_range",
];

macro_rules! options {
    ($($v:ident, $key:literal, $lang:literal, $kind:expr;)*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum K { $($v,)* }
        pub const TABLE: &[Opt] = &[$(Opt { key: $key, lang: $lang, kind: $kind },)*];
    };
}

options! {
    OffsetX, "camera.offset.offset_x", "", float(-20.0, 20.0, 0.025, -0.75);
    OffsetY, "camera.offset.offset_y", "", float(-10.0, 10.0, 0.025, 0.0);
    OffsetZ, "camera.offset.offset_z", "", float(-20.0, 20.0, 0.025, 4.0);
    MinOffsetX, "camera.offset.min.min_offset_x", "", float(-30.0, 30.0, 0.05, -3.0);
    MinOffsetY, "camera.offset.min.min_offset_y", "", float(-30.0, 30.0, 0.05, -1.0);
    MinOffsetZ, "camera.offset.min.min_offset_z", "", float(-30.0, 30.0, 0.05, -3.0);
    MaxOffsetX, "camera.offset.max.max_offset_x", "", float(-30.0, 30.0, 0.05, 3.0);
    MaxOffsetY, "camera.offset.max.max_offset_y", "", float(-30.0, 30.0, 0.05, 1.5);
    MaxOffsetZ, "camera.offset.max.max_offset_z", "", float(-30.0, 30.0, 0.05, 5.0);
    UnlimitedX, "camera.offset.limits.unlimited_offset_x", "", Kind::Bool(false);
    UnlimitedY, "camera.offset.limits.unlimited_offset_y", "", Kind::Bool(false);
    UnlimitedZ, "camera.offset.limits.unlimited_offset_z", "", Kind::Bool(false);
    PassengerMulX, "camera.offset.multiplier.passenger.multiplier_offset_x", "", float(-10.0, 10.0, 0.05, 1.0);
    PassengerMulY, "camera.offset.multiplier.passenger.multiplier_offset_y", "", float(-10.0, 10.0, 0.05, 1.0);
    PassengerMulZ, "camera.offset.multiplier.passenger.multiplier_offset_z", "", float(-10.0, 10.0, 0.05, 1.0);
    SprintMulX, "camera.offset.multiplier.sprint.multiplier_offset_x", "", float(-10.0, 10.0, 0.05, 1.0);
    SprintMulY, "camera.offset.multiplier.sprint.multiplier_offset_y", "", float(-10.0, 10.0, 0.05, 1.0);
    SprintMulZ, "camera.offset.multiplier.sprint.multiplier_offset_z", "", float(-10.0, 10.0, 0.05, 1.0);
    AimingMulX, "camera.offset.multiplier.aiming.multiplier_offset_x", "", float(-10.0, 10.0, 0.05, 1.0);
    AimingMulY, "camera.offset.multiplier.aiming.multiplier_offset_y", "", float(-10.0, 10.0, 0.05, 1.0);
    AimingMulZ, "camera.offset.multiplier.aiming.multiplier_offset_z", "", float(-10.0, 10.0, 0.05, 1.0);
    FallFlyingMulX, "camera.offset.multiplier.fall_flying.multiplier_offset_x", "", float(-10.0, 10.0, 0.05, 1.0);
    FallFlyingMulY, "camera.offset.multiplier.fall_flying.multiplier_offset_y", "", float(-10.0, 10.0, 0.05, 1.0);
    FallFlyingMulZ, "camera.offset.multiplier.fall_flying.multiplier_offset_z", "", float(-10.0, 10.0, 0.05, 1.0);
    ClimbingMulX, "camera.offset.multiplier.climbing.multiplier_offset_x", "", float(-10.0, 10.0, 0.05, 0.0);
    ClimbingMulY, "camera.offset.multiplier.climbing.multiplier_offset_y", "", float(-10.0, 10.0, 0.05, 1.0);
    ClimbingMulZ, "camera.offset.multiplier.climbing.multiplier_offset_z", "", float(-10.0, 10.0, 0.05, 1.0);
    PassengerModX, "camera.offset.modifier.passenger.modifier_offset_x", "", float(-10.0, 10.0, 0.05, 0.0);
    PassengerModY, "camera.offset.modifier.passenger.modifier_offset_y", "", float(-10.0, 10.0, 0.05, 0.0);
    PassengerModZ, "camera.offset.modifier.passenger.modifier_offset_z", "", float(-10.0, 10.0, 0.05, 0.0);
    SprintModX, "camera.offset.modifier.sprint.modifier_offset_x", "", float(-10.0, 10.0, 0.05, 0.0);
    SprintModY, "camera.offset.modifier.sprint.modifier_offset_y", "", float(-10.0, 10.0, 0.05, 0.0);
    SprintModZ, "camera.offset.modifier.sprint.modifier_offset_z", "", float(-10.0, 10.0, 0.05, 0.0);
    AimingModX, "camera.offset.modifier.aiming.modifier_offset_x", "", float(-10.0, 10.0, 0.05, 0.0);
    AimingModY, "camera.offset.modifier.aiming.modifier_offset_y", "", float(-10.0, 10.0, 0.05, 0.0);
    AimingModZ, "camera.offset.modifier.aiming.modifier_offset_z", "", float(-10.0, 10.0, 0.05, 0.0);
    FallFlyingModX, "camera.offset.modifier.fall_flying.modifier_offset_x", "", float(-10.0, 10.0, 0.05, 0.0);
    FallFlyingModY, "camera.offset.modifier.fall_flying.modifier_offset_y", "", float(-10.0, 10.0, 0.05, 0.0);
    FallFlyingModZ, "camera.offset.modifier.fall_flying.modifier_offset_z", "", float(-10.0, 10.0, 0.05, 0.0);
    ClimbingModX, "camera.offset.modifier.climbing.modifier_offset_x", "", float(-10.0, 10.0, 0.05, 0.0);
    ClimbingModY, "camera.offset.modifier.climbing.modifier_offset_y", "", float(-10.0, 10.0, 0.05, 0.0);
    ClimbingModZ, "camera.offset.modifier.climbing.modifier_offset_z", "", float(-10.0, 10.0, 0.05, 0.0);
    DynamicOffsets, "camera.offset.dynamic_offsets", "", Kind::Bool(true);
    StepSize, "camera.offset.step_size", "", float(0.001, 1.0, 0.001, 0.025);
    KeepOutOfHead, "camera.keep_camera_out_of_head_distance_multiplier", "", float(0.0, 5.0, 0.05, 0.75);
    TransitionSpeed, "camera.camera_transition_speed_multiplier", "", float(0.05, 1.0, 0.05, 0.25);
    CenterLookingDown, "camera.center_camera_when_looking_down_angle", "", float(0.0, 90.0, 1.0, 1.0);
    DistanceAttributeMode, "camera.camera_distance_attribute_mode", "", choice("distance_attribute_mode", &["relative", "absolute", "ignore"], 0);
    DecoupledCamera, "camera.decoupled_camera", "", Kind::Bool(true);
    FovOverrideEnabled, "camera.fov.fov_override_enabled", "", Kind::Bool(false);
    FovOverride, "camera.fov.fov_override", "", float(30.0, 110.0, 1.0, 70.0);
    DefaultPerspective, "perspective.default_perspective", "", choice("perspective", &["first_person", "third_person_back", "third_person_front", "shoulder_surfing"], 3);
    RememberLastPerspective, "perspective.remember_last_perspective", "", Kind::Bool(true);
    ReplaceDefaultPerspective, "perspective.replace_default_perspective", "", Kind::Bool(false);
    FirstPersonEnabled, "perspective.first_person_enabled", "", Kind::Bool(true);
    ThirdPersonFrontEnabled, "perspective.third_person_front_enabled", "", Kind::Bool(true);
    ThirdPersonBackEnabled, "perspective.third_person_back_enabled", "", Kind::Bool(true);
    HideLookingUpAngle, "player.hide_player_when_looking_up_angle", "", float(0.0, 90.0, 1.0, 0.0);
    AdjustTransparency, "player.transparency.adjust_transparency", "", Kind::Bool(true);
    TransparentWhenAiming, "player.transparency.when_aiming", "", Kind::Bool(false);
    TransparentWhenClimbing, "player.transparency.when_climbing", "", Kind::Bool(false);
    TurnWhenUsingItem, "player.turning.when_using_item", "", choice("turning_mode", MODES, 0);
    TurnWhenAttacking, "player.turning.when_attacking", "", choice("turning_mode", MODES, 2);
    TurnWhenInteracting, "player.turning.when_interacting", "", choice("turning_mode", MODES, 0);
    TurnWhenPicking, "player.turning.when_picking", "", choice("turning_mode", MODES, 0);
    TurningLockTime, "player.turning.turning_lock_time", "", int(0, 100, 4);
    TurningSpeed, "player.turning.turning_speed_multiplier", "", float(0.05, 1.0, 0.05, 0.25);
    PlayerXRotWithCamera, "player.turning.turn_player_x_rot_with_camera", "", Kind::Bool(true);
    PlayerXRotToCamera, "player.turning.turn_player_x_rot_to_camera", "", Kind::Bool(false);
    PlayerYRotWithCamera, "player.turning.turn_player_y_rot_with_camera", "", Kind::Bool(true);
    CustomRaytraceDistance, "object_picker.custom_raytrace_distance", "", float(0.0, 1000.0, 1.0, 400.0);
    UseCustomRaytraceDistance, "object_picker.use_custom_raytrace_distance", "", Kind::Bool(true);
    EntityPickOrigin, "object_picker.pick_origin.entity_pick_origin", "", choice("pick_origin", ORIGINS, 0);
    BlockPickOrigin, "object_picker.pick_origin.block_pick_origin", "", choice("pick_origin", ORIGINS, 0);
    PickVector, "object_picker.pick_vector.pick_vector", "", choice("pick_vector", ORIGINS, 1);
    CrosshairType, "crosshair.crosshair_type", "", choice("crosshair_type", &["adaptive", "dynamic", "static", "static_with_1pp", "dynamic_with_1pp"], 2);
    ObstructionIndicator, "crosshair.obstruction.obstruction_indicator", "obstruction.obstruction_indicator", Kind::Bool(true);
    ObstructionOnlyWhenAiming, "crosshair.obstruction.only_when_aiming", "obstruction.only_when_aiming", Kind::Bool(true);
    ObstructionMinDistance, "crosshair.obstruction.min_distance_to_crosshair", "obstruction.min_distance_to_crosshair", int(0, 500, 8);
    ObstructionMaxDistance, "crosshair.obstruction.max_distance_to_obstruction", "obstruction.max_distance_to_obstruction", float(0.0, 500.0, 1.0, 20.0);
    VisibilityFirstPerson, "crosshair.visibility.first_person", "", choice("visibility", VISIBILITY, 0);
    VisibilityThirdPersonBack, "crosshair.visibility.third_person_back", "", choice("visibility", VISIBILITY, 1);
    VisibilityThirdPersonFront, "crosshair.visibility.third_person_front", "", choice("visibility", VISIBILITY, 1);
    VisibilityShoulderSurfing, "crosshair.visibility.shoulder_surfing", "", choice("visibility", VISIBILITY, 0);
    MapOverlayEnabled, "gui.maps.map_item_overlay_enabled", "", Kind::Bool(true);
    MapOverlayAnchor, "gui.maps.map_item_overlay_anchor", "", choice("map_overlay_anchor", &["top", "bottom"], 1);
    MapOverlayScale, "gui.maps.map_item_overlay_scale", "", int(5, 100, 40);
    MapInsetTop, "gui.maps.map_item_overlay.insets.top", "", int(0, 500, 1);
    MapInsetRight, "gui.maps.map_item_overlay.insets.right", "", int(0, 500, 1);
    MapInsetBottom, "gui.maps.map_item_overlay.insets.bottom", "", int(0, 500, 1);
    MapInsetLeft, "gui.maps.map_item_overlay.insets.left", "", int(0, 500, 1);
    CenterPlayerSounds, "audio.center_player_sounds", "", Kind::Bool(false);
}

/// The option's translation key for one value of a choice.
pub fn choice_label_key(group: &str, value: &str) -> String {
    format!("shouldersurfing.option.{group}.{value}")
}

/// The keys the registrar issued, in `TABLE` order (None for one it refused).
static SETTING_KEYS: RwLock<Vec<Option<SettingKey>>> = RwLock::new(Vec::new());

fn setting_key(k: K) -> Option<SettingKey> {
    SETTING_KEYS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(k as usize)
        .copied()
        .flatten()
}

pub fn register_settings(r: &mut dyn Registrar) {
    let mut keys = Vec::with_capacity(TABLE.len());
    for opt in TABLE {
        let label = opt.label_key();
        let tooltip = format!("{label}.tooltip");
        let spec = match opt.kind {
            Kind::Bool(def) => SettingSpec::toggle(opt.key, label, def),
            Kind::Float {
                min,
                max,
                step,
                def,
            } => SettingSpec::range(opt.key, label, min, max, step, def),
            Kind::Int { min, max, def } => SettingSpec::range(
                opt.key,
                label,
                f64::from(min),
                f64::from(max),
                1.0,
                f64::from(def),
            ),
            Kind::Choice { group, opts, def } => SettingSpec::choice(
                opt.key,
                label,
                opts.iter().map(|value| choice_label_key(group, value)),
                def,
            ),
        };
        keys.push(r.register_setting(spec.tooltip(tooltip)).ok());
    }
    *SETTING_KEYS.write().unwrap_or_else(|e| e.into_inner()) = keys;
}

/// The current value of every option.
#[derive(Clone, Debug)]
pub struct Values(Vec<f64>);

impl Default for Values {
    fn default() -> Self {
        Self(TABLE.iter().map(Opt::default_value).collect())
    }
}

impl Values {
    /// Reads every setting; one that is missing, not finite or out of range keeps its last value.
    pub fn refresh(&mut self, host: &dyn ClientHost) {
        let keys = SETTING_KEYS.read().unwrap_or_else(|e| e.into_inner());
        for ((slot, opt), key) in self.0.iter_mut().zip(TABLE).zip(keys.iter()) {
            let Some(key) = *key else {
                continue;
            };
            let read = match opt.kind {
                Kind::Bool(_) => host.setting_bool(key).ok().map(|b| f64::from(u8::from(b))),
                Kind::Float { min, max, .. } => host
                    .setting_f64(key)
                    .ok()
                    .filter(|v| v.is_finite())
                    .map(|v| v.clamp(min, max)),
                Kind::Int { min, max, .. } => host
                    .setting_f64(key)
                    .ok()
                    .filter(|v| v.is_finite())
                    .map(|v| v.round().clamp(f64::from(min), f64::from(max))),
                Kind::Choice { opts, .. } => host
                    .setting_choice(key)
                    .ok()
                    .map(|i| (i as usize).min(opts.len() - 1) as f64),
            };
            if let Some(value) = read {
                *slot = value;
            }
        }
    }

    /// Takes the value a `setting_changed` notice carries; a key or value of another kind is
    /// ignored.
    pub fn apply(&mut self, key: &str, value: SettingValue) {
        let Some((slot, opt)) = self.0.iter_mut().zip(TABLE).find(|(_, o)| o.key == key) else {
            return;
        };
        let read = match (opt.kind, value) {
            (Kind::Bool(_), SettingValue::Toggle(b)) => Some(f64::from(u8::from(b))),
            (Kind::Float { min, max, .. }, SettingValue::Range(v)) if v.is_finite() => {
                Some(v.clamp(min, max))
            }
            (Kind::Int { min, max, .. }, SettingValue::Range(v)) if v.is_finite() => {
                Some(v.round().clamp(f64::from(min), f64::from(max)))
            }
            (Kind::Choice { opts, .. }, SettingValue::Choice(i)) => {
                Some(i.min(opts.len() - 1) as f64)
            }
            _ => None,
        };
        if let Some(value) = read {
            *slot = value;
        }
    }

    pub fn f(&self, k: K) -> f64 {
        self.0[k as usize]
    }

    pub fn b(&self, k: K) -> bool {
        self.0[k as usize] != 0.0
    }

    pub fn i(&self, k: K) -> i32 {
        self.0[k as usize] as i32
    }

    pub fn c(&self, k: K) -> usize {
        self.0[k as usize] as usize
    }

    pub fn set(&mut self, k: K, value: f64) {
        self.0[k as usize] = value;
    }

    pub fn v3(&self, x: K, y: K, z: K) -> [f64; 3] {
        [self.f(x), self.f(y), self.f(z)]
    }
}

/// Writes a number to the setting of `k` (and clamps it into its range).
pub fn write_f64(host: &mut dyn ClientHost, values: &mut Values, k: K, value: f64) {
    let opt = &TABLE[k as usize];
    let value = match opt.kind {
        Kind::Float { min, max, .. } => value.clamp(min, max),
        _ => value,
    };
    values.set(k, value);
    if let Some(key) = setting_key(k) {
        let _ = host.set_setting(key, SettingValue::Range(value));
    }
}

pub fn write_bool(host: &mut dyn ClientHost, values: &mut Values, k: K, value: bool) {
    values.set(k, f64::from(u8::from(value)));
    if let Some(key) = setting_key(k) {
        let _ = host.set_setting(key, SettingValue::Toggle(value));
    }
}

pub fn write_choice(host: &mut dyn ClientHost, values: &mut Values, k: K, index: usize) {
    values.set(k, index as f64);
    if let Some(key) = setting_key(k) {
        let _ = host.set_setting(key, SettingValue::Choice(index));
    }
}

macro_rules! choice_enum {
    ($name:ident { $($variant:ident),* $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name { $($variant),* }
        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),*];
            pub fn from_index(index: usize) -> Self {
                Self::ALL[index.min(Self::ALL.len() - 1)]
            }
        }
    };
}

choice_enum!(CrosshairType {
    Adaptive,
    Dynamic,
    Static,
    StaticWith1pp,
    DynamicWith1pp,
});
choice_enum!(Visibility {
    Always,
    Never,
    WhenAiming,
    WhenInRange,
    WhenAimingOrInRange,
});
choice_enum!(TurningMode {
    Always,
    Never,
    RequiresTarget,
});
choice_enum!(DistanceAttributeMode {
    Relative,
    Absolute,
    Ignore,
});
choice_enum!(Origin { Player, Camera });

impl TurningMode {
    /// `TurningMode.shouldTurn` for a crosshair that is on something.
    pub fn should_turn(self, has_target: bool) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::RequiresTarget => has_target,
        }
    }
}

/// The options that are lists, from `config/shouldersurfing-client.toml`.
#[derive(Clone, Debug, PartialEq)]
pub struct Lists {
    pub offset_x_presets: Vec<f64>,
    pub offset_y_presets: Vec<f64>,
    pub offset_z_presets: Vec<f64>,
    pub non_collidable_blocks: Vec<String>,
    pub unpickable_invisible_entities: Vec<String>,
    pub hold_items: Vec<String>,
    pub use_items: Vec<String>,
    pub hold_item_animations: Vec<String>,
    pub use_item_animations: Vec<String>,
    pub hold_item_components: Vec<String>,
    pub use_item_components: Vec<String>,
    pub hold_default_components: Vec<String>,
    pub use_default_components: Vec<String>,
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

impl Default for Lists {
    fn default() -> Self {
        Self {
            offset_x_presets: Vec::new(),
            offset_y_presets: Vec::new(),
            offset_z_presets: Vec::new(),
            non_collidable_blocks: Vec::new(),
            unpickable_invisible_entities: strings(&[
                "minecraft:armor_stand",
                "minecraft:block_display",
                "minecraft:item_display",
                "minecraft:text_display",
            ]),
            hold_items: strings(&[
                "minecraft:snowball",
                "minecraft:egg",
                "minecraft:experience_bottle",
                "minecraft:ender_pearl",
                "minecraft:splash_potion",
                "minecraft:fishing_rod",
                "minecraft:lingering_potion",
                "minecraft:wind_charge",
            ]),
            use_items: Vec::new(),
            hold_item_animations: Vec::new(),
            use_item_animations: strings(&["bow", "trident"]),
            hold_item_components: strings(&[
                "minecraft:charged_projectiles",
                "minecraft:piercing_weapon",
            ]),
            use_item_components: Vec::new(),
            hold_default_components: strings(&["minecraft:piercing_weapon"]),
            use_default_components: Vec::new(),
        }
    }
}

const STRING_KEYS: [&str; 10] = [
    "non_collidable_blocks",
    "unpickable_invisible_entities",
    "adaptive_crosshair_hold_items",
    "adaptive_crosshair_use_items",
    "adaptive_crosshair_hold_item_animations",
    "adaptive_crosshair_use_item_animations",
    "adaptive_crosshair_hold_item_components",
    "adaptive_crosshair_use_item_components",
    "adaptive_crosshair_item_hold_default_components",
    "adaptive_crosshair_item_use_default_components",
];
const PRESET_KEYS: [&str; 3] = ["offset_x_presets", "offset_y_presets", "offset_z_presets"];

impl Lists {
    fn string_lists(&self) -> [&Vec<String>; 10] {
        [
            &self.non_collidable_blocks,
            &self.unpickable_invisible_entities,
            &self.hold_items,
            &self.use_items,
            &self.hold_item_animations,
            &self.use_item_animations,
            &self.hold_item_components,
            &self.use_item_components,
            &self.hold_default_components,
            &self.use_default_components,
        ]
    }

    fn string_lists_mut(&mut self) -> [&mut Vec<String>; 10] {
        [
            &mut self.non_collidable_blocks,
            &mut self.unpickable_invisible_entities,
            &mut self.hold_items,
            &mut self.use_items,
            &mut self.hold_item_animations,
            &mut self.use_item_animations,
            &mut self.hold_item_components,
            &mut self.use_item_components,
            &mut self.hold_default_components,
            &mut self.use_default_components,
        ]
    }

    pub fn to_toml(&self) -> String {
        let mut out = String::from(
            "# Shoulder Surfing Reloaded: list options.\n\
             # Every other option is in Options > Mods > Shoulder Surfing Reloaded.\n\
             # Item, block and entity entries are regular expressions matched against the whole id.\n\n",
        );
        let presets = [
            &self.offset_x_presets,
            &self.offset_y_presets,
            &self.offset_z_presets,
        ];
        out.push_str("[camera.offset.presets]\n");
        for (key, list) in PRESET_KEYS.iter().zip(presets) {
            let items: Vec<String> = list.iter().map(|v| quote(&format!("{v}"))).collect();
            out.push_str(&format!("{key} = [{}]\n", items.join(", ")));
        }
        out.push_str("\n[lists]\n");
        for (key, list) in STRING_KEYS.iter().zip(self.string_lists()) {
            let items: Vec<String> = list.iter().map(|s| quote(s)).collect();
            out.push_str(&format!("{key} = [{}]\n", items.join(", ")));
        }
        out
    }

    /// Reads the file; a key that is missing or malformed keeps its default.
    pub fn parse(text: &str) -> Self {
        let table = parse_string_arrays(text);
        let mut lists = Self::default();
        let get = |key: &str| table.iter().find(|(k, _)| k == key).map(|(_, v)| v);
        for (key, slot) in PRESET_KEYS.iter().zip([
            &mut lists.offset_x_presets,
            &mut lists.offset_y_presets,
            &mut lists.offset_z_presets,
        ]) {
            if let Some(items) = get(key) {
                *slot = items
                    .iter()
                    .filter_map(|s| s.trim().parse::<f64>().ok())
                    .filter(|v| v.is_finite())
                    .collect();
            }
        }
        for (key, slot) in STRING_KEYS.iter().zip(lists.string_lists_mut()) {
            if let Some(items) = get(key) {
                *slot = items.clone();
            }
        }
        lists
    }
}

fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `key = ["a", "b"]` lines (arrays may span lines); table headers and comments are skipped.
/// Anything else is ignored, so a malformed file never panics.
pub fn parse_string_arrays(text: &str) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((key, rest)) = line.split_once('=') else {
            continue;
        };
        let mut body = rest.trim().to_owned();
        if !body.starts_with('[') {
            continue;
        }
        while !array_closed(&body) {
            match lines.next() {
                Some(more) => {
                    body.push(' ');
                    body.push_str(more.trim());
                }
                None => break,
            }
        }
        out.push((key.trim().to_owned(), array_items(&body)));
    }
    out
}

fn array_closed(body: &str) -> bool {
    let mut in_string = false;
    let mut escaped = false;
    for c in body.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
        } else if c == ']' {
            return true;
        } else if c == '#' {
            return false;
        }
    }
    false
}

fn array_items(body: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut escaped = false;
    for c in body.chars() {
        if in_string {
            if escaped {
                current.push(match c {
                    'n' => '\n',
                    other => other,
                });
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
                items.push(std::mem::take(&mut current));
            } else {
                current.push(c);
            }
        } else if c == '"' {
            in_string = true;
        } else if c == ']' {
            break;
        }
    }
    items
}

/// Declares `config/shouldersurfing-client.toml`; its listener keeps the lists current.
pub fn register_mod_configs(
    r: &mut dyn Registrar,
    on_lists: impl Fn(Lists) + Send + Sync + 'static,
) {
    let spec =
        ConfigSpec::new(ConfigKind::Client, Lists::default().to_toml()).file(CONFIG_FILE_NAME);
    let listener = move |event: ConfigEvent<'_>, snapshot: &ConfigSnapshot| -> Result<(), String> {
        if matches!(event, ConfigEvent::Loading(_) | ConfigEvent::Reloading(_)) {
            on_lists(Lists::parse(&snapshot.text));
        }
        Ok(())
    };
    let listener: Arc<ConfigListener> = Arc::new(listener);
    // A refused declaration (the file is registered twice) leaves the first one active.
    let _ = r.register_config(spec, Some(listener));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_valid_unique_and_defaults_fit() {
        let mut seen = std::collections::HashSet::new();
        for opt in TABLE {
            assert!(opt.key.len() <= 64, "{} too long", opt.key);
            assert!(seen.insert(opt.key), "{} twice", opt.key);
            assert!(
                opt.key
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-.".contains(&b))
            );
            match opt.kind {
                Kind::Float {
                    min,
                    max,
                    step,
                    def,
                } => {
                    assert!(
                        min < max && (min..=max).contains(&def) && step > 0.0 && step <= max - min
                    );
                }
                Kind::Int { min, max, def } => assert!(min < max && (min..=max).contains(&def)),
                Kind::Choice { opts, def, .. } => assert!(def < opts.len()),
                Kind::Bool(_) => {}
            }
        }
        assert_eq!(Values::default().0.len(), TABLE.len());
    }

    #[test]
    fn defaults_match_the_original() {
        let v = Values::default();
        assert_eq!(v.v3(K::OffsetX, K::OffsetY, K::OffsetZ), [-0.75, 0.0, 4.0]);
        assert!(v.b(K::DecoupledCamera));
        assert_eq!(
            CrosshairType::from_index(v.c(K::CrosshairType)),
            CrosshairType::Static
        );
        assert_eq!(
            TurningMode::from_index(v.c(K::TurnWhenAttacking)),
            TurningMode::RequiresTarget
        );
        assert_eq!(v.f(K::ClimbingMulX), 0.0);
    }

    #[test]
    fn registered_specs_pass_the_registrar_checks() {
        for opt in TABLE {
            let label = opt.label_key();
            let spec = match opt.kind {
                Kind::Bool(def) => SettingSpec::toggle(opt.key, label, def),
                Kind::Float {
                    min,
                    max,
                    step,
                    def,
                } => SettingSpec::range(opt.key, label, min, max, step, def),
                Kind::Int { min, max, def } => SettingSpec::range(
                    opt.key,
                    label,
                    f64::from(min),
                    f64::from(max),
                    1.0,
                    f64::from(def),
                ),
                Kind::Choice { group, opts, def } => SettingSpec::choice(
                    opt.key,
                    label,
                    opts.iter().map(|v| choice_label_key(group, v)),
                    def,
                ),
            };
            assert!(spec.check().is_ok(), "{}", opt.key);
        }
    }

    #[test]
    fn lists_round_trip() {
        let lists = Lists {
            offset_x_presets: vec![-0.75, 0.75],
            non_collidable_blocks: vec!["minecraft:.*leaves".to_owned(), "a\"b".to_owned()],
            ..Lists::default()
        };
        let parsed = Lists::parse(&lists.to_toml());
        assert_eq!(parsed, lists);
    }

    #[test]
    fn malformed_files_fall_back() {
        let parsed = Lists::parse(
            "garbage\nadaptive_crosshair_hold_items = 5\nadaptive_crosshair_use_items = [\"a\",\n \"b\"\n]\n",
        );
        assert_eq!(parsed.hold_items, Lists::default().hold_items);
        assert_eq!(parsed.use_items, vec!["a".to_owned(), "b".to_owned()]);
        assert_eq!(Lists::parse(""), Lists::default());
    }

    #[test]
    fn turning_modes() {
        assert!(TurningMode::Always.should_turn(false));
        assert!(!TurningMode::Never.should_turn(true));
        assert!(!TurningMode::RequiresTarget.should_turn(false));
        assert!(TurningMode::RequiresTarget.should_turn(true));
    }
}
