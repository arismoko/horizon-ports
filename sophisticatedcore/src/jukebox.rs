//! The jukebox upgrades (JukeboxUpgradeWrapper): music discs kept in the upgrade, played to everyone near the
//! storage. A carried backpack's disc follows its bearer; a placed storage's plays from its block. The disc's
//! state lives in the upgrade (`discInventory`, `isPlaying`, `shuffle`, `repeatMode`, `discSlotActive`,
//! `discPlayed`); the running timers are kept here and are lost with the server, which stops the music (the jar's
//! keep-alive does the same when nobody renews it).

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use mod_api::{EquipmentSlot, Host, ItemStack, LivingRef, ServerHost, SoundSource, Tag};

use crate::storage_items;

use crate::contents::{
    self, Nbt, custom_data, get_bool, get_int, get_str, put, read_items, set_custom_data,
    write_items,
};

/// The vanilla discs: song name and length in seconds (data/minecraft/jukebox_song/*.json).
const SONGS: [(&str, u32); 22] = [
    ("11", 71),
    ("13", 178),
    ("5", 178),
    ("blocks", 345),
    ("bounce", 234),
    ("cat", 185),
    ("chirp", 185),
    ("creator", 176),
    ("creator_music_box", 73),
    ("far", 174),
    ("lava_chicken", 134),
    ("mall", 197),
    ("mellohi", 96),
    ("otherside", 195),
    ("pigstep", 149),
    ("precipice", 299),
    ("relic", 218),
    ("stal", 150),
    ("strad", 188),
    ("tears", 175),
    ("wait", 238),
    ("ward", 251),
];

/// A disc's sound event and its length in ticks.
pub fn song(item_id: &str) -> Option<(String, u64)> {
    let name = item_id.strip_prefix("minecraft:music_disc_")?;
    let secs = SONGS.iter().find(|(n, _)| *n == name)?.1;
    Some((format!("minecraft:music_disc.{name}"), u64::from(secs) * 20))
}

/// The ids of the discs a mob may play (those not on the block list).
pub fn mob_discs(blocked: &[String]) -> Vec<String> {
    SONGS
        .iter()
        .map(|(n, _)| format!("minecraft:music_disc_{n}"))
        .filter(|id| !crate::options::listed(blocked, id))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Repeat {
    No,
    One,
    All,
}

impl Repeat {
    pub fn name(self) -> &'static str {
        match self {
            Repeat::No => "NO",
            Repeat::One => "ONE",
            Repeat::All => "ALL",
        }
    }
    fn from(name: &str) -> Self {
        match name {
            "ONE" => Repeat::One,
            "ALL" => Repeat::All,
            _ => Repeat::No,
        }
    }
    pub fn next(self) -> Self {
        match self {
            Repeat::No => Repeat::One,
            Repeat::One => Repeat::All,
            Repeat::All => Repeat::No,
        }
    }
}

/// The disc slot to play after `active` (the playlist): discs not played yet in slot order, or at random when
/// shuffled; when all were played, `wrap` starts over (the rest of the playlist is every disc but the active one)
/// and otherwise there is nothing more. Returns the slot and the new played set.
pub fn choose(
    filled: &[bool],
    active: Option<usize>,
    played: u64,
    shuffle: bool,
    wrap: bool,
    seed: u64,
) -> Option<(usize, u64)> {
    let bit = |i: usize| 1u64.checked_shl(i as u32).unwrap_or(0);
    let pick = |played: u64| {
        let left: Vec<usize> = (0..filled.len())
            .filter(|&i| filled[i] && played & bit(i) == 0 && Some(i) != active)
            .collect();
        if left.is_empty() {
            return None;
        }
        Some(if shuffle {
            left[(seed % left.len() as u64) as usize]
        } else {
            let after = active.map_or(0, |a| a + 1);
            left.iter()
                .copied()
                .find(|&i| i >= after)
                .unwrap_or(left[0])
        })
    };
    let active_bit = active.map_or(0, bit);
    if let Some(i) = pick(played) {
        return Some((i, (played | active_bit) | bit(i)));
    }
    if !wrap {
        return None;
    }
    // Start over; a lone disc plays again.
    pick(0).map(|i| (i, bit(i))).or_else(|| {
        active
            .filter(|&a| filled.get(a) == Some(&true))
            .map(|a| (a, bit(a)))
    })
}

