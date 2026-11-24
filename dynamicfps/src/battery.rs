//! System battery tracking: Linux `/sys/class/power_supply` reader, mock simulator,
//! transition notifications, and a stoppable background polling thread.

use crate::config::{BatteryIndicatorCondition, BatteryTrackerConfig};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Refresh interval matching Dynamic FPS's `BatteryTracker.updateInterval` (15 seconds).
const POLL_INTERVAL: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BatteryState {
    Charging,
    Discharging,
    Full,
    Empty,
    #[default]
    Unknown,
}

impl BatteryState {
    /// Whether the battery is charging or full (`BatteryUtil.isCharging`).
    pub const fn is_charging(self) -> bool {
        matches!(self, Self::Charging | Self::Full)
    }
}

/// Snapshot of system battery state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BatterySnapshot {
    pub has_batteries: bool,
    /// State of charge percentage (`0..=100`).
    pub charge: u8,
    pub state: BatteryState,
}

/// Battery transition toast notification kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatteryNotification {
    Charging,
    Discharging,
    Critical,
}

/// Whether the HUD battery indicator condition is met for `snapshot`.
pub fn indicator_condition_met(
    condition: BatteryIndicatorCondition,
    snapshot: BatterySnapshot,
    critical_level: u8,
) -> bool {
    if !snapshot.has_batteries {
        return false;
    }
    match condition {
        BatteryIndicatorCondition::Disabled => false,
        BatteryIndicatorCondition::Draining => snapshot.state == BatteryState::Discharging,
        BatteryIndicatorCondition::Critical => {
            snapshot.state == BatteryState::Discharging && snapshot.charge <= critical_level
        }
        BatteryIndicatorCondition::Constant => true,
    }
}

/// Compute which notification (if any) a transition from `before` to `after` triggers.
pub fn detect_notification(
    before: BatterySnapshot,
    after: BatterySnapshot,
    critical_level: u8,
) -> Option<BatteryNotification> {
    if !before.has_batteries || !after.has_batteries {
        return None;
    }
    if before.charge > critical_level && after.charge <= critical_level {
        return Some(BatteryNotification::Critical);
    }
    if before.state == BatteryState::Discharging && after.state.is_charging() {
        return Some(BatteryNotification::Charging);
    }
    if before.state.is_charging() && after.state == BatteryState::Discharging {
        return Some(BatteryNotification::Discharging);
    }
    None
}

/// Deterministic mock battery simulator for testing on desktops without a hardware battery.
#[derive(Clone, Debug)]
pub struct MockBatterySimulator {
    charge: u8,
    charging: bool,
}

impl Default for MockBatterySimulator {
    fn default() -> Self {
        Self {
            charge: 50,
            charging: false,
        }
    }
}

impl MockBatterySimulator {
    pub fn next_snapshot(&mut self) -> BatterySnapshot {
        let snap = BatterySnapshot {
            has_batteries: true,
            charge: self.charge,
            state: if self.charging {
                if self.charge >= 100 {
                    BatteryState::Full
                } else {
                    BatteryState::Charging
                }
            } else {
                BatteryState::Discharging
            },
        };

        if self.charging {
            if self.charge >= 100 {
                self.charging = false;
                self.charge = 95;
            } else {
                self.charge = self.charge.saturating_add(5).min(100);
            }
        } else if self.charge <= 5 {
            self.charging = true;
            self.charge = 10;
        } else {
            self.charge = self.charge.saturating_sub(5);
        }

        snap
    }
}

