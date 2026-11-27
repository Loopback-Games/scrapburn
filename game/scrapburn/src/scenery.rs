//! Set-piece props and scatter around each circuit: the Dustbowl's skull rock, refinery and
//! wrecks; Grindstone's towers, gatehouse and banners; Skyway's neon arches and pylons.

use std::path::Path;

use glam::{Mat4, Vec3};
use rivet_assets::{GlbError, Rgba8Image, load_glb};
use rivet_render::{MeshData, Vertex, material};

use crate::terrain::{self, TerrainKind};
use crate::track::{Solid, Track, TrackId};

/// A prop model: file stem, typical size (largest dimension, m), how much of its size must stay
/// clear of the road, and its shading.
pub struct PropKind {
    pub name: &'static str,
    pub size: f32,
    /// Fraction of `size` used as the clearance radius from the road.
    pub footprint: f32,
    pub specular: f32,
    /// Self-lit brightness (neon), 0 = lit by the sun.
    pub glow: f32,
    /// The parts a kart bumps off, in model space; empty for props nothing can hit.
    pub solid: &'static [Block],
}

impl PropKind {
    /// A sunlit landmark with half its largest dimension reserved for road clearance.
    const fn landmark(
        name: &'static str,
        size: f32,
        specular: f32,
        solid: &'static [Block],
    ) -> Self {
        Self {
            name,
            size,
            footprint: 0.5,
            specular,
            glow: 0.0,
            solid,
        }
    }
}

/// An upright box of a prop that karts collide with, in the normalized model space of
/// `load_prop` (largest dimension 1, centered on X/Z, base at y = 0). A little inside the
/// visible model, so a scrape that looks clear is clear.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Block {
    /// Center on the ground (x, z).
    pub center: [f32; 2],
    /// Half extents (x, z).
    pub half: [f32; 2],
    pub height: f32,
}

impl Block {
    const fn new(x: f32, z: f32, half_x: f32, half_z: f32, height: f32) -> Self {
        Self {
            center: [x, z],
            half: [half_x, half_z],
            height,
        }
    }
}

