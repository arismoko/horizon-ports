use crate::catalog::catalog;
use crate::config::{LEGACY_NAMESPACE, MOD_ID, SilenceSettings};
use mod_api::text::TextStyle;
use mod_api::{ItemStack, JavaString, SoundSource, Tag, Text};

pub const GENERAL_PATH: &str = "silence";
pub const ALL_PATH: &str = "silence_all";

/// What the Silence enchantments on one player's headwear silence for that player.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SilenceProfile {
    pub all: bool,
    pub everything: bool,
    pub prefixes: Vec<String>,
}

impl SilenceProfile {
    pub const NONE: Self = Self {
        all: false,
        everything: false,
        prefixes: Vec::new(),
    };

    pub fn is_empty(&self) -> bool {
        !self.all && !self.everything && self.prefixes.is_empty()
    }

    pub fn from_enchantment_ids<'a>(ids: impl IntoIterator<Item = &'a str>) -> Self {
        let mut all = false;
        let mut everything = false;
        let mut prefixes = Vec::new();
        let cat = catalog();

        for raw_id in ids {
            let Some((ns, path)) = raw_id.split_once(':') else {
                continue;
            };
            if ns != LEGACY_NAMESPACE && ns != MOD_ID {
                continue;
            }
            if path == GENERAL_PATH {
                all = true;
            } else if path == ALL_PATH {
                everything = true;
            } else if let Some(list) = cat.prefixes_for_enchantment(path) {
                prefixes.extend(list.iter().cloned());
            }
        }

        Self {
            all,
            everything,
            prefixes,
        }
    }

    pub fn from_stack(stack: &ItemStack) -> Self {
        let ids = read_component_enchantment_ids(stack, "minecraft:enchantments");
        if ids.is_empty() {
            return Self::NONE;
        }
        Self::from_enchantment_ids(ids.iter().map(String::as_str))
    }
}

/// Which Silence tooltip lines should be appended to an item stack.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TooltipFlags {
    pub general: bool,
    pub everything: bool,
    pub group: bool,
    pub mob: bool,
}

impl TooltipFlags {
    pub fn from_enchantment_ids<'a>(ids: impl IntoIterator<Item = &'a str>) -> Self {
        let mut flags = Self::default();
        let cat = catalog();

        for raw_id in ids {
            let Some((ns, path)) = raw_id.split_once(':') else {
                continue;
            };
            if ns != LEGACY_NAMESPACE && ns != MOD_ID {
                continue;
            }
            if path == GENERAL_PATH {
                flags.general = true;
            } else if path == ALL_PATH {
                flags.everything = true;
            } else if cat.is_group_enchantment(path) {
                flags.group = true;
            } else if cat.is_mob_enchantment(path) || path.starts_with("silence_") {
                flags.mob = true;
            }
        }

        flags
    }

    pub fn from_stack(stack: &ItemStack) -> Self {
        let mut ids = read_component_enchantment_ids(stack, "minecraft:stored_enchantments");
        ids.extend(read_component_enchantment_ids(
            stack,
            "minecraft:enchantments",
        ));
        Self::from_enchantment_ids(ids.iter().map(String::as_str))
    }
}

fn read_component_enchantment_ids(stack: &ItemStack, component_name: &str) -> Vec<String> {
    let Some(components) = stack.components() else {
        return Vec::new();
    };
    let key = JavaString::from(component_name);
    let Some(Tag::Compound(entry)) = components.as_nbt().get(&key) else {
        return Vec::new();
    };
    let levels = match entry.get(&JavaString::from("levels")) {
        Some(Tag::Compound(inner)) => inner,
        _ => entry,
    };
    levels
        .iter()
        .filter_map(|(id, tag)| match tag {
            Tag::Int(level) if *level > 0 => Some(id.to_string_lossy()),
            _ => None,
        })
        .collect()
}

/// Splits a sound id into its path (`entity.wither.spawn`) and full id (`minecraft:entity.wither.spawn`).
pub fn sound_parts(sound_id: &str) -> (&str, String) {
    if let Some((_, path)) = sound_id.split_once(':') {
        (path, sound_id.to_owned())
    } else {
        (sound_id, format!("minecraft:{sound_id}"))
    }
}

