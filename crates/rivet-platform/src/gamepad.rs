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

pub const STICK_DEADZONE: f32 = 0.15;
pub const TRIGGER_DEADZONE: f32 = 0.05;

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
        _ => return None,
    })
}

/// All connected pads; the active one is whichever sent input most recently.
pub struct Gamepads {
    gilrs: Gilrs,
    active: Option<GamepadId>,
    state: PadState,
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
                })
            }
            Err(e) => {
                log::warn!("gamepads unavailable: {e}");
                None
            }
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
                    }
                    continue;
                }
                _ => {}
            }
            if self.active != Some(ev.id) {
                self.active = Some(ev.id);
                self.state = PadState::default();
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
                }
                EventType::ButtonChanged(Button::LeftTrigger2, v, _) => {
                    s.left_trigger = apply_deadzone(v, TRIGGER_DEADZONE);
                }
                EventType::AxisChanged(Axis::LeftStickX, v, _) => {
                    s.left_x = apply_deadzone(v, STICK_DEADZONE);
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
}
