//! Pure data, wire messages, pose/alignment/scale/slider math and NBT helpers for Armor Statues.
use std::sync::atomic::{AtomicU64, Ordering};

use mod_api::{Compound, EquipmentSlot, Tag};

pub const MENU_ID: &str = "armorstatues:armor_stand";
pub const EDIT_CHANNEL: &str = "armorstatues:edit";
pub const SYNC_CHANNEL: &str = "armorstatues:sync";
pub const SCALE_BONUS_ID: &str = "statuemenus:scale_bonus";

/// `ArmorStandStatue.ALL_SLOTS_DISABLED` (`0b111111_00_111111_00_111111`).
pub const ALL_SLOTS_DISABLED: i32 = 0b0011_1111_0011_1111_0011_1111;

pub const DISABLE_ALL_OFFSET: u32 = 0;
pub const DISABLE_TAKING_OFFSET: u32 = 8;
pub const DISABLE_PUTTING_OFFSET: u32 = 16;

/// Vanilla `Attributes.SCALE` bounds (`RangedAttribute`: 0.0625..=16.0, default 1.0).
pub const MIN_SCALE: f32 = 0.0625;
pub const MAX_SCALE: f32 = 16.0;
pub const DEFAULT_SCALE: f32 = 1.0;

/// Logarithmic scale slider parameters (`StatuePositionScreen.ScaleWidget`).
pub const LOG_SCALE: f64 = 2.0;
pub const LOG_SCALE_POW: f64 = 0.01;

/// Snap interval for rotation sliders (`StatuePoses.DEGREES_SNAP_INTERVAL`).
pub const DEGREES_SNAP_INTERVAL: f64 = 0.125;

/// Keyboard step for rotation square sliders (`SquareSliderButton.VALUE_KEY_INTERVAL`).
pub const SQUARE_SLIDER_KEY_STEP: f64 = 0.035;

/// Position movement increments in blocks (`StatuePositionScreen.INCREMENTS`: 1, 4, 8, 16 pixels).
pub const MOVE_INCREMENTS: [f64; 4] = [0.0625, 0.25, 0.5, 1.0];

/// Max UTF-16 length of an armor stand custom name (`StatueStyleScreen.getNameMaxLength`).
pub const MAX_NAME_LEN: usize = 50;

/// Vanilla default armor stand pose (`ArmorStand.DEFAULT_*_POSE`).
/// Order: Head, Body, LeftArm, RightArm, LeftLeg, RightLeg.
pub const DEFAULT_POSE: [[f32; 3]; 6] = [
    [0.0, 0.0, 0.0],
    [0.0, 0.0, 0.0],
    [-10.0, 0.0, -10.0],
    [-15.0, 0.0, 10.0],
    [-1.0, 0.0, -1.0],
    [1.0, 0.0, 1.0],
];

/// NBT keys for the six pose parts, in `StandSnapshot::pose` order.
pub const POSE_NBT_KEYS: [&str; 6] = ["Head", "Body", "LeftArm", "RightArm", "LeftLeg", "RightLeg"];

/// Display order of the six equipment slots in `StatueMenu` (`slots 0..6`).
pub const MENU_EQUIPMENT_SLOTS: [EquipmentSlot; 6] = [
    EquipmentSlot::Head,
    EquipmentSlot::Chest,
    EquipmentSlot::Legs,
    EquipmentSlot::Feet,
    EquipmentSlot::MainHand,
    EquipmentSlot::OffHand,
];

/// `ENTITY_ITEMS` slot indices (`EquipmentSlot::ALL` ordinal) for `MENU_EQUIPMENT_SLOTS`.
pub const MENU_EQUIPMENT_ENTITY_INDICES: [usize; 6] = [5, 4, 3, 2, 0, 1];

/// Positions `[x, y]` of the 6 equipment slots on the 210x188 screen.
pub const EQUIPMENT_SLOT_POSITIONS: [[f32; 2]; 6] = [
    [58.0, 20.0],
    [58.0, 38.0],
    [58.0, 56.0],
    [58.0, 74.0],
    [136.0, 56.0],
    [136.0, 74.0],
];

/// Vanilla empty-slot sprites for the 6 equipment slots (`StatueMenu.TEXTURE_EMPTY_SLOTS`).
pub const EQUIPMENT_EMPTY_ICONS: [&str; 6] = [
    "minecraft:container/slot/helmet",
    "minecraft:container/slot/chestplate",
    "minecraft:container/slot/leggings",
    "minecraft:container/slot/boots",
    "minecraft:container/slot/sword",
    "minecraft:container/slot/shield",
];

/// `EquipmentSlot.getFilterBit(offset)` for armor stand `DisabledSlots`.
pub fn equipment_filter_bit(slot: EquipmentSlot, offset: u32) -> u32 {
    let base = match slot {
        EquipmentSlot::MainHand => 0,
        EquipmentSlot::Feet => 1,
        EquipmentSlot::Legs => 2,
        EquipmentSlot::Chest => 3,
        EquipmentSlot::Head => 4,
        EquipmentSlot::OffHand => 5,
        EquipmentSlot::Body => 6,
        EquipmentSlot::Saddle => 7,
    };
    base + offset
}

pub fn is_slot_disabled(disabled_slots: i32, slot: EquipmentSlot, offset: u32) -> bool {
    (disabled_slots & (1i32 << equipment_filter_bit(slot, offset))) != 0
}

pub fn is_slot_restricted(disabled_slots: i32, slot: EquipmentSlot) -> bool {
    is_slot_disabled(disabled_slots, slot, DISABLE_ALL_OFFSET)
        || is_slot_disabled(disabled_slots, slot, DISABLE_TAKING_OFFSET)
        || is_slot_disabled(disabled_slots, slot, DISABLE_PUTTING_OFFSET)
}

