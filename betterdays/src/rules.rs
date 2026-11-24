//! The rules of Better Days as plain functions: its two config files, the clock speeds, the sleep
//! speed curve, the interpolated speeds, the messages' templates and the bed clock's place.

use std::collections::HashMap;

pub const DAY_TICKS: i64 = 24_000;
/// `TimeService.SUNRISE`: past this tick of the day a night's sleep is over.
pub const SUNRISE: i64 = 23_000;
/// The season day used when no seasons mod gives one (`SeasonHelper.days[0]`).
pub const DEFAULT_SEASON_DAY: i32 = 110;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeedMethod {
    Ratio,
    Minutes,
    Realtime,
    Season,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Condition {
    Never,
    Sleeping,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatType {
    System,
    GameInfo,
}

impl ChatType {
    pub fn is_overlay(self) -> bool {
        self == Self::GameInfo
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    All,
    Dimension,
    Sleeping,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    CenterCenter,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

/// One of the three notices: its template, where it shows and who gets it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub text: String,
    pub chat: ChatType,
    pub target: Target,
}

/// `ConfigHandler.Common`.
#[derive(Clone, Debug, PartialEq)]
pub struct Common {
    pub speed_method: SpeedMethod,
    pub day_speed: f64,
    pub night_speed: f64,
    pub day_speed_minutes: f64,
    pub night_speed_minutes: f64,
    pub season_day_minutes: f64,
    pub season_latitude: f64,
    pub day_start: f64,
    pub night_start: f64,
    pub interpolated: bool,
    pub smoothing: f64,
    pub points: Vec<(i64, f64)>,
    pub weather_effect: Condition,
    pub potion_effect: Condition,
    pub hunger_effect: Condition,
    pub block_entity_effect: Condition,
    pub sleep_feature: bool,
    pub sleep_speed_max: f64,
    pub sleep_speed_min: f64,
    pub sleep_speed_all: f64,
    pub sleep_speed_curve: f64,
    pub display_bed_clock: bool,
    pub players_for_sleep: f64,
    pub morning: Message,
    pub enter_bed: Message,
    pub leave_bed: Message,
}

/// `ConfigHandler.Client`.
#[derive(Clone, Debug, PartialEq)]
pub struct Client {
    pub alignment: Alignment,
    pub scale: i32,
    pub margin: i32,
}

impl Default for Client {
    fn default() -> Self {
        Self::parse("")
    }
}

impl Default for Common {
    fn default() -> Self {
        Self::parse("")
    }
}

pub const DEFAULT_CLIENT: &str = "[gui]\n\t#Sets the screen alignment of the bed clock.\n\t#Allowed values: TOP_LEFT, TOP_CENTER, TOP_RIGHT, CENTER_LEFT, CENTER_CENTER, CENTER_RIGHT, BOTTOM_LEFT, BOTTOM_CENTER, BOTTOM_RIGHT\n\tclockAlignment = \"TOP_RIGHT\"\n\t#Sets the scale of the bed clock.\n\tclockScale = 64\n\t#Sets the distance between the clock and the edge of the screen.\n\t#Unused if clockAlignment is CENTER_CENTER.\n\tclockMargin = 16\n\tpreventClockWobble = true\n\tblacklistDimensions = [\"aether:the_aether\"]\n";

pub const DEFAULT_COMMON: &str = "[time]\n\t#RATIO: daySpeed and nightSpeed. MINUTES: daySpeedMinutes and nightSpeedMinutes.\n\t#REALTIME: 12 real-world hours each. SEASON: seasonDayMinutes and seasonLatitude.\n\tspeedMethod = \"RATIO\"\n\t#The ratio of time passing relative to vanilla. Vanilla speed: 1.0\n\tdaySpeed = 1.0\n\tnightSpeed = 1.0\n\t#The length in real-world minutes. Vanilla length: 10.0 minutes\n\tdaySpeedMinutes = 10.0\n\tnightSpeedMinutes = 10.0\n\tseasonDayMinutes = 20.0\n\tseasonLatitude = 48.0\n\t#The time to start day. Default: 23500\n\tdayStart = 23500.0\n\t#The time to start night. Default: 12500\n\tnightStart = 12500.0\n\t#Pairs (tick, speed) with spline interpolation between them. The pairs at 0 and 24000 are bounds.\n\tenableInterpolatedTime = false\n\tinterpolatedTimeSmoothingFactor = 0.25\n\tinterpolatedTimeList = [\"0,1.0\", \"24000,1.0\"]\n\n\t[time.effects]\n\t#NEVER or SLEEPING.\n\tweatherEffect = \"SLEEPING\"\n\tpotionEffect = \"NEVER\"\n\thungerEffect = \"NEVER\"\n\tblockEntityEffect = \"NEVER\"\n\n[sleep]\n\tenableSleepFeature = true\n\t#The maximum speed at which time passes when all players are sleeping.\n\tsleepSpeedMax = 110.0\n\t#The minimum speed at which time passes when only 1 player is sleeping in a full server.\n\tsleepSpeedMin = 1.0\n\t#Set to -1 to disable (sleepSpeedMax is used when all players are sleeping).\n\tsleepSpeedAll = -1.0\n\t#A Normalized Tunable Sigmoid Function: 0.5 is linear.\n\tsleepSpeedCurve = 0.3\n\tdisplayBedClock = true\n\tratioPlayersForSleep = 0.0\n\n\t[sleep.messages]\n\t\t#Messages use ${player}, ${totalPlayers}, ${sleepingPlayers} and ${sleepingPercentage}. Empty sends nothing.\n\n\t\t[sleep.messages.morning]\n\t\t\tmessage = \"§e§oTempus fugit!\"\n\t\t\t#SYSTEM or GAME_INFO.\n\t\t\ttype = \"GAME_INFO\"\n\t\t\t#ALL, DIMENSION or SLEEPING.\n\t\t\ttarget = \"DIMENSION\"\n\n\t\t[sleep.messages.enterBed]\n\t\t\tmessage = \"${player} is now sleeping. [${sleepingPlayers}/${totalPlayers}]\"\n\t\t\ttype = \"GAME_INFO\"\n\t\t\ttarget = \"DIMENSION\"\n\n\t\t[sleep.messages.leaveBed]\n\t\t\tmessage = \"${player} has left their bed. [${sleepingPlayers}/${totalPlayers}]\"\n\t\t\ttype = \"GAME_INFO\"\n\t\t\ttarget = \"DIMENSION\"\n";

/// Every `section.key = value` of a config file's text, the value as written (a string's quotes
/// and a list's brackets kept), the last of a key winning.
fn values(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut section = String::new();
    let mut pending: Option<(String, String)> = None;
    for raw in text.lines() {
        let line = strip_comment(raw).trim().to_owned();
        if let Some((key, mut value)) = pending.take() {
            value.push_str(&line);
            if line.contains(']') {
                map.insert(key, value);
            } else {
                pending = Some((key, value));
            }
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.trim().to_owned();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = format!("{section}.{}", key.trim());
        let value = value.trim().to_owned();
        if value.starts_with('[') && !value.contains(']') {
            pending = Some((key, value));
        } else {
            map.insert(key, value);
        }
    }
    map
}

/// The line without a `#` comment that is outside a string.
fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '#' if !quoted => return &line[..i],
            _ => {}
        }
    }
    line
}

fn text_of(value: &str) -> &str {
    value.trim().trim_matches('"')
}

fn float(map: &HashMap<String, String>, key: &str, default: f64, lo: f64, hi: f64) -> f64 {
    map.get(key)
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| (lo..=hi).contains(v))
        .unwrap_or(default)
}

