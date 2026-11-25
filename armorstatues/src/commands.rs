//! Client-only command queue (`CommandDataSyncHandler`) and Vanilla Tweaks `/trigger`
//! fallback (`VanillaTweaksDataSyncHandler`) for servers without `armorstatues`.
use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex, PoisonError};

use mod_api::client::widgets::{filter_text, is_allowed_chat_character, utf16_len};
use mod_api::text::TextStyle;
use mod_api::{ClientHost, Text};

use crate::model::{
    ALL_SLOTS_DISABLED, Alignment, DEFAULT_POSE, DEFAULT_SCALE, EditMessage, MAX_NAME_LEN, POSES,
    SCALE_BONUS_ID, StyleOption,
};

pub const COMMAND_DELAY_TICKS: i32 = 5;
pub const DEFAULT_VT_DELAY_TICKS: i32 = 20;
/// Vanilla `ServerboundChatCommandPacket` / `SharedConstants.MAX_CHAT_LENGTH` limit in UTF-16
/// code units (excluding the leading `/`).
pub const MAX_CHAT_COMMAND_LEN: usize = 256;
const MAX_INCREMENTAL_OPERATIONS: usize = 12;

// Vanilla Tweaks Armor Statues datapack trigger IDs (`VanillaTweaksDataSyncHandler`).
pub const VT_CHECK_TARGET: i32 = 999;
pub const VT_SWAP_MAINHAND_OFFHAND: i32 = 161;
pub const VT_SWAP_MAINHAND_HEAD: i32 = 162;
pub const VT_LOCK: i32 = 1000;
pub const VT_UNLOCK: i32 = 1001;
pub const VT_FLIP_MIRROR: i32 = 135;
pub const VT_TOOL_RACK: i32 = 155;

const VT_SHOW_BASE_PLATE_YES: i32 = 1;
const VT_SHOW_BASE_PLATE_NO: i32 = 2;
const VT_SHOW_ARMS_YES: i32 = 3;
const VT_SHOW_ARMS_NO: i32 = 4;
const VT_SMALL_YES: i32 = 5;
const VT_SMALL_NO: i32 = 6;
const VT_GRAVITY_YES: i32 = 7;
const VT_GRAVITY_NO: i32 = 8;
const VT_VISIBLE_YES: i32 = 9;
const VT_VISIBLE_NO: i32 = 10;

const VT_ANGLE_STEPS: [(f32, i32); 4] = [(45.0, 120), (15.0, 121), (5.0, 122), (1.0, 123)];
const VT_ROTATE_RIGHT: i32 = 56;
const VT_ROTATE_LEFT: i32 = 57;

const VT_NUDGE_POS: [[(f64, i32); 3]; 3] = [
    [(8.0 / 16.0, 43), (3.0 / 16.0, 104), (1.0 / 16.0, 103)],
    [(8.0 / 16.0, 47), (3.0 / 16.0, 108), (1.0 / 16.0, 107)],
    [(8.0 / 16.0, 51), (3.0 / 16.0, 112), (1.0 / 16.0, 111)],
];
const VT_NUDGE_NEG: [[(f64, i32); 3]; 3] = [
    [(8.0 / 16.0, 40), (3.0 / 16.0, 101), (1.0 / 16.0, 102)],
    [(8.0 / 16.0, 44), (3.0 / 16.0, 105), (1.0 / 16.0, 106)],
    [(8.0 / 16.0, 48), (3.0 / 16.0, 109), (1.0 / 16.0, 110)],
];

// `[x_neg, x_pos, y_neg, y_pos, z_neg, z_pos]` per part in `POSE_KEYS` order
// (`Head`, `Body`, `LeftArm`, `RightArm`, `LeftLeg`, `RightLeg`).
const VT_POSE_ADJUST: [[i32; 6]; 6] = [
    [60, 61, 62, 63, 64, 65], // Head
    [66, 67, 68, 69, 70, 71], // Body (X swapped in Vanilla Tweaks datapack)
    [78, 79, 80, 81, 82, 83], // LeftArm (Y swapped in Vanilla Tweaks datapack)
    [72, 73, 74, 75, 76, 77], // RightArm (Z swapped in Vanilla Tweaks datapack)
    [90, 91, 92, 93, 94, 95], // LeftLeg
    [84, 85, 86, 87, 88, 89], // RightLeg (Y and Z swapped in Vanilla Tweaks datapack)
];

