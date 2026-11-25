mod anvil;
mod client;
mod config;
mod formatting;
mod payloads;
mod server;
mod storage;

use crate::anvil::invalidate_catalog;
use crate::config::{
    MOD_ID, ServerConfig, register_mod_configs, register_settings, server_config,
    set_server_config, set_synced_server_config,
};
use crate::payloads::{
    Channels, RenameItemPayload, id_rename_item, id_server_config, init_channels,
};
use crate::server::{
    AnvilBlockPart, AnvilMenuPart, MENU_ID, broadcast_server_config, handle_rename_item,
    init_anvil_keys, init_block_particle, on_menu_closed, on_server_tick_post, reset_server_state,
    send_server_config_to,
};
use crate::storage::{ATTACH_ANVIL_CONTENTS, AnvilItemHandler};
use mod_api::{
    ClientDisconnectingEvent, ClientHost, ConfigContext, ConfigKind, ConfigLoadingEvent,
    ConfigReloadingEvent, Hooks, HostSide, ITEMS, LevelTickPostEvent, MenuClosedEvent,
    PlayerLoggedInEvent, RegisterConfigsEvent, Registrar, RegistrationContext, ReloadContext,
    ScreenInitPostEvent, ServerHost, ServerStartedEvent, ServerStoppingEvent, TagsUpdatedEvent,
};
use std::sync::Arc;

#[derive(Default)]
struct EasyAnvils;

#[mod_api::sunrise_mod]
impl Hooks for EasyAnvils {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        register_settings(r);

        let _ = r.register_attachment_type(ATTACH_ANVIL_CONTENTS, false);

        let anvil = r.block_key("minecraft:anvil");
        let chipped_anvil = r.block_key("minecraft:chipped_anvil");
        let damaged_anvil = r.block_key("minecraft:damaged_anvil");
        init_anvil_keys([anvil, chipped_anvil, damaged_anvil]);
        init_block_particle(r.particle_key("minecraft:block"));

        for key in [anvil, chipped_anvil, damaged_anvil] {
            r.block_part(key, AnvilBlockPart);
            r.block_capability(&ITEMS, &key, |_, side, _, _, offer| {
                offer.offer(|| Box::new(AnvilItemHandler::new(side)));
            });
        }

        r.menu_part(MENU_ID, AnvilMenuPart);
        let _ = r.register_menu_screen(MENU_ID, Arc::new(client::build_anvil_screen));

        let mut payloads = r.payload_handlers();
        let mut net = payloads.registrar("1");
        let server_config_ch =
            net.play_to_client_typed::<ServerConfig>(id_server_config(), |cfg, _| {
                set_synced_server_config(cfg);
                Ok(())
            });
        let rename_item_ch =
            net.play_to_server_typed::<RenameItemPayload>(id_rename_item(), handle_rename_item);
        init_channels(Channels {
            server_config: server_config_ch,
            rename_item: rename_item_ch,
        });
    }

    fn register_configs(&self, _ctx: &dyn ConfigContext, event: &mut RegisterConfigsEvent<'_>) {
        register_mod_configs(event.configs);
    }

    fn config_loading(&self, _ctx: &dyn ConfigContext, event: &mut ConfigLoadingEvent<'_>) {
        if event.config.mod_id != MOD_ID || event.config.kind != ConfigKind::Server {
            return;
        }
        let parsed = ServerConfig::parse_toml(&event.config.text);
        set_server_config(parsed.clone());
        set_synced_server_config(parsed);
    }

    fn config_reloading(&self, host: HostSide<'_>, event: &mut ConfigReloadingEvent<'_>) {
        if event.config.mod_id != MOD_ID || event.config.kind != ConfigKind::Server {
            return;
        }
        let parsed = ServerConfig::parse_toml(&event.config.text);
        set_server_config(parsed.clone());
        set_synced_server_config(parsed);
        if let HostSide::Server(server) = host {
            broadcast_server_config(server);
        }
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

    fn menu_closed(&self, host: &mut dyn ServerHost, event: &mut MenuClosedEvent<'_>) {
        on_menu_closed(host, event);
    }

    fn level_tick_post(&self, host: HostSide<'_>, event: &mut LevelTickPostEvent) {
        if let HostSide::Server(server) = host {
            on_server_tick_post(server, event.level);
        }
    }

    fn screen_init_post(&self, host: &dyn ClientHost, event: &mut ScreenInitPostEvent<'_>) {
        client::on_screen_init_post(host, event);
    }

    fn client_disconnecting(&self, _host: &dyn ClientHost, _event: &mut ClientDisconnectingEvent) {
        client::reset_client_state();
        set_synced_server_config(ServerConfig::default());
    }
}
