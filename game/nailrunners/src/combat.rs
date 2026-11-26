//! Item boxes and weapons: Nailer, Harpoon, Doom Orb.

use glam::Vec3;
use rivet_ai::RacingLine;
use rivet_physics::{TriMesh, VehicleState};

use crate::race::Racer;
use crate::track::Track;

pub const BOX_RESPAWN: f32 = 3.0;
const PICKUP_RADIUS: f32 = 2.8;
const BOX_LANES: [f32; 4] = [-4.5, -1.5, 1.5, 4.5];
const BOX_HEIGHT: f32 = 1.0;

const NAILS_PER_BURST: u32 = 12;
const NAIL_INTERVAL: f32 = 0.05;
const NAIL_SPEED: f32 = 90.0;
const NAIL_LIFE: f32 = 0.8;
const NAIL_HIT_RADIUS: f32 = 1.3;
const NAIL_SPIN: (f32, f32) = (0.6, 0.75);

const HOOK_SPEED: f32 = 120.0;
const HOOK_RANGE: (f32, f32) = (5.0, 70.0);
const HOOK_CONE: f32 = 0.42; // radians
const REEL_ACCEL: f32 = 45.0;
const REEL_MAX: f32 = 1.2;
/// The reel can't drag a kart faster than this (m/s).
const REEL_MAX_SPEED: f32 = 44.0;
const SLINGSHOT: (f32, f32) = (10.0, 0.8);
const YANK_KEEP: f32 = 0.6;

const ORB_SPEED: f32 = 52.0;
const ORB_HOME_RANGE: f32 = 16.0;
const ORB_ZAP_RADIUS: f32 = 8.0;
const ORB_ZAP_SPIN: (f32, f32) = (0.35, 0.85);
const ORB_BLAST_SPIN: (f32, f32) = (1.6, 0.35);
const ORB_SPLASH: (f32, f32, f32) = (7.0, 0.8, 0.6);
const ORB_LIFE: f32 = 15.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Nailer,
    Harpoon,
    DoomOrb,
}

