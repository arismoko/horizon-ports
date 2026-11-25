#![deny(unsafe_code)]

//! Mouse Wheelie for Sunrise: scroll items between slots and inventories with the wheel, move,
//! drop and sort with modifier clicks and the middle button, and refill an emptied hotbar slot.
//!
//! Every action is ordinary container clicks submitted through `MenuRef::click`, so it works on any
//! server. Screen hooks cannot submit clicks (they borrow the host read-only), so each action is
//! queued and sent on the next client tick.

mod logic;

use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use crate::logic::{
    Click, Entry, Mods, Scope, SortMode, View, click_reach, clicks_for, plan_sort, scope_of,
    scroll_clicks, should_refill, supported_menu,
};
use mod_api::{
    ClientHost, ClientModifiers, ClientTickPostEvent, Flow, Hooks, ItemStack, MenuClick,
    MenuSlotKind, MouseButton, Registrar, RegistrationContext, ScreenClosingEvent,
    ScreenMouseButtonPressedPreEvent, ScreenMouseButtonReleasedPreEvent,
    ScreenMouseDraggedPreEvent, ScreenMouseScrolledPreEvent, ScreenView, SettingKey, SettingSpec,
    SwapWith,
};

const MOD_ID: &str = "mousewheelie";
const SPACE: &str = "key.keyboard.space";

static SCROLLING: OnceLock<SettingKey> = OnceLock::new();
static CLICKS: OnceLock<SettingKey> = OnceLock::new();
static SORTING: OnceLock<SettingKey> = OnceLock::new();
static REFILL: OnceLock<SettingKey> = OnceLock::new();

/// A toggle's value; on until the player turns it off.
fn enabled(host: &dyn ClientHost, key: &OnceLock<SettingKey>) -> bool {
    key.get()
        .is_none_or(|key| !matches!(host.setting_bool(*key), Ok(false)))
}

#[derive(Default)]
struct Runtime {
    /// Wheel movement not yet worth a step (a trackpad scrolls in fractions).
    wheel: f64,
    /// Clicks waiting for the next tick, with the window id of the menu they were planned on.
    pending: Vec<(i32, Vec<MenuClick>)>,
    /// A press this mod took: its release must not click.
    swallow_release: bool,
    /// A modified left press this mod took is dragging; the slot last acted on.
    drag: Option<Option<usize>>,
    /// The selected hotbar slot and its stack as of the last playing tick.
    last_selected: Option<(usize, Option<ItemStack>)>,
}

static RUNTIME: Mutex<Runtime> = Mutex::new(Runtime {
    wheel: 0.0,
    pending: Vec::new(),
    swallow_release: false,
    drag: None,
    last_selected: None,
});

fn runtime() -> MutexGuard<'static, Runtime> {
    RUNTIME.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The open menu as the rules read it.
struct Ctx {
    window: i32,
    kinds: Vec<MenuSlotKind>,
    entries: Vec<Option<Entry>>,
    boxes: Vec<Option<[f32; 4]>>,
    carried_empty: bool,
    player_screen: bool,
}

impl Ctx {
    /// `None` unless the screen is the inventory or a supported container with all slots readable.
    fn read(host: &dyn ClientHost, screen: &ScreenView<'_>) -> Option<Self> {
        if screen.id != "inventory" || screen.mod_screen.is_some() || !supported_menu(screen.menu) {
            return None;
        }
        let menu = host.local_player()?.container_menu(host)?;
        let window = menu.container_id(host)?;
        let kinds = menu.slot_kinds(host)?;
        let boxes = menu.slot_boxes(host)?;
        let count = menu.slot_count(host)?;
        if kinds.len() != count || boxes.len() != count {
            return None;
        }
        let mut stacks = Vec::with_capacity(count);
        for slot in 0..count {
            stacks.push(menu.slot(host, slot)?);
        }
        let mut reps: Vec<&ItemStack> = Vec::new();
        let mut entries = Vec::with_capacity(count);
        for stack in &stacks {
            entries.push(stack.as_ref().map(|stack| {
                let kind = reps
                    .iter()
                    .position(|r| r.is_stackable_with(stack))
                    .unwrap_or_else(|| {
                        reps.push(stack);
                        reps.len() - 1
                    });
                Entry {
                    kind,
                    count: stack.count(),
                    max: host.max_stack_size(stack.item()).unwrap_or(64),
                    name: stack.item().id(host).unwrap_or("").to_owned(),
                }
            }));
        }
        let player_screen = !kinds
            .iter()
            .any(|k| matches!(k, MenuSlotKind::Storage | MenuSlotKind::Input));
        Some(Self {
            window,
            kinds,
            entries,
            boxes,
            carried_empty: matches!(menu.carried(host), Some(None)),
            player_screen,
        })
    }

