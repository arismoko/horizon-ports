//! Sooty Chimneys 1.3.4 (mortuusars): each chimney's block entity (ChimneyBlockEntity) keeps its
//! states, dirties, and smokes, and a click on a chimney (ChimneyBlock.use) toggles `blocked`
//! (empty main hand) or scrapes a dirty one clean with an axe. Homestead's common config:
//! SmokeFlow 0.75, DirtyChance 0.05, Wind true, WindStrength 1.0. The smoke is the client's
//! particleTick on Java; here the server sends it (`particle ... force`), with one wind for the
//! server (Wind.update, ticked by the server instead of each client).
//!
//! The chimneys are this package's data/ and assets/, made from mods/sootychimneys by tools/build_next.py. `register` declares the block entity type
//! `sootychimneys:chimney` on its fourteen blocks and attaches the `Chimney` part (the old port
//! gave them a block entity from the global `block_entity` hooks).
//! The click is the `right_click_block` hook (the old port patched
//! `native_hooks::Game::native_right_click_block` with a mixin and a game-side module). The
//! dirty-to-clean swap on scraping is a `setblock` keeping `lit`/`blocked`/`stacked`.
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use mod_api::{
    BlockEntityEvent, BlockEntityPart, BlockEntityTickEvent, BlockEntityTicksEvent, BlockPart, Compound,
    EquipmentSlot, Flow, Hand, Host, HostSide, Hooks, Identifier, InteractionResult, ItemStack, LevelRef,
    LevelSide, LivingRef, Registrar, RegistrationContext, RightClickBlockEvent, ServerHost, ServerTickEvent,
    SoundSource, Text,
};

const KINDS: [&str; 7] = ["brick", "cobblestone", "stone_brick", "mud_brick", "iron", "copper", "terracotta"];
const SMOKE_FLOW: f64 = 0.75;
const DIRTY_CHANCE: f64 = 0.05;
/// A block's chance to be picked by a random tick in a tick: randomTickSpeed 3 of 4096.
const RANDOM_TICK: f64 = 3.0 / 4096.0;
const WIND_STRENGTH: f32 = 1.0;

/// ChimneyTypes' SmokeProperties: origin (x, y, z), spread (x, y, z), intensity, speed.
struct Smoke {
    origin: [f64; 3],
    spread: [f64; 3],
    intensity: f32,
    speed: f64,
}

fn smoke_of(kind: &str) -> Option<Smoke> {
    let s = |origin, spread, intensity, speed| Some(Smoke { origin, spread, intensity, speed });
    match kind {
        "brick" => s([0.5, 1.25, 0.5], [0.25, 0.1, 0.25], 1.0, 1.2),
        "cobblestone" => s([0.5, 1.25, 0.5], [0.15, 0.1, 0.15], 1.0, 1.0),
        "stone_brick" => s([0.5, 1.2, 0.5], [0.025, 0.05, 0.025], 0.65, 1.0),
        "mud_brick" => s([0.5, 1.25, 0.5], [0.15, 0.1, 0.15], 1.0, 1.1),
        "iron" => s([0.5, 1.2, 0.5], [0.05, 0.05, 0.05], 0.65, 1.35),
        "copper" => s([0.5, 1.3, 0.5], [0.025, 0.05, 0.025], 0.55, 1.2),
        "terracotta" => s([0.5, 0.8, 0.5], [0.02, 0.05, 0.02], 0.3, 0.65),
        _ => None,
    }
}

/// `sootychimneys:<dirty_>?<kind>_chimney` → (kind, dirty).
pub fn parse(block: &str) -> Option<(&str, bool)> {
    let rest = block.strip_prefix("sootychimneys:")?.strip_suffix("_chimney")?;
    Some(match rest.strip_prefix("dirty_") {
        Some(kind) => (kind, true),
        None => (rest, false),
    })
}

struct State {
    rng: u64,
    queue: Vec<String>,
    wind_angle: f64,
    wind_strength: f32,
    last_tick: Option<i64>,
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
    /// RandomOffset.offset: within (-range, range).
    fn offset(&mut self, v: f64, range: f64) -> f64 {
        if range <= 0.0 { v } else { v + (self.double() * 2.0 - 1.0) * range }
    }
}

