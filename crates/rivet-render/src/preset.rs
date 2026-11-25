//! Quality presets. Low targets the UHD 620 minimum spec.

use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Preset {
    /// Minimum spec (UHD 620): 75% render scale, small shadow map.
    Low,
    #[default]
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QualitySettings {
    /// 3D scene resolution relative to the window; the HUD stays native.
    pub render_scale: f32,
    pub shadow_size: u32,
    /// Bloom mip levels (0 disables bloom).
    pub bloom_levels: u32,
    pub fxaa: bool,
    /// Texture mip levels skipped at upload (1 = half resolution).
    pub texture_skip_mips: u32,
    pub anisotropy: u16,
}

impl Preset {
    pub fn settings(self) -> QualitySettings {
        match self {
            Preset::Low => QualitySettings {
                render_scale: 0.75,
                shadow_size: 1024,
                bloom_levels: 4,
                fxaa: true,
                texture_skip_mips: 1,
                anisotropy: 4,
            },
            Preset::Medium => QualitySettings {
                render_scale: 1.0,
                shadow_size: 2048,
                bloom_levels: 5,
                fxaa: true,
                texture_skip_mips: 0,
                anisotropy: 8,
            },
            Preset::High => QualitySettings {
                render_scale: 1.0,
                shadow_size: 4096,
                bloom_levels: 6,
                fxaa: true,
                texture_skip_mips: 0,
                anisotropy: 16,
            },
        }
    }
}

impl FromStr for Preset {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "low" => Ok(Self::Low),
            "medium" | "med" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            other => Err(format!(
                "unknown preset '{other}' (use low, medium or high)"
            )),
        }
    }
}

/// `(w, h)` scaled by `scale`, rounded, at least 1×1.
pub fn scaled_size(w: u32, h: u32, scale: f32) -> (u32, u32) {
    let s = |v: u32| ((v as f32 * scale).round() as u32).max(1);
    (s(w), s(h))
}

/// Sizes of each bloom mip: half of the previous, starting from half of `(w, h)`.
pub fn bloom_sizes(w: u32, h: u32, levels: u32) -> Vec<(u32, u32)> {
    let mut out = Vec::with_capacity(levels as usize);
    let (mut cw, mut ch) = (w, h);
    for _ in 0..levels {
        cw = (cw / 2).max(1);
        ch = (ch / 2).max(1);
        out.push((cw, ch));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_scale_up() {
        let low = Preset::Low.settings();
        let high = Preset::High.settings();
        assert_eq!(low.render_scale, 0.75);
        assert_eq!(Preset::Medium.settings().render_scale, 1.0);
        assert!(low.shadow_size < high.shadow_size);
        assert!(low.bloom_levels <= high.bloom_levels);
        assert_eq!("low".parse::<Preset>().unwrap(), Preset::Low);
        assert!("ultra-mega".parse::<Preset>().is_err());
    }

    #[test]
    fn scaled_size_rounds_and_never_zero() {
        assert_eq!(scaled_size(1920, 1080, 0.75), (1440, 810));
        assert_eq!(scaled_size(1, 1, 0.5), (1, 1));
    }

    #[test]
    fn bloom_sizes_halve() {
        assert_eq!(
            bloom_sizes(1600, 900, 4),
            vec![(800, 450), (400, 225), (200, 112), (100, 56)]
        );
        assert_eq!(bloom_sizes(3, 3, 3), vec![(1, 1), (1, 1), (1, 1)]);
    }
}
