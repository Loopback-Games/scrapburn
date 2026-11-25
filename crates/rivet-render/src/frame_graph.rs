//! On-screen CPU/GPU frame-time graph.

use rivet_core::{FrameSample, FrameStats};

use crate::overlay::Overlay;

pub const GRAPH_W: f32 = 240.0;
pub const GRAPH_H: f32 = 80.0;
pub const GRAPH_MAX_MS: f32 = 33.3;

const GREEN: [f32; 4] = [0.30, 0.85, 0.40, 0.95];
const AMBER: [f32; 4] = [0.95, 0.70, 0.20, 0.95];
const RED: [f32; 4] = [0.95, 0.25, 0.20, 0.95];
const GPU: [f32; 4] = [0.35, 0.60, 1.00, 0.75];

/// CPU and GPU bar heights in pixels, clamped to `height_px`.
pub fn bar_heights(
    samples: &[FrameSample],
    max_ms: f32,
    height_px: f32,
) -> Vec<(f32, Option<f32>)> {
    let h = |ms: f32| (ms / max_ms).clamp(0.0, 1.0) * height_px;
    samples
        .iter()
        .map(|s| (h(s.cpu_ms), s.gpu_ms.map(h)))
        .collect()
}

/// `"CPU 6.1 GPU 4.3 FPS 162 P99 7.9"`.
pub fn stats_line(cpu_ms: f32, gpu_ms: Option<f32>, fps: f32, p99_ms: f32) -> String {
    let gpu = gpu_ms.map_or_else(|| "--".to_string(), |g| format!("{g:.1}"));
    format!("CPU {cpu_ms:.1} GPU {gpu} FPS {fps:.0} P99 {p99_ms:.1}")
}

fn bar_color(ms: f32) -> [f32; 4] {
    if ms <= 8.3 {
        GREEN
    } else if ms <= 16.7 {
        AMBER
    } else {
        RED
    }
}

/// Draws the graph panel with its top-left corner at `(x, y)`.
pub fn draw_frame_graph(
    overlay: &mut Overlay,
    stats: &FrameStats,
    adapter_name: &str,
    x: f32,
    y: f32,
) {
    let samples: Vec<FrameSample> = stats.samples().collect();
    let summary = stats.summary();
    let last_gpu = samples.iter().rev().find_map(|s| s.gpu_ms);
    let text_scale = 2.0;
    overlay.rect(
        x - 6.0,
        y - 6.0,
        GRAPH_W + 12.0,
        GRAPH_H + 50.0,
        [0.0, 0.0, 0.0, 0.55],
    );
    overlay.text(x, y, text_scale, [1.0; 4], adapter_name);
    let cpu_now = samples.last().map_or(0.0, |s| s.cpu_ms);
    overlay.text(
        x,
        y + 18.0,
        text_scale,
        [1.0; 4],
        &stats_line(cpu_now, last_gpu, summary.fps, summary.p99_ms),
    );
    let gy = y + 38.0;
    let base = gy + GRAPH_H;
    let bar_w = GRAPH_W / stats_capacity_hint(samples.len());
    for (i, (s, (cpu_h, gpu_h))) in samples
        .iter()
        .zip(bar_heights(&samples, GRAPH_MAX_MS, GRAPH_H))
        .enumerate()
    {
        let bx = x + i as f32 * bar_w;
        overlay.rect(bx, base - cpu_h, bar_w.max(1.0), cpu_h, bar_color(s.cpu_ms));
        if let Some(g) = gpu_h {
            overlay.rect(bx, base - g, bar_w.max(1.0), 1.5, GPU);
        }
    }
    for (ms, color) in [(16.7, AMBER), (8.3, GREEN)] {
        let ly = base - ms / GRAPH_MAX_MS * GRAPH_H;
        overlay.rect(x, ly, GRAPH_W, 1.0, [color[0], color[1], color[2], 0.6]);
    }
}

fn stats_capacity_hint(n: usize) -> f32 {
    n.max(GRAPH_W as usize) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use rivet_core::FrameSample;

    #[test]
    fn bar_heights_scale_and_clamp() {
        let samples = [
            FrameSample {
                cpu_ms: 16.65,
                gpu_ms: Some(8.325),
            },
            FrameSample {
                cpu_ms: 100.0,
                gpu_ms: None,
            },
        ];
        let h = bar_heights(&samples, 33.3, 80.0);
        assert!((h[0].0 - 40.0).abs() < 0.1, "{}", h[0].0);
        assert!((h[0].1.unwrap() - 20.0).abs() < 0.1);
        assert_eq!(h[1], (80.0, None));
    }

    #[test]
    fn stats_line_format() {
        let line = stats_line(6.14, Some(4.31), 162.4, 7.94);
        assert_eq!(line, "CPU 6.1 GPU 4.3 FPS 162 P99 7.9");
        assert_eq!(
            stats_line(6.14, None, 162.4, 7.94),
            "CPU 6.1 GPU -- FPS 162 P99 7.9"
        );
    }
}
