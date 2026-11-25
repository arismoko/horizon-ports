use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

const CATALOG_JSON: &str = include_str!("bwncr_mobs.json");

/// Mob and environment-group Silence enchantment catalog, parsed once from the embedded
/// `bwncr_mobs.json` shipped alongside the data-driven enchantments and recipes.
pub struct Catalog {
    mobs_by_enchantment: HashMap<String, Vec<String>>,
    groups_by_enchantment: HashMap<String, Vec<String>>,
}

static CATALOG: OnceLock<Catalog> = OnceLock::new();

pub fn catalog() -> &'static Catalog {
    CATALOG.get_or_init(|| Catalog::parse(CATALOG_JSON))
}

impl Catalog {
    pub fn parse(raw: &str) -> Self {
        let mut mobs_by_enchantment = HashMap::new();
        let mut groups_by_enchantment = HashMap::new();

        let Ok(root) = serde_json::from_str::<Value>(raw) else {
            return Self {
                mobs_by_enchantment,
                groups_by_enchantment,
            };
        };

        if let Some(mobs) = root.get("mobs").and_then(Value::as_array) {
            for entry in mobs {
                let Some(id) = entry.get("id").and_then(Value::as_str) else {
                    continue;
                };
                let sound = entry.get("sound").and_then(Value::as_str).unwrap_or(id);
                let mut prefixes =
                    vec![format!("entity.{sound}."), format!("entity.baby_{sound}.")];
                if let Some(extra) = entry.get("extra").and_then(Value::as_array) {
                    for prefix in extra.iter().filter_map(Value::as_str) {
                        prefixes.push(prefix.to_owned());
                    }
                }
                mobs_by_enchantment.insert(format!("silence_{id}"), prefixes);
            }
        }

        if let Some(groups) = root.get("groups").and_then(Value::as_array) {
            for entry in groups {
                let Some(id) = entry.get("id").and_then(Value::as_str) else {
                    continue;
                };
                let prefixes: Vec<String> = entry
                    .get("prefixes")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect();
                groups_by_enchantment.insert(format!("silence_{id}"), prefixes);
            }
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
    fn invalid_json_returns_empty_catalog_without_panicking() {
        let cat = Catalog::parse("{not valid json");
        assert_eq!(cat.mob_count(), 0);
        assert_eq!(cat.group_count(), 0);
    }
}
