//! Better Days for Sunrise: the overworld's clock runs at its own day and night speeds, a night's
//! sleep speeds it up with the share of players in bed, and the players are told when someone lies
//! down or gets up (`TimeService` and `BetterDaysMessages` in the original).
//!
//! | Moment | Where | What it does |
//! |---|---|---|
//! | `level_load`, `level_unload` | this module | the overworld's sleep state (`TimeServiceManager`) |
//! | `level_tick_post` | [`tick_overworld`] | sets the clock's rate: day, night or sleep speed; the morning at sunrise |
//! | `sleep_skip_night` | this module | holds vanilla's night skip back until the morning (`SleepStatus`) |
//! | `player_bed_enter`, `player_bed_leave` | this module | the "is now sleeping" and "has left their bed" notices |
//! | `screen_render_post` | this module | the clock drawn on the in-bed screen |
//! | `register` | this module | notes whether SleepWarp or Sleep Tight is loaded, which turns sleep off |
//! | `register` (configs) | this module | `config/betterdays-common.toml` and `config/betterdays-client.toml` |

pub mod rules;

use std::ops::ControlFlow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use mod_api::{
    ConfigEvent, ConfigKind, ConfigSnapshot, ConfigSpec, Flow, GameRuleValue, Hooks, HostSide,
    Identifier, LevelLoadEvent, LevelRef, LevelTickPostEvent, LevelUnloadEvent,
    PlayerBedEnterEvent, PlayerBedLeaveEvent, PlayerRef, Registrar, RegistrationContext,
    ScreenRenderPostEvent, ServerHost, SleepSkipNightEvent, Text,
};
use rules::{Client, Common, Interpolator, Message, Target};

const OVERWORLD: &str = "minecraft:overworld";
const ADVANCE_TIME: &str = "minecraft:advance_time";
const SLEEPING_PERCENTAGE: &str = "minecraft:players_sleeping_percentage";

struct Loaded {
    common: Common,
    spline: Option<Interpolator>,
}

static COMMON: Mutex<Option<Loaded>> = Mutex::new(None);
static CLIENT: Mutex<Option<Client>> = Mutex::new(None);

fn load_common(text: &str) {
    let common = Common::parse(text);
    let spline = Interpolator::new(&common.points, common.smoothing);
    *COMMON.lock().unwrap_or_else(|e| e.into_inner()) = Some(Loaded { common, spline });
}

fn with_common<R>(f: impl FnOnce(&Common, Option<&Interpolator>) -> R) -> R {
    let mut guard = COMMON.lock().unwrap_or_else(|e| e.into_inner());
    let loaded = guard.get_or_insert_with(|| {
        let common = Common::default();
        let spline = Interpolator::new(&common.points, common.smoothing);
        Loaded { common, spline }
    });
    f(&loaded.common, loaded.spline.as_ref())
}

/// Whether SleepWarp or Sleep Tight is loaded, read at startup: either one turns the sleep
/// feature off (`ConfigHandler.Common.enableSleepFeature`).
static SLEEP_MOD_LOADED: AtomicBool = AtomicBool::new(false);

/// `ConfigHandler.Common.enableSleepFeature`: the setting, unless another sleep mod is loaded.
fn sleep_feature(common: &Common) -> bool {
    common.sleep_feature && !SLEEP_MOD_LOADED.load(Ordering::Relaxed)
}

fn client() -> Client {
    CLIENT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(Client::default)
        .clone()
}

/// The overworld's sleep state while it is open (`TimeService`): the `players_sleeping_percentage`
/// rule when the level opened, and whether this night's morning was handled.
struct Service {
    default_percentage: i32,
    morning_handled: bool,
}

static SERVICE: Mutex<Option<Service>> = Mutex::new(None);

fn with_service<R>(f: impl FnOnce(&mut Service) -> R) -> Option<R> {
    SERVICE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
        .map(f)
}

fn set_sleeping_percentage(host: &mut dyn ServerHost, level: LevelRef, percentage: i32) {
    level.set_game_rule(host, SLEEPING_PERCENTAGE, GameRuleValue::Int(percentage));
}

fn rule_on(host: &dyn mod_api::Host, level: LevelRef, rule: &str) -> bool {
    matches!(
        level.game_rule(host, rule),
        Ok(Some(GameRuleValue::Bool(true)))
    )
}

fn asleep(host: &dyn mod_api::Host, player: PlayerRef) -> bool {
    host.living(player.entity())
        .and_then(|living| living.is_sleeping(host))
        == Some(true)
}

/// The level's players who count for sleeping, and how many of them are in bed.
fn sleep_counts(host: &dyn mod_api::Host, level: LevelRef) -> (usize, usize) {
    let (mut active, mut sleeping) = (0, 0);
    let _ = level.visit_online_players(host, &mut |player| {
        if player.counts_for_sleep(host) == Some(true) {
            active += 1;
            if asleep(host, player) {
                sleeping += 1;
            }
        }
        ControlFlow::Continue(())
    });
    (active, sleeping)
}