const POSE_KEYS: [&str; 6] = ["Head", "Body", "LeftArm", "RightArm", "LeftLeg", "RightLeg"];

struct CommandQueue {
    batches: VecDeque<Vec<String>>,
    target_entity_id: Option<u32>,
    vanilla_tweaks: bool,
    delay_ticks: i32,
    countdown: i32,
}

#[derive(Debug, PartialEq, Eq)]
enum EnqueueError {
    Busy,
    InvalidCommand,
}

impl CommandQueue {
    const fn new() -> Self {
        Self {
            batches: VecDeque::new(),
            target_entity_id: None,
            vanilla_tweaks: false,
            delay_ticks: COMMAND_DELAY_TICKS,
            countdown: 0,
        }
    }

    fn enqueue(
        &mut self,
        entity_id: u32,
        vanilla_tweaks: bool,
        delay_ticks: i32,
        batches: Vec<Vec<String>>,
    ) -> Result<(), EnqueueError> {
        if batches.is_empty() {
            return Ok(());
        }
        if !batches_are_valid(&batches) {
            return Err(EnqueueError::InvalidCommand);
        }
        if self.batches.is_empty() {
            self.target_entity_id = None;
        } else if self.target_entity_id.is_some() {
            return Err(EnqueueError::Busy);
        }
        self.batches.extend(batches);
        self.target_entity_id = Some(entity_id);
        self.vanilla_tweaks = vanilla_tweaks;
        self.delay_ticks = delay_ticks;
        Ok(())
    }
}

/// Returns true if `cmd` (without the leading `/`) fits vanilla's 256-unit
/// `ServerboundChatCommandPacket` limit and contains only allowed chat characters
/// (no `§`, `0x00..=0x1f`, or `0x7f`).
pub fn command_is_valid(cmd: &str) -> bool {
    !cmd.is_empty()
        && utf16_len(cmd) <= MAX_CHAT_COMMAND_LEN
        && cmd.chars().all(is_allowed_chat_character)
}

pub fn batches_are_valid(batches: &[Vec<String>]) -> bool {
    batches.iter().flatten().all(|cmd| command_is_valid(cmd))
}

static QUEUE: LazyLock<Mutex<CommandQueue>> = LazyLock::new(|| Mutex::new(CommandQueue::new()));

fn lock_queue() -> std::sync::MutexGuard<'static, CommandQueue> {
    QUEUE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Formats a compact single-entity selector targeting the armor stand at `pos`
/// (used because `EntityRef::uuid` is server-only on `3d34bbe`).
pub fn stand_selector(pos: [f64; 3]) -> String {
    format!(
        "@e[type=armor_stand,x={},y={},z={},sort=nearest,limit=1]",
        fmt_coord(pos[0]),
        fmt_coord(pos[1]),
        fmt_coord(pos[2])
    )
}

fn fmt_coord(v: f64) -> String {
    let r = (v * 10_000.0).round() / 10_000.0;
    let r = if r == -0.0 { 0.0 } else { r };
    let s = format!("{r:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn fmt_float(v: f32) -> String {
    let r = (f64::from(v) * 100.0).round() / 100.0;
    let r = if r == -0.0 { 0.0 } else { r };
    let s = format!("{r:.2}");
    format!("{}f", s.trim_end_matches('0').trim_end_matches('.'))
}

fn truncate_utf16(s: &str, max_units: usize) -> &str {
    let mut units = 0;
    for (idx, c) in s.char_indices() {
        let next = units + c.len_utf16();
        if next > max_units {
            return &s[..idx];
        }
        units = next;
    }
    s
}

/// Filters disallowed chat characters (`StringUtil.filterText`: strips `§`, `0x00..=0x1f`, and
/// `0x7f`), trims whitespace, and caps at `MAX_NAME_LEN` UTF-16 code units.
fn sanitize_name(name: &str) -> String {
    let filtered = filter_text(name, false);
    truncate_utf16(filtered.trim(), MAX_NAME_LEN).to_owned()
}

fn escape_snbt_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(c),
        }
    }
    out
}

