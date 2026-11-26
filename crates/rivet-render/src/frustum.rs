//! View-frustum culling with bounding spheres.

use glam::{Mat4, Vec3, Vec4};

/// The six planes of a view-projection matrix (depth 0..1), normals pointing inward.
#[derive(Debug, Clone, Copy)]
pub struct Frustum {
    planes: [Vec4; 6],
}

impl Frustum {
    pub fn from_view_proj(m: &Mat4) -> Self {
        let r = [m.row(0), m.row(1), m.row(2), m.row(3)];
        let planes = [
            r[3] + r[0],
            r[3] - r[0],
            r[3] + r[1],
            r[3] - r[1],
            r[2],
            r[3] - r[2],
        ]
        .map(|p| p / p.truncate().length().max(1e-12));
        Self { planes }
    }

    /// True unless the sphere is entirely outside one plane.
    pub fn sphere_visible(&self, center: Vec3, radius: f32) -> bool {
        self.planes
            .iter()
            .all(|p| p.truncate().dot(center) + p.w > -radius)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Mat4, Vec3};

    fn cam() -> Mat4 {
        glam::camera::rh::proj::directx::perspective(1.2, 16.0 / 9.0, 0.1, 1000.0)
            * glam::camera::rh::view::look_at_mat4(Vec3::ZERO, Vec3::NEG_Z, Vec3::Y)
    }

    #[test]
    fn sphere_in_front_is_visible() {
        let f = Frustum::from_view_proj(&cam());
        assert!(f.sphere_visible(Vec3::new(0.0, 0.0, -50.0), 1.0));
    }

    #[test]
    fn sphere_behind_or_far_off_side_is_culled() {
        let f = Frustum::from_view_proj(&cam());
        assert!(!f.sphere_visible(Vec3::new(0.0, 0.0, 50.0), 1.0), "behind");
        assert!(
            !f.sphere_visible(Vec3::new(500.0, 0.0, -50.0), 5.0),
            "far to the side"
        );
        assert!(
            !f.sphere_visible(Vec3::new(0.0, 0.0, -2000.0), 5.0),
            "beyond the far plane"
        );
    }

    #[test]
    fn big_sphere_straddling_the_edge_is_visible() {
        let f = Frustum::from_view_proj(&cam());
        assert!(
            f.sphere_visible(Vec3::new(0.0, 0.0, 6.0), 10.0),
            "camera inside it"
        );
    }
}
