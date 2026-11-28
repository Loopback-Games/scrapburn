//! Control bindings: the driving actions, the keys and gamepad buttons bound to each, the
//! rebinding (capture) rules, and the stable names the settings file uses.
//!
//! Menu navigation, pause (Esc / Start), and the FPS display (F3 / Select) are fixed and
//! are not actions, so a player can never lock themselves out of the menus.
//!
//! Settings file lines, one per action: `controls.bind.<action> key:<Key>... pad:<Button>...`
//! with one to three keys and one or two buttons, e.g.
//! `controls.bind.use_item key:KeyE key:ControlLeft key:KeyJ pad:West`. A line with an unknown name, a
//! reserved key, or no key or button for a device leaves that action on its default.

use rivet_platform::{KeyInput, PadButtons, PadState};
use winit::keyboard::KeyCode;

/// A driving action the player can rebind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Accelerate,
    Brake,
    SteerLeft,
    SteerRight,
    DriftHop,
    UseItem,
    Respawn,
    LookBehind,
}

impl Action {
    pub const COUNT: usize = 8;
    pub const ALL: [Action; Action::COUNT] = [
        Action::Accelerate,
        Action::Brake,
        Action::SteerLeft,
        Action::SteerRight,
        Action::DriftHop,
        Action::UseItem,
        Action::Respawn,
        Action::LookBehind,
    ];

    fn index(self) -> usize {
        self as usize
    }

    /// The settings file name, as in `controls.bind.use_item`.
    pub fn key(self) -> &'static str {
        match self {
            Action::Accelerate => "accelerate",
            Action::Brake => "brake",
            Action::SteerLeft => "steer_left",
            Action::SteerRight => "steer_right",
            Action::DriftHop => "drift_hop",
            Action::UseItem => "use_item",
            Action::Respawn => "respawn",
            Action::LookBehind => "look_behind",
        }
    }

    fn from_key(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.key() == name)
    }

    /// Gamepad steering is always the left stick, so these have no gamepad binding to change.
    pub fn stick_only(self) -> bool {
        matches!(self, Action::SteerLeft | Action::SteerRight)
    }
}

/// What a gamepad binding reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadBind {
    /// One button.
    Button(PadButtons),
    LeftTrigger,
    RightTrigger,
    /// The left stick, for steering only.
    Stick,
}

/// One key or button press offered to a capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    Key(KeyCode),
    /// A single `PadButtons` bit; the triggers arrive as `LEFT_TRIGGER` / `RIGHT_TRIGGER`.
    Pad(PadButtons),
}

/// Bindable keys: code, settings file name, and display label. Esc, F3 (the FPS display)
/// and anything not listed can't be bound. Right Shift counts as Shift, as it always has for
/// drifting; Ctrl and Alt are the left-hand keys only.
pub const KEY_NAMES: &[(KeyCode, &str, &str)] = {
    use KeyCode::*;
    &[
        (KeyA, "KeyA", "A"),
        (KeyB, "KeyB", "B"),
        (KeyC, "KeyC", "C"),
        (KeyD, "KeyD", "D"),
        (KeyE, "KeyE", "E"),
        (KeyF, "KeyF", "F"),
        (KeyG, "KeyG", "G"),
        (KeyH, "KeyH", "H"),
        (KeyI, "KeyI", "I"),
        (KeyJ, "KeyJ", "J"),
        (KeyK, "KeyK", "K"),
        (KeyL, "KeyL", "L"),
        (KeyM, "KeyM", "M"),
        (KeyN, "KeyN", "N"),
        (KeyO, "KeyO", "O"),
        (KeyP, "KeyP", "P"),
        (KeyQ, "KeyQ", "Q"),
        (KeyR, "KeyR", "R"),
        (KeyS, "KeyS", "S"),
        (KeyT, "KeyT", "T"),
        (KeyU, "KeyU", "U"),
        (KeyV, "KeyV", "V"),
        (KeyW, "KeyW", "W"),
        (KeyX, "KeyX", "X"),
        (KeyY, "KeyY", "Y"),
        (KeyZ, "KeyZ", "Z"),
        (Digit0, "Digit0", "0"),
        (Digit1, "Digit1", "1"),
        (Digit2, "Digit2", "2"),
        (Digit3, "Digit3", "3"),
        (Digit4, "Digit4", "4"),
        (Digit5, "Digit5", "5"),
        (Digit6, "Digit6", "6"),
        (Digit7, "Digit7", "7"),
        (Digit8, "Digit8", "8"),
        (Digit9, "Digit9", "9"),
        (ArrowUp, "ArrowUp", "Up"),
        (ArrowDown, "ArrowDown", "Down"),
        (ArrowLeft, "ArrowLeft", "Left"),
        (ArrowRight, "ArrowRight", "Right"),
        (Space, "Space", "Space"),
        (ShiftLeft, "ShiftLeft", "Shift"),
        (ControlLeft, "ControlLeft", "Ctrl"),
        (AltLeft, "AltLeft", "Alt"),
        (Tab, "Tab", "Tab"),
        (Enter, "Enter", "Enter"),
        (F1, "F1", "F1"),
        (F2, "F2", "F2"),
        (F3, "F3", "F3"),
        (F4, "F4", "F4"),
        (F5, "F5", "F5"),
        (F6, "F6", "F6"),
        (F7, "F7", "F7"),
        (F8, "F8", "F8"),
        (F9, "F9", "F9"),
        (F10, "F10", "F10"),
        (F11, "F11", "F11"),
        (F12, "F12", "F12"),
    ]
};

