//! Macaw's Bridges for Sunrise: declarations live in `data/mcwbridges/sunrise/`,
//! original assets and data load from the player's jar via `[[jar]]`, and this
//! crate implements the block behaviours on the typed mod API.
//!
//! | Block family | Part | Behaviour |
//! |---|---|---|
//! | `Bridge_Block`, `Log_Bridge`, `Iron_Bridge` | [`BridgePart`] | neighbour connections, pliers/shears cycle, quick-extend, no stacking |
//! | `Bridge_Block_Rope`, `Bamboo_Bridge` | [`RopeBridgePart`] | 24-state connections + N/E facing, locked toggled ends, quick-extend, rotate |
//! | `Bridge_Stairs` | [`BridgeStairPart`] | perpendicular side connections, quick-extend upward, no stacking |
//! | `Bridge_Support` | [`BridgeSupportPart`] | upward stacking on right-click (waterlogging is declarative) |
//! | `Rail_Bridge` | [`RailBridgePart`] | clockwise placement, redstone torch toggle (signal 10), drop on break/empty hand, rotate |
//! | `Bridge_Torch`, `Bridge_Lantern` | [`BridgeLightPart`] | bridge/stair auto-orientation, lightstate update, torch smoke+flame particles, rotate |

pub mod names;
pub mod rules;

use mod_api::{
    self, AnimateTickEvent, BlockKey, BlockPart, BlockSignalEvent, BlockState, BlockUpdateFlags,
    ClientHost, Direction, Hand, Hooks, Host, HostBlockStates, InteractionResult,
    IsSignalSourceEvent, ItemKey, ItemStack, LevelHost, LevelRef, LevelSide, LevelTicksHost,
    LivingRef, NeighbourUpdate, OnDestroyedByPlayerEvent, ParticleKey, ParticleOptions,
    PlacementSource, PlayerRef, Registrar, RegistrationContext, RotateEvent, ServerHost,
    SoundSource, StateForPlacementEvent, UpdateShapeEvent, UseItemOnEvent, UseWithoutItemEvent,
    WorldgenHost,
};
use rules::{
    Horizontal, bridge_connection, bridge_cycle, property, rope_connection,
    rope_connection_is_locked, rope_cycle, rope_rotate, stair_connection, torch_facing_on_bridge,
    torch_facing_on_rope, torch_facing_on_stair, torch_particle_pos,
};

const STONE_PLACE_SOUND: &str = "minecraft:block.stone.place";
const WOOD_PLACE_SOUND: &str = "minecraft:block.wood.place";
const METAL_PLACE_SOUND: &str = "minecraft:block.metal.place";

/// Whether `text` is one of the vanilla airs (`BlockState.isAir`).
fn is_air(text: &str) -> bool {
    matches!(
        text,
        "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
    )
}

fn is_air_at(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> bool {
    level
        .block_state(host, pos)
        .and_then(|s| host.block_state_string(s))
        .is_some_and(|text| is_air(&text))
}

fn horizontal_neighbors_match(
    level: &dyn LevelHost,
    pos: [i32; 3],
    target_block: BlockKey,
) -> (bool, bool, bool, bool) {
    let matches = |p: [i32; 3]| {
        level
            .block_state(p)
            .is_some_and(|s| s.block() == target_block)
    };
    (
        matches([pos[0], pos[1], pos[2] - 1]),
        matches([pos[0] + 1, pos[1], pos[2]]),
        matches([pos[0], pos[1], pos[2] + 1]),
        matches([pos[0] - 1, pos[1], pos[2]]),
    )
}

fn horizontal_neighbors_match_server(
    host: &dyn Host,
    level: LevelRef,
    pos: [i32; 3],
    target_block: BlockKey,
) -> (bool, bool, bool, bool) {
    let matches = |p: [i32; 3]| {
        level
            .block_state(host, p)
            .is_some_and(|s| s.block() == target_block)
    };
    (
        matches([pos[0], pos[1], pos[2] - 1]),
        matches([pos[0] + 1, pos[1], pos[2]]),
        matches([pos[0], pos[1], pos[2] + 1]),
        matches([pos[0] - 1, pos[1], pos[2]]),
    )
}

fn placement_horizontal(source: PlacementSource) -> Option<Horizontal> {
    match source {
        PlacementSource::Player { yaw, .. } => Some(Horizontal::from_yaw(yaw)),
        PlacementSource::Directional { direction } => Some(Horizontal::from_direction(direction)),
        _ => None,
    }
}

fn player_horizontal(host: &dyn Host, player: PlayerRef) -> Option<Horizontal> {
    let [yaw, _pitch] = player.entity().rotation(host)?;
    Some(Horizontal::from_yaw(yaw))
}

fn shrink_hand_if_survival(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    hand: Hand,
    stack: &ItemStack,
) {
    if !player.is_creative(&*host).unwrap_or(false) {
        let _ = LivingRef::new(player.entity()).set_hand_item(host, hand, stack.clone().shrink(1));
    }
}

fn play_place_sound(host: &mut dyn ServerHost, level: LevelRef, pos: [i32; 3], sound: &str) {
    let _ = level.play_sound(
        host,
        sound,
        [
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]) + 0.5,
            f64::from(pos[2]) + 0.5,
        ],
        SoundSource::Blocks,
        1.0,
        1.0,
        None,
    );
}