/// Screen tabs in Armor Statues' order (`ArmorStandScreenTypes.TYPES`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Tab {
    #[default]
    Rotations,
    Poses,
    Style,
    Position,
    Alignments,
    Equipment,
}

impl Tab {
    pub const ALL: [Self; 6] = [
        Self::Rotations,
        Self::Poses,
        Self::Style,
        Self::Position,
        Self::Alignments,
        Self::Equipment,
    ];

    pub fn translation_key(self) -> &'static str {
        match self {
            Self::Rotations => "screen.statuemenus.type.statuemenus.rotations",
            Self::Poses => "screen.statuemenus.type.statuemenus.poses",
            Self::Style => "screen.statuemenus.type.statuemenus.style",
            Self::Position => "screen.statuemenus.type.armorstatues.position",
            Self::Alignments => "screen.statuemenus.type.armorstatues.alignments",
            Self::Equipment => "screen.statuemenus.type.statuemenus.equipment",
        }
    }

    pub fn icon_item(self) -> &'static str {
        match self {
            Self::Rotations => "minecraft:compass",
            Self::Poses => "minecraft:spyglass",
            Self::Style => "minecraft:painting",
            Self::Position => "minecraft:grass_block",
            Self::Alignments => "minecraft:diamond_pickaxe",
            Self::Equipment => "minecraft:chest",
        }
    }

    pub fn has_close_button(self) -> bool {
        matches!(self, Self::Equipment | Self::Style | Self::Alignments)
    }

    pub fn cycle(self, backwards: bool) -> Self {
        let idx = Self::ALL.iter().position(|&t| t == self).unwrap_or(0);
        let len = Self::ALL.len();
        let next = if backwards {
            (idx + len - 1) % len
        } else {
            (idx + 1) % len
        };
        Self::ALL[next]
    }
}

/// Style options (`StatueStyleOption`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, mod_api::Wire)]
pub enum StyleOption {
    ShowName,
    ShowArms,
    Small,
    Invisible,
    NoBasePlate,
    Immovable,
    Invulnerable,
    Sealed,
}

impl StyleOption {
    /// The 7 checkboxes shown on the Style tab (`StatueStyleOption.TYPES`).
    pub const STYLE_TAB: [Self; 7] = [
        Self::ShowArms,
        Self::Small,
        Self::Invisible,
        Self::NoBasePlate,
        Self::Immovable,
        Self::Invulnerable,
        Self::Sealed,
    ];

    pub fn translation_key(self) -> &'static str {
        match self {
            Self::ShowName => "screen.statuemenus.style.statuemenus.show_name",
            Self::ShowArms => "screen.statuemenus.style.statuemenus.show_arms",
            Self::Small => "screen.statuemenus.style.statuemenus.small",
            Self::Invisible => "screen.statuemenus.style.statuemenus.invisible",
            Self::NoBasePlate => "screen.statuemenus.style.statuemenus.no_base_plate",
            Self::Immovable => "screen.statuemenus.style.statuemenus.immovable",
            Self::Invulnerable => "screen.statuemenus.style.statuemenus.invulnerable",
            Self::Sealed => "screen.statuemenus.style.statuemenus.sealed",
        }
    }

    pub fn description_key(self) -> &'static str {
        match self {
            Self::ShowName => "screen.statuemenus.style.statuemenus.show_name.description",
            Self::ShowArms => "screen.statuemenus.style.statuemenus.show_arms.description",
            Self::Small => "screen.statuemenus.style.statuemenus.small.description",
            Self::Invisible => "screen.statuemenus.style.statuemenus.invisible.description",
            Self::NoBasePlate => "screen.statuemenus.style.statuemenus.no_base_plate.description",
            Self::Immovable => "screen.statuemenus.style.statuemenus.immovable.description",
            Self::Invulnerable => "screen.statuemenus.style.statuemenus.invulnerable.description",
            Self::Sealed => "screen.statuemenus.style.statuemenus.sealed.description",
        }
    }

    pub fn creative_only(self) -> bool {
        matches!(self, Self::Sealed)
    }

    pub fn get(self, stand: &StandSnapshot) -> bool {
        match self {
            Self::ShowName => stand.custom_name_visible,
            Self::ShowArms => stand.show_arms,
            Self::Small => stand.small,
            Self::Invisible => stand.invisible,
            Self::NoBasePlate => stand.no_base_plate,
            Self::Immovable => stand.no_gravity,
            Self::Invulnerable => stand.invulnerable,
            Self::Sealed => stand.disabled_slots == ALL_SLOTS_DISABLED,
        }
    }

    pub fn set(self, stand: &mut StandSnapshot, value: bool) {
        match self {
            Self::ShowName => stand.custom_name_visible = value,
            Self::ShowArms => stand.show_arms = value,
            Self::Small => stand.small = value,
            Self::Invisible => stand.invisible = value,
            Self::NoBasePlate => stand.no_base_plate = value,
            Self::Immovable => stand.no_gravity = value,
            Self::Invulnerable => stand.invulnerable = value,
            Self::Sealed => {
                stand.disabled_slots = if value { ALL_SLOTS_DISABLED } else { 0 };
            }
        }
    }

    pub fn to_nbt(self, value: bool) -> Compound {
        let mut c = Compound::new();
        match self {
            Self::ShowName => {
                c.insert("CustomNameVisible".into(), Tag::Byte(i8::from(value)));
            }
            Self::ShowArms => {
                c.insert("ShowArms".into(), Tag::Byte(i8::from(value)));
            }
            Self::Small => {
                c.insert("Small".into(), Tag::Byte(i8::from(value)));
            }
            Self::Invisible => {
                c.insert("Invisible".into(), Tag::Byte(i8::from(value)));
            }
            Self::NoBasePlate => {
                c.insert("NoBasePlate".into(), Tag::Byte(i8::from(value)));
            }
            Self::Immovable => {
                c.insert("NoGravity".into(), Tag::Byte(i8::from(value)));
            }
            Self::Invulnerable => {
                c.insert("Invulnerable".into(), Tag::Byte(i8::from(value)));
            }
            Self::Sealed => {
                c.insert(
                    "DisabledSlots".into(),
                    Tag::Int(if value { ALL_SLOTS_DISABLED } else { 0 }),
                );
            }
        }
        c
    }
}

