//! Set-piece props and scatter around the circuit: the skull rock, the refinery, wrecks,
//! cacti and dead trees.

use std::path::Path;

use glam::{Mat4, Vec3};
use rivet_assets::{GlbError, Rgba8Image, load_glb};
use rivet_render::{MeshData, Vertex, material};

use crate::terrain;
use crate::track::Track;

/// A prop model: file stem, typical size (largest dimension, m), how much of its size must stay
/// clear of the road, and its shading.
pub struct PropKind {
    pub name: &'static str,
    pub size: f32,
    /// Fraction of `size` used as the clearance radius from the road.
    pub footprint: f32,
    pub specular: f32,
}

pub const PROPS: [PropKind; 10] = [
    PropKind {
        name: "skull_rock",
        size: 55.0,
        footprint: 0.5,
        specular: 0.04,
    },
    PropKind {
        name: "plane_wreck",
        size: 26.0,
        footprint: 0.5,
        specular: 0.25,
    },
    PropKind {
        name: "fuel_tanks",
        size: 22.0,
        footprint: 0.5,
        specular: 0.3,
    },
    PropKind {
        name: "pumpjack",
        size: 12.0,
        footprint: 0.5,
        specular: 0.3,
    },
    PropKind {
        name: "water_tower",
        size: 18.0,
        footprint: 0.3,
        specular: 0.3,
    },
    PropKind {
        name: "bus_wreck",
        size: 11.0,
        footprint: 0.5,
        specular: 0.25,
    },
    PropKind {
        name: "wreck_car",
        size: 5.5,
        footprint: 0.5,
        specular: 0.25,
    },
    PropKind {
        name: "barrel_stack",
        size: 3.5,
        footprint: 0.5,
        specular: 0.2,
    },
    PropKind {
        name: "cactus",
        size: 6.0,
        footprint: 0.3,
        specular: 0.05,
    },
    PropKind {
        name: "dead_tree",
        size: 8.0,
        footprint: 0.4,
        specular: 0.03,
    },
];

fn kind(name: &str) -> usize {
    PROPS
        .iter()
        .position(|p| p.name == name)
        .expect("known prop")
}

/// One placed prop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub kind: usize,
    pub pos: Vec3,
    pub yaw: f32,
    /// Largest dimension (m).
    pub size: f32,
}

impl Placement {
    pub fn model(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            Vec3::splat(self.size),
            glam::Quat::from_rotation_y(self.yaw),
            self.pos,
        )
    }
}

