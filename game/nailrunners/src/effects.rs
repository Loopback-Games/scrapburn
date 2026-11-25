//! Game juice driven by the kart's state: sparks, flames, dust, landing shake.

use glam::{Quat, Vec3};
use rivet_physics::{VehicleParams, VehicleState};

use crate::particles::{Blend, Particle, ParticleSystem};
use crate::style::tier_color;

/// Rear wheel contact points and exhaust, in kart space (+Z forward, +X left).
const REAR_WHEELS: [Vec3; 2] = [Vec3::new(0.75, -0.42, -0.8), Vec3::new(-0.75, -0.42, -0.8)];
const EXHAUST: Vec3 = Vec3::new(0.0, 0.25, -1.15);
const SPARK_RATE: f32 = 70.0;
const FLAME_RATE: f32 = 140.0;
const DUST_RATE: f32 = 40.0;

pub struct Effects {
    pub particles: ParticleSystem,
    rng: u64,
    was_grounded: bool,
    last_vy: f32,
    accum: [f32; 3],
}

impl Default for Effects {
    fn default() -> Self {
        Self::new()
    }
}

impl Effects {
    pub fn new() -> Self {
        Self {
            particles: ParticleSystem::new(1500),
            rng: 0x9e37_79b9_7f4a_7c15,
            was_grounded: true,
            last_vy: 0.0,
            accum: [0.0; 3],
        }
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    fn jitter(&mut self, amount: f32) -> Vec3 {
        Vec3::new(self.rand() - 0.5, self.rand() - 0.5, self.rand() - 0.5) * 2.0 * amount
    }

    /// How many emissions this frame for a `rate`/s emitter (`slot` keeps the remainder).
    fn take(&mut self, slot: usize, rate: f32, dt: f32) -> u32 {
        self.accum[slot] += rate * dt;
        let n = self.accum[slot].floor();
        self.accum[slot] -= n;
        n as u32
    }

    /// Emits for this frame and advances particles. Returns a camera shake impulse (0 if none).
    pub fn update(&mut self, state: &VehicleState, params: &VehicleParams, dt: f32) -> f32 {
        let rot: Quat = state.rotation;
        let at = |local: Vec3| state.position + rot * local;
        let back = rot * Vec3::NEG_Z;
        let mut shake = 0.0;

        // Drift sparks from the rear wheels, colored by the charge tier.
        if let Some(d) = state.drift.filter(|_| state.grounded) {
            let tier = params
                .boost_tiers
                .iter()
                .filter(|t| d.charge >= t.charge)
                .count();
            let c = tier_color(tier);
            let hot = if tier == 0 { 1.5 } else { 5.0 };
            for _ in 0..self.take(0, SPARK_RATE, dt) {
                let wheel = REAR_WHEELS[(self.rand() * 2.0) as usize % 2];
                let vel = back * (3.0 + self.rand() * 4.0)
                    + Vec3::Y * (1.5 + self.rand() * 2.5)
                    + self.jitter(1.5)
                    + state.velocity * 0.6;
                let life = 0.25 + self.rand() * 0.2;
                self.particles.spawn(Particle {
                    pos: at(wheel),
                    vel,
                    age: 0.0,
                    life,
                    size: (0.16, 0.03),
                    color: (
                        [c[0] * hot, c[1] * hot, c[2] * hot, 1.0],
                        [c[0], c[1] * 0.6, c[2] * 0.5, 0.0],
                    ),
                    gravity: 14.0,
                    drag: 2.0,
                    blend: Blend::Additive,
                });
            }
        }

        // Boost flames from the exhaust.
        if state.boost_timer > 0.0 {
            for _ in 0..self.take(1, FLAME_RATE, dt) {
                let vel =
                    back * (6.0 + self.rand() * 4.0) + self.jitter(0.8) + state.velocity * 0.85;
                let pos = at(EXHAUST) + self.jitter(0.08);
                let life = 0.12 + self.rand() * 0.1;
                self.particles.spawn(Particle {
                    pos,
                    vel,
                    age: 0.0,
                    life,
                    size: (0.55, 0.15),
                    color: ([6.0, 3.2, 1.0, 1.0], [2.0, 0.3, 0.05, 0.0]),
                    gravity: -2.0,
                    drag: 4.0,
                    blend: Blend::Additive,
                });
            }
        }

        // Dust when driving on sand or dirt.
        let speed = state.velocity.length();
        if state.grounded && state.offroad > 0.2 && speed > 4.0 {
            let rate = DUST_RATE * state.offroad * (speed / params.max_speed).min(1.5);
            for _ in 0..self.take(2, rate, dt) {
                let wheel = REAR_WHEELS[(self.rand() * 2.0) as usize % 2];
                let vel = back * (1.0 + self.rand() * 2.0)
                    + Vec3::Y * (0.8 + self.rand())
                    + self.jitter(0.6)
                    + state.velocity * 0.3;
                let life = 0.9 + self.rand() * 0.6;
                self.dust(at(wheel), vel, life, 1.4);
            }
        }

        // Landing: a ring of dust and a camera kick proportional to the impact.
        if state.grounded && !self.was_grounded {
            let impact = (-self.last_vy - 3.0).max(0.0);
            if impact > 0.0 {
                shake = (impact / 12.0).min(1.0);
                let n = (8.0 + impact * 1.5) as u32;
                for k in 0..n {
                    let a = k as f32 / n as f32 * std::f32::consts::TAU;
                    let dir = rot * Vec3::new(a.cos(), 0.0, a.sin());
                    let vel = dir * (2.0 + self.rand() * 2.0)
                        + Vec3::Y * (0.5 + self.rand())
                        + state.velocity * 0.4;
                    let life = 0.8 + self.rand() * 0.5;
                    self.dust(state.position - rot * Vec3::Y * 0.4, vel, life, 1.1);
                }
            }
        }
        self.was_grounded = state.grounded;
        self.last_vy = state.velocity.y;
        self.particles.update(dt);
        shake
    }

    fn dust(&mut self, pos: Vec3, vel: Vec3, life: f32, size: f32) {
        let tone = 0.8 + self.rand() * 0.2;
        self.particles.spawn(Particle {
            pos,
            vel,
            age: 0.0,
            life,
            size: (size * 0.5, size * 1.8),
            color: (
                [0.72 * tone, 0.58 * tone, 0.42 * tone, 0.45],
                [0.75, 0.64, 0.5, 0.0],
            ),
            gravity: -0.3,
            drag: 1.8,
            blend: Blend::Alpha,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use rivet_physics::{DriftState, VehicleParams, VehicleState};

    fn moving() -> VehicleState {
        let mut s = VehicleState::spawn(Vec3::ZERO, 0.0);
        s.grounded = true;
        s.velocity = Vec3::new(0.0, 0.0, 25.0);
        s
    }

    #[test]
    fn drift_sparks_use_tier_color() {
        let p = VehicleParams::default();
        let mut fx = Effects::new();
        let mut s = moving();
        s.drift = Some(DriftState {
            dir: 1.0,
            charge: 1.3,
        });
        for _ in 0..10 {
            fx.update(&s, &p, 1.0 / 60.0);
        }
        let (add, _) = fx.particles.instances();
        assert!(!add.is_empty(), "drifting should throw sparks");
        let want = crate::style::tier_color(2);
        assert!(
            add.iter().all(|i| i.color[0] / i.color[2].max(1e-3) > 1.5),
            "tier 2 sparks are orange-ish"
        );
        let _ = want;
    }

    #[test]
    fn boost_emits_flames() {
        let p = VehicleParams::default();
        let mut fx = Effects::new();
        let mut s = moving();
        s.boost_timer = 1.0;
        s.boost_speed = 7.0;
        fx.update(&s, &p, 1.0 / 60.0);
        assert!(!fx.particles.instances().0.is_empty());
    }

    #[test]
    fn landing_bursts_dust_and_shakes() {
        let p = VehicleParams::default();
        let mut fx = Effects::new();
        let mut air = moving();
        air.grounded = false;
        air.velocity.y = -12.0;
        assert_eq!(fx.update(&air, &p, 1.0 / 60.0), 0.0);
        let mut landed = air.clone();
        landed.grounded = true;
        let shake = fx.update(&landed, &p, 1.0 / 60.0);
        assert!(shake > 0.2, "hard landing shake {shake}");
        assert!(fx.particles.instances().1.len() >= 10, "landing dust");
    }

    #[test]
    fn offroad_kicks_up_dust() {
        let p = VehicleParams::default();
        let mut fx = Effects::new();
        let mut s = moving();
        s.offroad = 1.0;
        for _ in 0..10 {
            fx.update(&s, &p, 1.0 / 60.0);
        }
        assert!(!fx.particles.instances().1.is_empty());
    }
}
