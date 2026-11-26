use crate::config::{
    KEY_DEBUG_MODE, KEY_SILENCE_DRAGON, KEY_SILENCE_EXPLOSION, KEY_SILENCE_LIGHTNING,
    KEY_SILENCE_TRADER, KEY_SILENCE_WITHER, local_settings, normalize_sound_list,
    persist_config_file, split_sound_list, update_local_settings, validate_sound,
};
use mod_api::{
    ClientCommandRegistration, ClientHost, Command, CommandArgumentValue, CommandInvocation,
    CommandRegistration, CommandSource, ServerHost, Text,
};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

pub const COMMAND_ROOT: &str = "bad_wither_no_cookie_reloaded";
pub const COMMAND_ALIAS: &str = "bwncr";

const BOOL_KEYS: [&str; 6] = [
    KEY_SILENCE_WITHER,
    KEY_SILENCE_TRADER,
    KEY_SILENCE_DRAGON,
    KEY_SILENCE_LIGHTNING,
    KEY_SILENCE_EXPLOSION,
    KEY_DEBUG_MODE,
];

static SERVER_LISTENERS: LazyLock<Mutex<HashSet<u128>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));
static CONSOLE_LISTENING: AtomicBool = AtomicBool::new(false);
static CLIENT_LISTENING: AtomicBool = AtomicBool::new(false);

pub fn server_listeners_snapshot() -> (Vec<u128>, bool) {
    let players: Vec<u128> = SERVER_LISTENERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .copied()
        .collect();
    (players, CONSOLE_LISTENING.load(Ordering::Relaxed))
}

pub fn remove_server_listener(uuid: u128) {
    SERVER_LISTENERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&uuid);
}

pub fn clear_server_listeners() {
    SERVER_LISTENERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
    CONSOLE_LISTENING.store(false, Ordering::Relaxed);
}

pub fn client_listening() -> bool {
    CLIENT_LISTENING.load(Ordering::Relaxed)
}

pub fn set_client_listening(on: bool) {
    CLIENT_LISTENING.store(on, Ordering::Relaxed);
}

pub fn register_server_commands(
    out: &mut Vec<CommandRegistration>,
    broadcast: fn(&mut dyn ServerHost),
) {
    for root in [COMMAND_ROOT, COMMAND_ALIAS] {
        register_root(out, root, broadcast);
    }
}

