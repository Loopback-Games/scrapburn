//! Keyboard + gamepad → drive input and app actions, through the player's `Bindings`.

use rivet_physics::DriveInput;
use rivet_platform::{KeyInput, PadButtons, PadState};
use winit::keyboard::KeyCode;

use crate::bindings::{Action, Bindings, Press};

/// App-level actions this frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Actions {
    pub respawn: bool,
    /// Use the held item.
    pub fire: bool,
    /// Held: the camera looks back.
    pub look_behind: bool,
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

/// Driving input from the bound keys and buttons. Gamepad steering is the left stick.
pub fn drive_input(keys: &KeyInput, pad: Option<&PadState>, b: &Bindings) -> DriveInput {
    let key_steer = axis(
        b.key_held(Action::SteerLeft, keys),
        b.key_held(Action::SteerRight, keys),
    );
    DriveInput {
        throttle: b.value(Action::Accelerate, keys, pad),
        brake: b.value(Action::Brake, keys, pad),
        steer: merge(key_steer, pad.map_or(0.0, |p| p.left_x)).clamp(-1.0, 1.0),
        drift: b.held(Action::DriftHop, keys, pad),
        hop: b.pressed(Action::DriftHop, keys, pad),
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// The steer curve exponent: 1.6 at sensitivity 0, exactly 1.0 at 50, 0.6 at 100, linear
/// on each side of 50.
fn steer_exponent(sensitivity: u8) -> f32 {
    let s = f32::from(sensitivity.min(100));
    if s <= 50.0 {
        1.0 + 0.6 * (50.0 - s) / 50.0
    } else {
        1.0 - 0.4 * (s - 50.0) / 50.0
    }
}

/// Analog steering response: `sign(s)·|s|^e` (see `steer_exponent`). Low sensitivity is
/// gentle near center, high is sharp, 50 leaves the stick as it is, and full lock stays full.
pub fn steer_curve(s: f32, sensitivity: u8) -> f32 {
    let e = steer_exponent(sensitivity);
    if e == 1.0 {
        return s;
    }
    s.abs().powf(e).copysign(s)
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
    /// `digital` is true when the steer came from keys rather than a stick. `sensitivity`
    /// (0-100, 50 neutral) curves the stick (see `steer_curve`) and scales how fast keys
    /// ramp in by `lerp(0.6, 1.4, sensitivity / 100)`.
    pub fn apply(&mut self, target: f32, digital: bool, dt: f32, sensitivity: u8) -> f32 {
        if !digital {
            self.value = steer_curve(target, sensitivity);
            return self.value;
        }
        let ramp_in = STEER_RAMP_IN * lerp(0.6, 1.4, f32::from(sensitivity.min(100)) / 100.0);
        let returning = target.abs() < self.value.abs() || target.signum() != self.value.signum();
        let rate = if returning { STEER_RAMP_OUT } else { ramp_in } * dt;
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

/// Edge-triggered menu navigation from keyboard and gamepad.
pub fn menu_input(keys: &KeyInput, pad: Option<&PadState>) -> crate::menu::MenuInput {
    let k = |a: KeyCode, b: KeyCode| keys.just_pressed(a) || keys.just_pressed(b);
    let p = |b: PadButtons| pad.is_some_and(|p| p.just_pressed.contains(b));
    crate::menu::MenuInput {
        up: k(KeyCode::ArrowUp, KeyCode::KeyW) || p(PadButtons::DPAD_UP),
        down: k(KeyCode::ArrowDown, KeyCode::KeyS) || p(PadButtons::DPAD_DOWN),
        left: k(KeyCode::ArrowLeft, KeyCode::KeyA) || p(PadButtons::DPAD_LEFT),
        right: k(KeyCode::ArrowRight, KeyCode::KeyD) || p(PadButtons::DPAD_RIGHT),
        select: k(KeyCode::Enter, KeyCode::Space) || p(PadButtons::SOUTH),
        back: k(KeyCode::Escape, KeyCode::Backspace) || p(PadButtons::EAST),
        tab_prev: keys.just_pressed(KeyCode::KeyQ) || p(PadButtons::LEFT_BUMPER),
        tab_next: keys.just_pressed(KeyCode::KeyE) || p(PadButtons::RIGHT_BUMPER),
    }
}

/// Menu navigation while a text field has focus: arrows, Enter, Escape and the pad only, so
/// letters, Space and Backspace can be typed.
pub fn menu_input_typing(keys: &KeyInput, pad: Option<&PadState>) -> crate::menu::MenuInput {
    let k = |a: KeyCode| keys.just_pressed(a);
    let p = |b: PadButtons| pad.is_some_and(|p| p.just_pressed.contains(b));
    crate::menu::MenuInput {
        up: k(KeyCode::ArrowUp) || p(PadButtons::DPAD_UP),
        down: k(KeyCode::ArrowDown) || p(PadButtons::DPAD_DOWN),
        left: false,
        right: false,
        select: k(KeyCode::Enter) || k(KeyCode::NumpadEnter) || p(PadButtons::SOUTH),
        back: k(KeyCode::Escape) || p(PadButtons::EAST),
        ..Default::default()
    }
}

/// App actions from the bound keys and buttons. Pause (Esc) and the FPS display (F3 /
/// Select) are fixed.
pub fn actions(keys: &KeyInput, pad: Option<&PadState>, b: &Bindings) -> Actions {
    let pad_pressed = |bit: PadButtons| pad.is_some_and(|p| p.just_pressed.contains(bit));
    Actions {
        respawn: b.pressed(Action::Respawn, keys, pad),
        fire: b.pressed(Action::UseItem, keys, pad),
        look_behind: b.held(Action::LookBehind, keys, pad),
        toggle_graph: keys.just_pressed(KeyCode::F3) || pad_pressed(PadButtons::SELECT),
        quit: keys.just_pressed(KeyCode::Escape),
    }
}

/// This frame's key and gamepad presses, for a binding capture: keys first, then pad
/// buttons (the triggers as `LEFT_TRIGGER` / `RIGHT_TRIGGER`), each in a fixed order.
pub fn capture_presses(keys: &KeyInput, pad: Option<&PadState>) -> Vec<Press> {
    let mut v: Vec<Press> = keys.pressed_keys().into_iter().map(Press::Key).collect();
    if let Some(p) = pad {
        v.extend(
            (0..16)
                .map(|i| PadButtons(1 << i))
                .filter(|bit| p.just_pressed.contains(*bit))
                .map(Press::Pad),
        );
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bindings::{Action, Bindings, Press};
    use rivet_platform::{KeyInput, PadButtons, PadState};
    use winit::keyboard::KeyCode;

    fn d() -> Bindings {
        Bindings::default()
    }

    #[test]
    fn controls_map() {
        let mut k = KeyInput::default();
        k.on_key(KeyCode::KeyW, true);
        k.on_key(KeyCode::KeyD, true);
        let i = drive_input(&k, None, &d());
        assert_eq!(i.throttle, 1.0);
        assert_eq!(i.steer, 1.0);
        k.on_key(KeyCode::Space, true);
        let i = drive_input(&k, None, &d());
        assert!(i.drift && i.hop);
        k.end_frame();
        let i = drive_input(&k, None, &d());
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
        let i = drive_input(&k, Some(&pad), &d());
        assert_eq!(i.steer, 0.5);
        assert_eq!(i.throttle, 0.8);
        assert!(i.drift && i.hop);
        let mut k = KeyInput::default();
        k.on_key(KeyCode::KeyD, true);
        assert_eq!(drive_input(&k, Some(&pad), &d()).steer, 1.0);
    }

    #[test]
    fn digital_steering_ramps_analog_passes() {
        let mut ramp = SteerRamp::default();
        let first = ramp.apply(1.0, true, 1.0 / 60.0, 50);
        assert!(
            first > 0.05 && first < 0.2,
            "keyboard steer ramps in: {first}"
        );
        for _ in 0..30 {
            ramp.apply(1.0, true, 1.0 / 60.0, 50);
        }
        assert_eq!(ramp.apply(1.0, true, 1.0 / 60.0, 50), 1.0);
        let back = ramp.apply(0.0, true, 1.0 / 60.0, 50);
        assert!(back < 0.9, "and ramps out faster: {back}");
        assert_eq!(
            SteerRamp::default().apply(0.37, false, 1.0 / 60.0, 50),
            0.37
        );
    }

    #[test]
    fn sensitivity_curves_the_stick() {
        // e runs 1.6 at 0, exactly 1.0 at 50 (the stick passes through), 0.6 at 100.
        assert!((steer_curve(0.5, 0) - 0.5f32.powf(1.6)).abs() < 1e-6);
        assert_eq!(steer_curve(0.5, 50), 0.5);
        assert_eq!(steer_curve(-0.37, 50), -0.37);
        assert!((steer_curve(0.5, 25) - 0.5f32.powf(1.3)).abs() < 1e-6);
        assert!((steer_curve(0.5, 75) - 0.5f32.powf(0.8)).abs() < 1e-6);
        assert!((steer_curve(-0.5, 100) + 0.5f32.powf(0.6)).abs() < 1e-6);
        assert!(steer_curve(0.5, 0) < steer_curve(0.5, 50));
        assert!(steer_curve(0.5, 50) < steer_curve(0.5, 100));
        for sens in [0, 50, 100] {
            assert_eq!(steer_curve(1.0, sens), 1.0, "full lock stays full");
            assert_eq!(steer_curve(-1.0, sens), -1.0);
            assert_eq!(steer_curve(0.0, sens), 0.0);
        }
    }

    #[test]
    fn sensitivity_scales_the_keyboard_ramp_in() {
        let dt = 1.0 / 60.0;
        let first = |sens| SteerRamp::default().apply(1.0, true, dt, sens);
        // STEER_RAMP_IN (6 per second) times lerp(0.6, 1.4, sens / 100).
        assert!((first(0) - 6.0 * 0.6 * dt).abs() < 1e-6);
        assert!((first(50) - 6.0 * dt).abs() < 1e-6);
        assert!((first(100) - 6.0 * 1.4 * dt).abs() < 1e-6);
        // Ramp out is unchanged.
        let mut ramp = SteerRamp::default();
        for _ in 0..60 {
            ramp.apply(1.0, true, dt, 0);
        }
        assert!((ramp.apply(0.0, true, dt, 0) - (1.0 - 10.0 * dt)).abs() < 1e-6);
    }

    #[test]
    fn rebound_keys_drive() {
        let mut b = d();
        b.capture(Action::Accelerate, Press::Key(KeyCode::KeyI));
        b.capture(Action::LookBehind, Press::Pad(PadButtons::LEFT_BUMPER));
        let mut k = KeyInput::default();
        k.on_key(KeyCode::KeyI, true);
        assert_eq!(drive_input(&k, None, &b).throttle, 1.0);
        let mut k = KeyInput::default();
        k.on_key(KeyCode::KeyW, true);
        assert_eq!(
            drive_input(&k, None, &b).throttle,
            0.0,
            "W is no longer bound"
        );
        let pad = PadState {
            buttons: PadButtons::LEFT_BUMPER,
            just_pressed: PadButtons::LEFT_BUMPER,
            ..Default::default()
        };
        assert!(actions(&KeyInput::default(), Some(&pad), &b).look_behind);
        // Drift kept its other bumper.
        let i = drive_input(&KeyInput::default(), Some(&pad), &b);
        assert!(!i.drift && !i.hop);
    }

    #[test]
    fn look_behind_is_held() {
        let mut k = KeyInput::default();
        k.on_key(KeyCode::KeyQ, true);
        k.end_frame();
        assert!(actions(&k, None, &d()).look_behind);
        k.on_key(KeyCode::KeyQ, false);
        assert!(!actions(&k, None, &d()).look_behind);
    }

    #[test]
    fn capture_presses_lists_keys_then_pad_buttons() {
        let mut k = KeyInput::default();
        k.on_key(KeyCode::KeyZ, true);
        let pad = PadState {
            just_pressed: PadButtons::WEST | PadButtons::RIGHT_TRIGGER,
            ..Default::default()
        };
        assert_eq!(
            capture_presses(&k, Some(&pad)),
            vec![
                Press::Key(KeyCode::KeyZ),
                Press::Pad(PadButtons::WEST),
                Press::Pad(PadButtons::RIGHT_TRIGGER),
            ]
        );
        assert!(capture_presses(&KeyInput::default(), None).is_empty());
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
    fn typing_mode_ignores_letter_keys_and_backspace() {
        let mut k = KeyInput::default();
        k.on_key(KeyCode::KeyS, true);
        k.on_key(KeyCode::KeyD, true);
        k.on_key(KeyCode::Backspace, true);
        k.on_key(KeyCode::Space, true);
        let i = menu_input_typing(&k, None);
        assert_eq!(i, crate::menu::MenuInput::default());
        k.on_key(KeyCode::ArrowDown, true);
        k.on_key(KeyCode::Escape, true);
        let i = menu_input_typing(&k, None);
        assert!(i.down && i.back);
    }

    #[test]
    fn menu_input_from_keys_and_pad() {
        let mut k = KeyInput::default();
        k.on_key(KeyCode::ArrowDown, true);
        k.on_key(KeyCode::Enter, true);
        let m = menu_input(&k, None);
        assert!(m.down && m.select && !m.up);
        let pad = PadState {
            just_pressed: PadButtons::DPAD_UP | PadButtons::EAST,
            ..Default::default()
        };
        let m = menu_input(&KeyInput::default(), Some(&pad));
        assert!(m.up && m.back);
    }

    #[test]
    fn menu_tabs_from_q_e_and_bumpers() {
        let mut k = KeyInput::default();
        k.on_key(KeyCode::KeyQ, true);
        let m = menu_input(&k, None);
        assert!(m.tab_prev && !m.tab_next);
        let mut k = KeyInput::default();
        k.on_key(KeyCode::KeyE, true);
        assert!(menu_input(&k, None).tab_next);
        let pad = PadState {
            just_pressed: PadButtons::LEFT_BUMPER,
            ..Default::default()
        };
        let m = menu_input(&KeyInput::default(), Some(&pad));
        assert!(m.tab_prev && !m.tab_next);
        let pad = PadState {
            just_pressed: PadButtons::RIGHT_BUMPER,
            ..Default::default()
        };
        assert!(menu_input(&KeyInput::default(), Some(&pad)).tab_next);
        // A text field keeps Q and E for typing.
        let mut k = KeyInput::default();
        k.on_key(KeyCode::KeyQ, true);
        assert!(!menu_input_typing(&k, None).tab_prev);
    }

    #[test]
    fn actions_from_both_devices() {
        let mut k = KeyInput::default();
        k.on_key(KeyCode::KeyR, true);
        assert!(actions(&k, None, &d()).respawn);
        k.on_key(KeyCode::KeyE, true);
        assert!(actions(&k, None, &d()).fire);
        let mut k = KeyInput::default();
        k.on_key(KeyCode::KeyJ, true);
        assert!(actions(&k, None, &d()).fire, "J still fires");
        let mut k = KeyInput::default();
        k.on_key(KeyCode::ControlRight, true);
        assert!(!actions(&k, None, &d()).fire, "only Left Ctrl fires");
        let pad = PadState {
            just_pressed: PadButtons::WEST,
            ..Default::default()
        };
        assert!(actions(&KeyInput::default(), Some(&pad), &d()).fire);
        let pad = PadState {
            just_pressed: PadButtons::SELECT,
            ..Default::default()
        };
        let a = actions(&KeyInput::default(), Some(&pad), &d());
        assert!(a.toggle_graph && !a.respawn && !a.quit);
    }
}
