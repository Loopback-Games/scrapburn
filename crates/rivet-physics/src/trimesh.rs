//! Static triangle mesh with a uniform XZ grid for raycasts and sphere contacts.

use glam::{Vec2, Vec3, Vec3Swizzles};

/// What a triangle is made of, for handling.
#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Surface {
    #[default]
    Road = 0,
    /// Grass, dirt, sand: slow.
    Offroad = 1,
    Wall = 2,
    /// Gives a speed boost while touched.
    BoostPad = 3,
}

/// Nearest ray intersection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    pub t: f32,
    pub point: Vec3,
    /// Unit normal facing against the ray direction.
    pub normal: Vec3,
    pub tri: u32,
    pub surface: Surface,
}

/// Sphere penetration: push the sphere along `normal` by `depth` to resolve it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contact {
    pub normal: Vec3,
    pub depth: f32,
    /// Unit normal of the touched triangle's plane (either side), to tell walls from floors.
    pub face_normal: Vec3,
}

/// Static triangle soup with a uniform grid over XZ. Built once, queried every tick.
#[derive(Debug, Clone)]
pub struct TriMesh {
    positions: Vec<Vec3>,
    indices: Vec<[u32; 3]>,
    surfaces: Vec<Surface>,
    origin: Vec2,
    cell: f32,
    nx: usize,
    nz: usize,
    /// CSR layout: triangles of cell `i` are `cell_tris[cell_start[i]..cell_start[i + 1]]`.
    cell_start: Vec<u32>,
    cell_tris: Vec<u32>,
}

const MAX_CELLS_PER_AXIS: usize = 2048;

impl TriMesh {
    /// All triangles are `Surface::Road`.
    pub fn new(positions: Vec<Vec3>, indices: Vec<[u32; 3]>, cell_size: f32) -> Self {
        let n = indices.len();
        Self::with_surfaces(positions, indices, vec![Surface::Road; n], cell_size)
    }

    /// One `Surface` per triangle.
    pub fn with_surfaces(
        positions: Vec<Vec3>,
        indices: Vec<[u32; 3]>,
        surfaces: Vec<Surface>,
        cell_size: f32,
    ) -> Self {
        assert_eq!(surfaces.len(), indices.len(), "one surface per triangle");
        assert!(cell_size > 0.0, "cell_size must be positive");
        let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
        for p in &positions {
            lo = lo.min(p.xz());
            hi = hi.max(p.xz());
        }
        if positions.is_empty() {
            lo = Vec2::ZERO;
            hi = Vec2::ZERO;
        }
        let extent = hi - lo;
        let cell = cell_size
            .max(extent.x / MAX_CELLS_PER_AXIS as f32)
            .max(extent.y / MAX_CELLS_PER_AXIS as f32);
        let nx = ((extent.x / cell).floor() as usize + 1).max(1);
        let nz = ((extent.y / cell).floor() as usize + 1).max(1);
        let mut mesh = Self {
            positions,
            indices,
            surfaces,
            origin: lo,
            cell,
            nx,
            nz,
            cell_start: Vec::new(),
            cell_tris: Vec::new(),
        };
        mesh.build_grid();
        mesh
    }

    fn cell_range(&self, lo: Vec2, hi: Vec2) -> (usize, usize, usize, usize) {
        let c =
            |v: f32, o: f32, n: usize| (((v - o) / self.cell).floor().max(0.0) as usize).min(n - 1);
        (
            c(lo.x, self.origin.x, self.nx),
            c(hi.x, self.origin.x, self.nx),
            c(lo.y, self.origin.y, self.nz),
            c(hi.y, self.origin.y, self.nz),
        )
    }

    fn tri_bounds(&self, t: usize) -> (Vec2, Vec2) {
        let [a, b, c] = self.tri(t);
        (
            a.xz().min(b.xz()).min(c.xz()),
            a.xz().max(b.xz()).max(c.xz()),
        )
    }

    fn build_grid(&mut self) {
        let n = self.nx * self.nz;
        let mut counts = vec![0u32; n + 1];
        for t in 0..self.indices.len() {
            let (lo, hi) = self.tri_bounds(t);
            let (x0, x1, z0, z1) = self.cell_range(lo, hi);
            for z in z0..=z1 {
                for x in x0..=x1 {
                    counts[z * self.nx + x + 1] += 1;
                }
            }
        }
        for i in 1..=n {
            counts[i] += counts[i - 1];
        }
        let mut fill = counts.clone();
        let mut tris = vec![0u32; counts[n] as usize];
        for t in 0..self.indices.len() {
            let (lo, hi) = self.tri_bounds(t);
            let (x0, x1, z0, z1) = self.cell_range(lo, hi);
            for z in z0..=z1 {
                for x in x0..=x1 {
                    let c = z * self.nx + x;
                    tris[fill[c] as usize] = t as u32;
                    fill[c] += 1;
                }
            }
        }
        self.cell_start = counts;
        self.cell_tris = tris;
    }