fn register_root(
    out: &mut Vec<CommandRegistration>,
    root: &'static str,
    broadcast: fn(&mut dyn ServerHost),
) {
    // /<root>
    out.push(Command::op(root, 2).executes(move |host, _, _| {
        let _ = host
            .commands()
            .feedback(Text::literal(usage_text(root)), false);
        Ok(1)
    }));

    // /<root> show
    out.push(
        Command::op(&format!("{root} show"), 2).executes(|host, _, _| {
            let s = local_settings();
            let line = format!(
                "Bad Wither No Cookie Reloaded current values:\n\
             silenceWither = {}\n\
             silenceTrader = {}\n\
             silenceDragon = {}\n\
             silenceLightning = {}\n\
             silenceExplosion = {}\n\
             debugMode = {}\n\
             silenceUs = {:?} ({} entries)",
                s.silence_wither,
                s.silence_trader,
                s.silence_dragon,
                s.silence_lightning,
                s.silence_explosion,
                s.debug_mode,
                s.silence_us,
                s.silence_us.len()
            );
            let _ = host.commands().feedback(Text::literal(line), false);
            Ok(1)
        }),
    );

    // /<root> <bool_key> [value] and /<root> toggle <bool_key>
    for key in BOOL_KEYS {
        out.push(
            Command::op(&format!("{root} {key}"), 2).executes(move |host, _, _| {
                let value = local_settings().get_bool(key).unwrap_or(false);
                let _ = host
                    .commands()
                    .feedback(Text::literal(format!("{key} = {value}")), false);
                Ok(i32::from(value))
            }),
        );
        out.push(
            Command::op(&format!("{root} {key} <value:boolean>"), 2).executes(
                move |host, _, inv| {
                    let Some(value) = arg_bool(inv, "value") else {
                        return Err(Box::new(Text::literal("Missing boolean value.")));
                    };
                    let settings = update_local_settings(|s| {
                        s.set_bool(key, value);
                        s.clone()
                    });
                    persist_config_file(&*host, &settings);
                    broadcast(host);
                    let _ = host.commands().feedback(
                        Text::literal(format!("{key} = {value} (saved, effective immediately)")),
                        true,
                    );
                    Ok(1)
                },
            ),
        );
        out.push(
            Command::op(&format!("{root} toggle {key}"), 2).executes(move |host, _, _| {
                let (value, settings) = update_local_settings(|s| {
                    let next = !s.get_bool(key).unwrap_or(false);
                    s.set_bool(key, next);
                    (next, s.clone())
                });
                persist_config_file(&*host, &settings);
                broadcast(host);
                let _ = host.commands().feedback(
                    Text::literal(format!("{key} = {value} (saved, effective immediately)")),
                    true,
                );
                Ok(1)
            }),
        );
    }

    // /<root> all <value:boolean>
    out.push(
        Command::op(&format!("{root} all <value:boolean>"), 2).executes(move |host, _, inv| {
            let Some(value) = arg_bool(inv, "value") else {
                return Err(Box::new(Text::literal("Missing boolean value.")));
            };
            let settings = update_local_settings(|s| {
                s.set_all_silences(value);
                s.clone()
            });
            persist_config_file(&*host, &settings);
            broadcast(host);
            let _ = host.commands().feedback(
                Text::literal(format!(
                    "silenceWither, silenceTrader, silenceDragon, silenceLightning and silenceExplosion = {value} (saved, effective immediately)"
                )),
                true,
            );
            Ok(1)
        }),
    );

    // /<root> silenceUs
    out.push(
        Command::op(&format!("{root} silenceUs"), 2).executes(|host, _, _| {
            let entries = local_settings().silence_us;
            let count = entries.len() as i32;
            let _ = host.commands().feedback(
                Text::literal(format!("silenceUs = {entries:?} ({count} entries)")),
                false,
            );
            Ok(count)
        }),
    );

    // /<root> silenceUs add <sound:text>
    out.push(
        Command::op(&format!("{root} silenceUs add <sound:text>"), 2).executes(
            move |host, _, inv| {
                let raw = arg_str(inv, "sound").unwrap_or("").trim().to_owned();
                server_add_sound(host, &raw, broadcast)
            },
        ),
    );

    // /<root> silenceUs remove <sound:text>
    out.push(
        Command::op(&format!("{root} silenceUs remove <sound:text>"), 2).executes(
            move |host, _, inv| {
                let raw = arg_str(inv, "sound").unwrap_or("").trim().to_owned();
                let removed = update_local_settings(|s| {
                    if let Some(pos) = s.silence_us.iter().position(|e| e == &raw) {
                        s.silence_us.remove(pos);
                        Some(s.clone())
                    } else {
                        None
                    }
                });
                let Some(settings) = removed else {
                    let current = local_settings().silence_us;
                    let _ = host.commands().failure(Text::literal(format!(
                        "\"{raw}\" is not in silenceUs. Current entries: {current:?}. Nothing was changed."
                    )));
                    return Ok(0);
                };
                persist_config_file(&*host, &settings);
                broadcast(host);
                let _ = host.commands().feedback(
                    Text::literal(format!(
                        "Removed \"{raw}\" from silenceUs (saved, effective immediately)"
                    )),
                    true,
                );
                Ok(1)
            },
        ),
    );

    // /<root> silenceUs clear
    out.push(
        Command::op(&format!("{root} silenceUs clear"), 2).executes(move |host, _, _| {
            let settings = update_local_settings(|s| {
                s.silence_us.clear();
                s.clone()
            });
            persist_config_file(&*host, &settings);
            broadcast(host);
            let _ = host.commands().feedback(
                Text::literal("silenceUs cleared (saved, effective immediately)"),
                true,
            );
            Ok(1)
        }),
    );

    // /<root> silenceUs <value:text>
    out.push(
        Command::op(&format!("{root} silenceUs <value:text>"), 2).executes(move |host, _, inv| {
            let raw = arg_str(inv, "value").unwrap_or("");
            let parts = split_sound_list(raw);
            for entry in &parts {
                if let Err(problem) = validate_sound(entry) {
                    let _ = host.commands().failure(Text::literal(format!(
                        "Invalid sound \"{entry}\": {problem}. Nothing was changed."
                    )));
                    return Ok(0);
                }
            }
            let entries = normalize_sound_list(parts);
            let settings = update_local_settings(|s| {
                s.silence_us = entries.clone();
                s.clone()
            });
            persist_config_file(&*host, &settings);
            broadcast(host);
            let _ = host.commands().feedback(
                Text::literal(format!(
                    "silenceUs = {entries:?} (saved, effective immediately)"
                )),
                true,
            );
            Ok(1)
        }),
    );

    // /<root> listen
    out.push(
        Command::op(&format!("{root} listen"), 2).executes(|host, source, _| {
            let on = match source {
                CommandSource::Player { player, .. } => {
                    let uuid = player.uuid();
                    let mut listeners = SERVER_LISTENERS.lock().unwrap_or_else(|e| e.into_inner());
                    if listeners.contains(&uuid) {
                        listeners.remove(&uuid);
                        false
                    } else {
                        listeners.insert(uuid);
                        true
                    }
                }
                _ => {
                    let prev = CONSOLE_LISTENING.load(Ordering::Relaxed);
                    CONSOLE_LISTENING.store(!prev, Ordering::Relaxed);
                    !prev
                }
            };
            let state = if on { "on" } else { "off" };
            let _ = host.commands().feedback(
                Text::literal(format!("Event Listening is now {state}")),
                true,
            );
            Ok(1)
        }),
    );

    // /<root> listen mute <sound:text>
    out.push(
        Command::op(&format!("{root} listen mute <sound:text>"), 2).executes(
            move |host, _, inv| {
                let raw = arg_str(inv, "sound").unwrap_or("").trim().to_owned();
                server_add_sound(host, &raw, broadcast)
            },
        ),
    );
}

