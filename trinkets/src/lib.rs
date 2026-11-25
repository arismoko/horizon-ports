//! Trinkets 3.7.2 (Emi, C4; MIT): the player's extra equipment slots, written on Sunrise's typed mod API
//! (no `[[native.mixin]]`, no game module).
//!
//! - The worn stacks are a player attachment, `trinkets:worn`: a compound of slot id -> item NBT
//!   (`{id, count, components}`), saved with the player, kept through death, and synced to the player's client.
//!   Other packages (sophisticatedcore's back slot, elytraslot, things, radiantgear) read it with
//!   `attachment_key::<Compound>("trinkets:worn")` after declaring this package as a dependency.
//! - The menu (TrinketInventoryScreen's slots) is a vanilla chest menu the mod fills (`open_chest_menu`),
//!   opened by the Trinkets key (G, `key.trinkets.open` in Controls): the client sends `trinkets:open` and the
//!   server opens it on its next tick. A click on an inventory item wears it, a click on a worn one takes it off.
//! - A right click with a trinket wears it in the first empty slot that takes it (TrinketItem.use).
//! - The tooltip names the slots an item fits.
//! - Death: unless keepInventory, the worn stacks drop where the player died (vanishing-cursed ones vanish).
pub mod data;
pub use data::SLOTS;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use mod_api::{
    AttachmentCodec, AttachmentDescriptor, AttachmentKey, ClientHost, Compound, Flow, Hand, Holder,
    Hooks, Host, HostSide, IPayloadContext, IPayloadHandler, Identifier, InputKeyEvent,
    InteractionResult, ItemResource, ItemStack, ItemTooltipEvent, KeyAction, KeyRegistration,
    LivingDeathEvent, MenuClickedEvent, MenuPart, MenuRef, MenuRemovedEvent, MenuReplace,
    PayloadBytes, PayloadHandleError, PayloadId, PlayerRef, Registrar, RegistrationContext,
    RightClickItemEvent, ServerHost, ServerTickEvent, ServerTickPhase, Tag, Text,
};

/// The id of the worn-stacks attachment other packages look up.
pub const WORN_ID: &str = "trinkets:worn";
const OPEN: &str = "trinkets:open";
const KEY: &str = "open";
const MENU: &str = "minecraft:generic_9x2";

static WORN: RwLock<Option<AttachmentKey<Compound>>> = RwLock::new(None);

