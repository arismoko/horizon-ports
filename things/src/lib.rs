//! things: Things 0.3.3 (glisco, MIT), what the trinkets and tools do. The items are this
//! package's data/ and assets/; the Bater Wucket is game-side (runtime.rs and its mixin).
//!
//! The trinkets count as worn in native-next/trinkets' slots (`trinkets:worn`, declared here with
//! the same key and type). Homestead's things.json5: mossNecklaceRegen 2, miningGloveMomentum 2,
//! riotGauntletStrength 1, sockPerLevelSpeedAmplifier 0.02, infernalScepterDurability 64 (the
//! content's max damage).
//!
//! Effects are commands run at the server tick's end, refreshed every 10 ticks with 2 s left
//! (Java gives 5 ticks every tick), so one lingers up to 2 s after the trinket comes off.
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use mod_api::{
    AnvilUpdateEvent, AttachmentCodec, AttachmentError, AttachmentType, AttributeModifier, AttributeOperation,
    ClientHost, Compound, Flow, GameMode, Hand, Holder, Host, HostSide, Hooks,
    IPayloadContext, IPayloadHandler, Identifier, InputKeyEvent, InteractionResult, ItemStack, KeyAction,
    KeyRegistration, LightLayer, LivingEntityUseItemFinishEvent, LivingIncomingDamageEvent, LivingRef,
    LevelRandomHost, ModifierLifetime, PayloadBytes, PayloadHandleError, PayloadId, PlayerRef,
    PlayerTickPostEvent, Registrar, RegistrationContext, RightClickBlockEvent,
    RightClickItemEvent, ServerHost, ServerTickEvent, ServerTickPhase, Tag, Text,
};

const REFRESH: u64 = 10;
const MOSS_REGEN: i32 = 2;
const MINING_MOMENTUM: i32 = 2;
const RIOT_STRENGTH: i32 = 1;
const MAGNET_COST: u32 = 50;
const MAGNET_CHARGE: u32 = 200;
const SCEPTER_USES: u32 = 64;
/// SockDataComponent: sockPerLevelSpeedAmplifier (Homestead: 0.02) per level, level + 1.
const SOCK_SPEED: f64 = 0.019999999552965164;
const TOGGLE: &str = "things:toggle_socks";

fn same(c: &Compound) -> Result<Compound, AttachmentError> {
    Ok(c.clone())
}
fn empty() -> Compound {
    Compound::new()
}

/// native-next/trinkets' worn trinkets of a player (slot id -> item NBT).
static TRINKETS: AttachmentType<Compound> = AttachmentType {
    namespace: "trinkets",
    name: "worn",
    default: empty,
    codec: Some(AttachmentCodec { encode: same, decode: same }),
    copy_on_death: true,
    sync: true,
};

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn id_of(host: &dyn Host, stack: &ItemStack) -> String {
    stack.item().id(host).to_string()
}

fn component<'a>(stack: &'a ItemStack, id: &str) -> Option<&'a Tag> {
    stack.components()?.as_nbt().get(&id.into())
}

fn custom_int(stack: &ItemStack, key: &str) -> Option<i32> {
    let Some(Tag::Compound(c)) = component(stack, "minecraft:custom_data") else { return None };
    match c.get(&key.into())? {
        Tag::Int(v) => Some(*v),
        Tag::Byte(v) => Some(i32::from(*v)),
        Tag::Short(v) => Some(i32::from(*v)),
        _ => None,
    }
}

fn set_custom(stack: &mut ItemStack, key: &str, value: Tag) {
    let mut c = match component(stack, "minecraft:custom_data") {
        Some(Tag::Compound(c)) => c.clone(),
        _ => Compound::new(),
    };
    c.insert(key.into(), value);
    let mut patch = Compound::new();
    patch.insert("minecraft:custom_data".into(), Tag::Compound(c));
    let _ = stack.apply_components(&patch);
}

/// Lore lines as the content writes them: gray, not italic.
fn set_lore(stack: &mut ItemStack, lines: &[String]) {
    let lore = Tag::List(
        lines
            .iter()
            .map(|l| {
                Tag::compound([("text", Tag::String(l.as_str().into())), ("color", Tag::String("gray".into())), ("italic", Tag::Byte(0))])
            })
            .collect(),
    );
    let mut patch = Compound::new();
    patch.insert("minecraft:lore".into(), lore);
    let _ = stack.apply_components(&patch);
}

