//! Choosing a monitor video mode for exclusive fullscreen.

/// One monitor video mode: size in physical pixels and refresh rate in millihertz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeInfo {
    pub size: (u32, u32),
    pub refresh_mhz: u32,
}

/// The index in `modes` of the mode closest to `want` in size, at the highest refresh
/// rate that size offers. `None` when there are no modes.
pub fn pick_video_mode(modes: &[ModeInfo], want: (u32, u32)) -> Option<usize> {
    let distance = |m: &ModeInfo| {
        let dw = i64::from(m.size.0) - i64::from(want.0);
        let dh = i64::from(m.size.1) - i64::from(want.1);
        dw * dw + dh * dh
    };
    // Smallest distance first, then highest refresh; the first listed wins a full tie.
    modes
        .iter()
        .enumerate()
        .min_by_key(|(_, m)| (distance(m), std::cmp::Reverse(m.refresh_mhz)))
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(w: u32, h: u32, hz: u32) -> ModeInfo {
        ModeInfo {
            size: (w, h),
            refresh_mhz: hz * 1000,
        }
    }

    #[test]
    fn no_modes_picks_nothing() {
        assert_eq!(pick_video_mode(&[], (1600, 900)), None);
    }

    #[test]
    fn exact_size_at_the_highest_refresh() {
        let modes = [
            m(2560, 1440, 165),
            m(1600, 900, 60),
            m(1600, 900, 144),
            m(1600, 900, 120),
            m(1920, 1080, 240),
        ];
        assert_eq!(pick_video_mode(&modes, (1600, 900)), Some(2));
    }

    #[test]
    fn nearest_size_wins_over_refresh() {
        let modes = [m(1920, 1080, 60), m(2560, 1440, 165), m(1280, 720, 240)];
        assert_eq!(pick_video_mode(&modes, (1700, 1000)), Some(0));
        assert_eq!(pick_video_mode(&modes, (2400, 1350)), Some(1));
    }

    #[test]
    fn a_window_larger_than_every_mode_gets_the_largest() {
        let modes = [m(1280, 720, 60), m(1920, 1080, 60), m(1920, 1080, 75)];
        assert_eq!(pick_video_mode(&modes, (4000, 3000)), Some(2));
    }

    #[test]
    fn fractional_refresh_rates_compare_exactly() {
        let modes = [
            ModeInfo {
                size: (1920, 1080),
                refresh_mhz: 59_940,
            },
            ModeInfo {
                size: (1920, 1080),
                refresh_mhz: 60_000,
            },
        ];
        assert_eq!(pick_video_mode(&modes, (1920, 1080)), Some(1));
    }
}
