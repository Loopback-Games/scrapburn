//! The desert loop: a closed spline road with curbs, barriers, a banked turn, a jump and boost
//! pads, set in dunes and canyon walls.

use glam::{Mat4, Quat, Vec2, Vec3};
use rivet_physics::{Surface, TriMesh};
use rivet_render::{EMISSIVE, MeshData, TRIPLANAR_B, UNTEXTURED, Vertex, box_mesh, material};

use crate::materials::Layer;
use crate::terrain;

pub const ROAD_HALF_WIDTH: f32 = 7.0;
pub const WALL_HEIGHT: f32 = 1.5;
/// Barrier collision reaches far above the visible wall so jumps and blasts can't launch a
/// kart out of the circuit.
const COLLISION_WALL_HEIGHT: f32 = 8.0;
pub const JUMP_HEIGHT: f32 = 6.0;
const SAMPLE_SPACING: f32 = 2.0;
const CHECKPOINT_SPACING: f32 = 40.0;
const BANK_DEGREES: f32 = 15.0;
/// Texture repeat lengths (m).
const ROAD_TEX: f32 = 7.0;
const WALL_TEX: f32 = 3.0;
const CURB_WIDTH: f32 = 1.1;
const CURB_LIFT: f32 = 0.02;
const WALL_BASE: f32 = 0.5;
const PAD_LIFT: f32 = 0.04;
const PAD_HALF_WIDTH: f32 = 2.6;
const PAD_SEGMENTS: usize = 5;

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

const WHITE: [f32; 4] = [0.72, 0.72, 0.74, 0.0];
const CURB_RED: [f32; 4] = [0.95, 0.18, 0.12, 0.0];
const CURB_WHITE: [f32; 4] = [1.0, 0.98, 0.95, 0.0];
const BARRIER: [f32; 4] = [1.0, 0.95, 0.9, 0.0];

/// A plane the kart must cross, in order, to count a lap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Checkpoint {
    pub center: Vec3,
    pub forward: Vec3,
    pub half_width: f32,
}

