//! Sport quad and motocross chassis, built around their animated axles.
use super::{GROUND_OFFSET, KartParts, chassis_tube};
use glam::{Quat, Vec3, Vec4, vec3};
use rivet_render::{MeshData, UNTEXTURED, Vertex, box_mesh, material};
use std::f32::consts::TAU;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrailModel {
    Jackrabbit,
    Redline,
    Ridgeback,
}

type Color = [f32; 4];
const RUBBER: Color = [0.12, 0.13, 0.14, 0.];
const STEEL: Color = [0.65, 0.67, 0.68, 0.];
const GRAPHITE: Color = [0.26, 0.28, 0.29, 0.];

pub(super) fn panel(mesh: &mut MeshData, center: Vec3, half: Vec3, pitch: f32, color: Color) {
    let rotation = Quat::from_rotation_x(pitch);
    let mut part = box_mesh(half, color);
    for v in &mut part.vertices {
        v.pos = (center + rotation * Vec3::from_array(v.pos)).to_array();
        v.normal = (rotation * Vec3::from_array(v.normal)).to_array();
        v.material = material(0, 0, UNTEXTURED);
    }
    mesh.append(&part);
}

/// Each quarter of a row in the atlas is a separate painted surface. Keep UVs
/// inside its border so the neighboring livery does not bleed into the mipmaps.
#[derive(Clone, Copy)]
struct Surface {
    layer: u32,
    tile: u32,
}

impl Surface {
    fn uv(self, u: f32, v: f32) -> [f32; 2] {
        [
            ((self.tile % 4) as f32 + 0.02 + 0.96 * u) / 4.,
            ((self.tile / 4) as f32 + 0.02 + 0.96 * v) / 4.,
        ]
    }
}

/// Loft a rounded shell through longitudinal sections [z, half width, bottom, top].
/// Corner cutting rounds both the fender outline and the transition into the tank.
fn fairing(mesh: &mut MeshData, x: f32, sections: &[[f32; 4]], surface: Surface) {
    let mut rings: Vec<Vec4> = sections.iter().copied().map(Vec4::from_array).collect();
    for _ in 0..2 {
        let mut smooth = vec![rings[0]];
        for pair in rings.windows(2) {
            smooth.extend([pair[0].lerp(pair[1], 0.25), pair[0].lerp(pair[1], 0.75)]);
        }
        smooth.push(rings[rings.len() - 1]);
        rings = smooth;
    }
    const SIDES: usize = 20;
    let z0 = rings[0].x;
    let length = rings[rings.len() - 1].x - z0;
    let base = mesh.vertices.len() as u32;
    let ring_point = |r: Vec4, angle: f32| {
        let (s, c) = angle.sin_cos();
        let shoulder = c.signum() * c.abs().powf(0.68);
        vec3(
            x + r.y * shoulder,
            (r.z + r.w) * 0.5 + (r.w - r.z) * 0.5 * s.signum() * s.abs().powf(0.68),
            r.x,
        )
    };
    for (j, &ring) in rings.iter().enumerate() {
        for k in 0..=SIDES {
            let angle = TAU * k as f32 / SIDES as f32;
            let point = ring_point(ring, angle);
            let tangent = ring_point(ring, angle + 0.015) - ring_point(ring, angle - 0.015);
            let along = ring_point(rings[(j + 1).min(rings.len() - 1)], angle)
                - ring_point(rings[j.saturating_sub(1)], angle);
            let normal = tangent.cross(along).normalize();
            let along_uv = (ring.x - z0) / length;
            let uv = if surface.tile < 3 {
                surface.uv(along_uv, (ring.w - point.y) / (ring.w - ring.z))
            } else {
                surface.uv(0.5 + (point.x - x) / (2. * ring.y), 1. - along_uv)
            };
            let shade = 0.80 + 0.20 * (angle.sin() * 0.5 + 0.5);
            mesh.vertices.push(Vertex {
                pos: point.to_array(),
                normal: normal.to_array(),
                uv,
                color: [shade, shade, shade, 0.],
                material: material(surface.layer, surface.layer, 0),
            });
            if j + 1 < rings.len() && k < SIDES {
                let i = base + (j * (SIDES + 1) + k) as u32;
                let next = i + SIDES as u32 + 1;
                mesh.indices.extend([i, i + 1, next, i + 1, next + 1, next]);
            }
        }
    }
    for (ring, front) in [(0, false), (rings.len() - 1, true)] {
        let first = base + (ring * (SIDES + 1)) as u32;
        for k in 1..SIDES as u32 - 1 {
            let face = [first, first + k, first + k + 1];
            mesh.indices.extend(if front {
                face
            } else {
                [face[0], face[2], face[1]]
            });
        }
    }
}