pub const PROPS: [PropKind; 23] = [
    PropKind {
        name: "skull_rock",
        size: 55.0,
        footprint: 0.5,
        specular: 0.04,
        glow: 0.0,
        solid: &[Block::new(0.0, 0.0, 0.34, 0.44, 0.9)],
    },
    PropKind {
        name: "plane_wreck",
        size: 26.0,
        footprint: 0.5,
        specular: 0.25,
        glow: 0.0,
        solid: &[
            Block::new(0.05, 0.0, 0.43, 0.09, 0.3),
            Block::new(0.12, 0.0, 0.12, 0.3, 0.1),
        ],
    },
    PropKind {
        name: "fuel_tanks",
        size: 22.0,
        footprint: 0.5,
        specular: 0.3,
        glow: 0.0,
        solid: &[Block::new(0.0, 0.0, 0.41, 0.48, 0.7)],
    },
    PropKind {
        name: "pumpjack",
        size: 12.0,
        footprint: 0.5,
        specular: 0.3,
        glow: 0.0,
        solid: &[Block::new(0.0, 0.02, 0.48, 0.13, 0.55)],
    },
    PropKind {
        name: "water_tower",
        size: 18.0,
        footprint: 0.3,
        specular: 0.3,
        glow: 0.0,
        solid: &[
            Block::new(-0.15, 0.0, 0.035, 0.035, 0.45),
            Block::new(0.15, 0.0, 0.035, 0.035, 0.45),
            Block::new(0.0, -0.15, 0.035, 0.035, 0.45),
            Block::new(0.0, 0.15, 0.035, 0.035, 0.45),
        ],
    },
    PropKind {
        name: "bus_wreck",
        size: 11.0,
        footprint: 0.5,
        specular: 0.25,
        glow: 0.0,
        solid: &[Block::new(0.0, 0.0, 0.48, 0.2, 0.42)],
    },
    PropKind {
        name: "wreck_car",
        size: 5.5,
        footprint: 0.5,
        specular: 0.25,
        glow: 0.0,
        solid: &[Block::new(0.0, 0.0, 0.47, 0.19, 0.27)],
    },
    PropKind {
        name: "barrel_stack",
        size: 3.5,
        footprint: 0.5,
        specular: 0.2,
        glow: 0.0,
        solid: &[Block::new(0.0, 0.0, 0.3, 0.4, 0.9)],
    },
    PropKind {
        name: "cactus",
        size: 6.0,
        footprint: 0.3,
        specular: 0.05,
        glow: 0.0,
        solid: &[Block::new(0.01, -0.03, 0.08, 0.08, 1.0)],
    },
    PropKind {
        name: "dead_tree",
        size: 8.0,
        footprint: 0.4,
        specular: 0.03,
        glow: 0.0,
        solid: &[Block::new(0.0, -0.02, 0.07, 0.08, 0.9)],
    },
    PropKind {
        name: "castle_tower",
        size: 28.0,
        footprint: 0.4,
        specular: 0.05,
        glow: 0.0,
        solid: &[Block::new(0.0, 0.0, 0.31, 0.31, 1.0)],
    },
    PropKind {
        name: "gatehouse",
        size: 36.0,
        footprint: 0.45,
        specular: 0.05,
        glow: 0.0,
        solid: &[Block::new(0.0, 0.0, 0.42, 0.48, 0.73)],
    },
    PropKind {
        name: "brazier",
        size: 2.4,
        footprint: 0.5,
        specular: 0.3,
        glow: 0.0,
        solid: &[Block::new(0.0, 0.0, 0.3, 0.3, 1.0)],
    },
    PropKind {
        name: "banner_pole",
        size: 6.0,
        footprint: 0.3,
        specular: 0.1,
        glow: 0.0,
        solid: &[Block::new(0.0, 0.0, 0.04, 0.05, 1.0)],
    },
    PropKind {
        name: "guardian_statue",
        size: 9.0,
        footprint: 0.4,
        specular: 0.15,
        glow: 0.0,
        solid: &[Block::new(0.0, -0.03, 0.17, 0.28, 1.0)],
    },
    PropKind {
        name: "siege_wagon",
        size: 7.0,
        footprint: 0.5,
        specular: 0.1,
        glow: 0.0,
        solid: &[Block::new(0.0, 0.0, 0.27, 0.48, 0.63)],
    },
    PropKind {
        name: "neon_arch",
        size: 19.0,
        footprint: 0.0,
        specular: 0.4,
        glow: 0.22,
        solid: &[
            Block::new(0.0, -0.47, 0.12, 0.03, 0.57),
            Block::new(0.0, 0.47, 0.12, 0.03, 0.57),
        ],
    },
    PropKind {
        name: "antenna_pylon",
        size: 50.0,
        footprint: 0.2,
        specular: 0.3,
        glow: 0.0,
        solid: &[Block::new(0.0, 0.0, 0.13, 0.14, 1.0)],
    },
    PropKind::landmark(
        "solar_relay",
        42.0,
        0.45,
        &[Block::new(0.0, 0.0, 0.05, 0.05, 0.52)],
    ),
    // Cruising overhead: nothing to hit.
    PropKind::landmark("sky_freighter", 58.0, 0.25, &[]),
    PropKind::landmark(
        "great_hall",
        42.0,
        0.08,
        &[Block::new(0.0, 0.0, 0.45, 0.22, 0.92)],
    ),
    PropKind::landmark(
        "sail_mill",
        32.0,
        0.07,
        &[Block::new(0.0, 0.0, 0.2, 0.2, 1.0)],
    ),
    // Two legs: a kart can drive through the window.
    PropKind::landmark(
        "sandstone_window",
        39.0,
        0.04,
        &[
            Block::new(-0.35, 0.0, 0.15, 0.15, 0.6),
            Block::new(0.22, 0.0, 0.17, 0.15, 0.6),
        ],
    ),
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
    /// The prop's up axis (the road normal for things mounted on a banked road).
    pub up: Vec3,
}

