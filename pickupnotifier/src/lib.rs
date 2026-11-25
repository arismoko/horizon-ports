#![deny(unsafe_code)]

//! Pick Up Notifier for Sunrise: shows a customizable HUD list of items, experience orbs,
//! and shot arrows you have just collected.

mod config;
mod entries;
mod hud;
mod payloads;

use crate::config::{
    CLIENT_CONFIG_FILE, ClientConfig, MOD_ID, ServerConfig, register_mod_configs, register_settings,
};
use crate::entries::{
    ClientFallbackTracker, DisplayEntry, EntryCollector, count_in_local_inventory,
    is_arrow_item_id, is_stack_enchanted, max_entries_for_height, resolve_stack_rarity,
};
use crate::hud::{ensure_xp_orb_texture, render_entries};
use crate::payloads::{
    Channels, TakeItemPayload, TakeXpPayload, channels, id_take_item, id_take_xp, init_channels,
};
use mod_api::{
    ClientDisconnectingEvent, ClientHost, ClientPacketEvent, ClientPlayerCloneEvent,
    ClientPlayerLoggingOutEvent, ClientTickPostEvent, ConfigContext, ConfigKind,
    ConfigLoadingEvent, ConfigPath, ConfigReloadingEvent, Flow, GuiLayerPlacement, Hooks, HostSide,
    Identifier, ItemPickedUpEvent, ItemPickedUpPostEvent, ItemStack, PickupXpEvent,
    PlayerPickupArrowEvent, RegisterConfigsEvent, RegisterGuiLayersEvent, Registrar,
    RegistrationContext, RenderFramePreEvent, ServerHost, SettingChangedEvent, Text, TriState,
};
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError, RwLock};

const HUD_LAYER_ID: &str = "pickupnotifier:pick_up_entries";
const VANILLA_STATUS_EFFECTS_LAYER: &str = "minecraft:effects";

struct ClientState {
    config: ClientConfig,
    settings_synced: bool,
    collector: EntryCollector,
    fallback: ClientFallbackTracker,
    server_payloads_seen: bool,
    pending_items: Vec<TakeItemPayload>,
    pending_xp: Vec<TakeXpPayload>,
    partial_tick: f32,
    xp_texture_ready: bool,
}

