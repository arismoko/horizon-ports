use crate::config::{
    ChiseledBookshelfTooltip, EnchantmentHint, effective_client_config,
    keep_enchantment_screen_book, synced_server_config,
};
use crate::enchant::{
    BOOKSHELF_OFFSETS, ENCHANTMENT_POWER_TRANSMITTER_TAG, chiseled_bookshelf_hit_slot,
    enchantment_translation_key, generate_rune_phrase, is_stack_enchantable,
    is_unaltered_enchanting_table, is_valid_particle_bookshelf, parse_block_state_property,
    total_experience,
};
use crate::payloads::{CluesPayload, EnchantClue, ShelfItemPayload, ShelfQueryPayload, channels};
use crate::server::REROLL_BUTTON_ID;
use mod_api::client::hud_layout::Blend;
use mod_api::text::TextStyle;
use mod_api::wire::Reader;
use mod_api::{
    AnimateTickEvent, ClientHost, CrosshairTarget, DrawContext, GameMode, Gui3dSubject, Gui3dView,
    GuiDraw, IDENTITY_TRANSFORM, Identifier, ItemResource, ItemStack, MenuScreen, MenuScreenFrame,
    MenuScreenLayout, MenuScreenOpen, ParticleKey, ParticleOptions, ScreenSlot, SoundPlay, Text,
    TooltipElement,
};
use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock, PoisonError};

const COLOR_WHITE: u32 = 0xFFFF_FFFF;
const COLOR_GRAY: u32 = 0xFFAA_AAAA;
const COLOR_RED: u32 = 0xFFFF_5555;
const COLOR_RUNE_ACTIVE: u32 = 0xFF68_5E4A;
const COLOR_RUNE_HOVER: u32 = 0xFFFF_FF80;
const COLOR_RUNE_DISABLED: u32 = 0xFF34_2F25;
const COLOR_COST_ACTIVE: u32 = 0xFF80_FF20;
const COLOR_COST_DISABLED: u32 = 0xFF40_7F10;
const COLOR_ORB_XP: u32 = 0xFF55_FF55;
const COLOR_ORB_LAPIS: u32 = 0xFF55_55FF;

static ENCHANT_PARTICLE: OnceLock<ParticleKey> = OnceLock::new();
static PENDING_REROLL_SOUND: AtomicBool = AtomicBool::new(false);

pub fn init_enchant_particle(key: ParticleKey) {
    let _ = ENCHANT_PARTICLE.set(key);
}

static CLIENT_CLUES: LazyLock<Mutex<Option<CluesPayload>>> = LazyLock::new(Mutex::default);

#[derive(Clone, Debug)]
struct ShelfCache {
    pos: [i32; 3],
    slot: u8,
    stack: Option<ItemStack>,
    queried_tick: i32,
}

static SHELF_CACHE: LazyLock<Mutex<Option<ShelfCache>>> = LazyLock::new(Mutex::default);

pub fn reset_client_state() {
    *CLIENT_CLUES.lock().unwrap_or_else(PoisonError::into_inner) = None;
    *SHELF_CACHE.lock().unwrap_or_else(PoisonError::into_inner) = None;
    PENDING_REROLL_SOUND.store(false, Ordering::Relaxed);
}

pub fn on_clues_payload(payload: CluesPayload) {
    *CLIENT_CLUES.lock().unwrap_or_else(PoisonError::into_inner) = Some(payload);
}

pub fn on_shelf_item_payload(payload: ShelfItemPayload) {
    let mut cache = SHELF_CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(entry) = cache.as_mut()
        && entry.pos == payload.pos
        && entry.slot == payload.slot
    {
        entry.stack = payload.stack;
    } else {
        *cache = Some(ShelfCache {
            pos: payload.pos,
            slot: payload.slot,
            stack: payload.stack,
            queried_tick: 0,
        });
    }
}

