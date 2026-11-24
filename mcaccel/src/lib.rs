//! Minecraft Accelerated for Sunrise: shears carve nine faces into carved pumpkins,
//! jack o'lanterns and soul jack o'lanterns, one after another, and carve them back.
//!
//! | Moment | What it does |
//! |---|---|
//! | `register` | looks up the blocks and items once |
//! | `blocks` | the 28 blocks of `BlockInit`, each with its block item |
//! | `right_click_block` | shears carve the next face (`CarvedPumpkinModifier.onUse`) |
//! | `left_click_block` | shears carve the face before (`LeftClickPumpkinEvent`) |
//! | `build_creative_mode_tab_contents` | the blocks after the carved pumpkin in Natural Blocks |
//!
//! A worn carved face blurs the view through its item's `equippable` camera overlay
//! (`AddNewPumpkinOverlays`), and endermen don't see a player wearing one through the item tag
//! `minecraft:gaze_disguise_equipment` (`EndermanIgnorePumpkins`): both are data.

use std::sync::OnceLock;

use mod_api::{
    Block, BlockKey, BlockProperties, BlocksEvent, BuildCreativeModeTabContentsEvent, Compound,
    CreativeTabPlacement, CreativeTabVisibility, EquipmentSlot, Flow, GameEventContext, Hand,
    Hooks, Host, HostBlockStates, HostSide, InteractionResult, ItemKey, ItemStack,
    LeftClickBlockEvent, LevelRef, LivingRef, NoteInstrument, PlayerRef, PushReaction, Registrar,
    RegistrationContext, RightClickBlockEvent, ServerHost, SoundSource, StatType, Tag,
};

const MOD: &str = "mcaccel";

/// The faces, in the order shears carve them.
const FACES: [&str; 9] = [
    "x",
    "smile",
    "derp",
    "check",
    "creeper",
    "herobrine",
    "evil",
    "heart",
    "shout",
];

/// The three cycles: the block each starts from, then the prefix of its nine faces.
const CYCLES: [(&str, &str); 3] = [
    ("minecraft:carved_pumpkin", "carved_pumpkin"),
    ("minecraft:jack_o_lantern", "jack_o_lantern"),
    ("mcaccel:soul_jack_o_lantern_normal", "soul_jack_o_lantern"),
];

/// `BlockInit.PUMPKIN_BLOCKS`: the mod's blocks, in its order.
fn block_names() -> Vec<String> {
    let mut names: Vec<String> = CYCLES
        .iter()
        .flat_map(|(_, prefix)| FACES.iter().map(move |face| format!("{prefix}_{face}")))
        .collect();
    names.push("soul_jack_o_lantern_normal".to_owned());
    names
}

/// The block after (`forward`) or before `block` in its cycle. Going forward, the soul cycle
/// leaves its plain lantern out, as the original's map does: shout goes back to x, and shears
/// on a plain soul jack o'lantern do nothing.
fn step<T: Copy + PartialEq>(cycles: &[[T; 10]; 3], block: T, forward: bool) -> Option<T> {
    cycles.iter().enumerate().find_map(|(i, cycle)| {
        let at = cycle.iter().position(|b| *b == block)?;
        match (forward, i) {
            (true, 2) => (at != 0).then(|| cycle[at % 9 + 1]),
            (true, _) => Some(cycle[(at + 1) % 10]),
            (false, _) => Some(cycle[(at + 9) % 10]),
        }
    })
}

/// The facing a carved face takes: the side clicked, or for the top or the bottom the side
/// facing the player (`Direction.fromYRot(yRot).getOpposite()`).
fn facing(face: [i32; 3], y_rot: f32) -> &'static str {
    match face {
        [0, 0, -1] => "north",
        [0, 0, 1] => "south",
        [-1, 0, 0] => "west",
        [1, 0, 0] => "east",
        _ => ["north", "east", "south", "west"]
            [((f64::from(y_rot) / 90.0 + 0.5).floor() as i32 & 3) as usize],
    }
}

