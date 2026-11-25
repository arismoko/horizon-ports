#![deny(unsafe_code)]

//! Armor HUD for Sunrise: a vanilla-style widget beside the hotbar that shows the worn
//! armour, with durability bars, text and a low-durability warning.

mod config;
mod layout;

use crate::config::{ArmorHudConfig, MOD_ID, SET_ENABLED, Side, register_settings};
use crate::layout::{
    EMPTY_SLOT_SPRITES, Frame, Jitter, Label, PieceFacts, Remembered, SIZE, WARNING_SIZE,
    WatchedSlot, attack_indicator_showing, background, bar_color, plan_mainhand, plan_offhand,
    plan_slots, should_play_break_sound, should_show_warning, visible_slots, widget_rect,
};
use mod_api::client::hud_layout::{Blend, PoseOp};
use mod_api::{
    ClientHost, ClientTickPostEvent, ComponentId, DrawContext, GuiDraw, GuiLayerPlacement, Hooks,
    HotbarAttackIndicatorProgressEvent, HudMode, ItemKey, ItemResource, ItemStack, ItemStackView,
    KeyConflictContext, KeyRegistration, ModKeyMappingRef, RegisterGuiLayersEvent, Registrar,
    RegistrationContext, SoundPlay, SoundSource, Tag, Text,
};
use std::borrow::Cow;
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

const HUD_LAYER_ID: &str = "armorhud:armor_hud";
const WARNING_SPRITE: &str = "armorhud:warn";
const BREAK_SOUND: &str = "armorhud:armor_breaking";
const KEY_TOGGLE: &str = "toggle";
const LAYER_HOTBAR: &str = "minecraft:hotbar";

/// Inventory slots of the worn pieces, head to feet, then the offhand.
const INVENTORY_ARMOR: [u32; 4] = [39, 38, 37, 36];
const INVENTORY_OFFHAND: u32 = 40;

/// A worn or held stack, read once for a frame.
struct Piece {
    stack: ItemStack,
    item: ItemKey,
    facts: PieceFacts,
}

/// What the HUD reads of the local player.
struct Worn {
    /// Head, chest, legs, feet.
    armor: [Option<Piece>; 4],
    offhand: Option<Piece>,
    main_hand: Option<Piece>,
    selected: usize,
}

fn piece_from_view(host: &dyn ClientHost, view: ItemStackView<'_>) -> Option<Piece> {
    let stack = view.project()?;
    let resource = ItemResource::of(&stack);
    let max_damage = ComponentId::new("minecraft:max_damage")
        .and_then(|id| resource.value(host, &id))
        .and_then(|tag| match tag {
            Tag::Int(max) => u32::try_from(max).ok().filter(|max| *max > 0),
            _ => None,
        });
    let damage = stack.damage();
    let color = resource
        .bar(host)
        .map(|bar| bar.color)
        .unwrap_or_else(|| bar_color(damage, max_damage.unwrap_or(0)));
    Some(Piece {
        item: view.item(),
        facts: PieceFacts {
            damageable: resource.damageable(host),
            damage,
            max_damage,
            bar_color: color,
        },
        stack,
    })
}

/// The local player's worn and held stacks. On a client only the local player is readable.
fn read_worn(host: &dyn ClientHost) -> Option<Worn> {
    let player = host.local_player()?;
    let selected = player.selected_slot(host).unwrap_or(0);
    player.with_inventory(host, |inventory| {
        let piece = |slot: u32| {
            inventory
                .get(slot)
                .and_then(|view| piece_from_view(host, view))
        };
        Worn {
            armor: INVENTORY_ARMOR.map(piece),
            offhand: piece(INVENTORY_OFFHAND),
            main_hand: u32::try_from(selected).ok().and_then(piece),
            selected,
        }
    })
}

fn frame_of(
    host: &dyn ClientHost,
    cfg: &ArmorHudConfig,
    size: [f32; 2],
    worn: &Worn,
    attack_progress: Option<f32>,
) -> Frame {
    let main_left = host.gui_options().main_hand_left;
    let main_side = if main_left { Side::Left } else { Side::Right };
    Frame {
        gui_width: size[0] as i32,
        gui_height: size[1] as i32,
        main_arm_on_side: main_side == cfg.side,
        offhand_filled: worn.offhand.is_some(),
        attack_indicator_showing: attack_indicator_showing(attack_progress),
    }
}

/// The armour slots to show: head to feet pieces with their facts.
fn slots_of(cfg: &ArmorHudConfig, worn: &Worn) -> Vec<(usize, Option<usize>)> {
    // Index each present piece by its armour slot; `None` marks an empty one.
    let pieces: Vec<Option<usize>> = worn
        .armor
        .iter()
        .enumerate()
        .map(|(i, piece)| piece.as_ref().map(|_| i))
        .collect();
    visible_slots(cfg.widget_shown, cfg.reversed, pieces, |i| {
        worn.armor[*i].as_ref().is_some_and(|p| {
            should_show_warning(p.facts.damageable, p.facts.damage, p.facts.max_damage, cfg)
        })
    })
}

