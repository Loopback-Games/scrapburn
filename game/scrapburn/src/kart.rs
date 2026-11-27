//! The player's kart model: a prepared Tripo GLB, normalized to kart size and orientation.

use std::path::Path;

use glam::{Mat4, Vec3};
use rivet_assets::{GlbError, GlbMesh, Rgba8Image, load_glb};
use rivet_render::{MeshData, Vertex, material};

use crate::materials::KART_LAYER;

/// Bumper-to-bumper length (m).
pub const KART_LENGTH: f32 = 2.8;
/// Distance from the physics body center down to the tire contact patch (= ride height).
pub const GROUND_OFFSET: f32 = 0.45;
/// Rotation (radians about +Y) that turns the model's long axis/front toward +Z.
pub const MODEL_YAW: f32 = -std::f32::consts::FRAC_PI_2;

pub fn bounds(v: &[Vertex]) -> ([f32; 3], [f32; 3]) {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in v {
        for i in 0..3 {
            lo[i] = lo[i].min(p.pos[i]);
            hi[i] = hi[i].max(p.pos[i]);
        }
    }
    (lo, hi)
}

/// Rotates by `yaw` after aligning the longest horizontal axis with X, scales to `length`,
/// centers on X/Z and puts the lowest point at `-GROUND_OFFSET`.
pub fn normalize(g: &GlbMesh, length: f32, extra_yaw: f32) -> MeshData {
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in &g.positions {
        lo = lo.min(Vec3::from_array(*p));
        hi = hi.max(Vec3::from_array(*p));
    }
    let size = hi - lo;
    // Align the long horizontal axis with +X first, then apply the model yaw (+X -> +Z).
    let align = if size.z > size.x {
        Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2)
    } else {
        Mat4::IDENTITY
    };
    let rot = Mat4::from_rotation_y(MODEL_YAW + extra_yaw) * align;
    let scale = length / size.x.max(size.z).max(1e-6);
    let center = (lo + hi) * 0.5;
    let xf = rot * Mat4::from_scale(Vec3::splat(scale)) * Mat4::from_translation(-center);
    let mat = material(KART_LAYER, KART_LAYER, 0);
    let mut m = MeshData::default();
    for i in 0..g.positions.len() {
        m.vertices.push(Vertex {
            pos: xf
                .transform_point3(Vec3::from_array(g.positions[i]))
                .to_array(),
            normal: rot
                .transform_vector3(Vec3::from_array(g.normals[i]))
                .normalize_or_zero()
                .to_array(),
            uv: g.uvs[i],
            color: [1.0, 1.0, 1.0, 0.0],
            material: mat,
        });
    }
    let (lo, _) = bounds(&m.vertices);
    for v in &mut m.vertices {
        v.pos[1] += -GROUND_OFFSET - lo[1];
    }
    m.indices = g.indices.clone();
    m
}

/// A kart design: model file, the yaw that points its nose down +Z, and its tire size.
#[derive(Debug, Clone, Copy)]
pub struct KartModel {
    pub name: &'static str,
    pub yaw: f32,
    /// Wheel radius relative to the kart's length.
    pub wheel_ratio: f32,
}

/// The kart designs: the scrapper (all-rounder), the war-rig (heavy) and the speedster (light).
pub const KARTS: [KartModel; 3] = [
    KartModel {
        name: "scrapper",
        yaw: 0.0,
        wheel_ratio: WHEEL_RADIUS_RATIO,
    },
    KartModel {
        name: "warrig",
        yaw: 0.0,
        wheel_ratio: 0.19,
    },
    KartModel {
        name: "speedster",
        yaw: 0.0,
        wheel_ratio: 0.13,
    },
];

/// Garage finishes use the same tint palette as the race renderer.
pub const FINISHES: [&str; crate::race::TINTS.len()] = [
    "FACTORY", "GLACIER", "OXIDE", "MOSS", "BRASS", "VIOLET", "GRAPHITE", "EMBER",
];

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Loadout {
    pub design: usize,
    pub finish: usize,
}

impl Loadout {
    pub fn read(dir: &Path) -> Self {
        let Ok(data) = std::fs::read_to_string(dir.join("garage")) else {
            return Self::default();
        };
        let mut fields = data.split_whitespace();
        let design = fields
            .next()
            .and_then(|v| KARTS.iter().position(|k| k.name == v));
        let finish = fields
            .next()
            .and_then(|v| FINISHES.iter().position(|&name| name == v));
        Self {
            design: design.unwrap_or_default(),
            finish: finish.unwrap_or_default(),
        }
    }

