#![deny(unsafe_code)]

//! Dynamic FPS for Sunrise: reduces frame rate and audio volume while the game window
//! is unfocused, minimized, idle, or on battery.

mod battery;
mod config;
mod hud;
mod state;

use crate::battery::{BatteryMonitor, BatteryNotification, BatterySnapshot};
use crate::config::{
    CONFIG_FILE_NAME, DynamicFpsConfig, MOD_ID, PowerState, register_mod_configs, register_settings,
};
use crate::hud::{ensure_toast_icons_registered, render_hud, toast_icon_id};
use crate::state::{
    AtomicVolumeTable, ClickIgnoreTracker, IdleTracker, PowerStateInput, SmoothVolumeTracker,
    format_debug_line, limit_for, resolve_power_state,
};
use mod_api::{
    ClientHost, ClientSetupContext, ClientSetupEvent, ClientStoppingEvent, ClientTickPreEvent,
    ConfigContext, ConfigKind, ConfigLoadingEvent, ConfigPath, ConfigReloadingEvent, DebugEntry,
    DebugGroup, Flow, FramerateLimitEvent, Hooks, HostSide, Identifier, InputKeyEvent,
    KeyConflictContext, KeyRegistration, LoopContext, ModKeyMappingRef, ModToast,
    MouseButtonPreEvent, MouseScrollingEvent, PlaySoundEvent, RegisterConfigsEvent,
    RegisterDebugEntriesEvent, Registrar, RegistrationContext, RenderFramePreEvent,
    RenderGuiPostEvent, ScreenOpening, ScreenOpeningEvent, SettingChangedEvent, Text,
    ToastAddEvent, ToastIcon, ToastKind,
};
use std::sync::{Mutex, MutexGuard};

const KEY_TOGGLE_FORCED: &str = "toggle_forced";
const KEY_TOGGLE_DISABLED: &str = "toggle_disabled";

struct Runtime {
    config: DynamicFpsConfig,
    settings_synced: bool,
    state: PowerState,
    has_rendered_first_frame: bool,
    focused: bool,
    minimized: bool,
    disabled_by_user: bool,
    forcing_low_fps: bool,
    idle: IdleTracker,
    click_ignore: ClickIgnoreTracker,
    smooth_volume: SmoothVolumeTracker,
    battery: BatteryMonitor,
    pending_battery_toast: Option<(BatteryNotification, u8)>,
    toast_icons_registered: bool,
    key_toggle_forced: Option<ModKeyMappingRef>,
    key_toggle_disabled: Option<ModKeyMappingRef>,
    was_holding_forced: bool,
    was_holding_disabled: bool,
}

impl Runtime {
    fn new() -> Self {
        Self {
            config: DynamicFpsConfig::default(),
            settings_synced: false,
            state: PowerState::Focused,
            has_rendered_first_frame: true,
            focused: true,
            minimized: false,
            disabled_by_user: false,
            forcing_low_fps: false,
            idle: IdleTracker::default(),
            click_ignore: ClickIgnoreTracker::default(),
            smooth_volume: SmoothVolumeTracker::default(),
            battery: BatteryMonitor::new(),
            pending_battery_toast: None,
            toast_icons_registered: false,
            key_toggle_forced: None,
            key_toggle_disabled: None,
            was_holding_forced: false,
            was_holding_disabled: false,
        }
    }

    fn update_state(&mut self, now_ms: u64) -> PowerState {
        let battery = self.battery.snapshot();
        let idle = self.idle.is_idle(now_ms, &self.config, battery);
        let next = resolve_power_state(PowerStateInput {
            mod_enabled: self.config.enabled,
            disabled_by_user: self.disabled_by_user,
            forcing_low_fps: self.forcing_low_fps,
            focused: self.focused,
            // Window hover over unfocused window awaits sunrise-org/sunrise#5549.
            hovered: false,
            minimized: self.minimized,
            idle,
            battery,
            battery_enabled: self.config.battery.enabled,
            battery_switch_states: self.config.battery.switch_states,
        });
        if next != self.state {
            self.state = next;
            self.has_rendered_first_frame = false;
            self.smooth_volume.on_state_change(&self.config, next);
            VOLUME_TABLE.store(self.smooth_volume.snapshot());
        }
        next
    }

    fn sync_initial_settings(&mut self, host: &dyn ClientHost) {
        if self.settings_synced {
            return;
        }
        self.settings_synced = true;
        self.config.apply_stored_host_settings(host);
        self.config.write_to_host_settings(host);
        self.battery.configure(self.config.battery);
        self.smooth_volume.on_state_change(&self.config, self.state);
        VOLUME_TABLE.store(self.smooth_volume.snapshot());
    }