/// The disc slot before `active`, wrapping.
pub fn before(filled: &[bool], active: usize) -> Option<usize> {
    let n = filled.len();
    (1..n).map(|d| (active + n - d) % n).find(|&i| filled[i])
}

// ---- the upgrade's state ----

pub struct Deck {
    pub discs: Vec<Option<ItemStack>>,
    pub playing: bool,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub active: Option<usize>,
    pub played: u64,
}

impl Deck {
    pub fn read(host: &dyn Host, upgrade: &ItemStack, slots: usize) -> Self {
        let d = custom_data(upgrade);
        Self {
            discs: read_items(host, d.at("discInventory"), slots),
            playing: get_bool(&d, "isPlaying").unwrap_or(false),
            shuffle: get_bool(&d, "shuffle").unwrap_or(false),
            repeat: Repeat::from(&get_str(&d, "repeatMode").unwrap_or_default()),
            active: get_int(&d, "discSlotActive").and_then(|v| usize::try_from(v).ok()),
            played: get_int(&d, "discPlayed").map_or(0, |v| u64::from(v as u32)),
        }
    }

    pub fn write(&self, host: &dyn Host, upgrade: &mut ItemStack) {
        let mut d = custom_data(upgrade);
        put(&mut d, "discInventory", write_items(host, &self.discs));
        put(&mut d, "isPlaying", Tag::Byte(i8::from(self.playing)));
        put(&mut d, "shuffle", Tag::Byte(i8::from(self.shuffle)));
        put(&mut d, "repeatMode", Tag::String(self.repeat.name().into()));
        put(
            &mut d,
            "discSlotActive",
            Tag::Int(self.active.map_or(-1, |a| a as i32)),
        );
        put(&mut d, "discPlayed", Tag::Int(self.played as u32 as i32));
        set_custom_data(upgrade, d);
    }

    pub fn filled(&self) -> Vec<bool> {
        self.discs.iter().map(Option::is_some).collect()
    }

    /// The sound and length of the active disc.
    pub fn current(&self, host: &dyn Host) -> Option<(String, u64)> {
        let st = self.discs.get(self.active?)?.as_ref()?;
        song(&contents::item_id(host, st))
    }

    /// A disc a slot may hold: one this port knows the song of.
    pub fn valid(host: &dyn Host, stack: &ItemStack) -> bool {
        song(&contents::item_id(host, stack)).is_some()
    }
}

// ---- running ----

/// Where a disc plays from.
#[derive(Clone, Debug)]
pub enum Source {
    /// A carried backpack: the bearer, and the upgrade's slot in the backpack.
    Carried { player: u128, slot: usize },
    /// A monster wearing a backpack (it plays from its chest slot's backpack, upgrade slot 0).
    Mob { entity: u32 },
    /// A placed storage.
    Block {
        dim: String,
        pos: [i32; 3],
        slot: usize,
    },
}

#[derive(Clone, Debug)]
struct Run {
    finish: u64,
    sound: String,
    source: Source,
}

static NOW: AtomicU64 = AtomicU64::new(0);
static RUNS: Mutex<Option<HashMap<[i32; 4], Run>>> = Mutex::new(None);

fn runs() -> std::sync::MutexGuard<'static, Option<HashMap<[i32; 4], Run>>> {
    let mut g = RUNS.lock().unwrap_or_else(|e| e.into_inner());
    g.get_or_insert_with(HashMap::new);
    g
}

