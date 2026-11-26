//! Set-piece props and scatter around each circuit: the Dustbowl's skull rock, refinery and
//! wrecks; Grindstone's towers, gatehouse and banners; Skyway's neon arches and pylons.

use std::path::Path;

use glam::{Mat4, Vec3};
use rivet_assets::{GlbError, Rgba8Image, load_glb};
use rivet_render::{MeshData, Vertex, material};

use crate::terrain::{self, TerrainKind};
use crate::track::{Track, TrackId};

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
}

pub const PROPS: [PropKind; 19] = [
    PropKind {
        name: "skull_rock",
        size: 55.0,
        footprint: 0.5,
        specular: 0.04,
        glow: 0.0,
    },
    PropKind {
        name: "plane_wreck",
        size: 26.0,
        footprint: 0.5,
        specular: 0.25,
        glow: 0.0,
    },
    PropKind {
        name: "fuel_tanks",
        size: 22.0,
        footprint: 0.5,
        specular: 0.3,
        glow: 0.0,
    },
    PropKind {
        name: "pumpjack",
        size: 12.0,
        footprint: 0.5,
        specular: 0.3,
        glow: 0.0,
    },
    PropKind {
        name: "water_tower",
        size: 18.0,
        footprint: 0.3,
        specular: 0.3,
        glow: 0.0,
    },
    PropKind {
        name: "bus_wreck",
        size: 11.0,
        footprint: 0.5,
        specular: 0.25,
        glow: 0.0,
    },
    PropKind {
        name: "wreck_car",
        size: 5.5,
        footprint: 0.5,
        specular: 0.25,
        glow: 0.0,
    },
    PropKind {
        name: "barrel_stack",
        size: 3.5,
        footprint: 0.5,
        specular: 0.2,
        glow: 0.0,
    },
    PropKind {
        name: "cactus",
        size: 6.0,
        footprint: 0.3,
        specular: 0.05,
        glow: 0.0,
    },
    PropKind {
        name: "dead_tree",
        size: 8.0,
        footprint: 0.4,
        specular: 0.03,
        glow: 0.0,
    },
    PropKind {
        name: "castle_tower",
        size: 28.0,
        footprint: 0.4,
        specular: 0.05,
        glow: 0.0,
    },
    PropKind {
        name: "gatehouse",
        size: 36.0,
        footprint: 0.45,
        specular: 0.05,
        glow: 0.0,
    },
    PropKind {
        name: "brazier",
        size: 2.4,
        footprint: 0.5,
        specular: 0.3,
        glow: 0.0,
    },
    PropKind {
        name: "banner_pole",
        size: 6.0,
        footprint: 0.3,
        specular: 0.1,
        glow: 0.0,
    },
    PropKind {
        name: "guardian_statue",
        size: 9.0,
        footprint: 0.4,
        specular: 0.15,
        glow: 0.0,
    },
    PropKind {
        name: "siege_wagon",
        size: 7.0,
        footprint: 0.5,
        specular: 0.1,
        glow: 0.0,
    },
    PropKind {
        name: "neon_arch",
        size: 19.0,
        footprint: 0.0,
        specular: 0.4,
        glow: 0.22,
    },
    PropKind {
        name: "antenna_pylon",
        size: 50.0,
        footprint: 0.2,
        specular: 0.3,
        glow: 0.0,
    },
    PropKind {
        name: "billboard_frame",
        size: 12.0,
        footprint: 0.4,
        specular: 0.3,
        glow: 0.3,
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

/// Props that are part of the circuit itself (kept when background scenery is off): Skyway's
/// neon arches, seated on the banked road.
pub fn track_features(track: &Track) -> Vec<Placement> {
    if track.id != TrackId::Skyway {
        return Vec::new();
    }
    let cl = &track.centerline;
    let n = cl.len() - 1;
    [0.12, 0.3, 0.45, 0.62, 0.75, 0.9]
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
    // Towers on the outside of the corners (the loop runs counter-clockwise: left is outside).
    for (at, lat, sc) in [
        (0.1, -30.0, 1.0),
        (0.24, -32.0, 1.15),
        (0.43, 30.0, 0.9),
        (0.58, -30.0, 1.1),
        (0.8, -31.0, 1.0),
        (0.93, 30.0, 0.85),
    ] {
        put(&mut out, "castle_tower", at, lat, Some(at * 9.0), sc);
    }
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

/// Skyway Zero: neon arches over the deck, holo billboards, and pylons rising out of the haze.
fn skyway(track: &Track) -> Vec<Placement> {
    let mut out = track_features(track);
    for (k, at) in [0.06, 0.2, 0.37, 0.52, 0.68, 0.83, 0.95]
        .into_iter()
        .enumerate()
    {
        let side = if k % 2 == 0 { 1.0 } else { -1.0 };
        let (c, r) = frame(track, at);
        out.push(place(
            "billboard_frame",
            c + r * side * 16.5 + Vec3::Y * 0.5,
            yaw_to(-r * side),
            1.0,
        ));
    }
    let mut rng = Rng(0x05c7_e1e0);
    for k in 0..18 {
        let at = k as f32 / 18.0 + rng.next() * 0.03;
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
        for n in ["neon_arch", "antenna_pylon", "billboard_frame"] {
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
    fn scenery_off_keeps_only_track_features() {
        use crate::track::{TRACKS, build_track};
        for (i, def) in TRACKS.iter().enumerate() {
            let t = build_track(def);
            let kept: Vec<&str> = track_features(&t)
                .iter()
                .map(|p| PROPS[p.kind].name)
                .collect();
            if i == 2 {
                assert!(
                    !kept.is_empty() && kept.iter().all(|n| *n == "neon_arch"),
                    "{kept:?}"
                );
            } else {
                assert!(kept.is_empty(), "{}: {kept:?}", def.name);
            }
        }
    }

    #[test]
    fn skyway_arches_follow_the_road_bank() {
        use crate::track::{TRACKS, build_track};
        let t = build_track(&TRACKS[2]);
        for a in track_features(&t) {
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
