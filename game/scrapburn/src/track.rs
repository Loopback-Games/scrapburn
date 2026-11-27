//! The desert loop: a closed spline road with curbs, barriers, a banked turn, a jump and boost
//! pads, set in dunes and canyon walls.

pub mod canyon;
mod castle;

use glam::{Mat4, Quat, Vec2, Vec3};
use rivet_physics::{Surface, TriMesh};
use rivet_render::{EMISSIVE, MeshData, TRIPLANAR_B, UNTEXTURED, Vertex, box_mesh, material};

use crate::materials::Layer;
use crate::terrain::{self, TerrainKind};

pub const ROAD_HALF_WIDTH: f32 = 7.0;
pub const WALL_HEIGHT: f32 = 1.5;
/// Barrier collision reaches far above the visible wall so jumps and blasts can't launch a
/// kart out of the circuit.
const COLLISION_WALL_HEIGHT: f32 = 8.0;
pub const JUMP_HEIGHT: f32 = 6.0;
const SAMPLE_SPACING: f32 = 2.0;
const CHECKPOINT_SPACING: f32 = 40.0;
const BANK_DEGREES: f32 = 15.0;
/// Half-width (samples, so ×2 m) of the window curvature banking is smoothed over.
const BANK_SMOOTHING: usize = 12;
/// Texture repeat lengths (m).
const WALL_TEX: f32 = 3.0;
const CURB_WIDTH: f32 = 1.1;
const SAND_SHOULDER_WIDTH: f32 = 8.0;
const CURB_LIFT: f32 = 0.02;
const WALL_BASE: f32 = 0.5;
const PAD_LIFT: f32 = 0.04;
const PAD_HALF_WIDTH: f32 = 2.6;
const PAD_SEGMENTS: usize = 5;

/// Dustbowl: a kink off the start straight, the banked right-hander, the dune jump on the top
/// straight, then a hook that cuts back into the canyon floor before the run home.
const DUSTBOWL_POINTS: [Vec2; 16] = [
    Vec2::new(0.0, 0.0),
    Vec2::new(150.0, 0.0),
    Vec2::new(205.0, -35.0),
    Vec2::new(265.0, -20.0),
    Vec2::new(300.0, 60.0),
    Vec2::new(300.0, 200.0),
    Vec2::new(255.0, 265.0),
    Vec2::new(200.0, 282.0),
    Vec2::new(80.0, 282.0),
    Vec2::new(10.0, 262.0),
    Vec2::new(-45.0, 215.0),
    Vec2::new(0.0, 165.0),
    Vec2::new(55.0, 130.0),
    Vec2::new(30.0, 75.0),
    Vec2::new(-45.0, 60.0),
    Vec2::new(-50.0, 15.0),
];

/// Grindstone: a chicane off the start, then a deep hairpin pocket into the bailey and back
/// out around the keep: tight and technical.
const GRINDSTONE_POINTS: [Vec2; 18] = [
    Vec2::new(0.0, 0.0),
    Vec2::new(130.0, 0.0),
    Vec2::new(165.0, -22.0),
    Vec2::new(205.0, -5.0),
    Vec2::new(240.0, 50.0),
    Vec2::new(235.0, 125.0),
    Vec2::new(150.0, 125.0),
    Vec2::new(80.0, 108.0),
    Vec2::new(16.0, 148.0),
    Vec2::new(75.0, 185.0),
    Vec2::new(150.0, 190.0),
    Vec2::new(220.0, 220.0),
    Vec2::new(205.0, 285.0),
    Vec2::new(110.0, 300.0),
    Vec2::new(30.0, 270.0),
    Vec2::new(-45.0, 215.0),
    Vec2::new(-70.0, 120.0),
    Vec2::new(-40.0, 40.0),
];

/// Skyway: a long launch straight, then S-bends both ways around the towers: fast, with a few
/// corners that need the brakes.
const SKYWAY_POINTS: [Vec2; 17] = [
    Vec2::new(0.0, 0.0),
    Vec2::new(115.0, -18.0),
    Vec2::new(218.0, 16.0),
    Vec2::new(320.0, 0.0),
    Vec2::new(430.0, 70.0),
    Vec2::new(420.0, 180.0),
    Vec2::new(320.0, 210.0),
    Vec2::new(280.0, 280.0),
    Vec2::new(350.0, 360.0),
    Vec2::new(270.0, 440.0),
    Vec2::new(110.0, 420.0),
    Vec2::new(50.0, 340.0),
    Vec2::new(-60.0, 380.0),
    Vec2::new(-170.0, 320.0),
    Vec2::new(-150.0, 190.0),
    Vec2::new(-70.0, 130.0),
    Vec2::new(-120.0, 40.0),
];

/// Which circuit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackId {
    Dustbowl,
    Grindstone,
    Skyway,
}

impl TrackId {
    /// Stable key for saved data (ghosts).
    pub fn key(self) -> &'static str {
        match self {
            TrackId::Dustbowl => "dustbowl-loop-v2",
            TrackId::Grindstone => "castle-grindstone-v2",
            TrackId::Skyway => "skyway-zero-v2",
        }
    }
}

/// How a circuit looks.
#[derive(Debug, Clone, Copy)]
pub struct TrackStyle {
    pub road: Layer,
    pub road_tint: [f32; 4],
    /// Meters per repeat of the road texture.
    pub road_tex: f32,
    /// What the road blends into at its edges (sand drifts, mossy stones), if anything.
    pub road_edge: Option<Layer>,
    /// Brightness of the worn racing groove down the middle (1 = none).
    pub groove: f32,
    pub curbs: ([f32; 4], [f32; 4]),
    /// Curbs glow (Skyway neon).
    pub curb_glow: bool,
    pub barrier_low: Layer,
    pub barrier_high: Layer,
    pub barrier_tint: [f32; 4],
    /// Visible barrier height. Zero opens the road onto a sandy shoulder.
    pub barrier_height: f32,
    /// Castle merlons along the barrier tops.
    pub crenellations: bool,
    /// Emissive strip along the barrier tops, and its color.
    pub rail_glow: Option<[f32; 3]>,
    /// Flaming torches every so often along the walls.
    pub torches: bool,
    pub gantry: Layer,
    pub gantry_glow: [f32; 3],
    pub pad_glow: ([f32; 4], [f32; 4]),
    pub terrain: TerrainKind,
}

/// A circuit: layout, features and look.
pub struct TrackDef {
    pub id: TrackId,
    pub name: &'static str,
    pub tagline: &'static str,
    points: &'static [Vec2],
    /// Height of the road at a point (flat position, lap fraction).
    height: fn(Vec2, f32) -> f32,
    /// Bank weight 0..1 at a point; `None` = bank by curvature everywhere.
    bank: Option<fn(Vec2) -> f32>,
    bank_degrees: f32,
    /// Boost pads (lap fractions).
    pads: &'static [f32],
    /// Item box rows (lap fractions).
    pub box_rows: &'static [f32],
    pub style: TrackStyle,
}