/// `PosePartMutator` entry in `PosePartMutator.TYPES` display order (2-column Rotations grid).
#[derive(Clone, Copy, Debug)]
pub struct PartMutator {
    /// Index in `StandSnapshot::pose` (`[Head, Body, LeftArm, RightArm, LeftLeg, RightLeg]`).
    pub pose_index: usize,
    pub translation_key: &'static str,
    pub is_arm: bool,
    /// Clamped axis ranges `[X, Y, Z]`.
    pub ranges: [(f64, f64); 3],
}

/// `PosePartMutator.TYPES`: Head, Body, RightArm, LeftArm, RightLeg, LeftLeg.
pub const MUTATORS: [PartMutator; 6] = [
    PartMutator {
        pose_index: 0,
        translation_key: "screen.statuemenus.rotations.pose.statuemenus.head",
        is_arm: false,
        ranges: [(-60.0, 60.0), (-60.0, 60.0), (-120.0, 120.0)],
    },
    PartMutator {
        pose_index: 1,
        translation_key: "screen.statuemenus.rotations.pose.statuemenus.body",
        is_arm: false,
        ranges: [(-30.0, 30.0), (-30.0, 30.0), (-120.0, 120.0)],
    },
    PartMutator {
        pose_index: 3,
        translation_key: "screen.statuemenus.rotations.pose.statuemenus.right_arm",
        is_arm: true,
        ranges: [(-180.0, 0.0), (-90.0, 45.0), (-120.0, 120.0)],
    },
    PartMutator {
        pose_index: 2,
        translation_key: "screen.statuemenus.rotations.pose.statuemenus.left_arm",
        is_arm: true,
        ranges: [(-180.0, 0.0), (-45.0, 90.0), (-120.0, 120.0)],
    },
    PartMutator {
        pose_index: 5,
        translation_key: "screen.statuemenus.rotations.pose.statuemenus.right_leg",
        is_arm: false,
        ranges: [(-120.0, 120.0), (-90.0, 0.0), (-120.0, 120.0)],
    },
    PartMutator {
        pose_index: 4,
        translation_key: "screen.statuemenus.rotations.pose.statuemenus.left_leg",
        is_arm: false,
        ranges: [(-120.0, 120.0), (0.0, 90.0), (-120.0, 120.0)],
    },
];

/// `Mth.wrapDegrees(float)` in `[-180, 180)`.
pub fn wrap_degrees_f32(value: f32) -> f32 {
    let mut f = value % 360.0;
    if f >= 180.0 {
        f -= 360.0;
    }
    if f < -180.0 {
        f += 360.0;
    }
    if f == -0.0 { 0.0 } else { f }
}

/// `Mth.wrapDegrees(double)` in `[-180, 180)`.
pub fn wrap_degrees_f64(value: f64) -> f64 {
    let mut d = value % 360.0;
    if d >= 180.0 {
        d -= 360.0;
    }
    if d < -180.0 {
        d += 360.0;
    }
    if d == -0.0 { 0.0 } else { d }
}

impl PartMutator {
    fn range(self, axis: usize, clamp: bool) -> (f64, f64) {
        if clamp {
            self.ranges[axis]
        } else {
            (-180.0, 180.0)
        }
    }

    /// `PosePartMutator.invertAtAxis`: Y axis is inverted for display/edit.
    fn invert_axis(axis: usize, value: f32) -> f32 {
        wrap_degrees_f32(if axis == 1 { -value } else { value })
    }

    /// Rotation displayed for `axis` (`0=X, 1=Y, 2=Z`), with Y inverted.
    pub fn display_axis(self, pose: &[[f32; 3]; 6], axis: usize) -> f64 {
        f64::from(Self::invert_axis(axis, pose[self.pose_index][axis]))
    }

    /// Normalized `[0.0, 1.0]` slider value for `axis`.
    pub fn normalized_axis(self, pose: &[[f32; 3]; 6], axis: usize, clamp: bool) -> f64 {
        let (min, max) = self.range(axis, clamp);
        let v = self.display_axis(pose, axis).clamp(min, max);
        (v - min) / (max - min)
    }

    /// Sets `axis` on `pose` from normalized `[0.0, 1.0]` slider value.
    pub fn set_normalized_axis(
        self,
        pose: &mut [[f32; 3]; 6],
        axis: usize,
        norm: f64,
        clamp: bool,
    ) {
        let (min, max) = self.range(axis, clamp);
        let expanded = (norm.clamp(0.0, 1.0) * (max - min) + min) as f32;
        pose[self.pose_index][axis] = Self::invert_axis(axis, expanded);
    }

    pub fn axis_translation_key(axis: usize) -> &'static str {
        match axis {
            0 => "screen.statuemenus.rotations.x",
            1 => "screen.statuemenus.rotations.y",
            _ => "screen.statuemenus.rotations.z",
        }
    }
}

/// `StatuePoses.snapValue`: snaps `value` to multiples of `interval` within `0.1 * interval`.
pub fn snap_value(value: f64, interval: f64) -> f64 {
    if interval > 0.0 && interval < 1.0 {
        let mut snapped = 0.0;
        let region = interval * 0.1;
        while snapped < 1.0 {
            if value >= snapped - region && value < snapped + region {
                return snapped;
            }
            snapped += interval;
        }
    }
    value
}

/// `StatuePositionScreen.ScaleWidget.fromLogarithmicValue`.
pub fn from_log_scale(scale: f64) -> f64 {
    let inv = (scale - f64::from(MIN_SCALE)) / f64::from(MAX_SCALE - MIN_SCALE);
    ((inv.max(0.0) + LOG_SCALE_POW).log10() + LOG_SCALE) / LOG_SCALE
}

