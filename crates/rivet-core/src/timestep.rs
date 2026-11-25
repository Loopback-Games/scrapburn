//! Fixed-timestep accumulator: the sim ticks at a fixed rate, rendering interpolates.

/// Accumulates real frame time and reports how many fixed sim steps to run.
#[derive(Debug, Clone)]
pub struct FixedTimestep {
    /// Fixed step length in seconds.
    pub dt: f64,
    /// Upper bound on steps per frame; time beyond it is dropped (no spiral of death).
    pub max_steps: u32,
    accumulator: f64,
}

impl FixedTimestep {
    pub fn new(dt: f64, max_steps: u32) -> Self {
        assert!(dt > 0.0, "dt must be positive");
        Self {
            dt,
            max_steps,
            accumulator: 0.0,
        }
    }

    /// Adds `frame_dt` seconds and returns the number of sim steps to run now.
    pub fn advance(&mut self, frame_dt: f64) -> u32 {
        self.accumulator += frame_dt.max(0.0);
        let mut steps = 0;
        while self.accumulator >= self.dt && steps < self.max_steps {
            self.accumulator -= self.dt;
            steps += 1;
        }
        if steps == self.max_steps && self.accumulator >= self.dt {
            // Keep only the fractional remainder so interpolation stays valid.
            self.accumulator %= self.dt;
        }
        steps
    }

    /// Interpolation factor between the previous and current sim state, in `[0, 1)`.
    pub fn alpha(&self) -> f32 {
        (self.accumulator / self.dt) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advance_accumulates() {
        let mut ts = FixedTimestep::new(1.0 / 60.0, 5);
        assert_eq!(ts.advance(0.010), 0);
        assert_eq!(ts.advance(0.010), 1);
        let expected = ((0.020 - 1.0 / 60.0) / (1.0 / 60.0)) as f32;
        assert!(
            (ts.alpha() - expected).abs() < 1e-6 * expected.max(1.0),
            "{}",
            ts.alpha()
        );
    }

    #[test]
    fn advance_clamps_hitch() {
        let mut ts = FixedTimestep::new(1.0 / 60.0, 5);
        assert_eq!(ts.advance(2.0), 5);
        assert!(ts.alpha() < 1.0);
        assert_eq!(ts.advance(0.0), 0);
    }
}
