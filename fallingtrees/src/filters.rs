//! The editable filters (`TreeConfig.Filter`): which blocks are a tree's stems, its leaves, a cactus-like
//! column, and which items count as the felling tool. Each is a text file in `config/fallingtrees/`, written
//! with the original's defaults when missing. A line is a block or item id (`*` stands for any run of
//! characters), a `#tag`, or `!id` to refuse one that would
//! otherwise match; a line starting with `;` is a comment. A name passes unless refused, when it is listed or
//! in a listed tag.

use std::sync::OnceLock;

use mod_api::{ConfigPath, Host};

#[derive(Debug, Default, PartialEq)]
pub struct Filter {
    tags: Vec<String>,
    allow: Vec<String>,
    deny: Vec<String>,
}

/// `*` stands for any run of characters.
fn wildcard(name: &str, pattern: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    let [first, .., last] = parts.as_slice() else {
        return name == pattern;
    };
    if !name.starts_with(first) || name.len() < first.len() + last.len() || !name.ends_with(last) {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for mid in &parts[1..parts.len() - 1] {
        match rest.find(mid) {
            Some(i) => rest = &rest[i + mid.len()..],
            None => return false,
        }
    }
    true
}

impl Filter {
    pub fn parse(text: &str) -> Self {
        let mut filter = Self::default();
        for line in text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with(';'))
        {
            if let Some(tag) = line.strip_prefix('#') {
                filter.tags.push(tag.to_owned());
            } else if let Some(id) = line.strip_prefix('!') {
                filter.deny.push(id.to_owned());
            } else {
                filter.allow.push(line.to_owned());
            }
        }
        filter
    }

    /// `Filter.isValid`: not refused, and listed or in a listed tag.
    pub fn valid(&self, name: &str, in_tag: &dyn Fn(&str) -> bool) -> bool {
        !self.deny.iter().any(|p| wildcard(name, p))
            && (self.allow.iter().any(|p| wildcard(name, p)) || self.tags.iter().any(|t| in_tag(t)))
    }
}

/// The four filters.
pub struct Filters {
    pub logs: Filter,
    pub leaves: Filter,
    pub vertical: Filter,
    pub tools: Filter,
}

fn load(host: &dyn Host, file: &str, defaults: &str, help: &str) -> Filter {
    let Ok(path) = ConfigPath::new(&format!("fallingtrees/{file}")) else {
        return Filter::parse(defaults);
    };
    let files = host.config_files();
    match files.read(&path) {
        Ok(bytes) => Filter::parse(&String::from_utf8_lossy(&bytes)),
        Err(_) => {
            let _ = files.write(&path, format!("; {help}\n; An id (with * for any run of characters), a #tag, or !id to refuse one.\n{defaults}").as_bytes());
            Filter::parse(defaults)
        }
    }
}

impl Filters {
    /// The filters, read once per run.
    pub fn get(host: &dyn Host) -> &'static Filters {
        static FILTERS: OnceLock<Filters> = OnceLock::new();
        FILTERS.get_or_init(|| Filters {
            logs: load(
                host,
                "logs.txt",
                "#minecraft:logs\n",
                "Blocks that are a tree's stems.",
            ),
            leaves: load(
                host,
                "leaves.txt",
                "#minecraft:leaves\n",
                "Blocks that are a tree's leaves.",
            ),
            vertical: load(
                host,
                "vertical_blocks.txt",
                "minecraft:cactus\nminecraft:bamboo\n",
                "Blocks that fall as a column, from the chopped one up.",
            ),
            tools: load(
                host,
                "tools.txt",
                "#minecraft:axes\n",
                "Items that count as the tool (for the kinds that require one).",
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_filter_lists_ids_tags_and_refusals() {
        let f = Filter::parse("; note\n#minecraft:logs\nmymod:*_stem\n!mymod:bad_stem\n");
        let tag = |t: &str| t == "minecraft:logs";
        assert!(f.valid("mymod:good_stem", &|_| false));
        assert!(!f.valid("mymod:bad_stem", &|_| false));
        assert!(f.valid("minecraft:oak_log", &tag));
        assert!(!f.valid("minecraft:oak_log", &|_| false));
        assert!(!f.valid("minecraft:stone", &|_| false));
        // A refusal wins over a tag.
        assert!(!Filter::parse("#a:b\n!x:y\n").valid("x:y", &|_| true));
    }

    #[test]
    fn the_default_tool_filter_takes_axes_not_pickaxes() {
        let f = Filter::parse("#minecraft:axes\n");
        assert!(f.valid("minecraft:iron_axe", &|t| t == "minecraft:axes"));
        assert!(!f.valid("minecraft:iron_pickaxe", &|_| false));
    }
}