pub struct Track {
    /// Road, curbs, barriers, pads, start line, gantry.
    pub road: MeshData,
    pub terrain: MeshData,
    /// Rocks and other scenery.
    pub props: MeshData,
    pub collision: TriMesh,
    /// Road center every ~2 m; the last point repeats the first.
    pub centerline: Vec<Vec3>,
    pub checkpoints: Vec<Checkpoint>,
    /// Spawn position (on the road surface) and yaw.
    pub spawn: (Vec3, f32),
    pub rocks: Vec<Vec3>,
    pub boost_pads: Vec<Vec3>,
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

#[derive(Default)]
struct Collision {
    positions: Vec<Vec3>,
    indices: Vec<[u32; 3]>,
    surfaces: Vec<Surface>,
}

impl Collision {
    fn quad(&mut self, v: [Vec3; 4], surface: Surface) {
        let b = self.positions.len() as u32;
        self.positions.extend(v);
        self.indices.extend([[b, b + 1, b + 2], [b, b + 2, b + 3]]);
        self.surfaces.extend([surface, surface]);
    }
}

/// Appends a quad (counter-clockwise front face) with per-corner UVs.
fn quad(m: &mut MeshData, v: [Vec3; 4], uv: [[f32; 2]; 4], color: [f32; 4], mat: u32) {
    let n = (v[1] - v[0]).cross(v[2] - v[0]).normalize_or_zero();
    let base = m.vertices.len() as u32;
    for (p, t) in v.iter().zip(uv) {
        m.vertices.push(Vertex {
            pos: p.to_array(),
            normal: n.to_array(),
            uv: t,
            color,
            material: mat,
        });
    }
    m.indices
        .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// A box with a world-space triplanar material, transformed by `xf`.
fn textured_box(m: &mut MeshData, half: Vec3, xf: Mat4, layer: Layer, color: [f32; 3]) {
    let mut b = box_mesh(half, [color[0], color[1], color[2], 1.0]);
    let nm = xf;
    for v in &mut b.vertices {
        v.pos = xf.transform_point3(Vec3::from_array(v.pos)).to_array();
        v.normal = nm
            .transform_vector3(Vec3::from_array(v.normal))
            .normalize()
            .to_array();
        v.material = material(layer as u32, layer as u32, TRIPLANAR_B);
    }
    m.append(&b);
}

fn emissive_box(m: &mut MeshData, half: Vec3, xf: Mat4, color: [f32; 3]) {
    let mut b = box_mesh(half, [color[0], color[1], color[2], 0.0]);
    for v in &mut b.vertices {
        v.pos = xf.transform_point3(Vec3::from_array(v.pos)).to_array();
        v.normal = xf
            .transform_vector3(Vec3::from_array(v.normal))
            .normalize()
            .to_array();
        v.material = material(0, 0, UNTEXTURED | EMISSIVE);
    }
    m.append(&b);
}

/// Deterministic pseudo-random sequence for scenery placement.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// A lumpy low-poly rock (subdivided octahedron with noise), centered at the origin.
fn rock_mesh(seed: u32) -> Vec<(Vec3, Vec3)> {
    let base = [
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Y,
        Vec3::NEG_Y,
        Vec3::Z,
        Vec3::NEG_Z,
    ];
    let faces = [
        [0, 2, 4],
        [4, 2, 1],
        [1, 2, 5],
        [5, 2, 0],
        [4, 3, 0],
        [1, 3, 4],
        [5, 3, 1],
        [0, 3, 5],
    ];
    let mut tris = Vec::new();
    for f in faces {
        let (a, b, c) = (base[f[0]], base[f[1]], base[f[2]]);
        let (ab, bc, ca) = (
            (a + b).normalize(),
            (b + c).normalize(),
            (c + a).normalize(),
        );
        for t in [[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]] {
            tris.push(t);
        }
    }
    let bump = |p: Vec3| {
        let n = terrain::fbm(p.x * 3.0 + seed as f32 * 7.1, p.z * 3.0 + p.y * 5.0, 1.0);
        p * (0.75 + 0.5 * n) * Vec3::new(1.0, 0.65, 1.0)
    };
    let mut out = Vec::new();
    for t in tris {
        let v = t.map(bump);
        let n = (v[1] - v[0]).cross(v[2] - v[0]).normalize_or_zero();
        for p in v {
            out.push((p, n));
        }
    }
    out
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
        let turn = (next - p).perp_dot(p - prev).signum();
        let bank = bank_weight(p) * bank_max * turn;
        let rot = Quat::from_axis_angle(fwd, -bank);
        let right = rot * right_flat;
        let up = right.cross(fwd).normalize();
        let lift = ROAD_HALF_WIDTH * bank.abs().sin();
        let center = Vec3::new(p.x, h(p) + lift, p.y);
        frames.push((center, fwd, right, up));
    }
    let edge = |i: usize, side: f32, inset: f32| {
        let (c, _, r, _) = frames[i % n];
        c + r * side * (ROAD_HALF_WIDTH - inset)
    };

    let mut road = MeshData::default();
    let mut col = Collision::default();
    let asphalt = material(Layer::Asphalt as u32, Layer::Asphalt as u32, 0);

    // Road surface first (contiguous, so its UVs can be checked).
    for i in 0..n {
        let (v0, v1) = (
            i as f32 * SAMPLE_SPACING / ROAD_TEX,
            (i + 1) as f32 * SAMPLE_SPACING / ROAD_TEX,
        );
        let u = 2.0 * ROAD_HALF_WIDTH / ROAD_TEX;
        let v = [
            edge(i, -1.0, 0.0),
            edge(i, 1.0, 0.0),
            edge(i + 1, 1.0, 0.0),
            edge(i + 1, -1.0, 0.0),
        ];
        quad(
            &mut road,
            v,
            [[0.0, v0], [u, v0], [u, v1], [0.0, v1]],
            WHITE,
            asphalt,
        );
        col.quad(v, Surface::Road);
    }

    // Curbs, barriers.
    let concrete = material(Layer::Concrete as u32, Layer::Concrete as u32, 0);
    let metal = material(Layer::MetalRust as u32, Layer::MetalRust as u32, 0);
    for i in 0..n {
        let (s0, s1) = (i as f32 * SAMPLE_SPACING, (i + 1) as f32 * SAMPLE_SPACING);
        let (_, _, _, u0) = frames[i];
        let (_, _, _, u1) = frames[(i + 1) % n];
        let curb = if i % 2 == 0 { CURB_RED } else { CURB_WHITE };
        for side in [-1.0f32, 1.0] {
            let lift0 = u0 * CURB_LIFT;
            let lift1 = u1 * CURB_LIFT;
            let (o0, o1) = (edge(i, side, 0.0) + lift0, edge(i + 1, side, 0.0) + lift1);
            let (i0, i1) = (
                edge(i, side, CURB_WIDTH) + lift0,
                edge(i + 1, side, CURB_WIDTH) + lift1,
            );
            let uv = [
                [0.0, s0 / 2.0],
                [0.5, s0 / 2.0],
                [0.5, s1 / 2.0],
                [0.0, s1 / 2.0],
            ];
            if side < 0.0 {
                quad(&mut road, [o0, i0, i1, o1], uv, curb, concrete);
            } else {
                quad(&mut road, [i0, o0, o1, i1], uv, curb, concrete);
            }

            // Barrier: concrete base, rusty metal above; faces the road.
            let (e0, e1) = (edge(i, side, 0.0), edge(i + 1, side, 0.0));
            let (b0, b1) = (e0 + u0 * WALL_BASE, e1 + u1 * WALL_BASE);
            let (t0, t1) = (e0 + u0 * WALL_HEIGHT, e1 + u1 * WALL_HEIGHT);
            let (ua, ub) = (s0 / WALL_TEX, s1 / WALL_TEX);
            let hb = WALL_BASE / WALL_TEX;
            let ht = WALL_HEIGHT / WALL_TEX;
            if side < 0.0 {
                quad(
                    &mut road,
                    [e0, e1, b1, b0],
                    [[ua, 0.0], [ub, 0.0], [ub, hb], [ua, hb]],
                    BARRIER,
                    concrete,
                );
                quad(
                    &mut road,
                    [b0, b1, t1, t0],
                    [[ua, hb], [ub, hb], [ub, ht], [ua, ht]],
                    BARRIER,
                    metal,
                );
                col.quad(
                    [
                        e0,
                        e1,
                        e1 + u1 * COLLISION_WALL_HEIGHT,
                        e0 + u0 * COLLISION_WALL_HEIGHT,
                    ],
                    Surface::Wall,
                );
            } else {
                quad(
                    &mut road,
                    [e0, b0, b1, e1],
                    [[ua, 0.0], [ua, hb], [ub, hb], [ub, 0.0]],
                    BARRIER,
                    concrete,
                );
                quad(
                    &mut road,
                    [b0, t0, t1, b1],
                    [[ua, hb], [ua, ht], [ub, ht], [ub, hb]],
                    BARRIER,
                    metal,
                );
                col.quad(
                    [
                        e0,
                        e0 + u0 * COLLISION_WALL_HEIGHT,
                        e1 + u1 * COLLISION_WALL_HEIGHT,
                        e1,
                    ],
                    Surface::Wall,
                );
            }
            // Cap on top of the barrier, a little outward.
            let (_, _, r0, _) = frames[i];
            let (_, _, r1, _) = frames[(i + 1) % n];
            let (c0, c1) = (t0 + r0 * side * 0.35, t1 + r1 * side * 0.35);
            let cap_uv = [[ua, 0.0], [ub, 0.0], [ub, 0.12], [ua, 0.12]];
            if side < 0.0 {
                quad(&mut road, [t0, t1, c1, c0], cap_uv, BARRIER, metal);
            } else {
                quad(&mut road, [t0, c0, c1, t1], cap_uv, BARRIER, metal);
            }
        }
    }

    // Checkered start line across the road at checkpoint 0.
    {
        let (c, f, r, u) = frames[0];
        let cols = 10;
        let sq = 2.0 * ROAD_HALF_WIDTH / cols as f32;
        for row in 0..2 {
            for k in 0..cols {
                let a = c
                    + r * (-ROAD_HALF_WIDTH + k as f32 * sq)
                    + f * (row as f32 * sq - sq)
                    + u * 0.015;
                let v = [a, a + r * sq, a + r * sq + f * sq, a + f * sq];
                let dark = (row + k) % 2 == 0;
                let col4 = if dark {
                    [0.06, 0.06, 0.07, 0.0]
                } else {
                    [0.95, 0.95, 0.95, 0.0]
                };
                quad(
                    &mut road,
                    v,
                    [[0.0; 2]; 4],
                    col4,
                    material(0, 0, UNTEXTURED),
                );
            }
        }
        // Gantry: two rusty pillars and a beam with a glowing strip.
        let yaw = f.x.atan2(f.z);
        let rot = Quat::from_rotation_y(yaw);
        for side in [-1.0f32, 1.0] {
            let p = c + r * side * (ROAD_HALF_WIDTH + 1.2) + Vec3::Y * 3.5;
            textured_box(
                &mut road,
                Vec3::new(0.45, 4.0, 0.45),
                Mat4::from_rotation_translation(rot, p),
                Layer::MetalRust,
                [1.0, 1.0, 1.0],
            );
        }
        let beam = c + Vec3::Y * 7.2;
        textured_box(
            &mut road,
            Vec3::new(ROAD_HALF_WIDTH + 1.8, 0.6, 0.5),
            Mat4::from_rotation_translation(rot, beam),
            Layer::MetalRust,
            [0.9, 0.85, 0.8],
        );
        emissive_box(
            &mut road,
            Vec3::new(ROAD_HALF_WIDTH + 1.2, 0.12, 0.08),
            Mat4::from_rotation_translation(rot, beam - Vec3::Y * 0.65 - f * 0.45),
            [1.0, 0.55, 0.15],
        );
    }

    // Boost pads on the two long straights: glowing chevron strips.
    let mut boost_pads = Vec::new();
    for at in [n * 9 / 100, n * 60 / 100] {
        for k in 0..PAD_SEGMENTS {
            let i = at + k;
            let (c0, _, r0, u0) = frames[i % n];
            let (c1, _, r1, u1) = frames[(i + 1) % n];
            let v = [
                c0 - r0 * PAD_HALF_WIDTH + u0 * PAD_LIFT,
                c0 + r0 * PAD_HALF_WIDTH + u0 * PAD_LIFT,
                c1 + r1 * PAD_HALF_WIDTH + u1 * PAD_LIFT,
                c1 - r1 * PAD_HALF_WIDTH + u1 * PAD_LIFT,
            ];
            let glow = if k % 2 == 0 {
                [1.0, 0.45, 0.05, 0.0]
            } else {
                [1.0, 0.8, 0.2, 0.0]
            };
            quad(
                &mut road,
                v,
                [[0.0; 2]; 4],
                glow,
                material(0, 0, UNTEXTURED | EMISSIVE),
            );
            col.quad(v, Surface::BoostPad);
        }
        let (c, _, _, u) = frames[(at + PAD_SEGMENTS / 2) % n];
        boost_pads.push(c + u * PAD_LIFT);
    }

    // Terrain.
    let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for p in &flat {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    lo -= Vec2::splat(terrain::MARGIN);
    hi += Vec2::splat(terrain::MARGIN);
    let mut centerline: Vec<Vec3> = frames.iter().map(|f| f.0).collect();
    centerline.push(centerline[0]);
    let (terrain_mesh, tpos, ttris) = terrain::build(&centerline, lo, hi);
    let base = col.positions.len() as u32;
    col.positions.extend(tpos);
    col.surfaces
        .extend(std::iter::repeat_n(Surface::Offroad, ttris.len()));
    col.indices.extend(
        ttris
            .iter()
            .map(|t| [t[0] + base, t[1] + base, t[2] + base]),
    );

    // Rocks scattered in the dunes, away from the road.
    let mut props = MeshData::default();
    let mut rocks = Vec::new();
    let mut rng = Rng(0x5eed_cafe);
    let rock_mat = material(Layer::Rock as u32, Layer::Rock as u32, TRIPLANAR_B);
    let mut tries = 0;
    while rocks.len() < 110 && tries < 8000 {
        tries += 1;
        let x = lo.x + 100.0 + rng.next() * (hi.x - lo.x - 200.0);
        let z = lo.y + 100.0 + rng.next() * (hi.y - lo.y - 200.0);
        let d = terrain::distance_to(&centerline, x, z);
        if !(ROAD_HALF_WIDTH + 10.0..240.0).contains(&d) {
            continue;
        }
        // Boulders: bigger farther out, a few big ones near the road for parallax.
        let scale = 2.0 + rng.next() * rng.next() * 6.0 + (d / 240.0) * 5.0;
        let y = terrain::height_at(&centerline, x, z) - 0.3 * scale;
        let pos = Vec3::new(x, y, z);
        let rot = Quat::from_rotation_y(rng.next() * std::f32::consts::TAU);
        let xf = Mat4::from_scale_rotation_translation(Vec3::splat(scale), rot, pos);
        let tint = 0.85 + rng.next() * 0.15;
        let first = props.vertices.len() as u32;
        let mut cpts = Vec::new();
        for (p, nrm) in rock_mesh(rocks.len() as u32) {
            let wp = xf.transform_point3(p);
            props.vertices.push(Vertex {
                pos: wp.to_array(),
                normal: (rot * nrm).to_array(),
                uv: [0.0, 0.0],
                color: [tint, tint * 0.97, tint * 0.93, 1.0],
                material: rock_mat,
            });
            cpts.push(wp);
        }
        let count = props.vertices.len() as u32 - first;
        props.indices.extend(first..first + count);
        let cb = col.positions.len() as u32;
        col.positions.extend(cpts);
        for k in (0..count).step_by(3) {
            col.indices.push([cb + k, cb + k + 1, cb + k + 2]);
            col.surfaces.push(Surface::Wall);
        }
        rocks.push(pos);
    }

    let collision = TriMesh::with_surfaces(col.positions, col.indices, col.surfaces, 8.0);

    let every = (CHECKPOINT_SPACING / SAMPLE_SPACING).round() as usize;
    let checkpoints = (0..n)
        .step_by(every)
        .map(|i| Checkpoint {
            center: frames[i].0,
            forward: frames[i].1,
            half_width: ROAD_HALF_WIDTH + 0.5,
        })
        .collect();

    let (sp, sf, _, _) = frames[n - 2];
    let spawn = (sp, sf.x.atan2(sf.z));

    Track {
        road,
        terrain: terrain_mesh,
        props,
        collision,
        centerline,
        checkpoints,
        spawn,
        rocks,
        boost_pads,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Vec2, Vec3};

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
        let m = &t.road;
        for tri in m.indices.chunks(3) {
            // Road, curbs and barriers use plain (flag-free) materials; boxes are excluded.
            if m.vertices[tri[0] as usize].material >> 16 != 0 {
                continue;
            }
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
                // Only barrier faces sit right at the road edge; closed props (gantry) don't.
                if (to_road.length() - ROAD_HALF_WIDTH).abs() > 0.6 {
                    continue;
                }
                assert!(
                    n.dot(to_road) > 0.0,
                    "wall faces away from the road at {mid} (loop center {center})"
                );
            }
        }
    }