/// The socks' tooltip (SocksItem.appendTooltip) as lore.
fn socks_lore(speed: i32, jumpy: bool, disabled: bool) -> Vec<String> {
    let mut lines = Vec::new();
    if jumpy {
        lines.push(format!("↑ Jumpy{}", if disabled { " (off: Caps Lock to toggle)" } else { " (Caps Lock to toggle)" }));
    }
    lines.push(match speed {
        0 => "☄ Average".to_owned(),
        1 => "☄ Speedy".to_owned(),
        2 => "☄ Usain Bolt".to_owned(),
        n => format!("☄ Naughty ({n})"),
    });
    lines.push("Wear it in a trinket slot (G).".to_owned());
    lines
}

/// What a player wears.
#[derive(Clone, Default, PartialEq)]
struct Worn {
    items: HashSet<String>,
    /// Socks: speed level, jumpy, jump boost turned off.
    socks: Option<(i32, bool, bool)>,
}

impl Worn {
    fn has(&self, item: &str) -> bool {
        self.items.contains(item)
    }
}

fn worn_stacks(host: &mut dyn Host, uuid: u128) -> Vec<(String, ItemStack)> {
    let c = TRINKETS.get(host, Holder::Player(uuid)).unwrap_or_default();
    c.iter()
        .filter_map(|(k, t)| match t {
            Tag::Compound(t) => Some((k.to_string_lossy(), ItemStack::from_nbt(&*host, t)?)),
            _ => None,
        })
        .collect()
}

fn worn(host: &mut dyn Host, uuid: u128) -> Worn {
    let mut out = Worn::default();
    for (_, stack) in worn_stacks(host, uuid) {
        let id = id_of(&*host, &stack);
        if id == "things:socks" {
            out.socks = Some((
                custom_int(&stack, "Speed").unwrap_or(0),
                custom_int(&stack, "Jumpy").unwrap_or(0) != 0,
                custom_int(&stack, "JumpBoostDisabled").unwrap_or(0) != 0,
            ));
        }
        out.items.insert(id);
    }
    out
}

fn effect(name: &str, effect: &str, seconds: i32, amplifier: i32) -> String {
    format!("effect give {name} {effect} {seconds} {amplifier} true")
}

fn passable(block: &str) -> bool {
    matches!(block, "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air" | "minecraft:water" | "minecraft:lava" | "minecraft:bubble_column")
}

/// A lockable container (LockableContainerBlockEntity's blocks).
fn lockable(block: &str) -> bool {
    let b = block.trim_start_matches("minecraft:");
    !b.contains("ender_chest")
        && (b.ends_with("chest")
            || b == "barrel"
            || b.ends_with("furnace")
            || b == "smoker"
            || b == "hopper"
            || b == "dispenser"
            || b == "dropper"
            || b.ends_with("shulker_box")
            || b == "brewing_stand"
            || b == "crafter")
}

fn pretty(block: &str) -> String {
    block
        .rsplit(':')
        .next()
        .unwrap_or(block)
        .split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Default)]
struct PlayerState {
    worn: Worn,
    ticks: u64,
    /// When the Mossy Necklace last gave Regeneration (its 610 ticks).
    regen_at: Option<u64>,
    /// The attribute amounts set last: socks speed, step height, reach, water efficiency.
    modifiers: Option<[f64; 4]>,
}

#[derive(Default)]
struct State {
    queue: Vec<String>,
    players: HashMap<u128, PlayerState>,
    /// Entity ids of players wearing the Shock Absorber.
    shock: HashSet<i32>,
    rng: u64,
    /// Container key uses waiting for the tick: player, key, position.
    keys: Vec<(u128, i32, [i32; 3])>,
}

impl State {
    fn float(&mut self) -> f64 {
        if self.rng == 0 {
            self.rng = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) | 1;
        }
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[derive(Default)]
struct Mod {
    state: Mutex<State>,
    toggles: Arc<Mutex<Vec<u128>>>,
}

struct ToggleHandler(Arc<Mutex<Vec<u128>>>);

impl IPayloadHandler for ToggleHandler {
    fn handle(&self, _payload: PayloadBytes, context: &mut dyn IPayloadContext) -> Result<(), PayloadHandleError> {
        if let Ok(p) = context.player() {
            lock(&self.0).push(p.uuid());
        }
        Ok(())
    }
}

/// The hand slot (inventory index) a click used.
fn hand_slot(host: &dyn Host, player: PlayerRef, hand: Hand) -> Option<u32> {
    match hand {
        Hand::Main => player.selected_slot(host).map(|s| s as u32),
        _ => Some(40),
    }
}

impl Mod {
    fn st(&self) -> std::sync::MutexGuard<'_, State> {
        lock(&self.state)
    }