fn part_snbt(rot: [f32; 3]) -> String {
    format!(
        "[{},{},{}]",
        fmt_float(rot[0]),
        fmt_float(rot[1]),
        fmt_float(rot[2])
    )
}

/// Builds `/data merge` / `/attribute` command batches for one `EditMessage`.
/// Poses are split across up to three batches (head+body, arms, legs) to stay under the
/// 256-character client command limit, matching `CommandDataSyncHandler.sendPose`.
pub fn build_data_commands(
    pos: [f64; 3],
    prev_pose: &[[f32; 3]; 6],
    msg: &EditMessage,
) -> Vec<Vec<String>> {
    let sel = stand_selector(pos);
    match msg {
        EditMessage::Name(name) => {
            let sanitized = sanitize_name(name);
            let snbt = if sanitized.is_empty() {
                "{CustomName:\"\",CustomNameVisible:0b}".to_owned()
            } else {
                format!(
                    "{{CustomName:\"{}\",CustomNameVisible:1b}}",
                    escape_snbt_string(&sanitized)
                )
            };
            vec![vec![format!("data merge entity {sel} {snbt}")]]
        }
        EditMessage::Style { option, value } => {
            let b = if *value { "1b" } else { "0b" };
            let field = match option {
                StyleOption::ShowName => format!("CustomNameVisible:{b}"),
                StyleOption::ShowArms => format!("ShowArms:{b}"),
                StyleOption::Small => format!("Small:{b}"),
                StyleOption::Invisible => format!("Invisible:{b}"),
                StyleOption::NoBasePlate => format!("NoBasePlate:{b}"),
                StyleOption::Immovable => format!("NoGravity:{b}"),
                StyleOption::Invulnerable => format!("Invulnerable:{b}"),
                StyleOption::Sealed => {
                    format!(
                        "DisabledSlots:{}",
                        if *value { ALL_SLOTS_DISABLED } else { 0 }
                    )
                }
            };
            vec![vec![format!("data merge entity {sel} {{{field}}}")]]
        }
        EditMessage::Pose(pose) => {
            let mut batches = Vec::new();
            for pair in [[0usize, 1], [2, 3], [4, 5]] {
                let mut entries = Vec::new();
                for idx in pair {
                    if pose[idx] != prev_pose[idx] {
                        entries.push(format!("{}:{}", POSE_KEYS[idx], part_snbt(pose[idx])));
                    }
                }
                if !entries.is_empty() {
                    batches.push(vec![format!(
                        "data merge entity {sel} {{Pose:{{{}}}}}",
                        entries.join(",")
                    )]);
                }
            }
            batches
        }
        EditMessage::Position(next_pos) => {
            vec![vec![format!(
                "data merge entity {sel} {{Pos:[{}d,{}d,{}d]}}",
                fmt_coord(next_pos[0]),
                fmt_coord(next_pos[1]),
                fmt_coord(next_pos[2])
            )]]
        }
        EditMessage::Rotation(yaw) => {
            vec![vec![format!(
                "data merge entity {sel} {{Rotation:[{},0f]}}",
                fmt_float(*yaw)
            )]]
        }
        EditMessage::Scale(scale) => {
            let mut batch = vec![format!(
                "attribute {sel} minecraft:scale modifier remove {SCALE_BONUS_ID}"
            )];
            if (*scale - DEFAULT_SCALE).abs() > 1e-6 {
                let delta = f64::from(*scale) - f64::from(DEFAULT_SCALE);
                batch.push(format!(
                    "attribute {sel} minecraft:scale modifier add {SCALE_BONUS_ID} {} add_value",
                    fmt_coord(delta)
                ));
            }
            vec![batch]
        }
    }
}

