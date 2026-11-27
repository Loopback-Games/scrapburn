//! Tournament heraldry for the royal road at Grindstone.

use std::f32::consts::TAU;

use glam::{Mat4, Quat, Vec3, vec3};
use rivet_render::{EMISSIVE, MeshData, UNTEXTURED, material};

use super::{Layer, quad, textured_box};

const GARNET: [f32; 3] = [0.70, 0.038, 0.095];
const PEACOCK: [f32; 3] = [0.025, 0.40, 0.46];
const GOLD: [f32; 3] = [0.99, 0.63, 0.17];
const IVORY: [f32; 3] = [0.96, 0.84, 0.59];

fn beam(
    mesh: &mut MeshData,
    frame: Mat4,
    a: Vec3,
    b: Vec3,
    radius: f32,
    layer: Layer,
    tint: [f32; 3],
) {
    let span = b - a;
    let xf = Mat4::from_rotation_translation(
        Quat::from_rotation_arc(Vec3::Y, span.normalize()),
        (a + b) * 0.5,
    );
    textured_box(
        mesh,
        vec3(radius, span.length() * 0.5, radius),
        frame * xf,
        layer,
        tint,
    );
}

// Cloth and glass are thin enough to see from either side of the course.
fn panel(mesh: &mut MeshData, frame: Mat4, points: [Vec3; 4], tint: [f32; 3], lit: bool) {
    let mut v = points.map(|p| frame.transform_point3(p));
    let mat = material(0, 0, UNTEXTURED | if lit { EMISSIVE } else { 0 });
    for _ in 0..2 {
        quad(
            mesh,
            v,
            [[0.0; 2]; 4],
            [tint[0], tint[1], tint[2], 0.0],
            mat,
        );
        v.reverse();
    }
}

pub(super) fn pennant_line(mesh: &mut MeshData, frame: Mat4, index: usize) {
    let cord = |x: f32| vec3(x, 8.9 + 2.4 * (x / 9.5).powi(2), 0.0);
    for side in [-1.0, 1.0] {
        let x = side * 9.5;
        beam(
            mesh,
            frame,
            vec3(x, -3.0, 0.0),
            cord(x) + Vec3::Y * 0.8,
            0.14,
            Layer::WoodPlanks,
            IVORY,
        );
    }
    for i in 0..11 {
        let x = -9.5 + 19.0 * i as f32 / 11.0;
        let next = -9.5 + 19.0 * (i + 1) as f32 / 11.0;
        beam(
            mesh,
            frame,
            cord(x),
            cord(next),
            0.045,
            Layer::MetalRust,
            GOLD,
        );
        let a = cord(x + 0.17);
        let b = cord(next - 0.17);
        let tip = (a + b) * 0.5 + vec3(0.22, -1.7, 0.43);
        let tint = [GARNET, IVORY, PEACOCK][(i + index) % 3];
        panel(mesh, frame, [a, b, tip + Vec3::X * 0.035, tip], tint, false);
    }
}

pub(super) fn gate_standards(mesh: &mut MeshData, frame: Mat4) {
    for (x, tint) in [(-9.6, PEACOCK), (9.6, GARNET)] {
        let face = frame * Mat4::from_translation(vec3(x, 11.6, -2.05));
        // Three pleats, with a gold band following the swallowtail hem.
        for i in 0..6 {
            let point = |j: usize, t: f32| {
                let u = j as f32 / 6.0;
                vec3(
                    2.8 * (u - 0.5),
                    -t * (6.6 - 1.55 * (u * std::f32::consts::PI).sin()),
                    -0.19 * (j % 2) as f32 - 0.24 * t,
                )
            };
            for (top, bottom, color) in [
                (0.0, 0.08, GOLD),
                (0.08, 0.88, tint),
                (0.88, 0.94, GOLD),
                (0.94, 1.0, tint),
            ] {
                panel(
                    mesh,
                    face,
                    [
                        point(i, top),
                        point(i + 1, top),
                        point(i + 1, bottom),
                        point(i, bottom),
                    ],
                    color,
                    false,
                );
            }
        }
    }
}

pub(super) fn sun_gate(mesh: &mut MeshData, frame: Mat4) {
    let brass = |m: &mut MeshData, a, b, r| beam(m, frame, a, b, r, Layer::MetalRust, GOLD);
    for x in [-9.6, 9.6] {
        for (bottom, top, width) in [
            (-4.0, 2.8, 1.95),
            (2.8, 11.9, 1.7),
            (11.9, 12.6, 2.1),
            (12.6, 16.1, 1.36),
        ] {
            textured_box(
                mesh,
                vec3(width, (top - bottom) * 0.5, width),
                frame * Mat4::from_translation(vec3(x, (top + bottom) * 0.5, 0.0)),
                Layer::StoneWall,
                IVORY,
            );
        }
        for dx in [-1.45, 1.45] {
            for z in [-1.45, 1.45] {
                brass(mesh, vec3(x + dx, 15.8, z), vec3(x, 21.2, 0.0), 0.15);
            }
        }
        brass(mesh, vec3(x, 11.6, 0.0), vec3(x * 0.38, 14.6, 0.0), 0.36);
    }
    brass(mesh, vec3(-9.6, 12.5, 0.0), vec3(9.6, 12.5, 0.0), 0.34);
    gate_standards(mesh, frame);

    // Openwork solar rose: jeweled petals between two concentric brass hoops.
    let polar = |turn: f32, radius: f32| {
        let (s, c) = (turn * TAU / 12.0).sin_cos();
        vec3(s * radius, 17.7 + c * radius, 0.0)
    };
    for k in 0..12 {
        let t = k as f32;
        for r in [1.15, 4.95] {
            brass(mesh, polar(t, r), polar(t + 1.0, r), 0.15);
        }
        let petal = [
            polar(t, 1.15),
            polar(t - 0.45, 3.15),
            polar(t, 4.95),
            polar(t + 0.45, 3.15),
        ];
        panel(
            mesh,
            frame,
            petal,
            if k % 2 == 0 { GARNET } else { PEACOCK },
            true,
        );
        for i in 0..4 {
            brass(mesh, petal[i], petal[(i + 1) % 4], 0.075);
        }
        brass(
            mesh,
            polar(t, 4.95),
            polar(t, if k % 2 == 0 { 6.5 } else { 5.7 }),
            0.11,
        );
    }
}