/// Detect the current `facing` property of `state` using `WorldgenHost::state_with_property`.
fn worldgen_facing(context: &dyn WorldgenHost, state: BlockState) -> Option<Horizontal> {
    [
        Horizontal::North,
        Horizontal::East,
        Horizontal::South,
        Horizontal::West,
    ]
    .into_iter()
    .find(|&dir| context.state_with_property(state, "facing", dir.name()) == Some(state))
}

/// `Bridge_Block`, `Log_Bridge`, and `Iron_Bridge`.
pub struct BridgePart {
    id: &'static str,
    block: BlockKey,
    item: ItemKey,
    pliers: ItemKey,
    shears: ItemKey,
    place_sound: &'static str,
}

#[mod_api::sunrise_part]
impl BlockPart for BridgePart {
    fn state_for_placement(
        &self,
        level: LevelSide,
        event: &mut StateForPlacementEvent<'_>,
    ) -> Option<Option<BlockState>> {
        let (LevelSide::Server(level) | LevelSide::Client(level)) = level;
        let pos = [
            event.context.pos.x,
            event.context.pos.y,
            event.context.pos.z,
        ];
        if let Some(below) = level.block_state([pos[0], pos[1] - 1, pos[2]])
            && let Some(below_id) = level.state_block(below)
            && names::is_bridge_block(below_id)
        {
            return Some(None);
        }
        let (n, e, s, w) = horizontal_neighbors_match(level, pos, self.block);
        let conn = bridge_connection(n, e, s, w);
        Some(level.state_with_property(event.state, "connection", conn))
    }

    fn update_shape(
        &self,
        level: &mut dyn LevelTicksHost,
        event: &mut UpdateShapeEvent,
    ) -> Option<BlockState> {
        if matches!(event.direction, Direction::Up | Direction::Down) {
            return Some(event.state);
        }
        let Some(neighbour_id) = level.state_block(event.neighbour) else {
            return Some(event.state);
        };
        if !names::is_bridge_block(neighbour_id) {
            return Some(event.state);
        }
        let pos = [event.pos.x, event.pos.y, event.pos.z];
        let (n, e, s, w) = horizontal_neighbors_match(level, pos, self.block);
        let conn = bridge_connection(n, e, s, w);
        level
            .state_with_property(event.state, "connection", conn)
            .or(Some(event.state))
    }

