#![deny(unsafe_code)]

//! Bad Wither No Cookie - Reloaded for Sunrise: silences loud global and mob sounds,
//! with configurable toggles, craftable Silence books for headwear, and operator commands.

mod catalog;
mod commands;
mod config;
mod rules;

use crate::commands::{
    clear_server_listeners, client_listening, handle_client_command, register_client_commands,
    register_server_commands, remove_server_listener, server_listeners_snapshot,
    set_client_listening,
};
use crate::config::{
    MOD_ID, SilenceSettings, local_settings, persist_config_file, register_mod_configs,
    register_settings, remote_settings, set_local_settings, set_remote_settings,
    update_local_settings,
};
use crate::rules::{
    SilenceProfile, TooltipFlags, gray_translate, listen_message, silence_reason, sound_parts,
};
use mod_api::{
    ClientDisconnectingEvent, ClientHost, ClientPlayerLoggingOutEvent, ClientTickPostEvent,
    CommandEvent, ConfigContext, ConfigKind, ConfigLoadingEvent, ConfigReloadingEvent,
    EquipmentSlot, Flow, Hooks, Host, HostSide, ItemTooltipEvent, LoopContext, PayloadId,
    PlayLevelSoundAtEntityEvent, PlayLevelSoundAtPositionEvent, PlaySoundEvent,
    PlayerLoggedInEvent, PlayerLoggedOutEvent, RegisterClientCommandsEvent, RegisterCommandsEvent,
    RegisterConfigsEvent, Registrar, RegistrationContext, ServerHost, ServerStoppingEvent,
    SettingChangedEvent, SoundSource, ToClient,
};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock, RwLock};

const CHANNEL_SETTINGS: &str = "bwncr:settings";
const MAX_CLIENT_LISTEN_QUEUE: usize = 64;

static SETTINGS_CHANNEL: OnceLock<ToClient<SilenceSettings>> = OnceLock::new();
static CLIENT_PROFILE: LazyLock<RwLock<SilenceProfile>> =
    LazyLock::new(|| RwLock::new(SilenceProfile::NONE));
static CLIENT_LISTEN_QUEUE: LazyLock<Mutex<VecDeque<String>>> =
    LazyLock::new(|| Mutex::new(VecDeque::new()));
static CLIENT_SETTINGS_SYNCED: AtomicBool = AtomicBool::new(false);
static CLIENT_MOD_ENABLED: AtomicBool = AtomicBool::new(true);

fn broadcast_settings(host: &mut dyn ServerHost) {
    if let Some(ch) = SETTINGS_CHANNEL.get() {
        let _ = ch.send_to_all(host, &local_settings());
    }
}

fn send_settings_to(host: &mut dyn ServerHost, player: mod_api::PlayerRef) {
    if let Some(ch) = SETTINGS_CHANNEL.get() {
        let _ = ch.send(host, player, &local_settings());
    }
}

fn handle_server_level_sound(
    host: &mut dyn ServerHost,
    sound_id: &str,
    source: SoundSource,
) -> Flow {
    let settings = local_settings();
    let (path, _) = sound_parts(sound_id);

    if settings.debug_mode {
        eprintln!("[badwithernocookie] Intercepted {path} (server)");
    }

    let (listeners, console_on) = server_listeners_snapshot();
    if !listeners.is_empty() {
        let msg = listen_message(path);
        for uuid in listeners {
            if let Some(player) = host.player(uuid) {
                let _ = player.tell(host, msg.clone());
            }
        }
    }
    if console_on {
        eprintln!("[badwithernocookie] Sound is {path}");
    }

    if let Some(reason) = silence_reason(sound_id, Some(source), &settings, &SilenceProfile::NONE) {
        if settings.debug_mode {
            eprintln!("[badwithernocookie] {reason}");
        }
        Flow::Cancel
    } else {
        Flow::Continue
    }
}

