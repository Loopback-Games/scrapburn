//! Dustbowl's fossil bed, refinery cableway, and windblown mesa country.

use glam::{Mat4, Quat, Vec2, Vec3};
use rivet_render::{MeshData, ParticleInstance, TRIPLANAR_B, material};

use super::{Layer, Solid, quad, sandstone_pillar, textured_box};
use crate::terrain;

pub struct Canyon {
    tram: Mat4,
    whirlwind: Vec3,
}

const SPAN: f32 = 48.0;
const COPPER: [f32; 3] = [0.83, 0.54, 0.32];

fn beam(mesh: &mut MeshData, frame: Mat4, a: Vec3, b: Vec3, half_width: f32, color: [f32; 3]) {
    let span = b - a;
    let transform = Mat4::from_rotation_translation(
        Quat::from_rotation_arc(Vec3::Y, span.normalize()),
        (a + b) * 0.5,
    );
    textured_box(
        mesh,
        Vec3::new(half_width, span.length() * 0.5, half_width),
        frame * transform,
        Layer::MetalRust,
        color,
    );
}

// A two-lane aerial tramway. The cabin hangers and the cables use the same catenary approximation.
fn cable(x: f32, lane: f32) -> Vec3 {
    Vec3::new(x, 20.0 + 6.0 * (x / SPAN).powi(2) + 0.045 * x, lane)
}

fn cableway(mesh: &mut MeshData, frame: Mat4) {
    for x in [-SPAN, SPAN] {
        let y = cable(x, 0.0).y;
        for z in [-2.8, 2.8] {
            for side in [-1.0, 1.0] {
                beam(
                    mesh,
                    frame,
                    Vec3::new(x + side * 4.0, -4.0, z),
                    Vec3::new(x + side, y, z),
                    0.33,
                    COPPER,
                );
            }
            for level in 0..4 {
                let t = level as f32 / 4.0;
                let w = 3.6 - 2.6 * t;
                beam(
                    mesh,
                    frame,
                    Vec3::new(x - w, y * t, z),
                    Vec3::new(x + w - 0.65, y * (t + 0.25), z),
                    0.15,
                    COPPER,
                );
                beam(
                    mesh,
                    frame,
                    Vec3::new(x + w, y * t, z),
                    Vec3::new(x - w + 0.65, y * (t + 0.25), z),
                    0.15,
                    COPPER,
                );
            }
        }
        beam(
            mesh,
            frame,
            Vec3::new(x, y, -3.4),
            Vec3::new(x, y, 3.4),
            0.5,
            COPPER,
        );
    }
    for lane in [-2.8, 2.8] {
        for i in 0..16 {
            let x = -SPAN + i as f32 * SPAN / 8.0;
            beam(
                mesh,
                frame,
                cable(x, lane),
                cable(x + SPAN / 8.0, lane),
                0.10,
                [0.34, 0.32, 0.31],
            );
        }
    }
}

/// One ore skip with a flared lip, yellow side bands and exposed suspension arms.
/// The origin is the trolley at cable height.
pub fn ore_skip() -> MeshData {
    let mut mesh = MeshData::default();
    let mut plate = |pos, half, layer, tint| {
        textured_box(&mut mesh, half, Mat4::from_translation(pos), layer, tint)
    };
    plate(
        Vec3::ZERO,
        Vec3::new(0.9, 0.27, 0.34),
        Layer::MetalRust,
        [0.42; 3],
    );
    plate(
        Vec3::new(0.0, -4.6, 0.0),
        Vec3::new(1.8, 0.82, 1.2),
        Layer::MetalRust,
        COPPER,
    );
    plate(
        Vec3::new(0.0, -3.85, 0.0),
        Vec3::new(2.05, 0.24, 1.35),
        Layer::MetalRust,
        [0.98, 0.76, 0.28],
    );
    for (x, z, h) in [(-0.9, 0.0, 0.6), (0.55, -0.28, 0.88), (1.2, 0.46, 0.5)] {
        let rock = Mat4::from_rotation_translation(
            Quat::from_rotation_z(x * 0.28),
            Vec3::new(x, -3.65, z),
        );
        textured_box(
            &mut mesh,
            Vec3::new(0.6, h, 0.57),
            rock,
            Layer::Rock,
            [0.38, 0.32, 0.3],
        );
    }
    for z in [-1.15, 1.15] {
        for x in [-1.45, 1.45] {
            beam(
                &mut mesh,
                Mat4::IDENTITY,
                Vec3::new(0.0, -0.1, 0.0),
                Vec3::new(x, -4.0, z),
                0.10,
                COPPER,
            );
        }
    }
    mesh
}

