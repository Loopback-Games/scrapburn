//! Hand-built routes and procedural landmarks for Glass Orchard and Helios Crown.
use super::{TrackDef, TrackId, TrackStyle, emissive_box, quad_colors, textured_box};
use crate::materials::Layer;
use crate::terrain::{self, TerrainKind};
use glam::{Mat4, Quat, Vec2, Vec3};
use rivet_render::{EMISSIVE, MeshData, UNTEXTURED, material};
use std::f32::consts::TAU;

pub(super) const ORCHARD: TrackDef = TrackDef {
    id: TrackId::Orchard,
    name: "GLASS ORCHARD",
    tagline: "THREAD THE NEEDLES IN THE CRYSTAL GROVE.",
    points: &[
        Vec2::new(0., 0.),
        Vec2::new(115., 0.),
        Vec2::new(172., -32.),
        Vec2::new(230., -14.),
        Vec2::new(258., 52.),
        Vec2::new(224., 98.),
        Vec2::new(145., 100.),
        Vec2::new(110., 144.),
        Vec2::new(148., 186.),
        Vec2::new(210., 212.),
        Vec2::new(191., 268.),
        Vec2::new(102., 286.),
        Vec2::new(37., 241.),
        Vec2::new(-32., 225.),
        Vec2::new(-69., 165.),
        Vec2::new(-42., 99.),
        Vec2::new(-72., 47.),
        Vec2::new(-55., 9.),
    ],
    height: |_, t| 2.6 - 2.4 * (TAU * t * 2.).cos(),
    bank: None,
    bank_degrees: 9.,
    pads: &[0.09, 0.46, 0.79],
    box_rows: &[0.17, 0.57, 0.87],
    style: TrackStyle {
        road: Layer::Flagstone,
        road_tint: [0.54, 0.71, 0.77, 0.],
        road_tex: 4.,
        road_edge: Some(Layer::Rock),
        groove: 0.82,
        curbs: ([0.12, 0.42, 0.39, 0.], [0.52, 0.30, 0.60, 0.]),
        curb_glow: false,
        barrier_low: Layer::StoneWall,
        barrier_high: Layer::Flagstone,
        barrier_tint: [0.43, 0.56, 0.62, 0.],
        barrier_height: 0.85,
        crenellations: false,
        rail_glow: Some([0.18, 0.64, 0.53]),
        torches: false,
        gantry: Layer::Flagstone,
        gantry_glow: [0.52, 1., 0.81],
        pad_glow: ([0.08, 0.88, 0.56, 0.], [0.63, 1., 0.81, 0.]),
        terrain: TerrainKind::Hills,
    },
};

pub(super) const HELIOS: TrackDef = TrackDef {
    id: TrackId::Helios,
    name: "HELIOS CROWN",
    tagline: "CHASE THE SUN THROUGH THE TRANSIT RINGS.",
    points: &[
        Vec2::new(0., 0.),
        Vec2::new(145., 0.),
        Vec2::new(255., 22.),
        Vec2::new(318., 94.),
        Vec2::new(296., 168.),
        Vec2::new(240., 216.),
        Vec2::new(237., 283.),
        Vec2::new(170., 330.),
        Vec2::new(86., 312.),
        Vec2::new(55., 250.),
        Vec2::new(0., 217.),
        Vec2::new(-84., 234.),
        Vec2::new(-144., 169.),
        Vec2::new(-125., 97.),
        Vec2::new(-67., 34.),
    ],
    height: |_, t| 52. + 10. * (TAU * t).sin() - 5. * (TAU * t * 3.).cos(),
    bank: None,
    bank_degrees: 19.,
    pads: &[0.045, 0.26, 0.52, 0.76],
    box_rows: &[0.14, 0.39, 0.67, 0.88],
    style: TrackStyle {
        road: Layer::Asphalt,
        road_tint: [0.68, 0.73, 0.70, 0.],
        road_tex: 6.,
        road_edge: Some(Layer::TechPanel),
        groove: 0.84,
        curbs: ([0.87, 0.50, 0.12, 0.], [0.05, 0.24, 0.26, 0.]),
        curb_glow: false,
        barrier_low: Layer::MetalRust,
        barrier_high: Layer::TechPanel,
        barrier_tint: [0.69, 0.56, 0.34, 0.],
        barrier_height: 0.95,
        crenellations: false,
        rail_glow: Some([1., 0.55, 0.09]),
        torches: false,
        gantry: Layer::MetalRust,
        gantry_glow: [1., 0.64, 0.17],
        pad_glow: ([1., 0.43, 0.06, 0.], [1., 0.88, 0.43, 0.]),
        terrain: TerrainKind::Void,
    },
};

