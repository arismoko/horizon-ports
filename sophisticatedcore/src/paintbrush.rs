//! PaintbrushItem: a click on a storage paints it with the brush's colours, paid in the brush's leftover dye parts
//! first and then in dye items from the player's inventory.
use mod_api::{HostBlockStates, LevelRef, ParticleOptions, PlayerRef, ServerHost, SoundSource};

use crate::contents::{self, Nbt, custom_data, get_int, put, set_custom_data};
use crate::{places, storage, table};

pub const ID: &str = "sophisticatedstorage:paintbrush";
const MAIN: &str = "mainColor";
const ACCENT: &str = "accentColor";
const REMAINING: &str = "remainingParts";
const DYE_KEYS: [&str; 3] = ["c:dyes/red", "c:dyes/green", "c:dyes/blue"];

/// The brush's colours, `UNSET` for one it has none of.
pub fn colors(brush: &mod_api::ItemStack) -> (i32, i32) {
    let d = custom_data(brush);
    (
        get_int(&d, MAIN).unwrap_or(table::UNSET),
        get_int(&d, ACCENT).unwrap_or(table::UNSET),
    )
}

/// The dye parts the brush has left over, per channel.
pub fn remaining(brush: &mod_api::ItemStack) -> [i32; 3] {
    let d = custom_data(brush);
    let mut parts = [0; 3];
    if let Some(mod_api::Tag::Compound(c)) = d.at(REMAINING) {
        for (p, key) in parts.iter_mut().zip(DYE_KEYS) {
            *p = get_int(c, key).unwrap_or(0).max(0);
        }
    }
    parts
}

fn set_remaining(brush: &mut mod_api::ItemStack, parts: [i32; 3]) {
    let mut d = custom_data(brush);
    let mut c = mod_api::Compound::default();
    for (p, key) in parts.iter().zip(DYE_KEYS) {
        if *p > 0 {
            put(&mut c, key, mod_api::Tag::Int(*p));
        }
    }
    put(&mut d, REMAINING, mod_api::Tag::Compound(c));
    set_custom_data(brush, d);
}

/// What painting costs: the colours the storage ends up with, the parts needed, and the dye items to take, or `None`
/// when what the brush has left and the inventory's dyes cannot pay (`DecorationHelper.consumeDyes`).
#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    pub colors: (i32, i32),
    pub parts: [i32; 3],
    pub used: [u32; 3],
}

/// `PaintbrushItem.setColors`'s rule: the brush's colours (cut to rgb, an unset one is white) against the storage's
/// own. `None` when nothing changes or the dye is short.
pub fn plan(
    brush: (i32, i32),
    left: [i32; 3],
    have: [u32; 3],
    original: (i32, i32),
) -> Option<Plan> {
    let set = (brush.0 & 0xFF_FFFF, brush.1 & 0xFF_FFFF);
    if set == original {
        return None;
    }
    let need = table::parts_needed(set, original);
    let (parts, used) = table::spend(need, left, have);
    let paid = (0..3).all(|c| i64::from(used[c]) * 24 + i64::from(left[c]) >= i64::from(need[c]));
    paid.then_some(Plan {
        colors: set,
        parts,
        used,
    })
}

/// Paints the storage at `pos` (its main chest when it is a double one). Returns whether it changed.
fn paint(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    hand_slot: u32,
    brush: &mod_api::ItemStack,
    level: LevelRef,
    pos: [i32; 3],
    volume: f32,
) -> bool {
    let pos = storage::main_pos(&*host, level, pos);
    let Some(mut stack) = storage::read_be(&*host, level, pos) else {
        return false;
    };
    let data = custom_data(&stack);
    let original = (
        get_int(&data, MAIN).unwrap_or(table::UNSET),
        get_int(&data, ACCENT).unwrap_or(table::UNSET),
    );
    let inv = places::inventory(host, player);
    let mut have = [0u32; 3];
    for s in inv.iter().flatten() {
        if let Some(c) = table::dye_channel(&*host, s) {
            have[c] += s.count();
        }
    }
    let live = player_brush(&inv, hand_slot, brush);
    let Some(plan) = plan(colors(&live), remaining(&live), have, original) else {
        return false;
    };
    // Take the dye items, front to back.
    let mut owed = plan.used;
    for (slot, s) in inv.iter().enumerate() {
        let Some(s) = s else { continue };
        let Some(c) = table::dye_channel(&*host, s) else {
            continue;
        };
        if owed[c] == 0 {
            continue;
        }
        let take = owed[c].min(s.count());
        owed[c] -= take;
        let rest = contents::with_count(s, s.count() - take).filter(|r| r.count() > 0);
        places::set_slot(host, player, slot as u32, rest);
    }
    let mut paid = live;
    set_remaining(&mut paid, plan.parts);
    places::set_slot(host, player, hand_slot, Some(paid));
    let mut d = data;
    put(&mut d, MAIN, mod_api::Tag::Int(plan.colors.0));
    put(&mut d, ACCENT, mod_api::Tag::Int(plan.colors.1));
    set_custom_data(&mut stack, d);
    storage::write_be(host, level, pos, Some(stack));
    effects(host, level, pos, volume);
    true
}