// Tapered elliptical sections give the ribs a blade-like edge and a broad, weathered root.
fn rib(mesh: &mut MeshData, frame: Mat4, height: f32, reach: f32, lengthwise: f32, thickness: f32) {
    let bone = material(Layer::Concrete as u32, Layer::Concrete as u32, TRIPLANAR_B);
    let point = |t: f32, angle: f32| {
        let a = t * 2.55;
        let center = Vec3::new(
            reach * (1.0 - a.cos()),
            height * a.sin(),
            lengthwise * t * t,
        );
        let normal = Vec3::new(height * a.cos(), -reach * a.sin(), 0.0).normalize();
        let width = thickness * (1.0 - t * 0.97) * (1.0 + 0.18 * (t * 15.0).cos());
        frame.transform_point3(
            center + normal * angle.cos() * width + Vec3::Z * angle.sin() * width * 0.68,
        )
    };
    for section in 0..14 {
        let (s, t) = (section as f32 / 14.0, (section + 1) as f32 / 14.0);
        let ochre = (s * 4.0).min(1.0);
        let tint = [
            0.82 + 0.17 * ochre,
            0.62 + 0.31 * ochre,
            0.40 + 0.38 * ochre,
            1.0,
        ];
        for facet in 0..9 {
            let a = facet as f32 * std::f32::consts::TAU / 9.0;
            let b = (facet + 1) as f32 * std::f32::consts::TAU / 9.0;
            quad(
                mesh,
                [point(s, a), point(t, a), point(t, b), point(s, b)],
                [[0.0; 2]; 4],
                tint,
                bone,
            );
        }
    }
}

// Cross-sections along the length of a bone. Varying the width and height leaves
// real openings for the eye sockets and between the jaws.
fn bone(mesh: &mut MeshData, frame: Mat4, sections: &[[f32; 5]], tint: [f32; 4]) {
    let mat = material(Layer::Concrete as u32, Layer::Concrete as u32, TRIPLANAR_B);
    let ring = |[x, y, z, w, h]: [f32; 5], i: usize| {
        let angle = i as f32 * std::f32::consts::TAU / 10.0;
        frame.transform_point3(Vec3::new(x + w * angle.cos(), y + h * angle.sin(), z))
    };
    for pair in sections.windows(2) {
        for i in 0..10 {
            let mut face = [
                ring(pair[0], i),
                ring(pair[0], i + 1),
                ring(pair[1], i + 1),
                ring(pair[1], i),
            ];
            if pair[1][2] < pair[0][2] {
                face.reverse();
            }
            quad(mesh, face, [[0.0; 2]; 4], tint, mat);
        }
    }
}

fn skull(mesh: &mut MeshData, frame: Mat4) {
    let ivory = [1.0, 0.94, 0.77, 1.0];
    // Cranial vault narrowing into the nasal bridge.
    bone(
        mesh,
        frame,
        &[
            [0.0, 5.0, 7.0, 0.05, 0.1],
            [0.0, 6.1, 5.0, 4.8, 3.6],
            [0.0, 8.0, 1.5, 5.4, 2.5],
            [0.0, 8.1, -2.0, 3.2, 1.5],
            [0.0, 6.0, -6.5, 2.5, 1.6],
            [0.0, 4.5, -11.0, 1.8, 1.2],
            [0.0, 4.1, -12.3, 0.15, 0.2],
        ],
        ivory,
    );
    for side in [-1.0, 1.0] {
        let cheek = frame * Mat4::from_translation(Vec3::X * side * 3.7);
        bone(
            mesh,
            cheek,
            &[
                [0.0, 5.0, 5.9, 0.2, 0.4],
                [side, 4.9, 4.3, 1.4, 2.0],
                [side, 4.4, 1.0, 0.85, 0.65],
                [0.0, 5.0, -2.5, 0.65, 0.75],
                [-side, 6.7, -4.8, 0.9, 1.25],
                [-side, 6.5, -6.4, 0.2, 0.5],
            ],
            ivory,
        );
        // Swept horns, thinner and darker at their chipped tips.
        bone(
            mesh,
            cheek,
            &[
                [0.0, 8.5, 0.0, 1.1, 1.4],
                [side, 9.9, 2.0, 1.5, 1.45],
                [side * 2.7, 12.1, 4.5, 1.15, 1.0],
                [side * 3.8, 13.2, 7.6, 0.64, 0.56],
                [side * 4.3, 13.6, 10.8, 0.05, 0.05],
            ],
            [0.90, 0.80, 0.61, 1.0],
        );
        bone(
            mesh,
            cheek,
            &[
                [0.0, 4.8, 4.8, 0.5, 0.6],
                [0.0, 2.4, 2.6, 1.25, 1.15],
                [-side * 0.65, 1.0, -1.0, 0.95, 0.75],
                [-side * 1.45, 0.9, -6.3, 0.8, 0.7],
                [-side * 2.3, 1.4, -10.3, 0.65, 0.55],
                [-side * 2.7, 1.65, -11.0, 0.08, 0.09],
            ],
            ivory,
        );
        for k in 0..5 {
            let z = -5.2 - k as f32 * 1.15;
            let tooth = frame
                * Mat4::from_translation(Vec3::new(
                    side * (2.4 - k as f32 * 0.12),
                    5.1 - k as f32 * 0.35,
                    z,
                ))
                * Mat4::from_rotation_x(1.3);
            bone(
                mesh,
                tooth,
                &[
                    [0.0, 0.0, 0.0, 0.38, 0.48],
                    [0.0, 0.0, 0.7, 0.29, 0.32],
                    [0.0, 0.17, 1.4, 0.02, 0.02],
                ],
                ivory,
            );
        }
    }
}

