//! farmersdelight: Farmer's Delight 2.4.1+refabricated (vectorwing, MehVahdJukaar, MerchantPug;
//! MIT). One package: data/ and assets/ are the content mod mods/farmersdelight
//! (tools/farmersdelight_port.py, block shapes fixed by tools/next_content_fix.py), and the rules
//! are src/fd.rs. It also serves the delight add-ons' blocks (Ocean's, End's, Nether's and
//! Expanded Delight), as before.
//!
//! Mod API only since 0.5.0 (SDK dd67b76): no game module, no mixin. Until 0.4.x the rules were a
//! game module reached by 21 mixins; on dd67b76 the launcher can't compile mixins on most of those
//! targets (horizon-rust/horizon#18462), and the API now covers all of them:
//! - `register`: the recipe types (raw, so the recipe load keeps their files), the block entity
//!   types with their part (`Machine`: ticks, update tag, drawing, screens), each machine's slots
//!   (`block_container`), `Rules` on every block with a rule (placement, random and scheduled
//!   ticks, bone meal, survival, ground) and `Placer` on the items whose placing sets a second
//!   block;
//! - hooks: right clicks (`right_click_block`), items used on blocks (`use_item_on_block`), the
//!   foods' own effects (`consume_effect`), Comfort, Nourishment, the stove's burn and the
//!   propelplant (`player_tick_post`), the cabinets' open state (`mod_container_viewers`), the
//!   work level hooks hand over (`server_tick`), the add-ons' loot (`loot_table_load`).
mod fd;

use mod_api::serde_json::json;
use mod_api::{
    BlockEntityPart, BlockEntitySubmitEvent, BlockEntityTickEvent, BlockEntityTicksEvent, BlockEntityUpdateTagEvent,
    BlockPart, BlockState, CanSurviveEvent, ClientHost, Compound, ConsumeEffectEvent, DataLoadContext, Flow, Host,
    HostSide, Hooks, IsBonemealSuccessEvent, IsValidBonemealTargetEvent, ItemPart, LevelHost, LevelRandomHost, LevelSide,
    LevelWriteHost, LootTableLoadEvent, MayPlaceOnEvent, ModContainerViewersEvent, OnBlockPlacedByEvent,
    PerformBonemealEvent, PlayerTickPostEvent, RandomTickEvent, Registrar, RegistrationContext, RightClickBlockEvent,
    ScreenDrawEvent, ScreenEvent, ServerHost, ServerTickEvent, StateForPlacementEvent, TickBlockEvent,
    UseItemOnBlockEvent,
};

use fd::Cx;

#[derive(Default)]
struct Mod;

/// Farmer's Delight's recipe serializers, each its own type.
const RECIPE_TYPES: [&str; 4] = [
    "farmersdelight:cooking",
    "farmersdelight:cutting",
    "farmersdelight:dough",
    "farmersdelight:food_serving",
];

pub(crate) const WOODS: [&str; 12] = [
    "oak", "spruce", "birch", "jungle", "acacia", "dark_oak", "mangrove", "cherry", "bamboo", "crimson",
    "warped", "pale_oak",
];

/// (type, blocks, namespace the blocks need). The type ids are the old port's entity ids, so saved
/// worlds keep their block entities; the add-ons' are renamed into this namespace (a type is the
/// declaring mod's).
fn types() -> Vec<(&'static str, Vec<String>, &'static str)> {
    let fd = |names: &[&str]| names.iter().map(|n| format!("farmersdelight:{n}")).collect::<Vec<_>>();
    let cabinets: Vec<String> = WOODS
        .iter()
        .filter(|w| **w != "pale_oak")
        .map(|w| format!("farmersdelight:{w}_cabinet"))
        .collect();
    vec![
        ("stove", fd(&["stove"]), "farmersdelight"),
        ("end_stove", vec!["ends_delight:end_stove".into()], "ends_delight"),
        ("blackstone_stove", vec!["nethersdelight:blackstone_stove".into()], "nethersdelight"),
        ("cooking_pot", fd(&["cooking_pot"]), "farmersdelight"),
        ("skillet", fd(&["skillet"]), "farmersdelight"),
        ("cutting_board", fd(&["cutting_board"]), "farmersdelight"),
        ("basket", fd(&["basket"]), "farmersdelight"),
        ("cabinet", cabinets, "farmersdelight"),
        ("juicer", vec!["expandeddelight:juicer".into()], "expandeddelight"),
        (
            "furnace",
            vec![
                "nethersdelight:blackstone_furnace".into(),
                "nethersdelight:nether_brick_smoker".into(),
                "nethersdelight:blackstone_blast_furnace".into(),
            ],
            "nethersdelight",
        ),
    ]
}

