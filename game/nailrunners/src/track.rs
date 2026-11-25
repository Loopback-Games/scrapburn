//! The gray-box loop: a closed spline road with walls, a banked turn and a jump.

use glam::{Quat, Vec2, Vec3};
use rivet_physics::TriMesh;
use rivet_render::{MeshData, Vertex};

pub const ROAD_HALF_WIDTH: f32 = 7.0;
pub const WALL_HEIGHT: f32 = 1.5;
pub const JUMP_HEIGHT: f32 = 6.0;
const SAMPLE_SPACING: f32 = 2.0;
const CHECKPOINT_SPACING: f32 = 40.0;
const BANK_DEGREES: f32 = 15.0;
const GROUND_Y: f32 = -0.5;
const GROUND_MARGIN: f32 = 200.0;

const CONTROL_POINTS: [Vec2; 8] = [
    Vec2::new(0.0, 0.0),
    Vec2::new(220.0, 0.0),
    Vec2::new(300.0, 80.0),
    Vec2::new(300.0, 200.0),
    Vec2::new(220.0, 280.0),
    Vec2::new(60.0, 280.0),
    Vec2::new(-40.0, 200.0),
    Vec2::new(-60.0, 100.0),
];

const ROAD: [f32; 4] = [0.42, 0.42, 0.44, 1.0];
const ROAD_STRIPE: [f32; 4] = [0.62, 0.62, 0.64, 1.0];
const WALL: [f32; 4] = [0.85, 0.30, 0.18, 1.0];
const WALL_STRIPE: [f32; 4] = [0.95, 0.92, 0.88, 1.0];
const GROUND: [f32; 4] = [0.26, 0.30, 0.17, 1.0];

/// A plane the kart must cross, in order, to count a lap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Checkpoint {
    pub center: Vec3,
    pub forward: Vec3,
    pub half_width: f32,
}

pub struct Track {
    pub render: MeshData,
    pub collision: TriMesh,
    /// Road center every ~2 m; the last point repeats the first.
    pub centerline: Vec<Vec3>,
    pub checkpoints: Vec<Checkpoint>,
    /// Spawn position (on the road surface) and yaw.
    pub spawn: (Vec3, f32),
}

fn catmull_rom(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, t: f32) -> Vec2 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

/// Dense closed polyline through the control points, then resampled at equal arc length.
fn resampled_loop() -> Vec<Vec2> {
    let n = CONTROL_POINTS.len();
    let mut dense = Vec::new();
    for i in 0..n {
        let p = |k: usize| CONTROL_POINTS[(i + k + n - 1) % n];
        for s in 0..200 {
            dense.push(catmull_rom(p(0), p(1), p(2), p(3), s as f32 / 200.0));
        }
    }
    dense.push(dense[0]);
    let total: f32 = dense.windows(2).map(|w| w[0].distance(w[1])).sum();
    let count = (total / SAMPLE_SPACING).round() as usize;
    let step = total / count as f32;
    let mut out = Vec::with_capacity(count + 1);
    let (mut seg, mut seg_start) = (0usize, 0.0f32);
    for k in 0..count {
        let target = k as f32 * step;
        while seg_start + dense[seg].distance(dense[seg + 1]) < target {
            seg_start += dense[seg].distance(dense[seg + 1]);
            seg += 1;
        }
        let len = dense[seg].distance(dense[seg + 1]).max(1e-6);
        out.push(dense[seg].lerp(dense[seg + 1], (target - seg_start) / len));
    }
    out
}

/// Smooth 0→1→0 window over `[a, b]` with `ramp`-long ends.
fn window(v: f32, a: f32, b: f32, ramp: f32) -> f32 {
    let up = ((v - (a - ramp)) / ramp).clamp(0.0, 1.0);
    let down = (((b + ramp) - v) / ramp).clamp(0.0, 1.0);
    let s = |x: f32| x * x * (3.0 - 2.0 * x);
    s(up).min(s(down))
}

fn jump_height(p: Vec2) -> f32 {
    if p.y < 250.0 || !(110.0..=170.0).contains(&p.x) {
        return 0.0;
    }
    let t = (p.x - 110.0) / 60.0;
    JUMP_HEIGHT * 0.5 * (1.0 - (std::f32::consts::TAU * t).cos())
}

