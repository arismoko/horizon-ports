//! Right Click Harvest for Sunrise: right-click a grown crop to harvest and replant it
//! (`RightClickHarvest.onBlockUse` in the original), with a hoe's tier choosing how many
//! neighbours go with it.
//!
//! | Moment | Where | What it does |
//! |---|---|---|
//! | `right_click_block` | [`harvest`] | harvests on the server, and on the client claims the click |
//! | `register` | this module | the config file's listener and the block tag keys |

pub mod rules;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use mod_api::text::TextStyle;
use mod_api::{
    BlockClick, ClientHost, ConfigEvent, ConfigKind, ConfigSnapshot, ConfigSpec, EquipmentSlot,
    Flow, Hand, Hooks, HostBlockStates, HostSide, InteractionResult, ItemStack, LevelRef,
    LivingRef, PlayerRef, Registrar, RegistrationContext, RightClickBlockEvent, ServerHost,
    SoundPlay, StatType, TagKey, Text,
};
use rules::{Config, Gate, Kind, Player, Tiers};

const MOD: &str = "rightclickharvest";

static SETTINGS: Mutex<Config> = Mutex::new(Config {
    require_hoe: false,
    harvest_in_radius: true,
    hunger: rules::Hunger::None,
    experience: rules::Experience::None,
    warned: false,
});

/// Whether the hoe warning was shown in this session, in case the config file could not be written.
static WARNED: AtomicBool = AtomicBool::new(false);

/// The config file `register` declares (`ConfigKind::Common`).
const CONFIG_FILE: &str = "rightclickharvest-common.toml";

/// Records `hasUserBeenWarnedForNotUsingHoe = true` in the config file, as the original's
/// `CONFIG.save()` does; the reload that follows reads it back.
fn save_warned(host: &dyn mod_api::Host) {
    let path = host.config_dir().join(CONFIG_FILE);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| rules::DEFAULT_CONFIG.to_owned());
    let _ = std::fs::write(&path, rules::with_warned(&text));
}

/// The block tags the harvester reads, resolved once in `register` and kept across reloads.
struct BlockTags {
    blacklist: TagKey,
    hoe_never_required: TagKey,
    radius_blacklist: TagKey,
    crops: TagKey,
}

static BLOCK_TAGS: OnceLock<BlockTags> = OnceLock::new();

/// A green part of the warning, as the original styles its two arguments.
fn green(text: Text) -> Text {
    let mut style = TextStyle::default();
    style.color = Some("green".to_owned());
    text.with_style(style)
}

fn settings() -> Config {
    *SETTINGS.lock().unwrap_or_else(|e| e.into_inner())
}

fn tags_of(host: &dyn mod_api::Host, stack: Option<&ItemStack>) -> Vec<String> {
    stack
        .and_then(|stack| host.tags().item_tags(stack.item()))
        .map(|tags| tags.iter().map(ToString::to_string).collect())
        .unwrap_or_default()
}

/// `isHoe` and the tier tags of the held stack: `#minecraft:hoes` and the tier tags.
fn hoe(host: &dyn mod_api::Host, stack: Option<&ItemStack>) -> (bool, Tiers) {
    let tags = tags_of(host, stack);
    let has = |tag: &str| tags.iter().any(|t| t == tag);
    let tiers = Tiers {
        low: has("rightclickharvest:low_tier_hoes"),
        mid: has("rightclickharvest:mid_tier_hoes"),
        high: has("rightclickharvest:high_tier_hoes"),
    };
    (
        has("minecraft:hoes") || tiers.low || tiers.mid || tiers.high,
        tiers,
    )
}

/// A block as the harvester reads it.
struct Cell {
    id: String,
    kind: Option<Kind>,
    age: Option<(i32, i32)>,
    blacklisted: bool,
    hoe_never_required: bool,
    radius_blacklisted: bool,
}

