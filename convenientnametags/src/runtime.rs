// Convenient Name Tags for Horizon: the name tag that comes off a mob.
//
// Injected as `crate::game::creatures::convenientnametags`.

use super::Game;
use horizon_client::{items::ItemStack, mobs::Mob};

/// `new ItemEntity(world, x, y, z, new ItemStack(NAME_TAG).setCustomName(entity.getCustomName()))`,
/// spawned at the mob's position. The tag takes the mob's whole name component (`custom_name_tag`,
/// style included), so a coloured name comes back coloured. Nothing drops without a name.
pub(crate) fn drop_tag(game: &mut Game<'_>, m: &Mob) {
    let Some(name) = m.custom_name_tag() else {
        return;
    };
    let mut tag = ItemStack::new("minecraft:name_tag", 1);
    tag.data_mut().custom_name = Some(name);
    let pos = [m.position.x, m.position.y, m.position.z];
    game.spawn_item_own(tag, pos, 0);
}

/// `entity.getCustomName().equals(stack.getName())`: the mob's whole name against the tag's
/// whole `custom_name` component, style included, so a colour-only rename is a change.
pub(crate) fn same_name(m: &Mob, tag: &ItemStack) -> bool {
    let held = tag
        .data()
        .and_then(|d| d.other("minecraft:custom_name"))
        .map(horizon_client::entity_nbt::name_text);
    match (m.display_custom_name(), held) {
        (Some(mob), Some(held)) => mob == held,
        _ => false,
    }
}