    pub fn save(self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(format!("garage.{}.tmp", std::process::id()));
        std::fs::write(
            &tmp,
            format!("{} {}\n", KARTS[self.design].name, FINISHES[self.finish]),
        )?;
        std::fs::rename(tmp, dir.join("garage"))
    }
}

/// Which design a racer drives: heavy rivals get the war-rig, light ones the speedster; the
/// player and the middle of the field the scrapper.
pub fn model_for(mass_ratio: f32, is_player: bool) -> usize {
    if is_player {
        0
    } else if mass_ratio > 1.07 {
        1
    } else if mass_ratio < 0.93 {
        2
    } else {
        0
    }
}

/// Loads `models/karts/<name>.glb`: the mesh (textured from `layer`) and its base color.
pub fn load(
    assets: &Path,
    kart: &KartModel,
    layer: u32,
) -> Result<(MeshData, Option<Rgba8Image>), GlbError> {
    let g = load_glb(
        &assets
            .join("models")
            .join("karts")
            .join(format!("{}.glb", kart.name)),
    )?;
    let tex = g.base_color.clone();
    let mut m = normalize(&g, KART_LENGTH, kart.yaw);
    let mat = material(layer, layer, 0);
    for v in &mut m.vertices {
        v.material = mat;
    }
    Ok((m, tex))
}

