//! Tires, rims, hubs and brakes in wheel-local coordinates (+X along the axle).
use super::{chassis_tube, offroad::panel};
use glam::{Vec3, vec3};
use rivet_render::{MeshData, UNTEXTURED, Vertex, material};
use std::f32::consts::TAU;

type Color = [f32; 4];

fn radial(turn: f32) -> Vec3 {
    let (s, c) = (TAU * turn).sin_cos();
    vec3(0., c, s)
}

/// Turn a closed cross section around the axle. Each band owns its normals so
/// the bead, rim lips and machined steps catch distinct highlights.
fn lathe(mesh: &mut MeshData, profile: &[[f32; 2]], color: Color) {
    for (a, b) in profile.iter().zip(profile.iter().cycle().skip(1)) {
        let dx = b[0] - a[0];
        let dr = b[1] - a[1];
        let base = mesh.vertices.len() as u32;
        for ring in 0..=48 {
            let ray = radial(ring as f32 / 48.);
            let normal = (ray * dx - Vec3::X * dr).normalize();
            for &[x, r] in [a, b] {
                mesh.vertices.push(Vertex {
                    pos: (Vec3::X * x + ray * r).to_array(),
                    normal: normal.to_array(),
                    uv: [0.; 2],
                    color,
                    material: material(0, 0, UNTEXTURED),
                });
            }
            if ring < 48 {
                let i = base + ring * 2;
                mesh.indices.extend([i, i + 2, i + 1, i + 1, i + 2, i + 3]);
            }
        }
    }
}

pub(super) fn wheel(radius: f32, half: f32, quad: bool) -> MeshData {
    let mut mesh = MeshData::default();
    let rim = radius * if quad { 0.53 } else { 0.74 };
    let alloy = [0.55, 0.58, 0.59, 0.];
    let dark = [0.18, 0.20, 0.21, 0.];
    lathe(
        &mut mesh,
        &[
            [-half * 0.75, rim],
            [-half, rim + 0.035],
            [-half, radius * 0.80],
            [-half * 0.85, radius * 0.90],
            [-half * 0.52, radius * 0.95],
            [half * 0.52, radius * 0.95],
            [half * 0.85, radius * 0.90],
            [half, radius * 0.80],
            [half, rim + 0.035],
            [half * 0.75, rim],
        ],
        [0.15, 0.16, 0.17, 0.],
    );
    // Three staggered rows of tread blocks wrap over the shoulders.
    for i in 0..32 {
        for row in [-1., 0., 1.] {
            let angle = TAU * (i as f32 + row * 0.36) / 32.;
            let shoulder = row.abs();
            let center = vec3(row * half * 0.68, 0., 0.)
                + radial(angle / TAU) * radius * (0.966 - 0.045 * shoulder);
            panel(
                &mut mesh,
                center,
                vec3(half * 0.32, radius * 0.032, radius * 0.066),
                angle,
                [0.19, 0.195, 0.20, 0.],
            );
        }
    }
    let lip = half * 0.80;
    lathe(
        &mut mesh,
        &[
            [-lip, rim * 0.91],
            [-lip, rim * 1.035],
            [-lip * 0.80, rim * 1.035],
            [-lip * 0.60, rim * 0.97],
            [lip * 0.60, rim * 0.97],
            [lip * 0.80, rim * 1.035],
            [lip, rim * 1.035],
            [lip, rim * 0.91],
        ],
        if quad { dark } else { alloy },
    );
    let hub = rim * if quad { 0.32 } else { 0.18 };
    lathe(
        &mut mesh,
        &[
            [-half, 0.001],
            [-half, hub * 0.8],
            [-half * 0.70, hub],
            [half * 0.70, hub],
            [half, hub * 0.8],
            [half, 0.001],
        ],
        alloy,
    );
    let spokes = if quad { 6 } else { 16 };
    for i in 0..spokes {
        let turn = i as f32 / spokes as f32;
        for side in [-1., 1.] {
            let inner = Vec3::X * half * 0.60 * side + radial(turn) * hub;
            let outer = Vec3::X * half * 0.34 * side + radial(turn + 0.082 * side) * rim * 0.94;
            chassis_tube(
                &mut mesh,
                inner,
                outer,
                if quad { 0.018 } else { 0.0055 },
                alloy,
            );
        }
    }
    let rotor = rim * 0.58;
    let x = half * 0.73;
    lathe(
        &mut mesh,
        &[
            [x, rotor * 0.78],
            [x, rotor],
            [x + 0.008, rotor],
            [x + 0.008, rotor * 0.78],
        ],
        alloy,
    );
    for i in 0..6 {
        let ray = radial(i as f32 / 6.);
        chassis_tube(
            &mut mesh,
            Vec3::X * x + ray * hub,
            Vec3::X * x + ray * rotor * 0.86,
            0.013,
            dark,
        );
    }
    mesh
}
