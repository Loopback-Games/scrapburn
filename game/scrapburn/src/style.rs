//! Shared look-and-feel constants: drift tier colors and HUD palette.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_colors_saturate() {
        assert_eq!(tier_color(0), TIER_COLORS[0]);
        assert_eq!(tier_color(3), TIER_COLORS[3]);
        assert_eq!(tier_color(9), TIER_COLORS[3]);
    }
}
