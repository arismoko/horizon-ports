//! Convenient Name Tags 1.1.0 (Mim1q, Apache-2.0) for Sunrise.
//!
//! | Moment | What it does |
//! |---|---|
//! | `living_drops` | a named mob that dies drops a name tag with its name |
//! | `entity_interact` | sneak + shears on a named mob take the name off as a tag |
//! | `entity_interact` | a name tag with the mob's own name isn't used up; another name drops the old one |

mod rules;

use mod_api::{
    EntityInteractEvent, EntityRef, EquipmentSlot, Flow, Hand, Hooks, HostSide, Identifier,
    InteractionResult, ItemDrop, ItemStack, LivingDropsEvent, LivingRef, ServerHost, Text,
};
use rules::Click;

#[derive(Default)]
struct Mod;

/// `new ItemStack(NAME_TAG).setCustomName(name)`: the tag takes the mob's whole name component.
fn name_tag(host: &dyn mod_api::Host, name: &Text) -> Option<ItemStack> {
    let key = host.item_key(&Identifier::parse(rules::NAME_TAG).ok()?)?;
    let mut tag = ItemStack::new(key, 1)?;
    if tag.set_custom_name(host, Some(name)).is_err() {
        return None;
    }
    Some(tag)
}

/// Drops the name tag of `mob` at its position.
fn drop_tag(host: &mut dyn ServerHost, mob: EntityRef, name: &Text) -> Option<()> {
    let tag = name_tag(&*host, name)?;
    let level = mob.level(&*host)?;
    let [x, y, z] = mob.position(&*host)?;
    level.spawn_item(host, x, y, z, tag, 10).ok()?;
    Some(())
}

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    /// `LivingEntityMixin`: a named mob's name drops as a tag with its other drops.
    fn living_drops(&self, host: &mut dyn ServerHost, event: &mut LivingDropsEvent<'_>) -> Flow {
        if event.kind == rules::PLAYER {
            return Flow::Continue;
        }
        let Some(mob) = host.entity(event.entity) else {
            return Flow::Continue;
        };
        let (Some(name), Some(pos)) = (
            mob.custom_name(&*host).ok().flatten(),
            mob.position(&*host),
        ) else {
            return Flow::Continue;
        };
        if let Some(tag) = name_tag(&*host, &name) {
            event.drops.push(ItemDrop::new(tag, pos, [0.0, 0.2, 0.0], 10));
        }
        Flow::Continue
    }

    /// `ShearsItemMixin`, `NameTagItemMixin` and `CancelInteractMobMixin`.
    fn entity_interact(
        &self,
        host: HostSide<'_>,
        event: &mut EntityInteractEvent<'_>,
    ) -> Option<InteractionResult> {
        let HostSide::Server(host) = host else {
            return None;
        };
        let stack = event.stack?;
        let sneaking = host.players().acting_player()?.is_secondary_use_active(&*host) == Some(true);
        let mob = host.entity(u32::try_from(event.entity).ok()?)?;
        let mob_name = mob.custom_name(&*host).ok().flatten();
        let tag_name = stack.custom_name(&*host).ok().flatten();
        let held = stack.item().id(&*host)?.to_owned();
        let action = rules::classify(
            event.kind,
            &held,
            sneaking,
            mob_name.as_ref(),
            tag_name.as_ref(),
        );
        match action {
            Click::Pass => None,
            Click::KeepTag => Some(InteractionResult::Fail),
            Click::DropOldName => {
                drop_tag(host, mob, &mob_name?);
                None
            }
            Click::ShearName => {
                drop_tag(host, mob, &mob_name?)?;
                mob.set_custom_name(host, None).ok()?;
                let slot = match event.hand {
                    Hand::Main => EquipmentSlot::MainHand,
                    Hand::Off => EquipmentSlot::OffHand,
                };
                let player = host.players().acting_player()?;
                LivingRef::new(player.entity()).hurt_and_break(host, slot, 1);
                Some(InteractionResult::Success)
            }
        }
    }
}
