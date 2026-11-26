#![deny(unsafe_code)]

//! Pick Up Notifier for Sunrise: shows a customizable HUD list of items, experience orbs,
//! and shot arrows you have just collected.

mod config;
mod entries;
mod hud;
mod payloads;

use crate::config::{
    CLIENT_CONFIG_FILE, ClientConfig, CombineEntries, MOD_ID, ServerConfig, register_mod_configs,
    register_settings,
};
use crate::entries::{
    DisplayEntry, EntryCollector, ItemPickup, count_in_local_inventory, is_stack_enchanted,
    max_entries_for_height, resolve_stack_rarity,
};
use crate::hud::render_entries;
use crate::payloads::{
    Channels, TakeItemPayload, TakeXpPayload, channels, id_take_item, id_take_xp, init_channels,
};
use mod_api::{
    ClientHost, ClientPlayerCloneEvent, ClientPlayerLoggingOutEvent, ClientTakeItemEntityEvent,
    ClientTickPostEvent, ConfigEvent, ConfigKind, ConfigSnapshot, Flow, GuiLayerPlacement, Hooks,
    Host, Identifier, ItemPickedUpEvent, ItemPickedUpPostEvent, ItemStack, PickupXpEvent,
    PlayerPickupArrowEvent, RegisterGuiLayersEvent, Registrar, RegistrationContext,
    RenderFramePreEvent, ServerHost, SettingChangedEvent, Text, TriState,
};
use std::collections::VecDeque;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError, RwLock};

const HUD_LAYER_ID: &str = "pickupnotifier:pick_up_entries";
const VANILLA_STATUS_EFFECTS_LAYER: &str = "minecraft:effects";

/// Upper bound on queued server item payloads.
///
/// The original HUD retains at most `(gui_height / display_scale) * max_height / 18 - 1`
/// entries (~22 at default 1080p settings, ~89 at 4K `max_height = 1.0`), so 128 covers
/// realistic display limits with headroom while bounding memory under server floods.
const MAX_PENDING_ITEMS: usize = 128;

/// Upper bound on queued server XP payloads when `combine_entries == Never`.
/// When combining is enabled, all queued XP payloads coalesce into a single entry.
const MAX_PENDING_XP: usize = 64;

/// Maximum number of queued item or XP payloads drained per tick/frame so a burst
/// cannot stall the client thread.
const MAX_DRAIN_PER_TICK: usize = 32;

struct PendingQueue<T> {
    entries: VecDeque<T>,
    cap: usize,
}

impl<T> PendingQueue<T> {
    fn new(cap: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            cap: cap.max(1),
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
    }

    /// Coalesces `incoming` into an existing entry matching `matches` using `merge`,
    /// or pushes to the back (evicting the oldest entry if already at capacity).
    fn push_coalescing(
        &mut self,
        incoming: T,
        mut matches: impl FnMut(&T, &T) -> bool,
        merge: impl FnOnce(&mut T, T),
    ) {
        if let Some(existing) = self.entries.iter_mut().find(|e| matches(e, &incoming)) {
            merge(existing, incoming);
            return;
        }
        while self.entries.len() >= self.cap {
            self.entries.pop_front();
        }
        self.entries.push_back(incoming);
    }

    /// Drains at most `max_count` entries from the front in FIFO order.
    fn drain_up_to(&mut self, max_count: usize) -> Vec<T> {
        let count = self.entries.len().min(max_count);
        self.entries.drain(..count).collect()
    }
}

struct ClientState {
    config: ClientConfig,
    settings_synced: bool,
    collector: EntryCollector,
    server_payloads_seen: bool,
    pending_items: PendingQueue<TakeItemPayload>,
    pending_xp: PendingQueue<TakeXpPayload>,
    partial_tick: f32,
    /// True while this mod writes its own settings to the host, so the resulting
    /// `setting_changed` notices neither re-apply nor re-persist the file.
    syncing_settings: bool,
}

impl Default for ClientState {
    fn default() -> Self {
        Self {
            config: ClientConfig::default(),
            settings_synced: false,
            collector: EntryCollector::default(),
            server_payloads_seen: false,
            pending_items: PendingQueue::new(MAX_PENDING_ITEMS),
            pending_xp: PendingQueue::new(MAX_PENDING_XP),
            partial_tick: 0.0,
            syncing_settings: false,
        }
    }
}

