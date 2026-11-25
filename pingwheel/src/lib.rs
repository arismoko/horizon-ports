#![deny(unsafe_code)]

//! Ping Wheel for Sunrise: mark world locations and entities with HUD markers,
//! off-screen direction indicators, team/channel scoping, and directional audio alerts.

mod client;
mod config;
mod math;
mod payloads;
mod rate_limiter;
mod screen;
mod server;

use crate::config::{
    ClientConfig, MOD_ID, ServerConfig, persist_client_config, register_mod_configs,
    register_settings, set_client_config, set_server_config, update_client_config,
};
use crate::payloads::{
    Channels, PingLocationC2S, PingLocationS2C, UpdateChannelC2S, id_ping_location_c2s,
    id_ping_location_s2c, id_update_channel_c2s, init_channels,
};
use mod_api::{
    ClientDisconnectingEvent, ClientHost, ClientPlayerLoggingInEvent, ClientReloadHost,
    ClientResourcesReloadedEvent, ClientTickPreEvent, CommandEvent, ConfigContext, ConfigKind,
    ConfigLoadingEvent, ConfigReloadingEvent, Flow, Hooks, HostSide, PlayerLoggedOutEvent,
    RegisterClientCommandsEvent, RegisterCommandsEvent, RegisterConfigsEvent, Registrar,
    RegistrationContext, RenderGuiPostEvent, ScreenClosingEvent, ScreenInitPostEvent,
    ScreenRenderPostEvent, ServerHost, ServerStartedEvent, ServerStoppingEvent,
    SettingChangedEvent,
};

#[derive(Default)]
struct PingWheel;

#[mod_api::sunrise_mod]
impl Hooks for PingWheel {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        register_settings(r);
        client::register_key_mappings(r);

        let mut payloads = r.payload_handlers();
        let mut net = payloads.registrar("1");
        let ping_location_c2s = net.play_to_server_typed::<PingLocationC2S>(
            id_ping_location_c2s(),
            server::handle_ping_location,
        );
        let ping_location_s2c =
            net.play_to_client_typed::<PingLocationS2C>(id_ping_location_s2c(), |packet, _| {
                client::on_incoming_ping_packet(packet);
                Ok(())
            });
        let update_channel_c2s = net.play_to_server_typed::<UpdateChannelC2S>(
            id_update_channel_c2s(),
            server::handle_update_channel,
        );
        init_channels(Channels {
            ping_location_c2s,
            ping_location_s2c,
            update_channel_c2s,
        });
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
                set_client_config(ClientConfig::parse_toml(&event.config.text));
            }
            ConfigKind::Server => {
                set_server_config(ServerConfig::parse_toml(&event.config.text));
            }
            _ => {}
        }
    }

    fn config_reloading(&self, host: HostSide<'_>, event: &mut ConfigReloadingEvent<'_>) {
        if event.config.mod_id != MOD_ID {
            return;
        }
        match event.config.kind {
            ConfigKind::Client => {
                let parsed = ClientConfig::parse_toml(&event.config.text);
                set_client_config(parsed.clone());
                if let HostSide::Client(client_host) = host {
                    parsed.write_to_host_settings(client_host);
                }
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
        let updated = update_client_config(|cfg| {
            cfg.apply_setting_from_host(&*host, event.key);
        });
        persist_client_config(&*host, &updated);
    }

    fn register_commands(&self, _host: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        server::register_server_commands(event.commands);
    }

    fn register_client_commands(
        &self,
        _host: &mut dyn ClientHost,
        event: &mut RegisterClientCommandsEvent<'_>,
    ) {
        client::register_client_commands(event.commands);
    }

    fn command(&self, host: &mut dyn ClientHost, event: &mut CommandEvent<'_>) -> Flow {
        client::handle_client_command(host, event.command)
    }

    fn client_resources_reloaded(
        &self,
        _host: &dyn ClientReloadHost,
        _event: &mut ClientResourcesReloadedEvent,
    ) {
        client::on_resources_reloaded();
    }

    fn client_player_logging_in(
        &self,
        host: &mut dyn ClientHost,
        _event: &mut ClientPlayerLoggingInEvent,
    ) {
        client::on_client_logging_in(host);
    }

    fn client_disconnecting(&self, _host: &dyn ClientHost, _event: &mut ClientDisconnectingEvent) {
        client::on_client_disconnecting();
    }

    fn client_tick_pre(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPreEvent) {
        client::on_client_tick_pre(host);
    }

    fn render_gui_post(&self, host: &dyn ClientHost, event: &mut RenderGuiPostEvent<'_>) {
        client::on_render_gui_post(host, event.draw);
    }

    fn screen_init_post(&self, host: &dyn ClientHost, event: &mut ScreenInitPostEvent<'_>) {
        screen::on_screen_init_post(host, event.screen, event.widgets);
    }

    fn screen_render_post(&self, host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
        screen::on_screen_render_post(host, event.draw, event.screen);
    }

    fn screen_closing(&self, host: &dyn ClientHost, event: &mut ScreenClosingEvent<'_>) {
        screen::on_screen_closing(host, event.screen);
    }

    fn player_logged_out(&self, _host: &mut dyn ServerHost, event: &mut PlayerLoggedOutEvent<'_>) {
        server::on_player_logged_out(event.player);
    }

    fn server_started(&self, _host: &mut dyn ServerHost, _event: &mut ServerStartedEvent) {
        server::reset_server_state();
    }

    fn server_stopping(&self, _host: &mut dyn ServerHost, _event: &mut ServerStoppingEvent) {
        server::reset_server_state();
    }
}