/// TimeOfDay.fromSunAngle of the day time: 0 morning, 1 day, 2 evening, 3 night.
fn time_of_day(day_time: i64) -> u8 {
    // DimensionType.timeOfDay → the sun angle in degrees.
    let d = (day_time.rem_euclid(24000) as f64) / 24000.0 - 0.25;
    let f = d - d.floor();
    let g = 0.5 - (f * std::f64::consts::PI).cos() / 2.0;
    let degrees = (f * 2.0 + g) / 3.0 * 360.0;
    if degrees <= 290.0 && degrees > 270.0 {
        0
    } else if degrees <= 270.0 && degrees > 95.0 {
        3
    } else if degrees <= 95.0 && degrees > 75.0 {
        2
    } else {
        1
    }
}

/// Wind.getWindState: (min, max) strength for the weather, the time of day and the day.
fn wind_state(day_time: i64, rain: f32, thunder: f32) -> (f32, f32) {
    const CALM: (f32, f32) = (0.0, 0.05);
    const BREEZE: (f32, f32) = (0.05, 0.15);
    const WINDY: (f32, f32) = (0.15, 0.5);
    const STORMY: (f32, f32) = (0.5, 1.0);
    let day = day_time / 24000;
    let tod = time_of_day(day_time);
    if thunder > 0.0 {
        STORMY
    } else if rain > 0.0 {
        if tod != 2 && day % 3 > 0 { WINDY } else { BREEZE }
    } else if tod == 1 {
        if day % 5 > 2 { BREEZE } else { CALM }
    } else if day % 4 == 0 {
        BREEZE
    } else {
        CALM
    }
}

struct Mod;

static STATE: Mutex<State> = Mutex::new(State { rng: 0, queue: Vec::new(), wind_angle: 0.0, wind_strength: 0.0, last_tick: None });

impl Default for Mod {
    fn default() -> Self {
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0x9E37_79B9, |d| d.as_nanos() as u64);
        STATE.lock().unwrap_or_else(|e| e.into_inner()).rng = seed | 1;
        Mod
    }
}

fn lock() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

fn block_id(host: &dyn Host, level: LevelRef, pos: [i32; 3]) -> String {
    level.block(host, pos).map(|k| k.id(host)).unwrap_or_default()
}

/// The chimney block entity: its server tick.
struct Chimney;

#[mod_api::horizon_part]
impl BlockPart for Chimney {}

#[mod_api::horizon_part]
impl BlockEntityPart for Chimney {
    fn block_entity(&self, _level: LevelSide, _event: &mut BlockEntityEvent<'_>) -> Option<Compound> {
        Some(Compound::new())
    }

    fn block_entity_ticks(&self, _host: &dyn Host, event: &mut BlockEntityTicksEvent<'_>) {
        if parse(event.block).is_some() {
            event.ticks = true;
        }
    }

