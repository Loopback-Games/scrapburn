#![forbid(unsafe_code)]
//! Scrapburn game logic. The binary in `main.rs` wires it to a window.

pub mod args;
pub mod autopilot;
pub mod autotune;
pub mod bench;
pub mod bindings;
pub mod camera;
pub mod circuit;
pub mod combat;
pub mod controls;
pub mod debounce;
pub mod display;
pub mod effects;
pub mod embers;
pub mod fx_meshes;
pub mod ghost;
pub mod haptics;
pub mod hud;
pub mod kart;
pub mod laps;
pub mod limiter;
pub mod materials;
pub mod menu;
pub mod menu_draw;
pub mod netplay;
pub mod particles;
pub mod png;
pub mod posters;
pub mod prefs;
pub mod race;
pub mod scenery;
pub mod sound;
pub mod style;
pub mod terrain;
pub mod track;
pub mod update;

/// Fixed simulation tick.
pub const TICK_DT: f64 = 1.0 / 60.0;
/// Most sim steps run in one rendered frame.
pub const MAX_STEPS: u32 = 5;