/// Read system batteries from a Linux sysfs power_supply directory (`/sys/class/power_supply`).
pub fn read_linux_power_supply(dir: &Path) -> BatterySnapshot {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return BatterySnapshot::default();
    };

    let mut count = 0u32;
    let mut any_charging = false;
    let mut any_discharging = false;
    let mut any_full = false;
    let mut any_empty = false;

    let mut energy_now_sum = 0u128;
    let mut energy_full_sum = 0u128;
    let mut all_have_energy = true;
    let mut capacity_sum = 0u32;

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = std::fs::read_to_string(path.join("type")) else {
            continue;
        };
        if !kind.trim().eq_ignore_ascii_case("Battery") {
            continue;
        }
        // Skip peripheral/device batteries (wireless mice, keyboards, headsets).
        if let Ok(scope) = std::fs::read_to_string(path.join("scope"))
            && scope.trim().eq_ignore_ascii_case("Device")
        {
            continue;
        }

        let status = std::fs::read_to_string(path.join("status"))
            .ok()
            .map(|s| parse_sysfs_status(&s))
            .unwrap_or(BatteryState::Unknown);

        let energy_pair = read_u64_file(&path.join("energy_now"))
            .zip(read_u64_file(&path.join("energy_full")))
            .or_else(|| {
                read_u64_file(&path.join("charge_now"))
                    .zip(read_u64_file(&path.join("charge_full")))
            })
            .filter(|&(_, full)| full > 0);

        let cap = if let Some((now, full)) = energy_pair {
            energy_now_sum = energy_now_sum.saturating_add(u128::from(now));
            energy_full_sum = energy_full_sum.saturating_add(u128::from(full));
            let pct =
                (u128::from(now).saturating_mul(100) + u128::from(full) / 2) / u128::from(full);
            pct.min(100) as u32
        } else {
            all_have_energy = false;
            match read_u64_file(&path.join("capacity")) {
                Some(c) => c.min(100) as u32,
                None => continue,
            }
        };

        count += 1;
        capacity_sum = capacity_sum.saturating_add(cap);
        match status {
            BatteryState::Charging => any_charging = true,
            BatteryState::Discharging => any_discharging = true,
            BatteryState::Full => any_full = true,
            BatteryState::Empty => any_empty = true,
            BatteryState::Unknown => {}
        }
    }

    if count == 0 {
        return BatterySnapshot::default();
    }

    let state = if any_charging {
        BatteryState::Charging
    } else if any_discharging {
        BatteryState::Discharging
    } else if any_full {
        BatteryState::Full
    } else if any_empty {
        BatteryState::Empty
    } else {
        BatteryState::Unknown
    };

    let charge = if all_have_energy && energy_full_sum > 0 {
        ((energy_now_sum.saturating_mul(100) + energy_full_sum / 2) / energy_full_sum).min(100)
            as u8
    } else {
        ((capacity_sum + count / 2) / count).min(100) as u8
    };

    BatterySnapshot {
        has_batteries: true,
        charge,
        state,
    }
}

fn read_system_battery() -> BatterySnapshot {
    #[cfg(target_os = "linux")]
    {
        read_linux_power_supply(Path::new("/sys/class/power_supply"))
    }
    #[cfg(not(target_os = "linux"))]
    {
        BatterySnapshot::default()
    }
}

fn read_u64_file(path: &Path) -> Option<u64> {
    std::fs::read_to_string(path)
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
}

fn parse_sysfs_status(raw: &str) -> BatteryState {
    match raw.trim().to_ascii_lowercase().as_str() {
        "charging" => BatteryState::Charging,
        "discharging" => BatteryState::Discharging,
        "full" => BatteryState::Full,
        "empty" => BatteryState::Empty,
        _ => BatteryState::Unknown,
    }
}

#[derive(Default)]
struct SharedBattery {
    snapshot: BatterySnapshot,
    has_initial: bool,
    pending_notification: Option<BatteryNotification>,
    critical_level: u8,
    notifications_enabled: bool,
    mock: bool,
}

/// Manages background battery polling and delivers transition notifications safely to the main thread.
pub struct BatteryMonitor {
    shared: Arc<Mutex<SharedBattery>>,
    stop_signal: Arc<(Mutex<bool>, Condvar)>,
    worker: Option<JoinHandle<()>>,
}

