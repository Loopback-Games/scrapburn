//! Keyboard state with held / just-pressed queries.

use std::collections::HashSet;
use winit::keyboard::KeyCode;

/// Keyboard state for one frame. Feed winit key events, query, then call `end_frame`.
#[derive(Debug, Default, Clone)]
pub struct KeyInput {
    held: HashSet<KeyCode>,
    just_pressed: HashSet<KeyCode>,
}

impl KeyInput {
    pub fn on_key(&mut self, code: KeyCode, pressed: bool) {
        if pressed {
            // `insert` returns false for OS key repeat while held.
            if self.held.insert(code) {
                self.just_pressed.insert(code);
            }
        } else {
            self.held.remove(&code);
        }
    }

    pub fn held(&self, code: KeyCode) -> bool {
        self.held.contains(&code)
    }

    pub fn just_pressed(&self, code: KeyCode) -> bool {
        self.just_pressed.contains(&code)
    }

    /// The keys first pressed this frame, sorted so the order is repeatable.
    pub fn pressed_keys(&self) -> Vec<KeyCode> {
        let mut v: Vec<KeyCode> = self.just_pressed.iter().copied().collect();
        v.sort();
        v
    }

    /// Releases everything, e.g. when the window loses focus.
    pub fn clear(&mut self) {
        self.held.clear();
        self.just_pressed.clear();
    }

    pub fn end_frame(&mut self) {
        self.just_pressed.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::KeyCode;

    #[test]
    fn just_pressed_once() {
        let mut k = KeyInput::default();
        k.on_key(KeyCode::KeyR, true);
        assert!(k.just_pressed(KeyCode::KeyR));
        assert!(k.held(KeyCode::KeyR));
        k.end_frame();
        k.on_key(KeyCode::KeyR, true); // OS key repeat while held
        assert!(!k.just_pressed(KeyCode::KeyR));
        k.on_key(KeyCode::KeyR, false);
        assert!(!k.held(KeyCode::KeyR));
        k.end_frame();
        k.on_key(KeyCode::KeyR, true);
        assert!(k.just_pressed(KeyCode::KeyR));
    }

    #[test]
    fn lists_this_frames_presses_in_order() {
        let mut k = KeyInput::default();
        k.on_key(KeyCode::KeyZ, true);
        k.on_key(KeyCode::KeyA, true);
        assert_eq!(k.pressed_keys(), vec![KeyCode::KeyA, KeyCode::KeyZ]);
        k.end_frame();
        assert!(k.pressed_keys().is_empty());
    }
}
