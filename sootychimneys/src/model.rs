//! The port's pure model: the seven kinds of chimney, their smoke, the wind, the config file and
//! the soot-scraping recipe's results. Nothing here touches the game.

use mod_api::RandomSource;

/// One kind of chimney: a clean block and a dirty one (`ChimneyType`, and the block properties
/// `SootyChimneys.Blocks` gives the pair).
pub struct Kind {
    /// The clean block's path; the dirty one is `dirty_<name>`.
    pub name: &'static str,
    /// `MapColor` id.
    pub map_color: u8,
    /// The vanilla block whose `SoundType` the chimney has.
    pub sounds: &'static str,
    /// The dirty block's `destroyTime`; the clean one's is 2.2.
    pub dirty_hardness: f32,
    pub smoke: Smoke,
    /// The outline and collision boxes, in sixteenths of a block, without and with a chimney on top.
    pub regular: &'static [[f64; 6]],
    pub stacked: &'static [[f64; 6]],
}

/// `SmokeProperties`: where smoke starts, how far it spreads, how much and how fast.
pub struct Smoke {
    pub origin: [f32; 3],
    pub spread: [f32; 3],
    pub intensity: f32,
    pub speed: f32,
}

const MAP_ORANGE: u8 = 15;
const MAP_GRAY: u8 = 21;
const MAP_BROWN: u8 = 26;

/// `ChimneyTypes`, in the original's registration order.
pub const KINDS: [Kind; 7] = [
    Kind {
        name: "brick_chimney",
        map_color: MAP_ORANGE,
        sounds: "minecraft:deepslate_bricks",
        dirty_hardness: 2.2,
        smoke: Smoke {
            origin: [0.5, 1.25, 0.5],
            spread: [0.25, 0.1, 0.25],
            intensity: 1.0,
            speed: 1.2,
        },
        regular: &[
            [1.0, 0.0, 1.0, 15.0, 11.0, 15.0],
            [0.0, 11.0, 0.0, 16.0, 16.0, 16.0],
        ],
        stacked: &[[1.0, 0.0, 1.0, 15.0, 16.0, 15.0]],
    },
    Kind {
        name: "cobblestone_chimney",
        map_color: MAP_GRAY,
        sounds: "minecraft:stone",
        dirty_hardness: 2.2,
        smoke: Smoke {
            origin: [0.5, 1.25, 0.5],
            spread: [0.15, 0.1, 0.15],
            intensity: 1.0,
            speed: 1.0,
        },
        regular: &[
            [3.0, 0.0, 3.0, 13.0, 16.0, 13.0],
            [1.0, 4.0, 1.0, 15.0, 16.0, 15.0],
        ],
        stacked: &[[3.0, 0.0, 3.0, 13.0, 16.0, 13.0]],
    },
    Kind {
        name: "stone_brick_chimney",
        map_color: MAP_GRAY,
        sounds: "minecraft:basalt",
        dirty_hardness: 2.0,
        smoke: Smoke {
            origin: [0.5, 1.2, 0.5],
            spread: [0.025, 0.05, 0.025],
            intensity: 0.65,
            speed: 1.0,
        },
        regular: &[
            [4.0, 0.0, 4.0, 12.0, 11.0, 12.0],
            [3.0, 11.0, 3.0, 13.0, 16.0, 13.0],
        ],
        stacked: &[[4.0, 0.0, 4.0, 12.0, 16.0, 12.0]],
    },
    Kind {
        name: "mud_brick_chimney",
        map_color: MAP_BROWN,
        sounds: "minecraft:mud_bricks",
        dirty_hardness: 2.0,
        smoke: Smoke {
            origin: [0.5, 1.25, 0.5],
            spread: [0.15, 0.1, 0.15],
            intensity: 1.0,
            speed: 1.1,
        },
        regular: &[
            [0.0, 8.0, 0.0, 16.0, 16.0, 16.0],
            [0.0, 0.0, 5.0, 16.0, 8.0, 11.0],
            [5.0, 0.0, 0.0, 11.0, 8.0, 16.0],
        ],
        stacked: &[
            [0.0, 0.0, 5.0, 16.0, 16.0, 11.0],
            [5.0, 0.0, 0.0, 11.0, 16.0, 16.0],
        ],
    },
    Kind {
        name: "iron_chimney",
        map_color: MAP_GRAY,
        // SoundType.METAL.
        sounds: "minecraft:gold_block",
        dirty_hardness: 2.0,
        smoke: Smoke {
            origin: [0.5, 1.2, 0.5],
            spread: [0.05, 0.05, 0.05],
            intensity: 0.65,
            speed: 1.35,
        },
        regular: &[
            [5.0, 0.0, 5.0, 11.0, 15.0, 11.0],
            [4.0, 7.0, 4.0, 12.0, 15.0, 12.0],
        ],
        stacked: &[[5.0, 0.0, 5.0, 11.0, 16.0, 11.0]],
    },
    Kind {
        name: "copper_chimney",
        map_color: MAP_ORANGE,
        sounds: "minecraft:copper_block",
        dirty_hardness: 2.0,
        smoke: Smoke {
            origin: [0.5, 1.3, 0.5],
            spread: [0.025, 0.05, 0.025],
            intensity: 0.55,
            speed: 1.2,
        },
        regular: &[
            [5.0, 0.0, 5.0, 11.0, 4.0, 11.0],
            [6.0, 4.0, 6.0, 10.0, 16.0, 10.0],
            [5.0, 10.0, 5.0, 11.0, 14.0, 11.0],
        ],
        stacked: &[[5.0, 0.0, 5.0, 11.0, 16.0, 11.0]],
    },
    Kind {
        name: "terracotta_chimney",
        map_color: MAP_ORANGE,
        sounds: "minecraft:dripstone_block",
        dirty_hardness: 2.0,
        smoke: Smoke {
            origin: [0.5, 0.8, 0.5],
            spread: [0.02, 0.05, 0.02],
            intensity: 0.3,
            speed: 0.65,
        },
        regular: &[[5.0, 0.0, 5.0, 11.0, 8.0, 11.0]],
        stacked: &[[5.0, 0.0, 5.0, 11.0, 16.0, 11.0]],
    },
];