    fn use_item_on(
        &self,
        host: &mut dyn ServerHost,
        event: &mut UseItemOnEvent<'_>,
    ) -> Option<InteractionResult> {
        let click = event.click;
        let stack = click.stack?;
        let held = stack.item();
        let level = host.level(click.dimension.key().id())?;

        if held == self.pliers || held == self.shears {
            let text = host.block_state_string(event.state)?;
            let current = property(&text, "connection")?;
            let next = bridge_cycle(current);
            let new_state = event
                .state
                .with(&host.block_states(), "connection", next)
                .ok()?;
            let flags = BlockUpdateFlags::CLIENTS | BlockUpdateFlags::KNOWN_SHAPE;
            let _ = level.set_block(host, click.pos, new_state, flags);
            return Some(InteractionResult::Success);
        }

        if held == self.item {
            let player = host.player(click.player)?;
            let facing = player_horizontal(&*host, player)?;
            let place_pos = facing.step(click.pos);
            if is_air_at(&*host, level, place_pos) {
                // Emulate Bridge_Block.onPlace immediately computing connection on placement.
                let (n, e, s, w) =
                    horizontal_neighbors_match_server(&*host, level, place_pos, self.block);
                let conn = bridge_connection(n, e, s, w);
                let target_state =
                    host.block_state_id(&format!("{}[connection={conn}]", self.id))?;
                let _ = level.set_block(
                    host,
                    place_pos,
                    target_state,
                    BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS,
                );
                shrink_hand_if_survival(host, player, click.hand, stack);
                play_place_sound(host, level, click.pos, self.place_sound);
                return Some(InteractionResult::Success);
            }
        }

        Some(InteractionResult::Pass)
    }
}

/// `Bridge_Block_Rope` and `Bamboo_Bridge`.
pub struct RopeBridgePart {
    id: &'static str,
    block: BlockKey,
    item: ItemKey,
    pliers: ItemKey,
    shears: ItemKey,
}

#[mod_api::sunrise_part]
impl BlockPart for RopeBridgePart {
    fn state_for_placement(
        &self,
        level: LevelSide,
        event: &mut StateForPlacementEvent<'_>,
    ) -> Option<Option<BlockState>> {
        let (LevelSide::Server(level) | LevelSide::Client(level)) = level;
        let pos = [
            event.context.pos.x,
            event.context.pos.y,
            event.context.pos.z,
        ];
        if let Some(below) = level.block_state([pos[0], pos[1] - 1, pos[2]])
            && let Some(below_id) = level.state_block(below)
            && names::is_rope_bridge(below_id)
        {
            return Some(None);
        }
        let facing = placement_horizontal(event.context.source)
            .unwrap_or(Horizontal::North)
            .rope_axis();
        let (n, e, s, w) = horizontal_neighbors_match(level, pos, self.block);
        let conn = rope_connection(n, e, s, w);
        let s1 = level.state_with_property(event.state, "connection", conn)?;
        Some(level.state_with_property(s1, "facing", facing.name()))
    }

    fn update_shape(
        &self,
        level: &mut dyn LevelTicksHost,
        event: &mut UpdateShapeEvent,
    ) -> Option<BlockState> {
        let text = level.state_string(event.state)?;
        let current = property(&text, "connection")?;
        if rope_connection_is_locked(current) {
            return Some(event.state);
        }
        if matches!(event.direction, Direction::Up | Direction::Down) {
            return Some(event.state);
        }
        let Some(neighbour_id) = level.state_block(event.neighbour) else {
            return Some(event.state);
        };
        if !names::is_rope_bridge(neighbour_id) {
            return Some(event.state);
        }
        let pos = [event.pos.x, event.pos.y, event.pos.z];
        let (n, e, s, w) = horizontal_neighbors_match(level, pos, self.block);
        let conn = rope_connection(n, e, s, w);
        level
            .state_with_property(event.state, "connection", conn)
            .or(Some(event.state))
    }

    fn use_item_on(
        &self,
        host: &mut dyn ServerHost,
        event: &mut UseItemOnEvent<'_>,
    ) -> Option<InteractionResult> {
        let click = event.click;
        let stack = click.stack?;
        let held = stack.item();
        let level = host.level(click.dimension.key().id())?;

        if held == self.pliers || held == self.shears {
            let text = host.block_state_string(event.state)?;
            let current = property(&text, "connection")?;
            let next = rope_cycle(current);
            let new_state = event
                .state
                .with(&host.block_states(), "connection", next)
                .ok()?;
            let flags = BlockUpdateFlags::CLIENTS | BlockUpdateFlags::KNOWN_SHAPE;
            let _ = level.set_block(host, click.pos, new_state, flags);
            return Some(InteractionResult::Success);
        }

        if held == self.item {
            let player = host.player(click.player)?;
            let facing = player_horizontal(&*host, player)?;
            let place_pos = facing.step(click.pos);
            if is_air_at(&*host, level, place_pos) {
                let axis = facing.rope_axis();
                let (n, e, s, w) =
                    horizontal_neighbors_match_server(&*host, level, place_pos, self.block);
                let conn = rope_connection(n, e, s, w);
                let target_state = host.block_state_id(&format!(
                    "{}[connection={conn},facing={}]",
                    self.id,
                    axis.name()
                ))?;
                let _ = level.set_block(
                    host,
                    place_pos,
                    target_state,
                    BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS,
                );
                shrink_hand_if_survival(host, player, click.hand, stack);
                play_place_sound(host, level, click.pos, WOOD_PLACE_SOUND);
                return Some(InteractionResult::Success);
            }
        }

        Some(InteractionResult::Pass)
    }