fn desert_height(p: Vec2, t: f32) -> f32 {
    let climb = ((t - 0.77) / 0.075).clamp(-1.0, 1.0);
    jump_height(p) + 2.3 * (1.0 + (std::f32::consts::PI * climb).cos())
}

fn grindstone_height(_p: Vec2, t: f32) -> f32 {
    // A gentle rise to the keep, and the drawbridge jump on the back stretch.
    let rise = 3.0 * (1.0 - (std::f32::consts::TAU * (t - 0.3)).cos()) * 0.5;
    let bump = |c: f32, w: f32, h: f32| {
        let x = ((t - c) / w).clamp(-1.0, 1.0);
        h * 0.5 * (1.0 + (std::f32::consts::PI * x).cos())
    };
    rise + bump(0.755, 0.03, 4.5)
}

fn skyway_height(_p: Vec2, t: f32) -> f32 {
    // Floating high above the city, rolling like a ribbon with two big crests.
    let tau = std::f32::consts::TAU;
    40.0 + 7.0 * (tau * t * 2.0).sin() + 4.0 * (tau * t * 5.0 + 1.3).sin()
}

pub const TRACKS: [TrackDef; 3] = [
    TrackDef {
        id: TrackId::Dustbowl,
        name: "DUSTBOWL LOOP",
        tagline: "PAST THE GIANT BONES. UNDER THE ORE TRAM.",
        points: &DUSTBOWL_POINTS,
        height: desert_height,
        bank: Some(bank_weight),
        bank_degrees: BANK_DEGREES,
        pads: &[0.09, 0.60],
        box_rows: &[0.27, 0.5, 0.78],
        style: TrackStyle {
            road: Layer::Dirt,
            road_tint: [0.69, 0.78, 0.84, 0.0],
            road_tex: 9.0,
            road_edge: Some(Layer::Sand),
            groove: 0.78,
            curbs: ([0.52, 0.39, 0.30, 0.0], [0.66, 0.50, 0.36, 0.0]),
            curb_glow: false,
            barrier_low: Layer::Concrete,
            barrier_high: Layer::MetalRust,
            barrier_tint: [1.0, 0.95, 0.9, 0.0],
            barrier_height: 0.0,
            crenellations: false,
            rail_glow: None,
            torches: false,
            gantry: Layer::MetalRust,
            gantry_glow: [1.0, 0.55, 0.15],
            pad_glow: ([1.0, 0.45, 0.05, 0.0], [1.0, 0.8, 0.2, 0.0]),
            terrain: TerrainKind::Desert,
        },
    },
    TrackDef {
        id: TrackId::Grindstone,
        name: "CASTLE GRINDSTONE",
        tagline: "TORCHLIT RAMPARTS AT DUSK. JUMP THE DRAWBRIDGE.",
        points: &GRINDSTONE_POINTS,
        height: grindstone_height,
        bank: Some(no_bank),
        bank_degrees: 0.0,
        pads: &[0.04, 0.485],
        box_rows: &[0.17, 0.33, 0.86],
        style: TrackStyle {
            road: Layer::Cobblestone,
            road_tint: [0.85, 0.82, 0.78, 0.0],
            road_tex: 3.5,
            road_edge: Some(Layer::Flagstone),
            groove: 0.88,
            curbs: ([0.55, 0.08, 0.1, 0.0], [0.82, 0.74, 0.55, 0.0]),
            curb_glow: false,
            barrier_low: Layer::StoneWall,
            barrier_high: Layer::StoneWall,
            barrier_tint: [0.92, 0.9, 0.86, 0.0],
            barrier_height: 3.2,
            crenellations: true,
            rail_glow: None,
            torches: true,
            gantry: Layer::WoodPlanks,
            gantry_glow: [1.0, 0.45, 0.12],
            pad_glow: ([1.0, 0.35, 0.05, 0.0], [1.0, 0.65, 0.15, 0.0]),
            terrain: TerrainKind::Hills,
        },
    },
    TrackDef {
        id: TrackId::Skyway,
        name: "SKYWAY ZERO",
        tagline: "A NEON RIBBON IN THE NIGHT SKY. DON'T LOOK DOWN.",
        points: &SKYWAY_POINTS,
        height: skyway_height,
        bank: None,
        bank_degrees: 22.0,
        pads: &[0.05, 0.26, 0.55, 0.8],
        box_rows: &[0.15, 0.42, 0.7],
        style: TrackStyle {
            road: Layer::TechPanel,
            road_tint: [0.8, 0.85, 0.95, 0.0],
            road_tex: 7.0,
            road_edge: None,
            groove: 0.9,
            curbs: ([0.07, 0.46, 0.59, 0.0], [0.59, 0.09, 0.37, 0.0]),
            curb_glow: true,
            barrier_low: Layer::MetalRust,
            barrier_high: Layer::TechPanel,
            barrier_tint: [0.35, 0.38, 0.5, 0.0],
            barrier_height: 1.1,
            crenellations: false,
            rail_glow: Some([0.15, 0.85, 1.0]),
            torches: false,
            gantry: Layer::TechPanel,
            gantry_glow: [1.0, 0.2, 0.8],
            pad_glow: ([0.1, 0.8, 1.0, 0.0], [0.7, 0.3, 1.0, 0.0]),
            terrain: TerrainKind::Void,
        },
    },
];

/// The sun, sky and grade for each circuit.
pub fn lighting(id: TrackId) -> rivet_render::Lighting {
    let base = rivet_render::Lighting::default();
    match id {
        TrackId::Dustbowl => rivet_render::Lighting {
            sun_dir: Vec3::new(-0.63, -0.39, 0.68).normalize(),
            sun_color: Vec3::new(2.83, 2.37, 1.98),
            sky_zenith: Vec3::new(0.21, 0.085, 0.20),
            fog_color: Vec3::new(0.8, 0.54, 0.37),
            sky_horizon: Vec3::new(0.98, 0.39, 0.28),
            cloud_coverage: 0.66,
            saturation: 0.92,
            ambient_ground: Vec3::new(0.24, 0.18, 0.17),
            fog_density: 0.0018,
            ..base
        },
        // Dusk over the castle: a low burning sun, violet sky, warm haze.
        TrackId::Grindstone => rivet_render::Lighting {
            sun_dir: Vec3::new(-0.75, -0.2, 0.62).normalize(),
            sun_color: Vec3::new(3.0, 1.6, 0.85),
            sky_zenith: Vec3::new(0.12, 0.13, 0.36),
            sky_horizon: Vec3::new(0.95, 0.52, 0.34),
            ambient_ground: Vec3::new(0.22, 0.16, 0.14),
            fog_color: Vec3::new(0.62, 0.42, 0.34),
            fog_density: 0.0017,
            fog_height_falloff: 0.025,
            cloud_coverage: 0.6,
            exposure: 1.0,
            bloom_intensity: 0.1,
            vignette: 0.42,
            saturation: 1.12,
            stars: 0.15,
        },
        // Night over the city: cold moonlight, a magenta glow on the horizon, stars.
        TrackId::Skyway => rivet_render::Lighting {
            sun_dir: Vec3::new(-0.3, -0.55, 0.25).normalize(),
            sun_color: Vec3::new(0.32, 0.4, 0.7),
            sky_zenith: Vec3::new(0.004, 0.006, 0.022),
            sky_horizon: Vec3::new(0.09, 0.035, 0.14),
            ambient_ground: Vec3::new(0.06, 0.04, 0.1),
            fog_color: Vec3::new(0.06, 0.03, 0.11),
            fog_density: 0.0011,
            fog_height_falloff: 0.0,
            cloud_coverage: 0.3,
            exposure: 1.35,
            bloom_intensity: 0.2,
            vignette: 0.45,
            saturation: 1.2,
            stars: 1.0,
        },
    }
}

