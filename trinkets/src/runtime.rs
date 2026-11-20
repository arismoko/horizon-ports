// Trinkets 3.7.2 (Emi, MIT), the game-side halves no hook covers: a worn Totem of Undying and the
// death drop. Injected as `crate::game::trinkets`; the rest is the mod crate's Hooks (src/lib.rs).
use super::*;
use horizon_core::nbt::Compound;
use mod_api::{AttachmentCodec, AttachmentError, AttachmentType, Holder};

fn same(c: &Compound) -> std::result::Result<Compound, AttachmentError> {
    Ok(c.clone())
}
fn empty() -> Compound {
    Compound::new()
}
/// The worn trinkets (slot id -> item NBT), the same key and type as the mod crate's `WORN`.
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

/// The worn stacks of `uuid`, one per slot of the table.
fn worn(game: &mut Game<'_>, uuid: u128) -> Vec<Option<ItemStack>> {
    let c = WORN.get(game, Holder::Player(uuid)).unwrap_or_default();
    mod_trinkets::SLOTS
        .iter()
        .map(|s| c.get(&s.0.into()).and_then(horizon_client::save::item_from_nbt))
        .collect()
}

/// Empties the slots `ids` of `uuid`'s worn attachment. Every other saved entry (items this build
/// can't make, ids that aren't slots of this table) is kept as it is, as the menu writer does.
fn clear_slots(game: &mut Game<'_>, uuid: u128, ids: &[&str]) {
    let mut c = WORN.get(game, Holder::Player(uuid)).unwrap_or_default();
    for id in ids {
        c.remove(&(*id).into());
    }
    let _ = WORN.set(game, Holder::Player(uuid), c);
}

/// Player.dropInventory's trinket half when no grave took them: unless keepInventory, they drop
/// where the player died; vanishing-cursed ones are destroyed.
pub(crate) fn drop_on_death(game: &mut Game<'_>, at: [f64; 3]) {
    if game.ui.rules.bool_at(horizon_client::gamerules::vanilla::KEEP_INVENTORY) {
        return;
    }
    let Some(uuid) = game.lan_acting_uuid else { return };
    let worn = worn(game, uuid);
    if worn.iter().all(Option::is_none) {
        return;
    }
    let dropped: Vec<&str> =
        mod_trinkets::SLOTS.iter().zip(&worn).filter(|(_, s)| s.is_some()).map(|(s, _)| s.0).collect();
    clear_slots(game, uuid, &dropped);
    let pos = [at[0].floor() as i32, at[1].floor() as i32, at[2].floor() as i32];
    for s in worn.into_iter().flatten() {
        if s.enchantment_level("minecraft:vanishing_curse") == 0 {
            game.pop_resource(pos, s);
        }
    }
}

const TOTEM: &str = "minecraft:totem_of_undying";

/// LivingEntity.checkTotemDeathProtection's readiness: a worn totem counts (Friends & Foes'
/// charm slot). Hands come first: the game's own check then runs.
pub(crate) fn totem_ready(game: &mut Game<'_>) -> bool {
    let in_hand = game.held().is_some_and(|s| game.has_death_protection(s))
        || game.offhand.as_ref().is_some_and(|s| game.has_death_protection(s));
    if in_hand {
        return false;
    }
    let Some(uuid) = acting(game) else { return false };
    if !worn(game, uuid).iter().flatten().any(|s| &*s.item == TOTEM) {
        return false;
    }
    if let Some(p) = game.player.as_mut() {
        p.health.totem_ready = true;
        p.health.totem_off_hand = true;
    }
    true
}

/// The death protection from a worn totem: it moves to the off hand for the original to use up.
pub(crate) fn apply_worn_totem(game: &mut Game<'_>) -> Option<Option<super::totem::Body>> {
    let used = game.player.as_ref().is_some_and(|p| p.health.totem_used);
    let in_hand = game.held().is_some_and(|s| game.has_death_protection(s))
        || game.offhand.as_ref().is_some_and(|s| game.has_death_protection(s));
    if !used || in_hand {
        return None;
    }
    let uuid = acting(game)?;
    let mut worn = worn(game, uuid);
    let i = worn.iter().position(|s| s.as_ref().is_some_and(|s| &*s.item == TOTEM))?;
    let totem = worn[i].take();
    clear_slots(game, uuid, &[mod_trinkets::SLOTS[i].0]);
    let saved = std::mem::replace(&mut game.offhand, totem);
    let body = game.apply_totem();
    game.offhand = saved;
    Some(body)
}
