//! InvMove's pure rules: sneak modes, the movement-key toggle, screen classification and the
//! movement vector. Nothing here touches the game, so every rule is unit-tested.

/// What sneaking does inside a screen (`InvMoveConfig.Movement.SneakMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SneakMode {
    /// No sneaking.
    Off,
    /// Keep sneaking if the player was when the screen opened.
    Maintain,
    /// Sneak only while the key is held.
    Pressed,
    /// Maintain until the key is released.
    MaintainWhilePressed,
}

impl SneakMode {
    /// The option a choice setting's index names; an index out of range reads as `Maintain`.
    pub fn from_choice(index: u32) -> Self {
        match index {
            0 => Self::Off,
            1 => Self::Maintain,
            2 => Self::Pressed,
            3 => Self::MaintainWhilePressed,
            _ => Self::Maintain,
        }
    }
}

/// The sneak bookkeeping carried between ticks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SneakState {
    /// Whether the player sneaked on the last tick with no screen open.
    pub was_sneaking: bool,
    /// The sneak toggle's latch while "toggle sneak" is on.
    pub latched: bool,
    /// Whether the sneak key was physically down on the previous tick.
    pub prev_down: bool,
}

/// `InvMove.shouldSneak`.
pub fn should_sneak(mode: SneakMode, shift_down: bool, was_sneaking: &mut bool) -> bool {
    match mode {
        SneakMode::Off => false,
        SneakMode::MaintainWhilePressed => {
            if !shift_down {
                *was_sneaking = false;
            }
            *was_sneaking
        }
        SneakMode::Maintain => *was_sneaking,
        SneakMode::Pressed => shift_down,
    }
}

/// The three sneak options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SneakConfig {
    /// Mode inside screens that allow movement.
    pub sneak: SneakMode,
    /// Mode while creative flying inside screens that allow movement.
    pub flying: SneakMode,
    /// Mode inside screens that disallow movement.
    pub disallowed: SneakMode,
    /// Whether dismounting by sneaking is allowed.
    pub dismount: bool,
}

/// What the player and the keys are doing this tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SneakInputs {
    /// "Toggle sneak" is on in Controls.
    pub toggle_crouch: bool,
    /// The local player rides something.
    pub passenger: bool,
    /// The local player is creative-flying.
    pub flying: bool,
    /// The sneak key (if InvMove may read it) is physically down.
    pub shift_down: bool,
}

/// The sneak flag inside a screen that allows movement.
pub fn sneak_when_allowed(cfg: SneakConfig, now: SneakInputs, state: &mut SneakState) -> bool {
    let edge = now.shift_down && !state.prev_down;
    state.prev_down = now.shift_down;
    if now.toggle_crouch {
        if edge && cfg.sneak == SneakMode::Pressed {
            state.latched = !state.latched;
        }
        return state.latched;
    }
    if now.passenger {
        return cfg.dismount && now.shift_down;
    }
    let mode = if now.flying { cfg.flying } else { cfg.sneak };
    should_sneak(mode, now.shift_down, &mut state.was_sneaking)
}

/// The sneak flag inside a screen that disallows movement. `None` leaves the game's own flag
/// (vanilla unsneaks) alone: with "toggle sneak" on, or while riding.
pub fn sneak_when_disallowed(
    cfg: SneakConfig,
    now: SneakInputs,
    state: &mut SneakState,
) -> Option<bool> {
    state.prev_down = now.shift_down;
    if now.toggle_crouch || now.passenger {
        return None;
    }
    let mode = if now.flying {
        SneakMode::Off
    } else {
        cfg.disallowed
    };
    Some(should_sneak(mode, now.shift_down, &mut state.was_sneaking))
}

/// `KeyboardInput.tick`'s move vector: `[left, forward]`, normalised when both axes are used.
pub fn move_vector(forward: bool, backward: bool, left: bool, right: bool) -> [f32; 2] {
    let axis = |positive: bool, negative: bool| -> f32 {
        match (positive, negative) {
            (true, false) => 1.0,
            (false, true) => -1.0,
            _ => 0.0,
        }
    };
    let v = [axis(left, right), axis(forward, backward)];
    let length = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if length < 1.0e-4 {
        [0.0, 0.0]
    } else {
        [v[0] / length, v[1] / length]
    }
}

