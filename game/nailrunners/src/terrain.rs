//! Desert terrain around the track: flat near the road, dunes beyond, canyon walls far out.

use glam::{Vec2, Vec3};
use rivet_render::{MeshData, TRIPLANAR_B, Vertex, material};

use crate::materials::Layer;

/// Terrain height at the road and in the flat zone around it.
pub const FLAT_Y: f32 = -0.5;
const FLAT_RADIUS: f32 = 16.0;
const HILL_RADIUS: f32 = 70.0;
const CANYON_START: f32 = 150.0;
const CANYON_FULL: f32 = 230.0;
pub const CELL: f32 = 8.0;
pub const MARGIN: f32 = 340.0;

fn hash2(x: i32, z: i32) -> f32 {
    let mut h =
        (x as u32).wrapping_mul(0x8da6_b343) ^ (z as u32).wrapping_mul(0xd816_3841) ^ 0x9e37_79b9;
    h = (h ^ (h >> 15)).wrapping_mul(0x2c1b_3c6d);
    h = (h ^ (h >> 12)).wrapping_mul(0x297a_2d39);
    ((h ^ (h >> 15)) & 0xffff) as f32 / 65_535.0
}

fn value_noise(x: f32, z: f32) -> f32 {
    let (x0, z0) = (x.floor(), z.floor());
    let (tx, tz) = (x - x0, z - z0);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sz) = (s(tx), s(tz));
    let (ix, iz) = (x0 as i32, z0 as i32);
    let a = hash2(ix, iz) + (hash2(ix + 1, iz) - hash2(ix, iz)) * sx;
    let b = hash2(ix, iz + 1) + (hash2(ix + 1, iz + 1) - hash2(ix, iz + 1)) * sx;
    a + (b - a) * sz
}

/// Fractal noise in 0..1, `scale` meters per base cell.
pub fn fbm(x: f32, z: f32, scale: f32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0 / scale, 0.0);
    for _ in 0..5 {
        sum += value_noise(x * freq, z * freq) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.03;
    }
    sum / norm
}