/// Keys that are named (so a file mentioning them parses) but that a capture refuses because
/// a fixed control uses them.
const RESERVED_KEYS: [KeyCode; 1] = [KeyCode::F3];

/// Gamepad bindings: value, settings file name, and display label.
pub const PAD_NAMES: &[(PadBind, &str, &str)] = &[
    (PadBind::Button(PadButtons::SOUTH), "South", "A"),
    (PadBind::Button(PadButtons::EAST), "East", "B"),
    (PadBind::Button(PadButtons::WEST), "West", "X"),
    (PadBind::Button(PadButtons::NORTH), "North", "Y"),
    (PadBind::Button(PadButtons::LEFT_BUMPER), "LeftBumper", "LB"),
    (
        PadBind::Button(PadButtons::RIGHT_BUMPER),
        "RightBumper",
        "RB",
    ),
    (PadBind::Button(PadButtons::DPAD_UP), "DPadUp", "D-pad up"),
    (
        PadBind::Button(PadButtons::DPAD_DOWN),
        "DPadDown",
        "D-pad down",
    ),
    (
        PadBind::Button(PadButtons::DPAD_LEFT),
        "DPadLeft",
        "D-pad left",
    ),
    (
        PadBind::Button(PadButtons::DPAD_RIGHT),
        "DPadRight",
        "D-pad right",
    ),
    (PadBind::LeftTrigger, "LeftTrigger", "LT"),
    (PadBind::RightTrigger, "RightTrigger", "RT"),
    (PadBind::Stick, "Stick", "Left stick"),
];

/// Right Shift stands for Shift.
fn canonical(code: KeyCode) -> KeyCode {
    match code {
        KeyCode::ShiftRight => KeyCode::ShiftLeft,
        c => c,
    }
}

/// The other physical key a binding of `code` also answers to: either Shift is Shift.
fn twin(code: KeyCode) -> Option<KeyCode> {
    (code == KeyCode::ShiftLeft).then_some(KeyCode::ShiftRight)
}

pub fn key_name(code: KeyCode) -> Option<&'static str> {
    KEY_NAMES.iter().find(|e| e.0 == code).map(|e| e.1)
}

pub fn key_from_name(name: &str) -> Option<KeyCode> {
    KEY_NAMES.iter().find(|e| e.1 == name).map(|e| e.0)
}

/// The short label shown in menus and hints, e.g. "Ctrl".
pub fn key_label(code: KeyCode) -> &'static str {
    KEY_NAMES.iter().find(|e| e.0 == code).map_or("?", |e| e.2)
}

pub fn pad_name(bind: PadBind) -> &'static str {
    PAD_NAMES.iter().find(|e| e.0 == bind).map_or("?", |e| e.1)
}

pub fn pad_from_name(name: &str) -> Option<PadBind> {
    PAD_NAMES.iter().find(|e| e.1 == name).map(|e| e.0)
}

pub fn pad_label(bind: PadBind) -> &'static str {
    PAD_NAMES.iter().find(|e| e.0 == bind).map_or("?", |e| e.2)
}

