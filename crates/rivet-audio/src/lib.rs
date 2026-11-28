#![forbid(unsafe_code)]
//! Rivet audio: a small procedural synth mixer, a synthesized soundtrack and a cpal output
//! stream.

pub mod mixer;
pub mod music;
pub mod output;
pub mod songs;

pub use mixer::{Bus, Command, Mixer, Sfx};
pub use output::Audio;
pub use songs::{Theme, ThemeInfo};