/// A clean chimney's `destroyTime`.
pub const CLEAN_HARDNESS: f32 = 2.2;
/// `strength(2.0F, 2.0F)`'s explosion resistance.
pub const RESISTANCE: f32 = 2.0;

/// The settings of `sootychimneys-local.toml` (`Config.Common`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub smoke_flow: f64,
    pub dirty_chance: f64,
    pub wind: bool,
    pub wind_strength: f64,
    pub wind_affects_campfire: bool,
}

/// The file's text when it does not exist yet.
pub const DEFAULT_COMMON: &str = include_str!("common.toml");

impl Default for Settings {
    fn default() -> Self {
        Self {
            smoke_flow: 0.75,
            dirty_chance: 0.05,
            wind: true,
            wind_strength: 1.0,
            wind_affects_campfire: true,
        }
    }
}

impl Settings {
    /// Reads `key = value` lines. A value of the wrong type or outside its range keeps the
    /// default, as NeoForge's config spec corrects it.
    pub fn parse(text: &str) -> Self {
        let mut s = Self::default();
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("");
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            let real = |min: f64, max: f64| {
                value
                    .replace('_', "")
                    .parse::<f64>()
                    .ok()
                    .filter(|v| (min..=max).contains(v))
            };
            let boolean = match value {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            };
            match key.trim() {
                "SmokeFlow" => set(&mut s.smoke_flow, real(0.0, 1.0)),
                "DirtyChance" => set(&mut s.dirty_chance, real(0.0, 1.0)),
                "Wind" => set(&mut s.wind, boolean),
                "WindStrength" => set(&mut s.wind_strength, real(0.0, f64::MAX)),
                "WindAffectsCampfire" => set(&mut s.wind_affects_campfire, boolean),
                _ => {}
            }
        }
        s
    }
}

fn set<T>(field: &mut T, value: Option<T>) {
    if let Some(value) = value {
        *field = value;
    }
}

/// The sun's angle in degrees for a time of day in ticks: 0 at noon (tick 6000), 90 at
/// sunset, 180 at midnight, 270 at sunrise. It is the classic celestial-angle curve, which
/// a client can compute from the synchronized clock while it can't read the environment
/// attribute itself (sunrise-org/sunrise#5385).
pub fn sun_angle(day_time: i64) -> f64 {
    let fraction = day_time.rem_euclid(24000) as f64 / 24000.0 - 0.25;
    let fraction = fraction - fraction.floor();
    let eased = 0.5 - (fraction * std::f64::consts::PI).cos() / 2.0;
    (fraction + (eased - fraction) / 3.0) * 360.0
}