/// The binding a pad press captures, if it can be bound. Start (pause) and Select (FPS
/// display) can't.
fn pad_bind_of(bit: PadButtons) -> Option<PadBind> {
    let bind = if bit == PadButtons::LEFT_TRIGGER {
        PadBind::LeftTrigger
    } else if bit == PadButtons::RIGHT_TRIGGER {
        PadBind::RightTrigger
    } else {
        PadBind::Button(bit)
    };
    (bind != PadBind::Stick && PAD_NAMES.iter().any(|e| e.0 == bind)).then_some(bind)
}

/// Point `slots[a]`'s primary at `new`. Wherever else `new` was bound, the action's old
/// primary takes its place, so two actions swap rather than one losing its binding.
fn assign<T: Copy + PartialEq, const N: usize>(
    slots: &mut [[Option<T>; N]; Action::COUNT],
    a: usize,
    new: T,
) {
    let old = slots[a][0];
    if old == Some(new) {
        return;
    }
    for (i, pair) in slots.iter_mut().enumerate() {
        for (j, slot) in pair.iter_mut().enumerate() {
            if *slot == Some(new) && (i, j) != (a, 0) {
                *slot = old;
            }
        }
    }
    slots[a][0] = Some(new);
    for slot in slots.iter_mut() {
        *slot = packed(slot.iter().flatten().copied());
    }
}

/// `items` without repeats, filled from the first slot; extras past `N` are dropped.
fn packed<T: Copy + PartialEq, const N: usize>(items: impl Iterator<Item = T>) -> [Option<T>; N] {
    let mut out = [None; N];
    let mut n = 0;
    for item in items {
        if n < N && !out.contains(&Some(item)) {
            out[n] = Some(item);
            n += 1;
        }
    }
    out
}

/// Keys per action.
pub const KEY_SLOTS: usize = 3;
/// Gamepad bindings per action.
pub const PAD_SLOTS: usize = 2;

/// The player's control bindings: up to three keys and two gamepad bindings per action.
/// The first of each is the primary, which a capture replaces; it is never empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bindings {
    pub keys: [[Option<KeyCode>; KEY_SLOTS]; Action::COUNT],
    pub pad: [[Option<PadBind>; PAD_SLOTS]; Action::COUNT],
}

impl Default for Bindings {
    /// The game's original fixed controls, plus look behind on Q / Y.
    fn default() -> Self {
        use KeyCode::*;
        let b = PadBind::Button;
        Self {
            keys: [
                [Some(KeyW), Some(ArrowUp), None],
                [Some(KeyS), Some(ArrowDown), None],
                [Some(KeyA), Some(ArrowLeft), None],
                [Some(KeyD), Some(ArrowRight), None],
                [Some(Space), Some(ShiftLeft), None],
                [Some(KeyE), Some(ControlLeft), Some(KeyJ)],
                [Some(KeyR), None, None],
                [Some(KeyQ), None, None],
            ],
            pad: [
                [Some(PadBind::RightTrigger), Some(b(PadButtons::SOUTH))],
                [Some(PadBind::LeftTrigger), None],
                [Some(PadBind::Stick), None],
                [Some(PadBind::Stick), None],
                [
                    Some(b(PadButtons::LEFT_BUMPER)),
                    Some(b(PadButtons::RIGHT_BUMPER)),
                ],
                [Some(b(PadButtons::WEST)), None],
                [Some(b(PadButtons::EAST)), None],
                [Some(b(PadButtons::NORTH)), None],
            ],
        }
    }
}

/// A row of the Controls tab: one action, or the fixed pause control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindRow {
    Action(Action),
    /// Fixed: pause is never rebindable, so the player can't lock themselves out.
    Pause,
}

impl BindRow {
    pub const ALL: [BindRow; Action::COUNT + 1] = [
        BindRow::Action(Action::Accelerate),
        BindRow::Action(Action::Brake),
        BindRow::Action(Action::SteerLeft),
        BindRow::Action(Action::SteerRight),
        BindRow::Action(Action::DriftHop),
        BindRow::Action(Action::UseItem),
        BindRow::Action(Action::Respawn),
        BindRow::Action(Action::LookBehind),
        BindRow::Pause,
    ];
}

impl Bindings {
    pub fn keys_for(&self, a: Action) -> [Option<KeyCode>; KEY_SLOTS] {
        self.keys[a.index()]
    }

    pub fn pad_for(&self, a: Action) -> [Option<PadBind>; PAD_SLOTS] {
        self.pad[a.index()]
    }

    fn keys_of(&self, a: Action) -> impl Iterator<Item = KeyCode> {
        self.keys[a.index()].into_iter().flatten()
    }