fn worn_key() -> Option<AttachmentKey<Compound>> {
    WORN.read().unwrap_or_else(|e| e.into_inner()).clone()
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether slot `i` takes `item` (its trinkets tag).
pub fn accepts(i: usize, item: &str) -> bool {
    SLOTS.get(i).is_some_and(|s| s.3.contains(&item))
}

/// Whether some slot takes `item`.
pub fn is_trinket(item: &str) -> bool {
    SLOTS.iter().any(|s| s.3.contains(&item))
}

/// Items tagged into a trinket slot that aren't TrinketItems in Java: their own use comes first (a backpack
/// opens, a wireless terminal opens its terminal), so a right click doesn't wear them; the menu still does.
fn own_use(id: &str) -> bool {
    id.starts_with("sophisticatedbackpacks:") || id == "toms_storage:ts.adv_wireless_terminal"
}

/// The custom_data key of a stand-in for a worn item the game can't make (a mod that isn't installed): the saved
/// entry, written back unchanged.
const UNKNOWN: &str = "trinkets:unknown";

fn custom_data(stack: &ItemStack) -> Compound {
    match stack
        .components()
        .and_then(|c| c.as_nbt().get(&"minecraft:custom_data".into()))
    {
        Some(Tag::Compound(c)) => c.clone(),
        _ => Compound::new(),
    }
}

/// The saved entry a stand-in carries.
fn unknown_entry(stack: &ItemStack) -> Option<Compound> {
    match custom_data(stack).get(&UNKNOWN.into())? {
        Tag::Compound(c) => Some(c.clone()),
        _ => None,
    }
}

fn is_unknown(stack: &ItemStack) -> bool {
    unknown_entry(stack).is_some()
}

fn text_line(t: &str, color: Option<&str>) -> Tag {
    let mut line = vec![("text", Tag::String(t.into())), ("italic", Tag::Byte(0))];
    if let Some(color) = color {
        line.push(("color", Tag::String(color.into())));
    }
    Tag::compound(line)
}

/// A barrier standing in for the saved entry `raw` ("Not ported: vc_gliders:paraglider_wood"); it stays in its
/// slot and is saved back as it was.
fn stand_in(host: &dyn Host, raw: &Compound) -> Option<ItemStack> {
    let id = match raw.get(&"id".into())? {
        Tag::String(s) => s.to_string_lossy(),
        _ => return None,
    };
    if id.is_empty() || id == "minecraft:air" {
        return None;
    }
    let key = host.item_key(&Identifier::parse("minecraft:barrier").ok()?)?;
    let mut st = ItemStack::new(key, 1)?;
    let mut patch = Compound::new();
    patch.insert(
        "minecraft:custom_name".into(),
        text_line(&format!("Not ported: {id}"), Some("red")),
    );
    patch.insert(
        "minecraft:lore".into(),
        Tag::List(vec![text_line(
            "Kept in this slot until its mod is installed",
            Some("gray"),
        )]),
    );
    patch.insert(
        "minecraft:custom_data".into(),
        Tag::compound([(UNKNOWN, Tag::Compound(raw.clone()))]),
    );
    st.apply_components(&patch).ok()?;
    Some(st)
}

/// The player's worn stacks, one per slot of `SLOTS`. A worn item the game can't make comes back as a stand-in
/// (`stand_in`), so the slot stays taken and `set_worn` writes the entry back.
fn worn(host: &mut dyn Host, uuid: u128) -> Vec<Option<ItemStack>> {
    let c = worn_key()
        .and_then(|key| {
            key.get_existing(host, Holder::Player(uuid), |c| c.cloned())
                .ok()
                .flatten()
        })
        .unwrap_or_default();
    let reader: &dyn Host = &*host;
    SLOTS
        .iter()
        .map(|s| match c.get(&s.0.into()) {
            Some(Tag::Compound(t)) => {
                ItemStack::from_nbt(reader, t).or_else(|| stand_in(reader, t))
            }
            _ => None,
        })
        .collect()
}

/// Writes the worn stacks. Entries under ids that aren't slots of this table are kept as they are.
fn set_worn(host: &mut dyn Host, uuid: u128, stacks: &[Option<ItemStack>]) -> bool {
    let Some(key) = worn_key() else {
        return false;
    };
    let mut c = key
        .get_existing(host, Holder::Player(uuid), |c| c.cloned())
        .ok()
        .flatten()
        .unwrap_or_default();
    for s in SLOTS {
        c.remove(&s.0.into());
    }
    for (s, stack) in SLOTS.iter().zip(stacks) {
        let Some(stack) = stack.as_ref().filter(|s| s.count() > 0) else {
            continue;
        };
        if let Some(t) = unknown_entry(stack).or_else(|| stack.to_nbt(&*host)) {
            c.insert(s.0.into(), Tag::Compound(t));
        }
    }
    key.set(host, Holder::Player(uuid), c).is_ok()
}

fn item_id(host: &dyn Host, stack: &ItemStack) -> String {
    stack.item().id(host).unwrap_or_default().to_owned()
}

/// Whether the components' `minecraft:enchantments` (id -> level) hold the curse of vanishing.
fn vanishes(components: &Compound) -> bool {
    matches!(
        components.get(&"minecraft:enchantments".into()),
        Some(Tag::Compound(e)) if e.contains_key(&"minecraft:vanishing_curse".into())
    )
}

fn has_vanishing_curse(stack: &ItemStack) -> bool {
    stack.components().is_some_and(|c| vanishes(c.as_nbt()))
}

/// `stack` with `count` (a count of 0 is nothing).
fn with_count(stack: &ItemStack, count: u32) -> Option<ItemStack> {
    if count == 0 {
        return None;
    }
    let mut s = stack.clone();
    s.set_count(count).ok()?;
    Some(s)
}

// ---- the menu ----

/// The chest rows the slots fill.
pub fn rows() -> u8 {
    SLOTS.len().div_ceil(9).clamp(1, 6) as u8
}

/// What the menu shows: each worn stack, or the slot's placeholder named for the slot.
fn menu_items(host: &dyn Host, worn: &[Option<ItemStack>]) -> Vec<Option<ItemStack>> {
    let mut out: Vec<Option<ItemStack>> = SLOTS
        .iter()
        .zip(worn)
        .map(|(s, w)| {
            w.clone().or_else(|| {
                let key = host.item_key(&Identifier::parse(s.2).ok()?)?;
                let mut p = ItemStack::new(key, 1)?;
                let mut patch = Compound::new();
                patch.insert(
                    "minecraft:custom_name".into(),
                    text_line(&format!("{} (empty)", s.1), None),
                );
                patch.insert(
                    "minecraft:lore".into(),
                    Tag::List(vec![text_line(
                        "Click a trinket in your inventory to wear it.",
                        Some("gray"),
                    )]),
                );
                let _ = p.apply_components(&patch);
                Some(p)
            })
        })
        .collect();
    out.resize(rows() as usize * 9, None);
    out
}

/// A menu slot past the menu's own: the player's inventory slot (main 9..36 first, then the hotbar).
pub fn inventory_slot(size: usize, slot: i32) -> Option<u32> {
    let rel = usize::try_from(slot).ok()?.checked_sub(size)?;
    match rel {
        0..27 => Some(rel as u32 + 9),
        27..36 => Some(rel as u32 - 27),
        _ => None,
    }
}

/// The main inventory (slots 0..36), and whether every slot was read as it is. A slot whose stack can't be handed
/// to a mod reads as empty: it is not, so nothing is written to a slot that only looks empty unless the read was
/// whole (`give_stack` places the rest instead).
fn inventory(host: &dyn Host, player: PlayerRef) -> (Vec<Option<ItemStack>>, bool) {
    player
        .with_inventory(host, |inventory| {
            let mut inv = vec![None; 36];
            let mut whole = true;
            for (slot, view) in inventory.iter() {
                if (slot as usize) < 36 {
                    match view.project() {
                        Some(stack) => inv[slot as usize] = Some(stack),
                        None => whole = false,
                    }
                }
            }
            (inv, whole)
        })
        .unwrap_or_else(|| (vec![None; 36], false))
}

/// Drops `stack` where the player stands.
fn drop_at(host: &mut dyn ServerHost, player: PlayerRef, stack: ItemStack) {
    let Some(dim) = player.dimension(&*host).map(|d| d.to_string()) else {
        return;
    };
    let Some(level) = host.level(&dim) else {
        return;
    };
    let Some(pos) = player.entity().position(&*host) else {
        return;
    };
    let _ = level.pop_resource(
        host,
        [
            pos[0].floor() as i32,
            pos[1].floor() as i32,
            pos[2].floor() as i32,
        ],
        stack,
    );
}

/// Puts `stack` in the player's inventory as the game's `Inventory.add` does; what doesn't fit is thrown to the
/// player. Whether it was given.
fn give(host: &mut dyn ServerHost, player: PlayerRef, stack: ItemStack) -> bool {
    match player.give_stack(host, stack) {
        Ok(rest) => {
            if let Some(rest) = rest.filter(|r| r.count() > 0) {
                drop_at(host, player, rest);
            }
            true
        }
        Err(e) => {
            eprintln!("[trinkets] give_stack: {e:?}");
            false
        }
    }
}

/// What a click does to the worn stacks and the inventory, before any of it is written.
#[derive(Debug, PartialEq)]
enum Plan {
    /// Nothing moves; the player is told why.
    Refuse(&'static str),
    /// Inventory slot `inv` is worn in slot `i`: one item goes, and what was there comes back.
    Wear { i: usize, inv: usize },
}

/// Which trinket slots can take an item, as (index, taken by an unknown stand-in).
fn plan_wear(fits: &[usize], occupied: &[bool], count: u32, room: bool, inv: usize) -> Plan {
    match fits
        .iter()
        .copied()
        .find(|&i| !occupied[i])
        .or(fits.first().copied())
    {
        Some(i) if count == 1 || !occupied[i] || room => Plan::Wear { i, inv },
        Some(_) => Plan::Refuse("No room in your inventory"),
        None => Plan::Refuse("That doesn't go in a trinket slot"),
    }
}

#[derive(Default)]
struct Shared {
    /// Players who pressed the key, to open the menu for on the next server tick.
    opens: Mutex<Vec<u128>>,
    /// The open trinket menu's window of each player.
    menus: Mutex<HashMap<u128, i32>>,
}

/// The player's open menu, when it is the window `window`.
fn live(host: &dyn ServerHost, uuid: u128, window: i32) -> Option<MenuRef> {
    let menu = host.player(uuid)?.container_menu(host)?;
    (menu.container_id(host) == Some(window)).then_some(menu)
}

impl Shared {
    fn open(&self, host: &mut dyn ServerHost, uuid: u128) {
        // Already looking at it: don't rebuild the menu and resend the inventory.
        let window = lock(&self.menus).get(&uuid).copied();
        if window.is_some_and(|w| live(&*host, uuid, w).is_some()) {
            return;
        }
        let stacks = worn(host, uuid);
        let items = menu_items(&*host, &stacks);
        let Some(player) = host.player(uuid) else {
            return;
        };
        match player.open_chest_menu(
            host,
            rows(),
            Text::literal("Trinkets"),
            items,
            MenuReplace::CloseFirst,
        ) {
            Ok(menu) => {
                if let Some(window) = menu.container_id(&*host) {
                    lock(&self.menus).insert(uuid, window);
                }
            }
            Err(e) => eprintln!("[trinkets] open_chest_menu: {e:?}"),
        }
    }

    /// A click in the player's trinket menu.
    fn click(&self, host: &mut dyn ServerHost, uuid: u128, window: i32, slot: i32) {
        let Some(player) = host.player(uuid) else {
            return;
        };
        let size = rows() as usize * 9;
        let mut worn = worn(host, uuid);
        let (mut inv, complete) = inventory(&*host, player);
        // Room for one more stack: a free slot, or (with an incomplete read) the game decides and throws what
        // doesn't fit.
        let room = !complete || inv.iter().any(Option::is_none);
        let mut changed: Vec<u32> = Vec::new();
        let mut gives: Vec<ItemStack> = Vec::new();
        let mut message: Option<&str> = None;
        if let Ok(i) = usize::try_from(slot)
            && i < SLOTS.len()
        {
            // TrinketSlot: take a worn stack off, into the inventory.
            if worn[i].as_ref().is_some_and(is_unknown) {
                let _ = player.action_bar(
                    host,
                    Text::literal("This item's mod isn't installed: it stays in its slot"),
                );
                return;
            }
            let Some(stack) = worn[i].take() else {
                return;
            };
            if room {
                gives.push(stack);
            } else {
                worn[i] = Some(stack);
                message = Some("No room in your inventory");
            }
        } else if let Some(s) = inventory_slot(size, slot) {
            // A click on an inventory item: into the first empty slot that takes it, else swapped with the
            // first one that does.
            let s = s as usize;
            let Some(stack) = inv[s].clone() else {
                return;
            };
            let id = item_id(&*host, &stack);
            let fits: Vec<usize> = (0..SLOTS.len())
                .filter(|&i| accepts(i, &id) && !worn[i].as_ref().is_some_and(is_unknown))
                .collect();
            let occupied: Vec<bool> = worn.iter().map(Option::is_some).collect();
            match plan_wear(&fits, &occupied, stack.count(), room, s) {
                Plan::Wear { i, .. } => {
                    inv[s] = with_count(&stack, stack.count() - 1);
                    changed.push(s as u32);
                    if let Some(old) = worn[i].replace(with_count(&stack, 1).unwrap_or(stack)) {
                        gives.push(old);
                    }
                }
                Plan::Refuse(m) => message = Some(m),
                _ => {}
            }
        } else {
            return;
        }
        for s in changed {
            if let Err(e) = player.set_inventory_slot(host, s, inv[s as usize].clone()) {
                eprintln!("[trinkets] set_inventory_slot {s}: {e:?}");
            }
        }
        for g in gives {
            if !give(host, player, g.clone()) {
                // Not given: it goes back on, or (when its slot is taken again) is dropped.
                let id = item_id(&*host, &g);
                match (0..SLOTS.len()).find(|&i| worn[i].is_none() && accepts(i, &id)) {
                    Some(i) => worn[i] = Some(g),
                    None => drop_at(host, player, g),
                }
                message = Some("No room in your inventory");
            }
        }
        set_worn(host, uuid, &worn);
        let items = menu_items(&*host, &worn);
        if let Some(menu) = live(&*host, uuid, window)
            && let Err(e) = menu.set_items(host, items)
        {
            eprintln!("[trinkets] set_items: {e:?}");
        }
        if let Some(m) = message {
            let _ = player.action_bar(host, Text::literal(m));
        }
    }
}

/// The trinket menu's clicks: the menu is the vanilla two-row chest menu, so the part is asked for every such
/// menu and takes only the window it opened.
struct TrinketMenu(Arc<Shared>);

#[mod_api::sunrise_part]
impl MenuPart for TrinketMenu {
    fn menu_clicked(&self, host: &mut dyn ServerHost, event: &mut MenuClickedEvent<'_>) -> Flow {
        if lock(&self.0.menus).get(&event.uuid) != Some(&event.window) {
            return Flow::Continue;
        }
        // PICKUP and QUICK_MOVE; drags, swaps and throws do nothing, and nothing moves on its own.
        if matches!(event.mode, 0 | 1) {
            self.0.click(host, event.uuid, event.window, event.slot);
        }
        Flow::Cancel
    }

    fn menu_removed(&self, _host: &mut dyn ServerHost, event: &mut MenuRemovedEvent<'_>) {
        let mut menus = lock(&self.0.menus);
        if menus.get(&event.uuid) == Some(&event.window) {
            menus.remove(&event.uuid);
        }
    }
}

struct OpenHandler(Arc<Shared>);

impl IPayloadHandler for OpenHandler {
    fn handle(
        &self,
        _payload: PayloadBytes,
        context: &mut dyn IPayloadContext,
    ) -> Result<(), PayloadHandleError> {
        // One pending open per player, however often the key packet comes.
        if let Ok(p) = context.player() {
            let uuid = p.uuid();
            let mut opens = lock(&self.0.opens);
            if !opens.contains(&uuid) {
                opens.push(uuid);
            }
        }
        Ok(())
    }
}

/// TrinketItem.use: the slot a right click with `stack` wears it in, if any.
fn wear_slot(
    host: &mut dyn Host,
    uuid: u128,
    stack: &ItemStack,
) -> Option<(usize, Vec<Option<ItemStack>>)> {
    let id = item_id(&*host, stack);
    // An elytra (anything equippable) still goes to its armour slot, as in Java: ElytraItem isn't a
    // TrinketItem.
    if !is_trinket(&id) || own_use(&id) || ItemResource::of(stack).equippable_slot(&*host).is_some()
    {
        return None;
    }
    let worn = worn(host, uuid);
    let i = (0..SLOTS.len()).find(|&i| worn[i].is_none() && accepts(i, &id))?;
    Some((i, worn))
}

/// A key name for a tooltip from its saved form (`key.keyboard.g`, `key.keyboard.left.shift`, `key.mouse.4`):
/// `G`, `Left Shift`, `Mouse 4`. None when unbound.
fn key_label(key: &str) -> Option<String> {
    let (kind, rest) = if let Some(r) = key.strip_prefix("key.keyboard.") {
        ("", r)
    } else {
        let r = key.strip_prefix("key.mouse.")?;
        ("Mouse ", r)
    };
    if rest.is_empty() || rest == "unknown" {
        return None;
    }
    let words: Vec<String> = rest
        .split('.')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().chain(c).collect(),
                None => String::new(),
            }
        })
        .collect();
    Some(format!("{kind}{}", words.join(" ")))
}