    fn rotate(&self, context: &dyn WorldgenHost, event: &mut RotateEvent<'_>) {
        let Some(facing) = worldgen_facing(context, event.state) else {
            return;
        };
        let next = rope_rotate(facing, event.rotation);
        if let Some(state) = context.state_with_property(event.state, "facing", next.name()) {
            *event.rotated = state;
        }
    }
}

/// `Bridge_Stairs`.
pub struct BridgeStairPart {
    id: &'static str,
    block: BlockKey,
    item: ItemKey,
}

#[mod_api::sunrise_part]
impl BlockPart for BridgeStairPart {
    fn state_for_placement(
        &self,
        level: LevelSide,
        event: &mut StateForPlacementEvent<'_>,
    ) -> Option<Option<BlockState>> {
        let (LevelSide::Server(level) | LevelSide::Client(level)) = level;
        let pos = [
            event.context.pos.x,
            event.context.pos.y,
            event.context.pos.z,
        ];
        if let Some(below) = level.block_state([pos[0], pos[1] - 1, pos[2]])
            && let Some(below_id) = level.state_block(below)
            && names::is_bridge_stair(below_id)
        {
            return Some(None);
        }
        let placed_facing = placement_horizontal(event.context.source)
            .unwrap_or(Horizontal::North)
            .clockwise();
        let (n, e, s, w) = horizontal_neighbors_match(level, pos, self.block);
        let conn = stair_connection(placed_facing, n, e, s, w);
        let s1 = level.state_with_property(event.state, "facing", placed_facing.name())?;
        Some(level.state_with_property(s1, "connection", conn))
    }

    fn update_shape(
        &self,
        level: &mut dyn LevelTicksHost,
        event: &mut UpdateShapeEvent,
    ) -> Option<BlockState> {
        let text = level.state_string(event.state)?;
        let facing = property(&text, "facing").and_then(Horizontal::from_name)?;
        let pos = [event.pos.x, event.pos.y, event.pos.z];
        let (n, e, s, w) = horizontal_neighbors_match(level, pos, self.block);
        let conn = stair_connection(facing, n, e, s, w);
        level
            .state_with_property(event.state, "connection", conn)
            .or(Some(event.state))
    }

    fn use_item_on(
        &self,
        host: &mut dyn ServerHost,
        event: &mut UseItemOnEvent<'_>,
    ) -> Option<InteractionResult> {
        let click = event.click;
        let stack = click.stack?;
        if stack.item() != self.item {
            return Some(InteractionResult::Pass);
        }
        let level = host.level(click.dimension.key().id())?;
        let player = host.player(click.player)?;
        let facing = player_horizontal(&*host, player)?;
        let step = facing.step(click.pos);
        let place_pos = [step[0], step[1] + 1, step[2]];
        if is_air_at(&*host, level, place_pos) {
            let text = host.block_state_string(event.state)?;
            let block_facing = property(&text, "facing").and_then(Horizontal::from_name)?;
            let (n, e, s, w) =
                horizontal_neighbors_match_server(&*host, level, place_pos, self.block);
            let conn = stair_connection(block_facing, n, e, s, w);
            let target_state = host.block_state_id(&format!(
                "{}[connection={conn},facing={}]",
                self.id,
                block_facing.name()
            ))?;
            let _ = level.set_block(
                host,
                place_pos,
                target_state,
                BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS,
            );
            shrink_hand_if_survival(host, player, click.hand, stack);
            play_place_sound(host, level, click.pos, STONE_PLACE_SOUND);
            return Some(InteractionResult::Success);
        }
        Some(InteractionResult::Pass)
    }