    fn pads_of(&self, a: Action) -> impl Iterator<Item = PadBind> {
        self.pad[a.index()].into_iter().flatten()
    }

    /// A key bound to `a` is held.
    pub fn key_held(&self, a: Action, keys: &KeyInput) -> bool {
        self.keys_of(a)
            .any(|c| keys.held(c) || twin(c).is_some_and(|t| keys.held(t)))
    }

    /// How far a pad control bound to `a` is pressed, 0 to 1.
    fn pad_value(&self, a: Action, p: &PadState) -> f32 {
        self.pads_of(a)
            .map(|b| match b {
                PadBind::Button(bit) => f32::from(u8::from(p.buttons.contains(bit))),
                PadBind::LeftTrigger => p.left_trigger,
                PadBind::RightTrigger => p.right_trigger,
                PadBind::Stick => match a {
                    Action::SteerLeft => (-p.left_x).max(0.0),
                    Action::SteerRight => p.left_x.max(0.0),
                    _ => 0.0,
                },
            })
            .fold(0.0, f32::max)
    }

    /// How far `a` is pressed, 0 to 1: keys and buttons are 0 or 1, triggers are analog.
    pub fn value(&self, a: Action, keys: &KeyInput, pad: Option<&PadState>) -> f32 {
        let key = if self.key_held(a, keys) { 1.0 } else { 0.0 };
        pad.map_or(key, |p| self.pad_value(a, p).max(key))
    }

    /// `a` is held down. A trigger counts past half travel, the stick past half deflection.
    pub fn held(&self, a: Action, keys: &KeyInput, pad: Option<&PadState>) -> bool {
        self.key_held(a, keys)
            || pad.is_some_and(|p| {
                self.pads_of(a).any(|b| match b {
                    PadBind::Button(bit) => p.buttons.contains(bit),
                    PadBind::LeftTrigger => p.buttons.contains(PadButtons::LEFT_TRIGGER),
                    PadBind::RightTrigger => p.buttons.contains(PadButtons::RIGHT_TRIGGER),
                    PadBind::Stick => self.pad_value(a, p) > 0.5,
                })
            })
    }

    /// `a` was pressed this frame.
    pub fn pressed(&self, a: Action, keys: &KeyInput, pad: Option<&PadState>) -> bool {
        self.keys_of(a)
            .any(|c| keys.just_pressed(c) || twin(c).is_some_and(|t| keys.just_pressed(t)))
            || pad.is_some_and(|p| {
                self.pads_of(a).any(|b| match b {
                    PadBind::Button(bit) => p.just_pressed.contains(bit),
                    PadBind::LeftTrigger => p.just_pressed.contains(PadButtons::LEFT_TRIGGER),
                    PadBind::RightTrigger => p.just_pressed.contains(PadButtons::RIGHT_TRIGGER),
                    PadBind::Stick => false,
                })
            })
    }

    /// Bind `press` as `a`'s primary key or button. A key or button another action uses is
    /// swapped with this action's old one. Returns false, changing nothing, for a press that
    /// can't be bound: Esc, F3, Start, Select, keys outside `KEY_NAMES`, or a gamepad press
    /// for steering (always the left stick).
    pub fn capture(&mut self, a: Action, press: Press) -> bool {
        match press {
            Press::Key(code) => {
                let code = canonical(code);
                if key_name(code).is_none() || RESERVED_KEYS.contains(&code) {
                    return false;
                }
                assign(&mut self.keys, a.index(), code);
            }
            Press::Pad(bit) => {
                let Some(bind) = pad_bind_of(bit).filter(|_| !a.stick_only()) else {
                    return false;
                };
                assign(&mut self.pad, a.index(), bind);
            }
        }
        true
    }

    /// The settings file value for `a`, e.g. `key:KeyE key:ControlLeft pad:West`.
    pub fn line(&self, a: Action) -> String {
        let keys = self
            .keys_of(a)
            .filter_map(key_name)
            .map(|n| format!("key:{n}"));
        let pads = self.pads_of(a).map(|b| format!("pad:{}", pad_name(b)));
        keys.chain(pads).collect::<Vec<_>>().join(" ")
    }

