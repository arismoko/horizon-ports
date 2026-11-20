//! hearth_and_home: Hearth & Home 2.0.3 (Starfish Studios; MIT, kept LAN-only with the pack for now). One
//! package: data/ and assets/ are the content mod mods/hearth_and_home (tools/hearth_and_home_port.py, fixed for
//! this build by tools/next_content_fix.py). The block rules that only read and write the level (placement,
//! shape updates, fire odds) are src/runtime.rs, a game module reached by the mixins in horizon-mod.toml.
//!
//! What stays here, in the mod crate (the mod API, as native-next/sootychimneys does it):
//! - the chimneys' block entity type (`chimney`): every lit chimney top smokes as `ChimneyBlock.animateTick`
//!   does on the client (campfire smoke, smoke, now and then a lava spark and the campfire crackle), sent from
//!   the server tick as `particle` / `playsound` commands;
//! - `right_click_block`: `ChimneyBlock.use` on a chimney top, a shovel puts it out (fire extinguish, the
//!   shovel wears) and a creeper igniter (flint and steel, fire charge) lights it (the charge is used up, the
//!   flint and steel wears; neither in creative).
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use mod_api::{
    BlockEntityEvent, BlockEntityPart, BlockEntityTickEvent, BlockEntityTicksEvent, BlockPart, Compound,
    EquipmentSlot, Flow, GameMode, Hand, Host, HostSide, Hooks, Identifier, InteractionResult, ItemStack, LevelRef,
    LevelSide, LivingRef, Registrar, RegistrationContext, RightClickBlockEvent, ServerHost, ServerTickEvent,
    SoundSource,
};

const NS: &str = "hearth_and_home:";

/// HNHBlocks' 23 ChimneyBlocks.
const CHIMNEYS: [&str; 23] = [
    "cobblestone", "mossy_cobblestone", "stone_brick", "mossy_stone_brick", "granite", "diorite", "andesite",
    "cobbled_deepslate", "polished_deepslate", "deepslate_brick", "deepslate_tile", "brick", "mud_brick", "sandstone",
    "red_sandstone", "prismarine", "nether_brick", "red_nether_brick", "blackstone", "polished_blackstone",
    "polished_blackstone_brick", "end_stone_brick", "cobblestone_brick",
];

/// How often a block is picked by `ClientLevel.animateTick` in a client tick for a player close by: 667 tries
/// in a 32-block cube and 667 in a 64-block cube.
const ANIMATE_CHANCE: f64 = 667.0 / 32768.0 + 667.0 / 262_144.0;

pub fn is_chimney(block: &str) -> bool {
    block.strip_prefix(NS).and_then(|b| b.strip_suffix("_chimney")).is_some_and(|k| CHIMNEYS.contains(&k))
}

struct State {
    rng: u64,
    queue: Vec<String>,
}

