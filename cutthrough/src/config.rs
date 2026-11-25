//! Cut Through's one client option, as `config/cutthrough-client.toml` and an Options > Mods toggle.

use mod_api::{
    ClientHost, ConfigKind, ConfigSpec, Registrar, SettingKey, SettingSpec, SettingValue,
};

pub const MOD_ID: &str = "cutthrough";
pub const CONFIG_FILE_NAME: &str = "cutthrough-client.toml";
pub const SET_TARGET_ALIVE_ONLY: &str = "target_alive_only";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CutThroughConfig {
    /// Only entities that are alive can be targeted, so a mob that is dying does not absorb hits.
    pub target_alive_only: bool,
}

impl Default for CutThroughConfig {
    fn default() -> Self {
        Self {
            target_alive_only: true,
        }
    }
}

impl CutThroughConfig {
    /// Reads `key = value` lines; unknown keys and malformed values keep the default.
    pub fn parse_toml(text: &str) -> Self {
        let mut config = Self::default();
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if key.trim() == SET_TARGET_ALIVE_ONLY {
                match value.trim() {
                    "true" => config.target_alive_only = true,
                    "false" => config.target_alive_only = false,
                    _ => {}
                }
            }
        }
        config
    }

    pub fn to_toml(&self) -> String {
        format!(
            "# Only allow targeting alive entities, as opposed to entities that have already died and only remain to render a death animation. Greatly helps in combat as dead entities will no longer absorb hits.\n{SET_TARGET_ALIVE_ONLY} = {}\n",
            self.target_alive_only
        )
    }

    pub fn write_to_host_settings(&self, host: &mut dyn ClientHost, key: SettingKey) {
        let _ = host.set_setting(key, SettingValue::Toggle(self.target_alive_only));
    }

    /// Applies one changed Options setting; true when it was ours and a toggle.
    pub fn apply_single_setting(&mut self, key: &str, value: SettingValue) -> bool {
        if key == SET_TARGET_ALIVE_ONLY
            && let SettingValue::Toggle(value) = value
        {
            self.target_alive_only = value;
            return true;
        }
        false
    }
}

/// The client config file `config/cutthrough-client.toml`.
pub fn config_spec() -> ConfigSpec {
    ConfigSpec::new(ConfigKind::Client, CutThroughConfig::default().to_toml())
        .file(CONFIG_FILE_NAME)
}

/// Register the setting on Sunrise's Options > Mods > Cut Through page.
pub fn register_settings(r: &mut dyn Registrar) -> Option<SettingKey> {
    r.register_setting(
        SettingSpec::toggle(
            SET_TARGET_ALIVE_ONLY,
            "config.cutthrough.target_alive_only",
            true,
        )
        .tooltip("config.cutthrough.target_alive_only_tooltip"),
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_alive_only() {
        assert!(CutThroughConfig::default().target_alive_only);
    }

    #[test]
    fn round_trips_through_toml() {
        for value in [true, false] {
            let config = CutThroughConfig {
                target_alive_only: value,
            };
            assert_eq!(CutThroughConfig::parse_toml(&config.to_toml()), config);
        }
    }

    #[test]
    fn malformed_input_keeps_default() {
        assert!(CutThroughConfig::parse_toml("").target_alive_only);
        assert!(CutThroughConfig::parse_toml("target_alive_only = maybe").target_alive_only);
        assert!(CutThroughConfig::parse_toml("\u{0}= = =\n[x]").target_alive_only);
        assert!(!CutThroughConfig::parse_toml("target_alive_only=false # off").target_alive_only);
    }

    #[test]
    fn single_setting_applies_only_ours() {
        let mut config = CutThroughConfig::default();
        assert!(!config.apply_single_setting("other", SettingValue::Toggle(false)));
        assert!(!config.apply_single_setting(SET_TARGET_ALIVE_ONLY, SettingValue::Choice(0)));
        assert!(config.target_alive_only);
        assert!(config.apply_single_setting(SET_TARGET_ALIVE_ONLY, SettingValue::Toggle(false)));
        assert!(!config.target_alive_only);
    }
}
