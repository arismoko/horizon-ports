//! Tokimi's Toolshed for Sunrise: the items are declared in `data/tokimistoolshed/sunrise/`
//! (with their recipes, tags and assets); this crate is what they do, on the typed mod API only.
//!
//! | Moment | What it does |
//! |---|---|
//! | `left_click_block` | remembers the face a block was hit on (`LastBreakData.setFace`) |
//! | `player_destroyed_block` | hammer, excavator, scythe and clippers take the 3x3 around the block (in the plane of the face hit), the lumber axe the tree |
//! | `right_click_block` | the scythe tills the eight around a tilled block; the abacus binds and unbinds |
//! | `item_interact_mob` (copper bucket) | milks a cow or a mooshroom |
//! | `living_entity_use_item_finish` | the copper milk bucket comes back with a use of wear |
//! | `client_tick_post` | the abacus's counts over the hotbar |
//!
//! What cannot be written on the mod API yet is in `README.md`.

pub mod rules;

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use mod_api::{
    BlockHit, BlockKey, BlockUpdateFlags, ClientHost, ClientTickPostEvent, Compound, EquipmentSlot,
    Flow, GameMode, Hand, Hooks, Host, HostBlockStates, HostSide, Identifier, InteractionResult,
    ItemInteractMobEvent, ItemPart, ItemStack, LeftClickBlockEvent, LevelRef,
    LivingEntityUseItemFinishEvent, LivingRef, PlayerDestroyedBlockEvent, PlayerRef, Registrar,
    RegistrationContext, RightClickBlockEvent, ServerHost, SoundSource, Tag, Text,
};
use rules::{AbacusUse, Kind, Probe};

/// The block last hit and the face it was hit on.
type FaceHit = ([i32; 3], [i32; 3]);

/// Whether `key` is in the block tag `tag`.
fn block_in_tag(host: &dyn Host, key: BlockKey, tag: &str) -> bool {
    let tags = host.tags();
    Identifier::parse(tag)
        .ok()
        .and_then(|id| tags.key(&id))
        .and_then(|tag| tags.block_in_tag(key, tag))
        == Some(true)
}

fn is_air(id: &str) -> bool {
    matches!(
        id,
        "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
    )
}

/// The stack in the player's main hand.
fn main_hand(host: &dyn Host, player: PlayerRef) -> Option<ItemStack> {
    LivingRef::new(player.entity())
        .with_hand_item(host, Hand::Main, |held| held.project())
        .flatten()
}

/// What the lumber axe's search asks of the block at `pos`.
fn probe(host: &dyn Host, level: LevelRef, pos: [i32; 3], log: &str) -> Probe {
    let Some(key) = level.block(host, pos) else {
        return Probe::default();
    };
    let id = key.id(host).unwrap_or_default();
    let tag = |name: &str| block_in_tag(host, key, name);
    let persistent = level.block_property(host, pos, "persistent") == Some("true");
    let natural = is_air(id)
        || matches!(id, "minecraft:water" | "minecraft:lava")
        || level.block_property(host, pos, "waterlogged") == Some("true")
        || [
            "minecraft:logs",
            "minecraft:leaves",
            "minecraft:dirt",
            "minecraft:sand",
            "minecraft:snow",
            "minecraft:beehives",
            "minecraft:fire",
            "minecraft:replaceable",
        ]
        .iter()
        .any(|name| tag(name))
        || rules::VEGETATION.contains(&id)
        || rules::VEGETATION_TAGS.iter().any(|name| tag(name));
    Probe {
        same_log: id == log,
        natural,
        leaf: tag("minecraft:leaves") && !persistent,
    }
}

/// The ints and other values of a stack's `minecraft:custom_data`.
fn custom_data(host: &dyn Host, stack: &ItemStack) -> Compound {
    let Some(item) = stack.to_nbt(host) else {
        return Compound::new();
    };
    let Some(Tag::Compound(components)) = item.get(&"components".into()) else {
        return Compound::new();
    };
    match components.get(&"minecraft:custom_data".into()) {
        Some(Tag::Compound(data)) => data.clone(),
        _ => Compound::new(),
    }
}

fn custom_int(data: &Compound, key: &str) -> Option<i32> {
    match data.get(&key.into()) {
        Some(Tag::Int(v)) => Some(*v),
        _ => None,
    }
}

