//! Small procedural meshes for items and weapons.

use glam::Vec3;
use rivet_render::{DITHER, EMISSIVE, MeshData, UNTEXTURED, Vertex, box_mesh, material};

fn flag(m: &mut MeshData, flags: u32) {
    for v in &mut m.vertices {
        v.material = material(0, 0, UNTEXTURED | flags);
    }
}

/// Item box: a dithered glowing shell (the core is drawn separately, spinning).
pub fn item_shell() -> MeshData {
    let mut m = box_mesh(Vec3::splat(0.75), [0.35, 0.75, 0.95, 0.0]);
    flag(&mut m, EMISSIVE | DITHER);
    m
}

pub fn item_core() -> MeshData {
    let mut m = box_mesh(Vec3::splat(0.32), [1.0, 0.55, 0.12, 0.0]);
    flag(&mut m, EMISSIVE);
    m
}

/// A nail tracer, 1 m long along +Z.
pub fn nail() -> MeshData {
    let mut m = box_mesh(Vec3::new(0.05, 0.05, 0.5), [1.0, 0.85, 0.5, 0.0]);
    flag(&mut m, EMISSIVE);
    m
}

/// A unit chain segment along +Z (scaled to length when drawn).
pub fn chain() -> MeshData {
    let mut m = box_mesh(Vec3::new(0.06, 0.06, 0.5), [0.35, 0.33, 0.3, 1.0]);
    flag(&mut m, 0);
    m
}

pub fn hook_head() -> MeshData {
    let mut m = box_mesh(Vec3::new(0.25, 0.25, 0.4), [0.5, 0.48, 0.45, 1.0]);
    flag(&mut m, 0);
    m
}

/// Unit sphere (subdivided octahedron), smooth normals.
pub fn sphere(subdivisions: u32, color: [f32; 4], flags: u32) -> MeshData {
    let mut tris: Vec<[Vec3; 3]> = {
        let b = [
            Vec3::X,
            Vec3::NEG_X,
            Vec3::Y,
            Vec3::NEG_Y,
            Vec3::Z,
            Vec3::NEG_Z,
        ];
        [
            [0, 2, 4],
            [4, 2, 1],
            [1, 2, 5],
            [5, 2, 0],
            [4, 3, 0],
            [1, 3, 4],
            [5, 3, 1],
            [0, 3, 5],
        ]
        .iter()
        .map(|f| [b[f[0]], b[f[1]], b[f[2]]])
        .collect()
    };
    for _ in 0..subdivisions {
        tris = tris
            .into_iter()
            .flat_map(|[a, b, c]| {
                let (ab, bc, ca) = (
                    (a + b).normalize(),
                    (b + c).normalize(),
                    (c + a).normalize(),
                );
                [[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]
            })
            .collect();
    }
    let mut m = MeshData::default();
    for t in tris {
        for p in t {
            m.indices.push(m.vertices.len() as u32);
            m.vertices.push(Vertex {
                pos: p.to_array(),
                normal: p.to_array(),
                uv: [0.0; 2],
                color,
                material: material(0, 0, UNTEXTURED | flags),
            });
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sphere_is_unit_and_outward() {
        let s = sphere(2, [1.0; 4], 0);
        assert_eq!(s.vertices.len(), 8 * 16 * 3);
        for tri in s.indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from_array(s.vertices[tri[i] as usize].pos));
            assert!((a.length() - 1.0).abs() < 1e-5);
            assert!((b - a).cross(c - a).dot(a + b + c) > 0.0, "faces outward");
        }
    }
}