impl Default for BatteryMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl BatteryMonitor {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Mutex::new(SharedBattery {
                critical_level: 10,
                notifications_enabled: true,
                ..SharedBattery::default()
            })),
            stop_signal: Arc::new((Mutex::new(false), Condvar::new())),
            worker: None,
        }
    }

    /// Reconfigure polling to match `config`, starting or stopping the worker thread as needed.
    pub fn configure(&mut self, config: BatteryTrackerConfig) {
        let restart_needed = {
            let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            let mock_changed = s.mock != config.mock_battery_data;
            s.critical_level = config.critical_level;
            s.notifications_enabled = config.notifications;
            s.mock = config.mock_battery_data;
            if !config.enabled {
                s.snapshot = BatterySnapshot::default();
                s.has_initial = false;
                s.pending_notification = None;
            }
            mock_changed
        };

        if !config.enabled {
            self.stop();
            return;
        }

        if restart_needed && self.worker.is_some() {
            self.stop();
        }

        if self.worker.is_none() {
            self.start();
        }
    }

    fn start(&mut self) {
        let stop_signal = Arc::new((Mutex::new(false), Condvar::new()));
        self.stop_signal = Arc::clone(&stop_signal);
        let shared = Arc::clone(&self.shared);

        let handle = thread::Builder::new()
            .name("dynamicfps-battery".to_owned())
            .spawn(move || {
                let mut mock_sim = MockBatterySimulator::default();
                loop {
                    let use_mock = shared.lock().unwrap_or_else(|e| e.into_inner()).mock;
                    let next = if use_mock {
                        mock_sim.next_snapshot()
                    } else {
                        read_system_battery()
                    };

                    {
                        let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
                        if s.has_initial
                            && s.notifications_enabled
                            && let Some(notif) =
                                detect_notification(s.snapshot, next, s.critical_level)
                        {
                            s.pending_notification = Some(notif);
                        }
                        s.snapshot = next;
                        s.has_initial = true;
                    }

                    let (lock, cvar) = &*stop_signal;
                    let guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                    if *guard {
                        break;
                    }
                    let (guard, _) = cvar
                        .wait_timeout(guard, POLL_INTERVAL)
                        .unwrap_or_else(|e| e.into_inner());
                    if *guard {
                        break;
                    }
                }
            });

        if let Ok(h) = handle {
            self.worker = Some(h);
        }
    }

    /// Stop the polling thread promptly (called on disable or `client_stopping`).
    pub fn stop(&mut self) {
        let (lock, cvar) = &*self.stop_signal;
        *lock.lock().unwrap_or_else(|e| e.into_inner()) = true;
        cvar.notify_all();
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
    }

    pub fn snapshot(&self) -> BatterySnapshot {
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .snapshot
    }

    /// Take a pending notification (if any) without losing it if the caller is not yet ready.
    pub fn take_pending_notification(&self) -> Option<(BatteryNotification, u8)> {
        let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        let notif = s.pending_notification.take()?;
        Some((notif, s.snapshot.charge))
    }
}

