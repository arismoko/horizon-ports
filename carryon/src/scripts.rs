//! Carry On's pickup scripts (`common/scripting`): JSON files in a data pack's `carryon/scripts` folder,
//! read on every data load when `use_scripts` is on. A script names a block or a mob by type, numbers and
//! NBT; the first one that matches (highest `priority` first) may skip some pickup checks
//! (`override_checks`), add conditions on the player, change how the carried thing is drawn, and run a
//! command on pickup, every carried tick and on placement (as the console: `execute as <player> run ...`).
//!
//! The string conditions keep the original's own parsing (`Matchables`), quirks included. Where the
//! original throws (a bound that is not a number, an unknown block name, a scoreboard condition with
//! nothing to split on) the pickup is refused and the reason logged, as a thrown pickup does nothing.

use std::cmp::Reverse;
use std::sync::{Arc, LazyLock, RwLock};

use mod_api::{
    Compound, GameMode, Host, Identifier, LivingRef, PermissionLevel, PlayerRef,
    ReloadResourceError, ScoreHolder, ServerHost, ServerReloadContext, ServerReloadListener, Tag,
};
use serde_json::{Map, Value};

/// The loaded scripts, highest priority first.
static SCRIPTS: LazyLock<RwLock<Arc<Vec<Arc<Script>>>>> = LazyLock::new(RwLock::default);

fn scripts() -> Arc<Vec<Arc<Script>>> {
    SCRIPTS.read().unwrap_or_else(|e| e.into_inner()).clone()
}

type Fail = String;

/// `CarryOnScript`.
pub struct Script {
    priority: i64,
    block: Option<BlockObject>,
    entity: Option<EntityObject>,
    conditions: Option<Conditions>,
    /// The `render` object as written, validated; the clients read it.
    render: Option<String>,
    effects: Option<Effects>,
    pub override_checks: bool,
}

struct BlockObject {
    name: Option<String>,
    hardness: Bound,
    resistance: Bound,
    nbt: Compound,
}

struct EntityObject {
    name: Option<String>,
    health: Bound,
    height: Bound,
    width: Bound,
    nbt: Compound,
}

#[derive(Default)]
struct Conditions {
    advancement: String,
    xp: Bound,
    gamemode: Bound,
    scoreboard: String,
    position: String,
    effects: String,
}

/// `ScriptEffects`: the commands, empty for none.
#[derive(Default)]
pub struct Effects {
    pub pickup: String,
    pub r#loop: String,
    pub place: String,
}

/// A field, with JSON null read as absent (as `JsonOps` does).
fn field<'a>(m: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    m.get(key).filter(|v| !v.is_null())
}

fn object<'a>(v: &'a Value, what: &str) -> Result<&'a Map<String, Value>, Fail> {
    v.as_object()
        .ok_or_else(|| format!("{what}: not an object"))
}

/// `Codec.STRING` through `JsonOps`: a string, or a number's text.
fn string(m: &Map<String, Value>, key: &str) -> Result<String, Fail> {
    match field(m, key) {
        None => Ok(String::new()),
        Some(Value::String(s)) => Ok(s.clone()),
        Some(Value::Number(n)) => Ok(n.to_string()),
        Some(v) => Err(format!("{key}: not a string: {v}")),
    }
}

/// `ResourceKey.codec`: an identifier, `minecraft` when no namespace is given.
fn name(m: &Map<String, Value>, key: &str) -> Result<Option<String>, Fail> {
    match field(m, key) {
        None => Ok(None),
        Some(Value::String(s)) => Identifier::parse(s)
            .map(|id| Some(format!("{}:{}", id.namespace(), id.path())))
            .map_err(|e| format!("{key}: {e}: {s}")),
        Some(v) => Err(format!("{key}: not a string: {v}")),
    }
}