/// A carved face worn on the head: the view blurs through its own overlay, as a carved
/// pumpkin's does; a dispenser drops it and a right click places it, as the original's
/// block items do.
fn equippable(face: &str) -> Tag {
    let mut tag = Compound::new();
    tag.insert("slot".into(), Tag::String("head".into()));
    tag.insert(
        "camera_overlay".into(),
        Tag::String(format!("{MOD}:misc/pumpkin_blur_{face}").as_str().into()),
    );
    tag.insert("dispensable".into(), Tag::Byte(0));
    tag.insert("swappable".into(), Tag::Byte(0));
    Tag::Compound(tag)
}

struct Keys {
    cycles: [[BlockKey; 10]; 3],
    /// The mod's block items, in `BlockInit.PUMPKIN_BLOCKS` order.
    items: Vec<ItemKey>,
    carved_pumpkin: ItemKey,
    shears: ItemKey,
}

static KEYS: OnceLock<Keys> = OnceLock::new();

fn keys() -> &'static Keys {
    KEYS.get()
        .expect("the mod's keys are looked up at registration")
}

/// Plays the carving sound at `pos` and turns the block there into `block`, facing `facing`.
fn carve(
    host: &mut dyn ServerHost,
    level: LevelRef,
    pos: [i32; 3],
    block: BlockKey,
    facing: &str,
    flags: mod_api::BlockUpdateFlags,
) {
    let centre = pos.map(|v| f64::from(v) + 0.5);
    level.play_sound(
        host,
        "minecraft:block.pumpkin.carve",
        centre,
        SoundSource::Blocks,
        1.0,
        1.0,
        None,
    );
    let Some(state) = host
        .block_state_default(block)
        .and_then(|state| state.with(&host.block_states(), "facing", facing).ok())
    else {
        return;
    };
    let _ = level.set_block(host, pos, state, flags);
}

/// The block the shears in this click carve the clicked block into, or `None` when the click
/// isn't one: not shears, a spectator, a sneaking player (whose full hand skips block use), or a
/// block outside the cycles.
fn next_face(
    host: &dyn Host,
    player: PlayerRef,
    click: &mod_api::BlockClick<'_>,
) -> Option<BlockKey> {
    let keys = keys();
    if click.stack?.item() != keys.shears
        || player.is_spectator(host)?
        || player.is_secondary_use_active(host)?
    {
        return None;
    }
    let level = host.level(click.dimension.key().id())?;
    let block = level.block_state(host, click.pos)?.block();
    step(&keys.cycles, block, true)
}

