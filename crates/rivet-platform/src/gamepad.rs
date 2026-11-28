//! Gamepads through gilrs. The game must keep running on keyboard if gilrs is unavailable.

use gilrs::{Axis, Button, EventType, GamepadId, Gilrs};

/// Gamepad buttons as a small bit set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PadButtons(pub u16);

impl PadButtons {
    pub const SOUTH: Self = Self(1 << 0);
    pub const EAST: Self = Self(1 << 1);
    pub const WEST: Self = Self(1 << 2);
    pub const NORTH: Self = Self(1 << 3);
    pub const LEFT_BUMPER: Self = Self(1 << 4);
    pub const RIGHT_BUMPER: Self = Self(1 << 5);
    pub const START: Self = Self(1 << 6);
    pub const SELECT: Self = Self(1 << 7);
    pub const DPAD_UP: Self = Self(1 << 8);
    pub const DPAD_DOWN: Self = Self(1 << 9);
    pub const DPAD_LEFT: Self = Self(1 << 10);
    pub const DPAD_RIGHT: Self = Self(1 << 11);
    /// The analog triggers, pressed past half travel (see `trigger_press`).
    pub const LEFT_TRIGGER: Self = Self(1 << 12);
    pub const RIGHT_TRIGGER: Self = Self(1 << 13);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0 && other.0 != 0
    }
    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }
    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }
}

impl std::ops::BitOr for PadButtons {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// One pad's state after deadzones.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PadState {
    /// Left stick X, -1 (left) to 1 (right).
    pub left_x: f32,
    pub right_trigger: f32,
    pub left_trigger: f32,
    pub buttons: PadButtons,
    pub just_pressed: PadButtons,
}

/// The stick deadzone until `Gamepads::set_stick_deadzone` changes it.
pub const STICK_DEADZONE: f32 = 0.15;
pub const TRIGGER_DEADZONE: f32 = 0.05;
/// Trigger travel that counts as a button press, and the lower travel that releases it, so a
/// trigger held near half doesn't chatter.
const TRIGGER_PRESS: f32 = 0.5;
const TRIGGER_RELEASE: f32 = 0.4;

/// A usable stick deadzone: 0 to 0.95, the default for a non-number.
pub fn clamp_deadzone(dz: f32) -> f32 {
    if dz.is_nan() {
        STICK_DEADZONE
    } else {
        dz.clamp(0.0, 0.95)
    }
}

/// Sets or clears a trigger's button bit in `s` from its travel `v` (after the deadzone).
pub fn trigger_press(s: &mut PadState, bit: PadButtons, v: f32) {
    if v >= TRIGGER_PRESS {
        if !s.buttons.contains(bit) {
            s.just_pressed.insert(bit);
        }
        s.buttons.insert(bit);
    } else if v < TRIGGER_RELEASE {
        s.buttons.remove(bit);
    }
}

/// 1D deadzone: 0 inside `dz`, rescaled to 0..1 outside, sign kept.
pub fn apply_deadzone(v: f32, dz: f32) -> f32 {
    let a = v.abs();
    if a <= dz {
        0.0
    } else {
        (((a - dz) / (1.0 - dz)).min(1.0)).copysign(v)
    }
}

fn map_button(b: Button) -> Option<PadButtons> {
    Some(match b {
        Button::South => PadButtons::SOUTH,
        Button::East => PadButtons::EAST,
        Button::West => PadButtons::WEST,
        Button::North => PadButtons::NORTH,
        Button::LeftTrigger => PadButtons::LEFT_BUMPER,
        Button::RightTrigger => PadButtons::RIGHT_BUMPER,
        Button::Start => PadButtons::START,
        Button::Select => PadButtons::SELECT,
        Button::DPadUp => PadButtons::DPAD_UP,
        Button::DPadDown => PadButtons::DPAD_DOWN,
        Button::DPadLeft => PadButtons::DPAD_LEFT,
        Button::DPadRight => PadButtons::DPAD_RIGHT,
        _ => return None,
    })
}

/// All connected pads; the active one is whichever sent input most recently.
pub struct Gamepads {
    gilrs: Gilrs,
    active: Option<GamepadId>,
    state: PadState,
    stick_deadzone: f32,
    /// Left stick X before the deadzone, so a deadzone change applies to a stick held still.
    raw_left_x: f32,
}

impl Gamepads {
    /// `None` (with a warning) when the platform gamepad backend can't start.
    pub fn new() -> Option<Self> {
        match Gilrs::new() {
            Ok(gilrs) => {
                for (_, pad) in gilrs.gamepads() {
                    log::info!("gamepad connected: {}", pad.name());
                }
                Some(Self {
                    gilrs,
                    active: None,
                    state: PadState::default(),
                    stick_deadzone: STICK_DEADZONE,
                    raw_left_x: 0.0,
                })
            }
            Err(e) => {
                log::warn!("gamepads unavailable: {e}");
                None
            }
        }
    }