    fn persist_config_file(&self, host: &dyn ClientHost) {
        if let Ok(path) = ConfigPath::new(CONFIG_FILE_NAME) {
            let _ = host
                .config_files()
                .write(&path, self.config.to_toml().as_bytes());
        }
    }

    fn poll_keybinds(&mut self, host: &dyn ClientHost, now_ms: u64) {
        if self.key_toggle_forced.is_none() {
            self.key_toggle_forced = host.mod_key_mapping(MOD_ID, KEY_TOGGLE_FORCED).ok();
        }
        if self.key_toggle_disabled.is_none() {
            self.key_toggle_disabled = host.mod_key_mapping(MOD_ID, KEY_TOGGLE_DISABLED).ok();
        }

        let holding_forced = self
            .key_toggle_forced
            .and_then(|k| k.is_down(host).ok())
            .unwrap_or(false);
        if holding_forced && !self.was_holding_forced {
            self.forcing_low_fps = !self.forcing_low_fps;
            self.idle.on_activity(now_ms);
            self.update_state(now_ms);
        }
        self.was_holding_forced = holding_forced;

        let holding_disabled = self
            .key_toggle_disabled
            .and_then(|k| k.is_down(host).ok())
            .unwrap_or(false);
        if holding_disabled && !self.was_holding_disabled {
            self.disabled_by_user = !self.disabled_by_user;
            self.idle.on_activity(now_ms);
            self.update_state(now_ms);
        }
        self.was_holding_disabled = holding_disabled;
    }

    fn flush_battery_toast(&mut self, host: &mut dyn ClientHost) {
        if let Some(notif) = self.battery.take_pending_notification() {
            self.pending_battery_toast = Some(notif);
        }

        if !self.config.state(self.state).show_toasts {
            return;
        }
        let Some((kind, _)) = self.pending_battery_toast.take() else {
            return;
        };

        if !self.toast_icons_registered {
            self.toast_icons_registered = ensure_toast_icons_registered(host);
        }

        // Refresh charge percentage at display time, matching BatteryToast.onFirstRender.
        let current_charge = self.battery.snapshot().charge;
        let title_key = match kind {
            BatteryNotification::Charging => "toast.dynamicfps.battery_charging",
            BatteryNotification::Discharging => "toast.dynamicfps.battery_draining",
            BatteryNotification::Critical => "toast.dynamicfps.battery_critical",
        };
        let mut toast = ModToast::new(Text::translate(title_key, vec![]));
        toast.description = Some(Text::translate(
            "toast.dynamicfps.battery_charge",
            vec![Text::literal(current_charge.to_string())],
        ));
        if self.toast_icons_registered {
            toast.icon = Some(ToastIcon::Sprite(toast_icon_id(kind)));
        }
        if let Ok(token) = Identifier::from_parts(MOD_ID, "battery") {
            toast.kind = Some(token);
        }
        let _ = host.add_toast(toast);
    }
}

static VOLUME_TABLE: AtomicVolumeTable = AtomicVolumeTable::new();

struct DynamicFps {
    runtime: Mutex<Runtime>,
}

impl Default for DynamicFps {
    fn default() -> Self {
        Self {
            runtime: Mutex::new(Runtime::new()),
        }
    }
}

impl DynamicFps {
    fn lock(&self) -> MutexGuard<'_, Runtime> {
        self.runtime.lock().unwrap_or_else(|e| e.into_inner())
    }
}

static DEBUG_STATE: Mutex<(PowerState, DynamicFpsConfig)> = Mutex::new((
    PowerState::Focused,
    // Replaced on startup once config loads.
    DynamicFpsConfig {
        enabled: true,
        uncap_menu_frame_rate: false,
        ignore_initial_click: config::IgnoreInitialClick::Disabled,
        idle: config::IdleConfig {
            timeout_seconds: 300,
            condition: config::IdleCondition::Vanilla,
        },
        volume_transition: config::VolumeTransitionConfig { up: 1.0, down: 0.5 },
        battery: config::BatteryTrackerConfig {
            enabled: false,
            critical_level: 10,
            switch_states: false,
            notifications: true,
            show_when_debug: false,
            display_condition: config::BatteryIndicatorCondition::Critical,
            display_placement: config::BatteryIndicatorPlacement::TopLeft,
            mock_battery_data: false,
        },
        hovered: config::StateConfig::ACTIVE,
        unfocused: config::StateConfig::ACTIVE,
        invisible: config::StateConfig::ACTIVE,
        unplugged: config::StateConfig::ACTIVE,
        abandoned: config::StateConfig::ACTIVE,
    },
));

