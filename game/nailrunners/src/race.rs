//! A race: racers on a grid, countdown, laps, standings, bots and kart-to-kart contact.

use glam::Vec3;
use rivet_ai::{BotDriver, RacingLine};
use rivet_physics::{DriveInput, VehicleParams, VehicleState, collide_karts, step};

use crate::controls::HopLatch;
use crate::laps::{LapEvent, LapTracker};
use crate::track::Track;

pub const COUNTDOWN: f32 = 3.0;
const KART_RADIUS: f32 = 1.0;
const BASE_MASS: f32 = 300.0;
const START_BOOST: (f32, f32) = (6.0, 0.9);
const FALL_LIMIT_Y: f32 = -30.0;
const STUCK_SPEED: f32 = 2.0;
const STUCK_SECONDS: f32 = 3.0;
const SPAWN_LIFT: f32 = 0.6;
/// Catch-up range: bots far behind the player speed up, far ahead slow down.
const CATCH_UP: (f32, f32) = (0.95, 1.06);
const CATCH_UP_DISTANCE: f32 = 6.0; // in checkpoints

/// Eight palette tints for the field (multiplied into the kart texture).
pub const TINTS: [[f32; 3]; 8] = [
    [1.0, 1.0, 1.0],
    [0.55, 0.85, 1.25],
    [1.25, 0.6, 0.55],
    [0.7, 1.2, 0.6],
    [1.2, 1.05, 0.5],
    [1.05, 0.65, 1.2],
    [0.6, 0.6, 0.65],
    [1.3, 0.85, 0.45],
];
pub const NAMES: [&str; 8] = [
    "YOU", "RUST", "VOLT", "MOSS", "BRASS", "HEX", "ASH", "EMBER",
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RaceEvent {
    Go,
    Lap {
        racer: usize,
        number: u32,
        time: f64,
    },
    Finish {
        racer: usize,
        place: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Phase {
    Countdown(f32),
    Racing,
}

pub struct Racer {
    pub state: VehicleState,
    pub prev: VehicleState,
    pub params: VehicleParams,
    pub laps: LapTracker,
    /// `None` = the human player.
    pub bot: Option<BotDriver>,
    pub mass: f32,
    pub tint: [f32; 3],
    pub name: &'static str,
    pub finish: Option<(u32, f64)>,
    pub last_lap: Option<f64>,
    pub best_lap: Option<f64>,
    stuck: f32,
    hop: HopLatch,
}

pub struct Race {
    pub racers: Vec<Racer>,
    pub phase: Phase,
    /// Seconds since GO.
    pub time: f64,
    pub laps_to_win: u32,
    pub player: Option<usize>,
    line: RacingLine,
    finished: u32,
}

/// Grid positions behind the start line: two columns, staggered rows, facing forward.
pub fn grid_spots(track: &Track, n: usize) -> Vec<(Vec3, f32)> {
    let cl = &track.centerline;
    let len = cl.len() - 1;
    (0..n)
        .map(|k| {
            let row = k / 2;
            let back = 4 + row * 4 + (k % 2) * 2; // points (2 m each) behind the line
            let i = (len - back % len) % len;
            let f = (cl[(i + 1) % len] - cl[(i + len - 1) % len]).normalize();
            let right = f.cross(Vec3::Y).normalize();
            let side = if k % 2 == 0 { -3.2 } else { 3.2 };
            (cl[i] + right * side, f.x.atan2(f.z))
        })
        .collect()
}

impl Race {
    /// `count` racers; racer 0 is the player when `player` is true.
    pub fn new(track: &Track, count: usize, player: bool, laps_to_win: u32) -> Self {
        let line = RacingLine::new(track.centerline.clone());
        let racers = grid_spots(track, count)
            .into_iter()
            .enumerate()
            .map(|(k, (p, yaw))| {
                let state = VehicleState::spawn(p + Vec3::Y * SPAWN_LIFT, yaw);
                let is_player = player && k == 0;
                // Deterministic field spread: skill, lane and weight vary per seat.
                let seat = k as f32;
                let skill = 0.9 + 0.08 * ((seat * 2.39).sin() * 0.5 + 0.5);
                let lane = ((seat * 1.7).sin() * 2.5).clamp(-3.0, 3.0);
                let mut laps = LapTracker::new(track.checkpoints.len());
                laps.start(0.0);
                Racer {
                    prev: state.clone(),
                    state,
                    params: VehicleParams::default(),
                    laps,
                    bot: (!is_player).then(|| BotDriver::new(skill, lane)),
                    mass: BASE_MASS * (1.0 + 0.15 * (seat * 3.1).cos()),
                    tint: TINTS[k % TINTS.len()],
                    name: if is_player {
                        NAMES[0]
                    } else {
                        NAMES[1 + (k + NAMES.len() - 2) % (NAMES.len() - 1)]
                    },
                    finish: None,
                    last_lap: None,
                    best_lap: None,
                    stuck: 0.0,
                    hop: HopLatch::default(),
                }
            })
            .collect();
        Self {
            racers,
            phase: Phase::Countdown(COUNTDOWN),
            time: 0.0,
            laps_to_win,
            player: player.then_some(0),
            line,
            finished: 0,
        }
    }

    pub fn progress(&self, i: usize, track: &Track) -> f32 {
        let r = &self.racers[i];
        r.laps.progress(r.state.position, &track.checkpoints)
    }

    /// Racer indices from first to last: finishers by place, then by progress.
    pub fn standings(&self, track: &Track) -> Vec<usize> {
        let mut idx: Vec<usize> = (0..self.racers.len()).collect();
        idx.sort_by(|&a, &b| {
            let (fa, fb) = (self.racers[a].finish, self.racers[b].finish);
            match (fa, fb) {
                (Some((pa, _)), Some((pb, _))) => pa.cmp(&pb),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => self.progress(b, track).total_cmp(&self.progress(a, track)),
            }
        });
        idx
    }

    /// 1-based race position of racer `i`.
    pub fn position_of(&self, i: usize, track: &Track) -> usize {
        self.standings(track)
            .iter()
            .position(|&r| r == i)
            .map_or(0, |p| p + 1)
    }

    fn respawn(r: &mut Racer, track: &Track) {
        let cp = track.checkpoints[r.laps.last_checkpoint()];
        r.state = VehicleState::spawn(
            cp.center + Vec3::Y * SPAWN_LIFT,
            cp.forward.x.atan2(cp.forward.z),
        );
        r.prev = r.state.clone();
        r.stuck = 0.0;
    }

    /// One fixed tick. `player_input.hop` is latched until a tick consumes it.
    pub fn step(&mut self, track: &Track, player_input: DriveInput, dt: f32) -> Vec<RaceEvent> {
        let mut events = Vec::new();
        let racing = match self.phase {
            Phase::Countdown(left) => {
                let left = left - dt;
                if left <= 0.0 {
                    self.phase = Phase::Racing;
                    events.push(RaceEvent::Go);
                    for (k, r) in self.racers.iter_mut().enumerate() {
                        // A good launch: the player by holding throttle, bots by skill.
                        let good = match r.bot {
                            None => player_input.throttle > 0.5,
                            Some(ref b) => b.skill > 0.93 || k % 3 == 1,
                        };
                        if good {
                            r.state.boost_speed = START_BOOST.0;
                            r.state.boost_timer = START_BOOST.1;
                        }
                    }
                    true
                } else {
                    self.phase = Phase::Countdown(left);
                    false
                }
            }
            Phase::Racing => true,
        };
        if racing {
            self.time += dt as f64;
        }

        let player_progress = self.player.map(|p| self.progress(p, track));
        let positions: Vec<Vec3> = self.racers.iter().map(|r| r.state.position).collect();
        for i in 0..self.racers.len() {
            let progress = self.progress(i, track);
            let r = &mut self.racers[i];
            r.prev = r.state.clone();
            let input = if !racing {
                DriveInput::default()
            } else if let Some(bot) = r.bot.as_mut() {
                let scale = player_progress.map_or(1.0, |pp| {
                    let gap = ((pp - progress) / CATCH_UP_DISTANCE).clamp(-1.0, 1.0);
                    if gap > 0.0 {
                        1.0 + (CATCH_UP.1 - 1.0) * gap
                    } else {
                        1.0 + (1.0 - CATCH_UP.0) * gap
                    }
                });
                let others: Vec<Vec3> = positions
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, p)| *p)
                    .collect();
                bot.drive(&r.state, &r.params, &self.line, &others, scale)
            } else {
                r.hop.press(player_input.hop);
                DriveInput {
                    hop: r.hop.take(),
                    ..player_input
                }
            };
            step(&mut r.state, &r.params, input, &track.collision, dt);

            if racing {
                if let Some(LapEvent::Lap { number, time }) = r.laps.update(
                    r.prev.position,
                    r.state.position,
                    &track.checkpoints,
                    self.time,
                ) {
                    r.last_lap = Some(time);
                    r.best_lap = Some(r.best_lap.map_or(time, |b| b.min(time)));
                    events.push(RaceEvent::Lap {
                        racer: i,
                        number,
                        time,
                    });
                    if number >= self.laps_to_win && r.finish.is_none() {
                        self.finished += 1;
                        r.finish = Some((self.finished, self.time));
                        events.push(RaceEvent::Finish {
                            racer: i,
                            place: self.finished,
                        });
                        // A finished player hands the wheel to the autopilot for the cool-down lap.
                        if r.bot.is_none() {
                            r.bot = Some(BotDriver::new(0.85, 0.0));
                        }
                    }
                }
                let slow = r.state.speed().abs() < STUCK_SPEED && r.bot.is_some();
                r.stuck = if slow { r.stuck + dt } else { 0.0 };
            }
            if r.state.position.y < FALL_LIMIT_Y || r.stuck > STUCK_SECONDS {
                Self::respawn(r, track);
            }
        }

        // Kart-to-kart contact, resolved pairwise in index order (deterministic).
        for a in 0..self.racers.len() {
            for b in a + 1..self.racers.len() {
                let (left, right) = self.racers.split_at_mut(b);
                let (ra, rb) = (&mut left[a], &mut right[0]);
                collide_karts(&mut ra.state, ra.mass, &mut rb.state, rb.mass, KART_RADIUS);
            }
        }
        events
    }

    /// Teleports racer `i` to its last checkpoint (player respawn button).
    pub fn respawn_racer(&mut self, i: usize, track: &Track) {
        Self::respawn(&mut self.racers[i], track);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::build_graybox_loop;
    use rivet_physics::DriveInput;
    use std::sync::OnceLock;

    fn track() -> &'static Track {
        static T: OnceLock<Track> = OnceLock::new();
        T.get_or_init(build_graybox_loop)
    }

    #[test]
    fn grid_spots_distinct_and_on_road() {
        let t = track();
        let grid = grid_spots(t, 8);
        assert_eq!(grid.len(), 8);
        for (i, (p, _)) in grid.iter().enumerate() {
            let hit = t
                .collision
                .raycast(*p + glam::Vec3::Y * 2.0, glam::Vec3::NEG_Y, 5.0)
                .expect("on track");
            assert_eq!(hit.surface, rivet_physics::Surface::Road, "spot {i} at {p}");
            for (q, _) in &grid[i + 1..] {
                assert!(p.distance(*q) > 2.5, "spots overlap");
            }
        }
    }

    #[test]
    fn countdown_freezes_then_go() {
        let t = track();
        let mut r = Race::new(t, 8, true, 3);
        let start = r.racers[0].state.position;
        let full = DriveInput {
            throttle: 1.0,
            ..Default::default()
        };
        let mut go = false;
        for _ in 0..(2.9 * 60.0) as usize {
            go |= r.step(t, full, 1.0 / 60.0).contains(&RaceEvent::Go);
        }
        assert!(
            !go && r.racers[0].state.position.distance(start) < 1.0,
            "frozen during countdown"
        );
        for _ in 0..120 {
            go |= r.step(t, full, 1.0 / 60.0).contains(&RaceEvent::Go);
        }
        assert!(go, "GO fires");
        assert!(
            r.racers[0].state.position.distance(start) > 10.0,
            "and the race starts"
        );
        assert!(
            r.racers[0].state.boost_timer > 0.0 || r.racers[0].state.speed() > 20.0,
            "holding throttle at GO gives a start boost"
        );
    }

    #[test]
    fn standings_follow_progress() {
        let t = track();
        let mut r = Race::new(t, 3, false, 3);
        // Teleport racers along the centerline: 2 ahead of 0 ahead of 1.
        let cl = &t.centerline;
        r.racers[1].state.position = cl[10];
        r.racers[0].state.position = cl[60];
        r.racers[2].state.position = cl[200];
        for k in 0..3 {
            let pos = r.racers[k].state.position;
            r.racers[k].laps.sync_to(pos, &t.checkpoints);
        }
        assert_eq!(r.standings(t), vec![2, 0, 1]);
    }

    #[test]
    fn bots_finish_a_race() {
        let t = track();
        let mut r = Race::new(t, 8, false, 1);
        let mut finished = Vec::new();
        for _ in 0..(90.0 * 60.0) as usize {
            for e in r.step(t, DriveInput::default(), 1.0 / 60.0) {
                if let RaceEvent::Finish { racer, place } = e {
                    finished.push((racer, place));
                }
            }
            if finished.len() == 8 {
                break;
            }
        }
        assert!(
            finished.len() >= 6,
            "only {} of 8 bots finished a lap in 90 s",
            finished.len()
        );
        let mut places: Vec<_> = finished.iter().map(|f| f.1).collect();
        places.sort();
        places.dedup();
        assert_eq!(places.len(), finished.len(), "places are unique");
    }
}
