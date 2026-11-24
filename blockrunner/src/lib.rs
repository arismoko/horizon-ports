//! Block Runner by Fuzs for Sunrise: living entities move faster or slower on the blocks the
//! data map `blockrunner:block_speeds` gives a speed, through a transient `movement_speed`
//! modifier `blockrunner:block_speed` (ADD_VALUE, `speed × base − base`).
//!
//! As the original, the modifier is set again when an entity's block position changes
//! (`LivingEntity.onChangedBlock`) and when it lands: it reads the block the entity stands on
//! (`Entity.getBlockStateOn`), keeps the modifier while that is air (a jump keeps the speed), and
//! drops it while gliding or flying. A block with a speed moves at its vanilla speed factor
//! (`getBlockSpeedFactor` answers 1.0), so the map alone decides the speed on it.
//!
//! The server reads the map and sends it to its players, who need it for the item tooltip, the
//! speed factor their own movement predicts, and the field of view.
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError, RwLock};

use mod_api::client::camera::UseItem;
use mod_api::text::TextStyle;
use mod_api::{
    AttributeModifier, AttributeOperation, BlockKey, BlockPart, BlockSpeedFactorEvent, ClientHost,
    ComputeFovModifierEvent, DataMapType, DataMapTypesEvent, DataMapsUpdatedEvent,
    EntityTickListenedEvent, EntityTickPreEvent, Hooks, Host, HostSide, IPayloadContext,
    IPayloadHandler, Identifier, ItemTooltipEvent, LoopContext, ModifierLifetime, PayloadBytes,
    PayloadHandleError, PayloadId, PlayerLoggedInEvent, Registrar, RegistrationContext,
    ReloadContext, ServerHost, SettingSpec, Tag, Text,
};

const MOD_ID: &str = "blockrunner";
const MAP: &str = "blockrunner:block_speeds";
const MODIFIER: &str = "blockrunner:block_speed";
const MOVEMENT_SPEED: &str = "minecraft:movement_speed";
const CHANNEL: &str = "blockrunner:block_speeds";
const TOOLTIP: &str = "block.blockrunner.speedMultiplier";
/// The settings' keys, and the original's config names their labels keep.
const SHOW_TOOLTIP: &str = "block_speed_item_tooltip";
const STEADY_FOV: &str = "disable_field_of_view_changes";
const SETTINGS: [(&str, &str); 2] = [
    (SHOW_TOOLTIP, "blockSpeedItemTooltip"),
    (STEADY_FOV, "disableFieldOfViewChanges"),
];

/// The original's data: its six block tags and the data map giving each its speed, contributed
/// to every data load below the world's data packs, which may change them.
const DATA: [(&str, &str); 7] = [
    (
        "data_maps/blockrunner:block/block_speeds",
        r##"{
  "values": {
    "#blockrunner:very_slow_blocks": { "speed": 0.45 },
    "#blockrunner:slow_blocks": { "speed": 0.65 },
    "#blockrunner:slightly_slow_blocks": { "speed": 0.85 },
    "#blockrunner:slightly_quick_blocks": { "speed": 1.15 },
    "#blockrunner:quick_blocks": { "speed": 1.35 },
    "#blockrunner:very_quick_blocks": { "speed": 1.55 }
  }
}"##,
    ),
    ("tags/blockrunner:block/very_slow_blocks", EMPTY_TAG),
    ("tags/blockrunner:block/slow_blocks", EMPTY_TAG),
    ("tags/blockrunner:block/slightly_slow_blocks", EMPTY_TAG),
    (
        "tags/blockrunner:block/slightly_quick_blocks",
        r##"{ "values": ["#minecraft:stone_bricks"] }"##,
    ),
    (
        "tags/blockrunner:block/quick_blocks",
        r#"{ "values": ["minecraft:dirt_path"] }"#,
    ),
    ("tags/blockrunner:block/very_quick_blocks", EMPTY_TAG),
];

const EMPTY_TAG: &str = r#"{ "values": [] }"#;

