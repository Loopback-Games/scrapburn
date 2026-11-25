//! Tileable value-noise fBm, generated once on the CPU (cloud texture).

fn hash(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x.wrapping_mul(374_761_393)
        ^ y.wrapping_mul(668_265_263)
        ^ seed.wrapping_mul(2_147_483_647);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65_535.0
}

/// `size`×`size` grayscale fBm that wraps at the edges. `octaves` doublings from a 4-cell base.
pub fn tileable_fbm(size: usize, octaves: u32, seed: u32) -> Vec<u8> {
    let mut acc = vec![0.0f32; size * size];
    let mut amp = 1.0;
    for o in 0..octaves {
        let cells = 4u32 << o; // lattice cells across the image; wraps modulo `cells`
        for y in 0..size {
            for x in 0..size {
                let fx = x as f32 / size as f32 * cells as f32;
                let fy = y as f32 / size as f32 * cells as f32;
                let (x0, y0) = (fx.floor() as u32, fy.floor() as u32);
                let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
                let s = |t: f32| t * t * (3.0 - 2.0 * t);
                let (sx, sy) = (s(tx), s(ty));
                let g = |i: u32, j: u32| hash(i % cells, j % cells, seed + o);
                let a = g(x0, y0) + (g(x0 + 1, y0) - g(x0, y0)) * sx;
                let b = g(x0, y0 + 1) + (g(x0 + 1, y0 + 1) - g(x0, y0 + 1)) * sx;
                acc[y * size + x] += (a + (b - a) * sy) * amp;
            }
        }
        amp *= 0.5;
    }
    let (lo, hi) = acc
        .iter()
        .fold((f32::MAX, f32::MIN), |(l, h), &v| (l.min(v), h.max(v)));
    acc.iter()
        .map(|&v| (((v - lo) / (hi - lo).max(1e-6)) * 255.0 + 0.5) as u8)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_deterministic_and_in_range() {
        let a = tileable_fbm(64, 4, 7);
        assert_eq!(a, tileable_fbm(64, 4, 7));
        assert_eq!(a.len(), 64 * 64);
        let (lo, hi) = (*a.iter().min().unwrap(), *a.iter().max().unwrap());
        assert!(hi - lo > 100, "too flat: {lo}..{hi}");
    }

    #[test]
    fn noise_tiles_seamlessly() {
        let n = 64;
        let a = tileable_fbm(n, 4, 7);
        // Across the wrap seam, neighbors differ no more than typical in-image neighbors.
        let seam: f32 = (0..n)
            .map(|y| (a[y * n] as f32 - a[y * n + n - 1] as f32).abs())
            .sum::<f32>()
            / n as f32;
        let inner: f32 = (0..n)
            .map(|y| (a[y * n + 10] as f32 - a[y * n + 11] as f32).abs())
            .sum::<f32>()
            / n as f32;
        assert!(seam < inner * 3.0 + 2.0, "seam {seam} vs inner {inner}");
    }
}