/// Maps a Vanilla Tweaks-compatible preset pose (or vanilla default / mirrored pose) to its
/// single `/trigger as_trigger set N` ID, if one exists.
pub fn vt_preset_trigger(pose: &[[f32; 3]; 6], is_mirrored: bool) -> Option<i32> {
    if is_mirrored {
        return Some(VT_FLIP_MIRROR);
    }
    if *pose == DEFAULT_POSE {
        return Some(20); // POSE_PRESETS_ATTENTION
    }
    const PRESET_MAP: [(&str, i32); 20] = [
        ("screen.statuemenus.poses.minecraft.default", 20),
        ("screen.statuemenus.poses.vanillatweaks.walking", 21),
        ("screen.statuemenus.poses.vanillatweaks.running", 22),
        ("screen.statuemenus.poses.vanillatweaks.pointing", 23),
        ("screen.statuemenus.poses.vanillatweaks.blocking", 24),
        ("screen.statuemenus.poses.vanillatweaks.lungeing", 25),
        ("screen.statuemenus.poses.vanillatweaks.winning", 26),
        ("screen.statuemenus.poses.vanillatweaks.sitting", 27),
        ("screen.statuemenus.poses.vanillatweaks.arabesque", 28),
        ("screen.statuemenus.poses.vanillatweaks.cupid", 29),
        ("screen.statuemenus.poses.vanillatweaks.confident", 30),
        ("screen.statuemenus.poses.minecraft.salute", 31),
        ("screen.statuemenus.poses.vanillatweaks.death", 32),
        ("screen.statuemenus.poses.vanillatweaks.facepalm", 33),
        ("screen.statuemenus.poses.vanillatweaks.lazing", 34),
        ("screen.statuemenus.poses.vanillatweaks.confused", 35),
        ("screen.statuemenus.poses.vanillatweaks.formal", 36),
        ("screen.statuemenus.poses.vanillatweaks.sad", 37),
        ("screen.statuemenus.poses.vanillatweaks.joyous", 38),
        ("screen.statuemenus.poses.vanillatweaks.stargazing", 39),
    ];
    for preset in POSES {
        if preset.pose == *pose {
            return PRESET_MAP
                .iter()
                .find(|(key, _)| *key == preset.translation_key)
                .map(|(_, id)| *id);
        }
    }
    None
}

pub fn vt_alignment_trigger(alignment: Alignment) -> i32 {
    match alignment {
        Alignment::Block => 151,
        Alignment::FloatingItem => 152,
        Alignment::FlatItem => 153,
        Alignment::Tool => 154,
    }
}

fn vt_trigger_cmd(id: i32) -> String {
    format!("trigger as_trigger set {id}")
}

fn decompose_angle_triggers(old: f32, new: f32, neg_id: i32, pos_id: i32, out: &mut Vec<i32>) {
    let diff = new - old;
    let negative = diff < 0.0;
    let mut rem = diff.abs();
    let mut last_step = 0.0_f32;
    for _ in 0..MAX_INCREMENTAL_OPERATIONS {
        let Some(&(step, step_id)) = VT_ANGLE_STEPS.iter().find(|&&(s, _)| s <= rem + 1e-4) else {
            break;
        };
        rem -= step;
        if (step - last_step).abs() > 1e-4 {
            last_step = step;
            out.push(step_id);
        }
        out.push(if negative { pos_id } else { neg_id });
    }
}

fn decompose_position_triggers(old: f64, new: f64, axis: usize, out: &mut Vec<i32>) {
    let diff = new - old;
    let table = if diff < 0.0 {
        &VT_NUDGE_NEG[axis]
    } else {
        &VT_NUDGE_POS[axis]
    };
    let mut rem = diff.abs();
    for _ in 0..MAX_INCREMENTAL_OPERATIONS {
        let Some(&(step, id)) = table.iter().find(|&&(s, _)| s <= rem + 1e-6) else {
            break;
        };
        rem -= step;
        out.push(id);
    }
}