fn int(map: &HashMap<String, String>, key: &str, default: i32, lo: i32, hi: i32) -> i32 {
    map.get(key)
        .and_then(|v| v.trim().parse::<i32>().ok())
        .filter(|v| (lo..=hi).contains(v))
        .unwrap_or(default)
}

fn flag(map: &HashMap<String, String>, key: &str, default: bool) -> bool {
    match map.get(key).map(|v| text_of(v)) {
        Some("true") => true,
        Some("false") => false,
        _ => default,
    }
}

fn choice<T: Copy>(map: &HashMap<String, String>, key: &str, default: T, names: &[(&str, T)]) -> T {
    map.get(key)
        .and_then(|v| {
            let v = text_of(v);
            names
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(v))
                .map(|(_, value)| *value)
        })
        .unwrap_or(default)
}

/// `defineList` of `"tick,speed"` strings, as `valuePairValidator` and `pairTransformer` read
/// them: an entry that does not fit is left out.
fn pairs(map: &HashMap<String, String>, key: &str) -> Vec<(i64, f64)> {
    let Some(list) = map.get(key) else {
        return vec![(0, 1.0), (24_000, 1.0)];
    };
    let list: Vec<(i64, f64)> = list
        .split('"')
        .skip(1)
        .step_by(2)
        .filter_map(|entry| {
            let (tick, speed) = entry.split_once(',')?;
            Some((tick.trim().parse().ok()?, speed.trim().parse().ok()?))
        })
        .collect();
    list
}