fn publish_debug_state(state: PowerState, config: &DynamicFpsConfig) {
    let mut guard = DEBUG_STATE.lock().unwrap_or_else(|e| e.into_inner());
    guard.0 = state;
    guard.1 = config.clone();
}

#[mod_api::sunrise_mod]
impl Hooks for DynamicFps {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        register_settings(r);

        r.register_key_mapping(
            KeyRegistration::new(KEY_TOGGLE_FORCED, "key.keyboard.unknown")
                .label("key.dynamicfps.toggle_forced")
                .category("key.categories.misc")
                .context(KeyConflictContext::Universal),
        );
        r.register_key_mapping(
            KeyRegistration::new(KEY_TOGGLE_DISABLED, "key.keyboard.unknown")
                .label("key.dynamicfps.toggle_disabled")
                .category("key.categories.misc")
                .context(KeyConflictContext::Universal),
        );
    }

    fn register_configs(&self, _ctx: &dyn ConfigContext, event: &mut RegisterConfigsEvent<'_>) {
        register_mod_configs(event.configs);
    }

    fn config_loading(&self, _ctx: &dyn ConfigContext, event: &mut ConfigLoadingEvent<'_>) {
        if event.config.mod_id != MOD_ID || event.config.kind != ConfigKind::Client {
            return;
        }
        let parsed = DynamicFpsConfig::parse_toml(&event.config.text);
        let mut guard = self.lock();
        let rt = &mut *guard;
        rt.config = parsed;
        rt.battery.configure(rt.config.battery);
        rt.smooth_volume.on_state_change(&rt.config, rt.state);
        VOLUME_TABLE.store(rt.smooth_volume.snapshot());
        publish_debug_state(rt.state, &rt.config);
    }

    fn config_reloading(&self, host: HostSide<'_>, event: &mut ConfigReloadingEvent<'_>) {
        if event.config.mod_id != MOD_ID || event.config.kind != ConfigKind::Client {
            return;
        }
        let parsed = DynamicFpsConfig::parse_toml(&event.config.text);
        let mut guard = self.lock();
        let rt = &mut *guard;
        rt.config = parsed;
        rt.settings_synced = true;
        rt.battery.configure(rt.config.battery);
        if let HostSide::Client(client) = host {
            rt.config.write_to_host_settings(client);
            let now_ms = client.millis();
            rt.update_state(now_ms);
        }
        publish_debug_state(rt.state, &rt.config);
    }

    fn setting_changed(&self, host: &mut dyn ClientHost, event: &mut SettingChangedEvent<'_>) {
        if event.mod_id != MOD_ID {
            return;
        }
        let mut guard = self.lock();
        let rt = &mut *guard;
        rt.sync_initial_settings(host);
        if rt.config.apply_single_setting(host, event.key) {
            rt.battery.configure(rt.config.battery);
            let now_ms = host.millis();
            rt.update_state(now_ms);
            rt.persist_config_file(host);
            publish_debug_state(rt.state, &rt.config);
        }
    }

    fn client_setup(&self, _ctx: &mut dyn ClientSetupContext, _event: &mut ClientSetupEvent) {
        let rt = self.lock();
        publish_debug_state(rt.state, &rt.config);
    }

    fn client_stopping(&self, _host: &mut dyn ClientHost, _event: &mut ClientStoppingEvent) {
        self.lock().battery.stop();
    }

    fn register_debug_entries(
        &self,
        _ctx: &RegistrationContext,
        event: &mut RegisterDebugEntriesEvent<'_>,
    ) {
        event.entries.push(
            DebugEntry::text(
                format!("{MOD_ID}:power_state"),
                DebugGroup::Memory,
                |_host, lines| {
                    let guard = DEBUG_STATE.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(line) = format_debug_line(guard.0, &guard.1) {
                        lines.push(line);
                    }
                },
            )
            .with_reduced_debug_info(true),
        );
    }

    fn framerate_limit(&self, host: &dyn ClientHost, event: &mut FramerateLimitEvent) {
        if !host.on(MOD_ID) {
            return;
        }
        let now_ms = host.millis();
        let mut rt = self.lock();
        rt.sync_initial_settings(host);
        rt.idle.ensure_initialized(now_ms);

        if rt.click_ignore.observe_focus(event.focused, now_ms) {
            rt.idle.on_activity(now_ms);
        }
        rt.focused = event.focused;
        rt.minimized = event.minimized;

        let state = rt.update_state(now_ms);
        publish_debug_state(state, &rt.config);

        event.limit = limit_for(event.limit, state, &rt.config, rt.has_rendered_first_frame);
    }

    fn render_frame_pre(&self, host: &mut dyn ClientHost, _event: &mut RenderFramePreEvent) {
        if !host.on(MOD_ID) {
            return;
        }
        let now_ms = host.millis();
        let focused = host.window_focused();
        let mut rt = self.lock();
        rt.sync_initial_settings(host);
        if rt.click_ignore.observe_focus(focused, now_ms) {
            rt.idle.on_activity(now_ms);
        }
        rt.focused = focused;
        rt.has_rendered_first_frame = true;
    }

    fn client_tick_pre(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPreEvent) {
        if !host.on(MOD_ID) {
            return;
        }
        let now_ms = host.millis();
        let focused = host.window_focused();
        let cursor = host.cursor();
        let camera_pose = host.camera().map(|c| (c.position, c.yaw, c.pitch));

        let mut guard = self.lock();
        let rt = &mut *guard;
        rt.sync_initial_settings(host);

        if rt.click_ignore.observe_focus(focused, now_ms) {
            rt.idle.on_activity(now_ms);
        }
        rt.focused = focused;
        rt.idle.observe_motion(now_ms, cursor, camera_pose);
        rt.poll_keybinds(host, now_ms);

        let state = rt.update_state(now_ms);
        rt.smooth_volume.tick(&rt.config, state);
        VOLUME_TABLE.store(rt.smooth_volume.snapshot());
        publish_debug_state(state, &rt.config);

        rt.flush_battery_toast(host);
    }

    fn play_sound(&self, _ctx: &dyn LoopContext, event: &mut PlaySoundEvent<'_>) {
        let Some(sound) = event.sound.as_mut() else {
            return;
        };
        let factor = VOLUME_TABLE.effective_factor(sound.source);
        if factor <= 0.0 {
            if event.starting {
                *event.sound = None;
            } else {
                sound.volume = 0.0;
            }
        } else if (factor - 1.0).abs() > f32::EPSILON {
            let scaled = sound.volume * factor;
            if scaled.is_finite() {
                sound.volume = scaled.max(0.0);
            }
        }
    }

    fn toast_add(&self, host: &dyn ClientHost, event: &mut ToastAddEvent) -> Flow {
        if !host.on(MOD_ID) {
            return Flow::Continue;
        }
        // Always allow our own delayed battery notifications once flushed.
        if event.kind == ToastKind::Mod {
            return Flow::Continue;
        }
        let rt = self.lock();
        if !rt.config.state(rt.state).show_toasts {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    fn mouse_button_pre(
        &self,
        host: &mut dyn ClientHost,
        _event: &mut MouseButtonPreEvent,
    ) -> Flow {
        if !host.on(MOD_ID) {
            return Flow::Continue;
        }
        let now_ms = host.millis();
        let focused = host.window_focused();
        let screen_open = host.observed_screen_id().is_some();

        let mut rt = self.lock();
        rt.sync_initial_settings(host);
        rt.click_ignore.observe_focus(focused, now_ms);
        rt.focused = focused;
        rt.idle.on_activity(now_ms);

        if rt
            .click_ignore
            .should_ignore_click(now_ms, rt.config.ignore_initial_click, screen_open)
        {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    fn mouse_scrolling(&self, host: &mut dyn ClientHost, _event: &mut MouseScrollingEvent) -> Flow {
        if host.on(MOD_ID) {
            let now_ms = host.millis();
            self.lock().idle.on_activity(now_ms);
        }
        Flow::Continue
    }

    fn input_key(&self, host: &mut dyn ClientHost, _event: &mut InputKeyEvent<'_>) {
        if host.on(MOD_ID) {
            let now_ms = host.millis();
            self.lock().idle.on_activity(now_ms);
        }
    }

    fn screen_opening(
        &self,
        host: &dyn ClientHost,
        _event: &mut ScreenOpeningEvent<'_>,
    ) -> Option<ScreenOpening> {
        if host.on(MOD_ID) {
            let now_ms = host.millis();
            self.lock().idle.on_activity(now_ms);
        }
        None
    }

    fn render_gui_post(&self, host: &dyn ClientHost, event: &mut RenderGuiPostEvent<'_>) {
        if !host.on(MOD_ID) {
            return;
        }
        let (config, battery, disabled_by_user, forcing_low_fps) = {
            let rt = self.lock();
            (
                rt.config.clone(),
                rt.battery.snapshot(),
                rt.disabled_by_user,
                rt.forcing_low_fps,
            )
        };
        let battery: BatterySnapshot = battery;
        render_hud(
            host,
            event.draw,
            &config,
            battery,
            disabled_by_user,
            forcing_low_fps,
        );
    }
}