impl Drop for BatteryMonitor {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_temp_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("dynamicfps_test_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sysfs_reader_aggregates_system_batteries_and_skips_ac_and_peripherals() {
        let root = make_temp_dir("sysfs_multi");

        // AC adapter (ignored)
        let ac = root.join("AC0");
        fs::create_dir_all(&ac).unwrap();
        fs::write(ac.join("type"), "Mains\n").unwrap();

        // Wireless mouse battery (Device scope -> ignored)
        let mouse = root.join("hidpp_battery_0");
        fs::create_dir_all(&mouse).unwrap();
        fs::write(mouse.join("type"), "Battery\n").unwrap();
        fs::write(mouse.join("scope"), "Device\n").unwrap();
        fs::write(mouse.join("status"), "Discharging\n").unwrap();
        fs::write(mouse.join("capacity"), "5\n").unwrap();

        // System battery 0: 1000 / 2000 (50%), Full
        let bat0 = root.join("BAT0");
        fs::create_dir_all(&bat0).unwrap();
        fs::write(bat0.join("type"), "Battery\n").unwrap();
        fs::write(bat0.join("scope"), "System\n").unwrap();
        fs::write(bat0.join("status"), "Full\n").unwrap();
        fs::write(bat0.join("energy_now"), "1000\n").unwrap();
        fs::write(bat0.join("energy_full"), "2000\n").unwrap();

        // System battery 1: 3000 / 6000 (50%), Discharging -> total 4000 / 8000 = 50%
        let bat1 = root.join("BAT1");
        fs::create_dir_all(&bat1).unwrap();
        fs::write(bat1.join("type"), "Battery\n").unwrap();
        fs::write(bat1.join("status"), "Discharging\n").unwrap();
        fs::write(bat1.join("energy_now"), "3000\n").unwrap();
        fs::write(bat1.join("energy_full"), "6000\n").unwrap();

        let snap = read_linux_power_supply(&root);
        assert!(snap.has_batteries);
        assert_eq!(snap.charge, 50);
        assert_eq!(snap.state, BatteryState::Discharging);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn sysfs_reader_falls_back_to_capacity_and_handles_missing_dir() {
        let missing = read_linux_power_supply(Path::new("/nonexistent/power_supply_path"));
        assert!(!missing.has_batteries);

        let root = make_temp_dir("sysfs_cap");
        let bat = root.join("BAT0");
        fs::create_dir_all(&bat).unwrap();
        fs::write(bat.join("type"), "Battery\n").unwrap();
        fs::write(bat.join("status"), "Charging\n").unwrap();
        fs::write(bat.join("capacity"), "83\n").unwrap();

        let snap = read_linux_power_supply(&root);
        assert!(snap.has_batteries);
        assert_eq!(snap.charge, 83);
        assert_eq!(snap.state, BatteryState::Charging);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn notifications_match_charging_discharging_and_critical_transitions() {
        let discharging_15 = BatterySnapshot {
            has_batteries: true,
            charge: 15,
            state: BatteryState::Discharging,
        };
        let discharging_10 = BatterySnapshot {
            has_batteries: true,
            charge: 10,
            state: BatteryState::Discharging,
        };
        let charging_15 = BatterySnapshot {
            has_batteries: true,
            charge: 15,
            state: BatteryState::Charging,
        };
        let full_100 = BatterySnapshot {
            has_batteries: true,
            charge: 100,
            state: BatteryState::Full,
        };

        assert_eq!(
            detect_notification(discharging_15, discharging_10, 10),
            Some(BatteryNotification::Critical)
        );
        // Staying below critical does not re-trigger.
        assert_eq!(
            detect_notification(discharging_10, discharging_10, 10),
            None
        );
        assert_eq!(
            detect_notification(discharging_15, charging_15, 10),
            Some(BatteryNotification::Charging)
        );
        // Full <-> Charging does not spam notifications.
        assert_eq!(detect_notification(charging_15, full_100, 10), None);
        assert_eq!(detect_notification(full_100, charging_15, 10), None);
        // Full -> Discharging notifies draining.
        assert_eq!(
            detect_notification(full_100, discharging_15, 10),
            Some(BatteryNotification::Discharging)
        );
    }

    #[test]
    fn indicator_conditions_respect_state_and_critical_level() {
        let discharging_8 = BatterySnapshot {
            has_batteries: true,
            charge: 8,
            state: BatteryState::Discharging,
        };
        let charging_8 = BatterySnapshot {
            has_batteries: true,
            charge: 8,
            state: BatteryState::Charging,
        };
        assert!(!indicator_condition_met(
            BatteryIndicatorCondition::Disabled,
            discharging_8,
            10
        ));
        assert!(indicator_condition_met(
            BatteryIndicatorCondition::Draining,
            discharging_8,
            10
        ));
        assert!(!indicator_condition_met(
            BatteryIndicatorCondition::Draining,
            charging_8,
            10
        ));
        assert!(indicator_condition_met(
            BatteryIndicatorCondition::Critical,
            discharging_8,
            10
        ));
        assert!(!indicator_condition_met(
            BatteryIndicatorCondition::Critical,
            charging_8,
            10
        ));
        assert!(indicator_condition_met(
            BatteryIndicatorCondition::Constant,
            charging_8,
            10
        ));
    }
}
