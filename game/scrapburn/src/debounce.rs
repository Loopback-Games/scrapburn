//! Debounce for renderer rebuilds: a rebuild is expensive, so a burst of quality edits
//! (holding an arrow on a slider) rebuilds once, shortly after the last one.

use std::time::{Duration, Instant};

/// Quiet time after the last edit before the renderer is rebuilt.
pub const REBUILD_DELAY: Duration = Duration::from_millis(350);

/// When a pending rebuild is due, if one is pending.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RebuildSchedule {
    due: Option<Instant>,
}

impl RebuildSchedule {
    /// A change happened at `now`: (re)start the countdown.
    pub fn request(&mut self, now: Instant) {
        self.due = Some(now + REBUILD_DELAY);
    }

    /// A rebuild is waiting.
    pub fn pending(&self) -> bool {
        self.due.is_some()
    }

    /// True once, when the countdown has run out; clears the request.
    pub fn take_due(&mut self, now: Instant) -> bool {
        match self.due {
            Some(due) if now >= due => {
                self.due = None;
                true
            }
            _ => false,
        }
    }

    /// Rebuild now: true if one was pending, and clears it.
    pub fn flush(&mut self) -> bool {
        self.due.take().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_due_without_a_request() {
        let mut s = RebuildSchedule::default();
        let t = Instant::now();
        assert!(!s.take_due(t + Duration::from_secs(10)));
        assert!(!s.flush());
    }

    #[test]
    fn a_request_is_due_after_the_delay_once() {
        let mut s = RebuildSchedule::default();
        let t = Instant::now();
        s.request(t);
        assert!(s.pending());
        assert!(!s.take_due(t + Duration::from_millis(349)));
        assert!(s.take_due(t + Duration::from_millis(350)));
        assert!(!s.take_due(t + Duration::from_secs(5)), "taken once");
        assert!(!s.pending());
    }

    #[test]
    fn each_request_restarts_the_countdown() {
        let mut s = RebuildSchedule::default();
        let t = Instant::now();
        s.request(t);
        s.request(t + Duration::from_millis(300));
        assert!(!s.take_due(t + Duration::from_millis(500)));
        assert!(s.take_due(t + Duration::from_millis(650)));
    }

    #[test]
    fn flush_takes_a_pending_request_immediately() {
        let mut s = RebuildSchedule::default();
        s.request(Instant::now());
        assert!(s.flush());
        assert!(!s.pending());
        assert!(!s.flush());
    }
}
