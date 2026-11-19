//! Safe Spawn for Horizon: a player who joins a world for the first time standing on water,
//! lava, magma, fire, wool or leaves is moved, and their spawn point with them, to the nearest
//! safe surface within 128 blocks, as Safe Spawn 1.0.0 (Cozy Studios) does with Homestead's
//! config (`blocked_blocks` water, lava, magma block; `blocked_tags` `#minecraft:fire`,
//! `#minecraft:wool`, `#c:fluids`, `#minecraft:leaves`).
//!
//! The Java mod, on join, once per player (a flag in the player's NBT): if the block below the
//! player is blocked, it walks square rings around them, radius 1 to 128, and takes the first
//! column whose `MOTION_BLOCKING` surface stands on a safe, solid full block with room for a
//! player above; then it teleports them there and sets their spawn point.
//!
//! Here the flag is a list of UUIDs in `<world>/data/safespawn/spawned.txt`. The first time a
//! world is opened with this mod, everyone who already has player data goes on it, so an old
//! world's players aren't moved. `player_tick_post` checks a player once per session; the move runs
//! on the next server tick (`tp` and `spawnpoint`). Tags are matched by name (no block tag API).
//! Differences: only loaded columns are searched (the Java mod loads chunks for it), and
//! "solid full block" is "collision is one full cube".
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

use mod_api::{
    BlockShapeKind, Heightmap, HostSide, Hooks, LevelRef, PlayerTickPostEvent, ServerHost, ServerTickEvent,
    ServerStartedEvent, ServerTickPhase,
};

pub const MAX_SEARCH_RADIUS: i32 = 128;
pub const SPAWNED_FILE: &str = "data/safespawn/spawned.txt";

/// Whether a player may not spawn standing on `block`.
pub fn is_blocked(block: &str) -> bool {
    matches!(
        block,
        "minecraft:water" | "minecraft:lava" | "minecraft:magma_block" | "minecraft:fire" | "minecraft:soul_fire"
    ) || block.ends_with("_wool")
        || block.ends_with("_leaves")
}

/// A UUID as Minecraft writes it.
pub fn uuid_string(u: u128) -> String {
    let h = format!("{u:032x}");
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// The ring of columns at `radius` around `center`, in the Java mod's order (x outer, z inner).
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
pub struct Column<'a> {
    pub below: &'a str,
    pub below_full_cube: bool,
    pub feet_empty: bool,
    pub head_empty: bool,
}

/// `SpawnValidator.isValidSpawnPosition`.
pub fn valid(c: &Column) -> bool {
    !is_blocked(c.below) && c.below_full_cube && c.feet_empty && c.head_empty
}

#[derive(Default)]
struct State {
    file: Option<PathBuf>,
    spawned: HashSet<String>,
    /// Checked this session.
    checked: HashSet<u128>,
    queue: Vec<Vec<String>>,
}

#[derive(Default)]
struct Mod {
    state: Mutex<State>,
}

fn full_cube(boxes: &[[f64; 6]]) -> bool {
    boxes.len() == 1 && boxes[0] == [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]
}

fn block_id(host: &dyn ServerHost, level: LevelRef, pos: [i32; 3]) -> String {
    level.block(host, pos).map(|k| k.id(host)).unwrap_or_default()
}

fn collision(host: &dyn ServerHost, level: LevelRef, pos: [i32; 3]) -> Vec<[f64; 6]> {
    level
        .block_state(host, pos)
        .and_then(|state| level.block_shape(host, state, pos, BlockShapeKind::Collision))
        .unwrap_or_default()
}