/// `stack` with its `minecraft:custom_data` replaced by `data`.
fn with_custom_data(stack: &ItemStack, data: Compound) -> Option<ItemStack> {
    let mut next = stack.clone();
    let mut patch = Compound::new();
    patch.insert("minecraft:custom_data".into(), Tag::Compound(data));
    next.apply_components(&patch).ok()?;
    Some(next)
}

fn slot_of(hand: Hand) -> EquipmentSlot {
    match hand {
        Hand::Main => EquipmentSlot::MainHand,
        _ => EquipmentSlot::OffHand,
    }
}

#[derive(Default)]
struct Mod {
    /// `LastBreakData`: per player, the block last hit and the face it was hit on.
    faces: Mutex<HashMap<u128, FaceHit>>,
    /// The static guards of the Java items (`isHammering`, `isLumbering`): the breaks this mod
    /// makes are heard by `player_destroyed_block` again.
    busy: AtomicBool,
    /// The abacus HUD's tick count.
    hud_ticks: AtomicU32,
}

impl Mod {
    /// `HammerItem`, `ExcavatorItem`, `ScytheItem`, `ClippersItem` `postMine`: the eight blocks
    /// around, each broken as the player breaks a block, when the tool takes it.
    fn area(
        &self,
        host: &mut dyn ServerHost,
        player: PlayerRef,
        level: LevelRef,
        kind: Kind,
        pos: [i32; 3],
    ) {
        let face = {
            let faces = self.faces.lock().unwrap_or_else(|e| e.into_inner());
            faces
                .get(&player.uuid())
                .filter(|(at, _)| *at == pos)
                .map_or(rules::NORTH, |(_, face)| *face)
        };
        // ScytheItem: always the horizontal 3x3.
        let ring = match kind {
            Kind::Scythe => rules::around(pos, [0, 1, 0]),
            _ => rules::around(pos, face),
        };
        for at in ring {
            // The tool in hand, if any is left.
            let Some(tool) = main_hand(&*host, player) else {
                break;
            };
            let Some(key) = level.block(&*host, at) else {
                continue;
            };
            let Some(id) = key.id(&*host).map(str::to_owned) else {
                continue;
            };
            if is_air(&id) || !rules::takes(kind, &id, &|tag| block_in_tag(&*host, key, tag)) {
                continue;
            }
            // HammerItem: only what the hammer in hand can harvest.
            if kind == Kind::Hammer {
                let Some(state) = level.block_state(&*host, at) else {
                    continue;
                };
                if host.block_states().is_correct_tool_for_drops(&tool, state) != Some(true) {
                    continue;
                }
            }
            let _ = player.destroy_block(host, at, false);
        }
    }

    /// `LumberAxeItem.postMine`: the tree's logs go (`Level.removeBlock`), their drops by the axe
    /// fall at the broken log, and the axe takes one use per log.
    fn fell(
        &self,
        host: &mut dyn ServerHost,
        player: PlayerRef,
        level: LevelRef,
        pos: [i32; 3],
        log: &str,
    ) {
        let logs = rules::detect(pos, &|at| probe(&*host, level, at, log));
        if logs.is_empty() {
            return;
        }
        let Some(air) = host.block_state_id("minecraft:air") else {
            return;
        };
        let tool = main_hand(&*host, player);
        let mut drops = Vec::new();
        for at in &logs {
            if let Ok(Some(found)) =
                level.block_drops(host, *at, tool.as_ref(), Some(player.entity()))
            {
                drops.extend(found);
            }
            let _ = level.set_block(
                host,
                *at,
                air,
                BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS,
            );
        }
        for stack in drops {
            let _ = level.pop_resource(host, pos, stack);
        }
        let _ = LivingRef::new(player.entity()).hurt_and_break(
            host,
            EquipmentSlot::MainHand,
            i32::try_from(logs.len()).unwrap_or(i32::MAX),
        );
    }