/// `Codec.BOOL` through `JsonOps`: a boolean, or a number whose byte value is not 0.
fn boolean(m: &Map<String, Value>, key: &str, default: bool) -> Result<bool, Fail> {
    match field(m, key) {
        None => Ok(default),
        Some(Value::Bool(b)) => Ok(*b),
        Some(Value::Number(n)) => {
            Ok(n.as_i64()
                .unwrap_or_else(|| n.as_f64().unwrap_or(0.0) as i64) as i8
                != 0)
        }
        Some(v) => Err(format!("{key}: not a boolean: {v}")),
    }
}

/// `CompoundTag.CODEC` from JSON: an object, converted as `JsonOps.convertTo` does.
fn nbt(m: &Map<String, Value>, key: &str) -> Result<Option<Compound>, Fail> {
    match field(m, key) {
        None => Ok(None),
        Some(v @ Value::Object(_)) => match json_tag(v) {
            Some(Tag::Compound(c)) => Ok(Some(c)),
            _ => Err(format!("{key}: not a compound tag")),
        },
        Some(v) => Err(format!("{key}: not a compound tag: {v}")),
    }
}

/// `JsonOps.convertTo(NbtOps)`: whole numbers become the smallest of byte, short, int and long that holds
/// them, other numbers a float when one holds them exactly, else a double; booleans are bytes.
pub fn json_tag(v: &Value) -> Option<Tag> {
    Some(match v {
        Value::Null => return None,
        Value::Bool(b) => Tag::Byte(i8::from(*b)),
        Value::Number(n) => {
            let whole = n.as_i64().or_else(|| {
                n.as_f64()
                    .filter(|d| d.fract() == 0.0 && d.abs() < 9.2e18)
                    .map(|d| d as i64)
            });
            match whole {
                Some(l) => {
                    if let Ok(b) = i8::try_from(l) {
                        Tag::Byte(b)
                    } else if let Ok(s) = i16::try_from(l) {
                        Tag::Short(s)
                    } else if let Ok(i) = i32::try_from(l) {
                        Tag::Int(i)
                    } else {
                        Tag::Long(l)
                    }
                }
                None => {
                    let d = n.as_f64().unwrap_or(0.0);
                    if f64::from(d as f32) == d {
                        Tag::Float((d as f32).to_bits())
                    } else {
                        Tag::Double(d.to_bits())
                    }
                }
            }
        }
        Value::String(s) => Tag::String(s.as_str().into()),
        Value::Array(a) => Tag::List(a.iter().filter_map(json_tag).collect()),
        Value::Object(m) => {
            let mut c = Compound::new();
            for (k, v) in m {
                if let Some(t) = json_tag(v) {
                    c.insert(k.as_str().into(), t);
                }
            }
            Tag::Compound(c)
        }
    })
}

impl Script {
    /// `CarryOnScript.CODEC`.
    pub fn parse(v: &Value) -> Result<Self, Fail> {
        let m = object(v, "script")?;
        let priority = match field(m, "priority") {
            None => 0,
            Some(Value::Number(n)) => n
                .as_i64()
                .unwrap_or_else(|| n.as_f64().unwrap_or(0.0) as i64),
            Some(Value::Bool(b)) => i64::from(*b),
            Some(v) => return Err(format!("priority: not a number: {v}")),
        };
        let obj = object(field(m, "object").ok_or("No key object")?, "object")?;
        let block = field(obj, "block")
            .map(|v| {
                let b = object(v, "block")?;
                Ok::<_, Fail>(BlockObject {
                    name: name(b, "name")?,
                    hardness: Bound(string(b, "hardness")?),
                    resistance: Bound(string(b, "resistance")?),
                    nbt: nbt(b, "nbt")?.unwrap_or_default(),
                })
            })
            .transpose()?;
        let entity = field(obj, "entity")
            .map(|v| {
                let e = object(v, "entity")?;
                Ok::<_, Fail>(EntityObject {
                    name: name(e, "name")?,
                    health: Bound(string(e, "health")?),
                    height: Bound(string(e, "height")?),
                    width: Bound(string(e, "width")?),
                    nbt: nbt(e, "nbt")?.unwrap_or_default(),
                })
            })
            .transpose()?;
        let conditions = field(m, "conditions")
            .map(|v| {
                let c = object(v, "conditions")?;
                // Game Stages is not on Sunrise, so a `gamestage` condition always holds.
                string(c, "gamestage")?;
                Ok::<_, Fail>(Conditions {
                    advancement: string(c, "advancement")?,
                    xp: Bound(string(c, "xp")?),
                    gamemode: Bound(string(c, "gamemode")?),
                    scoreboard: string(c, "scoreboard")?,
                    position: string(c, "position")?,
                    effects: string(c, "effects")?,
                })
            })
            .transpose()?;
        let render = field(m, "render")
            .map(|v| Render::parse(v).map(|_| v.to_string()))
            .transpose()?;
        let effects = field(m, "effects")
            .map(|v| {
                let e = object(v, "effects")?;
                Ok::<_, Fail>(Effects {
                    pickup: string(e, "commandPickup")?,
                    r#loop: string(e, "commandLoop")?,
                    place: string(e, "commandPlace")?,
                })
            })
            .transpose()?;
        Ok(Self {
            priority,
            block,
            entity,
            conditions,
            render,
            effects,
            override_checks: boolean(m, "override_checks", false)?,
        })
    }