fn find_safe(host: &dyn ServerHost, level: LevelRef, dimension: &str, origin: [i32; 3]) -> Option<[i32; 3]> {
    for radius in 1..=MAX_SEARCH_RADIUS {
        let cols = ring([origin[0], origin[2]], radius);
        let Ok(heights) = host.surface_heights(dimension, &cols, Heightmap::MotionBlocking) else {
            return None;
        };
        for (&(x, z), h) in cols.iter().zip(heights) {
            let Some(y) = h else { continue };
            let below = block_id(host, level, [x, y - 1, z]);
            let c = Column {
                below: &below,
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

fn save(state: &State) {
    if let Some(path) = &state.file {
        let mut list: Vec<&String> = state.spawned.iter().collect();
        list.sort();
        let text: String = list.iter().map(|u| format!("{u}\n")).collect();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(path, text) {
            eprintln!("[safespawn] can't write {}: {e}", path.display());
        }
    }
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn server_started(&self, host: &mut dyn ServerHost, _event: &mut ServerStartedEvent) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        *state = State::default();
        let Some(dir) = host.world_dir() else { return };
        let path = dir.join(SPAWNED_FILE);
        match std::fs::read_to_string(&path) {
            Ok(text) => state.spawned = text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect(),
            Err(_) => {
                // First open with this mod: everyone with player data has spawned before.
                if let Ok(entries) = std::fs::read_dir(dir.join("players/data")) {
                    for e in entries.flatten() {
                        let name = e.file_name().to_string_lossy().into_owned();
                        if let Some(uuid) = name.strip_suffix(".dat") {
                            state.spawned.insert(uuid.to_string());
                        }
                    }
                }
                state.file = Some(path);
                save(&state);
                return;
            }
        }
        state.file = Some(path);
    }

    fn player_tick_post(&self, host: HostSide<'_>, event: &mut PlayerTickPostEvent) {
        let HostSide::Server(host) = host else { return };
        if !host.on("safespawn") {
            return;
        }
        let player = event.player;
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.file.is_none() || state.checked.contains(&player.uuid()) {
            return;
        }
        let (Some(level), Some(dim), Some(name), Some(p)) = (
            player.level(&*host),
            player.dimension(&*host),
            player.name(&*host),
            player.entity().position(&*host),
        ) else {
            return;
        };
        state.checked.insert(player.uuid());
        let id = uuid_string(player.uuid());
        if state.spawned.contains(&id) {
            return;
        }
        let dim = dim.to_string();
        let feet = [p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32];
        let below = block_id(&*host, level, [feet[0], feet[1] - 1, feet[2]]);
        if is_blocked(&below) {
            match find_safe(&*host, level, &dim, feet) {
                Some([x, y, z]) => {
                    eprintln!("[safespawn] moving {name} from {below} at {feet:?} to {x},{y},{z}");
                    // The spawn point moves only once the teleport worked.
                    state.queue.push(vec![
                        format!("execute in {dim} run tp {name} {x}.5 {y} {z}.5"),
                        format!("execute in {dim} run spawnpoint {name} {x} {y} {z}"),
                    ]);
                }
                None => eprintln!("[safespawn] no safe spawn near {feet:?} for {name}"),
            }
        }
        state.spawned.insert(id);
        save(&state);
    }

    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        if !matches!(event.phase, ServerTickPhase::ServerTickEnd { .. }) {
            return;
        }
        let queue = std::mem::take(&mut self.state.lock().unwrap_or_else(|e| e.into_inner()).queue);
        for steps in queue {
            run_in_order(&steps, &mut |line| host.commands().run_command(line));
        }
    }
}

/// Runs `lines` in order and stops at the first one that fails, so a later step never runs
/// without the one before it. True when every line ran.
pub fn run_in_order(lines: &[String], run: &mut dyn FnMut(&str) -> bool) -> bool {
    for line in lines {
        if !run(line) {
            eprintln!("[{}] command failed, the rest skipped: {line}", "safespawn");
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_step_stops_the_rest() {
        let lines = vec!["first".to_string(), "second".to_string()];
        let mut ran = Vec::new();
        assert!(!run_in_order(&lines, &mut |l| {
            ran.push(l.to_string());
            false
        }));
        assert_eq!(ran, ["first"]);
        ran.clear();
        assert!(run_in_order(&lines, &mut |l| {
            ran.push(l.to_string());
            true
        }));
        assert_eq!(ran, ["first", "second"]);
    }


    #[test]
    fn blocked_blocks() {
        for b in ["minecraft:water", "minecraft:lava", "minecraft:magma_block", "minecraft:white_wool", "minecraft:oak_leaves", "minecraft:soul_fire"] {
            assert!(is_blocked(b), "{b}");
        }
        for b in ["minecraft:grass_block", "minecraft:sand", "minecraft:white_carpet"] {
            assert!(!is_blocked(b), "{b}");
        }
    }

    #[test]
    fn uuids_print_as_minecraft_does() {
        assert_eq!(uuid_string(0xa01e3843e5213998958af459800e4d11), "a01e3843-e521-3998-958a-f459800e4d11");
    }

    #[test]
    fn rings_are_the_square_edge() {
        assert_eq!(ring([0, 0], 1).len(), 8);
        assert_eq!(ring([0, 0], 2).len(), 16);
        assert_eq!(ring([5, 5], 1)[0], (4, 4));
    }

    #[test]
    fn valid_needs_safe_solid_ground_and_room() {
        let ok = Column { below: "minecraft:stone", below_full_cube: true, feet_empty: true, head_empty: true };
        assert!(valid(&ok));
        assert!(!valid(&Column { below: "minecraft:magma_block", ..ok }));
        assert!(!valid(&Column { below_full_cube: false, ..ok }));
        assert!(!valid(&Column { head_empty: false, ..ok }));
    }
}
