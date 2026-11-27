//! Circuits: every track in a fixed order, with points for each finishing position.

use crate::track::TRACKS;

/// Points for 1st, 2nd, ... place. Positions beyond the table score nothing.
pub const POINTS: [u8; 8] = [10, 8, 6, 5, 4, 3, 2, 1];

/// Seats a circuit keeps points for (matches the largest online race).
pub const SEATS: usize = 12;

/// Races in a circuit: one on each track.
pub const ROUNDS: usize = TRACKS.len();

/// Points for finishing in `position` (0 = first).
pub fn points_for(position: usize) -> u8 {
    POINTS.get(position).copied().unwrap_or(0)
}

/// Single track or a full circuit, as chosen in race setup.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Event {
    #[default]
    Single,
    Circuit,
}

impl Event {
    pub fn name(self) -> &'static str {
        match self {
            Self::Single => "SINGLE TRACK",
            Self::Circuit => "CIRCUIT",
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Self::Single => Self::Circuit,
            Self::Circuit => Self::Single,
        }
    }
}

/// A circuit in progress: the current round and each seat's points before it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Circuit {
    /// 0-based round; also the index into `TRACKS`.
    pub round: u8,
    /// Points per race seat from the rounds already run.
    pub points: [u8; SEATS],
}

impl Circuit {
    /// The track this round runs on.
    pub fn track(&self) -> usize {
        self.round as usize
    }

    pub fn is_final(&self) -> bool {
        self.round as usize + 1 >= ROUNDS
    }

    /// Totals after this round, given its running order (`standings` lists seats, leader first).
    pub fn totals(&self, standings: &[usize]) -> [u8; SEATS] {
        let mut t = self.points;
        for (position, &seat) in standings.iter().enumerate() {
            if let Some(p) = t.get_mut(seat) {
                *p = p.saturating_add(points_for(position));
            }
        }
        t
    }

    /// Scores this round and moves to the next one; `None` after the final round.
    pub fn next(&self, standings: &[usize]) -> Option<Self> {
        (!self.is_final()).then(|| Self {
            round: self.round + 1,
            points: self.totals(standings),
        })
    }
}

/// The first `seats` seats ordered by points, most first. Ties go to the better
/// place in the latest race (`standings`), so the order always matches what players just saw.
pub fn table(totals: &[u8; SEATS], standings: &[usize], seats: usize) -> Vec<usize> {
    let place = |seat: usize| {
        standings
            .iter()
            .position(|&s| s == seat)
            .unwrap_or(usize::MAX)
    };
    let mut order: Vec<usize> = (0..seats.min(SEATS)).collect();
    order.sort_by_key(|&s| (std::cmp::Reverse(totals[s]), place(s)));
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_follow_the_table_and_run_out() {
        assert_eq!(points_for(0), 10);
        assert_eq!(points_for(7), 1);
        assert_eq!(points_for(8), 0);
    }

    #[test]
    fn a_circuit_visits_every_track_in_order_then_ends() {
        let mut c = Circuit::default();
        let mut tracks = vec![c.track()];
        while let Some(next) = c.next(&[0, 1]) {
            c = next;
            tracks.push(c.track());
        }
        assert_eq!(tracks, (0..ROUNDS).collect::<Vec<_>>());
        assert!(c.is_final());
    }

    #[test]
    fn points_accumulate_by_seat() {
        let c = Circuit::default();
        let c = c.next(&[2, 0, 1]).unwrap();
        assert_eq!(&c.points[..3], &[8, 6, 10]);
        let totals = c.totals(&[0, 2, 1]);
        assert_eq!(&totals[..3], &[18, 12, 18]);
    }

    #[test]
    fn table_breaks_ties_by_the_latest_race() {
        let mut totals = [0; SEATS];
        totals[0] = 16;
        totals[1] = 16;
        totals[2] = 20;
        assert_eq!(table(&totals, &[1, 2, 0], 3), vec![2, 1, 0]);
        assert_eq!(table(&totals, &[0, 2, 1], 3), vec![2, 0, 1]);
    }

    #[test]
    fn seats_outside_the_table_are_ignored() {
        let c = Circuit::default();
        assert_eq!(c.totals(&[SEATS + 3, 0])[0], 8);
    }
}
