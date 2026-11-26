//! TrueType text: printable ASCII rasterized once into an R8 atlas, with metrics for layout.

/// Pixel size glyphs are rasterized at (drawn smaller with linear filtering).
const RASTER_PX: f32 = 64.0;
const PAD: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    /// Atlas UV rectangle: u0, v0, u1, v1.
    pub uv: [f32; 4],
    /// Bitmap size at `RASTER_PX` (pixels).
    pub size: [f32; 2],
    /// Offset from the pen position to the bitmap's top-left, at `RASTER_PX` (y down).
    pub offset: [f32; 2],
    pub advance: f32,
}

/// Advance widths for measuring text without the atlas.
#[derive(Debug, Clone)]
pub struct FontMetrics {
    advances: Vec<f32>,
}

impl FontMetrics {
    pub fn text_width(&self, s: &str, px: f32) -> f32 {
        let q = self.advances.get('?' as usize).copied().unwrap_or(0.0);
        s.chars()
            .map(|c| {
                self.advances
                    .get(c as usize)
                    .copied()
                    .filter(|a| *a >= 0.0)
                    .unwrap_or(q)
            })
            .sum::<f32>()
            * px
            / RASTER_PX
    }
}

/// One quad to draw: screen rect (x, y, w, h) and atlas UVs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphQuad {
    pub rect: [f32; 4],
    pub uv: [f32; 4],
}

pub struct FontAtlas {
    pub width: u32,
    pub height: u32,
    /// R8 coverage.
    pub pixels: Vec<u8>,
    glyphs: Vec<Option<Glyph>>,
    /// Ascent above the baseline at `RASTER_PX`.
    ascent: f32,
}

impl FontAtlas {
    pub fn new(ttf: &[u8]) -> Result<Self, String> {
        let font = fontdue::Font::from_bytes(ttf, fontdue::FontSettings::default())
            .map_err(|e| e.to_string())?;
        let ascent = font
            .horizontal_line_metrics(RASTER_PX)
            .map_or(RASTER_PX * 0.8, |m| m.ascent);
        let (width, height) = (1024usize, 512usize);
        let mut pixels = vec![0u8; width * height];
        let mut glyphs = vec![None; 128];
        let (mut x, mut y, mut row_h) = (PAD, PAD, 0usize);
        for c in 32u8..127 {
            let (m, bitmap) = font.rasterize(c as char, RASTER_PX);
            if x + m.width + PAD > width {
                x = PAD;
                y += row_h + PAD;
                row_h = 0;
            }
            if y + m.height + PAD > height {
                return Err("font atlas is full".into());
            }
            for row in 0..m.height {
                let dst = (y + row) * width + x;
                pixels[dst..dst + m.width]
                    .copy_from_slice(&bitmap[row * m.width..(row + 1) * m.width]);
            }
            glyphs[c as usize] = Some(Glyph {
                uv: [
                    x as f32 / width as f32,
                    y as f32 / height as f32,
                    (x + m.width) as f32 / width as f32,
                    (y + m.height) as f32 / height as f32,
                ],
                size: [m.width as f32, m.height as f32],
                // fontdue's ymin is the bitmap bottom relative to the baseline (y up).
                offset: [m.xmin as f32, -(m.ymin as f32 + m.height as f32)],
                advance: m.advance_width,
            });
            x += m.width + PAD;
            row_h = row_h.max(m.height);
        }
        Ok(Self {
            width: width as u32,
            height: height as u32,
            pixels,
            glyphs,
            ascent,
        })
    }

    /// Advance widths and ascent only, cheap to share.
    pub fn metrics(&self) -> FontMetrics {
        FontMetrics {
            advances: self
                .glyphs
                .iter()
                .map(|g| g.map_or(-1.0, |g| g.advance))
                .collect(),
        }
    }

    /// Distance from a layout's `y` to the vertical middle of its capital letters, at `px`.
    pub fn cap_middle(&self, px: f32) -> f32 {
        let k = px / RASTER_PX;
        match self.glyph('H') {
            Some(h) => (self.ascent + h.offset[1] + h.size[1] * 0.5) * k,
            None => self.ascent * 0.65 * k,
        }
    }

    pub fn glyph(&self, c: char) -> Option<Glyph> {
        self.glyphs.get(c as usize).copied().flatten()
    }

    /// Advance width of `s` at font size `px`.
    pub fn text_width(&self, s: &str, px: f32) -> f32 {
        let k = px / RASTER_PX;
        s.chars()
            .map(|c| self.glyph(c).or(self.glyph('?')).map_or(0.0, |g| g.advance))
            .sum::<f32>()
            * k
    }

    /// Quads for `s` with its top (ascent line) at `y`.
    pub fn layout(&self, s: &str, x: f32, y: f32, px: f32) -> Vec<GlyphQuad> {
        let k = px / RASTER_PX;
        let baseline = y + self.ascent * k;
        let mut pen = x;
        let mut out = Vec::with_capacity(s.len());
        for c in s.chars() {
            let Some(g) = self.glyph(c).or(self.glyph('?')) else {
                continue;
            };
            if g.size[0] > 0.0 && g.size[1] > 0.0 {
                out.push(GlyphQuad {
                    rect: [
                        pen + g.offset[0] * k,
                        baseline + g.offset[1] * k,
                        g.size[0] * k,
                        g.size[1] * k,
                    ],
                    uv: g.uv,
                });
            }
            pen += g.advance * k;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atlas() -> FontAtlas {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/fonts/Rajdhani-Bold.ttf"
        );
        FontAtlas::new(&std::fs::read(path).expect("font in assets")).expect("parses")
    }

    #[test]
    fn rasterizes_printable_ascii_into_the_atlas() {
        let a = atlas();
        assert!(a.width.is_power_of_two() && a.height.is_power_of_two());
        let g = a.glyph('A').expect("A");
        assert!(g.size[0] > 0.0 && g.size[1] > 0.0);
        assert!(a.pixels.iter().any(|&p| p > 200), "ink in the atlas");
        assert!(a.glyph('\u{e000}').is_none());
    }

    #[test]
    fn widths_scale_and_differ_by_glyph() {
        let a = atlas();
        assert!(a.text_width("WW", 32.0) > a.text_width("ii", 32.0));
        let w1 = a.text_width("LAP 1/3", 20.0);
        let w2 = a.text_width("LAP 1/3", 40.0);
        assert!((w2 / w1 - 2.0).abs() < 0.02);
    }

    #[test]
    fn cap_middle_centers_capitals_on_a_line() {
        let a = atlas();
        let px = 40.0;
        let y = 100.0;
        let h = a.layout("H", 0.0, y, px)[0].rect;
        let mid = h[1] + h[3] * 0.5;
        assert!(
            (y + a.cap_middle(px) - mid).abs() < 0.5,
            "cap middle {} vs glyph {mid}",
            y + a.cap_middle(px)
        );
    }

    #[test]
    fn layout_produces_quads_inside_the_atlas() {
        let a = atlas();
        let quads = a.layout("GO!", 10.0, 20.0, 48.0);
        assert_eq!(quads.len(), 3);
        for q in &quads {
            assert!(q.uv[0] >= 0.0 && q.uv[2] <= 1.0 && q.uv[1] >= 0.0 && q.uv[3] <= 1.0);
            assert!(q.rect[2] > 0.0 && q.rect[3] > 0.0);
        }
    }
}
