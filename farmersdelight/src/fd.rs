// Farmer's Delight 2.4.1+refabricated (vectorwing, MehVahdJukaar, MerchantPug; MIT) for Horizon:
// the Java half of the content in data/ and assets/, ported from the decompiled
// `vectorwing.farmersdelight.common` classes, with Homestead's farmersdelight-common.toml
// (richSoilBoostChance 0.8, cuttingBoardFortuneBonus 0.1). Also the add-ons' blocks that work
// as Farmer's Delight's do (Ocean's, End's, Nether's and Expanded Delight).
//
// Mod API only (version 0.5.0, SDK dd67b76): this was a game module reached by 21 mixins until
// 0.4.x. The rules are the same; what reaches them is now lib.rs's parts and hooks:
// - block entities (`Machine`, a BlockEntityPart): ticks, slots (declared containers), the items
//   drawn on them, the cooking pot's, juicer's and furnaces' screens;
// - right clicks (`right_click_block`) and items used on blocks (`use_item_on_block`);
// - the level (`Rules`, a BlockPart on each block with a rule): random ticks, scheduled ticks,
//   bone meal, survival, ground and placement;
// - placing (`Placer`, an ItemPart): the two-block feasts, tatami and mats, propelplant canes;
// - foods: `consume_effect`, and Comfort, Nourishment, the stove's burn and the propelplant's
//   blast in `player_tick_post`; the cabinets' open state in `mod_container_viewers`.
use std::sync::{Arc, Mutex};

use mod_api::serde_json;

use mod_api::{
    BlockEntityRef, BlockPos, BlockState, BlockUpdateFlags, Compound, Direction, EquipmentSlot, Hand, Host,
    HostBlockStates, HostTransaction, Identifier, ItemKey, ItemStack, LevelHost, LevelRef, LevelWriteHost,
    LightLayer, LivingRef, PlayerRef, RecipeInput, ServerHost, SoundSource, Tag, Text,
};

pub(crate) const NS: &str = "farmersdelight:";
const RICH_SOIL_BOOST: f32 = 0.8;
const FORTUNE_BONUS: f64 = 0.1;

// ---------------------------------------------------------------------------------------------
// Names
// ---------------------------------------------------------------------------------------------

/// Farmer's Delight's short name of a block (`stove`), the add-ons' full names.
pub(crate) fn short(name: &str) -> Option<&str> {
    name.strip_prefix(NS).or_else(|| match name {
        // The End Stove and the Blackstone Stove are stoves; the others keep their full names.
        "ends_delight:end_stove" | ND_STOVE => Some("stove"),
        _ if ADDON_BLOCKS.contains(&name) => Some(name),
        _ => None,
    })
}
pub(crate) const ADDON_BLOCKS: &[&str] = &[
    "oceansdelight:guardian_soup",
    "ends_delight:chorus_fruit_pie",
    "ends_delight:steamed_dragon_egg_block",
    "ends_delight:dragon_meat_stew_block",
    "ends_delight:grilled_shulker_block",
    DRAGON_LEG,
    SUCCULENT,
    STUFFED_HOGLIN,
    PROPELPLANT,
    CRIMSON_COLONY,
    WARPED_COLONY,
    SOUL_COMPOST,
    RICH_SOUL_SOIL,
    MIMICARNATION,
    "nethersdelight:propelplant_torch",
    JUICER,
    MORTAR,
    CINNAMON_LOG,
    CINNAMON_SAPLING,
    "nethersdelight:blackstone_furnace",
    "nethersdelight:nether_brick_smoker",
    "nethersdelight:blackstone_blast_furnace",
];
const DRAGON_LEG: &str = "ends_delight:dragon_leg_with_sauce_block";
const SUCCULENT: &str = "ends_delight:chorus_succulent";
const ND_STOVE: &str = "nethersdelight:blackstone_stove";
const STUFFED_HOGLIN: &str = "nethersdelight:stuffed_hoglin";
const PROPELPLANT: &str = "nethersdelight:propelplant_cane";
const CRIMSON_COLONY: &str = "nethersdelight:crimson_fungus_colony";
const WARPED_COLONY: &str = "nethersdelight:warped_fungus_colony";
const SOUL_COMPOST: &str = "nethersdelight:soul_compost";
const RICH_SOUL_SOIL: &str = "nethersdelight:rich_soul_soil";
const MIMICARNATION: &str = "nethersdelight:mimicarnation";
const JUICER: &str = "expandeddelight:juicer";
const MORTAR: &str = "expandeddelight:mortar_and_pestle";
const CINNAMON_LOG: &str = "expandeddelight:cinnamon_log";
const CINNAMON_SAPLING: &str = "expandeddelight:cinnamon_sapling";

/// Nether's Delight's furnaces (`AbstractFurnaceBlockEntity`): the recipe type each cooks and
/// its fuel's speed (the blast furnace and smoker burn fuel twice as fast).
fn nd_furnace(n: &str) -> Option<(&'static str, &'static str, i32)> {
    Some(match n {
        "nethersdelight:blackstone_furnace" => ("minecraft:smelting", "furnace", 1),
        "nethersdelight:nether_brick_smoker" => ("minecraft:smoking", "smoker", 2),
        "nethersdelight:blackstone_blast_furnace" => ("minecraft:blasting", "blast_furnace", 2),
        _ => return None,
    })
}
pub(crate) const ND_FURNACE_MENU: &str = "nethersdelight:furnace";
/// The two-block feasts (a bed's head and foot sharing the servings).
fn two_part(n: &str) -> bool {
    n == DRAGON_LEG || n == STUFFED_HOGLIN
}

/// The blocks lib.rs puts `Rules` on (their placement, ticks, bone meal or ground are ours).
pub(crate) fn rule_blocks() -> Vec<String> {
    let mut v: Vec<String> = [
        "stove", "cooking_pot", "skillet", "cutting_board", "rice_bale", "tatami", "full_tatami_mat",
        "half_tatami_mat", "apple_pie", "sweet_berry_cheesecake", "chocolate_pie", "roast_chicken_block",
        "stuffed_pumpkin_block", "honey_glazed_ham_block", "shepherds_pie_block", "rice_roll_medley_block",
        "rich_soil", "rich_soil_farmland", "organic_compost", "budding_tomatoes", "tomatoes", "rice",
        "rice_panicles", "brown_mushroom_colony", "red_mushroom_colony",
    ]
    .iter()
    .map(|n| format!("{NS}{n}"))
    .collect();
    for w in crate::WOODS.iter().filter(|w| **w != "pale_oak") {
        v.push(format!("{NS}{w}_cabinet"));
    }
    v.extend(ADDON_BLOCKS.iter().map(|s| s.to_string()));
    v.push("ends_delight:end_stove".into());
    v.push(ND_STOVE.into());
    v
}

// ---------------------------------------------------------------------------------------------
// Directions
// ---------------------------------------------------------------------------------------------

fn step(dir: &str) -> (i32, i32) {
    match dir {
        "north" => (0, -1),
        "south" => (0, 1),
        "west" => (-1, 0),
        "east" => (1, 0),
        _ => (0, 0),
    }
}
fn opposite(dir: &str) -> &'static str {
    match dir {
        "north" => "south",
        "south" => "north",
        "west" => "east",
        "east" => "west",
        "up" => "down",
        _ => "up",
    }
}
fn clockwise(dir: &str) -> &'static str {
    match dir {
        "north" => "east",
        "east" => "south",
        "south" => "west",
        _ => "north",
    }
}
fn counter_clockwise(dir: &str) -> &'static str {
    match dir {
        "north" => "west",
        "west" => "south",
        "south" => "east",
        _ => "north",
    }
}
/// `Direction.toYRot`.
fn y_rot(dir: &str) -> f32 {
    match dir {
        "south" => 0.0,
        "west" => 90.0,
        "north" => 180.0,
        _ => 270.0,
    }
}
/// `Direction.fromYRot`: the placer's horizontal facing.
fn horizontal(yaw: f32) -> &'static str {
    match ((yaw / 90.0 + 0.5).floor() as i32) & 3 {
        0 => "south",
        1 => "west",
        2 => "north",
        _ => "east",
    }
}
fn dir_name(d: Direction) -> &'static str {
    match d {
        Direction::Down => "down",
        Direction::Up => "up",
        Direction::North => "north",
        Direction::South => "south",
        Direction::West => "west",
        _ => "east",
    }
}
fn centre(p: [i32; 3]) -> [f64; 3] {
    p.map(|c| f64::from(c) + 0.5)
}
fn up(p: [i32; 3], n: i32) -> [i32; 3] {
    [p[0], p[1] + n, p[2]]
}

// ---------------------------------------------------------------------------------------------
// Block states as text (`farmersdelight:stove[facing=north,lit=true]`)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub(crate) struct St {
    pub id: String,
    props: Vec<(String, String)>,
}

impl St {
    pub fn parse(s: &str) -> St {
        let (id, rest) = s.split_once('[').unwrap_or((s, ""));
        let props = rest
            .trim_end_matches(']')
            .split(',')
            .filter_map(|kv| kv.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        St { id: id.to_string(), props }
    }
    pub fn get(&self, k: &str) -> Option<&str> {
        self.props.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str())
    }
    fn int(&self, k: &str) -> i32 {
        self.get(k).and_then(|v| v.parse().ok()).unwrap_or(0)
    }
    fn bool(&self, k: &str) -> bool {
        self.get(k) == Some("true")
    }
    fn with(&self, k: &str, v: &str) -> St {
        let mut s = self.clone();
        match s.props.iter_mut().find(|(a, _)| a == k) {
            Some(p) => p.1 = v.to_string(),
            None => s.props.push((k.to_string(), v.to_string())),
        }
        s
    }
    fn text(&self) -> String {
        if self.props.is_empty() {
            return self.id.clone();
        }
        let p: Vec<String> = self.props.iter().map(|(k, v)| format!("{k}={v}")).collect();
        format!("{}[{}]", self.id, p.join(","))
    }
    fn is_air(&self) -> bool {
        matches!(self.id.as_str(), "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air")
    }
    fn is_water(&self) -> bool {
        matches!(
            self.id.as_str(),
            "minecraft:water" | "minecraft:bubble_column" | "minecraft:kelp" | "minecraft:kelp_plant" | "minecraft:seagrass" | "minecraft:tall_seagrass"
        ) || self.bool("waterlogged")
    }
}

// ---------------------------------------------------------------------------------------------
// NBT
// ---------------------------------------------------------------------------------------------

fn nbt_int(nbt: &Compound, k: &str) -> i32 {
    match nbt.get(&k.into()) {
        Some(Tag::Int(v)) => *v,
        Some(Tag::Short(v)) => i32::from(*v),
        Some(Tag::Byte(v)) => i32::from(*v),
        _ => 0,
    }
}
fn nbt_long(nbt: &Compound, k: &str) -> i64 {
    match nbt.get(&k.into()) {
        Some(Tag::Long(v)) => *v,
        Some(Tag::Int(v)) => i64::from(*v),
        _ => 0,
    }
}
fn nbt_ints(nbt: &Compound, k: &str, n: usize) -> Vec<i32> {
    let mut out = match nbt.get(&k.into()) {
        Some(Tag::IntArray(v)) => v.clone(),
        _ => Vec::new(),
    };
    out.resize(n, 0);
    out
}
fn nbt_str(nbt: &Compound, k: &str) -> Option<String> {
    match nbt.get(&k.into()) {
        Some(Tag::String(s)) => Some(s.to_string_lossy()).filter(|s| !s.is_empty()),
        _ => None,
    }
}
fn put(nbt: &mut Compound, k: &str, v: Tag) {
    if nbt.get(&k.into()) != Some(&v) {
        nbt.insert(k.into(), v);
    }
}
fn string_tag(s: &str) -> Tag {
    Tag::String(s.into())
}

/// What clients draw of the stove's, skillet's and board's items: `View`, a list of the stacks
/// with their `Slot`, kept in the saved compound (the update tag has no host to name items).
fn view_of(host: &dyn Host, items: &[Option<ItemStack>]) -> Tag {
    let mut list = Vec::new();
    for (i, s) in items.iter().enumerate() {
        let Some(s) = s else { continue };
        let Some(mut c) = s.to_nbt(host) else { continue };
        c.insert("Slot".into(), Tag::Byte(i as i8));
        list.push(Tag::Compound(c));
    }
    Tag::List(list)
}
fn view_items(host: &dyn Host, nbt: &Compound, n: usize) -> Vec<Option<ItemStack>> {
    let mut out = vec![None; n];
    if let Some(Tag::List(list)) = nbt.get(&"View".into()) {
        for t in list {
            let Tag::Compound(c) = t else { continue };
            let slot = nbt_int(c, "Slot") as usize;
            if slot < n {
                out[slot] = ItemStack::from_nbt(host, c);
            }
        }
    }
    out
}

fn sized(items: &mut Vec<Option<ItemStack>>, n: usize) {
    if items.len() < n {
        items.resize(n, None);
    }
}
fn with_count(s: &ItemStack, n: u32) -> Option<ItemStack> {
    if n == 0 {
        return None;
    }
    let mut s = s.clone();
    s.set_count(n).ok()?;
    Some(s)
}
fn take_one(slot: &mut Option<ItemStack>) {
    if let Some(s) = slot.take() {
        *slot = s.shrink(1);
    }
}

// ---------------------------------------------------------------------------------------------
// Block entities
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    Furnace,
    Juicer,
    Stove,
    Pot,
    Skillet,
    Board,
    Basket,
    Cabinet,
}

pub(crate) fn kind_of_block(block: &str) -> Option<Kind> {
    let n = short(block)?;
    if nd_furnace(n).is_some() {
        return Some(Kind::Furnace);
    }
    Some(match n {
        "stove" => Kind::Stove,
        "cooking_pot" => Kind::Pot,
        "skillet" => Kind::Skillet,
        "cutting_board" => Kind::Board,
        "basket" => Kind::Basket,
        JUICER => Kind::Juicer,
        _ if n.ends_with("_cabinet") => Kind::Cabinet,
        _ => return None,
    })
}
pub(crate) fn kind_of_entity(id: &str) -> Option<Kind> {
    Some(match id {
        "farmersdelight:stove" | "farmersdelight:end_stove" | "farmersdelight:blackstone_stove" => Kind::Stove,
        "farmersdelight:juicer" => Kind::Juicer,
        "farmersdelight:furnace" => Kind::Furnace,
        "farmersdelight:cooking_pot" => Kind::Pot,
        "farmersdelight:skillet" => Kind::Skillet,
        "farmersdelight:cutting_board" => Kind::Board,
        "farmersdelight:basket" => Kind::Basket,
        "farmersdelight:cabinet" => Kind::Cabinet,
        _ => return None,
    })
}

/// `EntityBlock.getTicker`.
pub(crate) fn ticks(block: &str) -> bool {
    matches!(kind_of_block(block), Some(Kind::Stove | Kind::Pot | Kind::Skillet | Kind::Basket | Kind::Juicer | Kind::Furnace))
}

pub(crate) const POT_MENU: &str = "farmersdelight:cooking_pot";
const MEAL: usize = 6;
const CONTAINER: usize = 7;
const OUTPUT: usize = 8;

/// The block entities' item slots.
pub(crate) fn container(block: &str) -> Option<mod_api::ModContainer> {
    use mod_api::ModContainer;
    let k = kind_of_block(block)?;
    let all = |n: usize| -> ModContainer {
        let s: Vec<usize> = (0..n).collect();
        ModContainer {
            slots: n,
            faces: std::array::from_fn(|_| s.clone()),
            inputs: s.clone(),
            outputs: s,
            menu: Some("minecraft:generic_9x3".into()),
        }
    };
    Some(match k {
        Kind::Stove => ModContainer { slots: 6, ..Default::default() },
        Kind::Skillet | Kind::Board => ModContainer { slots: 1, ..Default::default() },
        // CookingPotItemHandler: the top reaches the ingredients, the other faces the container
        // slot and the output.
        Kind::Pot => {
            let inputs: Vec<usize> = (0..6).collect();
            let side = vec![CONTAINER, OUTPUT];
            ModContainer {
                slots: 9,
                faces: [side.clone(), inputs, side.clone(), side.clone(), side.clone(), side],
                inputs: (0..6).chain([CONTAINER]).collect(),
                outputs: vec![OUTPUT],
                menu: Some(POT_MENU.into()),
            }
        }
        Kind::Basket | Kind::Cabinet => all(27),
        // AbstractFurnaceBlockEntity: the input from the top, fuel from the sides, the result
        // (and an emptied bucket) below.
        Kind::Furnace => ModContainer {
            slots: 3,
            faces: [vec![2, 1], vec![0], vec![1], vec![1], vec![1], vec![1]],
            inputs: vec![0, 1],
            outputs: vec![2],
            menu: Some(ND_FURNACE_MENU.into()),
        },
        // JuicerBlockEntity: the fruit and sugar from the top, bottles from the sides, juice below.
        Kind::Juicer => ModContainer {
            slots: 5,
            faces: [vec![4], vec![0, 1], vec![2], vec![2], vec![2], vec![2]],
            inputs: vec![0, 1, 2],
            outputs: vec![4],
            menu: Some(JUICER.into()),
        },
    })
}