    /// Each player's tick: the worn trinkets' effects, modifiers and Shock Absorber, and the
    /// carried Item Magnets' charge and new Container Keys' numbers (their inventoryTick).
    fn tick_player(&self, host: &mut dyn ServerHost, player: PlayerRef) {
        let Some(uuid) = Some(player.uuid()) else { return };
        let now = worn(host, uuid);
        let Some(name) = player.name(&*host) else { return };
        let entity = player.entity();
        let mode = player.game_mode(&*host);
        let at = entity.position(&*host).unwrap_or_default();
        let block = [at[0].floor() as i32, at[1].floor() as i32, at[2].floor() as i32];
        let level = player.level(&*host);
        let light = level
            .map(|l| (l.brightness(&*host, LightLayer::Block, block).unwrap_or(0), l.brightness(&*host, LightLayer::Sky, block).unwrap_or(0)))
            .unwrap_or((0, 0));
        let day = level.and_then(|l| l.day_time(&*host)).unwrap_or(0).rem_euclid(24000);
        let mut want = [0.0f64; 4];
        if let Some((speed, jumpy, _)) = now.socks {
            want[0] = SOCK_SPEED * f64::from(speed + 1);
            if jumpy {
                want[1] = 0.45;
            }
        }
        if now.has("things:arm_extender") {
            want[2] = 2.0;
        }
        if now.has("things:enchanted_wax_gland") {
            want[3] = 1.0;
        }
        let set_modifiers;
        {
            let mut st = self.st();
            let mut queue = Vec::new();
            let pl = st.players.entry(uuid).or_default();
            pl.ticks += 1;
            let before = std::mem::replace(&mut pl.worn, now.clone());
            // onUnequip: the effects Java takes away.
            if before.has("things:monocle") && !now.has("things:monocle") {
                queue.push(format!("effect clear {name} minecraft:night_vision"));
            }
            if before.has("things:moss_necklace") && !now.has("things:moss_necklace") {
                queue.push(format!("effect clear {name} minecraft:regeneration"));
                pl.regen_at = None;
            }
            if pl.ticks % REFRESH == 0 && mode != Some(GameMode::Spectator) {
                if now.has("things:monocle") {
                    queue.push(effect(&name, "minecraft:night_vision", 31, 0));
                }
                if now.has("things:mining_gloves") {
                    // The Momentum effect is Haste that stacks with a beacon's; Haste here.
                    queue.push(effect(&name, "minecraft:haste", 2, MINING_MOMENTUM - 1));
                }
                if now.has("things:riot_gauntlet") {
                    queue.push(effect(&name, "minecraft:strength", 2, RIOT_STRENGTH - 1));
                }
                if now.has("things:rabbit_foot_charm") || now.socks.is_some_and(|s| s.1 && !s.2) {
                    queue.push(effect(&name, "minecraft:jump_boost", 2, 1));
                }
                if now.has("things:hades_crystal") {
                    queue.push(effect(&name, "minecraft:fire_resistance", 2, 0));
                }
                if now.has("things:moss_necklace") {
                    // MossNecklaceItem.tick: block light over 7, or sky light over 7 by day;
                    // Regeneration for 610 ticks once the last one is nearly over.
                    let lit = light.0 > 7 || (light.1 > 7 && (day > 23500 || day < 12500));
                    let due = pl.regen_at.is_none_or(|t| pl.ticks - t >= 600);
                    if lit && due {
                        queue.push(effect(&name, "minecraft:regeneration", 30, MOSS_REGEN - 1));
                        pl.regen_at = Some(pl.ticks);
                    }
                }
            }
            set_modifiers = pl.modifiers != Some(want) || pl.ticks % 100 == 0;
            if set_modifiers {
                pl.modifiers = Some(want);
            }
            if now.has("things:shock_absorber") {
                st.shock.insert(entity.id() as i32);
            } else {
                st.shock.remove(&(entity.id() as i32));
            }
            st.queue.extend(queue);
        }
        if set_modifiers {
            let living = LivingRef::new(entity);
            let set = |host: &mut dyn ServerHost, attribute: &str, id: &str, v: f64| {
                let Ok(id) = Identifier::parse(id) else { return };
                if v == 0.0 {
                    living.remove_attribute_modifier(host, attribute, &id);
                } else {
                    living.replace_attribute_modifier(
                        host,
                        attribute,
                        AttributeModifier::new(id, v, AttributeOperation::AddValue),
                        ModifierLifetime::Transient,
                    );
                }
            };
            set(host, "minecraft:movement_speed", "things:socks", want[0]);
            set(host, "minecraft:step_height", "things:jumpy_socks", want[1]);
            set(host, "minecraft:block_interaction_range", "things:arm_extender", want[2]);
            set(host, "minecraft:entity_interaction_range", "things:arm_extender", want[2]);
            set(host, "minecraft:water_movement_efficiency", "things:wax_gland", want[3]);
        }
        self.tick_carried(host, player);
    }