/// Builds Vanilla Tweaks `/trigger as_trigger set N` batches (falling back to `/data merge` /
/// `/attribute` for actions without a trigger, as `VanillaTweaksDataSyncHandler` does).
pub fn build_vt_commands(
    prev_pos: [f64; 3],
    prev_yaw: f32,
    prev_pose: &[[f32; 3]; 6],
    msg: &EditMessage,
    is_mirrored: bool,
) -> Vec<Vec<String>> {
    match msg {
        EditMessage::Style { option, value } => {
            let trigger = match option {
                StyleOption::ShowArms => Some(if *value {
                    VT_SHOW_ARMS_YES
                } else {
                    VT_SHOW_ARMS_NO
                }),
                StyleOption::Small => Some(if *value { VT_SMALL_YES } else { VT_SMALL_NO }),
                StyleOption::Invisible => Some(if *value {
                    VT_VISIBLE_NO
                } else {
                    VT_VISIBLE_YES
                }),
                StyleOption::NoBasePlate => Some(if *value {
                    VT_SHOW_BASE_PLATE_NO
                } else {
                    VT_SHOW_BASE_PLATE_YES
                }),
                StyleOption::Immovable => Some(if *value {
                    VT_GRAVITY_NO
                } else {
                    VT_GRAVITY_YES
                }),
                StyleOption::ShowName => Some(if *value { 11 } else { 12 }),
                StyleOption::Invulnerable | StyleOption::Sealed => None,
            };
            match trigger {
                Some(id) => vec![vec![vt_trigger_cmd(id)]],
                None => build_data_commands(prev_pos, prev_pose, msg),
            }
        }
        EditMessage::Pose(pose) => {
            if let Some(id) = vt_preset_trigger(pose, is_mirrored) {
                return vec![vec![vt_trigger_cmd(id)]];
            }
            let mut ids = Vec::new();
            for part in 0..6 {
                if pose[part] == prev_pose[part] {
                    continue;
                }
                let t = VT_POSE_ADJUST[part];
                decompose_angle_triggers(prev_pose[part][0], pose[part][0], t[0], t[1], &mut ids);
                decompose_angle_triggers(prev_pose[part][1], pose[part][1], t[2], t[3], &mut ids);
                decompose_angle_triggers(prev_pose[part][2], pose[part][2], t[4], t[5], &mut ids);
            }
            ids.into_iter().map(|id| vec![vt_trigger_cmd(id)]).collect()
        }
        EditMessage::Position(next_pos) => {
            let mut ids = Vec::new();
            for axis in 0..3 {
                decompose_position_triggers(prev_pos[axis], next_pos[axis], axis, &mut ids);
            }
            ids.into_iter().map(|id| vec![vt_trigger_cmd(id)]).collect()
        }
        EditMessage::Rotation(next_yaw) => {
            let mut ids = Vec::new();
            decompose_angle_triggers(
                prev_yaw,
                *next_yaw,
                VT_ROTATE_RIGHT,
                VT_ROTATE_LEFT,
                &mut ids,
            );
            ids.into_iter().map(|id| vec![vt_trigger_cmd(id)]).collect()
        }
        EditMessage::Name(_) | EditMessage::Scale(_) => {
            build_data_commands(prev_pos, prev_pose, msg)
        }
    }
}

fn chat_status(host: &mut dyn ClientHost, text: Text, failure: bool) {
    let mut style = TextStyle::default();
    style.color = Some(if failure { "red" } else { "green" }.to_owned());
    let _ = host.chat_line(text.with_style(style));
}

fn chat_failure(host: &mut dyn ClientHost, reason_key: &str) {
    chat_status(
        host,
        Text::translate(
            "data_sync.armorstatues.failure",
            vec![Text::translate(reason_key, vec![])],
        ),
        true,
    );
}

fn validate_target(host: &mut dyn ClientHost, entity_id: u32, vanilla_tweaks: bool) -> bool {
    let Some(player) = host.local_player() else {
        return false;
    };
    if player.may_build(host) != Some(true) {
        return false;
    }
    let Some(stand) = host.entity(entity_id) else {
        chat_failure(host, "data_sync.armorstatues.failure.no_armor_stand");
        return false;
    };
    if vanilla_tweaks
        && let (Some(p), Some(s)) = (player.entity().position(host), stand.position(host))
    {
        let dx = p[0] - s[0];
        let dy = p[1] - s[1];
        let dz = p[2] - s[2];
        if dx * dx + dy * dy + dz * dz >= 9.0 {
            chat_failure(host, "data_sync.armorstatues.failure.out_of_range");
            return false;
        }
    }
    true
}

pub struct BuiltEdits {
    pub batches: Vec<Vec<String>>,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pose: [[f32; 3]; 6],
}