impl Placement {
    pub fn model(&self) -> Mat4 {
        let tilt = glam::Quat::from_rotation_arc(Vec3::Y, self.up.normalize_or(Vec3::Y));
        Mat4::from_scale_rotation_translation(
            Vec3::splat(self.size),
            tilt * glam::Quat::from_rotation_y(self.yaw),
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

/// Centerline point (with height) and the road's right vector at lap fraction `at`.
fn frame(track: &Track, at: f32) -> (Vec3, Vec3) {
    let cl = &track.centerline;
    let n = cl.len() - 1;
    let i = ((at.rem_euclid(1.0)) * n as f32) as usize % n;
    let f = (cl[(i + 1) % n] - cl[(i + n - 1) % n]).normalize();
    (cl[i], f.cross(Vec3::Y).normalize())
}

/// Yaw that turns a model's +Z axis to `dir`.
fn yaw_to(dir: Vec3) -> f32 {
    dir.x.atan2(dir.z)
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

/// The solid parts of every prop on the circuit, for the collision mesh. Deterministic and
/// independent of what is drawn.
pub fn solids(track: &Track) -> Vec<Solid> {
    let mut out = Vec::new();
    for p in placements(track) {
        let model = p.model();
        for b in PROPS[p.kind].solid {
            let [x, z] = b.center;
            let [hx, hz] = b.half;
            let base = [(-hx, -hz), (hx, -hz), (hx, hz), (-hx, hz)]
                .map(|(dx, dz)| model.transform_point3(Vec3::new(x + dx, 0.0, z + dz)));
            let rise = model.transform_vector3(Vec3::Y * b.height);
            out.push(Solid {
                base: base.to_vec(),
                up: rise.normalize_or(Vec3::Y),
                height: rise.length(),
            });
        }
    }
    out
}

/// Skyway's neon arches, seated on the banked road.
fn arches(track: &Track) -> Vec<Placement> {
    if track.id != TrackId::Skyway {
        return Vec::new();
    }
    let cl = &track.centerline;
    let n = cl.len() - 1;
    [0.12, 0.45, 0.75, 0.9]
        .into_iter()
        .map(|at: f32| {
            let i = (at * n as f32) as usize % n;
            let up = track.road_up[i];
            let fwd = (cl[i + 1] - cl[i]).normalize();
            let right = Vec3::new(fwd.x, 0.0, fwd.z).normalize().cross(Vec3::Y);
            let mut p = place("neon_arch", cl[i] - up * 0.3, yaw_to(right), 1.0);
            p.up = up;
            p
        })
        .collect()
}

/// Every prop on the circuit, deterministic.
pub fn placements(track: &Track) -> Vec<Placement> {
    match track.id {
        TrackId::Dustbowl => dustbowl(track),
        TrackId::Grindstone => grindstone(track),
        TrackId::Skyway => skyway(track),
    }
}

fn place(kind_name: &str, pos: Vec3, yaw: f32, scale: f32) -> Placement {
    let k = kind(kind_name);
    Placement {
        kind: k,
        pos,
        yaw,
        size: PROPS[k].size * scale,
        up: Vec3::Y,
    }
}

/// Scatter `count` props from `names` over the ground around the track, clear of the road and
/// of what is already placed.
fn scatter(track: &Track, out: &mut Vec<Placement>, names: &[&str], count: usize, seed: u64) {
    let cl = &track.centerline;
    let set_pieces: Vec<(Vec3, f32)> = out.iter().map(|p| (p.pos, p.size * 0.6)).collect();
    let mut rng = Rng(seed);
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in cl {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let (mut tries, mut placed) = (0, 0);
    while placed < count && tries < 6000 {
        tries += 1;
        let x = lo.x - 120.0 + rng.next() * (hi.x - lo.x + 240.0);
        let z = lo.z - 120.0 + rng.next() * (hi.z - lo.z + 240.0);
        let d = terrain::distance_to(cl, x, z);
        let name = names[((rng.next() * names.len() as f32) as usize).min(names.len() - 1)];
        let k = kind(name);
        let size = PROPS[k].size * (0.7 + rng.next() * 0.6);
        let clear = crate::track::ROAD_HALF_WIDTH + 4.0 + size * PROPS[k].footprint;
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
        let y = terrain::height_for(track.terrain_kind, cl, x, z);
        out.push(Placement {
            kind: k,
            pos: Vec3::new(x, y - 0.2, z),
            yaw: rng.next() * std::f32::consts::TAU,
            size,
            up: Vec3::Y,
        });
        placed += 1;
    }
}

/// Castle Grindstone: towers on the corners, the gatehouse by the drawbridge, statues guarding
/// the start, banners and braziers lining the ramparts.
fn grindstone(track: &Track) -> Vec<Placement> {
    let cl = &track.centerline;
    let mut out = Vec::new();
    let ground = |p: Vec3| {
        Vec3::new(
            p.x,
            terrain::height_for(TerrainKind::Hills, cl, p.x, p.z) - 0.2,
            p.z,
        )
    };
    let put = |out: &mut Vec<Placement>,
               name: &str,
               at: f32,
               lateral: f32,
               yaw: Option<f32>,
               scale: f32| {
        let (c, r) = frame(track, at);
        let p = ground(c + r * lateral);
        // Default: face the road.
        let face = yaw.unwrap_or_else(|| yaw_to(-r * lateral.signum()));
        out.push(place(name, p, face, scale));
    };
    put(
        &mut out,
        "sail_mill",
        0.075,
        -35.0,
        Some(yaw_to(frame(track, 0.075).1) - 0.85),
        1.0,
    );
    put(&mut out, "great_hall", 0.43, 42.0, None, 1.0);
    put(&mut out, "castle_tower", 0.24, -32.0, None, 1.15);
    put(&mut out, "castle_tower", 0.80, -31.0, None, 0.85);
    // The gatehouse, looming over the drawbridge jump.
    put(&mut out, "gatehouse", 0.7, -36.0, None, 1.0);
    // Guardians flanking the start line.
    put(&mut out, "guardian_statue", 0.985, 14.5, None, 1.0);
    put(&mut out, "guardian_statue", 0.985, -14.5, None, 1.0);
    put(&mut out, "guardian_statue", 0.68, 14.5, None, 1.1);
    // Banners and braziers along the walls.
    let mut k = 0;
    let mut at = 0.02;
    while at < 0.99 {
        let side = if k % 2 == 0 { 1.0 } else { -1.0 };
        put(&mut out, "banner_pole", at, 11.8 * side, None, 1.0);
        put(
            &mut out,
            "brazier",
            at + 0.025,
            -11.2 * side,
            Some(at * 5.0),
            1.0,
        );
        k += 1;
        at += 0.06;
    }
    // Siege wagons parked in the fields.
    put(&mut out, "siege_wagon", 0.16, 16.0, None, 1.0);
    put(&mut out, "siege_wagon", 0.5, -17.0, Some(1.2), 1.1);
    put(&mut out, "siege_wagon", 0.87, 16.5, Some(4.0), 0.95);
    // Drop anything that ended up too close to another stretch of the road.
    out.retain(|p| {
        terrain::distance_to(cl, p.pos.x, p.pos.z)
            > crate::track::ROAD_HALF_WIDTH + 2.5 + p.size * PROPS[p.kind].footprint
    });
    scatter(track, &mut out, &["dead_tree"], 34, 0x00ca_571e);
    out
}

/// Skyway Zero: neon arches over the deck, and pylons rising out of the clouds.
fn skyway(track: &Track) -> Vec<Placement> {
    // The billboards themselves are part of the track mesh (`track::skyway_billboards`).
    let mut out = arches(track);
    // Cruise above the launch straight. The relay hangs lower beside the far S-bends.
    for (name, at, lateral, altitude) in [
        ("sky_freighter", 0.065, -47.0, 8.0),
        ("solar_relay", 0.52, 44.0, 1.0),
    ] {
        let (c, right) = frame(track, at);
        out.push(place(
            name,
            c + right * lateral + Vec3::Y * altitude,
            yaw_to(right) + 0.85,
            1.0,
        ));
    }
    let mut rng = Rng(0x05c7_e1e0);
    for k in 0..6 {
        let at = k as f32 / 6.0 + rng.next() * 0.03;
        let side = if rng.next() < 0.5 { 1.0 } else { -1.0 };
        let (c, r) = frame(track, at);
        let lat = side * (45.0 + rng.next() * 80.0);
        let scale = 1.0 + rng.next() * 0.6;
        let top_above_deck = 6.0 + rng.next() * 20.0;
        let base = c.y + top_above_deck - PROPS[kind("antenna_pylon")].size * scale;
        out.push(place(
            "antenna_pylon",
            Vec3::new(c.x, base, c.z) + r * lat,
            rng.next() * 6.3,
            scale,
        ));
    }
    out
}

/// The Dustbowl Loop: skull rock, refinery, plane and bus wrecks, cacti and dead trees.
fn dustbowl(track: &Track) -> Vec<Placement> {
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
            up: Vec3::Y,
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
    put(
        "sandstone_window",
        beside(track, 0.09, -52.0),
        yaw_to(frame(track, 0.09).1) - 1.05,
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
    scatter(
        track,
        &mut out,
        &["cactus", "cactus", "cactus", "dead_tree", "dead_tree"],
        46,
        0xdead_7ee5,
    );
    out
}

/// Loads a prop GLB normalized to unit size (largest dimension 1, centered on X/Z, base at
/// y = 0) and textured from layer `layer`.
pub fn load_prop(
    assets: &Path,
    kind: &PropKind,
    layer: u32,
) -> Result<(MeshData, Option<Rgba8Image>), GlbError> {
    let name = kind.name;
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
    let (mat, tint) = if kind.glow > 0.0 {
        (material(layer, layer, rivet_render::EMISSIVE), kind.glow)
    } else {
        (material(layer, layer, 0), 1.0)
    };
    let mut m = MeshData::default();
    for i in 0..g.positions.len() {
        m.vertices.push(Vertex {
            pos: ((Vec3::from_array(g.positions[i]) - offset) / size).to_array(),
            normal: g.normals[i],
            uv: g.uvs[i],
            color: [tint, tint, tint, 0.0],
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
        let all: Vec<Placement> = crate::track::TRACKS
            .iter()
            .flat_map(|d| placements(&crate::track::build_track(d)))
            .collect();
        for (k, kind) in PROPS.iter().enumerate() {
            assert!(
                all.iter().any(|p| p.kind == k),
                "{} unused on every track",
                kind.name
            );
        }
        let t = build_graybox_loop();
        let placed = placements(&t);
        let skull = placed
            .iter()
            .find(|p| PROPS[p.kind].name == "skull_rock")
            .unwrap();
        assert!(skull.size >= 40.0);
    }

    #[test]
    fn each_track_dresses_in_its_own_props() {
        // (Grindstone drops props that land too near another stretch; keep most of them.)
        use crate::track::{TRACKS, build_track};
        let names = |i: usize| {
            let t = build_track(&TRACKS[i]);
            placements(&t)
                .iter()
                .map(|p| PROPS[p.kind].name)
                .collect::<Vec<_>>()
        };
        let castle = names(1);
        for n in [
            "castle_tower",
            "sail_mill",
            "great_hall",
            "gatehouse",
            "guardian_statue",
            "siege_wagon",
            "banner_pole",
            "brazier",
        ] {
            assert!(castle.contains(&n), "castle lacks {n}");
        }
        assert!(!castle.contains(&"cactus"), "no cacti at the castle");
        let sky = names(2);
        for n in ["neon_arch", "antenna_pylon", "sky_freighter", "solar_relay"] {
            assert!(sky.contains(&n), "skyway lacks {n}");
        }
        assert!(!sky.contains(&"skull_rock"));
    }

    #[test]
    fn castle_props_stay_off_the_road() {
        use crate::track::{TRACKS, build_track};
        let t = build_track(&TRACKS[1]);
        for p in placements(&t) {
            let d = crate::terrain::distance_to(&t.centerline, p.pos.x, p.pos.z);
            let need = ROAD_HALF_WIDTH + 2.0 + p.size * PROPS[p.kind].footprint;
            assert!(
                d > need,
                "{} is {d:.1} m from the road (needs {need:.1})",
                PROPS[p.kind].name
            );
        }
    }

    #[test]
    fn hidden_props_are_out_of_reach() {
        // With scenery off a walled circuit hides its props; their solid parts must all stand
        // beyond the barriers, or a client with scenery off would hit invisible walls.
        use crate::track::{TRACKS, build_track};
        for def in &TRACKS {
            let t = build_track(def);
            if !t.walled {
                continue;
            }
            for s in solids(&t) {
                for p in &s.base {
                    let d = crate::terrain::distance_to(&t.centerline, p.x, p.z);
                    assert!(
                        d > ROAD_HALF_WIDTH + 0.5,
                        "{}: a solid prop corner {d:.1} m from the centerline",
                        def.name
                    );
                }
            }
        }
    }

    #[test]
    fn karts_bump_off_props_instead_of_driving_through() {
        use crate::track::{TRACKS, build_track};
        use rivet_physics::{DriveInput, VehicleParams, VehicleState, step};
        let t = build_track(&TRACKS[0]);
        let params = VehicleParams::default();
        let (mut tested, mut reached) = (0, 0);
        for p in placements(&t) {
            let name = PROPS[p.kind].name;
            if !["bus_wreck", "wreck_car", "barrel_stack", "fuel_tanks"].contains(&name) {
                continue;
            }
            // Drive flat out at the prop's center from 12 m farther out, heading for the road
            // (from the road side, a kart can sail over a low stack off the banked edge).
            let block = PROPS[p.kind].solid[0];
            let model = p.model();
            let center = model.transform_point3(Vec3::new(block.center[0], 0.0, block.center[1]));
            let road = t
                .centerline
                .iter()
                .min_by(|a, b| {
                    let flat = |q: &Vec3| Vec3::new(q.x - center.x, 0.0, q.z - center.z).length();
                    flat(a).total_cmp(&flat(b))
                })
                .expect("a centerline");
            let dir = Vec3::new(road.x - center.x, 0.0, road.z - center.z).normalize();
            let start = center - dir * (12.0 + p.size * 0.5);
            let ground = t
                .collision
                .raycast(start + Vec3::Y * 30.0, Vec3::NEG_Y, 60.0)
                .expect("ground near the prop");
            let mut kart = VehicleState::spawn(ground.point + Vec3::Y * 0.6, dir.x.atan2(dir.z));
            kart.velocity = dir * 25.0;
            let to_local = model.inverse();
            let mut closest = f32::MAX;
            for tick in 0..120 {
                let input = DriveInput {
                    throttle: 1.0,
                    ..Default::default()
                };
                step(&mut kart, &params, input, &t.collision, 1.0 / 60.0);
                let local = to_local.transform_point3(kart.position);
                let inside = (local.x - block.center[0]).abs() < block.half[0]
                    && (local.z - block.center[1]).abs() < block.half[1];
                assert!(
                    !inside,
                    "drove into the {name} at {} (tick {tick}, kart {}, local {local})",
                    p.pos, kart.position
                );
                let gap = ((local.x - block.center[0]).abs() - block.half[0])
                    .max((local.z - block.center[1]).abs() - block.half[1]);
                closest = closest.min(gap * p.size);
            }
            // (A neighbor can block the way: a barrel stack beside a bus wreck.)
            if closest < 2.0 {
                reached += 1;
            }
            tested += 1;
        }
        assert!(tested >= 8, "only {tested} props tested");
        assert!(
            reached * 4 >= tested * 3,
            "reached only {reached} of {tested}"
        );
    }

    #[test]
    fn skyway_arches_follow_the_road_bank() {
        use crate::track::{TRACKS, build_track};
        let t = build_track(&TRACKS[2]);
        for a in arches(&t) {
            let i = t
                .centerline
                .iter()
                .enumerate()
                .min_by(|x, y| x.1.distance(a.pos).total_cmp(&y.1.distance(a.pos)))
                .unwrap()
                .0;
            let arch_up = a.model().transform_vector3(Vec3::Y).normalize();
            assert!(
                arch_up.dot(t.road_up[i]) > 0.995,
                "arch tilted {arch_up} vs road {}",
                t.road_up[i]
            );
            // The span (model +Z) lies across the road, perpendicular to travel.
            let span = a.model().transform_vector3(Vec3::Z).normalize();
            let fwd = (t.centerline[(i + 1) % t.centerline.len()] - t.centerline[i]).normalize();
            assert!(
                span.dot(fwd).abs() < 0.08,
                "arch skewed across the road ({:.2})",
                span.dot(fwd)
            );
        }
    }

    #[test]
    fn skyway_arches_span_the_road_at_road_height() {
        use crate::track::{TRACKS, build_track};
        let t = build_track(&TRACKS[2]);
        let arches: Vec<_> = placements(&t)
            .into_iter()
            .filter(|p| PROPS[p.kind].name == "neon_arch")
            .collect();
        assert!(arches.len() >= 4);
        for a in arches {
            let c = t
                .centerline
                .iter()
                .min_by(|x, y| x.distance(a.pos).total_cmp(&y.distance(a.pos)))
                .unwrap();
            assert!(
                Vec3::new(c.x - a.pos.x, 0.0, c.z - a.pos.z).length() < 1.0,
                "centered on the road"
            );
            assert!((a.pos.y - c.y).abs() < 1.5, "sits on the deck");
            assert!(
                a.size * 0.5 > ROAD_HALF_WIDTH + 1.5,
                "wide enough to clear the walls"
            );
        }
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