/// What clients see of a block entity: the items drawn on it.
pub(crate) fn update_tag(entity: &mod_api::ModBlockEntity) -> Option<Compound> {
    let k = kind_of_entity(entity.id())?;
    if !matches!(k, Kind::Stove | Kind::Skillet | Kind::Board) {
        return None;
    }
    let mut c = Compound::new();
    c.insert("View".into(), entity.nbt.get(&"View".into()).cloned().unwrap_or(Tag::List(Vec::new())));
    if k == Kind::Board {
        c.insert("IsItemCarved".into(), Tag::Byte(nbt_int(&entity.nbt, "IsItemCarved") as i8));
    }
    if k == Kind::Skillet {
        c.insert("FlipTime".into(), Tag::Long(nbt_long(&entity.nbt, "FlipTime")));
    }
    Some(c)
}

// ---------------------------------------------------------------------------------------------
// The server's world, through the host
// ---------------------------------------------------------------------------------------------

/// A server-side call's host and level, and the player acting, if any.
pub(crate) struct Cx<'a> {
    pub host: &'a mut dyn ServerHost,
    pub lv: LevelRef,
    pub player: Option<PlayerRef>,
    pub hand: Hand,
}

impl<'a> Cx<'a> {
    pub fn new(host: &'a mut dyn ServerHost, lv: LevelRef) -> Self {
        Cx { host, lv, player: None, hand: Hand::Main }
    }
    fn h(&self) -> &dyn ServerHost {
        &*self.host
    }
    fn state(&self, p: [i32; 3]) -> Option<BlockState> {
        self.lv.block_state(self.h(), p)
    }
    fn st(&self, p: [i32; 3]) -> St {
        self.state(p).and_then(|s| self.h().block_state_string(s)).map(|t| St::parse(&t)).unwrap_or_default()
    }
    fn name(&self, p: [i32; 3]) -> String {
        self.st(p).id
    }
    fn prop(&self, p: [i32; 3], k: &str) -> Option<String> {
        self.st(p).get(k).map(str::to_string)
    }
    fn set_st(&mut self, p: [i32; 3], st: &St, flags: BlockUpdateFlags) -> bool {
        let Some(state) = self.h().block_state_id(&st.text()) else { return false };
        matches!(self.lv.set_block(self.host, p, state, flags), Ok(true))
    }
    fn set_props(&mut self, p: [i32; 3], changes: &[(&str, &str)]) -> bool {
        let mut st = self.st(p);
        for (k, v) in changes {
            st = st.with(k, v);
        }
        self.set_st(p, &st, BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS)
    }
    fn set_props_quiet(&mut self, p: [i32; 3], changes: &[(&str, &str)]) -> bool {
        let mut st = self.st(p);
        for (k, v) in changes {
            st = st.with(k, v);
        }
        self.set_st(p, &st, BlockUpdateFlags::CLIENTS)
    }
    fn set_default(&mut self, p: [i32; 3], name: &str, changes: &[(&str, &str)]) -> bool {
        let Some(d) = self.h().block_state_id(name).and_then(|s| self.h().block_state_string(s)) else { return false };
        let mut st = St::parse(&d);
        for (k, v) in changes {
            st = st.with(k, v);
        }
        self.set_st(p, &st, BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS)
    }
    fn remove(&mut self, p: [i32; 3]) {
        let _ = self.set_default(p, "minecraft:air", &[]);
    }
    fn replaceable(&self, p: [i32; 3]) -> bool {
        self.state(p).and_then(|s| self.h().block_states().replaceable(s)).unwrap_or(false)
    }
    fn block_tag_at(&self, tag: &str, p: [i32; 3]) -> bool {
        self.state(p).is_some_and(|s| self.h().block_in_tag(s.block(), tag) == Some(true))
    }
    fn item_tag(&self, tag: &str, item: ItemKey) -> bool {
        item_tag(self.h(), tag, item)
    }
    fn id(&self, item: ItemKey) -> String {
        item.id(self.h())
    }
    fn stack(&self, id: &str, n: u32) -> Option<ItemStack> {
        stack_of(self.h(), id, n)
    }
    fn max_stack(&self, item: ItemKey) -> u32 {
        self.h().max_stack_size(item).unwrap_or(64)
    }
    fn remainder(&self, stack: &ItemStack) -> Option<ItemStack> {
        self.h().item_crafting_remainder(stack)
    }
    fn is_knife(&self, item: ItemKey) -> bool {
        self.item_tag("c:tools/knives", item) || self.item_tag("farmersdelight:tools/knives", item)
    }
    fn is_shears(&self, item: ItemKey) -> bool {
        self.id(item) == "minecraft:shears" || self.item_tag("c:shears", item) || self.item_tag("c:tools/shears", item)
    }
    fn rf(&mut self) -> f32 {
        self.lv.with_random(self.host, |r| mod_api::RandomSource::next_float(r)).unwrap_or(0.5)
    }
    fn rg(&mut self) -> f64 {
        self.lv.with_random(self.host, |r| mod_api::RandomSource::next_gaussian(r)).unwrap_or(0.0)
    }
    fn ri(&mut self, bound: i32) -> i32 {
        self.lv.with_random(self.host, |r| mod_api::RandomSource::next_int_bound(r, bound).unwrap_or(0)).unwrap_or(0)
    }
    fn play(&mut self, sound: &str, at: [f64; 3], volume: f32, pitch: f32) {
        self.lv.play_sound(self.host, sound, at, SoundSource::Blocks, volume, pitch);
    }
    /// `ItemUtils.spawnItemEntity`.
    fn spawn(&mut self, stack: ItemStack, at: [f64; 3], vel: [f64; 3]) {
        let _ = self.lv.spawn_item_with_motion(self.host, at[0], at[1], at[2], stack, vel, 10);
    }
    /// `Block.popResource`.
    fn pop(&mut self, p: [i32; 3], stack: ItemStack) {
        let _ = self.lv.pop_resource(self.host, p, stack);
    }
    fn tell(&mut self, text: Text) {
        if let Some(p) = self.player {
            let _ = p.action_bar(self.host, text);
        }
    }
    fn sneaking(&self) -> bool {
        self.player.and_then(|p| p.entity().is_shift_key_down(self.h())) == Some(true)
    }
    fn creative(&self) -> bool {
        self.player.and_then(|p| p.is_creative(self.h())) == Some(true)
    }
    fn living(&self) -> Option<LivingRef> {
        self.player.map(|p| LivingRef::new(p.entity()))
    }
    fn hand_item(&self, hand: Hand) -> Option<ItemStack> {
        self.living()?.hand_item(self.h(), hand).filter(|s| s.count() > 0)
    }
    fn held(&self) -> Option<ItemStack> {
        self.hand_item(self.hand)
    }
    fn set_held(&mut self, stack: Option<ItemStack>) {
        if let Some(l) = self.living() {
            let _ = l.set_hand_item(self.host, self.hand, stack);
        }
    }
    /// The held stack one smaller (none in creative).
    fn use_one(&mut self) {
        if self.creative() {
            return;
        }
        let held = self.held();
        self.set_held(held.and_then(|s| s.shrink(1)));
    }
    fn damage_held(&mut self, n: i32) {
        let slot = if self.hand == Hand::Main { EquipmentSlot::MainHand } else { EquipmentSlot::OffHand };
        if let Some(l) = self.living() {
            let _ = l.hurt_and_break(self.host, slot, n);
        }
    }
    /// Into the player's inventory, else dropped at their feet.
    fn give_or_drop(&mut self, stack: ItemStack) {
        let Some(p) = self.player else { return };
        if p.give_stack(self.host, stack.clone()).is_err() {
            let _ = p.drop_stack(self.host, stack, false);
        }
    }
    fn be(&self, p: [i32; 3]) -> Option<BlockEntityRef> {
        self.h().block_entity(self.lv, p)
    }
    fn be_items(&self, p: [i32; 3], n: usize) -> Option<Vec<Option<ItemStack>>> {
        let be = self.be(p)?;
        Some((0..n).map(|i| be.item_slot(self.h(), i as u32).ok().flatten().filter(|s| s.count() > 0)).collect())
    }
    fn be_nbt(&self, p: [i32; 3]) -> Compound {
        self.be(p).and_then(|b| b.data(self.h())).unwrap_or_default()
    }
    fn be_set_item(&mut self, p: [i32; 3], i: usize, stack: Option<ItemStack>) -> bool {
        let Some(be) = self.be(p) else { return false };
        self.host.transaction(|h, tx| be.set_item_slot(h, tx, i as u32, stack.clone()).map_err(|_| ())).is_ok()
    }
    /// Writes `nbt` keys into the block entity (which sends it to clients).
    fn be_merge(&mut self, p: [i32; 3], nbt: Compound) {
        if let Some(be) = self.be(p) {
            let _ = be.merge_data(self.host, nbt);
        }
    }
    /// The drawn items after a click changed them.
    fn refresh_view(&mut self, p: [i32; 3], n: usize, extra: &[(&str, Tag)]) {
        let Some(items) = self.be_items(p, n) else { return };
        let mut c = Compound::new();
        c.insert("View".into(), view_of(self.h(), &items));
        for (k, v) in extra {
            c.insert((*k).into(), v.clone());
        }
        self.be_merge(p, c);
        self.host.block_entity_sync(p);
    }
    fn campfire(&self, stack: &ItemStack) -> Option<(ItemStack, u32)> {
        cook(self.h(), self.lv, "minecraft:campfire_cooking", stack)
    }
}

pub(crate) fn item_tag(host: &dyn Host, tag: &str, item: ItemKey) -> bool {
    let Ok(id) = Identifier::parse(tag) else { return false };
    let tags = host.tags();
    tags.key(&id).and_then(|k| tags.is_in_tag(item, k)) == Some(true)
}
pub(crate) fn stack_of(host: &dyn Host, id: &str, n: u32) -> Option<ItemStack> {
    let key = host.item_key(&Identifier::parse(id).ok()?)?;
    ItemStack::new(key, n)
}
/// A cooking recipe's result and time for `stack` (`campfire_cooking`, `smelting`...).
fn cook(host: &dyn Host, lv: LevelRef, kind: &str, stack: &ItemStack) -> Option<(ItemStack, u32)> {
    let one = with_count(stack, 1)?;
    let m = lv.recipe_for(host, &Identifier::parse(kind).ok()?, &RecipeInput::Single(one)).ok()??;
    Some((m.output, m.processing_time))
}

/// `HeatableBlockEntity.isHeated`: a heat source below (lit, if it has `lit`), or one under a
/// heat conductor.
fn heated(cx: &Cx<'_>, p: [i32; 3], direct: bool) -> bool {
    let source = |q: [i32; 3]| -> Option<bool> {
        let st = cx.st(q);
        // End's Delight puts its End Stove in the tag; here it is a stove (`short`). Candlelight puts
        // its stoves in it too (`candlelight:stoves`), Meadow its tiled stoves (`meadow:allows_cooking`).
        (cx.block_tag_at("farmersdelight:heat_sources", q)
            || short(&st.id) == Some("stove")
            || nd_furnace(&st.id).is_some()
            || cx.block_tag_at("candlelight:stoves", q)
            || cx.block_tag_at("meadow:allows_cooking", q))
        .then(|| st.get("lit").is_none_or(|v| v == "true"))
    };
    let below = up(p, -1);
    if let Some(lit) = source(below) {
        return lit;
    }
    if !direct && cx.block_tag_at("farmersdelight:heat_conductors", below) {
        return source(up(p, -2)).unwrap_or(false);
    }
    false
}
fn tray_below(cx: &Cx<'_>, p: [i32; 3]) -> bool {
    cx.block_tag_at("farmersdelight:tray_heat_sources", up(p, -1))
}

/// `StoveBlockEntity.isStoveBlockedAbove`: the block above has a shape in the grill area
/// (3..13 × 0..1 × 3..13 pixels of it).
fn stove_blocked(cx: &Cx<'_>, p: [i32; 3]) -> bool {
    let Some(state) = cx.state(up(p, 1)) else { return false };
    let (lo, hi) = (3.0 / 16.0, 13.0 / 16.0);
    cx.h()
        .state_outline(state)
        .unwrap_or_default()
        .iter()
        .any(|b| b[0] < hi && b[3] > lo && b[1] < 1.0 / 16.0 && b[4] > 0.0 && b[2] < hi && b[5] > lo)
}

// ---------------------------------------------------------------------------------------------
// Ticks
// ---------------------------------------------------------------------------------------------

/// The tick of our block entity at `pos`; true when its saved fields changed.
pub(crate) fn tick(cx: &mut Cx<'_>, pos: [i32; 3], id: &str, e: &mut mod_api::item::containers::ModBlockEntityFields) -> bool {
    let Some(k) = kind_of_entity(id) else { return false };
    let before = e.clone();
    match k {
        Kind::Stove => stove_tick(cx, pos, e),
        Kind::Pot => pot_tick(cx, pos, e),
        Kind::Skillet => skillet_tick(cx, pos, e),
        Kind::Basket => basket_tick(cx, pos, e),
        Kind::Juicer => juicer_tick(cx, e),
        Kind::Furnace => furnace_tick(cx, pos, e),
        _ => return false,
    }
    *e != before
}

/// `StoveBlockEntity.cookingTick`.
fn stove_tick(cx: &mut Cx<'_>, pos: [i32; 3], e: &mut mod_api::item::containers::ModBlockEntityFields) {
    sized(&mut e.items, 6);
    let lit = cx.prop(pos, "lit").is_some_and(|v| v == "true");
    let mut times = nbt_ints(&e.nbt, "CookingTimes", 6);
    let totals = nbt_ints(&e.nbt, "CookingTotalTimes", 6);
    let mut changed = false;
    if stove_blocked(cx, pos) {
        if e.items.iter().any(Option::is_some) {
            for s in std::mem::take(&mut e.items).into_iter().flatten() {
                cx.pop(pos, s);
            }
            sized(&mut e.items, 6);
            changed = true;
        }
    } else if lit {
        for i in 0..6 {
            let Some(s) = e.items[i].clone() else { continue };
            times[i] += 1;
            if times[i] >= totals[i] {
                if let Some((result, _)) = cx.campfire(&s) {
                    let vel = [cx.rg() * 0.01, 0.1, cx.rg() * 0.01];
                    let at = [f64::from(pos[0]) + 0.5, f64::from(pos[1]) + 1.0, f64::from(pos[2]) + 0.5];
                    cx.spawn(result, at, vel);
                }
                e.items[i] = None;
                changed = true;
            }
        }
    } else {
        for i in 0..6 {
            if times[i] > 0 {
                times[i] = (times[i] - 2).clamp(0, totals[i].max(0));
            }
        }
    }
    put(&mut e.nbt, "CookingTimes", Tag::IntArray(times));
    if changed {
        put(&mut e.nbt, "View", view_of(cx.h(), &e.items));
        cx.host.block_entity_sync(pos);
    }
}

/// `CookingPotRecipe`, as its file says.
#[derive(Clone)]
struct PotRecipe {
    id: String,
    ingredients: Vec<mod_api::Ingredient>,
    result: ItemStack,
    container: Option<String>,
    time: i32,
}

/// A parsed recipe list, its recipe ids' hash and the tick it was last compared at.
type Cache<T> = Mutex<Option<(u64, u64, Arc<Vec<T>>)>>;
static POT_RECIPES: Cache<PotRecipe> = Mutex::new(None);
static JUICE_RECIPES: Cache<JuiceRecipe> = Mutex::new(None);
static CUT_RECIPES: Cache<CutRecipe> = Mutex::new(None);
/// Server ticks so far (lib.rs's `server_tick`).
pub(crate) static TICKS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A recipe type's list, parsed once: compared with the recipe table again at most every 100
/// ticks (a data pack reload changes it), and parsed again only when its recipe ids changed.
fn cached<T>(cache: &Cache<T>, host: &dyn Host, kind: &str, build: impl FnOnce(&[mod_api::Recipe]) -> Vec<T>) -> Arc<Vec<T>> {
    use std::hash::{Hash, Hasher};
    let now = TICKS.load(std::sync::atomic::Ordering::Relaxed);
    let mut c = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((checked, _, v)) = c.as_ref()
        && now.wrapping_sub(*checked) < 100
    {
        return Arc::clone(v);
    }
    let list = host.recipes_of_type(kind);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for r in list.iter() {
        r.id.hash(&mut h);
    }
    let key = h.finish();
    if let Some((checked, k, v)) = c.as_mut()
        && *k == key
    {
        *checked = now;
        return Arc::clone(v);
    }
    let v = Arc::new(build(&list));
    *c = Some((now, key, Arc::clone(&v)));
    v
}

fn ingredient(host: &dyn Host, json: &serde_json::Value) -> Option<mod_api::Ingredient> {
    host.recipe_ingredient(&json.to_string()).ok().flatten()
}
fn qualified(id: &str) -> String {
    if id.contains(':') { id.to_string() } else { format!("minecraft:{id}") }
}