/// Builds the custom `easymagic:enchanting` `MenuScreen` when the server opens it.
pub fn build_enchanting_screen(
    _host: &dyn ClientHost,
    open: MenuScreenOpen<'_>,
) -> Option<MenuScreen> {
    let window = open.window;
    if !open.extra.is_empty()
        && let Ok(mut initial) = Reader::decode::<CluesPayload>(open.extra)
    {
        initial.window = window;
        on_clues_payload(initial);
    }

    let dedicated = synced_server_config().dedicated_reroll_catalyst_active();
    #[allow(clippy::needless_update)]
    let slots = if dedicated {
        vec![
            Some(ScreenSlot {
                position: [5.0, 47.0],
                ..Default::default()
            }),
            Some(ScreenSlot {
                position: [23.0, 47.0],
                no_item_icon: Some("minecraft:container/slot/lapis_lazuli".to_owned()),
                ..Default::default()
            }),
            Some(ScreenSlot {
                position: [41.0, 47.0],
                no_item_icon: Some("minecraft:container/slot/amethyst_shard".to_owned()),
                ..Default::default()
            }),
        ]
    } else {
        vec![
            Some(ScreenSlot {
                position: [15.0, 47.0],
                ..Default::default()
            }),
            Some(ScreenSlot {
                position: [35.0, 47.0],
                no_item_icon: Some("minecraft:container/slot/lapis_lazuli".to_owned()),
                ..Default::default()
            }),
            None,
        ]
    };

    let layout = MenuScreenLayout {
        width: 176.0,
        height: 166.0,
        slots,
        inventory_x: 8.0,
        inventory_y: 84.0,
        own_background: true,
        own_labels: false,
        ..Default::default()
    };

    Some(
        MenuScreen::new(layout)
            .with_draw(move |host, frame, ctx| {
                draw_enchanting_screen(host, frame, ctx, window, dedicated)
            })
            .with_click(move |host, frame, right| {
                click_enchanting_screen(host, frame, right, window, dedicated)
            })
            .with_tooltip(move |host, frame, slot, lines| {
                tooltip_enchanting_screen(host, frame, slot, lines, window, dedicated)
            }),
    )
}

fn current_costs_and_seed(frame: &MenuScreenFrame<'_>, window: i32) -> ([i32; 3], i32) {
    if frame.data.len() >= 4 {
        return ([frame.data[0], frame.data[1], frame.data[2]], frame.data[3]);
    }
    if let Some(clues) = CLIENT_CLUES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        && clues.window == window
    {
        return (clues.costs, clues.seed);
    }
    ([0; 3], 0)
}

fn current_clues_for_row(window: i32, row: usize) -> Vec<EnchantClue> {
    CLIENT_CLUES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .filter(|c| c.window == window)
        .map(|c| c.rows[row].clone())
        .unwrap_or_default()
}

fn client_menu_slots(host: &dyn ClientHost) -> [Option<ItemStack>; 3] {
    let Some(player) = host.local_player() else {
        return [None, None, None];
    };
    let Some(menu) = player.container_menu(host) else {
        return [None, None, None];
    };
    [
        menu.slot(host, 0).flatten(),
        menu.slot(host, 1).flatten(),
        menu.slot(host, 2).flatten(),
    ]
}

fn client_player_state(host: &dyn ClientHost) -> (bool, i32, f32) {
    let Some(player) = host.local_player() else {
        return (false, 0, 0.0);
    };
    let creative = player.game_mode(host) == Some(GameMode::Creative);
    let (level, progress) = player
        .experience(host)
        .map_or((0, 0.0), |e| (e.level, e.progress));
    (creative, level, progress)
}

fn can_use_reroll_client(
    creative: bool,
    exp_level: i32,
    exp_progress: f32,
    catalyst_count: u32,
    server: &crate::config::ServerConfig,
) -> bool {
    if !server.reroll_enchantments {
        return false;
    }
    if creative {
        return true;
    }
    let catalyst_cost = server.reroll_catalyst_cost.max(0) as u32;
    let xp_cost = server.reroll_experience_points_cost.max(0);
    let enough_xp = if server.rerolling_takes_enchantment_levels {
        exp_level >= xp_cost
    } else {
        total_experience(exp_level, exp_progress) >= i128::from(xp_cost)
    };
    catalyst_count >= catalyst_cost && enough_xp
}

fn reroll_button_pos(dedicated: bool) -> (f32, f32) {
    (if dedicated { 12.0 } else { 14.0 }, 16.0)
}

fn in_rect(at: [f32; 2], x: f32, y: f32, w: f32, h: f32) -> bool {
    at[0] >= x && at[0] < x + w && at[1] >= y && at[1] < y + h
}

