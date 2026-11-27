//! Skyway Zero's billboard art: eight original pixel-art posters, drawn in code into one
//! texture layer (an atlas). They're affectionate nods to 90s shooters, platformers and
//! racers (a cheat code, a push wall, a warp zone, a shareware disk) without anyone's
//! names, logos or characters.

use rivet_assets::Rgba8Image;

/// Posters in the atlas, two across and four down.
pub const COUNT: usize = 8;
const COLS: usize = 2;
const ROWS: usize = 4;
/// Each poster is drawn at this resolution, then blown up into chunky pixels.
const W: usize = 128;
const H: usize = 64;

/// Texture coordinates (u0, v0, u1, v1) of poster `i` in the atlas.
pub fn uv_rect(i: usize) -> [f32; 4] {
    let (c, r) = ((i % COUNT) % COLS, (i % COUNT) / COLS);
    let (du, dv) = (1.0 / COLS as f32, 1.0 / ROWS as f32);
    // A texel in from the edges so mipmaps don't bleed in the neighbor.
    let pad = 1.0 / 512.0;
    [
        c as f32 * du + pad,
        r as f32 * dv + pad,
        (c + 1) as f32 * du - pad,
        (r + 1) as f32 * dv - pad,
    ]
}

type Rgb = [u8; 3];

/// A small RGB canvas.
struct Canvas {
    px: Vec<Rgb>,
}

impl Canvas {
    fn new(fill: Rgb) -> Self {
        Self {
            px: vec![fill; W * H],
        }
    }

    fn set(&mut self, x: i32, y: i32, c: Rgb) {
        if (0..W as i32).contains(&x) && (0..H as i32).contains(&y) {
            self.px[y as usize * W + x as usize] = c;
        }
    }

    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgb) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, c);
            }
        }
    }

    /// Vertical gradient over the whole canvas.
    fn gradient(&mut self, top: Rgb, bottom: Rgb) {
        for y in 0..H {
            let t = y as f32 / (H - 1) as f32;
            let c = mix(top, bottom, t);
            for x in 0..W {
                self.px[y * W + x] = c;
            }
        }
    }

    /// A `t`-pixel frame around the edge.
    fn border(&mut self, t: i32, c: Rgb) {
        let (w, h) = (W as i32, H as i32);
        self.rect(0, 0, w, t, c);
        self.rect(0, h - t, w, t, c);
        self.rect(0, 0, t, h, c);
        self.rect(w - t, 0, t, h, c);
    }

    fn disc(&mut self, cx: i32, cy: i32, r: i32, c: Rgb) {
        for y in -r..=r {
            for x in -r..=r {
                if x * x + y * y <= r * r {
                    self.set(cx + x, cy + y, c);
                }
            }
        }
    }

    /// Text in the 5x7 font at `scale`, with a one-pixel drop shadow.
    fn text(&mut self, x: i32, y: i32, s: &str, scale: i32, c: Rgb, shadow: Rgb) {
        self.text_raw(x + 1, y + 1, s, scale, shadow);
        self.text_raw(x, y, s, scale, c);
    }

    fn text_raw(&mut self, x: i32, y: i32, s: &str, scale: i32, c: Rgb) {
        let mut pen = x;
        for ch in s.chars() {
            if let Some(rows) = glyph(ch) {
                for (gy, row) in rows.iter().enumerate() {
                    for (gx, bit) in row.bytes().enumerate() {
                        if bit == b'#' {
                            self.rect(
                                pen + gx as i32 * scale,
                                y + gy as i32 * scale,
                                scale,
                                scale,
                                c,
                            );
                        }
                    }
                }
            }
            pen += 6 * scale;
        }
    }

    /// Text centered horizontally.
    fn centered(&mut self, y: i32, s: &str, scale: i32, c: Rgb, shadow: Rgb) {
        let x = (W as i32 - text_width(s, scale)) / 2;
        self.text(x, y, s, scale, c, shadow);
    }
}

fn text_width(s: &str, scale: i32) -> i32 {
    (s.chars().count() as i32 * 6 - 1) * scale
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    [m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2])]
}