/// `InvMove.handleToggleMovementKey` on a press of the toggle key. Returns the new value of the
/// "Move In Inventories" option and whether the player can move in this screen afterwards.
/// `allowed_if_enabled` answers whether the screen would allow movement with the option on.
pub fn toggle_movement(
    screen_open: bool,
    could_move: bool,
    enabled: bool,
    allowed_if_enabled: impl FnOnce() -> bool,
) -> (bool, bool) {
    if !screen_open {
        return (!enabled, could_move);
    }
    if could_move && enabled {
        return (false, false);
    }
    if !could_move && !enabled {
        return if allowed_if_enabled() {
            (true, true)
        } else {
            (false, false)
        };
    }
    (enabled, could_move)
}

/// The settings key of a vanilla screen InvMove lists by name.
pub type ScreenKey = &'static str;

/// Every vanilla screen the original lists, in its order: setting key, English name.
pub const KNOWN_SCREENS: &[(ScreenKey, &str)] = &[
    ("inventory", "Inventory"),
    ("horse_inventory", "Horse Inventory"),
    ("creative", "Creative Inventory"),
    ("crafting", "Crafting Table"),
    ("chest", "Chest"),
    ("shulker", "Shulker Box"),
    ("dispenser", "Dispenser"),
    ("hopper", "Hopper"),
    ("enchantment", "Enchanting Table"),
    ("anvil", "Anvil"),
    ("beacon", "Beacon"),
    ("brewing", "Brewing Stand"),
    ("furnace", "Furnace"),
    ("blast_furnace", "Blast Furnace"),
    ("smoker", "Smoker"),
    ("loom", "Loom"),
    ("cartography", "Cartography Table"),
    ("grindstone", "Grindstone"),
    ("stonecutter", "Stonecutter"),
    ("smithing", "Smithing Table"),
    ("villager", "Villager"),
    ("book", "Book"),
    ("advancements", "Advancements"),
];

/// How a screen is treated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Class {
    /// A listed vanilla screen with its own two options.
    Known(ScreenKey),
    /// Screens with fixed rules: movement off, background kept (`DeathScreen`, `StatsScreen`,
    /// `WinScreen`, `ChatScreen`, sign, command-block and block editors).
    Fixed,
    /// The book editor: movement off, background by the `book` option.
    BookEdit,
    /// The game mode switcher: movement on, background by the unrecognized default.
    GameMode,
    /// Anything else, listed in the config file under this key once met.
    Unrecognized(String),
}

/// Sorts a screen by its `ScreenView` identity (`id`, the menu type id and the owning mod).
pub fn classify(id: &str, menu: Option<&str>, mod_screen: Option<&str>) -> Class {
    match id {
        "dead" | "stats" | "win" | "chat" | "sign_editor" | "command_block" | "block_editor" => {
            Class::Fixed
        }
        "book_edit" | "book_sign" => Class::BookEdit,
        "book_view" | "lectern" => Class::Known("book"),
        "game_mode_switcher" => Class::GameMode,
        "advancements" => Class::Known("advancements"),
        "creative" => Class::Known("creative"),
        "mod" => Class::Unrecognized(format!("mod:{}", mod_screen.unwrap_or("?unknown"))),
        "inventory" => match menu {
            None => Class::Known("inventory"),
            Some(menu) => classify_menu(menu),
        },
        other => Class::Unrecognized(other.to_owned()),
    }
}

