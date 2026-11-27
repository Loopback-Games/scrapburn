//! Bench/demo driving: a skilled `rivet_ai` bot on the track centerline.

use glam::Vec3;
use rivet_ai::{BotDriver, RacingLine};
use rivet_physics::{DriveInput, VehicleParams, VehicleState};

/// Drives like a top bot with no lane offset and no traffic. Stateless: a fresh driver each
/// tick, so its recovery maneuvers (which need memory) never kick in here.
pub fn autopilot(state: &VehicleState, centerline: &[Vec3]) -> DriveInput {
    let line = RacingLine::new(centerline.to_vec());
    BotDriver::new(1.0, 0.0).drive(
        state,
        &VehicleParams::default(),
        &line,
        &[],
        1.0,
        1.0 / 60.0,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn autopilot_laps_every_track() {
        use crate::laps::{LapEvent, LapTracker};
        use crate::track::{TRACKS, build_track};
        use rivet_physics::{VehicleParams, VehicleState, step};
        for def in TRACKS.iter().skip(1) {
            let t = build_track(def);
            let p = VehicleParams::default();
            let mut s = VehicleState::spawn(t.spawn.0 + glam::Vec3::Y * 0.6, t.spawn.1);
            let mut laps = LapTracker::new(t.checkpoints.len());
            let mut done = false;
            for i in 0..(120 * 60) {
                let prev = s.position;
                let input = super::autopilot(&s, &t.centerline);
                step(&mut s, &p, input, &t.collision, 1.0 / 60.0);
                assert!(
                    s.position.y > t.centerline.iter().map(|c| c.y).fold(f32::MAX, f32::min) - 25.0,
                    "{}: fell off at tick {i} ({})",
                    def.name,
                    s.position
                );
                if let Some(LapEvent::Lap { .. }) =
                    laps.update(prev, s.position, &t.checkpoints, i as f64 / 60.0)
                {
                    done = true;
                    break;
                }
            }
            assert!(done, "{}: no lap in 120 s", def.name);
        }
    }

    use super::*;
    use crate::laps::{LapEvent, LapTracker};
    use crate::track::build_graybox_loop;
    use rivet_physics::{VehicleParams, VehicleState, step};

    #[test]
    fn autopilot_completes_lap() {
        let t = build_graybox_loop();
        let p = VehicleParams::default();
        let mut s = VehicleState::spawn(t.spawn.0 + glam::Vec3::Y * 0.6, t.spawn.1);
        let mut laps = LapTracker::new(t.checkpoints.len());
        let mut completed = 0;
        let (mut drift_ticks, mut max_charge) = (0u32, 0.0f32);
        let dt = 1.0 / 60.0;
        for i in 0..(90 * 60) {
            let prev = s.position;
            let input = autopilot(&s, &t.centerline);
            step(&mut s, &p, input, &t.collision, dt);
            drift_ticks += s.drift.is_some() as u32;
            max_charge = max_charge.max(s.drift.map_or(0.0, |d| d.charge));
            assert!(s.position.y > -30.0, "fell off at tick {i}: {}", s.position);
            if let Some(LapEvent::Lap { .. }) =
                laps.update(prev, s.position, &t.checkpoints, i as f64 * dt as f64)
            {
                completed += 1;
            }
        }
        assert!(completed >= 1, "no lap in 90 s");
        assert!(
            drift_ticks > 120,
            "the autopilot should drift through corners ({drift_ticks} ticks)"
        );
        assert!(
            max_charge >= p.boost_tiers[0].charge,
            "and charge a drift boost (max {max_charge})"
        );
    }
}
