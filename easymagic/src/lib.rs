mod client;
mod config;
mod enchant;
mod payloads;
mod server;
mod storage;

use crate::config::{
    ServerConfig, register_server_config, register_settings, server_config,
    set_synced_server_config,
};
use crate::enchant::invalidate_catalog;
use crate::payloads::{
    Channels, CluesPayload, ShelfItemPayload, ShelfQueryPayload, id_clues, id_server_config,
    id_shelf_item, id_shelf_query, init_channels,
};
use crate::server::{
    EnchantingMenuPart, EnchantingTablePart, MENU_ID, finish_menu_open, handle_shelf_query,
    on_server_tick_post, reset_server_state, send_server_config_to,
};
use crate::storage::{EnchantingTableItemHandler, register_attachments};
use mod_api::{
    ClientHost, ClientPlayerLoggingOutEvent, ClientTickPostEvent, Hooks, HostSide, HudMode, ITEMS,
    LevelTickPostEvent, PlayerContainerOpenEvent, PlayerLoggedInEvent, Registrar,
    RegistrationContext, ReloadContext, RenderGuiPostEvent, ServerHost, ServerStartedEvent,
    ServerStoppingEvent, TagsUpdatedEvent,
};
use std::sync::Arc;

#[derive(Default)]
struct EasyMagic;

#[mod_api::sunrise_mod]
impl Hooks for EasyMagic {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        register_settings(r);
        register_server_config(r);
        register_attachments(r);

        let enchanting_table = r.block_key("minecraft:enchanting_table");
        r.block_part(enchanting_table, EnchantingTablePart);
        r.block_capability(&ITEMS, &enchanting_table, |_, side, _, _, offer| {
            offer.offer(|| Box::new(EnchantingTableItemHandler::new(side)));
        });

        r.menu_part(MENU_ID, EnchantingMenuPart);
        let _ = r.register_menu_screen(MENU_ID, Arc::new(client::build_enchanting_screen));

        client::init_enchant_particle(r.particle_key("minecraft:enchant"));

        let mut payloads = r.payload_handlers();
        let mut net = payloads.registrar("1");
        let server_config_ch =
            net.play_to_client_typed::<ServerConfig>(id_server_config(), |cfg, _| {
                set_synced_server_config(cfg);
                Ok(())
            });
        let clues_ch = net.play_to_client_typed::<CluesPayload>(id_clues(), |payload, _| {
            client::on_clues_payload(payload);
            Ok(())
        });
        let shelf_query_ch =
            net.play_to_server_typed::<ShelfQueryPayload>(id_shelf_query(), handle_shelf_query);
        let shelf_item_ch =
            net.play_to_client_typed::<ShelfItemPayload>(id_shelf_item(), |payload, _| {
                client::on_shelf_item_payload(payload);
                Ok(())
            });
        init_channels(Channels {
            server_config: server_config_ch,
            clues: clues_ch,
            shelf_query: shelf_query_ch,
            shelf_item: shelf_item_ch,
        });
    }

    fn server_started(&self, _host: &mut dyn ServerHost, _event: &mut ServerStartedEvent) {
        set_synced_server_config(server_config());
        invalidate_catalog();
    }

    fn server_stopping(&self, _host: &mut dyn ServerHost, _event: &mut ServerStoppingEvent) {
        reset_server_state();
        invalidate_catalog();
    }

    fn tags_updated(&self, _ctx: &dyn ReloadContext, _event: &mut TagsUpdatedEvent) {
        invalidate_catalog();
    }

    fn player_logged_in(&self, host: &mut dyn ServerHost, event: &mut PlayerLoggedInEvent<'_>) {
        if let Some(player) = host.player(event.player) {
            send_server_config_to(host, player);
        }
    }

    fn player_container_open(
        &self,
        host: &mut dyn ServerHost,
        event: &mut PlayerContainerOpenEvent<'_>,
    ) {
        finish_menu_open(host, event);
    }

    fn level_tick_post(&self, host: HostSide<'_>, event: &mut LevelTickPostEvent) {
        if let HostSide::Server(server) = host {
            on_server_tick_post(server, event.level);
        }
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPostEvent) {
        client::on_client_tick_post(host);
    }

    fn render_gui_post(&self, host: &dyn ClientHost, event: &mut RenderGuiPostEvent<'_>) {
        client::on_gui_render_post(host, event.draw, host.hud_mode() != HudMode::AllVisible);
    }

    fn client_player_logging_out(
        &self,
        _host: &mut dyn ClientHost,
        _event: &mut ClientPlayerLoggingOutEvent,
    ) {
        client::reset_client_state();
        set_synced_server_config(ServerConfig::default());
    }
}