// Each prismatic needle has a dark foot, a saturated shoulder and a pale luminous tip.
// Varying the shoulder by face preserves the cut-glass silhouette in shadow.
fn crystal(mesh: &mut MeshData, frame: Mat4, radius: f32, height: f32, hue: Vec3) {
    for face in 0..6 {
        let edge = |i: i32| {
            let a = TAU * i as f32 / 6.;
            Vec3::new(a.cos(), 0., a.sin()) * radius
        };
        let a = edge(face);
        let b = edge(face + 1);
        let shoulder = Vec3::Y * height * 0.73;
        let tip = Vec3::Y * height;
        let facet = hue * [0.52, 0.84, 0.64, 1., 0.58, 0.91][face as usize];
        let foot = (facet * 0.15).extend(0.).to_array();
        let mid = facet.extend(0.).to_array();
        let peak = facet.lerp(Vec3::ONE, 0.57).extend(0.).to_array();
        let mut surface = |points: [Vec3; 4], colors| {
            quad_colors(
                mesh,
                points.map(|p| frame.transform_point3(p)),
                [[0.; 2]; 4],
                colors,
                material(0, 0, UNTEXTURED | EMISSIVE),
            )
        };
        surface(
            [b * 0.68, a * 0.68, a + shoulder, b + shoulder],
            [foot, foot, mid, mid],
        );
        surface(
            [b + shoulder, a + shoulder, tip, tip],
            [mid, mid, peak, peak],
        );
    }
}

pub(super) fn crystal_gate(mesh: &mut MeshData, frame: Mat4) {
    for side in [-1., 1.] {
        let root = frame * Mat4::from_translation(Vec3::new(side * 14., -2., 0.));
        crystal(
            mesh,
            root * Mat4::from_rotation_z(side * 0.46),
            2.0,
            23.,
            Vec3::new(0.12, 0.67, 0.53),
        );
        crystal(
            mesh,
            root * Mat4::from_rotation_z(-side * 0.28),
            1.7,
            12.,
            Vec3::new(0.54, 0.23, 0.62),
        );
    }
}

pub(super) fn groves(mesh: &mut MeshData, line: &[Vec3]) {
    for (k, i) in (0..line.len() - 1).step_by(15).enumerate() {
        let p = line[i];
        let right = (line[i + 1] - p).cross(Vec3::Y).normalize();
        let side = if k % 2 == 0 { 1. } else { -1. };
        let pos = p + right * side * (24. + (k * 17 % 43) as f32);
        if terrain::distance_to(line, pos.x, pos.z) < 21. {
            continue;
        }
        let base = Vec3::new(
            pos.x,
            terrain::height_for(TerrainKind::Hills, line, pos.x, pos.z) - 2.,
            pos.z,
        );
        let hue = [
            Vec3::new(0.11, 0.67, 0.61),
            Vec3::new(0.56, 0.21, 0.66),
            Vec3::new(0.18, 0.43, 0.76),
        ][k % 3];
        for j in 0..5 {
            let angle = j as f32 * 2.4 + k as f32;
            let frame = Mat4::from_translation(base)
                * Mat4::from_rotation_y(angle)
                * Mat4::from_translation(Vec3::X * j as f32 * 0.85)
                * Mat4::from_rotation_z(-0.13 * j as f32);
            let height = (15. + (k * 7 % 19) as f32) / (1. + j as f32 * 0.30);
            crystal(mesh, frame, height * 0.11, height, hue);
        }
    }
}

