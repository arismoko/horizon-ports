mod client;
mod config;
mod server;
mod storage;

use crate::config::{
    MOD_ID, ServerConfig, register_mod_configs, register_settings, set_server_config,
};
use crate::server::{
    CraftingMenuPart, CraftingTablePart, MENU_ID, on_menu_closed, on_server_tick_post,
    reset_server_state,
};
use crate::storage::{ATTACH_TABLE_CONTENTS, CraftingTableItemHandler};
use mod_api::{
    ConfigContext, ConfigKind, ConfigLoadingEvent, ConfigReloadingEvent, Hooks, HostSide, ITEMS,
    LevelTickPostEvent, MenuClosedEvent, RegisterConfigsEvent, Registrar, RegistrationContext,
    ServerHost, ServerStoppingEvent,
};
use std::sync::Arc;

#[derive(Default)]
struct VisualWorkbench;

#[mod_api::sunrise_mod]
impl Hooks for VisualWorkbench {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        register_settings(r);

        let _ = r.register_attachment_type(ATTACH_TABLE_CONTENTS, false);

        let crafting_table = r.block_key("minecraft:crafting_table");
        r.block_part(crafting_table, CraftingTablePart);
        r.block_capability(&ITEMS, &crafting_table, |_, side, _, _, offer| {
            offer.offer(|| Box::new(CraftingTableItemHandler::new(side)));
        });

        r.menu_part(MENU_ID, CraftingMenuPart);
        let _ = r.register_menu_screen(MENU_ID, Arc::new(client::build_crafting_screen));
    }

    fn register_configs(&self, _ctx: &dyn ConfigContext, event: &mut RegisterConfigsEvent<'_>) {
        register_mod_configs(event.configs);
    }

    fn config_loading(&self, _ctx: &dyn ConfigContext, event: &mut ConfigLoadingEvent<'_>) {
        if event.config.mod_id == MOD_ID && event.config.kind == ConfigKind::Server {
            set_server_config(ServerConfig::parse_toml(&event.config.text));
        }
    }

    fn config_reloading(&self, _host: HostSide<'_>, event: &mut ConfigReloadingEvent<'_>) {
        if event.config.mod_id == MOD_ID && event.config.kind == ConfigKind::Server {
            set_server_config(ServerConfig::parse_toml(&event.config.text));
        }
    }

    fn server_stopping(&self, _host: &mut dyn ServerHost, _event: &mut ServerStoppingEvent) {
        reset_server_state();
    }

    fn menu_closed(&self, host: &mut dyn ServerHost, event: &mut MenuClosedEvent<'_>) {
        on_menu_closed(host, event);
    }

    fn level_tick_post(&self, host: HostSide<'_>, event: &mut LevelTickPostEvent) {
        if let HostSide::Server(server) = host {
            on_server_tick_post(server, event.level);
        }
    }
}