/// `TimeOfDay`, from the sun's angle in degrees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeOfDay {
    Morning,
    Day,
    Evening,
    Night,
}

impl TimeOfDay {
    pub fn from_sun_angle(degrees: f64) -> Self {
        if degrees <= 290.0 && degrees > 270.0 {
            Self::Morning
        } else if degrees <= 270.0 && degrees > 95.0 {
            Self::Night
        } else if degrees <= 95.0 && degrees > 75.0 {
            Self::Evening
        } else {
            Self::Day
        }
    }
}

/// `Weather.of`: from the thunder and rain levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weather {
    Clear,
    Rain,
    Thunder,
}

impl Weather {
    pub fn of(rain: f32, thunder: f32) -> Self {
        if thunder > 0.0 {
            Self::Thunder
        } else if rain > 0.0 {
            Self::Rain
        } else {
            Self::Clear
        }
    }
}

/// `WindState`: the range a wind's target strength is drawn from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindState {
    pub min: f32,
    pub max: f32,
}

pub const CALM: WindState = WindState {
    min: 0.0,
    max: 0.05,
};
pub const BREEZE: WindState = WindState {
    min: 0.05,
    max: 0.15,
};
pub const WINDY: WindState = WindState {
    min: 0.15,
    max: 0.5,
};
pub const STORMY: WindState = WindState { min: 0.5, max: 1.0 };

/// `Wind.getWindState`, for the level's day count (`getGameTime() / 24000`, as an int).
pub fn wind_state(day: i32, time: TimeOfDay, weather: Weather) -> WindState {
    match weather {
        Weather::Thunder => STORMY,
        Weather::Clear if time == TimeOfDay::Day => {
            if day % 5 > 2 {
                BREEZE
            } else {
                CALM
            }
        }
        Weather::Clear => {
            if day % 4 == 0 {
                BREEZE
            } else {
                CALM
            }
        }
        Weather::Rain => {
            if time != TimeOfDay::Evening && day % 3 > 0 {
                WINDY
            } else {
                BREEZE
            }
        }
    }
}

/// `WindData`: one wind for the whole game, which every level's tick moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wind {
    pub angle_degrees: f64,
    pub strength: f32,
    /// The direction's x and z, `cos` and `sin` of the angle (the original's x and "y").
    pub x: f64,
    pub z: f64,
}

impl Wind {
    pub const STILL: Self = Self {
        angle_degrees: 0.0,
        strength: 0.0,
        x: 0.0,
        z: 0.0,
    };

    /// `Wind.update`: the angle may turn back by the state's `max` degrees, and the strength
    /// moves a tenth of the way to a target drawn from the state's range.
    pub fn update(&mut self, state: WindState, random: &mut dyn RandomSource) {
        let add_degrees = f64::from(state.max * (random.next_int_bound(2) - 1) as f32);
        let target = map(random.next_float(), state.min, state.max);
        let strength = lerp(0.1, self.strength, target);
        self.angle_degrees += add_degrees;
        self.strength = strength;
        let radians = self.angle_degrees * (std::f64::consts::PI / 180.0);
        self.x = radians.cos();
        self.z = radians.sin();
    }

    /// `getAdjustedStrength`.
    pub fn adjusted_strength(&self, settings: &Settings) -> f32 {
        self.strength * 0.1 * settings.wind_strength as f32
    }
}

/// A campfire's smoke particle after the wind has pushed it: the wind's direction times its
/// adjusted strength is added to the horizontal speed; the rise is left alone.
pub fn campfire_velocity(velocity: [f64; 3], wind: &Wind, settings: &Settings) -> [f64; 3] {
    let strength = f64::from(wind.adjusted_strength(settings));
    [
        velocity[0] + wind.x * strength,
        velocity[1],
        velocity[2] + wind.z * strength,
    ]
}

/// `Mth.map(value, 0, 1, min, max)`.
fn map(value: f32, min: f32, max: f32) -> f32 {
    lerp((value - 0.0) / (1.0 - 0.0), min, max)
}

