//! Gamepad vibration: maps a race impact to motor strengths. Pure, so it can be tested.

/// What shook the player's kart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Wall,
    Landing,
    Boost,
    ItemHit,
}

/// One rumble pulse: motor strengths (0..1) and a length in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pulse {
    pub strong: f32,
    pub weak: f32,
    pub ms: u32,
}

/// Impacts below this feel like noise and make no pulse.
const MIN_IMPACT: f32 = 0.05;

/// The pulse for an `impact` of 0 to 1 (clamped), or `None` when vibration is off or the
/// impact is too light to notice.
pub fn pulse(enabled: bool, kind: Kind, impact: f32) -> Option<Pulse> {
    if !enabled || impact.is_nan() {
        return None;
    }
    let i = impact.clamp(0.0, 1.0);
    if i < MIN_IMPACT {
        return None;
    }
    // (strong, weak, base ms, extra ms at full impact)
    let (s, w, base, extra) = match kind {
        Kind::Wall => (1.0, 0.5, 80.0, 120.0),
        Kind::Landing => (0.9, 0.4, 70.0, 130.0),
        Kind::Boost => (0.3, 0.6, 140.0, 80.0),
        Kind::ItemHit => (1.0, 1.0, 200.0, 150.0),
    };
    Some(Pulse {
        strong: s * i,
        weak: w * i,
        ms: (base + extra * i) as u32,
    })
}

/// Wall impact (0..1) from one frame's change in speed while grounded. Braking and coasting
/// lose well under 3 m/s per frame, so a bigger drop is taken for a wall hit. Other sudden
/// slowdowns (a spin-out, a respawn) fool this; `Watch` screens those out.
pub fn wall_impact(prev_speed: f32, speed: f32, grounded: bool) -> f32 {
    const MIN_DROP: f32 = 3.0;
    const FULL_DROP: f32 = 15.0;
    let drop = prev_speed - speed;
    if !grounded || drop.is_nan() || drop < MIN_DROP {
        return 0.0;
    }
    ((drop - MIN_DROP) / (FULL_DROP - MIN_DROP))
        .clamp(0.0, 1.0)
        .max(MIN_IMPACT)
}

/// One frame of the followed kart.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    /// Which racer is followed.
    pub who: usize,
    pub speed: f32,
    pub pos: glam::Vec3,
    /// Race time in seconds.
    pub time: f64,
    pub grounded: bool,
    pub boosting: bool,
    /// Spinning out from a weapon hit.
    pub spinning: bool,
}

/// What began this frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Edges {
    pub wall: f32,
    pub boost_start: bool,
}

/// Spots wall hits and boost starts from frame to frame, ignoring the first frame after a
/// restart, respawn, or change of followed kart, and the moments around an item hit.
#[derive(Debug, Clone, Default)]
pub struct Watch {
    prev: Option<Frame>,
    mute_until: f64,
}

/// A kart that moves farther than this in one frame was teleported (respawn or restart).
const TELEPORT: f32 = 8.0;
/// Seconds after an item hit when speed drops are not wall hits.
pub const HIT_MUTE: f64 = 0.5;

impl Watch {
    /// Forgets the last frame; the next one sets the baseline.
    pub fn reset(&mut self) {
        self.prev = None;
        self.mute_until = 0.0;
    }

    /// No wall hits until race time `until`: the followed kart took an item hit.
    pub fn mute_until(&mut self, until: f64) {
        self.mute_until = self.mute_until.max(until);
    }