/// The brush as the inventory holds it now (an earlier storage in the same click may have changed its parts).
fn player_brush(
    inv: &[Option<mod_api::ItemStack>],
    slot: u32,
    fallback: &mod_api::ItemStack,
) -> mod_api::ItemStack {
    inv.get(slot as usize)
        .and_then(Option::as_ref)
        .unwrap_or(fallback)
        .clone()
}

/// `playSoundAndParticles`: the block's place sound, and four happy villager sparkles around the block.
fn effects(host: &mut dyn ServerHost, level: LevelRef, pos: [i32; 3], volume: f32) {
    let at = pos.map(|v| f64::from(v) + 0.5);
    if let Some(sound) = level
        .block_state(&*host, pos)
        .and_then(|s| host.block_states().sound(s))
    {
        level.play_sound(
            host,
            &sound.place,
            at,
            SoundSource::Blocks,
            volume,
            1.0,
            None,
        );
    }
    if let Some(key) = crate::everlasting::SPARKLE.get() {
        level.send_particles(
            host,
            &ParticleOptions::simple(*key),
            None,
            at,
            [0.4, 0.1, 0.4],
            1.0,
            4,
        );
    }
}

/// A click on a storage paints it (`paintStorage`, volume 1); on a controller, every storage linked to it (volume
/// 0.6). `None` passes the click on: the brush has no colour, or the block is neither.
pub fn use_on(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    hand_slot: u32,
    brush: &mod_api::ItemStack,
    level: LevelRef,
    pos: [i32; 3],
    block: &str,
) -> bool {
    if colors(brush) == (table::UNSET, table::UNSET) {
        return false;
    }
    if storage::kind_of_block(block).is_some() {
        paint(host, player, hand_slot, brush, level, pos, 1.0);
        return true;
    }
    if block == crate::network::CONTROLLER {
        let net = crate::network::net_of(&*host, level, pos);
        let mut seen = Vec::new();
        for &p in &net.storages {
            let main = storage::main_pos(&*host, level, p);
            if !seen.contains(&main) {
                seen.push(main);
                paint(host, player, hand_slot, brush, level, main, 0.6);
            }
        }
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNSET: i32 = table::UNSET;

    #[test]
    fn nothing_changes_when_the_colours_already_match() {
        assert_eq!(
            plan((0xFF0000, UNSET), [0; 3], [9; 3], (0xFF0000, 0xFFFFFF)),
            None
        );
    }

    #[test]
    fn leftover_parts_pay_before_dye_items() {
        // Red 0xFF0000 as the main colour: 18 parts of red.
        let p = plan((0xFF0000, 0xFFFFFF), [20, 0, 0], [0; 3], (UNSET, 0xFFFFFF)).unwrap();
        assert_eq!(p.parts, [2, 0, 0]);
        assert_eq!(p.used, [0; 3]);
    }

    #[test]
    fn a_short_dye_takes_an_item_and_keeps_the_rest() {
        let p = plan(
            (0xFF0000, 0xFFFFFF),
            [4, 0, 0],
            [1, 0, 0],
            (UNSET, 0xFFFFFF),
        )
        .unwrap();
        assert_eq!(p.used, [1, 0, 0]);
        assert_eq!(p.parts, [10, 0, 0]);
    }

    #[test]
    fn too_little_dye_paints_nothing() {
        assert_eq!(
            plan((0xFF0000, 0xFFFFFF), [4, 0, 0], [0; 3], (UNSET, 0xFFFFFF)),
            None
        );
    }

    #[test]
    fn an_unset_brush_colour_is_painted_as_white() {
        let p = plan((0x00FF00, UNSET), [0; 3], [9; 3], (UNSET, UNSET)).unwrap();
        assert_eq!(p.colors, (0x00FF00, 0xFFFFFF));
    }
}
