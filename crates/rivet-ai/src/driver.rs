//! A bot driver that follows a closed racing line.

use glam::Vec3;
use rivet_physics::{DriveInput, VehicleParams, VehicleState};

/// A closed loop of points (~2 m apart) bots aim along.
#[derive(Debug, Clone)]
pub struct RacingLine {
    points: Vec<Vec3>,
}

impl RacingLine {
    /// `points` must form a loop; a duplicated closing point is dropped.
    pub fn new(mut points: Vec<Vec3>) -> Self {
        if points.len() > 2 && points[0].distance(points[points.len() - 1]) < 0.5 {
            points.pop();
        }
        assert!(
            points.len() >= 3,
            "a racing line needs at least three points"
        );
        Self { points }
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn point(&self, i: usize) -> Vec3 {
        self.points[i % self.points.len()]
    }

    pub fn nearest(&self, pos: Vec3) -> usize {
        let mut best = (0, f32::MAX);
        for (i, p) in self.points.iter().enumerate() {
            let d = Vec3::new(p.x - pos.x, 0.0, p.z - pos.z).length_squared();
            if d < best.1 {
                best = (i, d);
            }
        }
        best.0
    }

    /// Horizontal unit tangent at point `i`.
    pub fn tangent(&self, i: usize) -> Vec3 {
        let n = self.points.len();
        let d = self.point(i + 1) - self.point(i + n - 1);
        Vec3::new(d.x, 0.0, d.z).normalize_or(Vec3::Z)
    }

    /// Unit vector to the kart's right at point `i` (+Y up, forward = tangent).
    pub fn right(&self, i: usize) -> Vec3 {
        self.tangent(i).cross(Vec3::Y)
    }

    /// Signed distance of `pos` to the right of the line.
    pub fn lateral_offset(&self, pos: Vec3) -> f32 {
        let i = self.nearest(pos);
        (pos - self.point(i)).dot(self.right(i))
    }
}

/// How far ahead to aim the steering, in seconds of travel (plus a floor in points).
const LOOK_AHEAD_SECONDS: f32 = 0.45;
const LOOK_AHEAD_MIN_POINTS: usize = 6;
/// The line's bend is measured between these points ahead (≈ 8 m to 40 m).
const CURVE_FROM: usize = 4;
const CURVE_TO: usize = 20;
const DRIFT_ENTER_TURN: f32 = 0.35;
const DRIFT_EXIT_TURN: f32 = 0.12;
const DRIFT_ENTRY_STEER: f32 = 0.4;
const DRIFT_MIN_SPEED: f32 = 18.0;
/// A kart this close ahead (m), within this lateral distance, makes the bot change lanes.
const DODGE_RANGE: f32 = 16.0;
const DODGE_WIDTH: f32 = 2.4;
const DODGE_SHIFT: f32 = 3.2;
const MAX_LANE: f32 = 5.0;
/// Above this speed a big heading error means brake; below it, turn around under power.
const SHARP_BRAKE_SPEED: f32 = 10.0;
/// Rolling backwards faster than this (m/s) flips the steering: a reversing kart yaws the
/// other way for the same steer, so the nose must be steered as if in reverse gear.
const ROLLING_BACK_SPEED: f32 = 0.5;
/// Throttle on, grounded, yet slower than this (m/s) for `BLOCKED_SECONDS`: nose against a
/// barrier, where a kart can't turn (steering needs speed). Back out for `BACK_OUT_SECONDS`.
const BLOCKED_SPEED: f32 = 1.0;
const BLOCKED_SECONDS: f32 = 0.4;
const BACK_OUT_SECONDS: f32 = 0.9;
/// Backing out ends early once the nose is this close (rad) to the way to go.
const BACK_OUT_DONE_ANGLE: f32 = 0.6;
/// Heading error (rad) beyond which the target counts as right behind and the bot picks its
/// turn by the road's room instead (see `drive`).
const COMMIT_TURN_ANGLE: f32 = 2.6;
/// Farther than this (m) off the line, a turn-around always swings toward the line.
const TURN_ROOM_OFFSET: f32 = 1.5;

/// One bot's driving style and short-term state.
#[derive(Debug, Clone)]
pub struct BotDriver {
    /// Share of the kart's top speed this bot holds (0.8–1.0 is a typical field).
    pub skill: f32,
    /// Preferred lateral offset from the racing line (m, + = right).
    pub lane: f32,
    dodge: f32,
    /// Seconds of throttle without moving (`BLOCKED_SECONDS`).
    blocked: f32,
    /// Seconds left of backing out from a barrier.
    back_out: f32,
    /// Turn direction committed to while the target is nearly straight behind (+ = left).
    turn: f32,
}

impl BotDriver {
    pub fn new(skill: f32, lane: f32) -> Self {
        Self {
            skill,
            lane,
            dodge: 0.0,
            blocked: 0.0,
            back_out: 0.0,
            turn: 0.0,
        }
    }

