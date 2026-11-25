//! Benchmark mode: warm up, record every frame, report one summary line and a CSV.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use rivet_core::FrameStats;

pub const WARMUP_S: f64 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchPhase {
    Warmup,
    Running,
    Done,
}

#[derive(Debug, Clone)]
pub struct Bench {
    duration_s: f64,
    stats: FrameStats,
}

impl Bench {
    pub fn new(duration_s: u32) -> Self {
        // Keep every measured frame, however fast the GPU is.
        Self {
            duration_s: duration_s as f64,
            stats: FrameStats::new(usize::MAX),
        }
    }

    /// Records one frame at `elapsed_s` since launch.
    pub fn record(&mut self, elapsed_s: f64, cpu_ms: f32, gpu_ms: Option<f32>) -> BenchPhase {
        if elapsed_s < WARMUP_S {
            BenchPhase::Warmup
        } else if elapsed_s < WARMUP_S + self.duration_s {
            self.stats.push(cpu_ms, gpu_ms);
            BenchPhase::Running
        } else {
            BenchPhase::Done
        }
    }

    pub fn stats(&self) -> &FrameStats {
        &self.stats
    }

    pub fn line(&self, adapter: &str, backend: &str, size: (u32, u32), present: &str) -> String {
        let s = self.stats.summary();
        let gpu = s
            .gpu_avg_ms
            .map_or_else(|| "na".to_string(), |g| format!("{g:.2}"));
        format!(
            "BENCH adapter=\"{adapter}\" backend={backend} size={}x{} present={present} frames={} avg_ms={:.2} p50_ms={:.2} p99_ms={:.2} max_ms={:.2} fps={:.1} gpu_avg_ms={gpu}",
            size.0,
            size.1,
            self.stats.len(),
            s.avg_ms,
            s.p50_ms,
            s.p99_ms,
            s.max_ms,
            s.fps,
        )
    }

    /// Writes `cpu_ms,gpu_ms` per frame into `dir/<unix_time>.csv`.
    pub fn write_csv(&self, dir: &Path) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(dir)?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let path = dir.join(format!("{stamp}.csv"));
        let mut out = String::from("cpu_ms,gpu_ms\n");
        for s in self.stats.samples() {
            let _ = writeln!(
                out,
                "{:.3},{}",
                s.cpu_ms,
                s.gpu_ms.map_or(String::new(), |g| format!("{g:.3}"))
            );
        }
        std::fs::write(&path, out)?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bench_phases() {
        let mut b = Bench::new(2);
        assert_eq!(b.record(1.0, 5.0, None), BenchPhase::Warmup);
        assert_eq!(b.record(3.5, 5.0, Some(2.0)), BenchPhase::Running);
        assert_eq!(b.record(4.9, 7.0, Some(2.0)), BenchPhase::Running);
        assert_eq!(b.record(5.1, 6.0, None), BenchPhase::Done);
        assert_eq!(b.stats().len(), 2);
    }

    #[test]
    fn bench_keeps_every_frame() {
        let mut b = Bench::new(1);
        for i in 0..30_000 {
            b.record(3.0 + i as f64 * 1e-5, 0.3, None);
        }
        assert_eq!(b.stats().len(), 30_000);
    }

    #[test]
    fn bench_line_format() {
        let mut b = Bench::new(30);
        b.record(4.0, 10.0, Some(4.0));
        b.record(5.0, 20.0, Some(6.0));
        let line = b.line(
            "Intel(R) UHD Graphics 730",
            "Vulkan",
            (1600, 900),
            "Immediate",
        );
        assert!(line.starts_with("BENCH adapter=\"Intel(R) UHD Graphics 730\" backend=Vulkan size=1600x900 present=Immediate frames=2 "), "{line}");
        assert!(
            line.contains("avg_ms=15.00")
                && line.contains("p99_ms=20.00")
                && line.contains("gpu_avg_ms=5.00"),
            "{line}"
        );
    }
}