impl Cell {
    /// The block at `pos`, or `None` where it cannot be read.
    fn read(host: &dyn mod_api::Host, level: LevelRef, pos: [i32; 3]) -> Option<Self> {
        let state = level.block_state(host, pos)?;
        let text = host.block_state_string(state)?;
        let id = text
            .split_once('[')
            .map_or(text.as_str(), |(id, _)| id)
            .to_owned();
        let key = state.block();
        let in_tag = |pick: fn(&BlockTags) -> TagKey| {
            BLOCK_TAGS
                .get()
                .and_then(|tags| host.tags().block_in_tag(key, pick(tags)))
                .unwrap_or(false)
        };
        let age = text
            .split_once('[')
            .and_then(|(_, props)| {
                props
                    .trim_end_matches(']')
                    .split(',')
                    .find_map(|pair| pair.strip_prefix("age="))
            })
            .and_then(|value| value.parse::<i32>().ok());
        let max = age.and_then(|_| {
            host.block_states()
                .definition(key)?
                .iter()
                .find(|property| property.name == "age")?
                .values
                .iter()
                .filter_map(|value| value.parse::<i32>().ok())
                .max()
        });
        Some(Self {
            kind: Kind::of(&id, in_tag(|tags| tags.crops), age.is_some()),
            id,
            age: age.zip(max),
            blacklisted: in_tag(|tags| tags.blacklist),
            hoe_never_required: in_tag(|tags| tags.hoe_never_required),
            radius_blacklisted: in_tag(|tags| tags.radius_blacklist),
        })
    }

    /// `isMature`: the age is the largest the block has.
    fn mature(&self) -> bool {
        self.age.is_some_and(|(age, max)| age >= max)
    }
}

/// The player as the rules ask.
fn player_state(host: &dyn mod_api::Host, player: PlayerRef, hand: Hand) -> Option<Player> {
    Some(Player {
        spectator: player.is_spectator(host)?,
        crouching: player.entity().is_crouching(host).unwrap_or(false),
        main_hand: hand == Hand::Main,
        creative: player.is_creative(host).unwrap_or(false),
        food_level: player.food(host).map_or(20, |food| food.level),
        xp_level: player.experience(host).map_or(0, |xp| xp.level),
    })
}

/// What the harvest of one cell needs from the click that started it.
struct Click<'a> {
    level: LevelRef,
    player: PlayerRef,
    hand: Hand,
    stack: Option<&'a ItemStack>,
    face_up: bool,
}

/// `RightClickHarvest.onBlockUse` on the server for the block at `pos`. True when it harvested.
fn harvest_at(host: &mut dyn ServerHost, click: &Click<'_>, pos: [i32; 3], initial: bool) -> bool {
    let config = settings();
    let Some(cell) = Cell::read(&*host, click.level, pos) else {
        return false;
    };
    let Some(player) = player_state(&*host, click.player, click.hand) else {
        return false;
    };
    let (holding_hoe, tiers) = hoe(&*host, click.stack);
    let hoe_in_use = match rules::gate(
        player,
        config,
        cell.blacklisted,
        cell.hoe_never_required,
        holding_hoe,
    ) {
        Gate::Go { hoe_in_use } => hoe_in_use,
        Gate::Pass | Gate::NeedHoe => return false,
    };
    if !initial && cell.radius_blacklisted {
        return false;
    }
    let Some(kind) = cell.kind else {
        return false;
    };
    let (break_pos, replant) = if kind.is_column() {
        // Holding the block's own item and clicking its top places another on it.
        let held = click.stack.and_then(|stack| stack.item().id(&*host));
        let own = if kind == Kind::SugarCane {
            "minecraft:sugar_cane"
        } else {
            "minecraft:cactus"
        };
        if click.face_up && held == Some(own) {
            return false;
        }
        let level = click.level;
        let Some(at) = rules::column_break(pos, |at| {
            level
                .block(&*host, at)
                .is_some_and(|key| key.id(&*host) == Some(cell.id.as_str()))
        }) else {
            return false;
        };
        (at, false)
    } else {
        if !cell.mature() {
            return false;
        }
        if initial
            && config.harvest_in_radius
            && !cell.radius_blacklisted
            && holding_hoe
            && let Some((radius, circle)) = tiers.radius()
        {
            for around in rules::radius_positions(pos, radius, circle) {
                harvest_at(host, click, around, false);
            }
        }
        (pos, true)
    };
    let held = click.stack;
    let may = held.is_none_or(|stack| {
        click
            .player
            .may_use_item_at(&*host, break_pos, mod_api::BlockFace::Up, stack)
            .unwrap_or(true)
    });
    if !may {
        return false;
    }
    complete(host, click, &cell, break_pos, hoe_in_use, replant)
}