fn server_add_sound(
    host: &mut dyn ServerHost,
    raw: &str,
    broadcast: fn(&mut dyn ServerHost),
) -> Result<i32, Box<Text>> {
    if let Err(problem) = validate_sound(raw) {
        let _ = host.commands().failure(Text::literal(format!(
            "Invalid sound \"{raw}\": {problem}. Nothing was changed."
        )));
        return Ok(0);
    }
    let added = update_local_settings(|s| {
        if s.silence_us.iter().any(|e| e == raw) {
            None
        } else {
            s.silence_us.push(raw.to_owned());
            Some(s.clone())
        }
    });
    let Some(settings) = added else {
        let _ = host.commands().failure(Text::literal(format!(
            "\"{raw}\" is already in silenceUs. Nothing was changed."
        )));
        return Ok(0);
    };
    persist_config_file(&*host, &settings);
    broadcast(host);
    let _ = host.commands().feedback(
        Text::literal(format!(
            "Added \"{raw}\" to silenceUs (saved, effective immediately)."
        )),
        true,
    );
    Ok(1)
}

pub fn register_client_commands(commands: &mut Vec<ClientCommandRegistration>) {
    commands.push(Command::new("bwncr listen").executes_client(|host, _| {
        let next = !CLIENT_LISTENING.fetch_xor(true, Ordering::Relaxed);
        let state = if next { "on" } else { "off" };
        let _ = host.chat_line(Text::literal(format!("Event Listening is now {state}")));
        Some(1)
    }));
    commands.push(
        Command::new("bwncr listen mute <sound:text>").executes_client(|host, invocation| {
            let Some(CommandArgumentValue::String(sound)) = invocation.argument("sound") else {
                return None;
            };
            mute_sound(host, sound.trim());
            Some(1)
        }),
    );
}

/// `/bwncr listen mute <sound>`: adds the sound to this client's `silenceUs`.
fn mute_sound(host: &mut dyn ClientHost, sound: &str) {
    if let Err(problem) = validate_sound(sound) {
        let _ = host.chat_line(Text::literal(format!(
            "Invalid sound \"{sound}\": {problem}. Nothing was changed."
        )));
        return;
    }
    let added = update_local_settings(|s| {
        if s.silence_us.iter().any(|e| e == sound) {
            None
        } else {
            s.silence_us.push(sound.to_owned());
            Some(s.clone())
        }
    });
    let Some(settings) = added else {
        let _ = host.chat_line(Text::literal(format!(
            "\"{sound}\" is already in silenceUs. Nothing was changed."
        )));
        return;
    };
    persist_config_file(&*host, &settings);
    let _ = host.chat_line(Text::literal(format!(
        "Added \"{sound}\" to silenceUs in this client's config."
    )));
}

fn usage_text(name: &str) -> String {
    format!(
        "Bad Wither No Cookie Reloaded config. Usage:\n\
         /{name} show\n\
         /{name} <silenceWither|silenceTrader|silenceDragon|silenceLightning|silenceExplosion|debugMode> [true|false]\n\
         /{name} toggle <silenceWither|silenceTrader|silenceDragon|silenceLightning|silenceExplosion|debugMode>\n\
         /{name} all <true|false>   (the five silence toggles)\n\
         /{name} silenceUs [sound,sound,...]\n\
         /{name} silenceUs add|remove <sound>\n\
         /{name} silenceUs clear\n\
         /{name} listen   (toggle: shows the name of every sound sent to you)\n\
         /{name} listen mute <sound>   (adds it to silenceUs)\n\
         Sound: the path, e.g. entity.sheep.step, or the full id, e.g. othermod:entity.thing.hurt"
    )
}

fn arg_bool(inv: &CommandInvocation, name: &str) -> Option<bool> {
    inv.arguments.iter().find_map(|arg| {
        if arg.name == name {
            match arg.value {
                CommandArgumentValue::Boolean(v) => Some(v),
                _ => None,
            }
        } else {
            None
        }
    })
}

fn arg_str<'a>(inv: &'a CommandInvocation, name: &str) -> Option<&'a str> {
    inv.arguments.iter().find_map(|arg| {
        if arg.name == name {
            match &arg.value {
                CommandArgumentValue::String(s) => Some(s.as_str()),
                _ => None,
            }
        } else {
            None
        }
    })
}
