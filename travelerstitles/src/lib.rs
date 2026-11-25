#![deny(unsafe_code)]

//! Traveler's Titles for Sunrise: a large title fades in and out over the HUD when the
//! player enters a new biome or dimension.

mod commands;
mod config;
mod titles;

use crate::config::{Kind, Lists, MOD_ID};
use crate::titles::{Style, Title, description_id, opacity, origin, parse_color};
use mod_api::{
    ClientDisconnectingEvent, ClientHost, ClientTickPostEvent, ClientTickPreEvent, CommandEvent,
    ConfigContext, ConfigKind, ConfigLoadingEvent, ConfigReloadingEvent, DrawContext, Flow, Hooks,
    Host, HostSide, RegisterClientCommandsEvent, RegisterConfigsEvent, Registrar,
    RegistrationContext, RenderFramePreEvent, RenderGuiPostEvent, SoundPlay,
};
use std::sync::Mutex;

/// Biomes whose titles show even when the player is underground
/// (`Biomes.LUSH_CAVES`, `Biomes.DRIPSTONE_CAVES` and the `travelerstitles:is_underground` tag).
const UNDERGROUND_BIOMES: [&str; 2] = ["minecraft:lush_caves", "minecraft:dripstone_caves"];
const UNDERGROUND_TAG: &str = "travelerstitles:is_underground";

#[derive(Default)]
pub(crate) struct State {
    pub biome: Title,
    pub dimension: Title,
    pub lists: Lists,
    /// The dimension the player was in at the last tick, to notice a change.
    last_dimension: Option<String>,
    /// The frame's world partial tick, from `render_frame_pre`.
    partial_tick: f32,
}

#[derive(Default)]
struct TravelersTitles {
    state: Mutex<State>,
}

/// Run `f` on the shared state.
pub(crate) fn with_state<R>(state: &Mutex<State>, f: impl FnOnce(&mut State) -> R) -> R {
    f(&mut state.lock().unwrap_or_else(|e| e.into_inner()))
}

/// A title's text and colour for `id`: the mod's own override first, then the vanilla-style
/// entry (`TravelersTitlesCommon` language lookups). None when no language has an entry.
pub(crate) fn biome_title(
    host: &dyn ClientHost,
    id: &str,
    default_color: u32,
) -> Option<(String, u32)> {
    let overridden = description_id("travelerstitles.biome", id)?;
    let normal = description_id("biome", id)?;
    let text = host
        .translate(&overridden)
        .or_else(|| host.translate(&normal))?;
    let color = host
        .translate(&format!("{overridden}.color"))
        .or_else(|| host.translate(&format!("{normal}.color")))
        .map_or(default_color, |c| parse_color(&c));
    Some((text, color))
}

/// A dimension's title: its entry, or `???` for a dimension nobody named.
pub(crate) fn dimension_title(
    host: &dyn ClientHost,
    id: &str,
    default_color: u32,
) -> (String, u32) {
    let Some(key) = description_id("travelerstitles", id) else {
        return ("???".to_owned(), default_color);
    };
    let text = host.translate(&key).unwrap_or_else(|| "???".to_owned());
    let color = host
        .translate(&format!("{key}.color"))
        .map_or(default_color, |c| parse_color(&c));
    (text, color)
}

pub(crate) fn play_entry_sound(host: &dyn ClientHost, kind: Kind) {
    let (volume, pitch) = config::sound(host, kind);
    let id = match kind {
        Kind::Biome => "travelerstitles:biome",
        Kind::Dimension => "travelerstitles:dimension",
    };
    let sound = SoundPlay::ui(id)
        .with_volume(volume.clamp(0.0, 2.0))
        .with_pitch(pitch.clamp(0.5, 2.0));
    let _ = host.play_local_sound(&sound, 0);
}