/// `Mth.lerp(float, float, float)`.
fn lerp(alpha: f32, from: f32, to: f32) -> f32 {
    from + alpha * (to - from)
}

/// `RandomOffset.offset`: `input` moved by up to `range` either way, with
/// `java.util.Random.nextDouble(origin, bound)`.
pub fn offset(random: &mut dyn RandomSource, input: f64, range: f64) -> f64 {
    if range <= 0.0 {
        return input;
    }
    let origin = -range;
    let mut r = random.next_double() * (range - origin) + origin;
    if r >= range {
        r = range.next_down();
    }
    input + r
}

/// One particle of a chimney's smoke: where and how fast.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Puff {
    pub at: [f64; 3],
    pub velocity: [f64; 3],
}

/// `ChimneyBlock.emitParticle` for a chimney whose block centre is `centre`: the puffs it
/// adds, drawing from `random` (the level's) and `offsets` (`RandomOffset`'s).
pub fn emit(
    smoke: &Smoke,
    centre: [f64; 3],
    wind: &Wind,
    settings: &Settings,
    random: &mut dyn RandomSource,
    offsets: &mut dyn RandomSource,
) -> Vec<Puff> {
    if random.next_float() > smoke.intensity {
        return Vec::new();
    }
    let at: [f64; 3] = std::array::from_fn(|i| centre[i] + (f64::from(smoke.origin[i]) - 0.5));
    let strength = wind.adjusted_strength(settings);
    let mut x_speed = wind.x * f64::from(strength);
    let y_speed = 0.05 * f64::from(smoke.speed);
    let mut z_speed = wind.z * f64::from(strength);
    x_speed += f64::from((random.next_float() * strength - strength / 2.0) * 0.2);
    z_speed += f64::from((random.next_float() * strength - strength / 2.0) * 0.2);
    let max_particles = (4.0 * smoke.intensity.max(0.5)) as i32;
    let mut puffs = Vec::new();
    // The bound is drawn again before every particle, as the original's loop condition is.
    while (puffs.len() as i32) < random.next_int_bound(max_particles) {
        let at = std::array::from_fn(|i| offset(offsets, at[i], f64::from(smoke.spread[i])));
        puffs.push(Puff {
            at,
            velocity: [x_speed, y_speed, z_speed],
        });
    }
    puffs
}

/// One result of a soot-scraping recipe (`ChanceResult`): the stack's JSON and its chance.
#[derive(Clone, Debug, PartialEq)]
pub struct ChanceResult {
    /// The `ItemStackTemplate` JSON (`id`, `count`, `components`).
    pub item: String,
    pub count: u32,
    pub chance: f32,
}

/// A `soot_scraping` recipe file: the chimney ingredient's JSON and up to six results.
#[derive(Clone, Debug, PartialEq)]
pub struct ScrapingRecipe {
    pub chimney: String,
    pub results: Vec<ChanceResult>,
}

/// `SootScrapingRecipe.MAX_RESULTS`.
const MAX_RESULTS: usize = 6;