fn reset_client_state() {
    set_remote_settings(None);
    set_client_listening(false);
    *CLIENT_PROFILE.write().unwrap_or_else(|e| e.into_inner()) = SilenceProfile::NONE;
    CLIENT_LISTEN_QUEUE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
}

#[derive(Default)]
struct BadWitherNoCookie;

#[mod_api::sunrise_mod]
impl Hooks for BadWitherNoCookie {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        register_settings(r);

        let mut payloads = r.payload_handlers();
        let mut net = payloads.registrar("1").optional();
        let ch = net.play_to_client_typed::<SilenceSettings>(
            PayloadId(CHANNEL_SETTINGS.into()),
            |settings, _| {
                set_remote_settings(Some(settings));
                Ok(())
            },
        );
        let _ = SETTINGS_CHANNEL.set(ch);
    }

    fn register_configs(&self, _ctx: &dyn ConfigContext, event: &mut RegisterConfigsEvent<'_>) {
        register_mod_configs(event.configs);
    }

    fn config_loading(&self, _ctx: &dyn ConfigContext, event: &mut ConfigLoadingEvent<'_>) {
        if event.config.mod_id != MOD_ID || event.config.kind != ConfigKind::Common {
            return;
        }
        set_local_settings(SilenceSettings::parse_toml(&event.config.text));
    }

    fn config_reloading(&self, host: HostSide<'_>, event: &mut ConfigReloadingEvent<'_>) {
        if event.config.mod_id != MOD_ID || event.config.kind != ConfigKind::Common {
            return;
        }
        let parsed = SilenceSettings::parse_toml(&event.config.text);
        set_local_settings(parsed.clone());
        match host {
            HostSide::Server(server) => broadcast_settings(server),
            HostSide::Client(client) => parsed.write_to_host_settings(client),
        }
    }

    fn setting_changed(&self, host: &mut dyn ClientHost, event: &mut SettingChangedEvent<'_>) {
        if event.mod_id != MOD_ID {
            return;
        }
        let updated = update_local_settings(|settings| {
            if settings.apply_single_setting(host, event.key) {
                Some(settings.clone())
            } else {
                None
            }
        });
        if let Some(settings) = updated {
            persist_config_file(host.config_files(), &settings);
        }
    }

    fn register_commands(&self, _host: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        register_server_commands(event.commands, broadcast_settings);
    }

    fn register_client_commands(
        &self,
        _host: &mut dyn ClientHost,
        event: &mut RegisterClientCommandsEvent<'_>,
    ) {
        register_client_commands(event.commands);
    }

    fn command(&self, host: &mut dyn ClientHost, event: &mut CommandEvent<'_>) -> Flow {
        if !host.on(MOD_ID) {
            return Flow::Continue;
        }
        if handle_client_command(host, event.command) {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    fn player_logged_in(&self, host: &mut dyn ServerHost, event: &mut PlayerLoggedInEvent<'_>) {
        if !host.on(MOD_ID) {
            return;
        }
        if let Some(player) = host.player(event.player) {
            send_settings_to(host, player);
        }
    }

    fn player_logged_out(&self, _host: &mut dyn ServerHost, event: &mut PlayerLoggedOutEvent<'_>) {
        remove_server_listener(event.player);
    }

    fn server_stopping(&self, _host: &mut dyn ServerHost, _event: &mut ServerStoppingEvent) {
        clear_server_listeners();
    }

    fn play_level_sound_at_position(
        &self,
        host: HostSide<'_>,
        event: &mut PlayLevelSoundAtPositionEvent,
    ) -> Flow {
        let HostSide::Server(server) = host else {
            return Flow::Continue;
        };
        if !server.on(MOD_ID) {
            return Flow::Continue;
        }
        let sound_id = event.sound.to_string();
        let source = *event.source;
        handle_server_level_sound(server, &sound_id, source)
    }

    fn play_level_sound_at_entity(
        &self,
        host: HostSide<'_>,
        event: &mut PlayLevelSoundAtEntityEvent,
    ) -> Flow {
        let HostSide::Server(server) = host else {
            return Flow::Continue;
        };
        if !server.on(MOD_ID) {
            return Flow::Continue;
        }
        let sound_id = event.sound.to_string();
        let source = *event.source;
        handle_server_level_sound(server, &sound_id, source)
    }

    fn play_sound(&self, _ctx: &dyn LoopContext, event: &mut PlaySoundEvent<'_>) {
        if !CLIENT_MOD_ENABLED.load(Ordering::Relaxed) {
            return;
        }
        // Work from `event.sound`, so sounds another mod already silenced or replaced are left alone.
        let Some(sound) = event.sound.as_ref() else {
            return;
        };
        let settings = local_settings();
        let (path, _) = sound_parts(&sound.identifier);

        if settings.debug_mode && event.starting {
            eprintln!("[badwithernocookie] Intercepted {path}");
        }

        let profile = CLIENT_PROFILE
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let mut reason = silence_reason(&sound.identifier, Some(sound.source), &settings, &profile);
        if reason.is_none()
            && let Some(remote) = remote_settings()
        {
            reason = silence_reason(
                &sound.identifier,
                Some(sound.source),
                &remote,
                &SilenceProfile::NONE,
            );
        }

        if event.starting && client_listening() {
            let mut queue = CLIENT_LISTEN_QUEUE
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if queue.len() < MAX_CLIENT_LISTEN_QUEUE {
                queue.push_back(path.to_owned());
            }
        }

        if let Some(why) = reason {
            if settings.debug_mode && event.starting {
                eprintln!("[badwithernocookie] {why}");
            }
            *event.sound = None;
        }
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPostEvent) {
        if !CLIENT_SETTINGS_SYNCED.swap(true, Ordering::AcqRel) {
            local_settings().write_to_host_settings(host);
        }

        let enabled = host.on(MOD_ID);
        CLIENT_MOD_ENABLED.store(enabled, Ordering::Relaxed);

        let profile = if enabled {
            let reader: &dyn Host = &*host;
            host.local_player()
                .and_then(|player| {
                    player
                        .living()
                        .with_equipment(reader, EquipmentSlot::Head, |view| {
                            view.project()
                                .map(|stack| SilenceProfile::from_stack(&stack))
                                .unwrap_or_default()
                        })
                })
                .unwrap_or_default()
        } else {
            SilenceProfile::NONE
        };
        *CLIENT_PROFILE.write().unwrap_or_else(|e| e.into_inner()) = profile;

        let pending: Vec<String> = {
            let mut queue = CLIENT_LISTEN_QUEUE
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            queue.drain(..).collect()
        };
        for path in pending {
            let _ = host.chat_line(listen_message(&path));
        }
    }

    fn client_player_logging_out(
        &self,
        _host: &mut dyn ClientHost,
        _event: &mut ClientPlayerLoggingOutEvent,
    ) {
        reset_client_state();
    }

    fn client_disconnecting(
        &self,
        _host: &mut dyn ClientHost,
        _event: &mut ClientDisconnectingEvent,
    ) {
        reset_client_state();
    }

    fn item_tooltip(&self, host: &dyn ClientHost, event: &mut ItemTooltipEvent<'_>) {
        if !host.on(MOD_ID) {
            return;
        }
        let Some(stack) = event.stack.project() else {
            return;
        };
        let flags = TooltipFlags::from_stack(&stack);
        if flags.general {
            let _ = event
                .lines
                .push(gray_translate("tooltip.bwncr.silence"), u32::MAX);
        }
        if flags.everything {
            let _ = event
                .lines
                .push(gray_translate("tooltip.bwncr.silence_all"), u32::MAX);
        }
        if flags.group {
            let _ = event
                .lines
                .push(gray_translate("tooltip.bwncr.silence_group"), u32::MAX);
        }
        if flags.mob {
            let _ = event
                .lines
                .push(gray_translate("tooltip.bwncr.silence_mob"), u32::MAX);
        }
    }
}
