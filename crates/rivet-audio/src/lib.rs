#![forbid(unsafe_code)]
//! Rivet audio: a small procedural synth mixer and a cpal output stream.

pub mod mixer;
pub mod output;

pub use mixer::{Command, Mixer, Sfx};
pub use output::Audio;