impl ScrapingRecipe {
    /// Reads a recipe's JSON as `SootScrapingRecipe.CODEC` does; None where the codec fails.
    pub fn parse(json: &mod_api::serde_json::Value) -> Option<Self> {
        let chimney = json.get("chimney")?.to_string();
        let results = json.get("results")?.as_array()?;
        if results.len() > MAX_RESULTS {
            return None;
        }
        let results = results
            .iter()
            .map(|result| {
                let item = result.get("item")?;
                let chance = match result.get("chance") {
                    None => 1.0,
                    Some(chance) => chance.as_f64()? as f32,
                };
                if chance.is_nan() || chance <= 0.0 {
                    return None;
                }
                let count = match item.get("count") {
                    None => 1,
                    Some(count) => u32::try_from(count.as_u64()?).ok()?,
                };
                Some(ChanceResult {
                    item: item.to_string(),
                    count,
                    chance,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self { chimney, results })
    }
}

/// `ChanceResult.rollOutput`: each of the stack's items stays with the result's chance.
pub fn roll(count: u32, chance: f32, random: &mut dyn RandomSource) -> u32 {
    (0..count).filter(|_| random.next_float() <= chance).count() as u32
}

/// Where `Containers.dropItemStack` puts an item entity for `at`, and the stacks it splits
/// the drop into with their motions, drawing from the level's random.
pub fn scatter(
    at: [f64; 3],
    count: u32,
    random: &mut dyn RandomSource,
) -> ([f64; 3], Vec<(u32, [f64; 3])>) {
    // EntityTypes.ITEM's width.
    let size = 0.25_f64;
    let centre_range = 1.0 - size;
    let half = size / 2.0;
    let x = at[0].floor() + random.next_double() * centre_range + half;
    let y = at[1].floor() + random.next_double() * centre_range;
    let z = at[2].floor() + random.next_double() * centre_range + half;
    let mut left = count;
    let mut parts = Vec::new();
    while left > 0 {
        let split = ((random.next_int_bound(21) + 10) as u32).min(left);
        left -= split;
        let mut triangle = |mode: f64| {
            mode + 0.114_850_001_711_398_36 * (random.next_double() - random.next_double())
        };
        let motion = [triangle(0.0), triangle(0.2), triangle(0.0)];
        parts.push((split, motion));
    }
    ([x, y, z], parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A random that hands out the given floats and ints in turn.
    struct Script {
        floats: Vec<f32>,
        ints: Vec<i32>,
    }

    impl RandomSource for Script {
        fn next_int(&mut self) -> i32 {
            self.ints.remove(0)
        }
        fn next_int_bound(&mut self, _: i32) -> i32 {
            self.ints.remove(0)
        }
        fn next_long(&mut self) -> i64 {
            0
        }
        fn next_boolean(&mut self) -> bool {
            false
        }
        fn next_float(&mut self) -> f32 {
            self.floats.remove(0)
        }
        fn next_double(&mut self) -> f64 {
            0.5
        }
        fn next_gaussian(&mut self) -> f64 {
            0.0
        }
    }

    #[test]
    fn sun_angle_bounds_follow_the_original() {
        assert_eq!(TimeOfDay::from_sun_angle(290.0), TimeOfDay::Morning);
        assert_eq!(TimeOfDay::from_sun_angle(270.0), TimeOfDay::Night);
        assert_eq!(TimeOfDay::from_sun_angle(95.0), TimeOfDay::Evening);
        assert_eq!(TimeOfDay::from_sun_angle(75.0), TimeOfDay::Day);
        assert_eq!(TimeOfDay::from_sun_angle(300.0), TimeOfDay::Day);
    }

    #[test]
    fn sun_angle_follows_the_day() {
        assert!(sun_angle(6000).abs() < 1e-9);
        assert!((sun_angle(18000) - 180.0).abs() < 1e-9);
        assert!(sun_angle(30000).abs() < 1e-9);
        assert_eq!(TimeOfDay::from_sun_angle(sun_angle(0)), TimeOfDay::Morning);
        assert_eq!(TimeOfDay::from_sun_angle(sun_angle(1000)), TimeOfDay::Day);
        assert_eq!(TimeOfDay::from_sun_angle(sun_angle(6000)), TimeOfDay::Day);
        assert_eq!(
            TimeOfDay::from_sun_angle(sun_angle(12000)),
            TimeOfDay::Evening
        );
        assert_eq!(
            TimeOfDay::from_sun_angle(sun_angle(18000)),
            TimeOfDay::Night
        );
        assert_eq!(
            TimeOfDay::from_sun_angle(sun_angle(23000)),
            TimeOfDay::Night
        );
    }

    #[test]
    fn wind_state_by_weather_time_and_day() {
        assert_eq!(wind_state(0, TimeOfDay::Night, Weather::Thunder), STORMY);
        assert_eq!(wind_state(3, TimeOfDay::Day, Weather::Clear), BREEZE);
        assert_eq!(wind_state(2, TimeOfDay::Day, Weather::Clear), CALM);
        assert_eq!(wind_state(4, TimeOfDay::Night, Weather::Clear), BREEZE);
        assert_eq!(wind_state(5, TimeOfDay::Night, Weather::Clear), CALM);
        assert_eq!(wind_state(1, TimeOfDay::Day, Weather::Rain), WINDY);
        assert_eq!(wind_state(1, TimeOfDay::Evening, Weather::Rain), BREEZE);
        assert_eq!(wind_state(3, TimeOfDay::Day, Weather::Rain), BREEZE);
    }

    #[test]
    fn wind_turns_back_or_holds_and_eases_toward_its_target() {
        let mut wind = Wind::STILL;
        let mut random = Script {
            floats: vec![1.0],
            ints: vec![0],
        };
        wind.update(WINDY, &mut random);
        assert_eq!(wind.angle_degrees, -0.5);
        assert!((wind.strength - 0.05).abs() < 1e-7);
        let mut random = Script {
            floats: vec![0.0],
            ints: vec![1],
        };
        wind.update(WINDY, &mut random);
        assert_eq!(wind.angle_degrees, -0.5);
        assert!((wind.strength - (0.05 + 0.1 * (0.15 - 0.05))).abs() < 1e-7);
    }

    #[test]
    fn campfire_smoke_is_pushed_along_the_wind_only_sideways() {
        let wind = Wind {
            angle_degrees: 0.0,
            strength: 1.0,
            x: 1.0,
            z: 0.0,
        };
        let settings = Settings::default();
        let pushed = campfire_velocity([0.0, 0.07, 0.01], &wind, &settings);
        assert!((pushed[0] - 0.1).abs() < 1e-7);
        assert_eq!(pushed[1], 0.07);
        assert_eq!(pushed[2], 0.01);
        assert_eq!(
            campfire_velocity([0.0, 0.07, 0.0], &Wind::STILL, &settings),
            [0.0, 0.07, 0.0]
        );
    }

    #[test]
    fn wind_affects_campfire_is_read() {
        assert!(Settings::default().wind_affects_campfire);
        assert!(!Settings::parse("WindAffectsCampfire = false\n").wind_affects_campfire);
    }

    #[test]
    fn settings_keep_defaults_for_bad_values() {
        let s =
            Settings::parse("SmokeFlow = 2.0\nDirtyChance = 0.5\nWind = maybe\nWindStrength = 3\n");
        assert_eq!(s.smoke_flow, 0.75);
        assert_eq!(s.dirty_chance, 0.5);
        assert!(s.wind);
        assert_eq!(s.wind_strength, 3.0);
        assert_eq!(Settings::parse(DEFAULT_COMMON), Settings::default());
    }

    #[test]
    fn a_dim_chimney_adds_nothing_and_a_bright_one_draws_its_count_each_time() {
        let smoke = &KINDS[6].smoke;
        let settings = Settings::default();
        let mut random = Script {
            floats: vec![0.31],
            ints: vec![],
        };
        let none = emit(
            smoke,
            [0.5; 3],
            &Wind::STILL,
            &settings,
            &mut random,
            &mut Script {
                floats: vec![],
                ints: vec![],
            },
        );
        assert!(none.is_empty());
        let mut random = Script {
            floats: vec![0.0, 0.5, 0.5],
            ints: vec![2, 2, 1],
        };
        let mut offsets = Script {
            floats: vec![],
            ints: vec![],
        };
        let puffs = emit(
            &KINDS[0].smoke,
            [0.5; 3],
            &Wind::STILL,
            &settings,
            &mut random,
            &mut offsets,
        );
        assert_eq!(puffs.len(), 2);
        assert_eq!(puffs[0].at, [0.5, 1.25, 0.5]);
        assert!((puffs[0].velocity[1] - 0.06).abs() < 1e-6);
    }

    #[test]
    fn recipe_reads_chance_results_and_refuses_bad_chances() {
        let json: mod_api::serde_json::Value = mod_api::serde_json::from_str(
            r#"{"chimney":"sootychimneys:dirty_brick_chimney","results":[{"chance":0.75,"item":{"id":"minecraft:black_dye","count":2}}]}"#,
        )
        .unwrap();
        let recipe = ScrapingRecipe::parse(&json).unwrap();
        assert_eq!(recipe.results[0].count, 2);
        assert_eq!(recipe.results[0].chance, 0.75);
        let bad: mod_api::serde_json::Value = mod_api::serde_json::from_str(
            r#"{"chimney":"x","results":[{"chance":0,"item":{"id":"minecraft:black_dye"}}]}"#,
        )
        .unwrap();
        assert!(ScrapingRecipe::parse(&bad).is_none());
    }

    #[test]
    fn roll_drops_each_item_past_the_chance() {
        let mut random = Script {
            floats: vec![0.8, 0.2, 0.75],
            ints: vec![],
        };
        assert_eq!(roll(3, 0.75, &mut random), 2);
    }
}