/// `BlockSpeed`'s codec range.
const MAX_SPEED: f64 = 8.0;

/// The vanilla blocks whose speed factor is not 1.0 (`Blocks.SOUL_SAND`, `Blocks.HONEY_BLOCK`).
const SLOW_FACTOR_BLOCKS: [&str; 2] = ["minecraft:soul_sand", "minecraft:honey_block"];

/// Block id to speed, as the server's data map gave it (on a client, as its server sent it).
static SPEEDS: RwLock<Option<HashMap<String, f64>>> = RwLock::new(None);
/// The server's map was reloaded and has not been read and sent yet.
static RELOADED: AtomicBool = AtomicBool::new(true);

fn speed_of(block: &str) -> Option<f64> {
    SPEEDS
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()?
        .get(block)
        .copied()
}

/// `BlockSpeed.CODEC`: `{"speed": x}` or a bare number, within 0..=8.
fn decode(value: &Tag) -> Option<f64> {
    let number = match value {
        Tag::Compound(compound) => compound.get(&"speed".into())?,
        other => other,
    };
    let speed = match number {
        Tag::Double(bits) => f64::from_bits(*bits),
        Tag::Float(bits) => f64::from(f32::from_bits(*bits)),
        Tag::Int(value) => f64::from(*value),
        Tag::Long(value) => *value as f64,
        _ => return None,
    };
    (0.0..=MAX_SPEED).contains(&speed).then_some(speed)
}

/// The table as the payload carries it: per block, its id's length (u16), the id, the speed (f64).
fn encode(speeds: &HashMap<String, f64>) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (block, speed) in speeds {
        let Ok(len) = u16::try_from(block.len()) else {
            continue;
        };
        bytes.extend(len.to_be_bytes());
        bytes.extend(block.as_bytes());
        bytes.extend(speed.to_be_bytes());
    }
    bytes
}

fn decode_table(bytes: &[u8]) -> HashMap<String, f64> {
    let mut speeds = HashMap::new();
    let mut rest = bytes;
    while let Some((len, tail)) = rest.split_first_chunk::<2>() {
        let len = usize::from(u16::from_be_bytes(*len));
        let Some((name, tail)) = tail.split_at_checked(len) else {
            break;
        };
        let Some((speed, tail)) = tail.split_first_chunk::<8>() else {
            break;
        };
        if let Ok(name) = std::str::from_utf8(name) {
            speeds.insert(name.to_owned(), f64::from_be_bytes(*speed));
        }
        rest = tail;
    }
    speeds
}

/// Java's `String.valueOf(double)` for the tooltip's `%s`: a whole number keeps its `.0`.
fn java_double(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e7 {
        format!("{value:.1}")
    } else {
        value.to_string()
    }
}

/// Reads the server's map into the table, if it was reloaded, and returns the table's bytes to
/// send then.
fn refresh(host: &dyn Host) -> Option<Vec<u8>> {
    if !RELOADED.swap(false, Ordering::AcqRel) {
        return None;
    }
    let speeds: HashMap<String, f64> = mod_api::data_map_entries(host, "minecraft:block", MAP)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|(block, value)| Some((block.clone(), decode(value)?)))
                .collect()
        })
        .unwrap_or_default();
    let bytes = encode(&speeds);
    *SPEEDS.write().unwrap_or_else(PoisonError::into_inner) = Some(speeds);
    Some(bytes)
}

fn channel() -> PayloadId {
    PayloadId(CHANNEL.into())
}

fn broadcast(host: &mut dyn ServerHost, bytes: &[u8]) {
    let players = host.players().server_players();
    for player in players {
        let _ = player.send_payload(host, &channel(), PayloadBytes(bytes.to_vec()));
    }
}

/// The server's table, sent at each login and after each reload.
struct SpeedsHandler;

impl IPayloadHandler for SpeedsHandler {
    fn handle(
        &self,
        payload: PayloadBytes,
        _context: &mut dyn IPayloadContext,
    ) -> Result<(), PayloadHandleError> {
        *SPEEDS.write().unwrap_or_else(PoisonError::into_inner) = Some(decode_table(&payload.0));
        Ok(())
    }
}