/// Horizontal distance from `(x, z)` to the centerline polyline.
pub fn distance_to(centerline: &[Vec3], x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let mut best = f32::MAX;
    for w in centerline.windows(2) {
        let (a, b) = (Vec2::new(w[0].x, w[0].z), Vec2::new(w[1].x, w[1].z));
        let ab = b - a;
        let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
        best = best.min((a + ab * t).distance_squared(p));
    }
    best.sqrt()
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The landscape style around a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerrainKind {
    /// Dunes rising to terraced mesas.
    Desert,
    /// Rolling grass hills ringed by rocky crags (the castle).
    Hills,
    /// No ground at all (the skyway floats over a city).
    Void,
}

fn hills_height(x: f32, z: f32, d: f32) -> f32 {
    let rolling = smoothstep(FLAT_RADIUS, 60.0, d) * (fbm(x, z, 70.0) * 14.0 + 1.0);
    let crag = 1.0 - (fbm(x - 211.0, z + 97.0, 90.0) * 2.0 - 1.0).abs();
    let ring = smoothstep(170.0, 260.0, d) * (25.0 + crag * 45.0);
    FLAT_Y + rolling + ring
}

/// Terrain height for a style (`Void` has none: very low).
pub fn height_for(kind: TerrainKind, centerline: &[Vec3], x: f32, z: f32) -> f32 {
    let d = distance_to(centerline, x, z);
    match kind {
        TerrainKind::Desert => height_with_distance(x, z, d),
        TerrainKind::Hills => hills_height(x, z, d),
        TerrainKind::Void => -1000.0,
    }
}

fn height_with_distance(x: f32, z: f32, d: f32) -> f32 {
    let hills = smoothstep(FLAT_RADIUS, HILL_RADIUS, d) * (fbm(x, z, 90.0) * 26.0 + 2.0);
    let ridge = 1.0 - (fbm(x + 913.0, z - 377.0, 140.0) * 2.0 - 1.0).abs();
    // Mesa walls: a steep rise with terraced ledges, so rock shows on the faces.
    let rise = smoothstep(CANYON_START, CANYON_FULL, d);
    let terraced = (rise * 4.0).floor() / 4.0 * 0.35 + rise * 0.65;
    let canyon = terraced * (55.0 + ridge * 50.0);
    FLAT_Y + hills + canyon
}

/// Terrain height at `(x, z)`.
pub fn height_at(centerline: &[Vec3], x: f32, z: f32) -> f32 {
    height_with_distance(x, z, distance_to(centerline, x, z))
}

/// Builds the terrain grid over `lo..hi` (XZ). Returns render mesh plus collision triangles.
pub fn build(centerline: &[Vec3], lo: Vec2, hi: Vec2) -> (MeshData, Vec<Vec3>, Vec<[u32; 3]>) {
    build_kind(TerrainKind::Desert, centerline, lo, hi)
}

/// Builds the terrain grid in a style. `Void` returns nothing.
pub fn build_kind(
    kind: TerrainKind,
    centerline: &[Vec3],
    lo: Vec2,
    hi: Vec2,
) -> (MeshData, Vec<Vec3>, Vec<[u32; 3]>) {
    if kind == TerrainKind::Void {
        return (MeshData::default(), Vec::new(), Vec::new());
    }
    let nx = ((hi.x - lo.x) / CELL).ceil() as usize + 1;
    let nz = ((hi.y - lo.y) / CELL).ceil() as usize + 1;
    let mut heights = vec![0.0f32; nx * nz];
    let mut dists = vec![0.0f32; nx * nz];
    for j in 0..nz {
        for i in 0..nx {
            let (x, z) = (lo.x + i as f32 * CELL, lo.y + j as f32 * CELL);
            let d = distance_to(centerline, x, z);
            dists[j * nx + i] = d;
            heights[j * nx + i] = match kind {
                TerrainKind::Hills => hills_height(x, z, d),
                _ => height_with_distance(x, z, d),
            };
        }
    }
    let h = |i: usize, j: usize| heights[j.min(nz - 1) * nx + i.min(nx - 1)];
    let mat = match kind {
        TerrainKind::Hills => material(Layer::Grass as u32, Layer::Rock as u32, TRIPLANAR_B),
        _ => material(Layer::Sand as u32, Layer::Rock as u32, TRIPLANAR_B),
    };
    let mut mesh = MeshData::default();
    let mut positions = Vec::with_capacity(nx * nz);
    for j in 0..nz {
        for i in 0..nx {
            let (x, z) = (lo.x + i as f32 * CELL, lo.y + j as f32 * CELL);
            let y = h(i, j);
            let dx = h(i + 1, j) - h(i.saturating_sub(1), j);
            let dz = h(i, j + 1) - h(i, j.saturating_sub(1));
            let n = Vec3::new(-dx, 2.0 * CELL, -dz).normalize();
            // Rock on steep faces and on the high mesa bands.
            let rock = match kind {
                // Green hills: bare rock only on genuinely steep faces.
                TerrainKind::Hills => smoothstep(0.8, 0.6, n.y),
                _ => smoothstep(0.9, 0.72, n.y).max(smoothstep(22.0, 40.0, y) * 0.85),
            };
            let d = dists[j * nx + i];
            // Dusty, slightly darker ochre near the road; gentle large-scale color variation.
            let near = 1.0 - smoothstep(FLAT_RADIUS - 6.0, FLAT_RADIUS + 20.0, d);
            let var = 0.92 + 0.16 * fbm(x - 51.0, z + 17.0, 40.0);
            let tint = match kind {
                // Dusky, muted meadow grass, worn to earth near the road.
                TerrainKind::Hills => [
                    var * 0.62 * (1.0 - 0.25 * near) + 0.18 * near,
                    var * 0.66 * (1.0 - 0.30 * near) + 0.12 * near,
                    var * 0.48 * (1.0 - 0.20 * near) + 0.08 * near,
                    rock,
                ],
                // Warm ochre sand, a little dustier and darker near the road.
                _ => [
                    var * 0.92 * (1.0 - 0.10 * near),
                    var * 0.74 * (1.0 - 0.14 * near),
                    var * 0.54 * (1.0 - 0.18 * near),
                    rock,
                ],
            };
            mesh.vertices.push(Vertex {
                pos: [x, y, z],
                normal: n.to_array(),
                uv: [x / 6.0, z / 6.0],
                color: tint,
                material: mat,
            });
            positions.push(Vec3::new(x, y, z));
        }
    }
    let mut tris = Vec::with_capacity((nx - 1) * (nz - 1) * 2);
    for j in 0..nz - 1 {
        for i in 0..nx - 1 {
            let a = (j * nx + i) as u32;
            let b = a + 1;
            let c = a + nx as u32;
            let d = c + 1;
            // Counter-clockwise seen from above (+Y normal).
            tris.push([a, c, b]);
            tris.push([b, c, d]);
        }
    }
    for t in &tris {
        mesh.indices.extend(t);
    }
    (mesh, positions, tris)
}
