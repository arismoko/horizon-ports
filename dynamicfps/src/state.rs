//! Pure state-machine logic for Dynamic FPS: power state resolution, frame rate limiting,
//! idle tracking, initial-click suppression, and smooth volume transitions.

use crate::battery::{BatterySnapshot, BatteryState};
use crate::config::{
    DynamicFpsConfig, IdleCondition, IgnoreInitialClick, NO_FRAME_RATE_LIMIT, PowerState,
    VOLUME_TRANSITION_IMMEDIATE,
};
use mod_api::SoundSource;
use std::sync::atomic::{AtomicU32, Ordering};

/// Window after regaining focus during which accidental clicks are ignored (`ClickIgnoreHandler`).
pub const CLICK_IGNORE_WINDOW_MS: u64 = 10;

/// Inputs used to decide the active `PowerState`.
#[derive(Clone, Copy, Debug)]
pub struct PowerStateInput {
    pub mod_enabled: bool,
    pub disabled_by_user: bool,
    pub forcing_low_fps: bool,
    pub focused: bool,
    pub hovered: bool,
    pub minimized: bool,
    pub idle: bool,
    pub battery: BatterySnapshot,
    pub battery_enabled: bool,
    pub battery_switch_states: bool,
}

/// Resolve the active `PowerState` in Dynamic FPS's exact priority order.
pub fn resolve_power_state(input: PowerStateInput) -> PowerState {
    if input.disabled_by_user || !input.mod_enabled {
        PowerState::Focused
    } else if input.forcing_low_fps {
        PowerState::Unfocused
    } else if input.focused {
        if input.idle {
            PowerState::Abandoned
        } else if input.battery_enabled
            && input.battery_switch_states
            && input.battery.has_batteries
            && input.battery.state == BatteryState::Discharging
        {
            PowerState::Unplugged
        } else {
            PowerState::Focused
        }
    } else if input.hovered {
        PowerState::Hovered
    } else if !input.minimized {
        PowerState::Unfocused
    } else {
        PowerState::Invisible
    }
}

/// Frame rate cap to apply for `state`.
///
/// Sunrise's `framerate_limit` hook only lowers the limit and ignores values below 1,
/// so a configured target of `0` clamps to `1` FPS and `>= 260` leaves `original` intact.
/// When `has_rendered_first_frame` is `false` after entering a throttled state, one frame
/// renders immediately so HUD status changes appear without waiting a full second.
pub fn limit_for(
    original: i32,
    state: PowerState,
    config: &DynamicFpsConfig,
    has_rendered_first_frame: bool,
) -> i32 {
    let base = original.max(1);
    if state == PowerState::Focused {
        return base;
    }
    let target = config.state(state).frame_rate_target;
    if target >= NO_FRAME_RATE_LIMIT {
        return base;
    }
    if !has_rendered_first_frame && target < 15 {
        return base;
    }
    base.min(target.max(1))
}

/// Tracks user activity to determine when the window enters the `Abandoned` (Idle) state.
#[derive(Clone, Debug, Default)]
pub struct IdleTracker {
    last_activity_ms: Option<u64>,
    last_camera_pose: Option<([f64; 3], f32, f32)>,
    last_cursor: Option<[f32; 2]>,
}

impl IdleTracker {
    pub fn on_activity(&mut self, now_ms: u64) {
        self.last_activity_ms = Some(now_ms);
    }

    pub fn ensure_initialized(&mut self, now_ms: u64) {
        if self.last_activity_ms.is_none() {
            self.last_activity_ms = Some(now_ms);
        }
    }

    /// Observe cursor and camera position/rotation; any change counts as user activity.
    pub fn observe_motion(
        &mut self,
        now_ms: u64,
        cursor: [f32; 2],
        camera_pose: Option<([f64; 3], f32, f32)>,
    ) {
        self.ensure_initialized(now_ms);

        if let Some(prev) = self.last_cursor
            && (prev[0] != cursor[0] || prev[1] != cursor[1])
        {
            self.on_activity(now_ms);
        }
        self.last_cursor = Some(cursor);

        if let Some(pose) = camera_pose {
            if let Some(prev) = self.last_camera_pose
                && prev != pose
            {
                self.on_activity(now_ms);
            }
            self.last_camera_pose = Some(pose);
        } else {
            self.last_camera_pose = None;
        }
    }