/// Silences `sound` for every player (`ServerStorageSoundHandler.stopPlayingDisc`).
fn silence(host: &mut dyn ServerHost, sound: &str) {
    let players: Vec<_> = host.players().server_players();
    for p in players {
        let _ = p.stop_sound(host, Some(sound), Some(SoundSource::Records));
    }
}

fn sound_out(host: &mut dyn ServerHost, source: &Source, sound: &str) {
    match source {
        Source::Carried { player, .. } => {
            if let Some(p) = host.player(*player) {
                let _ = p
                    .entity()
                    .play_bound_sound(host, sound, SoundSource::Records, 4.0, 1.0);
            }
        }
        Source::Mob { entity } => {
            if let Some(e) = host.entity(*entity) {
                let _ = e.play_bound_sound(host, sound, SoundSource::Records, 4.0, 1.0);
            }
        }
        Source::Block { dim, pos, .. } => {
            if let Some(level) = host.level(dim) {
                level.play_sound(
                    host,
                    sound,
                    [
                        f64::from(pos[0]) + 0.5,
                        f64::from(pos[1]) + 0.5,
                        f64::from(pos[2]) + 0.5,
                    ],
                    SoundSource::Records,
                    4.0,
                    1.0,
                    None,
                );
            }
        }
    }
}

/// Starts the deck's active disc (or the first one) from `source`. False when no disc can play.
pub fn play(
    host: &mut dyn ServerHost,
    id: [i32; 4],
    deck: &mut Deck,
    source: Source,
    seed: u64,
) -> bool {
    stop(host, id, deck);
    let filled = deck.filled();
    let Some((slot, played)) = choose(&filled, None, 0, deck.shuffle, true, seed) else {
        return false;
    };
    start(host, id, deck, source, slot, played)
}

fn start(
    host: &mut dyn ServerHost,
    id: [i32; 4],
    deck: &mut Deck,
    source: Source,
    slot: usize,
    played: u64,
) -> bool {
    deck.active = Some(slot);
    deck.played = played;
    let Some((sound, ticks)) = deck.current(&*host) else {
        deck.active = None;
        return false;
    };
    sound_out(host, &source, &sound);
    runs().as_mut().map(|m| {
        m.insert(
            id,
            Run {
                finish: NOW.load(Ordering::Relaxed) + ticks,
                sound,
                source,
            },
        )
    });
    deck.playing = true;
    true
}

/// Stops the deck (`stop`): the sound ends, the playlist is forgotten.
pub fn stop(host: &mut dyn ServerHost, id: [i32; 4], deck: &mut Deck) {
    if let Some(run) = runs().as_mut().and_then(|m| m.remove(&id)) {
        silence(host, &run.sound);
    }
    deck.playing = false;
    deck.active = None;
    deck.played = 0;
}

/// The next disc (`next`) or the one before it (`previous`), while playing.
pub fn skip(host: &mut dyn ServerHost, id: [i32; 4], deck: &mut Deck, forward: bool, seed: u64) {
    let Some(run) = runs().as_ref().and_then(|m| m.get(&id).cloned()) else {
        return;
    };
    let Some(active) = deck.active else { return };
    let filled = deck.filled();
    let next = if forward {
        choose(&filled, Some(active), deck.played, deck.shuffle, true, seed)
    } else {
        before(&filled, active).map(|i| (i, deck.played))
    };
    let Some((slot, played)) = next else { return };
    silence(host, &run.sound);
    start(host, id, deck, run.source, slot, played);
}

/// What a finished disc leads to (`onDiscFinished`): the next disc per the repeat mode, or the end.
fn finished(host: &mut dyn ServerHost, id: [i32; 4], deck: &mut Deck, source: Source, seed: u64) {
    let filled = deck.filled();
    let active = deck.active;
    let next = match deck.repeat {
        Repeat::One => active
            .filter(|&a| filled.get(a) == Some(&true))
            .map(|a| (a, deck.played)),
        Repeat::All => choose(&filled, active, deck.played, deck.shuffle, true, seed),
        Repeat::No => choose(&filled, active, deck.played, deck.shuffle, false, seed),
    };
    match next {
        Some((slot, played)) => {
            if !start(host, id, deck, source, slot, played) {
                stop(host, id, deck);
            }
        }
        None => stop(host, id, deck),
    }
}