/// A 5x7 pixel font: the characters the posters need.
fn glyph(c: char) -> Option<[&'static str; 7]> {
    Some(match c {
        'A' => [
            ".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#",
        ],
        'B' => [
            "####.", "#...#", "#...#", "####.", "#...#", "#...#", "####.",
        ],
        'C' => [
            ".###.", "#...#", "#....", "#....", "#....", "#...#", ".###.",
        ],
        'D' => [
            "####.", "#...#", "#...#", "#...#", "#...#", "#...#", "####.",
        ],
        'E' => [
            "#####", "#....", "#....", "####.", "#....", "#....", "#####",
        ],
        'F' => [
            "#####", "#....", "#....", "####.", "#....", "#....", "#....",
        ],
        'G' => [
            ".###.", "#...#", "#....", "#.###", "#...#", "#...#", ".####",
        ],
        'H' => [
            "#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#",
        ],
        'I' => [
            ".###.", "..#..", "..#..", "..#..", "..#..", "..#..", ".###.",
        ],
        'J' => [
            "..###", "...#.", "...#.", "...#.", "...#.", "#..#.", ".##..",
        ],
        'K' => [
            "#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#",
        ],
        'L' => [
            "#....", "#....", "#....", "#....", "#....", "#....", "#####",
        ],
        'M' => [
            "#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#",
        ],
        'N' => [
            "#...#", "#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#",
        ],
        'O' => [
            ".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###.",
        ],
        'P' => [
            "####.", "#...#", "#...#", "####.", "#....", "#....", "#....",
        ],
        'Q' => [
            ".###.", "#...#", "#...#", "#...#", "#.#.#", "#..#.", ".##.#",
        ],
        'R' => [
            "####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#",
        ],
        'S' => [
            ".####", "#....", "#....", ".###.", "....#", "....#", "####.",
        ],
        'T' => [
            "#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#..",
        ],
        'U' => [
            "#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###.",
        ],
        'V' => [
            "#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#..",
        ],
        'W' => [
            "#...#", "#...#", "#...#", "#.#.#", "#.#.#", "#.#.#", ".#.#.",
        ],
        'X' => [
            "#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#",
        ],
        'Y' => [
            "#...#", "#...#", ".#.#.", "..#..", "..#..", "..#..", "..#..",
        ],
        'Z' => [
            "#####", "....#", "...#.", "..#..", ".#...", "#....", "#####",
        ],
        '0' => [
            ".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###.",
        ],
        '1' => [
            "..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###.",
        ],
        '2' => [
            ".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####",
        ],
        '3' => [
            "####.", "....#", "....#", ".###.", "....#", "....#", "####.",
        ],
        '4' => [
            "...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#.",
        ],
        '5' => [
            "#####", "#....", "####.", "....#", "....#", "#...#", ".###.",
        ],
        '6' => [
            ".###.", "#....", "#....", "####.", "#...#", "#...#", ".###.",
        ],
        '7' => [
            "#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#...",
        ],
        '8' => [
            ".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###.",
        ],
        '9' => [
            ".###.", "#...#", "#...#", ".####", "....#", "....#", ".###.",
        ],
        '-' => [
            ".....", ".....", ".....", ".###.", ".....", ".....", ".....",
        ],
        '.' => [
            ".....", ".....", ".....", ".....", ".....", ".##..", ".##..",
        ],
        '!' => [
            "..#..", "..#..", "..#..", "..#..", "..#..", ".....", "..#..",
        ],
        '%' => [
            "##...", "##..#", "...#.", "..#..", ".#...", "#..##", "...##",
        ],
        '>' => [
            ".#...", "..#..", "...#.", "....#", "...#.", "..#..", ".#...",
        ],
        ' ' => [
            ".....", ".....", ".....", ".....", ".....", ".....", ".....",
        ],
        _ => return None,
    })
}

const BLACK: Rgb = [8, 6, 14];
const WHITE: Rgb = [250, 248, 240];