/// The tooltip line for the slots `names` (the slots an item fits), with the key as the player bound it.
fn tooltip_line(names: &[&str], hint: &str) -> Option<String> {
    let mut names: Vec<&str> = names.to_vec();
    names.dedup();
    match names.as_slice() {
        [] => None,
        [one] => Some(format!("§7Equippable in the §9{one}§7 trinket slot{hint}")),
        many => Some(format!(
            "§7Equippable in the trinket slots: §9{}§7{hint}",
            many.join(", ")
        )),
    }
}

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        let codec = AttachmentCodec {
            encode: |c: &Compound| Ok(c.clone()),
            decode: |c: &Compound| Ok(c.clone()),
        };
        let descriptor = AttachmentDescriptor::new(WORN_ID, Compound::new)
            .serialize(codec)
            .sync(codec)
            .copy_on_death();
        match r.register_attachment_type(descriptor) {
            Ok(key) => *WORN.write().unwrap_or_else(|e| e.into_inner()) = Some(key),
            Err(e) => eprintln!("[trinkets] {WORN_ID} not registered: {e}"),
        }
        r.register_key_mapping(
            KeyRegistration::new(KEY, "key.keyboard.g").label("key.trinkets.open"),
        );
        r.menu_part(MENU, TrinketMenu(self.shared.clone()));
        r.payload_handlers()
            .registrar("1")
            .optional()
            .play_to_server(
                PayloadId(OPEN.into()),
                Arc::new(OpenHandler(self.shared.clone())),
            );
    }

    /// The Trinkets key, while playing or in the player's own inventory: asks the server for the menu.
    fn input_key(&self, host: &mut dyn ClientHost, event: &mut InputKeyEvent<'_>) {
        if !matches!(event.action, KeyAction::Press) {
            return;
        }
        let bound = host
            .mod_key_mapping("trinkets", KEY)
            .ok()
            .and_then(|m| m.binding(&*host).ok().map(|(key, _)| key == event.key))
            == Some(true);
        if !bound {
            return;
        }
        let inventory = host
            .observed_screen_id()
            .is_some_and(|s| s.id == "inventory")
            && host.screen_title().is_none();
        if !(host.playing() || inventory) {
            return;
        }
        if let Err(e) =
            host.send_payload_to_server(&PayloadId(OPEN.into()), PayloadBytes(Vec::new()))
        {
            eprintln!("[trinkets] open: {e:?}");
        }
    }

    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        if !matches!(event.phase, ServerTickPhase::ServerTickEnd { .. }) {
            return;
        }
        let opens = std::mem::take(&mut *lock(&self.shared.opens));
        for uuid in opens {
            self.shared.open(host, uuid);
        }
    }

    /// TrinketItem.use: a right click with a trinket wears it in the first empty slot that takes it. The server
    /// does the wearing; the client does not predict it.
    fn right_click_item(&self, host: HostSide<'_>, event: &mut RightClickItemEvent<'_>) -> Flow {
        let HostSide::Server(host) = host else {
            return Flow::Continue;
        };
        let click = event.click;
        let Some((i, mut worn)) = wear_slot(host, click.player, click.stack) else {
            return Flow::Continue;
        };
        let Some(player) = host.player(click.player) else {
            return Flow::Continue;
        };
        let hand_slot = match click.hand {
            Hand::Main => player.selected_slot(&*host).map(|s| s as u32),
            _ => Some(40),
        };
        let Some(hand_slot) = hand_slot else {
            return Flow::Continue;
        };
        let remainder = with_count(click.stack, click.stack.count().saturating_sub(1));
        if player
            .set_inventory_slot(host, hand_slot, remainder)
            .is_err()
        {
            return Flow::Continue;
        }
        worn[i] = with_count(click.stack, 1);
        set_worn(host, click.player, &worn);
        if let Some(entity) = host.player(click.player).map(|p| p.entity()) {
            entity.play_sound(host, "minecraft:item.armor.equip_generic", 1.0, 1.0);
        }
        *event.cancellation_result = InteractionResult::Success;
        event.cancelled = true;
        Flow::Cancel
    }

    /// TrinketsApi's death drop (`dropInventory`): unless keepInventory, the worn stacks drop where the player
    /// died, vanishing-cursed ones destroyed. With keepInventory they stay worn.
    fn living_death(&self, host: &mut dyn ServerHost, event: &mut LivingDeathEvent<'_>) -> Flow {
        if event.kind != "minecraft:player" {
            return Flow::Continue;
        }
        let Some(entity) = host.entity(event.entity) else {
            return Flow::Continue;
        };
        let Some(uuid) = entity.uuid(host) else {
            return Flow::Continue;
        };
        let reader: &dyn ServerHost = &*host;
        let Some(level) = entity.level(reader) else {
            return Flow::Continue;
        };
        let keep = matches!(
            level.game_rule(reader, "minecraft:keep_inventory"),
            Ok(Some(mod_api::GameRuleValue::Bool(true)))
        );
        if keep {
            return Flow::Continue;
        }
        let Some(at) = entity.position(reader) else {
            return Flow::Continue;
        };
        let dim = host
            .player(uuid)
            .and_then(|p| p.dimension(&*host))
            .map(|d| d.to_string());
        let (Some(dim), stacks) = (dim, worn(host, uuid)) else {
            return Flow::Continue;
        };
        if stacks.iter().all(Option::is_none) {
            return Flow::Continue;
        }
        // Unknown stand-ins and every other saved entry stay as they are.
        let kept: Vec<Option<ItemStack>> = stacks
            .iter()
            .map(|s| s.clone().filter(is_unknown))
            .collect();
        set_worn(host, uuid, &kept);
        let Some(level) = host.level(&dim) else {
            return Flow::Continue;
        };
        let pos = [
            at[0].floor() as i32,
            at[1].floor() as i32,
            at[2].floor() as i32,
        ];
        for stack in stacks.into_iter().flatten().filter(|s| !is_unknown(s)) {
            if !has_vanishing_curse(&stack) {
                let _ = level.pop_resource(host, pos, stack);
            }
        }
        Flow::Continue
    }

    /// TrinketsClient's tooltip: which trinket slots take the item (placeholders show nothing).
    fn item_tooltip(&self, host: &dyn ClientHost, event: &mut ItemTooltipEvent<'_>) {
        let Some(id) = event.stack.item().id(host) else {
            return;
        };
        if id.starts_with("trinkets:slot_") {
            return;
        }
        let names: Vec<&str> = SLOTS
            .iter()
            .filter(|s| s.3.contains(&id))
            .map(|s| s.1)
            .collect();
        // The key as the player bound it; no hint when it's unbound.
        let hint = host
            .mod_key_mapping("trinkets", KEY)
            .ok()
            .and_then(|m| m.binding(host).ok())
            .and_then(|(key, _)| key_label(key))
            .map(|k| format!(" ({k})"))
            .unwrap_or_default();
        if let Some(line) = tooltip_line(&names, &hint) {
            let _ = event.lines.push(Text::literal(line), 0xFFAAAAAA);
        }
    }
}