    fn rotate(&self, context: &dyn WorldgenHost, event: &mut RotateEvent<'_>) {
        let Some(facing) = worldgen_facing(context, event.state) else {
            return;
        };
        let next = facing.rotate(event.rotation);
        if let Some(state) = context.state_with_property(event.state, "facing", next.name()) {
            *event.rotated = state;
        }
    }
}

/// `Bridge_Support` (piers): stack upward when right-clicked with the same pier item.
/// Waterlogging is handled declaratively by the boolean `waterlogged` property.
pub struct BridgeSupportPart {
    block: BlockKey,
    item: ItemKey,
}

#[mod_api::sunrise_part]
impl BlockPart for BridgeSupportPart {
    fn use_item_on(
        &self,
        host: &mut dyn ServerHost,
        event: &mut UseItemOnEvent<'_>,
    ) -> Option<InteractionResult> {
        let click = event.click;
        let stack = click.stack?;
        if stack.item() != self.item {
            return Some(InteractionResult::Pass);
        }
        let level = host.level(click.dimension.key().id())?;
        let player = host.player(click.player)?;
        let mut highest = click.pos;
        // Walk upward through matching piers; `block_state` returns None above build height.
        while let Some(above_state) =
            level.block_state(&*host, [highest[0], highest[1] + 1, highest[2]])
        {
            if above_state.block() == self.block {
                highest[1] += 1;
            } else {
                break;
            }
        }
        let place_pos = [highest[0], highest[1] + 1, highest[2]];
        if is_air_at(&*host, level, place_pos) {
            let default_state = host.block_state_default(self.block)?;
            if level
                .set_block(
                    host,
                    place_pos,
                    default_state,
                    BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS,
                )
                .unwrap_or(false)
            {
                shrink_hand_if_survival(host, player, click.hand, stack);
                play_place_sound(host, level, click.pos, STONE_PLACE_SOUND);
                return Some(InteractionResult::Success);
            }
        }
        Some(InteractionResult::Pass)
    }
}

/// `Rail_Bridge`: clockwise placement, redstone torch insertion/removal, signal 10, drop torch on break.
pub struct RailBridgePart {
    redstone_torch: ItemKey,
}

#[mod_api::sunrise_part]
impl BlockPart for RailBridgePart {
    fn state_for_placement(
        &self,
        level: LevelSide,
        event: &mut StateForPlacementEvent<'_>,
    ) -> Option<Option<BlockState>> {
        let (LevelSide::Server(level) | LevelSide::Client(level)) = level;
        let facing = placement_horizontal(event.context.source)
            .unwrap_or(Horizontal::North)
            .clockwise();
        Some(level.state_with_property(event.state, "facing", facing.name()))
    }

    fn is_signal_source(&self, level: &dyn LevelHost, event: &mut IsSignalSourceEvent) {
        if let Some(text) = level.state_string(event.state)
            && property(&text, "powered") == Some("true")
        {
            event.source = true;
        }
    }

    fn block_signal(&self, level: &dyn LevelHost, event: &mut BlockSignalEvent) {
        if !event.direct
            && let Some(text) = level.state_string(event.state)
            && property(&text, "powered") == Some("true")
        {
            event.signal = 10;
        }
    }

    fn use_item_on(
        &self,
        host: &mut dyn ServerHost,
        event: &mut UseItemOnEvent<'_>,
    ) -> Option<InteractionResult> {
        let click = event.click;
        let text = host.block_state_string(event.state)?;
        let powered = property(&text, "powered") == Some("true");
        let level = host.level(click.dimension.key().id())?;

        match click.stack {
            None if powered => Some(InteractionResult::TryWithEmptyHand),
            Some(stack) if stack.item() == self.redstone_torch && !powered => {
                let new_state = event
                    .state
                    .with(&host.block_states(), "powered", "true")
                    .ok()?;
                if level
                    .set_block(host, click.pos, new_state, BlockUpdateFlags::CLIENTS)
                    .unwrap_or(false)
                {
                    let _ = level.update_neighbors(
                        host,
                        click.pos,
                        new_state,
                        NeighbourUpdate::Neighbours,
                    );
                    if let Some(player) = host.player(click.player) {
                        shrink_hand_if_survival(host, player, click.hand, stack);
                    }
                }
                Some(InteractionResult::Consume)
            }
            _ => Some(InteractionResult::Pass),
        }
    }