/// A slightly convex number board (or fluted lens) facing forward.
fn plate(mesh: &mut MeshData, center: Vec3, width: f32, height: f32, surface: Surface) {
    let base = mesh.vertices.len() as u32;
    for y in 0..=4 {
        let v = y as f32 / 4.;
        for x in 0..=6 {
            let u = x as f32 / 6.;
            let side = 2. * u - 1.;
            let p = center
                + vec3(
                    side * width * (0.78 + 0.22 * (v * std::f32::consts::PI).sin()),
                    height * (1. - 2. * v),
                    0.038 * (1. - side * side) + 0.045 * v,
                );
            mesh.vertices.push(Vertex {
                pos: p.to_array(),
                normal: vec3(0.3 * side, 0.13, 1.).normalize().to_array(),
                uv: surface.uv(u, v),
                color: [0.92, 0.92, 0.92, 0.],
                material: material(surface.layer, surface.layer, 0),
            });
            if x < 6 && y < 4 {
                let i = base + y * 7 + x;
                mesh.indices.extend([i, i + 7, i + 1, i + 1, i + 7, i + 8]);
            }
        }
    }
}

fn shock(mesh: &mut MeshData, a: Vec3, b: Vec3, paint: Color) {
    chassis_tube(mesh, a, b, 0.026, STEEL);
    let axis = (b - a).normalize();
    let tangent = axis.any_orthonormal_vector();
    let normal = axis.cross(tangent);
    let mut prev = a + tangent * 0.052;
    for i in 1..=48 {
        let t = i as f32 / 48.;
        let (s, c) = (t * TAU * 6.).sin_cos();
        let next = a.lerp(b, t) + (tangent * c + normal * s) * 0.052;
        chassis_tube(mesh, prev, next, 0.009, paint);
        prev = next;
    }
}

/// The seated rider's local origin is the North Star seat socket.
pub(super) fn mount_rider(body: &mut MeshData, rider: &MeshData, seat: Vec3, grip_width: f32) {
    let mut mesh = rider.clone();
    for v in &mut mesh.vertices {
        let hand = v.joint_weights()[0];
        let mut pos = Vec3::from_array(v.pos);
        pos.x *= 1. + hand * (grip_width - 1.);
        v.pos = (seat + pos).to_array();
    }
    body.append(&mesh);
}

impl TrailModel {
    pub(super) fn scale(self) -> f32 {
        if self == Self::Jackrabbit { 0.86 } else { 0.87 }
    }
    pub(super) fn axle(self) -> f32 {
        if self == Self::Ridgeback { 0.91 } else { 0.84 }
    }

