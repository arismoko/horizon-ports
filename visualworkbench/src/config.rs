use mod_api::{Config, ConfigKind, Registrar, SettingSpec};
use std::sync::{PoisonError, RwLock};

pub const MOD_ID: &str = "visualworkbench";

pub const SETTING_INGREDIENT_RENDERING: &str = "ingredient_rendering";
pub const SETTING_RESULT_RENDERING: &str = "result_rendering";
pub const SETTING_ROTATE_INGREDIENTS: &str = "rotate_ingredients";

/// The server config: `config/visualworkbench-server.toml`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerConfig {
    /// Leftover vanilla crafting tables turn into Visual Workbench tables when used.
    pub convert_vanilla_workbench_when_interacting: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            convert_vanilla_workbench_when_interacting: true,
        }
    }
}

impl ServerConfig {
    pub fn default_toml() -> &'static str {
        "# Visual Workbench server configuration\n\
         # Leftover vanilla crafting tables will transform when trying to use them.\n\
         convert_vanilla_workbench_when_interacting = true\n"
    }

    /// Reads the flat `key = value` lines; anything it does not understand keeps the default.
    pub fn parse_toml(source: &str) -> Self {
        let mut cfg = Self::default();
        for raw in source.lines() {
            let line = raw.split('#').next().unwrap_or("").trim();
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if key.trim() == "convert_vanilla_workbench_when_interacting" {
                match value.trim() {
                    "true" => cfg.convert_vanilla_workbench_when_interacting = true,
                    "false" => cfg.convert_vanilla_workbench_when_interacting = false,
                    _ => {}
                }
            }
        }
        cfg
    }
}

static SERVER_CONFIG: RwLock<ServerConfig> = RwLock::new(ServerConfig {
    convert_vanilla_workbench_when_interacting: true,
});

pub fn server_config() -> ServerConfig {
    *SERVER_CONFIG.read().unwrap_or_else(PoisonError::into_inner)
}

pub fn set_server_config(cfg: ServerConfig) {
    *SERVER_CONFIG
        .write()
        .unwrap_or_else(PoisonError::into_inner) = cfg;
}

pub fn register_mod_configs(configs: &mut Vec<Config>) {
    configs.push(Config::new(
        MOD_ID,
        ConfigKind::Server,
        ServerConfig::default_toml(),
    ));
}

/// The three client options of the original. The world drawing they steer is not possible on the
/// mod API yet (see the README), so they are kept for when it is.
pub fn register_settings(r: &mut dyn Registrar) {
    let _ = r.register_setting(
        SettingSpec::choice(
            SETTING_INGREDIENT_RENDERING,
            "visualworkbench.setting.ingredient_rendering",
            [
                "visualworkbench.setting.ingredient_rendering.none",
                "visualworkbench.setting.ingredient_rendering.flat",
                "visualworkbench.setting.ingredient_rendering.floating",
            ],
            2,
        )
        .tooltip("visualworkbench.setting.ingredient_rendering.tooltip"),
    );
    let _ = r.register_setting(
        SettingSpec::toggle(
            SETTING_RESULT_RENDERING,
            "visualworkbench.setting.result_rendering",
            true,
        )
        .tooltip("visualworkbench.setting.result_rendering.tooltip"),
    );
    let _ = r.register_setting(
        SettingSpec::choice(
            SETTING_ROTATE_INGREDIENTS,
            "visualworkbench.setting.rotate_ingredients",
            [
                "visualworkbench.setting.rotate_ingredients.closest_player",
                "visualworkbench.setting.rotate_ingredients.crafting_player",
                "visualworkbench.setting.rotate_ingredients.never",
            ],
            0,
        )
        .tooltip("visualworkbench.setting.rotate_ingredients.tooltip"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_toml_parses_to_default() {
        assert_eq!(
            ServerConfig::parse_toml(ServerConfig::default_toml()),
            ServerConfig::default()
        );
    }

    #[test]
    fn false_and_garbage_are_read_safely() {
        let off =
            ServerConfig::parse_toml("convert_vanilla_workbench_when_interacting = false # no");
        assert!(!off.convert_vanilla_workbench_when_interacting);
        let junk =
            ServerConfig::parse_toml("convert_vanilla_workbench_when_interacting = maybe\n=\n[x]");
        assert!(junk.convert_vanilla_workbench_when_interacting);
    }
}
