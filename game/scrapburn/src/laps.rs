//! Ordered checkpoint crossing and lap timing.

use glam::Vec3;

use crate::track::Checkpoint;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LapEvent {
    Lap { number: u32, time: f64 },
}

/// Counts laps when checkpoints are crossed forward, in order. Checkpoint 0 is the start/finish.
#[derive(Debug, Clone)]
pub struct LapTracker {
    count: usize,
    next: usize,
    laps: u32,
    lap_start: f64,
}

/// Checkpoint planes are only valid this far above/below their center.
const VERTICAL_TOLERANCE: f32 = 12.0;

impl LapTracker {
    pub fn new(checkpoint_count: usize) -> Self {
        assert!(
            checkpoint_count >= 2,
            "a lap needs at least two checkpoints"
        );
        Self {
            count: checkpoint_count,
            next: 1,
            laps: 0,
            lap_start: 0.0,
        }
    }

    /// Restarts timing for the current lap (race start).
    pub fn start(&mut self, now: f64) {
        self.lap_start = now;
    }

    pub fn laps(&self) -> u32 {
        self.laps
    }

    pub fn lap_start(&self) -> f64 {
        self.lap_start
    }

    /// (next checkpoint, laps done, lap start time), for network snapshots.
    pub fn net_state(&self) -> (u16, u16, f64) {
        (
            self.next as u16,
            self.laps.min(u16::MAX as u32) as u16,
            self.lap_start,
        )
    }

    /// Restores a snapshot's lap state (an out-of-range checkpoint is clamped).
    pub fn set_net_state(&mut self, next: u16, laps: u16, lap_start: f64) {
        self.next = (next as usize).min(self.count - 1);
        self.laps = laps as u32;
        self.lap_start = lap_start;
    }

    /// Index of the most recently passed checkpoint (respawn point).
    pub fn last_checkpoint(&self) -> usize {
        (self.next + self.count - 1) % self.count
    }

    /// Index of the checkpoint to cross next.
    pub fn next_checkpoint(&self) -> usize {
        self.next
    }

    /// Lap progress in checkpoints, fractional: passed checkpoints this lap plus the share of
    /// the way to the next one.
    pub fn progress(&self, pos: Vec3, checkpoints: &[Checkpoint]) -> f32 {
        let next = &checkpoints[self.next];
        let prev = &checkpoints[self.last_checkpoint()];
        let span = prev.center.distance(next.center).max(1.0);
        // May go negative behind the previous checkpoint (the grid), so order still counts there.
        let frac = (1.0 - pos.distance(next.center) / span).clamp(-1.5, 1.0);
        let passed = if self.next == 0 {
            self.count - 1
        } else {
            self.next - 1
        };
        (self.laps as usize * self.count + passed) as f32 + frac
    }

    /// Points `next` at the first checkpoint ahead of `pos` (after teleports in tests/tools).
    pub fn sync_to(&mut self, pos: Vec3, checkpoints: &[Checkpoint]) {
        let nearest = (0..checkpoints.len())
            .min_by(|&a, &b| {
                checkpoints[a]
                    .center
                    .distance(pos)
                    .total_cmp(&checkpoints[b].center.distance(pos))
            })
            .unwrap_or(0);
        let ahead = (pos - checkpoints[nearest].center).dot(checkpoints[nearest].forward) < 0.0;
        self.next = if ahead {
            nearest
        } else {
            (nearest + 1) % self.count
        };
        if self.next == 0 {
            self.next = 1 % self.count;
        }
    }

    pub fn update(
        &mut self,
        prev: Vec3,
        pos: Vec3,
        checkpoints: &[Checkpoint],
        now: f64,
    ) -> Option<LapEvent> {
        let cp = &checkpoints[self.next];
        let a = (prev - cp.center).dot(cp.forward);
        let b = (pos - cp.center).dot(cp.forward);
        if !(a < 0.0 && b >= 0.0) {
            return None;
        }
        let t = a / (a - b);
        let hit = prev.lerp(pos, t) - cp.center;
        let lateral = hit - cp.forward * hit.dot(cp.forward);
        if lateral.y.abs() > VERTICAL_TOLERANCE
            || (lateral - Vec3::Y * lateral.y).length() > cp.half_width
        {
            return None;
        }
        let crossed = self.next;
        self.next = (self.next + 1) % self.count;
        if crossed == 0 {
            self.laps += 1;
            let time = now - self.lap_start;
            self.lap_start = now;
            return Some(LapEvent::Lap {
                number: self.laps,
                time,
            });
        }
        None
    }
}

#[cfg(test)]
mod net_tests {
    use super::*;

    #[test]
    fn net_state_round_trips_and_clamps() {
        let mut a = LapTracker::new(10);
        a.set_net_state(4, 2, 31.5);
        assert_eq!(a.net_state(), (4, 2, 31.5));
        assert_eq!(a.laps(), 2);
        a.set_net_state(99, 0, 0.0);
        assert_eq!(a.net_state().0, 9);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::build_graybox_loop;

    #[test]
    fn laps_count_in_order() {
        let t = build_graybox_loop();
        let mut laps = LapTracker::new(t.checkpoints.len());
        let n = t.centerline.len() - 1; // last point duplicates the first
        let mut events = Vec::new();
        let mut prev = t.centerline[0];
        for i in 1..=(2 * n) {
            let pos = t.centerline[i % n];
            if let Some(e) = laps.update(prev, pos, &t.checkpoints, i as f64) {
                events.push(e);
            }
            prev = pos;
        }
        assert_eq!(events.len(), 2, "{events:?}");
        assert!(matches!(events[0], LapEvent::Lap { number: 1, .. }));
        assert!(matches!(events[1], LapEvent::Lap { number: 2, .. }));
    }

    #[test]
    fn laps_reject_shortcut() {
        let t = build_graybox_loop();
        let mut laps = LapTracker::new(t.checkpoints.len());
        let last = t.checkpoints.last().unwrap();
        let path = [
            t.centerline[0],
            t.centerline[1],
            last.center - last.forward * 2.0,
            last.center + last.forward * 2.0,
            t.checkpoints[0].center - t.checkpoints[0].forward * 2.0,
            t.checkpoints[0].center + t.checkpoints[0].forward * 2.0,
        ];
        for (i, w) in path.windows(2).enumerate() {
            assert_eq!(laps.update(w[0], w[1], &t.checkpoints, i as f64), None);
        }
    }

    #[test]
    fn backwards_does_not_count() {
        let t = build_graybox_loop();
        let mut laps = LapTracker::new(t.checkpoints.len());
        let pts: Vec<_> = t.centerline.iter().rev().copied().collect();
        for (i, w) in pts.windows(2).enumerate() {
            assert_eq!(laps.update(w[0], w[1], &t.checkpoints, i as f64), None);
        }
    }

    #[test]
    fn last_checkpoint_tracks_progress() {
        let t = build_graybox_loop();
        let mut laps = LapTracker::new(t.checkpoints.len());
        assert_eq!(laps.last_checkpoint(), 0);
        let cp = &t.checkpoints[1];
        laps.update(
            cp.center - cp.forward,
            cp.center + cp.forward,
            &t.checkpoints,
            1.0,
        );
        assert_eq!(laps.last_checkpoint(), 1);
    }
}
