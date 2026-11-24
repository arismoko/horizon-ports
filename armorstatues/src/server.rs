//! Server-side armor stand interaction, menu slot rules, shift-click routing, reach checks and
//! edit payload handling.
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, MutexGuard, OnceLock, PoisonError};

use mod_api::client::widgets::{filter_text, utf16_len};
use mod_api::{
    AttributeModifier, AttributeOperation, ComponentId, EntityInteractEvent, EntityReach,
    EquipmentSlot, Flow, Host, HostSide, HostTransaction, IPayloadContext, Identifier,
    InteractionResult, ItemPlace, ItemResource, ItemStack, MenuClickedEvent, MenuClosedEvent,
    MenuContainer, MenuKey, MenuPart, MenuReplace, MenuSlotChangedEvent, MenuSlotPlace,
    MenuSlotRulesEvent, ModMenuOpen, ModifierLifetime, PAYLOAD_DECODE_BUDGET, PREVENT_ARMOR_CHANGE,
    PayloadId, PlayerRef, ServerHost, ServerTickEvent, ServerTickPhase, Tag, Text, ToClient,
    encode_payload,
};

use crate::client;
use crate::model::{
    ALL_SLOTS_DISABLED, DEFAULT_SCALE, DISABLE_ALL_OFFSET, DISABLE_PUTTING_OFFSET,
    DISABLE_TAKING_OFFSET, EDIT_CHANNEL, EditMessage, MAX_NAME_LEN, MAX_SCALE,
    MENU_EQUIPMENT_ENTITY_INDICES, MENU_EQUIPMENT_SLOTS, MENU_ID, MIN_SCALE, SCALE_BONUS_ID,
    StandSnapshot, StyleOption, SyncMessage, is_slot_disabled, pose_nbt,
};

#[derive(Clone, Debug)]
struct ServerSession {
    window: i32,
    entity_id: u32,
    disabled_slots: i32,
    show_arms: bool,
    last_hands: [bool; 2],
    last_equipment: Option<Tag>,
}

static SESSIONS: LazyLock<Mutex<HashMap<u128, ServerSession>>> = LazyLock::new(Mutex::default);
static SYNC_TOKEN: OnceLock<ToClient<SyncMessage>> = OnceLock::new();

fn sessions() -> MutexGuard<'static, HashMap<u128, ServerSession>> {
    SESSIONS.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn set_sync_token(token: ToClient<SyncMessage>) {
    let _ = SYNC_TOKEN.set(token);
}

fn send_sync(host: &mut dyn ServerHost, player: PlayerRef, msg: &SyncMessage) {
    if let Some(token) = SYNC_TOKEN.get() {
        let _ = token.send(host, player, msg);
    }
}

fn resolve_menu_key(host: &dyn Host) -> Option<MenuKey> {
    host.menu_keys()
        .into_iter()
        .find(|&key| key.id(host).is_some_and(|name| name.as_str() == MENU_ID))
}

/// Vanilla `LivingEntity.isEquippableInSlot` / `Equippable.canBeEquippedBy` for an armor stand.
pub fn equips_armor_stand(host: &dyn Host, stack: &ItemStack, slot: EquipmentSlot) -> bool {
    let slot_name = match slot {
        EquipmentSlot::Head => "head",
        EquipmentSlot::Chest => "chest",
        EquipmentSlot::Legs => "legs",
        EquipmentSlot::Feet => "feet",
        EquipmentSlot::MainHand => "mainhand",
        EquipmentSlot::OffHand => "offhand",
        EquipmentSlot::Body => "body",
        EquipmentSlot::Saddle => "saddle",
    };
    let resource = ItemResource::of(stack);
    let id = ComponentId::new("minecraft:equippable").expect("valid component id");
    let Some(equippable) = resource.value(host, &id) else {
        return false;
    };
    let matches_name = |tag: Option<&Tag>, expected: &str| matches!(tag, Some(Tag::String(s)) if s.to_string_lossy() == expected);
    if !matches_name(equippable.get("slot"), slot_name) {
        return false;
    }
    match equippable.get("allowed_entities") {
        None => true,
        Some(Tag::List(ids)) => ids
            .iter()
            .any(|entry| matches_name(Some(entry), "minecraft:armor_stand")),
        Some(Tag::String(entities)) => {
            let s = entities.to_string_lossy();
            match s.strip_prefix('#') {
                Some(tag) => host.entity_type_in_tag("minecraft:armor_stand", tag),
                None => s == "minecraft:armor_stand",
            }
        }
        _ => false,
    }
}