fn draw_label(draw: &mut dyn DrawContext, label: &Label, shadow: bool) {
    if label.text.is_empty() {
        return;
    }
    let x = if label.centered {
        label.x as f32 - (draw.text_width(&label.text) / 2.0).floor()
    } else {
        label.x as f32
    };
    draw.text(&label.text, [x, label.y as f32], label.color, shadow);
}

fn draw_warning(draw: &mut dyn DrawContext, at: (i32, i32)) {
    let size = WARNING_SIZE as f32;
    draw.sprite(
        WARNING_SPRITE,
        [0.0, 0.0, size, size],
        [at.0 as f32, at.1 as f32, size, size],
        None,
    );
}

struct State {
    jitter: Option<Jitter>,
    last: [Remembered<ItemKey>; 6],
    key_toggle: Option<ModKeyMappingRef>,
    /// The attack charge the hotbar's indicator drew this frame, if it drew one.
    attack_progress: Option<f32>,
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| {
    Mutex::new(State {
        jitter: None,
        last: [Remembered::default(); 6],
        key_toggle: None,
        attack_progress: None,
    })
});

fn lock() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn render_widget(host: &dyn ClientHost, draw: &mut dyn DrawContext) {
    if !host.on(MOD_ID) || host.hud_mode() != HudMode::AllVisible {
        return;
    }
    let cfg = ArmorHudConfig::read(host);
    if !cfg.enabled {
        return;
    }
    let Some(worn) = read_worn(host) else {
        return;
    };
    let size = draw.size();
    let mut jitter_guard = lock();
    // The hotbar layer, below this one, reports the charge each frame it draws the indicator.
    let attack_progress = jitter_guard.attack_progress.take();
    let frame = frame_of(host, &cfg, size, &worn, attack_progress);
    let slots = slots_of(&cfg, &worn);
    let jitter = jitter_guard
        .jitter
        .get_or_insert_with(|| Jitter::new(host.millis()));
    let mut bob = |bound: i32| jitter.below(bound);

    if let Some(rect) = widget_rect(&cfg, &frame, slots.len()) {
        let mut ops = vec![GuiDraw::Push(
            if cfg.orientation == config::Orientation::Vertical {
                vec![
                    PoseOp::Translate(rect.x as f32, rect.y as f32),
                    PoseOp::Rotate(std::f32::consts::FRAC_PI_2),
                    PoseOp::Translate(0.0, -(SIZE as f32)),
                ]
            } else {
                vec![PoseOp::Translate(rect.x as f32, rect.y as f32)]
            },
        )];
        for slice in background(cfg.style, slots.len()) {
            ops.push(GuiDraw::Sprite {
                sprite: Cow::Borrowed(slice.sprite),
                x: slice.dst[0],
                y: slice.dst[1],
                w: slice.dst[2],
                h: slice.dst[3],
                region: Some(slice.region),
                color: 0xFFFF_FFFF,
                blend: Blend::Textured,
            });
        }
        ops.push(GuiDraw::Pop);
        let _ = draw.gui_paint(&ops);

        let facts: Vec<(usize, Option<PieceFacts>)> = slots
            .iter()
            .map(|(slot, piece)| {
                let facts = piece
                    .and_then(|i| worn.armor[i].as_ref())
                    .map(|p| p.facts.clone());
                (*slot, facts)
            })
            .collect();
        let width = |text: &str| draw.text_width(text).ceil() as i32;
        let plans = plan_slots(&cfg, rect, &facts, &width, &mut bob);
        for plan in &plans {
            let (x, y) = (plan.x as f32, plan.y as f32);
            if plan.empty_icon
                && let Some(sprite) = EMPTY_SLOT_SPRITES.get(plan.armor_slot)
            {
                draw.sprite(
                    sprite,
                    [0.0, 0.0, 16.0, 16.0],
                    [x + 3.0, y + 3.0, 16.0, 16.0],
                    None,
                );
            }
            if let Some(piece) = worn.armor.get(plan.armor_slot).and_then(Option::as_ref) {
                draw.item_stack(&piece.stack, [x + 3.0, y + 3.0], 16.0);
            }
            if let Some(label) = &plan.label {
                draw_label(draw, label, true);
            }
            if let Some(at) = plan.warning {
                draw_warning(draw, at);
            }
        }
    }

    // The offhand's and the main hand's extras are drawn even with no armour to show.
    let main_arm_right = !host.gui_options().main_hand_left;
    if cfg.offhand_durability
        && let Some(piece) = worn.offhand.as_ref().filter(|p| p.facts.damageable)
    {
        let plan = plan_offhand(&cfg, &frame, main_arm_right, &piece.facts, &mut bob);
        if let Some(label) = &plan.label {
            draw_label(draw, label, true);
        }
        if let Some(at) = plan.warning {
            draw_warning(draw, at);
        }
    }
    if cfg.mainhand_durability
        && let Some(piece) = worn.main_hand.as_ref().filter(|p| p.facts.damageable)
        && let Some(label) = plan_mainhand(
            &cfg,
            &frame,
            i32::try_from(worn.selected).unwrap_or(0),
            &piece.facts,
        )
    {
        // An outline keeps the text readable over the world.
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let outline = Label {
                x: label.x + dx,
                y: label.y + dy,
                color: 0xFF00_0000,
                ..label.clone()
            };
            draw_label(draw, &outline, false);
        }
        draw_label(draw, &label, false);
    }
}