/// Decides whether a sound is silenced under `settings` and `profile`.
/// Returns a short debug reason when silenced, or `None` when allowed.
pub fn silence_reason(
    sound_id: &str,
    source: Option<SoundSource>,
    settings: &SilenceSettings,
    profile: &SilenceProfile,
) -> Option<String> {
    let (path, full_id) = sound_parts(sound_id);

    if settings.silence_wither && matches!(path, "entity.wither.spawn" | "entity.wither.death") {
        return Some("Silencing the wither's spawn/death.".to_owned());
    }
    if settings.silence_trader
        && matches!(
            path,
            "entity.wandering_trader.ambient" | "entity.llama.ambient"
        )
    {
        return Some("Silencing wandering trader and llama ambient sounds.".to_owned());
    }
    if settings.silence_dragon && path == "entity.ender_dragon.death" {
        return Some("Silencing the ender dragon death.".to_owned());
    }
    if settings.silence_lightning && path == "entity.lightning_bolt.thunder" {
        return Some("Silencing thunder.".to_owned());
    }
    if settings.silence_explosion && is_explosion_sound(path) {
        return Some("Silencing an explosion sound.".to_owned());
    }
    if !settings.silence_us.is_empty()
        && settings
            .silence_us
            .iter()
            .any(|entry| !entry.is_empty() && (entry == path || entry == &full_id))
    {
        return Some(format!("Silencing {path}"));
    }
    if !profile.is_empty() {
        if profile.everything {
            return Some(format!("Silencing {path} (Silence All enchantment)"));
        }
        if profile.all && is_entity_sound(path, source) {
            return Some(format!("Silencing {path} (Silence enchantment)"));
        }
        for prefix in &profile.prefixes {
            if matches_prefix(path, prefix) {
                return Some(format!("Silencing {path} (Silence: mob/group enchantment)"));
            }
        }
    }
    None
}

/// A catalog prefix: plain prefixes match the start of the path; a trailing `$` matches the exact path.
pub fn matches_prefix(path: &str, prefix: &str) -> bool {
    if let Some(exact) = prefix.strip_suffix('$') {
        path == exact
    } else {
        path.starts_with(prefix)
    }
}

pub fn is_explosion_sound(path: &str) -> bool {
    matches!(
        path,
        "entity.generic.explode"
            | "entity.tnt.primed"
            | "entity.creeper.primed"
            | "entity.wind_charge.wind_burst"
            | "entity.breeze.wind_burst"
            | "entity.dragon_fireball.explode"
    )
}

pub fn is_entity_sound(path: &str, source: Option<SoundSource>) -> bool {
    if path.starts_with("entity.") {
        return true;
    }
    matches!(
        source,
        Some(
            SoundSource::Hostile
                | SoundSource::Neutral
                | SoundSource::Weather
                | SoundSource::Ambient
        )
    )
}

/// The aqua/red chat line shown by `/bwncr listen`, matching the original mod.
pub fn listen_message(sound_path: &str) -> Text {
    let mut aqua = TextStyle::default();
    aqua.color = Some("aqua".to_owned());
    let mut red = TextStyle::default();
    red.color = Some("red".to_owned());
    Text::literal("Sound is ")
        .with_style(aqua)
        .append(Text::literal(sound_path).with_style(red))
}