/// Builds combined command batches for a sequence of `(EditMessage, is_mirrored)` edits,
/// advancing simulated `(pos, yaw, pose)` between steps so multi-edit operations (alignments,
/// multi-field dirty flushes) enqueue and finalize once.
pub fn build_edits_batches(
    vanilla_tweaks: bool,
    mut pos: [f64; 3],
    mut yaw: f32,
    mut pose: [[f32; 3]; 6],
    edits: &[(EditMessage, bool)],
) -> BuiltEdits {
    let mut batches = Vec::new();
    for (msg, is_mirrored) in edits {
        let step = if vanilla_tweaks {
            build_vt_commands(pos, yaw, &pose, msg, *is_mirrored)
        } else {
            build_data_commands(pos, &pose, msg)
        };
        batches.extend(step);
        match msg {
            EditMessage::Position(next_pos) => pos = *next_pos,
            EditMessage::Rotation(next_yaw) => yaw = *next_yaw,
            EditMessage::Pose(next_pose) => pose = *next_pose,
            EditMessage::Name(_) | EditMessage::Style { .. } | EditMessage::Scale(_) => {}
        }
    }
    BuiltEdits {
        batches,
        pos,
        yaw,
        pose,
    }
}

/// Enqueues command batches for one operation. Matches `CommandDataSyncHandler`:
/// refuses with "Queue Not Empty" if an earlier operation is still draining, and refuses any
/// command exceeding vanilla's 256-unit `ServerboundChatCommandPacket` limit or containing
/// disallowed chat characters.
pub fn enqueue_batches(
    host: &mut dyn ClientHost,
    entity_id: u32,
    vanilla_tweaks: bool,
    batches: Vec<Vec<String>>,
) -> bool {
    if batches.is_empty() {
        return true;
    }
    if !batches_are_valid(&batches) {
        return false;
    }
    if !validate_target(host, entity_id, vanilla_tweaks) {
        return false;
    }
    let delay = if vanilla_tweaks {
        crate::VT_DELAY_SETTING
            .get()
            .and_then(|key| host.setting_f64(*key).ok())
            .map(|v| (v.round() as i32).max(DEFAULT_VT_DELAY_TICKS))
            .unwrap_or(DEFAULT_VT_DELAY_TICKS)
    } else {
        COMMAND_DELAY_TICKS
    };
    let res = lock_queue().enqueue(entity_id, vanilla_tweaks, delay, batches);
    match res {
        Ok(()) => true,
        Err(EnqueueError::Busy) => {
            chat_failure(host, "data_sync.armorstatues.failure.not_finished");
            false
        }
        Err(EnqueueError::InvalidCommand) => false,
    }
}

pub fn enqueue_single_trigger(host: &mut dyn ClientHost, entity_id: u32, trigger_id: i32) -> bool {
    enqueue_batches(
        host,
        entity_id,
        true,
        vec![vec![vt_trigger_cmd(trigger_id)]],
    )
}