fn players_of(host: &dyn mod_api::Host, level: LevelRef) -> Vec<PlayerRef> {
    let mut players = Vec::new();
    let _ = level.visit_online_players(host, &mut |player| {
        players.push(player);
        ControlFlow::Continue(())
    });
    players
}

/// The text with its `§` codes turned into styled runs.
fn text_of(line: &str) -> Text {
    rules::legacy_runs(line)
        .into_iter()
        .fold(Text::literal(""), |root, (run, look)| {
            let mut part = Text::literal(run);
            let style = &mut part.style;
            style.color = look.color.map(str::to_owned);
            style.bold = look.bold.then_some(true);
            style.italic = look.italic.then_some(true);
            style.underlined = look.underlined.then_some(true);
            style.strikethrough = look.strikethrough.then_some(true);
            style.obfuscated = look.obfuscated.then_some(true);
            root.append(part)
        })
}

/// `BetterDaysMessages.sendMessage`: the template filled in and sent to who the message names.
fn send(
    host: &mut dyn ServerHost,
    message: &Message,
    level: LevelRef,
    variables: &[(&str, String)],
) {
    if message.text.is_empty() {
        return;
    }
    let line = text_of(&rules::fill(&message.text, variables));
    let mut targets: Vec<PlayerRef> = match message.target {
        Target::Dimension => players_of(&*host, level),
        Target::Sleeping => players_of(&*host, level)
            .into_iter()
            .filter(|p| asleep(&*host, *p))
            .collect(),
        Target::All => [OVERWORLD, "minecraft:the_nether", "minecraft:the_end"]
            .into_iter()
            .filter_map(|dimension| host.level(dimension))
            .flat_map(|l| players_of(&*host, l))
            .collect(),
    };
    targets.dedup();
    for player in targets {
        let _ = if message.chat.is_overlay() {
            player.action_bar(host, line.clone())
        } else {
            player.tell(host, line.clone())
        };
    }
}

fn percent(sleeping: usize, active: usize) -> usize {
    if active == 0 {
        0
    } else {
        (sleeping as f64 / active as f64 * 100.0) as usize
    }
}

fn notice_variables(name: &str, active: usize, sleeping: usize) -> Vec<(&'static str, String)> {
    vec![
        ("player", name.to_owned()),
        ("totalPlayers", active.to_string()),
        ("sleepingPlayers", sleeping.to_string()),
        ("sleepingPercentage", percent(sleeping, active).to_string()),
    ]
}

/// `TimeService.tick`: the clock's rate for this tick, then, while players sleep, the morning
/// once the clock is past sunrise; once everyone is awake after it, vanilla's sleep rule again.
fn tick_overworld(host: &mut dyn ServerHost, level: LevelRef) {
    if !rule_on(&*host, level, ADVANCE_TIME) {
        return;
    }
    let (Some(clock), Some(world_clock)) = (level.clock_time(&*host), level.clock(&*host)) else {
        return;
    };
    let (active, sleeping) = sleep_counts(&*host, level);
    let (feature, rate) = with_common(|common, spline| {
        let feature = sleep_feature(common);
        let sleep = if feature {
            common.sleep_speed(sleeping, active)
        } else {
            None
        };
        let rate = sleep.unwrap_or_else(|| common.time_speed(clock as f64, spline));
        (feature, rate)
    });
    world_clock.set_rate(host, rate as f32);
    if !feature {
        return;
    }
    if sleeping > 0 {
        if rules::past_sunrise(clock)
            && with_service(|service| !service.morning_handled) == Some(true)
        {
            morning(host, level, active, sleeping);
        }
    } else if rules::past_day_start(clock)
        && let Some(Some(default)) = with_service(|service| {
            std::mem::take(&mut service.morning_handled).then_some(service.default_percentage)
        })
    {
        set_sleeping_percentage(host, level, default);
    }
}

/// `TimeService.handleMorning`: the morning notice, and vanilla's night skip let through with
/// the sleeping percentage at 0, so it wakes everyone on the next tick.
fn morning(host: &mut dyn ServerHost, level: LevelRef, active: usize, sleeping: usize) {
    let message = with_common(|common, _| common.morning.clone());
    send(
        host,
        &message,
        level,
        &notice_variables("", active, sleeping),
    );
    set_sleeping_percentage(host, level, 0);
    with_service(|service| service.morning_handled = true);
}

fn level_of(host: &mut dyn ServerHost, uuid: u128) -> Option<LevelRef> {
    host.player(uuid)?.level(&*host)
}

fn is_overworld(host: &dyn mod_api::Host, level: LevelRef) -> bool {
    level.dimension().id(host) == Some(OVERWORLD)
}

#[derive(Default)]
struct Mod;