/// The items whose placing sets a second block or changes a neighbour (`setPlacedBy`).
const PLACERS: [&str; 5] = [
    "farmersdelight:tatami",
    "farmersdelight:full_tatami_mat",
    "ends_delight:dragon_leg_with_sauce_block",
    "nethersdelight:stuffed_hoglin",
    "nethersdelight:propelplant_cane",
];

fn ns_of(id: &str) -> &str {
    id.split(':').next().unwrap_or("")
}

fn server_cx(host: &mut dyn ServerHost) -> Option<Cx<'_>> {
    let dim = host.dimension();
    let lv = host.level(dim)?;
    Some(Cx::new(host, lv))
}

fn click_cx<'a>(host: &'a mut dyn ServerHost, click: &mod_api::BlockClick<'_>) -> Option<Cx<'a>> {
    let dim = click.dimension.key().id().to_string();
    let lv = host.level(&dim)?;
    let player = host.player(click.player);
    let mut cx = Cx::new(host, lv);
    cx.player = player;
    cx.hand = click.hand;
    Some(cx)
}

/// The machines' block entity: ticks, what clients get and draw, the screens. Its slots are the
/// blocks' declared containers.
struct Machine;

#[mod_api::horizon_part]
impl BlockPart for Machine {}

#[mod_api::horizon_part]
impl BlockEntityPart for Machine {
    fn block_entity_ticks(&self, _host: &dyn Host, event: &mut BlockEntityTicksEvent<'_>) {
        if fd::ticks(event.block) {
            event.ticks = true;
        }
    }

    fn block_entity_tick(&self, host: &mut dyn ServerHost, event: &mut BlockEntityTickEvent<'_>) {
        let id = event.entity.id().to_string();
        let pos = event.pos;
        let Some(mut cx) = server_cx(host) else {
            *event.reply = mod_api::BlockEntityTickReply::Unchanged;
            return;
        };
        let mut fields = (*event.entity).clone();
        if fd::tick(&mut cx, pos, &id, &mut fields) {
            *event.entity = fields;
        } else {
            *event.reply = mod_api::BlockEntityTickReply::Unchanged;
        }
    }

    fn block_entity_update_tag(&self, event: &mut BlockEntityUpdateTagEvent<'_>) -> Option<Compound> {
        fd::update_tag(event.entity)
    }

    fn screen(&self, _host: &dyn Host, event: &mut ScreenEvent<'_>) -> Option<mod_api::Screen> {
        fd::screen(event.block)
    }

    fn screen_draw(&self, _host: &dyn ClientHost, event: &mut ScreenDrawEvent<'_>) {
        fd::screen_draw(event);
    }

    fn block_entity_submit(&self, host: &dyn ClientHost, event: &mut BlockEntitySubmitEvent<'_>) {
        fd::submit(host, event);
    }
}

/// The blocks' own rules on the level.
struct Rules;

#[mod_api::horizon_part]
impl BlockPart for Rules {
    fn random_tick(&self, level: &mut dyn LevelWriteHost, event: &mut RandomTickEvent) -> Flow {
        let dim = event.dimension.key().id().to_string();
        if fd::random_tick(level, &dim, event.pos, event.state) { Flow::Cancel } else { Flow::Continue }
    }

    fn tick_block(&self, level: &mut dyn LevelWriteHost, event: &mut TickBlockEvent) -> Flow {
        if fd::tick_block(level, event.pos, event.state) { Flow::Cancel } else { Flow::Continue }
    }

    fn can_survive(&self, level: &dyn LevelHost, event: &mut CanSurviveEvent) -> Option<bool> {
        fd::can_survive(level, event.pos, event.state)
    }

    fn may_place_on(&self, level: &dyn LevelHost, event: &mut MayPlaceOnEvent) -> Option<bool> {
        fd::may_place_on(level, event.state, event.ground)
    }

    fn is_valid_bonemeal_target(&self, level: LevelSide, event: &mut IsValidBonemealTargetEvent) -> Option<bool> {
        let (LevelSide::Server(level) | LevelSide::Client(level)) = level;
        fd::bonemeal_valid(level, event.pos, event.state)
    }

    fn is_bonemeal_success(&self, level: &mut dyn LevelRandomHost, event: &mut IsBonemealSuccessEvent) {
        if let Some(s) = fd::bonemeal_success(level, event.pos, event.state) {
            event.success = s;
        }
    }

    fn perform_bonemeal(&self, level: &mut dyn LevelWriteHost, event: &mut PerformBonemealEvent) {
        let dim = event.dimension.key().id().to_string();
        fd::bonemeal_perform(level, &dim, event.pos, event.state);
    }

    fn state_for_placement(&self, level: &dyn LevelHost, event: &mut StateForPlacementEvent<'_>) -> Option<Option<BlockState>> {
        fd::placement(level, event.state, event.context)
    }
}

