#![forbid(unsafe_code)]
//! Rivet AI: racing-line following bots that drift, keep lanes, dodge and respect a skill cap.

pub mod driver;

pub use driver::{BotDriver, RacingLine};