    /// Chooses this tick's input, `dt` seconds after the last. `others` are the other karts'
    /// positions; `speed_scale` is a race-level multiplier (catch-up).
    pub fn drive(
        &mut self,
        me: &VehicleState,
        params: &VehicleParams,
        line: &RacingLine,
        others: &[Vec3],
        speed_scale: f32,
        dt: f32,
    ) -> DriveInput {
        let i = line.nearest(me.position);
        let f = me.forward();
        let f = Vec3::new(f.x, 0.0, f.z).normalize_or(Vec3::Z);
        let signed_speed = me.speed();
        let speed = signed_speed.max(0.0);

        // Dodge: a kart right ahead in our lane pushes us toward the roomier side.
        let mut want_dodge = 0.0;
        let right = line.right(i);
        for o in others {
            let rel = *o - me.position;
            let ahead = rel.dot(f);
            if ahead > 0.0 && ahead < DODGE_RANGE {
                let side = rel.dot(right) - (self.lane + self.dodge);
                if side.abs() < DODGE_WIDTH {
                    want_dodge = if side >= 0.0 {
                        -DODGE_SHIFT
                    } else {
                        DODGE_SHIFT
                    };
                }
            }
        }
        self.dodge += (want_dodge - self.dodge) * 0.08;
        let lane = (self.lane + self.dodge).clamp(-MAX_LANE, MAX_LANE);

        let ahead_pts = ((speed * LOOK_AHEAD_SECONDS) / 2.0) as usize + LOOK_AHEAD_MIN_POINTS;
        let j = i + ahead_pts;
        let target = line.point(j) + line.right(j) * lane;
        let d = Vec3::new(target.x - me.position.x, 0.0, target.z - me.position.z).normalize_or(f);
        let mut angle = f.cross(d).y.atan2(f.dot(d));
        if angle.abs() > COMMIT_TURN_ANGLE {
            // Right behind, the sign of the error is noise. Turn toward the middle of the road:
            // turning toward the near barrier hits it, and the barrier's glancing contact swings
            // the nose back along it, so the kart scrapes down the wall the wrong way for good.
            // Near the middle, keep whichever turn was already chosen.
            let to_middle = -right * (me.position - line.point(i)).dot(right);
            let toward_middle = f.cross(to_middle).y;
            let side = if to_middle.length() > TURN_ROOM_OFFSET {
                toward_middle.signum()
            } else if self.turn != 0.0 {
                self.turn
            } else {
                angle.signum()
            };
            angle = side * angle.abs();
            self.turn = side;
        } else {
            self.turn = 0.0;
        }

        // Corners come from the line's own curvature, not the kart's heading, so the bot never
        // drifts down a straight (no boost-chaining "snaking").
        let t0 = line.tangent(i + CURVE_FROM);
        let t1 = line.tangent(i + CURVE_TO);
        let turn = t0.cross(t1).y.atan2(t0.dot(t1)); // + = bends left
        let turn_dir = -turn.signum(); // drift dir convention: +1 = right
        let sharp = angle.abs() > 0.9;
        let corner = turn.abs() > DRIFT_ENTER_TURN && speed > DRIFT_MIN_SPEED;
        let keep = me
            .drift
            .is_some_and(|dr| dr.dir == turn_dir && turn.abs() > DRIFT_EXIT_TURN);
        let drift = !sharp && (corner || keep);

        // Spun out or airborne the kart ignores (or barely feels) the wheel: don't count that
        // as being blocked, and don't start backing out.
        let in_control = me.spin <= 0.0 && me.grounded;
        if self.back_out > 0.0 {
            self.back_out = if angle.abs() < BACK_OUT_DONE_ANGLE {
                0.0
            } else {
                (self.back_out - dt).max(0.0)
            };
        }
        if self.back_out > 0.0 {
            // Reverse with the wheel turned so the nose swings toward the way to go.
            self.blocked = 0.0;
            return DriveInput {
                throttle: 0.0,
                brake: 1.0,
                steer: (angle * 2.5).clamp(-1.0, 1.0),
                drift: false,
                hop: false,
            };
        }

        let top = params.max_speed * self.skill * speed_scale;
        // Off-line at speed: brake. Slow and pointing the wrong way (after a spin-out): drive
        // forward on full lock to turn around rather than braking, which would reverse.
        let brake_hard = sharp && speed > SHARP_BRAKE_SPEED;
        let throttle = if brake_hard || speed > top { 0.0 } else { 1.0 };

        // Throttle on but going nowhere: nose into a barrier (or another kart). Back out.
        if throttle > 0.0 && in_control && signed_speed.abs() < BLOCKED_SPEED {
            self.blocked += dt;
            if self.blocked > BLOCKED_SECONDS && sharp {
                self.blocked = 0.0;
                self.back_out = BACK_OUT_SECONDS;
            }
        } else {
            self.blocked = 0.0;
        }

        // Rolling backwards (knocked back, or downhill after a spin): throttle stops the roll
        // and the steering is mirrored, or the bot would turn its nose away from the line and
        // settle facing backwards.
        if signed_speed < -ROLLING_BACK_SPEED {
            return DriveInput {
                throttle: 1.0,
                brake: 0.0,
                steer: (angle * 2.5).clamp(-1.0, 1.0),
                drift: false,
                hop: false,
            };
        }

        let steer = if drift {
            // Hold the arc to the line: a light bias into the corner plus tracking. Entry needs a
            // committed steer, so the first tick steers at least DRIFT_ENTRY_STEER into the turn.
            let trim = (turn_dir * 0.15 - angle * 6.0).clamp(-1.0, 1.0);
            if me.drift.is_none() && trim * turn_dir < DRIFT_ENTRY_STEER {
                turn_dir * DRIFT_ENTRY_STEER
            } else {
                trim
            }
        } else {
            (-angle * 2.5).clamp(-1.0, 1.0)
        };
        DriveInput {
            throttle,
            brake: if brake_hard { 1.0 } else { 0.0 },
            steer,
            drift,
            hop: drift && me.drift.is_none(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use rivet_physics::{TriMesh, VehicleParams, VehicleState, step};

    const DT: f32 = 1.0 / 60.0;

    fn ground() -> TriMesh {
        let h = 600.0;
        let p = vec![
            Vec3::new(-h, 0.0, -h),
            Vec3::new(h, 0.0, -h),
            Vec3::new(h, 0.0, h),
            Vec3::new(-h, 0.0, h),
        ];
        TriMesh::new(p, vec![[0, 2, 1], [0, 3, 2]], 16.0)
    }

    fn oval() -> RacingLine {
        // A stadium oval: two 200 m straights joined by radius-70 turns, a point every 2 m.
        let mut pts = Vec::new();
        let r = 70.0;
        for i in 0..100 {
            pts.push(Vec3::new(-r, 0.0, -100.0 + i as f32 * 2.0));
        }
        for i in 0..110 {
            let a = std::f32::consts::PI + i as f32 / 110.0 * std::f32::consts::PI;
            pts.push(Vec3::new(r * a.cos(), 0.0, 100.0 - r * a.sin()));
        }
        for i in 0..100 {
            pts.push(Vec3::new(r, 0.0, 100.0 - i as f32 * 2.0));
        }
        for i in 0..110 {
            let a = i as f32 / 110.0 * std::f32::consts::PI;
            pts.push(Vec3::new(r * a.cos(), 0.0, -100.0 - r * a.sin()));
        }
        RacingLine::new(pts)
    }

    fn run(bot: &mut BotDriver, secs: f32, others: &[Vec3]) -> (VehicleState, Vec<f32>) {
        let (s, samples) = run_with_positions(bot, secs, others);
        (s, samples.into_iter().map(|(_, l)| l).collect())
    }

    fn run_with_positions(
        bot: &mut BotDriver,
        secs: f32,
        others: &[Vec3],
    ) -> (VehicleState, Vec<(Vec3, f32)>) {
        let w = ground();
        let line = oval();
        let p = VehicleParams::default();
        let mut s = VehicleState::spawn(Vec3::new(-70.0, 0.6, -90.0), 0.0);
        let mut lateral = Vec::new();
        for _ in 0..(secs / DT) as usize {
            let input = bot.drive(&s, &p, &line, others, 1.0, DT);
            step(&mut s, &p, input, &w, DT);
            lateral.push((s.position, line.lateral_offset(s.position)));
        }
        (s, lateral)
    }

    #[test]
    fn follows_the_line() {
        let mut bot = BotDriver::new(1.0, 0.0);
        let (_, lat) = run(&mut bot, 40.0, &[]);
        let worst = lat[300..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(worst < 6.0, "strays {worst} m from the line");
    }

    #[test]
    fn lane_offset_shifts_the_path() {
        let mut bot = BotDriver::new(1.0, 3.0);
        let (_, samples) = run_with_positions(&mut bot, 30.0, &[]);
        // On the straights (turns are drifted, which cuts inside).
        let straight: Vec<f32> = samples[300..]
            .iter()
            .filter(|(p, _)| p.z.abs() < 70.0)
            .map(|(_, l)| *l)
            .collect();
        assert!(straight.len() > 200);
        let mean = straight.iter().sum::<f32>() / straight.len() as f32;
        assert!((mean - 3.0).abs() < 1.5, "mean lateral {mean}");
    }

    #[test]
    fn skill_caps_speed() {
        let mut bot = BotDriver::new(0.8, 0.0);
        let p = VehicleParams::default();
        let (s, _) = run(&mut bot, 6.0, &[]);
        assert!(s.speed() < p.max_speed * 0.8 + 1.0, "speed {}", s.speed());
        assert!(
            s.speed() > p.max_speed * 0.6,
            "but still racing: {}",
            s.speed()
        );
    }

    #[test]
    fn turns_around_when_facing_backwards() {
        let line = oval();
        let p = VehicleParams::default();
        // On the first straight (heading +Z) but facing -Z, nearly stopped (just spun out).
        let mut s = VehicleState::spawn(Vec3::new(-70.0, 0.45, -40.0), std::f32::consts::PI);
        s.grounded = true;
        let input = BotDriver::new(1.0, 0.0).drive(&s, &p, &line, &[], 1.0, DT);
        assert!(
            input.brake == 0.0 && input.throttle > 0.5,
            "drive out of it, don't reverse: {input:?}"
        );
        assert!(input.steer.abs() > 0.9, "full lock to turn around");
    }

    #[test]
    fn brakes_when_facing_backwards_at_speed() {
        let line = oval();
        let p = VehicleParams::default();
        let mut s = VehicleState::spawn(Vec3::new(-70.0, 0.45, -40.0), std::f32::consts::PI);
        s.grounded = true;
        s.velocity = s.rotation * Vec3::Z * 25.0;
        let input = BotDriver::new(1.0, 0.0).drive(&s, &p, &line, &[], 1.0, DT);
        assert!(
            input.brake > 0.5 && input.throttle == 0.0,
            "brake, don't coast the wrong way: {input:?}"
        );
    }

    #[test]
    fn rolling_backwards_steers_the_nose_toward_the_line() {
        // A reversing kart yaws the other way for the same steer. Facing up the first straight
        // (+Z) with the line off to the kart's right (-X), rolling backwards: forward driving
        // would steer right (+), so the mirrored steer is left, and the throttle stops the roll.
        let line = oval();
        let p = VehicleParams::default();
        let mut s = VehicleState::spawn(Vec3::new(-66.0, 0.45, -40.0), 0.0);
        s.grounded = true;
        let ahead = BotDriver::new(1.0, 0.0).drive(&s, &p, &line, &[], 1.0, DT);
        s.velocity = Vec3::new(0.0, 0.0, -8.0);
        let back = BotDriver::new(1.0, 0.0).drive(&s, &p, &line, &[], 1.0, DT);
        assert!(
            ahead.steer > 0.0,
            "toward the line going forward: {ahead:?}"
        );
        assert!(back.steer < 0.0, "mirrored while rolling back: {back:?}");
        assert!(back.throttle > 0.5 && back.brake == 0.0, "{back:?}");
    }

    #[test]
    fn spun_backwards_while_rolling_back_turns_around() {
        // Facing backwards and rolling backwards (so moving forward along the line): the
        // un-mirrored steering held the kart facing backwards for as long as it rolled.
        let w = ground();
        let line = oval();
        let p = VehicleParams::default();
        let mut s = VehicleState::spawn(Vec3::new(-70.0, 0.6, -60.0), std::f32::consts::PI);
        s.velocity = Vec3::new(0.0, 0.0, 10.0);
        let mut bot = BotDriver::new(1.0, 0.0);
        for _ in 0..(3.0 / DT) as usize {
            let input = bot.drive(&s, &p, &line, &[], 1.0, DT);
            step(&mut s, &p, input, &w, DT);
        }
        let along = s.forward().dot(line.tangent(line.nearest(s.position)));
        assert!(along > 0.7, "still facing backwards ({along:.2})");
        assert!(s.speed() > 5.0, "and driving on ({:.1} m/s)", s.speed());
    }

    #[test]
    fn turning_around_beside_a_barrier_swings_toward_the_middle() {
        let line = oval();
        let p = VehicleParams::default();
        for side in [-1.0f32, 1.0] {
            // Facing backwards near one edge of the first straight (+Z; right is -X).
            let x = -70.0 - side * 5.5;
            let mut s = VehicleState::spawn(Vec3::new(x, 0.45, -40.0), std::f32::consts::PI);
            s.grounded = true;
            s.velocity = s.forward() * 6.0;
            let mut bot = BotDriver::new(1.0, 0.0);
            let input = bot.drive(&s, &p, &line, &[], 1.0, DT);
            // Facing -Z, steering right (+) swings the nose toward +X, the middle for side +1.
            let toward = side;
            assert!(
                input.steer * toward > 0.9,
                "side {side}: steer {} should swing away from the near edge",
                input.steer
            );
        }
    }

    #[test]
    fn backs_out_when_pinned_nose_first() {
        // Pointing across the road at a standstill (as if nose-in to a barrier): a kart can't
        // turn without speed, so after a moment of going nowhere the bot reverses.
        let line = oval();
        let p = VehicleParams::default();
        let mut s = VehicleState::spawn(Vec3::new(-70.0, 0.45, -40.0), -1.7);
        s.grounded = true;
        let mut bot = BotDriver::new(1.0, 0.0);
        let first = bot.drive(&s, &p, &line, &[], 1.0, DT);
        assert!(first.throttle > 0.5, "tries to drive out first: {first:?}");
        let mut last = first;
        for _ in 0..(BLOCKED_SECONDS / DT) as usize + 2 {
            last = bot.drive(&s, &p, &line, &[], 1.0, DT);
        }
        assert!(
            last.brake > 0.5 && last.throttle == 0.0,
            "backs out: {last:?}"
        );
    }

    #[test]
    fn dodges_a_kart_ahead() {
        let line = oval();
        let p = VehicleParams::default();
        let mut s = VehicleState::spawn(Vec3::new(-70.0, 0.45, -60.0), 0.0);
        s.velocity = Vec3::new(0.0, 0.0, 25.0);
        s.grounded = true;
        let blocker = Vec3::new(-70.0, 0.45, -52.0);
        let mut bot = BotDriver::new(1.0, 0.0);
        let mut steer = 0.0;
        for _ in 0..10 {
            steer = bot.drive(&s, &p, &line, &[blocker], 1.0, DT).steer;
        }
        assert!(
            steer.abs() > 0.2,
            "should steer around the kart ahead (steer {steer})"
        );
    }
}