/// Drains one queued command batch every `delay_ticks` on `client_tick_post`
/// (even after the statue screen closes, matching `DataSyncTickHandler`).
pub fn tick(host: &mut dyn ClientHost) {
    let action = {
        let mut q = lock_queue();
        if q.countdown > 0 {
            q.countdown -= 1;
        }
        if q.countdown == 0 && q.target_entity_id.is_some() && !q.batches.is_empty() {
            let entity_id = q.target_entity_id.unwrap();
            let vt = q.vanilla_tweaks;
            let batch = q.batches.pop_front().unwrap();
            q.countdown = q.delay_ticks;
            Some((entity_id, vt, batch, false))
        } else if q.countdown == 1 && q.batches.is_empty() && q.target_entity_id.is_some() {
            q.target_entity_id = None;
            Some((0, false, Vec::new(), true))
        } else {
            None
        }
    };
    let Some((entity_id, vt, batch, finished)) = action else {
        return;
    };
    if finished {
        chat_status(
            host,
            Text::translate("data_sync.armorstatues.finished", vec![]),
            false,
        );
        return;
    }
    if validate_target(host, entity_id, vt) {
        for cmd in batch {
            if command_is_valid(&cmd) {
                let _ = host.send_chat(&format!("/{cmd}"));
            }
        }
    } else {
        let mut q = lock_queue();
        q.batches.clear();
        q.target_entity_id = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pose_data_commands_split_by_limb_pair_and_diff() {
        let mut next = DEFAULT_POSE;
        next[0] = [15.0, -30.0, 0.0]; // Head changed
        next[4] = [10.0, 0.0, 0.0]; // LeftLeg changed
        let batches =
            build_data_commands([1.5, 64.0, -2.25], &DEFAULT_POSE, &EditMessage::Pose(next));
        assert_eq!(batches.len(), 2);
        assert!(batches[0][0].contains(
            "data merge entity @e[type=armor_stand,x=1.5,y=64,z=-2.25,sort=nearest,limit=1]"
        ));
        assert!(batches[0][0].contains("{Pose:{Head:[15f,-30f,0f]}}"));
        assert!(batches[1][0].contains("{Pose:{LeftLeg:[10f,0f,0f]}}"));
    }

    #[test]
    fn scale_command_removes_at_default_and_adds_delta_otherwise() {
        let reset = build_data_commands([0.0, 64.0, 0.0], &DEFAULT_POSE, &EditMessage::Scale(1.0));
        assert_eq!(reset.len(), 1);
        assert_eq!(reset[0].len(), 1);
        assert!(reset[0][0].contains("modifier remove statuemenus:scale_bonus"));

        let grown = build_data_commands([0.0, 64.0, 0.0], &DEFAULT_POSE, &EditMessage::Scale(2.5));
        assert_eq!(grown.len(), 1);
        assert_eq!(grown[0].len(), 2);
        assert!(grown[0][1].contains("modifier add statuemenus:scale_bonus 1.5 add_value"));
    }

    #[test]
    fn vt_preset_and_style_map_to_expected_triggers() {
        let walking = POSES
            .iter()
            .find(|p| p.translation_key.ends_with(".walking"))
            .unwrap();
        assert_eq!(vt_preset_trigger(&walking.pose, false), Some(21));
        assert_eq!(vt_preset_trigger(&DEFAULT_POSE, false), Some(20));
        assert_eq!(vt_preset_trigger(&DEFAULT_POSE, true), Some(VT_FLIP_MIRROR));

        let arms = build_vt_commands(
            [0.0; 3],
            0.0,
            &DEFAULT_POSE,
            &EditMessage::Style {
                option: StyleOption::ShowArms,
                value: true,
            },
            false,
        );
        assert_eq!(arms, vec![vec!["trigger as_trigger set 3".to_owned()]]);

        // Invulnerable has no VT trigger and falls back to `/data merge`.
        let invuln = build_vt_commands(
            [0.0; 3],
            0.0,
            &DEFAULT_POSE,
            &EditMessage::Style {
                option: StyleOption::Invulnerable,
                value: true,
            },
            false,
        );
        assert!(invuln[0][0].starts_with("data merge entity "));
    }

    #[test]
    fn vt_position_and_rotation_decompose_greedily() {
        // +0.75 blocks on X = 8/16 (43) + 3/16 (104) + 1/16 (103)
        let cmds = build_vt_commands(
            [0.0, 64.0, 0.0],
            0.0,
            &DEFAULT_POSE,
            &EditMessage::Position([0.75, 64.0, 0.0]),
            false,
        );
        assert_eq!(
            cmds,
            vec![
                vec!["trigger as_trigger set 43".to_owned()],
                vec!["trigger as_trigger set 104".to_owned()],
                vec!["trigger as_trigger set 103".to_owned()],
            ]
        );

        // +60 deg yaw = step45 (120) + right (56) + step15 (121) + right (56)
        let rot = build_vt_commands(
            [0.0; 3],
            0.0,
            &DEFAULT_POSE,
            &EditMessage::Rotation(60.0),
            false,
        );
        assert_eq!(
            rot,
            vec![
                vec!["trigger as_trigger set 120".to_owned()],
                vec!["trigger as_trigger set 56".to_owned()],
                vec!["trigger as_trigger set 121".to_owned()],
                vec!["trigger as_trigger set 56".to_owned()],
            ]
        );
    }

    #[test]
    fn name_command_filters_disallowed_chars_and_caps_at_max_name_len() {
        let dirty = "  \u{0000}\u{001f}§aHello\n\"World\"\\\u{007f}  ";
        let cmds = build_data_commands(
            [1.0, 64.0, 2.0],
            &DEFAULT_POSE,
            &EditMessage::Name(dirty.into()),
        );
        assert_eq!(cmds.len(), 1);
        let cmd = &cmds[0][0];
        assert!(command_is_valid(cmd));
        assert!(cmd.contains(r#"{CustomName:"aHello\"World\"\\",CustomNameVisible:1b}"#));

        // 60 ASCII chars capped to MAX_NAME_LEN (50).
        let long_ascii = "a".repeat(MAX_NAME_LEN + 10);
        let cmds = build_data_commands(
            [0.0, 64.0, 0.0],
            &DEFAULT_POSE,
            &EditMessage::Name(long_ascii),
        );
        let expected = format!(
            "{{CustomName:\"{}\",CustomNameVisible:1b}}",
            "a".repeat(MAX_NAME_LEN)
        );
        assert!(cmds[0][0].contains(&expected));

        // 30 surrogate-pair emoji (60 UTF-16 units) capped to 25 emoji (50 UTF-16 units).
        let long_emoji = "😀".repeat(30);
        let cmds = build_data_commands(
            [0.0, 64.0, 0.0],
            &DEFAULT_POSE,
            &EditMessage::Name(long_emoji),
        );
        let expected = format!(
            "{{CustomName:\"{}\",CustomNameVisible:1b}}",
            "😀".repeat(MAX_NAME_LEN / 2)
        );
        assert!(cmds[0][0].contains(&expected));

        // Only forbidden chars and whitespace clears CustomName.
        let cleared = build_data_commands(
            [0.0, 64.0, 0.0],
            &DEFAULT_POSE,
            &EditMessage::Name("§\u{0007}\u{007f}   ".into()),
        );
        assert!(cleared[0][0].contains("{CustomName:\"\",CustomNameVisible:0b}"));
    }

    #[test]
    fn queue_refuses_commands_over_256_units_and_enqueues_multi_edit_alignment_atomically() {
        let mut q = CommandQueue::new();
        let exact = "a".repeat(MAX_CHAT_COMMAND_LEN);
        assert!(command_is_valid(&exact));
        let over = "a".repeat(MAX_CHAT_COMMAND_LEN + 1);
        assert!(!command_is_valid(&over));
        assert_eq!(
            q.enqueue(1, false, COMMAND_DELAY_TICKS, vec![vec![over]]),
            Err(EnqueueError::InvalidCommand)
        );
        assert!(q.batches.is_empty());

        // Over-limit Name command assembled at huge coordinates with escaped quotes is refused.
        let huge_pos = [1e25, 1e25, -1e25];
        let quotes = "\"".repeat(MAX_NAME_LEN);
        let name_batches = build_data_commands(huge_pos, &DEFAULT_POSE, &EditMessage::Name(quotes));
        assert!(utf16_len(&name_batches[0][0]) > MAX_CHAT_COMMAND_LEN);
        assert_eq!(
            q.enqueue(1, false, COMMAND_DELAY_TICKS, name_batches),
            Err(EnqueueError::InvalidCommand)
        );
        assert!(q.batches.is_empty());

        // Alignment produces 4 edits batched into one enqueue without hitting Busy.
        let edits = vec![
            (
                EditMessage::Style {
                    option: StyleOption::Invisible,
                    value: true,
                },
                false,
            ),
            (
                EditMessage::Style {
                    option: StyleOption::Immovable,
                    value: true,
                },
                false,
            ),
            (EditMessage::Pose(Alignment::Block.pose()), false),
            (EditMessage::Position([1.5, 64.5, 2.5]), false),
        ];
        let built = build_edits_batches(false, [1.0, 64.0, 2.0], 0.0, DEFAULT_POSE, &edits);
        assert_eq!(built.batches.len(), 4);
        assert_eq!(built.pos, [1.5, 64.5, 2.5]);
        assert_eq!(built.pose, Alignment::Block.pose());
        assert_eq!(
            q.enqueue(7, false, COMMAND_DELAY_TICKS, built.batches),
            Ok(())
        );
        assert_eq!(q.batches.len(), 4);
        // Subsequent separate operation while still draining is refused as Busy.
        assert_eq!(
            q.enqueue(7, false, COMMAND_DELAY_TICKS, vec![vec![exact]]),
            Err(EnqueueError::Busy)
        );
    }
}
