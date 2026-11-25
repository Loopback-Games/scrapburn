#![forbid(unsafe_code)]
//! Rivet platform layer: GPU adapter choice, keyboard state and gamepads.

pub mod adapter;
pub mod gamepad;
pub mod input;

pub use adapter::{AdapterDesc, AdapterError, AdapterPreference, select_adapter};
pub use gamepad::{Gamepads, PadButtons, PadState, apply_deadzone};
pub use input::KeyInput;
