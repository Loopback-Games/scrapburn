//! TrueType text: a few faces rasterized once into one R8 atlas, with metrics for layout.

const PAD: usize = 2;
const ATLAS_W: usize = 2048;
const ATLAS_H: usize = 1024;
/// Glyph tables cover Latin-1 so a face can carry `°` and `×` next to ASCII.
const TABLE: usize = 256;

/// The text faces the overlay can draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    /// Race HUD text (Rajdhani Bold).
    Hud,
    /// Menu rows and body text (Chakra Petch SemiBold).
    Row,
    /// Large stencil headlines (Big Shoulders Stencil Display Black), capitals only.
    Display,
}

/// Which characters of a font to rasterize.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphSet {
    /// Printable ASCII.
    Ascii,
    /// Printable ASCII plus `°` and `×`.
    AsciiExtras,
    /// Uppercase, digits, and punctuation; lowercase draws as uppercase.
    Caps,
}

impl GlyphSet {
    fn chars(self) -> Vec<char> {
        let ascii = (32u8..127).map(char::from);
        match self {
            Self::Ascii => ascii.collect(),
            Self::AsciiExtras => ascii.chain(['\u{b0}', '\u{d7}']).collect(),
            Self::Caps => ascii.filter(|c| !c.is_ascii_lowercase()).collect(),
        }
    }
}

/// One font to pack: its bytes, the pixel size to rasterize at, and the characters wanted.
#[derive(Debug, Clone, Copy)]
pub struct FaceSpec<'a> {
    pub ttf: &'a [u8],
    pub raster_px: f32,
    pub glyphs: GlyphSet,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    /// Atlas UV rectangle: u0, v0, u1, v1.
    pub uv: [f32; 4],
    /// Bitmap size at the face's raster size (pixels).
    pub size: [f32; 2],
    /// Offset from the pen position to the bitmap's top-left, at the raster size (y down).
    pub offset: [f32; 2],
    pub advance: f32,
}

/// Letter spacing and case for a run of text.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Tracking {
    /// Extra pixels between neighboring letters.
    pub px: f32,
    /// Set ASCII lowercase as capitals.
    pub caps: bool,
}

impl Tracking {
    /// The character drawn for `c`.
    fn map(self, c: char) -> char {
        if self.caps { c.to_ascii_uppercase() } else { c }
    }

    /// The extra width tracking adds to `n` letters.
    fn spacing(self, n: usize) -> f32 {
        self.px * n.saturating_sub(1) as f32
    }
}

/// Advance widths for measuring text without the atlas.
#[derive(Debug, Clone)]
pub struct FontMetrics {
    advances: Vec<f32>,
    raster_px: f32,
}

impl FontMetrics {
    pub fn text_width(&self, s: &str, px: f32) -> f32 {
        self.text_width_tracked(s, px, Tracking::default())
    }

    /// `text_width` with letter spacing and case.
    pub fn text_width_tracked(&self, s: &str, px: f32, tracking: Tracking) -> f32 {
        let q = self.advances.get('?' as usize).copied().unwrap_or(0.0);
        let mut n = 0;
        let advance = s
            .chars()
            .map(|c| {
                n += 1;
                self.advances
                    .get(tracking.map(c) as usize)
                    .copied()
                    .filter(|a| *a >= 0.0)
                    .unwrap_or(q)
            })
            .sum::<f32>();
        advance * px / self.raster_px + tracking.spacing(n)
    }
}

/// One quad to draw: screen rect (x, y, w, h) and atlas UVs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphQuad {
    pub rect: [f32; 4],
    pub uv: [f32; 4],
}

struct FaceData {
    raster_px: f32,
    glyphs: Vec<Option<Glyph>>,
    /// Ascent above the baseline at `raster_px`.
    ascent: f32,
}

pub struct FontAtlas {
    pub width: u32,
    pub height: u32,
    /// R8 coverage.
    pub pixels: Vec<u8>,
    faces: Vec<FaceData>,
}

