//! The garage service bay is one static mesh. Only the displayed kart rotates.
use glam::{Quat, Vec3};
use rivet_render::{EMISSIVE, Lighting, MeshData, UNTEXTURED, box_mesh, material};
use scrapburn::materials::Layer;

pub fn lighting() -> Lighting {
    Lighting {
        sun_dir: Vec3::new(-0.4, -0.82, -0.32).normalize(),
        sun_color: Vec3::new(2.6, 2.55, 2.4),
        sky_zenith: Vec3::new(0.24, 0.36, 0.51),
        ambient_ground: Vec3::new(0.28, 0.30, 0.33),
        fog_density: 0.0,
        ..Lighting::default()
    }
}

pub fn service_bay() -> MeshData {
    let mut mesh = MeshData::default();
    let steel = material(Layer::TechPanel as u32, 0, 0);
    let concrete = material(Layer::Concrete as u32, 0, 0);
    let light = material(0, 0, UNTEXTURED | EMISSIVE);
    let metal = [0.40, 0.51, 0.57, 0.0];
    let cyan = [0.16, 0.86, 0.78, 0.0];
    let amber = [1.0, 0.54, 0.11, 0.0];
    let mut part = |position: Vec3, half: Vec3, yaw: f32, tint, surface| {
        let rotation = Quat::from_rotation_y(yaw);
        let mut piece = box_mesh(half, tint);
        for v in &mut piece.vertices {
            let p = Vec3::from_array(v.pos);
            let normal = Vec3::from_array(v.normal);
            v.uv = if normal.y != 0.0 {
                [p.x, p.z]
            } else if normal.z != 0.0 {
                [p.x, p.y]
            } else {
                [p.z, p.y]
            };
            v.pos = (position + rotation * p).to_array();
            v.normal = (rotation * normal).to_array();
            v.material = surface;
        }
        mesh.append(&piece);
    };
    part(
        Vec3::new(0., -0.33, 0.),
        Vec3::new(14., 0.12, 12.),
        0.,
        metal,
        concrete,
    );
    part(
        Vec3::new(0., -0.10, 0.),
        Vec3::new(2.65, 0.10, 2.6),
        0.,
        [0.71, 0.75, 0.76, 0.],
        steel,
    );
    for index in 0..60 {
        let angle = std::f32::consts::TAU * index as f32 / 60.;
        let position = Vec3::new(angle.sin() * 2.24, 0.004, angle.cos() * 2.24);
        part(
            position,
            Vec3::new(0.103, 0.004, if index % 5 == 0 { 0.046 } else { 0.016 }),
            angle,
            if index % 5 == 0 { amber } else { cyan },
            light,
        );
    }
    // Service shutters, columns and overhead inspection strips.
    for bay in -3..=3 {
        let x = bay as f32 * 3.0;
        part(
            Vec3::new(x, 2.3, -5.2),
            Vec3::new(1.49, 2.6, 0.14),
            0.,
            metal,
            steel,
        );
        for slat in 0..10 {
            part(
                Vec3::new(x, 0.18 + slat as f32 * 0.31, -5.02),
                Vec3::new(1.28, 0.019, 0.065),
                0.,
                metal,
                steel,
            );
        }
        part(
            Vec3::new(x + 1.44, 2.2, -4.85),
            Vec3::new(0.12, 2.5, 0.25),
            0.,
            metal,
            steel,
        );
        part(
            Vec3::new(x, 3.55, -4.91),
            Vec3::new(1.15, 0.041, 0.06),
            0.,
            cyan,
            light,
        );
        part(
            Vec3::new(x + 1.44, 0.75, -4.58),
            Vec3::new(0.065, 0.28, 0.015),
            0.,
            amber,
            light,
        );
    }
    part(
        Vec3::new(-7., 2., 0.),
        Vec3::new(0.12, 3., 6.),
        0.,
        metal,
        steel,
    );
    for z in -3..=2 {
        part(
            Vec3::new(-6.83, 2.1, z as f32 * 1.8),
            Vec3::new(0.035, 1.8, 0.055),
            0.,
            cyan,
            light,
        );
    }
    mesh
}