/// Arena shooters: a rocket arcing up on its smoke trail.
fn rocket_jump() -> Canvas {
    let mut c = Canvas::new(BLACK);
    c.gradient([92, 34, 18], [24, 10, 8]);
    // The smoke trail, a rising arc of puffs.
    for k in 0..9 {
        let t = k as f32 / 8.0;
        let x = 12.0 + t * 30.0;
        let y = 52.0 - t * t * 34.0;
        let shade = (150.0 - t * 60.0) as u8;
        c.disc(x as i32, y as i32, 2 + (k % 2), [shade, shade, shade]);
    }
    // The rocket: a body, a red nose, fins, a flame.
    c.rect(42, 14, 4, 9, [200, 200, 205]);
    c.rect(42, 11, 4, 3, [230, 40, 30]);
    c.rect(40, 20, 2, 4, [230, 40, 30]);
    c.rect(46, 20, 2, 4, [230, 40, 30]);
    c.rect(43, 23, 2, 3, [255, 200, 40]);
    c.text(54, 10, "ROCKET", 2, [255, 196, 64], BLACK);
    c.text(60, 28, "JUMP", 2, WHITE, BLACK);
    c.text(58, 48, "OPEN FINALS", 1, [255, 120, 60], BLACK);
    c.border(2, [255, 140, 40]);
    c
}

/// Classic shooters: a famous cheat code over pixel hellfire.
fn god_mode() -> Canvas {
    let mut c = Canvas::new(BLACK);
    // Flames: columns of hashed heights, yellow at the root to red at the tips.
    for x in 0..W as i32 {
        let h = 14 + ((x * 37 + (x * x) % 23) % 13);
        for y in 0..h {
            let t = y as f32 / h as f32;
            let col = if t < 0.35 {
                [255, 220, 60]
            } else if t < 0.7 {
                [250, 120, 20]
            } else {
                [180, 24, 12]
            };
            c.set(x, H as i32 - 1 - y, col);
        }
    }
    c.centered(8, "IDDQD", 3, [230, 30, 24], BLACK);
    c.centered(34, "GOD MODE ON", 1, WHITE, BLACK);
    c.border(2, [120, 16, 12]);
    c
}

/// 90s action heroes: the line about bubblegum, and very dark shades.
fn out_of_gum() -> Canvas {
    let mut c = Canvas::new(BLACK);
    c.gradient([255, 214, 40], [255, 120, 30]);
    // Sunglasses.
    let shades = [12, 12, 20];
    c.rect(10, 22, 16, 9, shades);
    c.rect(30, 22, 16, 9, shades);
    c.rect(24, 23, 8, 2, shades);
    c.rect(4, 22, 6, 2, shades);
    c.rect(46, 22, 6, 2, shades);
    c.rect(12, 24, 4, 2, [120, 140, 255]);
    c.rect(32, 24, 4, 2, [120, 140, 255]);
    c.text(58, 10, "ALL OUT", 1, BLACK, [255, 240, 140]);
    c.text(58, 22, "OF", 1, BLACK, [255, 240, 140]);
    c.text(58, 32, "GUM", 3, [200, 20, 90], BLACK);
    c.border(2, [20, 20, 30]);
    c
}

/// Maze shooters: push the odd wall panel for a secret.
fn push_walls() -> Canvas {
    let mut c = Canvas::new(BLACK);
    let (brick, mortar) = ([70, 84, 130], [36, 42, 70]);
    c.rect(0, 0, W as i32, H as i32, mortar);
    for row in 0..8 {
        let off = if row % 2 == 0 { 0 } else { 8 };
        for col in -1..9 {
            c.rect(col * 16 + off + 1, row * 8 + 1, 14, 6, brick);
        }
    }
    // The secret: one panel pushed in, marked with an arrow.
    c.rect(10, 22, 30, 22, [24, 24, 36]);
    c.rect(12, 24, 26, 18, [150, 110, 40]);
    c.text(19, 27, ">", 2, [255, 230, 120], BLACK);
    c.rect(48, 14, 74, 36, [14, 14, 26]);
    c.text(54, 18, "PUSH", 2, WHITE, BLACK);
    c.text(54, 36, "FOR SECRETS", 1, [255, 230, 120], BLACK);
    c.border(2, [180, 190, 230]);
    c
}