impl FontAtlas {
    /// Packs every face into one atlas; face `i` is addressed by index `i` afterwards.
    pub fn new(specs: &[FaceSpec<'_>]) -> Result<Self, String> {
        let (width, height) = (ATLAS_W, ATLAS_H);
        let mut pixels = vec![0u8; width * height];
        let (mut x, mut y, mut row_h) = (PAD, PAD, 0usize);
        let mut faces = Vec::with_capacity(specs.len());
        for spec in specs {
            let font = fontdue::Font::from_bytes(spec.ttf, fontdue::FontSettings::default())
                .map_err(|e| e.to_string())?;
            let px = spec.raster_px;
            let ascent = font
                .horizontal_line_metrics(px)
                .map_or(px * 0.8, |m| m.ascent);
            let mut glyphs = vec![None; TABLE];
            for c in spec.glyphs.chars() {
                let (m, bitmap) = font.rasterize(c, px);
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
            faces.push(FaceData {
                raster_px: px,
                glyphs,
                ascent,
            });
        }
        Ok(Self {
            width: width as u32,
            height: height as u32,
            pixels,
            faces,
        })
    }

    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    /// Advance widths only, cheap to share.
    pub fn metrics(&self, face: usize) -> Option<FontMetrics> {
        let f = self.faces.get(face)?;
        let mut advances: Vec<f32> = f
            .glyphs
            .iter()
            .map(|g| g.map_or(-1.0, |g| g.advance))
            .collect();
        // Capitals-only faces draw lowercase as uppercase, so measure it that way too.
        for c in b'a'..=b'z' {
            if advances[c as usize] < 0.0 {
                advances[c as usize] = advances[c.to_ascii_uppercase() as usize];
            }
        }
        Some(FontMetrics {
            advances,
            raster_px: f.raster_px,
        })
    }

    /// Top of the capitals below a layout's `y`, and their height, at `px`.
    pub fn cap_span(&self, face: usize, px: f32) -> (f32, f32) {
        let Some(f) = self.faces.get(face) else {
            return (0.0, px);
        };
        let k = px / f.raster_px;
        match self.glyph(face, 'H') {
            Some(h) => ((f.ascent + h.offset[1]) * k, h.size[1] * k),
            None => (f.ascent * 0.1 * k, f.ascent * 0.65 * k),
        }
    }

    /// Distance from a layout's `y` to the vertical middle of its capital letters, at `px`.
    pub fn cap_middle(&self, face: usize, px: f32) -> f32 {
        let (top, height) = self.cap_span(face, px);
        top + height * 0.5
    }

    pub fn glyph(&self, face: usize, c: char) -> Option<Glyph> {
        let f = self.faces.get(face)?;
        let get = |c: char| f.glyphs.get(c as usize).copied().flatten();
        get(c).or_else(|| get(c.to_ascii_uppercase()))
    }

    /// Advance width of `s` at font size `px`.
    pub fn text_width(&self, face: usize, s: &str, px: f32) -> f32 {
        let Some(f) = self.faces.get(face) else {
            return 0.0;
        };
        let k = px / f.raster_px;
        s.chars()
            .map(|c| {
                self.glyph(face, c)
                    .or_else(|| self.glyph(face, '?'))
                    .map_or(0.0, |g| g.advance)
            })
            .sum::<f32>()
            * k
    }

    /// Quads for `s` with its top (ascent line) at `y`.
    pub fn layout(&self, face: usize, s: &str, x: f32, y: f32, px: f32) -> Vec<GlyphQuad> {
        let mut out = Vec::with_capacity(s.len());
        self.layout_with(face, s, x, y, px, Tracking::default(), |q| out.push(q));
        out
    }

    /// Calls `emit` with each glyph quad of `s`, its top (ascent line) at `y` and `tracking`
    /// pixels between letters, without allocating.
    #[allow(clippy::too_many_arguments)]
    pub fn layout_with(
        &self,
        face: usize,
        s: &str,
        x: f32,
        y: f32,
        px: f32,
        tracking: Tracking,
        mut emit: impl FnMut(GlyphQuad),
    ) {
        let Some(f) = self.faces.get(face) else {
            return;
        };
        let k = px / f.raster_px;
        let baseline = y + f.ascent * k;
        let mut pen = x;
        for c in s.chars() {
            let c = tracking.map(c);
            let Some(g) = self.glyph(face, c).or_else(|| self.glyph(face, '?')) else {
                continue;
            };
            if g.size[0] > 0.0 && g.size[1] > 0.0 {
                emit(GlyphQuad {
                    rect: [
                        pen + g.offset[0] * k,
                        baseline + g.offset[1] * k,
                        g.size[0] * k,
                        g.size[1] * k,
                    ],
                    uv: g.uv,
                });
            }
            pen += g.advance * k + tracking.px;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font(name: &str) -> Vec<u8> {
        let path = format!("{}/../../assets/fonts/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read(path).expect("font in assets")
    }

    fn specs<'a>(hud: &'a [u8], row: &'a [u8], display: &'a [u8]) -> [FaceSpec<'a>; 3] {
        [
            FaceSpec {
                ttf: hud,
                raster_px: 64.0,
                glyphs: GlyphSet::Ascii,
            },
            FaceSpec {
                ttf: row,
                raster_px: 64.0,
                glyphs: GlyphSet::AsciiExtras,
            },
            FaceSpec {
                ttf: display,
                raster_px: 176.0,
                glyphs: GlyphSet::Caps,
            },
        ]
    }

    fn atlas() -> FontAtlas {
        let (h, r, d) = (
            font("Rajdhani-Bold.ttf"),
            font("ChakraPetch-SemiBold.ttf"),
            font("BigShouldersStencilDisplay-Black.ttf"),
        );
        FontAtlas::new(&specs(&h, &r, &d)).expect("parses")
    }

    #[test]
    fn rasterizes_printable_ascii_into_the_atlas() {
        let a = atlas();
        assert!(a.width.is_power_of_two() && a.height.is_power_of_two());
        let g = a.glyph(0, 'A').expect("A");
        assert!(g.size[0] > 0.0 && g.size[1] > 0.0);
        assert!(a.pixels.iter().any(|&p| p > 200), "ink in the atlas");
        assert!(a.glyph(0, '\u{e000}').is_none());
    }

    #[test]
    fn three_faces_fit_without_overlapping() {
        let a = atlas();
        assert_eq!((a.width, a.height), (2048, 1024));
        assert_eq!(a.face_count(), 3);
        let mut seen = Vec::new();
        for face in 0..3 {
            let g = a.glyph(face, 'A').expect("A in every face");
            assert!(g.uv[2] <= 1.0 && g.uv[3] <= 1.0 && g.size[0] > 0.0);
            seen.push(g.uv);
        }
        seen.dedup();
        assert_eq!(seen.len(), 3, "each face has its own atlas cell");
        // The display face is rasterized larger than the text faces.
        let (h, d) = (a.glyph(0, 'H').unwrap(), a.glyph(2, 'H').unwrap());
        assert!(d.size[1] > 2.0 * h.size[1]);
    }

    #[test]
    fn display_face_is_capitals_only_but_reads_lowercase() {
        let a = atlas();
        assert!(a.glyph(2, '7').is_some() && a.glyph(2, '!').is_some());
        assert_eq!(a.glyph(2, 'q'), a.glyph(2, 'Q'));
        assert_eq!(
            a.text_width(2, "race", 100.0),
            a.text_width(2, "RACE", 100.0)
        );
        let m = a.metrics(2).expect("metrics");
        assert!((m.text_width("race", 100.0) - m.text_width("RACE", 100.0)).abs() < 1e-3);
    }

    #[test]
    fn row_face_carries_degree_and_times() {
        let a = atlas();
        assert!(a.glyph(1, '\u{b0}').is_some());
        assert!(a.glyph(1, '\u{d7}').is_some());
    }

    #[test]
    fn atlas_full_error_still_fires() {
        let d = font("BigShouldersStencilDisplay-Black.ttf");
        let huge = FaceSpec {
            ttf: &d,
            raster_px: 900.0,
            glyphs: GlyphSet::Caps,
        };
        match FontAtlas::new(&[huge]) {
            Err(e) => assert_eq!(e, "font atlas is full"),
            Ok(_) => panic!("should not fit"),
        }
    }

    #[test]
    fn widths_scale_and_differ_by_glyph() {
        let a = atlas();
        assert!(a.text_width(0, "WW", 32.0) > a.text_width(0, "ii", 32.0));
        let w1 = a.text_width(0, "LAP 1/3", 20.0);
        let w2 = a.text_width(0, "LAP 1/3", 40.0);
        assert!((w2 / w1 - 2.0).abs() < 0.02);
    }

    #[test]
    fn cap_middle_centers_capitals_on_a_line() {
        let a = atlas();
        for face in 0..3 {
            let px = 40.0;
            let y = 100.0;
            let h = a.layout(face, "H", 0.0, y, px)[0].rect;
            let mid = h[1] + h[3] * 0.5;
            assert!(
                (y + a.cap_middle(face, px) - mid).abs() < 0.5,
                "face {face}: cap middle {} vs glyph {mid}",
                y + a.cap_middle(face, px)
            );
        }
    }

    #[test]
    fn tracking_spaces_letters_and_sets_capitals() {
        let a = atlas();
        let plain = a.layout(1, "AVA", 10.0, 20.0, 40.0);
        let mut spaced = Vec::new();
        let t = Tracking {
            px: 6.0,
            caps: false,
        };
        a.layout_with(1, "AVA", 10.0, 20.0, 40.0, t, |q| spaced.push(q));
        assert_eq!(spaced.len(), 3);
        for (i, (p, q)) in plain.iter().zip(&spaced).enumerate() {
            assert!((q.rect[0] - p.rect[0] - 6.0 * i as f32).abs() < 1e-3);
            assert_eq!((&p.rect[1..], p.uv), (&q.rect[1..], q.uv));
        }
        let mut caps = Vec::new();
        let c = Tracking {
            px: 0.0,
            caps: true,
        };
        a.layout_with(1, "ava", 10.0, 20.0, 40.0, c, |q| caps.push(q));
        assert_eq!(caps, plain);
        let m = a.metrics(1).expect("metrics");
        let w = m.text_width("AVA", 40.0);
        let spaced_w = m.text_width_tracked(
            "ava",
            40.0,
            Tracking {
                px: 6.0,
                caps: true,
            },
        );
        assert!((spaced_w - (w + 12.0)).abs() < 1e-3);
        assert_eq!(m.text_width_tracked("", 40.0, t), 0.0);
    }

    #[test]
    fn layout_produces_quads_inside_the_atlas() {
        let a = atlas();
        let quads = a.layout(0, "GO!", 10.0, 20.0, 48.0);
        assert_eq!(quads.len(), 3);
        for q in &quads {
            assert!(q.uv[0] >= 0.0 && q.uv[2] <= 1.0 && q.uv[1] >= 0.0 && q.uv[3] <= 1.0);
            assert!(q.rect[2] > 0.0 && q.rect[3] > 0.0);
        }
    }
}