/// `completeHarvest` on the server: stats, the drops without one seed, the replant or removal,
/// tool wear, exhaustion and experience.
fn complete(
    host: &mut dyn ServerHost,
    click: &Click<'_>,
    cell: &Cell,
    pos: [i32; 3],
    hoe_in_use: bool,
    replant: bool,
) -> bool {
    let config = settings();
    let Some(state) = click.level.block_state(&*host, pos) else {
        return false;
    };
    let block_id = cell.id.clone();
    let _ = click.player.award_stat(host, StatType::Mined, &block_id, 1);
    if let Some(stack) = click.stack
        && let Some(item) = stack.item().id(&*host).map(str::to_owned)
    {
        let _ = click.player.award_stat(host, StatType::Used, &item, 1);
    }
    let drops = click
        .level
        .block_drops(host, pos, click.stack, Some(click.player.entity()))
        .ok()
        .flatten()
        .unwrap_or_default();
    let mut named: Vec<(String, u32)> = drops
        .iter()
        .map(|stack| {
            (
                stack.item().id(&*host).unwrap_or_default().to_owned(),
                stack.count(),
            )
        })
        .collect();
    if replant {
        // `getCloneItemStack`: the block's own pick item, so a modded crop keeps back its seed.
        let picked = click
            .player
            .clone_item_stack(host, click.level, pos, false)
            .and_then(|stack| stack.item().id(&*host).map(str::to_owned));
        if let Some(item) = picked.as_deref().or_else(|| rules::replant_item(&cell.id)) {
            rules::take_replant(&mut named, item);
        }
    }
    for (stack, (_, count)) in drops.into_iter().zip(&named) {
        if *count == 0 {
            continue;
        }
        let mut stack = stack;
        if stack.set_count(*count).is_ok() {
            let _ = click.level.pop_resource(host, pos, stack);
        }
    }
    if replant {
        let Some(text) = host.block_state_string(state) else {
            return false;
        };
        let Some(next) = with_age_zero(&text).and_then(|text| host.block_state_id(&text)) else {
            return false;
        };
        let _ = click.level.set_block(
            host,
            pos,
            next,
            mod_api::BlockUpdateFlags::NEIGHBORS | mod_api::BlockUpdateFlags::CLIENTS,
        );
        let _ = click.player.award_stat(host, StatType::Used, &block_id, 1);
    } else {
        let _ = click.level.remove_block(host, pos, false);
    }
    if hoe_in_use {
        let slot = match click.hand {
            Hand::Main => EquipmentSlot::MainHand,
            Hand::Off => EquipmentSlot::OffHand,
        };
        let _ = LivingRef::new(click.player.entity()).hurt_and_break(host, slot, 1);
    }
    // Regular block breaking causes 0.005 exhaustion.
    let _ = click
        .player
        .add_exhaustion(host, rules::exhaustion(config.hunger));
    let roll = click
        .level
        .with_random(host, |random| random.next_int_bound(rules::XP_MAX + 1))
        .unwrap_or(0);
    let change = rules::experience_change(config.experience, roll);
    if change < 0 {
        let total = click.player.experience(&*host).map_or(0, |xp| xp.total);
        let _ = click
            .player
            .set_total_experience(host, (total + change).max(0));
    } else if change > 0 {
        let [x, y, z] = click.player.entity().position(&*host).unwrap_or([0.0; 3]);
        let _ = click.level.award_experience(host, x, y, z, change as u32);
    }
    true
}

/// A block state's text with `age` set to 0, every other property kept (`getReplantState`).
pub fn with_age_zero(text: &str) -> Option<String> {
    let (block, props) = text.strip_suffix(']')?.split_once('[')?;
    let mut found = false;
    let props: Vec<String> = props
        .split(',')
        .map(|pair| {
            if pair.starts_with("age=") {
                found = true;
                "age=0".to_owned()
            } else {
                pair.to_owned()
            }
        })
        .collect();
    found.then(|| format!("{block}[{}]", props.join(",")))
}

/// What the client makes of a click.
enum Predicted {
    /// A harvest the server will do.
    Harvest(String),
    /// A bare hand over a crop that needs a hoe.
    Warn,
    /// Nothing of ours.
    Nothing,
}