    /// `CarryOnScript.isValid`: names a block or a mob, not both, and does something.
    fn is_valid(&self) -> bool {
        (self.block.is_some() != self.entity.is_some())
            && (self.conditions.is_some()
                || self.render.is_some()
                || self.effects.is_some()
                || self.override_checks)
    }

    /// `CarryOnScript.fulfillsConditions`: every condition is read, as the original does.
    pub fn fulfills(&self, host: &mut dyn ServerHost, player: PlayerRef) -> Result<bool, Fail> {
        let Some(c) = &self.conditions else {
            return Ok(true);
        };
        let advancement = advancement_done(host, player, &c.advancement)?;
        let reader: &dyn ServerHost = &*host;
        let mode = match player.game_mode(reader) {
            Some(GameMode::Survival) | None => 0,
            Some(GameMode::Creative) => 1,
            Some(GameMode::Adventure) => 2,
            Some(GameMode::Spectator) => 3,
        };
        let gamemode = c.gamemode.matches(f64::from(mode))?;
        let position = position_holds(
            &c.position,
            player.entity().block_position(reader).unwrap_or([0; 3]),
        );
        let level = player.experience(reader).map_or(0, |x| x.level);
        let xp = c.xp.matches(f64::from(level))?;
        let scoreboard = if c.scoreboard.is_empty() {
            true
        } else {
            let (objective, bound) = score_split(&c.scoreboard)
                .ok_or_else(|| format!("scoreboard condition {:?}", c.scoreboard))?;
            let score = reader
                .objective(&objective)
                .and_then(|o| o.score(reader, ScoreHolder::Player(player)))
                .unwrap_or(0);
            Bound(bound.to_string()).matches(f64::from(score))?
        };
        let mut active: Vec<(String, i32)> = Vec::new();
        let _ = LivingRef::new(player.entity()).visit_effects(reader, &mut |e| {
            active.push((e.id.id(reader).unwrap_or_default().to_string(), e.amplifier));
            std::ops::ControlFlow::Continue(())
        });
        let effects = effects_hold(&c.effects, &active);
        Ok(advancement && gamemode && position && xp && scoreboard && effects)
    }

    pub fn effects(&self) -> Option<&Effects> {
        self.effects.as_ref()
    }

    pub fn render(&self) -> Option<&str> {
        self.render.as_deref()
    }
}

