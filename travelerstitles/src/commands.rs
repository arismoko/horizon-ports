//! `/biometitle <biome>`, `/dimensiontitle <dimension>` and `/tt_reload`, run on the client.

use crate::config::{self, Kind, MOD_ID};
use crate::{State, biome_title, dimension_title, with_state};
use mod_api::text::TextStyle;
use mod_api::{
    ClientCommandRegistration, ClientHost, Command, CommandArgumentValue, CommandInvocation,
    Identifier, Text,
};
use std::sync::{Arc, Mutex};

pub fn register(commands: &mut Vec<ClientCommandRegistration>, state: &Arc<Mutex<State>>) {
    let titles = Arc::clone(state);
    commands.push(Command::new("biometitle <biome:id>").executes_client(
        move |host, invocation| {
            if !host.on(MOD_ID) {
                return None;
            }
            let arg = id_argument(invocation, "biome")?;
            biome_command(&titles, host, &arg)
        },
    ));
    let titles = Arc::clone(state);
    commands.push(
        Command::new("dimensiontitle <dimension:id>").executes_client(move |host, invocation| {
            if !host.on(MOD_ID) {
                return None;
            }
            let arg = id_argument(invocation, "dimension")?;
            dimension_command(&titles, host, &arg)
        }),
    );
    let titles = Arc::clone(state);
    commands.push(Command::new("tt_reload").executes_client(move |host, _| {
        if !host.on(MOD_ID) {
            return None;
        }
        with_state(&titles, |s| {
            s.biome.clear_timer();
            s.biome.clear_recent();
            s.dimension.clear_timer();
            s.dimension.clear_recent();
        });
        say(
            host,
            Text::literal("Loading changes from Traveler's Titles config..."),
        );
        Some(1)
    }));
}

/// The `id` argument `name` as text.
fn id_argument(invocation: &CommandInvocation, name: &str) -> Option<String> {
    match invocation.argument(name)? {
        CommandArgumentValue::String(text) => Some(text.clone()),
        CommandArgumentValue::Id(id) => Some(id.to_string()),
        _ => None,
    }
}

fn red(text: Text) -> Text {
    let mut style = TextStyle::default();
    style.color = Some("red".to_owned());
    text.with_style(style)
}

fn say(host: &mut dyn ClientHost, line: Text) {
    let _ = host.chat_line(line);
}

fn biome_command(state: &Mutex<State>, host: &mut dyn ClientHost, arg: &str) -> Option<i32> {
    let Ok(identifier) = Identifier::parse(arg) else {
        say(
            host,
            red(Text::translate(
                "travelerstitles.commands.biometitle.invalid",
                vec![Text::literal(arg)],
            )),
        );
        return None;
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
        return None;
    };
    let cooldown = config::biome_cooldown(host);
    with_state(state, |s| {
        s.biome.display(text, color, &style);
        s.biome.cooldown = cooldown;
    });
    Some(1)
}

fn dimension_command(state: &Mutex<State>, host: &mut dyn ClientHost, arg: &str) -> Option<i32> {
    let Ok(identifier) = Identifier::parse(arg) else {
        say(
            host,
            red(Text::literal(format!(
                "\"{arg}\" does not seem to be a valid dimension"
            ))),
        );
        return None;
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
    Some(1)
}
