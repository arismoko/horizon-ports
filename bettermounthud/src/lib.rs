#![deny(unsafe_code)]

//! Better Mount HUD for Sunrise: while riding a mount that shows its health, the HUD keeps
//! the player's own hunger bar, puts the mount's hearts and the air bubbles above it, shows
//! the experience bar (with its level) instead of the jump bar, and brings the jump bar
//! back only while a jump is charged.

mod layout;

use crate::layout::{
    AirIcon, Bubble, HeartFill, air_icons, food_icons, pop_sound, vehicle_hearts,
    vehicle_max_hearts, xp_bar, xp_level_origin,
};
use mod_api::{
    ClientHost, DrawContext, GameMode, Hooks, HudMode, PlayerRef, RegisterGuiLayersEvent,
    RegistrationContext, SoundPlay, SoundSource,
};
use std::sync::atomic::{AtomicI32, Ordering};

const FOOD_LAYER: &str = "minecraft:food_level";
const VEHICLE_LAYER: &str = "minecraft:vehicle_health";
const AIR_LAYER: &str = "minecraft:air_level";
const CONTEXTUAL_LAYER: &str = "minecraft:contextual_info_bar";
const JUMP_KEY: &str = "key.jump";
const POP_SOUND: &str = "minecraft:ui.hud.bubble_pop";

/// The air bubble slot whose pop already sounded (`Hud.lastBubblePopSoundPlayed`).
static LAST_POP_SLOT: AtomicI32 = AtomicI32::new(0);

#[derive(Default)]
struct BetterMountHud;

/// The local player riding a mount that shows its health.
struct Ride {
    player: PlayerRef,
    hearts: i32,
    health: f32,
}

fn hud_visible(host: &dyn ClientHost) -> bool {
    host.hud_mode() == HudMode::AllVisible
}

/// Whether the local player takes damage, the modes whose HUD shows status rows and experience
/// (`MultiPlayerGameMode.canHurtPlayer`, `hasExperience`).
fn survival_like(host: &dyn ClientHost, player: PlayerRef) -> bool {
    matches!(
        player.game_mode(host),
        Some(GameMode::Survival | GameMode::Adventure)
    )
}

/// The local player's mount and its health, when it shows hearts (`Hud.getPlayerVehicleWithHealth`
/// with `getVehicleMaxHearts` not zero).
fn ride(host: &dyn ClientHost) -> Option<Ride> {
    let player = host.local_player()?;
    let vehicle = player.entity().vehicle(host)?;
    let living = host.living(vehicle)?;
    let hearts = vehicle_max_hearts(living.max_health(host)?);
    if hearts == 0 {
        return None;
    }
    Some(Ride {
        player,
        hearts,
        health: living.health(host)?,
    })
}

/// The ride to redraw: HUD visible, survival or adventure, and a mount with hearts.
fn survival_ride(host: &dyn ClientHost) -> Option<Ride> {
    if !hud_visible(host) {
        return None;
    }
    let ride = ride(host)?;
    survival_like(host, ride.player).then_some(ride)
}

fn jump_key_down(host: &dyn ClientHost) -> bool {
    host.vanilla_key_mapping(JUMP_KEY)
        .ok()
        .and_then(|mapping| mapping.is_down(host).ok())
        .unwrap_or(false)
}

fn paint_icon(draw: &mut dyn DrawContext, sprite: &str, x: i32, y: i32) {
    draw.sprite(
        sprite,
        [0.0, 0.0, 9.0, 9.0],
        [x as f32, y as f32, 9.0, 9.0],
        None,
    );
}

struct FoodPlan {
    level: i32,
    saturation: f32,
    hunger: bool,
    tick: i32,
}

