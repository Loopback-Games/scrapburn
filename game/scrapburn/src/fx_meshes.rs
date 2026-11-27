//! Small procedural meshes for items and weapons.

use glam::{Quat, Vec3};
use rivet_render::{EMISSIVE, MeshData, UNTEXTURED, Vertex, box_mesh, material};

fn flag(m: &mut MeshData, flags: u32) {
    for v in &mut m.vertices {
        v.material = material(0, 0, UNTEXTURED | flags);
    }
}

/// A hollow metal hoop with an illuminated outer stripe. Its axis is local Y.
fn hoop(radius: f32, tube: f32, rotation: Quat, glow: [f32; 4]) -> MeshData {
    let mut mesh = MeshData::default();
    const SEGMENTS: u32 = 32;
    const SIDES: u32 = 8;
    for i in 0..SEGMENTS {
        let angle = std::f32::consts::TAU * i as f32 / SEGMENTS as f32;
        let radial = Vec3::new(angle.cos(), 0.0, angle.sin());
        for j in 0..SIDES {
            let section = std::f32::consts::TAU * j as f32 / SIDES as f32;
            let normal = radial * section.cos() + Vec3::Y * section.sin();
            let lit = j == 0;
            mesh.vertices.push(Vertex {
                pos: (rotation * (radial * radius + normal * tube)).to_array(),
                normal: (rotation * normal).to_array(),
                uv: [0.0; 2],
                color: if lit { glow } else { [0.39, 0.48, 0.53, 0.0] },
                material: material(0, 0, UNTEXTURED | if lit { EMISSIVE } else { 0 }),
            });
            let next = (i + 1) % SEGMENTS;
            let side = (j + 1) % SIDES;
            let (a, b, c, d) = (
                i * SIDES + j,
                i * SIDES + side,
                next * SIDES + side,
                next * SIDES + j,
            );
            mesh.indices.extend([a, b, c, a, c, d]);
        }
    }
    mesh
}

pub fn plasma_halo() -> MeshData {
    let ion = [0.42, 1.0, 0.18, 0.0];
    let mut halo = hoop(0.95, 0.048, Quat::from_rotation_x(0.8), ion);
    halo.append(&hoop(0.82, 0.035, Quat::from_rotation_z(1.0), ion));
    halo
}

pub fn item_shell() -> MeshData {
    let mut cage = hoop(
        0.81,
        0.055,
        Quat::from_rotation_x(0.6),
        [0.1, 0.87, 1.0, 0.0],
    );
    cage.append(&hoop(
        0.69,
        0.05,
        Quat::from_rotation_z(1.1),
        [0.18, 0.64, 1.0, 0.0],
    ));
    cage
}

pub fn item_core() -> MeshData {
    let mut core = sphere(0, [1.0, 0.52, 0.12, 0.0], EMISSIVE);
    for v in &mut core.vertices {
        v.pos = (Vec3::from_array(v.pos) * Vec3::new(0.39, 0.57, 0.39)).to_array();
    }
    core
}

fn bar(mesh: &mut MeshData, a: Vec3, b: Vec3, radius: f32, color: [f32; 4], flags: u32) {
    let axis = b - a;
    let rot = Quat::from_rotation_arc(Vec3::Z, axis.normalize());
    let mut piece = box_mesh(Vec3::new(radius, radius, axis.length() * 0.5), color);
    flag(&mut piece, flags);
    for v in &mut piece.vertices {
        v.pos = (rot * Vec3::from_array(v.pos) + (a + b) * 0.5).to_array();
        v.normal = (rot * Vec3::from_array(v.normal)).to_array();
    }
    mesh.append(&piece);
}

pub fn nail() -> MeshData {
    let mut mesh = MeshData::default();
    bar(
        &mut mesh,
        Vec3::NEG_Z * 0.74,
        Vec3::ZERO,
        0.031,
        [1.0, 0.31, 0.05, 0.0],
        EMISSIVE,
    );
    bar(
        &mut mesh,
        Vec3::ZERO,
        Vec3::Z * 0.38,
        0.065,
        [1.0, 0.86, 0.52, 0.0],
        EMISSIVE,
    );
    mesh
}

pub fn chain() -> MeshData {
    let mut m = box_mesh(Vec3::new(0.043, 0.043, 0.5), [0.31, 0.38, 0.42, 0.0]);
    flag(&mut m, 0);
    bar(
        &mut m,
        Vec3::NEG_Z * 0.08,
        Vec3::Z * 0.08,
        0.065,
        [0.2, 0.8, 0.95, 0.0],
        EMISSIVE,
    );
    m
}

pub fn hook_head() -> MeshData {
    let mut mesh = MeshData::default();
    for i in 0..3 {
        let rot = Quat::from_rotation_z(i as f32 * std::f32::consts::TAU / 3.0);
        let elbow = rot * Vec3::new(0.29, 0.0, 0.21);
        bar(
            &mut mesh,
            Vec3::NEG_Z * 0.35,
            elbow,
            0.075,
            [0.51, 0.58, 0.63, 0.0],
            0,
        );
        bar(
            &mut mesh,
            elbow,
            rot * Vec3::new(0.38, 0.0, -0.04),
            0.05,
            [0.15, 0.79, 0.97, 0.0],
            EMISSIVE,
        );
    }
    mesh
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
