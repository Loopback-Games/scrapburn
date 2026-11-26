//! Picks the graphics preset automatically: start at the highest, watch frame times on the
//! title screen, and step down until the game runs comfortably above 60 fps.

use rivet_render::Preset;

/// Frames ignored after a (re)start while shaders and caches warm up.
const WARMUP_FRAMES: usize = 90;
/// Frames measured per decision.
const WINDOW: usize = 180;
/// Step down when the 90th-percentile frame is slower than this (ms): about 80 fps, which
/// leaves headroom for a busy race at 60.
const BUDGET_MS: f32 = 12.5;

/// Decides the preset from measured frame times.
#[derive(Debug, Clone)]
pub struct AutoTuner {
    preset: Preset,
    seen: usize,
    samples: Vec<f32>,
    done: bool,
}

impl Default for AutoTuner {
    fn default() -> Self {
        Self::new()
    }
}

impl AutoTuner {
    /// Starts at the highest preset.
    pub fn new() -> Self {
        Self {
            preset: Preset::High,
            seen: 0,
            samples: Vec::with_capacity(WINDOW),
            done: false,
        }
    }

    pub fn preset(&self) -> Preset {
        self.preset
    }

    /// Still measuring.
    pub fn active(&self) -> bool {
        !self.done
    }

    /// Stops adapting (the player picked a preset).
    pub fn stop(&mut self) {
        self.done = true;
    }

    /// Feeds one frame's GPU (or, without a GPU timer, CPU) time. Returns a new preset when it
    /// steps down.
    pub fn feed(&mut self, frame_ms: f32) -> Option<Preset> {
        if self.done || !frame_ms.is_finite() {
            return None;
        }
        self.seen += 1;
        if self.seen <= WARMUP_FRAMES {
            return None;
        }
        self.samples.push(frame_ms);
        if self.samples.len() < WINDOW {
            return None;
        }
        let mut s = std::mem::take(&mut self.samples);
        s.sort_by(f32::total_cmp);
        let p90 = s[s.len() * 9 / 10];
        if p90 <= BUDGET_MS {
            self.done = true;
            return None;
        }
        let lower = match self.preset {
            Preset::High => Preset::Medium,
            Preset::Medium => Preset::Low,
            Preset::Low => {
                self.done = true;
                return None;
            }
        };
        self.preset = lower;
        self.seen = 0;
        Some(lower)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(t: &mut AutoTuner, ms: f32, frames: usize) -> Vec<Preset> {
        (0..frames).filter_map(|_| t.feed(ms)).collect()
    }

    #[test]
    fn a_fast_machine_keeps_high() {
        let mut t = AutoTuner::new();
        assert_eq!(run(&mut t, 7.0, 1000), vec![]);
        assert_eq!(t.preset(), Preset::High);
        assert!(!t.active(), "settled");
    }

    #[test]
    fn a_slow_machine_steps_down_until_it_fits() {
        let mut t = AutoTuner::new();
        // Too slow on High, still too slow on Medium.
        assert_eq!(
            run(&mut t, 20.0, WARMUP_FRAMES + WINDOW),
            vec![Preset::Medium]
        );
        assert_eq!(run(&mut t, 15.0, WARMUP_FRAMES + WINDOW), vec![Preset::Low]);
        // Fine on Low: settles there.
        assert_eq!(run(&mut t, 9.0, WARMUP_FRAMES + WINDOW + 10), vec![]);
        assert_eq!(t.preset(), Preset::Low);
        assert!(!t.active());
    }

    #[test]
    fn warmup_hitches_and_rare_spikes_are_ignored() {
        let mut t = AutoTuner::new();
        assert_eq!(run(&mut t, 50.0, WARMUP_FRAMES), vec![], "shader warmup");
        // One spike in twenty frames: the 90th percentile stays fast.
        let got: Vec<Preset> = (0..WINDOW)
            .filter_map(|k| t.feed(if k % 20 == 0 { 40.0 } else { 8.0 }))
            .collect();
        assert!(got.is_empty());
        assert_eq!(t.preset(), Preset::High);
    }

    #[test]
    fn stopping_freezes_the_choice() {
        let mut t = AutoTuner::new();
        t.stop();
        assert_eq!(run(&mut t, 30.0, 2000), vec![]);
        assert_eq!(t.preset(), Preset::High);
    }
}