#[derive(Default)]
struct Mod {
    shared: Arc<Shared>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_labels() {
        assert_eq!(key_label("key.keyboard.g").as_deref(), Some("G"));
        assert_eq!(
            key_label("key.keyboard.left.shift").as_deref(),
            Some("Left Shift")
        );
        assert_eq!(
            key_label("key.keyboard.keypad.1").as_deref(),
            Some("Keypad 1")
        );
        assert_eq!(key_label("key.mouse.4").as_deref(), Some("Mouse 4"));
        assert_eq!(key_label("key.keyboard.unknown"), None);
        assert_eq!(key_label("key.keyboard."), None);
        assert_eq!(key_label("g"), None);
    }

    #[test]
    fn inventory_slots_follow_the_menu() {
        let size = rows() as usize * 9;
        assert_eq!(inventory_slot(size, size as i32), Some(9));
        assert_eq!(inventory_slot(size, size as i32 + 26), Some(35));
        assert_eq!(inventory_slot(size, size as i32 + 27), Some(0));
        assert_eq!(inventory_slot(size, size as i32 + 35), Some(8));
        assert_eq!(inventory_slot(size, size as i32 + 36), None);
        assert_eq!(inventory_slot(size, 3), None);
        assert_eq!(inventory_slot(size, -999), None);
    }