    pub fn is_idle(
        &self,
        now_ms: u64,
        config: &DynamicFpsConfig,
        battery: BatterySnapshot,
    ) -> bool {
        if config.idle.timeout_seconds == 0 {
            return false;
        }
        match config.idle.condition {
            IdleCondition::Vanilla => return false,
            IdleCondition::OnBattery => {
                if !battery.has_batteries || battery.state != BatteryState::Discharging {
                    return false;
                }
            }
            IdleCondition::None => {}
        }
        let Some(last) = self.last_activity_ms else {
            return false;
        };
        now_ms.saturating_sub(last) >= u64::from(config.idle.timeout_seconds) * 1000
    }
}

/// Tracks window focus transitions to cancel accidental clicks during the first 10 ms after focus.
#[derive(Clone, Debug)]
pub struct ClickIgnoreTracker {
    was_focused: bool,
    focused_at_ms: Option<u64>,
}

impl Default for ClickIgnoreTracker {
    fn default() -> Self {
        Self {
            was_focused: true,
            focused_at_ms: None,
        }
    }
}

impl ClickIgnoreTracker {
    /// Update focus state. Returns `true` when focus changed.
    pub fn observe_focus(&mut self, focused: bool, now_ms: u64) -> bool {
        if focused == self.was_focused {
            return false;
        }
        self.was_focused = focused;
        if focused {
            self.focused_at_ms = Some(now_ms);
        }
        true
    }

    pub fn should_ignore_click(
        &self,
        now_ms: u64,
        mode: IgnoreInitialClick,
        screen_open: bool,
    ) -> bool {
        match mode {
            IgnoreInitialClick::Disabled => false,
            IgnoreInitialClick::InWorld if screen_open => false,
            IgnoreInitialClick::InWorld | IgnoreInitialClick::Constant => self
                .focused_at_ms
                .is_some_and(|at| now_ms <= at.saturating_add(CLICK_IGNORE_WINDOW_MS)),
        }
    }
}

/// Smoothly interpolates per-`SoundSource` volume multipliers across client ticks (20 Hz).
#[derive(Clone, Debug)]
pub struct SmoothVolumeTracker {
    current: [f32; 11],
}

impl Default for SmoothVolumeTracker {
    fn default() -> Self {
        Self { current: [1.0; 11] }
    }
}

impl SmoothVolumeTracker {
    /// Advance all 11 source multipliers by one client tick (1/20 s) toward `state`'s targets.
    pub fn tick(&mut self, config: &DynamicFpsConfig, state: PowerState) {
        let state_cfg = config.state(state);
        let speed = config.volume_transition;

        for source in SoundSource::ALL {
            let idx = source.index();
            let desired = state_cfg.volume_multiplier(source);
            let cur = self.current[idx];
            if (cur - desired).abs() <= f32::EPSILON {
                self.current[idx] = desired;
                continue;
            }
            if cur < desired {
                if speed.up >= VOLUME_TRANSITION_IMMEDIATE {
                    self.current[idx] = desired;
                } else {
                    self.current[idx] = (cur + speed.up / 20.0).min(desired);
                }
            } else if speed.down >= VOLUME_TRANSITION_IMMEDIATE {
                self.current[idx] = desired;
            } else {
                self.current[idx] = (cur - speed.down / 20.0).max(desired);
            }
        }
    }

    /// Snap immediately if transitions are disabled in `config`.
    pub fn on_state_change(&mut self, config: &DynamicFpsConfig, state: PowerState) {
        if !config.volume_transition.is_active() {
            let state_cfg = config.state(state);
            for source in SoundSource::ALL {
                self.current[source.index()] = state_cfg.volume_multiplier(source);
            }
        }
    }

    pub fn snapshot(&self) -> [f32; 11] {
        self.current
    }
}

/// Lock-free atomic snapshot of the 11 smoothed volume multipliers for the `play_sound` hot path.
pub struct AtomicVolumeTable([AtomicU32; 11]);

impl AtomicVolumeTable {
    pub const fn new() -> Self {
        let one = 1.0_f32.to_bits();
        Self([
            AtomicU32::new(one),
            AtomicU32::new(one),
            AtomicU32::new(one),
            AtomicU32::new(one),
            AtomicU32::new(one),
            AtomicU32::new(one),
            AtomicU32::new(one),
            AtomicU32::new(one),
            AtomicU32::new(one),
            AtomicU32::new(one),
            AtomicU32::new(one),
        ])
    }