    pub fn update(&mut self, f: Frame) -> Edges {
        let prev = self.prev.replace(f);
        let Some(p) = prev else {
            return Edges::default();
        };
        if p.who != f.who || f.time < p.time || p.pos.distance(f.pos) > TELEPORT {
            return Edges::default();
        }
        let quiet = f.spinning || p.spinning || f.time < self.mute_until;
        Edges {
            wall: if quiet {
                0.0
            } else {
                wall_impact(p.speed, f.speed, f.grounded)
            },
            boost_start: f.boosting && !p.boosting,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_or_tiny_impact_makes_no_pulse() {
        assert_eq!(pulse(true, Kind::Wall, 0.0), None);
        assert_eq!(pulse(true, Kind::Landing, 0.04), None);
        assert_eq!(pulse(true, Kind::ItemHit, f32::NAN), None);
        assert_eq!(pulse(true, Kind::Wall, -3.0), None);
    }

    #[test]
    fn gate_off_never_pulses() {
        for k in [Kind::Wall, Kind::Landing, Kind::Boost, Kind::ItemHit] {
            assert_eq!(pulse(false, k, 1.0), None);
        }
    }

    #[test]
    fn impact_is_clamped_and_scales() {
        let full = pulse(true, Kind::Wall, 1.0).unwrap();
        assert_eq!(pulse(true, Kind::Wall, 50.0), Some(full));
        assert_eq!(full.strong, 1.0);
        assert_eq!(full.ms, 200);
        let half = pulse(true, Kind::Wall, 0.5).unwrap();
        assert!(half.strong < full.strong && half.weak < full.weak && half.ms < full.ms);
        for k in [Kind::Wall, Kind::Landing, Kind::Boost, Kind::ItemHit] {
            let p = pulse(true, k, 1.0).unwrap();
            assert!((0.0..=1.0).contains(&p.strong) && (0.0..=1.0).contains(&p.weak));
            assert!(p.ms <= 400);
        }
    }

    #[test]
    fn wall_impact_needs_a_sharp_grounded_drop() {
        assert_eq!(wall_impact(30.0, 29.5, true), 0.0);
        assert_eq!(wall_impact(30.0, 10.0, false), 0.0);
        assert_eq!(wall_impact(10.0, 30.0, true), 0.0);
        assert!(wall_impact(30.0, 24.0, true) > 0.05);
        assert_eq!(wall_impact(40.0, 5.0, true), 1.0);
        assert_eq!(wall_impact(30.0, f32::NAN, true), 0.0);
    }

    fn frame(who: usize, speed: f32, x: f32, time: f64) -> Frame {
        Frame {
            who,
            speed,
            pos: glam::Vec3::X * x,
            time,
            grounded: true,
            boosting: false,
            spinning: false,
        }
    }

    #[test]
    fn watch_flags_a_wall_hit_but_not_the_first_frame() {
        let mut w = Watch::default();
        assert_eq!(w.update(frame(0, 30.0, 0.0, 1.0)).wall, 0.0);
        assert!(w.update(frame(0, 15.0, 0.5, 1.02)).wall > 0.5);
    }

    #[test]
    fn watch_ignores_restarts_respawns_and_focus_changes() {
        let mut w = Watch::default();
        w.update(frame(0, 38.0, 0.0, 50.0));
        // Restart: time rewinds and the kart is back at rest.
        assert_eq!(w.update(frame(0, 0.0, 0.0, 0.0)).wall, 0.0);
        w.update(frame(0, 38.0, 0.0, 50.0));
        // Respawn: teleported and stopped.
        assert_eq!(w.update(frame(0, 0.0, 100.0, 50.02)).wall, 0.0);
        w.update(frame(0, 38.0, 100.0, 51.0));
        // Followed kart changes.
        assert_eq!(w.update(frame(1, 0.0, 100.0, 51.02)).wall, 0.0);
        w.update(frame(1, 38.0, 100.0, 52.0));
        w.reset();
        assert_eq!(w.update(frame(1, 0.0, 100.0, 52.02)).wall, 0.0);
    }

    #[test]
    fn watch_ignores_spin_outs_and_item_hits() {
        let mut w = Watch::default();
        w.update(frame(0, 30.0, 0.0, 1.0));
        let mut spun = frame(0, 12.0, 0.5, 1.02);
        spun.spinning = true;
        assert_eq!(w.update(spun).wall, 0.0);
        w.update(frame(0, 30.0, 1.0, 2.0));
        w.mute_until(2.0 + HIT_MUTE);
        assert_eq!(w.update(frame(0, 12.0, 1.5, 2.02)).wall, 0.0);
        w.update(frame(0, 30.0, 2.0, 3.0));
        assert!(w.update(frame(0, 12.0, 2.5, 3.02)).wall > 0.0);
    }

    #[test]
    fn watch_reports_boost_starts() {
        let mut w = Watch::default();
        w.update(frame(0, 30.0, 0.0, 1.0));
        let mut b = frame(0, 30.0, 0.5, 1.02);
        b.boosting = true;
        assert!(w.update(b).boost_start);
        b.time = 1.04;
        assert!(!w.update(b).boost_start);
    }
}