/// Whether `stack` has no `minecraft:equippable` component (`StatueSlot.mayPlace` head rule).
fn is_non_equippable(host: &dyn Host, stack: &ItemStack) -> bool {
    ItemResource::of(stack).equippable_slot(host).is_none()
}

/// `ArmorStandSlot` + `StatueSlot` placement check.
pub fn slot_may_place(
    host: &dyn Host,
    slot_idx: usize,
    stack: &ItemStack,
    slot_has_item: bool,
    disabled_slots: i32,
    creative: bool,
) -> bool {
    let Some(&eq_slot) = MENU_EQUIPMENT_SLOTS.get(slot_idx) else {
        return true;
    };
    if !creative {
        if is_slot_disabled(disabled_slots, eq_slot, DISABLE_ALL_OFFSET) {
            return false;
        }
        if is_slot_disabled(disabled_slots, eq_slot, DISABLE_PUTTING_OFFSET) && !slot_has_item {
            return false;
        }
    }
    match eq_slot {
        EquipmentSlot::MainHand | EquipmentSlot::OffHand => true,
        EquipmentSlot::Head => {
            is_non_equippable(host, stack) || equips_armor_stand(host, stack, EquipmentSlot::Head)
        }
        _ => equips_armor_stand(host, stack, eq_slot),
    }
}

/// `ArmorStandSlot` + `ArmorSlot` pickup check.
pub fn slot_may_pickup(
    host: &dyn Host,
    slot_idx: usize,
    stack: &ItemStack,
    disabled_slots: i32,
    creative: bool,
) -> bool {
    let Some(&eq_slot) = MENU_EQUIPMENT_SLOTS.get(slot_idx) else {
        return true;
    };
    if !creative {
        if is_slot_disabled(disabled_slots, eq_slot, DISABLE_TAKING_OFFSET) {
            return false;
        }
        if slot_idx < 4
            && ItemResource::of(stack).has_enchantment_effect(host, PREVENT_ARMOR_CHANGE)
        {
            return false;
        }
    }
    true
}