    pub fn store(&self, values: [f32; 11]) {
        for (slot, val) in self.0.iter().zip(values) {
            slot.store(val.to_bits(), Ordering::Relaxed);
        }
    }

    /// Effective combined volume multiplier for a sound on `source` (includes `Master`).
    pub fn effective_factor(&self, source: SoundSource) -> f32 {
        let master = f32::from_bits(self.0[SoundSource::Master.index()].load(Ordering::Relaxed));
        if master <= 0.0 {
            return 0.0;
        }
        if source == SoundSource::Master {
            master
        } else {
            let cat = f32::from_bits(self.0[source.index()].load(Ordering::Relaxed));
            (cat * master).clamp(0.0, 1.0)
        }
    }
}

/// Format the F3 debug line matching Dynamic FPS's `DebugEntryFpsMixin` phrasing.
pub fn format_debug_line(state: PowerState, config: &DynamicFpsConfig) -> Option<String> {
    if state == PowerState::Focused {
        return None;
    }
    let target = config.state(state).frame_rate_target;
    if target < 15 {
        Some(format!(
            "§cDynamic FPS (rT: {} P: {})§r",
            target,
            state.key()
        ))
    } else {
        Some(format!("§cDynamic FPS (P: {})§r", state.key()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_input() -> PowerStateInput {
        PowerStateInput {
            mod_enabled: true,
            disabled_by_user: false,
            forcing_low_fps: false,
            focused: true,
            hovered: false,
            minimized: false,
            idle: false,
            battery: BatterySnapshot::default(),
            battery_enabled: false,
            battery_switch_states: false,
        }
    }

    #[test]
    fn power_state_priority_matches_original() {
        // Default focused
        assert_eq!(resolve_power_state(base_input()), PowerState::Focused);

        // Disabled overrides forced low FPS and unfocused
        let mut inp = base_input();
        inp.disabled_by_user = true;
        inp.forcing_low_fps = true;
        inp.focused = false;
        assert_eq!(resolve_power_state(inp), PowerState::Focused);

        // Forced low FPS overrides focused & idle
        let mut inp = base_input();
        inp.forcing_low_fps = true;
        inp.idle = true;
        assert_eq!(resolve_power_state(inp), PowerState::Unfocused);

        // Focused + idle -> Abandoned (wins over Unplugged)
        let mut inp = base_input();
        inp.idle = true;
        inp.battery_enabled = true;
        inp.battery_switch_states = true;
        inp.battery = BatterySnapshot {
            has_batteries: true,
            charge: 40,
            state: BatteryState::Discharging,
        };
        assert_eq!(resolve_power_state(inp), PowerState::Abandoned);

        // Focused + battery discharging -> Unplugged
        inp.idle = false;
        assert_eq!(resolve_power_state(inp), PowerState::Unplugged);

        // Unfocused + hovered -> Hovered
        let mut inp = base_input();
        inp.focused = false;
        inp.hovered = true;
        inp.minimized = true;
        assert_eq!(resolve_power_state(inp), PowerState::Hovered);

        // Unfocused + not minimized -> Unfocused
        inp.hovered = false;
        inp.minimized = false;
        assert_eq!(resolve_power_state(inp), PowerState::Unfocused);

        // Unfocused + minimized -> Invisible
        inp.minimized = true;
        assert_eq!(resolve_power_state(inp), PowerState::Invisible);
    }

    #[test]
    fn limit_for_lowers_only_and_renders_first_frame_immediately() {
        let cfg = DynamicFpsConfig::default();

        // Focused keeps original
        assert_eq!(limit_for(120, PowerState::Focused, &cfg, true), 120);

        // Unfocused default is 1 FPS, but first frame before flag stays unthrottled
        assert_eq!(limit_for(120, PowerState::Unfocused, &cfg, false), 120);
        assert_eq!(limit_for(120, PowerState::Unfocused, &cfg, true), 1);

        // Invisible default is 0 -> clamped to 1 FPS (minimum accepted by framerate_limit)
        assert_eq!(limit_for(10, PowerState::Invisible, &cfg, true), 1);

        // Never raises a lower limit
        let mut cfg_high = DynamicFpsConfig::default();
        cfg_high.unfocused.frame_rate_target = 60;
        assert_eq!(limit_for(30, PowerState::Unfocused, &cfg_high, true), 30);

        // Unlimited target (260) leaves original untouched
        assert_eq!(limit_for(144, PowerState::Unplugged, &cfg, true), 144);
    }

    #[test]
    fn idle_tracker_respects_timeout_condition_and_activity() {
        let mut cfg = DynamicFpsConfig::default();
        cfg.idle.timeout_seconds = 60;
        cfg.idle.condition = IdleCondition::None;

        let mut tracker = IdleTracker::default();
        tracker.on_activity(10_000);
        assert!(!tracker.is_idle(69_999, &cfg, BatterySnapshot::default()));
        assert!(tracker.is_idle(70_000, &cfg, BatterySnapshot::default()));

        // Cursor motion resets idle timer
        tracker.observe_motion(70_500, [10.0, 20.0], None);
        tracker.observe_motion(71_000, [11.0, 20.0], None);
        assert!(!tracker.is_idle(100_000, &cfg, BatterySnapshot::default()));

        // Vanilla condition disables Dynamic FPS idle
        cfg.idle.condition = IdleCondition::Vanilla;
        assert!(!tracker.is_idle(999_999, &cfg, BatterySnapshot::default()));

        // OnBattery requires discharging battery
        cfg.idle.condition = IdleCondition::OnBattery;
        let charging = BatterySnapshot {
            has_batteries: true,
            charge: 50,
            state: BatteryState::Charging,
        };
        let discharging = BatterySnapshot {
            has_batteries: true,
            charge: 50,
            state: BatteryState::Discharging,
        };
        assert!(!tracker.is_idle(999_999, &cfg, charging));
        assert!(tracker.is_idle(999_999, &cfg, discharging));
    }

    #[test]
    fn click_ignore_tracker_enforces_10ms_window_and_mode() {
        let mut tracker = ClickIgnoreTracker::default();
        assert!(tracker.observe_focus(false, 1000));
        assert!(tracker.observe_focus(true, 2000));

        assert!(!tracker.should_ignore_click(2005, IgnoreInitialClick::Disabled, false));
        assert!(tracker.should_ignore_click(2005, IgnoreInitialClick::InWorld, false));
        assert!(!tracker.should_ignore_click(2005, IgnoreInitialClick::InWorld, true));
        assert!(tracker.should_ignore_click(2010, IgnoreInitialClick::Constant, true));
        assert!(!tracker.should_ignore_click(2011, IgnoreInitialClick::Constant, true));
    }

    #[test]
    fn smooth_volume_transitions_fade_at_configured_rates() {
        let cfg = DynamicFpsConfig::default();
        let mut smooth = SmoothVolumeTracker::default();

        // Unfocused default: master = 0.25, down = 0.5/s = 0.025/tick.
        // After 10 ticks: 1.0 - 0.25 = 0.75. After 30 ticks: reaches 0.25.
        for _ in 0..10 {
            smooth.tick(&cfg, PowerState::Unfocused);
        }
        assert!((smooth.snapshot()[SoundSource::Master.index()] - 0.75).abs() < 1e-4);

        for _ in 0..20 {
            smooth.tick(&cfg, PowerState::Unfocused);
        }
        assert!((smooth.snapshot()[SoundSource::Master.index()] - 0.25).abs() < 1e-4);

        // Focused: up = 1.0/s = 0.05/tick -> 15 ticks from 0.25 back to 1.0.
        for _ in 0..15 {
            smooth.tick(&cfg, PowerState::Focused);
        }
        assert!((smooth.snapshot()[SoundSource::Master.index()] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn atomic_volume_table_combines_master_and_source() {
        let table = AtomicVolumeTable::new();
        let mut values = [1.0; 11];
        values[SoundSource::Master.index()] = 0.5;
        values[SoundSource::Music.index()] = 0.4;
        table.store(values);

        assert!((table.effective_factor(SoundSource::Master) - 0.5).abs() < 1e-6);
        assert!((table.effective_factor(SoundSource::Music) - 0.2).abs() < 1e-6);
        assert!((table.effective_factor(SoundSource::Blocks) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn debug_line_formats_only_when_not_focused() {
        let cfg = DynamicFpsConfig::default();
        assert_eq!(format_debug_line(PowerState::Focused, &cfg), None);
        assert_eq!(
            format_debug_line(PowerState::Unfocused, &cfg).as_deref(),
            Some("§cDynamic FPS (rT: 1 P: unfocused)§r")
        );
        assert_eq!(
            format_debug_line(PowerState::Hovered, &cfg).as_deref(),
            Some("§cDynamic FPS (P: hovered)§r")
        );
    }
}