/// `LivingEntityMixin.getBlockSpeedFactor`: a block with a speed keeps the entity at factor 1.0.
struct NeutralFactor(&'static str);

#[mod_api::sunrise_part]
impl BlockPart for NeutralFactor {
    fn block_speed_factor(&self, _: &dyn LoopContext, event: &mut BlockSpeedFactorEvent) {
        if speed_of(self.0).is_some() {
            event.factor = 1.0;
        }
    }
}

/// Where an entity was before its last tick: what `onChangedBlock` and landing compare.
#[derive(Clone, Copy)]
struct Seen {
    pos: [i32; 3],
    on_ground: bool,
    tick: u64,
}

/// Entities not told for this many `entity_tick_pre` calls are forgotten.
const FORGET_AFTER: u64 = 4096;

static TICKS: AtomicU64 = AtomicU64::new(0);

fn seen() -> MutexGuard<'static, HashMap<u32, Seen>> {
    static SEEN: LazyLock<Mutex<HashMap<u32, Seen>>> = LazyLock::new(Mutex::default);
    SEEN.lock().unwrap_or_else(PoisonError::into_inner)
}

fn modifier_id() -> Identifier {
    Identifier::parse(MODIFIER).expect("a valid id")
}

/// `blockrunner$onChangedBlock`: drop the modifier unless the entity stands on air, then add the
/// block's while not flying.
fn changed_block(host: &mut dyn ServerHost, id: u32, player: bool) {
    let Some(entity) = host.entity(id) else {
        return;
    };
    let Some(living) = host.living(entity) else {
        return;
    };
    let flying = player
        && entity
            .uuid(host)
            .and_then(|uuid| host.player(uuid))
            .and_then(|p| p.abilities(host))
            .is_some_and(|abilities| abilities.flying);
    let gliding = living.is_gliding(host).unwrap_or(false);
    let block = entity
        .on_block(host)
        .zip(entity.level(host))
        .and_then(|(pos, level)| level.block(host, pos))
        .and_then(|block| block.id(host).map(str::to_owned));
    let air = block.as_deref().is_none_or(is_air);
    if !air || gliding || flying {
        let _ = living.remove_attribute_modifier(host, MOVEMENT_SPEED, &modifier_id());
    }
    if flying || air {
        return;
    }
    let Some(speed) = block.as_deref().and_then(speed_of) else {
        return;
    };
    let Some(base) = living.attribute_base(host, MOVEMENT_SPEED) else {
        return;
    };
    if speed == 1.0 {
        return;
    }
    let modifier = AttributeModifier::new(
        modifier_id(),
        speed * base - base,
        AttributeOperation::AddValue,
    );
    let _ =
        living.add_attribute_modifier(host, MOVEMENT_SPEED, modifier, ModifierLifetime::Transient);
}

/// `BlockState.isAir`.
fn is_air(block: &str) -> bool {
    matches!(
        block,
        "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
    )
}

fn flag(host: &dyn Host, key: &str) -> bool {
    host.setting(MOD_ID, key)
        .is_none_or(|text| text.trim() == "true")
}

/// `AttributeInstance.calculateValue` without the modifier `skip`, clamped to the movement
/// speed's range (`RangedAttribute.sanitizeValue`, 0 to 1024).
fn speed_without(base: f64, modifiers: &[AttributeModifier], skip: &Identifier) -> f64 {
    let kept = || modifiers.iter().filter(|m| &m.id != skip);
    let added = base
        + kept()
            .filter(|m| m.operation == AttributeOperation::AddValue)
            .map(|m| m.amount)
            .sum::<f64>();
    let mut value = added
        + kept()
            .filter(|m| m.operation == AttributeOperation::AddMultipliedBase)
            .map(|m| added * m.amount)
            .sum::<f64>();
    for m in kept().filter(|m| m.operation == AttributeOperation::AddMultipliedTotal) {
        value *= 1.0 + m.amount;
    }
    if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1024.0)
    }
}

