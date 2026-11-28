//! A race: racers on a grid, countdown, laps, standings, bots and kart-to-kart contact.

use glam::Vec3;
use rivet_ai::{BotDriver, RacingLine};
use rivet_physics::{DriveInput, VehicleParams, VehicleState, collide_karts, step_graded};

use crate::combat::{Combat, CombatEvent};
use crate::controls::HopLatch;
use crate::laps::{LapEvent, LapTracker};
use crate::track::Track;

pub const COUNTDOWN: f32 = 3.0;
/// Preserve the original kart-to-kart buffer around each wall contact sphere.
const CONTACT_BUFFER: f32 = 1.25;
/// Mass of an average kart (kg); seats vary around it.
pub const BASE_MASS: f32 = 300.0;
const START_BOOST: (f32, f32) = (6.0, 0.9);
/// A bot slower than this (m/s) for `STUCK_SECONDS` is put back at its last checkpoint: nose
/// against a rock or pillar it can't see its way round. Long enough for a back-out or a
/// turn-around (both keep moving), short enough not to sit out a chunk of the lap.
const STUCK_SPEED: f32 = 2.0;
const STUCK_SECONDS: f32 = 2.0;
/// Farther than this from the centerline (beyond the barriers) for this long = put back.
const OFF_TRACK_DISTANCE: f32 = crate::track::ROAD_HALF_WIDTH + 18.0;
const OFF_TRACK_SECONDS: f32 = 1.5;
const SPAWN_LIFT: f32 = 0.6;
/// Catch-up range: bots far behind the player speed up, far ahead slow down.
const CATCH_UP: (f32, f32) = (0.95, 1.06);
const CATCH_UP_DISTANCE: f32 = 6.0; // in checkpoints
/// How far (m) the kart's center may stray past the road edge before offroad ground slows it:
/// a wheel-track plus a kart-width of slack, so clipping a curb or hanging the tail over the
/// edge mid-drift is free.
const OFFROAD_GRACE: f32 = 2.5;
/// Beyond the grace, the penalty ramps up over this many meters to its full strength (the
/// outer edge of Dustbowl's sand shoulder, where the open desert starts).
const OFFROAD_RAMP: f32 = 5.0;

/// Offroad penalty grade (0..1) for a kart `distance` m from the track centerline: zero within
/// the grace past the road edge, then easing in to 1 at full depth.
fn offroad_grade(distance: f32) -> f32 {
    let past = distance - crate::track::ROAD_HALF_WIDTH - OFFROAD_GRACE;
    let t = (past / OFFROAD_RAMP).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// One physics tick for `state`, with the offroad penalty graded by distance from the road.
fn step_kart(
    state: &mut VehicleState,
    params: &VehicleParams,
    input: DriveInput,
    track: &Track,
    dt: f32,
) {
    let p = state.position;
    let grade = offroad_grade(crate::terrain::distance_to(&track.centerline, p.x, p.z));
    step_graded(state, params, input, &track.collision, grade, dt);
}

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
    /// Signed driver input for the visible steering linkage.
    pub steering: f32,
    pub design: usize,
    pub state: VehicleState,
    pub prev: VehicleState,
    pub params: VehicleParams,
    pub laps: LapTracker,
    /// `None` = the human player.
    pub bot: Option<BotDriver>,
    pub mass: f32,
    pub tint: [f32; 3],
    pub name: std::borrow::Cow<'static, str>,
    pub finish: Option<(u32, f64)>,
    pub last_lap: Option<f64>,
    pub best_lap: Option<f64>,
    /// Knocked out of an elimination race: parked, untouchable, ranked by `finish`.
    pub out: bool,
    /// Driven by a player on another machine: the host takes this kart's state from the
    /// network instead of simulating it.
    pub remote: bool,
    /// Weapon hits the host has applied to this (remote) kart, and the latest hit's spin
    /// seconds and kept speed share, so the owner's machine can apply it.
    pub hits: u32,
    pub last_hit: (f32, f32),
    incoming: Option<VehicleState>,
    /// Seconds spent moving against the track direction.
    wrong: f32,
    stuck: f32,
    /// Seconds spent far off the track.
    off_track: f32,
    hop: HopLatch,
}