/// `AdvancementCondition`: an advancement the server doesn't have holds; otherwise the player must have
/// done it. The mod API has no read of a player's advancement progress, so it is asked of the
/// `advancements` selector argument, which reads the same progress.
fn advancement_done(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    text: &str,
) -> Result<bool, Fail> {
    if text.is_empty() {
        return Ok(true);
    }
    let id = Identifier::parse(text).map_err(|e| format!("advancement {text:?}: {e}"))?;
    if host.advancement(&id).is_none() {
        return Ok(true);
    }
    let line = format!(
        "execute if entity @s[advancements={{{}:{}=true}}]",
        id.namespace(),
        id.path()
    );
    Ok(player
        .entity()
        .run_command(host, &line, PermissionLevel::Owners, true))
}

/// Runs a script command as the original does: as the console, `execute as <player> run <command>`.
pub fn run(host: &mut dyn ServerHost, player: PlayerRef, command: &str) {
    if command.is_empty() {
        return;
    }
    let Some(name) = player.name(&*host).map(str::to_owned) else {
        return;
    };
    host.commands()
        .run_command(&format!("execute as {name} run {command}"));
}

/// The first loaded block script that matches, when scripts are on (`ScriptManager.inspectBlock`).
/// `nbt` is the block entity's data with its `id`, None without one.
pub fn inspect_block(
    host: &dyn Host,
    block: &str,
    hardness: f32,
    resistance: f32,
    nbt: Option<&Compound>,
) -> Result<Option<Arc<Script>>, Fail> {
    for script in scripts().iter() {
        let Some(b) = &script.block else { continue };
        let name = match &b.name {
            None => true,
            Some(n) if host.block_key(n).is_none() => return Err(format!("unknown block {n}")),
            Some(n) => n == block,
        };
        let tag = nbt.is_none_or(|t| compare_compound(&b.nbt, t));
        let hard = b.hardness.matches(f64::from(hardness))?;
        let resist = b.resistance.matches(f64::from(resistance))?;
        if name && tag && hard && resist {
            return Ok(Some(script.clone()));
        }
    }
    Ok(None)
}

/// What an entity script reads of a mob.
pub struct EntityFacts<'a> {
    pub kind: &'a str,
    pub width: f32,
    pub height: f32,
    pub health: f32,
    /// The mob's data with its `id`.
    pub nbt: &'a Compound,
}

/// The first loaded entity script that matches (`ScriptManager.inspectEntity`). The original compares the
/// mob's type with a registry lookup's `Optional`, which is never equal, so a script naming a mob type never
/// matches there; here the name matches the mob's type, as the script means.
pub fn inspect_entity(e: &EntityFacts<'_>) -> Result<Option<Arc<Script>>, Fail> {
    for script in scripts().iter() {
        let Some(s) = &script.entity else { continue };
        let name = s.name.as_deref().is_none_or(|n| n == e.kind);
        let height = s.height.matches(f64::from(e.height))?;
        let width = s.width.matches(f64::from(e.width))?;
        let health = s.health.matches(f64::from(e.health))?;
        let tag = compare_compound(&s.nbt, e.nbt);
        if name && height && width && health && tag {
            return Ok(Some(script.clone()));
        }
    }
    Ok(None)
}

/// `NumberBoundCondition`: `<=n`, `>=n`, `<n`, `>n`, `=n` or `n`; empty always holds.
#[derive(Default, Debug)]
struct Bound(String);

impl Bound {
    fn matches(&self, number: f64) -> Result<bool, Fail> {
        let b = &self.0;
        if b.is_empty() {
            return Ok(true);
        }
        let num = |op: &str| -> Result<f64, Fail> {
            java_number::<f64>(&b.replace(op, "")).ok_or_else(|| format!("number bound {b:?}"))
        };
        Ok(if b.contains("<=") {
            number <= num("<=")?
        } else if b.contains(">=") {
            number >= num(">=")?
        } else if b.contains('<') {
            number < num("<")?
        } else if b.contains('>') {
            number > num(">")?
        } else if b.contains('=') {
            number == num("=")?
        } else {
            number == java_number::<f64>(b).ok_or_else(|| format!("number bound {b:?}"))?
        })
    }
}

