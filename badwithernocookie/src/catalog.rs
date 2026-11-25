use crate::catalog_data::{GROUPS, GroupEntry, MOBS, MobEntry};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Mob and environment-group Silence enchantment catalog, built once from the static
/// Rust tables matching the data-driven enchantments and recipes.
pub struct Catalog {
    mobs_by_enchantment: HashMap<String, Vec<String>>,
    groups_by_enchantment: HashMap<String, Vec<String>>,
}

static CATALOG: OnceLock<Catalog> = OnceLock::new();

pub fn catalog() -> &'static Catalog {
    CATALOG.get_or_init(|| Catalog::from_entries(MOBS, GROUPS))
}

impl Catalog {
    pub fn from_entries(mobs: &[MobEntry], groups: &[GroupEntry]) -> Self {
        let mut mobs_by_enchantment = HashMap::with_capacity(mobs.len());
        for entry in mobs {
            let sound = entry.sound.unwrap_or(entry.id);
            let mut prefixes = Vec::with_capacity(2 + entry.extra.len());
            prefixes.push(format!("entity.{sound}."));
            prefixes.push(format!("entity.baby_{sound}."));
            for extra in entry.extra {
                prefixes.push((*extra).to_owned());
            }
            mobs_by_enchantment.insert(format!("silence_{}", entry.id), prefixes);
        }

        let mut groups_by_enchantment = HashMap::with_capacity(groups.len());
        for entry in groups {
            let prefixes = entry.prefixes.iter().map(|p| (*p).to_owned()).collect();
            groups_by_enchantment.insert(format!("silence_{}", entry.id), prefixes);
        }

        Self {
            mobs_by_enchantment,
            groups_by_enchantment,
        }
    }

    pub fn prefixes_for_enchantment(&self, path: &str) -> Option<&[String]> {
        self.mobs_by_enchantment
            .get(path)
            .or_else(|| self.groups_by_enchantment.get(path))
            .map(Vec::as_slice)
    }

    pub fn is_group_enchantment(&self, path: &str) -> bool {
        self.groups_by_enchantment.contains_key(path)
    }

    pub fn is_mob_enchantment(&self, path: &str) -> bool {
        self.mobs_by_enchantment.contains_key(path)
    }

    #[cfg(test)]
    pub fn mob_count(&self) -> usize {
        self.mobs_by_enchantment.len()
    }

    #[cfg(test)]
    pub fn group_count(&self) -> usize {
        self.groups_by_enchantment.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_loads_all_88_mobs_and_37_groups() {
        let cat = catalog();
        assert_eq!(cat.mob_count(), 88);
        assert_eq!(cat.group_count(), 37);

        let creeper = cat
            .prefixes_for_enchantment("silence_creeper")
            .expect("creeper entry");
        assert!(creeper.iter().any(|p| p == "entity.creeper."));
        assert!(creeper.iter().any(|p| p == "entity.baby_creeper."));

        assert!(cat.is_mob_enchantment("silence_creeper"));
        assert!(!cat.is_group_enchantment("silence_creeper"));

        let cave = cat
            .prefixes_for_enchantment("silence_cave_ambience")
            .expect("cave_ambience group");
        assert_eq!(cave, &["ambient.cave".to_owned()]);
        assert!(cat.is_group_enchantment("silence_cave_ambience"));
        assert!(!cat.is_mob_enchantment("silence_cave_ambience"));
    }

    #[test]
    fn custom_sound_and_extra_prefixes_are_preserved() {
        let cat = catalog();

        let puffer = cat
            .prefixes_for_enchantment("silence_pufferfish")
            .expect("pufferfish entry");
        assert!(puffer.iter().any(|p| p == "entity.puffer_fish."));
        assert!(puffer.iter().any(|p| p == "entity.baby_puffer_fish."));

        let cave_spider = cat
            .prefixes_for_enchantment("silence_cave_spider")
            .expect("cave_spider entry");
        assert!(cave_spider.iter().any(|p| p == "entity.cave_spider."));
        assert!(cave_spider.iter().any(|p| p == "entity.spider."));
    }
}