/// `ArmorStandInteractHelper.openStatueMenu`.
pub fn on_entity_interact(
    host: HostSide<'_>,
    event: &mut EntityInteractEvent<'_>,
) -> Option<InteractionResult> {
    if event.kind != "minecraft:armor_stand" || event.stack.is_some() {
        return None;
    }
    match host {
        HostSide::Client(client_host) => {
            let player = client_host.local_player()?;
            if player.is_secondary_use_active(client_host) == Some(true)
                && player.is_spectator(client_host) == Some(false)
                && player.may_build(client_host) == Some(true)
            {
                if !client_host.has_channel(&PayloadId(EDIT_CHANNEL.into())) {
                    let entity_id = u32::try_from(event.entity).ok()?;
                    if client::open_client_only_screen(client_host, entity_id) {
                        return Some(InteractionResult::Consume);
                    }
                    return None;
                }
                return Some(InteractionResult::Success);
            }
            None
        }
        HostSide::Server(server_host) => {
            let player = server_host.players().acting_player()?;
            if player.is_secondary_use_active(server_host) != Some(true)
                || player.is_spectator(server_host) != Some(false)
                || player.may_build(server_host) != Some(true)
            {
                return None;
            }
            let entity_id = u32::try_from(event.entity).ok()?;
            let stand = server_host.entity(entity_id)?;
            let living = server_host.living(stand)?;
            let nbt = stand.nbt(server_host)?;
            let pos = stand.position(server_host).unwrap_or([0.0; 3]);
            let rot = stand.rotation(server_host).unwrap_or([0.0; 2]);
            let scale = living
                .attribute_value(server_host, "minecraft:scale")
                .unwrap_or(f64::from(DEFAULT_SCALE)) as f32;
            let creative = player.has_infinite_materials(server_host) == Some(true);
            let snapshot = StandSnapshot::from_nbt(entity_id, &nbt, pos, rot, scale, creative);

            if snapshot.disabled_slots == ALL_SLOTS_DISABLED && !creative {
                return None;
            }
            let menu_key = resolve_menu_key(server_host)?;
            let extra = encode_payload(&snapshot, PAYLOAD_DECODE_BUDGET).ok()?;
            let title = stand
                .name(server_host)
                .filter(|name| !name.is_empty())
                .map_or_else(
                    || Text::translate("entity.minecraft.armor_stand", vec![]),
                    Text::literal,
                );
            let mut open = ModMenuOpen::new(menu_key, title);
            open.extra = extra;
            open.replace = MenuReplace::CloseFirst;
            let window = player.open_mod_menu(server_host, open).ok()??;

            let mut places = Vec::with_capacity(42);
            for &entity_slot in &MENU_EQUIPMENT_ENTITY_INDICES {
                places.push(MenuSlotPlace::EntityItems {
                    entity: stand,
                    slot: entity_slot,
                });
            }
            let player_entity = player.entity();
            for inv_slot in 9..36 {
                places.push(MenuSlotPlace::EntityItems {
                    entity: player_entity,
                    slot: inv_slot,
                });
            }
            for hotbar_slot in 0..9 {
                places.push(MenuSlotPlace::EntityItems {
                    entity: player_entity,
                    slot: hotbar_slot,
                });
            }
            if player
                .back_mod_menu_slots(server_host, window, places)
                .is_err()
            {
                server_host.close_menu(player.uuid());
                return None;
            }

            let last_hands = [
                hand_occupied(server_host, entity_id, EquipmentSlot::MainHand),
                hand_occupied(server_host, entity_id, EquipmentSlot::OffHand),
            ];
            sessions().insert(
                player.uuid(),
                ServerSession {
                    window,
                    entity_id,
                    disabled_slots: snapshot.disabled_slots,
                    show_arms: snapshot.show_arms,
                    last_hands,
                    last_equipment: snapshot.equipment,
                },
            );
            Some(InteractionResult::Success)
        }
    }
}

fn hand_occupied(host: &mut dyn ServerHost, entity_id: u32, slot: EquipmentSlot) -> bool {
    ItemPlace::Equipment {
        entity: entity_id,
        slot,
    }
    .amount(host)
        > 0
}

fn ensure_show_arms(host: &mut dyn ServerHost, player: PlayerRef, session: &mut ServerSession) {
    if session.show_arms {
        return;
    }
    if let Some(stand) = host.entity(session.entity_id) {
        let _ = stand.merge_nbt(host, StyleOption::ShowArms.to_nbt(true));
        session.show_arms = true;
        send_sync(host, player, &SyncMessage::ShowArms(true));
    }
}

fn sync_stand_equipment(host: &mut dyn ServerHost, player: PlayerRef, session: &mut ServerSession) {
    let Some(stand) = host.entity(session.entity_id) else {
        return;
    };
    let Some(nbt) = stand.nbt(host) else {
        return;
    };
    let eq = nbt.get(&"equipment".into()).cloned();
    if eq != session.last_equipment {
        session.last_equipment = eq.clone();
        send_sync(host, player, &SyncMessage::Equipment(eq));
    }
}

pub fn on_menu_closed(event: &MenuClosedEvent<'_>) {
    let mut map = sessions();
    if map
        .get(&event.uuid)
        .is_some_and(|s| s.window == event.window)
    {
        map.remove(&event.uuid);
    }
}