fn message(map: &HashMap<String, String>, section: &str, text: &str, target: Target) -> Message {
    let key = |name: &str| format!("sleep.messages.{section}.{name}");
    let targets = [
        ("ALL", Target::All),
        ("DIMENSION", Target::Dimension),
        ("SLEEPING", Target::Sleeping),
    ];
    Message {
        text: map
            .get(&key("message"))
            .map_or_else(|| text.to_owned(), |v| unescape(text_of(v))),
        chat: choice(
            map,
            &key("type"),
            ChatType::GameInfo,
            &[
                ("SYSTEM", ChatType::System),
                ("GAME_INFO", ChatType::GameInfo),
            ],
        ),
        target: choice(map, &key("target"), target, &targets),
    }
}

fn unescape(text: &str) -> String {
    text.replace("\\\"", "\"").replace("\\\\", "\\")
}

impl Client {
    pub fn parse(text: &str) -> Self {
        let map = values(text);
        Self {
            alignment: choice(
                &map,
                "gui.clockAlignment",
                Alignment::TopRight,
                &[
                    ("TOP_LEFT", Alignment::TopLeft),
                    ("TOP_CENTER", Alignment::TopCenter),
                    ("TOP_RIGHT", Alignment::TopRight),
                    ("CENTER_LEFT", Alignment::CenterLeft),
                    ("CENTER_CENTER", Alignment::CenterCenter),
                    ("CENTER_RIGHT", Alignment::CenterRight),
                    ("BOTTOM_LEFT", Alignment::BottomLeft),
                    ("BOTTOM_CENTER", Alignment::BottomCenter),
                    ("BOTTOM_RIGHT", Alignment::BottomRight),
                ],
            ),
            scale: int(&map, "gui.clockScale", 64, 1, i32::MAX),
            margin: int(&map, "gui.clockMargin", 16, 0, i32::MAX),
        }
    }

    /// `SleepGui.renderSleepInterface`: where the clock's top left corner goes on a screen of
    /// `width` by `height`.
    pub fn clock_origin(&self, width: f32, height: f32) -> [f32; 2] {
        use Alignment::*;
        let scale = self.scale as f32;
        let margin = self.margin as f32;
        let x = match self.alignment {
            TopLeft | CenterLeft | BottomLeft => margin,
            TopCenter | CenterCenter | BottomCenter => width / 2.0 - scale / 2.0,
            _ => width - scale - margin,
        };
        let y = match self.alignment {
            TopLeft | TopCenter | TopRight => margin,
            CenterLeft | CenterCenter | CenterRight => height / 2.0 - scale / 2.0,
            _ => height - scale - margin,
        };
        [x, y]
    }
}

