//! Ring buffer of per-frame CPU/GPU times with summary statistics.

use std::collections::VecDeque;

/// One frame's timings in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameSample {
    pub cpu_ms: f32,
    pub gpu_ms: Option<f32>,
}

/// Summary over the samples currently held. Percentiles use nearest rank on CPU (wall) time.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StatsSummary {
    pub avg_ms: f32,
    pub p50_ms: f32,
    pub p99_ms: f32,
    pub max_ms: f32,
    pub fps: f32,
    /// Average GPU time over samples that have one.
    pub gpu_avg_ms: Option<f32>,
}

/// Fixed-capacity ring buffer of frame samples; the oldest sample is dropped when full.
#[derive(Debug, Clone)]
pub struct FrameStats {
    capacity: usize,
    samples: VecDeque<FrameSample>,
}

impl FrameStats {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be positive");
        Self {
            capacity,
            samples: VecDeque::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, cpu_ms: f32, gpu_ms: Option<f32>) {
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(FrameSample { cpu_ms, gpu_ms });
    }

    /// Samples oldest first.
    pub fn samples(&self) -> impl Iterator<Item = FrameSample> + '_ {
        self.samples.iter().copied()
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn summary(&self) -> StatsSummary {
        if self.samples.is_empty() {
            return StatsSummary::default();
        }
        let mut cpu: Vec<f32> = self.samples.iter().map(|s| s.cpu_ms).collect();
        cpu.sort_by(f32::total_cmp);
        let n = cpu.len();
        let rank = |p: f64| cpu[((p * n as f64).ceil() as usize).clamp(1, n) - 1];
        let avg = (cpu.iter().map(|&v| v as f64).sum::<f64>() / n as f64) as f32;
        let gpu: Vec<f32> = self.samples.iter().filter_map(|s| s.gpu_ms).collect();
        let gpu_avg_ms = (!gpu.is_empty())
            .then(|| (gpu.iter().map(|&v| v as f64).sum::<f64>() / gpu.len() as f64) as f32);
        StatsSummary {
            avg_ms: avg,
            p50_ms: rank(0.50),
            p99_ms: rank(0.99),
            max_ms: cpu[n - 1],
            fps: if avg > 0.0 { 1000.0 / avg } else { 0.0 },
            gpu_avg_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_percentiles() {
        let mut s = FrameStats::new(240);
        for i in 1..=100 {
            s.push(i as f32, None);
        }
        let sum = s.summary();
        assert_eq!(sum.p50_ms, 50.0);
        assert_eq!(sum.p99_ms, 99.0);
        assert_eq!(sum.max_ms, 100.0);
        assert!((sum.avg_ms - 50.5).abs() < 1e-4);
        assert!((sum.fps - 1000.0 / 50.5).abs() < 1e-3);
    }

    #[test]
    fn stats_ring_overwrites() {
        let mut s = FrameStats::new(4);
        for i in 1..=5 {
            s.push(i as f32, Some(1.0));
        }
        assert_eq!(s.samples().count(), 4);
        assert_eq!(s.samples().next().unwrap().cpu_ms, 2.0);
    }
}