impl ItemKind {
    pub fn name(self) -> &'static str {
        match self {
            ItemKind::Nailer => "NAILER",
            ItemKind::Harpoon => "HARPOON",
            ItemKind::DoomOrb => "DOOM ORB",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CombatEvent {
    Pickup { racer: usize, item: ItemKind },
    Fire { racer: usize, item: ItemKind },
    Hit { victim: usize, item: ItemKind },
    Slingshot { racer: usize },
    Zap { victim: usize },
    Explode { victim: usize, pos: Vec3 },
}

#[derive(Debug, Clone, Copy)]
pub struct ItemBox {
    pub pos: Vec3,
    pub respawn: f32,
}

impl ItemBox {
    pub fn active(&self) -> bool {
        self.respawn <= 0.0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Nail {
    pub pos: Vec3,
    pub vel: Vec3,
    owner: usize,
    life: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum HookState {
    Flying(f32),
    Reeling(f32),
}

#[derive(Debug, Clone, Copy)]
pub struct Hook {
    pub owner: usize,
    pub pos: Vec3,
    target: Option<usize>,
    vel: Vec3,
    state: HookState,
}

#[derive(Debug, Clone)]
pub struct Orb {
    pub pos: Vec3,
    owner: usize,
    target: usize,
    line_pos: f32,
    zapped: Vec<bool>,
    life: f32,
}

impl Orb {
    /// The racer this orb hunts.
    pub fn target(&self) -> usize {
        self.target
    }
}

/// Applies one nail hit: a spin-out, but a kart already spinning just spins longer (a burst
/// can't grind it to a halt).
pub fn nail_hit(state: &mut VehicleState) {
    if state.spin > 0.0 {
        state.spin = state.spin.max(NAIL_SPIN.0);
    } else {
        state.spin_out(NAIL_SPIN.0, NAIL_SPIN.1);
    }
}

/// Where item boxes sit: rows of four across the road at three places on the lap.
pub fn item_box_spots(track: &Track) -> Vec<Vec3> {
    let cl = &track.centerline;
    let n = cl.len() - 1;
    let mut out = Vec::new();
    for &row in &track.box_rows {
        let i = (n as f32 * row) as usize;
        let f = (cl[(i + 1) % n] - cl[(i + n - 1) % n]).normalize();
        let right = f.cross(Vec3::Y).normalize();
        for lane in BOX_LANES {
            out.push(cl[i] + right * lane + Vec3::Y * BOX_HEIGHT);
        }
    }
    out
}

fn hash(seed: u32) -> f32 {
    let mut h = seed.wrapping_mul(0x9e37_79b9) ^ 0x85eb_ca6b;
    h = (h ^ (h >> 16)).wrapping_mul(0x7feb_352d);
    h = (h ^ (h >> 15)).wrapping_mul(0x846c_a68b);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65_536.0
}

/// Position-weighted item roll (`pos` 1-based of `racers`): leaders get Nailers, the middle
/// Harpoons, the back of the pack the Doom Orb. The leader never gets the orb.
pub fn roll_item(pos: usize, racers: usize, seed: u32) -> ItemKind {
    let f = if racers > 1 {
        (pos - 1) as f32 / (racers - 1) as f32
    } else {
        0.0
    };
    let nail = 1.0 - 0.6 * f;
    let hook = 0.35 + 0.5 * (1.0 - (2.0 * f - 1.0).abs());
    let orb = if pos == 1 { 0.0 } else { 0.05 + 1.1 * f * f };
    let u = hash(seed) * (nail + hook + orb);
    if u < nail {
        ItemKind::Nailer
    } else if u < nail + hook {
        ItemKind::Harpoon
    } else {
        ItemKind::DoomOrb
    }
}

pub struct Combat {
    pub boxes: Vec<ItemBox>,
    pub held: Vec<Option<ItemKind>>,
    pub nails: Vec<Nail>,
    pub hooks: Vec<Hook>,
    pub orbs: Vec<Orb>,
    bursts: Vec<(usize, u32, f32)>,
    fire_queue: Vec<usize>,
    hold_time: Vec<f32>,
    rolls: u32,
}

fn horizontal(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

impl Combat {
    pub fn new(track: &Track, racers: usize) -> Self {
        Self {
            boxes: item_box_spots(track)
                .into_iter()
                .map(|pos| ItemBox { pos, respawn: 0.0 })
                .collect(),
            held: vec![None; racers],
            nails: Vec::new(),
            hooks: Vec::new(),
            orbs: Vec::new(),
            bursts: Vec::new(),
            fire_queue: Vec::new(),
            hold_time: vec![0.0; racers],
            rolls: 0,
        }
    }

    /// Requests racer `i` to use its held item on the next tick.
    pub fn fire(&mut self, i: usize) {
        if !self.fire_queue.contains(&i) {
            self.fire_queue.push(i);
        }
    }

    /// Drops hooks that involve racer `r` (after a respawn teleports it).
    pub fn cancel_for(&mut self, r: usize) {
        self.hooks.retain(|h| h.owner != r && h.target != Some(r));
    }

    /// Nearest kart ahead of `i` inside the harpoon cone and range.
    fn harpoon_target(racers: &[Racer], i: usize) -> Option<usize> {
        let me = &racers[i].state;
        let fwd = horizontal(me.forward()).normalize_or(Vec3::Z);
        racers
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .filter_map(|(j, r)| {
                let rel = horizontal(r.state.position - me.position);
                let d = rel.length();
                let angle = fwd.angle_between(rel.normalize_or(fwd));
                ((HOOK_RANGE.0..HOOK_RANGE.1).contains(&d) && angle < HOOK_CONE).then_some((j, d))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(j, _)| j)
    }

    fn bot_wants_fire(racers: &[Racer], i: usize, item: ItemKind, held_for: f32) -> bool {
        if held_for > 8.0 {
            return true;
        }
        if held_for < 0.8 {
            return false;
        }
        let me = &racers[i].state;
        let fwd = horizontal(me.forward()).normalize_or(Vec3::Z);
        match item {
            ItemKind::Nailer => racers.iter().enumerate().any(|(j, r)| {
                let rel = horizontal(r.state.position - me.position);
                let ahead = rel.dot(fwd);
                j != i && (3.0..35.0).contains(&ahead) && (rel - fwd * ahead).length() < 3.0
            }),
            ItemKind::Harpoon => Self::harpoon_target(racers, i).is_some_and(|j| {
                horizontal(racers[j].state.position - me.position).length() > 12.0
            }),
            ItemKind::DoomOrb => held_for > 1.5,
        }
    }

    /// One tick. `standings` lists racers first to last.
    pub fn step(
        &mut self,
        racers: &mut [Racer],
        standings: &[usize],
        line: &RacingLine,
        world: &TriMesh,
        dt: f32,
    ) -> Vec<CombatEvent> {
        let mut events = Vec::new();
        let n = racers.len();

        // Boxes and pickups.
        for b in &mut self.boxes {
            b.respawn = (b.respawn - dt).max(0.0);
        }
        for i in 0..n {
            if self.held[i].is_some() {
                continue;
            }
            let p = racers[i].state.position;
            if let Some(b) = self
                .boxes
                .iter_mut()
                .find(|b| b.active() && b.pos.distance(p) < PICKUP_RADIUS)
            {
                b.respawn = BOX_RESPAWN;
                let place = standings.iter().position(|&r| r == i).map_or(n, |x| x + 1);
                self.rolls += 1;
                let item = roll_item(place, n, self.rolls.wrapping_mul(7919) ^ i as u32);
                self.held[i] = Some(item);
                self.hold_time[i] = 0.0;
                events.push(CombatEvent::Pickup { racer: i, item });
            }
        }

        // Bots decide.
        for i in 0..n {
            if let Some(item) = self.held[i] {
                self.hold_time[i] += dt;
                if racers[i].bot.is_some()
                    && Self::bot_wants_fire(racers, i, item, self.hold_time[i])
                {
                    self.fire(i);
                }
            }
        }

        // Fire.
        for i in std::mem::take(&mut self.fire_queue) {
            let Some(item) = self.held[i].take() else {
                continue;
            };
            events.push(CombatEvent::Fire { racer: i, item });
            let me = &racers[i].state;
            match item {
                ItemKind::Nailer => self.bursts.push((i, NAILS_PER_BURST, 0.0)),
                ItemKind::Harpoon => {
                    let target = Self::harpoon_target(racers, i);
                    let fwd = me.forward();
                    self.hooks.push(Hook {
                        owner: i,
                        pos: me.position + fwd * 1.5 + Vec3::Y * 0.6,
                        target,
                        vel: fwd * HOOK_SPEED + me.velocity,
                        state: HookState::Flying(0.0),
                    });
                }
                ItemKind::DoomOrb => {
                    // Nobody else still racing: the orb has no one to hunt.
                    let Some(leader) = standings.iter().copied().find(|&r| r != i) else {
                        continue;
                    };
                    self.orbs.push(Orb {
                        pos: me.position + Vec3::Y * 1.8,
                        owner: i,
                        target: leader,
                        line_pos: line.nearest(me.position) as f32,
                        zapped: vec![false; n],
                        life: ORB_LIFE,
                    });
                }
            }
        }

        // Nailer bursts.
        for (owner, left, timer) in &mut self.bursts {
            *timer -= dt;
            while *timer <= 0.0 && *left > 0 {
                let s = &racers[*owner].state;
                let fwd = s.forward();
                self.nails.push(Nail {
                    pos: s.position + fwd * 1.8 + Vec3::Y * 0.4,
                    vel: fwd * NAIL_SPEED + s.velocity,
                    owner: *owner,
                    life: NAIL_LIFE,
                });
                *timer += NAIL_INTERVAL;
                *left -= 1;
            }
        }
        self.bursts.retain(|b| b.1 > 0);

        // Nails fly and hit.
        let mut hits = Vec::new();
        self.nails.retain_mut(|nail| {
            let from = nail.pos;
            nail.pos += nail.vel * dt;
            nail.life -= dt;
            let seg = nail.pos - from;
            // Barriers and terrain stop nails.
            let len = seg.length();
            if len > 1e-4 && world.raycast(from, seg / len, len).is_some() {
                return false;
            }
            for (j, r) in racers.iter().enumerate() {
                if j == nail.owner {
                    continue;
                }
                let c = r.state.position;
                let t = ((c - from).dot(seg) / seg.length_squared().max(1e-6)).clamp(0.0, 1.0);
                if (from + seg * t).distance(c) < NAIL_HIT_RADIUS {
                    hits.push(j);
                    return false;
                }
            }
            nail.life > 0.0
        });
        for j in hits {
            nail_hit(&mut racers[j].state);
            events.push(CombatEvent::Hit {
                victim: j,
                item: ItemKind::Nailer,
            });
        }

        // Harpoons.
        let mut done = Vec::new();
        for (k, hook) in self.hooks.iter_mut().enumerate() {
            match hook.state {
                HookState::Flying(t) => {
                    let t = t + dt;
                    match hook.target {
                        Some(tg) => {
                            let to = racers[tg].state.position - hook.pos;
                            if to.length() < 2.0 {
                                hook.state = HookState::Reeling(0.0);
                            } else {
                                hook.pos += to.normalize() * HOOK_SPEED * dt;
                                hook.state = HookState::Flying(t);
                            }
                        }
                        None => {
                            hook.pos += hook.vel * dt;
                            hook.state = HookState::Flying(t);
                        }
                    }
                    if t > 0.8 && hook.state == HookState::Flying(t) {
                        done.push(k);
                    }
                }
                HookState::Reeling(t) => {
                    let (o, tg) = (hook.owner, hook.target.unwrap_or(hook.owner));
                    let to = racers[tg].state.position - racers[o].state.position;
                    hook.pos = racers[tg].state.position + Vec3::Y * 0.6;
                    let t = t + dt;
                    let owner = &mut racers[o].state;
                    owner.velocity += to.normalize_or_zero() * REEL_ACCEL * dt;
                    owner.velocity = owner.velocity.clamp_length_max(REEL_MAX_SPEED);
                    if to.length() < 5.0 || t > REEL_MAX {
                        owner.boost_speed = owner.boost_speed.max(SLINGSHOT.0);
                        owner.boost_timer = owner.boost_timer.max(SLINGSHOT.1);
                        racers[tg].state.velocity *= YANK_KEEP;
                        events.push(CombatEvent::Slingshot { racer: o });
                        events.push(CombatEvent::Hit {
                            victim: tg,
                            item: ItemKind::Harpoon,
                        });
                        done.push(k);
                    } else {
                        hook.state = HookState::Reeling(t);
                    }
                }
            }
        }
        for k in done.into_iter().rev() {
            self.hooks.remove(k);
        }

        // Doom orbs.
        let mut gone = Vec::new();
        for (k, orb) in self.orbs.iter_mut().enumerate() {
            orb.life -= dt;
            let target_pos = racers[orb.target].state.position;
            if orb.pos.distance(target_pos) < ORB_HOME_RANGE {
                orb.pos += (target_pos + Vec3::Y * 0.8 - orb.pos).normalize_or_zero()
                    * ORB_SPEED
                    * 1.2
                    * dt;
            } else {
                orb.line_pos += ORB_SPEED * dt / 2.0;
                let i = orb.line_pos as usize % line.len();
                let goal = line.point(i) + Vec3::Y * 1.8;
                orb.pos += (goal - orb.pos) * (1.0 - (-10.0 * dt).exp());
            }
            for (j, r) in racers.iter_mut().enumerate() {
                if j != orb.owner
                    && j != orb.target
                    && !orb.zapped[j]
                    && r.state.position.distance(orb.pos) < ORB_ZAP_RADIUS
                {
                    orb.zapped[j] = true;
                    r.state.spin_out(ORB_ZAP_SPIN.0, ORB_ZAP_SPIN.1);
                    events.push(CombatEvent::Zap { victim: j });
                }
            }
            if orb.pos.distance(target_pos) < 2.2 {
                let victim = orb.target;
                racers[victim]
                    .state
                    .spin_out(ORB_BLAST_SPIN.0, ORB_BLAST_SPIN.1);
                racers[victim].state.velocity.y += 9.0;
                for (j, r) in racers.iter_mut().enumerate() {
                    if j != victim
                        && j != orb.owner
                        && r.state.position.distance(orb.pos) < ORB_SPLASH.0
                    {
                        r.state.spin_out(ORB_SPLASH.1, ORB_SPLASH.2);
                    }
                }
                events.push(CombatEvent::Explode {
                    victim,
                    pos: orb.pos,
                });
                gone.push(k);
            } else if orb.life <= 0.0 {
                gone.push(k);
            }
        }
        for k in gone.into_iter().rev() {
            self.orbs.remove(k);
        }
        events
    }

    /// Chain segments (from shooter to hook) for rendering.
    pub fn chains<'a>(&'a self, racers: &'a [Racer]) -> impl Iterator<Item = (Vec3, Vec3)> + 'a {
        self.hooks
            .iter()
            .map(|h| (racers[h.owner].state.position + Vec3::Y * 0.6, h.pos))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::race::Race;
    use crate::track::build_graybox_loop;
    use glam::Vec3;
    use rivet_physics::DriveInput;
    use std::sync::OnceLock;

    fn track() -> &'static Track {
        static T: OnceLock<Track> = OnceLock::new();
        T.get_or_init(build_graybox_loop)
    }

    /// Two racers on the first straight, `gap` meters apart, both moving.
    fn duel(gap: f32) -> Race {
        let t = track();
        let mut r = Race::new(t, 2, true, 3);
        r.phase = crate::race::Phase::Racing;
        let (a, b) = (t.centerline[40], t.centerline[40 + (gap / 2.0) as usize]);
        let f = (t.centerline[41] - t.centerline[39]).normalize();
        let yaw = f.x.atan2(f.z);
        for (k, p) in [a, b].into_iter().enumerate() {
            r.racers[k].state = rivet_physics::VehicleState::spawn(p + Vec3::Y * 0.5, yaw);
            r.racers[k].state.velocity = f * 25.0;
            r.racers[k].state.grounded = true;
            r.racers[k].prev = r.racers[k].state.clone();
            let pos = r.racers[k].state.position;
            r.racers[k].laps.sync_to(pos, &t.checkpoints);
        }
        r.racers[1].bot = None; // keep the target passive
        r
    }

    #[test]
    fn boxes_sit_on_the_road_in_rows() {
        let t = track();
        let boxes = item_box_spots(t);
        assert_eq!(boxes.len(), 12);
        for b in &boxes {
            let hit = t
                .collision
                .raycast(*b + Vec3::Y * 3.0, Vec3::NEG_Y, 6.0)
                .unwrap();
            assert_eq!(hit.surface, rivet_physics::Surface::Road);
        }
    }

    #[test]
    fn odds_favor_the_back() {
        let count = |pos: usize| {
            (0..1000u32)
                .filter(|&s| roll_item(pos, 8, s) == ItemKind::DoomOrb)
                .count()
        };
        assert_eq!(count(1), 0, "the leader never gets the orb");
        assert!(count(8) > count(4) * 2, "last place gets it far more often");
        assert!((0..1000u32).any(|s| roll_item(1, 8, s) == ItemKind::Nailer));
    }

    #[test]
    fn pickup_gives_an_item_and_box_respawns() {
        let t = track();
        let mut r = duel(20.0);
        // Racer 1 already holds an item, so only racer 0 can take this box.
        r.combat.held[1] = Some(ItemKind::Nailer);
        let spot = r.combat.boxes[0].pos;
        r.racers[0].state.position = spot + Vec3::Y * 0.5;
        r.racers[0].prev = r.racers[0].state.clone();
        r.step(t, DriveInput::default(), 1.0 / 60.0);
        assert!(r.combat.held[0].is_some(), "picked up");
        assert!(!r.combat.boxes[0].active(), "box gone");
        for _ in 0..(BOX_RESPAWN * 60.0) as usize + 5 {
            r.step(t, DriveInput::default(), 1.0 / 60.0);
        }
        assert!(r.combat.boxes[0].active(), "box back");
    }

    #[test]
    fn nailer_hits_the_kart_ahead() {
        let t = track();
        let mut r = duel(16.0);
        r.combat.held[0] = Some(ItemKind::Nailer);
        r.fire(0);
        for _ in 0..40 {
            r.step(
                t,
                DriveInput {
                    throttle: 1.0,
                    ..Default::default()
                },
                1.0 / 60.0,
            );
        }
        assert!(r.racers[1].state.spin > 0.0, "target spun out");
        assert!(r.combat.held[0].is_none(), "item used up");
    }

    #[test]
    fn harpoon_reels_in_and_slingshots() {
        let t = track();
        let mut r = duel(40.0);
        r.combat.held[0] = Some(ItemKind::Harpoon);
        let gap0 = r.racers[0]
            .state
            .position
            .distance(r.racers[1].state.position);
        r.fire(0);
        let mut slingshot = false;
        for _ in 0..120 {
            for e in r.step(
                t,
                DriveInput {
                    throttle: 1.0,
                    ..Default::default()
                },
                1.0 / 60.0,
            ) {
                slingshot |= matches!(
                    e,
                    crate::race::RaceEvent::Combat(CombatEvent::Slingshot { racer: 0 })
                );
            }
        }
        let gap1 = r.racers[0]
            .state
            .position
            .distance(r.racers[1].state.position);
        assert!(slingshot, "release fires a slingshot");
        assert!(
            gap1 < gap0 * 0.5 || r.progress(0, t) > r.progress(1, t),
            "pulled close or past: {gap0} -> {gap1}"
        );
    }

    #[test]
    fn doom_orb_hunts_the_leader() {
        let t = track();
        let mut r = duel(60.0);
        r.combat.held[0] = Some(ItemKind::DoomOrb);
        r.fire(0);
        let mut boom = false;
        for _ in 0..(6.0 * 60.0) as usize {
            for e in r.step(t, DriveInput::default(), 1.0 / 60.0) {
                boom |= matches!(
                    e,
                    crate::race::RaceEvent::Combat(CombatEvent::Explode { victim: 1, .. })
                );
            }
        }
        assert!(boom, "the orb detonated on the leader");
    }

    #[test]
    fn doom_orb_ignores_finished_racers() {
        let t = track();
        let mut r = Race::new(t, 3, true, 3);
        r.phase = crate::race::Phase::Racing;
        r.racers[1].finish = Some((1, 50.0)); // already done, on the cool-down lap
        r.combat.held[0] = Some(ItemKind::DoomOrb);
        r.fire(0);
        r.step(t, DriveInput::default(), 1.0 / 60.0);
        assert_eq!(
            r.combat.orbs[0].target(),
            2,
            "targets the leader of those still racing"
        );
    }

    #[test]
    fn harpoon_cancels_when_the_target_respawns() {
        let t = track();
        let mut r = duel(30.0);
        r.combat.held[0] = Some(ItemKind::Harpoon);
        r.fire(0);
        for _ in 0..10 {
            r.step(t, DriveInput::default(), 1.0 / 60.0);
        }
        assert!(!r.combat.hooks.is_empty(), "hook out");
        r.respawn_racer(1, t);
        assert!(
            r.combat.hooks.is_empty(),
            "respawn cancels hooks involving that racer"
        );
    }

    #[test]
    fn stacked_nail_hits_do_not_crush_speed() {
        let t = track();
        let mut r = duel(16.0);
        let v0 = 25.0;
        r.racers[1].state.velocity = r.racers[1].state.velocity.normalize() * v0;
        for _ in 0..12 {
            crate::combat::nail_hit(&mut r.racers[1].state);
        }
        let v = r.racers[1].state.velocity.length();
        assert!(v > v0 * 0.6, "12 nails left {v} of {v0}");
        let _ = t;
    }

    #[test]
    fn nails_stop_at_barriers() {
        let t = track();
        let mut r = duel(16.0);
        // Aim racer 0 straight at the left barrier, racer 1 just beyond it (other side).
        let c = t.centerline[40];
        let f = (t.centerline[41] - t.centerline[39]).normalize();
        let left = -f.cross(Vec3::Y).normalize();
        r.racers[0].state =
            rivet_physics::VehicleState::spawn(c + Vec3::Y * 0.5, left.x.atan2(left.z));
        r.racers[0].prev = r.racers[0].state.clone();
        r.racers[1].state =
            rivet_physics::VehicleState::spawn(c + left * 12.0 + Vec3::Y * 0.5, 0.0);
        r.racers[1].prev = r.racers[1].state.clone();
        r.combat.held[0] = Some(ItemKind::Nailer);
        r.fire(0);
        for _ in 0..40 {
            r.step(t, DriveInput::default(), 1.0 / 60.0);
        }
        assert_eq!(
            r.racers[1].state.spin, 0.0,
            "nails went through the barrier"
        );
    }

    #[test]
    fn bots_use_items() {
        let t = track();
        let mut r = Race::new(t, 8, false, 3);
        let mut fired = 0;
        for _ in 0..(60.0 * 60.0) as usize {
            for e in r.step(t, DriveInput::default(), 1.0 / 60.0) {
                fired +=
                    matches!(e, crate::race::RaceEvent::Combat(CombatEvent::Fire { .. })) as u32;
            }
        }
        assert!(fired >= 5, "bots fired only {fired} items in a minute");
    }
}