    fn use_without_item(
        &self,
        host: &mut dyn ServerHost,
        event: &mut UseWithoutItemEvent<'_>,
    ) -> Option<InteractionResult> {
        let click = event.click;
        let text = host.block_state_string(event.state)?;
        if property(&text, "powered") != Some("true") {
            return Some(InteractionResult::Pass);
        }
        let level = host.level(click.dimension.key().id())?;
        let new_state = event
            .state
            .with(&host.block_states(), "powered", "false")
            .ok()?;
        if level
            .set_block(host, click.pos, new_state, BlockUpdateFlags::CLIENTS)
            .unwrap_or(false)
        {
            let _ = level.update_neighbors(host, click.pos, new_state, NeighbourUpdate::Neighbours);
            let _ = level.pop_resource(host, click.pos, ItemStack::of(self.redstone_torch));
        }
        Some(InteractionResult::Consume)
    }

    fn on_destroyed_by_player(
        &self,
        host: &mut dyn ServerHost,
        event: &mut OnDestroyedByPlayerEvent<'_>,
    ) -> Option<bool> {
        let text = host.block_state_string(event.state)?;
        if property(&text, "powered") == Some("true")
            && let Some(player) = host.player(event.player)
            && let Some(level) = player.entity().level(&*host)
        {
            let _ = level.pop_resource(host, event.pos, ItemStack::of(self.redstone_torch));
        }
        None
    }

    fn rotate(&self, context: &dyn WorldgenHost, event: &mut RotateEvent<'_>) {
        let Some(facing) = worldgen_facing(context, event.state) else {
            return;
        };
        let next = facing.rotate(event.rotation);
        if let Some(state) = context.state_with_property(event.state, "facing", next.name()) {
            *event.rotated = state;
        }
    }
}

/// `Bridge_Torch` (`particles = Some(...)`) and `Bridge_Lantern` (`particles = None`).
pub struct BridgeLightPart {
    particles: Option<(ParticleKey, ParticleKey)>,
}

#[mod_api::sunrise_part]
impl BlockPart for BridgeLightPart {
    fn state_for_placement(
        &self,
        level: LevelSide,
        event: &mut StateForPlacementEvent<'_>,
    ) -> Option<Option<BlockState>> {
        let (LevelSide::Server(level) | LevelSide::Client(level)) = level;
        let pos = [
            event.context.pos.x,
            event.context.pos.y,
            event.context.pos.z,
        ];
        let below_pos = [pos[0], pos[1] - 1, pos[2]];
        let below_state = level.block_state(below_pos);
        let below_id = below_state.and_then(|s| level.state_block(s));

        let (facing, lightstate) = match (below_state, below_id) {
            (Some(st), Some(id)) if names::is_bridge_block(id) => {
                let text = level.state_string(st)?;
                let conn = property(&text, "connection")?;
                let Some(f) = torch_facing_on_bridge(conn) else {
                    return Some(None);
                };
                (f, "bridge")
            }
            (Some(st), Some(id)) if names::is_rope_bridge(id) => {
                let text = level.state_string(st)?;
                let conn = property(&text, "connection")?;
                let Some(f) = torch_facing_on_rope(conn) else {
                    return Some(None);
                };
                (f, "bridge")
            }
            (Some(st), Some(id)) if names::is_bridge_stair(id) => {
                let text = level.state_string(st)?;
                let conn = property(&text, "connection")?;
                let stair_facing = property(&text, "facing").and_then(Horizontal::from_name)?;
                let Some(f) = torch_facing_on_stair(conn, stair_facing) else {
                    return Some(None);
                };
                (f, "stair")
            }
            _ => {
                let f = placement_horizontal(event.context.source)
                    .unwrap_or(Horizontal::North)
                    .clockwise();
                (f, "bridge")
            }
        };

        let s1 = level.state_with_property(event.state, "facing", facing.name())?;
        Some(level.state_with_property(s1, "lightstate", lightstate))
    }

    fn update_shape(
        &self,
        level: &mut dyn LevelTicksHost,
        event: &mut UpdateShapeEvent,
    ) -> Option<BlockState> {
        if event.direction != Direction::Down {
            return None;
        }
        let lightstate = level
            .state_block(event.neighbour)
            .filter(|id| names::is_bridge_stair(id))
            .map_or("bridge", |_| "stair");
        level
            .state_with_property(event.state, "lightstate", lightstate)
            .or(Some(event.state))
    }