    #[test]
    fn surfaces_road_offroad_wall() {
        use rivet_physics::Surface;
        let t = build_graybox_loop();
        let c = t.centerline[20];
        let down = |p: Vec3| {
            t.collision
                .raycast(p + Vec3::Y * 3.0, Vec3::NEG_Y, 10.0)
                .unwrap()
                .surface
        };
        assert_eq!(down(c), Surface::Road);
        let high = |p: Vec3| {
            t.collision
                .raycast(p + Vec3::Y * 300.0, Vec3::NEG_Y, 400.0)
                .unwrap()
                .surface
        };
        assert_eq!(
            high(Vec3::new(140.0, 0.0, 140.0)),
            Surface::Offroad,
            "infield is offroad"
        );
        let fwd = (t.centerline[21] - t.centerline[19]).normalize();
        let right = fwd.cross(Vec3::Y).normalize();
        let wall = t.collision.raycast(c + Vec3::Y * 0.5, right, 20.0).unwrap();
        assert_eq!(wall.surface, Surface::Wall);
    }

    #[test]
    fn road_uvs_continuous() {
        let t = build_graybox_loop();
        // Consecutive road quads share their v coordinate at the seam (no texture jumps).
        let road: Vec<_> = t.road.vertices.chunks(4).take(50).collect();
        for w in road.windows(2) {
            assert!(
                (w[0][2].uv[1] - w[1][1].uv[1]).abs() < 1e-4,
                "{:?} vs {:?}",
                w[0][2].uv,
                w[1][1].uv
            );
        }
    }