/// Checks `stillValid` (`entity.isAlive() && player.isWithinEntityInteractionRange(entity, 4.0)`)
/// and mirrors hand-item `ShowArms` / equipment changes to open clients.
pub fn on_server_tick(host: &mut dyn ServerHost, event: &ServerTickEvent) {
    if !matches!(event.phase, ServerTickPhase::ServerTickEnd { .. }) {
        return;
    }
    let snapshot: Vec<(u128, ServerSession)> = sessions()
        .iter()
        .map(|(&uuid, s)| (uuid, s.clone()))
        .collect();
    for (uuid, mut session) in snapshot {
        let Some(player) = host.player(uuid) else {
            sessions().remove(&uuid);
            continue;
        };
        let window_matches = player
            .container_menu(host)
            .and_then(|m| m.container_id(host))
            == Some(session.window);
        if !window_matches {
            sessions().remove(&uuid);
            continue;
        }
        let valid = host.entity(session.entity_id).is_some_and(|stand| {
            host.living(stand)
                .and_then(|l| l.alive(host))
                .unwrap_or(false)
                && player
                    .within_entity_reach(host, stand, EntityReach::Container)
                    .unwrap_or(false)
        });
        if !valid {
            sessions().remove(&uuid);
            host.close_menu(uuid);
            continue;
        }

        let hands = [
            hand_occupied(host, session.entity_id, EquipmentSlot::MainHand),
            hand_occupied(host, session.entity_id, EquipmentSlot::OffHand),
        ];
        let newly_held =
            (hands[0] && !session.last_hands[0]) || (hands[1] && !session.last_hands[1]);
        session.last_hands = hands;
        if newly_held {
            ensure_show_arms(host, player, &mut session);
        }
        sync_stand_equipment(host, player, &mut session);
        if let Some(live) = sessions().get_mut(&uuid)
            && live.window == session.window
        {
            live.last_hands = session.last_hands;
            live.show_arms = session.show_arms;
            live.last_equipment = session.last_equipment;
        }
    }
}

/// Converts a 42-slot menu index into its backing `ItemPlace`.
fn menu_slot_place(uuid: u128, entity_id: u32, slot: usize) -> Option<ItemPlace> {
    if slot < 6 {
        Some(ItemPlace::Equipment {
            entity: entity_id,
            slot: MENU_EQUIPMENT_SLOTS[slot],
        })
    } else if slot < 33 {
        Some(ItemPlace::PlayerSlot {
            player: uuid,
            slot: (slot - 6 + 9) as i32,
        })
    } else if slot < 42 {
        Some(ItemPlace::PlayerSlot {
            player: uuid,
            slot: (slot - 33) as i32,
        })
    } else {
        None
    }
}

/// Vanilla `AbstractContainerMenu.moveItemStackTo` over a slice of destination menu slot indices.
/// Runs merge pass first, then empty pass. Returns `true` if any item moved.
fn move_to_slots(
    host: &mut dyn ServerHost,
    uuid: u128,
    entity_id: u32,
    src_slot: usize,
    destinations: &[usize],
    max_per_dest: Option<u32>,
) -> bool {
    let Some(src_place) = menu_slot_place(uuid, entity_id, src_slot) else {
        return false;
    };
    let Some(src_stack) = src_place.with_stack(host, |v| v.project()).flatten() else {
        return false;
    };
    let resource = ItemResource::of(&src_stack);
    let max_stack = {
        let id = ComponentId::new("minecraft:max_stack_size").expect("valid id");
        match resource.value(host, &id) {
            Some(Tag::Int(n)) => u32::try_from(n).unwrap_or(1).max(1),
            _ => 1,
        }
    };
    let mut moved_any = false;
    let mut remaining = src_stack.count();

    // Pass 1: merge into existing matching stacks (only when item stacks above 1).
    if max_stack > 1 && max_per_dest.unwrap_or(max_stack) > 1 {
        for &dst in destinations {
            if remaining == 0 {
                break;
            }
            let Some(dst_place) = menu_slot_place(uuid, entity_id, dst) else {
                continue;
            };
            let existing = dst_place
                .with_stack(host, |v| {
                    v.project()
                        .filter(|s| resource.matches(s))
                        .map(|s| s.count())
                })
                .flatten();
            let Some(count) = existing else {
                continue;
            };
            let cap = max_stack.min(max_per_dest.unwrap_or(max_stack));
            let room = cap.saturating_sub(count);
            if room == 0 {
                continue;
            }
            let take = remaining.min(room);
            let moved = host
                .transaction(|host, tx| {
                    let extracted = src_place
                        .extract(host, tx, &resource, take)
                        .map_err(|_| ())?;
                    if extracted == 0 {
                        return Err(());
                    }
                    let inserted = ItemPlace::Strict(Box::new(dst_place))
                        .insert(host, tx, &resource, extracted)
                        .map_err(|_| ())?;
                    if inserted != extracted {
                        return Err(());
                    }
                    Ok(inserted)
                })
                .unwrap_or(0);
            if moved > 0 {
                remaining -= moved;
                moved_any = true;
            }
        }
    }

    // Pass 2: place into empty slots.
    for &dst in destinations {
        if remaining == 0 {
            break;
        }
        let Some(dst_place) = menu_slot_place(uuid, entity_id, dst) else {
            continue;
        };
        if dst_place.amount(host) > 0 {
            continue;
        }
        let cap = max_stack.min(max_per_dest.unwrap_or(max_stack));
        let take = remaining.min(cap);
        if take == 0 {
            continue;
        }
        let moved = host
            .transaction(|host, tx| {
                let extracted = src_place
                    .extract(host, tx, &resource, take)
                    .map_err(|_| ())?;
                if extracted == 0 {
                    return Err(());
                }
                let inserted = ItemPlace::Strict(Box::new(dst_place))
                    .insert(host, tx, &resource, extracted)
                    .map_err(|_| ())?;
                if inserted != extracted {
                    return Err(());
                }
                Ok(inserted)
            })
            .unwrap_or(0);
        if moved > 0 {
            remaining -= moved;
            moved_any = true;
        }
    }

    moved_any
}