fn no_bank(_p: Vec2) -> f32 {
    0.0
}

/// A plane the kart must cross, in order, to count a lap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Checkpoint {
    pub center: Vec3,
    pub forward: Vec3,
    pub half_width: f32,
}

pub struct Track {
    pub canyon: Option<canyon::Canyon>,
    pub id: TrackId,
    pub name: &'static str,
    pub box_rows: Vec<f32>,
    pub terrain_kind: TerrainKind,
    /// Below this height a kart has fallen off and respawns.
    pub fall_y: f32,
    /// Where live fire burns (torch tops), for flame particles.
    pub flames: Vec<Vec3>,
    /// Road, curbs, barriers, pads, start line, gantry.
    pub road: MeshData,
    pub terrain: MeshData,
    /// Rocks and other scenery.
    pub props: MeshData,
    pub collision: TriMesh,
    /// Road center every ~2 m; the last point repeats the first.
    pub centerline: Vec<Vec3>,
    /// Road surface normal at each centerline point (banking included).
    pub road_up: Vec<Vec3>,
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
fn resampled_loop(points: &[Vec2]) -> Vec<Vec2> {
    let n = points.len();
    let mut dense = Vec::new();
    for i in 0..n {
        let p = |k: usize| points[(i + k + n - 1) % n];
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

/// Appends a quad with per-corner UVs and colors.
fn quad_colors(m: &mut MeshData, v: [Vec3; 4], uv: [[f32; 2]; 4], colors: [[f32; 4]; 4], mat: u32) {
    let n = (v[1] - v[0]).cross(v[2] - v[0]).normalize_or_zero();
    let base = m.vertices.len() as u32;
    for ((p, t), color) in v.iter().zip(uv).zip(colors) {
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

/// Architecture along one sector of each circuit. All supports sit beyond the barriers.
fn sector_structure(mesh: &mut MeshData, def: &TrackDef, frame: Mat4) {
    let block = |m: &mut MeshData, a: Vec3, b: Vec3, width, depth, layer, color| {
        let span = b - a;
        let transform = Mat4::from_rotation_translation(
            Quat::from_rotation_arc(Vec3::Y, span.normalize()),
            (a + b) * 0.5,
        );
        textured_box(
            m,
            Vec3::new(width, span.length() * 0.5, depth),
            frame * transform,
            layer,
            color,
        );
    };
    let brick = [0.76, 0.73, 0.67];
    for sign in [-1.0_f32, 1.0] {
        let p = |x, y, z| Vec3::new(sign * x, y, z);
        match def.id {
            TrackId::Dustbowl => {
                for z in [-1.5, 1.5] {
                    block(
                        mesh,
                        p(9.0, -1.0, z),
                        p(9.0, 9.0, z),
                        0.21,
                        0.21,
                        Layer::MetalRust,
                        brick,
                    );
                    block(
                        mesh,
                        p(9.0, 5.0, z),
                        p(5.5, 8.4, z),
                        0.14,
                        0.14,
                        Layer::MetalRust,
                        brick,
                    );
                    block(
                        mesh,
                        p(9.0, 8.6, z),
                        p(0.0, 8.6, z),
                        0.24,
                        0.24,
                        Layer::MetalRust,
                        brick,
                    );
                }
                for x in [1.0, 4.0, 7.0] {
                    block(
                        mesh,
                        p(x, 8.7, -1.5),
                        p(x, 8.7, 1.5),
                        0.12,
                        0.12,
                        Layer::MetalRust,
                        brick,
                    );
                }
            }
            TrackId::Grindstone => {
                block(
                    mesh,
                    p(9.6, -1.5, 0.0),
                    p(9.6, 12.4, 0.0),
                    1.6,
                    1.9,
                    Layer::StoneWall,
                    brick,
                );
                block(
                    mesh,
                    p(9.3, 5.0, 0.0),
                    p(5.0, 9.3, 0.0),
                    0.8,
                    1.6,
                    Layer::Flagstone,
                    brick,
                );
                block(
                    mesh,
                    p(9.6, 10.4, 0.0),
                    p(0.0, 10.4, 0.0),
                    1.1,
                    1.7,
                    Layer::StoneWall,
                    brick,
                );
                for x in [1.2, 3.8, 6.4, 9.0] {
                    textured_box(
                        mesh,
                        Vec3::new(0.65, 0.7, 1.6),
                        frame * Mat4::from_translation(p(x, 12.0, 0.0)),
                        Layer::StoneWall,
                        brick,
                    );
                }
            }
            TrackId::Skyway => {
                let a = p(9.0, -2.0, 0.0);
                let b = p(9.0, 5.5, 0.0);
                let c = p(4.5, 10.0, -2.0);
                block(mesh, a, b, 0.4, 0.52, Layer::TechPanel, brick);
                block(mesh, b, c, 0.4, 0.52, Layer::TechPanel, brick);
                let span = c - b;
                let transform = Mat4::from_rotation_translation(
                    Quat::from_rotation_arc(Vec3::Y, span.normalize()),
                    (b + c) * 0.5 + Vec3::Z * 0.48,
                );
                emissive_box(
                    mesh,
                    Vec3::new(0.13, span.length() * 0.48, 0.11),
                    frame * transform,
                    if sign < 0.0 {
                        [0.09, 0.81, 0.98]
                    } else {
                        [0.98, 0.12, 0.55]
                    },
                );
            }
        }
    }
}

/// A scrap-metal checkpoint built into the canyon. The cloth hangs in torn strips.
fn canyon_gate(mesh: &mut MeshData, frame: Mat4) {
    let steel = [0.67, 0.55, 0.46];
    let girder = |m: &mut MeshData, a: Vec3, b: Vec3, r: f32| {
        let axis = b - a;
        textured_box(
            m,
            Vec3::new(r, axis.length() * 0.5, r),
            frame
                * Mat4::from_rotation_translation(
                    Quat::from_rotation_arc(Vec3::Y, axis.normalize()),
                    (a + b) * 0.5,
                ),
            Layer::MetalRust,
            steel,
        );
    };
    for x in [-10.0, 10.0] {
        let mount = |y| frame * Mat4::from_translation(Vec3::new(x, y, 0.0));
        textured_box(
            mesh,
            Vec3::new(2.5, 3.5, 3.0),
            mount(2.5),
            Layer::Rock,
            [0.80, 0.57, 0.42],
        );
        textured_box(
            mesh,
            Vec3::new(1.8, 3.8, 2.15),
            mount(8.3),
            Layer::MetalRust,
            steel,
        );
        for z in [-2.25, 2.25] {
            for side in [-1.6, 1.6] {
                girder(
                    mesh,
                    Vec3::new(x + side, 4.9, z),
                    Vec3::new(x + side, 15.2, z),
                    0.18,
                );
            }
            girder(
                mesh,
                Vec3::new(x - 1.6, 10.0, z),
                Vec3::new(x + 1.6, 14.7, z),
                0.20,
            );
        }
    }
    for y in [12.7, 14.3] {
        girder(
            mesh,
            Vec3::new(-11.8, y, 0.0),
            Vec3::new(11.8, y, 0.0),
            0.16,
        );
    }
    for i in 0..12 {
        let x = -11.4 + i as f32 * 1.9;
        let (y0, y1) = if i % 2 == 0 {
            (12.7, 14.3)
        } else {
            (14.3, 12.7)
        };
        girder(
            mesh,
            Vec3::new(x, y0, 0.0),
            Vec3::new(x + 1.9, y1, 0.0),
            0.11,
        );
    }
    let cloth = material(
        Layer::CanyonStandard as u32,
        Layer::CanyonStandard as u32,
        0,
    );
    let folds = 18;
    for strip in 0..folds {
        let u = strip as f32 / folds as f32;
        let v = (strip + 1) as f32 / folds as f32;
        let corner = |t: f32, bottom: bool| {
            let hem = ((t * 91.0).sin() * 0.54 + (t * 39.0).cos() * 0.28).abs();
            let y = if bottom { 7.35 + hem } else { 12.72 };
            frame.transform_point3(Vec3::new(
                (t - 0.5) * 8.1,
                y,
                -0.24 + 0.21 * (t * 28.0).sin() + if bottom { 0.36 } else { 0.0 },
            ))
        };
        let points = [
            corner(u, false),
            corner(v, false),
            corner(v, true),
            corner(u, true),
        ];
        let uv = [[u, 0.0], [v, 0.0], [v, 1.0], [u, 1.0]];
        quad(mesh, points, uv, [1.0, 1.0, 1.0, 0.0], cloth);
        quad(
            mesh,
            [points[3], points[2], points[1], points[0]],
            [uv[3], uv[2], uv[1], uv[0]],
            [0.82, 0.78, 0.74, 0.0],
            cloth,
        );
    }
}

/// Wind-cut sandstone pillars with overhanging caprock and irregular sediment ledges.
fn sandstone_pillar(mesh: &mut MeshData, center: Vec3, radius: f32, height: f32, seed: u64) {
    let mut rng = Rng(seed);
    let rock = material(
        Layer::SandstoneBeds as u32,
        Layer::SandstoneBeds as u32,
        TRIPLANAR_B,
    );
    let mut outline = [Vec3::ZERO; 10];
    for (i, p) in outline.iter_mut().enumerate() {
        let angle = (i as f32 + 0.32 * rng.next()) * std::f32::consts::TAU / 10.0;
        *p = Vec3::new(angle.cos(), 0.0, angle.sin()) * radius * (0.82 + 0.18 * rng.next());
    }
    let levels = [
        (-0.36, 1.0),
        (0.12, 0.96),
        (0.22, 0.81),
        (0.47, 0.78),
        (0.49, 0.68),
        (0.75, 0.61),
        (0.80, 0.70),
        (0.97, 0.64),
        (1.0, 0.53),
    ];
    let ring = |i: usize, (y, r): (f32, f32)| {
        center
            + outline[i] * r
            + Vec3::new(
                y * radius * 0.11,
                height * y + (i as f32 * 2.3).sin() * radius * 0.12,
                0.0,
            )
    };
    for (band, w) in levels.windows(2).enumerate() {
        let tint = if band == 2 || band == 6 {
            [0.91, 0.83, 0.78, 1.0]
        } else {
            [1.0, 0.92, 0.85, 1.0]
        };
        for i in 0..outline.len() {
            let j = (i + 1) % outline.len();
            quad(
                mesh,
                [ring(i, w[0]), ring(i, w[1]), ring(j, w[1]), ring(j, w[0])],
                [[0.0; 2]; 4],
                tint,
                rock,
            );
        }
    }
    let top = levels[levels.len() - 1];
    for i in 1..outline.len() - 1 {
        let vertices = [ring(0, top), ring(i + 1, top), ring(i, top)];
        let normal = (vertices[1] - vertices[0])
            .cross(vertices[2] - vertices[0])
            .normalize();
        for p in vertices {
            mesh.indices.push(mesh.vertices.len() as u32);
            mesh.vertices.push(Vertex {
                pos: p.to_array(),
                normal: normal.to_array(),
                uv: [0.0; 2],
                color: [0.91, 0.73, 0.62, 1.0],
                material: rock,
            });
        }
    }
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

/// The first circuit (kept for tests and tools that predate track selection).
pub fn build_graybox_loop() -> Track {
    build_track(&TRACKS[0])
}

/// Plan view sampled from the same spline as the road.
pub fn route_preview(def: &TrackDef) -> Vec<Vec3> {
    resampled_loop(def.points)
        .into_iter()
        .map(|p| Vec3::new(p.x, 0.0, p.y))
        .collect()
}

pub fn build_track(def: &TrackDef) -> Track {
    let st = def.style;
    let flat = resampled_loop(def.points);
    let n = flat.len();
    let bank_max = def.bank_degrees.to_radians();

    // Signed bank per sample: explicit bank zones, or bank by curvature (Skyway's sweeping
    // turns). A Catmull-Rom spline's curvature jumps at its control points, so curvature banking
    // is smoothed along the road; unsmoothed, the bank (and the lift that keeps the inner edge
    // up) steps by meters there, and karts slam into the step.
    let bend_at = |i: usize| {
        let (prev, p, next) = (flat[(i + n - 1) % n], flat[i], flat[(i + 1) % n]);
        (next - p).perp_dot(p - prev) / ((next - p).length() * (p - prev).length()).max(1e-6)
    };
    let raw_bank: Vec<f32> = (0..n)
        .map(|i| {
            let bend = bend_at(i);
            let weight = match def.bank {
                Some(f) => f(flat[i]),
                None => (bend.abs() * 30.0).clamp(0.0, 1.0),
            };
            weight * bank_max * bend.signum()
        })
        .collect();
    let banks: Vec<f32> = match def.bank {
        Some(_) => raw_bank,
        None => (0..n)
            .map(|i| {
                // Triangular window over ±BANK_SMOOTHING samples.
                let r = BANK_SMOOTHING as isize;
                let (mut sum, mut weights) = (0.0, 0.0);
                for d in -r..=r {
                    let w = (r + 1 - d.abs()) as f32;
                    sum += w * raw_bank[(i as isize + d).rem_euclid(n as isize) as usize];
                    weights += w;
                }
                sum / weights
            })
            .collect(),
    };

    // Frames: position, forward, right, up. Banks pivot on the inner edge so nothing dips below 0.
    let mut frames = Vec::with_capacity(n);
    for i in 0..n {
        let p = flat[i];
        let prev = flat[(i + n - 1) % n];
        let next = flat[(i + 1) % n];
        let h = |q: Vec2, k: usize| (def.height)(q, k as f32 / n as f32);
        let fwd = (Vec3::new(next.x, h(next, (i + 1) % n), next.y)
            - Vec3::new(prev.x, h(prev, (i + n - 1) % n), prev.y))
        .normalize();
        let right_flat = fwd.cross(Vec3::Y).normalize();
        let bank = banks[i];
        let rot = Quat::from_axis_angle(fwd, -bank);
        let right = rot * right_flat;
        let up = right.cross(fwd).normalize();
        let lift = ROAD_HALF_WIDTH * bank.abs().sin();
        let center = Vec3::new(p.x, h(p, i) + lift, p.y);
        frames.push((center, fwd, right, up));
    }
    let edge = |i: usize, side: f32, inset: f32| {
        let (c, _, r, _) = frames[i % n];
        c + r * side * (ROAD_HALF_WIDTH - inset)
    };

    let mut road = MeshData::default();
    let mut col = Collision::default();
    let mut flames = Vec::new();
    let road_mat = material(st.road as u32, st.road_edge.unwrap_or(st.road) as u32, 0);
    let road_tex = st.road_tex;
    let u_total = 2.0 * ROAD_HALF_WIDTH / road_tex;
    // Lateral strips across the road (-1 = left edge, 1 = right edge): the edge material drifts
    // in at the sides and a worn groove darkens the middle.
    let strips: &[f32] = if def.id == TrackId::Dustbowl {
        &[-1.0, -0.68, -0.30, -0.15, 0.0, 0.15, 0.30, 0.68, 1.0]
    } else {
        &[-1.0, -0.62, -0.22, 0.22, 0.62, 1.0]
    };
    let corner = |side: f32, sample: usize| -> [f32; 4] {
        let edge_mix = match st.road_edge {
            Some(_) => ((side.abs() - 0.45) / 0.55).clamp(0.0, 1.0).powf(1.4) * 0.9,
            None => 0.0,
        };
        let p = frames[sample % n].0;
        let drift = (p.x * 0.065 + p.z * 0.042).sin();
        let groove = (1.0 - (side - 0.20 * drift).abs()).clamp(0.0, 1.0);
        let mut dark = 1.0 - groove * (1.0 - st.groove);
        if def.id == TrackId::Dustbowl {
            // Twin wheel ruts fade where wind carries fresh sand across the racing line.
            let rut = (1.0 - (side.abs() - 0.15).abs() / 0.15).max(0.0);
            dark *= 1.0 - rut * (0.16 + 0.07 * drift);
        }
        let edge_mix = edge_mix * (0.75 + 0.25 * (p.x * 0.13 - p.z * 0.09 + side).sin());
        let t = st.road_tint;
        [t[0] * dark, t[1] * dark, t[2] * dark, edge_mix]
    };

    // Road surface first, strip by strip (each strip contiguous, so its UVs can be checked).
    for strip in strips.windows(2) {
        let (a, b) = (strip[0], strip[1]);
        let (ua, ub) = ((a + 1.0) * 0.5 * u_total, (b + 1.0) * 0.5 * u_total);
        for i in 0..n {
            let (v0, v1) = (
                i as f32 * SAMPLE_SPACING / road_tex,
                (i + 1) as f32 * SAMPLE_SPACING / road_tex,
            );
            let v = [
                edge(i, a, 0.0),
                edge(i, b, 0.0),
                edge(i + 1, b, 0.0),
                edge(i + 1, a, 0.0),
            ];
            quad_colors(
                &mut road,
                v,
                [[ua, v0], [ub, v0], [ub, v1], [ua, v1]],
                [
                    corner(a, i),
                    corner(b, i),
                    corner(b, i + 1),
                    corner(a, i + 1),
                ],
                road_mat,
            );
        }
    }
    for i in 0..n {
        let v = [
            edge(i, -1.0, 0.0),
            edge(i, 1.0, 0.0),
            edge(i + 1, 1.0, 0.0),
            edge(i + 1, -1.0, 0.0),
        ];
        col.quad(v, Surface::Road);
    }

    // Curbs, barriers.
    let concrete = material(st.barrier_low as u32, st.barrier_low as u32, 0);
    let metal = material(st.barrier_high as u32, st.barrier_high as u32, 0);
    let curb_mat = if st.curb_glow {
        material(0, 0, UNTEXTURED | EMISSIVE)
    } else {
        material(Layer::Concrete as u32, Layer::Concrete as u32, 0)
    };
    let wall_h = st.barrier_height;

    for i in 0..n {
        let (s0, s1) = (i as f32 * SAMPLE_SPACING, (i + 1) as f32 * SAMPLE_SPACING);
        let (_, _, _, u0) = frames[i];
        let (_, _, _, u1) = frames[(i + 1) % n];
        let curb = if i % 2 == 0 { st.curbs.0 } else { st.curbs.1 };
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
            if wall_h > 0.0 {
                let points = if side < 0.0 {
                    [o0, i0, i1, o1]
                } else {
                    [i0, o0, o1, i1]
                };
                quad(&mut road, points, uv, curb, curb_mat);
            }

            // Paint paired diagonal approach marks along the outside of upcoming bends.
            let (_, _, lateral, _) = frames[i];
            let turn = frames[(i + 8) % n].1.dot(lateral);
            if turn * side < -0.20 && i % 6 < 2 {
                let point = |sample, inset, up: Vec3| edge(sample, side, inset) + up * 0.025;
                let mut stripe = [
                    point(i, CURB_WIDTH + 0.25, u0),
                    point(i, CURB_WIDTH + 0.55, u0),
                    point(i + 1, CURB_WIDTH + 1.55, u1),
                    point(i + 1, CURB_WIDTH + 1.25, u1),
                ];
                if side > 0.0 {
                    stripe.reverse();
                }
                quad(&mut road, stripe, uv, st.curbs.1, curb_mat);
            }

            if wall_h == 0.0 {
                // Feather the bank and jump into the drivable valley floor.
                let shoulder = |sample| {
                    let mut p = edge(sample, side, -SAND_SHOULDER_WIDTH);
                    p.y = terrain::FLAT_Y - 0.05;
                    p
                };
                let mut v = [
                    edge(i, side, 0.0),
                    shoulder(i),
                    shoulder(i + 1),
                    edge(i + 1, side, 0.0),
                ];
                if side < 0.0 {
                    v.reverse();
                }
                let uv = v.map(|p| [p.x / road_tex, p.z / road_tex]);
                quad(
                    &mut road,
                    v,
                    uv,
                    [0.88, 0.73, 0.55, 0.0],
                    material(Layer::Sand as u32, Layer::Sand as u32, 0),
                );
                col.quad(v, Surface::Offroad);
                continue;
            }

            // Barrier: concrete base, rusty metal above; faces the road.
            let (e0, e1) = (edge(i, side, 0.0), edge(i + 1, side, 0.0));
            // Lower the parapet along the mill field and the great hall approach.
            let parapet = |sample: usize| {
                if def.id != TrackId::Grindstone {
                    return wall_h;
                }
                let landmark = if side < 0.0 { 0.075 } else { 0.43 };
                let distance = (sample as f32 / n as f32 - landmark).abs();
                1.0 + (wall_h - 1.0) * ((distance - 0.035) / 0.025).clamp(0.0, 1.0)
            };
            let (h0, h1) = (parapet(i), parapet((i + 1) % n));
            let ratio = WALL_BASE / WALL_HEIGHT;
            let (b0, b1) = (e0 + u0 * (h0 * ratio), e1 + u1 * (h1 * ratio));
            let (t0, t1) = (e0 + u0 * h0, e1 + u1 * h1);
            let (ua, ub) = (s0 / WALL_TEX, s1 / WALL_TEX);
            let hb = wall_h * ratio / WALL_TEX;
            let ht = wall_h / WALL_TEX;
            if side < 0.0 {
                quad(
                    &mut road,
                    [e0, e1, b1, b0],
                    [[ua, 0.0], [ub, 0.0], [ub, hb], [ua, hb]],
                    st.barrier_tint,
                    concrete,
                );
                quad(
                    &mut road,
                    [b0, b1, t1, t0],
                    [[ua, hb], [ub, hb], [ub, ht], [ua, ht]],
                    st.barrier_tint,
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
                    st.barrier_tint,
                    concrete,
                );
                quad(
                    &mut road,
                    [b0, t0, t1, b1],
                    [[ua, hb], [ua, ht], [ub, ht], [ub, hb]],
                    st.barrier_tint,
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
                quad(&mut road, [t0, t1, c1, c0], cap_uv, st.barrier_tint, metal);
            } else {
                quad(&mut road, [t0, c0, c1, t1], cap_uv, st.barrier_tint, metal);
            }
            // Castle merlons: a stone block every other segment on the wall top.
            if st.crenellations && h0 == wall_h && i % 2 == 0 {
                let mid = (t0 + t1) * 0.5 + (r0 + r1) * 0.5 * side * 0.2 + (u0 + u1) * 0.25 * 0.55;
                let f = (t1 - t0).normalize_or(Vec3::Z);
                let rot = Quat::from_rotation_y(f.x.atan2(f.z));
                textured_box(
                    &mut road,
                    Vec3::new(0.45, 0.55, 0.55),
                    Mat4::from_rotation_translation(rot, mid),
                    st.barrier_high,
                    [0.9, 0.88, 0.84],
                );
            }
            // Torches: a glowing flame bowl on the wall every 30 m.
            if st.torches && i % 15 == 7 {
                let at = t0 + r0 * side * 0.1 + u0 * 0.35;
                // Glowing embers in the bowl; the flame itself is particles.
                emissive_box(
                    &mut road,
                    Vec3::new(0.14, 0.1, 0.14),
                    Mat4::from_translation(at),
                    [0.9, 0.32, 0.06],
                );
                flames.push(at + u0 * 0.15);
                textured_box(
                    &mut road,
                    Vec3::new(0.12, 0.35, 0.12),
                    Mat4::from_translation(at - u0 * 0.55),
                    Layer::MetalRust,
                    [0.5, 0.5, 0.5],
                );
            }
            // Neon rail along the barrier top.
            if let Some(glow) = st.rail_glow {
                let (g0, g1) = (t0 + u0 * 0.02, t1 + u1 * 0.02);
                let (h0, h1) = (g0 + r0 * side * 0.18, g1 + r1 * side * 0.18);
                let glow4 = [glow[0], glow[1], glow[2], 0.0];
                if side < 0.0 {
                    quad(
                        &mut road,
                        [g0, g1, h1, h0],
                        [[0.0; 2]; 4],
                        glow4,
                        material(0, 0, UNTEXTURED | EMISSIVE),
                    );
                } else {
                    quad(
                        &mut road,
                        [g0, h0, h1, g1],
                        [[0.0; 2]; 4],
                        glow4,
                        material(0, 0, UNTEXTURED | EMISSIVE),
                    );
                }
            }
        }
    }

    // A run of silhouettes marks the entry to a distinct sector at racing speed.
    let (sector, spacing, repeats) = match def.id {
        TrackId::Dustbowl => (0.16, 9, 3),
        TrackId::Grindstone => (0.39, 15, 2),
        TrackId::Skyway => (0.09, 8, 3),
    };
    for k in 0..repeats {
        let (center, fwd, _, _) = frames[((n as f32 * sector) as usize + k * spacing) % n];
        let frame =
            Mat4::from_rotation_translation(Quat::from_rotation_y(fwd.x.atan2(fwd.z)), center);
        sector_structure(&mut road, def, frame);
        if def.id == TrackId::Grindstone {
            castle::gate_standards(&mut road, frame);
        }
    }

    if def.id == TrackId::Grindstone {
        for (i, at) in [0.027, 0.055, 0.085, 0.115].into_iter().enumerate() {
            let (center, fwd, _, _) = frames[(n as f32 * at) as usize];
            let frame =
                Mat4::from_rotation_translation(Quat::from_rotation_y(fwd.x.atan2(fwd.z)), center);
            if i == 3 {
                castle::sun_gate(&mut road, frame);
            } else {
                castle::pennant_line(&mut road, frame, i);
            }
        }
    }

    if def.id == TrackId::Dustbowl {
        let at = 0.83;
        let (c, f, right, _) = frames[(n as f32 * at) as usize];
        flames.push(c + right * 8.4 + Vec3::Y * 5.9);
        flames.push(c - right * 8.4 + Vec3::Y * 5.9);
        canyon_gate(
            &mut road,
            Mat4::from_rotation_translation(Quat::from_rotation_y(f.x.atan2(f.z)), c),
        );
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
                st.gantry,
                [1.0, 1.0, 1.0],
            );
        }
        let beam = c + Vec3::Y * 7.2;
        textured_box(
            &mut road,
            Vec3::new(ROAD_HALF_WIDTH + 1.8, 0.6, 0.5),
            Mat4::from_rotation_translation(rot, beam),
            st.gantry,
            [0.9, 0.85, 0.8],
        );
        emissive_box(
            &mut road,
            Vec3::new(ROAD_HALF_WIDTH + 1.2, 0.12, 0.08),
            Mat4::from_rotation_translation(rot, beam - Vec3::Y * 0.65 - f * 0.45),
            st.gantry_glow,
        );
    }

    // Boost pads on the two long straights: glowing chevron strips.
    let mut boost_pads = Vec::new();
    for at in def.pads.iter().map(|f| (f * n as f32) as usize) {
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
                st.pad_glow.0
            } else {
                st.pad_glow.1
            };
            quad(
                &mut road,
                v,
                [[0.0, 0.0], [1.0, 0.0], [1.0, 0.5], [0.0, 0.5]],
                [0.37, 0.40, 0.44, 0.0],
                material(Layer::TechPanel as u32, Layer::TechPanel as u32, 0),
            );
            let point = |side: f32, along: f32| {
                let near = c0 + r0 * side + u0 * (PAD_LIFT + 0.018);
                let far = c1 + r1 * side + u1 * (PAD_LIFT + 0.018);
                near.lerp(far, along)
            };
            for sign in [-1.0, 1.0] {
                let mut marks = [
                    point(0.0, 0.67),
                    point(sign * 2.18, 0.12),
                    point(sign * 2.18, 0.32),
                    point(0.0, 0.87),
                ];
                if sign < 0.0 {
                    marks.reverse();
                }
                quad(
                    &mut road,
                    marks,
                    [[0.0; 2]; 4],
                    glow,
                    material(0, 0, UNTEXTURED | EMISSIVE),
                );
            }
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
    let mut road_up: Vec<Vec3> = frames.iter().map(|f| f.3).collect();
    road_up.push(road_up[0]);
    let (terrain_mesh, tpos, ttris) = terrain::build_kind(st.terrain, &centerline, lo, hi);
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
    let rock_count = match st.terrain {
        TerrainKind::Desert => 110,
        TerrainKind::Hills => 60,
        TerrainKind::Void => 0,
    };
    while rocks.len() < rock_count && tries < 8000 {
        tries += 1;
        let x = lo.x + 100.0 + rng.next() * (hi.x - lo.x - 200.0);
        let z = lo.y + 100.0 + rng.next() * (hi.y - lo.y - 200.0);
        let d = terrain::distance_to(&centerline, x, z);
        if !(ROAD_HALF_WIDTH + 10.0..240.0).contains(&d) {
            continue;
        }
        // Boulders: bigger farther out, a few big ones near the road for parallax.
        let scale = 2.0 + rng.next() * rng.next() * 6.0 + (d / 240.0) * 5.0;
        let y = terrain::height_for(st.terrain, &centerline, x, z) - 0.3 * scale;
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

    let canyon =
        (def.id == TrackId::Dustbowl).then(|| canyon::Canyon::build(&mut props, &centerline));
    if def.id == TrackId::Dustbowl {
        for (i, &(c, _, side, _)) in frames.iter().enumerate().step_by(39) {
            let sign = if (i / 39) % 2 == 0 { -1.0 } else { 1.0 };
            let radius = 4.0 + rng.next() * 6.0;
            let p = c + side * sign * (ROAD_HALF_WIDTH + radius + 36.0 + 41.0 * rng.next());
            if terrain::distance_to(&centerline, p.x, p.z) < ROAD_HALF_WIDTH + radius + 3.0 {
                continue;
            }
            let floor = terrain::height_at(&centerline, p.x, p.z);
            let height = radius * (1.4 + 2.1 * rng.next());
            sandstone_pillar(
                &mut props,
                Vec3::new(p.x, floor, p.z),
                radius,
                height,
                i as u64 + u64::from(sign > 0.0) + 11,
            );
        }
    }

    // Fan-shaped clumps break up the flat shoulder between the barriers and larger scenery.
    if st.terrain != TerrainKind::Void {
        for &(c, _, right, _) in frames.iter().step_by(5) {
            let sign = if rng.next() < 0.5 { -1.0 } else { 1.0 };
            let p = c + right * sign * (ROAD_HALF_WIDTH + 3.0 + rng.next() * 29.0);
            if terrain::distance_to(&centerline, p.x, p.z) < ROAD_HALF_WIDTH + 2.0 {
                continue;
            }
            let root = Vec3::new(
                p.x,
                terrain::height_for(st.terrain, &centerline, p.x, p.z) - 0.18,
                p.z,
            );
            let tint = if st.terrain == TerrainKind::Desert {
                [0.50, 0.47, 0.28, 0.0]
            } else {
                [0.23, 0.39, 0.19, 0.0]
            };
            let scale = 0.65 + rng.next() * 1.2;
            for blade in 0..9 {
                let angle = blade as f32 * 2.4 + rng.next() * 0.3;
                let outward = Vec3::new(angle.cos(), 0.0, angle.sin());
                let across = outward.cross(Vec3::Y) * 0.10 * scale;
                let base = root + outward * scale * 0.16;
                let tip = base
                    + (outward * (0.22 + rng.next() * 0.55) + Vec3::Y * (0.54 + rng.next()))
                        * scale;
                let pts = [
                    base - across,
                    base + across,
                    tip + across * 0.055,
                    tip - across * 0.055,
                ];
                quad(
                    &mut props,
                    pts,
                    [[0.0; 2]; 4],
                    tint,
                    material(0, 0, UNTEXTURED),
                );
                quad(
                    &mut props,
                    [pts[3], pts[2], pts[1], pts[0]],
                    [[0.0; 2]; 4],
                    tint,
                    material(0, 0, UNTEXTURED),
                );
            }
        }
    }

    // The skyway rides on slender pylons down into the haze.
    if st.terrain == TerrainKind::Void {
        for i in (0..n).step_by(30) {
            let (c, f, _, _) = frames[i];
            let rot = Quat::from_rotation_y(f.x.atan2(f.z));
            let depth = c.y + 120.0;
            textured_box(
                &mut road,
                Vec3::new(1.1, depth * 0.5, 1.1),
                Mat4::from_rotation_translation(rot, c - Vec3::Y * (depth * 0.5 + 0.6)),
                Layer::TechPanel,
                [0.3, 0.32, 0.42],
            );
            emissive_box(
                &mut road,
                Vec3::new(1.15, 0.12, 1.15),
                Mat4::from_translation(c - Vec3::Y * 2.0),
                [0.15, 0.85, 1.0],
            );
        }
        // The city: lit skyscrapers far below, clear of the road's footprint.
        let low = centerline.iter().map(|c| c.y).fold(f32::MAX, f32::min);
        let mut rng = Rng(0x000c_17e5);
        let (mut towers, mut tries) = (0, 0);
        while towers < 70 && tries < 4000 {
            tries += 1;
            let x = lo.x - 200.0 + rng.next() * (hi.x - lo.x + 400.0);
            let z = lo.y - 200.0 + rng.next() * (hi.y - lo.y + 400.0);
            if terrain::distance_to(&centerline, x, z) < 22.0 {
                continue;
            }
            let (hx, hz) = (8.0 + rng.next() * 10.0, 8.0 + rng.next() * 10.0);
            let top = low - 18.0 - rng.next() * 55.0;
            let bottom = low - 220.0;
            let h = (top - bottom) * 0.5;
            let rot = Quat::from_rotation_y((rng.next() * 4.0).floor() * 0.4);
            let warmth = rng.next();
            let tint = [0.3 + 0.12 * warmth, 0.31, 0.4 - 0.1 * warmth];
            let mut b = box_mesh(Vec3::new(hx, h, hz), [tint[0], tint[1], tint[2], 1.0]);
            let xf = Mat4::from_rotation_translation(rot, Vec3::new(x, bottom + h, z));
            for v in &mut b.vertices {
                v.pos = xf.transform_point3(Vec3::from_array(v.pos)).to_array();
                v.normal = xf
                    .transform_vector3(Vec3::from_array(v.normal))
                    .normalize()
                    .to_array();
                v.material = material(
                    Layer::CityFacade as u32,
                    Layer::CityFacade as u32,
                    TRIPLANAR_B | EMISSIVE,
                );
            }
            road.append(&b);
            // A red aircraft beacon on every third roof.
            if towers % 3 == 0 {
                emissive_box(
                    &mut road,
                    Vec3::splat(0.6),
                    Mat4::from_translation(Vec3::new(x, top + 0.6, z)),
                    [1.0, 0.1, 0.15],
                );
            }
            towers += 1;
        }
    }

    let collision = TriMesh::with_surfaces(col.positions, col.indices, col.surfaces, 8.0);

    let every = (CHECKPOINT_SPACING / SAMPLE_SPACING).round() as usize;
    let checkpoints = (0..n)
        .step_by(every)
        .map(|i| Checkpoint {
            center: frames[i].0,
            forward: frames[i].1,
            half_width: ROAD_HALF_WIDTH
                + if wall_h == 0.0 {
                    SAND_SHOULDER_WIDTH
                } else {
                    0.5
                },
        })
        .collect();

    let (sp, sf, _, _) = frames[n - 2];
    let spawn = (sp, sf.x.atan2(sf.z));

    Track {
        canyon,
        id: def.id,
        name: def.name,
        box_rows: def.box_rows.to_vec(),
        terrain_kind: st.terrain,
        fall_y: centerline.iter().map(|c| c.y).fold(f32::MAX, f32::min) - 12.0,
        flames,
        road,
        terrain: terrain_mesh,
        props,
        collision,
        centerline,
        road_up,
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
        // The pinned control points plus the jump and banking come to ~1.3 km of 3D road.
        assert!((1150.0..=1400.0).contains(&len), "length {len}");
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
    fn castle_walls_present() {
        let t = build_track(&TRACKS[1]);
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
        let t = build_track(&TRACKS[1]);
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
    fn dustbowl_has_open_drivable_shoulders() {
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
        for side in [-right, right] {
            assert!(
                t.collision
                    .raycast(c + Vec3::Y * 0.5, side, ROAD_HALF_WIDTH + 2.0)
                    .is_none()
            );
            assert_eq!(down(c + side * (ROAD_HALF_WIDTH + 2.0)), Surface::Offroad);
        }
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
        let t = build_track(&TRACKS[2]);
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
    fn roads_have_no_steps() {
        // Ramps and crests climb under 0.8 m per 2 m sample; a bank that flips at a spline
        // control point used to lift the road by meters in one sample.
        for def in TRACKS.iter() {
            let t = build_track(def);
            for (i, w) in t.centerline.windows(2).enumerate() {
                let dy = (w[1].y - w[0].y).abs();
                assert!(dy < 0.8, "{}: {dy} m step at sample {i}", def.name);
            }
        }
    }

    #[test]
    fn tracks_keep_their_stretches_apart() {
        // Stretches of road more than 60 m apart along the lap stay 40 m apart in the world, so
        // barriers, terrain and scenery between them never overlap.
        for def in TRACKS.iter() {
            let t = build_track(def);
            let c = &t.centerline;
            let n = c.len() - 1;
            for i in (0..n).step_by(2) {
                for j in (i + 30..n).step_by(2) {
                    if n - (j - i) < 30 {
                        continue;
                    }
                    let d = Vec2::new(c[i].x - c[j].x, c[i].z - c[j].z).length();
                    assert!(
                        d > 40.0,
                        "{}: samples {i} and {j} only {d} m apart",
                        def.name
                    );
                }
            }
        }
    }

    #[test]
    fn every_track_builds_with_a_drivable_road() {
        for def in TRACKS.iter() {
            let t = build_track(def);
            assert_eq!(t.id, def.id);
            let len: f32 = t.centerline.windows(2).map(|w| w[0].distance(w[1])).sum();
            assert!(
                (900.0..=2100.0).contains(&len),
                "{}: length {len}",
                def.name
            );
            for (i, c) in t.centerline.iter().enumerate().step_by(7) {
                let hit = t
                    .collision
                    .raycast(*c + Vec3::Y * 3.0, Vec3::NEG_Y, 10.0)
                    .unwrap_or_else(|| panic!("{}: no road under point {i}", def.name));
                assert!(hit.normal.y > 0.75, "{}: road too steep at {i}", def.name);
            }
            assert!(
                t.checkpoints.len() >= 15,
                "{}: {} checkpoints",
                def.name,
                t.checkpoints.len()
            );
            assert!(!t.box_rows.is_empty());
        }
    }

    #[test]
    fn castle_torches_and_canyon_gate_flares_burn() {
        let c = build_track(&TRACKS[1]);
        assert!(c.flames.len() >= 20, "{} torches", c.flames.len());
        assert_eq!(build_track(&TRACKS[0]).flames.len(), 2);
        assert!(build_track(&TRACKS[2]).flames.is_empty());
    }

    #[test]
    fn skyway_has_a_lit_city_far_below() {
        let t = build_track(&TRACKS[2]);
        let low = t.centerline.iter().map(|c| c.y).fold(f32::MAX, f32::min);
        let city = material(
            Layer::CityFacade as u32,
            Layer::CityFacade as u32,
            TRIPLANAR_B | EMISSIVE,
        );
        let towers: Vec<_> = t
            .road
            .vertices
            .iter()
            .filter(|v| v.material == city)
            .collect();
        assert!(
            towers.len() >= 24 * 40,
            "at least 40 towers ({} verts)",
            towers.len()
        );
        assert!(
            towers.iter().all(|v| v.pos[1] < low - 12.0),
            "all below the deck"
        );
        let d = build_track(&TRACKS[0]);
        assert!(!d.road.vertices.iter().any(|v| v.material == city));
    }

    #[test]
    fn each_track_has_its_own_light() {
        let (d, c, s) = (
            lighting(TrackId::Dustbowl),
            lighting(TrackId::Grindstone),
            lighting(TrackId::Skyway),
        );
        assert!(c.sun_dir.y > d.sun_dir.y, "dusk sun is lower");
        assert!(
            s.sun_color.length() < d.sun_color.length() * 0.3,
            "night is dark"
        );
        assert!(s.stars > 0.5 && d.stars == 0.0);
    }

    #[test]
    fn tracks_have_distinct_character() {
        let ids: Vec<_> = TRACKS.iter().map(|d| d.id).collect();
        assert_eq!(
            ids,
            vec![TrackId::Dustbowl, TrackId::Grindstone, TrackId::Skyway]
        );
        assert!(
            TRACKS
                .iter()
                .all(|d| !d.name.is_empty() && !d.tagline.is_empty())
        );
        let sky = build_track(&TRACKS[2]);
        assert!(
            sky.terrain.indices.is_empty() || sky.centerline.iter().all(|c| c.y > 15.0),
            "the skyway floats"
        );
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