    #[test]
    fn terrain_stays_below_road() {
        let t = build_graybox_loop();
        for c in &t.centerline {
            let h = terrain::height_at(&t.centerline, c.x, c.z);
            assert!(h <= c.y - 0.4, "terrain {h} pokes through road at {c}");
        }
    }

    #[test]
    fn terrain_faces_up() {
        let t = build_graybox_loop();
        for tri in t.terrain.indices.chunks(3) {
            let [a, b, c] =
                [0, 1, 2].map(|i| Vec3::from_array(t.terrain.vertices[tri[i] as usize].pos));
            assert!(
                (b - a).cross(c - a).y > 0.0,
                "terrain triangle faces down near {a}"
            );
        }
    }

    #[test]
    fn rocks_avoid_road() {
        let t = build_graybox_loop();
        assert!(t.rocks.len() >= 40, "only {} rocks", t.rocks.len());
        for r in &t.rocks {
            let d = t
                .centerline
                .iter()
                .map(|c| Vec2::new(c.x - r.x, c.z - r.z).length())
                .fold(f32::MAX, f32::min);
            assert!(
                d > ROAD_HALF_WIDTH + 6.0,
                "rock {r} only {d} m from the road"
            );
        }
    }

    #[test]
    fn boost_pads_on_track() {
        use rivet_physics::Surface;
        let t = build_graybox_loop();
        assert_eq!(t.boost_pads.len(), 2);
        for p in &t.boost_pads {
            let hit = t
                .collision
                .raycast(*p + Vec3::Y * 2.0, Vec3::NEG_Y, 5.0)
                .unwrap();
            assert_eq!(hit.surface, Surface::BoostPad, "pad at {p}");
        }
    }

    #[test]
    fn barrier_collision_reaches_well_above_the_visible_wall() {
        let t = build_graybox_loop();
        let c = t.centerline[20];
        let fwd = (t.centerline[21] - t.centerline[19]).normalize();
        let right = fwd.cross(Vec3::Y).normalize();
        let hit = t
            .collision
            .raycast(c + Vec3::Y * 5.0, right, 20.0)
            .expect("invisible wall above the barrier");
        assert!((hit.t - ROAD_HALF_WIDTH).abs() < 0.5);
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
