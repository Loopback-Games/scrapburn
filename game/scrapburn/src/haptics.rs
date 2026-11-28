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

/// Wall impact (0..1) from one frame's change in speed while grounded: karts brake and
/// coast at well under 3 m/s per frame, a wall hit costs far more.
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
}