    /// ItemMagnetItem.inventoryTick (charge + 1 + charge / 80, up to 200) and
    /// ContainerKeyItem.inventoryTick (a new key gets its number).
    fn tick_carried(&self, host: &mut dyn ServerHost, player: PlayerRef) {
        let Some(slots) = player.inventory_slots(&*host) else { return };
        for (slot, mut stack) in slots {
            match id_of(&*host, &stack).as_str() {
                "things:item_magnet" if stack.damage() > 0 => {
                    let charge = MAGNET_CHARGE - stack.damage().min(MAGNET_CHARGE);
                    let next = (charge + 1 + charge / 80).min(MAGNET_CHARGE);
                    stack.set_damage(MAGNET_CHARGE - next);
                }
                "things:container_key" if custom_int(&stack, "Lock").is_none() => {
                    let key = (self.st().float() * 200000.0) as i32;
                    set_custom(&mut stack, "Lock", Tag::Int(key));
                    set_lore(&mut stack, &[
                        format!("Key: #{key:x}"),
                        "Sneak-use on a container to lock or unlock it.".to_owned(),
                        "Hold the key to open a locked container.".to_owned(),
                    ]);
                }
                _ => continue,
            }
            let _ = player.set_inventory_slot(host, slot, Some(stack));
        }
    }

    /// ItemMagnetItem.use: along the view from 2 to 10 blocks, until a block, the items within
    /// 1.25 + charge / 200 * 2 of each point come to the player.
    fn use_magnet(&self, host: &mut dyn ServerHost, player: PlayerRef, slot: u32, mut stack: ItemStack) -> bool {
        let charge = MAGNET_CHARGE.saturating_sub(stack.damage());
        if charge < MAGNET_COST {
            return true;
        }
        let entity = player.entity();
        let (Some(name), Some(eye), Some(dir), Some(level), Some(dim)) = (
            player.name(&*host),
            entity.eye_position(&*host),
            entity.view_vector(&*host),
            player.level(&*host),
            player.dimension(&*host),
        ) else {
            return false;
        };
        let radius = 1.25 + f64::from(charge) / 200.0 * 2.0;
        let mut queue = Vec::new();
        let mut d = 2.0;
        while d < 10.0 {
            let at = [eye[0] + dir[0] * d, eye[1] + dir[1] * d, eye[2] + dir[2] * d];
            let block = level
                .block(&*host, [at[0].floor() as i32, at[1].floor() as i32, at[2].floor() as i32])
                .map(|b| b.id(&*host).to_string())
                .unwrap_or_default();
            if !passable(&block) {
                break;
            }
            // The same selector twice, in a row: only the items this point moves lose their pickup delay.
            let near = format!("execute in {dim} positioned {:.2} {:.2} {:.2} as @e[type=item,distance=..{radius:.2}] run", at[0], at[1], at[2]);
            queue.push(format!("{near} data merge entity @s {{PickupDelay:0s}}"));
            queue.push(format!("{near} tp @s {name}"));
            d += 1.0;
        }
        queue.push(format!("execute as {name} at @s run playsound minecraft:entity.enderman.teleport player {name} ~ ~ ~ 0.125 2"));
        stack.set_damage((stack.damage() + MAGNET_COST).min(MAGNET_CHARGE));
        let _ = player.set_inventory_slot(host, slot, Some(stack));
        self.st().queue.extend(queue);
        true
    }