fn draw_enchanting_screen(
    host: &dyn ClientHost,
    frame: MenuScreenFrame<'_>,
    ctx: &mut dyn DrawContext,
    window: i32,
    dedicated: bool,
) {
    let ox = frame.origin[0].round() as i32;
    let oy = frame.origin[1].round() as i32;

    let mut draws = vec![GuiDraw::Blit {
        texture: Cow::Borrowed("gui/container/enchanting_table"),
        blend: Blend::Textured,
        x: ox,
        y: oy,
        w: 176,
        h: 166,
        uv: [0.0, 0.0],
        src: [176, 166],
        tex: [256, 256],
        color: COLOR_WHITE,
    }];

    // Extra 3-slot layout when dedicated reroll catalyst is enabled
    if dedicated {
        // Cover vanilla 2-slot area cleanly with panel background color before painting 3 slots
        draws.push(GuiDraw::Fill {
            x0: ox + 4,
            y0: oy + 45,
            x1: ox + 59,
            y1: oy + 65,
            color: 0xFFC6_C6C6,
        });
        for sx in [4, 22, 40] {
            draws.push(GuiDraw::Sprite {
                sprite: Cow::Borrowed("minecraft:container/slot"),
                x: ox + sx,
                y: oy + 46,
                w: 18,
                h: 18,
                region: None,
                color: COLOR_WHITE,
                blend: Blend::Textured,
            });
        }
    }

    let (costs, seed) = current_costs_and_seed(&frame, window);
    let slots = client_menu_slots(host);
    let (creative, exp_level, exp_progress) = client_player_state(host);
    let lapis_count = slots[1].as_ref().map_or(0, ItemStack::count);
    let can_enchant = slots[0]
        .as_ref()
        .is_some_and(|s| is_stack_enchantable(host, s));

    const LEVEL_SPRITES: [&str; 3] = [
        "minecraft:container/enchanting_table/level_1",
        "minecraft:container/enchanting_table/level_2",
        "minecraft:container/enchanting_table/level_3",
    ];
    const LEVEL_DISABLED_SPRITES: [&str; 3] = [
        "minecraft:container/enchanting_table/level_1_disabled",
        "minecraft:container/enchanting_table/level_2_disabled",
        "minecraft:container/enchanting_table/level_3_disabled",
    ];

    // Base disabled slot backgrounds + active offer buttons
    for (i, &cost) in costs.iter().enumerate() {
        let row_y = 14 + 19 * (i as i32);
        let hovered = in_rect(frame.at, 60.0, row_y as f32, 108.0, 19.0);
        let active = cost > 0 && (creative || (lapis_count >= (i as u32 + 1) && exp_level >= cost));
        let slot_sprite = if cost == 0 || !active {
            "minecraft:container/enchanting_table/enchantment_slot_disabled"
        } else if hovered {
            "minecraft:container/enchanting_table/enchantment_slot_highlighted"
        } else {
            "minecraft:container/enchanting_table/enchantment_slot"
        };
        draws.push(GuiDraw::Sprite {
            sprite: Cow::Borrowed(slot_sprite),
            x: ox + 60,
            y: oy + row_y,
            w: 108,
            h: 19,
            region: None,
            color: COLOR_WHITE,
            blend: Blend::Textured,
        });
        if cost > 0 {
            let orb_sprite = if active {
                LEVEL_SPRITES[i]
            } else {
                LEVEL_DISABLED_SPRITES[i]
            };
            draws.push(GuiDraw::Sprite {
                sprite: Cow::Borrowed(orb_sprite),
                x: ox + 61,
                y: oy + row_y + 1,
                w: 16,
                h: 16,
                region: None,
                color: COLOR_WHITE,
                blend: Blend::Textured,
            });
        }
    }

    let _ = ctx.gui_paint(&draws);

    // Draw Standard Galactic Alphabet runes and level costs on each populated row
    for (i, &cost) in costs.iter().enumerate() {
        if cost <= 0 {
            continue;
        }
        let row_y = frame.origin[1] + (14 + 19 * (i as i32)) as f32;
        let hovered = in_rect(frame.at, 60.0, (14 + 19 * i) as f32, 108.0, 19.0);
        let active = creative || (lapis_count >= (i as u32 + 1) && exp_level >= cost);

        let cost_str = cost.to_string();
        let cost_w = ctx.text_width(&cost_str);
        let max_rune_w = (86.0 - cost_w).max(20.0);
        let phrase = generate_rune_phrase(seed, i);
        let rune_color = if active {
            if hovered {
                COLOR_RUNE_HOVER
            } else {
                COLOR_RUNE_ACTIVE
            }
        } else {
            COLOR_RUNE_DISABLED
        };

        let mut line1 = String::new();
        let mut line2 = String::new();
        for word in phrase.split_whitespace() {
            if line1.is_empty() {
                line1.push_str(word);
            } else if ctx.text_width(&format!("{line1} {word}")) <= max_rune_w {
                line1.push(' ');
                line1.push_str(word);
            } else if line2.is_empty() {
                line2.push_str(word);
            } else if ctx.text_width(&format!("{line2} {word}")) <= max_rune_w {
                line2.push(' ');
                line2.push_str(word);
            }
        }
        if let Ok(alt_font) = Identifier::parse("minecraft:alt") {
            let _ = ctx.text_with_font(
                &alt_font,
                &Text::literal(&line1),
                [frame.origin[0] + 80.0, row_y + 2.0],
                rune_color,
                false,
            );
            if !line2.is_empty() {
                let _ = ctx.text_with_font(
                    &alt_font,
                    &Text::literal(&line2),
                    [frame.origin[0] + 80.0, row_y + 10.0],
                    rune_color,
                    false,
                );
            }
        }

        let cost_color = if active {
            COLOR_COST_ACTIVE
        } else {
            COLOR_COST_DISABLED
        };
        ctx.text(
            &cost_str,
            [frame.origin[0] + 166.0 - cost_w, row_y + 9.0],
            cost_color,
            true,
        );
    }

    // Reroll button (or static book icon when keep_enchantment_screen_book is active)
    let server_cfg = synced_server_config();
    let client_cfg = effective_client_config(host);
    let keep_book = keep_enchantment_screen_book(&client_cfg, &server_cfg);
    let (rx_rel, ry_rel) = reroll_button_pos(dedicated);
    let rx = frame.origin[0] + rx_rel;
    let ry = frame.origin[1] + ry_rel;

    if keep_book {
        // Draw a static book model in the book region so the area remains visually clear.
        // Note: vanilla's animated opening 3D BookModel is not yet exposed on DrawContext.
        if let Ok(book_id) = Identifier::parse("minecraft:enchanted_book")
            && let Some(book_key) = host.item_key(&book_id)
        {
            let book = ItemStack::of(book_key);
            let _ = ctx.model_3d(
                Gui3dSubject::Item(&book),
                Gui3dView::new([rx + 19.0, ry + 13.0], 24.0, IDENTITY_TRANSFORM),
            );
        }
    } else {
        let cat_slot = if dedicated { 2 } else { 1 };
        let cat_count = slots[cat_slot].as_ref().map_or(0, ItemStack::count);
        let active = can_enchant
            && can_use_reroll_client(creative, exp_level, exp_progress, cat_count, &server_cfg);
        let hovered = in_rect(frame.at, rx_rel, ry_rel, 38.0, 27.0);
        draw_procedural_reroll_button(
            ctx,
            rx,
            ry,
            can_enchant,
            active,
            hovered,
            server_cfg.reroll_catalyst_cost.max(0),
            server_cfg.reroll_experience_points_cost.max(0),
            dedicated,
        );
    }
}