/// Gray translatable tooltip line for Silence books and enchanted headwear.
pub fn gray_translate(key: &str) -> Text {
    let mut gray = TextStyle::default();
    gray.color = Some("gray".to_owned());
    Text::translate(key, Vec::new()).with_style(gray)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_toggles_silence_broadcast_sounds_only() {
        let settings = SilenceSettings::default();
        assert!(
            silence_reason(
                "minecraft:entity.wither.spawn",
                Some(SoundSource::Hostile),
                &settings,
                &SilenceProfile::NONE
            )
            .is_some()
        );
        assert!(
            silence_reason(
                "minecraft:entity.wither.death",
                Some(SoundSource::Hostile),
                &settings,
                &SilenceProfile::NONE
            )
            .is_some()
        );
        assert!(
            silence_reason(
                "minecraft:entity.ender_dragon.death",
                Some(SoundSource::Hostile),
                &settings,
                &SilenceProfile::NONE
            )
            .is_some()
        );
        assert!(
            silence_reason(
                "minecraft:entity.lightning_bolt.thunder",
                Some(SoundSource::Weather),
                &settings,
                &SilenceProfile::NONE
            )
            .is_some()
        );
        assert!(
            silence_reason(
                "minecraft:entity.wandering_trader.ambient",
                Some(SoundSource::Neutral),
                &settings,
                &SilenceProfile::NONE
            )
            .is_some()
        );
        assert!(
            silence_reason(
                "minecraft:entity.llama.ambient",
                Some(SoundSource::Neutral),
                &settings,
                &SilenceProfile::NONE
            )
            .is_some()
        );
        // Explosion is off by default.
        assert!(
            silence_reason(
                "minecraft:entity.generic.explode",
                Some(SoundSource::Blocks),
                &settings,
                &SilenceProfile::NONE
            )
            .is_none()
        );
    }

    #[test]
    fn explosion_toggle_and_custom_list_work() {
        let mut settings = SilenceSettings {
            silence_wither: false,
            silence_trader: false,
            silence_dragon: false,
            silence_lightning: false,
            silence_explosion: true,
            debug_mode: false,
            silence_us: vec![
                "entity.sheep.step".to_owned(),
                "othermod:custom.alarm".to_owned(),
            ],
        };
        assert!(
            silence_reason(
                "minecraft:entity.tnt.primed",
                Some(SoundSource::Blocks),
                &settings,
                &SilenceProfile::NONE
            )
            .is_some()
        );
        assert!(
            silence_reason(
                "minecraft:entity.sheep.step",
                Some(SoundSource::Neutral),
                &settings,
                &SilenceProfile::NONE
            )
            .is_some()
        );
        assert!(
            silence_reason(
                "othermod:custom.alarm",
                Some(SoundSource::Blocks),
                &settings,
                &SilenceProfile::NONE
            )
            .is_some()
        );
        assert!(
            silence_reason(
                "minecraft:custom.alarm",
                Some(SoundSource::Blocks),
                &settings,
                &SilenceProfile::NONE
            )
            .is_none()
        );
        settings.silence_explosion = false;
        assert!(
            silence_reason(
                "minecraft:entity.tnt.primed",
                Some(SoundSource::Blocks),
                &settings,
                &SilenceProfile::NONE
            )
            .is_none()
        );
    }

    #[test]
    fn silence_profiles_match_general_mob_group_and_all() {
        let off = SilenceSettings {
            silence_wither: false,
            silence_trader: false,
            silence_dragon: false,
            silence_lightning: false,
            silence_explosion: false,
            debug_mode: false,
            silence_us: Vec::new(),
        };

        let general = SilenceProfile::from_enchantment_ids(["bwncr:silence"]);
        assert!(
            silence_reason(
                "minecraft:entity.cow.ambient",
                Some(SoundSource::Neutral),
                &off,
                &general
            )
            .is_some()
        );
        assert!(
            silence_reason(
                "minecraft:block.stone.break",
                Some(SoundSource::Blocks),
                &off,
                &general
            )
            .is_none()
        );

        let mob_and_group =
            SilenceProfile::from_enchantment_ids(["bwncr:silence_creeper", "bwncr:silence_lava"]);
        assert!(
            silence_reason(
                "minecraft:entity.creeper.hurt",
                Some(SoundSource::Hostile),
                &off,
                &mob_and_group
            )
            .is_some()
        );
        assert!(
            silence_reason(
                "minecraft:item.bucket.fill_lava",
                Some(SoundSource::Blocks),
                &off,
                &mob_and_group
            )
            .is_some()
        );
        // Exact `$` suffix must not match longer paths.
        assert!(
            silence_reason(
                "minecraft:item.bucket.fill_lava_extra",
                Some(SoundSource::Blocks),
                &off,
                &mob_and_group
            )
            .is_none()
        );

        let all = SilenceProfile::from_enchantment_ids(["bwncr:silence_all"]);
        assert!(
            silence_reason("minecraft:music.menu", Some(SoundSource::Music), &off, &all).is_some()
        );
    }

    #[test]
    fn tooltip_flags_classify_all_four_kinds() {
        let flags = TooltipFlags::from_enchantment_ids([
            "bwncr:silence",
            "bwncr:silence_all",
            "bwncr:silence_stone",
            "bwncr:silence_creeper",
        ]);
        assert!(flags.general);
        assert!(flags.everything);
        assert!(flags.group);
        assert!(flags.mob);
    }
}