/// A running deck whose storage is gone: its sound ends.
pub fn drop_run(host: &mut dyn ServerHost, id: [i32; 4]) {
    if let Some(run) = runs().as_mut().and_then(|m| m.remove(&id)) {
        silence(host, &run.sound);
    }
}

/// Advances a deck whose disc ran out (see [`due`]).
pub fn advance(host: &mut dyn ServerHost, id: [i32; 4], deck: &mut Deck, source: Source) {
    let seed = NOW.load(Ordering::Relaxed) ^ u64::from(id[0] as u32);
    finished(host, id, deck, source, seed);
}

/// Runs `f` on the deck of the storage `id` at `source` and saves it; false when the storage isn't there.
fn with_deck(
    host: &mut dyn ServerHost,
    id: [i32; 4],
    source: &Source,
    f: impl FnOnce(&mut dyn ServerHost, &mut Deck),
) -> bool {
    match source {
        Source::Carried { player, slot } => {
            let Some(mut c) = crate::ticking::carried(host, *player)
                .into_iter()
                .find(|c| contents::uuid(&c.stack) == Some(id))
            else {
                return false;
            };
            let Some(mut up) = c.contents.upgrades.get(*slot).cloned().flatten() else {
                return false;
            };
            let Some(u) = crate::upgrades::of(&*host, &up) else {
                return false;
            };
            let mut deck = Deck::read(&*host, &up, crate::upgrades::disc_slots(&u));
            f(host, &mut deck);
            deck.write(&*host, &mut up);
            c.contents.upgrades[*slot] = Some(up);
            c.save(host, *player)
        }
        Source::Mob { entity } => {
            let Some(e) = host.entity(*entity) else {
                return false;
            };
            let living = LivingRef::new(e);
            let Some(mut st) = living
                .with_equipment(&*host, EquipmentSlot::Chest, |v| v.project())
                .flatten()
            else {
                return false;
            };
            let Some(kind) = storage_items::by_item(&contents::item_id(&*host, &st)) else {
                return false;
            };
            if contents::uuid(&st) != Some(id) {
                return false;
            }
            let mut c = contents::Contents::read(&*host, &st, kind);
            let Some(mut up) = c.upgrades.first().cloned().flatten() else {
                return false;
            };
            let Some(u) = crate::upgrades::of(&*host, &up) else {
                return false;
            };
            let mut deck = Deck::read(&*host, &up, crate::upgrades::disc_slots(&u));
            f(host, &mut deck);
            deck.write(&*host, &mut up);
            c.upgrades[0] = Some(up);
            c.write(&*host, &mut st, kind);
            living
                .set_equipment(host, EquipmentSlot::Chest, Some(st))
                .is_ok()
        }
        Source::Block { dim, pos, slot } => {
            let Some(level) = host.level(dim) else {
                return false;
            };
            if !level.is_loaded(&*host, *pos) {
                return true;
            }
            let Some(kind) =
                crate::storage::kind_of_block(&crate::storage::block_id(&*host, level, *pos))
            else {
                return false;
            };
            let Some(mut st) = crate::storage::read_be(&*host, level, *pos) else {
                return false;
            };
            if contents::uuid(&st) != Some(id) {
                return false;
            }
            let mut c = contents::Contents::read(&*host, &st, kind);
            let Some(mut up) = c.upgrades.get(*slot).cloned().flatten() else {
                return false;
            };
            let Some(u) = crate::upgrades::of(&*host, &up) else {
                return false;
            };
            let mut deck = Deck::read(&*host, &up, crate::upgrades::disc_slots(&u));
            f(host, &mut deck);
            deck.write(&*host, &mut up);
            c.upgrades[*slot] = Some(up);
            c.write(&*host, &mut st, kind);
            crate::storage::write_be(host, level, *pos, Some(st))
        }
    }
}