    #[test]
    fn vanishing_curse_is_read_from_the_enchantments() {
        let mut enchantments = Compound::new();
        enchantments.insert("minecraft:vanishing_curse".into(), Tag::Int(1));
        let mut components = Compound::new();
        assert!(!vanishes(&components));
        components.insert("minecraft:enchantments".into(), Tag::Compound(enchantments));
        assert!(vanishes(&components));
        let mut other = Compound::new();
        other.insert(
            "minecraft:enchantments".into(),
            Tag::Compound(Compound::new()),
        );
        assert!(!vanishes(&other));
    }

    #[test]
    fn the_menu_is_two_rows() {
        assert_eq!(SLOTS.len(), 15);
        assert_eq!(rows(), 2);
        assert_eq!(MENU, "minecraft:generic_9x2");
    }

    #[test]
    fn backpacks_open_rather_than_wear() {
        assert!(own_use("sophisticatedbackpacks:iron_backpack"));
        assert!(!own_use("things:socks"));
    }

    #[test]
    fn the_cape_takes_an_elytra() {
        let cape = SLOTS.iter().position(|s| s.0 == "chest/cape").unwrap();
        assert!(accepts(cape, "minecraft:elytra"));
        assert!(!accepts(cape, "minecraft:stick"));
        assert!(!accepts(999, "minecraft:elytra"));
        assert!(is_trinket("minecraft:totem_of_undying"));
        assert!(!is_trinket("minecraft:stick"));
    }