/// Tubular steel fitted to the chassis in local space. UVs stay fixed as the car moves.
fn chassis_tube(mesh: &mut MeshData, a: Vec3, b: Vec3, radius: f32, tint: [f32; 4]) {
    let axis = (b - a).normalize();
    let across = axis.any_orthonormal_vector();
    let up = axis.cross(across);
    let layer = crate::materials::Layer::MetalRust as u32;
    for seg in 0..10 {
        let base = mesh.vertices.len() as u32;
        for (end, k) in [(false, seg), (false, seg + 1), (true, seg + 1), (true, seg)] {
            let t = k as f32 / 10.0;
            let (sin, cos) = (t * std::f32::consts::TAU).sin_cos();
            let normal = across * cos + up * sin;
            let pos = if end { b } else { a };
            mesh.vertices.push(Vertex {
                pos: (pos + normal * radius).to_array(),
                normal: normal.to_array(),
                uv: [t, if end { a.distance(b) } else { 0.0 }],
                color: tint,
                material: material(layer, layer, 0),
            });
        }
        mesh.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

pub fn fit_buggy_frame(body: &mut MeshData, design: usize) {
    let (lo, hi) = bounds(&body.vertices);
    let steel = [0.65, 0.64, 0.62, 0.0];
    let width = (hi[0] - lo[0]) * 0.5;
    let roof = lo[1] + (hi[1] - lo[1]) * if design == 1 { 0.95 } else { 0.88 };
    for nozzle in crate::effects::EXHAUSTS {
        let mouth = nozzle + Vec3::Z * 0.025;
        chassis_tube(body, nozzle + Vec3::Z * 0.45, mouth, 0.100, steel);
        chassis_tube(
            body,
            mouth + Vec3::Z * 0.055,
            nozzle,
            0.115,
            [0.78, 0.72, 0.66, 0.0],
        );
        // A recessed dark exhaust opening behind the lip.
        let mut bore =
            rivet_render::box_mesh(Vec3::new(0.076, 0.076, 0.012), [0.06, 0.06, 0.07, 0.0]);
        for v in &mut bore.vertices {
            v.pos = (Vec3::from_array(v.pos) + mouth + Vec3::Z * 0.06).to_array();
        }
        body.append(&bore);
    }
    if design == 2 {
        return;
    }
    for side in [-1.0, 1.0] {
        let rear = Vec3::new(side * width * 0.57, 0.14, -1.16);
        let hoop = Vec3::new(side * width * 0.49, roof, -0.33);
        let nose = Vec3::new(side * width * 0.52, 0.28, 0.82);
        chassis_tube(body, rear, hoop, 0.043, steel);
        chassis_tube(body, hoop, nose, 0.04, steel);
        chassis_tube(
            body,
            rear,
            Vec3::new(-side * width * 0.49, roof, -0.33),
            0.032,
            steel,
        );
    }
    for (y, z, fraction) in [(roof, -0.33, 0.49), (0.14, -1.16, 0.57)] {
        let side = width * fraction;
        chassis_tube(
            body,
            Vec3::new(-side, y, z),
            Vec3::new(side, y, z),
            0.046,
            steel,
        );
    }
}

/// Mount marker lamps on the actual front and rear bodywork of each chassis.
pub fn add_marker_lamps(body: &mut MeshData) {
    let (lo, hi) = bounds(&body.vertices);
    let width = hi[0] - lo[0];
    let height = hi[1] - lo[1];
    let mut lamps = MeshData::default();
    for front in [false, true] {
        let z = if front { hi[2] } else { lo[2] };
        let direction = if front { Vec3::Z } else { Vec3::NEG_Z };
        let color = if front {
            [0.94, 0.86, 0.55, 0.0]
        } else {
            [0.92, 0.11, 0.045, 0.0]
        };
        for side in [-1.0, 1.0] {
            let target = Vec3::new(width * 0.30 * side, lo[1] + height * 0.43, z);
            let score = |v: &Vertex| Vec3::from_array(v.pos).distance_squared(target);
            if let Some(mount) = body
                .vertices
                .iter()
                .filter(|v| Vec3::from_array(v.normal).dot(direction) > 0.6)
                .min_by(|a, b| score(a).total_cmp(&score(b)))
            {
                let normal = Vec3::from_array(mount.normal).normalize();
                let mount_pos = Vec3::from_array(mount.pos);
                let rotation = glam::Quat::from_rotation_arc(direction, normal);
                for (half, offset, tint, flags) in [
                    (
                        Vec3::new(0.115, 0.047, 0.034),
                        0.0,
                        [0.19, 0.21, 0.22, 0.0],
                        0,
                    ),
                    (
                        Vec3::new(0.090, 0.026, 0.010),
                        0.038,
                        color,
                        rivet_render::EMISSIVE,
                    ),
                ] {
                    let mut part = rivet_render::box_mesh(half, tint);
                    for vertex in &mut part.vertices {
                        vertex.pos =
                            (mount_pos + normal * offset + rotation * Vec3::from_array(vertex.pos))
                                .to_array();
                        vertex.normal = (rotation * Vec3::from_array(vertex.normal)).to_array();
                        vertex.material = material(0, 0, rivet_render::UNTEXTURED | flags);
                    }
                    lamps.append(&part);
                }
            }
        }
    }
    body.append(&lamps);
}

/// Wheel radius relative to the kart's length (the scrap kart's chunky tires).
pub const WHEEL_RADIUS_RATIO: f32 = 0.16;

/// The kart split for animation: body plus four wheels stored around their hubs.
/// Wheel order: front-left(+X), front-right(-X), rear-left(+X), rear-right(-X).
pub struct KartParts {
    pub body: MeshData,
    pub wheels: [MeshData; 4],
    pub hubs: [Vec3; 4],
    /// Each wheel's measured radius.
    pub radii: [f32; 4],
    /// Mean wheel radius.
    pub radius: f32,
}

/// Heights above the ground (m) at which a tire's width is measured. Only tires reach this low
/// (the speedster's front fairings already come down to 0.12 m), so the chords are clean of
/// fenders and bodywork.
const CHORD_HEIGHTS: [f32; 9] = [0.04, 0.05, 0.06, 0.07, 0.08, 0.09, 0.10, 0.11, 0.12];
/// Half-thickness (m) of the horizontal slice a chord is measured in.
const CHORD_BAND: f32 = 0.012;
/// A connected piece of the model lying wholly inside a tire (this far out radially, as a share
/// of the measured radius, and within `TIRE_SLAB` m of the hub sideways) turns with it whole.
/// Tripo tires come in many such pieces; taking each whole keeps a sidewall or tread chunk from
/// being cut in two, half turning and half standing still.
const PIECE_REACH: f32 = 1.12;
const TIRE_SLAB: f32 = 0.32;

/// Labels each triangle with its connected piece: triangles sharing a vertex position (the GLB
/// splits vertices at UV seams, so positions are welded) belong to the same piece.
fn pieces(m: &MeshData) -> Vec<u32> {
    fn root(parent: &mut [u32], mut a: u32) -> u32 {
        while parent[a as usize] != a {
            let up = parent[parent[a as usize] as usize];
            parent[a as usize] = up;
            a = up;
        }
        a
    }
    let mut welded = std::collections::HashMap::with_capacity(m.vertices.len());
    let ids: Vec<u32> = m
        .vertices
        .iter()
        .map(|v| {
            let key = v.pos.map(|c| (c * 1e4).round() as i32);
            let next = welded.len() as u32;
            *welded.entry(key).or_insert(next)
        })
        .collect();
    let mut parent: Vec<u32> = (0..welded.len() as u32).collect();
    for tri in m.indices.chunks(3) {
        let a = root(&mut parent, ids[tri[0] as usize]);
        for &i in &tri[1..] {
            let b = root(&mut parent, ids[i as usize]);
            parent[b as usize] = a;
        }
    }
    m.indices
        .chunks(3)
        .map(|tri| root(&mut parent, ids[tri[0] as usize]))
        .collect()
}

/// Measures one tire from the vertices near its corner: at each height `h` above the ground the
/// tire's front-to-back chord has half-width `w = sqrt(2Rh - h²)`, so `R = (w² + h²) / 2h`. The
/// median over several heights shrugs off a slice that caught something else. Returns the hub's
/// z and the radius.
fn fit_tire(verts: &[Vec3], min_y: f32, hub: Vec3, half_width: f32) -> Option<(f32, f32)> {
    let mut radii = Vec::with_capacity(CHORD_HEIGHTS.len());
    let mut mids = Vec::with_capacity(CHORD_HEIGHTS.len());
    for h in CHORD_HEIGHTS {
        let y = min_y + h;
        let (lo, hi) = verts
            .iter()
            .filter(|p| {
                (p.x - hub.x).abs() < half_width
                    && (p.z - hub.z).abs() < 0.9
                    && (p.y - y).abs() < CHORD_BAND
            })
            .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.z), b.max(p.z)));
        if hi <= lo {
            continue;
        }
        let w = (hi - lo) * 0.5;
        radii.push((w * w + h * h) / (2.0 * h));
        mids.push((hi + lo) * 0.5);
    }
    if radii.len() < CHORD_HEIGHTS.len() / 2 {
        return None;
    }
    let median = |v: &mut Vec<f32>| {
        v.sort_by(f32::total_cmp);
        v[v.len() / 2]
    };
    Some((median(&mut mids), median(&mut radii)))
}