fn classify_menu(menu: &str) -> Class {
    if menu.starts_with("sunrise:mount") {
        return Class::Known("horse_inventory");
    }
    let key = match menu {
        "minecraft:generic_9x1"
        | "minecraft:generic_9x2"
        | "minecraft:generic_9x3"
        | "minecraft:generic_9x4"
        | "minecraft:generic_9x5"
        | "minecraft:generic_9x6" => "chest",
        "minecraft:generic_3x3" => "dispenser",
        "minecraft:crafting" => "crafting",
        "minecraft:shulker_box" => "shulker",
        "minecraft:hopper" => "hopper",
        "minecraft:enchantment" => "enchantment",
        "minecraft:anvil" => "anvil",
        "minecraft:beacon" => "beacon",
        "minecraft:brewing_stand" => "brewing",
        "minecraft:furnace" => "furnace",
        "minecraft:blast_furnace" => "blast_furnace",
        "minecraft:smoker" => "smoker",
        "minecraft:loom" => "loom",
        "minecraft:cartography_table" => "cartography",
        "minecraft:grindstone" => "grindstone",
        "minecraft:stonecutter" => "stonecutter",
        "minecraft:smithing" => "smithing",
        "minecraft:merchant" => "villager",
        other => return Class::Unrecognized(other.to_owned()),
    };
    Class::Known(key)
}

/// The class `screen_transparent_background` names by its own string, when the open screen is not
/// known from the screen hooks.
pub fn classify_background_name(name: &str) -> Class {
    match name {
        "inventory" => Class::Known("inventory"),
        "creative" => Class::Known("creative"),
        "merchant" => Class::Known("villager"),
        "beacon" => Class::Known("beacon"),
        "book" => Class::Known("book"),
        "sign_editor" => Class::Fixed,
        other => Class::Unrecognized(other.to_owned()),
    }
}

/// Screens vanilla calls pause screens (`Screen.isPauseScreen`).
pub fn is_pause_screen(id: &str) -> bool {
    matches!(
        id,
        "paused"
            | "options"
            | "stats"
            | "advancements"
            | "world_options"
            | "social"
            | "friends"
            | "server_links"
            | "debug_options"
    )
}

/// "Pause Screens Background" (`InvMoveConfig.Background.PauseScreenMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseScreenMode {
    /// Always show the background.
    Show,
    /// Show it in single player, otherwise allow hiding.
    ShowSp,
    /// Allow hiding.
    AllowHide,
}

impl PauseScreenMode {
    /// The option a choice setting's index names.
    pub fn from_choice(index: u32) -> Self {
        match index {
            1 => Self::ShowSp,
            2 => Self::AllowHide,
            _ => Self::Show,
        }
    }

    /// Whether a pause screen's background may be hidden at all. `game_paused` is whether the
    /// client is paused, which only an unpublished single-player world does.
    pub fn allows_hiding(self, game_paused: bool) -> bool {
        match self {
            Self::Show => false,
            Self::ShowSp => !game_paused,
            Self::AllowHide => true,
        }
    }
}

