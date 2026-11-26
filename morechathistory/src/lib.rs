#![deny(unsafe_code)]

//! More Chat History for Sunrise: raises the chat history cap
//! (`ChatComponent.MAX_CHAT_HISTORY`, 100 in vanilla) to 16,384 for received messages,
//! wrapped lines and sent lines, through `ClientHost::set_chat_history_limit`.
//!
//! `client_setup` has no live host, so the cap is applied from `client_tick_post`, and again
//! whenever the setting changes.

use std::sync::RwLock;

use mod_api::{
    ClientHost, ClientTickPostEvent, Hooks, Registrar, RegistrationContext, SettingKey,
    SettingSpec, SettingValue,
};

const LIMIT: &str = "limit";
/// The original's cap, and the most the host accepts.
const MAX_LIMIT: u32 = 16_384;
/// Vanilla's `ChatComponent.MAX_CHAT_HISTORY`.
const MIN_LIMIT: u32 = 100;

/// The key the registration pass issued for the limit setting.
static LIMIT_KEY: RwLock<Option<SettingKey>> = RwLock::new(None);

/// The cap a setting value asks for: its whole part clamped to 100..=16,384; the original's
/// 16,384 when the value is missing or not a finite number.
fn wanted_limit(value: Option<SettingValue>) -> u32 {
    match value {
        Some(SettingValue::Range(v)) if v.is_finite() => {
            v.round().clamp(f64::from(MIN_LIMIT), f64::from(MAX_LIMIT)) as u32
        }
        _ => MAX_LIMIT,
    }
}

struct MoreChatHistory;

#[mod_api::sunrise_mod]
impl Hooks for MoreChatHistory {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        let spec = SettingSpec::range(
            LIMIT,
            "morechathistory.config.limit",
            f64::from(MIN_LIMIT),
            f64::from(MAX_LIMIT),
            1.0,
            f64::from(MAX_LIMIT),
        )
        .tooltip("morechathistory.config.limit_tooltip");
        if let Ok(key) = r.register_setting(spec) {
            *LIMIT_KEY.write().unwrap_or_else(|e| e.into_inner()) = Some(key);
        }
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPostEvent) {
        let key = *LIMIT_KEY.read().unwrap_or_else(|e| e.into_inner());
        let wanted = wanted_limit(key.and_then(|k| host.setting(k).ok()));
        if host.chat_history_limit() != wanted {
            host.set_chat_history_limit(wanted);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_the_originals_cap() {
        assert_eq!(wanted_limit(Some(SettingValue::Range(16_384.0))), 16_384);
        assert_eq!(wanted_limit(None), 16_384);
    }

    #[test]
    fn values_are_clamped_to_vanilla_and_the_host_maximum() {
        assert_eq!(wanted_limit(Some(SettingValue::Range(100.0))), 100);
        assert_eq!(wanted_limit(Some(SettingValue::Range(1.0))), 100);
        assert_eq!(wanted_limit(Some(SettingValue::Range(-5.0))), 100);
        assert_eq!(wanted_limit(Some(SettingValue::Range(1e9))), 16_384);
        assert_eq!(wanted_limit(Some(SettingValue::Range(2_500.4))), 2_500);
    }

    #[test]
    fn wrong_kinds_and_non_finite_fall_back_to_the_default() {
        assert_eq!(wanted_limit(Some(SettingValue::Toggle(false))), 16_384);
        assert_eq!(wanted_limit(Some(SettingValue::Range(f64::NAN))), 16_384);
        assert_eq!(
            wanted_limit(Some(SettingValue::Range(f64::INFINITY))),
            16_384
        );
    }
}