impl Racer {
    pub fn equip(&mut self, design: usize) {
        self.design = design;
        (
            self.params.half_track,
            self.params.half_wheelbase,
            self.params.body_radius,
        ) = crate::kart::KARTS[design].contacts();
    }
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

/// Moving backwards along the track faster than this (m/s) counts toward WRONG WAY.
const WRONG_WAY_SPEED: f32 = 3.0;
/// Seconds of that before the warning shows.
const WRONG_WAY_SECONDS: f32 = 0.8;
/// A bot (never a human) still going the wrong way after this many seconds is put back at
/// its last checkpoint: a safety net under the driver's own turn-around.
const WRONG_WAY_RESPAWN_SECONDS: f32 = 4.0;

/// Brakes on during the countdown: a kart may settle on its suspension but not roll.
fn hold_on_grid(r: &mut Racer) {
    r.state.position.x = r.prev.position.x;
    r.state.position.z = r.prev.position.z;
    r.state.velocity.x = 0.0;
    r.state.velocity.z = 0.0;
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
                    steering: 0.,
                    design: 0,
                    prev: state.clone(),
                    state,
                    params: VehicleParams::default(),
                    laps,
                    bot: (!is_player).then(|| BotDriver::new(skill, lane)),
                    mass: BASE_MASS * (1.0 + 0.15 * (seat * 3.1).cos()),
                    tint: TINTS[k % TINTS.len()],
                    name: std::borrow::Cow::Borrowed(if is_player {
                        NAMES[0]
                    } else {
                        NAMES[1 + (k + NAMES.len() - 2) % (NAMES.len() - 1)]
                    }),
                    finish: None,
                    last_lap: None,
                    best_lap: None,
                    out: false,
                    remote: false,
                    hits: 0,
                    last_hit: (0.0, 1.0),
                    incoming: None,
                    wrong: 0.0,
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

    pub fn equip_grid(&mut self, equipped: usize) {
        for (seat, racer) in self.racers.iter_mut().enumerate() {
            racer.equip(if self.player == Some(seat) || racer.remote {
                equipped
            } else {
                crate::kart::rival_design(equipped, seat, racer.mass / BASE_MASS)
            });
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

    /// Racer `i` has been driving against the track for a while (show WRONG WAY).
    pub fn wrong_way(&self, i: usize) -> bool {
        self.racers
            .get(i)
            .is_some_and(|r| r.wrong > WRONG_WAY_SECONDS)
    }

    /// Accumulates time spent moving backwards along the track.
    fn track_direction(line: &RacingLine, r: &mut Racer, dt: f32) {
        let along = r
            .state
            .velocity
            .dot(line.tangent(line.nearest(r.state.position)));
        r.wrong = if along < -WRONG_WAY_SPEED {
            r.wrong + dt
        } else {
            (r.wrong - dt * 3.0).max(0.0)
        };
    }

    /// Seat `i` belongs to a player on another machine.
    pub fn set_remote(&mut self, i: usize) {
        let r = &mut self.racers[i];
        r.remote = true;
        r.bot = None;
    }

    /// A remote player left: their kart carries on as a bot.
    pub fn make_bot(&mut self, i: usize) {
        if let Some(r) = self.racers.get_mut(i) {
            r.remote = false;
            r.incoming = None;
            if r.finish.is_none() {
                r.bot = Some(BotDriver::new(0.9, 0.0));
            }
        }
    }

    /// The latest kart state reported by a remote player; applied on the next tick.
    pub fn push_remote_state(&mut self, i: usize, state: VehicleState) {
        if let Some(r) = self.racers.get_mut(i)
            && r.remote
        {
            r.incoming = Some(state);
        }
    }

    /// Every kart's spin-out timer (to detect hits made during a tick).
    pub fn spins(&self) -> Vec<f32> {
        self.racers.iter().map(|r| r.state.spin).collect()
    }

    /// Records hits that started a fresh spin on remote karts during this tick.
    pub fn note_remote_hits(&mut self, spins_before: &[f32], vel_before: &[Vec3]) {
        for (i, r) in self.racers.iter_mut().enumerate() {
            if !r.remote || r.state.spin <= spins_before[i] + 0.05 {
                continue;
            }
            let (v0, v1) = (vel_before[i].length(), r.state.velocity.length());
            let keep = if v0 > 0.5 {
                (v1 / v0).clamp(0.0, 1.0)
            } else {
                1.0
            };
            r.hits = r.hits.wrapping_add(1);
            r.last_hit = (r.state.spin, keep);
        }
    }

    /// Client side: one tick for this machine's own kart only (everything else arrives in
    /// snapshots). Handles respawns locally; pushes off other karts without moving them.
    pub fn step_own(&mut self, track: &Track, i: usize, input: DriveInput, dt: f32) {
        let racing = self.phase == Phase::Racing;
        let others: Vec<(Vec3, VehicleState, f32, f32)> = self
            .racers
            .iter()
            .enumerate()
            .filter(|&(j, o)| j != i && !o.out)
            .map(|(_, o)| {
                (
                    o.state.position,
                    o.state.clone(),
                    o.mass,
                    o.params.body_radius,
                )
            })
            .collect();
        let line = &self.line;
        let r = &mut self.racers[i];
        r.prev = r.state.clone();
        if r.out {
            return;
        }
        let input = if !racing {
            DriveInput::default()
        } else if let Some(bot) = r.bot.as_mut() {
            let near: Vec<Vec3> = others.iter().map(|o| o.0).collect();
            bot.drive(&r.state, &r.params, line, &near, 1.0, dt)
        } else {
            r.hop.press(input.hop);
            DriveInput {
                hop: r.hop.take(),
                ..input
            }
        };
        r.steering = input.steer;
        step_kart(&mut r.state, &r.params, input, track, dt);
        if !racing {
            hold_on_grid(r);
        } else {
            Self::track_direction(line, r, dt);
        }
        let p = r.state.position;
        let far = crate::terrain::distance_to(&track.centerline, p.x, p.z) > OFF_TRACK_DISTANCE;
        r.off_track = if far { r.off_track + dt } else { 0.0 };
        if p.y < track.fall_y || r.off_track > OFF_TRACK_SECONDS {
            Self::respawn(r, track);
        }
        for (_, mut other, mass, radius) in others {
            collide_karts(
                &mut r.state,
                r.mass,
                &mut other,
                mass,
                (r.params.body_radius + radius) * 0.5 * CONTACT_BUFFER,
            );
        }
    }

    pub fn progress(&self, i: usize, track: &Track) -> f32 {
        let r = &self.racers[i];
        r.laps
            .progress(r.state.position, &track.checkpoints, &track.centerline)
    }

    /// Racer indices from first to last: finishers by place, then by progress.
    pub fn standings(&self, track: &Track) -> Vec<usize> {
        // Progress projects onto the centerline; do that once per racer, not per comparison.
        let progress: Vec<f32> = (0..self.racers.len())
            .map(|i| self.progress(i, track))
            .collect();
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
                (None, None) => progress[b].total_cmp(&progress[a]),
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
        r.wrong = 0.0;
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
            if racing {
                Self::track_direction(&self.line, r, dt);
            }
            r.prev = r.state.clone();
            if r.out {
                continue;
            }
            let input = if r.remote {
                if let Some(s) = r.incoming.take() {
                    r.state = s;
                }
                None
            } else if !racing {
                Some(DriveInput::default())
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
                Some(bot.drive(&r.state, &r.params, &self.line, &others, scale, dt))
            } else {
                r.hop.press(player_input.hop);
                Some(DriveInput {
                    hop: r.hop.take(),
                    ..player_input
                })
            };
            if let Some(input) = input {
                r.steering = input.steer;
                step_kart(&mut r.state, &r.params, input, track, dt);
                if !racing {
                    hold_on_grid(r);
                }
            }

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
                        // A finished player hands the wheel to the autopilot for the cool-down lap
                        // (a remote player's own machine does that for its kart).
                        if r.bot.is_none() && !r.remote {
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
            let bot_wrong_way =
                r.bot.is_some() && self.player != Some(i) && r.wrong > WRONG_WAY_RESPAWN_SECONDS;
            let lost = p.y < track.fall_y
                || r.stuck > STUCK_SECONDS
                || r.off_track > OFF_TRACK_SECONDS
                || bot_wrong_way;
            // Remote karts respawn on their own machine.
            if lost && !r.remote {
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
                collide_karts(
                    &mut ra.state,
                    ra.mass,
                    &mut rb.state,
                    rb.mass,
                    (ra.params.body_radius + rb.params.body_radius) * 0.5 * CONTACT_BUFFER,
                );
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
            let (spins, vels): (Vec<f32>, Vec<Vec3>) = self
                .racers
                .iter()
                .map(|r| (r.state.spin, r.state.velocity))
                .unzip();
            let ce = self.combat.step(
                &mut self.racers,
                &standings,
                &self.line,
                &track.collision,
                dt,
            );
            self.note_remote_hits(&spins, &vels);
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
    fn driving_against_the_track_raises_wrong_way() {
        let t = track();
        let mut race = Race::new(t, 1, true, 3);
        race.phase = Phase::Racing;
        assert!(!race.wrong_way(0));
        let r = &mut race.racers[0];
        r.state.rotation *= glam::Quat::from_rotation_y(std::f32::consts::PI);
        for _ in 0..30 {
            race.step(
                t,
                DriveInput {
                    throttle: 1.0,
                    ..Default::default()
                },
                1.0 / 60.0,
            );
        }
        assert!(
            !race.wrong_way(0),
            "not instantly (a brief wobble isn't wrong way)"
        );
        for _ in 0..60 {
            race.step(
                t,
                DriveInput {
                    throttle: 1.0,
                    ..Default::default()
                },
                1.0 / 60.0,
            );
        }
        assert!(
            race.wrong_way(0),
            "after a second of driving backwards along the track"
        );
        // Turning back around clears it.
        race.racers[0].state.rotation *= glam::Quat::from_rotation_y(std::f32::consts::PI);
        race.racers[0].state.velocity = -race.racers[0].state.velocity;
        for _ in 0..30 {
            race.step(
                t,
                DriveInput {
                    throttle: 1.0,
                    ..Default::default()
                },
                1.0 / 60.0,
            );
        }
        assert!(!race.wrong_way(0));
    }

    #[test]
    fn a_bot_knocked_backwards_recovers_and_races_on() {
        for track_i in 0..3 {
            let t = crate::track::build_track(&crate::track::TRACKS[track_i]);
            let mut race = Race::new(&t, 4, false, 3);
            for _ in 0..(12 * 60) {
                race.step(&t, DriveInput::default(), 1.0 / 60.0);
            }
            // A big crash: bot 1 flipped around, moving backwards along the track.
            let r = &mut race.racers[1];
            let back = r.state.rotation * glam::Quat::from_rotation_y(std::f32::consts::PI);
            r.state.rotation = back;
            r.state.velocity = back * Vec3::Z * 22.0;
            let p0 = race.progress(1, &t);
            for _ in 0..(8 * 60) {
                race.step(&t, DriveInput::default(), 1.0 / 60.0);
            }
            let p1 = race.progress(1, &t);
            let r = &race.racers[1];
            let i = race.line.nearest(r.state.position);
            let along = (r.state.rotation * Vec3::Z).dot(race.line.tangent(i));
            assert!(
                along > 0.5,
                "track {track_i}: still facing the wrong way ({along:.2})"
            );
            assert!(
                p1 > p0 + 0.5,
                "track {track_i}: no progress after recovering ({p0:.2} -> {p1:.2})"
            );
        }
    }

    /// How a wreck leaves a bot.
    #[derive(Debug, Clone, Copy)]
    struct Wreck {
        /// Centerline index the bot lands at.
        at: usize,
        /// Heading relative to the track direction (degrees, + = left).
        heading: f32,
        /// Lateral offset from the centerline (m, + = right).
        lateral: f32,
        /// Speed along the new heading (m/s); negative = rolling backwards.
        speed: f32,
        /// Extra velocity along the track direction (m/s; negative = knocked back down it).
        shove: f32,
        /// Spun out by a hit (a one-second spin keeping 70% of the speed).
        spin: bool,
        /// Height above the road (m): airborne.
        lift: f32,
    }

    /// The ways a wreck leaves a bot, as (lateral, speed, shove, spin, lift). `HARD_WRECKS` is
    /// the slice that has caught regressions; the rest only run in `wreck_sweep`.
    const WRECK_MODES: [(f32, f32, f32, bool, f32); 8] = [
        (0.0, 0.0, 0.0, false, 0.0),
        (0.0, 22.0, 0.0, false, 0.0),
        // Against a barrier, still rolling (a turn-around toward the barrier scraped along it).
        (5.5, 10.0, 0.0, false, 0.0),
        (-5.5, 10.0, 0.0, false, 0.0),
        // Spun out while knocked back down the track, or flung along it.
        (0.0, 0.0, -20.0, true, 0.0),
        (0.0, 0.0, 20.0, true, 0.0),
        // Rolling backwards.
        (0.0, -8.0, 0.0, false, 0.0),
        // Airborne.
        (0.0, 15.0, 0.0, false, 3.0),
    ];
    const HARD_WRECKS: std::ops::Range<usize> = 2..7;
    /// Seconds a wrecked bot gets to sort itself out, and then to show it is racing again.
    const WRECK_SETTLE: f32 = 5.0;
    const WRECK_WATCH: f32 = 4.0;
    /// Centerline points (2 m) per second a recovered bot must cover: 12 m/s on average, a
    /// third of racing speed, so a respawn or a slow turn-around still passes but rolling
    /// backwards or scraping a wall the wrong way does not.
    const WRECK_MIN_RATE: f32 = 6.0;

    /// Puts racer 0 of a one-bot race into `w` (lap state as if it had driven there), runs
    /// `settle + watch` seconds, and returns the centerline points it gained per second over
    /// the last `watch` seconds, and whether it shows WRONG WAY at the end.
    fn run_wreck(t: &Track, w: Wreck, settle: f32, watch: f32) -> (f32, bool) {
        let mut race = Race::new(t, 1, false, 99);
        race.phase = Phase::Racing;
        let line = race.line.clone();
        let n = line.len();
        let i = w.at % n;
        let tan = line.tangent(i);
        let yaw = tan.x.atan2(tan.z) + w.heading.to_radians();
        let ground = t.centerline[i] + line.right(i) * w.lateral;
        let mut s = VehicleState::spawn(ground + Vec3::Y * (SPAWN_LIFT + w.lift), yaw);
        s.velocity = s.forward() * w.speed + tan * w.shove;
        if w.spin {
            s.spin_out(1.0, 0.7);
        }
        // The first checkpoint past the wreck is next (wrapping to the start/finish line).
        let next = t
            .checkpoints
            .iter()
            .position(|c| line.nearest(c.center) > i)
            .unwrap_or(0);
        let r = &mut race.racers[0];
        r.laps.set_net_state(next as u16, 1, 0.0);
        r.prev = s.clone();
        r.state = s;
        let dt = 1.0 / 60.0;
        let mut last = line.nearest(race.racers[0].state.position) as i64;
        let mut gained = 0;
        let half = n as i64 / 2;
        for k in 0..((settle + watch) / dt) as usize {
            race.step(t, DriveInput::default(), dt);
            let now = line.nearest(race.racers[0].state.position) as i64;
            // Signed points moved, across the start/finish wrap (a respawn counts as lost ground).
            let d = (now - last + half).rem_euclid(n as i64) - half;
            last = now;
            if k as f32 * dt >= settle {
                gained += d;
            }
        }
        (gained as f32 / watch, race.wrong_way(0))
    }

    /// The hard wrecks at a few headings, at the start/finish line (both sides), the tightest
    /// bend, and `extra` spots; returns the cases where the bot did not race on.
    fn failed_wrecks(track_i: usize, extra: &[usize]) -> Vec<String> {
        let t = crate::track::build_track(&crate::track::TRACKS[track_i]);
        let line = RacingLine::new(t.centerline.clone());
        let n = line.len();
        let bend = |i: usize| line.tangent(i + n - 5).dot(line.tangent(i + 5));
        let hairpin = (0..n)
            .min_by(|&a, &b| bend(a).total_cmp(&bend(b)))
            .unwrap_or(0);
        let mut spots = vec![n - 2, 1, hairpin, (hairpin + n - 12) % n];
        spots.extend_from_slice(extra);
        let mut failed = Vec::new();
        for &at in &spots {
            for heading in [0.0, 180.0, 100.0, -120.0] {
                for &(lateral, speed, shove, spin, lift) in &WRECK_MODES[HARD_WRECKS] {
                    let w = Wreck {
                        at,
                        heading,
                        lateral,
                        speed,
                        shove,
                        spin,
                        lift,
                    };
                    let (rate, wrong) = run_wreck(&t, w, WRECK_SETTLE, WRECK_WATCH);
                    if rate < WRECK_MIN_RATE || wrong {
                        failed.push(format!("{w:?}: {rate:.1} points/s, wrong way {wrong}"));
                    }
                }
            }
        }
        failed
    }

    // The extra spots are where wrecked bots used to lose the most ground: long downhills
    // (rolling backwards) and the canyon.
    #[test]
    fn wrecked_bots_race_on_at_dustbowl() {
        let failed = failed_wrecks(0, &[259, 518]);
        assert!(failed.is_empty(), "{}", failed.join("\n"));
    }

    #[test]
    fn wrecked_bots_race_on_at_grindstone() {
        let failed = failed_wrecks(1, &[37, 296]);
        assert!(failed.is_empty(), "{}", failed.join("\n"));
    }

    #[test]
    fn wrecked_bots_race_on_at_skyway() {
        let failed = failed_wrecks(2, &[222, 629, 666]);
        assert!(failed.is_empty(), "{}", failed.join("\n"));
    }

    /// The full grid of wrecks (every 37th point, ten headings, every mode: ~6000 runs).
    /// Slow; run with `cargo test --release -p scrapburn wreck_sweep -- --ignored`.
    #[test]
    #[ignore = "slow: an exhaustive sweep, run in release"]
    fn wreck_sweep() {
        let mut failed = Vec::new();
        for track_i in 0..crate::track::TRACKS.len() {
            let t = crate::track::build_track(&crate::track::TRACKS[track_i]);
            let n = t.centerline.len() - 1;
            let mut spots: Vec<usize> = (0..n).step_by(37).collect();
            spots.extend([n - 4, n - 1, 1, 3]);
            for &at in &spots {
                for heading in [
                    0.0, 60.0, 90.0, 120.0, 170.0, 180.0, -170.0, -120.0, -90.0, -60.0,
                ] {
                    for (lateral, speed, shove, spin, lift) in WRECK_MODES {
                        let w = Wreck {
                            at,
                            heading,
                            lateral,
                            speed,
                            shove,
                            spin,
                            lift,
                        };
                        let (rate, wrong) = run_wreck(&t, w, WRECK_SETTLE, WRECK_WATCH);
                        if rate < WRECK_MIN_RATE || wrong {
                            failed.push(format!("track {track_i} {w:?}: {rate:.1} points/s"));
                        }
                    }
                }
            }
        }
        assert!(failed.is_empty(), "{}", failed.join("\n"));
    }

    #[test]
    fn only_local_bots_are_put_back_for_going_the_wrong_way() {
        let t = track();
        let mut race = Race::new(t, 3, true, 3);
        race.set_remote(2);
        race.phase = Phase::Racing;
        // Everyone mid-lap, with a long WRONG WAY stint behind them.
        for (k, r) in race.racers.iter_mut().enumerate() {
            let at = 200 + k * 30;
            r.state = VehicleState::spawn(t.centerline[at] + Vec3::Y * SPAWN_LIFT, 0.0);
            r.prev = r.state.clone();
            r.laps.sync_to(r.state.position, &t.checkpoints);
            r.wrong = WRONG_WAY_RESPAWN_SECONDS + 1.0;
        }
        let before: Vec<Vec3> = race.racers.iter().map(|r| r.state.position).collect();
        race.step(t, DriveInput::default(), 1.0 / 60.0);
        let moved = |i: usize| race.racers[i].state.position.distance(before[i]);
        assert!(moved(0) < 1.0, "the player is never put back for wrong way");
        assert!(moved(1) > 5.0, "the bot goes back to its last checkpoint");
        assert!(!race.wrong_way(1), "and is facing the right way");
        assert!(moved(2) < 1.0, "a remote kart respawns on its own machine");
    }

    #[test]
    fn karts_hold_still_on_a_sloped_grid_during_the_countdown() {
        let t = crate::track::build_track(&crate::track::TRACKS[2]);
        let mut race = Race::new(&t, 8, true, 3);
        // Settle onto the deck first.
        for _ in 0..30 {
            race.step(&t, DriveInput::default(), 1.0 / 60.0);
        }
        let start: Vec<Vec3> = race.racers.iter().map(|r| r.state.position).collect();
        while matches!(race.phase, Phase::Countdown(_)) {
            race.step(&t, DriveInput::default(), 1.0 / 60.0);
        }
        for (r, s) in race.racers.iter().zip(&start) {
            let d = Vec3::new(r.state.position.x - s.x, 0.0, r.state.position.z - s.z).length();
            assert!(d < 0.05, "rolled {d:.2} m during the countdown");
        }
    }

    #[test]
    fn remote_karts_take_their_state_from_the_network() {
        let t = track();
        let mut race = Race::new(t, 2, false, 3);
        race.set_remote(1);
        let mut moved = race.racers[1].state.clone();
        moved.position += Vec3::new(0.0, 0.0, 3.0);
        moved.velocity = Vec3::new(0.0, 0.0, 20.0);
        race.push_remote_state(1, moved.clone());
        race.step(t, DriveInput::default(), 1.0 / 60.0);
        assert_eq!(
            race.racers[1].state.position, moved.position,
            "not simulated by the host"
        );
        // Without a fresh state the kart holds its last known one.
        race.step(t, DriveInput::default(), 1.0 / 60.0);
        assert_eq!(race.racers[1].state.position, moved.position);
        assert!(race.racers[1].bot.is_none());
    }

    #[test]
    fn host_counts_weapon_hits_on_remote_karts() {
        let t = track();
        let mut race = Race::new(t, 2, false, 3);
        race.set_remote(1);
        let before = race.spins();
        race.racers[1].state.velocity = Vec3::new(0.0, 0.0, 20.0);
        race.racers[1].state.spin_out(1.2, 0.5);
        race.note_remote_hits(&before, &[Vec3::new(0.0, 0.0, 20.0); 2]);
        assert_eq!(race.racers[1].hits, 1);
        let (secs, keep) = race.racers[1].last_hit;
        assert!((secs - 1.2).abs() < 1e-4 && (keep - 0.5).abs() < 0.01);
        assert_eq!(race.racers[0].hits, 0, "local karts are not counted");
    }

    #[test]
    fn a_client_steps_only_its_own_kart() {
        let t = track();
        let mut race = Race::new(t, 3, true, 3);
        race.phase = Phase::Racing;
        let others: Vec<Vec3> = race.racers[1..].iter().map(|r| r.state.position).collect();
        let start = race.racers[0].state.position;
        for _ in 0..60 {
            race.step_own(
                t,
                0,
                DriveInput {
                    throttle: 1.0,
                    ..Default::default()
                },
                1.0 / 60.0,
            );
        }
        assert!(race.racers[0].state.position.distance(start) > 5.0);
        let now: Vec<Vec3> = race.racers[1..].iter().map(|r| r.state.position).collect();
        assert_eq!(now, others, "rivals move only by snapshot");
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
    fn offroad_grade_forgives_the_edge_then_ramps_to_a_cap() {
        let edge = crate::track::ROAD_HALF_WIDTH;
        assert_eq!(offroad_grade(0.0), 0.0, "on the road");
        assert_eq!(offroad_grade(edge + 1.5), 0.0, "just past the edge is free");
        assert_eq!(offroad_grade(edge + OFFROAD_GRACE), 0.0);
        let mut last = 0.0;
        for k in 1..=10 {
            let g = offroad_grade(edge + OFFROAD_GRACE + OFFROAD_RAMP * k as f32 / 10.0);
            assert!(g > last, "rises with depth: {g} after {last}");
            last = g;
        }
        assert_eq!(last, 1.0, "full at the end of the ramp");
        assert_eq!(offroad_grade(edge + 40.0), 1.0, "and never more");
    }

    /// Mean speed (m/s) over the last 1.5 s of a 4 s full-throttle run on Dustbowl, holding
    /// `offset` m to the right of the centerline along the back straight (no pads, gentle).
    fn dustbowl_run(t: &Track, offset: f32) -> f32 {
        let line = RacingLine::new(t.centerline.clone());
        let p = VehicleParams::default();
        // A lane clear of the (solid) rocks and props on both sides, near and deep.
        let i0 = 240;
        let start = line.point(i0) + line.right(i0) * offset + Vec3::Y * 1.0;
        let f = line.tangent(i0);
        let mut s = VehicleState::spawn(start, f.x.atan2(f.z));
        s.velocity = f * p.max_speed;
        let (dt, mut sum, mut n) = (1.0 / 60.0, 0.0, 0);
        for k in 0..240 {
            let i = line.nearest(s.position);
            let aim = line.point(i + 6) + line.right(i + 6) * offset - s.position;
            let fwd = s.forward();
            let side = fwd.cross(Vec3::new(aim.x, 0.0, aim.z).normalize_or(fwd)).y;
            let input = DriveInput {
                throttle: 1.0,
                steer: (-side * 3.0).clamp(-1.0, 1.0),
                ..Default::default()
            };
            step_kart(&mut s, &p, input, t, dt);
            if k >= 150 {
                sum += s.speed();
                n += 1;
            }
        }
        let d = crate::terrain::distance_to(&t.centerline, s.position.x, s.position.z);
        assert!(
            (d - offset.abs()).abs() < 2.0,
            "kept the lane: {d:.1} m from the centerline, wanted {offset}"
        );
        sum / n as f32
    }

    #[test]
    fn driving_off_dustbowl_is_slower_but_not_crippling() {
        let t = crate::track::build_track(&crate::track::TRACKS[0]);
        let p = VehicleParams::default();
        let edge = crate::track::ROAD_HALF_WIDTH;
        let road = dustbowl_run(&t, 0.0);
        assert!(road > p.max_speed - 1.5, "road speed {road:.1}");
        for side in [1.0f32, -1.0] {
            let clipping = dustbowl_run(&t, side * (edge + 1.5));
            let deep = dustbowl_run(&t, side * (edge + OFFROAD_GRACE + OFFROAD_RAMP + 1.0));
            assert!(
                clipping > road - 1.0,
                "side {side}: clipping the edge is free ({clipping:.1} vs {road:.1})"
            );
            assert!(
                deep < road * 0.8,
                "side {side}: deep offroad is noticeably slower ({deep:.1} vs {road:.1})"
            );
            assert!(
                deep > p.max_speed * p.offroad_speed_scale - 1.0,
                "side {side}: but not below the cap ({deep:.1})"
            );
        }
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
    fn standings_hold_across_the_road_on_every_track() {
        // A kart 4 m behind another stays behind, wherever it is across the road.
        for def in &crate::track::TRACKS {
            let t = crate::track::build_track(def);
            let mut r = Race::new(&t, 2, false, 3);
            let cl = &t.centerline;
            let mut wrong = Vec::new();
            // Up to the last checkpoint: `sync_to` (a teleport) puts karts past it on the grid.
            let end = t.checkpoints.last().map_or(cl.len(), |c| c.line) - 2;
            for i in (4..end).step_by(3) {
                let tangent = (cl[i + 1] - cl[i - 1]).normalize();
                let side = tangent.cross(Vec3::Y).normalize();
                // Near the middle of a tight bend every part of it is about as close: "ahead"
                // is ambiguous there, so skip spots within 4 m of the bend's center.
                let (a, b, c) = (cl[i - 2], cl[i], cl[i + 2]);
                let turn = (b - a).normalize().angle_between((c - b).normalize());
                let radius = b.distance(a) / turn.max(1e-6);
                let inward = (a + c - 2.0 * b).dot(side).signum();
                for offset in [-9.0f32, -5.0, 5.0, 9.0] {
                    if offset.signum() == inward && offset.abs() > radius - 4.0 {
                        continue;
                    }
                    r.racers[0].state.position = cl[i] + side * offset;
                    r.racers[1].state.position = cl[i + 2];
                    for racer in &mut r.racers {
                        let pos = racer.state.position;
                        racer.laps.sync_to(pos, &t.checkpoints);
                    }
                    if r.progress(0, &t) >= r.progress(1, &t) {
                        wrong.push((i, offset));
                    }
                }
            }
            assert!(
                wrong.is_empty(),
                "{}: {} misordered spots, e.g. {:?}",
                def.name,
                wrong.len(),
                &wrong[..wrong.len().min(6)]
            );
        }
    }

    #[test]
    fn bots_finish_laps_on_every_track_without_piling_into_props() {
        // Solid props and structures line the open Dustbowl's shoulders; the bots must still
        // get round every circuit, and only rarely touch one (after a wild landing or a hit).
        for def in &crate::track::TRACKS {
            let t = crate::track::build_track(def);
            let mut r = Race::new(&t, 8, false, 2);
            let mut touching = vec![vec![false; t.solids.len()]; 8];
            let (mut finished, mut touches, mut ticks) = (0, 0, 0);
            while finished < 8 && ticks < 150 * 60 {
                ticks += 1;
                finished += r
                    .step(&t, DriveInput::default(), 1.0 / 60.0)
                    .iter()
                    .filter(|e| matches!(e, RaceEvent::Finish { .. }))
                    .count();
                for (i, racer) in r.racers.iter().enumerate() {
                    let p = racer.state.position;
                    for (k, s) in t.solids.iter().enumerate() {
                        let low = s.base.iter().map(|b| b.y).fold(f32::MAX, f32::min);
                        let near = s.edge_distance_xz(p) < 1.7
                            && p.y > low - 3.0
                            && p.y < low + s.height + 1.0;
                        touches += usize::from(near && !touching[i][k]);
                        touching[i][k] = near;
                    }
                }
            }
            assert!(
                finished >= 6,
                "{}: only {finished} of 8 bots finished two laps in 150 s",
                def.name
            );
            assert!(
                touches <= 3,
                "{}: bots ran into solid props {touches} times in two laps",
                def.name
            );
        }
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