/// Shift-click (`StatueMenu.quickMoveStack` via `QuickMoveRuleSet` RELAXED).
fn handle_quick_move(
    host: &mut dyn ServerHost,
    uuid: u128,
    slot: usize,
    session: &mut ServerSession,
    creative: bool,
) {
    let Some(src_place) = menu_slot_place(uuid, session.entity_id, slot) else {
        return;
    };
    let Some(src_stack) = src_place.with_stack(host, |v| v.project()).flatten() else {
        return;
    };
    if slot < 6 && !slot_may_pickup(host, slot, &src_stack, session.disabled_slots, creative) {
        return;
    }

    if slot >= 6 {
        // Container slot rules in StatueMenu order: Head(0), Chest(1), Legs(2), Feet(3), OffHand(5).
        let mut equipped = false;
        for &target in &[0usize, 1, 2, 3, 5] {
            let eq_slot = MENU_EQUIPMENT_SLOTS[target];
            if !equips_armor_stand(host, &src_stack, eq_slot) {
                continue;
            }
            let target_place = ItemPlace::Equipment {
                entity: session.entity_id,
                slot: eq_slot,
            };
            if target_place.amount(host) > 0 {
                continue;
            }
            if !slot_may_place(
                host,
                target,
                &src_stack,
                false,
                session.disabled_slots,
                creative,
            ) {
                continue;
            }
            if move_to_slots(host, uuid, session.entity_id, slot, &[target], Some(1)) {
                equipped = true;
                if target == 5
                    && let Some(player) = host.player(uuid)
                {
                    ensure_show_arms(host, player, session);
                }
                break;
            }
        }
        if !equipped {
            // Inventory compartment rules: main (6..33) <-> hotbar (33..42).
            let targets: Vec<usize> = if slot < 33 {
                (33..42).collect()
            } else {
                (6..33).collect()
            };
            let _ = move_to_slots(host, uuid, session.entity_id, slot, &targets, None);
        }
    } else {
        // From equipment -> player inventory in reverse order (41 down to 6).
        let targets: Vec<usize> = (6..42).rev().collect();
        let _ = move_to_slots(host, uuid, session.entity_id, slot, &targets, None);
    }
}