/// Draws an original procedural reroll button matching Easy Magic's 38×27 layout and cost orbs.
#[allow(clippy::too_many_arguments)]
fn draw_procedural_reroll_button(
    ctx: &mut dyn DrawContext,
    x: f32,
    y: f32,
    visible_active_item: bool,
    active: bool,
    hovered: bool,
    catalyst_cost: i32,
    xp_cost: i32,
    dedicated: bool,
) {
    // Button panel (38×27) with subtle 1px border & bevel
    let (bg, border_hi, border_lo) = if !visible_active_item || !active {
        (0xFF5A_4D41, 0xFF42_382F, 0xFF35_2D26)
    } else if hovered {
        (0xFFA0_8D72, 0xFFD9_C9AE, 0xFF5D_4F3D)
    } else {
        (0xFF8B_7A63, 0xFFB8_A58A, 0xFF4F_4334)
    };
    ctx.fill([x, y, 38.0, 27.0], border_lo);
    ctx.fill([x, y, 37.0, 26.0], border_hi);
    ctx.fill([x + 1.0, y + 1.0, 36.0, 25.0], bg);

    if !visible_active_item {
        return;
    }

    let arrow_color = if !active {
        0xFF88_7C70
    } else if hovered {
        0xFFFF_FF80
    } else {
        0xFFE8_DFC8
    };
    let arrow_x = if xp_cost == 0 && catalyst_cost == 0 {
        x + 12.0
    } else {
        x + 3.0
    };
    draw_reroll_arrows(ctx, arrow_x, y + 6.0, arrow_color);

    let catalyst_orb_tint = if !active {
        0xFF6A_4040
    } else if dedicated {
        0xFFA8_64E8 // amethyst purple
    } else {
        0xFF3A_62D8 // lapis blue
    };
    let xp_orb_tint = if active { 0xFF44_B83A } else { 0xFF6A_4040 };
    let xp_text_color = if active { COLOR_ORB_XP } else { COLOR_RED };
    let cat_text_color = if active {
        if dedicated {
            0xFFD5_94FF
        } else {
            COLOR_ORB_LAPIS
        }
    } else {
        COLOR_RED
    };

    if xp_cost > 0 && catalyst_cost > 0 {
        draw_cost_orb(
            ctx,
            x + 20.0,
            y + 2.0,
            catalyst_cost,
            catalyst_orb_tint,
            cat_text_color,
        );
        draw_cost_orb(ctx, x + 20.0, y + 14.0, xp_cost, xp_orb_tint, xp_text_color);
    } else if xp_cost > 0 {
        draw_cost_orb(ctx, x + 20.0, y + 8.0, xp_cost, xp_orb_tint, xp_text_color);
    } else if catalyst_cost > 0 {
        draw_cost_orb(
            ctx,
            x + 20.0,
            y + 8.0,
            catalyst_cost,
            catalyst_orb_tint,
            cat_text_color,
        );
    }
}

