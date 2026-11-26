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

/// Wheel radius relative to the kart's length (the scrap kart's chunky tires).
pub const WHEEL_RADIUS_RATIO: f32 = 0.16;

/// The kart split for animation: body plus four wheels stored around their hubs.
/// Wheel order: front-left(+X), front-right(-X), rear-left(+X), rear-right(-X).
pub struct KartParts {
    pub body: MeshData,
    pub wheels: [MeshData; 4],
    pub hubs: [Vec3; 4],
    pub radius: f32,
}

/// Splits wheels out of a single kart mesh by geometry: the lowest vertices mark the four
/// contact patches, and triangles inside a wheel-sized cylinder around each axle go to that
/// wheel.
pub fn split_wheels(m: &MeshData, radius: f32) -> KartParts {
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

    let mut body = MeshData::default();
    let mut wheels: [MeshData; 4] = Default::default();
    for tri in m.indices.chunks(3) {
        let c = tri.iter().fold(Vec3::ZERO, |a, &i| {
            a + Vec3::from_array(m.vertices[i as usize].pos)
        }) / 3.0;
        let wheel = (0..4).find(|&k| {
            let h = hubs[k];
            (c.x - h.x).abs() < half_width[k]
                && ((c.y - h.y).powi(2) + (c.z - h.z).powi(2)).sqrt() < radius * 1.08
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
        radius,
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
