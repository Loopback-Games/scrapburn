//! Embers rising behind the menus: a fixed set of sparks that spawn below the screen, drift
//! up with a slow sway, fade, and respawn. Pure simulation in the 1280x720 menu space; the
//! menu drawing scales them to the window.

/// How many embers are alive at once.
pub const COUNT: usize = 70;
/// The space the embers live in (the menu's 1280x720 design size).
pub const WIDTH: f32 = 1280.0;
pub const HEIGHT: f32 = 720.0;
/// Embers start this far below the bottom edge and respawn once this far above the top.
const BELOW: f32 = 10.0;

/// One spark, in design pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ember {
    pub x: f32,
    pub y: f32,
    /// Rise speed, pixels per 60 Hz frame.
    rise: f32,
    pub radius: f32,
    /// Sway phase offset.
    phase: f32,
    /// 0 (deep orange) to 1 (yellow).
    pub heat: f32,
    /// Fades out as this falls; respawns at 0.
    pub life: f32,
}

impl Ember {
    /// 0-1 opacity.
    pub fn alpha(&self) -> f32 {
        self.life.clamp(0.0, 1.0) * 0.9
    }
}

/// A tiny xorshift generator: repeatable for a seed, no crate needed.
#[derive(Debug, Clone)]
struct XorShift(u32);

impl XorShift {
    fn new(seed: u32) -> Self {
        Self(seed.max(1))
    }

    /// Uniform in [0, 1).
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
}

#[derive(Debug, Clone)]
pub struct Embers {
    sparks: Vec<Ember>,
    rng: XorShift,
}

impl Embers {
    /// `COUNT` embers scattered over the screen.
    pub fn new(seed: u32) -> Self {
        let mut rng = XorShift::new(seed);
        let sparks = (0..COUNT).map(|_| spawn(&mut rng, true)).collect();
        Self { sparks, rng }
    }

    pub fn sparks(&self) -> &[Ember] {
        &self.sparks
    }

    /// Advances `dt` seconds. With `motion` off (reduced motion) nothing moves.
    pub fn step(&mut self, dt: f32, motion: bool) {
        if !motion {
            return;
        }
        // The mockup's per-frame rates, scaled to 60 Hz frames.
        let frames = dt.clamp(0.0, 0.25) * 60.0;
        for e in &mut self.sparks {
            e.y -= e.rise * frames;
            e.x = (e.x + ((e.y + e.phase) / 40.0).sin() * 0.5 * frames).rem_euclid(WIDTH);
            e.life -= 0.002 * frames;
            if e.y < -BELOW || e.life <= 0.0 {
                *e = spawn(&mut self.rng, false);
            }
        }
    }
}

/// A new ember: anywhere on screen when `anywhere`, else just below the bottom edge.
fn spawn(rng: &mut XorShift, anywhere: bool) -> Ember {
    Ember {
        x: rng.next() * WIDTH,
        y: if anywhere {
            rng.next() * HEIGHT
        } else {
            HEIGHT + BELOW
        },
        rise: 0.3 + rng.next() * 1.1,
        radius: 0.6 + rng.next() * 1.8,
        phase: rng.next() * 400.0,
        heat: rng.next(),
        life: 0.6 + rng.next() * 0.6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_bounds(e: &Ember) -> bool {
        (-2.0..=WIDTH + 2.0).contains(&e.x) && (-BELOW..=HEIGHT + BELOW).contains(&e.y)
    }

    #[test]
    fn particles_stay_in_bounds() {
        let mut embers = Embers::new(7);
        for _ in 0..2000 {
            embers.step(1.0 / 60.0, true);
            for e in embers.sparks() {
                assert!(in_bounds(e), "{e:?}");
                assert!((0.0..=1.0).contains(&e.alpha()));
            }
        }
    }

    #[test]
    fn count_is_stable() {
        let mut embers = Embers::new(3);
        assert_eq!(embers.sparks().len(), COUNT);
        for _ in 0..1000 {
            embers.step(0.05, true);
            assert_eq!(embers.sparks().len(), COUNT);
        }
    }

    #[test]
    fn embers_rise_and_respawn() {
        let mut embers = Embers::new(11);
        let before = embers.sparks().to_vec();
        embers.step(1.0 / 60.0, true);
        let rose = before
            .iter()
            .zip(embers.sparks())
            .all(|(a, b)| b.y < a.y || b.y == HEIGHT + BELOW);
        assert!(rose, "every ember rises or has respawned below");
        // Long enough for every ember to leave the top or burn out at least once.
        let first: Vec<f32> = embers.sparks().iter().map(|e| e.phase).collect();
        for _ in 0..60 * 60 {
            embers.step(1.0 / 60.0, true);
        }
        let respawned = embers
            .sparks()
            .iter()
            .zip(&first)
            .all(|(e, p)| e.phase != *p);
        assert!(respawned, "every ember respawns");
    }

    #[test]
    fn reduced_motion_freezes_them() {
        let mut embers = Embers::new(5);
        let before = embers.sparks().to_vec();
        for _ in 0..100 {
            embers.step(1.0 / 60.0, false);
        }
        assert_eq!(embers.sparks(), &before[..]);
    }

    #[test]
    fn a_seed_repeats() {
        let (mut a, mut b) = (Embers::new(9), Embers::new(9));
        for _ in 0..50 {
            a.step(0.016, true);
            b.step(0.016, true);
        }
        assert_eq!(a.sparks(), b.sparks());
        assert_ne!(Embers::new(9).sparks(), Embers::new(10).sparks());
    }
}