/// The client's side of a click: the same checks without any effect, so the swing and the
/// planting sound go with a harvest the server does.
fn predict(host: &dyn ClientHost, click: &BlockClick<'_>) -> Predicted {
    let Some(level) = host.level(click.dimension.key().id()) else {
        return Predicted::Nothing;
    };
    let Some(player) = host.local_player() else {
        return Predicted::Nothing;
    };
    let Some(cell) = Cell::read(host, level, click.pos) else {
        return Predicted::Nothing;
    };
    let Some(state) = player_state(host, player, click.hand) else {
        return Predicted::Nothing;
    };
    let (holding_hoe, _) = hoe(host, click.stack);
    let Some(kind) = cell.kind else {
        return Predicted::Nothing;
    };
    let harvestable = if kind.is_column() {
        true
    } else {
        cell.mature()
    };
    match rules::gate(
        state,
        settings(),
        cell.blacklisted,
        cell.hoe_never_required,
        holding_hoe,
    ) {
        Gate::Go { .. } => {}
        Gate::NeedHoe => {
            return if harvestable && click.stack.is_none() {
                Predicted::Warn
            } else {
                Predicted::Nothing
            };
        }
        Gate::Pass => return Predicted::Nothing,
    }
    if kind.is_column() {
        let id = cell.id.clone();
        let found = rules::column_break(click.pos, |at| {
            level
                .block(host, at)
                .is_some_and(|key| key.id(host) == Some(id.as_str()))
        });
        if found.is_none() {
            return Predicted::Nothing;
        }
    } else if !cell.mature() {
        return Predicted::Nothing;
    }
    Predicted::Harvest(cell.id)
}

#[derive(Default)]
struct Mod;

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn right_click_block(&self, host: HostSide<'_>, event: &mut RightClickBlockEvent<'_>) -> Flow {
        let click = event.click;
        let handled = match host {
            HostSide::Server(host) => {
                let Some(player) = host.player(click.player) else {
                    return Flow::Continue;
                };
                let Some(level) = host.level(click.dimension.key().id()) else {
                    return Flow::Continue;
                };
                let context = Click {
                    level,
                    player,
                    hand: click.hand,
                    stack: click.stack,
                    face_up: click.hit.face == [0, 1, 0],
                };
                harvest_at(host, &context, click.pos, true)
            }
            HostSide::Client(host) => match predict(&*host, click) {
                Predicted::Harvest(id) => {
                    let sound = if id == "minecraft:nether_wart" {
                        "minecraft:item.nether_wart.plant"
                    } else {
                        "minecraft:item.crop.plant"
                    };
                    host.play_local_sound(&SoundPlay::ui(sound), 0);
                    true
                }
                Predicted::Warn => {
                    // `hasUserBeenWarnedForNotUsingHoe`: told once, saved, then left alone.
                    if !settings().warned && !WARNED.swap(true, Ordering::Relaxed) {
                        let _ = host.chat_line(Text::translate(
                            "text.rightclickharvest.use_a_hoe_warning",
                            vec![
                                green(Text::translate(
                                    "config.rightclickharvest.requireHoe",
                                    vec![],
                                )),
                                green(Text::literal("false")),
                            ],
                        ));
                        save_warned(&*host);
                    }
                    false
                }
                Predicted::Nothing => false,
            },
        };
        if handled {
            *event.cancellation_result = InteractionResult::Success;
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        let _ = BLOCK_TAGS.set(BlockTags {
            blacklist: r.tag_key("rightclickharvest:blacklist"),
            hoe_never_required: r.tag_key("rightclickharvest:hoe_never_required"),
            radius_blacklist: r.tag_key("rightclickharvest:radius_harvest_blacklist"),
            crops: r.tag_key("minecraft:crops"),
        });
        let listener = |event: ConfigEvent<'_>, snapshot: &ConfigSnapshot| -> Result<(), String> {
            if matches!(event, ConfigEvent::Loading(_) | ConfigEvent::Reloading(_)) {
                *SETTINGS.lock().unwrap_or_else(|e| e.into_inner()) = Config::parse(&snapshot.text);
            }
            Ok(())
        };
        let spec = ConfigSpec::new(ConfigKind::Common, rules::DEFAULT_CONFIG);
        // A refused registration leaves the defaults in charge.
        let _ = r.register_config(spec, Some(Arc::new(listener)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replanting_resets_only_the_age() {
        assert_eq!(
            with_age_zero("minecraft:cocoa[age=2,facing=west]").as_deref(),
            Some("minecraft:cocoa[age=0,facing=west]")
        );
        assert_eq!(
            with_age_zero("farmersdelight:tomatoes[age=3,ropelogged=true]").as_deref(),
            Some("farmersdelight:tomatoes[age=0,ropelogged=true]")
        );
        assert_eq!(with_age_zero("minecraft:stone"), None);
    }
}