    pub fn parts(self, rider: &MeshData, layer: u32) -> KartParts {
        let quad = self == Self::Jackrabbit;
        let enduro = self == Self::Ridgeback;
        let paint = match self {
            Self::Jackrabbit => [0.21, 0.69, 0.53, 0.],
            Self::Redline => [0.90, 0.21, 0.12, 0.],
            Self::Ridgeback => [0.89, 0.60, 0.13, 0.],
        };
        let tile = match self {
            Self::Jackrabbit => 0,
            Self::Redline => 1,
            Self::Ridgeback => 2,
        };
        let livery = Surface { layer, tile };
        let fender = Surface {
            layer,
            tile: 12 + tile,
        };
        let saddle = Surface { layer, tile: 8 };
        let radius = if quad {
            0.34
        } else if enduro {
            0.44
        } else {
            0.40
        };
        let axle = self.axle();
        let width = if quad { 0.40 } else { 0.14 };
        let mut parts = KartParts {
            rig: None,
            body: MeshData::default(),
            wheels: Default::default(),
            hubs: [Vec3::ZERO; 4],
            radii: [radius; 4],
            radius,
        };
        let body = &mut parts.body;
        let seat = vec3(0., 0.54, -0.20);
        // Model in design coordinates, then ground and size the complete rolling chassis.
        let scale = self.scale();
        let ground = Vec3::NEG_Y * GROUND_OFFSET;
        let socket = seat + Vec3::Y * 0.075;
        let pivot = vec3(0., 0.64, 0.46);
        let rider_width = if quad { 0.97 } else { 0.84 };
        let mut controls = MeshData::default();
        for k in 0..4 {
            if !quad && k % 2 == 1 {
                continue;
            }
            let x = if quad {
                if k % 2 == 0 { 0.54 } else { -0.54 }
            } else {
                0.
            };
            let z = if k < 2 { axle } else { -axle };
            let hub = vec3(x, radius - GROUND_OFFSET, z);
            parts.hubs[k] = hub;
            parts.wheels[k] =
                super::trail_wheel::wheel(radius, if quad { 0.17 } else { 0.085 }, quad);
            if quad {
                chassis_tube(body, hub, vec3(0., 0.23, z * 0.58), 0.045, STEEL);
                let crown = hub.y + radius + 0.085;
                fairing(
                    body,
                    x,
                    &[
                        [z - 0.37, 0.14, crown - 0.14, crown - 0.09],
                        [z - 0.06, 0.24, crown - 0.055, crown],
                        [z + 0.24, 0.22, crown - 0.075, crown - 0.02],
                        [z + 0.37, 0.13, crown - 0.16, crown - 0.12],
                    ],
                    fender,
                );
            }
        }
        for side in [-1., 1.] {
            let bottom = vec3(side * width * 0.7, -0.14, 0.13);
            let neck = vec3(side * 0.10, 0.64, 0.46);
            let rear = vec3(side * 0.11, radius - GROUND_OFFSET, -axle);
            chassis_tube(body, rear, bottom, 0.055, STEEL);
            chassis_tube(body, bottom, neck, 0.055, paint);
            chassis_tube(body, neck, seat + Vec3::X * side * width, 0.05, paint);
            let foot = vec3(side * 0.24, 0.02, -0.12);
            panel(
                body,
                foot,
                vec3(0.115, 0.022, if quad { 0.29 } else { 0.065 }),
                0.,
                GRAPHITE,
            );
            if quad {
                let outer = vec3(0.49 * side, radius - GROUND_OFFSET, axle);
                chassis_tube(body, vec3(0.17 * side, 0.19, 0.54), outer, 0.029, STEEL);
                shock(
                    body,
                    vec3(0.43 * side, -0.035, 0.80),
                    vec3(0.20 * side, 0.35, 0.63),
                    paint,
                );
                let corner = vec3(0.40 * side, 0.22, 1.12);
                chassis_tube(body, bottom, corner, 0.041, GRAPHITE);
                chassis_tube(body, corner, vec3(0., 0.32, 1.18), 0.042, GRAPHITE);
            } else {
                let end = vec3(side * 0.11, radius - GROUND_OFFSET, axle);
                chassis_tube(&mut controls, neck, end, 0.029, STEEL);
                chassis_tube(
                    &mut controls,
                    neck,
                    neck.lerp(end, 0.54),
                    0.044,
                    [0.67, 0.55, 0.35, 0.],
                );
                chassis_tube(&mut controls, end, end.lerp(neck, 0.31), 0.048, GRAPHITE);
            }
            // Both glove positions come from the posed rider's asymmetric grip sockets.
            let offset = if side > 0. {
                vec3(0.495, 0.237, 0.44)
            } else {
                vec3(-0.43711, 0.286, 0.42663)
            };
            let grip = socket + offset * vec3(rider_width, 1., 1.) / scale;
            let bend = grip - vec3(0.16 * side, 0.032, -0.015);
            chassis_tube(&mut controls, neck, bend, 0.021, STEEL);
            chassis_tube(
                &mut controls,
                bend,
                grip + Vec3::X * side * 0.072,
                0.028,
                RUBBER,
            );
            chassis_tube(
                &mut controls,
                bend,
                grip + vec3(0.05 * side, -0.025, 0.077),
                0.011,
                STEEL,
            );
            if enduro {
                chassis_tube(&mut controls, bend, grip + Vec3::Z * 0.095, 0.02, GRAPHITE);
                panel(
                    &mut controls,
                    grip + Vec3::Z * 0.095,
                    vec3(0.098, 0.042, 0.020),
                    -0.24,
                    paint,
                );
            }
        }
        // Cast crankcase and bolted clutch cover below the narrower finned cylinder.
        let mut case = crate::fx_meshes::sphere(3, [0.35, 0.36, 0.34, 0.], 0);
        let casing = vec3(if quad { 0.24 } else { 0.165 }, 0.15, 0.245);
        for v in &mut case.vertices {
            v.pos = (Vec3::from_array(v.pos) * casing + vec3(0., 0.05, -0.04)).to_array();
            v.normal = (Vec3::from_array(v.normal) / casing).normalize().to_array();
        }
        body.append(&case);
        for side in [-1., 1.] {
            let cover = vec3(side * casing.x * 0.87, 0.055, -0.04);
            let mut cap = crate::fx_meshes::sphere(2, [0.47, 0.46, 0.43, 0.], 0);
            let dims = vec3(0.032, 0.10, 0.115);
            for v in &mut cap.vertices {
                v.pos = (cover + Vec3::from_array(v.pos) * dims).to_array();
                v.normal = (Vec3::from_array(v.normal) / dims).normalize().to_array();
            }
            body.append(&cap);
            for i in 0..6 {
                let (y, z) = (TAU * i as f32 / 6.).sin_cos();
                panel(
                    body,
                    cover + vec3(side * 0.021, y * 0.086, z * 0.10),
                    Vec3::splat(0.009),
                    0.,
                    STEEL,
                );
            }
            let rad = vec3(side * width * 0.90, 0.33, 0.32);
            panel(body, rad, vec3(0.041, 0.13, 0.083), -0.22, GRAPHITE);
            for slot in 0..6 {
                panel(
                    body,
                    rad + vec3(0., slot as f32 * 0.033 - 0.079, 0.074),
                    vec3(0.040, 0.007, 0.022),
                    -0.2,
                    STEEL,
                );
            }
            if quad {
                fairing(
                    body,
                    side * 0.33,
                    &[
                        [0.28, 0.10, 0.32, 0.50],
                        [0.51, 0.24, 0.29, 0.50],
                        [0.80, 0.19, 0.26, 0.39],
                        [0.95, 0.10, 0.23, 0.32],
                    ],
                    livery,
                );
                chassis_tube(
                    body,
                    vec3(side * 0.51, -0.10, -0.55),
                    vec3(side * 0.51, -0.10, 0.39),
                    0.030,
                    STEEL,
                );
            }
        }
        // Radiator shrouds, tapered saddle and upswept rear mudguard.
        fairing(
            body,
            0.,
            &[
                [-0.28, width * 0.74, 0.26, 0.54],
                [0.12, width + 0.055, 0.27, 0.63],
                [0.36, width + 0.075, 0.18, 0.67],
                [0.51, width * 0.72, 0.38, 0.56],
            ],
            livery,
        );
        fairing(
            body,
            0.,
            &[
                [-0.63, width * 0.67, 0.54, 0.64],
                [-0.23, width, 0.48, 0.615],
                [0.12, width * 0.78, 0.54, 0.64],
            ],
            saddle,
        );
        fairing(
            body,
            0.,
            &[
                [-1.09, width * 0.44, 0.65, 0.685],
                [-0.65, width, 0.51, 0.62],
                [-0.32, width * 1.08, 0.35, 0.53],
            ],
            fender,
        );
        fairing(
            if quad { body } else { &mut controls },
            0.,
            &[
                [0.52, width * 0.70, 0.39, 0.46],
                [0.73, width * 1.10, 0.54, 0.61],
                [1.04, width, 0.52, 0.57],
                [1.19, width * 0.49, 0.46, 0.49],
            ],
            fender,
        );
        for fin in 0..7 {
            panel(
                body,
                vec3(0., 0.13 + fin as f32 * 0.023, 0.07 + fin as f32 * 0.008),
                vec3(0.12, 0.009, 0.10),
                0.15,
                GRAPHITE,
            );
        }
        let silencer = vec3(width + 0.045, 0.39, -0.51);
        chassis_tube(body, vec3(width, 0.04, 0.17), silencer, 0.040, STEEL);
        chassis_tube(
            body,
            silencer,
            vec3(width + 0.045, 0.47, -0.87),
            0.074,
            STEEL,
        );
        if quad {
            for side in [-1., 1.] {
                let housing = vec3(0.22 * side, 0.43, 0.82);
                panel(body, housing, vec3(0.13, 0.074, 0.085), -0.18, GRAPHITE);
                plate(
                    body,
                    housing + Vec3::Z * 0.08,
                    0.112,
                    0.053,
                    Surface { layer, tile: 15 },
                );
            }
        }
        let board = if quad {
            body as &mut MeshData
        } else {
            &mut controls
        };
        let center = if quad {
            vec3(0., 0.42, 0.98)
        } else {
            vec3(0., 0.76, 0.53)
        };
        plate(
            board,
            center,
            if quad { 0.155 } else { 0.17 },
            0.145,
            Surface {
                layer,
                tile: 4 + tile,
            },
        );
        if enduro {
            let lamp = center + vec3(0., -0.19, 0.055);
            panel(board, lamp, vec3(0.132, 0.083, 0.038), -0.16, GRAPHITE);
            plate(
                board,
                lamp + Vec3::Z * 0.039,
                0.116,
                0.068,
                Surface { layer, tile: 15 },
            );
        }
        for v in &mut controls.vertices {
            v.set_joint_weights(1., 0.);
        }
        body.append(&controls);
        for v in &mut body.vertices {
            v.pos = (ground + (Vec3::from_array(v.pos) - ground) * scale).to_array();
        }
        for (wheel, hub) in parts.wheels.iter_mut().zip(&mut parts.hubs) {
            for v in &mut wheel.vertices {
                v.pos = (Vec3::from_array(v.pos) * scale).to_array();
            }
            *hub = ground + (*hub - ground) * scale;
        }
        parts.radius *= scale;
        parts.radii.fill(parts.radius);
        let seat = ground + (socket - ground) * scale;
        let axis = if quad {
            Vec3::Y
        } else {
            (pivot - vec3(0., radius - GROUND_OFFSET, axle)).normalize()
        };
        parts.rig = Some(super::SteeringRig {
            seat,
            pivot: ground + (pivot - ground) * scale,
            axis,
            exhaust: ground + (vec3(width + 0.045, 0.47, -0.87) - ground) * scale,
            limit: if quad { 0.38 } else { 0.48 },
        });
        mount_rider(body, rider, seat, rider_width);
        parts
    }
}