/// Whether the key press is one a focused button would take as a click (space, Enter).
pub fn is_selection_key(key: &str) -> bool {
    matches!(
        key,
        "key.keyboard.space" | "key.keyboard.enter" | "key.keyboard.keypad.enter"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const CFG: SneakConfig = SneakConfig {
        sneak: SneakMode::Maintain,
        flying: SneakMode::Pressed,
        disallowed: SneakMode::Maintain,
        dismount: false,
    };

    fn inputs(shift_down: bool) -> SneakInputs {
        SneakInputs {
            toggle_crouch: false,
            passenger: false,
            flying: false,
            shift_down,
        }
    }

    #[test]
    fn sneak_modes() {
        let mut was = true;
        assert!(!should_sneak(SneakMode::Off, true, &mut was));
        assert!(should_sneak(SneakMode::Maintain, false, &mut was));
        assert!(was);
        assert!(should_sneak(SneakMode::Pressed, true, &mut was));
        assert!(!should_sneak(SneakMode::Pressed, false, &mut was));
        assert!(should_sneak(
            SneakMode::MaintainWhilePressed,
            true,
            &mut was
        ));
        assert!(!should_sneak(
            SneakMode::MaintainWhilePressed,
            false,
            &mut was
        ));
        assert!(!was);
        assert!(!should_sneak(SneakMode::Maintain, true, &mut was));
    }

    #[test]
    fn choice_indices_never_panic() {
        assert_eq!(SneakMode::from_choice(0), SneakMode::Off);
        assert_eq!(SneakMode::from_choice(3), SneakMode::MaintainWhilePressed);
        assert_eq!(SneakMode::from_choice(99), SneakMode::Maintain);
        assert_eq!(PauseScreenMode::from_choice(2), PauseScreenMode::AllowHide);
        assert_eq!(PauseScreenMode::from_choice(77), PauseScreenMode::Show);
    }

    #[test]
    fn maintain_keeps_the_sneak_the_screen_opened_with() {
        let mut state = SneakState {
            was_sneaking: true,
            ..SneakState::default()
        };
        assert!(sneak_when_allowed(CFG, inputs(false), &mut state));
        let mut idle = SneakState::default();
        assert!(!sneak_when_allowed(CFG, inputs(true), &mut idle));
    }

    #[test]
    fn flying_uses_its_own_mode() {
        let mut state = SneakState {
            was_sneaking: true,
            ..SneakState::default()
        };
        let mut now = inputs(false);
        now.flying = true;
        assert!(!sneak_when_allowed(CFG, now, &mut state));
        now.shift_down = true;
        assert!(sneak_when_allowed(CFG, now, &mut state));
    }

    #[test]
    fn riding_dismounts_only_when_allowed() {
        let mut state = SneakState::default();
        let mut now = inputs(true);
        now.passenger = true;
        assert!(!sneak_when_allowed(CFG, now, &mut state));
        let allowed = SneakConfig {
            dismount: true,
            ..CFG
        };
        assert!(sneak_when_allowed(allowed, now, &mut state));
        now.shift_down = false;
        assert!(!sneak_when_allowed(allowed, now, &mut state));
    }

    #[test]
    fn toggle_sneak_latches_on_a_press_in_pressed_mode_only() {
        let pressed = SneakConfig {
            sneak: SneakMode::Pressed,
            ..CFG
        };
        let mut state = SneakState::default();
        let mut now = inputs(false);
        now.toggle_crouch = true;
        assert!(!sneak_when_allowed(pressed, now, &mut state));
        now.shift_down = true;
        assert!(sneak_when_allowed(pressed, now, &mut state));
        // Holding the key is not a second press.
        assert!(sneak_when_allowed(pressed, now, &mut state));
        now.shift_down = false;
        assert!(sneak_when_allowed(pressed, now, &mut state));
        now.shift_down = true;
        assert!(!sneak_when_allowed(pressed, now, &mut state));
        // Maintain never toggles.
        let mut kept = SneakState {
            latched: true,
            ..SneakState::default()
        };
        now.shift_down = false;
        sneak_when_allowed(CFG, now, &mut kept);
        now.shift_down = true;
        assert!(sneak_when_allowed(CFG, now, &mut kept));
    }

    #[test]
    fn disallowed_screens_follow_their_own_mode() {
        let mut state = SneakState {
            was_sneaking: true,
            ..SneakState::default()
        };
        assert_eq!(
            sneak_when_disallowed(CFG, inputs(false), &mut state),
            Some(true)
        );
        let mut flying = inputs(true);
        flying.flying = true;
        assert_eq!(sneak_when_disallowed(CFG, flying, &mut state), Some(false));
        let mut riding = inputs(true);
        riding.passenger = true;
        assert_eq!(sneak_when_disallowed(CFG, riding, &mut state), None);
        let mut toggled = inputs(true);
        toggled.toggle_crouch = true;
        assert_eq!(sneak_when_disallowed(CFG, toggled, &mut state), None);
    }

    #[test]
    fn the_move_vector_is_normalised() {
        assert_eq!(move_vector(false, false, false, false), [0.0, 0.0]);
        assert_eq!(move_vector(true, false, false, false), [0.0, 1.0]);
        assert_eq!(move_vector(false, true, false, false), [0.0, -1.0]);
        assert_eq!(move_vector(true, true, false, false), [0.0, 0.0]);
        assert_eq!(move_vector(false, false, true, false), [1.0, 0.0]);
        assert_eq!(move_vector(false, false, false, true), [-1.0, 0.0]);
        let [l, f] = move_vector(true, false, true, false);
        assert!((l - std::f32::consts::FRAC_1_SQRT_2).abs() < 1.0e-6);
        assert!((f - std::f32::consts::FRAC_1_SQRT_2).abs() < 1.0e-6);
    }

    #[test]
    fn the_toggle_key() {
        // No screen: flips the option, movement unchanged.
        assert_eq!(toggle_movement(false, false, true, || true), (false, false));
        assert_eq!(toggle_movement(false, false, false, || true), (true, false));
        // Could move and enabled: turns it off.
        assert_eq!(toggle_movement(true, true, true, || true), (false, false));
        // Could not move, option off: turns on only if the screen then allows movement.
        assert_eq!(toggle_movement(true, false, false, || true), (true, true));
        assert_eq!(
            toggle_movement(true, false, false, || false),
            (false, false)
        );
        // Could not move with the option on (a text field, say): nothing changes.
        assert_eq!(toggle_movement(true, false, true, || true), (true, false));
    }

    #[test]
    fn vanilla_screens_are_recognised() {
        assert_eq!(classify("inventory", None, None), Class::Known("inventory"));
        assert_eq!(
            classify("inventory", Some("minecraft:generic_9x3"), None),
            Class::Known("chest")
        );
        assert_eq!(
            classify("inventory", Some("minecraft:generic_3x3"), None),
            Class::Known("dispenser")
        );
        assert_eq!(
            classify("inventory", Some("sunrise:mount2"), None),
            Class::Known("horse_inventory")
        );
        assert_eq!(
            classify("inventory", Some("minecraft:merchant"), None),
            Class::Known("villager")
        );
        assert_eq!(
            classify("inventory", Some("minecraft:crafter_3x3"), None),
            Class::Unrecognized("minecraft:crafter_3x3".into())
        );
        assert_eq!(classify("chat", None, None), Class::Fixed);
        assert_eq!(classify("book_edit", None, None), Class::BookEdit);
        assert_eq!(classify("lectern", None, None), Class::Known("book"));
        assert_eq!(classify("game_mode_switcher", None, None), Class::GameMode);
        assert_eq!(
            classify("mod", None, Some("zoom")),
            Class::Unrecognized("mod:zoom".into())
        );
        assert_eq!(
            classify("paused", None, None),
            Class::Unrecognized("paused".into())
        );
    }

    #[test]
    fn every_known_screen_is_reachable() {
        for (key, _) in KNOWN_SCREENS {
            let reachable = ["inventory", "creative", "advancements", "book_view"]
                .iter()
                .any(|id| classify(id, None, None) == Class::Known(key))
                || [
                    "sunrise:mount1",
                    "minecraft:generic_9x1",
                    "minecraft:generic_3x3",
                    "minecraft:crafting",
                    "minecraft:shulker_box",
                    "minecraft:hopper",
                    "minecraft:enchantment",
                    "minecraft:anvil",
                    "minecraft:beacon",
                    "minecraft:brewing_stand",
                    "minecraft:furnace",
                    "minecraft:blast_furnace",
                    "minecraft:smoker",
                    "minecraft:loom",
                    "minecraft:cartography_table",
                    "minecraft:grindstone",
                    "minecraft:stonecutter",
                    "minecraft:smithing",
                    "minecraft:merchant",
                ]
                .iter()
                .any(|menu| classify("inventory", Some(menu), None) == Class::Known(key));
            assert!(reachable, "{key} is never produced");
        }
    }

    #[test]
    fn pause_screen_background_modes() {
        assert!(!PauseScreenMode::Show.allows_hiding(true));
        assert!(!PauseScreenMode::Show.allows_hiding(false));
        assert!(!PauseScreenMode::ShowSp.allows_hiding(true));
        assert!(PauseScreenMode::ShowSp.allows_hiding(false));
        assert!(PauseScreenMode::AllowHide.allows_hiding(true));
        assert!(is_pause_screen("paused"));
        assert!(!is_pause_screen("inventory"));
    }

    #[test]
    fn selection_keys() {
        assert!(is_selection_key("key.keyboard.space"));
        assert!(is_selection_key("key.keyboard.enter"));
        assert!(!is_selection_key("key.keyboard.w"));
    }
}