/// Crisp 15×15 procedural double circular reroll arrow icon.
fn draw_reroll_arrows(ctx: &mut dyn DrawContext, x: f32, y: f32, color: u32) {
    // Top arc + right arrowhead
    ctx.fill([x + 4.0, y + 2.0, 7.0, 2.0], color);
    ctx.fill([x + 2.0, y + 4.0, 2.0, 2.0], color);
    ctx.fill([x + 11.0, y + 3.0, 2.0, 3.0], color);
    ctx.fill([x + 9.0, y + 5.0, 5.0, 1.0], color);
    // Bottom arc + left arrowhead
    ctx.fill([x + 4.0, y + 11.0, 7.0, 2.0], color);
    ctx.fill([x + 11.0, y + 9.0, 2.0, 2.0], color);
    ctx.fill([x + 2.0, y + 9.0, 2.0, 3.0], color);
    ctx.fill([x + 1.0, y + 9.0, 5.0, 1.0], color);
}

/// Draws a compact cost pill/orb with the numeric cost centered inside.
fn draw_cost_orb(
    ctx: &mut dyn DrawContext,
    x: f32,
    y: f32,
    amount: i32,
    orb_color: u32,
    text_color: u32,
) {
    let label = amount.to_string();
    let w = (ctx.text_width(&label) + 4.0).clamp(11.0, 16.0);
    let draw_x = if amount > 9 { x - 2.0 } else { x };
    ctx.fill([draw_x, y + 1.0, w, 9.0], 0xCC18_1410);
    ctx.fill([draw_x + 1.0, y, w - 2.0, 11.0], 0xCC18_1410);
    ctx.fill([draw_x + 1.0, y + 1.0, w - 2.0, 9.0], orb_color);
    let tx = draw_x + (w - ctx.text_width(&label)) * 0.5;
    ctx.text(&label, [tx, y + 2.0], text_color, true);
}

fn click_enchanting_screen(
    host: &dyn ClientHost,
    frame: MenuScreenFrame<'_>,
    right: bool,
    window: i32,
    dedicated: bool,
) -> Option<i32> {
    if right {
        return None;
    }
    let (costs, _) = current_costs_and_seed(&frame, window);
    let slots = client_menu_slots(host);
    let (creative, exp_level, exp_progress) = client_player_state(host);
    let lapis_count = slots[1].as_ref().map_or(0, ItemStack::count);

    for (i, &cost) in costs.iter().enumerate() {
        let row_y = (14 + 19 * i) as f32;
        if cost > 0 && in_rect(frame.at, 60.0, row_y, 108.0, 19.0) {
            let active = creative || (lapis_count >= (i as u32 + 1) && exp_level >= cost);
            if active {
                return Some(i as i32);
            }
            return None;
        }
    }

    let server_cfg = synced_server_config();
    let (rx, ry) = reroll_button_pos(dedicated);
    if server_cfg.reroll_enchantments && in_rect(frame.at, rx, ry, 38.0, 27.0) {
        let can_enchant = slots[0]
            .as_ref()
            .is_some_and(|s| is_stack_enchantable(host, s));
        let cat_slot = if dedicated { 2 } else { 1 };
        let cat_count = slots[cat_slot].as_ref().map_or(0, ItemStack::count);
        if can_enchant
            && can_use_reroll_client(creative, exp_level, exp_progress, cat_count, &server_cfg)
        {
            PENDING_REROLL_SOUND.store(true, Ordering::Relaxed);
            return Some(REROLL_BUTTON_ID);
        }
    }

    None
}