    /// InfernalScepterItem: a fireball of power 3 two blocks ahead at eye height minus 1, flying
    /// along the view, for a fire charge (none in creative); 64 uses.
    fn use_scepter(&self, host: &mut dyn ServerHost, player: PlayerRef, slot: u32, mut stack: ItemStack) -> bool {
        let creative = player.game_mode(&*host) == Some(GameMode::Creative);
        let Some(slots) = player.inventory_slots(&*host) else { return false };
        let charge = slots.iter().find(|(_, s)| id_of(&*host, s) == "minecraft:fire_charge").cloned();
        if charge.is_none() && !creative {
            return false;
        }
        let entity = player.entity();
        let (Some(pos), Some(dir), Some(dim)) = (entity.position(&*host), entity.view_vector(&*host), player.dimension(&*host)) else {
            return false;
        };
        let at = [pos[0] + dir[0] * 2.0, pos[1] + 1.62 - 1.0, pos[2] + dir[2] * 2.0];
        let uuid = player.uuid();
        let owner = [(uuid >> 96) as i32, (uuid >> 64) as i32, (uuid >> 32) as i32, uuid as i32];
        let motion: Vec<String> = dir.iter().map(|v| format!("{:.5}d", v * 0.1)).collect();
        let mut st = self.st();
        st.queue.push(format!(
            "execute in {dim} run summon minecraft:fireball {:.3} {:.3} {:.3} {{ExplosionPower:3b,acceleration_power:0.1d,Motion:[{}],Owner:[I;{},{},{},{}]}}",
            at[0], at[1], at[2], motion.join(","), owner[0], owner[1], owner[2], owner[3]
        ));
        st.queue.push(format!("execute in {dim} run playsound minecraft:entity.ghast.shoot player @a {:.2} {:.2} {:.2} 1 1", pos[0], pos[1], pos[2]));
        drop(st);
        if !creative {
            if let Some((s, c)) = charge {
                let _ = player.set_inventory_slot(host, s, c.shrink(1));
            }
            stack.set_damage(stack.damage() + 1);
            let left = (stack.damage() < SCEPTER_USES).then_some(stack);
            let _ = player.set_inventory_slot(host, slot, left);
        }
        true
    }

    /// ThingsClient.TOGGLE_SOCKS_JUMP_BOOST's packet: the worn jumpy socks' jump boost on or off.
    fn toggle_socks(&self, host: &mut dyn ServerHost, uuid: u128) {
        let mut c = TRINKETS.get(host, Holder::Player(uuid)).unwrap_or_default();
        let Some((key, mut s)) = worn_stacks(host, uuid).into_iter().find(|(_, s)| id_of(&*host, s) == "things:socks") else {
            return;
        };
        let speed = custom_int(&s, "Speed").unwrap_or(0);
        if custom_int(&s, "Jumpy").unwrap_or(0) == 0 {
            return;
        }
        let off = custom_int(&s, "JumpBoostDisabled").unwrap_or(0) == 0;
        set_custom(&mut s, "JumpBoostDisabled", Tag::Byte(i8::from(off)));
        set_lore(&mut s, &socks_lore(speed, true, off));
        let Some(t) = s.to_nbt(&*host) else { return };
        c.insert(key.as_str().into(), Tag::Compound(t));
        let _ = TRINKETS.set(host, Holder::Player(uuid), c);
        if let Some(p) = host.player(uuid) {
            let _ = p.action_bar(host, Text::literal(if off { "Jump Boost off" } else { "Jump Boost on" }));
        }
    }