/// Java's `Double.parseDouble` / `Float.parseFloat` for the decimal forms: surrounding whitespace and one
/// `d`/`f` suffix allowed, `Infinity` and `NaN` by name.
fn java_number<T: std::str::FromStr>(text: &str) -> Option<T> {
    let t = text.trim_matches(|c: char| c <= ' ');
    let unsigned = t.trim_start_matches(['+', '-']);
    if unsigned.len() + 1 < t.len() {
        return None;
    }
    let sign = &t[..t.len() - unsigned.len()];
    match unsigned {
        "Infinity" => return format!("{sign}inf").parse().ok(),
        "NaN" => return "NaN".parse().ok(),
        _ => {}
    }
    let body = unsigned
        .strip_suffix(['d', 'D', 'f', 'F'])
        .unwrap_or(unsigned);
    if !body.bytes().any(|c| c.is_ascii_digit())
        || !body
            .bytes()
            .all(|c| c.is_ascii_digit() || b".eE+-".contains(&c))
    {
        return None;
    }
    format!("{sign}{body}").parse().ok()
}

/// Java's `String.split(",")`: trailing empty parts are dropped.
fn java_split(s: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = s.split(',').collect();
    while parts.len() > 1 && parts.last() == Some(&"") {
        parts.pop();
    }
    if parts == [""] && s.contains(',') {
        parts.clear();
    }
    parts
}

/// `Matchables.getValueFromString`: the first comma-separated part that contains `key` and `=`, with
/// `key=` taken out, read as a float (0 when it isn't one).
fn value_from_string(text: &str, key: &str) -> Option<f32> {
    if text.is_empty() {
        return None;
    }
    java_split(text)
        .into_iter()
        .find(|part| part.contains(key) && part.contains('='))
        .map(|part| java_number(&part.replace(&format!("{key}="), "")).unwrap_or(0.0))
}

/// `ScoreboardCondition`'s split of `objective<op>number` into the objective and the bound; None where the
/// original's `substring(-1)` throws.
fn score_split(cond: &str) -> Option<(String, &str)> {
    let at = |c: char| cond.find(c).map_or(-1, |i| i as i64);
    let (ie, ig, il) = (at('='), at('>'), at('<'));
    let start = if ig == -1 || (ie < ig && il == -1) || (ie < il && ie != -1) {
        ie
    } else if ie == -1 || (ig < ie && il == -1) || (ig < il && ig != -1) {
        ig
    } else {
        il
    };
    let numb = &cond[usize::try_from(start).ok()?..];
    Some((cond.replace(numb, ""), numb))
}

/// `PositionCondition`: `x=,y=,z=` with `dx=,dy=,dz=` spans; an axis whose start is 0 always holds.
fn position_holds(cond: &str, pos: [i32; 3]) -> bool {
    if cond.is_empty() {
        return true;
    }
    let v = |k: &str| value_from_string(cond, k).unwrap_or(0.0) as i32;
    let start = [v("x"), v("y"), v("z")];
    let span = [v("dx"), v("dy"), v("dz")];
    (0..3).all(|i| {
        let end = start[i].wrapping_add(span[i]);
        (pos[i] >= start[i] && pos[i] <= end) || start[i] == 0
    })
}

/// `EffectsCondition`: `name` or `name#level` parts; every part must be met by an active effect of that
/// name at that amplifier or above.
fn effects_hold(cond: &str, active: &[(String, i32)]) -> bool {
    if cond.is_empty() {
        return true;
    }
    let wanted: Vec<(&str, i32)> = java_split(cond)
        .into_iter()
        .map(|part| match part.find('#') {
            Some(i) => (&part[..i], part[i..].replace('#', "").parse().unwrap_or(0)),
            None => (part, 0),
        })
        .collect();
    let matches = active
        .iter()
        .filter(|(name, amp)| {
            wanted
                .iter()
                .find(|(n, _)| n == name)
                .is_some_and(|(_, level)| level <= amp)
        })
        .count();
    matches == wanted.len()
}