    fn animate_tick(&self, host: &mut dyn ClientHost, event: &mut AnimateTickEvent<'_>) {
        let Some((smoke, flame)) = self.particles else {
            return;
        };
        let Some(text) = host.block_state_string(event.state) else {
            return;
        };
        let Some(facing) = property(&text, "facing").and_then(Horizontal::from_name) else {
            return;
        };
        let lightstate = property(&text, "lightstate").unwrap_or("bridge");
        let at = torch_particle_pos([event.pos.x, event.pos.y, event.pos.z], facing, lightstate);
        host.add_particle(&ParticleOptions::simple(smoke), at, [0.0, 0.0, 0.0]);
        host.add_particle(&ParticleOptions::simple(flame), at, [0.0, 0.0, 0.0]);
    }

    fn rotate(&self, context: &dyn WorldgenHost, event: &mut RotateEvent<'_>) {
        let Some(facing) = worldgen_facing(context, event.state) else {
            return;
        };
        let next = facing.rotate(event.rotation);
        if let Some(state) = context.state_with_property(event.state, "facing", next.name()) {
            *event.rotated = state;
        }
    }
}

#[derive(Default)]
pub struct McwBridges;

#[mod_api::sunrise_mod]
impl Hooks for McwBridges {
    fn register(&self, r: &mut dyn Registrar, _ctx: &RegistrationContext) {
        let pliers = r.item_key(names::PLIERS);
        let shears = r.item_key("minecraft:shears");
        let redstone_torch = r.item_key("minecraft:redstone_torch");
        let smoke = r.particle_key("minecraft:smoke");
        let flame = r.particle_key("minecraft:flame");

        for (ids, sound) in [
            (names::STONE_BRIDGES, STONE_PLACE_SOUND),
            (names::LOG_BRIDGES, WOOD_PLACE_SOUND),
            (names::IRON_BRIDGES, METAL_PLACE_SOUND),
        ] {
            for &id in ids {
                let block = r.block_key(id);
                let item = r.item_key(id);
                r.block_part(
                    block,
                    BridgePart {
                        id,
                        block,
                        item,
                        pliers,
                        shears,
                        place_sound: sound,
                    },
                );
            }
        }

        for &id in names::ROPE_BRIDGES {
            let block = r.block_key(id);
            let item = r.item_key(id);
            r.block_part(
                block,
                RopeBridgePart {
                    id,
                    block,
                    item,
                    pliers,
                    shears,
                },
            );
        }

        for &id in names::BRIDGE_STAIRS {
            let block = r.block_key(id);
            let item = r.item_key(id);
            r.block_part(block, BridgeStairPart { id, block, item });
        }

        for &id in names::BRIDGE_SUPPORTS {
            let block = r.block_key(id);
            let item = r.item_key(id);
            r.block_part(block, BridgeSupportPart { block, item });
        }

        for &id in names::RAIL_BRIDGES {
            let block = r.block_key(id);
            r.block_part(block, RailBridgePart { redstone_torch });
        }

        let torch = r.block_key(names::BRIDGE_TORCH);
        r.block_part(
            torch,
            BridgeLightPart {
                particles: Some((smoke, flame)),
            },
        );
        let lantern = r.block_key(names::BRIDGE_LANTERN);
        r.block_part(lantern, BridgeLightPart { particles: None });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn every_registered_block_has_a_declaration_file() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/mcwbridges/sunrise/block");
        for id in names::STONE_BRIDGES
            .iter()
            .chain(names::LOG_BRIDGES)
            .chain(names::IRON_BRIDGES)
            .chain(names::ROPE_BRIDGES)
            .chain(names::BRIDGE_STAIRS)
            .chain(names::BRIDGE_SUPPORTS)
            .chain(names::RAIL_BRIDGES)
            .chain(&[names::BRIDGE_TORCH, names::BRIDGE_LANTERN])
        {
            let stem = id.strip_prefix("mcwbridges:").unwrap();
            assert!(
                dir.join(format!("{stem}.json")).exists(),
                "missing declaration for {id}"
            );
        }
    }
}