/// Splits wheels out of a single kart mesh by geometry: the lowest vertices mark the four
/// contact patches, and triangles inside a wheel-sized cylinder around each axle go to that
/// wheel.
pub fn split_wheels(m: &MeshData, radius: f32) -> KartParts {
    split_wheels_textured(m, radius, None)
}

/// Luminance of `tex` at a triangle's mean UV (0..1).
fn texel_luma(tex: &Rgba8Image, uvs: [[f32; 2]; 3]) -> f32 {
    let u = (uvs.iter().map(|t| t[0]).sum::<f32>() / 3.0).rem_euclid(1.0);
    let v = (uvs.iter().map(|t| t[1]).sum::<f32>() / 3.0).rem_euclid(1.0);
    let x = ((u * tex.width as f32) as u32).min(tex.width.saturating_sub(1));
    let y = ((v * tex.height as f32) as u32).min(tex.height.saturating_sub(1));
    let i = ((y * tex.width + x) * 4) as usize;
    match tex.pixels.get(i..i + 3) {
        Some(p) => (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0,
        None => 0.0,
    }
}

/// Like [`split_wheels`], but with the base color texture: near the tread, only dark (rubber)
/// triangles turn with the wheel, so painted fenders hugging the tire stay on the body.
pub fn split_wheels_textured(m: &MeshData, radius: f32, tex: Option<&Rgba8Image>) -> KartParts {
    let (lo, hi) = bounds(&m.vertices);
    let min_y = lo[1];
    let mut groups: [Vec<Vec3>; 4] = Default::default();
    for v in &m.vertices {
        let p = Vec3::from_array(v.pos);
        if p.y < min_y + 0.06 {
            let k = (p.z < 0.0) as usize * 2 + (p.x < 0.0) as usize;
            groups[k].push(p);
        }
    }
    let mut hubs = [Vec3::ZERO; 4];
    let mut half_width = [0.15f32; 4];
    for k in 0..4 {
        let sx = if k % 2 == 0 { 1.0 } else { -1.0 };
        let sz = if k < 2 { 1.0 } else { -1.0 };
        if groups[k].is_empty() {
            hubs[k] = Vec3::new(sx * hi[0] * 0.8, min_y + radius, sz * hi[2] * 0.65);
            continue;
        }
        let n = groups[k].len() as f32;
        let mean = groups[k].iter().fold(Vec3::ZERO, |a, p| a + *p) / n;
        let (xmin, xmax) = groups[k]
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.x), b.max(p.x)));
        hubs[k] = Vec3::new(mean.x, min_y + radius, mean.z);
        half_width[k] = ((xmax - xmin) * 0.5 + 0.04).clamp(0.08, 0.35);
    }
    // The design's radius is only a first guess: measure each tire, so it turns about its own
    // axle and all of its rubber turns with it.
    let positions: Vec<Vec3> = m.vertices.iter().map(|v| Vec3::from_array(v.pos)).collect();
    let mut radii = [radius; 4];
    for k in 0..4 {
        if let Some((z, r)) = fit_tire(&positions, min_y, hubs[k], half_width[k])
            && (0.4 * radius..1.6 * radius).contains(&r)
        {
            hubs[k] = Vec3::new(hubs[k].x, min_y + r, z);
            radii[k] = r;
        }
    }

    // Whole pieces first: for each piece, the one tire that contains every vertex of it.
    let piece_of = pieces(m);
    let inside = |k: usize, p: Vec3| {
        let h = hubs[k];
        (p.x - h.x).abs() < TIRE_SLAB
            && ((p.y - h.y).powi(2) + (p.z - h.z).powi(2)).sqrt() < radii[k] * PIECE_REACH
    };
    let mut piece_wheel: std::collections::HashMap<u32, Option<usize>> =
        std::collections::HashMap::new();
    for (tri, &piece) in m.indices.chunks(3).zip(&piece_of) {
        let entry = piece_wheel.entry(piece).or_insert_with(|| {
            (0..4).find(|&k| inside(k, Vec3::from_array(m.vertices[tri[0] as usize].pos)))
        });
        if let Some(k) = *entry
            && !tri
                .iter()
                .all(|&i| inside(k, Vec3::from_array(m.vertices[i as usize].pos)))
        {
            *entry = None;
        }
    }

    let mut body = MeshData::default();
    let mut wheels: [MeshData; 4] = Default::default();
    for (tri, piece) in m.indices.chunks(3).zip(&piece_of) {
        let c = tri.iter().fold(Vec3::ZERO, |a, &i| {
            a + Vec3::from_array(m.vertices[i as usize].pos)
        }) / 3.0;
        let whole = piece_wheel.get(piece).copied().flatten();
        let wheel = whole.or_else(|| {
            (0..4).find(|&k| {
                let h = hubs[k];
                let r = radii[k];
                let d = ((c.y - h.y).powi(2) + (c.z - h.z).powi(2)).sqrt();
                if (c.x - h.x).abs() >= half_width[k] || d >= r * 1.05 {
                    return false;
                }
                // Everything inside the tire turns; just past its edge, only rubber does (fenders
                // hugging the tread stay on the body).
                match tex {
                    Some(t) if d > r * 0.98 => {
                        let uvs = [0, 1, 2].map(|j| m.vertices[tri[j] as usize].uv);
                        texel_luma(t, uvs) < 0.22
                    }
                    _ => true,
                }
            })
        });
        let (target, offset) = match wheel {
            Some(k) => (&mut wheels[k], hubs[k]),
            None => (&mut body, Vec3::ZERO),
        };
        for &i in tri {
            let mut v = m.vertices[i as usize];
            v.pos = (Vec3::from_array(v.pos) - offset).to_array();
            target.indices.push(target.vertices.len() as u32);
            target.vertices.push(v);
        }
    }
    KartParts {
        body,
        wheels,
        hubs,
        radii,
        radius: radii.iter().sum::<f32>() / 4.0,
    }
}