impl Common {
    pub fn parse(text: &str) -> Self {
        let map = values(text);
        let conditions = [
            ("NEVER", Condition::Never),
            ("SLEEPING", Condition::Sleeping),
        ];
        let effect =
            |key: &str, default| choice(&map, &format!("time.effects.{key}"), default, &conditions);
        Self {
            speed_method: choice(
                &map,
                "time.speedMethod",
                SpeedMethod::Ratio,
                &[
                    ("RATIO", SpeedMethod::Ratio),
                    ("MINUTES", SpeedMethod::Minutes),
                    ("REALTIME", SpeedMethod::Realtime),
                    ("SEASON", SpeedMethod::Season),
                ],
            ),
            day_speed: float(&map, "time.daySpeed", 1.0, 0.0, 24_000.0),
            night_speed: float(&map, "time.nightSpeed", 1.0, 0.0, 24_000.0),
            day_speed_minutes: float(&map, "time.daySpeedMinutes", 10.0, 0.1, 10_000.0),
            night_speed_minutes: float(&map, "time.nightSpeedMinutes", 10.0, 0.1, 10_000.0),
            season_day_minutes: float(&map, "time.seasonDayMinutes", 20.0, 0.1, 10_000.0),
            season_latitude: float(&map, "time.seasonLatitude", 48.0, -90.0, 90.0),
            day_start: float(&map, "time.dayStart", 23_500.0, 22_300.0, 24_000.0),
            night_start: float(&map, "time.nightStart", 12_500.0, 12_000.0, 13_000.0),
            interpolated: flag(&map, "time.enableInterpolatedTime", false),
            smoothing: float(
                &map,
                "time.interpolatedTimeSmoothingFactor",
                0.25,
                0.0,
                10.0,
            ),
            points: pairs(&map, "time.interpolatedTimeList"),
            weather_effect: effect("weatherEffect", Condition::Sleeping),
            potion_effect: effect("potionEffect", Condition::Never),
            hunger_effect: effect("hungerEffect", Condition::Never),
            block_entity_effect: effect("blockEntityEffect", Condition::Never),
            sleep_feature: flag(&map, "sleep.enableSleepFeature", true),
            sleep_speed_max: float(&map, "sleep.sleepSpeedMax", 110.0, 0.0, 24_000.0),
            sleep_speed_min: float(&map, "sleep.sleepSpeedMin", 1.0, 0.0, 24_000.0),
            sleep_speed_all: float(&map, "sleep.sleepSpeedAll", -1.0, -1.0, 24_000.0),
            sleep_speed_curve: float(&map, "sleep.sleepSpeedCurve", 0.3, 0.0, 1.0),
            display_bed_clock: flag(&map, "sleep.displayBedClock", true),
            players_for_sleep: float(&map, "sleep.ratioPlayersForSleep", 0.0, 0.0, 1.0),
            morning: message(&map, "morning", "§e§oTempus fugit!", Target::Dimension),
            enter_bed: message(
                &map,
                "enterBed",
                "${player} is now sleeping. [${sleepingPlayers}/${totalPlayers}]",
                Target::Dimension,
            ),
            leave_bed: message(
                &map,
                "leaveBed",
                "${player} has left their bed. [${sleepingPlayers}/${totalPlayers}]",
                Target::Dimension,
            ),
        }
    }

    /// `COMMON.speedMethod` speeds: `daySpeed(level)` and `nightSpeed(level)` in one.
    pub fn base_speed(&self, day: bool) -> f64 {
        let (ratio, minutes) = if day {
            (self.day_speed, self.day_speed_minutes)
        } else {
            (self.night_speed, self.night_speed_minutes)
        };
        match self.speed_method {
            SpeedMethod::Minutes => (100.0 / minutes) / 10.0,
            SpeedMethod::Realtime => (100.0 / 720.0) / 10.0,
            SpeedMethod::Season => {
                let share = day_ratio(DEFAULT_SEASON_DAY, self.season_latitude);
                let share = if day { share } else { 1.0 - share };
                (100.0 / (self.season_day_minutes * share)) / 10.0
            }
            SpeedMethod::Ratio => ratio,
        }
    }