/// Banking weight for the right-hand turn between (300, 80) and (300, 200).
fn bank_weight(p: Vec2) -> f32 {
    if p.x < 260.0 {
        return 0.0;
    }
    window(p.y, 80.0, 200.0, 30.0)
}

fn quad(
    m: &mut MeshData,
    c: &mut Vec<Vec3>,
    ci: &mut Vec<[u32; 3]>,
    v: [Vec3; 4],
    color: [f32; 4],
) {
    let n = (v[1] - v[0]).cross(v[2] - v[0]).normalize_or_zero();
    let base = m.vertices.len() as u32;
    for p in v {
        m.vertices.push(Vertex {
            pos: p.to_array(),
            normal: n.to_array(),
            color,
        });
    }
    m.indices
        .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    let cb = c.len() as u32;
    c.extend(v);
    ci.extend([[cb, cb + 1, cb + 2], [cb, cb + 2, cb + 3]]);
}

pub fn build_graybox_loop() -> Track {
    let flat = resampled_loop();
    let n = flat.len();
    let bank_max = BANK_DEGREES.to_radians();

    // Frames: position, forward, right, up. Banks pivot on the inner edge so nothing dips below 0.
    let mut frames = Vec::with_capacity(n);
    for i in 0..n {
        let p = flat[i];
        let prev = flat[(i + n - 1) % n];
        let next = flat[(i + 1) % n];
        let h = |q: Vec2| jump_height(q);
        let fwd =
            (Vec3::new(next.x, h(next), next.y) - Vec3::new(prev.x, h(prev), prev.y)).normalize();
        let right_flat = fwd.cross(Vec3::Y).normalize();
        // Turn direction: positive when turning right.
        let turn = (next - p).perp_dot(p - prev).signum();
        let bank = bank_weight(p) * bank_max * turn;
        // Rotating about fwd by -bank lifts the left (outer) side on a right turn.
        let rot = Quat::from_axis_angle(fwd, -bank);
        let right = rot * right_flat;
        let up = right.cross(fwd).normalize();
        let lift = ROAD_HALF_WIDTH * bank.abs().sin();
        let center = Vec3::new(p.x, h(p) + lift, p.y);
        frames.push((center, fwd, right, up));
    }

    let mut render = MeshData::default();
    let (mut cpos, mut cidx) = (Vec::new(), Vec::new());
    for i in 0..n {
        let (c0, _, r0, u0) = frames[i];
        let (c1, _, r1, u1) = frames[(i + 1) % n];
        let s = i as f32 * SAMPLE_SPACING;
        let stripe = s % 10.0 < SAMPLE_SPACING;
        let (l0, rr0) = (c0 - r0 * ROAD_HALF_WIDTH, c0 + r0 * ROAD_HALF_WIDTH);
        let (l1, rr1) = (c1 - r1 * ROAD_HALF_WIDTH, c1 + r1 * ROAD_HALF_WIDTH);
        // Road, counter-clockwise seen from above (normal up).
        quad(
            &mut render,
            &mut cpos,
            &mut cidx,
            [l0, rr0, rr1, l1],
            if stripe { ROAD_STRIPE } else { ROAD },
        );
        let wall = if ((s / 4.0) as usize).is_multiple_of(2) {
            WALL
        } else {
            WALL_STRIPE
        };
        // Left wall faces right (inward), right wall faces left.
        quad(
            &mut render,
            &mut cpos,
            &mut cidx,
            [l0, l1, l1 + u1 * WALL_HEIGHT, l0 + u0 * WALL_HEIGHT],
            wall,
        );
        quad(
            &mut render,
            &mut cpos,
            &mut cidx,
            [rr0, rr0 + u0 * WALL_HEIGHT, rr1 + u1 * WALL_HEIGHT, rr1],
            wall,
        );
    }

    // Ground plane well beyond the loop.
    let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for p in &flat {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    lo -= Vec2::splat(GROUND_MARGIN);
    hi += Vec2::splat(GROUND_MARGIN);
    let g = |x: f32, z: f32| Vec3::new(x, GROUND_Y, z);
    quad(
        &mut render,
        &mut cpos,
        &mut cidx,
        [g(lo.x, lo.y), g(lo.x, hi.y), g(hi.x, hi.y), g(hi.x, lo.y)],
        GROUND,
    );

    let collision = TriMesh::new(cpos, cidx, 8.0);

    let mut centerline: Vec<Vec3> = frames.iter().map(|f| f.0).collect();
    centerline.push(centerline[0]);

    let every = (CHECKPOINT_SPACING / SAMPLE_SPACING).round() as usize;
    let checkpoints = (0..n)
        .step_by(every)
        .map(|i| Checkpoint {
            center: frames[i].0,
            forward: frames[i].1,
            half_width: ROAD_HALF_WIDTH + 0.5,
        })
        .collect();

    let spawn_i = n - 2;
    let (sp, sf, _, _) = frames[spawn_i];
    let spawn = (sp, sf.x.atan2(sf.z));

    Track {
        render,
        collision,
        centerline,
        checkpoints,
        spawn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn loop_is_closed() {
        let t = build_graybox_loop();
        let first = t.centerline[0];
        let last = *t.centerline.last().unwrap();
        assert!(first.distance(last) < 2.5, "gap {}", first.distance(last));
        let len: f32 = t.centerline.windows(2).map(|w| w[0].distance(w[1])).sum();
        // The pinned control points plus the jump and banking come to ~1.1 km of 3D road.
        assert!((900.0..=1150.0).contains(&len), "length {len}");
    }

    #[test]
    fn road_is_drivable() {
        let t = build_graybox_loop();
        for (i, c) in t.centerline.iter().enumerate() {
            let hit = t
                .collision
                .raycast(*c + Vec3::Y * 3.0, Vec3::NEG_Y, 10.0)
                .unwrap_or_else(|| panic!("no road under centerline point {i} at {c}"));
            assert!(
                (hit.point.y - c.y).abs() < 0.05,
                "point {i}: road at {} expected {}",
                hit.point.y,
                c.y
            );
            assert!(hit.normal.y > 0.9, "point {i}: normal {}", hit.normal);
        }
    }

    #[test]
    fn walls_present() {
        let t = build_graybox_loop();
        // On the first straight, look sideways from the centerline toward both edges.
        let c = t.centerline[20];
        let fwd = (t.centerline[21] - t.centerline[19]).normalize();
        let right = fwd.cross(Vec3::Y).normalize();
        for side in [right, -right] {
            let hit = t
                .collision
                .raycast(c + Vec3::Y * 0.5, side, 20.0)
                .expect("wall");
            assert!((hit.t - ROAD_HALF_WIDTH).abs() < 0.3, "wall at {} m", hit.t);
        }
    }

    #[test]
    fn render_faces_point_up_and_inward() {
        let t = build_graybox_loop();
        let center =
            t.centerline.iter().fold(Vec3::ZERO, |a, p| a + *p) / t.centerline.len() as f32;
        let m = &t.render;
        for tri in m.indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from_array(m.vertices[tri[i] as usize].pos));
            let n = (b - a).cross(c - a).normalize_or_zero();
            if n.y.abs() > 0.7 {
                assert!(n.y > 0.0, "floor triangle faces down at {a}");
            } else {
                // Walls face back toward the road; the road centerline is nearest.
                let mid = (a + b + c) / 3.0;
                let nearest = t
                    .centerline
                    .iter()
                    .min_by(|p, q| p.distance_squared(mid).total_cmp(&q.distance_squared(mid)))
                    .unwrap();
                let to_road = Vec3::new(nearest.x - mid.x, 0.0, nearest.z - mid.z);
                assert!(
                    n.dot(to_road) > 0.0,
                    "wall faces away from the road at {mid} (loop center {center})"
                );
            }
        }
    }

    #[test]
    fn jump_height() {
        let t = build_graybox_loop();
        let max_y = t
            .centerline
            .iter()
            .filter(|p| p.z > 250.0)
            .map(|p| p.y)
            .fold(f32::MIN, f32::max);
        assert!((max_y - JUMP_HEIGHT).abs() < 0.1, "jump top {max_y}");
    }

    #[test]
    fn checkpoints_spaced_along_loop() {
        let t = build_graybox_loop();
        assert!(
            t.checkpoints.len() >= 20,
            "{} checkpoints",
            t.checkpoints.len()
        );
        assert!(t.checkpoints[0].center.distance(t.centerline[0]) < 0.01);
    }
}
