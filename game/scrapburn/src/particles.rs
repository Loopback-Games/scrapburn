//! CPU particle simulation for sparks, flames and dust.

use glam::Vec3;
use rivet_render::ParticleInstance;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    /// Light-emitting (sparks, flames): adds to the scene.
    Additive,
    /// Occluding (dust, smoke).
    Alpha,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Particle {
    pub pos: Vec3,
    pub vel: Vec3,
    pub age: f32,
    pub life: f32,
    /// Start and end diameter (m).
    pub size: (f32, f32),
    /// Start and end color (linear HDR; alpha fades).
    pub color: ([f32; 4], [f32; 4]),
    pub gravity: f32,
    /// Velocity damping rate (1/s).
    pub drag: f32,
    pub blend: Blend,
}

/// Fixed-capacity pool; when full, the oldest particle is replaced.
pub struct ParticleSystem {
    capacity: usize,
    particles: Vec<Particle>,
    next: usize,
    additive: Vec<ParticleInstance>,
    alpha: Vec<ParticleInstance>,
}

impl ParticleSystem {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            particles: Vec::with_capacity(capacity),
            next: 0,
            additive: Vec::new(),
            alpha: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.particles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }

    pub fn spawn(&mut self, p: Particle) {
        if self.particles.len() < self.capacity {
            self.particles.push(p);
        } else {
            self.particles[self.next] = p;
            self.next = (self.next + 1) % self.capacity;
        }
    }

    pub fn update(&mut self, dt: f32) {
        for p in &mut self.particles {
            p.age += dt;
            p.vel.y -= p.gravity * dt;
            p.vel *= (-p.drag * dt).exp();
            p.pos += p.vel * dt;
        }
        self.particles.retain(|p| p.age < p.life);
        self.next %= self.capacity.max(1);
    }

    /// GPU instances split by blend mode.
    pub fn instances(&mut self) -> (&[ParticleInstance], &[ParticleInstance]) {
        self.additive.clear();
        self.alpha.clear();
        for p in &self.particles {
            let t = (p.age / p.life).clamp(0.0, 1.0);
            let lerp = |a: f32, b: f32| a + (b - a) * t;
            let c = std::array::from_fn(|i| lerp(p.color.0[i], p.color.1[i]));
            let inst = ParticleInstance {
                pos: p.pos.to_array(),
                size: lerp(p.size.0, p.size.1),
                color: c,
            };
            match p.blend {
                Blend::Additive => self.additive.push(inst),
                Blend::Alpha => self.alpha.push(inst),
            }
        }
        (&self.additive, &self.alpha)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn spark() -> Particle {
        Particle {
            pos: Vec3::ZERO,
            vel: Vec3::new(0.0, 5.0, 0.0),
            age: 0.0,
            life: 0.5,
            size: (0.2, 0.0),
            color: ([4.0, 2.0, 0.5, 1.0], [1.0, 0.2, 0.0, 0.0]),
            gravity: 10.0,
            drag: 0.0,
            blend: Blend::Additive,
        }
    }

    #[test]
    fn particles_expire() {
        let mut ps = ParticleSystem::new(16);
        ps.spawn(spark());
        ps.update(0.3);
        assert_eq!(ps.len(), 1);
        ps.update(0.3);
        assert_eq!(ps.len(), 0);
    }

    #[test]
    fn pool_caps_and_recycles_oldest() {
        let mut ps = ParticleSystem::new(4);
        for _ in 0..10 {
            ps.spawn(spark());
        }
        assert_eq!(ps.len(), 4);
    }

    #[test]
    fn motion_size_and_color_interpolate() {
        let mut ps = ParticleSystem::new(4);
        ps.spawn(spark());
        ps.update(0.25);
        let (add, alpha) = ps.instances();
        assert!(alpha.is_empty());
        let p = add[0];
        assert!(p.pos[1] > 0.5 && p.pos[1] < 1.25, "y {}", p.pos[1]);
        assert!((p.size - 0.1).abs() < 1e-4, "size halfway {}", p.size);
        assert!(
            (p.color[0] - 2.5).abs() < 1e-4,
            "color halfway {}",
            p.color[0]
        );
    }
}
