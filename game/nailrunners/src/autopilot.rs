//! Follows the track centerline. Drives bench runs; the seed of the M4 bots.

use glam::Vec3;
use rivet_physics::{DriveInput, VehicleState};

/// How far ahead along the centerline to aim, in points (~2 m each).
const LOOK_AHEAD_POINTS: usize = 8;
const DRIFT_LOOK_AHEAD_POINTS: usize = 16;
const DRIFT_ENTER_ANGLE: f32 = 0.16;
const DRIFT_EXIT_ANGLE: f32 = 0.03;
const DRIFT_MIN_SPEED: f32 = 18.0;

fn nearest(points: &[Vec3], pos: Vec3) -> usize {
    let mut best = (0, f32::MAX);
    for (i, p) in points.iter().enumerate() {
        let d = p.distance_squared(pos);
        if d < best.1 {
            best = (i, d);
        }
    }
    best.0
}

/// Steer toward the centerline point ~15 m ahead, full throttle, brake on sharp heading error.
pub fn autopilot(state: &VehicleState, centerline: &[Vec3]) -> DriveInput {
    // The last point repeats the first; skip it so indices wrap cleanly.
    let pts = &centerline[..centerline.len().saturating_sub(1).max(1)];
    let i = nearest(pts, state.position);
    let target = pts[(i + LOOK_AHEAD_POINTS) % pts.len()];
    let f = state.forward();
    let f = Vec3::new(f.x, 0.0, f.z).normalize_or(Vec3::Z);
    let d = Vec3::new(
        target.x - state.position.x,
        0.0,
        target.z - state.position.z,
    )
    .normalize_or(f);
    // Positive angle = target to the left (counter-clockwise from above).
    let angle = f.cross(d).y.atan2(f.dot(d));
    let sharp = angle.abs() > 0.9;
    // Drift through real corners at speed: hop in, hold, and let the arc carry it.
    // Corner detection looks farther ahead than steering does.
    let far = pts[(i + DRIFT_LOOK_AHEAD_POINTS) % pts.len()];
    let fd = Vec3::new(far.x - state.position.x, 0.0, far.z - state.position.z).normalize_or(f);
    // Measure the corner against the direction of travel: mid-drift the nose points inward.
    let travel = Vec3::new(state.velocity.x, 0.0, state.velocity.z).normalize_or(f);
    let far_angle = travel.cross(fd).y.atan2(travel.dot(fd));
    let same_way = far_angle.signum() == angle.signum();
    let corner = far_angle.abs() > DRIFT_ENTER_ANGLE && same_way && state.speed() > DRIFT_MIN_SPEED;
    // Hold the drift while the corner still bends the drift's way.
    let keep = state
        .drift
        .is_some_and(|d| -far_angle.signum() == d.dir && far_angle.abs() > DRIFT_EXIT_ANGLE);
    let drift = !sharp && (corner || keep);
    DriveInput {
        throttle: if sharp { 0.0 } else { 1.0 },
        brake: if sharp { 1.0 } else { 0.0 },
        steer: if drift {
            // Commit into the corner (entry needs a real steer), then trim the arc.
            let into = -far_angle.signum();
            (into * 0.55 - angle * 3.0).clamp(-1.0, 1.0)
        } else {
            (-angle * 2.5).clamp(-1.0, 1.0)
        },
        drift,
        hop: drift && state.drift.is_none(),
    }
}

#[cfg(test)]
mod tests {
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