/// `NbtUtils.compareNbt(expected, actual, true)` for compounds: every expected key is there and matches;
/// a list matches when each expected element matches some element.
fn compare_compound(expected: &Compound, actual: &Compound) -> bool {
    actual.len() >= expected.len()
        && expected
            .iter()
            .all(|(k, v)| actual.get(k).is_some_and(|a| compare(v, a)))
}

fn compare(expected: &Tag, actual: &Tag) -> bool {
    match (expected, actual) {
        (Tag::Compound(e), Tag::Compound(a)) => compare_compound(e, a),
        (Tag::List(e), Tag::List(a)) => {
            if e.is_empty() {
                a.is_empty()
            } else {
                a.len() >= e.len() && e.iter().all(|t| a.iter().any(|v| compare(t, v)))
            }
        }
        _ => expected == actual,
    }
}

/// `OptionalVec3`: `x=,y=,z=`, each optional.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Vec3Opt([Option<f32>; 3]);

impl Vec3Opt {
    fn parse(text: &str) -> Self {
        Self(["x", "y", "z"].map(|k| value_from_string(text, k)))
    }
    /// The vector, `default` where an axis is missing.
    pub fn or(self, default: f32) -> [f32; 3] {
        self.0.map(|v| v.unwrap_or(default))
    }
}

/// `ScriptRender`: what a client draws in place of the carried thing, and how it moves it.
#[derive(Clone)]
pub struct Render {
    pub name_block: Option<String>,
    pub name_entity: Option<String>,
    pub nbt: Option<Compound>,
    pub translation: Vec3Opt,
    pub rotation: Vec3Opt,
    pub scale: Vec3Opt,
    pub left_arm: bool,
    pub right_arm: bool,
}

impl Render {
    pub fn parse(v: &Value) -> Result<Self, Fail> {
        let m = object(v, "render")?;
        // The arm rotations are read for validity; the API does not pose a carrying player's arms.
        string(m, "rotation_left_arm")?;
        string(m, "rotation_right_arm")?;
        Ok(Self {
            name_block: name(m, "name_block")?,
            name_entity: name(m, "name_entity")?,
            nbt: nbt(m, "nbt")?,
            translation: Vec3Opt::parse(&string(m, "translation")?),
            rotation: Vec3Opt::parse(&string(m, "rotation")?),
            scale: Vec3Opt::parse(&string(m, "scale")?),
            left_arm: boolean(m, "render_left_arm", true)?,
            right_arm: boolean(m, "render_right_arm", true)?,
        })
    }

    /// The render a script sent with a carry, read on the client.
    pub fn from_text(text: &str) -> Option<Self> {
        serde_json::from_str(text)
            .ok()
            .and_then(|v| Self::parse(&v).ok())
    }
}

/// `ScriptReloadListener` over `data/<namespace>/carryon/scripts/*.json`.
pub struct Listener;

impl ServerReloadListener for Listener {
    fn prepare(
        &self,
        context: &ServerReloadContext,
    ) -> Result<Box<dyn FnOnce() + Send + 'static>, ReloadResourceError> {
        let mut read = Vec::new();
        for id in context.list() {
            let Some(bytes) = context.read(id) else {
                continue;
            };
            let parsed = serde_json::from_slice::<Value>(bytes)
                .map_err(|e| e.to_string())
                .and_then(|v| Script::parse(&v));
            match parsed {
                Ok(script) => read.push((id.clone(), script)),
                Err(e) => eprintln!("[carryon] Couldn't parse data file '{id}': {e}"),
            }
        }
        let loaded = Arc::new(ordered(read));
        Ok(Box::new(move || {
            *SCRIPTS.write().unwrap_or_else(|e| e.into_inner()) = loaded;
        }))
    }
}

