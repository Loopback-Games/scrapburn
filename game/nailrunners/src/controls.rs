//! Keyboard + gamepad → drive input and app actions.

use rivet_physics::DriveInput;
use rivet_platform::{KeyInput, PadButtons, PadState};
use winit::keyboard::KeyCode;

/// App-level actions this frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Actions {
    pub respawn: bool,
    pub toggle_graph: bool,
    pub quit: bool,
}

fn axis(neg: bool, pos: bool) -> f32 {
    (pos as i32 - neg as i32) as f32
}

/// Larger magnitude wins, so a full keyboard press beats a partial stick.
fn merge(a: f32, b: f32) -> f32 {
    if a.abs() >= b.abs() { a } else { b }
}

pub fn drive_input(keys: &KeyInput, pad: Option<&PadState>) -> DriveInput {
    let held = |a: KeyCode, b: KeyCode| keys.held(a) || keys.held(b);
    let pressed = |a: KeyCode, b: KeyCode| keys.just_pressed(a) || keys.just_pressed(b);
    let mut throttle = if held(KeyCode::KeyW, KeyCode::ArrowUp) {
        1.0f32
    } else {
        0.0
    };
    let mut brake = if held(KeyCode::KeyS, KeyCode::ArrowDown) {
        1.0f32
    } else {
        0.0
    };
    let key_steer = axis(
        held(KeyCode::KeyA, KeyCode::ArrowLeft),
        held(KeyCode::KeyD, KeyCode::ArrowRight),
    );
    let mut drift = held(KeyCode::Space, KeyCode::ShiftLeft) || keys.held(KeyCode::ShiftRight);
    let mut hop =
        pressed(KeyCode::Space, KeyCode::ShiftLeft) || keys.just_pressed(KeyCode::ShiftRight);
    let mut steer = key_steer;
    if let Some(p) = pad {
        let bumpers = |set: PadButtons| {
            set.contains(PadButtons::LEFT_BUMPER) || set.contains(PadButtons::RIGHT_BUMPER)
        };
        throttle = throttle.max(p.right_trigger);
        if p.buttons.contains(PadButtons::SOUTH) {
            throttle = 1.0;
        }
        brake = brake.max(p.left_trigger);
        steer = merge(key_steer, p.left_x);
        drift |= bumpers(p.buttons);
        hop |= bumpers(p.just_pressed);
    }
    DriveInput {
        throttle,
        brake,
        steer: steer.clamp(-1.0, 1.0),
        drift,
        hop,
    }
}

/// Smooths digital (keyboard) steering so taps don't snap the kart; analog passes through.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SteerRamp {
    value: f32,
}

/// Keyboard steer ramp rates (full lock per second): in, and back toward center.
const STEER_RAMP_IN: f32 = 6.0;
const STEER_RAMP_OUT: f32 = 10.0;

impl SteerRamp {
    /// `digital` is true when the steer came from keys rather than a stick.
    pub fn apply(&mut self, target: f32, digital: bool, dt: f32) -> f32 {
        if !digital {
            self.value = target;
            return target;
        }
        let returning = target.abs() < self.value.abs() || target.signum() != self.value.signum();
        let rate = if returning {
            STEER_RAMP_OUT
        } else {
            STEER_RAMP_IN
        } * dt;
        self.value += (target - self.value).clamp(-rate, rate);
        self.value
    }
}

/// Holds a hop press until a sim step consumes it, so presses on frames that run zero sim
/// steps (common at high refresh rates) aren't lost.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HopLatch {
    pending: bool,
}

impl HopLatch {
    /// Call once per rendered frame with this frame's `DriveInput::hop`.
    pub fn press(&mut self, hop: bool) {
        self.pending |= hop;
    }

    /// Call once per sim step; true for exactly one step per press.
    pub fn take(&mut self) -> bool {
        std::mem::take(&mut self.pending)
    }
}

pub fn actions(keys: &KeyInput, pad: Option<&PadState>) -> Actions {
    let pad_pressed = |b: PadButtons| pad.is_some_and(|p| p.just_pressed.contains(b));
    Actions {
        respawn: keys.just_pressed(KeyCode::KeyR) || pad_pressed(PadButtons::EAST),
        toggle_graph: keys.just_pressed(KeyCode::F3) || pad_pressed(PadButtons::SELECT),
        quit: keys.just_pressed(KeyCode::Escape),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rivet_platform::{KeyInput, PadButtons, PadState};
    use winit::keyboard::KeyCode;

    #[test]
    fn controls_map() {
        let mut k = KeyInput::default();
        k.on_key(KeyCode::KeyW, true);
        k.on_key(KeyCode::KeyD, true);
        let i = drive_input(&k, None);
        assert_eq!(i.throttle, 1.0);
        assert_eq!(i.steer, 1.0);
        k.on_key(KeyCode::Space, true);
        let i = drive_input(&k, None);
        assert!(i.drift && i.hop);
        k.end_frame();
        let i = drive_input(&k, None);
        assert!(i.drift && !i.hop, "hop only on the press");
    }

    #[test]
    fn controls_pad() {
        let k = KeyInput::default();
        let pad = PadState {
            left_x: 0.5,
            right_trigger: 0.8,
            buttons: PadButtons::RIGHT_BUMPER,
            just_pressed: PadButtons::RIGHT_BUMPER,
            ..Default::default()
        };
        let i = drive_input(&k, Some(&pad));
        assert_eq!(i.steer, 0.5);
        assert_eq!(i.throttle, 0.8);
        assert!(i.drift && i.hop);
        let mut k = KeyInput::default();
        k.on_key(KeyCode::KeyD, true);
        assert_eq!(drive_input(&k, Some(&pad)).steer, 1.0);
    }

    #[test]
    fn digital_steering_ramps_analog_passes() {
        let mut ramp = SteerRamp::default();
        let first = ramp.apply(1.0, true, 1.0 / 60.0);
        assert!(
            first > 0.05 && first < 0.2,
            "keyboard steer ramps in: {first}"
        );
        for _ in 0..30 {
            ramp.apply(1.0, true, 1.0 / 60.0);
        }
        assert_eq!(ramp.apply(1.0, true, 1.0 / 60.0), 1.0);
        let back = ramp.apply(0.0, true, 1.0 / 60.0);
        assert!(back < 0.9, "and ramps out faster: {back}");
        assert_eq!(SteerRamp::default().apply(0.37, false, 1.0 / 60.0), 0.37);
    }

    #[test]
    fn hop_survives_frames_without_sim_steps() {
        let mut latch = HopLatch::default();
        latch.press(true); // pressed on a frame that runs 0 sim steps
        latch.press(false); // next frame, button no longer "just pressed"
        assert!(latch.take(), "the press must reach the first sim step");
        assert!(!latch.take(), "and hop only once");
    }

    #[test]
    fn actions_from_both_devices() {
        let mut k = KeyInput::default();
        k.on_key(KeyCode::KeyR, true);
        assert!(actions(&k, None).respawn);
        let pad = PadState {
            just_pressed: PadButtons::SELECT,
            ..Default::default()
        };
        let a = actions(&KeyInput::default(), Some(&pad));
        assert!(a.toggle_graph && !a.respawn && !a.quit);
    }
}
