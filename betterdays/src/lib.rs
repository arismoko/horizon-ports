//! Better Days for Sunrise: the overworld's clock runs at its own day and night speeds, a night's
//! sleep speeds it up with the share of players in bed, and the players are told when someone lies
//! down or gets up (`TimeService` and `BetterDaysMessages` in the original).
//!
//! | Moment | Where | What it does |
//! |---|---|---|
//! | `level_tick_post` | [`tick_overworld`] | sets the clock's rate: day, night or sleep speed |
//! | `sleep_finished_time` | this module | the morning notice |
//! | `player_bed_enter`, `player_bed_leave` | this module | the "is now sleeping" and "has left their bed" notices |
//! | `screen_render_post` | this module | the clock drawn on the in-bed screen |
//! | `register` | this module | notes whether SleepWarp or Sleep Tight is loaded, which turns sleep off |
//! | `register_configs`, `config_loading`, `config_reloading` | this module | `config/betterdays-common.toml` and `config/betterdays-client.toml` |

pub mod rules;

use std::ops::ControlFlow;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use mod_api::{
    Config as ModConfig, ConfigContext, ConfigKind, ConfigLoadingEvent, ConfigReloadingEvent, Flow,
    GameRuleValue, Hooks, HostSide, Identifier, LevelRef, LevelTickPostEvent, PlayerBedEnterEvent,
    PlayerBedLeaveEvent, PlayerRef, RegisterConfigsEvent, Registrar, RegistrationContext,
    ScreenRenderPostEvent, ServerHost, SleepFinishedTimeEvent, Text,
};
use rules::{Client, Common, Interpolator, Message, Target};

const MOD: &str = "betterdays";
const OVERWORLD: &str = "minecraft:overworld";
const ADVANCE_TIME: &str = "minecraft:advance_time";
/// Ticks between two sends of the rate line while it has not changed.
const RATE_REFRESH: u32 = 200;

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

/// The rate last sent, as float bits, and the ticks since.
static RATE: Mutex<Option<(u32, u32)>> = Mutex::new(None);

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

/// `TimeService.tick`: the clock's rate for this tick.
fn tick_overworld(host: &mut dyn ServerHost, level: LevelRef) {
    if !rule_on(&*host, level, ADVANCE_TIME) {
        return;
    }
    let Some(clock) = level.clock_time(&*host) else {
        return;
    };
    let (active, sleeping) = sleep_counts(&*host, level);
    let rate = with_common(|common, spline| {
        let sleep = if sleep_feature(common) {
            common.sleep_speed(sleeping, active)
        } else {
            None
        };
        sleep.unwrap_or_else(|| common.time_speed(clock as f64, spline))
    });
    send_rate(host, rate);
}

fn send_rate(host: &mut dyn ServerHost, rate: f64) {
    let bits = (rate as f32).to_bits();
    {
        let mut last = RATE.lock().unwrap_or_else(|e| e.into_inner());
        match &mut *last {
            Some((sent, since)) if *sent == bits && *since < RATE_REFRESH => {
                *since += 1;
                return;
            }
            slot => *slot = Some((bits, 0)),
        }
    }
    host.commands().run_command(&rules::rate_command(rate));
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

    fn sleep_finished_time(
        &self,
        host: &mut dyn ServerHost,
        event: &mut SleepFinishedTimeEvent<'_>,
    ) -> Flow {
        if event.dimension == OVERWORLD
            && let Some(level) = host.level(OVERWORLD)
        {
            let message =
                with_common(|common, _| sleep_feature(common).then(|| common.morning.clone()));
            if let Some(message) = message {
                let (active, sleeping) = sleep_counts(&*host, level);
                send(
                    host,
                    &message,
                    level,
                    &notice_variables("", active, sleeping),
                );
            }
        }
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

    fn register(&self, _: &mut dyn Registrar, context: &RegistrationContext) {
        let other = context
            .mods()
            .ids()
            .any(|id| id == "sleepwarp" || id == "sleep_tight");
        SLEEP_MOD_LOADED.store(other, Ordering::Relaxed);
    }

    fn register_configs(&self, _: &dyn ConfigContext, event: &mut RegisterConfigsEvent<'_>) {
        event.configs.push(ModConfig::new(
            MOD,
            ConfigKind::Common,
            rules::DEFAULT_COMMON,
        ));
        event.configs.push(ModConfig::new(
            MOD,
            ConfigKind::Client,
            rules::DEFAULT_CLIENT,
        ));
    }

    fn config_loading(&self, _: &dyn ConfigContext, event: &mut ConfigLoadingEvent<'_>) {
        apply(
            event.config.mod_id.as_str(),
            event.config.kind,
            &event.config.text,
        );
    }

    fn config_reloading(&self, _: HostSide, event: &mut ConfigReloadingEvent<'_>) {
        apply(
            event.config.mod_id.as_str(),
            event.config.kind,
            &event.config.text,
        );
    }
}

fn apply(mod_id: &str, kind: ConfigKind, text: &str) {
    if mod_id != MOD {
        return;
    }
    match kind {
        ConfigKind::Common => load_common(text),
        ConfigKind::Client => {
            *CLIENT.lock().unwrap_or_else(|e| e.into_inner()) = Some(Client::parse(text));
        }
        _ => {}
    }
}
