#![forbid(unsafe_code)]
//! Rivet core: the fixed-tick main loop clock and frame statistics.

pub mod frame_stats;
pub mod timestep;

pub use frame_stats::{FrameSample, FrameStats, StatsSummary};
pub use timestep::FixedTimestep;