    #[test]
    fn the_back_slot_takes_backpacks() {
        let back = SLOTS.iter().position(|s| s.0 == "chest/back").unwrap();
        assert!(accepts(back, "sophisticatedbackpacks:iron_backpack"));
    }

    #[test]
    fn slot_ids_are_unique_and_have_placeholders() {
        for (n, s) in SLOTS.iter().enumerate() {
            assert!(SLOTS[n + 1..].iter().all(|t| t.0 != s.0), "{}", s.0);
            assert!(s.2.starts_with("trinkets:slot_"));
            assert_eq!(s.2, format!("trinkets:slot_{}", s.0.replace('/', "_")));
        }
    }

    #[test]
    fn wearing_fills_an_empty_slot_first() {
        let mut occupied = vec![false; SLOTS.len()];
        occupied[1] = true;
        assert_eq!(
            plan_wear(&[1, 4], &occupied, 1, false, 7),
            Plan::Wear { i: 4, inv: 7 }
        );
    }

    #[test]
    fn wearing_swaps_when_every_fit_is_taken() {
        let occupied = vec![true; SLOTS.len()];
        assert_eq!(
            plan_wear(&[2, 3], &occupied, 1, false, 0),
            Plan::Wear { i: 2, inv: 0 }
        );
        // A stack of several needs room for what comes back.
        assert_eq!(
            plan_wear(&[2], &occupied, 5, false, 0),
            Plan::Refuse("No room in your inventory")
        );
        assert_eq!(
            plan_wear(&[2], &occupied, 5, true, 0),
            Plan::Wear { i: 2, inv: 0 }
        );
    }

    #[test]
    fn wearing_needs_a_slot_that_takes_it() {
        assert_eq!(
            plan_wear(&[], &vec![false; SLOTS.len()], 1, true, 0),
            Plan::Refuse("That doesn't go in a trinket slot")
        );
    }

    #[test]
    fn tooltips_name_the_slots() {
        assert_eq!(
            tooltip_line(&["Cape"], " (G)").as_deref(),
            Some("§7Equippable in the §9Cape§7 trinket slot (G)")
        );
        assert_eq!(
            tooltip_line(&["Ring", "Ring", "Belt"], "").as_deref(),
            Some("§7Equippable in the trinket slots: §9Ring, Belt§7")
        );
        assert_eq!(tooltip_line(&[], ""), None);
    }
}