#[cfg(test)]
mod model_tests {
    use super::*;

    #[test]
    fn heavy_rivals_drive_war_rigs_light_ones_speedsters() {
        assert_eq!(
            KARTS[model_for(1.14, true)].name,
            "scrapper",
            "the player's kart is the hero"
        );
        assert_eq!(KARTS[model_for(1.14, false)].name, "warrig");
        assert_eq!(KARTS[model_for(0.87, false)].name, "speedster");
        assert_eq!(KARTS[model_for(1.0, false)].name, "scrapper");
    }

    #[test]
    fn every_shipped_kart_splits_into_four_corner_wheels() {
        let assets = crate::materials::find_assets_dir().expect("assets directory");
        for (d, k) in KARTS.iter().enumerate() {
            let (mesh, tex) = load(&assets, k, 20 + d as u32).expect(k.name);
            assert!(tex.is_some(), "{} has a texture", k.name);
            let parts = split_wheels_textured(&mesh, KART_LENGTH * k.wheel_ratio, tex.as_ref());
            let (lo, hi) = bounds(&parts.body.vertices);
            for (w, wheel) in parts.wheels.iter().enumerate() {
                let tris = wheel.indices.len() / 3;
                assert!(tris > 200, "{} wheel {w}: only {tris} triangles", k.name);
                let h = parts.hubs[w];
                // Left wheels on +X, right on -X; fronts toward +Z.
                assert_eq!(h.x > 0.0, w % 2 == 0, "{} wheel {w} side ({h})", k.name);
                assert_eq!(h.z > 0.0, w < 2, "{} wheel {w} front/rear ({h})", k.name);
                assert!(
                    h.x.abs() > 0.25 * hi[0].max(-lo[0]),
                    "{} wheel {w} near the edge",
                    k.name
                );
            }
            for (w, &r) in parts.radii.iter().enumerate() {
                // Each tire is measured, sits on the ground, and is smaller than half the kart.
                assert!((0.2..0.55).contains(&r), "{} wheel {w} radius {r}", k.name);
                let bottom = parts.hubs[w].y - r;
                assert!(
                    (bottom - lo[1].min(-GROUND_OFFSET)).abs() < 0.05,
                    "{} wheel {w} floats: bottom {bottom}",
                    k.name
                );
            }
            // Almost all of each tire turns: no tread or sidewall left standing on the body.
            for w in 0..4 {
                let (h, r) = (parts.hubs[w], parts.radii[w]);
                let in_disc = |p: Vec3| {
                    (p.x - h.x).abs() < 0.3
                        && ((p.y - h.y).powi(2) + (p.z - h.z).powi(2)).sqrt() < 0.95 * r
                };
                let on_wheel = parts.wheels[w]
                    .vertices
                    .iter()
                    .filter(|v| in_disc(Vec3::from_array(v.pos) + h))
                    .count();
                let on_body = parts
                    .body
                    .vertices
                    .iter()
                    .filter(|v| in_disc(Vec3::from_array(v.pos)))
                    .count();
                let share = on_wheel as f32 / (on_wheel + on_body).max(1) as f32;
                assert!(
                    share > 0.75,
                    "{} wheel {w}: only {share:.2} of the tire turns",
                    k.name
                );
            }
            // Left/right hubs mirror each other.
            for (a, b) in [(0, 1), (2, 3)] {
                let (ha, hb) = (parts.hubs[a], parts.hubs[b]);
                assert!(
                    (ha.x + hb.x).abs() < 0.25 && (ha.z - hb.z).abs() < 0.25,
                    "{} axle {a}/{b}: {ha} vs {hb}",
                    k.name
                );
            }
            let body = parts.body.indices.len() / 3;
            assert!(
                body > 3000,
                "{} body keeps most of the model ({body} triangles)",
                k.name
            );
        }
    }