fn tooltip_enchanting_screen(
    host: &dyn ClientHost,
    frame: MenuScreenFrame<'_>,
    slot: Option<u32>,
    lines: &mut Vec<(Text, u32)>,
    window: i32,
    dedicated: bool,
) {
    if slot.is_some() {
        return;
    }
    let server_cfg = synced_server_config();
    let (costs, _) = current_costs_and_seed(&frame, window);
    let slots = client_menu_slots(host);
    let (creative, exp_level, exp_progress) = client_player_state(host);
    let lapis_count = slots[1].as_ref().map_or(0, ItemStack::count);

    // Enchantment offer row tooltips
    for (i, &cost) in costs.iter().enumerate() {
        let row_y = (14 + 19 * i) as f32;
        if cost > 0 && in_rect(frame.at, 60.0, row_y, 108.0, 19.0) {
            let row_clues = current_clues_for_row(window, i);
            let max_levels: std::collections::HashMap<String, u32> = host
                .enchantments()
                .into_iter()
                .map(|e| (e.id, e.max_level))
                .collect();

            for clue in &row_clues {
                let max_l = max_levels.get(&clue.id).copied().unwrap_or(1);
                let name = format_enchantment_name(clue, max_l);
                let line = if server_cfg.enchantment_hint == EnchantmentHint::All {
                    name
                } else {
                    Text::translate("container.enchant.clue", vec![name])
                };
                lines.push((line, COLOR_GRAY));

                // Optional enchantment description from resource/language packs (".desc" or ".description")
                let base_key = enchantment_translation_key(&clue.id);
                let desc_key = if host.translate(&format!("{base_key}.desc")).is_some() {
                    Some(format!("{base_key}.desc"))
                } else if host.translate(&format!("{base_key}.description")).is_some() {
                    Some(format!("{base_key}.description"))
                } else {
                    None
                };
                if let Some(dkey) = desc_key {
                    let mut style = TextStyle::default();
                    style.color = Some("gray".to_owned());
                    let desc = Text::literal(" \u{25C6} ")
                        .append(Text::translate(dkey, Vec::new()))
                        .with_style(style);
                    for split_line in host.text_split(&desc, 200) {
                        lines.push((split_line, COLOR_GRAY));
                    }
                }
            }

            if !creative {
                let mut cost_lines: Vec<(Text, u32)> = Vec::new();
                if exp_level < cost {
                    cost_lines.push((
                        Text::translate(
                            "container.enchant.level.requirement",
                            vec![Text::literal(cost.to_string())],
                        ),
                        COLOR_RED,
                    ));
                } else {
                    let req = i as i32 + 1;
                    push_cost_line(
                        &mut cost_lines,
                        req,
                        i128::from(lapis_count),
                        "container.enchant.lapis.one",
                        "container.enchant.lapis.many",
                    );
                    push_cost_line(
                        &mut cost_lines,
                        req,
                        i128::from(exp_level),
                        "container.enchant.level.one",
                        "container.enchant.level.many",
                    );
                }
                if !cost_lines.is_empty() {
                    if !lines.is_empty() {
                        lines.push((Text::literal(""), COLOR_WHITE));
                    }
                    lines.extend(cost_lines);
                }
            }
            return;
        }
    }

    // Reroll button tooltip
    let (rx, ry) = reroll_button_pos(dedicated);
    if server_cfg.reroll_enchantments && in_rect(frame.at, rx, ry, 38.0, 27.0) {
        let can_enchant = slots[0]
            .as_ref()
            .is_some_and(|s| is_stack_enchantable(host, s));
        if !can_enchant {
            return;
        }
        lines.push((
            Text::translate("container.easymagic.enchanting.reroll", Vec::new()),
            COLOR_GRAY,
        ));
        if !creative {
            let mut cost_lines: Vec<(Text, u32)> = Vec::new();
            let cat_slot = if dedicated { 2 } else { 1 };
            let cat_have = slots[cat_slot].as_ref().map_or(0, ItemStack::count) as i32;
            if server_cfg.reroll_catalyst_cost > 0 {
                let (one, many) = if dedicated {
                    (
                        "container.easymagic.enchanting.reroll.one",
                        "container.easymagic.enchanting.reroll.many",
                    )
                } else {
                    (
                        "container.enchant.lapis.one",
                        "container.enchant.lapis.many",
                    )
                };
                push_cost_line(
                    &mut cost_lines,
                    server_cfg.reroll_catalyst_cost,
                    i128::from(cat_have),
                    one,
                    many,
                );
            }
            if server_cfg.reroll_experience_points_cost > 0 {
                let (one, many, have) = if server_cfg.rerolling_takes_enchantment_levels {
                    (
                        "container.enchant.level.one",
                        "container.enchant.level.many",
                        i128::from(exp_level),
                    )
                } else {
                    (
                        "container.easymagic.enchanting.experience.one",
                        "container.easymagic.enchanting.experience.many",
                        total_experience(exp_level, exp_progress),
                    )
                };
                push_cost_line(
                    &mut cost_lines,
                    server_cfg.reroll_experience_points_cost,
                    have,
                    one,
                    many,
                );
            }
            if !cost_lines.is_empty() {
                lines.push((Text::literal(""), COLOR_WHITE));
                lines.extend(cost_lines);
            }
        }
    }
}