/// Platformers: a warp zone, a big green arrow and a couple of clouds.
fn warp_zone() -> Canvas {
    let mut c = Canvas::new(BLACK);
    c.gradient([44, 92, 200], [92, 146, 232]);
    for (x, y) in [(18, 12), (98, 50)] {
        c.disc(x, y, 5, WHITE);
        c.disc(x + 6, y - 2, 6, WHITE);
        c.disc(x + 12, y, 5, WHITE);
    }
    c.centered(14, "WARP ZONE", 2, WHITE, [40, 60, 140]);
    // The arrow.
    let green = [40, 180, 60];
    c.rect(34, 38, 40, 10, green);
    for k in 0..12 {
        c.rect(74 + k, 32 + k, 1, 22 - 2 * k, green);
    }
    c.text(30, 54, "AHEAD", 1, [30, 40, 110], WHITE);
    c.border(2, [30, 40, 110]);
    c
}

/// Hover racers: a wedge-shaped machine streaking through the 26th century.
fn grand_prix() -> Canvas {
    let mut c = Canvas::new(BLACK);
    c.gradient([10, 14, 60], [60, 12, 90]);
    // Speed lines trailing behind the machine.
    for (k, y) in [27, 30, 33, 36, 39, 42, 45].into_iter().enumerate() {
        let x = 84 + (k as i32 * 11) % 17;
        c.rect(x, y, 40 - (k as i32 * 7) % 18, 1, [90, 200, 255]);
    }
    // The machine, nose left: a wedge body, a canopy, thrusters, an underglow.
    for k in 0..10 {
        let left = 30 + (9 - k) * 4;
        c.rect(left, 30 + k, 80 - left, 1, [240, 70, 120]);
    }
    c.rect(56, 30, 12, 3, [120, 230, 255]);
    c.rect(30, 39, 50, 1, [150, 30, 70]);
    c.rect(80, 32, 4, 3, [255, 230, 120]);
    c.rect(80, 36, 4, 3, [255, 160, 60]);
    c.rect(34, 42, 42, 1, [90, 200, 255]);
    c.centered(8, "GRAND PRIX", 2, WHITE, BLACK);
    c.centered(52, "ZERO-G LEAGUE 2560", 1, [255, 230, 120], BLACK);
    c.border(2, [90, 200, 255]);
    c
}

/// The shareware era: a floppy disk with the first episode free.
fn shareware() -> Canvas {
    let mut c = Canvas::new(BLACK);
    c.gradient([20, 60, 70], [8, 24, 30]);
    // The disk: body, metal shutter, label.
    c.rect(10, 12, 36, 40, [30, 30, 36]);
    c.rect(20, 12, 16, 12, [190, 196, 205]);
    c.rect(28, 14, 4, 8, [30, 30, 36]);
    c.rect(14, 32, 28, 16, WHITE);
    c.rect(16, 36, 24, 1, [220, 40, 40]);
    c.rect(16, 40, 20, 1, [40, 40, 200]);
    c.text(54, 12, "SHARE", 2, [120, 255, 200], BLACK);
    c.text(54, 28, "WARE", 2, [120, 255, 200], BLACK);
    c.text(54, 48, "EP. 1 FREE", 1, WHITE, BLACK);
    c.border(2, [120, 255, 200]);
    c
}

/// The arcade: insert coin.
fn insert_coin() -> Canvas {
    let mut c = Canvas::new(BLACK);
    c.disc(22, 32, 13, [200, 150, 20]);
    c.disc(22, 32, 11, [255, 210, 60]);
    c.rect(20, 24, 4, 16, [200, 150, 20]);
    c.text(42, 12, "INSERT", 2, [255, 80, 200], BLACK);
    c.text(42, 30, "COIN", 2, [255, 80, 200], BLACK);
    c.text(44, 50, "PLAYER 1", 1, [120, 220, 255], BLACK);
    c.border(1, [255, 80, 200]);
    c.rect(3, 3, W as i32 - 6, 1, [120, 220, 255]);
    c.rect(3, H as i32 - 4, W as i32 - 6, 1, [120, 220, 255]);
    c
}