/// Handles putting/swapping a non-equippable item into slot 0 (Head), because the default
/// `LivingEquipment` capability rejects non-equippable items in armor slots whereas
/// `ItemPlace::Equipment` uses the permissive `set_equipment`.
fn handle_head_non_equippable_click(
    host: &mut dyn ServerHost,
    uuid: u128,
    button: i32,
    mode: i32,
    session: &ServerSession,
    creative: bool,
) -> bool {
    let Some(player) = host.player(uuid) else {
        return false;
    };
    let Some(menu) = player.container_menu(host) else {
        return false;
    };
    let head_place = ItemPlace::Equipment {
        entity: session.entity_id,
        slot: EquipmentSlot::Head,
    };
    let head_stack = head_place.with_stack(host, |v| v.project()).flatten();

    match mode {
        0 if button == 0 || button == 1 => {
            let cursor_place = menu.cursor();
            let Some(cursor_stack) = cursor_place.with_stack(host, |v| v.project()).flatten()
            else {
                return false;
            };
            if !is_non_equippable(host, &cursor_stack) {
                return false;
            }
            if !slot_may_place(
                host,
                0,
                &cursor_stack,
                head_stack.is_some(),
                session.disabled_slots,
                creative,
            ) {
                return true;
            }
            let cursor_res = ItemResource::of(&cursor_stack);
            match head_stack {
                None => {
                    let _ = host.transaction(|host, tx| {
                        let taken = cursor_place
                            .extract(host, tx, &cursor_res, 1)
                            .map_err(|_| ())?;
                        if taken != 1 {
                            return Err(());
                        }
                        let put = head_place
                            .insert(host, tx, &cursor_res, 1)
                            .map_err(|_| ())?;
                        if put != 1 {
                            return Err(());
                        }
                        Ok(())
                    });
                    true
                }
                Some(existing) => {
                    if !slot_may_pickup(host, 0, &existing, session.disabled_slots, creative) {
                        return true;
                    }
                    if cursor_stack.count() != 1 {
                        return true;
                    }
                    let existing_res = ItemResource::of(&existing);
                    let existing_count = existing.count();
                    let _ = host.transaction(|host, tx| {
                        let taken_c = cursor_place
                            .extract(host, tx, &cursor_res, 1)
                            .map_err(|_| ())?;
                        let taken_h = head_place
                            .extract(host, tx, &existing_res, existing_count)
                            .map_err(|_| ())?;
                        if taken_c != 1 || taken_h != existing_count {
                            return Err(());
                        }
                        let put_h = head_place
                            .insert(host, tx, &cursor_res, 1)
                            .map_err(|_| ())?;
                        let put_c = cursor_place
                            .insert(host, tx, &existing_res, existing_count)
                            .map_err(|_| ())?;
                        if put_h != 1 || put_c != existing_count {
                            return Err(());
                        }
                        Ok(())
                    });
                    true
                }
            }
        }
        2 => {
            let inv_slot = match button {
                0..=8 => button,
                40 => -106,
                _ => return false,
            };
            let inv_place = ItemPlace::PlayerSlot {
                player: uuid,
                slot: inv_slot,
            };
            let Some(inv_stack) = inv_place.with_stack(host, |v| v.project()).flatten() else {
                return false;
            };
            if !is_non_equippable(host, &inv_stack) {
                return false;
            }
            if !slot_may_place(
                host,
                0,
                &inv_stack,
                head_stack.is_some(),
                session.disabled_slots,
                creative,
            ) {
                return true;
            }
            let inv_res = ItemResource::of(&inv_stack);
            match head_stack {
                None => {
                    let _ = host.transaction(|host, tx| {
                        let taken = inv_place.extract(host, tx, &inv_res, 1).map_err(|_| ())?;
                        if taken != 1 {
                            return Err(());
                        }
                        let put = head_place.insert(host, tx, &inv_res, 1).map_err(|_| ())?;
                        if put != 1 {
                            return Err(());
                        }
                        Ok(())
                    });
                    true
                }
                Some(existing) => {
                    if !slot_may_pickup(host, 0, &existing, session.disabled_slots, creative) {
                        return true;
                    }
                    if inv_stack.count() != 1 {
                        return true;
                    }
                    let existing_res = ItemResource::of(&existing);
                    let existing_count = existing.count();
                    let _ = host.transaction(|host, tx| {
                        let taken_i = inv_place.extract(host, tx, &inv_res, 1).map_err(|_| ())?;
                        let taken_h = head_place
                            .extract(host, tx, &existing_res, existing_count)
                            .map_err(|_| ())?;
                        if taken_i != 1 || taken_h != existing_count {
                            return Err(());
                        }
                        let put_h = head_place.insert(host, tx, &inv_res, 1).map_err(|_| ())?;
                        let put_i = inv_place
                            .insert(host, tx, &existing_res, existing_count)
                            .map_err(|_| ())?;
                        if put_h != 1 || put_i != existing_count {
                            return Err(());
                        }
                        Ok(())
                    });
                    true
                }
            }
        }
        _ => false,
    }
}