fn push_cost_line(
    out: &mut Vec<(Text, u32)>,
    required: i32,
    current: i128,
    single_key: &str,
    many_key: &str,
) {
    if required < 1 {
        return;
    }
    let text = if required == 1 {
        Text::translate(single_key, Vec::new())
    } else {
        Text::translate(many_key, vec![Text::literal(required.to_string())])
    };
    let color = if current >= i128::from(required) {
        COLOR_GRAY
    } else {
        COLOR_RED
    };
    out.push((text, color));
}

fn format_enchantment_name(clue: &EnchantClue, max_level: u32) -> Text {
    let mut style = TextStyle::default();
    style.color = Some(if clue.curse { "red" } else { "gray" }.to_owned());
    let base = Text::translate(enchantment_translation_key(&clue.id), Vec::new());
    let full = if clue.level != 1 || max_level != 1 {
        base.append(Text::literal(" ")).append(Text::translate(
            format!("enchantment.level.{}", clue.level),
            Vec::new(),
        ))
    } else {
        base
    };
    full.with_style(style)
}

/// Client `BlockPart::animate_tick` on `minecraft:enchanting_table`: spawns enchanting particles
/// from lenient bookshelves (whose middle block is not a vanilla transmitter) and chiseled
/// bookshelves.
pub fn animate_enchanting_table(host: &mut dyn ClientHost, event: &mut AnimateTickEvent<'_>) {
    let Some(&particle_key) = ENCHANT_PARTICLE.get() else {
        return;
    };
    let cfg = synced_server_config();
    if !cfg.convert_vanilla_enchanting_table_when_interacting
        || is_unaltered_enchanting_table(&*host, event.state.block())
    {
        return;
    }
    let pos = [event.pos.x, event.pos.y, event.pos.z];
    let opts = ParticleOptions::simple(particle_key);

    for offset in BOOKSHELF_OFFSETS {
        if event.random.next_int_bound(16) != 0 {
            continue;
        }
        if !is_valid_particle_bookshelf(&*host, event.level, pos, offset, &cfg) {
            continue;
        }
        // Skip cases vanilla EnchantingTableBlock.animateTick already covers (non-chiseled
        // bookshelf behind a vanilla transmitter) to avoid doubling vanilla particles.
        let mid = [
            pos[0] + offset[0] / 2,
            pos[1] + offset[1],
            pos[2] + offset[2] / 2,
        ];
        let shelf_pos = [pos[0] + offset[0], pos[1] + offset[1], pos[2] + offset[2]];
        let vanilla_transmitter = event
            .level
            .block_state(&*host, mid)
            .and_then(|s| host.block_in_tag(s.block(), ENCHANTMENT_POWER_TRANSMITTER_TAG))
            .unwrap_or(false);
        let is_chiseled = event
            .level
            .block_state(&*host, shelf_pos)
            .and_then(|s| s.block().id(&*host))
            == Some("minecraft:chiseled_bookshelf");
        if vanilla_transmitter && !is_chiseled {
            continue;
        }

        let rx = event.random.next_float() as f64;
        let ry = event.random.next_float() as f64;
        let rz = event.random.next_float() as f64;
        let at = [
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]) + 2.0,
            f64::from(pos[2]) + 0.5,
        ];
        let vel = [
            f64::from(offset[0]) + rx - 0.5,
            f64::from(offset[1]) - ry - 1.0,
            f64::from(offset[2]) + rz - 0.5,
        ];
        let _ = host.add_particle(&opts, at, vel);
    }
}