    /// `ScytheItem.useOnBlock`: a till that works tills the eight around (each from above, with
    /// the same scythe), unless sneaking.
    fn till_around(
        &self,
        host: &mut dyn ServerHost,
        player: PlayerRef,
        level: LevelRef,
        hand: Hand,
        stack: &ItemStack,
        click: &mod_api::BlockClick<'_>,
    ) {
        if player.entity().is_shift_key_down(&*host) == Some(true) {
            return;
        }
        // Only when the click itself tills (the game's own use does that one).
        if level
            .block_transform(&*host, player, hand, stack, click.pos, click.hit)
            .is_none()
        {
            return;
        }
        for at in rules::around(click.pos, [0, 1, 0]) {
            let hit = BlockHit::new(
                [
                    f64::from(at[0]) + 0.5,
                    f64::from(at[1]) + 1.0,
                    f64::from(at[2]) + 0.5,
                ],
                [0, 1, 0],
                false,
            );
            let Some(preview) = level.block_transform(&*host, player, hand, stack, at, hit) else {
                continue;
            };
            let _ = level.set_block(
                host,
                at,
                preview.state,
                BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS,
            );
            let centre = [
                f64::from(at[0]) + 0.5,
                f64::from(at[1]) + 0.5,
                f64::from(at[2]) + 0.5,
            ];
            let _ = level.play_sound(
                host,
                &preview.sound,
                centre,
                SoundSource::Blocks,
                1.0,
                1.0,
                None,
            );
            let wear = i32::try_from(preview.durability.max(1)).unwrap_or(1);
            let _ = LivingRef::new(player.entity()).hurt_and_break(host, slot_of(hand), wear);
        }
    }

    /// `AbacusItem.useOnBlock`: binds to the block, or unbinds when bound.
    fn abacus(
        &self,
        host: &mut dyn ServerHost,
        player: PlayerRef,
        click: &mod_api::BlockClick<'_>,
        stack: &ItemStack,
    ) -> bool {
        let mut data = custom_data(&*host, stack);
        let action = rules::abacus_use(custom_int(&data, "boundPosY"), click.pos);
        match &action {
            AbacusUse::Unbind => {
                data.insert("boundPosY".into(), Tag::Int(rules::UNBOUND));
            }
            AbacusUse::Bind(p) => {
                data.insert("boundPosX".into(), Tag::Int(p[0]));
                data.insert("boundPosY".into(), Tag::Int(p[1]));
                data.insert("boundPosZ".into(), Tag::Int(p[2]));
            }
        }
        let Some(next) = with_custom_data(stack, data) else {
            return false;
        };
        let _ = LivingRef::new(player.entity()).set_hand_item(host, click.hand, Some(next));
        let _ = player.action_bar(host, Text::literal(rules::abacus_message(&action)));
        true
    }
}

/// `CowMilkMixin`: an empty copper bucket milks a cow or a mooshroom.
pub struct MilkPart;