/// The cooking recipes.
fn pot_recipes(host: &dyn Host) -> Arc<Vec<PotRecipe>> {
    cached(&POT_RECIPES, host, "farmersdelight:cooking", |list| {
        let mut out = Vec::new();
        for r in list.iter() {
            let j = &r.json;
            let Some(result) = host.recipe_result(&j["result"].to_string()).ok().flatten() else { continue };
            let Some(ingredients) = j["ingredients"]
                .as_array()
                .and_then(|a| a.iter().map(|i| ingredient(host, i)).collect::<Option<Vec<_>>>())
            else {
                continue;
            };
            let container = j["container"]["id"]
                .as_str()
                .or(j["container"]["item"].as_str())
                .map(qualified)
                .or_else(|| host.item_crafting_remainder(&result).map(|s| s.item().id(host)));
            out.push(PotRecipe { id: r.id.clone(), ingredients, result, container, time: j["cookingtime"].as_i64().unwrap_or(200) as i32 });
        }
        out
    })
}

/// `RecipeMatcher.findMatches`: each filled input slot to its own ingredient.
fn shapeless(host: &dyn Host, ingredients: &[mod_api::Ingredient], stacks: &[&ItemStack]) -> bool {
    if ingredients.len() != stacks.len() {
        return false;
    }
    fn place(host: &dyn Host, i: usize, ingredients: &[mod_api::Ingredient], stacks: &[&ItemStack], used: &mut [bool]) -> bool {
        if i == ingredients.len() {
            return true;
        }
        for (s, stack) in stacks.iter().enumerate() {
            if !used[s] && ingredients[i].test(host, stack) {
                used[s] = true;
                if place(host, i + 1, ingredients, stacks, used) {
                    return true;
                }
                used[s] = false;
            }
        }
        false
    }
    place(host, 0, ingredients, stacks, &mut vec![false; stacks.len()])
}

/// `CookingPotBlockEntity.INGREDIENT_REMAINDER_OVERRIDES`.
fn remainder_override(item: &str) -> Option<&'static str> {
    Some(match item {
        "minecraft:powder_snow_bucket"
        | "minecraft:axolotl_bucket"
        | "minecraft:cod_bucket"
        | "minecraft:pufferfish_bucket"
        | "minecraft:salmon_bucket"
        | "minecraft:tropical_fish_bucket" => "minecraft:bucket",
        "minecraft:suspicious_stew" | "minecraft:mushroom_stew" | "minecraft:rabbit_stew" | "minecraft:beetroot_soup" => "minecraft:bowl",
        "minecraft:potion" | "minecraft:splash_potion" | "minecraft:lingering_potion" | "minecraft:experience_bottle" => {
            "minecraft:glass_bottle"
        }
        _ => return None,
    })
}

/// `CookingPotBlockEntity.cookingTick`.
fn pot_tick(cx: &mut Cx<'_>, pos: [i32; 3], e: &mut mod_api::item::containers::ModBlockEntityFields) {
    sized(&mut e.items, 9);
    let is_heated = heated(cx, pos, false);
    let mut cook = nbt_int(&e.nbt, "CookTime");
    let mut total = nbt_int(&e.nbt, "CookTimeTotal");
    let has_input = e.items[..6].iter().any(Option::is_some);
    if is_heated && has_input {
        let recipes = pot_recipes(cx.h());
        let filled: Vec<&ItemStack> = e.items[..6].iter().flatten().collect();
        let found = recipes.iter().find(|r| shapeless(cx.h(), &r.ingredients, &filled)).cloned();
        drop(filled);
        let can_cook = found.as_ref().is_some_and(|r| match &e.items[MEAL] {
            None => true,
            Some(m) => m.same_item(&r.result) && m.count() + r.result.count() <= cx.max_stack(r.result.item()),
        });
        match found.filter(|_| can_cook) {
            Some(r) => {
                if nbt_str(&e.nbt, "LastRecipe").as_deref() != Some(&r.id) {
                    if nbt_str(&e.nbt, "LastRecipe").is_some() {
                        cook = 0;
                    }
                    put(&mut e.nbt, "LastRecipe", string_tag(&r.id));
                }
                cook += 1;
                total = r.time;
                if cook >= total {
                    cook = 0;
                    put(&mut e.nbt, "Container", string_tag(r.container.as_deref().unwrap_or("")));
                    match &mut e.items[MEAL] {
                        Some(m) => {
                            let _ = m.grow(r.result.count());
                        }
                        slot => *slot = Some(r.result.clone()),
                    }
                    let facing = cx.prop(pos, "facing").unwrap_or_else(|| "north".into());
                    let (dx, dz) = step(counter_clockwise(&facing));
                    for i in 0..6 {
                        let Some(s) = e.items[i].clone() else { continue };
                        let rest = cx.remainder(&s).or_else(|| remainder_override(&cx.id(s.item())).and_then(|r| cx.stack(r, 1)));
                        if let Some(rest) = rest {
                            let at = [
                                f64::from(pos[0]) + 0.5 + f64::from(dx) * 0.25,
                                f64::from(pos[1]) + 0.7,
                                f64::from(pos[2]) + 0.5 + f64::from(dz) * 0.25,
                            ];
                            cx.spawn(rest, at, [f64::from(dx) * 0.08, 0.25, f64::from(dz) * 0.08]);
                        }
                        take_one(&mut e.items[i]);
                    }
                }
            }
            None => cook = (cook - 2).clamp(0, total.max(0)),
        }
    } else if cook > 0 {
        cook = (cook - 2).clamp(0, total.max(0));
    }

    if let Some(meal) = e.items[MEAL].clone() {
        let container = nbt_str(&e.nbt, "Container").or_else(|| cx.remainder(&meal).map(|s| cx.id(s.item())));
        let max = cx.max_stack(meal.item());
        let out = e.items[OUTPUT].clone();
        let have = out.as_ref().map_or(0, ItemStack::count);
        let out_ok = out.as_ref().is_none_or(|o| o.same_item(&meal) && o.same_components(&meal));
        match container {
            // moveMealToOutput
            None => {
                let n = meal.count().min(max.saturating_sub(have));
                if out_ok && n > 0 {
                    e.items[OUTPUT] = with_count(&meal, have + n);
                    e.items[MEAL] = with_count(&meal, meal.count() - n);
                }
            }
            // useStoredContainersOnMeal
            Some(c) => {
                if let Some(held) = e.items[CONTAINER].clone().filter(|h| cx.id(h.item()) == c)
                    && out_ok
                    && have < max
                {
                    let n = meal.count().min(held.count()).min(max - have);
                    if n > 0 {
                        e.items[OUTPUT] = with_count(&meal, have + n);
                        e.items[MEAL] = with_count(&meal, meal.count() - n);
                        e.items[CONTAINER] = with_count(&held, held.count() - n);
                    }
                }
            }
        }
    }
    put(&mut e.nbt, "CookTime", Tag::Int(cook));
    put(&mut e.nbt, "CookTimeTotal", Tag::Int(total));
    let data = vec![cook, total, i32::from(is_heated)];
    if e.data != data {
        e.data = data;
    }
}

/// `AbstractFurnaceBlockEntity.serverTick` (Nether's Delight's furnaces), and the block's `lit`
/// following it.
fn furnace_tick(cx: &mut Cx<'_>, pos: [i32; 3], e: &mut mod_api::item::containers::ModBlockEntityFields) {
    let Some((kind, _, fuel_div)) = nd_furnace(&cx.name(pos)) else { return };
    sized(&mut e.items, 3);
    let mut lit = nbt_int(&e.nbt, "lit_time_remaining");
    let mut lit_total = nbt_int(&e.nbt, "lit_total_time");
    let mut timer = nbt_int(&e.nbt, "cooking_time_spent");
    let mut total = nbt_int(&e.nbt, "cooking_total_time");
    let was_lit = lit > 0;
    if lit > 0 {
        lit -= 1;
    }
    let recipe = e.items[0].as_ref().and_then(|i| cook(cx.h(), cx.lv, kind, i));
    let fits = |r: &ItemStack, out: &Option<ItemStack>| match out {
        None => true,
        Some(o) => o.same_item(r) && o.same_components(r) && o.count() + r.count() <= cx.max_stack(o.item()),
    };
    let can_burn = recipe.as_ref().is_some_and(|(r, _)| fits(r, &e.items[2]));
    if lit <= 0 && can_burn
        && let Some(fuel) = e.items[1].clone()
    {
        let burn = cx.h().item_burn_time(fuel.item()).unwrap_or(0) / fuel_div;
        if burn > 0 {
            lit = burn;
            lit_total = burn;
            let rest = cx.remainder(&fuel);
            e.items[1] = fuel.shrink(1).or(rest);
        }
    }
    if lit > 0 && can_burn {
        let (result, time) = recipe.clone().unwrap();
        total = time as i32;
        timer += 1;
        if timer >= total {
            timer = 0;
            match &mut e.items[2] {
                Some(o) => {
                    let _ = o.grow(result.count());
                }
                slot => *slot = Some(result),
            }
            take_one(&mut e.items[0]);
        }
    } else if lit <= 0 && timer > 0 {
        timer = (timer - 2).clamp(0, total.max(0));
    } else if !can_burn {
        timer = 0;
    }
    put(&mut e.nbt, "lit_time_remaining", Tag::Int(lit));
    put(&mut e.nbt, "lit_total_time", Tag::Int(lit_total));
    put(&mut e.nbt, "cooking_time_spent", Tag::Int(timer));
    put(&mut e.nbt, "cooking_total_time", Tag::Int(total));
    let data = vec![lit, lit_total, timer, total];
    if e.data != data {
        e.data = data;
    }
    let now_lit = lit > 0;
    if now_lit != was_lit || cx.prop(pos, "lit").is_some_and(|v| v != now_lit.to_string()) {
        cx.set_props(pos, &[("lit", if now_lit { "true" } else { "false" })]);
    }
}

struct JuiceRecipe {
    ingredients: Vec<mod_api::Ingredient>,
    result: ItemStack,
}
fn juice_recipes(host: &dyn Host) -> Arc<Vec<JuiceRecipe>> {
    cached(&JUICE_RECIPES, host, "expandeddelight:juicing", |list| {
        let mut out = Vec::new();
        for r in list.iter() {
            let j = &r.json;
            let Some(result) = host.recipe_result(&j["result"].to_string()).ok().flatten() else { continue };
            let Some(ingredients) = j["ingredients"]
                .as_array()
                .and_then(|a| a.iter().map(|i| ingredient(host, i)).collect::<Option<Vec<_>>>())
            else {
                continue;
            };
            if ingredients.len() == 2 {
                out.push(JuiceRecipe { ingredients, result });
            }
        }
        out
    })
}

/// `JuicerBlockEntity.tick`: 200 ticks per juice (the fruit in slot 0, its partner in slot 1), into
/// slot 3, then bottled from slot 2 into slot 4.
fn juicer_tick(cx: &mut Cx<'_>, e: &mut mod_api::item::containers::ModBlockEntityFields) {
    const MAX: i32 = 200;
    sized(&mut e.items, 5);
    let mut progress = nbt_int(&e.nbt, "juicer.progress");
    let recipes = juice_recipes(cx.h());
    let found = match (&e.items[0], &e.items[1]) {
        (Some(a), Some(b)) => recipes.iter().find(|r| r.ingredients[0].test(cx.h(), a) && r.ingredients[1].test(cx.h(), b)),
        _ => None,
    };
    let fits = |r: &JuiceRecipe, slot: &Option<ItemStack>| slot.as_ref().is_none_or(|s| s.same_item(&r.result) && s.count() < cx.max_stack(s.item()));
    match found.filter(|r| fits(r, &e.items[3])) {
        Some(r) => {
            progress += 1;
            if progress > MAX {
                take_one(&mut e.items[0]);
                take_one(&mut e.items[1]);
                match &mut e.items[3] {
                    Some(s) => {
                        let _ = s.grow(1);
                    }
                    slot => *slot = with_count(&r.result, 1),
                }
                progress = 0;
            }
        }
        None => progress = 0,
    }
    // useStoredBottleOnJuice
    if let (Some(juice), Some(bottle)) = (e.items[3].clone(), e.items[2].clone())
        && cx.id(bottle.item()) == "minecraft:glass_bottle"
    {
        let max = cx.max_stack(juice.item());
        let have = e.items[4].as_ref().map_or(0, ItemStack::count);
        if e.items[4].as_ref().is_none_or(|o| o.same_item(&juice)) && have < max {
            let n = juice.count().min(bottle.count()).min(max - have);
            e.items[4] = with_count(&juice, have + n);
            e.items[3] = with_count(&juice, juice.count() - n);
            e.items[2] = with_count(&bottle, bottle.count() - n);
        }
    }
    put(&mut e.nbt, "juicer.progress", Tag::Int(progress));
    let data = vec![progress, MAX];
    if e.data != data {
        e.data = data;
    }
}

/// `SkilletBlock.getSkilletCookingTime`.
fn skillet_time(original: u32) -> i32 {
    let original = if original > 0 { original as i32 } else { 600 };
    let result = ((original / 20) as f32 * 0.2) as i32 * 20;
    result.clamp(60.min(original), original)
}

/// `SkilletBlockEntity.cookingTick`.
fn skillet_tick(cx: &mut Cx<'_>, pos: [i32; 3], e: &mut mod_api::item::containers::ModBlockEntityFields) {
    sized(&mut e.items, 1);
    let mut cook = nbt_int(&e.nbt, "CookTime");
    let total = nbt_int(&e.nbt, "CookTimeTotal");
    let mut changed = false;
    if heated(cx, pos, true) {
        match e.items[0].clone() {
            None => cook = 0,
            Some(s) => {
                cook += 1;
                if cook >= total
                    && let Some((result, _)) = cx.campfire(&s)
                {
                    let facing = cx.prop(pos, "facing").unwrap_or_else(|| "north".into());
                    let (dx, dz) = step(clockwise(&facing));
                    let at = [f64::from(pos[0]) + 0.5, f64::from(pos[1]) + 0.3, f64::from(pos[2]) + 0.5];
                    cx.spawn(result, at, [f64::from(dx) * 0.08, 0.25, f64::from(dz) * 0.08]);
                    cook = 0;
                    e.items[0] = s.shrink(1);
                    changed = true;
                }
            }
        }
    } else if cook > 0 {
        cook = (cook - 2).clamp(0, total.max(0));
    }
    put(&mut e.nbt, "CookTime", Tag::Int(cook));
    if changed {
        put(&mut e.nbt, "View", view_of(cx.h(), &e.items));
        cx.host.block_entity_sync(pos);
    }
}

/// `BasketBlockEntity.pushItemsTick`: items in front of its opening go in, every 8 ticks at most.
fn basket_tick(cx: &mut Cx<'_>, pos: [i32; 3], e: &mut mod_api::item::containers::ModBlockEntityFields) {
    sized(&mut e.items, 27);
    let cooldown = nbt_int(&e.nbt, "TransferCooldown") - 1;
    if cooldown > 0 {
        put(&mut e.nbt, "TransferCooldown", Tag::Int(cooldown));
        return;
    }
    put(&mut e.nbt, "TransferCooldown", Tag::Int(0));
    let st = cx.st(pos);
    if st.get("enabled") == Some("false") {
        return;
    }
    // The block's own space and the half block beyond its opening.
    let facing = st.get("facing").unwrap_or("up").to_string();
    let (dx, dz) = step(&facing);
    let dy = match facing.as_str() {
        "up" => 1,
        "down" => -1,
        _ => 0,
    };
    let p = pos.map(f64::from);
    let grow = |d: i32, lo: f64| if d < 0 { (lo - 0.5, lo + 1.0) } else if d > 0 { (lo, lo + 1.5) } else { (lo, lo + 1.0) };
    let (x0, x1) = grow(dx, p[0]);
    let (y0, y1) = grow(dy, p[1]);
    let (z0, z1) = grow(dz, p[2]);
    let Some(found) = cx.lv.entities_in(cx.h(), [x0, y0, z0], [x1, y1, z1]) else { return };
    let mut took = false;
    for ent in found {
        let Some(item) = cx.h().item_entity(ent) else { continue };
        if item.pickup_delay(cx.h()).unwrap_or(0) > 0 {
            continue;
        }
        let Some(stack) = item.stack(cx.h()) else { continue };
        let max = cx.max_stack(stack.item());
        let mut left = stack.count();
        for slot in e.items.iter_mut() {
            if left == 0 {
                break;
            }
            if slot.is_none() {
                *slot = with_count(&stack, left);
                left = 0;
            } else if let Some(s) = slot.as_mut()
                && s.same_item(&stack)
                && s.same_components(&stack)
                && s.count() < max
            {
                let n = left.min(max - s.count());
                let _ = s.grow(n);
                left -= n;
            }
        }
        if left == stack.count() {
            continue;
        }
        took = true;
        match with_count(&stack, left) {
            None => {
                let _ = ent.discard(cx.host);
            }
            Some(rest) => {
                let _ = item.set_stack(cx.host, rest);
            }
        }
    }
    if took {
        put(&mut e.nbt, "TransferCooldown", Tag::Int(8));
    }
}

