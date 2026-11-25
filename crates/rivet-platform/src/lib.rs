#![forbid(unsafe_code)]
//! Rivet platform layer: GPU adapter choice, keyboard state and gamepads.

pub mod adapter;
pub mod display;
pub mod gamepad;
pub mod input;

pub use adapter::{
    AdapterDesc, AdapterError, AdapterPreference, candidate_order, select_adapter,
    window_candidates,
};
pub use display::display_gpus;
pub use gamepad::{Gamepads, PadButtons, PadState, apply_deadzone};
pub use input::KeyInput;