fn food_plan(host: &dyn ClientHost, ride: &Ride) -> Option<FoodPlan> {
    let food = ride.player.food(host)?;
    let hunger = host
        .effect_key("minecraft:hunger")
        .zip(host.living(ride.player.entity()))
        .is_some_and(|(effect, living)| living.potion_affected(host, effect));
    Some(FoodPlan {
        level: food.level,
        saturation: food.saturation,
        hunger,
        tick: host.gui_ticks(),
    })
}

fn paint_food(draw: &mut dyn DrawContext, plan: &FoodPlan) {
    let [width, height] = draw.size();
    let (empty, half, full) = if plan.hunger {
        (
            "hud/food_empty_hunger",
            "hud/food_half_hunger",
            "hud/food_full_hunger",
        )
    } else {
        ("hud/food_empty", "hud/food_half", "hud/food_full")
    };
    for icon in food_icons(
        width as i32,
        height as i32,
        plan.level,
        plan.saturation,
        plan.tick,
    ) {
        paint_icon(draw, empty, icon.x, icon.y);
        match icon.fill {
            HeartFill::Full => paint_icon(draw, full, icon.x, icon.y),
            HeartFill::Half => paint_icon(draw, half, icon.x, icon.y),
            HeartFill::Empty => {}
        }
    }
}

fn paint_vehicle(draw: &mut dyn DrawContext, ride: &Ride, above_food: bool) {
    let [width, height] = draw.size();
    for heart in vehicle_hearts(
        width as i32,
        height as i32,
        ride.hearts,
        ride.health,
        above_food,
    ) {
        paint_icon(draw, "hud/heart/vehicle_container", heart.x, heart.y);
        match heart.fill {
            HeartFill::Full => paint_icon(draw, "hud/heart/vehicle_full", heart.x, heart.y),
            HeartFill::Half => paint_icon(draw, "hud/heart/vehicle_half", heart.x, heart.y),
            HeartFill::Empty => {}
        }
    }
}

struct AirPlan {
    hearts: i32,
    air: i32,
    max_air: i32,
    under_water: bool,
    tick: i32,
}

fn air_plan(host: &dyn ClientHost, ride: &Ride) -> Option<AirPlan> {
    let entity = ride.player.entity();
    Some(AirPlan {
        hearts: ride.hearts,
        air: entity.air_supply(host)?,
        max_air: entity.max_air_supply(host)?,
        under_water: entity.is_under_water(host)?,
        tick: host.gui_ticks(),
    })
}

/// Paint the air row and return the slot that is bursting, if any.
fn paint_air(draw: &mut dyn DrawContext, plan: &AirPlan) -> Option<i32> {
    let [width, height] = draw.size();
    let icons = air_icons(
        width as i32,
        height as i32,
        plan.hearts,
        plan.air,
        plan.max_air,
        plan.under_water,
        plan.tick,
    )?;
    let mut bursting = None;
    for AirIcon { x, y, kind, slot } in icons {
        match kind {
            Bubble::Full => paint_icon(draw, "hud/air", x, y),
            Bubble::Bursting => {
                paint_icon(draw, "hud/air_bursting", x, y);
                bursting = Some(slot);
            }
            Bubble::Empty => paint_icon(draw, "hud/air_empty", x, y),
        }
    }
    bursting
}

struct ExperiencePlan {
    level: i32,
    progress: f32,
}

/// Whether the experience bar replaces the jump bar: a jumpable mount, a mode with
/// experience, and no jump being charged.
fn experience_plan(host: &dyn ClientHost) -> Option<ExperiencePlan> {
    if !hud_visible(host) || host.jumpable_vehicle().is_none() || jump_key_down(host) {
        return None;
    }
    let player = host.local_player()?;
    if !survival_like(host, player) {
        return None;
    }
    let experience = player.experience(host)?;
    Some(ExperiencePlan {
        level: experience.level,
        progress: experience.progress,
    })
}