    fn block_entity_tick(&self, host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) {
        let pos = event.pos;
        let dim = host.dimension();
        let Some(level) = host.level(dim) else { return };
        let block = block_id(&*host, level, pos);
        let Some((kind, dirty)) = parse(&block) else { return };
        let Some(smoke) = smoke_of(kind) else { return };
        let [x, y, z] = pos;
        let above = block_id(&*host, level, [x, y + 1, z]);
        let below = block_id(&*host, level, [x, y - 1, z]);
        let prop = |k: &str| level.block_property(&*host, pos, k).is_some_and(|v| v == "true");
        let (lit, blocked, stacked) = (prop("lit"), prop("blocked"), prop("stacked"));
        // ChimneyBlock.updateState: lit without a redstone signal, stacked under a chimney.
        let want_lit = !level.has_neighbor_signal(&*host, pos);
        let want_stacked = parse(&above).is_some();
        let below_stacked = parse(&below).is_some() && level.block_property(&*host, [x, y - 1, z], "stacked").is_some_and(|v| v == "true");
        let mut st = lock();
        if dirty && want_stacked {
            // A stacked chimney is never dirty.
            let clean = format!("sootychimneys:{kind}_chimney");
            st.queue.push(format!(
                "execute in {dim} run setblock {x} {y} {z} {clean}[lit={want_lit},blocked={blocked},stacked=true]"
            ));
            st.queue.push(format!(
                "execute in {dim} run particle minecraft:smoke {} {} {} 0.2 0.2 0.2 0.02 6 force",
                f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5
            ));
            return;
        }
        drop(st);
        if lit != want_lit {
            host.set_block_property(pos, "lit", if want_lit { "true" } else { "false" });
        }
        if stacked != want_stacked {
            host.set_block_property(pos, "stacked", if want_stacked { "true" } else { "false" });
        }
        // shouldEmitSmoke (no block is in sootychimneys:smoke_blocking).
        if !want_lit || blocked || want_stacked {
            return;
        }
        let mut st = lock();
        // randomTick: a clean chimney that smokes gets dirty.
        if !dirty && st.double() < RANDOM_TICK * DIRTY_CHANCE {
            st.queue.push(format!(
                "execute in {dim} run setblock {x} {y} {z} sootychimneys:dirty_{kind}_chimney[lit=true,blocked=false,stacked=false]"
            ));
        }
        // ChimneyBlockEntity.particleTick and ChimneyBlock.emitParticle.
        if st.double() >= SMOKE_FLOW || st.float() > smoke.intensity {
            return;
        }
        let particle = if below_stacked { "minecraft:campfire_signal_smoke" } else { "minecraft:campfire_cosy_smoke" };
        let strength = st.wind_strength * 0.1 * WIND_STRENGTH;
        let rad = st.wind_angle.to_radians();
        let mut vx = rad.cos() * f64::from(strength);
        let vy = 0.05 * smoke.speed;
        let mut vz = rad.sin() * f64::from(strength);
        vx += f64::from((st.float() * strength - strength / 2.0) * 0.2);
        vz += f64::from((st.float() * strength - strength / 2.0) * 0.2);
        let max = (4.0 * smoke.intensity.max(0.5)) as u32;
        let (cx, cy, cz) = (
            f64::from(x) + smoke.origin[0],
            f64::from(y) + smoke.origin[1],
            f64::from(z) + smoke.origin[2],
        );
        for _ in 0..st.below(max) {
            let px = st.offset(cx, smoke.spread[0]);
            let py = st.offset(cy, smoke.spread[1]);
            let pz = st.offset(cz, smoke.spread[2]);
            st.queue.push(format!(
                "execute in {dim} run particle {particle} {px:.3} {py:.3} {pz:.3} {vx:.4} {vy:.4} {vz:.4} 1 0 force"
            ));
        }
    }
}

/// SootScrapingRecipe results (data/sootychimneys/recipe/soot_scraping): black dye at a chance.
fn scraping_chance(dirty: &str) -> f32 {
    match dirty.strip_prefix("sootychimneys:dirty_").unwrap_or("") {
        "iron_chimney" | "copper_chimney" => 0.5,
        _ => 0.75,
    }
}

fn is_axe(host: &dyn Host, stack: &ItemStack) -> bool {
    let Ok(tag) = Identifier::parse("minecraft:axes") else { return false };
    let tags = host.tags();
    tags.key(&tag).and_then(|key| tags.is_in_tag(stack.item(), key)).unwrap_or(false)
}

/// What a click on a chimney does.
#[derive(Debug, PartialEq)]
enum Use {
    Toggle,
    Scrape,
}

fn chimney_use(host: &dyn Host, block: &str, hand: Hand, stack: Option<&ItemStack>) -> Option<Use> {
    let (_, dirty) = parse(block)?;
    match stack {
        None if matches!(hand, Hand::Main) => Some(Use::Toggle),
        Some(s) if dirty && is_axe(host, s) => Some(Use::Scrape),
        _ => None,
    }
}

