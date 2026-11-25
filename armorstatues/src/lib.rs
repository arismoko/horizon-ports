//! Armor Statues (Fuzs, MPL-2.0) and Statue Menus for Sunrise.
//!
//! Sneak and use an armor stand with an empty hand to open a tabbed configuration screen
//! (Rotations, Poses, Style, Position, Alignments, Equipment, plus Vanilla Tweaks triggers in
//! client-only mode) built on Sunrise's typed mod API.
mod client;
mod commands;
mod model;
mod server;
mod widgets;

use std::sync::Arc;

use mod_api::{
    ClientHost, ClientTickPostEvent, EntityInteractEvent, Flow, Hooks, HostSide, InteractionResult,
    ItemTooltipEvent, PayloadId, Registrar, RegistrationContext, ScreenClosingEvent,
    ScreenInitPostEvent, ScreenKeyPressedPreEvent, ScreenMouseButtonReleasedPreEvent,
    ScreenMouseScrolledPreEvent, ScreenRenderBackgroundEvent, ScreenRenderPostEvent, ServerHost,
    ServerTickEvent, SettingSpec,
};

use crate::commands::{self, DEFAULT_VT_DELAY_TICKS};
use crate::model::{EDIT_CHANNEL, EditMessage, MENU_ID, SYNC_CHANNEL, SyncMessage};
use crate::server::ArmorStandMenuPart;

#[derive(Default)]
struct Mod;

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        r.menu_part(MENU_ID, ArmorStandMenuPart);
        let _ = r.register_menu_screen(MENU_ID, Arc::new(client::create_menu_screen));

        let mut payloads = r.payload_handlers();
        let mut net = payloads.registrar("1").optional();
        let edit_token = net.play_to_server_typed::<EditMessage>(
            PayloadId(EDIT_CHANNEL.into()),
            server::handle_edit,
        );
        let sync_token = net.play_to_client_typed::<SyncMessage>(
            PayloadId(SYNC_CHANNEL.into()),
            client::handle_sync,
        );
        client::set_edit_token(edit_token);
        server::set_sync_token(sync_token);

        if let Ok(key) = r.register_setting(
            SettingSpec::toggle(
                "use_vanilla_tweaks_triggers",
                "setting.armorstatues.use_vanilla_tweaks_triggers",
                false,
            )
            .tooltip("setting.armorstatues.use_vanilla_tweaks_triggers.tooltip"),
        ) {
            let _ = commands::VANILLA_TWEAKS_SETTING.set(key);
        }
        if let Ok(key) = r.register_setting(
            SettingSpec::range(
                "client_command_delay",
                "setting.armorstatues.client_command_delay",
                f64::from(DEFAULT_VT_DELAY_TICKS),
                200.0,
                1.0,
                f64::from(DEFAULT_VT_DELAY_TICKS),
            )
            .tooltip("setting.armorstatues.client_command_delay.tooltip"),
        ) {
            let _ = commands::CLIENT_DELAY_SETTING.set(key);
        }
    }

    fn entity_interact(
        &self,
        host: HostSide<'_>,
        event: &mut EntityInteractEvent<'_>,
    ) -> Option<InteractionResult> {
        server::on_entity_interact(host, event)
    }

    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent) {
        server::on_server_tick(host, event);
    }

    fn screen_init_post(&self, host: &dyn ClientHost, event: &mut ScreenInitPostEvent<'_>) {
        client::on_screen_init_post(host, event);
    }

    fn screen_key_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenKeyPressedPreEvent<'_>,
    ) -> Flow {
        client::on_screen_key_pressed_pre(host, event)
    }

    fn screen_mouse_scrolled_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseScrolledPreEvent<'_>,
    ) -> Flow {
        client::on_screen_mouse_scrolled_pre(host, event)
    }

    fn screen_mouse_button_released_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseButtonReleasedPreEvent<'_>,
    ) -> Flow {
        client::on_screen_mouse_button_released_pre(host, event)
    }

    fn screen_closing(&self, host: &dyn ClientHost, event: &mut ScreenClosingEvent<'_>) {
        client::on_screen_closing(host, event);
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPostEvent) {
        client::on_client_tick_post(host);
    }

    fn screen_render_background(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenRenderBackgroundEvent<'_>,
    ) {
        client::on_screen_render_background(host, event);
    }

    fn screen_render_post(&self, host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
        client::on_screen_render_post(host, event);
    }

    fn item_tooltip(&self, host: &dyn ClientHost, event: &mut ItemTooltipEvent<'_>) {
        client::on_item_tooltip(host, event);
    }
}
