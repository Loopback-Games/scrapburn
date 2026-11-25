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

    /// Index of the most recently passed checkpoint (respawn point).
    pub fn last_checkpoint(&self) -> usize {
        (self.next + self.count - 1) % self.count
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
