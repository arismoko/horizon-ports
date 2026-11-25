mod anvil;
mod client;
mod config;
mod formatting;
mod payloads;
mod server;
mod storage;

use crate::anvil::invalidate_catalog;
use crate::config::{
    ServerConfig, register_settings, server_config, set_server_config, set_synced_server_config,
};
use crate::payloads::{
    Channels, RenameItemPayload, id_rename_item, id_server_config, init_channels,
};
use crate::server::{
    AnvilBlockPart, AnvilMenuPart, MENU_ID, broadcast_server_config, handle_rename_item,
    init_anvil_keys, init_block_particle, on_server_tick_post, reset_server_state,
    send_server_config_to,
};
use crate::storage::{AnvilItemHandler, anvil_contents_descriptor, set_anvil_contents_key};
use mod_api::{
    ClientHost, ClientPlayerLoggingOutEvent, ConfigEvent, ConfigKind, ConfigSnapshot, ConfigSpec,
    Hooks, HostSide, ITEMS, LevelTickPostEvent, PlayerLoggedInEvent, Registrar,
    RegistrationContext, ReloadContext, ScreenInitPostEvent, ServerHost, ServerStartedEvent,
    ServerStoppingEvent, TagsUpdatedEvent,
};
use std::sync::Arc;

fn apply_server_config(snapshot: &ConfigSnapshot) {
    let parsed = ServerConfig::parse_toml(&snapshot.text);
    set_server_config(parsed.clone());
    set_synced_server_config(parsed);
}

#[derive(Default)]
struct EasyAnvils;

#[mod_api::sunrise_mod]
impl Hooks for EasyAnvils {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        register_settings(r);

        if let Ok(key) = r.register_attachment_type(anvil_contents_descriptor()) {
            set_anvil_contents_key(key);
        }

        let listener = |event: ConfigEvent<'_>, snapshot: &ConfigSnapshot| -> Result<(), String> {
            match event {
                ConfigEvent::Loading(_) => apply_server_config(snapshot),
                ConfigEvent::Reloading(host) => {
                    apply_server_config(snapshot);
                    if let HostSide::Server(server) = host {
                        broadcast_server_config(server);
                    }
                }
                _ => {}
            }
            Ok(())
        };
        let _ = r.register_config(
            ConfigSpec::new(ConfigKind::Server, ServerConfig::default_toml()),
            Some(Arc::new(listener)),
        );

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

    fn level_tick_post(&self, host: HostSide<'_>, event: &mut LevelTickPostEvent) {
        if let HostSide::Server(server) = host {
            on_server_tick_post(server, event.level);
        }
    }

    fn screen_init_post(&self, host: &dyn ClientHost, event: &mut ScreenInitPostEvent<'_>) {
        client::on_screen_init_post(host, event);
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