    /// The left stick deadzone, 0 to 0.95 of travel. Cheap to call every frame.
    pub fn set_stick_deadzone(&mut self, dz: f32) {
        let dz = clamp_deadzone(dz);
        if dz != self.stick_deadzone {
            self.stick_deadzone = dz;
            self.state.left_x = apply_deadzone(self.raw_left_x, dz);
        }
    }

    /// Drains pending events. Call once per frame before reading `active`.
    pub fn poll(&mut self) {
        self.state.just_pressed = PadButtons::default();
        while let Some(ev) = self.gilrs.next_event() {
            match ev.event {
                EventType::Connected => {
                    log::info!("gamepad connected: {}", self.gilrs.gamepad(ev.id).name());
                    continue;
                }
                EventType::Disconnected => {
                    log::info!("gamepad disconnected");
                    if self.active == Some(ev.id) {
                        self.active = None;
                        self.state = PadState::default();
                        self.raw_left_x = 0.0;
                    }
                    continue;
                }
                _ => {}
            }
            if self.active != Some(ev.id) {
                self.active = Some(ev.id);
                self.state = PadState::default();
                self.raw_left_x = 0.0;
            }
            let s = &mut self.state;
            match ev.event {
                EventType::ButtonPressed(b, _) => {
                    if let Some(m) = map_button(b) {
                        if !s.buttons.contains(m) {
                            s.just_pressed.insert(m);
                        }
                        s.buttons.insert(m);
                    }
                }
                EventType::ButtonReleased(b, _) => {
                    if let Some(m) = map_button(b) {
                        s.buttons.remove(m);
                    }
                }
                EventType::ButtonChanged(Button::RightTrigger2, v, _) => {
                    s.right_trigger = apply_deadzone(v, TRIGGER_DEADZONE);
                    trigger_press(s, PadButtons::RIGHT_TRIGGER, s.right_trigger);
                }
                EventType::ButtonChanged(Button::LeftTrigger2, v, _) => {
                    s.left_trigger = apply_deadzone(v, TRIGGER_DEADZONE);
                    trigger_press(s, PadButtons::LEFT_TRIGGER, s.left_trigger);
                }
                EventType::AxisChanged(Axis::LeftStickX, v, _) => {
                    self.raw_left_x = v;
                    s.left_x = apply_deadzone(v, self.stick_deadzone);
                }
                _ => {}
            }
        }
    }

    pub fn active(&self) -> Option<&PadState> {
        self.active.map(|_| &self.state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadzone() {
        assert_eq!(apply_deadzone(0.1, 0.15), 0.0);
        assert!((apply_deadzone(1.0, 0.15) - 1.0).abs() < 1e-6);
        assert!((apply_deadzone(-0.575, 0.15) + 0.5).abs() < 1e-5);
    }

    #[test]
    fn deadzone_at_the_settings_range() {
        // 0 passes the stick through untouched.
        assert_eq!(apply_deadzone(0.1, 0.0), 0.1);
        assert_eq!(apply_deadzone(-1.0, 0.0), -1.0);
        // 30% ignores more and still reaches full lock.
        assert_eq!(apply_deadzone(0.29, 0.3), 0.0);
        assert!((apply_deadzone(0.65, 0.3) - 0.5).abs() < 1e-5);
        assert!((apply_deadzone(-1.0, 0.3) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn stick_deadzone_is_clamped() {
        assert_eq!(clamp_deadzone(0.2), 0.2);
        assert_eq!(clamp_deadzone(-1.0), 0.0);
        assert_eq!(clamp_deadzone(5.0), 0.95);
        assert_eq!(clamp_deadzone(f32::NAN), STICK_DEADZONE);
    }

    #[test]
    fn triggers_press_like_buttons_past_half_travel() {
        let mut s = PadState::default();
        trigger_press(&mut s, PadButtons::RIGHT_TRIGGER, 0.3);
        assert!(!s.buttons.contains(PadButtons::RIGHT_TRIGGER));
        trigger_press(&mut s, PadButtons::RIGHT_TRIGGER, 0.6);
        assert!(s.buttons.contains(PadButtons::RIGHT_TRIGGER));
        assert!(s.just_pressed.contains(PadButtons::RIGHT_TRIGGER));
        s.just_pressed = PadButtons::default();
        // Hysteresis: a trigger hovering at half doesn't chatter.
        trigger_press(&mut s, PadButtons::RIGHT_TRIGGER, 0.45);
        assert!(s.buttons.contains(PadButtons::RIGHT_TRIGGER));
        trigger_press(&mut s, PadButtons::RIGHT_TRIGGER, 0.6);
        assert!(!s.just_pressed.contains(PadButtons::RIGHT_TRIGGER));
        trigger_press(&mut s, PadButtons::RIGHT_TRIGGER, 0.1);
        assert!(!s.buttons.contains(PadButtons::RIGHT_TRIGGER));
    }
}