    /// ContainerKeyItem.useOnBlock, at the tick: lock an unlocked container (and its other half)
    /// with the key, unlock one locked with it, or say it has another key. The lock is the
    /// block entity's `lock` (any Container Key with this number in its custom data).
    fn lock_containers(&self, host: &mut dyn ServerHost, keys: Vec<(u128, i32, [i32; 3])>) {
        for (uuid, key, pos) in keys {
            let Some(player) = host.player(uuid) else { continue };
            let (Some(level), Some(dim)) = (player.level(&*host), player.dimension(&*host)) else { continue };
            let block = level.block(&*host, pos).map(|b| b.id(&*host).to_string()).unwrap_or_default();
            let mut spots = vec![pos];
            if let Some(o) = other_half(&*host, level, pos) {
                spots.push(o);
            }
            let cmd = |s: [i32; 3], rest: &str| format!("execute in {dim} run data {rest} block {} {} {}", s[0], s[1], s[2]);
            let locked_with = |host: &mut dyn ServerHost, s: [i32; 3], k: Option<i32>| {
                let path = match k {
                    Some(k) => format!("execute in {dim} if data block {} {} {} lock.predicates.\"minecraft:custom_data\"{{Lock:{k}}}", s[0], s[1], s[2]),
                    None => format!("execute in {dim} if data block {} {} {} lock", s[0], s[1], s[2]),
                };
                host.commands().run_command(&path)
            };
            let any = spots.iter().any(|s| locked_with(host, *s, None));
            let mine = spots.iter().any(|s| locked_with(host, *s, Some(key)));
            let line = if !any {
                for s in &spots {
                    host.commands().run_command(&format!(
                        "{} {{lock:{{items:\"things:container_key\",predicates:{{\"minecraft:custom_data\":{{Lock:{key}}}}}}}}}",
                        cmd(*s, "merge")
                    ));
                }
                format!("{} locked!", pretty(&block))
            } else if mine {
                for s in &spots {
                    host.commands().run_command(&format!("{} lock", cmd(*s, "remove")));
                }
                format!("{} unlocked!", pretty(&block))
            } else {
                format!("{} is locked with another key!", pretty(&block))
            };
            host.commands().run_command(&format!(
                "execute in {dim} run playsound minecraft:block.chest.locked block @a {} {} {} 1 1",
                pos[0], pos[1], pos[2]
            ));
            let _ = player.action_bar(host, Text::literal(line));
        }
    }
}

