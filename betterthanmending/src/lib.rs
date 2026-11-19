//! Better Than Mending for Horizon: sneak and use a damaged item with Mending to repair it with
//! your experience, as Better Than Mending 1.3.0 (legobmw99) does: 20 points of experience for
//! 40 durability when you have at least 30 points and the item lacks 40, else 2 points for 4.
//! Without 2 points the item's own use goes ahead.
//!
//! `right_click_item` reads the player's sneaking (`is_shift_key_down`) and experience (level and
//! progress) straight from the player, and decides on both sides: the client consumes the click
//! so it doesn't start the item's own use, the server queues the repair for the server tick:
//! `item modify` sets the hand's `minecraft:damage`, `experience add` takes the points.
//! (The old port, without those reads, probed experience with scoreboard commands each tick a
//! player sneaked and tracked sneaking from `movement_input`/`player_toggle_sneak`.)
use std::sync::Mutex;

use mod_api::{
    Compound, Flow, Hand, HostSide, Hooks, Host, InteractionResult, ItemStack, PlayerRef, RightClickItemEvent,
    ServerHost, ServerTickEvent, ServerTickPhase, Tag,
};

/// What the player's experience allows, from the last probe.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Xp {
    pub at_least_2: bool,
    pub at_least_30: bool,
}

/// The repair for an item with `damage`, as the Java mod picks it: (points to take, new damage).
pub fn repair(xp: Xp, damage: u32) -> Option<(u32, u32)> {
    if damage == 0 {
        return None;
    }
    if xp.at_least_30 && damage >= 40 {
        return Some((20, damage - 40));
    }
    xp.at_least_2.then(|| (2, damage.saturating_sub(4)))
}

/// Total experience points of `level` full levels (vanilla `Player.getXpNeededForNextLevel` summed).
pub fn points_for_level(level: i32) -> i64 {
    let l = level.max(0) as i64;
    if l <= 16 {
        l * l + 6 * l
    } else if l <= 31 {
        (5 * l * l - 81 * l + 720) / 2
    } else {
        (9 * l * l - 325 * l + 4440) / 2
    }
}

/// What `level` and `points` into the next level allow.
pub fn xp_of(level: i32, points: i32) -> Xp {
    let total = points_for_level(level) + points.max(0) as i64;
    Xp { at_least_2: total >= 2, at_least_30: total >= 30 }
}

/// A queued repair.
#[derive(Debug, PartialEq)]
pub struct Repair {
    pub player: String,
    pub main_hand: bool,
    pub damage: u32,
    pub points: u32,
}

impl Repair {
    pub fn commands(&self) -> [String; 2] {
        let slot = if self.main_hand { "weapon.mainhand" } else { "weapon.offhand" };
        [
            format!(
                "item modify entity {} {slot} {{\"type\":\"minecraft:set_components\",\"components\":{{\"minecraft:damage\":{}}}}}",
                self.player, self.damage
            ),
            format!("experience add {} -{} points", self.player, self.points),
        ]
    }
}

#[derive(Default)]
struct Mod {
    queue: Mutex<Vec<Repair>>,
}

/// The Mending level in a stack's component patch (`minecraft:enchantments`, ids to levels).
fn mending_level(stack: &ItemStack) -> i32 {
    let Some(components) = stack.components() else { return 0 };
    let find = |c: &Compound, key: &str| c.iter().find(|(k, _)| k.to_string_lossy() == key).map(|(_, v)| v.clone());
    let Some(Tag::Compound(mut levels)) = find(components.as_nbt(), "minecraft:enchantments") else { return 0 };
    if let Some(Tag::Compound(inner)) = find(&levels, "levels") {
        levels = inner;
    }
    match find(&levels, "minecraft:mending") {
        Some(Tag::Int(n)) => n,
        Some(Tag::Byte(n)) => n as i32,
        Some(Tag::Short(n)) => n as i32,
        _ => 0,
    }
}