/// One server tick (`n` counts them): discs that ran out lead to the next; every second, a deck whose storage is
/// gone (dropped, moved on, broken) stops.
pub fn tick(host: &mut dyn ServerHost, n: u64) {
    NOW.store(n, Ordering::Relaxed);
    let live: Vec<([i32; 4], Source, bool)> = {
        let now = n;
        runs()
            .as_ref()
            .map(|m| {
                m.iter()
                    .map(|(k, r)| (*k, r.source.clone(), r.finish <= now))
                    .collect()
            })
            .unwrap_or_default()
    };
    for (id, source, due) in live {
        if !due && !n.is_multiple_of(20) {
            continue;
        }
        let found = with_deck(host, id, &source, |host, deck| {
            if due {
                advance(host, id, deck, source.clone());
            }
        });
        if !found {
            drop_run(host, id);
        }
    }
}

/// A monster's deck starts playing (it spawned with a jukebox upgrade, or came back with the world): the
/// backpack in its chest slot is read and its first disc played, looping over all discs.
pub fn mob_starts(host: &mut dyn ServerHost, entity: u32, seed: u64) {
    let source = Source::Mob { entity };
    let Some(e) = host.entity(entity) else { return };
    let Some(st) = LivingRef::new(e)
        .with_equipment(&*host, EquipmentSlot::Chest, |v| v.project())
        .flatten()
    else {
        return;
    };
    let Some(id) = contents::uuid(&st) else {
        return;
    };
    with_deck(host, id, &source, |host, deck| {
        deck.repeat = Repeat::All;
        play(host, id, deck, source.clone(), seed);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn songs_have_their_vanilla_length() {
        assert_eq!(
            song("minecraft:music_disc_13"),
            Some(("minecraft:music_disc.13".into(), 178 * 20))
        );
        assert_eq!(
            song("minecraft:music_disc_creator_music_box").unwrap().1,
            73 * 20
        );
        assert_eq!(song("minecraft:stone"), None);
    }

    #[test]
    fn playlist_goes_in_order_then_ends_or_wraps() {
        let filled = [true, false, true, true];
        let (a, p) = choose(&filled, None, 0, false, true, 0).unwrap();
        assert_eq!(a, 0);
        let (b, p) = choose(&filled, Some(a), p, false, false, 0).unwrap();
        assert_eq!(b, 2);
        let (c, p) = choose(&filled, Some(b), p, false, false, 0).unwrap();
        assert_eq!(c, 3);
        assert_eq!(choose(&filled, Some(c), p, false, false, 0), None);
        assert_eq!(choose(&filled, Some(c), p, false, true, 0).unwrap().0, 0);
    }

    #[test]
    fn shuffle_never_repeats_before_all_played() {
        let filled = [true; 4];
        let mut played = 0;
        let mut active = None;
        let mut seen = Vec::new();
        for seed in [7, 11, 3, 5] {
            let Some((i, p)) = choose(&filled, active, played, true, false, seed) else {
                break;
            };
            seen.push(i);
            active = Some(i);
            played = p;
        }
        seen.sort_unstable();
        assert_eq!(seen, vec![0, 1, 2, 3]);
    }

    #[test]
    fn a_lone_disc_repeats_and_previous_wraps() {
        assert_eq!(
            choose(&[false, true], Some(1), 2, false, true, 0)
                .unwrap()
                .0,
            1
        );
        assert_eq!(choose(&[false, true], Some(1), 2, false, false, 0), None);
        assert_eq!(before(&[true, false, true], 0), Some(2));
        assert_eq!(before(&[true], 0), None);
    }
}
