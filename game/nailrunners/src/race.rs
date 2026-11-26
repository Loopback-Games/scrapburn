//! A race: racers on a grid, countdown, laps, standings, bots and kart-to-kart contact.

use glam::Vec3;
use rivet_ai::{BotDriver, RacingLine};
use rivet_physics::{DriveInput, VehicleParams, VehicleState, collide_karts, step};

use crate::combat::{Combat, CombatEvent};
use crate::controls::HopLatch;
use crate::laps::{LapEvent, LapTracker};
use crate::track::Track;

pub const COUNTDOWN: f32 = 3.0;
const KART_RADIUS: f32 = 1.0;
const BASE_MASS: f32 = 300.0;
const START_BOOST: (f32, f32) = (6.0, 0.9);
const STUCK_SPEED: f32 = 2.0;
const STUCK_SECONDS: f32 = 3.0;
/// Farther than this from the centerline (beyond the barriers) for this long = put back.
const OFF_TRACK_DISTANCE: f32 = crate::track::ROAD_HALF_WIDTH + 18.0;
const OFF_TRACK_SECONDS: f32 = 1.5;
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
    Combat(CombatEvent),
    /// Elimination: racer knocked out in last place, finishing `place`.
    Eliminated {
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
    /// Knocked out of an elimination race: parked, untouchable, ranked by `finish`.
    pub out: bool,
    stuck: f32,
    /// Seconds spent far off the track.
    off_track: f32,
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
    pub combat: Combat,
    /// Racers respawned this tick (their hooks are cancelled).
    respawned: Vec<usize>,
    /// Items and weapons are on (any race with rivals).
    pub combat_on: bool,
    /// Elimination mode: last place is knocked out each time the leader completes a lap.
    pub elimination: bool,
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
        // The player starts at the back of the grid and has to fight through the field.
        let mut spots = grid_spots(track, count);
        if player {
            spots.rotate_right(1);
        }
        let racers = spots
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
                    out: false,
                    stuck: 0.0,
                    off_track: 0.0,
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
            combat: Combat::new(track, count),
            respawned: Vec::new(),
            combat_on: count > 1,
            elimination: false,
        }
    }

    /// An elimination race: everyone drives until one survivor is left.
    pub fn new_elimination(track: &Track, count: usize, player: bool) -> Self {
        let mut race = Self::new(track, count, player, u32::MAX);
        race.elimination = true;
        race
    }

    /// Racers still running in an elimination race (or everyone otherwise).
    fn active(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.racers.len()).filter(|&i| !self.racers[i].out && self.racers[i].finish.is_none())
    }

    /// Knocks out the last active racer whenever the leader starts a new lap; crowns the survivor.
    fn eliminate(&mut self, track: &Track, events: &mut Vec<RaceEvent>) {
        let active: Vec<usize> = self.active().collect();
        if active.len() < 2 {
            return;
        }
        let cuts = (self.racers.len() - active.len()) as u32;
        let leader_laps = active
            .iter()
            .map(|&i| self.racers[i].laps.laps())
            .max()
            .unwrap_or(0);
        if leader_laps <= cuts {
            return;
        }
        let Some(&last) = active
            .iter()
            .min_by(|&&a, &&b| self.progress(a, track).total_cmp(&self.progress(b, track)))
        else {
            return;
        };
        let place = active.len() as u32;
        let r = &mut self.racers[last];
        r.out = true;
        r.finish = Some((place, self.time));
        r.state.velocity = Vec3::ZERO;
        r.prev = r.state.clone();
        self.combat.cancel_for(last);
        events.push(RaceEvent::Eliminated { racer: last, place });
        if active.len() == 2 {
            let survivor = active[if active[0] == last { 1 } else { 0 }];
            self.finished += 1;
            self.racers[survivor].finish = Some((1, self.time));
            events.push(RaceEvent::Finish {
                racer: survivor,
                place: 1,
            });
            if self.racers[survivor].bot.is_none() {
                self.racers[survivor].bot = Some(BotDriver::new(0.85, 0.0));
            }
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
            let (oa, ob) = (self.racers[a].out, self.racers[b].out);
            match (fa, fb) {
                // Knocked-out racers rank below everyone still running.
                _ if oa != ob => oa.cmp(&ob),
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
        r.off_track = 0.0;
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
            if r.out {
                continue;
            }
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
            let p = r.state.position;
            let far = crate::terrain::distance_to(&track.centerline, p.x, p.z) > OFF_TRACK_DISTANCE;
            r.off_track = if far { r.off_track + dt } else { 0.0 };
            if p.y < track.fall_y || r.stuck > STUCK_SECONDS || r.off_track > OFF_TRACK_SECONDS {
                Self::respawn(r, track);
                self.respawned.push(i);
            }
        }

        // Kart-to-kart contact, resolved pairwise in index order (deterministic).
        for a in 0..self.racers.len() {
            for b in a + 1..self.racers.len() {
                let (left, right) = self.racers.split_at_mut(b);
                let (ra, rb) = (&mut left[a], &mut right[0]);
                if ra.out || rb.out {
                    continue;
                }
                collide_karts(&mut ra.state, ra.mass, &mut rb.state, rb.mass, KART_RADIUS);
            }
        }
        if racing && self.elimination {
            self.eliminate(track, &mut events);
        }
        for i in std::mem::take(&mut self.respawned) {
            self.combat.cancel_for(i);
        }
        if racing && self.combat_on {
            // Weapons only see racers still racing (finishers are on their cool-down lap).
            let standings: Vec<usize> = self
                .standings(track)
                .into_iter()
                .filter(|&i| self.racers[i].finish.is_none())
                .collect();
            let ce = self.combat.step(
                &mut self.racers,
                &standings,
                &self.line,
                &track.collision,
                dt,
            );
            events.extend(ce.into_iter().map(RaceEvent::Combat));
        }
        events
    }

    /// Racer `i` uses its held item.
    pub fn fire(&mut self, i: usize) {
        self.combat.fire(i);
    }

    /// Teleports racer `i` to its last checkpoint (player respawn button).
    pub fn respawn_racer(&mut self, i: usize, track: &Track) {
        Self::respawn(&mut self.racers[i], track);
        self.combat.cancel_for(i);
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
    fn falling_off_the_skyway_respawns_quickly() {
        let t = crate::track::build_track(&crate::track::TRACKS[2]);
        let mut race = Race::new(&t, 1, false, 3);
        race.phase = Phase::Racing;
        // Over the void, just past the rail.
        let c = t.centerline[40];
        let f = (t.centerline[41] - c).normalize();
        race.racers[0].state.position = c + f.cross(Vec3::Y).normalize() * 12.0 + Vec3::Y * 2.0;
        race.racers[0].prev = race.racers[0].state.clone();
        let back = (1..=120).find(|_| {
            race.step(&t, DriveInput::default(), 1.0 / 60.0);
            let p = race.racers[0].state.position;
            crate::terrain::distance_to(&t.centerline, p.x, p.z) < crate::track::ROAD_HALF_WIDTH
        });
        assert!(
            back.is_some(),
            "still falling after 2 s (y = {})",
            race.racers[0].state.position.y
        );
    }

    #[test]
    fn elimination_cuts_last_place_each_lap_until_one_survives() {
        let t = track();
        let mut race = Race::new_elimination(t, 4, false);
        let mut cuts = Vec::new();
        let mut winner = None;
        let mut parked: Vec<(usize, Vec3)> = Vec::new();
        for _ in 0..(240 * 60) {
            for e in race.step(t, DriveInput::default(), 1.0 / 60.0) {
                match e {
                    RaceEvent::Eliminated { racer, place } => {
                        cuts.push(place);
                        parked.push((racer, race.racers[racer].state.position));
                    }
                    RaceEvent::Finish { racer, place } => winner = Some((racer, place)),
                    _ => {}
                }
            }
            for &(r, p) in &parked {
                assert_eq!(
                    race.racers[r].state.position, p,
                    "eliminated karts stay parked"
                );
            }
            if winner.is_some() {
                break;
            }
        }
        assert_eq!(cuts, vec![4, 3, 2], "last place out each lap");
        let (w, place) = winner.expect("a survivor wins");
        assert_eq!(place, 1);
        let st = race.standings(t);
        assert_eq!(st[0], w);
        let places: Vec<u32> = st
            .iter()
            .map(|&i| race.racers[i].finish.unwrap().0)
            .collect();
        assert_eq!(places, vec![1, 2, 3, 4]);
    }

    #[test]
    fn eliminated_racers_rank_below_everyone_still_running() {
        let t = track();
        let mut race = Race::new_elimination(t, 3, false);
        race.racers[2].finish = Some((3, 10.0));
        race.racers[2].out = true;
        let st = race.standings(t);
        assert_eq!(st[2], 2);
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
        let mut boosted_at_go = false;
        for _ in 0..120 {
            let ev = r.step(t, full, 1.0 / 60.0);
            if ev.contains(&RaceEvent::Go) {
                go = true;
                boosted_at_go = r.racers[0].state.boost_timer > 0.0;
            }
        }
        assert!(go, "GO fires");
        assert!(boosted_at_go, "holding throttle at GO gives a start boost");
        assert!(
            r.racers[0].state.position.distance(start) > 5.0,
            "and the race starts"
        );
    }

    #[test]
    fn player_starts_last() {
        let t = track();
        let r = Race::new(t, 8, true, 3);
        assert_eq!(r.position_of(0, t), 8, "player on the back row");
    }

    #[test]
    fn out_of_bounds_karts_are_put_back() {
        let t = track();
        let mut r = Race::new(t, 2, true, 3);
        r.phase = Phase::Racing;
        let lost = t.centerline[100] + glam::Vec3::new(0.0, 5.0, 60.0);
        r.racers[0].state = rivet_physics::VehicleState::spawn(lost, 0.0);
        r.racers[0].prev = r.racers[0].state.clone();
        for _ in 0..120 {
            r.step(t, DriveInput::default(), 1.0 / 60.0);
        }
        let p = r.racers[0].state.position;
        let d = crate::terrain::distance_to(&t.centerline, p.x, p.z);
        assert!(
            d < crate::track::ROAD_HALF_WIDTH,
            "still {d:.1} m off the track"
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
        for racer in &mut r.racers {
            let pos = racer.state.position;
            racer.laps.sync_to(pos, &t.checkpoints);
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