/// Client tick: plays local reroll UI sound and polls targeted chiseled bookshelf slot from server.
pub fn on_client_tick_post(host: &mut dyn ClientHost) {
    if PENDING_REROLL_SOUND.swap(false, Ordering::Relaxed) {
        let _ = host.play_local_sound(&SoundPlay::ui("minecraft:block.enchantment_table.use"), 0);
    }

    let cfg = effective_client_config(&*host);
    if cfg.chiseled_bookshelf_tooltip == ChiseledBookshelfTooltip::Disabled {
        return;
    }
    if host.observed_screen_id().is_some() {
        return;
    }
    let Some((pos, slot)) = targeted_chiseled_slot(&*host, &cfg) else {
        return;
    };

    let now = host.gui_ticks();
    let should_query = {
        let cache = SHELF_CACHE.lock().unwrap_or_else(PoisonError::into_inner);
        match cache.as_ref() {
            Some(entry) if entry.pos == pos && entry.slot == slot => {
                now.wrapping_sub(entry.queried_tick).abs() >= 10
            }
            _ => true,
        }
    };
    if should_query && let Some(ch) = channels() {
        {
            let mut cache = SHELF_CACHE.lock().unwrap_or_else(PoisonError::into_inner);
            match cache.as_mut() {
                Some(entry) if entry.pos == pos && entry.slot == slot => {
                    entry.queried_tick = now;
                }
                _ => {
                    *cache = Some(ShelfCache {
                        pos,
                        slot,
                        stack: None,
                        queried_tick: now,
                    });
                }
            }
        }
        let _ = ch.shelf_query.send(host, &ShelfQueryPayload { pos, slot });
    }
}

/// Renders the hovered chiseled bookshelf book tooltip on the HUD.
pub fn on_gui_render_post(host: &dyn ClientHost, batch: &mut dyn DrawContext, gui_hidden: bool) {
    if gui_hidden || host.observed_screen_id().is_some() {
        return;
    }
    let cfg = effective_client_config(host);
    if cfg.chiseled_bookshelf_tooltip == ChiseledBookshelfTooltip::Disabled {
        return;
    }
    let Some((pos, slot)) = targeted_chiseled_slot(host, &cfg) else {
        return;
    };
    let stack = {
        let cache = SHELF_CACHE.lock().unwrap_or_else(PoisonError::into_inner);
        cache
            .as_ref()
            .filter(|e| e.pos == pos && e.slot == slot)
            .and_then(|e| e.stack.clone())
    };
    let Some(stack) = stack else {
        return;
    };
    let Some(elements) = ItemResource::of(&stack).tooltip(host, false, false) else {
        return;
    };
    let lines: Vec<Text> = elements
        .into_iter()
        .filter_map(|el| match el {
            TooltipElement::Text(text, _) => Some(text),
            _ => None,
        })
        .collect();
    if lines.is_empty() {
        return;
    }

    let [w, h] = batch.size();
    let total_height = (lines.len() as f32) * 10.0 + if lines.len() > 1 { 2.0 } else { 0.0 };
    let x = w * 0.5 + 10.0 + (cfg.chiseled_bookshelf_tooltip_offset_x as f32);
    let y = h * 0.5 + 12.0 - total_height * 0.5 + (cfg.chiseled_bookshelf_tooltip_offset_y as f32);
    let _ = batch.gui_tooltip(&lines, [x, y]);
}

fn targeted_chiseled_slot(
    host: &dyn ClientHost,
    cfg: &crate::config::ClientConfig,
) -> Option<([i32; 3], u8)> {
    let player = host.local_player()?;
    if player.game_mode(host) == Some(GameMode::Spectator) {
        return None;
    }
    if cfg.chiseled_bookshelf_tooltip == ChiseledBookshelfTooltip::Crouching
        && !player.entity().is_shift_key_down(host).unwrap_or(false)
    {
        return None;
    }
    let Some(CrosshairTarget::Block { pos, state, hit }) = host.crosshair_target() else {
        return None;
    };
    if state.block().id(host) != Some("minecraft:chiseled_bookshelf") {
        return None;
    }
    let serialized = host.block_state_string(state)?;
    let facing = parse_block_state_property(&serialized, "facing")?;
    let block_pos = [pos.x, pos.y, pos.z];
    let slot = chiseled_bookshelf_hit_slot(block_pos, &hit, facing)?;
    let occ_prop = match slot {
        0 => "slot_0_occupied",
        1 => "slot_1_occupied",
        2 => "slot_2_occupied",
        3 => "slot_3_occupied",
        4 => "slot_4_occupied",
        5 => "slot_5_occupied",
        _ => return None,
    };
    if parse_block_state_property(&serialized, occ_prop) != Some("true") {
        return None;
    }
    Some((block_pos, slot))
}