    /// `TimeService.internalGetTimeSpeed` for a clock at `time`.
    pub fn time_speed(&self, time: f64, interpolator: Option<&Interpolator>) -> f64 {
        if let Some(interpolator) = interpolator.filter(|_| self.interpolated) {
            return interpolator.evaluate(time.rem_euclid(DAY_TICKS as f64).floor());
        }
        let t = time.rem_euclid(DAY_TICKS as f64);
        let day = t == self.day_start || between_mod(t, self.day_start, self.night_start);
        self.base_speed(day)
    }

    /// `TimeService.getSleepTimeSpeed`, for the sleeping share of the players: None when the
    /// ordinary speed applies.
    pub fn sleep_speed(&self, sleeping: usize, active: usize) -> Option<f64> {
        if sleeping == 0 {
            return None;
        }
        let ratio = sleeping as f64 / active as f64;
        if ratio < self.players_for_sleep {
            return None;
        }
        if sleeping == active && self.sleep_speed_all >= 0.0 {
            return Some(self.sleep_speed_all);
        }
        let shaped = sigmoid(ratio, self.sleep_speed_curve);
        Some(lerp(shaped, self.sleep_speed_min, self.sleep_speed_max))
    }
}

/// `Time.betweenMod(a, b)`: `t` is after `a` and before `b`, going round the day.
pub fn between_mod(t: f64, a: f64, b: f64) -> bool {
    if a == b {
        false
    } else if a < b {
        t > a && t < b
    } else {
        t > a || t < b
    }
}

/// `MathUtils.lerp`.
pub fn lerp(percent: f64, a: f64, b: f64) -> f64 {
    a + percent * (b - a)
}

/// `MathUtils.normalizedTunableSigmoid`.
pub fn sigmoid(x: f64, c: f64) -> f64 {
    c * x / (2.0 * c * x - c - x + 1.0)
}

/// `SeasonHelper.getDayRatio`: the share of the day that is daylight on `season_day` at a latitude.
pub fn day_ratio(season_day: i32, latitude: f64) -> f64 {
    let p = (0.39795
        * (0.2163108
            + 2.0 * (0.9671396 * (0.00860 * (f64::from(season_day) - 186.0)).tan()).atan())
        .cos())
    .asin();
    let lat = latitude.to_radians();
    let daylight = 24.0
        - (24.0 / std::f64::consts::PI)
            * (((0.8333f64).to_radians().sin() + lat.sin() * p.sin()) / (lat.cos() * p.cos()))
                .acos();
    daylight / 24.0
}

/// `MonotonicInterpolator`: a monotone cubic spline through (tick, speed) points.
#[derive(Clone, Debug, PartialEq)]
pub struct Interpolator {
    x: Vec<f64>,
    y: Vec<f64>,
    m: Vec<f64>,
}

impl Interpolator {
    /// None for fewer than two points, or points whose ticks do not rise (the original throws
    /// for the first and divides by zero for the second).
    pub fn new(points: &[(i64, f64)], smoothing: f64) -> Option<Self> {
        if points.len() < 2 || points.windows(2).any(|w| w[1].0 <= w[0].0) {
            return None;
        }
        let n = points.len();
        let x: Vec<f64> = points.iter().map(|p| p.0 as f64).collect();
        let y: Vec<f64> = points.iter().map(|p| p.1).collect();
        let delta: Vec<f64> = (0..n - 1)
            .map(|i| (y[i + 1] - y[i]) / (x[i + 1] - x[i]))
            .collect();
        let mut m = vec![0.0; n];
        m[0] = delta[0];
        for i in 1..n - 1 {
            m[i] = if delta[i - 1] * delta[i] > 0.0 {
                smoothing * (delta[i - 1] + delta[i]) / 2.0
            } else {
                0.0
            };
        }
        m[n - 1] = delta[n - 2];
        for i in 0..n - 1 {
            if delta[i] == 0.0 {
                m[i] = 0.0;
                m[i + 1] = 0.0;
            } else {
                let alpha = m[i] / delta[i];
                let beta = m[i + 1] / delta[i];
                let sum = alpha * alpha + beta * beta;
                if sum > 9.0 {
                    let tau = 3.0 / sum.sqrt();
                    m[i] = tau * alpha * delta[i];
                    m[i + 1] = tau * beta * delta[i];
                }
            }
        }
        Some(Self { x, y, m })
    }