/// The repair this use makes, if any: (points, new damage).
fn decide(host: &dyn Host, player: PlayerRef, stack: &ItemStack) -> Option<(u32, u32)> {
    if stack.damage() == 0 || mending_level(stack) == 0 {
        return None;
    }
    if player.entity().is_shift_key_down(host) != Some(true) {
        return None;
    }
    let exp = player.experience(host)?;
    let needed = player.xp_needed_for_next_level(host).unwrap_or(0);
    let points = (exp.progress * needed as f32).round() as i32;
    repair(xp_of(exp.level, points), stack.damage())
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn right_click_item(&self, host: HostSide<'_>, event: &mut RightClickItemEvent<'_>) -> Flow {
        let click = event.click;
        let decided = match host {
            HostSide::Server(host) => {
                if !host.on("betterthanmending") {
                    return Flow::Continue;
                }
                let Some(player) = host.player(click.player) else { return Flow::Continue };
                let Some((points, damage)) = decide(&*host, player, click.stack) else { return Flow::Continue };
                let Some(name) = player.name(&*host) else { return Flow::Continue };
                let main_hand = matches!(click.hand, Hand::Main);
                self.queue.lock().unwrap_or_else(|e| e.into_inner()).push(Repair { player: name, main_hand, damage, points });
                true
            }
            HostSide::Client(host) => {
                let host: &dyn Host = &*host;
                host.player(click.player).is_some_and(|player| decide(host, player, click.stack).is_some())
            }
        };
        if !decided {
            return Flow::Continue;
        }
        *event.cancellation_result = InteractionResult::Consume;
        event.cancelled = true;
        Flow::Cancel
    }

    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        if !matches!(event.phase, ServerTickPhase::ServerTickEnd { .. }) || !host.on("betterthanmending") {
            return;
        }
        let queue = std::mem::take(&mut *self.queue.lock().unwrap_or_else(|e| e.into_inner()));
        // The repair, then the experience it costs: no experience is taken for a failed repair.
        for repair in queue {
            run_in_order(&repair.commands(), &mut |line| host.commands().run_command(line));
        }
    }
}

/// Runs `lines` in order and stops at the first one that fails, so a later step never runs
/// without the one before it. True when every line ran.
pub fn run_in_order(lines: &[String], run: &mut dyn FnMut(&str) -> bool) -> bool {
    for line in lines {
        if !run(line) {
            eprintln!("[{}] command failed, the rest skipped: {line}", "betterthanmending");
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


    const RICH: Xp = Xp { at_least_2: true, at_least_30: true };
    const POOR: Xp = Xp { at_least_2: true, at_least_30: false };

    #[test]
    fn repairs_as_the_java_mod() {
        assert_eq!(repair(RICH, 100), Some((20, 60)));
        assert_eq!(repair(RICH, 39), Some((2, 35)));
        assert_eq!(repair(POOR, 100), Some((2, 96)));
        assert_eq!(repair(POOR, 3), Some((2, 0)));
        assert_eq!(repair(Xp::default(), 100), None);
        assert_eq!(repair(RICH, 0), None);
    }

    #[test]
    fn repair_commands() {
        let r = Repair { player: "Steve".into(), main_hand: true, damage: 60, points: 20 };
        assert_eq!(
            r.commands(),
            [
                "item modify entity Steve weapon.mainhand {\"type\":\"minecraft:set_components\",\"components\":{\"minecraft:damage\":60}}".to_string(),
                "experience add Steve -20 points".to_string(),
            ]
        );
    }

    #[test]
    fn experience_totals() {
        assert_eq!(points_for_level(3), 27);
        assert_eq!(points_for_level(4), 40);
        assert_eq!(points_for_level(17), 394);
        assert_eq!(points_for_level(32), 1628);
        assert_eq!(xp_of(3, 3), Xp { at_least_2: true, at_least_30: true });
        assert_eq!(xp_of(0, 1), Xp::default());
        assert_eq!(xp_of(1, 0), Xp { at_least_2: true, at_least_30: false });
    }
}