    /// Apply a settings file line for the action named `action`. Anything wrong with it
    /// (an unknown name, a reserved key, no key or no button) leaves the action as it is.
    pub fn set_line(&mut self, action: &str, value: &str) {
        let Some(a) = Action::from_key(action) else {
            return;
        };
        let mut keys = Vec::new();
        let mut pads = Vec::new();
        for token in value.split_whitespace() {
            if let Some(name) = token.strip_prefix("key:")
                && let Some(code) = key_from_name(name)
                && !RESERVED_KEYS.contains(&code)
            {
                keys.push(code);
            } else if let Some(name) = token.strip_prefix("pad:")
                && let Some(bind) = pad_from_name(name)
                && (bind == PadBind::Stick) == a.stick_only()
            {
                pads.push(bind);
            } else {
                return;
            }
        }
        if keys.is_empty() || keys.len() > KEY_SLOTS || pads.is_empty() || pads.len() > PAD_SLOTS {
            return;
        }
        self.keys[a.index()] = packed(keys.into_iter());
        self.pad[a.index()] = packed(pads.into_iter());
    }

    /// Display text for a Controls row: keys joined by " / ", buttons by " or ".
    pub fn text(&self, row: BindRow, pad: bool) -> String {
        let a = match row {
            BindRow::Action(a) => a,
            BindRow::Pause => return if pad { "Menu" } else { "Esc" }.to_string(),
        };
        if pad {
            self.pads_of(a)
                .map(pad_label)
                .collect::<Vec<_>>()
                .join(" or ")
        } else {
            self.keys_of(a)
                .map(key_label)
                .collect::<Vec<_>>()
                .join(" / ")
        }
    }

    /// The short in-race hint for `a`: primary key / primary button, e.g. "E / X".
    pub fn hint(&self, a: Action) -> String {
        let key = self.keys_of(a).next().map_or("?", key_label);
        let pad = self.pads_of(a).next().map_or("?", pad_label);
        format!("{key} / {pad}")
    }

