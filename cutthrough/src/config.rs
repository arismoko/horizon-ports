//! Cut Through's one client option, as `config/cutthrough-client.toml` and an Options > Mods toggle.

use mod_api::{ClientHost, Config, ConfigKind, Registrar, SettingSpec};

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

    pub fn write_to_host_settings(&self, host: &dyn ClientHost) {
        host.set_setting(
            MOD_ID,
            SET_TARGET_ALIVE_ONLY,
            if self.target_alive_only {
                "true"
            } else {
                "false"
            },
        );
    }

    /// Applies one changed Options setting; true when it was ours and readable.
    pub fn apply_single_setting(&mut self, host: &dyn ClientHost, key: &str) -> bool {
        if key == SET_TARGET_ALIVE_ONLY
            && let Some(value) = host.setting_bool(key)
        {
            self.target_alive_only = value;
            return true;
        }
        false
    }
}

/// Register the client config file `config/cutthrough-client.toml`.
pub fn register_mod_configs(configs: &mut Vec<Config>) {
    let default_toml = CutThroughConfig::default().to_toml();
    configs.push(Config::new(MOD_ID, ConfigKind::Client, &default_toml).file(CONFIG_FILE_NAME));
}

/// Register the setting on Sunrise's Options > Mods > Cut Through page.
pub fn register_settings(r: &mut dyn Registrar) {
    let _ = r.register_setting(
        SettingSpec::toggle(
            SET_TARGET_ALIVE_ONLY,
            "config.cutthrough.target_alive_only",
            true,
        )
        .tooltip("config.cutthrough.target_alive_only_tooltip"),
    );
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
    fn default_toml_is_the_registered_file() {
        let mut configs = Vec::new();
        register_mod_configs(&mut configs);
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].file, CONFIG_FILE_NAME);
    }
}