/// The valid scripts, highest priority first. Scripts of the same priority keep the order the original
/// meets them in: its `HashMap` of file ids, filled in `listResources` order (path, then namespace).
fn ordered(mut read: Vec<(Identifier, Script)>) -> Vec<Arc<Script>> {
    read.sort_by(|a, b| (a.0.path(), a.0.namespace()).cmp(&(b.0.path(), b.0.namespace())));
    let mut capacity = 16usize;
    while read.len() > capacity * 3 / 4 {
        capacity *= 2;
    }
    let java_hash = |s: &str| {
        s.encode_utf16()
            .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(i32::from(c)))
    };
    read.sort_by_key(|(id, _)| {
        let h = java_hash(id.namespace())
            .wrapping_mul(31)
            .wrapping_add(java_hash(id.path())) as u32;
        (h ^ (h >> 16)) as usize & (capacity - 1)
    });
    let mut scripts: Vec<Arc<Script>> = read
        .into_iter()
        .map(|(_, s)| s)
        .filter(Script::is_valid)
        .map(Arc::new)
        .collect();
    scripts.sort_by_key(|s| Reverse(s.priority));
    scripts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script(json: &str) -> Result<Script, Fail> {
        Script::parse(&serde_json::from_str(json).unwrap())
    }

    #[test]
    fn bounds_read_as_the_original_reads_them() {
        let b = |s: &str, n: f64| Bound(s.into()).matches(n);
        assert_eq!(b("", 3.0), Ok(true));
        assert_eq!(b(">=5", 5.0), Ok(true));
        assert_eq!(b("<5", 5.0), Ok(false));
        assert_eq!(b("=2", 2.0), Ok(true));
        assert_eq!(b(" 2.5f ", 2.5), Ok(true));
        assert_eq!(b("<=Infinity", 1e300), Ok(true));
        assert!(b(">five", 1.0).is_err());
        assert!(b("inf", 1.0).is_err());
    }

    #[test]
    fn scoreboard_conditions_split_like_the_original() {
        assert_eq!(score_split("kills>=5"), Some(("kills".into(), ">=5")));
        assert_eq!(score_split("kills=5"), Some(("kills".into(), "=5")));
        // `>` or `<` alone: the original takes substring(-1) of the `=` that isn't there and throws.
        assert_eq!(score_split("kills>5"), None);
        assert_eq!(score_split("kills<5"), None);
        // `<=` is split at `=`, leaving `kills<` as the objective.
        assert_eq!(score_split("kills<=5"), Some(("kills<".into(), "=5")));
    }

    #[test]
    fn positions_and_their_quirks() {
        assert!(position_holds(
            "x=10,y=60,z=-5,dx=4,dy=10,dz=2",
            [12, 65, -4]
        ));
        assert!(!position_holds(
            "x=10,y=60,z=-5,dx=4,dy=10,dz=2",
            [15, 65, -4]
        ));
        // A 0 start leaves the axis open.
        assert!(position_holds("y=60,dy=5", [1000, 62, -1000]));
        // `dx` before `x`: the `x` lookup finds `dx=4` first, reads `d4` as 0, and opens the axis.
        assert!(position_holds("dx=4,x=10", [500, 0, 0]));
    }

    #[test]
    fn effects_need_every_part_at_its_level() {
        let active = vec![
            ("minecraft:speed".to_string(), 1),
            ("minecraft:haste".into(), 0),
        ];
        assert!(effects_hold("minecraft:speed#1,minecraft:haste", &active));
        assert!(!effects_hold("minecraft:speed#2", &active));
        assert!(!effects_hold(
            "minecraft:speed,minecraft:jump_boost",
            &active
        ));
        assert!(effects_hold("minecraft:speed,", &active));
    }

    #[test]
    fn json_numbers_take_the_smallest_tag() {
        let t = |s: &str| json_tag(&serde_json::from_str(s).unwrap());
        assert_eq!(t("1"), Some(Tag::Byte(1)));
        assert_eq!(t("1.0"), Some(Tag::Byte(1)));
        assert_eq!(t("300"), Some(Tag::Short(300)));
        assert_eq!(t("70000"), Some(Tag::Int(70000)));
        assert_eq!(t("5000000000"), Some(Tag::Long(5_000_000_000)));
        assert_eq!(t("0.5"), Some(Tag::Float(0.5f32.to_bits())));
        assert_eq!(t("0.1"), Some(Tag::Double(0.1f64.to_bits())));
        assert_eq!(t("true"), Some(Tag::Byte(1)));
    }

    #[test]
    fn nbt_matches_partially_and_by_type() {
        let c = |s: &str| match json_tag(&serde_json::from_str(s).unwrap()) {
            Some(Tag::Compound(c)) => c,
            _ => unreachable!(),
        };
        let actual = c(
            r#"{"id":"minecraft:chest","Items":[{"Slot":0,"id":"minecraft:dirt"},{"Slot":1,"id":"minecraft:stone"}],"Lock":""}"#,
        );
        assert!(compare_compound(&c("{}"), &actual));
        assert!(compare_compound(
            &c(r#"{"Items":[{"id":"minecraft:stone"}]}"#),
            &actual
        ));
        assert!(!compare_compound(
            &c(r#"{"Items":[{"id":"minecraft:gold_block"}]}"#),
            &actual
        ));
        assert!(!compare_compound(&c(r#"{"Items":[]}"#), &actual));
        // A byte 0 is not an int 0.
        let mut ints = Compound::new();
        ints.insert("Slot".into(), Tag::Int(0));
        assert!(!compare_compound(&c(r#"{"Slot":0}"#), &ints));
    }

    #[test]
    fn a_script_must_name_one_thing_and_do_something() {
        let ok =
            script(r#"{"object":{"block":{"name":"chest"}},"effects":{"commandPickup":"say hi"}}"#)
                .unwrap();
        assert!(ok.is_valid());
        assert_eq!(
            ok.block.as_ref().unwrap().name.as_deref(),
            Some("minecraft:chest")
        );
        assert!(!script(r#"{"object":{"block":{}}}"#).unwrap().is_valid());
        assert!(
            !script(r#"{"object":{"block":{},"entity":{}},"override_checks":true}"#)
                .unwrap()
                .is_valid()
        );
        // An empty section still counts as present, as the original compares against its shared default.
        assert!(
            script(r#"{"object":{"entity":{}},"conditions":{}}"#)
                .unwrap()
                .is_valid()
        );
        assert!(script(r#"{"conditions":{}}"#).is_err());
        assert!(script(r#"{"object":{"block":{"name":"Not An Id"}}}"#).is_err());
        assert!(script(r#"{"object":{"block":{"nbt":"x"}}}"#).is_err());
    }

    #[test]
    fn render_vectors_keep_missing_axes() {
        let r = Render::from_text(
            r#"{"translation":"x=0.5,z=-1","scale":"y=2","render_left_arm":false}"#,
        )
        .unwrap();
        assert_eq!(r.translation.or(0.0), [0.5, 0.0, -1.0]);
        assert_eq!(r.scale.or(1.0), [1.0, 2.0, 1.0]);
        assert!(!r.left_arm && r.right_arm);
    }

    #[test]
    fn equal_priorities_keep_the_original_map_order() {
        let read = |names: &[&str]| {
            names
                .iter()
                .map(|n| {
                    let s = script(&format!(
                        r#"{{"priority":1,"object":{{"block":{{"name":"{n}"}}}},"override_checks":true}}"#
                    ))
                    .unwrap();
                    (Identifier::parse(&format!("pack:{n}")).unwrap(), s)
                })
                .collect::<Vec<_>>()
        };
        let order = |names: &[&str]| {
            ordered(read(names))
                .iter()
                .map(|s| s.block.as_ref().unwrap().name.clone().unwrap())
                .collect::<Vec<_>>()
        };
        // Whatever order the files are read in, the result is the same Java HashMap order.
        assert_eq!(
            order(&["chest", "barrel", "furnace"]),
            order(&["furnace", "chest", "barrel"])
        );
    }
}