impl State {
    fn next(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn float(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
    fn double(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn below(&mut self, n: u32) -> u32 {
        if n == 0 { 0 } else { (self.next() % u64::from(n)) as u32 }
    }
    fn sign(&mut self) -> f64 {
        if self.below(2) == 0 { 1.0 } else { -1.0 }
    }
}

static STATE: Mutex<State> = Mutex::new(State { rng: 0, queue: Vec::new() });

fn lock() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

struct Mod;

impl Default for Mod {
    fn default() -> Self {
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0x9E37_79B9, |d| d.as_nanos() as u64);
        lock().rng = seed | 1;
        Mod
    }
}

fn block_id(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> String {
    level.block(host, pos).map(|k| k.id(host)).unwrap_or_default()
}

/// The chimney block entity: its server tick sends the smoke.
struct Chimney;

#[mod_api::horizon_part]
impl BlockPart for Chimney {}

#[mod_api::horizon_part]
impl BlockEntityPart for Chimney {
    fn block_entity(&self, _level: LevelSide, _event: &mut BlockEntityEvent<'_>) -> Option<Compound> {
        Some(Compound::new())
    }

    fn block_entity_ticks(&self, _host: &dyn Host, event: &mut BlockEntityTicksEvent<'_>) {
        if is_chimney(event.block) {
            event.ticks = true;
        }
    }

    /// `ChimneyBlock.animateTick`, at the rate a client close by would pick the block.
    fn block_entity_tick(&self, host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) {
        let pos = event.pos;
        let dim = host.dimension();
        let Some(level) = host.level(dim) else { return };
        if !is_chimney(&block_id(&*host, level, pos)) {
            return;
        }
        let prop = |k: &str| level.block_property(&*host, pos, k).is_some_and(|v| v == "true");
        if !(prop("lit") && prop("top")) {
            return;
        }
        let mut st = lock();
        if st.double() >= ANIMATE_CHANCE {
            return;
        }
        let (x, y, z) = (f64::from(pos[0]) + 0.5, f64::from(pos[1]) + 0.5, f64::from(pos[2]) + 0.5);
        if st.below(10) == 0 {
            let volume = 0.5 + st.float();
            let pitch = st.float() * 0.7 + 0.6;
            st.queue.push(format!(
                "execute in {dim} run playsound minecraft:block.campfire.crackle block @a {x:.3} {y:.3} {z:.3} {volume:.3} {pitch:.3}"
            ));
        }
        if st.below(5) == 0 {
            let (vx, vz) = (st.float() / 2.0, st.float() / 2.0);
            st.queue.push(format!("execute in {dim} run particle minecraft:lava {x:.3} {y:.3} {z:.3} {vx:.4} 0.00005 {vz:.4} 1 0 force"));
        }
        let cx = x + st.double() / 3.0 * st.sign();
        let cy = f64::from(pos[1]) + st.double() + st.double();
        let cz = z + st.double() / 3.0 * st.sign();
        st.queue.push(format!("execute in {dim} run particle minecraft:campfire_cosy_smoke {cx:.3} {cy:.3} {cz:.3} 0 0.07 0 1 0 force"));
        let sx = x + st.double() / 4.0 * st.sign();
        let sz = z + st.double() / 4.0 * st.sign();
        let sy = f64::from(pos[1]) + 0.4;
        st.queue.push(format!("execute in {dim} run particle minecraft:smoke {sx:.3} {sy:.3} {sz:.3} 0 0.005 0 1 0 force"));
    }
}

fn in_item_tag(host: &dyn Host, stack: &ItemStack, tag: &str) -> bool {
    let Ok(tag) = Identifier::parse(tag) else { return false };
    let tags = host.tags();
    tags.key(&tag).and_then(|key| tags.is_in_tag(stack.item(), key)).unwrap_or(false)
}

#[derive(Debug, PartialEq)]
enum Use {
    PutOut,
    Light,
}

/// What `ChimneyBlock.use` does with this click: only a chimney top takes one.
fn chimney_use(host: &dyn Host, level: LevelRef, pos: [i32; 3], stack: Option<&ItemStack>) -> Option<Use> {
    if !is_chimney(&block_id(host, level, pos)) {
        return None;
    }
    let prop = |k: &str| level.block_property(host, pos, k).is_some_and(|v| v == "true");
    if !prop("top") {
        return None;
    }
    let stack = stack?;
    let lit = prop("lit");
    if lit && in_item_tag(host, stack, "minecraft:shovels") {
        Some(Use::PutOut)
    } else if !lit && in_item_tag(host, stack, "minecraft:creeper_igniters") {
        Some(Use::Light)
    } else {
        None
    }
}

fn use_chimney(host: &mut dyn ServerHost, event: &RightClickBlockEvent<'_>) -> bool {
    let click = event.click;
    let dim = click.dimension.key().id();
    let Some(level) = host.level(&dim) else { return false };
    let at = click.pos;
    let Some(what) = chimney_use(&*host, level, at, click.stack) else { return false };
    let Some(stack) = click.stack.cloned() else { return false };
    let lit = matches!(what, Use::Light);
    if !host.set_block_property(at, "lit", if lit { "true" } else { "false" }) {
        return false;
    }
    let center = at.map(|c| f64::from(c) + 0.5);
    let sound = if lit { "minecraft:item.firecharge.use" } else { "minecraft:block.fire.extinguish" };
    level.play_sound(host, sound, center, SoundSource::Blocks, 1.0, 1.0);
    if let Some(player) = host.player(click.player) {
        if player.game_mode(&*host) != Some(GameMode::Creative) {
            // Utils.hurtStack: a damageable stack wears, any other is used up.
            if stack.item().id(&*host) == "minecraft:fire_charge" {
                let slot = match click.hand {
                    Hand::Main => player.selected_slot(&*host).map(|s| s as u32),
                    Hand::Off => Some(40),
                };
                if let Some(slot) = slot {
                    let _ = player.set_inventory_slot(host, slot, stack.clone().shrink(1));
                }
            } else {
                let slot = if matches!(click.hand, Hand::Main) { EquipmentSlot::MainHand } else { EquipmentSlot::OffHand };
                LivingRef::new(player.entity()).hurt_and_break(host, slot, 1);
            }
        }
    }
    true
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        let blocks: Vec<String> = CHIMNEYS.iter().map(|k| format!("{NS}{k}_chimney")).collect();
        let names: Vec<&str> = blocks.iter().map(String::as_str).collect();
        let kind = r.block_entity_type("chimney").blocks(&names).build();
        if let Err(e) = r.block_entity_part(&kind, Chimney) {
            eprintln!("[hearth_and_home] block entity part refused: {e:?}");
        }
    }

    fn right_click_block(&self, host: HostSide<'_>, event: &mut RightClickBlockEvent<'_>) -> Flow {
        let taken = match host {
            HostSide::Server(host) => host.on("hearth_and_home") && use_chimney(host, event),
            HostSide::Client(host) => {
                let host: &dyn Host = &*host;
                let click = event.click;
                host.level(&click.dimension.key().id()).is_some_and(|level| chimney_use(host, level, click.pos, click.stack).is_some())
            }
        };
        if !taken {
            return Flow::Continue;
        }
        *event.cancellation_result = InteractionResult::Success;
        event.cancelled = true;
        Flow::Cancel
    }

    fn server_tick(&self, host: &mut dyn ServerHost, _event: &mut ServerTickEvent<'_>) {
        let pending = std::mem::take(&mut lock().queue);
        for line in pending {
            if !host.commands().run_command(&line) {
                eprintln!("[hearth_and_home] command failed: {line}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chimneys() {
        assert!(is_chimney("hearth_and_home:brick_chimney"));
        assert!(is_chimney("hearth_and_home:cobblestone_brick_chimney"));
        assert!(!is_chimney("hearth_and_home:oak_lattice"));
        assert!(!is_chimney("sootychimneys:brick_chimney"));
    }
}