static CLIENT_STATE: LazyLock<Mutex<ClientState>> =
    LazyLock::new(|| Mutex::new(ClientState::default()));
static SERVER_CONFIG: RwLock<ServerConfig> = RwLock::new(ServerConfig {
    partial_pick_ups: true,
    backpack_integration: true,
});

fn lock_client() -> MutexGuard<'static, ClientState> {
    CLIENT_STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn server_config() -> ServerConfig {
    SERVER_CONFIG
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

fn set_server_config(cfg: ServerConfig) {
    *SERVER_CONFIG
        .write()
        .unwrap_or_else(PoisonError::into_inner) = cfg;
}

/// Arrow entities whose item shares their id (vanilla `AbstractArrow.getPickupItem`).
fn is_arrow_entity_id(id: &str) -> bool {
    matches!(id, "minecraft:arrow" | "minecraft:spectral_arrow")
}

fn gui_height_for(host: &dyn ClientHost) -> f32 {
    match (host.window_size(), host.gui_scale()) {
        (Some([_, h]), Some(scale)) if scale > 0 => (h as f32 / scale as f32).ceil(),
        _ => 240.0,
    }
}

fn is_disabled_in_creative(host: &dyn ClientHost, config: &ClientConfig) -> bool {
    config.general.disable_in_creative
        && host
            .local_player()
            .and_then(|p| p.abilities(host))
            .is_some_and(|a| a.instabuild)
}

fn add_item_entry(
    state: &mut ClientState,
    host: &dyn ClientHost,
    stack: ItemStack,
    amount: u32,
    is_arrow: bool,
) {
    if amount == 0 || is_disabled_in_creative(host, &state.config) {
        return;
    }
    if is_arrow {
        if !state.config.general.include_arrows {
            return;
        }
    } else if !state.config.general.include_items {
        return;
    }

    if state
        .config
        .general
        .hidden_items
        .is_hidden(host, stack.item())
    {
        return;
    }

    let enchanted = is_stack_enchanted(host, &stack);
    let rarity = resolve_stack_rarity(host, &stack, enchanted);
    let hover_name = host
        .hover_name(&stack)
        .unwrap_or_else(|_| Text::literal(stack.item().id(host).unwrap_or("item")));
    let base_name = host
        .hover_name(&ItemStack::of(stack.item()))
        .unwrap_or_else(|_| hover_name.clone());
    let inv_count = if state.config.display.inventory_count {
        count_in_local_inventory(host, stack.item())
    } else {
        0
    };

    let entry = DisplayEntry::new_item(
        ItemPickup {
            stack,
            amount,
            hover_name,
            base_name,
            rarity,
            enchanted,
            inventory_count: inv_count,
        },
        state.config.behavior.display_time,
    );
    let max_size = max_entries_for_height(
        gui_height_for(host),
        state.config.display.scale(),
        state.config.display.max_height,
    );
    state
        .collector
        .push(entry, state.config.behavior.combine_entries, max_size);
}

fn add_xp_entry(state: &mut ClientState, host: &dyn ClientHost, value: u32, count: u32) {
    if !state.config.general.include_experience || is_disabled_in_creative(host, &state.config) {
        return;
    }
    let amount = if state.config.general.experience_value {
        value
    } else {
        count
    };
    if amount == 0 {
        return;
    }

    let orb_name = Text::translate("entity.minecraft.experience_orb", vec![]);
    let entry = DisplayEntry::new_experience(
        orb_name,
        amount,
        host.gui_ticks(),
        state.config.behavior.display_time,
    );
    let max_size = max_entries_for_height(
        gui_height_for(host),
        state.config.display.scale(),
        state.config.display.max_height,
    );
    state
        .collector
        .push(entry, state.config.behavior.combine_entries, max_size);
}

fn drain_pending_payloads(state: &mut ClientState, host: &dyn ClientHost) {
    if state.config.general.client_only {
        state.pending_items.clear();
        state.pending_xp.clear();
        return;
    }
    let items = state.pending_items.drain_up_to(MAX_DRAIN_PER_TICK);
    let xps = state.pending_xp.drain_up_to(MAX_DRAIN_PER_TICK);
    for item in items {
        add_item_entry(state, host, item.stack, item.amount, item.is_arrow);
    }
    for xp in xps {
        add_xp_entry(state, host, xp.value, xp.count);
    }
}

fn persist_client_config(host: &dyn Host, config: &ClientConfig) {
    let path = host.config_dir().join(CLIENT_CONFIG_FILE);
    if let Some(parent) = path.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return;
    }
    let _ = std::fs::write(&path, config.to_toml());
}

