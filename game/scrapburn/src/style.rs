//! Shared look-and-feel constants: drift tier colors, HUD palette, and the menu grade.

use glam::Vec3;
use rivet_render::Lighting;

/// Drift charge colors: none, blue, orange, purple (the genre's readable progression).
pub const TIER_COLORS: [[f32; 4]; 4] = [
    [0.85, 0.85, 0.85, 1.0],
    [0.25, 0.65, 1.0, 1.0],
    [1.0, 0.55, 0.12, 1.0],
    [0.85, 0.3, 1.0, 1.0],
];

/// Color for a drift tier index (0 = not charged yet); saturates at the top tier.
pub fn tier_color(tier: usize) -> [f32; 4] {
    TIER_COLORS[tier.min(TIER_COLORS.len() - 1)]
}

/// A `0xRRGGBB` color with alpha `a`, as overlay color channels.
const fn hex(rgb: u32, a: f32) -> [f32; 4] {
    [
        ((rgb >> 16) & 0xff) as f32 / 255.0,
        ((rgb >> 8) & 0xff) as f32 / 255.0,
        (rgb & 0xff) as f32 / 255.0,
        a,
    ]
}

/// Crucible menu palette (from the design mockup).
pub mod crucible {
    use super::hex;

    /// The smoke over the scene beside a menu (the mockup's tint and vignette), darker toward
    /// the column and the edges so previews read over bright tracks. The scene renderer lays it
    /// on in its composite pass.
    pub const MENU_WASH: rivet_render::Wash = rivet_render::Wash {
        color: [14.0 / 255.0, 8.0 / 255.0, 6.0 / 255.0],
        across: [0.62, 0.32],
        top: (0.45, 0.28),
        bottom: (0.55, 0.28),
        right: (0.4, 0.18),
    };

    /// Near-black warm char: text on the molten selection.
    pub const CHAR: [f32; 4] = hex(0x140d0a, 1.0);
    /// Smoke glass: the column's tint over the blurred scene.
    pub const GLASS: [f32; 4] = hex(0x1a100c, 0.52);
    /// Ash: body text and row labels.
    pub const ASH: [f32; 4] = hex(0xcdbfb2, 1.0);
    /// Dim ash: inactive tabs and footer words.
    pub const ASH_DIM: [f32; 4] = hex(0xcdbfb2, 0.5);
    /// Ember orange: accents, the active tab's underline, preview strokes.
    pub const EMBER: [f32; 4] = hex(0xff6a1a, 1.0);
    /// Gold: kickers and preview captions.
    pub const GOLD: [f32; 4] = hex(0xffc35c, 1.0);
    /// White heat: highlighted text and slider knobs.
    pub const HOT: [f32; 4] = hex(0xfff1d6, 1.0);
    /// The molten gradient, left to right (or bottom to top), with its stop positions.
    pub const MOLTEN: [([f32; 4], f32); 4] = [
        (hex(0xff3a0a, 1.0), 0.0),
        (hex(0xff8a1f, 1.0), 0.35),
        (hex(0xffc35c, 1.0), 0.75),
        (hex(0xfff1d6, 1.0), 1.0),
    ];
    /// The glow around molten shapes.
    pub const GLOW: [f32; 4] = hex(0xff6e1e, 0.55);

    /// The molten gradient's color at `t` (0-1).
    pub fn molten(t: f32) -> [f32; 4] {
        let t = t.clamp(0.0, 1.0);
        let i = MOLTEN.iter().rposition(|(_, at)| *at <= t).unwrap_or(0);
        let Some(&(b, tb)) = MOLTEN.get(i + 1) else {
            return MOLTEN[MOLTEN.len() - 1].0;
        };
        let (a, ta) = MOLTEN[i];
        let k = (t - ta) / (tb - ta);
        std::array::from_fn(|c| a[c] + (b[c] - a[c]) * k)
    }

    /// `c` with its alpha multiplied by `a`.
    pub fn fade(c: [f32; 4], a: f32) -> [f32; 4] {
        [c[0], c[1], c[2], c[3] * a]
    }
}

/// Warm tint multiplied into the light and sky colors behind menus.
const MENU_WARMTH: Vec3 = Vec3::new(1.0, 0.86, 0.68);

/// The scene behind menus: `track` lighting made warm and dark (dimmer exposure, muted
/// saturation, a heavier vignette) so menu text reads over it.
pub fn menu_grade(track: Lighting) -> Lighting {
    Lighting {
        sun_color: track.sun_color * MENU_WARMTH,
        sky_zenith: track.sky_zenith * MENU_WARMTH,
        sky_horizon: track.sky_horizon * MENU_WARMTH,
        ambient_ground: track.ambient_ground * MENU_WARMTH,
        fog_color: track.fog_color * MENU_WARMTH,
        exposure: track.exposure * 0.7,
        saturation: 0.8,
        vignette: track.vignette.max(0.7),
        ..track
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_grade_is_warm_and_dark() {
        let track = Lighting::default();
        let g = menu_grade(track);
        assert!(g.exposure < track.exposure);
        assert!((g.saturation - 0.8).abs() < 1e-6);
        assert!(g.vignette > track.vignette);
        let warmth = |c: Vec3| c.x / c.z;
        assert!(warmth(g.sun_color) > warmth(track.sun_color));
        assert!(warmth(g.sky_zenith) > warmth(track.sky_zenith));
        assert_eq!(g.sun_dir, track.sun_dir, "the grade never moves the sun");
        assert_eq!(g.fog_density, track.fog_density);
    }

    #[test]
    fn molten_hits_its_stops() {
        use crucible::*;
        for (c, at) in MOLTEN {
            assert_eq!(molten(at), c);
        }
        assert_eq!(molten(-1.0), MOLTEN[0].0);
        assert_eq!(molten(2.0), HOT);
        let mid = molten(0.175);
        assert!(mid[1] > MOLTEN[0].0[1] && mid[1] < MOLTEN[1].0[1]);
        assert_eq!(CHAR, [20.0 / 255.0, 13.0 / 255.0, 10.0 / 255.0, 1.0]);
    }

    #[test]
    fn tier_colors_saturate() {
        assert_eq!(tier_color(0), TIER_COLORS[0]);
        assert_eq!(tier_color(3), TIER_COLORS[3]);
        assert_eq!(tier_color(9), TIER_COLORS[3]);
    }
}