    #[test]
    fn every_kart_model_has_sane_proportions() {
        for k in &KARTS {
            assert!((0.1..0.25).contains(&k.wheel_ratio), "{}", k.name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rivet_assets::GlbMesh;
    use rivet_render::box_mesh;

    /// A body box plus four wheel "drums" (boxes at the corners touching the ground).
    fn synthetic_kart() -> MeshData {
        let mut m = MeshData::default();
        let mut add = |half: Vec3, at: Vec3| {
            let mut b = box_mesh(half, [1.0; 4]);
            for v in &mut b.vertices {
                v.pos = (Vec3::from_array(v.pos) + at).to_array();
            }
            m.append(&b);
        };
        add(Vec3::new(0.6, 0.2, 1.2), Vec3::new(0.0, 0.1, 0.0));
        for (x, z) in [(0.8, 0.9), (-0.8, 0.9), (0.8, -0.9), (-0.8, -0.9)] {
            add(Vec3::new(0.15, 0.4, 0.4), Vec3::new(x, -0.05, z));
        }
        m
    }

    #[test]
    fn splits_four_wheels_from_the_body() {
        let parts = split_wheels(&synthetic_kart(), 0.4);
        for (k, (x, z)) in [(0.8, 0.9), (-0.8, 0.9), (0.8, -0.9), (-0.8, -0.9)]
            .into_iter()
            .enumerate()
        {
            let h = parts.hubs[k];
            assert!(
                (h.x - x).abs() < 0.12 && (h.z - z).abs() < 0.15,
                "hub {k} at {h}"
            );
            assert!(
                !parts.wheels[k].indices.is_empty(),
                "wheel {k} has geometry"
            );
            // Wheel geometry is stored around its hub.
            let far = parts.wheels[k]
                .vertices
                .iter()
                .map(|v| Vec3::from_array(v.pos).length())
                .fold(0.0f32, f32::max);
            assert!(far < 0.75, "wheel {k} vertices hug the hub ({far})");
        }
        assert!(!parts.body.indices.is_empty());
        let wheel_tris: usize = parts.wheels.iter().map(|w| w.indices.len()).sum();
        assert_eq!(
            parts.body.indices.len() + wheel_tris,
            synthetic_kart().indices.len(),
            "every triangle lands somewhere"
        );
    }

    #[test]
    fn painted_fenders_stay_on_the_body() {
        // Tires sample the black texel, fenders (hugging the tire top) the red one.
        let mut m = synthetic_kart();
        for v in &mut m.vertices {
            v.uv = [0.25, 0.5];
        }
        let tire_tris = m.indices.len() / 3;
        let mut fender = MeshData::default();
        for (x, z) in [(0.8, 0.9), (-0.8, 0.9), (0.8, -0.9), (-0.8, -0.9)] {
            let mut b = box_mesh(Vec3::new(0.14, 0.02, 0.2), [1.0; 4]);
            for v in &mut b.vertices {
                v.pos = (Vec3::from_array(v.pos) + Vec3::new(x, -0.05 + 0.4, z)).to_array();
                v.uv = [0.75, 0.5];
            }
            fender.append(&b);
        }
        m.append(&fender);
        let tex = Rgba8Image {
            width: 2,
            height: 1,
            pixels: vec![10, 10, 10, 255, 200, 30, 20, 255],
        };
        let parts = split_wheels_textured(&m, 0.4, Some(&tex));
        let wheel_tris: usize = parts.wheels.iter().map(|w| w.indices.len() / 3).sum();
        let body_tris = parts.body.indices.len() / 3;
        assert!(wheel_tris > 0);
        assert_eq!(body_tris + wheel_tris, tire_tris + fender.indices.len() / 3);
        // Every fender triangle is in the body.
        assert!(
            body_tris >= 12 + fender.indices.len() / 3,
            "fenders roll with the wheels ({body_tris} body triangles)"
        );
    }

    #[test]
    fn normalize_scales_orients_and_grounds() {
        // A 1 × 0.4 × 0.5 box-ish cloud, long along +X, centered at (5, 5, 5).
        let mut g = GlbMesh::default();
        for x in [4.5f32, 5.5] {
            for y in [4.8f32, 5.2] {
                for z in [4.75f32, 5.25] {
                    g.positions.push([x, y, z]);
                    g.normals.push([1.0, 0.0, 0.0]);
                    g.uvs.push([0.0, 0.0]);
                }
            }
        }
        g.indices = (0..8).collect();
        let m = normalize(&g, KART_LENGTH, 0.0);
        let (lo, hi) = bounds(&m.vertices);
        assert!(
            ((hi[2] - lo[2]) - KART_LENGTH).abs() < 1e-3,
            "length along +Z after the yaw: {lo:?} {hi:?}"
        );
        assert!((lo[1] + GROUND_OFFSET).abs() < 1e-4, "wheels on the ground");
        assert!(
            (hi[0] + lo[0]).abs() < 1e-4 && (hi[2] + lo[2]).abs() < 1e-4,
            "centered"
        );
        assert!(
            m.vertices
                .iter()
                .all(|v| v.material == material(KART_LAYER, KART_LAYER, 0))
        );
    }
}