    fn tri(&self, t: usize) -> [Vec3; 3] {
        let [a, b, c] = self.indices[t];
        [
            self.positions[a as usize],
            self.positions[b as usize],
            self.positions[c as usize],
        ]
    }

    fn cell_tris(&self, x: usize, z: usize) -> &[u32] {
        let c = z * self.nx + x;
        &self.cell_tris[self.cell_start[c] as usize..self.cell_start[c + 1] as usize]
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len()
    }

    /// Nearest hit along `dir` (normalized) within `max_t`.
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max_t: f32) -> Option<RayHit> {
        // Clip the ray's XZ projection to the grid rectangle.
        let lo = self.origin;
        let hi = self.origin + Vec2::new(self.nx as f32, self.nz as f32) * self.cell;
        let (mut t0, mut t1) = (0.0f32, max_t);
        for (o, d, l, h) in [(origin.x, dir.x, lo.x, hi.x), (origin.z, dir.z, lo.y, hi.y)] {
            if d.abs() < 1e-12 {
                if o < l || o > h {
                    return None;
                }
            } else {
                let (a, b) = ((l - o) / d, (h - o) / d);
                t0 = t0.max(a.min(b));
                t1 = t1.min(a.max(b));
            }
        }
        if t0 > t1 {
            return None;
        }

        // 2D DDA over cells from t0 to t1.
        let start = origin + dir * t0;
        let (mut cx, _, mut cz, _) = self.cell_range(start.xz(), start.xz());
        let step = |d: f32| if d > 0.0 { 1i64 } else { -1i64 };
        let next_t = |o: f32, d: f32, c: usize, org: f32| -> (f32, f32) {
            if d.abs() < 1e-12 {
                return (f32::INFINITY, f32::INFINITY);
            }
            let boundary = org + (c as f32 + if d > 0.0 { 1.0 } else { 0.0 }) * self.cell;
            ((boundary - o) / d, self.cell / d.abs())
        };
        let (mut tx, dtx) = next_t(origin.x, dir.x, cx, self.origin.x);
        let (mut tz, dtz) = next_t(origin.z, dir.z, cz, self.origin.y);
        let (sx, sz) = (step(dir.x), step(dir.z));

        let mut best: Option<RayHit> = None;
        loop {
            for &t in self.cell_tris(cx, cz) {
                let [a, b, c] = self.tri(t as usize);
                if let Some(ht) = ray_triangle(origin, dir, a, b, c)
                    && ht <= max_t
                    && best.is_none_or(|h| ht < h.t)
                {
                    let mut n = (b - a).cross(c - a).normalize_or_zero();
                    if n.dot(dir) > 0.0 {
                        n = -n;
                    }
                    best = Some(RayHit {
                        t: ht,
                        point: origin + dir * ht,
                        normal: n,
                        tri: t,
                        surface: self.surfaces[t as usize],
                    });
                }
            }
            let cell_exit = tx.min(tz);
            if best.is_some_and(|h| h.t <= cell_exit) || cell_exit > t1 {
                return best;
            }
            if tx < tz {
                let nx = cx as i64 + sx;
                if nx < 0 || nx >= self.nx as i64 {
                    return best;
                }
                cx = nx as usize;
                tx += dtx;
            } else {
                let nz = cz as i64 + sz;
                if nz < 0 || nz >= self.nz as i64 {
                    return best;
                }
                cz = nz as usize;
                tz += dtz;
            }
        }
    }

    /// Appends one contact per distinct surface the sphere penetrates.
    pub fn sphere_contacts(&self, center: Vec3, radius: f32, out: &mut Vec<Contact>) {
        let r2 = Vec2::splat(radius);
        let (x0, x1, z0, z1) = self.cell_range(center.xz() - r2, center.xz() + r2);
        let mut seen: Vec<u32> = Vec::new();
        for z in z0..=z1 {
            for x in x0..=x1 {
                seen.extend_from_slice(self.cell_tris(x, z));
            }
        }
        seen.sort_unstable();
        seen.dedup();
        let first = out.len();
        for t in seen {
            let [a, b, c] = self.tri(t as usize);
            let p = closest_point_on_triangle(center, a, b, c);
            let d = center - p;
            let dist2 = d.length_squared();
            if dist2 >= radius * radius {
                continue;
            }
            let dist = dist2.sqrt();
            let face_normal = (b - a).cross(c - a).normalize_or_zero();
            let normal = if dist > 1e-6 { d / dist } else { face_normal };
            let depth = radius - dist;
            // Merge with an existing contact on the same surface (shared edges, coplanar tris).
            if let Some(existing) = out[first..]
                .iter_mut()
                .find(|e| e.normal.dot(normal) > 0.999)
            {
                existing.depth = existing.depth.max(depth);
            } else {
                out.push(Contact {
                    normal,
                    depth,
                    face_normal,
                });
            }
        }
    }
}

