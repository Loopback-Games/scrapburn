#![forbid(unsafe_code)]
//! Nailrunners game logic. The binary in `main.rs` wires it to a window.

pub mod args;
pub mod autopilot;
pub mod bench;
pub mod camera;
pub mod controls;
pub mod laps;
pub mod png;
pub mod style;
pub mod track;

/// Fixed simulation tick.
pub const TICK_DT: f64 = 1.0 / 60.0;
/// Most sim steps run in one rendered frame.
pub const MAX_STEPS: u32 = 5;
