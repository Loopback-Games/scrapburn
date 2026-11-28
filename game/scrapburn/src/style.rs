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
    fn tier_colors_saturate() {
        assert_eq!(tier_color(0), TIER_COLORS[0]);
        assert_eq!(tier_color(3), TIER_COLORS[3]);
        assert_eq!(tier_color(9), TIER_COLORS[3]);
    }
}