// ---------------------------------------------------------------------------------------------
// Drawing (client)
// ---------------------------------------------------------------------------------------------

/// `StoveBlockEntity.getStoveItemOffset`.
const STOVE_OFFSETS: [[f32; 2]; 6] = [[0.3, 0.2], [0.0, 0.2], [-0.3, 0.2], [0.3, -0.2], [0.0, -0.2], [-0.3, -0.2]];

pub(crate) fn submit(host: &dyn mod_api::ClientHost, ev: &mut mod_api::BlockEntitySubmitEvent<'_>) {
    use mod_api::ItemDisplayContext;
    let Some(k) = kind_of_entity(ev.entity.id()) else { return };
    let st = host.block_state_string(ev.state).map(|t| St::parse(&t)).unwrap_or_default();
    let facing = st.get("facing").unwrap_or("north").to_string();
    let pos = ev.pos;
    let seed = pos[0].wrapping_mul(31).wrapping_add(pos[1]).wrapping_mul(31).wrapping_add(pos[2]);
    let y = [0.0, 1.0, 0.0];
    let x = [1.0, 0.0, 0.0];
    let o = [0.0; 3];
    let nbt = &ev.entity.nbt;
    let draw = &mut *ev.draw;
    match k {
        // StoveRenderer
        Kind::Stove => {
            let dir = opposite(&facing);
            for (i, s) in view_items(host, nbt, 6).iter().enumerate() {
                let Some(s) = s else { continue };
                draw.push();
                draw.translate([0.5, 1.02, 0.5]);
                draw.rotate_about(y, -y_rot(dir), o);
                draw.rotate_about(x, 90.0, o);
                draw.translate([STOVE_OFFSETS[i][0], STOVE_OFFSETS[i][1], 0.0]);
                draw.scale([0.375; 3]);
                draw.item(s, ItemDisplayContext::Fixed, None, seed.wrapping_add(i as i32));
                draw.pop();
            }
            draw.keep();
        }
        // SkilletRenderer (without the flip)
        Kind::Skillet => {
            let Some(s) = view_items(host, nbt, 1).into_iter().next().flatten() else {
                draw.keep();
                return;
            };
            let n = match s.count() {
                49.. => 5,
                33.. => 4,
                17.. => 3,
                2.. => 2,
                _ => 1,
            };
            let mut r = mod_api::LegacyRandom::new(i64::from(s.item().id(host).len() as i32 * 7919 + s.damage() as i32));
            for i in 0..n {
                let xo = (mod_api::RandomSource::next_float(&mut r) * 2.0 - 1.0) * 0.15 * 0.5;
                let zo = (mod_api::RandomSource::next_float(&mut r) * 2.0 - 1.0) * 0.15 * 0.5;
                draw.push();
                draw.translate([0.5 + xo, 0.1 + 0.03 * (i as f32 + 1.0), 0.5 + zo]);
                draw.rotate_about(y, -y_rot(&facing), o);
                draw.rotate_about(x, 90.0, o);
                draw.scale([0.5; 3]);
                draw.item(&s, ItemDisplayContext::Fixed, None, seed);
                draw.pop();
            }
            draw.keep();
        }
        // CuttingBoardRenderer
        Kind::Board => {
            let Some(s) = view_items(host, nbt, 1).into_iter().next().flatten() else {
                draw.keep();
                return;
            };
            let dir = opposite(&facing);
            let carved = nbt_int(nbt, "IsItemCarved") != 0;
            let id = s.item().id(host);
            let block = host.block_key(&id).is_some() && !item_tag(host, "farmersdelight:flat_on_cutting_board", s.item());
            draw.push();
            if carved {
                draw.translate([0.5, 0.23, 0.5]);
                draw.rotate_about(y, -y_rot(dir) + 180.0, o);
                let angle = if item_tag(host, "minecraft:pickaxes", s.item()) || item_tag(host, "minecraft:hoes", s.item()) {
                    225.0
                } else if id == "minecraft:trident" {
                    135.0
                } else {
                    180.0
                };
                draw.rotate_about([0.0, 0.0, 1.0], angle, o);
                draw.scale([0.6; 3]);
            } else if block {
                draw.translate([0.5, 0.27, 0.5]);
                draw.rotate_about(y, -y_rot(dir), o);
                draw.scale([0.8; 3]);
            } else {
                draw.translate([0.5, 0.08, 0.5]);
                draw.rotate_about(y, -y_rot(dir), o);
                draw.rotate_about(x, 90.0, o);
                draw.scale([0.6; 3]);
            }
            draw.item(&s, ItemDisplayContext::Fixed, None, seed);
            draw.pop();
            draw.keep();
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------------------------
// Screens (CookingPotMenu/Screen, AbstractFurnaceScreen, JuicerScreen)
// ---------------------------------------------------------------------------------------------

pub(crate) fn screen(block: &str) -> Option<mod_api::Screen> {
    let base = |slots: Vec<Option<[f32; 2]>>| mod_api::Screen {
        width: 176.0,
        height: 166.0,
        slots,
        inventory_y: 84.0,
        inventory_x: 8.0,
        own_labels: false,
        own_background: true,
        ..Default::default()
    };
    match kind_of_block(block)? {
        // AbstractFurnaceMenu: input, fuel, result.
        Kind::Furnace => Some(base(vec![Some([56.0, 17.0]), Some([56.0, 53.0]), Some([116.0, 35.0])])),
        // JuicerScreenHandler: the fruit, its partner, bottles, (the juice, kept out of reach:
        // LockedSlot) and the bottled juice.
        Kind::Juicer => Some(base(vec![Some([48.0, 25.0]), Some([48.0, 43.0]), Some([80.0, 60.0]), None, Some([116.0, 60.0])])),
        Kind::Pot => {
            let mut slots = Vec::new();
            for row in 0..2 {
                for col in 0..3 {
                    slots.push(Some([30.0 + col as f32 * 18.0, 17.0 + row as f32 * 18.0]));
                }
            }
            slots.push(Some([124.0, 26.0]));
            slots.push(Some([92.0, 55.0]));
            slots.push(Some([124.0, 55.0]));
            Some(base(slots))
        }
        _ => None,
    }
}

pub(crate) fn screen_draw(ev: &mut mod_api::ScreenDrawEvent<'_>) {
    let [ox, oy] = ev.origin;
    let data = ev.data;
    let get = |i: usize| data.get(i).copied().unwrap_or(0);
    let draw = &mut *ev.draw;
    // AbstractFurnaceScreen.renderBg: the flame (lit time left) and the arrow (cooking).
    if let Some((_, name, _)) = nd_furnace(ev.block) {
        draw.sprite(&format!("gui/container/{name}"), [0.0, 0.0, 176.0, 166.0], [ox, oy, 176.0, 166.0], None);
        let (lit, lit_total, cook, cook_total) = (get(0), get(1), get(2), get(3));
        if lit > 0 {
            let total = if lit_total == 0 { 200 } else { lit_total };
            let h = (lit as f32 / total as f32 * 13.0).clamp(0.0, 13.0).ceil() + 1.0;
            draw.sprite("gui/sprites/container/furnace/lit_progress", [0.0, 14.0 - h, 14.0, h], [ox + 56.0, oy + 36.0 + 14.0 - h, 14.0, h], None);
        }
        if cook > 0 && cook_total > 0 {
            let w = (cook as f32 / cook_total as f32 * 24.0).clamp(0.0, 24.0).ceil();
            draw.sprite("gui/sprites/container/furnace/burn_progress", [0.0, 0.0, w, 16.0], [ox + 79.0, oy + 34.0, w, 16.0], None);
        }
        return;
    }
    match kind_of_block(ev.block) {
        // JuicerScreen.renderBg: the arrow, 22 pixels at full progress.
        Some(Kind::Juicer) => {
            let tex = "expandeddelight:gui/juicer_gui";
            draw.sprite(tex, [0.0, 0.0, 176.0, 166.0], [ox, oy, 176.0, 166.0], None);
            let (p, max) = (get(0), get(1));
            let w = if p != 0 && max != 0 { p * 22 / max } else { 0 };
            if w > 0 {
                draw.sprite(tex, [177.0, 0.0, w as f32, 17.0], [ox + 79.0, oy + 35.0, w as f32, 17.0], None);
            }
        }
        // CookingPotScreen.renderBg: the flame when heated, the arrow.
        Some(Kind::Pot) => {
            let tex = "farmersdelight:gui/cooking_pot";
            draw.sprite(tex, [0.0, 0.0, 176.0, 166.0], [ox, oy, 176.0, 166.0], None);
            if get(2) != 0 {
                draw.sprite(tex, [176.0, 0.0, 17.0, 15.0], [ox + 47.0, oy + 55.0, 17.0, 15.0], None);
            }
            let (cook, total) = (get(0), get(1));
            let l = if cook != 0 && total != 0 { cook * 24 / total } else { 0 };
            if l > 0 {
                let w = (l + 1) as f32;
                draw.sprite(tex, [176.0, 15.0, w, 17.0], [ox + 89.0, oy + 25.0, w, 17.0], None);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------------------------
// Cabinets open and close (CabinetBlockEntity.ContainerOpenersCounter)
// ---------------------------------------------------------------------------------------------

pub(crate) fn viewers(cx: &mut Cx<'_>, pos: [i32; 3], viewers: u32, previous: u32) {
    if kind_of_block(&cx.name(pos)) != Some(Kind::Cabinet) {
        return;
    }
    let open = viewers > 0;
    if open == (previous > 0) {
        return;
    }
    cx.set_props(pos, &[("open", if open { "true" } else { "false" })]);
    let facing = cx.prop(pos, "facing").unwrap_or_else(|| "north".into());
    let (dx, dz) = step(&facing);
    let at = [f64::from(pos[0]) + 0.5 + f64::from(dx) * 0.5, f64::from(pos[1]) + 0.5, f64::from(pos[2]) + 0.5 + f64::from(dz) * 0.5];
    let pitch = cx.rf() * 0.1 + 0.9;
    let sound = if open { "farmersdelight:block.cabinet.open" } else { "farmersdelight:block.cabinet.close" };
    cx.play(sound, at, 0.5, pitch);
}

// ---------------------------------------------------------------------------------------------
// Right clicks on blocks
// ---------------------------------------------------------------------------------------------

/// On the clicking client: the blocks whose template would use the click on its own (the
/// stove's furnace menu, a pie's or feast's cake bite) don't, as on the server.
pub(crate) fn client_not_block(name: &str) -> bool {
    short(name).is_some_and(|n| n == "stove" || is_pie(n) || feast(n).is_some() || n == STUFFED_HOGLIN)
}

/// Whether the right click on `name` could be ours (cheap; every click on every block asks).
pub(crate) fn clickable(name: &str) -> bool {
    short(name).is_some()
}

/// What a right click on one of Farmer's Delight's blocks comes to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Click {
    /// Used: nothing else acts.
    Taken,
    /// Not used, and the block's template must not act either (the stove's furnace menu, a pie's
    /// cake bite): the held item's own use goes on (`InteractionResult.PASS` on a Java block
    /// whose template would use the click), so a pot is placed on a stove.
    NotBlock,
    /// Not ours: the game goes on as it would.
    Pass,
}

/// `use` of Farmer's Delight's blocks.
pub(crate) fn right_click(cx: &mut Cx<'_>, at: [i32; 3]) -> Click {
    let name = cx.name(at);
    let Some(n) = short(&name).map(str::to_string) else { return Click::Pass };
    let n = n.as_str();
    let main = cx.hand == Hand::Main;
    let sneak = cx.sneaking();
    let held = cx.held();
    // Blocks whose template would act on its own (a furnace's menu, a cake's bite) take every
    // click their Java block would not use.
    let template_acts = n == "stove" || is_pie(n) || feast(n).is_some() || n == STUFFED_HOGLIN;
    let answer = |taken: bool| {
        if taken {
            Click::Taken
        } else if template_acts {
            Click::NotBlock
        } else {
            Click::Pass
        }
    };
    if !main {
        return answer(false);
    }
    // CuttingBoardBlock.ToolCarvingEvent: sneaking with a tool carves it onto an empty board.
    if n == "cutting_board" && sneak {
        return answer(held.is_some_and(|h| board_carve(cx, at, &h)));
    }
    // isSecondaryUseActive with something in a hand: the block isn't used.
    let hands_full = cx.hand_item(Hand::Main).is_some() || cx.hand_item(Hand::Off).is_some();
    if sneak && hands_full {
        return Click::Pass;
    }
    let taken = match n {
        "stove" => stove_use(cx, at, held.as_ref()),
        CRIMSON_COLONY | WARPED_COLONY => colony_use(cx, at, n, held.as_ref()),
        MORTAR => {
            mortar_use(cx, at, held.as_ref());
            true
        }
        PROPELPLANT => propelplant_use(cx, at),
        STUFFED_HOGLIN => {
            hoglin_use(cx, at, held.as_ref());
            true
        }
        "cooking_pot" => pot_use(cx, at, held.as_ref(), sneak),
        "skillet" => skillet_use(cx, at, held.as_ref()),
        "cutting_board" => board_use(cx, at, held.as_ref()),
        "tomatoes" => tomato_use(cx, at),
        "brown_mushroom_colony" | "red_mushroom_colony" => colony_use(cx, at, n, held.as_ref()),
        _ if is_pie(n) => {
            pie_use(cx, at, n, held.as_ref());
            true
        }
        _ if feast(n).is_some() => {
            feast_use(cx, at, n, held.as_ref());
            true
        }
        _ => false,
    };
    answer(taken)
}

/// `StoveBlock.use`: true when used (`PASS` is false).
fn stove_use(cx: &mut Cx<'_>, at: [i32; 3], held: Option<&ItemStack>) -> bool {
    let c = centre(at);
    let lit = cx.prop(at, "lit").is_some_and(|v| v == "true");
    if let Some(h) = held {
        let item = cx.id(h.item());
        if lit {
            // BlackstoneStoveBlock.use: soul sand or soil makes the fire a soul fire.
            if (item == "minecraft:soul_sand" || item == "minecraft:soul_soil") && cx.prop(at, "soul").as_deref() == Some("false") {
                cx.play("minecraft:entity.generic.extinguish_fire", c, 1.0, 1.0);
                cx.set_props(at, &[("soul", "true")]);
                cx.play("minecraft:entity.generic.explode", c, 0.2, 2.6);
                cx.play("minecraft:particle.soul_escape", c, 1.5, 2.6);
                cx.use_one();
                return true;
            }
            if cx.item_tag("minecraft:shovels", h.item()) {
                cx.set_props(at, &[("lit", "false")]);
                cx.set_props(at, &[("soul", "false")]);
                cx.play("minecraft:block.fire.extinguish", c, 0.5, 2.6);
                cx.damage_held(1);
                return true;
            }
            if item == "minecraft:water_bucket" {
                cx.play("minecraft:entity.generic.extinguish_fire", c, 1.0, 1.0);
                cx.set_props(at, &[("lit", "false")]);
                cx.set_props(at, &[("soul", "false")]);
                cx.play("minecraft:block.fire.extinguish", c, 0.5, 2.6);
                if !cx.creative() {
                    let bucket = cx.stack("minecraft:bucket", 1);
                    cx.set_held(bucket);
                }
                return true;
            }
        } else {
            if item == "minecraft:flint_and_steel" {
                let pitch = cx.rf() * 0.4 + 0.8;
                cx.play("minecraft:item.flintandsteel.use", c, 1.0, pitch);
                cx.set_props(at, &[("lit", "true")]);
                cx.damage_held(1);
                return true;
            }
            if item == "minecraft:fire_charge" {
                let pitch = (cx.rf() - cx.rf()) * 0.2 + 1.0;
                cx.play("minecraft:item.firecharge.use", c, 1.0, pitch);
                cx.set_props(at, &[("lit", "true")]);
                cx.use_one();
                return true;
            }
        }
    }
    let Some(h) = held else { return false };
    let Some(items) = cx.be_items(at, 6) else { return false };
    let Some(slot) = items.iter().position(Option::is_none) else { return false };
    if stove_blocked(cx, at) {
        return false;
    }
    let Some((_, time)) = cx.campfire(h) else { return false };
    let mut nbt = cx.be_nbt(at);
    let mut totals = nbt_ints(&nbt, "CookingTotalTimes", 6);
    let mut times = nbt_ints(&nbt, "CookingTimes", 6);
    totals[slot] = time as i32;
    times[slot] = 0;
    if !cx.be_set_item(at, slot, with_count(h, 1)) {
        return true;
    }
    cx.use_one();
    nbt = Compound::new();
    nbt.insert("CookingTotalTimes".into(), Tag::IntArray(totals));
    nbt.insert("CookingTimes".into(), Tag::IntArray(times));
    cx.be_merge(at, nbt);
    cx.refresh_view(at, 6, &[]);
    true
}

/// `CookingPotBlock.use`: the handle, or a serving into the held container. False lets the
/// pot's menu open.
fn pot_use(cx: &mut Cx<'_>, at: [i32; 3], held: Option<&ItemStack>, sneak: bool) -> bool {
    let c = centre(at);
    let Some(h) = held else {
        if sneak {
            let support = cx.prop(at, "support").unwrap_or_default();
            let next = if support == "handle" {
                if tray_below(cx, at) { "tray" } else { "none" }
            } else {
                "handle"
            };
            cx.set_props(at, &[("support", next)]);
            cx.play("minecraft:block.lantern.place", c, 0.7, 1.0);
            return true;
        }
        return false;
    };
    // useHeldItemOnMeal
    let Some(items) = cx.be_items(at, 9) else { return false };
    let Some(meal) = items[MEAL].clone() else { return false };
    let container = nbt_str(&cx.be_nbt(at), "Container").or_else(|| cx.remainder(&meal).map(|s| cx.id(s.item())));
    let valid = match &container {
        Some(c) => cx.id(h.item()) == *c,
        None => h.same_item(&meal),
    };
    if !valid || !cx.be_set_item(at, MEAL, meal.clone().shrink(1)) {
        return false;
    }
    cx.use_one();
    if let Some(one) = with_count(&meal, 1) {
        cx.give_or_drop(one);
    }
    cx.play("minecraft:item.armor.equip_generic", c, 1.0, 1.0);
    true
}

/// `SkilletBlock.use`.
fn skillet_use(cx: &mut Cx<'_>, at: [i32; 3], held: Option<&ItemStack>) -> bool {
    let c = centre(at);
    let Some(items) = cx.be_items(at, 1) else { return false };
    let stored = items[0].clone();
    let Some(h) = held else {
        // An empty hand takes the whole stack.
        let Some(s) = stored else { return true };
        cx.be_set_item(at, 0, None);
        if !cx.creative() {
            cx.set_held(Some(s));
        }
        cx.refresh_view(at, 1, &[]);
        return true;
    };
    let is_heated = heated(cx, at, true);
    if cx.item_tag("minecraft:shovels", h.item()) && is_heated && stored.is_some() {
        let now = cx.lv.game_time(cx.h()).unwrap_or(0);
        if nbt_long(&cx.be_nbt(at), "FlipTime") + 12 < now {
            cx.refresh_view(at, 1, &[("FlipTime", Tag::Long(now))]);
        }
        return true;
    }
    // addItemToCook
    let Some((_, time)) = cx.campfire(h) else {
        cx.tell(Text::translate("farmersdelight.block.skillet.invalid_item", vec![]));
        return false;
    };
    let max = cx.max_stack(h.item());
    let fits = match &stored {
        None => h.count().min(max),
        Some(s) if s.same_item(h) && s.same_components(h) => h.count().min(max.saturating_sub(s.count())),
        Some(_) => 0,
    };
    if fits == 0 {
        return false;
    }
    let was_empty = stored.is_none();
    let added = with_count(h, stored.as_ref().map_or(0, ItemStack::count) + fits);
    if !cx.be_set_item(at, 0, added) {
        return false;
    }
    if was_empty && is_heated {
        cx.play("farmersdelight:block.skillet.add_food", c, 0.8, 1.0);
    }
    if !cx.creative() {
        let rest = h.clone().shrink(fits);
        cx.set_held(rest);
    }
    cx.play("minecraft:block.lantern.place", c, 0.7, 1.0);
    cx.refresh_view(at, 1, &[("CookTimeTotal", Tag::Int(skillet_time(time))), ("CookTime", Tag::Int(0))]);
    true
}

/// Tools that can be carved onto a board: `TieredItem`, `TridentItem`, `ShearsItem`.
fn is_tool(cx: &Cx<'_>, item: ItemKey) -> bool {
    ["minecraft:swords", "minecraft:axes", "minecraft:pickaxes", "minecraft:shovels", "minecraft:hoes"].iter().any(|t| cx.item_tag(t, item))
        || cx.is_knife(item)
        || cx.id(item) == "minecraft:trident"
        || cx.is_shears(item)
}

/// `CuttingBoardBlockEntity.carveToolOnBoard`.
fn board_carve(cx: &mut Cx<'_>, at: [i32; 3], held: &ItemStack) -> bool {
    if !is_tool(cx, held.item()) {
        return false;
    }
    let Some(items) = cx.be_items(at, 1) else { return false };
    if items[0].is_some() || !cx.be_set_item(at, 0, with_count(held, 1)) {
        return false;
    }
    cx.use_one();
    cx.play("minecraft:block.wood.place", at.map(f64::from), 1.0, 0.8);
    cx.refresh_view(at, 1, &[("IsItemCarved", Tag::Byte(1))]);
    true
}

/// `CuttingBoardBlock.use`.
fn board_use(cx: &mut Cx<'_>, at: [i32; 3], held: Option<&ItemStack>) -> bool {
    let Some(items) = cx.be_items(at, 1) else { return false };
    let corner = at.map(f64::from);
    match (&items[0], held) {
        (None, None) => false,
        (None, Some(h)) => {
            // An off hand item that isn't equipment keeps the main hand's item off the board,
            // unless that is a block.
            if let Some(off) = cx.hand_item(Hand::Off)
                && !cx.item_tag("farmersdelight:offhand_equipment", off.item())
                && cx.h().block_key(&cx.id(h.item())).is_none()
            {
                return false;
            }
            if !cx.be_set_item(at, 0, with_count(h, 1)) {
                return false;
            }
            cx.use_one();
            cx.play("minecraft:block.wood.place", corner, 1.0, 0.8);
            cx.refresh_view(at, 1, &[("IsItemCarved", Tag::Byte(0))]);
            true
        }
        (Some(stored), Some(tool)) => {
            board_cut(cx, at, stored.clone(), tool.clone());
            true
        }
        (Some(stored), None) => {
            let stored = stored.clone();
            cx.be_set_item(at, 0, None);
            if !cx.creative() {
                cx.give_or_drop(stored);
            }
            cx.play("minecraft:block.wood.hit", corner, 0.25, 0.5);
            cx.refresh_view(at, 1, &[]);
            true
        }
    }
}

/// `CuttingBoardRecipe.getTool().test`: an item tag or a tool action.
fn tool_matches(cx: &Cx<'_>, tool: &serde_json::Value, item: ItemKey) -> bool {
    if let Some(t) = tool.as_str() {
        return match t.strip_prefix('#') {
            Some("c:shears") => cx.is_shears(item),
            Some("c:tools/knives") => cx.is_knife(item),
            Some(tag) => cx.item_tag(tag, item),
            None => qualified(t) == cx.id(item),
        };
    }
    if let Some(t) = tool["tag"].as_str() {
        return cx.item_tag(t, item);
    }
    if let Some(i) = tool["item"].as_str() {
        return qualified(i) == cx.id(item);
    }
    match tool["action"].as_str().unwrap_or("") {
        "axe_dig" | "axe_strip" => cx.item_tag("minecraft:axes", item),
        "pickaxe_dig" => cx.item_tag("minecraft:pickaxes", item),
        "shovel_dig" | "shovel_flatten" => cx.item_tag("minecraft:shovels", item),
        "hoe_dig" | "hoe_till" => cx.item_tag("minecraft:hoes", item),
        "shears_dig" | "shears_harvest" => cx.is_shears(item),
        "sword_dig" => cx.item_tag("minecraft:swords", item),
        _ => false,
    }
}

/// `CuttingBoardRecipe`: its ingredient, and its file for the tool, results and sound.
struct CutRecipe {
    ingredient: mod_api::Ingredient,
    json: Arc<serde_json::Value>,
}
fn cut_recipes(host: &dyn Host) -> Arc<Vec<CutRecipe>> {
    cached(&CUT_RECIPES, host, "farmersdelight:cutting", |list| {
        list.iter()
            .filter_map(|r| {
                let j = &r.json;
                let first = j["ingredients"].get(0).or(j["ingredient"].get(0))?;
                Some(CutRecipe { ingredient: ingredient(host, first)?, json: Arc::clone(&r.json) })
            })
            .collect()
    })
}

/// `CuttingBoardBlockEntity.processStoredItemUsingTool`.
fn board_cut(cx: &mut Cx<'_>, at: [i32; 3], stored: ItemStack, tool: ItemStack) {
    if nbt_int(&cx.be_nbt(at), "IsItemCarved") != 0 {
        return;
    }
    let recipes = cut_recipes(cx.h());
    let mut any = false;
    let mut found = None;
    for r in recipes.iter() {
        if !r.ingredient.test(cx.h(), &stored) {
            continue;
        }
        any = true;
        if tool_matches(cx, &r.json["tool"], tool.item()) {
            found = Some(Arc::clone(&r.json));
            break;
        }
    }
    let Some(j) = found else {
        let key = if any { "farmersdelight.block.cutting_board.invalid_tool" } else { "farmersdelight.block.cutting_board.invalid_item" };
        cx.tell(Text::translate(key, vec![]));
        return;
    };
    let facing = cx.prop(at, "facing").unwrap_or_else(|| "north".into());
    let (dx, dz) = step(counter_clockwise(&facing));
    for res in j["result"].as_array().into_iter().flatten() {
        let Some(id) = res["id"].as_str().or(res["item"].as_str()).or(res["item"]["id"].as_str()) else { continue };
        let count = res["count"].as_u64().or(res["item"]["count"].as_u64()).unwrap_or(1) as u32;
        let chance = res["chance"].as_f64().unwrap_or(1.0);
        let mut out = count;
        for _ in 0..count {
            if f64::from(cx.rf()) > chance + FORTUNE_BONUS * 0.0 {
                out -= 1;
            }
        }
        if out == 0 {
            continue;
        }
        let Some(stack) = cx.stack(&qualified(id), out) else { continue };
        let spot = [f64::from(at[0]) + 0.5 + f64::from(dx) * 0.2, f64::from(at[1]) + 0.2, f64::from(at[2]) + 0.5 + f64::from(dz) * 0.2];
        cx.spawn(stack, spot, [f64::from(dx) * 0.2, 0.0, f64::from(dz) * 0.2]);
    }
    cx.damage_held(1);
    // playProcessingSound
    let c = centre(at);
    if let Some(s) = j["sound"].as_str().or(j["sound"]["sound_id"].as_str()) {
        cx.play(s, c, 1.0, 1.0);
    } else if cx.is_shears(tool.item()) {
        cx.play("minecraft:entity.sheep.shear", c, 1.0, 1.0);
    } else if cx.is_knife(tool.item()) {
        cx.play("farmersdelight:block.cutting_board.knife", c, 0.8, 1.0);
    } else if let Some(sound) = cx
        .h()
        .block_state_id(&cx.id(stored.item()))
        .and_then(|s| cx.h().block_states().sound(s))
    {
        let name = sound.break_sound.clone();
        cx.play(&name, c, 1.0, 0.8);
    } else {
        cx.play("minecraft:block.wood.break", c, 1.0, 0.8);
    }
    cx.be_set_item(at, 0, None);
    cx.refresh_view(at, 1, &[]);
}

/// `TomatoVineBlock.use`: picking ripe tomatoes.
fn tomato_use(cx: &mut Cx<'_>, at: [i32; 3]) -> bool {
    let st = cx.st(at);
    if st.int("age") < 3 {
        return false;
    }
    let n = 1 + cx.ri(2) as u32;
    if let Some(s) = cx.stack("farmersdelight:tomato", n) {
        cx.pop(at, s);
    }
    if cx.rf() < 0.05
        && let Some(s) = cx.stack("farmersdelight:rotten_tomato", 1)
    {
        cx.pop(at, s);
    }
    let pitch = 0.8 + cx.rf() * 0.4;
    cx.play("farmersdelight:block.tomato_bush.pick_tomatoes", centre(at), 1.0, pitch);
    cx.set_props_quiet(at, &[("age", "0")]);
    true
}

/// `MushroomColonyBlock.use`: shears take a mushroom.
fn colony_use(cx: &mut Cx<'_>, at: [i32; 3], n: &str, held: Option<&ItemStack>) -> bool {
    let age = cx.st(at).int("age");
    if age == 0 || !held.is_some_and(|h| cx.id(h.item()) == "minecraft:shears") {
        return false;
    }
    let mushroom = match n {
        CRIMSON_COLONY => "minecraft:crimson_fungus",
        WARPED_COLONY => "minecraft:warped_fungus",
        _ if n.starts_with("brown") => "minecraft:brown_mushroom",
        _ => "minecraft:red_mushroom",
    };
    if let Some(s) = cx.stack(mushroom, 1) {
        cx.pop(at, s);
    }
    cx.play("minecraft:entity.mooshroom.shear", centre(at), 1.0, 1.0);
    cx.set_props_quiet(at, &[("age", &(age - 1).to_string())]);
    cx.damage_held(1);
    true
}

fn is_pie(n: &str) -> bool {
    matches!(n, "apple_pie" | "sweet_berry_cheesecake" | "chocolate_pie" | "ends_delight:chorus_fruit_pie")
}

/// `PieBlock.use`: a knife cuts a slice, anything else eats one.
fn pie_use(cx: &mut Cx<'_>, at: [i32; 3], n: &str, held: Option<&ItemStack>) {
    let slice = if n.contains(':') { format!("{n}_slice") } else { format!("{NS}{n}_slice") };
    let bites = cx.st(at).int("bites");
    let c = centre(at);
    let knife = held.is_some_and(|h| cx.is_knife(h.item()));
    if !knife {
        // consumeBite
        let Some(key) = cx.stack(&slice, 1).map(|s| s.item()) else { return };
        let Some(Tag::Compound(food)) = cx.h().item_default_component(key, "minecraft:food") else { return };
        let nutrition = nbt_int(&food, "nutrition");
        let saturation = match food.get(&"saturation".into()) {
            Some(Tag::Float(f)) => f32::from_bits(*f),
            Some(Tag::Double(f)) => f64::from_bits(*f) as f32,
            Some(Tag::Int(i)) => *i as f32,
            Some(Tag::Byte(i)) => f32::from(*i),
            Some(Tag::Short(i)) => f32::from(*i),
            Some(Tag::Long(i)) => *i as f32,
            other => {
                eprintln!("[farmersdelight] pie slice food saturation unreadable: {other:?} in {food:?}");
                0.0
            }
        };
        let Some(player) = cx.player else { return };
        let hungry = player.food(cx.h()).is_some_and(|f| f.level < 20);
        if !cx.creative() && !hungry {
            return;
        }
        // FoodData.eat(nutrition, saturationModifier): saturation = nutrition * modifier * 2.
        let modifier = if nutrition > 0 { saturation / (2.0 * nutrition as f32) } else { 0.0 };
        let _ = player.feed(cx.host, nutrition, modifier);
    }
    if bites < 3 {
        cx.set_props(at, &[("bites", &(bites + 1).to_string())]);
    } else {
        cx.remove(at);
    }
    if knife {
        let yaw = cx.player.and_then(|p| p.entity().rotation(cx.h())).map_or(0.0, |r| r[0]);
        let (dx, dz) = step(opposite(horizontal(yaw)));
        let spot = [c[0], f64::from(at[1]) + 0.3, c[2]];
        if let Some(s) = cx.stack(&slice, 1) {
            cx.spawn(s, spot, [f64::from(dx) * 0.15, 0.05, f64::from(dz) * 0.15]);
        }
        cx.play("minecraft:block.wool.break", c, 0.8, 0.8);
    } else {
        cx.play("minecraft:entity.generic.eat", c, 0.8, 0.8);
    }
}

/// A feast's serving item and whether it leaves its platter behind.
fn feast(n: &str) -> Option<(&'static str, bool)> {
    Some(match n {
        "roast_chicken_block" => ("farmersdelight:roast_chicken", true),
        "stuffed_pumpkin_block" => ("farmersdelight:stuffed_pumpkin", false),
        "honey_glazed_ham_block" => ("farmersdelight:honey_glazed_ham", true),
        "shepherds_pie_block" => ("farmersdelight:shepherds_pie", true),
        "rice_roll_medley_block" => ("farmersdelight:salmon_roll", true),
        // GuardianSoupBlock; EDFeastBlock's; DragonLegBlock (two halves sharing the servings).
        "oceansdelight:guardian_soup" => ("oceansdelight:bowl_of_guardian_soup", true),
        "ends_delight:steamed_dragon_egg_block" => ("ends_delight:steamed_dragon_egg", true),
        "ends_delight:dragon_meat_stew_block" => ("ends_delight:dragon_meat_stew", true),
        "ends_delight:grilled_shulker_block" => ("ends_delight:grilled_shulker", true),
        DRAGON_LEG => ("ends_delight:dragon_leg_with_sauce", true),
        _ => return None,
    })
}
/// `RiceRollMedleyBlock.riceRollServings`.
const MEDLEY: [&str; 8] = [
    "farmersdelight:cod_roll",
    "farmersdelight:cod_roll",
    "farmersdelight:salmon_roll",
    "farmersdelight:salmon_roll",
    "farmersdelight:salmon_roll",
    "farmersdelight:kelp_roll_slice",
    "farmersdelight:kelp_roll_slice",
    "farmersdelight:kelp_roll_slice",
];

/// `FeastBlock.takeServing`.
fn feast_use(cx: &mut Cx<'_>, at: [i32; 3], n: &str, held: Option<&ItemStack>) {
    let Some((serving, leftovers)) = feast(n) else { return };
    let c = centre(at);
    let servings = cx.st(at).int("servings");
    let pair = if two_part(n) { pair_of(cx, at) } else { None };
    if servings == 0 {
        cx.play("minecraft:block.wood.break", c, 0.8, 0.8);
        let _ = cx.lv.destroy_block(cx.host, at, true, None, false);
        if let Some(q) = pair {
            cx.remove(q);
        }
        return;
    }
    let serving = if n == "rice_roll_medley_block" { MEDLEY[(servings as usize - 1).min(7)] } else { serving };
    let Some(serving_stack) = cx.stack(serving, 1) else { return };
    let container = cx.remainder(&serving_stack).map(|s| cx.id(s.item()));
    let ok = container.as_ref().is_none_or(|c| held.is_some_and(|h| cx.id(h.item()) == *c));
    if !ok {
        let c = container.unwrap_or_default();
        let item_name = Text::translate(format!("item.{}", c.replace(':', ".")), vec![]);
        cx.tell(Text::translate("farmersdelight.block.feast.use_container", vec![item_name]));
        return;
    }
    let next = (servings - 1).to_string();
    cx.set_props(at, &[("servings", &next)]);
    if let Some(q) = pair {
        cx.set_props(q, &[("servings", &next)]);
    }
    if container.is_some() {
        cx.use_one();
    }
    cx.give_or_drop(serving_stack);
    if servings - 1 == 0 && !leftovers {
        cx.remove(at);
    }
    cx.play("minecraft:item.armor.equip_generic", c, 1.0, 1.0);
}

/// `DragonLegBlock.getDirectionTowardsOtherPart`, `StuffedHoglinBlock.getDirectionToOther`: the
/// other half, when it is there.
fn pair_of(cx: &Cx<'_>, at: [i32; 3]) -> Option<[i32; 3]> {
    let st = cx.st(at);
    let (dx, dz) = step(st.get("facing")?);
    let k = if st.get("part") == Some("head") { 1 } else { -1 };
    let q = [at[0] + dx * k, at[1], at[2] + dz * k];
    let other = cx.st(q);
    (other.id == st.id && other.get("part") != st.get("part")).then_some(q)
}

/// `StuffedHoglinBlock.use`: a knife cuts the ears (servings 11 and 10), then a bowl takes the
/// snout, four of ham and four of roast; both halves count down together.
fn hoglin_use(cx: &mut Cx<'_>, at: [i32; 3], held: Option<&ItemStack>) {
    let servings = cx.st(at).int("servings");
    let pair = pair_of(cx, at);
    let c = centre(at);
    let set_both = |cx: &mut Cx<'_>, v: i32| {
        cx.set_props(at, &[("servings", &v.to_string())]);
        if let Some(q) = pair {
            cx.set_props(q, &[("servings", &v.to_string())]);
        }
    };
    if servings > 9 {
        if held.is_some_and(|h| cx.is_knife(h.item())) {
            set_both(cx, servings - 1);
            if let Some(s) = cx.stack("nethersdelight:hoglin_ear", 1) {
                cx.spawn(s, at.map(f64::from), [0.0, 0.2, 0.0]);
            }
            cx.play("farmersdelight:block.cutting_board.knife", c, 0.8, 0.8);
        } else {
            cx.tell(Text::translate("nethersdelight.block.feast.use_knife", vec![]));
        }
        return;
    }
    let bowl = held.is_some_and(|h| cx.id(h.item()) == "minecraft:bowl");
    if bowl && servings > 0 {
        let serving = match servings {
            9 => "nethersdelight:plate_of_stuffed_hoglin_snout",
            5..=8 => "nethersdelight:plate_of_stuffed_hoglin_ham",
            _ => "nethersdelight:plate_of_stuffed_hoglin_roast",
        };
        set_both(cx, servings - 1);
        cx.use_one();
        if let Some(s) = cx.stack(serving, 1) {
            cx.give_or_drop(s);
        }
        cx.play("minecraft:item.armor.equip_generic", c, 1.0, 1.0);
        return;
    }
    if servings == 0 {
        cx.play("minecraft:block.wood.break", c, 1.0, 1.0);
        let _ = cx.lv.destroy_block(cx.host, at, true, None, false);
        if let Some(q) = pair {
            cx.remove(q);
        }
    } else {
        let bowl = Text::translate("item.minecraft.bowl", vec![]);
        cx.tell(Text::translate("farmersdelight.block.feast.use_container", vec![bowl]));
    }
}

/// `MortarPestleBlock.use`: sugar cane, raw cinnamon or rock salt ground into two.
fn mortar_use(cx: &mut Cx<'_>, at: [i32; 3], held: Option<&ItemStack>) {
    let out = match held.map(|h| cx.id(h.item())).as_deref() {
        Some("minecraft:sugar_cane") => "minecraft:sugar",
        Some("expandeddelight:raw_cinnamon") => "expandeddelight:ground_cinnamon",
        Some("expandeddelight:salt_rock") => "expandeddelight:ground_salt",
        _ => {
            cx.tell(Text::translate("block.expandeddelight.mortar_and_pestle.pass", vec![]));
            return;
        }
    };
    cx.use_one();
    if let Some(s) = cx.stack(out, 2) {
        cx.pop(at, s);
    }
    cx.play("minecraft:ui.stonecutter.take_result", centre(at), 1.0, 1.0);
}

/// `PropelplantCaneBlock.harvestPearls`: 1-2 propelpearls from a cane with its pearl.
fn propelplant_use(cx: &mut Cx<'_>, at: [i32; 3]) -> bool {
    if cx.prop(at, "pearl").as_deref() != Some("true") {
        return false;
    }
    let n = 1 + cx.ri(2) as u32;
    if let Some(s) = cx.stack("nethersdelight:propelpearl", n) {
        cx.pop(at, s);
    }
    let pitch = 0.8 + cx.rf() * 0.4;
    cx.play("minecraft:block.sweet_berry_bush.pick_berries", centre(at), 1.0, pitch);
    cx.set_props(at, &[("pearl", "false")]);
    true
}

// ---------------------------------------------------------------------------------------------
// Items used on blocks
// ---------------------------------------------------------------------------------------------

/// The crop a seed item plants (an item whose block has another id).
fn seed_crop(item: &str) -> Option<&'static str> {
    Some(match item {
        "farmersdelight:cabbage_seeds" => "farmersdelight:cabbages",
        "farmersdelight:tomato_seeds" => "farmersdelight:budding_tomatoes",
        "farmersdelight:onion" => "farmersdelight:onions",
        // Expanded Delight's ItemNameBlockItems.
        "expandeddelight:asparagus_seeds" => "expandeddelight:asparagus_crop",
        "expandeddelight:chili_pepper_seeds" => "expandeddelight:chili_pepper_crop",
        "expandeddelight:sweet_potato" => "expandeddelight:sweet_potatoes_crop",
        "expandeddelight:peanut" => "expandeddelight:peanut_crop",
        _ => return None,
    })
}

/// Whether `use_on` could take an item used on `block` (cheap; asked for every use).
pub(crate) fn use_on_candidate(item: &str, block: &str) -> bool {
    item.starts_with(NS)
        || item.starts_with("expandeddelight:")
        || item.starts_with("nethersdelight:")
        || item.starts_with("ends_delight:")
        || block == "farmersdelight:rich_soil"
        || block == CINNAMON_LOG
}

/// `useOn` of Farmer's Delight's items, and hoes on rich soil: true when taken.
pub(crate) fn use_on(cx: &mut Cx<'_>, item: &ItemStack, at: [i32; 3], side: [i32; 3]) -> bool {
    let name = cx.name(at);
    let item_id = cx.id(item.item());
    let above = up(at, 1);
    // ChorusSucculentBlock.canBeReplaced / getStateForPlacement: one more succulent, up to 3.
    if name == SUCCULENT && item_id == SUCCULENT && !cx.sneaking() {
        let n = cx.prop(at, "succulent").and_then(|v| v.parse().ok()).unwrap_or(1);
        if n < 3 {
            cx.set_props(at, &[("succulent", &(n + 1).to_string())]);
            cx.use_one();
            cx.play("minecraft:block.fungus.place", centre(at), 1.0, 0.8);
            return true;
        }
    }
    // StandingAndWallBlockItem: the propelplant torch on a wall's side is its own wall torch (the
    // torch template's item would place vanilla's).
    if item_id == "nethersdelight:propelplant_torch" && side[1] == 0 {
        let dir = match (side[0], side[2]) {
            (0, -1) => "north",
            (0, 1) => "south",
            (-1, 0) => "west",
            _ => "east",
        };
        let spot = [at[0] + side[0], at[1], at[2] + side[2]];
        let face = match dir {
            "north" => mod_api::BlockFace::North,
            "south" => mod_api::BlockFace::South,
            "west" => mod_api::BlockFace::West,
            _ => mod_api::BlockFace::East,
        };
        let sturdy = cx.lv.block_face_sturdy(cx.h(), at, face, mod_api::SupportType::Full) == Some(true);
        if cx.replaceable(spot) && sturdy && cx.set_default(spot, "nethersdelight:propelplant_wall_torch", &[("facing", dir)]) {
            cx.play("minecraft:block.wood.place", centre(spot), 1.0, 0.8);
            cx.use_one();
        }
        return true;
    }
    // CinnamonLogBlock.use: an axe strips it into an oak log's stripped form, with 2 raw cinnamon.
    if name == CINNAMON_LOG && cx.item_tag("minecraft:axes", item.item()) {
        let axis = cx.prop(at, "axis").unwrap_or_else(|| "y".into());
        cx.damage_held(1);
        if let Some(s) = cx.stack("expandeddelight:raw_cinnamon", 2) {
            cx.pop(at, s);
        }
        cx.play("minecraft:item.axe.strip", centre(at), 1.0, 1.0);
        cx.set_default(at, "minecraft:stripped_oak_log", &[("axis", &axis)]);
        return true;
    }
    // RichSoilBlock.getToolModifiedState: a hoe tills it, with air above.
    if name == "farmersdelight:rich_soil" && cx.item_tag("minecraft:hoes", item.item()) && side != [0, -1, 0] {
        if !cx.st(above).is_air() {
            return false;
        }
        cx.set_default(at, "farmersdelight:rich_soil_farmland", &[]);
        cx.play("minecraft:item.hoe.till", centre(at), 1.0, 1.0);
        cx.damage_held(1);
        return true;
    }
    // ItemNameBlockItem seeds: on the top of farmland.
    if let Some(crop) = seed_crop(&item_id) {
        let ground_ok = matches!(name.as_str(), "minecraft:farmland" | "farmersdelight:rich_soil_farmland");
        if side != [0, 1, 0] || !ground_ok || !cx.st(above).is_air() {
            return false;
        }
        if !cx.set_default(above, crop, &[]) {
            return false;
        }
        cx.play("minecraft:item.crop.plant", centre(above), 1.0, 1.0);
        cx.use_one();
        return true;
    }
    // RopeItem.updatePlacementContext: a rope on a rope goes to the bottom of it.
    if item_id == "farmersdelight:rope" && name == "farmersdelight:rope" && !cx.sneaking() {
        let mut p = at;
        for _ in 0..256 {
            p[1] -= 1;
            let below = cx.st(p);
            if below.id == "farmersdelight:rope" {
                continue;
            }
            if below.id == "minecraft:lava" {
                return true;
            }
            if cx.replaceable(p) && cx.set_default(p, "farmersdelight:rope", &[]) {
                cx.play("minecraft:block.wool.place", centre(p), 1.0, 0.8);
                cx.use_one();
            }
            return true;
        }
        return true;
    }
    false
}

/// `setPlacedBy`: the two-block feasts' foot, the full mat's other half, the tatami's pair, the
/// propelplant below no longer the top (`TatamiBlock`, `TatamiMatBlock`, `DragonLegBlock`,
/// `StuffedHoglinBlock`, `PropelplantCaneBlock.updateShape`).
pub(crate) fn placed(cx: &mut Cx<'_>, pos: [i32; 3]) {
    let st = cx.st(pos);
    let facing = |d: &str| st.get("facing").unwrap_or(d).to_string();
    match short(&st.id) {
        Some(n @ (DRAGON_LEG | STUFFED_HOGLIN)) if st.get("part") == Some("head") => {
            let f = facing("north");
            let (dx, dz) = step(&f);
            let foot = [pos[0] + dx, pos[1], pos[2] + dz];
            if cx.replaceable(foot) {
                cx.set_default(foot, n, &[("facing", &f), ("part", "foot")]);
            }
        }
        Some(PROPELPLANT) => {
            let below = up(pos, -1);
            if cx.name(below) == PROPELPLANT {
                cx.set_props(below, &[("bud", "false"), ("pearl", "false")]);
            }
        }
        Some("full_tatami_mat") => {
            let f = facing("north");
            let (dx, dz) = step(&f);
            let head = [pos[0] + dx, pos[1], pos[2] + dz];
            if cx.replaceable(head) {
                cx.set_default(head, "farmersdelight:full_tatami_mat", &[("facing", &f), ("part", "head")]);
            }
        }
        Some("tatami") if !cx.sneaking() => {
            let f = facing("down");
            let d: [i32; 3] = match f.as_str() {
                "up" => [0, 1, 0],
                "down" => [0, -1, 0],
                f => {
                    let (x, z) = step(f);
                    [x, 0, z]
                }
            };
            let other = [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
            let o = cx.st(other);
            if o.id == "farmersdelight:tatami" && o.get("paired") == Some("false") {
                cx.set_props(other, &[("facing", opposite(&f)), ("paired", "true")]);
                cx.set_props(pos, &[("paired", "true")]);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------------------------
// Foods and effects
// ---------------------------------------------------------------------------------------------

/// The drinks' and the popsicle's own effects (`affectConsumer`), for the consuming player.
pub(crate) fn consume_effect(cx: &mut Cx<'_>, kind: &str) {
    let Some(player) = cx.player else { return };
    let living = LivingRef::new(player.entity());
    match kind {
        // MilkBottleItem, HotCocoaItem: one random effect goes (a harmful one, for cocoa).
        "farmersdelight:cure_one" | "farmersdelight:cure_one_harmful" => {
            let harmful_only = kind.ends_with("harmful");
            let ids: Vec<mod_api::EffectKey> = living
                .effects(cx.h())
                .unwrap_or_default()
                .iter()
                .map(|e| e.id)
                .filter(|id| !harmful_only || id.category(cx.h()) == Some(mod_api::EffectCategory::Harmful))
                .collect();
            if ids.is_empty() {
                return;
            }
            let pick = ids[cx.ri(ids.len() as i32) as usize];
            let _ = living.remove_effect(cx.host, pick);
        }
        // End's Delight's chorus foods: a chorus fruit teleport when eaten sneaking.
        "ends_delight:sneak_teleport" => {
            if cx.sneaking()
                && let Some(at) = player.entity().position(cx.h())
            {
                let _ = living.random_teleport(cx.host, at, None);
            }
        }
        // MelonJuiceItem
        "farmersdelight:heal" => {
            let _ = living.heal(cx.host, 2.0);
        }
        // MagmaGelatinItem, PropelpearlItem: setSecondsOnFire.
        "nethersdelight:ignite_60" | "nethersdelight:ignite_4" => {
            let secs: i32 = kind.rsplit('_').next().and_then(|v| v.parse().ok()).unwrap_or(0);
            let now = player.entity().fire_ticks(cx.h()).unwrap_or(0);
            let _ = player.entity().set_remaining_fire_ticks(cx.host, now.max(secs * 20));
        }
        // PopsicleItem
        "farmersdelight:extinguish" => {
            let _ = player.entity().clear_fire(cx.host);
        }
        _ => {}
    }
}

/// Comfort and Nourishment (`ComfortEffect`, `NourishmentEffect`), the propelplant's blast
/// (`PropelplantCaneBlock.entityInside`) and the lit stove's burn (`StoveBlock.stepOn`), after
/// the player's tick.
pub(crate) fn player_tick(cx: &mut Cx<'_>) {
    let Some(player) = cx.player else { return };
    let living = LivingRef::new(player.entity());
    if living.alive(cx.h()) != Some(true) {
        return;
    }
    let effect = |cx: &Cx<'_>, id: &str| cx.h().effect_key(id).and_then(|k| living.effect(cx.h(), k));
    let health = living.health(cx.h()).unwrap_or(20.0);
    let max = living.max_health(cx.h()).unwrap_or(20.0);
    let food = player.food(cx.h());
    // ComfortEffect: 1 health every 80 ticks (80 >> amplifier), without regeneration or saturation.
    if let Some(c) = effect(cx, "farmersdelight:comfort")
        && effect(cx, "minecraft:regeneration").is_none()
    {
        let k = 80 >> c.amplifier.clamp(0, 31);
        let due = k <= 0 || c.duration % k == 0;
        if due && food.as_ref().is_some_and(|f| f.saturation <= 0.0) && health < max {
            let _ = living.heal(cx.host, 1.0);
        }
    }
    // NourishmentEffect: no exhaustion, unless natural regeneration is healing on hunger.
    if effect(cx, "farmersdelight:nourishment").is_some()
        && let Some(f) = &food
    {
        let healing = health < max && f.level >= 18;
        if !healing && f.exhaustion > 0.0 {
            let _ = player.set_exhaustion(cx.host, (f.exhaustion - f.exhaustion.min(4.0)).max(0.0));
        }
    }
    if cx.creative() {
        return;
    }
    let Some(pos) = player.entity().position(cx.h()) else { return };
    // PropelplantCaneBlock.entityInside: walking (not crouching) into a cane sets it off.
    if !cx.sneaking() {
        let feet = [pos[0].floor() as i32, pos[1].floor() as i32, pos[2].floor() as i32];
        if let Some(q) = [feet, up(feet, 1)].into_iter().find(|q| cx.name(*q) == PROPELPLANT) {
            cx.remove(q);
            let _ = cx.lv.explode(
                cx.host,
                None,
                centre(q),
                1.0,
                false,
                mod_api::ExplosionInteraction::None,
                mod_api::ExplosionSettings::default(),
            );
            return;
        }
    }
    // StoveBlock.stepOn: standing on a lit stove burns.
    if player.entity().on_ground(cx.h()) != Some(true) {
        return;
    }
    let under = [pos[0].floor() as i32, (pos[1] - 0.2).floor() as i32, pos[2].floor() as i32];
    let st = cx.st(under);
    if short(&st.id) == Some("stove") && st.bool("lit") {
        let boots = living.equipment(cx.h(), EquipmentSlot::Feet);
        let frost = boots.is_some_and(|s| format!("{:?}", s.components()).contains("frost_walker"));
        if !frost {
            // BlackstoneStoveBlock.stepOn: a soul fire burns for 2.
            let amount = if st.bool("soul") { 2.0 } else { 1.0 };
            let _ = player.entity().hurt(cx.host, "minecraft:hot_floor", amount, None, None);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The level: random ticks, bone meal, survival, placement (read-only or level-write hosts)
// ---------------------------------------------------------------------------------------------

/// Work a level hook can't do with its host: bone meal on the plant above rich soil
/// (`performBonemeal` of another block) and the cinnamon tree's feature. lib.rs's
/// `server_tick` does them, the same tick.
pub(crate) enum Deferred {
    BoneMeal(String, [i32; 3]),
    CinnamonTree(String, [i32; 3], String),
}
pub(crate) static DEFERRED: Mutex<Vec<Deferred>> = Mutex::new(Vec::new());
fn defer(d: Deferred) {
    DEFERRED.lock().unwrap_or_else(|e| e.into_inner()).push(d);
}

pub(crate) fn run_deferred(host: &mut dyn ServerHost) {
    let work: Vec<Deferred> = std::mem::take(&mut *DEFERRED.lock().unwrap_or_else(|e| e.into_inner()));
    for d in work {
        match d {
            Deferred::BoneMeal(dim, p) => {
                if let Some(lv) = host.level(&dim) {
                    let _ = lv.bone_meal(host, p);
                }
            }
            Deferred::CinnamonTree(dim, p, sapling) => {
                let Some(lv) = host.level(&dim) else { continue };
                let mut cx = Cx::new(host, lv);
                cx.remove(p);
                let line = format!("execute in {dim} run place feature expandeddelight:cinnamon_tree {} {} {}", p[0], p[1], p[2]);
                cx.host.commands().run_command(&line);
                if cx.st(p).is_air() {
                    let st = St::parse(&sapling);
                    cx.set_st(p, &st, BlockUpdateFlags::CLIENTS);
                }
            }
        }
    }
}

fn lst(level: &dyn LevelHost, p: [i32; 3]) -> St {
    level.block_state(p).and_then(|s| level.state_string(s)).map(|t| St::parse(&t)).unwrap_or_default()
}
fn sst(level: &dyn LevelHost, s: BlockState) -> St {
    level.state_string(s).map(|t| St::parse(&t)).unwrap_or_default()
}
fn ltag(level: &dyn LevelHost, tag: &str, p: [i32; 3]) -> bool {
    level.block_state(p).is_some_and(|s| level.block_in_tag(s.block(), tag) == Some(true))
}
fn stag(level: &dyn LevelHost, tag: &str, s: BlockState) -> bool {
    level.block_in_tag(s.block(), tag) == Some(true)
}
/// `BlockState.isSolid`, near enough: a block that isn't air, a fluid, replaceable or a plant.
fn solid(level: &dyn LevelHost, s: BlockState) -> bool {
    let st = sst(level, s);
    !(st.is_air()
        || matches!(st.id.as_str(), "minecraft:water" | "minecraft:lava")
        || stag(level, "minecraft:replaceable", s)
        || stag(level, "minecraft:flowers", s)
        || stag(level, "minecraft:saplings", s)
        || stag(level, "minecraft:crops", s)
        || stag(level, "minecraft:replaceable_by_trees", s))
}
fn lreplaceable(level: &dyn LevelHost, p: [i32; 3]) -> bool {
    let st = lst(level, p);
    st.is_air() || ltag(level, "minecraft:replaceable", p)
}
fn lset(level: &mut dyn LevelWriteHost, p: [i32; 3], st: &St, flags: i32) {
    let Some(state) = level.block_state_id(&st.text()) else { return };
    let mut f = BlockUpdateFlags::NONE;
    if flags & 1 != 0 {
        f = f | BlockUpdateFlags::NEIGHBORS;
    }
    if flags & 2 != 0 {
        f = f | BlockUpdateFlags::CLIENTS;
    }
    if flags & 4 != 0 {
        f = f | BlockUpdateFlags::INVISIBLE;
    }
    level.set_block(p, state, f);
}
fn ldefault(level: &dyn LevelHost, name: &str) -> Option<St> {
    level.block_state_id(name).map(|s| sst(level, s))
}
fn lrand<T: Default>(level: &mut dyn LevelWriteHost, f: impl FnOnce(&mut mod_api::LegacyRandom) -> T) -> T {
    level.with_random(f).unwrap_or_default()
}
fn lnext_int(level: &mut dyn LevelWriteHost, bound: i32) -> i32 {
    lrand(level, |r| mod_api::RandomSource::next_int_bound(r, bound).unwrap_or(0))
}
fn lfloat(level: &mut dyn LevelWriteHost) -> f32 {
    level.with_random(|r| mod_api::RandomSource::next_float(r)).unwrap_or(1.0)
}
/// `Mth.nextInt(random, a, b)`.
fn lbetween(level: &mut dyn LevelWriteHost, a: i32, b: i32) -> i32 {
    a + lnext_int(level, b - a + 1)
}
/// `Level.getRawBrightness(pos, 0)`.
fn brightness(level: &dyn LevelHost, p: [i32; 3]) -> i32 {
    level.raw_brightness(p, 0)
}
fn sees_sky(level: &dyn LevelHost, p: [i32; 3]) -> bool {
    level.brightness(LightLayer::Sky, p) >= 15
}
fn pos3(p: BlockPos) -> [i32; 3] {
    [p.x, p.y, p.z]
}
fn off(p: [i32; 3], dx: i32, dy: i32, dz: i32) -> [i32; 3] {
    [p[0] + dx, p[1] + dy, p[2] + dz]
}

/// `CropBlock.getGrowthSpeed` (and `BuddingBushBlock.getGrowthSpeed`, the same here).
fn growth_speed(level: &dyn LevelHost, pos: [i32; 3], block: &str) -> f32 {
    let mut speed = 1.0f32;
    for dx in -1..=1 {
        for dz in -1..=1 {
            let s = lst(level, off(pos, dx, -1, dz));
            let mut bonus = 0.0;
            if s.id == "minecraft:farmland" || s.id == "farmersdelight:rich_soil_farmland" {
                bonus = if s.int("moisture") > 0 { 3.0 } else { 1.0 };
            }
            if dx != 0 || dz != 0 {
                bonus /= 4.0;
            }
            speed += bonus;
        }
    }
    let is = |dx, dz| lst(level, off(pos, dx, 0, dz)).id == block;
    let ew = is(-1, 0) || is(1, 0);
    let ns = is(0, -1) || is(0, 1);
    if (ew && ns) || is(-1, -1) || is(1, -1) || is(1, 1) || is(-1, 1) {
        speed /= 2.0;
    }
    speed
}

/// The rich soil boost: the plant above takes bone meal (`performBonemeal`) at 80%.
fn rich_boost(level: &mut dyn LevelWriteHost, dim: &str, pos: [i32; 3], skip_tall_flowers: bool) {
    let above = off(pos, 0, 1, 0);
    let st = lst(level, above);
    if ltag(level, "farmersdelight:unaffected_by_rich_soil", above)
        || (skip_tall_flowers && ltag(level, "minecraft:tall_flowers", above))
        || st.is_air()
    {
        return;
    }
    if lfloat(level) <= RICH_SOIL_BOOST {
        defer(Deferred::BoneMeal(dim.to_string(), above));
    }
}

/// `randomTick` of Farmer's Delight's blocks: true when handled.
pub(crate) fn random_tick(level: &mut dyn LevelWriteHost, dim: &str, pos: BlockPos, state: BlockState) -> bool {
    let pos = pos3(pos);
    let st = sst(level, state);
    let Some(n) = short(&st.id).map(str::to_string) else { return false };
    let lvl: &dyn LevelHost = level;
    match n.as_str() {
        // RichSoilBlock.randomTick
        "rich_soil" => {
            let above = off(pos, 0, 1, 0);
            if ltag(lvl, "farmersdelight:unaffected_by_rich_soil", above) {
                return true;
            }
            let colony = match lst(lvl, above).id.as_str() {
                "minecraft:brown_mushroom" => "farmersdelight:brown_mushroom_colony",
                "minecraft:red_mushroom" => "farmersdelight:red_mushroom_colony",
                _ => {
                    rich_boost(level, dim, pos, false);
                    return true;
                }
            };
            if let Some(s) = ldefault(lvl, colony) {
                lset(level, above, &s, 3);
            }
            true
        }
        // RichSoilFarmlandBlock.randomTick
        "rich_soil_farmland" => {
            let moisture = st.int("moisture");
            let mut water = false;
            'scan: for dx in -4..=4 {
                for dy in 0..=1 {
                    for dz in -4..=4 {
                        if lst(lvl, off(pos, dx, dy, dz)).is_water() {
                            water = true;
                            break 'scan;
                        }
                    }
                }
            }
            if !water && !lvl.is_raining_at(off(pos, 0, 1, 0)) {
                if moisture > 0 {
                    lset(level, pos, &st.with("moisture", &(moisture - 1).to_string()), 2);
                }
            } else if moisture < 7 {
                lset(level, pos, &st.with("moisture", "7"), 2);
            } else {
                rich_boost(level, dim, pos, true);
            }
            true
        }
        // OrganicCompostBlock.randomTick
        "organic_compost" => {
            let mut chance = 0.0f32;
            let mut water = false;
            let mut max_light = 0;
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        let p = off(pos, dx, dy, dz);
                        if ltag(lvl, "farmersdelight:compost_activators", p) {
                            chance += 0.02;
                        }
                        if lst(lvl, p).is_water() {
                            water = true;
                        }
                        max_light = max_light.max(lvl.brightness(LightLayer::Sky, off(p, 0, 1, 0)));
                    }
                }
            }
            chance += if max_light > 12 { 0.1 } else { 0.05 };
            chance += if water { 0.1 } else { 0.0 };
            if lfloat(level) <= chance {
                let stage = st.int("composting");
                let next = if stage == 7 { ldefault(level, "farmersdelight:rich_soil") } else { Some(st.with("composting", &(stage + 1).to_string())) };
                if let Some(s) = next {
                    lset(level, pos, &s, 3);
                }
            }
            true
        }
        // BuddingBushBlock.randomTick, growing into the vine past age 3.
        "budding_tomatoes" => {
            if brightness(lvl, pos) >= 9 {
                let age = st.int("age");
                if age <= 3 {
                    let speed = growth_speed(lvl, pos, &st.id);
                    if lnext_int(level, (25.0 / speed) as i32 + 1) == 0 {
                        let next = if age >= 3 { ldefault(level, "farmersdelight:tomatoes") } else { Some(st.with("age", &(age + 1).to_string())) };
                        if let Some(s) = next {
                            lset(level, pos, &s, 3);
                        }
                    }
                }
            }
            true
        }
        // TomatoVineBlock.randomTick
        "tomatoes" => {
            if brightness(lvl, pos) >= 9 {
                let age = st.int("age");
                if age < 3 {
                    let speed = growth_speed(lvl, pos, &st.id);
                    if lnext_int(level, (25.0 / speed) as i32 + 1) == 0 {
                        lset(level, pos, &st.with("age", &(age + 1).to_string()), 2);
                    }
                }
                rope_climb(level, pos);
            }
            true
        }
        // RiceBlock.tick (its random tick), growing panicles past age 3.
        "rice" => {
            let above = off(pos, 0, 1, 0);
            let supporting = lst(lvl, above).id == "farmersdelight:rice_panicles";
            if st.bool("supporting") != supporting {
                lset(level, pos, &st.with("supporting", if supporting { "true" } else { "false" }), 2);
                return true;
            }
            if brightness(lvl, above) >= 6 {
                let age = st.int("age");
                if age <= 3 && lnext_int(level, 3) == 0 {
                    if age == 3 {
                        let free = lst(level, above).is_air();
                        if free
                            && (brightness(level, above) >= 8 || sees_sky(level, above))
                            && let Some(p) = ldefault(level, "farmersdelight:rice_panicles")
                        {
                            lset(level, above, &p, 3);
                            lset(level, pos, &st.with("supporting", "true"), 2);
                        }
                    } else {
                        lset(level, pos, &st.with("age", &(age + 1).to_string()), 2);
                    }
                }
            }
            true
        }
        // RicePaniclesBlock (CropBlock.randomTick)
        "rice_panicles" => {
            if brightness(lvl, pos) >= 9 {
                let age = st.int("age");
                if age < 3 {
                    let speed = growth_speed(lvl, pos, &st.id);
                    if lnext_int(level, (25.0 / speed) as i32 + 1) == 0 {
                        lset(level, pos, &st.with("age", &(age + 1).to_string()), 2);
                    }
                }
            }
            true
        }
        // MushroomColonyBlock.tick (its random tick); FungusColonyBlock.tick.
        "brown_mushroom_colony" | "red_mushroom_colony" | CRIMSON_COLONY | WARPED_COLONY => {
            let age = st.int("age");
            let tag = if n.starts_with("nethersdelight:") { "nethersdelight:fungus_colony_growable_on" } else { "farmersdelight:mushroom_colony_growable_on" };
            if age < 3 && ltag(lvl, tag, off(pos, 0, -1, 0)) && lnext_int(level, 4) == 0 {
                lset(level, pos, &st.with("age", &(age + 1).to_string()), 2);
            }
            true
        }
        // SoulCompostBlock.randomTick: soul fire and fungi near it, lava; in the Nether only (the
        // soul sand valley's bonus needs the biome, which a level hook can't read).
        SOUL_COMPOST => {
            let mut chance = 0.0f32;
            let mut lava = false;
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        let p = off(pos, dx, dy, dz);
                        if ltag(lvl, "nethersdelight:soul_compost_activators", p) || ltag(lvl, "nethersdelight:soul_compost_flames", p) {
                            chance += 0.02;
                        }
                        if lst(lvl, p).id == "minecraft:lava" {
                            lava = true;
                        }
                    }
                }
            }
            chance += if lava { 0.1 } else { 0.0 };
            if lfloat(level) <= chance && dim == "minecraft:the_nether" {
                let stage = st.int("composting");
                let next = if stage == 3 { ldefault(level, RICH_SOUL_SOIL) } else { Some(st.with("composting", &(stage + 1).to_string())) };
                if let Some(s) = next {
                    lset(level, pos, &s, 3);
                }
            }
            true
        }
        // SaplingBlock.randomTick with CinnamonSaplingGenerator.
        CINNAMON_SAPLING => {
            if brightness(lvl, off(pos, 0, 1, 0)) >= 9 && lnext_int(level, 7) == 0 {
                advance_cinnamon(level, dim, pos, &st);
            }
            true
        }
        // RichSoulSoilBlock.randomTick (Homestead's richSoilBoostChance 0.8).
        RICH_SOUL_SOIL => {
            let above = off(pos, 0, 1, 0);
            let below = off(pos, 0, -1, 0);
            let up_st = lst(lvl, above);
            let down = lst(lvl, below).id;
            let colony = match up_st.id.as_str() {
                "minecraft:crimson_fungus" => Some(CRIMSON_COLONY),
                "minecraft:warped_fungus" => Some(WARPED_COLONY),
                _ => None,
            };
            if let Some(c) = colony.and_then(|c| ldefault(lvl, c)) {
                lset(level, above, &c, 3);
                return true;
            }
            if up_st.is_air() && brightness(lvl, above) < 1 && lnext_int(level, 50) == 0 {
                if let Some(m) = ldefault(level, MIMICARNATION) {
                    lset(level, above, &m, 3);
                }
                return true;
            }
            if matches!(up_st.id.as_str(), "minecraft:twisting_vines" | "minecraft:twisting_vines_plant") && lfloat(level) <= RICH_SOIL_BOOST * 0.3 {
                defer(Deferred::BoneMeal(dim.to_string(), above));
                return true;
            }
            if matches!(down.as_str(), "minecraft:weeping_vines" | "minecraft:weeping_vines_plant") && lfloat(level) <= RICH_SOIL_BOOST * 0.3 {
                defer(Deferred::BoneMeal(dim.to_string(), below));
                return true;
            }
            if up_st.id == "minecraft:nether_wart" && lfloat(level) <= RICH_SOIL_BOOST * 0.8 {
                let age = up_st.int("age");
                if age < 3 {
                    lset(level, above, &up_st.with("age", &(age + 1).to_string()), 3);
                }
            }
            if up_st.id == PROPELPLANT && lfloat(level) <= RICH_SOIL_BOOST {
                let mut top = above;
                while lst(level, off(top, 0, 1, 0)).id == PROPELPLANT {
                    top = off(top, 0, 1, 0);
                }
                let ts = lst(level, top);
                if !ts.bool("pearl") {
                    lset(level, top, &ts.with("pearl", "true"), 3);
                }
            }
            true
        }
        // PropelplantCaneBlock.randomTick: the top cane grows a pearl (1 in 8) and grows up
        // (1 in 12), at most 3 high.
        PROPELPLANT => {
            let above = off(pos, 0, 1, 0);
            let bud = lst(lvl, above).id != PROPELPLANT;
            let mut st = st;
            if st.bool("bud") != bud {
                st = st.with("bud", if bud { "true" } else { "false" });
                lset(level, pos, &st, 2);
            }
            if !bud {
                return true;
            }
            if lnext_int(level, 8) == 0 && !st.bool("pearl") {
                st = st.with("pearl", "true");
                lset(level, pos, &st, 2);
            }
            if lst(level, above).is_air() && lnext_int(level, 12) == 0 {
                let tall = lst(level, off(pos, 0, -1, 0)).id == PROPELPLANT && lst(level, off(pos, 0, -2, 0)).id == PROPELPLANT;
                if tall {
                    return true;
                }
                let pearl = if st.bool("pearl") { "true" } else { "false" };
                lset(level, pos, &st.with("bud", "false").with("pearl", "false"), 2);
                if let Some(t) = ldefault(level, PROPELPLANT) {
                    lset(level, above, &t.with("bud", "true").with("pearl", pearl), 2);
                }
            }
            true
        }
        _ => false,
    }
}

/// `SaplingBlock.advanceTree`: the first stage, then the cinnamon tree in its place.
fn advance_cinnamon(level: &mut dyn LevelWriteHost, dim: &str, pos: [i32; 3], st: &St) {
    if st.int("stage") == 0 {
        lset(level, pos, &st.with("stage", "1"), 4);
        return;
    }
    defer(Deferred::CinnamonTree(dim.to_string(), pos, st.text()));
}

/// `TomatoVineBlock.attemptRopeClimb`: a vine under a rope climbs it, up to 3 high.
fn rope_climb(level: &mut dyn LevelWriteHost, pos: [i32; 3]) {
    if lfloat(level) >= 0.3 {
        return;
    }
    let above = off(pos, 0, 1, 0);
    if !ltag(level, "farmersdelight:ropes", above) {
        return;
    }
    let mut height = 1;
    while lst(level, off(pos, 0, -height, 0)).id == "farmersdelight:tomatoes" {
        height += 1;
    }
    if height < 3
        && let Some(s) = ldefault(level, "farmersdelight:tomatoes")
    {
        lset(level, above, &s.with("ropelogged", "true"), 3);
    }
}

/// `RichSoilFarmlandBlock.tick`: buried farmland turns back into rich soil, not dirt.
pub(crate) fn tick_block(level: &mut dyn LevelWriteHost, pos: BlockPos, state: BlockState) -> bool {
    let pos = pos3(pos);
    if sst(level, state).id != "farmersdelight:rich_soil_farmland" {
        return false;
    }
    let above = off(pos, 0, 1, 0);
    let a = level.block_state(above);
    if a.is_some_and(|a| solid(level, a)) && !ltag(level, "minecraft:fence_gates", above)
        && let Some(s) = ldefault(level, "farmersdelight:rich_soil")
    {
        lset(level, pos, &s, 3);
    }
    true
}

/// `isValidBonemealTarget`.
pub(crate) fn bonemeal_valid(level: &dyn LevelHost, pos: BlockPos, state: BlockState) -> Option<bool> {
    let pos = pos3(pos);
    let st = sst(level, state);
    let n = short(&st.id)?;
    Some(match n {
        "budding_tomatoes" | SUCCULENT | CINNAMON_SAPLING => true,
        "tomatoes" | "rice_panicles" | "brown_mushroom_colony" | "red_mushroom_colony" | CRIMSON_COLONY | WARPED_COLONY => st.int("age") < 3,
        // PropelplantCaneBlock.isValidBonemealTarget: the top cane, without its pearl.
        PROPELPLANT => st.bool("bud") && !st.bool("pearl"),
        "rice" => {
            let up = lst(level, off(pos, 0, 1, 0));
            up.id != "farmersdelight:rice_panicles" || up.int("age") < 3
        }
        _ => return None,
    })
}

/// `isBonemealSuccess`: always for the crops, 45% for the cinnamon sapling (`SaplingBlock`).
pub(crate) fn bonemeal_success(level: &mut dyn mod_api::LevelRandomHost, pos: BlockPos, state: BlockState) -> Option<bool> {
    bonemeal_valid(level, pos, state)?;
    if sst(level, state).id == CINNAMON_SAPLING {
        let f = level.with_random(|r| mod_api::RandomSource::next_float(r)).unwrap_or(1.0);
        return Some(f64::from(f) < 0.45);
    }
    Some(true)
}

/// `performBonemeal`.
pub(crate) fn bonemeal_perform(level: &mut dyn LevelWriteHost, dim: &str, pos: BlockPos, state: BlockState) {
    let p = pos3(pos);
    let st = sst(level, state);
    let Some(n) = short(&st.id).map(str::to_owned) else { return };
    let age = st.int("age");
    match n.as_str() {
        // BuddingTomatoBlock.performBonemeal
        "budding_tomatoes" => {
            let grown = (age + lbetween(level, 1, 4)).min(7);
            let next = if grown <= 3 {
                Some(st.with("age", &grown.to_string()))
            } else {
                ldefault(level, "farmersdelight:tomatoes").map(|d| d.with("age", &(grown - 4).to_string()))
            };
            if let Some(s) = next {
                lset(level, p, &s, 3);
            }
        }
        CINNAMON_SAPLING => advance_cinnamon(level, dim, p, &st),
        // ChorusSucculentBlock.performBonemeal: three on end stone.
        SUCCULENT => {
            if lst(level, off(p, 0, -1, 0)).id == "minecraft:end_stone" {
                lset(level, p, &st.with("succulent", "3"), 3);
            }
        }
        // TomatoVineBlock.performBonemeal
        "tomatoes" => {
            let grown = (age + lbetween(level, 2, 5) / 2).min(3);
            lset(level, p, &st.with("age", &grown.to_string()), 3);
            rope_climb(level, p);
        }
        // RicePaniclesBlock: CropBlock's increase over 3
        "rice_panicles" => {
            let grown = (age + lbetween(level, 2, 5) / 3).min(3);
            lset(level, p, &st.with("age", &grown.to_string()), 3);
        }
        PROPELPLANT => lset(level, p, &st.with("pearl", "true"), 2),
        "brown_mushroom_colony" | "red_mushroom_colony" | CRIMSON_COLONY | WARPED_COLONY => {
            let grown = (age + lbetween(level, 1, 2)).min(3);
            lset(level, p, &st.with("age", &grown.to_string()), 2);
        }
        // RiceBlock.performBonemeal
        "rice" => {
            let grown = (age + lbetween(level, 1, 4)).min(7);
            if grown <= 3 {
                lset(level, p, &st.with("age", &grown.to_string()), 3);
                return;
            }
            let above = off(p, 0, 1, 0);
            let up = lst(level, above);
            if up.id == "farmersdelight:rice_panicles" {
                if up.int("age") < 3
                    && let Some(s) = level.block_state(above)
                {
                    bonemeal_perform(level, dim, BlockPos::new(above[0], above[1], above[2]), s);
                }
            } else if up.is_air()
                && (brightness(level, above) >= 8 || sees_sky(level, above))
                && let Some(panicles) = ldefault(level, "farmersdelight:rice_panicles")
            {
                lset(level, p, &st.with("age", "3").with("supporting", "true"), 3);
                lset(level, above, &panicles.with("age", &(grown - 4).to_string()), 2);
            }
        }
        _ => {}
    }
}

/// `PropelplantCaneBlock.mayPlaceOn`: nylium, netherrack, rich soul soil.
fn propelplant_ground(level: &dyn LevelHost, ground: BlockState) -> bool {
    let g = sst(level, ground).id;
    stag(level, "minecraft:nylium", ground) || g == "minecraft:netherrack" || g == RICH_SOUL_SOIL
}
/// `FungusColonyBlock.mayPlaceOn`: nylium, mycelium, soul soil, dirt, farmland, or (as a mushroom
/// colony) any solid floor.
fn fungus_ground(level: &dyn LevelHost, ground: BlockState) -> bool {
    let g = sst(level, ground).id;
    stag(level, "minecraft:nylium", ground)
        || stag(level, "minecraft:dirt", ground)
        || matches!(g.as_str(), "minecraft:mycelium" | "minecraft:soul_soil" | "minecraft:farmland" | RICH_SOUL_SOIL)
        || solid(level, ground)
}
/// `MimicarnationBlock.mayPlaceOn`: a flower's ground, netherrack and the soul soils.
fn mimicarnation_ground(level: &dyn LevelHost, ground: BlockState) -> bool {
    let g = sst(level, ground).id;
    stag(level, "minecraft:dirt", ground)
        || g == "minecraft:farmland"
        || matches!(g.as_str(), "minecraft:netherrack" | "minecraft:soul_sand" | "minecraft:soul_soil" | RICH_SOUL_SOIL | SOUL_COMPOST)
}

/// `canSurvive` where the template's rule isn't Farmer's Delight's.
pub(crate) fn can_survive(level: &dyn LevelHost, pos: BlockPos, state: BlockState) -> Option<bool> {
    let pos = pos3(pos);
    let st = sst(level, state);
    let n = short(&st.id)?;
    let below_p = off(pos, 0, -1, 0);
    let below = level.block_state(below_p)?;
    let below_st = sst(level, below);
    let light = brightness(level, pos) >= 8 || sees_sky(level, pos);
    match n {
        "tomatoes" if st.bool("ropelogged") => Some(below_st.id == "farmersdelight:tomatoes" && light),
        "rice_panicles" => Some(below_st.id == "farmersdelight:rice" && light),
        // ChorusSucculentBlock.mayPlaceOn: a floor with a top face.
        SUCCULENT => Some(solid(level, below)),
        // PropelplantCaneBlock.canSurvive: on a cane without its pearl, or on its ground.
        PROPELPLANT => Some((below_st.id == PROPELPLANT && !below_st.bool("pearl")) || propelplant_ground(level, below)),
        CRIMSON_COLONY | WARPED_COLONY => Some(fungus_ground(level, below)),
        MIMICARNATION => Some(mimicarnation_ground(level, below)),
        "brown_mushroom_colony" | "red_mushroom_colony" => Some(
            stag(level, "minecraft:mushroom_grow_block", below)
                || below_st.id == "farmersdelight:rich_soil"
                || below_st.id == "farmersdelight:rich_soil_farmland"
                || (brightness(level, pos) < 13 && solid(level, below)),
        ),
        _ => None,
    }
}

/// `mayPlaceOn` for the plants on a template whose ground isn't theirs.
pub(crate) fn may_place_on(level: &dyn LevelHost, state: BlockState, ground: BlockState) -> Option<bool> {
    let st = sst(level, state);
    let g = sst(level, ground).id;
    match short(&st.id)? {
        "rice_panicles" => Some(g == "farmersdelight:rice"),
        SUCCULENT => Some(solid(level, ground)),
        PROPELPLANT => Some(g == PROPELPLANT || propelplant_ground(level, ground)),
        CRIMSON_COLONY | WARPED_COLONY => Some(fungus_ground(level, ground)),
        MIMICARNATION => Some(mimicarnation_ground(level, ground)),
        "brown_mushroom_colony" | "red_mushroom_colony" => Some(solid(level, ground)),
        "tomatoes" | "budding_tomatoes" => Some(g == "minecraft:farmland" || g == "farmersdelight:rich_soil_farmland" || g == "farmersdelight:tomatoes"),
        _ => None,
    }
}

/// `getStateForPlacement`: `Some(Some(state))` to choose one, `Some(None)` to refuse.
#[allow(clippy::option_option)]
pub(crate) fn placement(level: &dyn LevelHost, state: BlockState, context: &mod_api::PlacementContext) -> Option<Option<BlockState>> {
    let mut st = sst(level, state);
    if st.id.is_empty() {
        st.id = level.state_block(state)?.to_string();
    }
    let n = short(&st.id)?;
    let hor = horizontal(context.yaw);
    let cpos = pos3(context.pos);
    let tray = || ltag(level, "farmersdelight:tray_heat_sources", off(cpos, 0, -1, 0));
    let face = dir_name(context.face);
    let b = |v: bool| if v { "true" } else { "false" };
    let chosen: St = match n {
        "stove" => st.with("facing", opposite(hor)).with("lit", "true"),
        "cooking_pot" => {
            let support = if face == "down" { "handle" } else if tray() { "tray" } else { "none" };
            st.with("facing", opposite(hor)).with("support", support)
        }
        "skillet" => st.with("facing", hor).with("support", b(tray())),
        "cutting_board" => st.with("facing", opposite(hor)),
        "rice_bale" => st.with("facing", face),
        "tatami" => {
            let ts = lst(level, pos3(context.hit_pos));
            let pairing = !context.sneaking && ts.id == "farmersdelight:tatami" && !ts.bool("paired");
            st.with("facing", opposite(face)).with("paired", b(pairing))
        }
        "full_tatami_mat" => {
            let (dx, dz) = step(hor);
            if !lreplaceable(level, off(cpos, dx, 0, dz)) {
                return Some(None);
            }
            st.with("facing", hor).with("part", "foot")
        }
        "half_tatami_mat" => st.with("facing", hor),
        JUICER | MORTAR => st.with("facing", opposite(hor)),
        // DragonLegBlock / StuffedHoglinBlock.getStateForPlacement: the head here, the foot one
        // step on.
        DRAGON_LEG | STUFFED_HOGLIN => {
            let (dx, dz) = step(hor);
            if !lreplaceable(level, off(cpos, dx, 0, dz)) {
                return Some(None);
            }
            st.with("facing", hor).with("part", "head")
        }
        "rice" => {
            let here = lst(level, cpos);
            let source = here.id == "minecraft:water" && here.get("level") == Some("0");
            if !source && !here.id.is_empty() {
                return Some(None);
            }
            st.clone()
        }
        "brown_mushroom_colony" | "red_mushroom_colony" | CRIMSON_COLONY | WARPED_COLONY => st.with("age", "3"),
        // StandingAndWallBlockItem (Direction.DOWN): on a wall's side, the wall torch (only the
        // torch's own block can be answered here; use_on places the wall torch).
        "nethersdelight:propelplant_torch" => return None,
        // PropelplantCaneBlock.getStateForPlacement: a stem on the ground, a bud on top.
        PROPELPLANT => {
            let below = lst(level, off(cpos, 0, -1, 0)).id == PROPELPLANT;
            let above = lst(level, off(cpos, 0, 1, 0)).id == PROPELPLANT;
            st.with("stem", b(!below)).with("bud", b(!above))
        }
        _ if n.ends_with("_cabinet") => st.with("facing", opposite(hor)),
        _ if is_pie(n) => st.with("facing", hor),
        _ if feast(n).is_some() => st.with("facing", opposite(hor)),
        _ => return None,
    };
    // The changed properties, set on the block's own default state.
    let mut out = Some(state);
    for (k, v) in &chosen.props {
        if st.get(k) != Some(v.as_str()) {
            out = out.and_then(|s| level.state_with_property(s, k, v));
        }
    }
    out.or_else(|| level.block_state_id(&chosen.text())).map(Some)
}
