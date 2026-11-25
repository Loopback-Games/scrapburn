//! RGBA8 images: decoding from disk and sRGB-correct mipmap generation.

use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error("loading image {path}: {source}")]
    Image {
        path: String,
        source: image::ImageError,
    },
}

/// Tightly packed RGBA8, rows top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba8Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

pub fn load_rgba(path: &Path) -> Result<Rgba8Image, AssetError> {
    let img = image::open(path)
        .map_err(|source| AssetError::Image {
            path: path.display().to_string(),
            source,
        })?
        .into_rgba8();
    Ok(Rgba8Image {
        width: img.width(),
        height: img.height(),
        pixels: img.into_raw(),
    })
}

fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(l: f32) -> u8 {
    let l = l.clamp(0.0, 1.0);
    let c = if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    };
    (c * 255.0 + 0.5) as u8
}

/// The full mip chain down to 1×1 (level 0 is a copy of `base`). Color channels are averaged in
/// linear space so mips don't darken; alpha is averaged directly.
pub fn mip_chain(base: &Rgba8Image) -> Vec<Rgba8Image> {
    let to_lin: Vec<f32> = (0..=255u8).map(srgb_to_linear).collect();
    let mut chain = vec![base.clone()];
    while let Some(prev) = chain.last() {
        if prev.width == 1 && prev.height == 1 {
            break;
        }
        let (w, h) = ((prev.width / 2).max(1), (prev.height / 2).max(1));
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let mut acc = [0.0f32; 4];
                let mut n = 0.0;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let sx = (x * 2 + dx).min(prev.width - 1);
                    let sy = (y * 2 + dy).min(prev.height - 1);
                    let i = ((sy * prev.width + sx) * 4) as usize;
                    for c in 0..3 {
                        acc[c] += to_lin[prev.pixels[i + c] as usize];
                    }
                    acc[3] += prev.pixels[i + 3] as f32;
                    n += 1.0;
                }
                let o = ((y * w + x) * 4) as usize;
                for c in 0..3 {
                    pixels[o + c] = linear_to_srgb(acc[c] / n);
                }
                pixels[o + 3] = (acc[3] / n + 0.5) as u8;
            }
        }
        chain.push(Rgba8Image {
            width: w,
            height: h,
            pixels,
        });
    }
    chain
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> Rgba8Image {
        Rgba8Image {
            width: w,
            height: h,
            pixels: rgba.repeat((w * h) as usize),
        }
    }

    #[test]
    fn mip_chain_sizes() {
        let sizes: Vec<_> = mip_chain(&solid(8, 4, [0; 4]))
            .iter()
            .map(|m| (m.width, m.height))
            .collect();
        assert_eq!(sizes, vec![(8, 4), (4, 2), (2, 1), (1, 1)]);
    }

    #[test]
    fn mips_average_in_linear_space() {
        // Black and white checker: the linear average is 0.5, which is ~188 in sRGB, not 128.
        let mut img = solid(2, 2, [0, 0, 0, 255]);
        img.pixels[0..4].copy_from_slice(&[255, 255, 255, 255]);
        img.pixels[12..16].copy_from_slice(&[255, 255, 255, 255]);
        let chain = mip_chain(&img);
        let px = &chain[1].pixels;
        assert!((px[0] as i32 - 188).abs() <= 1, "got {}", px[0]);
        assert_eq!(px[3], 255, "alpha averages linearly and stays opaque");
    }

    #[test]
    fn load_missing_file_errors() {
        assert!(load_rgba(std::path::Path::new("/nonexistent/rivet.png")).is_err());
    }
}