    pub fn evaluate(&self, at: f64) -> f64 {
        let last = self.x.len() - 1;
        if at <= self.x[0] {
            return self.y[0];
        }
        if at >= self.x[last] {
            return self.y[last];
        }
        let i = self.x.partition_point(|&x| x <= at) - 1;
        let h = self.x[i + 1] - self.x[i];
        let t = (at - self.x[i]) / h;
        let (t2, t3) = (t * t, t * t * t);
        (2.0 * t3 - 3.0 * t2 + 1.0) * self.y[i]
            + (t3 - 2.0 * t2 + t) * h * self.m[i]
            + (-2.0 * t3 + 3.0 * t2) * self.y[i + 1]
            + (t3 - t2) * h * self.m[i + 1]
    }
}

/// `StrSubstitutor`'s `${name}` in `template`; an unknown name stays as written.
pub fn fill(template: &str, variables: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let name = &after[..end];
                match variables.iter().find(|(n, _)| *n == name) {
                    Some((_, value)) => out.push_str(value),
                    None => out.push_str(&rest[start..start + 3 + end]),
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// What the `§` codes set for a run of text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Look {
    pub color: Option<&'static str>,
    pub bold: bool,
    pub italic: bool,
    pub underlined: bool,
    pub strikethrough: bool,
    pub obfuscated: bool,
}

/// `text` split at its `§` formatting codes into runs with their look.
pub fn legacy_runs(text: &str) -> Vec<(String, Look)> {
    let mut runs: Vec<(String, Look)> = Vec::new();
    let mut look = Look::default();
    let mut current = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '§' {
            current.push(c);
            continue;
        }
        let Some(code) = chars.next() else { break };
        let code = code.to_ascii_lowercase();
        let colors = [
            ('0', "black"),
            ('1', "dark_blue"),
            ('2', "dark_green"),
            ('3', "dark_aqua"),
            ('4', "dark_red"),
            ('5', "dark_purple"),
            ('6', "gold"),
            ('7', "gray"),
            ('8', "dark_gray"),
            ('9', "blue"),
            ('a', "green"),
            ('b', "aqua"),
            ('c', "red"),
            ('d', "light_purple"),
            ('e', "yellow"),
            ('f', "white"),
        ];
        let next = if let Some((_, name)) = colors.iter().find(|(k, _)| *k == code) {
            Some(Look {
                color: Some(name),
                ..Look::default()
            })
        } else {
            match code {
                'l' => Some(Look { bold: true, ..look }),
                'o' => Some(Look {
                    italic: true,
                    ..look
                }),
                'n' => Some(Look {
                    underlined: true,
                    ..look
                }),
                'm' => Some(Look {
                    strikethrough: true,
                    ..look
                }),
                'k' => Some(Look {
                    obfuscated: true,
                    ..look
                }),
                'r' => Some(Look::default()),
                _ => None,
            }
        };
        if let Some(next) = next {
            if !current.is_empty() {
                runs.push((std::mem::take(&mut current), look));
            }
            look = next;
        }
    }
    if !current.is_empty() {
        runs.push((current, look));
    }
    runs
}

/// `Time.crossedMorning(SUNRISE, time)`: the clock is past the day's `SUNRISE` tick.
pub fn past_sunrise(clock: i64) -> bool {
    clock.rem_euclid(DAY_TICKS) > SUNRISE
}

