//! Invisible Frames for Sunrise: sneak and use an item frame holding an item to toggle its
//! invisibility, as Invisible Frames 2.0.1 (Roundaround) does in `ItemFrame.interact`; a frame
//! that loses its item becomes visible again.
use mod_api::{
    EntityInteractEvent, EntityTickListenedEvent, EntityTickPostEvent, GameEventContext, Hooks,
    HostSide, InteractionResult, JavaString, ServerHost, Tag,
};

const FRAMES: [&str; 2] = ["minecraft:item_frame", "minecraft:glow_item_frame"];

/// Whether a frame's saved data (`ItemFrame.addAdditionalSaveData`) holds an item: vanilla
/// writes `Item` only for a non-empty stack.
fn holds_item(host: &mut dyn ServerHost, frame: mod_api::EntityRef) -> Option<bool> {
    Some(frame.nbt(host)?.get(&JavaString::from("Item")).is_some())
}

#[derive(Default)]
struct Mod;

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    /// Where vanilla's server `interact` would turn the item (a frame that is not fixed and
    /// holds an item), a crouching player toggles the frame's invisibility instead, with the
    /// same `block_change` game event and no turn or sound. The client keeps vanilla's
    /// answer, which succeeds on a frame with an item.
    fn entity_interact(
        &self,
        host: HostSide<'_>,
        event: &mut EntityInteractEvent<'_>,
    ) -> Option<InteractionResult> {
        let HostSide::Server(host) = host else {
            return None;
        };
        if !FRAMES.contains(&event.kind) {
            return None;
        }
        let player = host.players().acting_player()?.entity();
        if !player.is_crouching(&*host)? {
            return None;
        }
        let frame = host.entity(u32::try_from(event.entity).ok()?)?;
        let nbt = frame.nbt(host)?;
        let fixed = matches!(nbt.get(&JavaString::from("Fixed")), Some(Tag::Byte(b)) if *b != 0);
        if fixed || nbt.get(&JavaString::from("Item")).is_none() {
            return None;
        }
        let invisible = frame.is_invisible(&*host)?;
        frame.set_invisible(host, !invisible);
        if let (Some(level), Some(at)) = (frame.level(&*host), frame.position(&*host)) {
            let context = GameEventContext {
                source_entity: Some(player),
                ..Default::default()
            };
            level.game_event(host, "minecraft:block_change", at, context);
        }
        Some(InteractionResult::Success)
    }

    fn entity_tick_listened(&self, host: HostSide, event: &mut EntityTickListenedEvent) {
        if matches!(host, HostSide::Server(_)) {
            event.listened = true;
        }
    }

    /// The original's `onSyncedDataUpdated`: an invisible frame whose item is gone (taken,
    /// knocked out or emptied by a command) becomes visible again.
    fn entity_tick_post(&self, host: HostSide, event: &mut EntityTickPostEvent<'_>) {
        let HostSide::Server(host) = host else {
            return;
        };
        for (&id, kind) in event.entities.iter().zip(event.kinds) {
            if !FRAMES.contains(kind) {
                continue;
            }
            let Some(frame) = host.entity(id) else {
                continue;
            };
            if frame.is_invisible(&*host) == Some(true) && holds_item(host, frame) == Some(false) {
                frame.set_invisible(host, false);
            }
        }
    }
}