/// `StatuePositionScreen.ScaleWidget.toLogarithmicValue`.
pub fn to_log_scale(norm: f64) -> f32 {
    let t = 10.0_f64.powf(norm * LOG_SCALE - LOG_SCALE) - LOG_SCALE_POW;
    let raw = f64::from(MIN_SCALE) + t * f64::from(MAX_SCALE - MIN_SCALE);
    (raw as f32).clamp(MIN_SCALE, MAX_SCALE)
}

/// `StatuePositionScreen.SliderWidget.fromWrappedDegrees`.
pub fn from_wrapped_degrees(deg: f64) -> f64 {
    (wrap_degrees_f64(deg) + 180.0) / 360.0
}

/// `StatuePositionScreen.SliderWidget.toWrappedDegrees`.
pub fn to_wrapped_degrees(norm: f64) -> f32 {
    wrap_degrees_f64(norm * 360.0 - 180.0) as f32
}

/// Round to 1/16 block and 4 decimal places (`PositionComponentWidget.roundWithPrecision`).
pub fn round_sixteenth(v: f64) -> f64 {
    let s = (v * 16.0).round() / 16.0;
    let r = (s * 10_000.0).round() / 10_000.0;
    if r == -0.0 { 0.0 } else { r }
}

/// Format like Java `DecimalFormat("#.##")` (Locale.ROOT).
pub fn format_two_decimals(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    let r = if r == -0.0 { 0.0 } else { r };
    let s = format!("{r:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// Format like Java `DecimalFormat("#.####")` (Locale.ROOT).
pub fn format_four_decimals(v: f64) -> String {
    let r = (v * 10_000.0).round() / 10_000.0;
    let r = if r == -0.0 { 0.0 } else { r };
    let s = format!("{r:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// `StatuePoseImpl.mirror`: swaps left/right arms and legs, negates Y and Z on all parts.
pub fn mirror_pose(pose: &[[f32; 3]; 6]) -> [[f32; 3]; 6] {
    let m = |r: [f32; 3]| [r[0], wrap_degrees_f32(-r[1]), wrap_degrees_f32(-r[2])];
    [
        m(pose[0]),
        m(pose[1]),
        m(pose[3]),
        m(pose[2]),
        m(pose[5]),
        m(pose[4]),
    ]
}

static RNG_STATE: AtomicU64 = AtomicU64::new(0);

fn next_unit_f64() -> f64 {
    let mut s = RNG_STATE.load(Ordering::Relaxed);
    if s == 0 {
        s = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x9E37_79B9_7F4A_7C15, |d| d.as_nanos() as u64)
            | 1;
    }
    s ^= s << 13;
    s ^= s >> 7;
    s ^= s << 17;
    RNG_STATE.store(s, Ordering::Relaxed);
    ((s >> 11) as f64) / ((1u64 << 53) as f64)
}

/// `StatueRotationsScreen` randomize: each part random in range, Z = 0 when clamped, body = 0.
pub fn random_pose(clamp: bool) -> [[f32; 3]; 6] {
    let mut pose = [[0.0_f32; 3]; 6];
    for m in MUTATORS {
        if m.pose_index == 1 {
            // Body is reset to (0, 0, 0) by `setupRandomPose`.
            pose[1] = [0.0, 0.0, 0.0];
            continue;
        }
        for axis in 0..3 {
            if clamp && axis == 2 {
                pose[m.pose_index][2] = 0.0;
            } else {
                let (lo, hi) = m.range(axis, clamp);
                pose[m.pose_index][axis] = (lo + next_unit_f64() * (hi - lo)) as f32;
            }
        }
    }
    pose
}

/// Preset pose entry (`StatuePoses.VALUES`: 29 poses).
pub struct PresetPose {
    pub translation_key: &'static str,
    pub source_name: &'static str,
    /// `[Head, Body, LeftArm, RightArm, LeftLeg, RightLeg]`.
    pub pose: [[f32; 3]; 6],
}

pub const POSES: &[PresetPose] = &[
    PresetPose {
        translation_key: "screen.statuemenus.poses.minecraft.default",
        source_name: "Minecraft",
        pose: [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [-10.0, 0.0, -10.0],
            [-15.0, 0.0, 10.0],
            [-1.0, 0.0, -1.0],
            [1.0, 0.0, 1.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.minecraft.solemn",
        source_name: "Minecraft",
        pose: [
            [15.0, 0.0, 0.0],
            [0.0, 0.0, 2.0],
            [-30.0, 15.0, 15.0],
            [-60.0, -20.0, -10.0],
            [-1.0, 0.0, -1.0],
            [1.0, 0.0, 1.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.minecraft.athena",
        source_name: "Minecraft",
        pose: [
            [-5.0, 0.0, 0.0],
            [0.0, 0.0, 2.0],
            [10.0, 0.0, -5.0],
            [-60.0, 20.0, -10.0],
            [-3.0, -3.0, -3.0],
            [3.0, 3.0, 3.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.minecraft.brandish",
        source_name: "Minecraft",
        pose: [
            [-15.0, 0.0, 0.0],
            [0.0, 0.0, -2.0],
            [20.0, 0.0, -10.0],
            [-110.0, 50.0, 0.0],
            [5.0, -3.0, -3.0],
            [-5.0, 3.0, 3.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.minecraft.honor",
        source_name: "Minecraft",
        pose: [
            [-15.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [-110.0, 35.0, 0.0],
            [-110.0, -35.0, 0.0],
            [5.0, -3.0, -3.0],
            [-5.0, 3.0, 3.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.minecraft.entertain",
        source_name: "Minecraft",
        pose: [
            [-15.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [-110.0, -35.0, 0.0],
            [-110.0, 35.0, 0.0],
            [5.0, -3.0, -3.0],
            [-5.0, 3.0, 3.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.minecraft.salute",
        source_name: "Minecraft",
        pose: [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [10.0, 0.0, -5.0],
            [-70.0, -40.0, 0.0],
            [-1.0, 0.0, -1.0],
            [1.0, 0.0, 1.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.minecraft.hero",
        source_name: "Minecraft",
        pose: [
            [-4.0, 67.0, 0.0],
            [0.0, 8.0, 0.0],
            [16.0, 32.0, -8.0],
            [-99.0, 63.0, 0.0],
            [0.0, -75.0, -8.0],
            [4.0, 63.0, 8.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.minecraft.riposte",
        source_name: "Minecraft",
        pose: [
            [16.0, 20.0, 0.0],
            [0.0, 0.0, 0.0],
            [4.0, 8.0, 237.0],
            [246.0, 0.0, 89.0],
            [-14.0, -18.0, -16.0],
            [8.0, 20.0, 4.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.minecraft.zombie",
        source_name: "Minecraft",
        pose: [
            [-10.0, 0.0, -5.0],
            [0.0, 0.0, 0.0],
            [-105.0, 0.0, 0.0],
            [-100.0, 0.0, 0.0],
            [7.0, 0.0, 0.0],
            [-46.0, 0.0, 0.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.minecraft.cancan",
        source_name: "Minecraft",
        pose: [
            [-5.0, 18.0, 0.0],
            [0.0, 22.0, 0.0],
            [8.0, 0.0, -114.0],
            [0.0, 84.0, 111.0],
            [-111.0, 55.0, 0.0],
            [0.0, 23.0, -13.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.vanillatweaks.walking",
        source_name: "Vanilla Tweaks",
        pose: [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [-20.0, 0.0, -10.0],
            [20.0, 0.0, 10.0],
            [20.0, 0.0, 0.0],
            [-20.0, 0.0, 0.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.vanillatweaks.running",
        source_name: "Vanilla Tweaks",
        pose: [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [40.0, 0.0, -10.0],
            [-40.0, 0.0, 10.0],
            [-40.0, 0.0, 0.0],
            [40.0, 0.0, 0.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.vanillatweaks.pointing",
        source_name: "Vanilla Tweaks",
        pose: [
            [0.0, 20.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, -10.0],
            [-90.0, 18.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.vanillatweaks.blocking",
        source_name: "Vanilla Tweaks",
        pose: [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [-50.0, 50.0, 0.0],
            [-20.0, -20.0, 0.0],
            [20.0, 0.0, 0.0],
            [-20.0, 0.0, 0.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.vanillatweaks.lungeing",
        source_name: "Vanilla Tweaks",
        pose: [
            [0.0, 0.0, 0.0],
            [15.0, 0.0, 0.0],
            [10.0, 0.0, -10.0],
            [-60.0, -10.0, 0.0],
            [30.0, 0.0, 0.0],
            [-15.0, 0.0, 0.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.vanillatweaks.winning",
        source_name: "Vanilla Tweaks",
        pose: [
            [-15.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [10.0, 0.0, -10.0],
            [-120.0, -10.0, 0.0],
            [15.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.vanillatweaks.sitting",
        source_name: "Vanilla Tweaks",
        pose: [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [-80.0, -20.0, 0.0],
            [-80.0, 20.0, 0.0],
            [-90.0, -10.0, 0.0],
            [-90.0, 10.0, 0.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.vanillatweaks.arabesque",
        source_name: "Vanilla Tweaks",
        pose: [
            [-15.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [70.0, 0.0, -10.0],
            [-140.0, -10.0, 0.0],
            [75.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.vanillatweaks.cupid",
        source_name: "Vanilla Tweaks",
        pose: [
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [-75.0, 0.0, 10.0],
            [-90.0, -10.0, 0.0],
            [75.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.vanillatweaks.confident",
        source_name: "Vanilla Tweaks",
        pose: [
            [-10.0, 20.0, 0.0],
            [-2.0, 0.0, 0.0],
            [5.0, 0.0, 0.0],
            [5.0, 0.0, 0.0],
            [0.0, -10.0, -4.0],
            [16.0, 2.0, 10.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.vanillatweaks.death",
        source_name: "Vanilla Tweaks",
        pose: [
            [-85.0, 0.0, 0.0],
            [-90.0, 0.0, 0.0],
            [-90.0, -10.0, 0.0],
            [-90.0, 10.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.vanillatweaks.facepalm",
        source_name: "Vanilla Tweaks",
        pose: [
            [45.0, -4.0, 1.0],
            [10.0, 0.0, 0.0],
            [-72.0, 24.0, 47.0],
            [18.0, -14.0, 0.0],
            [-4.0, -6.0, -2.0],
            [25.0, -2.0, 0.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.vanillatweaks.lazing",
        source_name: "Vanilla Tweaks",
        pose: [
            [14.0, -12.0, 6.0],
            [5.0, 0.0, 0.0],
            [-4.0, -20.0, -10.0],
            [-40.0, 20.0, 0.0],
            [-88.0, 46.0, 0.0],
            [-88.0, 71.0, 0.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.vanillatweaks.confused",
        source_name: "Vanilla Tweaks",
        pose: [
            [0.0, 30.0, 0.0],
            [0.0, 13.0, 0.0],
            [145.0, 22.0, -49.0],
            [-22.0, 31.0, 10.0],
            [-6.0, 0.0, 0.0],
            [6.0, -20.0, 0.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.vanillatweaks.formal",
        source_name: "Vanilla Tweaks",
        pose: [
            [4.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [30.0, -20.0, 21.0],
            [30.0, 22.0, -20.0],
            [0.0, 0.0, -5.0],
            [0.0, 0.0, 5.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.vanillatweaks.sad",
        source_name: "Vanilla Tweaks",
        pose: [
            [63.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [-5.0, 0.0, -5.0],
            [-5.0, 0.0, 5.0],
            [-5.0, 16.0, -5.0],
            [-5.0, -10.0, 5.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.vanillatweaks.joyous",
        source_name: "Vanilla Tweaks",
        pose: [
            [-11.0, 0.0, 0.0],
            [-4.0, 0.0, 0.0],
            [0.0, 0.0, -100.0],
            [0.0, 0.0, 100.0],
            [-8.0, 0.0, -60.0],
            [-8.0, 0.0, 60.0],
        ],
    },
    PresetPose {
        translation_key: "screen.statuemenus.poses.vanillatweaks.stargazing",
        source_name: "Vanilla Tweaks",
        pose: [
            [-22.0, 25.0, 0.0],
            [-4.0, 10.0, 0.0],
            [4.0, 18.0, 0.0],
            [-153.0, 34.0, -3.0],
            [6.0, 24.0, 0.0],
            [-4.0, 17.0, 2.0],
        ],
    },
];

/// Held-item surface alignments (`StatueAlignment`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    Block,
    FloatingItem,
    FlatItem,
    Tool,
}

impl Alignment {
    pub const ALL: [Self; 4] = [Self::Block, Self::FloatingItem, Self::FlatItem, Self::Tool];

    pub fn translation_key(self) -> &'static str {
        match self {
            Self::Block => "screen.statuemenus.alignments.statuemenus.block",
            Self::FloatingItem => "screen.statuemenus.alignments.statuemenus.floating_item",
            Self::FlatItem => "screen.statuemenus.alignments.statuemenus.flat_item",
            Self::Tool => "screen.statuemenus.alignments.statuemenus.tool",
        }
    }

    pub fn description_key(self) -> &'static str {
        match self {
            Self::Block => "screen.statuemenus.alignments.statuemenus.block.description",
            Self::FloatingItem => {
                "screen.statuemenus.alignments.statuemenus.floating_item.description"
            }
            Self::FlatItem => "screen.statuemenus.alignments.statuemenus.flat_item.description",
            Self::Tool => "screen.statuemenus.alignments.statuemenus.tool.description",
        }
    }

    pub fn right_arm_pose(self) -> [f32; 3] {
        match self {
            Self::Block => [-15.0, 135.0, 0.0],
            Self::FloatingItem => [-90.0, 0.0, 0.0],
            Self::FlatItem => [0.0, 0.0, 0.0],
            Self::Tool => [-10.0, 0.0, -90.0],
        }
    }

    /// `StatueAlignment.getPose()` serialized through `ArmorStandPose.CODEC`: default pose with
    /// right arm replaced by `right_arm_pose()`.
    pub fn pose(self) -> [[f32; 3]; 6] {
        let mut p = DEFAULT_POSE;
        p[3] = self.right_arm_pose();
        p
    }

    pub fn offset(self, small: bool) -> [f64; 3] {
        match (self, small) {
            (Self::Block, false) => [0.5725, -0.655, 0.352],
            (Self::Block, true) => [0.28625, -0.3275, 0.176],
            (Self::FloatingItem, false) => [0.36, -1.41, -0.5625],
            (Self::FloatingItem, true) => [0.18, -0.705, -0.28125],
            (Self::FlatItem, false) => [0.385, -0.78, -0.295],
            (Self::FlatItem, true) => [0.1925, -0.39, -0.1475],
            (Self::Tool, false) => [-0.17, -1.285, -0.44],
            (Self::Tool, true) => [-0.085, -0.6425, -0.22],
        }
    }
}

/// `DataSyncHandler.getLocalPosition` (`LocalCoordinates.getPosition`):
/// `offset` is `[left, up, forward]` relative to the entity's `(yaw, pitch)`.
pub fn local_position(pos: [f64; 3], yaw: f32, pitch: f32, offset: [f64; 3]) -> [f64; 3] {
    let rad = std::f32::consts::PI / 180.0;
    let f = ((yaw + 90.0) * rad).cos();
    let g = ((yaw + 90.0) * rad).sin();
    let h = (-pitch * rad).cos();
    let i = (-pitch * rad).sin();
    let j = ((-pitch + 90.0) * rad).cos();
    let k = ((-pitch + 90.0) * rad).sin();
    let forward = [f64::from(f * h), f64::from(i), f64::from(g * h)];
    let up = [f64::from(f * j), f64::from(k), f64::from(g * j)];
    // cross(forward, up) * -1
    let left = [
        -(forward[1] * up[2] - forward[2] * up[1]),
        -(forward[2] * up[0] - forward[0] * up[2]),
        -(forward[0] * up[1] - forward[1] * up[0]),
    ];
    [
        pos[0] + forward[0] * offset[2] + up[0] * offset[1] + left[0] * offset[0],
        pos[1] + forward[1] * offset[2] + up[1] * offset[1] + left[1] * offset[0],
        pos[2] + forward[2] * offset[2] + up[2] * offset[1] + left[2] * offset[0],
    ]
}

/// Stand snapshot sent from server to client in `ModMenuOpen::extra`.
#[derive(Clone, Debug, PartialEq, mod_api::Wire)]
pub struct StandSnapshot {
    pub entity_id: u32,
    pub pose: [[f32; 3]; 6],
    pub show_arms: bool,
    pub small: bool,
    pub invisible: bool,
    pub no_base_plate: bool,
    pub no_gravity: bool,
    pub invulnerable: bool,
    pub disabled_slots: i32,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub scale: f32,
    pub custom_name: String,
    pub custom_name_visible: bool,
    pub creative: bool,
    /// Optional saved `equipment` compound from the armor stand, for client previews.
    pub equipment: Option<Tag>,
}

impl StandSnapshot {
    pub fn from_nbt(
        entity_id: u32,
        nbt: &Compound,
        pos: [f64; 3],
        rot: [f32; 2],
        scale: f32,
        creative: bool,
    ) -> Self {
        let mut pose = DEFAULT_POSE;
        if let Some(Tag::Compound(pose_tag)) = nbt.get(&"Pose".into()) {
            for (i, key) in POSE_NBT_KEYS.iter().enumerate() {
                if let Some(Tag::List(list)) = pose_tag.get(&(*key).into())
                    && list.len() == 3
                {
                    for a in 0..3 {
                        if let Some(v) = nbt_float(&list[a]) {
                            pose[i][a] = v;
                        }
                    }
                }
            }
        }
        let bool_field = |key: &str| matches!(nbt.get(&key.into()), Some(Tag::Byte(b)) if *b != 0);
        let disabled_slots = match nbt.get(&"DisabledSlots".into()) {
            Some(Tag::Int(i)) => *i,
            _ => 0,
        };
        let custom_name = match nbt.get(&"CustomName".into()) {
            Some(Tag::String(s)) => s.to_string_lossy(),
            Some(Tag::Compound(c)) => match c.get(&"text".into()) {
                Some(Tag::String(s)) => s.to_string_lossy(),
                _ => String::new(),
            },
            _ => String::new(),
        };
        let equipment = nbt.get(&"equipment".into()).cloned();
        Self {
            entity_id,
            pose,
            show_arms: bool_field("ShowArms"),
            small: bool_field("Small"),
            invisible: bool_field("Invisible"),
            no_base_plate: bool_field("NoBasePlate"),
            no_gravity: bool_field("NoGravity"),
            invulnerable: bool_field("Invulnerable"),
            disabled_slots,
            pos,
            yaw: rot[0],
            pitch: rot[1],
            scale: scale.clamp(MIN_SCALE, MAX_SCALE),
            custom_name,
            custom_name_visible: bool_field("CustomNameVisible"),
            creative,
            equipment,
        }
    }

    /// Build NBT compound for `GuiEntity::with_data("minecraft:armor_stand", ...)` previews.
    pub fn preview_nbt(&self, pose_override: Option<&[[f32; 3]; 6]>) -> Compound {
        let mut c = Compound::new();
        c.insert(
            "Pose".into(),
            Tag::Compound(pose_inner_compound(pose_override.unwrap_or(&self.pose))),
        );
        c.insert("ShowArms".into(), Tag::Byte(i8::from(self.show_arms)));
        c.insert("Small".into(), Tag::Byte(i8::from(self.small)));
        c.insert("Invisible".into(), Tag::Byte(i8::from(self.invisible)));
        c.insert(
            "NoBasePlate".into(),
            Tag::Byte(i8::from(self.no_base_plate)),
        );
        if !self.custom_name.is_empty() {
            c.insert(
                "CustomName".into(),
                Tag::String(self.custom_name.as_str().into()),
            );
            c.insert(
                "CustomNameVisible".into(),
                Tag::Byte(i8::from(self.custom_name_visible)),
            );
        }
        if let Some(eq) = &self.equipment {
            c.insert("equipment".into(), eq.clone());
        }
        c
    }
}

fn nbt_float(t: &Tag) -> Option<f32> {
    match t {
        Tag::Float(b) => Some(f32::from_bits(*b)),
        Tag::Double(b) => Some(f64::from_bits(*b) as f32),
        Tag::Int(i) => Some(*i as f32),
        _ => None,
    }
}

fn pose_inner_compound(pose: &[[f32; 3]; 6]) -> Compound {
    let mut inner = Compound::new();
    for (key, vals) in POSE_NBT_KEYS.iter().zip(pose) {
        let list = Tag::List(vals.iter().map(|v| Tag::Float(v.to_bits())).collect());
        inner.insert((*key).into(), list);
    }
    inner
}

pub fn pose_nbt(pose: &[[f32; 3]; 6]) -> Compound {
    let mut c = Compound::new();
    c.insert("Pose".into(), Tag::Compound(pose_inner_compound(pose)));
    c
}

/// Serverbound typed edit message (`armorstatues:edit`).
#[derive(Clone, Debug, PartialEq, mod_api::Wire)]
pub enum EditMessage {
    Name(String),
    Style { option: StyleOption, value: bool },
    Pose([[f32; 3]; 6]),
    Position([f64; 3]),
    Scale(f32),
    Rotation(f32),
}

/// Clientbound typed sync message (`armorstatues:sync`), sent when server-side actions change stand
/// flags or equipment while the menu is open (such as placing a hand item turning `ShowArms` on).
#[derive(Clone, Debug, PartialEq, mod_api::Wire)]
pub enum SyncMessage {
    ShowArms(bool),
    Equipment(Option<Tag>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_slot_bits_match_all_slots_disabled() {
        let mut combined = 0i32;
        for slot in MENU_EQUIPMENT_SLOTS {
            for offset in [
                DISABLE_ALL_OFFSET,
                DISABLE_TAKING_OFFSET,
                DISABLE_PUTTING_OFFSET,
            ] {
                combined |= 1i32 << equipment_filter_bit(slot, offset);
            }
        }
        assert_eq!(combined, ALL_SLOTS_DISABLED);
        assert!(is_slot_disabled(
            1 << 4,
            EquipmentSlot::Head,
            DISABLE_ALL_OFFSET
        ));
        assert!(is_slot_disabled(
            1 << (5 + 8),
            EquipmentSlot::OffHand,
            DISABLE_TAKING_OFFSET
        ));
        assert!(is_slot_disabled(
            1 << (1 + 16),
            EquipmentSlot::Feet,
            DISABLE_PUTTING_OFFSET
        ));
        assert!(!is_slot_restricted(1 << 4, EquipmentSlot::Chest));
    }

    #[test]
    fn logarithmic_scale_round_trips() {
        assert!((from_log_scale(f64::from(MIN_SCALE)) - 0.0).abs() < 1e-12);
        for scale in [MIN_SCALE, 0.25, 1.0, 2.5, 8.0, MAX_SCALE] {
            let n = from_log_scale(f64::from(scale));
            assert!((0.0..=1.01).contains(&n), "scale {scale} -> {n}");
            let back = to_log_scale(n);
            assert!(
                (back - scale).abs() < 1e-4,
                "scale {scale} -> {n} -> {back}"
            );
        }
        // Slider clamps normalized values to [0, 1]; check end values remain valid scales.
        assert!((to_log_scale(0.0) - MIN_SCALE).abs() < 1e-4);
        assert!(to_log_scale(1.0) <= MAX_SCALE && to_log_scale(1.0) > 15.0);
    }

    #[test]
    fn rotation_wrapping_and_snapping() {
        assert_eq!(from_wrapped_degrees(0.0), 0.5);
        assert_eq!(to_wrapped_degrees(0.5), 0.0);
        assert_eq!(to_wrapped_degrees(0.0), -180.0);
        assert_eq!(snap_value(0.505, DEGREES_SNAP_INTERVAL), 0.5);
        assert_eq!(snap_value(0.55, DEGREES_SNAP_INTERVAL), 0.55);
        assert_eq!(format_two_decimals(-0.0), "0");
        assert_eq!(format_two_decimals(12.50), "12.5");
        assert_eq!(format_four_decimals(0.0625), "0.0625");
        assert_eq!(format_four_decimals(1.0), "1");
        assert_eq!(round_sixteenth(1.0624), 1.0625);
    }

    #[test]
    fn pose_mutator_inverts_y_and_clamps() {
        let head = MUTATORS[0];
        let mut pose = DEFAULT_POSE;
        head.set_normalized_axis(&mut pose, 1, 1.0, true);
        // Max Y for head is +60 displayed -> -60 stored.
        assert!((pose[0][1] - (-60.0)).abs() < 1e-4);
        assert!((head.display_axis(&pose, 1) - 60.0).abs() < 1e-4);
        assert!((head.normalized_axis(&pose, 1, true) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn mirror_pose_swaps_sides_and_negates_yz() {
        let p = [
            [10.0, 20.0, 30.0],
            [1.0, -2.0, 3.0],
            [11.0, 12.0, 13.0],
            [21.0, 22.0, 23.0],
            [31.0, 32.0, 33.0],
            [41.0, 42.0, 43.0],
        ];
        let m = mirror_pose(&p);
        assert_eq!(m[0], [10.0, -20.0, -30.0]);
        assert_eq!(m[1], [1.0, 2.0, -3.0]);
        assert_eq!(m[2], [21.0, -22.0, -23.0]);
        assert_eq!(m[3], [11.0, -12.0, -13.0]);
        assert_eq!(m[4], [41.0, -42.0, -43.0]);
        assert_eq!(m[5], [31.0, -32.0, -33.0]);
        assert_eq!(mirror_pose(&m), p);
    }

    #[test]
    fn local_position_matches_vanilla_local_coordinates() {
        // Yaw 0 faces +Z, so forward (+z) moves +Z, left (+x) moves +X, up (+y) moves +Y.
        let p = local_position([10.0, 64.0, 20.0], 0.0, 0.0, [1.0, 2.0, 3.0]);
        assert!((p[0] - 11.0).abs() < 1e-5, "{p:?}");
        assert!((p[1] - 66.0).abs() < 1e-5, "{p:?}");
        assert!((p[2] - 23.0).abs() < 1e-5, "{p:?}");
        // Yaw 90 faces -X, so forward (+z) moves -X, left (+x) moves +Z.
        let p2 = local_position([10.0, 64.0, 20.0], 90.0, 0.0, [1.0, 0.0, 3.0]);
        assert!((p2[0] - 7.0).abs() < 1e-5, "{p2:?}");
        assert!((p2[2] - 21.0).abs() < 1e-5, "{p2:?}");
    }

    #[test]
    fn random_pose_keeps_body_zero_and_clamps_z_when_clamped() {
        for _ in 0..16 {
            let p = random_pose(true);
            assert_eq!(p[1], [0.0, 0.0, 0.0]);
            for m in MUTATORS {
                assert_eq!(p[m.pose_index][2], 0.0);
                let (xlo, xhi) = m.ranges[0];
                let (ylo, yhi) = m.ranges[1];
                assert!((xlo as f32..=xhi as f32).contains(&p[m.pose_index][0]));
                assert!((ylo as f32..=yhi as f32).contains(&p[m.pose_index][1]));
            }
        }
    }

    #[test]
    fn all_29_poses_present_and_tab_cycles() {
        assert_eq!(POSES.len(), 29);
        assert_eq!(Tab::Rotations.cycle(false), Tab::Poses);
        assert_eq!(Tab::Rotations.cycle(true), Tab::Equipment);
        assert_eq!(Tab::Equipment.cycle(false), Tab::Rotations);
    }

    #[test]
    fn snapshot_parses_nbt_and_builds_preview() {
        let mut nbt = pose_nbt(&POSES[1].pose);
        nbt.insert("ShowArms".into(), Tag::Byte(1));
        nbt.insert("Small".into(), Tag::Byte(1));
        nbt.insert("DisabledSlots".into(), Tag::Int(ALL_SLOTS_DISABLED));
        nbt.insert("CustomName".into(), Tag::String("Guard".into()));
        nbt.insert("CustomNameVisible".into(), Tag::Byte(1));
        let snap = StandSnapshot::from_nbt(7, &nbt, [1.5, 64.0, -2.5], [45.0, 0.0], 2.0, true);
        assert_eq!(snap.pose, POSES[1].pose);
        assert!(snap.show_arms);
        assert!(snap.small);
        assert!(StyleOption::Sealed.get(&snap));
        assert_eq!(snap.custom_name, "Guard");
        let preview = snap.preview_nbt(None);
        assert_eq!(preview.get(&"ShowArms".into()), Some(&Tag::Byte(1)));
        assert_eq!(preview.get(&"Small".into()), Some(&Tag::Byte(1)));
    }
}