impl TravelersTitles {
    /// `TitleRenderManager.playerTick`.
    fn player_tick(&self, host: &dyn ClientHost) {
        let Some(player) = host.local_player() else {
            return;
        };
        let h: &dyn Host = host;
        let (Some(level), Some(pos), Some(dimension)) = (
            player.level(h),
            player.entity().block_position(h),
            player.dimension(h),
        ) else {
            return;
        };
        if !level.is_loaded(h, pos) {
            return;
        }
        let Some(dimension_type) = level.dimension_type(h) else {
            return;
        };
        let dimension = dimension.to_string();
        let underground = dimension_type.has_skylight && !level.can_see_sky(h, pos).unwrap_or(true);

        let biome_style = config::read_style(host, Kind::Biome);
        let dimension_style = config::read_style(host, Kind::Dimension);

        with_state(&self.state, |state| {
            // `playerChangedDimension`: forget the recent biomes when the dimension changes.
            let changed = state
                .last_dimension
                .as_ref()
                .is_some_and(|last| *last != dimension);
            state.last_dimension = Some(dimension.clone());
            if changed && biome_style.enabled && config::biome_reset_cache_on_dimension_change(host)
            {
                state.biome.clear_timer();
                state.biome.clear_recent();
                state.biome.displayed = None;
            }

            // Dimension title.
            let skip_dimension = underground && config::only_at_surface(host, Kind::Dimension);
            if !skip_dimension
                && dimension_style.enabled
                && !state.dimension.recently_shown(&dimension_type.id)
                && !state.lists.dimensions.contains(&dimension)
            {
                let (text, color) = dimension_title(host, &dimension, dimension_style.color);
                state.dimension.display(text, color, &dimension_style);
                state.dimension.add_recent(1, dimension_type.id.clone());
                play_entry_sound(host, Kind::Dimension);
            }

            // Biome title.
            let Some(biome) = level.client_biome(h, pos) else {
                return;
            };
            let underground_biome = UNDERGROUND_BIOMES.contains(&biome.as_str())
                || level.biome_in_tag(h, &biome, UNDERGROUND_TAG) == Some(true);
            if underground && config::only_at_surface(host, Kind::Biome) && !underground_biome {
                return;
            }
            if !biome_style.enabled
                || state.biome.cooldown > 0
                || state.biome.recently_shown(&biome)
                || state.lists.biomes.contains(&biome)
            {
                return;
            }
            let Some((text, color)) = biome_title(host, &biome, biome_style.color) else {
                return;
            };
            // No need to display if the title has not changed.
            if state.biome.displayed.as_deref() == Some(text.as_str()) {
                return;
            }
            state.biome.display(text, color, &biome_style);
            state.biome.cooldown = config::biome_cooldown(host);
            state
                .biome
                .add_recent(config::biome_cache_size(host), biome);
            // The biome sound would overlap the dimension's.
            if !state.dimension.is_showing() {
                play_entry_sound(host, Kind::Biome);
            }
        });
    }

    fn draw_title(draw: &mut dyn DrawContext, title: &Title, style: &Style, partial: f32) {
        let Some(text) = title.displayed.as_deref() else {
            return;
        };
        if title.timer <= 0 || !style.size.is_finite() {
            return;
        }
        let opacity = opacity(title.timer, partial, style);
        if opacity <= 8 {
            return;
        }
        let width = draw.text_width(text);
        let at = origin(style, width, draw.size());
        let argb = ((opacity as u32) << 24) | (title.color & 0xFF_FFFF);
        draw.text_scaled(text, at, style.size, argb, style.shadow);
    }
}

#[mod_api::sunrise_mod]
impl Hooks for TravelersTitles {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        config::register_settings(r);
    }

    fn register_configs(&self, _ctx: &dyn ConfigContext, event: &mut RegisterConfigsEvent<'_>) {
        config::register_mod_configs(event.configs);
    }

    fn config_loading(&self, _ctx: &dyn ConfigContext, event: &mut ConfigLoadingEvent<'_>) {
        if event.config.mod_id != MOD_ID || event.config.kind != ConfigKind::Client {
            return;
        }
        let lists = Lists::parse(&event.config.text);
        with_state(&self.state, |state| state.lists = lists);
    }

    fn config_reloading(&self, _host: HostSide<'_>, event: &mut ConfigReloadingEvent<'_>) {
        if event.config.mod_id != MOD_ID || event.config.kind != ConfigKind::Client {
            return;
        }
        let lists = Lists::parse(&event.config.text);
        with_state(&self.state, |state| state.lists = lists);
    }

    fn register_client_commands(
        &self,
        _host: &mut dyn ClientHost,
        event: &mut RegisterClientCommandsEvent<'_>,
    ) {
        commands::register(event.commands);
    }

    fn command(&self, host: &mut dyn ClientHost, event: &mut CommandEvent<'_>) -> Flow {
        if !host.on(MOD_ID) {
            return Flow::Continue;
        }
        commands::run(&self.state, host, event.command)
    }

    /// `Minecraft.tick` head: every title counts down while the game runs.
    fn client_tick_pre(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPreEvent) {
        if !host.on(MOD_ID) || host.is_paused() {
            return;
        }
        with_state(&self.state, |state| {
            state.dimension.tick();
            state.biome.tick();
        });
    }

    /// `LocalPlayer.tick` tail.
    fn client_tick_post(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPostEvent) {
        if !host.on(MOD_ID) || host.is_paused() {
            return;
        }
        self.player_tick(&*host);
    }

    fn client_disconnecting(&self, _host: &dyn ClientHost, _event: &mut ClientDisconnectingEvent) {
        with_state(&self.state, |state| state.last_dimension = None);
    }

    /// `DeltaTracker.getGameTimeDeltaPartialTick` for the frame, held while paused.
    fn render_frame_pre(&self, host: &mut dyn ClientHost, event: &mut RenderFramePreEvent) {
        if !host.on(MOD_ID) {
            return;
        }
        let partial = event.world_partial_tick;
        with_state(&self.state, |state| state.partial_tick = partial);
    }

    fn render_gui_post(&self, host: &dyn ClientHost, event: &mut RenderGuiPostEvent<'_>) {
        if !host.on(MOD_ID) || host.debug_overlay_visible() {
            return;
        }
        let dimension_style = config::read_style(host, Kind::Dimension);
        let biome_style = config::read_style(host, Kind::Biome);
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let partial = state.partial_tick;
        Self::draw_title(event.draw, &state.dimension, &dimension_style, partial);
        Self::draw_title(event.draw, &state.biome, &biome_style, partial);
    }
}
