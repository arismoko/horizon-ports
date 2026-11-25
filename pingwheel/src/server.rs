use crate::config::{
    ChannelMode, MAX_CHANNEL_LENGTH, persist_server_config, server_config, update_server_config,
};
use crate::math::team_color_to_argb;
use crate::payloads::{PingLocationC2S, PingLocationS2C, UpdateChannelC2S, channels};
use crate::rate_limiter::RateLimiter;
use mod_api::text::TextStyle;
use mod_api::{
    Command, CommandArgumentValue, CommandRegistration, IPayloadContext, ItemStack, PlayerRef,
    ScoreHolder, ServerHost, Text,
};
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Instant;

#[derive(Default)]
struct ServerState {
    player_channels: HashMap<u128, String>,
    player_rates: HashMap<u128, RateLimiter>,
}

static SERVER_STATE: OnceLock<Mutex<ServerState>> = OnceLock::new();

fn lock_state() -> MutexGuard<'static, ServerState> {
    SERVER_STATE
        .get_or_init(|| Mutex::new(ServerState::default()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

pub fn reset_server_state() {
    let mut state = lock_state();
    state.player_channels.clear();
    state.player_rates.clear();
}

pub fn on_player_logged_out(player_uuid: u128) {
    let mut state = lock_state();
    state.player_channels.remove(&player_uuid);
    state.player_rates.remove(&player_uuid);
}

fn validate_channel(channel: &str) -> bool {
    channel.chars().count() <= MAX_CHANNEL_LENGTH
}

fn is_valid_pos(pos: [f64; 3]) -> bool {
    pos.iter().all(|v| v.is_finite())
}

pub fn handle_update_channel(
    packet: UpdateChannelC2S,
    ctx: &mut dyn IPayloadContext,
) -> Result<(), String> {
    let Ok(player) = ctx.player() else {
        return Ok(());
    };
    let Ok(host) = ctx.server_host() else {
        return Ok(());
    };
    if !validate_channel(&packet.channel) {
        let _ = player.tell(
            host,
            Text::literal(
                "§8[Ping-Wheel] §cChannel couldn't be updated: channel name exceeds maximum length",
            ),
        );
        return Ok(());
    }

    update_player_channel(player.uuid(), &packet.channel);
    Ok(())
}

pub fn handle_ping_location(
    packet: PingLocationC2S,
    ctx: &mut dyn IPayloadContext,
) -> Result<(), String> {
    let Ok(player) = ctx.player() else {
        return Ok(());
    };
    let Ok(host) = ctx.server_host() else {
        return Ok(());
    };

    if !validate_channel(&packet.channel) || !is_valid_pos(packet.pos) {
        let _ = player.tell(
            host,
            Text::literal("§8[Ping-Wheel] §cUnable to send ping: invalid ping payload"),
        );
        return Ok(());
    }

    let cfg = server_config();
    let sender_uuid = player.uuid();

    // Rate limit check.
    if cfg.rate_limit > 0 {
        let mut state = lock_state();
        let limiter = state.player_rates.entry(sender_uuid).or_default();
        if limiter.check_exceeded(
            Instant::now(),
            cfg.ms_to_regenerate.max(0) as u64,
            cfg.rate_limit.max(0) as u64,
        ) {
            return Ok(());
        }
    }

    let channel = packet.channel.as_str();
    let sender_team_name = player_team_name(&*host, player);

    if channel.is_empty() && cfg.default_channel_mode == ChannelMode::Disabled {
        let _ = player.tell(
            host,
            Text::literal(
                "§8[Ping-Wheel] §eMust be in a channel to ping location\n§fUse §a/pingwheel channel§f to switch",
            ),
        );
        return Ok(());
    }

    if channel.is_empty()
        && cfg.default_channel_mode == ChannelMode::TeamOnly
        && sender_team_name.is_none()
    {
        let _ = player.tell(
            host,
            Text::literal(
                "§8[Ping-Wheel] §eMust be in a team or channel to ping location\n§fUse §a/pingwheel channel§f to switch",
            ),
        );
        return Ok(());
    }

    update_player_channel(sender_uuid, channel);

    // Strip entity tracking when player tracking is disabled and the target is a player.
    let target_entity_id = match packet.entity_id {
        Some(ent_id) if !cfg.player_tracking_enabled && entity_is_player(host, ent_id) => None,
        other => other,
    };

    // Resolve dropped item stack on the server so clients can render item icons.
    let item_stack = target_entity_id.and_then(|ent_id| resolve_item_stack(&*host, ent_id));

    let author_name = player.name(&*host).unwrap_or("Player").to_owned();
    let team_color = player_team_color(&*host, player);

    let packet_out = PingLocationS2C {
        channel: packet.channel.clone(),
        pos: packet.pos,
        entity_id: target_entity_id,
        sequence: packet.sequence,
        dimension: packet.dimension.clone(),
        author: sender_uuid,
        author_name,
        team_color,
        item_stack,
    };

    let Some(ch) = channels() else {
        return Ok(());
    };

    // Collect recipients before sending.
    let all_players = host.players().server_players();
    let recipients: Vec<PlayerRef> = {
        let state = lock_state();
        all_players
            .into_iter()
            .filter(|recipient| {
                let rec_channel = state
                    .player_channels
                    .get(&recipient.uuid())
                    .map(String::as_str)
                    .unwrap_or("");
                if rec_channel != channel {
                    return false;
                }
                if channel.is_empty() && cfg.default_channel_mode != ChannelMode::Global {
                    let rec_team = player_team_name(&*host, *recipient);
                    return rec_team == sender_team_name;
                }
                true
            })
            .collect()
    };

    for recipient in recipients {
        let _ = ch.ping_location_s2c.send(host, recipient, &packet_out);
    }

    Ok(())
}

fn update_player_channel(player_uuid: u128, channel: &str) {
    let mut state = lock_state();
    if channel.is_empty() {
        state.player_channels.remove(&player_uuid);
    } else {
        state
            .player_channels
            .insert(player_uuid, channel.to_owned());
    }
}

fn player_team_name(host: &dyn ServerHost, player: PlayerRef) -> Option<String> {
    let team = host.holder_team(ScoreHolder::Player(player))?;
    team.name(host)
}

fn player_team_color(host: &dyn ServerHost, player: PlayerRef) -> Option<u32> {
    let team = host.holder_team(ScoreHolder::Player(player))?;
    let color = team.color(host).ok()??;
    Some(team_color_to_argb(color))
}

fn entity_is_player(host: &mut dyn ServerHost, entity_id: u32) -> bool {
    let Some(entity) = host.entity(entity_id) else {
        return false;
    };
    let Some(uuid) = entity.uuid(host) else {
        return false;
    };
    host.player(uuid).is_some()
}

fn resolve_item_stack(host: &dyn ServerHost, entity_id: u32) -> Option<ItemStack> {
    let entity = host.entity(entity_id)?;
    let item_entity = host.item_entity(entity)?;
    item_entity
        .with_stack(host, |view| view.project())
        .flatten()
}

fn yellow(text: Text) -> Text {
    let mut style = TextStyle::default();
    style.color = Some("yellow".to_owned());
    text.with_style(style)
}

fn gray(text: Text) -> Text {
    let mut style = TextStyle::default();
    style.color = Some("gray".to_owned());
    text.with_style(style)
}

fn send_server_help(host: &mut dyn ServerHost) {
    let entries = [
        (
            "/pingwheel-server default_channel",
            "command.pingwheel.default_channel.get.description",
        ),
        (
            "/pingwheel-server default_channel <mode_name>",
            "command.pingwheel.default_channel.set.description",
        ),
        (
            "/pingwheel-server player_tracking",
            "command.pingwheel.player_tracking.get.description",
        ),
        (
            "/pingwheel-server player_tracking true|false",
            "command.pingwheel.player_tracking.set.description",
        ),
        (
            "/pingwheel-server regen_time",
            "command.pingwheel.regen_time.get.description",
        ),
        (
            "/pingwheel-server regen_time <milliseconds>",
            "command.pingwheel.regen_time.set.description",
        ),
        (
            "/pingwheel-server rate_limit",
            "command.pingwheel.rate_limit.get.description",
        ),
        (
            "/pingwheel-server rate_limit <limit>",
            "command.pingwheel.rate_limit.set.description",
        ),
    ];
    for (usage, desc_key) in entries {
        let line = Text::literal(format!("{usage} - "))
            .append(gray(Text::translate(desc_key, Vec::new())));
        let _ = host.commands().feedback(line, false);
    }
}

/// Registers `/pingwheel-server` (op level 2 / gamemaster) with all server config subcommands.
///
/// Note: Sunrise's command literal parser disallows `:` in command root names, so
/// `/pingwheel-server` is used in place of upstream's `/pingwheel:server`.
pub fn register_server_commands(commands: &mut Vec<CommandRegistration>) {
    commands.push(Command::op("pingwheel-server", 2).executes(|host, _, _| {
        send_server_help(host);
        Ok(1)
    }));
    commands.push(
        Command::op("pingwheel-server help", 2).executes(|host, _, _| {
            send_server_help(host);
            Ok(1)
        }),
    );
    commands.push(
        Command::op("pingwheel-server default_channel", 2).executes(|host, _, _| {
            let mode = server_config().default_channel_mode;
            let value = yellow(Text::translate(
                format!("value.pingwheel.{}", mode.as_str()),
                Vec::new(),
            ));
            let _ = host.commands().feedback(
                Text::translate(
                    "command.pingwheel.default_channel.get.response",
                    vec![value],
                ),
                false,
            );
            Ok(1)
        }),
    );
    commands.push(
        Command::op("pingwheel-server default_channel <mode_name:word>", 2).executes(
            |host, _, call| {
                let raw = match call.argument("mode_name") {
                    Some(CommandArgumentValue::String(s)) => s.clone(),
                    _ => String::new(),
                };
                let Some(mode) = ChannelMode::parse(&raw) else {
                    let valid = ChannelMode::ALL
                        .iter()
                        .map(|m| m.as_str())
                        .collect::<Vec<_>>()
                        .join(" | ");
                    let _ = host.commands().failure(Text::translate(
                        "command.pingwheel.default_channel.set.reject",
                        vec![Text::literal(valid)],
                    ));
                    return Ok(0);
                };
                let updated = update_server_config(|cfg| cfg.default_channel_mode = mode);
                persist_server_config(host, &updated);
                let value = yellow(Text::translate(
                    format!("value.pingwheel.{}", mode.as_str()),
                    Vec::new(),
                ));
                let _ = host.commands().feedback(
                    Text::translate(
                        "command.pingwheel.default_channel.set.response",
                        vec![value],
                    ),
                    true,
                );
                Ok(1)
            },
        ),
    );
    commands.push(
        Command::op("pingwheel-server player_tracking", 2).executes(|host, _, _| {
            let enabled = server_config().player_tracking_enabled;
            let _ = host.commands().feedback(
                Text::translate(
                    "command.pingwheel.player_tracking.get.response",
                    vec![yellow(Text::literal(enabled.to_string()))],
                ),
                false,
            );
            Ok(1)
        }),
    );
    commands.push(
        Command::op("pingwheel-server player_tracking <state:boolean>", 2).executes(
            |host, _, call| {
                let enabled = match call.argument("state") {
                    Some(CommandArgumentValue::Boolean(b)) => *b,
                    _ => return Ok(0),
                };
                let updated = update_server_config(|cfg| cfg.player_tracking_enabled = enabled);
                persist_server_config(host, &updated);
                let _ = host.commands().feedback(
                    Text::translate(
                        "command.pingwheel.player_tracking.set.response",
                        vec![yellow(Text::literal(enabled.to_string()))],
                    ),
                    true,
                );
                Ok(1)
            },
        ),
    );
    commands.push(
        Command::op("pingwheel-server regen_time", 2).executes(|host, _, _| {
            let ms = server_config().ms_to_regenerate;
            let _ = host.commands().feedback(
                Text::translate(
                    "command.pingwheel.regen_time.get.response",
                    vec![yellow(Text::literal(ms.to_string()))],
                ),
                false,
            );
            Ok(1)
        }),
    );
    commands.push(
        Command::op("pingwheel-server regen_time <time:integer(0,)>", 2).executes(
            |host, _, call| {
                let ms = match call.argument("time") {
                    Some(CommandArgumentValue::Integer(v)) => *v,
                    _ => return Ok(0),
                };
                let updated = update_server_config(|cfg| cfg.ms_to_regenerate = ms);
                persist_server_config(host, &updated);
                let _ = host.commands().feedback(
                    Text::translate(
                        "command.pingwheel.regen_time.set.response",
                        vec![yellow(Text::literal(ms.to_string()))],
                    ),
                    true,
                );
                Ok(1)
            },
        ),
    );
    commands.push(
        Command::op("pingwheel-server rate_limit", 2).executes(|host, _, _| {
            let limit = server_config().rate_limit;
            let _ = host.commands().feedback(
                Text::translate(
                    "command.pingwheel.rate_limit.get.response",
                    vec![yellow(Text::literal(limit.to_string()))],
                ),
                false,
            );
            Ok(1)
        }),
    );
    commands.push(
        Command::op("pingwheel-server rate_limit <limit:integer(0,)>", 2).executes(
            |host, _, call| {
                let limit = match call.argument("limit") {
                    Some(CommandArgumentValue::Integer(v)) => *v,
                    _ => return Ok(0),
                };
                let updated = update_server_config(|cfg| cfg.rate_limit = limit);
                persist_server_config(host, &updated);
                let _ = host.commands().feedback(
                    Text::translate(
                        "command.pingwheel.rate_limit.set.response",
                        vec![yellow(Text::literal(limit.to_string()))],
                    ),
                    true,
                );
                Ok(1)
            },
        ),
    );
}
