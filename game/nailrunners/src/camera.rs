//! Spring-damped chase camera.

use glam::{Mat4, Quat, Vec3};

const BACK: f32 = 7.0;
const HEIGHT: f32 = 2.6;
const LOOK_AHEAD: f32 = 3.0;
const OMEGA: f32 = 8.0;
const FOV_MIN: f32 = 70.0;
/// Longest frame the spring integrates; longer hitches just catch up next frames.
const MAX_FRAME_DT: f32 = 0.1;
const SPRING_SUBSTEP: f32 = 1.0 / 120.0;
const FOV_MAX: f32 = 82.0;
/// Speed at which the FOV reaches its maximum (top speed + tier-2 boost).
const FOV_FULL_SPEED: f32 = 39.0;

#[derive(Debug, Clone, Default)]
pub struct ChaseCamera {
    pos: Vec3,
    vel: Vec3,
    target: Vec3,
    fov: f32,
    initialized: bool,
}

impl ChaseCamera {
    pub fn update(&mut self, target_pos: Vec3, target_rot: Quat, speed: f32, dt: f32) {
        let fwd = target_rot * Vec3::Z;
        let flat = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or(Vec3::Z);
        let desired = target_pos - flat * BACK + Vec3::Y * HEIGHT;
        self.target = target_pos + flat * LOOK_AHEAD + Vec3::Y * 1.0;
        let fov_goal =
            FOV_MIN + (FOV_MAX - FOV_MIN) * (speed.abs() / FOV_FULL_SPEED).clamp(0.0, 1.0);
        if !self.initialized {
            self.pos = desired;
            self.vel = Vec3::ZERO;
            self.fov = fov_goal;
            self.initialized = true;
            return;
        }
        // Critically damped spring toward the desired position. Explicit Euler is only stable
        // for small ω·dt, so cap long frames (hitches) and integrate in small substeps.
        let dt = dt.clamp(0.0, MAX_FRAME_DT);
        let steps = (dt / SPRING_SUBSTEP).ceil().max(1.0);
        let h = dt / steps;
        for _ in 0..steps as u32 {
            let accel = (desired - self.pos) * (OMEGA * OMEGA) - self.vel * (2.0 * OMEGA);
            self.vel += accel * h;
            self.pos += self.vel * h;
        }
        self.fov += (fov_goal - self.fov) * (1.0 - (-4.0 * dt).exp());
    }

    /// Snaps to the target next update (respawn, teleport).
    pub fn reset(&mut self) {
        self.initialized = false;
    }

    pub fn position(&self) -> Vec3 {
        self.pos
    }

    pub fn fov_degrees(&self) -> f32 {
        self.fov
    }

    pub fn view(&self) -> Mat4 {
        glam::camera::rh::view::look_at_mat4(self.pos, self.target, Vec3::Y)
    }

    pub fn proj(&self, aspect: f32) -> Mat4 {
        glam::camera::rh::proj::directx::perspective(
            self.fov.to_radians(),
            aspect.max(0.01),
            0.1,
            3000.0,
        )
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let proj = glam::camera::rh::proj::directx::perspective(
            self.fov.to_radians(),
            aspect.max(0.01),
            0.1,
            3000.0,
        );
        proj * glam::camera::rh::view::look_at_mat4(self.pos, self.target, Vec3::Y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Quat, Vec3};

    #[test]
    fn camera_settles_behind() {
        let mut cam = ChaseCamera::default();
        for _ in 0..120 {
            cam.update(Vec3::ZERO, Quat::IDENTITY, 0.0, 1.0 / 60.0);
        }
        assert!(
            cam.position().distance(Vec3::new(0.0, 2.6, -7.0)) < 0.2,
            "{}",
            cam.position()
        );
    }

    #[test]
    fn camera_survives_long_frame() {
        let mut cam = ChaseCamera::default();
        for _ in 0..120 {
            cam.update(Vec3::ZERO, Quat::IDENTITY, 0.0, 1.0 / 60.0);
        }
        // The car moved 10 m during a 2 s stall.
        cam.update(Vec3::new(0.0, 0.0, 10.0), Quat::IDENTITY, 0.0, 2.0);
        let goal = Vec3::new(0.0, 2.6, 3.0);
        assert!(
            cam.position().distance(goal) < 12.0,
            "camera flung to {}",
            cam.position()
        );
    }

    #[test]
    fn fov_widens_with_speed() {
        let mut cam = ChaseCamera::default();
        cam.update(Vec3::ZERO, Quat::IDENTITY, 0.0, 1.0 / 60.0);
        let slow = cam.fov_degrees();
        for _ in 0..120 {
            cam.update(Vec3::ZERO, Quat::IDENTITY, 39.0, 1.0 / 60.0);
        }
        assert!((slow - 70.0).abs() < 0.5 && (cam.fov_degrees() - 82.0).abs() < 0.5);
    }
}