/// The items whose placing sets a second block (`setPlacedBy`).
struct Placer;

#[mod_api::horizon_part]
impl ItemPart for Placer {
    fn on_block_placed_by(&self, host: &mut dyn ServerHost, event: &mut OnBlockPlacedByEvent<'_>) {
        let pos = event.pos;
        let placer = event.placer;
        let Some(mut cx) = server_cx(host) else { return };
        cx.player = placer;
        fd::placed(&mut cx, pos);
    }
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, cx: &RegistrationContext) {
        let has = |ns: &str| {
            ns == "farmersdelight"
                || cx
                    .mods()
                    .iter()
                    .any(|p| p.namespaces().any(|n| n == ns))
        };
        for kind in RECIPE_TYPES {
            r.register_recipe_type(kind);
            r.register_raw_recipe_serializer(kind, kind);
        }
        for (id, blocks, ns) in types() {
            if !has(ns) {
                continue;
            }
            for b in &blocks {
                if let Some(c) = fd::container(b) {
                    let key = r.block_key(b);
                    r.block_container(&key, c);
                }
            }
            let names: Vec<&str> = blocks.iter().map(String::as_str).collect();
            let kind = r.block_entity_type(id).blocks(&names).build();
            if let Err(e) = r.block_entity_part(&kind, Machine) {
                eprintln!("[farmersdelight] block entity part {id} refused: {e:?}");
            }
        }
        for b in fd::rule_blocks() {
            if has(ns_of(&b)) {
                let key = r.block_key(&b);
                r.block_part(key, Rules);
            }
        }
        for i in PLACERS {
            if has(ns_of(i)) {
                let key = r.item_key(i);
                r.item_part(key, Placer);
            }
        }
    }

    fn right_click_block(&self, host: HostSide<'_>, event: &mut RightClickBlockEvent<'_>) -> Flow {
        let click = event.click;
        let answer = match host {
            HostSide::Server(host) => {
                let Some(mut cx) = click_cx(host, click) else { return Flow::Continue };
                let name = cx.lv.block_state(&*cx.host, click.pos).and_then(|s| cx.host.block_state_string(s)).unwrap_or_default();
                if !fd::clickable(fd::St::parse(&name).id.as_str()) {
                    return Flow::Continue;
                }
                fd::right_click(&mut cx, click.pos)
            }
            HostSide::Client(host) => {
                let dim = click.dimension.key().id().to_string();
                let Some(lv) = host.level(&dim) else { return Flow::Continue };
                let name = lv.block_state(&*host, click.pos).and_then(|s| host.block_state_string(s)).unwrap_or_default();
                if fd::client_not_block(fd::St::parse(&name).id.as_str()) { fd::Click::NotBlock } else { fd::Click::Pass }
            }
        };
        match answer {
            fd::Click::Taken => {
                *event.cancellation_result = mod_api::InteractionResult::Success;
                Flow::Cancel
            }
            fd::Click::NotBlock => {
                *event.use_block = mod_api::TriState::False;
                Flow::Continue
            }
            fd::Click::Pass => Flow::Continue,
        }
    }

    fn use_item_on_block(&self, host: HostSide<'_>, event: &mut UseItemOnBlockEvent<'_>) -> Flow {
        let HostSide::Server(host) = host else { return Flow::Continue };
        if !matches!(event.phase, mod_api::UsePhase::ItemAfterBlock) {
            return Flow::Continue;
        }
        let click = event.click;
        let Some(held) = click.stack.filter(|s| s.count() > 0).cloned() else { return Flow::Continue };
        let Some(mut cx) = click_cx(host, click) else { return Flow::Continue };
        let item = held.item().id(&*cx.host);
        let block = cx.lv.block_state(&*cx.host, click.pos).and_then(|s| cx.host.block_state_string(s)).unwrap_or_default();
        if !fd::use_on_candidate(&item, fd::St::parse(&block).id.as_str()) {
            return Flow::Continue;
        }
        if fd::use_on(&mut cx, &held, click.pos, click.hit.face) {
            *event.cancellation_result = mod_api::InteractionResult::Success;
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    fn consume_effect(&self, host: &mut dyn ServerHost, event: &mut ConsumeEffectEvent<'_>) {
        let player = host.players().acting_player();
        let Some(mut cx) = server_cx(host) else { return };
        cx.player = player;
        fd::consume_effect(&mut cx, event.kind);
    }

    fn player_tick_post(&self, host: HostSide<'_>, event: &mut PlayerTickPostEvent) {
        let HostSide::Server(host) = host else { return };
        let Some(dim) = event.player.dimension(&*host).map(|d| d.to_string()) else { return };
        let Some(lv) = host.level(&dim) else { return };
        let mut cx = Cx::new(host, lv);
        cx.player = Some(event.player);
        fd::player_tick(&mut cx);
    }

    fn mod_container_viewers(&self, host: &mut dyn ServerHost, event: &mut ModContainerViewersEvent<'_>) {
        let Some(lv) = host.level(event.dimension) else { return };
        let mut cx = Cx::new(host, lv);
        fd::viewers(&mut cx, event.pos, event.viewers, event.previous);
    }

    fn server_tick(&self, host: &mut dyn ServerHost, _event: &mut ServerTickEvent<'_>) {
        fd::TICKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        fd::run_deferred(host);
    }

    fn loot_table_load(&self, _context: &dyn DataLoadContext, event: &mut LootTableLoadEvent<'_>) {
        addon_loot(event.id, event.table);
    }
}

/// The add-ons' loot injections (Fabric LootTableEvents.MODIFY / REPLACE, Forge loot modifiers): a
/// pool drawing the add-on's table, added to the vanilla table it extends. A missing add-on's table
/// draws nothing.
fn addon_loot(id: &str, table: &mut mod_api::serde_json::Value) {
    let refs: &[&str] = match id {
        // OceansDelight.registerLootTable
        "minecraft:entities/squid" => &["oceansdelight:inject/entities/squid"],
        "minecraft:entities/guardian" => &["oceansdelight:inject/entities/guardian"],
        "minecraft:entities/elder_guardian" => &["oceansdelight:inject/entities/elder_guardian"],
        // EndsDelight.registerLootTable; Nether's Delight's machete on shulkers.
        "minecraft:entities/ender_dragon" => &["ends_delight:inject/entities/ender_dragon"],
        "minecraft:entities/enderman" => &["ends_delight:inject/entities/enderman"],
        "minecraft:entities/endermite" => &["ends_delight:inject/entities/endermite"],
        "minecraft:entities/shulker" => &["ends_delight:inject/entities/shulker", "nethersdelight:inject/entities/shulker"],
        // NDCommonSetup (the strider's slice) and Nether's Delight's loot modifiers (machetes).
        "minecraft:entities/strider" => &["nethersdelight:inject/entities/strider"],
        "minecraft:entities/pig" => &["nethersdelight:inject/entities/pig"],
        "minecraft:entities/chicken" => &["nethersdelight:inject/entities/chicken"],
        "minecraft:entities/rabbit" => &["nethersdelight:inject/entities/rabbit"],
        "minecraft:entities/cow" => &["nethersdelight:inject/entities/cow"],
        "minecraft:entities/donkey" => &["nethersdelight:inject/entities/donkey"],
        "minecraft:entities/horse" => &["nethersdelight:inject/entities/horse"],
        "minecraft:entities/llama" => &["nethersdelight:inject/entities/llama"],
        "minecraft:entities/mule" => &["nethersdelight:inject/entities/mule"],
        "minecraft:entities/spider" => &["nethersdelight:inject/entities/spider"],
        "minecraft:entities/cave_spider" => &["nethersdelight:inject/entities/cave_spider"],
        // Expanded Delight's LootTableUtil: crops in village house chests.
        "minecraft:chests/village/village_desert_house" => &["expandeddelight:inject/chests/village_desert_house"],
        "minecraft:chests/village/village_savanna_house" => &["expandeddelight:inject/chests/village_savanna_house"],
        "minecraft:chests/village/village_snowy_house" => &["expandeddelight:inject/chests/village_snowy_house"],
        // NDCommonSetup: the hoglin's table is Nether's Delight's (hoglin loin, leather unless a
        // hunting tool takes its hide).
        "minecraft:entities/hoglin" => {
            *table = json!({"type": "minecraft:entity", "pools": [
                {"rolls": 1, "entries": [{"type": "minecraft:loot_table", "value": "nethersdelight:replace/entities/hoglin"}]}]});
            return;
        }
        // NDCommonSetup: propelpearls in every bartering pool, weight 20.
        "minecraft:gameplay/piglin_bartering" => {
            let Some(pools) = table.get_mut("pools").and_then(|p| p.as_array_mut()) else { return };
            for pool in pools {
                if let Some(entries) = pool.get_mut("entries").and_then(|e| e.as_array_mut()) {
                    entries.push(json!({"type": "minecraft:loot_table", "weight": 20,
                        "value": "nethersdelight:inject/gameplay/piglin_bartering"}));
                }
            }
            return;
        }
        _ => return,
    };
    let Some(t) = table.as_object_mut() else { return };
    let Some(pools) = t.entry("pools").or_insert_with(|| json!([])).as_array_mut() else { return };
    for r in refs {
        pools.push(json!({"rolls": 1, "entries": [{"type": "minecraft:loot_table", "value": r}]}));
    }
}
