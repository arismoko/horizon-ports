//! `/biometitle <biome>`, `/dimensiontitle <dimension>` and `/tt_reload`, run on the client.

use crate::config::{self, Kind};
use crate::{State, biome_title, dimension_title, with_state};
use mod_api::text::TextStyle;
use mod_api::{ClientHost, Command, Flow, Identifier, Text};
use std::sync::Mutex;

pub fn register(commands: &mut Vec<Command>) {
    commands.push(Command::new("biometitle <biome:id>"));
    commands.push(Command::new("dimensiontitle <dimension:id>"));
    commands.push(Command::new("tt_reload"));
}

fn red(text: Text) -> Text {
    let mut style = TextStyle::default();
    style.color = Some("red".to_owned());
    text.with_style(style)
}

fn say(host: &mut dyn ClientHost, line: Text) {
    let _ = host.chat_line(line);
}

/// Run one command line; `Flow::Continue` when it is not ours.
pub fn run(state: &Mutex<State>, host: &mut dyn ClientHost, raw: &str) -> Flow {
    let mut words = raw.trim().trim_start_matches('/').split_whitespace();
    let name = words.next();
    let args: Vec<&str> = words.collect();
    match (name, args.as_slice()) {
        (Some("tt_reload"), []) => {
            with_state(state, |s| {
                s.biome.clear_timer();
                s.biome.clear_recent();
                s.dimension.clear_timer();
                s.dimension.clear_recent();
            });
            say(
                host,
                Text::literal("Loading changes from Traveler's Titles config..."),
            );
            Flow::Cancel
        }
        (Some("biometitle"), [arg]) => {
            biome_command(state, host, arg);
            Flow::Cancel
        }
        (Some("dimensiontitle"), [arg]) => {
            dimension_command(state, host, arg);
            Flow::Cancel
        }
        _ => Flow::Continue,
    }
}

fn biome_command(state: &Mutex<State>, host: &mut dyn ClientHost, arg: &str) {
    let Ok(identifier) = Identifier::parse(arg) else {
        say(
            host,
            red(Text::translate(
                "travelerstitles.commands.biometitle.invalid",
                vec![Text::literal(arg)],
            )),
        );
        return;
    };
    let id = identifier.to_string();
    let style = config::read_style(host, Kind::Biome);
    if with_state(state, |s| s.lists.biomes.contains(&id)) {
        say(
            host,
            Text::literal("That biome is blacklisted, so its title won't normally show!"),
        );
    }
    let Some((text, color)) = biome_title(host, &id, style.color) else {
        say(
            host,
            red(Text::translate(
                "travelerstitles.commands.biometitle.notfound",
                vec![Text::literal(arg)],
            )),
        );
        return;
    };
    let cooldown = config::biome_cooldown(host);
    with_state(state, |s| {
        s.biome.display(text, color, &style);
        s.biome.cooldown = cooldown;
    });
}

fn dimension_command(state: &Mutex<State>, host: &mut dyn ClientHost, arg: &str) {
    let Ok(identifier) = Identifier::parse(arg) else {
        say(
            host,
            red(Text::literal(format!(
                "\"{arg}\" does not seem to be a valid dimension"
            ))),
        );
        return;
    };
    let id = identifier.to_string();
    let style = config::read_style(host, Kind::Dimension);
    if with_state(state, |s| s.lists.dimensions.contains(&id)) {
        say(
            host,
            Text::literal("That dimension is blacklisted, so its title won't normally show!"),
        );
    }
    let (text, color) = dimension_title(host, &id, style.color);
    with_state(state, |s| s.dimension.display(text, color, &style));
}