// Segments share endpoints so the hoops read as continuous cast brass at speed.
fn hoop(mesh: &mut MeshData, frame: Mat4, radius: f32, width: f32) {
    let point = |i: usize| {
        let angle = TAU * i as f32 / 48.;
        Vec3::new(angle.cos(), angle.sin(), 0.) * radius
    };
    for i in 0..48 {
        let (a, b) = (point(i), point(i + 1));
        let xf = frame
            * Mat4::from_rotation_translation(
                Quat::from_rotation_arc(Vec3::Y, (b - a).normalize()),
                (a + b) * 0.5,
            );
        textured_box(
            mesh,
            Vec3::new(width, a.distance(b) * 0.52, width),
            xf,
            Layer::MetalRust,
            [0.95, 0.66, 0.35],
        );
        emissive_box(
            mesh,
            Vec3::new(width * 0.32, a.distance(b) * 0.51, width * 1.04),
            xf,
            [1., 0.58, 0.13],
        );
    }
}

pub(super) fn transit_ring(mesh: &mut MeshData, frame: Mat4) {
    let dial = frame * Mat4::from_translation(Vec3::Y * 2.) * Mat4::from_rotation_x(0.23);
    hoop(mesh, dial, 13., 0.44);
    for hour in 0..12 {
        let spoke = dial
            * Mat4::from_rotation_z(TAU * hour as f32 / 12.)
            * Mat4::from_translation(Vec3::Y * 13.9);
        textured_box(
            mesh,
            Vec3::new(0.18, 1.25, 0.45),
            spoke,
            Layer::TechPanel,
            [0.47, 0.67, 0.66],
        );
        emissive_box(
            mesh,
            Vec3::new(0.20, 0.18, 0.47),
            spoke * Mat4::from_translation(Vec3::Y * 1.25),
            [1., 0.82, 0.37],
        );
    }
}

pub(super) fn orrery(mesh: &mut MeshData) {
    // The route curls around this fixed point. Its nested orbits resolve into an
    // armillary sphere as the road swings around to the opposite side.
    let center = Mat4::from_translation(Vec3::new(109., 92., 133.));
    for (tilt, yaw, radius) in [(0.24, 0.1, 50.), (0.92, 1.2, 46.), (-0.65, 2.05, 42.)] {
        hoop(
            mesh,
            center * Mat4::from_rotation_y(yaw) * Mat4::from_rotation_x(tilt),
            radius,
            1.1,
        );
    }
    // Mirror banks rise from the cloud layer, with each tile angled toward the receiver.
    for (x, z, h) in [
        (190., 44., 78.),
        (222., 62., 85.),
        (241., 91., 91.),
        (192., 251., 77.),
        (-77., 174., 68.),
        (-47., 150., 83.),
    ] {
        let hub = Vec3::new(x, h, z);
        let receiver = center.transform_point3(Vec3::ZERO);
        let bearing = (receiver.x - x).atan2(receiver.z - z);
        let dish = Mat4::from_translation(hub)
            * Mat4::from_rotation_y(bearing)
            * Mat4::from_rotation_x(-0.42);
        textured_box(
            mesh,
            Vec3::new(1.25, 55., 1.25),
            Mat4::from_translation(hub - Vec3::Y * 55.),
            Layer::TechPanel,
            [0.35, 0.49, 0.51],
        );
        for row in -2..=2_i32 {
            for col in -3..=3_i32 {
                if row.abs() + col.abs() > 4 {
                    continue;
                }
                let tile =
                    dish * Mat4::from_translation(Vec3::new(
                        col as f32 * 3.65,
                        row as f32 * 3.65,
                        (col * col + row * row) as f32 * 0.18,
                    )) * Mat4::from_rotation_y(-0.10 * col as f32)
                        * Mat4::from_rotation_x(0.10 * row as f32);
                textured_box(
                    mesh,
                    Vec3::new(1.8, 1.8, 0.24),
                    tile,
                    Layer::TechPanel,
                    [0.70, 0.52, 0.27],
                );
                let glint = 0.09 * (col + row + 5) as f32;
                emissive_box(
                    mesh,
                    Vec3::new(1.64, 1.64, 0.25),
                    tile,
                    [0.08 + 0.3 * glint, 0.21 + glint * 0.54, 0.29 + glint * 0.5],
                );
            }
        }
    }
    let core = Vec3::new(1., 0.57, 0.09);
    crystal(
        mesh,
        center * Mat4::from_translation(-Vec3::Y * 11.),
        10.,
        22.,
        core,
    );
    textured_box(
        mesh,
        Vec3::new(2.8, 64., 2.8),
        center * Mat4::from_translation(-Vec3::Y * 76.),
        Layer::TechPanel,
        [0.55, 0.51, 0.36],
    );
}