/// Remember the worn and held stacks and play the breaking sound when a piece got worn down
/// to the warning.
fn watch_wear(host: &mut dyn ClientHost) {
    let cfg = ArmorHudConfig::read(host);
    if !cfg.play_break_sound {
        return;
    }
    let Some(worn) = read_worn(host) else {
        return;
    };
    let current: [(WatchedSlot, Option<&Piece>); 6] = [
        (WatchedSlot::Armor, worn.armor[0].as_ref()),
        (WatchedSlot::Armor, worn.armor[1].as_ref()),
        (WatchedSlot::Armor, worn.armor[2].as_ref()),
        (WatchedSlot::Armor, worn.armor[3].as_ref()),
        (WatchedSlot::MainHand, worn.main_hand.as_ref()),
        (WatchedSlot::OffHand, worn.offhand.as_ref()),
    ];
    let mut play = false;
    {
        let mut state = lock();
        for (i, (slot, piece)) in current.iter().enumerate() {
            let now = Remembered {
                item: piece.map(|p| p.item),
                damage: piece.map_or(0, |p| p.facts.damage),
            };
            let warns = piece.is_some_and(|p| {
                should_show_warning(p.facts.damageable, p.facts.damage, p.facts.max_damage, &cfg)
            });
            let last = std::mem::replace(&mut state.last[i], now);
            play |= should_play_break_sound(*slot, &last, &now, warns, &cfg);
        }
    }
    if play {
        let mut sound = SoundPlay::ui(BREAK_SOUND);
        sound.source = SoundSource::Players;
        let _ = host.play_local_sound(&sound, 0);
    }
}

fn poll_toggle_key(host: &mut dyn ClientHost) {
    let key = {
        let mut state = lock();
        if state.key_toggle.is_none() {
            state.key_toggle = host.mod_key_mapping(MOD_ID, KEY_TOGGLE).ok();
        }
        state.key_toggle
    };
    let Some(key) = key else {
        return;
    };
    let mut toggled = false;
    while key.consume_click(host).unwrap_or(false) {
        toggled = !toggled;
    }
    if toggled {
        let enabled = !host.setting_bool(SET_ENABLED).unwrap_or(true);
        host.set_setting(MOD_ID, SET_ENABLED, if enabled { "true" } else { "false" });
        let _ = host.set_overlay_message(
            Text::translate("armorhud.keybind.toggle.msg", vec![]),
            false,
        );
    }
}

#[derive(Default)]
struct ArmorHud;

#[mod_api::sunrise_mod]
impl Hooks for ArmorHud {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        register_settings(r);
        r.register_key_mapping(
            KeyRegistration::new(KEY_TOGGLE, "key.keyboard.unknown")
                .label("armorhud.keybind.toggle")
                .category("key.category.armorhud.key")
                .context(KeyConflictContext::Universal),
        );
    }

    fn register_gui_layers(
        &self,
        _cx: &RegistrationContext,
        event: &mut RegisterGuiLayersEvent<'_>,
    ) {
        let _ = event.layers.register(
            HUD_LAYER_ID.into(),
            GuiLayerPlacement::Above(LAYER_HOTBAR.into()),
            Box::new(render_widget),
        );
    }

    fn hotbar_attack_indicator_progress(
        &self,
        _host: &dyn ClientHost,
        event: &mut HotbarAttackIndicatorProgressEvent,
    ) {
        lock().attack_progress = Some(event.progress);
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPostEvent) {
        if !host.on(MOD_ID) {
            return;
        }
        poll_toggle_key(host);
        watch_wear(host);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_ids_and_sprites_are_namespaced() {
        assert!(WARNING_SPRITE.starts_with(MOD_ID));
        assert!(BREAK_SOUND.starts_with(MOD_ID));
        assert!(HUD_LAYER_ID.starts_with(MOD_ID));
        assert_eq!(layout::HOTBAR_SPRITE_ID, "hud/hotbar");
    }
}
