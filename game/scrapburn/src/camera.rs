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
/// Speed at which the FOV reaches its maximum (top speed + a tier-2 drift boost).
const FOV_FULL_SPEED: f32 = 48.0;
/// Height above the kart that the camera looks out from when checking for walls in the way.
const PIVOT_HEIGHT: f32 = 1.0;
/// Gap kept between the eye and a wall, so the near plane never cuts into it.
const WALL_MARGIN: f32 = 0.45;
/// Closest the camera is pulled in toward the pivot.
const MIN_REACH: f32 = 0.8;
/// How fast the camera eases back out once a wall stops blocking it (1/s).
const REACH_RECOVERY: f32 = 3.0;

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
    /// Point the camera's line of sight to the kart starts from.
    pivot: Vec3,
    /// Share (0..1] of the spring's offset the camera may use without passing a wall.
    reach: f32,
}

impl ChaseCamera {
    pub fn update(&mut self, target_pos: Vec3, target_rot: Quat, speed: f32, dt: f32) {
        let fwd = target_rot * Vec3::Z;
        let flat = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or(Vec3::Z);
        self.pivot = target_pos + Vec3::Y * PIVOT_HEIGHT;
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
            self.reach = 1.0;
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

    /// Pulls the camera in front of anything between it and the kart (walls, terrain), and eases
    /// it back out when the way is clear. `cast(origin, unit_dir, max_t)` returns the distance to
    /// the nearest hit. Call after [`Self::update`].
    pub fn keep_clear(&mut self, cast: impl Fn(Vec3, Vec3, f32) -> Option<f32>, dt: f32) {
        let ray = self.pos - self.pivot;
        let len = ray.length();
        if len < 1e-4 {
            return;
        }
        let dir = ray / len;
        let allowed = match cast(self.pivot, dir, len + WALL_MARGIN) {
            Some(t) => ((t - WALL_MARGIN).max(MIN_REACH) / len).min(1.0),
            None => 1.0,
        };
        if allowed < self.reach {
            // Snap in: a camera that eases through a wall has already shown the inside of it.
            self.reach = allowed;
        } else {
            let k = 1.0 - (-REACH_RECOVERY * dt.clamp(0.0, MAX_FRAME_DT)).exp();
            self.reach += (allowed - self.reach) * k;
        }
        self.pos = self.pivot + ray * self.reach;
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
        // Shake shrinks with the reach, so a camera pressed against a wall can't wobble into it.
        let shake = self.shake * self.reach;
        self.pos + Vec3::new((p * 1.3).sin(), (p * 1.7).cos(), (p * 0.9).sin() * 0.5) * shake
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

    /// A wall plane at `z = wall_z` facing the kart, for `keep_clear`.
    fn wall_at(wall_z: f32) -> impl Fn(Vec3, Vec3, f32) -> Option<f32> {
        move |o: Vec3, d: Vec3, max: f32| {
            if d.z.abs() < 1e-6 {
                return None;
            }
            let t = (wall_z - o.z) / d.z;
            (t >= 0.0 && t <= max).then_some(t)
        }
    }

    #[test]
    fn wall_behind_pulls_the_camera_in_front_of_it() {
        let mut cam = ChaseCamera::default();
        let dt = 1.0 / 60.0;
        for _ in 0..120 {
            cam.update(Vec3::ZERO, Quat::IDENTITY, 0.0, dt);
            cam.keep_clear(wall_at(-2.0), dt);
        }
        // The camera would sit at z = -6; the wall at z = -2 holds it on the kart's side.
        assert!(cam.eye().z > -2.0 + 0.3, "eye at {}", cam.eye());
        assert!(
            cam.eye().z < 0.0,
            "but still behind the kart: {}",
            cam.eye()
        );
    }

    #[test]
    fn camera_snaps_in_at_once_and_eases_back_out() {
        let mut cam = ChaseCamera::default();
        let dt = 1.0 / 60.0;
        for _ in 0..120 {
            cam.update(Vec3::ZERO, Quat::IDENTITY, 0.0, dt);
            cam.keep_clear(|_, _, _| None, dt);
        }
        cam.update(Vec3::ZERO, Quat::IDENTITY, 0.0, dt);
        cam.keep_clear(wall_at(-3.0), dt);
        assert!(
            cam.position().z > -3.0,
            "never shows the wall's inside: {}",
            cam.position()
        );
        cam.update(Vec3::ZERO, Quat::IDENTITY, 0.0, dt);
        cam.keep_clear(|_, _, _| None, dt);
        assert!(
            cam.position().z > -3.5,
            "no jump back out: {}",
            cam.position()
        );
        for _ in 0..240 {
            cam.update(Vec3::ZERO, Quat::IDENTITY, 0.0, dt);
            cam.keep_clear(|_, _, _| None, dt);
        }
        assert!(
            (cam.position().z + BACK).abs() < 0.2,
            "back out: {}",
            cam.position()
        );
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
            cam.update(Vec3::ZERO, Quat::IDENTITY, FOV_FULL_SPEED, 1.0 / 60.0);
        }
        assert!((slow - 70.0).abs() < 0.5 && (cam.fov_degrees() - 82.0).abs() < 0.5);
    }
}