fn posters() -> [Canvas; COUNT] {
    [
        rocket_jump(),
        god_mode(),
        out_of_gum(),
        push_walls(),
        warp_zone(),
        grand_prix(),
        shareware(),
        insert_coin(),
    ]
}

/// The atlas as an RGBA image, `size` pixels square (the material layers' size).
pub fn atlas(size: u32) -> Rgba8Image {
    let size = size.max((COLS * W).max(ROWS * H) as u32) as usize;
    let mut pixels = vec![0u8; size * size * 4];
    let (cell_w, cell_h) = (size / COLS, size / ROWS);
    for (i, poster) in posters().iter().enumerate() {
        let (ox, oy) = ((i % COLS) * cell_w, (i / COLS) * cell_h);
        for y in 0..cell_h {
            let sy = y * H / cell_h;
            for x in 0..cell_w {
                let sx = x * W / cell_w;
                let p = poster.px[sy * W + sx];
                let o = ((oy + y) * size + ox + x) * 4;
                pixels[o..o + 4].copy_from_slice(&[p[0], p[1], p[2], 255]);
            }
        }
    }
    Rgba8Image {
        width: size as u32,
        height: size as u32,
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_poster_fits_its_text_and_has_contrast() {
        for (i, p) in posters().iter().enumerate() {
            // Not a flat field: plenty of distinct colors.
            let mut colors: Vec<Rgb> = p.px.clone();
            colors.sort_unstable();
            colors.dedup();
            assert!(colors.len() >= 5, "poster {i} has {} colors", colors.len());
        }
        // Every placed line ends inside the border: (x, text, scale).
        for (x, s, scale) in [
            (54, "ROCKET", 2),
            (58, "OPEN FINALS", 1),
            (58, "GUM", 3),
            (54, "FOR SECRETS", 1),
            (54, "SHARE", 2),
            (54, "EP. 1 FREE", 1),
            (42, "INSERT", 2),
            (44, "PLAYER 1", 1),
        ] {
            assert!(x + text_width(s, scale) <= W as i32 - 2, "{s} overflows");
        }
        assert!(text_width("ZERO-G LEAGUE 2560", 1) <= W as i32 - 4);
        for s in ["IDDQD", "WARP ZONE", "GRAND PRIX"] {
            let scale = if s == "IDDQD" { 3 } else { 2 };
            assert!(text_width(s, scale) <= W as i32 - 4, "{s} overflows");
        }
    }

    #[test]
    fn every_character_used_has_a_glyph() {
        for s in [
            "ROCKET JUMP OPEN FINALS",
            "IDDQD GOD MODE ON",
            "ALL OUT OF GUM",
            "PUSH FOR SECRETS >",
            "WARP ZONE AHEAD",
            "GRAND PRIX ZERO-G LEAGUE 2560",
            "SHAREWARE EP. 1 FREE",
            "INSERT COIN PLAYER 1",
        ] {
            for ch in s.chars() {
                assert!(glyph(ch).is_some(), "no glyph for {ch:?}");
            }
        }
    }

    #[test]
    fn the_atlas_is_layer_sized_and_cells_dont_overlap() {
        let a = atlas(1024);
        assert_eq!((a.width, a.height), (1024, 1024));
        assert_eq!(a.pixels.len(), 1024 * 1024 * 4);
        for i in 0..COUNT {
            let [u0, v0, u1, v1] = uv_rect(i);
            assert!(u0 < u1 && v0 < v1 && u1 <= 1.0 && v1 <= 1.0);
            for j in i + 1..COUNT {
                let [a0, b0, a1, b1] = uv_rect(j);
                let apart = u1 <= a0 || a1 <= u0 || v1 <= b0 || b1 <= v0;
                assert!(apart, "posters {i} and {j} overlap");
            }
        }
    }
}