impl Canyon {
    /// Builds the landmarks into `mesh` and their solid parts into `solids`: the tram towers,
    /// the rib roots, the skull and the mesa pillars. The rib arcs and the cable are overhead.
    pub(super) fn build(mesh: &mut MeshData, solids: &mut Vec<Solid>, line: &[Vec3]) -> Self {
        let frame = |at: f32| {
            let i = (at * (line.len() - 1) as f32) as usize;
            let f = line[i + 1] - line[i];
            Mat4::from_rotation_translation(Quat::from_rotation_y(f.x.atan2(f.z)), line[i])
        };
        let ground = |p: Vec3| Vec3::new(p.x, terrain::height_at(line, p.x, p.z), p.z);
        let tram = frame(0.325) * Mat4::from_rotation_y(0.24);
        cableway(mesh, tram);
        for x in [-SPAN, SPAN] {
            solids.push(Solid::upright_box(
                tram,
                Vec3::new(x, -4.0, 0.0),
                Vec2::new(4.3, 3.2),
                cable(x, 0.0).y + 4.0,
            ));
        }
        // The carcass curves with the first straight. Only the tips reach over the lane.
        for i in 0..9 {
            let t = i as f32 / 8.0;
            let size = 0.62 + 0.38 * (t * std::f32::consts::PI).sin();
            let road = frame(0.038 + t * 0.043);
            let root = ground(road.transform_point3(Vec3::new(-22.0, 0.0, 0.0))) - Vec3::Y * 1.8;
            let fossil = Mat4::from_cols(road.x_axis, road.y_axis, road.z_axis, root.extend(1.0))
                * Mat4::from_rotation_y(0.18 * (i as f32 * 1.9).sin());
            // The rib roots and the vertebra between them.
            solids.push(Solid::upright_box(
                fossil,
                Vec3::new(0.0, 0.0, 0.1 * size),
                Vec2::new(2.2 * size, 3.4 * size),
                6.0,
            ));
            // Broad vertebrae peek through the sand between the rib roots.
            bone(
                mesh,
                fossil * Mat4::from_scale(Vec3::splat(size)),
                &[
                    [0.0, 1.2, -3.5, 0.8, 0.8],
                    [0.0, 1.9, -2.7, 2.8, 2.1],
                    [0.0, 2.2, 0.0, 2.0, 1.5],
                    [0.0, 2.1, 2.4, 2.7, 2.0],
                    [0.0, 1.3, 3.7, 0.6, 0.7],
                ],
                [0.88, 0.78, 0.61, 1.0],
            );
            rib(
                mesh,
                fossil,
                21.0 * size,
                13.8 * size,
                2.0 + 4.0 * t,
                2.25 * size,
            );
            rib(
                mesh,
                fossil * Mat4::from_rotation_y(std::f32::consts::PI),
                12.5 * size,
                7.8 * size,
                -4.0 * t,
                1.7 * size,
            );
        }
        let neck = frame(0.091);
        let head = ground(neck.transform_point3(Vec3::new(-24.0, 0.0, 0.0))) - Vec3::Y * 0.75;
        let skull_frame = Mat4::from_cols(neck.x_axis, neck.y_axis, neck.z_axis, head.extend(1.0))
            * Mat4::from_rotation_y(-0.75);
        skull(mesh, skull_frame);
        solids.push(Solid::upright_box(
            skull_frame,
            Vec3::new(0.0, 0.0, -2.7),
            Vec2::new(5.2, 9.2),
            9.0,
        ));
        let basin = frame(0.53);
        for (i, (x, z, r, h)) in [
            (95.0, 85.0, 25.0, 56.0),
            (127.0, 108.0, 15.0, 73.0),
            (155.0, 92.0, 32.0, 47.0),
        ]
        .into_iter()
        .enumerate()
        {
            let p = ground(basin.transform_point3(Vec3::new(x, 0.0, z)));
            if terrain::distance_to(line, p.x, p.z) > r + 16.0 {
                sandstone_pillar(mesh, p, r, h, i as u64 + 310);
                solids.push(Solid {
                    base: (0..12)
                        .map(|k| {
                            let a = k as f32 * std::f32::consts::TAU / 12.0;
                            // Sunk deep: the dunes around a wide pillar fall away.
                            p - Vec3::Y * 6.0 + Vec3::new(a.cos(), 0.0, a.sin()) * r * 0.8
                        })
                        .collect(),
                    up: Vec3::Y,
                    height: h + 6.0,
                });
            }
        }
        Self {
            tram,
            whirlwind: ground(frame(0.61).transform_point3(Vec3::new(35.0, 0.0, 6.0))),
        }
    }

