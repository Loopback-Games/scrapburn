//! Frame cap: when to start the next frame, and how to wait for it.
//!
//! The event loop sleeps (`ControlFlow::WaitUntil`) until shortly before the deadline,
//! then spins for the last stretch, because OS timers wake late by up to a millisecond
//! or so.

use std::time::{Duration, Instant};

/// How long before a deadline the loop stops sleeping and spins.
pub const SPIN: Duration = Duration::from_millis(1);

/// One frame at `cap_hz` frames per second.
pub fn period(cap_hz: u32) -> Duration {
    let hz = u64::from(cap_hz.max(1));
    // Rounded to the nearest nanosecond.
    Duration::from_nanos((1_000_000_000 + hz / 2) / hz)
}

/// When the frame after one released at `last` may start. `last` is when the previous
/// frame actually started, not when it was due, so a late frame pushes the next one back
/// instead of leaving a debt of frames to catch up on.
pub fn next_deadline(last: Instant, cap_hz: u32) -> Instant {
    last + period(cap_hz)
}

/// What the loop does at `now` while waiting for `deadline`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wait {
    /// Sleep until this instant (`SPIN` before the deadline).
    Sleep(Instant),
    /// Close enough: busy-wait for the deadline.
    Spin,
    /// The deadline has passed: start the frame.
    Go,
}

pub fn wait_mode(now: Instant, deadline: Instant) -> Wait {
    match deadline.checked_duration_since(now) {
        None => Wait::Go,
        Some(d) if d.is_zero() => Wait::Go,
        Some(d) if d <= SPIN => Wait::Spin,
        Some(_) => Wait::Sleep(deadline - SPIN),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn period_is_one_over_the_cap() {
        assert_eq!(period(60), Duration::from_nanos(16_666_667));
        assert_eq!(period(144), Duration::from_nanos(6_944_444));
        assert_eq!(period(30), Duration::from_nanos(33_333_333));
    }

    #[test]
    fn deadline_is_one_period_after_the_last_frame() {
        let t = Instant::now();
        assert_eq!(next_deadline(t, 60), t + period(60));
        assert_eq!(next_deadline(t, 144), t + period(144));
    }

    #[test]
    fn a_missed_deadline_does_not_accumulate_debt() {
        let t0 = Instant::now();
        let due = next_deadline(t0, 60);
        // The frame ran 50 ms late (three periods). The next one is a full period after it
        // actually started, not three overdue frames back to back.
        let late = due + Duration::from_millis(50);
        assert_eq!(wait_mode(late, due), Wait::Go);
        let next = next_deadline(late, 60);
        assert_eq!(next, late + period(60));
        assert!(matches!(
            wait_mode(late + Duration::from_micros(1), next),
            Wait::Sleep(_)
        ));
    }

    #[test]
    fn wait_sleeps_then_spins_then_goes() {
        let now = Instant::now();
        let deadline = now + Duration::from_millis(10);
        assert_eq!(wait_mode(now, deadline), Wait::Sleep(deadline - SPIN));
        assert_eq!(
            wait_mode(deadline - Duration::from_micros(500), deadline),
            Wait::Spin
        );
        // Exactly one spin window away still spins, rather than sleeping for zero time.
        assert_eq!(wait_mode(deadline - SPIN, deadline), Wait::Spin);
        assert_eq!(wait_mode(deadline, deadline), Wait::Go);
        assert_eq!(
            wait_mode(deadline + Duration::from_millis(3), deadline),
            Wait::Go
        );
    }
}