/// A point `lateral` meters to the right of centerline point `at` (a fraction of the lap).
fn beside(track: &Track, at: f32, lateral: f32) -> Vec3 {
    let cl = &track.centerline;
    let n = cl.len() - 1;
    let i = ((at.rem_euclid(1.0)) * n as f32) as usize % n;
    let f = (cl[(i + 1) % n] - cl[(i + n - 1) % n]).normalize();
    let right = f.cross(Vec3::Y).normalize();
    Vec3::new(cl[i].x, 0.0, cl[i].z) + right * lateral
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Every prop on the circuit, deterministic.
pub fn placements(track: &Track) -> Vec<Placement> {
    let cl = &track.centerline;
    let mut out = Vec::new();
    let mut put = |name: &str, p: Vec3, yaw: f32, scale: f32| {
        let k = kind(name);
        let y = terrain::height_at(cl, p.x, p.z);
        out.push(Placement {
            kind: k,
            pos: Vec3::new(p.x, y - 0.2, p.z),
            yaw,
            size: PROPS[k].size * scale,
        });
    };
    // The landmark: a horned skull of sandstone looming over the infield.
    let center = cl.iter().fold(Vec3::ZERO, |a, p| a + *p) / cl.len() as f32;
    put(
        "skull_rock",
        Vec3::new(center.x - 10.0, 0.0, center.z + 25.0),
        3.6,
        1.0,
    );
    // Refinery outside the banked turn: tanks, pumpjacks nodding, barrels.
    put("fuel_tanks", beside(track, 0.33, -48.0), 0.4, 1.0);
    put("fuel_tanks", beside(track, 0.37, -62.0), 1.1, 0.8);
    put("pumpjack", beside(track, 0.29, -30.0), 2.0, 1.0);
    put("pumpjack", beside(track, 0.31, -40.0), 2.3, 0.9);
    put("pumpjack", beside(track, 0.41, -34.0), 1.7, 1.1);
    put("barrel_stack", beside(track, 0.335, -16.0), 0.3, 1.0);
    put("barrel_stack", beside(track, 0.36, -18.0), 1.9, 1.2);
    // A cargo plane that came down on the dunes beyond the back straight.
    put("plane_wreck", beside(track, 0.6, -46.0), 0.9, 1.0);
    // Start/finish: a water tower and barrel stacks by the gantry.
    put("water_tower", beside(track, 0.985, 22.0), 0.2, 1.0);
    put("barrel_stack", beside(track, 0.003, 11.5), 0.8, 1.0);
    put("barrel_stack", beside(track, 0.006, -11.5), 2.4, 0.9);
    // Wrecks along the straights.
    put("bus_wreck", beside(track, 0.12, 17.0), 1.3, 1.0);
    put("wreck_car", beside(track, 0.16, -13.0), 0.4, 1.0);
    put("wreck_car", beside(track, 0.52, 14.0), 2.7, 1.0);
    put("wreck_car", beside(track, 0.7, 13.5), 4.0, 0.95);
    put("wreck_car", beside(track, 0.88, -14.0), 5.4, 1.05);
    put("bus_wreck", beside(track, 0.78, -22.0), 3.9, 1.0);
    put("barrel_stack", beside(track, 0.09, 12.0), 1.0, 1.0);
    put("barrel_stack", beside(track, 0.6, 12.0), 0.2, 1.1);

    // Scatter: cacti and dead trees across the dunes, clear of the road and set pieces.
    let set_pieces: Vec<(Vec3, f32)> = out.iter().map(|p| (p.pos, p.size * 0.6)).collect();
    let mut rng = Rng(0xdead_7ee5);
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in cl {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let mut tries = 0;
    let mut scattered = 0;
    while scattered < 46 && tries < 6000 {
        tries += 1;
        let x = lo.x - 120.0 + rng.next() * (hi.x - lo.x + 240.0);
        let z = lo.z - 120.0 + rng.next() * (hi.z - lo.z + 240.0);
        let d = terrain::distance_to(cl, x, z);
        let name = if rng.next() < 0.6 {
            "cactus"
        } else {
            "dead_tree"
        };
        let size = PROPS[kind(name)].size * (0.7 + rng.next() * 0.6);
        let clear = crate::track::ROAD_HALF_WIDTH + 4.0 + size * PROPS[kind(name)].footprint;
        if d < clear || d > 150.0 {
            continue;
        }
        let p = Vec3::new(x, 0.0, z);
        if set_pieces
            .iter()
            .any(|(q, r)| Vec3::new(q.x, 0.0, q.z).distance(p) < r + 4.0)
        {
            continue;
        }
        let k = kind(name);
        let y = terrain::height_at(cl, x, z);
        out.push(Placement {
            kind: k,
            pos: Vec3::new(x, y - 0.2, z),
            yaw: rng.next() * std::f32::consts::TAU,
            size,
        });
        scattered += 1;
    }
    out
}

/// Loads a prop GLB normalized to unit size (largest dimension 1, centered on X/Z, base at
/// y = 0) and textured from layer `layer`.
pub fn load_prop(
    assets: &Path,
    name: &str,
    layer: u32,
) -> Result<(MeshData, Option<Rgba8Image>), GlbError> {
    let g = load_glb(
        &assets
            .join("models")
            .join("props")
            .join(format!("{name}.glb")),
    )?;
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in &g.positions {
        lo = lo.min(Vec3::from_array(*p));
        hi = hi.max(Vec3::from_array(*p));
    }
    let size = (hi - lo).max_element().max(1e-6);
    let offset = Vec3::new((lo.x + hi.x) * 0.5, lo.y, (lo.z + hi.z) * 0.5);
    let mat = material(layer, layer, 0);
    let mut m = MeshData::default();
    for i in 0..g.positions.len() {
        m.vertices.push(Vertex {
            pos: ((Vec3::from_array(g.positions[i]) - offset) / size).to_array(),
            normal: g.normals[i],
            uv: g.uvs[i],
            color: [1.0, 1.0, 1.0, 0.0],
            material: mat,
        });
    }
    m.indices = g.indices.clone();
    Ok((m, g.base_color))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{ROAD_HALF_WIDTH, build_graybox_loop};

    #[test]
    fn props_stay_off_the_road() {
        let t = build_graybox_loop();
        let placed = placements(&t);
        assert!(placed.len() >= 50, "only {} props", placed.len());
        for p in &placed {
            let d = crate::terrain::distance_to(&t.centerline, p.pos.x, p.pos.z);
            let need = ROAD_HALF_WIDTH + 2.0 + p.size * PROPS[p.kind].footprint;
            assert!(
                d > need,
                "{} at {} is {d:.1} m from the road (needs {need:.1})",
                PROPS[p.kind].name,
                p.pos
            );
        }
    }

    #[test]
    fn every_kind_is_used_and_landmark_is_big() {
        let t = build_graybox_loop();
        let placed = placements(&t);
        for (k, kind) in PROPS.iter().enumerate() {
            assert!(placed.iter().any(|p| p.kind == k), "{} unused", kind.name);
        }
        let skull = placed
            .iter()
            .find(|p| PROPS[p.kind].name == "skull_rock")
            .unwrap();
        assert!(skull.size >= 40.0);
    }

    #[test]
    fn placement_is_deterministic() {
        let t = build_graybox_loop();
        let (a, b) = (placements(&t), placements(&t));
        assert_eq!(a.len(), b.len());
        assert!(
            a.iter()
                .zip(&b)
                .all(|(x, y)| x.pos == y.pos && x.yaw == y.yaw)
        );
    }
}