/// `Time.crossedMorning(SUNRISE_END, time)`: the clock is past the day's first tick.
pub fn past_day_start(clock: i64) -> bool {
    clock.rem_euclid(DAY_TICKS) > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_files_are_the_defaults() {
        assert_eq!(Common::parse(DEFAULT_COMMON), Common::parse(""));
        assert_eq!(Client::parse(DEFAULT_CLIENT), Client::parse(""));
        let common = Common::parse(DEFAULT_COMMON);
        assert_eq!(common.morning.text, "§e§oTempus fugit!");
        assert_eq!(common.points, vec![(0, 1.0), (24_000, 1.0)]);
        assert_eq!(common.weather_effect, Condition::Sleeping);
        assert!(common.sleep_feature);
    }

    #[test]
    fn values_out_of_range_fall_back_to_the_default() {
        let common = Common::parse(
            "[time]\ndayStart = 100.0\nnightStart = 12600.0\ndaySpeedMinutes = 0.0\n",
        );
        assert_eq!(common.day_start, 23_500.0);
        assert_eq!(common.night_start, 12_600.0);
        assert_eq!(common.day_speed_minutes, 10.0);
    }

    #[test]
    fn config_files_read_sections_lists_and_messages() {
        let text = "[time]\nspeedMethod = \"MINUTES\"\ndaySpeedMinutes = 20.0 # slow\ninterpolatedTimeList = [\"0,0.5\",\n \"12000,2\", \"bad\",\n \"24000,0.5\"]\n[sleep.messages.enterBed]\nmessage = \"\"\ntarget = \"ALL\"\ntype = \"SYSTEM\"\n";
        let common = Common::parse(text);
        assert_eq!(common.speed_method, SpeedMethod::Minutes);
        assert_eq!(common.day_speed_minutes, 20.0);
        assert_eq!(common.points, vec![(0, 0.5), (12_000, 2.0), (24_000, 0.5)]);
        assert_eq!(common.enter_bed.text, "");
        assert_eq!(common.enter_bed.target, Target::All);
        assert!(!common.enter_bed.chat.is_overlay());
        assert_eq!(common.leave_bed.target, Target::Dimension);
    }

    #[test]
    fn minutes_set_the_speed_as_the_original_does() {
        let mut common = Common {
            speed_method: SpeedMethod::Minutes,
            day_speed_minutes: 25.0,
            night_speed_minutes: 5.0,
            ..Common::default()
        };
        assert!((common.time_speed(6000.0, None) - 0.4).abs() < 1e-9);
        assert!((common.time_speed(15_000.0, None) - 2.0).abs() < 1e-9);
        common.speed_method = SpeedMethod::Realtime;
        assert!((common.time_speed(6000.0, None) - 100.0 / 7200.0).abs() < 1e-9);
    }

    #[test]
    fn day_runs_from_day_start_to_night_start() {
        let common = Common {
            day_speed: 0.5,
            night_speed: 2.0,
            ..Common::default()
        };
        assert_eq!(common.time_speed(23_500.0, None), 0.5);
        assert_eq!(common.time_speed(23_999.0, None), 0.5);
        assert_eq!(common.time_speed(0.0, None), 0.5);
        assert_eq!(common.time_speed(12_499.0, None), 0.5);
        assert_eq!(common.time_speed(12_500.0, None), 2.0);
        assert_eq!(common.time_speed(20_000.0, None), 2.0);
    }

    #[test]
    fn the_season_speed_uses_early_spring_at_the_latitude() {
        let ratio = day_ratio(DEFAULT_SEASON_DAY, 48.0);
        assert!((0.5..0.6).contains(&ratio), "{ratio}");
        let common = Common {
            speed_method: SpeedMethod::Season,
            ..Common::default()
        };
        assert!((common.base_speed(true) - 10.0 / (20.0 * ratio)).abs() < 1e-9);
        assert!((common.base_speed(false) - 10.0 / (20.0 * (1.0 - ratio))).abs() < 1e-9);
    }

    #[test]
    fn sleep_speed_follows_the_sleeping_share() {
        let common = Common::default();
        assert_eq!(common.sleep_speed(0, 4), None);
        assert!((common.sleep_speed(4, 4).unwrap() - 110.0).abs() < 1e-9);
        let one_in_four = common.sleep_speed(1, 4).unwrap();
        assert!(one_in_four > 1.0 && one_in_four < 110.0);
        let half = common.sleep_speed(2, 4).unwrap();
        assert!(half > one_in_four);
        let mut all = common.clone();
        all.sleep_speed_all = 200.0;
        assert_eq!(all.sleep_speed(3, 3), Some(200.0));
        assert_eq!(all.sleep_speed(1, 3).map(|s| s < 200.0), Some(true));
        let needs = Common {
            players_for_sleep: 0.5,
            ..common
        };
        assert_eq!(needs.sleep_speed(1, 4), None);
        assert!(needs.sleep_speed(2, 4).is_some());
    }

    #[test]
    fn the_sigmoid_is_linear_at_one_half_and_fixes_its_ends() {
        assert!((sigmoid(0.3, 0.5) - 0.3).abs() < 1e-12);
        assert_eq!(sigmoid(0.0, 0.3), 0.0);
        assert!((sigmoid(1.0, 0.3) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn the_interpolator_keeps_to_its_points_and_stays_monotone() {
        let points = [(0, 1.0), (6000, 3.0), (12_000, 3.0), (24_000, 1.0)];
        let spline = Interpolator::new(&points, 0.25).unwrap();
        assert_eq!(spline.evaluate(0.0), 1.0);
        assert_eq!(spline.evaluate(6000.0), 3.0);
        assert_eq!(spline.evaluate(24_000.0), 1.0);
        assert_eq!(spline.evaluate(30_000.0), 1.0);
        assert_eq!(spline.evaluate(9000.0), 3.0, "flat between equal points");
        let rising: Vec<f64> = (0..=6000)
            .step_by(500)
            .map(|t| spline.evaluate(t as f64))
            .collect();
        assert!(rising.windows(2).all(|w| w[1] >= w[0]), "{rising:?}");
        assert!(Interpolator::new(&[(0, 1.0)], 0.25).is_none());
    }

    #[test]
    fn interpolated_time_replaces_the_day_and_night_speeds() {
        let common = Common {
            interpolated: true,
            ..Common::default()
        };
        let spline = Interpolator::new(&[(0, 2.0), (24_000, 2.0)], 0.25);
        assert_eq!(common.time_speed(5000.0, spline.as_ref()), 2.0);
        assert_eq!(common.time_speed(5000.0, None), 1.0);
    }

    #[test]
    fn templates_fill_known_variables_only() {
        let vars = [
            ("player", "Alex".to_owned()),
            ("sleepingPlayers", "1".to_owned()),
        ];
        assert_eq!(
            fill("${player} [${sleepingPlayers}/${total}]", &vars),
            "Alex [1/${total}]"
        );
        assert_eq!(fill("no vars $ {", &vars), "no vars $ {");
        assert_eq!(fill("open ${player", &vars), "open ${player");
    }

    #[test]
    fn legacy_codes_make_runs() {
        let runs = legacy_runs("§e§oTempus §rfugit!");
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].0, "Tempus ");
        assert_eq!(runs[0].1.color, Some("yellow"));
        assert!(runs[0].1.italic);
        assert_eq!(runs[1], ("fugit!".to_owned(), Look::default()));
        let yellow = legacy_runs("§eHi");
        assert_eq!(yellow[0].1.color, Some("yellow"));
        assert_eq!(legacy_runs("plain")[0].1, Look::default());
    }

    #[test]
    fn the_clock_sits_where_the_alignment_says() {
        let mut client = Client::default();
        assert_eq!(
            client.clock_origin(400.0, 300.0),
            [400.0 - 64.0 - 16.0, 16.0]
        );
        client.alignment = Alignment::CenterCenter;
        assert_eq!(client.clock_origin(400.0, 300.0), [168.0, 118.0]);
        client.alignment = Alignment::BottomLeft;
        assert_eq!(
            client.clock_origin(400.0, 300.0),
            [16.0, 300.0 - 64.0 - 16.0]
        );
    }

    #[test]
    fn the_morning_is_the_last_thousand_ticks() {
        assert!(past_sunrise(23_001));
        assert!(!past_sunrise(23_000));
        assert!(!past_sunrise(30_000 + 24_000 * 3 - 6000));
    }

    #[test]
    fn the_day_starts_after_its_first_tick() {
        assert!(!past_day_start(24_000 * 2));
        assert!(past_day_start(24_000 * 2 + 1));
        assert!(past_day_start(23_999));
    }
}