    fn scopes(&self, split_hotbar: bool) -> Vec<Option<Scope>> {
        self.kinds
            .iter()
            .map(|k| scope_of(*k, self.player_screen, split_hotbar))
            .collect()
    }

    /// The slot under the pointer (slot boxes are absolute GUI pixels).
    fn hovered(&self, mouse: [f32; 2]) -> Option<usize> {
        self.boxes.iter().position(|b| {
            b.is_some_and(|[x, y, w, h]| {
                mouse[0] >= x && mouse[0] < x + w && mouse[1] >= y && mouse[1] < y + h
            })
        })
    }
}

fn to_menu_clicks(clicks: &[Click]) -> Vec<MenuClick> {
    clicks
        .iter()
        .map(|click| match *click {
            Click::Pick { slot, right } => MenuClick::Pickup {
                slot: Some(slot),
                button: if right {
                    MouseButton::Right
                } else {
                    MouseButton::Left
                },
            },
            Click::Quick(slot) => MenuClick::QuickMove { slot },
            Click::Throw { slot, whole } => MenuClick::Throw {
                slot,
                whole_stack: whole,
            },
        })
        .collect()
}

fn mods(host: &dyn ClientHost, bits: u32) -> Mods {
    let m = ClientModifiers::new(bits);
    Mods {
        shift: m.shift(),
        ctrl: m.control(),
        alt: m.alt(),
        space: host.is_key_down(SPACE),
    }
}

fn queue(ctx: &Ctx, clicks: &[Click]) {
    if !clicks.is_empty() {
        runtime().pending.push((ctx.window, to_menu_clicks(clicks)));
    }
}

/// The clicks a modified press (or each slot crossed by a drag) makes; `None` for a plain one.
fn modified_clicks(ctx: &Ctx, slot: usize, mods: Mods) -> Option<Vec<Click>> {
    let (drop, reach) = click_reach(mods)?;
    let scopes = ctx.scopes(false);
    let view = View {
        entries: &ctx.entries,
        scopes: &scopes,
        player_screen: ctx.player_screen,
    };
    Some(clicks_for(&view, slot, reach, drop))
}