pub struct ArmorStandMenuPart;

#[mod_api::sunrise_part]
impl MenuPart for ArmorStandMenuPart {
    fn menu_slot_rules(&self, host: &dyn Host, event: &mut MenuSlotRulesEvent<'_>) {
        if event.container != MenuContainer::Main || event.slot >= 6 {
            return;
        }
        let player = event.context.player.and_then(|uuid| host.player(uuid));
        let creative = player.is_some_and(|p| {
            p.has_infinite_materials(host)
                .or_else(|| p.is_creative(host))
                .unwrap_or(false)
        });
        let disabled_slots = event
            .context
            .player
            .and_then(|uuid| sessions().get(&uuid).map(|s| s.disabled_slots))
            .unwrap_or_else(client::current_disabled_slots);
        let slot_has_item = event
            .context
            .slot(MenuContainer::Main, event.slot)
            .is_some();

        if !slot_may_place(
            host,
            event.slot,
            event.stack,
            slot_has_item,
            disabled_slots,
            creative,
        ) {
            event.may_place = false;
        }
        if !slot_may_pickup(host, event.slot, event.stack, disabled_slots, creative) {
            event.may_pickup = false;
        }
        event.max_stack = event.max_stack.min(1);
    }

    fn menu_slot_changed(&self, host: &mut dyn ServerHost, event: &mut MenuSlotChangedEvent<'_>) {
        let Some(uuid) = event.context.player else {
            return;
        };
        let Some(player) = host.player(uuid) else {
            return;
        };
        let mut session = {
            let map = sessions();
            let Some(s) = map.get(&uuid) else {
                return;
            };
            s.clone()
        };
        let hand_placed = event.changed.iter().any(|change| {
            change.container == MenuContainer::Main
                && (change.slot == 4 || change.slot == 5)
                && change.after.is_some()
        });
        if hand_placed {
            ensure_show_arms(host, player, &mut session);
        }
        sync_stand_equipment(host, player, &mut session);
        if let Some(live) = sessions().get_mut(&uuid)
            && live.window == session.window
        {
            live.show_arms = session.show_arms;
            live.last_equipment = session.last_equipment;
        }
    }

    fn menu_clicked(&self, host: &mut dyn ServerHost, event: &mut MenuClickedEvent<'_>) -> Flow {
        let mut session = {
            let map = sessions();
            let Some(s) = map.get(&event.uuid) else {
                return Flow::Continue;
            };
            if s.window != event.window {
                return Flow::Continue;
            }
            s.clone()
        };
        let creative = host
            .player(event.uuid)
            .and_then(|p| p.has_infinite_materials(host))
            .unwrap_or(false);

        if event.mode == 1 && (0..42).contains(&event.slot) {
            handle_quick_move(
                host,
                event.uuid,
                event.slot as usize,
                &mut session,
                creative,
            );
            if let Some(player) = host.player(event.uuid) {
                sync_stand_equipment(host, player, &mut session);
            }
            if let Some(live) = sessions().get_mut(&event.uuid)
                && live.window == session.window
            {
                live.show_arms = session.show_arms;
                live.last_equipment = session.last_equipment;
            }
            return Flow::Cancel;
        }

        if event.slot == 0
            && handle_head_non_equippable_click(
                host,
                event.uuid,
                event.button,
                event.mode,
                &session,
                creative,
            )
        {
            if let Some(player) = host.player(event.uuid) {
                sync_stand_equipment(host, player, &mut session);
            }
            if let Some(live) = sessions().get_mut(&event.uuid)
                && live.window == session.window
            {
                live.last_equipment = session.last_equipment;
            }
            return Flow::Cancel;
        }

        Flow::Continue
    }
}

