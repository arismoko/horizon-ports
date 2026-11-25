//! A backpack drawn on the back of its bearer (`BackpackLayerRenderer`): the server tells the clients which
//! entities wear one and of what tier, and a layer on every living renderer draws the tier's block model on the
//! body, following its pose.
//!
//! A client keeps no other entity's equipment, so the server sends what it draws: the entity's network id and the
//! tier (or none), when a player's worn backpack changes and when a client starts tracking a bearer.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use mod_api::{
    BlockEntityDrawContext, BlockState, EntityLayers, EntityRenderLayer, EntityRenderState,
    EquipmentSlot, FrameModel, Host, HostBlockStates, LayerTarget, LivingRef, PayloadBytes,
    PayloadId, PlayerModel, ServerHost,
};

use crate::contents;
use crate::places::{self, Loc};
use crate::storage_items::{self, Family};

pub const VIEW: &str = "sophisticatedcore:worn_view";
const NONE: u8 = 255;

/// The backpack block ids by tier, kept from registration.
static BLOCKS: OnceLock<Vec<(u8, String)>> = OnceLock::new();

/// Their north-facing states, looked up once by a client tick: a frame has no host to look a state up with.
static STATES: OnceLock<Vec<(u8, BlockState)>> = OnceLock::new();

pub fn set_blocks(blocks: Vec<(u8, String)>) {
    let _ = BLOCKS.set(blocks);
}

pub fn resolve_states(host: &dyn mod_api::ClientHost) {
    if STATES.get().is_some() {
        return;
    }
    let Some(blocks) = BLOCKS.get() else {
        return;
    };
    let states = blocks
        .iter()
        .filter_map(|(tier, id)| {
            host.block_state_id(&format!("{id}[facing=north]"))
                .map(|s| (*tier, s))
        })
        .collect();
    let _ = STATES.set(states);
}

/// The tier drawn per entity id, as the client last heard.
static SEEN: OnceLock<Mutex<HashMap<u32, u8>>> = OnceLock::new();

fn seen() -> std::sync::MutexGuard<'static, HashMap<u32, u8>> {
    SEEN.get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

pub fn encode(entity: u32, tier: Option<u8>) -> PayloadBytes {
    let mut bytes = entity.to_be_bytes().to_vec();
    bytes.push(tier.unwrap_or(NONE));
    PayloadBytes(bytes)
}

fn decode(bytes: &[u8]) -> Option<(u32, Option<u8>)> {
    let id = u32::from_be_bytes(bytes.get(..4)?.try_into().ok()?);
    let tier = *bytes.get(4)?;
    Some((id, (tier != NONE).then_some(tier)))
}

/// The client heard that `bytes` says an entity wears (or no longer wears) a backpack.
pub fn heard(bytes: &[u8]) {
    let Some((id, tier)) = decode(bytes) else {
        return;
    };
    let mut seen = seen();
    match tier {
        Some(t) => {
            seen.insert(id, t);
        }
        None => {
            seen.remove(&id);
        }
    }
}

/// The tier of the backpack a player wears: the trinkets back slot, else the chest slot.
pub fn player_tier(host: &mut dyn ServerHost, uuid: u128) -> Option<u8> {
    if let Some((_, k)) = places::storage_at(host, uuid, &Loc::Worn(places::BACK.into()))
        && k.family == Family::Backpack
    {
        return Some(k.tier);
    }
    let player = host.player(uuid)?;
    let id = LivingRef::new(player.entity())
        .with_equipment(&*host, EquipmentSlot::Chest, |v| v.project())
        .flatten()
        .map(|s| contents::item_id(&*host, &s))?;
    storage_items::by_item(&id)
        .filter(|k| k.family == Family::Backpack)
        .map(|k| k.tier)
}

/// What the server last told the clients about each player.
#[derive(Default)]
pub struct Told(Mutex<HashMap<u32, Option<u8>>>);

