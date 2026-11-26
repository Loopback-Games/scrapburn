//! Spring-damped chase camera.

use glam::{Mat4, Quat, Vec3};

const BACK: f32 = 6.0;
const HEIGHT: f32 = 2.2;
const LOOK_AHEAD: f32 = 3.0;
const OMEGA: f32 = 8.0;
const FOV_MIN: f32 = 70.0;
const SPEED_PULLBACK: f32 = 0.025;
const BOOST_FOV: f32 = 9.0;
const SHAKE_MAX: f32 = 0.35;
const SHAKE_DECAY: f32 = 7.0;
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
    offset: Vec3,
    boost: bool,
    /// Shake amplitude (m), decays; `phase` drives the wobble.
    shake: f32,
    phase: f32,
}

impl ChaseCamera {
    pub fn update(&mut self, target_pos: Vec3, target_rot: Quat, speed: f32, dt: f32) {
        let fwd = target_rot * Vec3::Z;
        let flat = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or(Vec3::Z);
        // Pull back slightly with speed, so fast sections feel fast without losing the kart.
        let back = BACK + speed.abs() * SPEED_PULLBACK;
        let desired_offset = -flat * back + Vec3::Y * HEIGHT;
        self.target = target_pos + flat * LOOK_AHEAD + Vec3::Y * 1.0;
        let boost_fov = if self.boost { BOOST_FOV } else { 0.0 };
        let fov_goal = boost_fov
            + FOV_MIN
            + (FOV_MAX - FOV_MIN) * (speed.abs() / FOV_FULL_SPEED).clamp(0.0, 1.0);
        if !self.initialized {
            self.offset = desired_offset;
            self.pos = target_pos + desired_offset;
            self.vel = Vec3::ZERO;
            self.fov = fov_goal;
            self.initialized = true;
            return;
        }
        // Critically damped spring on the offset from the kart (not world position), so the
        // camera never trails at speed but still smooths turns. Explicit Euler is only stable for
        // small ω·dt, so cap long frames (hitches) and integrate in small substeps.
        let dt = dt.clamp(0.0, MAX_FRAME_DT);
        let steps = (dt / SPRING_SUBSTEP).ceil().max(1.0);
        let h = dt / steps;
        for _ in 0..steps as u32 {
            let accel = (desired_offset - self.offset) * (OMEGA * OMEGA) - self.vel * (2.0 * OMEGA);
            self.vel += accel * h;
            self.offset += self.vel * h;
        }
        self.pos = target_pos + self.offset;
        self.fov += (fov_goal - self.fov) * (1.0 - (-4.0 * dt).exp());
        self.shake *= (-SHAKE_DECAY * dt).exp();
        self.phase += dt * 37.0;
    }

    /// Widens the view while a boost runs.
    pub fn set_boost(&mut self, on: bool) {
        self.boost = on;
    }

    /// Adds a camera shake impulse (0..1, e.g. a hard landing).
    pub fn kick(&mut self, amount: f32) {
        self.shake = (self.shake + amount * SHAKE_MAX).min(SHAKE_MAX);
    }

    /// Camera position including shake.
    pub fn eye(&self) -> Vec3 {
        let p = self.phase;
        self.pos + Vec3::new((p * 1.3).sin(), (p * 1.7).cos(), (p * 0.9).sin() * 0.5) * self.shake
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
        glam::camera::rh::view::look_at_mat4(self.eye(), self.target, Vec3::Y)
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
            cam.position().distance(Vec3::new(0.0, HEIGHT, -BACK)) < 0.2,
            "{}",
            cam.position()
        );
    }

    #[test]
    fn camera_keeps_up_at_speed() {
        let mut cam = ChaseCamera::default();
        let dt = 1.0 / 60.0;
        for i in 0..120 {
            let target = Vec3::new(0.0, 0.0, i as f32 * 40.0 * dt);
            cam.update(target, Quat::IDENTITY, 40.0, dt);
        }
        let target = Vec3::new(0.0, 0.0, 119.0 * 40.0 * dt);
        let dist = cam.position().distance(target);
        assert!(dist < 8.5, "camera trails {dist} m behind at 40 m/s");
    }

    #[test]
    fn camera_survives_long_frame() {
        let mut cam = ChaseCamera::default();
        for _ in 0..120 {
            cam.update(Vec3::ZERO, Quat::IDENTITY, 0.0, 1.0 / 60.0);
        }
        // The car moved 10 m during a 2 s stall.
        cam.update(Vec3::new(0.0, 0.0, 10.0), Quat::IDENTITY, 0.0, 2.0);
        let goal = Vec3::new(0.0, HEIGHT, 10.0 - BACK);
        assert!(
            cam.position().distance(goal) < 12.0,
            "camera flung to {}",
            cam.position()
        );
    }

    #[test]
    fn kick_shakes_then_settles() {
        let mut cam = ChaseCamera::default();
        for _ in 0..120 {
            cam.update(Vec3::ZERO, Quat::IDENTITY, 0.0, 1.0 / 60.0);
        }
        let rest = cam.eye();
        cam.kick(1.0);
        cam.update(Vec3::ZERO, Quat::IDENTITY, 0.0, 1.0 / 60.0);
        assert!(cam.eye().distance(rest) > 0.02, "kick moves the eye");
        for _ in 0..120 {
            cam.update(Vec3::ZERO, Quat::IDENTITY, 0.0, 1.0 / 60.0);
        }
        assert!(cam.eye().distance(rest) < 0.01, "and settles");
    }

    #[test]
    fn boost_widens_fov() {
        let mut cam = ChaseCamera::default();
        cam.update(Vec3::ZERO, Quat::IDENTITY, 30.0, 1.0 / 60.0);
        let base = cam.fov_degrees();
        cam.set_boost(true);
        for _ in 0..60 {
            cam.update(Vec3::ZERO, Quat::IDENTITY, 30.0, 1.0 / 60.0);
        }
        assert!(cam.fov_degrees() > base + 5.0);
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