    pub fn skips(&self, time: f32) -> [Mat4; 2] {
        let x = (time * 0.145).cos() * (SPAN - 1.5);
        [-1.0, 1.0].map(|dir| {
            self.tram
                * Mat4::from_translation(cable(x * dir, 2.8 * dir))
                * Mat4::from_scale(Vec3::splat(1.4))
        })
    }

    pub fn dust_devil(&self, time: f32, eye: Vec3, particles: &mut Vec<ParticleInstance>) {
        if eye.distance_squared(self.whirlwind) > 290.0_f32.powi(2) {
            return;
        }
        // A low skirt of blown sand connects the twisting column to the basin floor.
        for i in 0..12 {
            let angle = i as f32 * 2.4 + time * 0.7;
            let radius = 3.0 + 2.3 * (angle * 3.0).sin();
            particles.push(ParticleInstance {
                pos: (self.whirlwind + Vec3::new(radius * angle.cos(), 1.6, radius * angle.sin()))
                    .to_array(),
                size: 4.2,
                color: [0.49, 0.22, 0.11, 0.15],
            });
        }
        for i in 0..48 {
            let t = (i as f32 / 48.0 + time * 0.13).fract();
            let a = time * 1.7 + t * 13.0 + 0.4 * (i as f32 * 2.4).sin();
            let r = 1.4 + t * t * 7.0;
            let pos = self.whirlwind + Vec3::new(r * a.cos() + 4.0 * t * t, t * 35.0, r * a.sin());
            particles.push(ParticleInstance {
                pos: pos.to_array(),
                size: 3.2 + t * 8.1,
                color: [0.40, 0.20, 0.09, 0.32 * (1.0 - t)],
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{ROAD_HALF_WIDTH, SAND_SHOULDER_WIDTH, TRACKS, build_track};

    #[test]
    fn canyon_landmarks_leave_room_to_drive() {
        let track = build_track(&TRACKS[0]);
        let mut landmarks = MeshData::default();
        let canyon = Canyon::build(&mut landmarks, &mut Vec::new(), &track.centerline);
        let check = |p: Vec3| {
            let road = track
                .centerline
                .iter()
                .min_by(|a, b| {
                    let horizontal = |q: &Vec3| (q.x - p.x).powi(2) + (q.z - p.z).powi(2);
                    horizontal(a).total_cmp(&horizontal(b))
                })
                .expect("Dustbowl has a centerline");
            let width = ROAD_HALF_WIDTH + SAND_SHOULDER_WIDTH;
            assert!(
                (p.x - road.x).hypot(p.z - road.z) > width || p.y > road.y + 5.0,
                "landmark at {p} intrudes on the road or shoulder near {road}"
            );
        };
        for vertex in landmarks.vertices {
            check(Vec3::from_array(vertex.pos));
        }
        let skip = ore_skip();
        for tick in 0..60 {
            for transform in canyon.skips(tick as f32) {
                for vertex in &skip.vertices {
                    check(transform.transform_point3(Vec3::from_array(vertex.pos)));
                }
            }
        }
    }
}