    /// Both steering directions on one line, e.g. "A D / Left Right".
    pub fn steer_keys_text(&self) -> String {
        let (l, r) = (
            self.keys_for(Action::SteerLeft),
            self.keys_for(Action::SteerRight),
        );
        (0..KEY_SLOTS)
            .filter_map(|i| match (l[i], r[i]) {
                (Some(a), Some(b)) => Some(format!("{} {}", key_label(a), key_label(b))),
                (Some(a), None) | (None, Some(a)) => Some(key_label(a).to_string()),
                (None, None) => None,
            })
            .collect::<Vec<_>>()
            .join(" / ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rivet_platform::{KeyInput, PadButtons, PadState};
    use winit::keyboard::KeyCode;

    fn b() -> Bindings {
        Bindings::default()
    }

    #[test]
    fn defaults_equal_the_old_fixed_controls() {
        let d = b();
        assert_eq!(
            d.keys_for(Action::Accelerate),
            [Some(KeyCode::KeyW), Some(KeyCode::ArrowUp), None]
        );
        assert_eq!(
            d.keys_for(Action::UseItem),
            [
                Some(KeyCode::KeyE),
                Some(KeyCode::ControlLeft),
                Some(KeyCode::KeyJ)
            ]
        );
        assert_eq!(
            d.pad_for(Action::Accelerate),
            [
                Some(PadBind::RightTrigger),
                Some(PadBind::Button(PadButtons::SOUTH))
            ]
        );
        assert_eq!(
            d.keys_for(Action::LookBehind),
            [Some(KeyCode::KeyQ), None, None]
        );
        assert_eq!(
            d.pad_for(Action::LookBehind),
            [Some(PadBind::Button(PadButtons::NORTH)), None]
        );
        assert_eq!(d.pad_for(Action::SteerLeft), [Some(PadBind::Stick), None]);
    }

    #[test]
    fn key_names_round_trip() {
        assert!(KEY_NAMES.len() >= 26 + 10 + 4 + 5 + 12);
        for &(code, name, _) in KEY_NAMES {
            assert_eq!(key_from_name(name), Some(code), "{name}");
            assert_eq!(key_name(code), Some(name), "{name}");
        }
        assert_eq!(key_from_name("Escape"), None);
        assert_eq!(key_name(KeyCode::Escape), None);
        for &(bind, name, _) in PAD_NAMES {
            assert_eq!(pad_from_name(name), Some(bind), "{name}");
        }
    }

    #[test]
    fn unknown_names_keep_the_default() {
        let mut x = b();
        x.set_line("use_item", "key:KeyZ key:Bogus pad:North");
        assert_eq!(x, b());
        x.set_line("use_item", "pad:Stick");
        assert_eq!(x, b(), "only steering uses the stick");
        x.set_line("use_item", "key:Escape");
        assert_eq!(x, b(), "Esc can't be bound");
        x.set_line("use_item", "");
        assert_eq!(x, b(), "never unbound");
        x.set_line("no_such_action", "key:KeyZ");
        assert_eq!(x, b());
    }

    #[test]
    fn lines_round_trip() {
        let mut x = b();
        assert!(x.capture(Action::UseItem, Press::Key(KeyCode::KeyZ)));
        assert!(x.capture(Action::Respawn, Press::Pad(PadButtons::LEFT_TRIGGER)));
        let mut y = b();
        for a in Action::ALL {
            y.set_line(a.key(), &x.line(a));
        }
        assert_eq!(y, x);
        assert_eq!(
            b().line(Action::UseItem),
            "key:KeyE key:ControlLeft key:KeyJ pad:West"
        );
        // A two-key line from before the third slot still parses.
        let mut z = b();
        z.set_line("use_item", "key:KeyE key:ControlLeft pad:West");
        assert_eq!(
            z.keys_for(Action::UseItem),
            [Some(KeyCode::KeyE), Some(KeyCode::ControlLeft), None]
        );
        // Four keys is too many.
        let mut z = b();
        z.set_line("use_item", "key:KeyE key:KeyF key:KeyG key:KeyH pad:West");
        assert_eq!(z, b());
    }

    #[test]
    fn capture_replaces_the_primary_slot() {
        let mut x = b();
        assert!(x.capture(Action::UseItem, Press::Key(KeyCode::KeyZ)));
        assert_eq!(
            x.keys_for(Action::UseItem),
            [
                Some(KeyCode::KeyZ),
                Some(KeyCode::ControlLeft),
                Some(KeyCode::KeyJ)
            ]
        );
        assert!(x.capture(Action::UseItem, Press::Pad(PadButtons::DPAD_UP)));
        assert_eq!(
            x.pad_for(Action::UseItem)[0],
            Some(PadBind::Button(PadButtons::DPAD_UP))
        );
    }

    #[test]
    fn conflict_swaps_the_two_actions() {
        let mut x = b();
        // W accelerates; give it to use item and accelerate takes E.
        assert!(x.capture(Action::UseItem, Press::Key(KeyCode::KeyW)));
        assert_eq!(x.keys_for(Action::UseItem)[0], Some(KeyCode::KeyW));
        assert_eq!(
            x.keys_for(Action::Accelerate),
            [Some(KeyCode::KeyE), Some(KeyCode::ArrowUp), None]
        );
        // Binding an action's own secondary key swaps its two slots.
        assert!(x.capture(Action::Accelerate, Press::Key(KeyCode::ArrowUp)));
        assert_eq!(
            x.keys_for(Action::Accelerate),
            [Some(KeyCode::ArrowUp), Some(KeyCode::KeyE), None]
        );
        // A third-slot key moves the same way: J goes to respawn, which gives up R.
        assert!(x.capture(Action::Respawn, Press::Key(KeyCode::KeyJ)));
        assert_eq!(
            x.keys_for(Action::Respawn),
            [Some(KeyCode::KeyJ), None, None]
        );
        assert_eq!(x.keys_for(Action::UseItem)[2], Some(KeyCode::KeyR));
        // Pads too: B respawns; give it to look behind.
        assert!(x.capture(Action::LookBehind, Press::Pad(PadButtons::EAST)));
        assert_eq!(
            x.pad_for(Action::Respawn)[0],
            Some(PadBind::Button(PadButtons::NORTH))
        );
    }

    #[test]
    fn never_left_unbound() {
        let mut x = b();
        let presses = [
            Press::Key(KeyCode::KeyW),
            Press::Key(KeyCode::KeyS),
            Press::Key(KeyCode::Space),
            Press::Key(KeyCode::KeyE),
            Press::Key(KeyCode::ShiftRight),
            Press::Key(KeyCode::KeyQ),
            Press::Key(KeyCode::KeyJ),
            Press::Key(KeyCode::ControlLeft),
            Press::Pad(PadButtons::SOUTH),
            Press::Pad(PadButtons::LEFT_TRIGGER),
            Press::Pad(PadButtons::RIGHT_BUMPER),
            Press::Pad(PadButtons::WEST),
        ];
        for (i, p) in presses.iter().cycle().take(200).enumerate() {
            let a = Action::ALL[(i * 7 + i / 3) % Action::COUNT];
            x.capture(a, *p);
            for a in Action::ALL {
                assert!(x.keys_for(a)[0].is_some(), "{a:?} lost its key");
                assert!(x.pad_for(a)[0].is_some(), "{a:?} lost its button");
            }
        }
    }

    #[test]
    fn reserved_and_unknown_presses_are_refused() {
        let mut x = b();
        for p in [
            Press::Key(KeyCode::Escape),
            Press::Key(KeyCode::F3),
            Press::Key(KeyCode::Backspace),
            Press::Pad(PadButtons::START),
            Press::Pad(PadButtons::SELECT),
        ] {
            assert!(!x.capture(Action::UseItem, p), "{p:?}");
        }
        // Gamepad steering is always the left stick.
        assert!(!x.capture(Action::SteerLeft, Press::Pad(PadButtons::WEST)));
        assert_eq!(x, b());
    }

    #[test]
    fn only_shift_counts_either_side() {
        let mut x = b();
        // Right Shift binds as Shift, and either Shift drifts (as it always has).
        assert!(x.capture(Action::Respawn, Press::Key(KeyCode::ShiftRight)));
        assert_eq!(x.keys_for(Action::Respawn)[0], Some(KeyCode::ShiftLeft));
        let mut k = KeyInput::default();
        k.on_key(KeyCode::ShiftRight, true);
        assert!(b().held(Action::DriftHop, &k, None));
        assert!(b().pressed(Action::DriftHop, &k, None));
        // Ctrl and Alt are the left key only: the right ones fire nothing and can't be bound.
        let mut k = KeyInput::default();
        k.on_key(KeyCode::ControlRight, true);
        k.on_key(KeyCode::AltRight, true);
        for a in Action::ALL {
            assert!(!b().held(a, &k, None), "{a:?}");
        }
        assert!(!x.capture(Action::Respawn, Press::Key(KeyCode::ControlRight)));
        assert!(!x.capture(Action::Respawn, Press::Key(KeyCode::AltRight)));
    }

    #[test]
    fn values_from_keys_buttons_and_triggers() {
        let mut k = KeyInput::default();
        k.on_key(KeyCode::ArrowUp, true);
        assert_eq!(b().value(Action::Accelerate, &k, None), 1.0);
        let pad = PadState {
            right_trigger: 0.3,
            ..Default::default()
        };
        assert_eq!(
            b().value(Action::Accelerate, &KeyInput::default(), Some(&pad)),
            0.3
        );
        let pad = PadState {
            buttons: PadButtons::SOUTH,
            ..Default::default()
        };
        assert_eq!(
            b().value(Action::Accelerate, &KeyInput::default(), Some(&pad)),
            1.0
        );
        // A trigger bound to a button action presses past half travel.
        let mut x = b();
        x.capture(Action::UseItem, Press::Pad(PadButtons::RIGHT_TRIGGER));
        let pad = PadState {
            right_trigger: 0.8,
            buttons: PadButtons::RIGHT_TRIGGER,
            just_pressed: PadButtons::RIGHT_TRIGGER,
            ..Default::default()
        };
        assert!(x.pressed(Action::UseItem, &KeyInput::default(), Some(&pad)));
    }

    #[test]
    fn display_text_comes_from_the_bindings() {
        let d = b();
        assert_eq!(d.text(BindRow::Action(Action::Accelerate), false), "W / Up");
        assert_eq!(d.text(BindRow::Action(Action::Accelerate), true), "RT or A");
        assert_eq!(
            d.text(BindRow::Action(Action::DriftHop), false),
            "Space / Shift"
        );
        assert_eq!(d.text(BindRow::Action(Action::DriftHop), true), "LB or RB");
        assert_eq!(
            d.text(BindRow::Action(Action::SteerLeft), true),
            "Left stick"
        );
        assert_eq!(d.text(BindRow::Action(Action::LookBehind), false), "Q");
        assert_eq!(d.text(BindRow::Pause, true), "Menu");
        assert_eq!(d.hint(Action::UseItem), "E / X");
        assert_eq!(d.steer_keys_text(), "A D / Left Right");
        let mut x = d;
        x.capture(Action::UseItem, Press::Key(KeyCode::F5));
        assert_eq!(x.hint(Action::UseItem), "F5 / X");
    }
}