/// ChimneyBlock.use on the server: true when the click was the chimney's.
fn use_chimney(host: &mut dyn ServerHost, event: &RightClickBlockEvent<'_>) -> bool {
    let click = event.click;
    let dim = click.dimension.key().id();
    let Some(level) = host.level(&dim) else { return false };
    let at = click.pos;
    let name = block_id(&*host, level, at);
    let Some(what) = chimney_use(&*host, &name, click.hand, click.stack) else { return false };
    let center = at.map(|c| f64::from(c) + 0.5);
    let player = host.player(click.player);
    match what {
        Use::Toggle => {
            // The smoke blocked or let through, with the iron trapdoor's sound and a line over
            // the hotbar.
            let blocked = level.block_property(&*host, at, "blocked").is_some_and(|v| v == "true");
            if !host.set_block_property(at, "blocked", if blocked { "false" } else { "true" }) {
                return false;
            }
            let pitch = 0.85 + lock().float() * 0.05;
            let sound = if blocked { "minecraft:block.iron_trapdoor.open" } else { "minecraft:block.iron_trapdoor.close" };
            level.play_sound(host, sound, center, SoundSource::Blocks, 0.8, pitch);
            if let Some(player) = player {
                let key = if blocked { "message.sootychimneys.open" } else { "message.sootychimneys.blocked" };
                let _ = player.action_bar(host, Text::translate(key, vec![]));
            }
            true
        }
        Use::Scrape => {
            let Some(tool) = click.stack.cloned() else { return false };
            let Some(clean) = name.strip_prefix("sootychimneys:dirty_") else { return false };
            let prop = |k: &str| level.block_property(&*host, at, k).unwrap_or_else(|| "false".into());
            let (lit, blocked, stacked) = (prop("lit"), prop("blocked"), prop("stacked"));
            let [x, y, z] = at;
            let line = format!(
                "execute in {dim} run setblock {x} {y} {z} sootychimneys:{clean}[lit={lit},blocked={blocked},stacked={stacked}]"
            );
            if !host.commands().run_command(&line) {
                eprintln!("[sootychimneys] command failed: {line}");
                return false;
            }
            if lock().float() < scraping_chance(&name) {
                let n = click.hit.face.map(f64::from);
                let spot = [center[0] + n[0] * 0.65, f64::from(y) + 0.6 + n[1] * 0.65, center[2] + n[2] * 0.65];
                let dye = Identifier::parse("minecraft:black_dye").ok().and_then(|id| host.item_key(&id));
                if let Some(stack) = dye.and_then(|k| ItemStack::new(k, 1)) {
                    let _ = level.spawn_item_with_motion(host, spot[0], spot[1], spot[2], stack, [0.0, 0.1, 0.0], 10);
                }
            }
            if let Some(player) = player {
                let slot = if matches!(click.hand, Hand::Main) { EquipmentSlot::MainHand } else { EquipmentSlot::OffHand };
                LivingRef::new(player.entity()).hurt_and_break(host, slot, 1);
                let _ = player.trigger_item_used_on_block(host, at, &tool);
            }
            level.play_sound(host, "minecraft:item.axe.scrape", center, SoundSource::Blocks, 1.0, 1.0);
            true
        }
    }
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        let blocks: Vec<String> = KINDS
            .iter()
            .flat_map(|k| [format!("{k}_chimney"), format!("dirty_{k}_chimney")])
            .collect();
        // The chimneys are this package's own content (data/), so they always exist.
        let names: Vec<&str> = blocks.iter().map(String::as_str).collect();
        let kind = r.block_entity_type("chimney").blocks(&names).build();
        if let Err(e) = r.block_entity_part(&kind, Chimney) {
            eprintln!("[sootychimneys] block entity part refused: {e:?}");
        }
    }

    fn right_click_block(&self, host: HostSide<'_>, event: &mut RightClickBlockEvent<'_>) -> Flow {
        let taken = match host {
            HostSide::Server(host) => host.on("sootychimneys") && use_chimney(host, event),
            HostSide::Client(host) => {
                let host: &dyn Host = &*host;
                let click = event.click;
                host.level(&click.dimension.key().id()).is_some_and(|level| {
                    chimney_use(host, &block_id(host, level, click.pos), click.hand, click.stack).is_some()
                })
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
        let Some(overworld) = host.level("minecraft:overworld") else { return };
        let day_time = overworld.day_time(&*host).unwrap_or(0);
        let (rain, thunder) = overworld.weather(&*host).unwrap_or((0.0, 0.0));
        let pending = {
            let mut st = lock();
            // Wind.update once a game tick (the server ticks twice here: start and end).
            if st.last_tick != Some(day_time) {
                st.last_tick = Some(day_time);
                let (min, max) = wind_state(day_time, rain, thunder);
                let add = f64::from(max) * (f64::from(st.below(2)) - 1.0);
                let target = min + st.float() * (max - min);
                st.wind_strength += 0.1 * (target - st.wind_strength);
                st.wind_angle = (st.wind_angle + add) % 360.0;
            }
            std::mem::take(&mut st.queue)
        };
        for line in pending {
            if !host.commands().run_command(&line) {
                eprintln!("[sootychimneys] command failed: {line}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(parse("sootychimneys:dirty_mud_brick_chimney"), Some(("mud_brick", true)));
        assert_eq!(parse("sootychimneys:iron_chimney"), Some(("iron", false)));
        assert_eq!(parse("minecraft:stone"), None);
        assert!(smoke_of("terracotta").is_some());
    }

    #[test]
    fn times_of_day() {
        assert_eq!(time_of_day(6000), 1);
        assert_eq!(time_of_day(18000), 3);
        assert_eq!(wind_state(6000, 0.0, 1.0), (0.5, 1.0));
    }
}