/// `AbstractClientPlayer.getFieldOfViewModifier` before its effect scale, at `movement_speed`.
fn unscaled_fov(event: &ComputeFovModifierEvent<'_>, movement_speed: f64) -> f32 {
    let player = event.player;
    let mut modifier = if player.flying { 1.1 } else { 1.0 };
    if player.walking_speed != 0.0 {
        modifier *= (movement_speed as f32 / player.walking_speed + 1.0) / 2.0;
    }
    if let Some((UseItem::Bow, ticks)) = player.using {
        let scale = (ticks as f32 / 20.0).min(1.0);
        modifier *= 1.0 - scale * scale * 0.15;
    }
    modifier
}

#[derive(Default)]
struct Mod;

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        for (key, name) in SETTINGS {
            let label = format!("{MOD_ID}.config.{name}");
            let spec = SettingSpec::toggle(key, &label, true).tooltip(format!("{label}.tooltip"));
            let _ = r.register_setting(spec);
        }
        for block in SLOW_FACTOR_BLOCKS {
            let key: BlockKey = r.block_key(block);
            r.block_part(key, NeutralFactor(block));
        }
        let mut payloads = r.payload_handlers();
        let mut channel_registrar = payloads.registrar("1");
        channel_registrar.play_to_client(channel(), Some(Arc::new(SpeedsHandler)));
    }

    fn data_pack_files(&self, _: &mut dyn ServerHost, event: &mut mod_api::DataPackFilesEvent<'_>) {
        for (path, bytes) in DATA {
            event
                .files
                .push((path.to_owned(), bytes.as_bytes().to_vec()));
        }
    }

    fn data_map_types(&self, _: &mut dyn ServerHost, event: &mut DataMapTypesEvent<'_>) {
        event.types.push(DataMapType {
            id: MAP.to_owned(),
            registry: "minecraft:block".to_owned(),
        });
    }

    fn data_maps_updated(&self, _: &dyn ReloadContext, event: &mut DataMapsUpdatedEvent<'_>) {
        if event.registry == "minecraft:block" {
            RELOADED.store(true, Ordering::Release);
        }
    }

    fn player_logged_in(&self, host: &mut dyn ServerHost, event: &mut PlayerLoggedInEvent<'_>) {
        if let Some(bytes) = refresh(&*host) {
            broadcast(host, &bytes);
            return;
        }
        let bytes = SPEEDS
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(encode)
            .unwrap_or_default();
        if let Some(player) = host.player(event.player) {
            let _ = player.send_payload(host, &channel(), PayloadBytes(bytes));
        }
    }

    fn entity_tick_listened(&self, host: HostSide, event: &mut EntityTickListenedEvent) {
        if matches!(host, HostSide::Server(_)) {
            event.listened = true;
        }
    }

    /// Before each tick, as `baseTick` checks the position the last tick's move left, so the
    /// tick's own travel already moves at the new speed.
    fn entity_tick_pre(&self, host: HostSide, event: &mut EntityTickPreEvent<'_>) {
        let HostSide::Server(host) = host else {
            return;
        };
        if let Some(bytes) = refresh(&*host) {
            broadcast(host, &bytes);
        }
        let tick = TICKS.fetch_add(1, Ordering::Relaxed);
        if tick.is_multiple_of(FORGET_AFTER) {
            seen().retain(|_, s| tick - s.tick < FORGET_AFTER);
        }
        for (&id, &kind) in event.entities.iter().zip(event.kinds) {
            let Some(entity) = host.entity(id) else {
                continue;
            };
            if host.living(entity).is_none() {
                continue;
            }
            let (Some(pos), Some(on_ground)) =
                (entity.block_position(&*host), entity.on_ground(&*host))
            else {
                continue;
            };
            let now = Seen {
                pos,
                on_ground,
                tick,
            };
            let before = seen().insert(id, now);
            // `baseTick`'s `lastPos` starts unset, so an entity's first tick counts as a change;
            // `checkFallDamage` runs it again on landing, in the last tick's move.
            let changed = before.is_none_or(|b| b.pos != pos || (on_ground && !b.on_ground));
            if changed {
                changed_block(host, id, kind == "minecraft:player");
            }
        }
    }

    fn item_tooltip(&self, host: &dyn ClientHost, event: &mut ItemTooltipEvent<'_>) {
        if !flag(host, SHOW_TOOLTIP) {
            return;
        }
        let Some(block) = host.item_block(event.stack.item()) else {
            return;
        };
        let Some(speed) = block.id(host).and_then(speed_of) else {
            return;
        };
        let mut style = TextStyle::default();
        style.color = Some("gray".to_owned());
        let line =
            Text::translate(TOOLTIP, vec![Text::literal(java_double(speed))]).with_style(style);
        let _ = event.lines.push(line, u32::MAX);
    }

    fn compute_fov_modifier(&self, host: &dyn ClientHost, event: &mut ComputeFovModifierEvent<'_>) {
        if !flag(host, STEADY_FOV) {
            return;
        }
        let Some(living) = host
            .local_player()
            .and_then(|player| host.living(player.entity()))
        else {
            return;
        };
        let id = modifier_id();
        let Some(modifiers) = living.attribute_modifiers(host, MOVEMENT_SPEED) else {
            return;
        };
        if !modifiers.iter().any(|m| m.id == id) {
            return;
        }
        let Some(base) = living.attribute_base(host, MOVEMENT_SPEED) else {
            return;
        };
        // The event gives the scaled modifier only: the effect scale is what turned the
        // unscaled one into it.
        let unscaled = unscaled_fov(event, event.player.movement_speed);
        if unscaled == 1.0 {
            return;
        }
        let scale = (event.base - 1.0) / (unscaled - 1.0);
        let without = unscaled_fov(event, speed_without(base, &modifiers, &id));
        event.modifier = 1.0 + scale * (without - 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speeds_decode_as_the_codec_reads_them() {
        assert_eq!(decode(&Tag::Double(1.35f64.to_bits())), Some(1.35));
        assert_eq!(decode(&Tag::Int(2)), Some(2.0));
        let compound = Tag::compound([("speed", Tag::Double(0.45f64.to_bits()))]);
        assert_eq!(decode(&compound), Some(0.45));
        assert_eq!(decode(&Tag::Double(8.5f64.to_bits())), None);
        assert_eq!(decode(&Tag::Double((-0.1f64).to_bits())), None);
    }

    #[test]
    fn the_table_survives_its_payload() {
        let speeds = HashMap::from([
            ("minecraft:dirt_path".to_owned(), 1.35),
            ("minecraft:stone_bricks".to_owned(), 1.15),
        ]);
        assert_eq!(decode_table(&encode(&speeds)), speeds);
        // A cut payload keeps the whole entries before the cut.
        let bytes = encode(&HashMap::from([("a:b".to_owned(), 0.5)]));
        assert!(decode_table(&bytes[..bytes.len() - 1]).is_empty());
    }

    #[test]
    fn tooltip_numbers_read_as_java_prints_them() {
        assert_eq!(java_double(1.35), "1.35");
        assert_eq!(java_double(1.0), "1.0");
        assert_eq!(java_double(0.45), "0.45");
    }

    #[test]
    fn the_value_without_the_block_speed_skips_only_it() {
        let ours = Identifier::parse(MODIFIER).unwrap();
        let sprint = Identifier::parse("minecraft:sprinting").unwrap();
        let modifiers = [
            AttributeModifier::new(ours.clone(), 0.035, AttributeOperation::AddValue),
            AttributeModifier::new(sprint, 0.3, AttributeOperation::AddMultipliedTotal),
        ];
        let without = speed_without(0.1, &modifiers, &ours);
        assert!((without - 0.13).abs() < 1e-12, "{without}");
    }
}