/// Möller–Trumbore, double-sided. Returns `t >= 0`.
fn ray_triangle(o: Vec3, d: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let e1 = b - a;
    let e2 = c - a;
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(-1e-6..=1.0 + 1e-6).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < -1e-6 || u + v > 1.0 + 1e-6 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t >= 0.0).then_some(t)
}

/// Closest point on triangle `abc` to `p` (Ericson, Real-Time Collision Detection 5.1.5).
fn closest_point_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn plane(half: f32, y: f32, cell: f32) -> TriMesh {
        let p = vec![
            Vec3::new(-half, y, -half),
            Vec3::new(half, y, -half),
            Vec3::new(half, y, half),
            Vec3::new(-half, y, half),
        ];
        TriMesh::new(p, vec![[0, 2, 1], [0, 3, 2]], cell)
    }

    fn two_layers() -> TriMesh {
        let mut pos = Vec::new();
        let mut idx = Vec::new();
        for y in [0.0, 2.0] {
            let b = pos.len() as u32;
            pos.extend([
                Vec3::new(-50.0, y, -50.0),
                Vec3::new(50.0, y, -50.0),
                Vec3::new(50.0, y, 50.0),
                Vec3::new(-50.0, y, 50.0),
            ]);
            idx.extend([[b, b + 2, b + 1], [b, b + 3, b + 2]]);
        }
        TriMesh::new(pos, idx, 8.0)
    }

    #[test]
    fn ray_hits_flat_ground() {
        let m = plane(50.0, 0.0, 8.0);
        let h = m
            .raycast(Vec3::new(3.0, 5.0, 7.0), Vec3::NEG_Y, 100.0)
            .unwrap();
        assert!((h.t - 5.0).abs() < 1e-5);
        assert!((h.normal - Vec3::Y).length() < 1e-5);
        assert!((h.point - Vec3::new(3.0, 0.0, 7.0)).length() < 1e-5);
    }

    #[test]
    fn ray_misses_outside() {
        let m = plane(50.0, 0.0, 8.0);
        assert!(
            m.raycast(Vec3::new(500.0, 5.0, 0.0), Vec3::NEG_Y, 100.0)
                .is_none()
        );
    }

    #[test]
    fn nearest_of_two_layers() {
        let h = two_layers()
            .raycast(Vec3::new(1.0, 10.0, 1.0), Vec3::NEG_Y, 100.0)
            .unwrap();
        assert!((h.t - 8.0).abs() < 1e-5);
    }

    #[test]
    fn long_diagonal_ray() {
        let m = plane(500.0, 0.0, 8.0);
        let o = Vec3::new(-400.0, 1.0, -400.0);
        let d = (Vec3::new(400.0, 0.0, 400.0) - o).normalize();
        let h = m.raycast(o, d, 2000.0).unwrap();
        assert!(
            (h.point - Vec3::new(400.0, 0.0, 400.0)).length() < 1e-3,
            "{:?}",
            h.point
        );
    }

    #[test]
    fn sphere_against_wall() {
        // Vertical wall in the x = 0 plane.
        let p = vec![
            Vec3::new(0.0, -5.0, -5.0),
            Vec3::new(0.0, 5.0, -5.0),
            Vec3::new(0.0, 5.0, 5.0),
            Vec3::new(0.0, -5.0, 5.0),
        ];
        let m = TriMesh::new(p, vec![[0, 1, 2], [0, 2, 3]], 4.0);
        let mut out = Vec::new();
        m.sphere_contacts(Vec3::new(0.3, 0.0, 0.0), 0.5, &mut out);
        assert_eq!(out.len(), 1, "{out:?}");
        assert!((out[0].normal - Vec3::X).length() < 1e-5);
        assert!((out[0].depth - 0.2).abs() < 1e-4);
    }
}
