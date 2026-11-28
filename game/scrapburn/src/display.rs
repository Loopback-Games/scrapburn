//! Display-mode helpers: the video mode for exclusive fullscreen, and the size to restore
//! on returning to a window.

/// One monitor video mode: size in physical pixels and refresh rate in millihertz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeInfo {
    pub size: (u32, u32),
    pub refresh_mhz: u32,
}

/// The index in `modes` of the monitor's native mode: the largest size, at the highest
/// refresh rate that size offers. `None` when there are no modes.
pub fn pick_video_mode(modes: &[ModeInfo]) -> Option<usize> {
    // Largest area, then the wider of equal areas, then the highest refresh. `max_by_key`
    // keeps the last of equal keys, so compare the reverse index to let the first win.
    modes
        .iter()
        .enumerate()
        .max_by_key(|(i, m)| {
            let (w, h) = m.size;
            (
                u64::from(w) * u64::from(h),
                w,
                m.refresh_mhz,
                std::cmp::Reverse(*i),
            )
        })
        .map(|(i, _)| i)
}

/// The window's size while windowed, kept from `Resized` events so it can be restored after
/// fullscreen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowedSize {
    size: (u32, u32),
    /// Returning from fullscreen: resizes to the fullscreen size are still arriving.
    restoring: bool,
}

impl WindowedSize {
    pub fn new(size: (u32, u32)) -> Self {
        Self {
            size,
            restoring: false,
        }
    }

    /// The size to restore.
    pub fn get(&self) -> (u32, u32) {
        self.size
    }

    /// The window has just been asked to leave fullscreen.
    pub fn begin_restore(&mut self) {
        self.restoring = true;
    }

    /// A `Resized` event while the window is windowed. `fills_monitor` is true when `size`
    /// equals the monitor's size: while restoring, that is the fullscreen size lingering,
    /// not a size to remember.
    pub fn on_resized(&mut self, size: (u32, u32), fills_monitor: bool) {
        if size.0 == 0 || size.1 == 0 || (self.restoring && fills_monitor) {
            return;
        }
        self.restoring = false;
        self.size = size;
    }
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
        assert_eq!(pick_video_mode(&[]), None);
    }

    #[test]
    fn largest_size_at_its_highest_refresh() {
        let modes = [
            m(1600, 900, 144),
            m(2560, 1440, 60),
            m(2560, 1440, 165),
            m(2560, 1440, 120),
            m(1920, 1080, 240),
        ];
        assert_eq!(pick_video_mode(&modes), Some(2));
    }

    #[test]
    fn size_wins_over_refresh() {
        let modes = [m(1920, 1080, 240), m(3840, 2160, 60), m(1280, 720, 360)];
        assert_eq!(pick_video_mode(&modes), Some(1));
    }

    #[test]
    fn equal_areas_prefer_the_wider_mode() {
        // 1600×1200 and 1920×1000 have the same area; the wider is the native widescreen.
        let modes = [m(1600, 1200, 60), m(1920, 1000, 60)];
        assert_eq!(pick_video_mode(&modes), Some(1));
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
        assert_eq!(pick_video_mode(&modes), Some(1));
    }

    #[test]
    fn windowed_resizes_are_remembered() {
        let mut w = WindowedSize::new((1600, 900));
        w.on_resized((1200, 700), false);
        assert_eq!(w.get(), (1200, 700));
        // Zero is a minimized window, not a size.
        w.on_resized((0, 0), false);
        assert_eq!(w.get(), (1200, 700));
        // Not restoring, a window that fills the monitor (maximized) is a real size.
        w.on_resized((2560, 1440), true);
        assert_eq!(w.get(), (2560, 1440));
    }

    #[test]
    fn the_fullscreen_size_is_ignored_while_restoring() {
        let mut w = WindowedSize::new((1200, 700));
        w.begin_restore();
        w.on_resized((2560, 1440), true);
        assert_eq!(w.get(), (1200, 700));
        // Back at the remembered size: restoring is over and later drags count.
        w.on_resized((1200, 700), false);
        w.on_resized((1000, 600), false);
        assert_eq!(w.get(), (1000, 600));
    }

    #[test]
    fn a_different_size_from_the_window_manager_ends_the_restore() {
        let mut w = WindowedSize::new((1200, 700));
        w.begin_restore();
        w.on_resized((1210, 690), false);
        assert_eq!(w.get(), (1210, 690));
        w.on_resized((2560, 1440), true);
        assert_eq!(w.get(), (2560, 1440), "restore finished; a maximize counts");
    }
}