/// ContainerKeyItem.maybeGetOtherChest: a double chest's other half.
fn other_half(host: &dyn Host, level: mod_api::LevelRef, p: [i32; 3]) -> Option<[i32; 3]> {
    let kind = level.block_property(host, p, "type")?;
    let facing = level.block_property(host, p, "facing")?;
    // ChestBlock.getFacing: LEFT → facing clockwise, RIGHT → counterclockwise.
    let (cw, ccw) = match facing.as_str() {
        "north" => ([1, 0], [-1, 0]),
        "east" => ([0, 1], [0, -1]),
        "south" => ([-1, 0], [1, 0]),
        "west" => ([0, -1], [0, 1]),
        _ => return None,
    };
    let d = match kind.as_str() {
        "left" => cw,
        "right" => ccw,
        _ => return None,
    };
    Some([p[0] + d[0], p[1], p[2] + d[1]])
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, registrar: &mut dyn Registrar, _: &RegistrationContext) {
        registrar.register_key_mapping(
            KeyRegistration::new("toggle_socks_jump_boost", "key.keyboard.caps.lock").label("key.things.toggle_socks_jump_boost"),
        );
        let mut payloads = registrar.payload_handlers();
        payloads
            .registrar("1")
            .optional()
            .play_to_server(PayloadId(TOGGLE.into()), Arc::new(ToggleHandler(self.toggles.clone())));
    }

    /// The socks' key (Caps Lock unless the player chose another), while playing.
    fn input_key(&self, host: &mut dyn ClientHost, event: &mut InputKeyEvent<'_>) {
        if !matches!(event.action, KeyAction::Press) || !host.playing() {
            return;
        }
        let Some((key, _)) = host.mod_key("things", "toggle_socks_jump_boost") else { return };
        if event.key == key {
            let _ = host.send_payload_to_server(&PayloadId(TOGGLE.into()), PayloadBytes(Vec::new()));
        }
    }

    fn player_tick_post(&self, host: HostSide<'_>, event: &mut PlayerTickPostEvent) {
        if let HostSide::Server(host) = host {
            self.tick_player(host, event.player);
        }
    }

    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        if !matches!(event.phase, ServerTickPhase::ServerTickEnd { .. }) {
            return;
        }
        let toggles = std::mem::take(&mut *lock(&self.toggles));
        for uuid in toggles {
            self.toggle_socks(host, uuid);
        }
        let (pending, keys) = {
            let mut st = self.st();
            (std::mem::take(&mut st.queue), std::mem::take(&mut st.keys))
        };
        for line in pending {
            if !host.commands().run_command(&line) {
                eprintln!("[things] command failed: {line}");
            }
        }
        if !keys.is_empty() {
            self.lock_containers(host, keys);
        }
    }

    /// The Item Magnet and the Infernal Scepter in hand.
    fn right_click_item(&self, host: HostSide<'_>, event: &mut RightClickItemEvent<'_>) -> Flow {
        let click = event.click;
        let used = match host {
            HostSide::Server(host) => {
                let id = id_of(&*host, click.stack);
                if !matches!(id.as_str(), "things:item_magnet" | "things:infernal_scepter") {
                    return Flow::Continue;
                }
                let Some(player) = host.player(click.player) else { return Flow::Continue };
                let Some(slot) = hand_slot(&*host, player, click.hand) else { return Flow::Continue };
                if id == "things:item_magnet" {
                    self.use_magnet(host, player, slot, click.stack.clone())
                } else {
                    self.use_scepter(host, player, slot, click.stack.clone())
                }
            }
            HostSide::Client(host) => matches!(id_of(host, click.stack).as_str(), "things:item_magnet" | "things:infernal_scepter"),
        };
        if !used {
            return Flow::Continue;
        }
        *event.cancellation_result = InteractionResult::Success;
        event.cancelled = true;
        Flow::Cancel
    }

    /// ContainerKeyItem.useOnBlock: a sneaking use on a lockable container.
    fn right_click_block(&self, host: HostSide<'_>, event: &mut RightClickBlockEvent<'_>) -> Flow {
        let click = event.click;
        let HostSide::Server(host) = host else { return Flow::Continue };
        let Some(stack) = click.stack else { return Flow::Continue };
        if id_of(&*host, stack) != "things:container_key" {
            return Flow::Continue;
        }
        let Some(player) = host.player(click.player) else { return Flow::Continue };
        let Some(level) = player.level(&*host) else { return Flow::Continue };
        let block = level.block(&*host, click.pos).map(|b| b.id(&*host).to_string()).unwrap_or_default();
        if !lockable(&block) {
            return Flow::Continue;
        }
        if player.is_secondary_use_active(&*host) != Some(true) {
            // Opening: the game's lock check only asks for a Container Key (its predicate's
            // custom_data isn't tested), so a key with another number is refused here.
            let Some(dim) = player.dimension(&*host) else { return Flow::Continue };
            let p = click.pos;
            let at = format!("execute in {dim} if data block {} {} {} lock", p[0], p[1], p[2]);
            if !host.commands().run_command(&at) {
                return Flow::Continue;
            }
            let mine = custom_int(stack, "Lock").is_some_and(|k| {
                host.commands().run_command(&format!("{at}.predicates.\"minecraft:custom_data\"{{Lock:{k}}}"))
            });
            if mine {
                return Flow::Continue;
            }
            host.commands().run_command(&format!(
                "execute in {dim} run playsound minecraft:block.chest.locked block @a {} {} {} 1 1",
                p[0], p[1], p[2]
            ));
            let _ = player.action_bar(host, Text::literal(format!("{} is locked!", pretty(&block))));
            *event.cancellation_result = InteractionResult::Fail;
            event.cancelled = true;
            return Flow::Cancel;
        }
        let Some(key) = custom_int(stack, "Lock") else { return Flow::Continue };
        self.st().keys.push((click.player, key, click.pos));
        *event.cancellation_result = InteractionResult::Success;
        event.cancelled = true;
        Flow::Cancel
    }

    /// The Recall Potion (RecallPotionItem.finishUsing), the Placebo (PotionItemMixin) and the
    /// Luck of the Irish (PlayerEntityMixin).
    fn living_entity_use_item_finish(&self, host: HostSide<'_>, event: &mut LivingEntityUseItemFinishEvent<'_>) {
        let HostSide::Server(host) = host else { return };
        let Some(id) = event.entity else { return };
        let Some(player) = host.players().acting_player().filter(|p| p.entity().id() as i32 == id) else { return };
        let item = id_of(&*host, event.stack);
        let Some(name) = player.name(&*host) else { return };
        let creative = player.game_mode(&*host) == Some(GameMode::Creative);
        let worn = worn(host, player.uuid());
        match item.as_str() {
            "things:recall_potion" => recall(host, player),
            "minecraft:potion" if worn.has("things:placebo") && !creative => {
                if self.st().float() <= 0.75 {
                    *event.result = Some(event.stack.clone());
                }
            }
            "minecraft:poisonous_potato" if worn.has("things:luck_of_the_irish") => {
                // The 2 ticks of Saturation are left out: /effect counts whole seconds.
                let mut st = self.st();
                st.queue.push(format!("effect give {name} minecraft:regeneration 5 1"));
                st.queue.push(format!("effect give {name} minecraft:luck 20 0"));
                st.queue.push(format!("effect clear {name} minecraft:poison"));
            }
            _ => {}
        }
    }

    /// The Shock Absorber: falls minus 20% (at most 16), flying into a wall a quarter.
    fn living_incoming_damage(&self, _level: &mut dyn LevelRandomHost, event: &mut LivingIncomingDamageEvent<'_>) -> Flow {
        if !self.st().shock.contains(&event.entity) {
            return Flow::Continue;
        }
        let amount = event.container.new_damage();
        match event.kind {
            "minecraft:fall" => event.container.set_new_damage(amount - (amount * 0.2).min(16.0).floor()),
            "minecraft:fly_into_wall" => event.container.set_new_damage(amount / 4.0),
            _ => {}
        }
        Flow::Continue
    }

    /// AnvilScreenHandlerMixin: a damageable item and the Hardening Catalyst make the item
    /// unbreakable for 30 levels (not one already unbreakable; Homestead's blacklist tag is
    /// empty), the catalyst used up, the name as the rename box has it.
    fn anvil_update(&self, host: HostSide<'_>, event: &mut AnvilUpdateEvent<'_>) -> Flow {
        let host: &dyn Host = match &host {
            HostSide::Server(h) => &**h,
            HostSide::Client(h) => &**h,
        };
        let update = &mut *event.update;
        let Some(right) = &update.right else { return Flow::Continue };
        if id_of(host, right) != "things:hardening_catalyst" {
            return Flow::Continue;
        }
        let left = &update.left;
        let damageable = host.max_damage(left.item()).is_some_and(|m| m > 0);
        if !damageable || component(left, "minecraft:unbreakable").is_some() {
            update.output = None;
            return Flow::Cancel;
        }
        let mut out = left.clone();
        let mut patch = Compound::new();
        patch.insert("minecraft:unbreakable".into(), Tag::Compound(Compound::new()));
        match update.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
            Some(n) => {
                patch.insert("minecraft:custom_name".into(), Tag::String(n.into()));
            }
            None if component(left, "minecraft:custom_name").is_some() => {
                patch.insert("!minecraft:custom_name".into(), Tag::Compound(Compound::new()));
            }
            None => {}
        }
        let _ = out.apply_components(&patch);
        update.output = Some(out);
        update.xp_cost = 30;
        update.material_cost = 1;
        Flow::Continue
    }
}