/// Applies a loaded or reloaded config file to this mod's state.
fn on_config_event(event: ConfigEvent<'_>, snapshot: &ConfigSnapshot) -> Result<(), String> {
    if !matches!(event, ConfigEvent::Loading(_) | ConfigEvent::Reloading(_)) {
        return Ok(());
    }
    match snapshot.kind {
        ConfigKind::Client => {
            let mut state = lock_client();
            state.config = ClientConfig::parse_toml(&snapshot.text);
            state.settings_synced = false;
        }
        ConfigKind::Server => set_server_config(ServerConfig::parse_toml(&snapshot.text)),
        _ => {}
    }
    Ok(())
}

#[derive(Default)]
struct PickUpNotifier;

#[mod_api::sunrise_mod]
impl Hooks for PickUpNotifier {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        register_settings(r);
        register_mod_configs(r, Arc::new(on_config_event));

        let mut payloads = r.payload_handlers();
        let mut net = payloads.registrar("1").optional();
        let take_item =
            net.play_to_client_typed::<TakeItemPayload>(id_take_item(), |payload, _| {
                let mut state = lock_client();
                state.server_payloads_seen = true;
                if !state.config.general.client_only && payload.amount > 0 {
                    state.pending_items.push_coalescing(
                        payload,
                        |existing, incoming| {
                            existing.is_arrow == incoming.is_arrow
                                && existing.stack.is_stackable_with(&incoming.stack)
                        },
                        |existing, incoming| {
                            existing.amount = existing.amount.saturating_add(incoming.amount);
                        },
                    );
                }
                Ok(())
            });
        let take_xp = net.play_to_client_typed::<TakeXpPayload>(id_take_xp(), |payload, _| {
            let mut state = lock_client();
            state.server_payloads_seen = true;
            if !state.config.general.client_only && (payload.value > 0 || payload.count > 0) {
                let combine_xp = state.config.behavior.combine_entries != CombineEntries::Never;
                state.pending_xp.push_coalescing(
                    payload,
                    |_, _| combine_xp,
                    |existing, incoming| {
                        existing.value = existing.value.saturating_add(incoming.value);
                        existing.count = existing.count.saturating_add(incoming.count);
                    },
                );
            }
            Ok(())
        });
        init_channels(Channels { take_item, take_xp });
    }

    fn setting_changed(&self, host: &dyn Host, event: &mut SettingChangedEvent<'_>) {
        if event.mod_id != MOD_ID {
            return;
        }
        let mut state = lock_client();
        if state.syncing_settings {
            return;
        }
        if state.config.apply_setting(event.key, event.value) {
            persist_client_config(host, &state.config);
        }
    }

    fn register_gui_layers(
        &self,
        _cx: &RegistrationContext,
        event: &mut RegisterGuiLayersEvent<'_>,
    ) {
        let _ = event.layers.register(
            HUD_LAYER_ID.into(),
            GuiLayerPlacement::Above(VANILLA_STATUS_EFFECTS_LAYER.into()),
            Box::new(|host, draw| {
                if !host.on(MOD_ID) {
                    return;
                }
                let state = lock_client();
                render_entries(
                    host,
                    draw,
                    &state.collector.entries,
                    &state.config,
                    state.partial_tick,
                );
            }),
        );
    }

    fn render_frame_pre(&self, host: &mut dyn ClientHost, event: &mut RenderFramePreEvent) {
        if !host.on(MOD_ID) {
            return;
        }
        let mut state = lock_client();
        state.partial_tick = event.world_partial_tick;
        drain_pending_payloads(&mut state, host);
    }

    /// Observes pickups straight from the client's own packet, so it works on servers without
    /// this mod. Skipped on servers that send this mod's payloads, which carry exact counts.
    fn client_take_item_entity(
        &self,
        host: &dyn ClientHost,
        event: &mut ClientTakeItemEntityEvent,
    ) {
        if !host.on(MOD_ID) {
            return;
        }
        let Some(local) = host.local_player() else {
            return;
        };
        if event.collector != Some(local.entity()) {
            return;
        }
        let mut state = lock_client();
        if !state.config.general.client_only && state.server_payloads_seen {
            return;
        }
        let collected = event.collected;
        let amount = u32::try_from(event.amount).unwrap_or(0);
        if let Some(item) = host.item_entity(collected) {
            if let Some(Some(stack)) = item.with_stack(host, |view| view.project()) {
                add_item_entry(&mut state, host, stack, amount, false);
            }
            return;
        }
        let kind = collected
            .kind(host)
            .and_then(|key| key.id(host).map(|id| id.to_string()));
        if let Some(id) = kind.as_deref().filter(|id| is_arrow_entity_id(id))
            && let Ok(id) = Identifier::parse(id)
            && let Some(key) = host.item_key(&id)
        {
            add_item_entry(&mut state, host, ItemStack::of(key), amount.max(1), true);
        }
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPostEvent) {
        if !host.on(MOD_ID) {
            return;
        }
        // Writing settings notifies `setting_changed` synchronously, so the state lock is
        // released around the writes.
        let pending_sync = {
            let mut state = lock_client();
            if state.settings_synced {
                None
            } else {
                state.settings_synced = true;
                state.syncing_settings = true;
                Some(state.config.clone())
            }
        };
        if let Some(config) = pending_sync {
            config.write_to_host_settings(host);
            lock_client().syncing_settings = false;
        }
        let mut state = lock_client();

        drain_pending_payloads(&mut state, host);

        if !host.is_paused() {
            let display_time = state.config.behavior.display_time;
            state.collector.tick(display_time);
        }
    }

    fn client_player_logging_out(
        &self,
        _host: &mut dyn ClientHost,
        _event: &mut ClientPlayerLoggingOutEvent,
    ) {
        let mut state = lock_client();
        state.collector.clear();
        state.pending_items.clear();
        state.pending_xp.clear();
        state.server_payloads_seen = false;
    }

    fn client_player_clone(&self, _host: &mut dyn ClientHost, _event: &mut ClientPlayerCloneEvent) {
        let mut state = lock_client();
        state.collector.clear();
    }

    #[listener(priority = -100)]
    fn item_picked_up(&self, host: &mut dyn ServerHost, event: &mut ItemPickedUpEvent<'_>) {
        if !host.on(MOD_ID) || !server_config().backpack_integration {
            return;
        }
        if *event.can_pickup != TriState::False {
            return;
        }
        if event.item.pickup_delay(&*host) != Some(0) {
            return;
        }
        let Some(Some(stack)) = event.item.with_stack(host, |view| view.project()) else {
            return;
        };
        let amount = stack.count();
        if amount == 0 {
            return;
        }
        if let (Some(ch), Some(player)) = (channels(), host.player(event.player)) {
            let payload = TakeItemPayload {
                stack,
                amount,
                is_arrow: false,
            };
            let _ = ch.take_item.send(host, player, &payload);
        }
    }

    fn item_picked_up_post(
        &self,
        host: &mut dyn ServerHost,
        event: &mut ItemPickedUpPostEvent<'_>,
    ) {
        if !host.on(MOD_ID) {
            return;
        }
        let remaining_count = event.remaining.as_ref().map_or(0, |s| s.count());
        let taken = event.original.count().saturating_sub(remaining_count);
        if taken == 0 {
            return;
        }
        if remaining_count > 0 && !server_config().partial_pick_ups {
            return;
        }
        if let (Some(ch), Some(player)) = (channels(), host.player(event.player)) {
            let payload = TakeItemPayload {
                stack: event.original.clone(),
                amount: taken,
                is_arrow: false,
            };
            let _ = ch.take_item.send(host, player, &payload);
        }
    }

    fn player_pickup_arrow(
        &self,
        host: &mut dyn ServerHost,
        event: &mut PlayerPickupArrowEvent<'_>,
    ) {
        if !host.on(MOD_ID) {
            return;
        }
        let Ok(id) = Identifier::parse(event.item) else {
            return;
        };
        let Some(key) = host.item_key(&id) else {
            return;
        };
        if let (Some(ch), Some(player)) = (channels(), host.player(event.player)) {
            let payload = TakeItemPayload {
                stack: ItemStack::of(key),
                amount: 1,
                is_arrow: true,
            };
            let _ = ch.take_item.send(host, player, &payload);
        }
    }

    fn pickup_xp(&self, host: &mut dyn ServerHost, event: &mut PickupXpEvent<'_>) -> Flow {
        if !host.on(MOD_ID) {
            return Flow::Continue;
        }
        let (Ok(value), Ok(count)) = (event.orb.value(&*host), event.orb.count(&*host)) else {
            return Flow::Continue;
        };
        let value = value.max(0) as u32;
        if value > 0
            && count > 0
            && let (Some(ch), Some(player)) = (channels(), host.player(event.player))
        {
            let payload = TakeXpPayload { value, count };
            let _ = ch.take_xp.send(host, player, &payload);
        }
        Flow::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrow_entities_match_their_items() {
        assert!(is_arrow_entity_id("minecraft:arrow"));
        assert!(is_arrow_entity_id("minecraft:spectral_arrow"));
        assert!(!is_arrow_entity_id("minecraft:trident"));
    }

    #[test]
    fn default_config_uses_exclude_named_combining() {
        let state = ClientState::default();
        assert_eq!(
            state.config.behavior.combine_entries,
            CombineEntries::ExcludeNamed
        );
    }

    #[test]
    fn pending_item_queue_caps_coalesces_and_drains_per_tick() {
        let mut queue: PendingQueue<(u32, u32)> = PendingQueue::new(MAX_PENDING_ITEMS);

        // Coalesce repeated pickups of the same item, saturating on overflow.
        queue.push_coalescing(
            (1, u32::MAX - 5),
            |a, b| a.0 == b.0,
            |a, b| a.1 = a.1.saturating_add(b.1),
        );
        queue.push_coalescing(
            (1, 20),
            |a, b| a.0 == b.0,
            |a, b| a.1 = a.1.saturating_add(b.1),
        );
        assert_eq!(queue.entries.len(), 1);
        assert_eq!(queue.entries[0], (1, u32::MAX));

        // Flood with distinct items well past MAX_PENDING_ITEMS.
        for id in 2..500u32 {
            queue.push_coalescing(
                (id, 1),
                |a, b| a.0 == b.0,
                |a, b| a.1 = a.1.saturating_add(b.1),
            );
        }
        assert_eq!(queue.entries.len(), MAX_PENDING_ITEMS);
        // Oldest entries were evicted; newest entries are retained in order.
        let expected_first_id = 500 - MAX_PENDING_ITEMS as u32;
        assert_eq!(queue.entries.front().copied(), Some((expected_first_id, 1)));
        assert_eq!(queue.entries.back().copied(), Some((499, 1)));

        // Draining is capped at MAX_DRAIN_PER_TICK per tick in FIFO order.
        let batch1 = queue.drain_up_to(MAX_DRAIN_PER_TICK);
        assert_eq!(batch1.len(), MAX_DRAIN_PER_TICK);
        assert_eq!(batch1[0], (expected_first_id, 1));
        assert_eq!(queue.entries.len(), MAX_PENDING_ITEMS - MAX_DRAIN_PER_TICK);
    }

    #[test]
    fn pending_xp_queue_coalesces_and_bounds_when_uncombined() {
        let mut combined: PendingQueue<TakeXpPayload> = PendingQueue::new(MAX_PENDING_XP);
        for _ in 0..500 {
            combined.push_coalescing(
                TakeXpPayload {
                    value: 10,
                    count: 1,
                },
                |_, _| true,
                |a, b| {
                    a.value = a.value.saturating_add(b.value);
                    a.count = a.count.saturating_add(b.count);
                },
            );
        }
        assert_eq!(combined.entries.len(), 1);
        assert_eq!(
            combined.entries[0],
            TakeXpPayload {
                value: 5_000,
                count: 500
            }
        );

        let mut uncombined: PendingQueue<TakeXpPayload> = PendingQueue::new(MAX_PENDING_XP);
        for i in 1..=200u32 {
            uncombined.push_coalescing(
                TakeXpPayload { value: i, count: 1 },
                |_, _| false,
                |a, b| {
                    a.value = a.value.saturating_add(b.value);
                    a.count = a.count.saturating_add(b.count);
                },
            );
        }
        assert_eq!(uncombined.entries.len(), MAX_PENDING_XP);
        assert_eq!(
            uncombined.entries.front(),
            Some(&TakeXpPayload {
                value: 201 - MAX_PENDING_XP as u32,
                count: 1
            })
        );
        let drained = uncombined.drain_up_to(MAX_DRAIN_PER_TICK);
        assert_eq!(drained.len(), MAX_DRAIN_PER_TICK);
        assert_eq!(
            uncombined.entries.len(),
            MAX_PENDING_XP - MAX_DRAIN_PER_TICK
        );
    }
}
