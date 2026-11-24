//! SafeSpawn for Sunrise: a player joining a world for the first time while standing on a blocked
//! block (water, lava, magma, fire, wool, leaves) is moved, with their spawn point, to the nearest
//! safe surface within 128 blocks, as SafeSpawn 1.0.0 (Tokimi, CozyStudios) does.
//!
//! Once per player, marked in the player's saved data: if the block below their feet is blocked,
//! square rings around them are walked, radius 1 to 128, loading the chunks they cross, and the
//! first column whose `MOTION_BLOCKING` surface stands on a safe, solid full block with room for a
//! player above is taken; the player is teleported there and their spawn point set to it.
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use mod_api::{
    BlockPos, BlockShapeKind, ConfigPath, GlobalPos, Heightmap, Hooks, Host, HostSide, LevelRef,
    PlayerLoggedInEvent, PlayerRef, PlayerRespawnConfig, PlayerTickPostEvent, ServerHost, Tag,
};

pub const MAX_SEARCH_RADIUS: i32 = 128;
/// The key in the player's saved data that marks them as checked.
pub const SPAWNED_KEY: &str = "safespawn:spawned";
const CONFIG: &str = "safespawn/blocked_blocks.txt";
/// The blocks a player may not spawn standing on: ids and `#tags`.
pub const DEFAULT_BLOCKED: &[&str] = &[
    "minecraft:water",
    "minecraft:lava",
    "minecraft:magma_block",
    "#minecraft:fire",
    "#minecraft:wool",
    "#c:fluids",
    "#minecraft:leaves",
];

/// The blocked list: ids and tag names (without `#`).
#[derive(Debug, Default, PartialEq)]
pub struct Blocked {
    pub ids: Vec<String>,
    pub tags: Vec<String>,
}

/// Reads the list: one id or `#tag` per line; blank lines and lines starting with `;` are skipped.
pub fn parse_blocked(text: &str) -> Blocked {
    let mut out = Blocked::default();
    for line in text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with(';'))
    {
        match line.strip_prefix('#') {
            Some(tag) => out.tags.push(tag.to_owned()),
            None => out.ids.push(line.to_owned()),
        }
    }
    out
}

/// The list in `config/safespawn/blocked_blocks.txt`, written with the defaults when missing; read once.
fn blocked(host: &dyn Host) -> &'static Blocked {
    static BLOCKED: OnceLock<Blocked> = OnceLock::new();
    BLOCKED.get_or_init(|| {
        let files = host.config_files();
        let Ok(path) = ConfigPath::new(CONFIG) else {
            return parse_blocked(&DEFAULT_BLOCKED.join("\n"));
        };
        match files.read(&path) {
            Ok(bytes) => parse_blocked(&String::from_utf8_lossy(&bytes)),
            Err(_) => {
                let text = format!(
                    "; Blocks a new player may not spawn standing on.\n; One entry per line: a block id or a #tag.\n{}\n",
                    DEFAULT_BLOCKED.join("\n")
                );
                let _ = files.write(&path, text.as_bytes());
                parse_blocked(&text)
            }
        }
    })
}

/// The ring of columns at `radius` around `center`, in the original's order (x outer, z inner).
pub fn ring(center: [i32; 2], radius: i32) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for x in -radius..=radius {
        for z in -radius..=radius {
            if x.abs() == radius || z.abs() == radius {
                out.push((center[0] + x, center[1] + z));
            }
        }
    }
    out
}

/// What the check needs to know about the blocks around a surface position.
pub struct Column {
    pub below_blocked: bool,
    pub below_full_cube: bool,
    pub feet_empty: bool,
    pub head_empty: bool,
}

/// `SpawnValidator.isValidSpawnPosition`.
pub fn valid(c: &Column) -> bool {
    !c.below_blocked && c.below_full_cube && c.feet_empty && c.head_empty
}

fn full_cube(boxes: &[[f64; 6]]) -> bool {
    boxes == [[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]]
}