fn paint_experience(draw: &mut dyn DrawContext, plan: &ExperiencePlan) {
    let [width, height] = draw.size();
    let (width, height) = (width as i32, height as i32);
    let (left, top, filled) = xp_bar(width, height, plan.progress);
    draw.sprite(
        "hud/experience_bar_background",
        [0.0, 0.0, 182.0, 5.0],
        [left as f32, top as f32, 182.0, 5.0],
        None,
    );
    if filled > 0 {
        draw.sprite(
            "hud/experience_bar_progress",
            [0.0, 0.0, filled as f32, 5.0],
            [left as f32, top as f32, filled as f32, 5.0],
            None,
        );
    }
    if plan.level > 0 {
        let text = plan.level.to_string();
        let (x, y) = xp_level_origin(width, height, draw.text_width(&text) as i32);
        let (x, y) = (x as f32, y as f32);
        for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            draw.text(&text, [x + dx, y + dy], 0xFF00_0000, false);
        }
        draw.text(&text, [x, y], 0xFF80_FF20, false);
    }
}

#[mod_api::sunrise_mod]
impl Hooks for BetterMountHud {
    fn register_gui_layers(
        &self,
        _ctx: &RegistrationContext,
        event: &mut RegisterGuiLayersEvent<'_>,
    ) {
        // The hunger bar stays while riding: vanilla leaves it out for a mount with hearts.
        event.layers.wrap_layer(
            FOOD_LAYER,
            Box::new(|ctx| {
                let plan = {
                    let host = ctx.host();
                    survival_ride(host).and_then(|ride| food_plan(host, &ride))
                };
                match plan {
                    Some(plan) => ctx.with_draw(&mut |draw| paint_food(draw, &plan)),
                    None => ctx.draw_previous(),
                }
            }),
        );

        // The mount's hearts go one row up when the hunger row is drawn beneath them. When the
        // mount's health cannot be read, vanilla's own hearts draw.
        event.layers.wrap_layer(
            VEHICLE_LAYER,
            Box::new(|ctx| {
                let plan = {
                    let host = ctx.host();
                    hud_visible(host).then(|| ride(host)).flatten().map(|ride| {
                        let above_food = survival_like(host, ride.player);
                        (ride, above_food)
                    })
                };
                match plan {
                    Some((ride, above_food)) => {
                        ctx.with_draw(&mut |draw| paint_vehicle(draw, &ride, above_food))
                    }
                    None => ctx.draw_previous(),
                }
            }),
        );

        // The air bubbles go above the mount's rows.
        event.layers.wrap_layer(
            AIR_LAYER,
            Box::new(|ctx| {
                let plan = {
                    let host = ctx.host();
                    survival_ride(host).and_then(|ride| air_plan(host, &ride))
                };
                let Some(plan) = plan else {
                    ctx.draw_previous();
                    return;
                };
                let mut bursting = None;
                ctx.with_draw(&mut |draw| bursting = paint_air(draw, &plan));
                match bursting {
                    Some(slot) if LAST_POP_SLOT.swap(slot, Ordering::Relaxed) != slot => {
                        let (volume, pitch) = pop_sound(plan.air, plan.max_air);
                        let host = ctx.host();
                        let position = host
                            .local_player()
                            .and_then(|player| player.entity().position(host))
                            .unwrap_or([0.0; 3]);
                        let sound = SoundPlay::at(POP_SOUND, SoundSource::Players, position)
                            .with_volume(volume)
                            .with_pitch(pitch);
                        let _ = host.play_local_sound(&sound, 0);
                    }
                    Some(_) => {}
                    None if !plan.under_water => LAST_POP_SLOT.store(0, Ordering::Relaxed),
                    None => {}
                }
            }),
        );

        // The experience bar and level stay while riding; the jump bar only shows while
        // the jump key is held.
        event.layers.wrap_layer(
            CONTEXTUAL_LAYER,
            Box::new(|ctx| match experience_plan(ctx.host()) {
                Some(plan) => ctx.with_draw(&mut |draw| paint_experience(draw, &plan)),
                None => ctx.draw_previous(),
            }),
        );
    }
}
