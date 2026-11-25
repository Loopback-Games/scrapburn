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

/// Loads `models/karts/<name>.glb`: the mesh and its base color texture.
pub fn load(assets: &Path, name: &str) -> Result<(MeshData, Option<Rgba8Image>), GlbError> {
    let g = load_glb(
        &assets
            .join("models")
            .join("karts")
            .join(format!("{name}.glb")),
    )?;
    let tex = g.base_color.clone();
    Ok((normalize(&g, KART_LENGTH, 0.0), tex))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rivet_assets::GlbMesh;

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