/// `BetterDaysMessages.onEnterBed`.
fn enter_notice(host: &mut dyn ServerHost, event: &PlayerBedEnterEvent<'_>) {
    let Some(level) = level_of(host, event.player) else {
        return;
    };
    if !rule_on(&*host, level, ADVANCE_TIME) {
        return;
    }
    let (active, sleeping) = sleep_counts(&*host, level);
    let message = with_common(|common, _| sleep_feature(common).then(|| common.enter_bed.clone()));
    if let Some(message) = message.filter(|_| active > 1) {
        let variables = notice_variables(event.name, active, sleeping + 1);
        send(host, &message, level, &variables);
    }
}

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn level_load(&self, host: &mut dyn ServerHost, event: &mut LevelLoadEvent<'_>) {
        if event.dimension != OVERWORLD {
            return;
        }
        let rule = host
            .level(OVERWORLD)
            .and_then(|level| level.game_rule(&*host, SLEEPING_PERCENTAGE).ok().flatten());
        let Some(GameRuleValue::Int(default_percentage)) = rule else {
            return;
        };
        *SERVICE.lock().unwrap_or_else(|e| e.into_inner()) = Some(Service {
            default_percentage,
            morning_handled: false,
        });
    }

    fn level_unload(&self, _: &mut dyn ServerHost, event: &mut LevelUnloadEvent<'_>) {
        if event.dimension == OVERWORLD {
            *SERVICE.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
    }

    /// `SleepStatus.areEnoughSleeping` answering false: with the sleep feature on, the night is
    /// slept through at the sleep speed and only skipped once the morning is handled.
    fn sleep_skip_night(
        &self,
        _: &mut dyn ServerHost,
        event: &mut SleepSkipNightEvent<'_>,
    ) -> Flow {
        if event.dimension == OVERWORLD
            && with_common(|common, _| sleep_feature(common))
            && with_service(|service| !service.morning_handled) == Some(true)
        {
            event.cancelled = true;
        }
        Flow::Continue
    }

    fn level_tick_post(&self, host: HostSide<'_>, event: &mut LevelTickPostEvent) {
        if let HostSide::Server(host) = host
            && is_overworld(&*host, event.level)
        {
            tick_overworld(host, event.level);
        }
    }

    fn player_bed_enter(
        &self,
        host: &mut dyn ServerHost,
        event: &mut PlayerBedEnterEvent<'_>,
    ) -> Flow {
        if event.cancelled {
            return Flow::Continue;
        }
        enter_notice(host, event);
        Flow::Continue
    }

    fn player_bed_leave(&self, host: &mut dyn ServerHost, event: &mut PlayerBedLeaveEvent<'_>) {
        if !event.update_level {
            return;
        }
        let Some(level) = level_of(host, event.player) else {
            return;
        };
        let Some(clock) = level.clock_time(&*host) else {
            return;
        };
        if !rule_on(&*host, level, ADVANCE_TIME) || rules::past_sunrise(clock) {
            return;
        }
        let (active, sleeping) = sleep_counts(&*host, level);
        let message =
            with_common(|common, _| sleep_feature(common).then(|| common.leave_bed.clone()));
        let others = sleeping.saturating_sub(1);
        if let Some(message) = message.filter(|_| active > 1 && others > 0) {
            let variables = notice_variables(event.name, active, others);
            send(host, &message, level, &variables);
        }
    }

    fn screen_render_post(
        &self,
        host: &dyn mod_api::ClientHost,
        event: &mut ScreenRenderPostEvent<'_>,
    ) {
        if event.screen.id != "chat" {
            return;
        }
        let shown = with_common(|common, _| sleep_feature(common) && common.display_bed_clock);
        let sleeping = host
            .local_player()
            .is_some_and(|player| asleep(host, player));
        if !shown || !sleeping {
            return;
        }
        let Some(clock) = Identifier::parse("minecraft:clock")
            .ok()
            .and_then(|id| host.item_key(&id))
        else {
            return;
        };
        let client = client();
        let [width, height] = event.draw.size();
        event.draw.item(
            clock,
            client.clock_origin(width, height),
            client.scale as f32,
        );
    }

    fn register(&self, r: &mut dyn Registrar, context: &RegistrationContext) {
        let other = context
            .mods()
            .ids()
            .any(|id| id == "sleepwarp" || id == "sleep_tight");
        SLEEP_MOD_LOADED.store(other, Ordering::Relaxed);
        for (kind, default) in [
            (ConfigKind::Common, rules::DEFAULT_COMMON),
            (ConfigKind::Client, rules::DEFAULT_CLIENT),
        ] {
            let listener =
                move |event: ConfigEvent<'_>, snapshot: &ConfigSnapshot| -> Result<(), String> {
                    if matches!(event, ConfigEvent::Loading(_) | ConfigEvent::Reloading(_)) {
                        apply(kind, &snapshot.text);
                    }
                    Ok(())
                };
            // A duplicate file name is refused and the first registration stays active.
            let _ = r.register_config(ConfigSpec::new(kind, default), Some(Arc::new(listener)));
        }
    }
}

fn apply(kind: ConfigKind, text: &str) {
    match kind {
        ConfigKind::Common => load_common(text),
        ConfigKind::Client => {
            *CLIENT.lock().unwrap_or_else(|e| e.into_inner()) = Some(Client::parse(text));
        }
        _ => {}
    }
}