#[derive(Default)]
struct Mod;

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        for (key, name, cell) in [
            ("scrolling", "setting.mousewheelie.scrolling", &SCROLLING),
            (
                "click_shortcuts",
                "setting.mousewheelie.click_shortcuts",
                &CLICKS,
            ),
            ("sorting", "setting.mousewheelie.sorting", &SORTING),
            ("refill", "setting.mousewheelie.refill", &REFILL),
        ] {
            if let Ok(setting) = r.register_setting(
                SettingSpec::toggle(key, name, true).tooltip(format!("{name}.tooltip")),
            ) {
                let _ = cell.set(setting);
            }
        }
    }

    /// The wheel over a slot moves its items: one, the stack (Shift), every stack of the kind
    /// (Ctrl), or the matching kinds (Space).
    fn screen_mouse_scrolled_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseScrolledPreEvent<'_>,
    ) -> Flow {
        if !host.on(MOD_ID) || !enabled(host, &SCROLLING) {
            return Flow::Continue;
        }
        let Some(ctx) = Ctx::read(host, event.screen) else {
            return Flow::Continue;
        };
        let Some(slot) = ctx.hovered(event.screen.mouse) else {
            return Flow::Continue;
        };
        let scopes = ctx.scopes(false);
        let view = View {
            entries: &ctx.entries,
            scopes: &scopes,
            player_screen: ctx.player_screen,
        };
        let up = event.scroll[1] > 0.0;
        let clicks = scroll_clicks(&view, slot, mods(host, event.modifiers), up);
        if clicks.is_empty() {
            return Flow::Continue;
        }
        {
            let mut rt = runtime();
            rt.wheel += event.scroll[1];
            if rt.wheel.abs() < 1.0 {
                return Flow::Cancel;
            }
            rt.wheel = 0.0;
        }
        queue(&ctx, &clicks);
        Flow::Cancel
    }

    /// Modified left presses move or drop; the middle button sorts the part under the pointer.
    fn screen_mouse_button_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseButtonPressedPreEvent<'_>,
    ) -> Flow {
        if !host.on(MOD_ID) || !(event.button == 0 || event.button == 2) {
            return Flow::Continue;
        }
        let Some(ctx) = Ctx::read(host, event.screen) else {
            return Flow::Continue;
        };
        let Some(slot) = ctx.hovered(event.screen.mouse) else {
            return Flow::Continue;
        };
        let m = mods(host, event.modifiers);
        if event.button == 2 {
            if !enabled(host, &SORTING) || !ctx.carried_empty {
                return Flow::Continue;
            }
            let scopes = ctx.scopes(true);
            let Some(scope) = scopes[slot] else {
                return Flow::Continue;
            };
            let slots: Vec<usize> = (0..scopes.len())
                .filter(|&i| scopes[i] == Some(scope))
                .collect();
            let entries: Vec<Option<Entry>> =
                slots.iter().map(|&i| ctx.entries[i].clone()).collect();
            let mode = if m.shift {
                SortMode::Quantity
            } else if m.ctrl {
                SortMode::Name
            } else {
                SortMode::Creative
            };
            let clicks: Vec<Click> = plan_sort(&entries, mode)
                .into_iter()
                .map(|p| Click::Pick {
                    slot: slots[p],
                    right: false,
                })
                .collect();
            queue(&ctx, &clicks);
            runtime().swallow_release = true;
            return Flow::Cancel;
        }
        if !enabled(host, &CLICKS) {
            return Flow::Continue;
        }
        let Some(clicks) = modified_clicks(&ctx, slot, m) else {
            return Flow::Continue;
        };
        queue(&ctx, &clicks);
        let mut rt = runtime();
        rt.swallow_release = true;
        rt.drag = Some(Some(slot));
        Flow::Cancel
    }

    /// Dragging with the modifiers held does the same on each slot crossed.
    fn screen_mouse_dragged_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseDraggedPreEvent<'_>,
    ) -> Flow {
        let last = {
            let rt = runtime();
            match rt.drag {
                Some(last) if event.button == 0 => last,
                _ => return Flow::Continue,
            }
        };
        let Some(ctx) = Ctx::read(host, event.screen) else {
            return Flow::Cancel;
        };
        let Some(slot) = ctx.hovered(event.screen.mouse) else {
            return Flow::Cancel;
        };
        if last == Some(slot) {
            return Flow::Cancel;
        }
        if let Some(clicks) = modified_clicks(&ctx, slot, mods(host, event.modifiers)) {
            queue(&ctx, &clicks);
        }
        runtime().drag = Some(Some(slot));
        Flow::Cancel
    }

    /// The release of a press this mod took must not click.
    fn screen_mouse_button_released_pre(
        &self,
        _host: &dyn ClientHost,
        _event: &mut ScreenMouseButtonReleasedPreEvent<'_>,
    ) -> Flow {
        let mut rt = runtime();
        rt.drag = None;
        if std::mem::take(&mut rt.swallow_release) {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    fn screen_closing(&self, _host: &dyn ClientHost, _event: &mut ScreenClosingEvent<'_>) {
        let mut rt = runtime();
        rt.pending.clear();
        rt.swallow_release = false;
        rt.drag = None;
        rt.wheel = 0.0;
    }

    /// Sends the queued clicks, then refills an emptied selected hotbar slot from the main inventory.
    fn client_tick_post(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPostEvent) {
        let pending = std::mem::take(&mut runtime().pending);
        let Some(player) = host.local_player() else {
            runtime().last_selected = None;
            return;
        };
        let Some(menu) = player.container_menu(&*host) else {
            runtime().last_selected = None;
            return;
        };
        for (window, clicks) in pending {
            if menu.container_id(&*host) == Some(window) {
                menu.click(host, &clicks);
            }
        }

        let playing = host.playing();
        if !playing || !enabled(&*host, &REFILL) {
            runtime().last_selected = None;
            return;
        }
        let Some(selected) = player.selected_slot(&*host) else {
            return;
        };
        let (Some(kinds), Some(count)) = (menu.slot_kinds(&*host), menu.slot_count(&*host)) else {
            return;
        };
        let Some(own) = kinds
            .iter()
            .position(|k| *k == MenuSlotKind::Player(selected as u32))
        else {
            return;
        };
        let Some(now) = menu.slot(&*host, own) else {
            return;
        };
        let drop_down = host
            .vanilla_key_mapping("key.drop")
            .ok()
            .and_then(|m| m.physical_down(&*host).ok())
            .unwrap_or(false);
        let previous = runtime().last_selected.replace((selected, now.clone()));
        let Some((prev_selected, prev_stack)) = previous else {
            return;
        };
        if !should_refill(
            prev_stack.is_some(),
            prev_selected == selected,
            now.is_none(),
            drop_down,
            playing,
        ) {
            return;
        }
        let Some(wanted) = prev_stack else {
            return;
        };
        let source = (0..count).find(|&slot| {
            matches!(kinds[slot], MenuSlotKind::Player(9..=35))
                && matches!(menu.slot(&*host, slot), Some(Some(s)) if s.same_item(&wanted))
        });
        if let Some(slot) = source {
            menu.click(
                host,
                &[MenuClick::Swap {
                    slot,
                    with: SwapWith::Hotbar(selected as u8),
                }],
            );
        }
    }
}
