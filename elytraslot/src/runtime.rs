// Elytra Slot 6.4.4 (Illusive Soulworks, LGPL-3.0-or-later): an elytra (anything that glides as a
// chestplate) worn in a trinket slot (the cape slot; native/trinkets) lets the player glide, wears
// as the chest's would, and draws its wings (ElytraSlotLayer). Injected as `crate::game::elytraslot`.
use super::*;
use mod_api::{AttachmentCodec, AttachmentError, AttachmentType, Holder};
use horizon_core::nbt::{Compound, Tag};

fn same(c: &Compound) -> std::result::Result<Compound, AttachmentError> {
    Ok(c.clone())
}
fn empty() -> Compound {
    Compound::new()
}
/// native/trinkets' worn trinkets (slot id -> item NBT): the same key and type read the same
/// value, so this bundle needs no code of native/trinkets (nothing is worn without it).
static WORN: AttachmentType<Compound> = AttachmentType {
    namespace: "trinkets",
    name: "worn",
    default: empty,
    codec: Some(AttachmentCodec { encode: same, decode: same }),
    copy_on_death: true,
    sync: true,
};

fn own_uuid(game: &Game<'_>) -> Option<u128> {
    match &game.remote {
        Some(r) => Some(r.profile()),
        None => game.level.player_uuid.map(|i| i.iter().fold(0u128, |a, &v| a << 32 | u128::from(v as u32))),
    }
}

fn acting(game: &Game<'_>) -> Option<u128> {
    game.lan_acting_uuid.or_else(|| own_uuid(game))
}

/// The worn stacks of `uuid` where only `&Game` is at hand, read from the holder's saved form
/// without inserting anything; a remote client reads its synced copy (none for a player whose
/// attachment the server didn't sync).
fn worn_view(game: &Game<'_>, uuid: u128) -> Vec<(String, ItemStack)> {
    let att: Option<&mod_api::Attachments> = if game.native_authoritative() {
        if game.lan_acting_uuid == Some(uuid) {
            Some(&game.player_state.attachments)
        } else {
            game.lan
                .as_ref()
                .and_then(|host| host.hub.guests.iter().find(|g| g.uuid == uuid))
                .map(|g| &*g.attachments)
        }
    } else {
        game.remote.as_ref().and_then(|r| r.attachments.get(&Holder::Player(uuid))).map(|a| &**a)
    };
    let nbt = att.map(|a| a.to_nbt());
    let value = nbt
        .as_ref()
        .and_then(|n| match n.get(&"trinkets".into()) {
            Some(Tag::Compound(ns)) => Some(ns),
            _ => None,
        })
        .and_then(|ns| match ns.get(&"worn".into()) {
            Some(Tag::Compound(e)) => Some(e),
            _ => None,
        })
        .and_then(|e| match e.get(&"value".into()) {
            Some(Tag::Compound(v)) => Some(v),
            _ => None,
        });
    value
        .map(|v| {
            v.iter()
                .filter_map(|(k, t)| horizon_client::save::item_from_nbt(t).map(|s| (String::from_utf16_lossy(&k.0), s)))
                .collect()
        })
        .unwrap_or_default()
}

/// A worn glider: its slot id and stack.
fn worn_glider(game: &Game<'_>, worn: &[(String, ItemStack)]) -> Option<(String, ItemStack)> {
    let items = game.res.items();
    worn.iter().find(|(_, s)| horizon_client::item_sync::can_glide_using(s, "chest", items)).cloned()
}

/// LivingEntity.canGlide with ElytraSlot: also a glider in a trinket slot.
pub(crate) fn glider_worn(game: &Game<'_>, value: bool) -> bool {
    value || acting(game).is_some_and(|uuid| worn_glider(game, &worn_view(game, uuid)).is_some())
}

/// LivingEntity.updateFallFlying's wear: with no glider in the armour, the trinket one wears.
pub(crate) fn wear_glider(game: &mut Game<'_>, points: u32) -> bool {
    if super::glide_wear::gliding_slots(
        game.res.items(),
        &game.armor,
        game.inventory[game.selected].as_ref(),
        game.offhand.as_ref(),
    )
    .next()
    .is_some()
    {
        return false;
    }
    let Some(uuid) = acting(game) else { return false };
    let Ok(mut c) = WORN.get(game, Holder::Player(uuid)) else { return false };
    let worn: Vec<(String, ItemStack)> = c
        .iter()
        .filter_map(|(k, t)| horizon_client::save::item_from_nbt(t).map(|s| (String::from_utf16_lossy(&k.0), s)))
        .collect();
    let Some((id, mut stack)) = worn_glider(game, &worn) else { return false };
    let loss = game.wear(&stack, points);
    stack.damage += loss;
    c.insert(id.as_str().into(), horizon_client::save::item_to_nbt(&stack));
    let _ = WORN.set(game, Holder::Player(uuid), c);
    true
}

/// The player the avatar `m` is: another player's (a mob of the level, on a server or a LAN
/// guest) by its entity id, else the window's own model (`player_model` or its inventory copy).
fn wearer(game: &Game<'_>, m: &horizon_client::mobs::Mob) -> Option<u128> {
    if !game.mobs.iter().any(|x| std::ptr::eq(x, m)) {
        return acting(game);
    }
    match &game.remote {
        Some(r) => r.player_uuid(m.id),
        None => game.lan.as_ref()?.hub.guests.iter().find(|g| g.entity_id as u32 == m.id).map(|g| g.uuid),
    }
}

/// ElytraSlotLayer for any player avatar: the trinket glider's wings of the player `m` is, when
/// the chest has none.
pub(crate) fn wings(
    game: &Game<'_>,
    m: &horizon_client::mobs::Mob,
    chest: Option<&str>,
    camera: glam::DVec3,
    partial: f32,
    light: u32,
    out: &mut Vec<horizon_client::mesher::Vertex>,
) -> bool {
    let items = game.res.items();
    if chest.is_some_and(|c| items.get(c).is_some_and(|i| i.glider)) {
        return false;
    }
    let Some(uuid) = wearer(game, m) else { return false };
    let Some((_, stack)) = worn_glider(game, &worn_view(game, uuid)) else { return false };
    game.wings_geometry(
        Some(&*stack.item),
        Some(&stack),
        m.cape.as_deref(),
        false,
        horizon_client::mobs::render::base_transform(m, partial, camera),
        m.extra.elytra.angles(partial),
        m.extra.crouching,
        light,
        game.wings_overlay(Some(&stack)),
        out,
    );
    true
}