fn is_blocked(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> bool {
    let Some(block) = level.block(host, pos) else {
        return false;
    };
    let list = blocked(host);
    let id = block.id(host);
    list.ids.iter().any(|b| Some(b.as_str()) == id)
        || list
            .tags
            .iter()
            .any(|t| host.block_in_tag(block, t) == Some(true))
}

fn collision(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> Vec<[f64; 6]> {
    level
        .block_state(host, pos)
        .and_then(|state| level.block_shape(host, state, pos, BlockShapeKind::Collision))
        .unwrap_or_default()
}

fn find_safe(
    host: &mut dyn ServerHost,
    level: LevelRef,
    dimension: &str,
    origin: [i32; 3],
) -> Option<[i32; 3]> {
    let mut loaded = HashSet::new();
    for radius in 1..=MAX_SEARCH_RADIUS {
        let cols = ring([origin[0], origin[2]], radius);
        for &(x, z) in &cols {
            let chunk = [x >> 4, z >> 4];
            if loaded.insert(chunk) {
                let _ = level.load_chunk(host, chunk);
            }
        }
        let heights = host
            .surface_heights(dimension, &cols, Heightmap::MotionBlocking)
            .ok()?;
        let host: &dyn Host = &*host;
        for (&(x, z), h) in cols.iter().zip(heights) {
            let Some(y) = h else { continue };
            let c = Column {
                below_blocked: is_blocked(host, level, [x, y - 1, z]),
                below_full_cube: full_cube(&collision(host, level, [x, y - 1, z])),
                feet_empty: collision(host, level, [x, y, z]).is_empty(),
                head_empty: collision(host, level, [x, y + 1, z]).is_empty(),
            };
            if valid(&c) {
                return Some([x, y, z]);
            }
        }
    }
    None
}

/// The first-join check for `player`, with their level in place.
fn check(host: &mut dyn ServerHost, player: PlayerRef) {
    let key = SPAWNED_KEY.into();
    let seen = player
        .persistent_data(&*host)
        .is_some_and(|data| data.get(&key).is_some());
    if seen
        || player
            .set_persistent_data_key(host, &key, Some(&Tag::Byte(1)))
            .is_err()
    {
        return;
    }
    let (Some(level), Some(dim), Some(p)) = (
        player.level(&*host),
        player.dimension(&*host),
        player.entity().position(&*host),
    ) else {
        return;
    };
    let feet = [
        p[0].floor() as i32,
        p[1].floor() as i32,
        p[2].floor() as i32,
    ];
    if !is_blocked(&*host, level, [feet[0], feet[1] - 1, feet[2]]) {
        return;
    }
    let Some([x, y, z]) = find_safe(host, level, &dim.to_string(), feet) else {
        eprintln!("[safespawn] no safe spawn within {MAX_SEARCH_RADIUS} blocks of {feet:?}");
        return;
    };
    // The spawn point moves only once the teleport worked.
    if player.entity().teleport_to(
        host,
        [f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5],
        None,
    ) {
        let position = GlobalPos::new(dim, BlockPos::new(x, y, z));
        player.set_respawn_config(
            host,
            Some(PlayerRespawnConfig {
                position,
                yaw: 0.0,
                forced: true,
            }),
        );
    }
}

#[derive(Default)]
struct Mod {
    /// Players who joined and have not ticked yet.
    joined: Mutex<HashSet<u128>>,
}

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn player_logged_in(&self, _host: &mut dyn ServerHost, event: &mut PlayerLoggedInEvent<'_>) {
        self.joined
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(event.player);
    }

    fn player_tick_post(&self, host: HostSide<'_>, event: &mut PlayerTickPostEvent) {
        let HostSide::Server(host) = host else { return };
        let player = event.player;
        {
            let mut joined = self.joined.lock().unwrap_or_else(|e| e.into_inner());
            if joined.is_empty() || !joined.remove(&player.uuid()) {
                return;
            }
        }
        check(host, player);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_reads_ids_and_tags() {
        let list = parse_blocked("; comment\nminecraft:water\n\n #minecraft:wool \n");
        assert_eq!(
            list,
            Blocked {
                ids: vec!["minecraft:water".into()],
                tags: vec!["minecraft:wool".into()]
            }
        );
        let defaults = parse_blocked(&DEFAULT_BLOCKED.join("\n"));
        assert_eq!((defaults.ids.len(), defaults.tags.len()), (3, 4));
    }

    #[test]
    fn rings_are_the_square_edge() {
        assert_eq!(ring([0, 0], 1).len(), 8);
        assert_eq!(ring([0, 0], 2).len(), 16);
        assert_eq!(ring([5, 5], 1)[0], (4, 4));
    }

    #[test]
    fn valid_needs_safe_solid_ground_and_room() {
        let ok = || Column {
            below_blocked: false,
            below_full_cube: true,
            feet_empty: true,
            head_empty: true,
        };
        assert!(valid(&ok()));
        assert!(!valid(&Column {
            below_blocked: true,
            ..ok()
        }));
        assert!(!valid(&Column {
            below_full_cube: false,
            ..ok()
        }));
        assert!(!valid(&Column {
            head_empty: false,
            ..ok()
        }));
    }
}