#[mod_api::sunrise_part]
impl ItemPart for MilkPart {
    fn item_interact_mob(
        &self,
        host: HostSide<'_>,
        event: &mut ItemInteractMobEvent<'_>,
    ) -> Option<InteractionResult> {
        let HostSide::Server(host) = host else {
            return None;
        };
        if !rules::milkable(event.kind) {
            return None;
        }
        let cow = host.entity(u32::try_from(event.entity).ok()?)?;
        if LivingRef::new(cow).baby(&*host) == Some(true) {
            return None;
        }
        let player = host.players().acting_player()?;
        let level = player.level(&*host)?;
        let at = cow.position(&*host)?;
        // A creative player's bucket stays as it was; the milk is still taken.
        if player.is_creative(&*host) != Some(true) {
            let key = host.item_key(&Identifier::parse(rules::COPPER_MILK_BUCKET).ok()?)?;
            let mut milk = ItemStack::of(key);
            milk.set_damage(event.stack.damage());
            LivingRef::new(player.entity())
                .set_hand_item(host, event.hand, Some(milk))
                .ok()?;
        }
        let _ = level.play_sound(
            host,
            "minecraft:entity.cow.milk",
            at,
            SoundSource::Neutral,
            1.0,
            1.0,
            None,
        );
        Some(InteractionResult::Success)
    }
}

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        let bucket = r.item_key(rules::COPPER_BUCKET);
        r.item_part(bucket, MilkPart);
    }

    fn left_click_block(&self, host: &mut dyn ServerHost, event: &mut LeftClickBlockEvent) -> Flow {
        if let Some(player) = host.players().acting_player() {
            let mut faces = self.faces.lock().unwrap_or_else(|e| e.into_inner());
            faces.insert(player.uuid(), (event.pos, event.face));
        }
        Flow::Continue
    }

    fn player_destroyed_block(&self, host: HostSide<'_>, event: &mut PlayerDestroyedBlockEvent) {
        let HostSide::Server(host) = host else {
            return;
        };
        if self.busy.load(Ordering::Relaxed) {
            return;
        }
        let Some(player) = host.players().acting_player() else {
            return;
        };
        // ServerPlayerGameMode.destroyBlock returns before postMine in creative.
        if player.game_mode(&*host) != Some(GameMode::Survival)
            || player.entity().is_shift_key_down(&*host) == Some(true)
        {
            return;
        }
        let Some(level) = player.level(&*host) else {
            return;
        };
        let Some(held) = main_hand(&*host, player) else {
            return;
        };
        let Some(kind) = held.item().id(&*host).and_then(rules::kind) else {
            return;
        };
        let broken = event.state.block();
        let Some(id) = broken.id(&*host).map(str::to_owned) else {
            return;
        };
        if !rules::takes(kind, &id, &|tag| block_in_tag(&*host, broken, tag)) {
            return;
        }
        self.busy.store(true, Ordering::Relaxed);
        match kind {
            Kind::LumberAxe => self.fell(host, player, level, event.pos, &id),
            _ => self.area(host, player, level, kind, event.pos),
        }
        self.busy.store(false, Ordering::Relaxed);
    }

    fn right_click_block(&self, host: HostSide<'_>, event: &mut RightClickBlockEvent<'_>) -> Flow {
        let click = event.click;
        let HostSide::Server(host) = host else {
            return Flow::Continue;
        };
        let Some(stack) = click.stack else {
            return Flow::Continue;
        };
        let Some(item) = stack.item().id(&*host).map(str::to_owned) else {
            return Flow::Continue;
        };
        let (Some(player), Some(level)) = (
            host.player(click.player),
            host.level(click.dimension.key().id()),
        ) else {
            return Flow::Continue;
        };
        if item == rules::ABACUS {
            if matches!(click.hand, Hand::Main) && self.abacus(host, player, click, stack) {
                *event.cancellation_result = InteractionResult::Success;
                event.cancelled = true;
                return Flow::Cancel;
            }
        } else if rules::kind(&item) == Some(Kind::Scythe) {
            self.till_around(host, player, level, click.hand, stack, click);
        }
        Flow::Continue
    }

    /// `CopperMilkBucketItem.finishUsing`: the copper bucket comes back with one more use of
    /// wear (the item's `use_remainder` gives a fresh one), or gone when worn out.
    fn living_entity_use_item_finish(
        &self,
        host: HostSide<'_>,
        event: &mut LivingEntityUseItemFinishEvent<'_>,
    ) {
        let HostSide::Server(host) = host else {
            return;
        };
        if event.stack.item().id(&*host) != Some(rules::COPPER_MILK_BUCKET) {
            return;
        }
        let Some(player) = host.players().acting_player() else {
            return;
        };
        if player.is_creative(&*host) == Some(true) {
            return;
        }
        *event.result = rules::worn(event.stack.damage()).and_then(|damage| {
            let key = host.item_key(&Identifier::parse(rules::COPPER_BUCKET).ok()?)?;
            let mut bucket = ItemStack::of(key);
            bucket.set_damage(damage);
            Some(bucket)
        });
    }

    /// `AbacusHudRenderer`: while a bound abacus is in either hand and a block is aimed at, the
    /// blocks counted along each axis that differs.
    fn client_tick_post(&self, host: &mut dyn ClientHost, _: &mut ClientTickPostEvent) {
        if self.hud_ticks.fetch_add(1, Ordering::Relaxed) % 5 != 4 {
            return;
        }
        let Some(player) = host.local_player() else {
            return;
        };
        let bound = [Hand::Main, Hand::Off].into_iter().find_map(|hand| {
            let stack = LivingRef::new(player.entity())
                .with_hand_item(&*host, hand, |held| held.project())
                .flatten()?;
            if stack.item().id(&*host) != Some(rules::ABACUS) {
                return None;
            }
            let data = custom_data(&*host, &stack);
            let at = [
                custom_int(&data, "boundPosX")?,
                custom_int(&data, "boundPosY")?,
                custom_int(&data, "boundPosZ")?,
            ];
            (at[1] != rules::UNBOUND).then_some(at)
        });
        let Some(bound) = bound else {
            return;
        };
        let Some(hit) = host.raycast(5.0) else {
            return;
        };
        let line = rules::abacus_line(bound, [hit.block_x, hit.block_y, hit.block_z]);
        let _ = host.set_overlay_message(Text::literal(line), false);
    }
}