impl Told {
    /// Tells every player of the changes in who wears what.
    pub fn sync(&self, host: &mut dyn ServerHost, players: &[u128]) {
        let mut told = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut changes = Vec::new();
        for &uuid in players {
            let Some(id) = host.player(uuid).map(|p| p.entity().id()) else {
                continue;
            };
            let tier = player_tier(host, uuid);
            if told.get(&id) != Some(&tier) {
                told.insert(id, tier);
                changes.push((id, tier));
            }
        }
        drop(told);
        for (id, tier) in changes {
            for &uuid in players {
                if let Some(p) = host.player(uuid) {
                    let _ = p.send_payload(host, &PayloadId(VIEW.into()), encode(id, tier));
                }
            }
        }
    }

    /// What is known of a player's backpack.
    pub fn of(&self, id: u32) -> Option<u8> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .copied()
            .flatten()
    }
}

/// The tier of a mob that spawned with a backpack.
pub fn mob_tier(host: &dyn Host, id: u32) -> Option<u8> {
    let entity = host.entity(id)?;
    crate::mobs::tags_of(host, entity).map(|(tier, _)| tier as u8)
}

pub fn register(layers: &mut dyn EntityLayers) {
    let layer = Arc::new(Backpack);
    for model in [PlayerModel::Wide, PlayerModel::Slim] {
        if let Err(e) = layers.add(LayerTarget::Player(model), layer.clone()) {
            eprintln!("[sophisticatedcore] backpack layer: {e}");
        }
    }
    let kinds: Vec<String> = layers.entity_types().to_vec();
    for kind in kinds {
        if kind != "minecraft:player" && layers.is_living(&kind) {
            // The armour stand and the dragon refuse layers; they never wear one.
            let _ = layers.add(LayerTarget::Entity(kind), layer.clone());
        }
    }
}

/// Where the backpack sits against a body of the model's own frame (+y down, front towards -z, in texels): the
/// back of the body (2 texels behind its middle), the pack's lower edge at the body's lower edge.
struct Backpack;

impl EntityRenderLayer for Backpack {
    fn submit(
        &self,
        entity: &EntityRenderState,
        model: Option<&dyn FrameModel>,
        draw: &mut dyn BlockEntityDrawContext,
    ) -> Result<(), String> {
        let Some(tier) = seen().get(&entity.id).copied() else {
            return Ok(());
        };
        let Some(&(_, state)) = STATES
            .get()
            .and_then(|b| b.iter().find(|(t, _)| *t == tier))
        else {
            return Ok(());
        };
        draw.push();
        // The model's frame has +y down and its front towards -z; the entity's has +y up and +z forward. A part's
        // pose is in the model's, so the translation flips y and z and the turns about y and z their sign.
        let body = model.and_then(|m| m.part("body"));
        if let Some(part) = body {
            if !part.visible {
                draw.pop();
                return Ok(());
            }
            let [x, y, z, rx, ry, rz, ..] = *part.local_pose;
            draw.translate([x / 16.0, (24.0 - y) / 16.0, -z / 16.0]);
            draw.rotate_about([0.0, 0.0, 1.0], (-rz).to_degrees(), [0.0; 3]);
            draw.rotate_about([0.0, 1.0, 0.0], (-ry).to_degrees(), [0.0; 3]);
            draw.rotate_about([1.0, 0.0, 0.0], rx.to_degrees(), [0.0; 3]);
        } else {
            draw.translate([0.0, 1.5, 0.0]);
        }
        // The pack's back plane (11.5 of the block's 16) on the body's back, below its top by the body's height.
        let size = 0.8;
        draw.translate([-0.5 * size, -0.78, -0.125 - 0.71875 * size]);
        draw.scale([size, size, size]);
        draw.block(state);
        draw.pop();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn views_round_trip_and_clear() {
        assert_eq!(decode(&encode(70_000, Some(4)).0), Some((70_000, Some(4))));
        assert_eq!(decode(&encode(3, None).0), Some((3, None)));
        assert_eq!(decode(&[1, 2]), None);
        heard(&encode(9_001, Some(2)).0);
        assert_eq!(seen().get(&9_001), Some(&2));
        heard(&encode(9_001, None).0);
        assert_eq!(seen().get(&9_001), None);
    }
}