impl Default for ClientState {
    fn default() -> Self {
        Self {
            config: ClientConfig::default(),
            settings_synced: false,
            collector: EntryCollector::default(),
            fallback: ClientFallbackTracker::default(),
            server_payloads_seen: false,
            pending_items: Vec::new(),
            pending_xp: Vec::new(),
            partial_tick: 0.0,
            xp_texture_ready: false,
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
        stack,
        amount,
        hover_name,
        base_name,
        rarity,
        enchanted,
        inv_count,
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
    let items = std::mem::take(&mut state.pending_items);
    let xps = std::mem::take(&mut state.pending_xp);
    if state.config.general.client_only {
        return;
    }
    for item in items {
        add_item_entry(state, host, item.stack, item.amount, item.is_arrow);
    }
    for xp in xps {
        add_xp_entry(state, host, xp.value, xp.count);
    }
}

fn persist_client_config(host: &dyn ClientHost, config: &ClientConfig) {
    if let Ok(path) = ConfigPath::new(CLIENT_CONFIG_FILE) {
        let _ = host
            .config_files()
            .write(&path, config.to_toml().as_bytes());
    }
}

#[derive(Default)]
struct PickUpNotifier;

#[mod_api::sunrise_mod]
impl Hooks for PickUpNotifier {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        register_settings(r);

        let mut payloads = r.payload_handlers();
        let mut net = payloads.registrar("1").optional();
        let take_item =
            net.play_to_client_typed::<TakeItemPayload>(id_take_item(), |payload, _| {
                let mut state = lock_client();
                state.server_payloads_seen = true;
                if !state.config.general.client_only {
                    state.pending_items.push(payload);
                }
                Ok(())
            });
        let take_xp = net.play_to_client_typed::<TakeXpPayload>(id_take_xp(), |payload, _| {
            let mut state = lock_client();
            state.server_payloads_seen = true;
            if !state.config.general.client_only {
                state.pending_xp.push(payload);
            }
            Ok(())
        });
        init_channels(Channels { take_item, take_xp });
    }

    fn register_configs(&self, _ctx: &dyn ConfigContext, event: &mut RegisterConfigsEvent<'_>) {
        register_mod_configs(event.configs);
    }

    fn config_loading(&self, _ctx: &dyn ConfigContext, event: &mut ConfigLoadingEvent<'_>) {
        if event.config.mod_id != MOD_ID {
            return;
        }
        match event.config.kind {
            ConfigKind::Client => {
                let mut state = lock_client();
                state.config = ClientConfig::parse_toml(&event.config.text);
                state.settings_synced = false;
            }
            ConfigKind::Server => {
                set_server_config(ServerConfig::parse_toml(&event.config.text));
            }
            _ => {}
        }
    }

    fn config_reloading(&self, _host: HostSide<'_>, event: &mut ConfigReloadingEvent<'_>) {
        if event.config.mod_id != MOD_ID {
            return;
        }
        match event.config.kind {
            ConfigKind::Client => {
                let mut state = lock_client();
                state.config = ClientConfig::parse_toml(&event.config.text);
                state.settings_synced = false;
            }
            ConfigKind::Server => {
                set_server_config(ServerConfig::parse_toml(&event.config.text));
            }
            _ => {}
        }
    }

    fn setting_changed(&self, host: &mut dyn ClientHost, event: &mut SettingChangedEvent<'_>) {
        if event.mod_id != MOD_ID {
            return;
        }
        let mut state = lock_client();
        if state.config.apply_setting(host, event.key) {
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
                    state.xp_texture_ready,
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
        if !state.xp_texture_ready {
            state.xp_texture_ready = ensure_xp_orb_texture(host);
        }
        drain_pending_payloads(&mut state, host);
    }

    fn client_packet(&self, host: &mut dyn ClientHost, event: &mut ClientPacketEvent<'_>) {
        if !host.on(MOD_ID) {
            return;
        }
        if event.packet.id.namespace() == "minecraft"
            && event.packet.id.path() == "take_item_entity"
        {
            lock_client().fallback.on_take_packet();
        }
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPostEvent) {
        if !host.on(MOD_ID) {
            return;
        }
        let mut state = lock_client();
        if !state.settings_synced {
            state.settings_synced = true;
            state.config.write_to_host_settings(host);
        }

        drain_pending_payloads(&mut state, host);

        let use_fallback = state.config.general.client_only || !state.server_payloads_seen;
        let (gained_items, gained_xp) = state.fallback.tick_and_diff(host, use_fallback);
        if use_fallback {
            for (stack, amount) in gained_items {
                let is_arrow = stack.item().id(host).is_some_and(is_arrow_item_id);
                add_item_entry(&mut state, host, stack, amount, is_arrow);
            }
            if gained_xp > 0 {
                add_xp_entry(&mut state, host, gained_xp, 1);
            }
        }

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
        state.fallback.reset();
        state.pending_items.clear();
        state.pending_xp.clear();
        state.server_payloads_seen = false;
    }

    fn client_disconnecting(&self, _host: &dyn ClientHost, _event: &mut ClientDisconnectingEvent) {
        let mut state = lock_client();
        state.collector.clear();
        state.fallback.reset();
        state.pending_items.clear();
        state.pending_xp.clear();
        state.server_payloads_seen = false;
    }

    fn client_player_clone(&self, _host: &mut dyn ClientHost, _event: &mut ClientPlayerCloneEvent) {
        let mut state = lock_client();
        state.collector.clear();
        state.fallback.reset();
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
        let Some(Some(stack)) = event.item.with_stack(&*host, |view| view.project()) else {
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
        let value = event.value.max(0) as u32;
        let count = event.count;
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
    use crate::config::CombineEntries;

    #[test]
    fn default_config_uses_exclude_named_combining() {
        let state = ClientState::default();
        assert_eq!(
            state.config.behavior.combine_entries,
            CombineEntries::ExcludeNamed
        );
    }
}