#[derive(Default)]
struct Mod;

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        let names = block_names();
        let cycles = std::array::from_fn(|i| {
            std::array::from_fn(|at| match at {
                0 => r.block_key(CYCLES[i].0),
                _ => r.block_key(&format!("{MOD}:{}", names[i * 9 + at - 1])),
            })
        });
        let items = names
            .iter()
            .map(|name| r.item_key(&format!("{MOD}:{name}")))
            .collect();
        let _ = KEYS.set(Keys {
            cycles,
            items,
            carved_pumpkin: r.item_key("minecraft:carved_pumpkin"),
            shears: r.item_key("minecraft:shears"),
        });
    }

    /// `FabricBlockSettings.copyOf(Blocks.PUMPKIN).pistonBehavior(DESTROY)`, the lanterns with
    /// light 15. The carved faces are wearable carved pumpkins, the lanterns carved pumpkin blocks.
    fn blocks(&self, _: &RegistrationContext, event: &mut BlocksEvent<'_>) {
        for name in block_names() {
            let face = name.strip_prefix("carved_pumpkin_");
            let template = match face {
                Some(_) => "minecraft:carved_pumpkin",
                None => "minecraft:jack_o_lantern",
            };
            let mut block = Block::new(format!("{MOD}:{name}"), template).with_item(true);
            block.properties = BlockProperties {
                hardness: Some(1.0),
                explosion_resistance: Some(1.0),
                light: Some(if face.is_some() { 0 } else { 15 }),
                // MapColor.COLOR_ORANGE.
                map_color: Some(15),
                sounds: Some("minecraft:pumpkin".into()),
                instrument: Some(NoteInstrument::Didgeridoo),
                push_reaction: Some(PushReaction::Popped),
                ..Default::default()
            };
            if let Some(face) = face {
                block.components = vec![("minecraft:equippable".into(), Some(equippable(face)))];
            }
            // A refused declaration is reported at startup with its reason.
            let _ = event.register(block);
        }
    }

    /// Shears on a block of the cycles carve its next face. The client claims the click, so the
    /// hand swings and the click ends, as the original's `ActionResult.success` does.
    fn right_click_block(&self, host: HostSide<'_>, event: &mut RightClickBlockEvent<'_>) -> Flow {
        let click = event.click;
        let next = match &host {
            HostSide::Server(host) => host
                .player(click.player)
                .and_then(|player| next_face(&**host, player, click)),
            HostSide::Client(host) => host
                .local_player()
                .and_then(|player| next_face(&**host, player, click)),
        };
        let Some(next) = next else {
            return Flow::Continue;
        };
        if let HostSide::Server(host) = host
            && let Some(player) = host.player(click.player)
            && let Some(level) = host.level(click.dimension.key().id())
            && let Some(original) = click.stack.cloned()
        {
            let y_rot = player.entity().rotation(&*host).map_or(0.0, |[yaw, _]| yaw);
            let flags = mod_api::BlockUpdateFlags::NEIGHBORS | mod_api::BlockUpdateFlags::CLIENTS;
            carve(
                host,
                level,
                click.pos,
                next,
                facing(click.hit.face, y_rot),
                flags,
            );
            let slot = match click.hand {
                Hand::Main => EquipmentSlot::MainHand,
                Hand::Off => EquipmentSlot::OffHand,
            };
            let _ = LivingRef::new(player.entity()).hurt_and_break(host, slot, 1);
            let centre = click.pos.map(|v| f64::from(v) + 0.5);
            let context = GameEventContext {
                source_entity: Some(player.entity()),
                ..Default::default()
            };
            let _ = level.game_event(host, "minecraft:shear", centre, context);
            let _ = player.award_stat(host, StatType::Used, "minecraft:shears", 1);
            // A block's use with an item counts for `item_used_on_block` with the stack as it was.
            let _ = player.trigger_item_used_on_block(host, click.pos, &original);
        }
        *event.cancellation_result = InteractionResult::Success;
        Flow::Cancel
    }

    /// Shears in the main hand left-clicking a block of the cycles carve the face before it. The
    /// dig goes on as usual.
    fn left_click_block(&self, host: &mut dyn ServerHost, event: &mut LeftClickBlockEvent) -> Flow {
        let keys = keys();
        let Some(player) = host.players().acting_player() else {
            return Flow::Continue;
        };
        let shears = LivingRef::new(player.entity())
            .with_hand_item(&*host, Hand::Main, |stack| stack.item() == keys.shears)
            .unwrap_or(false);
        let Some(previous) = shears
            .then(|| step(&keys.cycles, event.state.block(), false))
            .flatten()
        else {
            return Flow::Continue;
        };
        let Some(level) = player.level(&*host) else {
            return Flow::Continue;
        };
        let y_rot = player.entity().rotation(&*host).map_or(0.0, |[yaw, _]| yaw);
        let flags = mod_api::BlockUpdateFlags::NEIGHBORS | mod_api::BlockUpdateFlags::CLIENTS;
        carve(
            host,
            level,
            event.pos,
            previous,
            facing(event.face, y_rot),
            flags,
        );
        Flow::Continue
    }

    /// `ItemGroupInit`: each block goes right after the carved pumpkin in Natural Blocks, in
    /// `PUMPKIN_BLOCKS` order, so the last one ends up first.
    fn build_creative_mode_tab_contents(
        &self,
        host: HostSide,
        event: &mut BuildCreativeModeTabContentsEvent<'_>,
    ) {
        if event.tab != "minecraft:natural_blocks" {
            return;
        }
        let host: &dyn Host = match &host {
            HostSide::Server(host) => &**host,
            HostSide::Client(host) => &**host,
        };
        let keys = keys();
        for item in &keys.items {
            let _ = event.edit.insert(
                host,
                CreativeTabPlacement::After(ItemStack::of(keys.carved_pumpkin)),
                ItemStack::of(*item),
                CreativeTabVisibility::Both,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cycles() -> [[String; 10]; 3] {
        let names = block_names();
        std::array::from_fn(|i| {
            std::array::from_fn(|at| match at {
                0 => CYCLES[i].0.to_owned(),
                _ => format!("{MOD}:{}", names[i * 9 + at - 1]),
            })
        })
    }

    fn step_name(name: &str, forward: bool) -> Option<String> {
        let cycles = cycles();
        let refs: [[&str; 10]; 3] =
            std::array::from_fn(|i| std::array::from_fn(|at| cycles[i][at].as_str()));
        step(&refs, name, forward).map(str::to_owned)
    }

    #[test]
    fn the_blocks_are_the_originals_in_its_order() {
        let names = block_names();
        assert_eq!(names.len(), 28);
        assert_eq!(names[0], "carved_pumpkin_x");
        assert_eq!(names[17], "jack_o_lantern_shout");
        assert_eq!(names[27], "soul_jack_o_lantern_normal");
    }

    #[test]
    fn shears_carve_forward_through_each_cycle() {
        assert_eq!(
            step_name("minecraft:carved_pumpkin", true).as_deref(),
            Some("mcaccel:carved_pumpkin_x")
        );
        assert_eq!(
            step_name("mcaccel:carved_pumpkin_x", true).as_deref(),
            Some("mcaccel:carved_pumpkin_smile")
        );
        assert_eq!(
            step_name("mcaccel:carved_pumpkin_shout", true).as_deref(),
            Some("minecraft:carved_pumpkin")
        );
        assert_eq!(
            step_name("mcaccel:jack_o_lantern_shout", true).as_deref(),
            Some("minecraft:jack_o_lantern")
        );
        assert_eq!(
            step_name("mcaccel:soul_jack_o_lantern_shout", true).as_deref(),
            Some("mcaccel:soul_jack_o_lantern_x")
        );
        assert_eq!(step_name("mcaccel:soul_jack_o_lantern_normal", true), None);
        assert_eq!(step_name("minecraft:pumpkin", true), None);
    }

    #[test]
    fn shears_carve_back_through_each_cycle() {
        assert_eq!(
            step_name("minecraft:jack_o_lantern", false).as_deref(),
            Some("mcaccel:jack_o_lantern_shout")
        );
        assert_eq!(
            step_name("mcaccel:carved_pumpkin_x", false).as_deref(),
            Some("minecraft:carved_pumpkin")
        );
        assert_eq!(
            step_name("mcaccel:soul_jack_o_lantern_x", false).as_deref(),
            Some("mcaccel:soul_jack_o_lantern_normal")
        );
        assert_eq!(
            step_name("mcaccel:soul_jack_o_lantern_normal", false).as_deref(),
            Some("mcaccel:soul_jack_o_lantern_shout")
        );
        assert_eq!(step_name("minecraft:soul_lantern", false), None);
    }

    #[test]
    fn a_side_click_faces_that_side_and_the_top_faces_the_player() {
        assert_eq!(facing([1, 0, 0], 0.0), "east");
        assert_eq!(facing([0, 0, -1], 0.0), "north");
        // Looking south (yaw 0): the face looks north, back at the player.
        assert_eq!(facing([0, 1, 0], 0.0), "north");
        // Looking west (yaw 90): east. 44.9 still rounds to south.
        assert_eq!(facing([0, -1, 0], 90.0), "east");
        assert_eq!(facing([0, 1, 0], 44.9), "north");
        assert_eq!(facing([0, 1, 0], 45.0), "east");
        assert_eq!(facing([0, 1, 0], -90.0), "west");
        assert_eq!(facing([0, 1, 0], 180.0), "south");
        assert_eq!(facing([0, 1, 0], -540.0), "south");
    }
}