/// Validates and applies a client `EditMessage` on the server main thread.
pub fn handle_edit(msg: EditMessage, ctx: &mut dyn IPayloadContext) -> Result<(), String> {
    let Ok(player) = ctx.player() else {
        return Ok(());
    };
    let Ok(host) = ctx.server_host() else {
        return Ok(());
    };
    let uuid = player.uuid();
    let Some(session) = sessions().get(&uuid).cloned() else {
        return Ok(());
    };
    if player
        .container_menu(host)
        .and_then(|m| m.container_id(host))
        != Some(session.window)
    {
        return Ok(());
    }
    let Some(stand) = host.entity(session.entity_id) else {
        return Ok(());
    };
    let Some(living) = host.living(stand) else {
        return Ok(());
    };
    if !living.alive(host).unwrap_or(false)
        || !player
            .within_entity_reach(host, stand, EntityReach::Container)
            .unwrap_or(false)
    {
        return Ok(());
    }

    match msg {
        EditMessage::Name(raw) => {
            let filtered = filter_text(&raw, false);
            if utf16_len(&filtered) <= MAX_NAME_LEN {
                let blank = filtered.trim().is_empty();
                let name = (!blank).then(|| Text::literal(filtered));
                let _ = stand.set_custom_name(host, name);
                let _ = stand.set_custom_name_visible(host, !blank);
            }
        }
        EditMessage::Style { option, value } => {
            if option.creative_only() && player.has_infinite_materials(host) != Some(true) {
                return Ok(());
            }
            let _ = stand.merge_nbt(host, option.to_nbt(value));
            if let Some(live) = sessions().get_mut(&uuid)
                && live.window == session.window
            {
                match option {
                    StyleOption::Sealed => {
                        live.disabled_slots = if value { ALL_SLOTS_DISABLED } else { 0 };
                    }
                    StyleOption::ShowArms => {
                        live.show_arms = value;
                    }
                    _ => {}
                }
            }
        }
        EditMessage::Pose(parts) => {
            if parts.iter().flatten().all(|v| v.is_finite()) {
                let _ = stand.merge_nbt(host, pose_nbt(&parts));
            }
        }
        EditMessage::Position(pos) => {
            if pos.iter().all(|v| v.is_finite())
                && player
                    .within_entity_reach(host, stand, EntityReach::Interact)
                    .unwrap_or(false)
            {
                let _ = stand.teleport_to(host, pos, None);
            }
        }
        EditMessage::Scale(scale) => {
            if scale.is_finite() {
                let scale = scale.clamp(MIN_SCALE, MAX_SCALE);
                let id = Identifier::parse(SCALE_BONUS_ID).expect("valid modifier id");
                if (scale - DEFAULT_SCALE).abs() < 1e-6 {
                    let _ = living.remove_attribute_modifier(host, "minecraft:scale", &id);
                } else {
                    let modifier = AttributeModifier::new(
                        id,
                        f64::from(scale - DEFAULT_SCALE),
                        AttributeOperation::AddValue,
                    );
                    let _ = living.replace_attribute_modifier(
                        host,
                        "minecraft:scale",
                        modifier,
                        ModifierLifetime::Permanent,
                    );
                }
            }
        }
        EditMessage::Rotation(yaw) => {
            if yaw.is_finite() {
                let _ = stand.set_rotation(host, yaw, 0.0);
                let mut c = mod_api::Compound::new();
                c.insert(
                    "Rotation".into(),
                    Tag::List(vec![
                        Tag::Float(yaw.to_bits()),
                        Tag::Float(0.0_f32.to_bits()),
                    ]),
                );
                let _ = stand.merge_nbt(host, c);
            }
        }
    }
    Ok(())
}
