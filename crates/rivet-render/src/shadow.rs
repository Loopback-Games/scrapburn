//! Directional sun shadow map fitted around a focus point.

use glam::{Mat4, Vec3};

/// Orthographic sun view-projection covering a sphere of `radius` around `focus`, with the
/// light-space origin snapped to shadow-map texels so shadows don't shimmer as the focus moves.
pub fn light_view_proj(sun_dir: Vec3, focus: Vec3, radius: f32, shadow_size: u32) -> Mat4 {
    let dir = sun_dir.normalize();
    let up = if dir.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
    // View from a fixed origin along the light, so snapping is in a stable frame.
    let view = glam::camera::rh::view::look_to_mat4(Vec3::ZERO, dir, up);
    let center = view.transform_point3(focus);
    let texel = 2.0 * radius / shadow_size as f32;
    let cx = (center.x / texel).round() * texel;
    let cy = (center.y / texel).round() * texel;
    // Depth range: generous behind the focus so tall casters (walls, cliffs) still cast.
    let depth_back = radius * 4.0;
    let near = -center.z - depth_back;
    let far = -center.z + radius;
    let proj = glam::camera::rh::proj::directx::orthographic(
        cx - radius,
        cx + radius,
        cy - radius,
        cy + radius,
        near,
        far,
    );
    proj * view
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Vec3, Vec4Swizzles};

    fn sun() -> Vec3 {
        Vec3::new(-0.4, -0.8, 0.35).normalize()
    }

    #[test]
    fn fit_contains_focus_sphere() {
        let focus = Vec3::new(120.0, 3.0, -40.0);
        let m = light_view_proj(sun(), focus, 60.0, 2048);
        for dx in [-1.0, 1.0] {
            for dy in [-1.0, 1.0] {
                for dz in [-1.0, 1.0] {
                    let p = focus + Vec3::new(dx, dy, dz) * 60.0 * 0.57;
                    let c = m * p.extend(1.0);
                    let n = c.xyz() / c.w;
                    assert!(n.x.abs() <= 1.0 && n.y.abs() <= 1.0, "xy out: {n}");
                    assert!((0.0..=1.0).contains(&n.z), "z out: {n}");
                }
            }
        }
    }

    #[test]
    fn fit_snaps_to_texels() {
        // Moving the focus by a fraction of a texel must move a fixed world point by whole texels
        // (or not at all), so shadows don't shimmer while driving.
        let size = 1024;
        let p = Vec3::new(10.0, 0.0, 10.0).extend(1.0);
        let a = light_view_proj(sun(), Vec3::ZERO, 50.0, size) * p;
        let b = light_view_proj(sun(), Vec3::new(0.013, 0.0, 0.021), 50.0, size) * p;
        let texels = |v: f32| v * size as f32 * 0.5;
        let d = (texels(a.x) - texels(b.x), texels(a.y) - texels(b.y));
        assert!(
            (d.0 - d.0.round()).abs() < 1e-2 && (d.1 - d.1.round()).abs() < 1e-2,
            "{d:?}"
        );
    }
}