/// RecallPotionItem.finishUsing: to the player's own respawn point (a bed or respawn anchor still
/// there, or a forced point), in its dimension; the world spawn isn't one.
fn recall(host: &mut dyn ServerHost, player: PlayerRef) {
    let Some(Some(config)) = player.respawn_config(&*host) else {
        let _ = player.action_bar(host, Text::literal("No respawn point"));
        return;
    };
    let dim = config.position.dimension().to_string();
    let p = config.position.pos();
    let p = [p.x, p.y, p.z];
    let block = host
        .level(&dim)
        .and_then(|l| l.block(&*host, p))
        .map(|b| b.id(&*host).to_string())
        .unwrap_or_default();
    let bed = block.ends_with("_bed");
    let anchor = block == "minecraft:respawn_anchor";
    if !(config.forced || bed || anchor) {
        let _ = player.action_bar(host, Text::literal("No respawn point"));
        return;
    }
    let up = if bed { 0.5625 } else if anchor { 1.0 } else { 0.0 };
    let at = [f64::from(p[0]) + 0.5, f64::from(p[1]) + up, f64::from(p[2]) + 0.5];
    let entity = player.entity();
    if player.dimension(&*host).is_some_and(|d| d.to_string() == dim) {
        entity.teleport_to(host, at, None);
    } else {
        entity.teleport_to_level(host, &dim, at, None);
    }
}
