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

/// One bot's driving style and short-term state.
#[derive(Debug, Clone)]
pub struct BotDriver {
    /// Share of the kart's top speed this bot holds (0.8–1.0 is a typical field).
    pub skill: f32,
    /// Preferred lateral offset from the racing line (m, + = right).
    pub lane: f32,
    dodge: f32,
}

impl BotDriver {
    pub fn new(skill: f32, lane: f32) -> Self {
        Self {
            skill,
            lane,
            dodge: 0.0,
        }
    }

    /// Chooses this tick's input. `others` are the other karts' positions; `speed_scale` is a
    /// race-level multiplier (catch-up).
    pub fn drive(
        &mut self,
        me: &VehicleState,
        params: &VehicleParams,
        line: &RacingLine,
        others: &[Vec3],
        speed_scale: f32,
    ) -> DriveInput {
        let i = line.nearest(me.position);
        let f = me.forward();
        let f = Vec3::new(f.x, 0.0, f.z).normalize_or(Vec3::Z);
        let speed = me.speed().max(0.0);

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
        let angle = f.cross(d).y.atan2(f.dot(d));

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

        let top = params.max_speed * self.skill * speed_scale;
        let throttle = if sharp || speed > top { 0.0 } else { 1.0 };
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
            brake: if sharp { 1.0 } else { 0.0 },
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
            let input = bot.drive(&s, &p, &line, others, 1.0);
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
            steer = bot.drive(&s, &p, &line, &[blocker], 1.0).steer;
        }
        assert!(
            steer.abs() > 0.2,
            "should steer around the kart ahead (steer {steer})"
        );
    }
}
