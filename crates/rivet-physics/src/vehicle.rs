//! Arcade kart: raycast suspension, kart-style grip, drift-to-boost and hop.
//!
//! Conventions: +Y up, yaw 0 faces +Z, the kart's right is local -X, `steer > 0` turns right.

use crate::trimesh::{Contact, TriMesh};
use glam::{Quat, Vec3};

/// One tick of player or bot intent.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DriveInput {
    /// 0..1
    pub throttle: f32,
    /// 0..1; brakes while moving forward, reverses when nearly stopped.
    pub brake: f32,
    /// -1..1, positive turns right.
    pub steer: f32,
    /// Held to drift.
    pub drift: bool,
    /// True on the tick the hop button is pressed.
    pub hop: bool,
}

/// A drift-boost tier: drift at least `charge` seconds to earn `speed` m/s extra for `duration` s.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoostTier {
    pub charge: f32,
    pub speed: f32,
    pub duration: f32,
}

/// Tuning for one vehicle class. `Default` is the medium kart.
#[derive(Debug, Clone, PartialEq)]
pub struct VehicleParams {
    pub mass: f32,
    pub max_speed: f32,
    pub accel: f32,
    pub brake_decel: f32,
    pub reverse_max: f32,
    pub coast_decel: f32,
    pub gravity: f32,
    /// Body-center height above flat ground at rest.
    pub ride_height: f32,
    /// Suspension ray length from each corner.
    pub suspension_rest: f32,
    /// Spring stiffness per wheel, per unit mass (1/s²).
    pub spring: f32,
    /// Damping per wheel, per unit mass (1/s).
    pub damping: f32,
    /// Lateral velocity damping rate (1/s).
    pub grip: f32,
    pub drift_grip: f32,
    /// Yaw rate (rad/s) at low speed and at max speed.
    pub steer_rate_low: f32,
    pub steer_rate_high: f32,
    /// Extra yaw (rad/s) into the drift direction.
    pub drift_yaw_bias: f32,
    pub drift_min_speed: f32,
    pub hop_speed: f32,
    pub body_radius: f32,
    /// Acceleration toward the boosted top speed while a boost runs.
    pub boost_accel: f32,
    pub boost_tiers: [BoostTier; 2],
}

impl Default for VehicleParams {
    fn default() -> Self {
        Self {
            mass: 300.0,
            max_speed: 32.0,
            accel: 12.0,
            brake_decel: 30.0,
            reverse_max: 8.0,
            coast_decel: 2.0,
            gravity: 25.0,
            ride_height: 0.45,
            // Equilibrium: rest - gravity / (4 * spring) = 0.55 - 0.104 ≈ ride_height.
            suspension_rest: 0.55,
            spring: 60.0,
            damping: 8.0,
            grip: 10.0,
            drift_grip: 2.5,
            steer_rate_low: 2.2,
            steer_rate_high: 1.1,
            drift_yaw_bias: 0.5,
            drift_min_speed: 10.0,
            hop_speed: 4.5,
            body_radius: 0.8,
            boost_accel: 40.0,
            boost_tiers: [
                BoostTier {
                    charge: 0.6,
                    speed: 4.0,
                    duration: 0.5,
                },
                BoostTier {
                    charge: 1.4,
                    speed: 7.0,
                    duration: 1.0,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DriftState {
    /// -1 (left) or 1 (right), locked when the drift starts.
    pub dir: f32,
    /// Seconds of grounded drifting.
    pub charge: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VehicleState {
    pub position: Vec3,
    pub rotation: Quat,
    pub velocity: Vec3,
    /// Yaw rate applied last tick (rad/s), for camera and effects.
    pub angular_yaw: f32,
    pub grounded: bool,
    pub drift: Option<DriftState>,
    pub boost_timer: f32,
    pub boost_speed: f32,
}

impl VehicleState {
    pub fn spawn(position: Vec3, yaw: f32) -> Self {
        Self {
            position,
            rotation: Quat::from_rotation_y(yaw),
            velocity: Vec3::ZERO,
            angular_yaw: 0.0,
            grounded: false,
            drift: None,
            boost_timer: 0.0,
            boost_speed: 0.0,
        }
    }

    /// Render interpolation between two ticks.
    pub fn lerp(a: &Self, b: &Self, alpha: f32) -> (Vec3, Quat) {
        (
            a.position.lerp(b.position, alpha),
            a.rotation.slerp(b.rotation, alpha),
        )
    }

    pub fn forward(&self) -> Vec3 {
        self.rotation * Vec3::Z
    }

    pub fn speed(&self) -> f32 {
        self.velocity.dot(self.forward())
    }
}

/// Wheel attachment points in body space (x is the kart's left).
const CORNERS: [Vec3; 4] = [
    Vec3::new(0.6, 0.0, 0.9),
    Vec3::new(-0.6, 0.0, 0.9),
    Vec3::new(0.6, 0.0, -0.9),
    Vec3::new(-0.6, 0.0, -0.9),
];
/// Minimum distance from body center to the ground along the ground normal (bump stop).
const MIN_CLEARANCE: f32 = 0.2;
/// Contacts steeper than this (normal.y above it) are ground, handled by suspension.
const GROUND_NORMAL_Y: f32 = 0.7;
/// Share of damped sideways speed handed back to forward speed.
const GRIP_REDIRECT: f32 = 0.9;

fn approach(v: f32, target: f32, rate: f32) -> f32 {
    if v < target {
        (v + rate).min(target)
    } else {
        (v - rate).max(target)
    }
}

/// Advances one fixed tick.
pub fn step(
    state: &mut VehicleState,
    params: &VehicleParams,
    input: DriveInput,
    world: &TriMesh,
    dt: f32,
) {
    let p = params;
    let throttle = input.throttle.clamp(0.0, 1.0);
    let brake = input.brake.clamp(0.0, 1.0);
    let steer = input.steer.clamp(-1.0, 1.0);

    let up = state.rotation * Vec3::Y;

    // --- Suspension ---
    let mut normal_sum = Vec3::ZERO;
    let mut hits = 0;
    let mut accel = Vec3::new(0.0, -p.gravity, 0.0);
    for corner in CORNERS {
        let origin = state.position + state.rotation * corner;
        if let Some(hit) = world.raycast(origin, -up, p.suspension_rest) {
            hits += 1;
            normal_sum += hit.normal;
            let compression = p.suspension_rest - hit.t;
            let closing = state.velocity.dot(up);
            let force = (p.spring * compression - p.damping * closing).max(0.0);
            accel += up * force;
        }
    }
    state.grounded = hits > 0;
    let ground_n = if hits > 0 {
        normal_sum.normalize_or(Vec3::Y)
    } else {
        Vec3::Y
    };

    state.velocity += accel * dt;

    // --- Boost timer, drift state ---
    if state.boost_timer > 0.0 {
        state.boost_timer = (state.boost_timer - dt).max(0.0);
        if state.boost_timer == 0.0 {
            state.boost_speed = 0.0;
        }
    }

    let fwd = state.rotation * Vec3::Z;
    let fwd_g = (fwd - ground_n * fwd.dot(ground_n)).normalize_or(fwd);
    let right_g = ground_n.cross(fwd_g).normalize_or(Vec3::NEG_X) * -1.0;
    let mut speed = state.velocity.dot(fwd_g);

    match state.drift {
        Some(ref mut d) => {
            if !input.drift || speed < p.drift_min_speed * 0.5 {
                let earned = p
                    .boost_tiers
                    .iter()
                    .rev()
                    .find(|t| d.charge >= t.charge)
                    .copied();
                state.drift = None;
                if let Some(t) = earned {
                    state.boost_speed = t.speed;
                    state.boost_timer = t.duration;
                }
            } else if state.grounded {
                d.charge += dt;
            }
        }
        None => {
            if input.drift && state.grounded && steer.abs() > 0.3 && speed > p.drift_min_speed {
                state.drift = Some(DriftState {
                    dir: steer.signum(),
                    charge: 0.0,
                });
            }
        }
    }

    // --- Traction (grounded only) ---
    if state.grounded {
        let boosting = state.boost_timer > 0.0;
        if boosting {
            speed = approach(speed, p.max_speed + state.boost_speed, p.boost_accel * dt);
        } else if throttle > 0.0 && speed >= -0.5 {
            let r = (speed / p.max_speed).clamp(0.0, 1.0);
            speed += p.accel * throttle * (1.0 - r * r) * dt;
            if speed > p.max_speed {
                speed = approach(speed, p.max_speed, p.coast_decel * 4.0 * dt);
            }
        } else if brake > 0.0 && speed > 0.5 {
            speed = (speed - p.brake_decel * brake * dt).max(0.0);
        } else if brake > 0.0 {
            speed = approach(speed, -p.reverse_max, p.accel * brake * dt);
        } else {
            speed = approach(speed, 0.0, p.coast_decel * dt);
        }

        let lateral = state.velocity.dot(right_g);
        let grip = if state.drift.is_some() {
            p.drift_grip
        } else {
            p.grip
        };
        let damped = lateral * (-grip * dt).exp();
        // Arcade grip redirects most of the sideways slide into forward motion.
        speed += speed.signum() * (lateral.abs() - damped.abs()) * GRIP_REDIRECT;
        let lateral = damped;
        let normal_v = state.velocity.dot(ground_n);
        state.velocity = fwd_g * speed + right_g * lateral + ground_n * normal_v;
    }

    // --- Steering ---
    let speed_abs = speed.abs();
    let t = (speed_abs / p.max_speed).clamp(0.0, 1.0);
    let rate = p.steer_rate_low + (p.steer_rate_high - p.steer_rate_low) * t;
    let low_speed_scale = (speed_abs / 5.0).min(1.0);
    let mut yaw_rate = -steer * rate * low_speed_scale * speed.signum();
    if let Some(d) = state.drift {
        yaw_rate -= d.dir * p.drift_yaw_bias;
    }
    if !state.grounded {
        yaw_rate *= 0.3;
    }
    state.angular_yaw = yaw_rate;
    state.rotation = (Quat::from_axis_angle(up, yaw_rate * dt) * state.rotation).normalize();

    // --- Orientation: align to ground, or slowly self-right in the air ---
    let target_up = if state.grounded { ground_n } else { Vec3::Y };
    let align_rate = if state.grounded { 10.0 } else { 2.0 };
    let cur_up = state.rotation * Vec3::Y;
    let target = Quat::from_rotation_arc(cur_up, target_up) * state.rotation;
    state.rotation = state
        .rotation
        .slerp(target, 1.0 - (-align_rate * dt).exp())
        .normalize();

    // --- Hop ---
    if input.hop && state.grounded {
        let n = ground_n;
        state.velocity += n * (p.hop_speed - state.velocity.dot(n)).max(0.0);
    }

    // --- Integrate ---
    state.position += state.velocity * dt;

    // --- Walls ---
    let mut contacts: Vec<Contact> = Vec::new();
    world.sphere_contacts(state.position, p.body_radius, &mut contacts);
    for c in contacts.iter().filter(|c| c.normal.y < GROUND_NORMAL_Y) {
        state.position += c.normal * c.depth;
        let into = state.velocity.dot(c.normal);
        if into < 0.0 {
            state.velocity -= c.normal * into;
        }
    }

    // --- Bump stop: never sink the body into the ground ---
    if let Some(hit) = world.raycast(
        state.position + Vec3::Y * 1.0,
        Vec3::NEG_Y,
        1.0 + MIN_CLEARANCE,
    ) && hit.normal.y >= GROUND_NORMAL_Y
    {
        let clearance = hit.t - 1.0;
        if clearance < MIN_CLEARANCE {
            state.position.y += MIN_CLEARANCE - clearance;
            let into = state.velocity.dot(hit.normal);
            if into < 0.0 {
                state.velocity -= hit.normal * into;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Quat, Vec3};

    const DT: f32 = 1.0 / 60.0;

    fn flat_world() -> TriMesh {
        let h = 400.0;
        let p = vec![
            Vec3::new(-h, 0.0, -h),
            Vec3::new(h, 0.0, -h),
            Vec3::new(h, 0.0, h),
            Vec3::new(-h, 0.0, h),
        ];
        TriMesh::new(p, vec![[0, 2, 1], [0, 3, 2]], 8.0)
    }

    fn walled_world() -> TriMesh {
        // Flat ground plus a wall in the z = 50 plane.
        let h = 400.0;
        let p = vec![
            Vec3::new(-h, 0.0, -h),
            Vec3::new(h, 0.0, -h),
            Vec3::new(h, 0.0, h),
            Vec3::new(-h, 0.0, h),
            Vec3::new(-50.0, -1.0, 50.0),
            Vec3::new(50.0, -1.0, 50.0),
            Vec3::new(50.0, 5.0, 50.0),
            Vec3::new(-50.0, 5.0, 50.0),
        ];
        TriMesh::new(p, vec![[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7]], 8.0)
    }

    fn forward(s: &VehicleState) -> Vec3 {
        s.rotation * Vec3::Z
    }

    fn forward_speed(s: &VehicleState) -> f32 {
        s.velocity.dot(forward(s))
    }

    fn heading(s: &VehicleState) -> f32 {
        let f = forward(s);
        f.x.atan2(f.z)
    }

    fn settled(world: &TriMesh, p: &VehicleParams) -> VehicleState {
        let mut s = VehicleState::spawn(Vec3::new(0.0, 1.0, 0.0), 0.0);
        for _ in 0..120 {
            step(&mut s, p, DriveInput::default(), world, DT);
        }
        s
    }

    fn with_speed(world: &TriMesh, p: &VehicleParams, v: f32) -> VehicleState {
        let mut s = settled(world, p);
        s.velocity = forward(&s) * v;
        s
    }

    #[test]
    fn settles_at_ride_height() {
        let w = flat_world();
        let p = VehicleParams::default();
        let mut s = VehicleState::spawn(Vec3::new(0.0, 2.0, 0.0), 0.0);
        for _ in 0..180 {
            step(&mut s, &p, DriveInput::default(), &w, DT);
        }
        assert!(s.grounded);
        assert!(
            (s.position.y - p.ride_height).abs() < 0.05,
            "y = {}",
            s.position.y
        );
        assert!(s.velocity.length() < 0.05, "v = {}", s.velocity);
    }

    #[test]
    fn accelerates_to_max_speed() {
        let w = flat_world();
        let p = VehicleParams::default();
        let mut s = settled(&w, &p);
        let input = DriveInput {
            throttle: 1.0,
            ..Default::default()
        };
        for i in 1..=600 {
            step(&mut s, &p, input, &w, DT);
            if i == 60 {
                let v = forward_speed(&s);
                assert!((8.0..=14.0).contains(&v), "speed at 1 s = {v}");
            }
        }
        let v = forward_speed(&s);
        assert!((v - p.max_speed).abs() < 0.5, "speed at 10 s = {v}");
    }

    #[test]
    fn steers_right() {
        let w = flat_world();
        let p = VehicleParams::default();
        let mut s = with_speed(&w, &p, 20.0);
        let h0 = heading(&s);
        let input = DriveInput {
            steer: 1.0,
            ..Default::default()
        };
        for _ in 0..60 {
            step(&mut s, &p, input, &w, DT);
        }
        let turned = h0 - heading(&s); // right turn = heading decreases
        assert!((0.6..=2.0).contains(&turned), "turned {turned} rad");
        assert!(s.velocity.length() > 15.0, "speed {}", s.velocity.length());
    }

    #[test]
    fn drift_tier2_boost() {
        let w = flat_world();
        let p = VehicleParams::default();
        let mut s = with_speed(&w, &p, 25.0);
        let drifting = DriveInput {
            throttle: 1.0,
            steer: 1.0,
            drift: true,
            ..Default::default()
        };
        for _ in 0..100 {
            step(&mut s, &p, drifting, &w, DT);
        }
        assert!(s.drift.is_some(), "should be drifting");
        let release = DriveInput {
            throttle: 1.0,
            ..Default::default()
        };
        step(&mut s, &p, release, &w, DT);
        assert_eq!(s.boost_speed, 7.0);
        assert!(
            (s.boost_timer - 1.0).abs() < 0.02,
            "timer {}",
            s.boost_timer
        );
        for _ in 0..10 {
            step(&mut s, &p, release, &w, DT);
        }
        assert!(
            forward_speed(&s) > p.max_speed,
            "speed {}",
            forward_speed(&s)
        );
    }

    #[test]
    fn short_drift_no_boost() {
        let w = flat_world();
        let p = VehicleParams::default();
        let mut s = with_speed(&w, &p, 25.0);
        let drifting = DriveInput {
            throttle: 1.0,
            steer: 1.0,
            drift: true,
            ..Default::default()
        };
        for _ in 0..20 {
            step(&mut s, &p, drifting, &w, DT);
        }
        step(
            &mut s,
            &p,
            DriveInput {
                throttle: 1.0,
                ..Default::default()
            },
            &w,
            DT,
        );
        assert_eq!(s.boost_speed, 0.0);
    }

    #[test]
    fn hop_only_when_grounded() {
        let w = flat_world();
        let p = VehicleParams::default();
        let mut s = settled(&w, &p);
        let hop = DriveInput {
            hop: true,
            ..Default::default()
        };
        step(&mut s, &p, hop, &w, DT);
        assert!(
            (s.velocity.y - p.hop_speed).abs() < 0.5,
            "vy {}",
            s.velocity.y
        );
        for _ in 0..10 {
            step(&mut s, &p, DriveInput::default(), &w, DT);
        }
        assert!(!s.grounded);
        let vy = s.velocity.y;
        step(&mut s, &p, hop, &w, DT);
        assert!(
            s.velocity.y < vy + 0.1,
            "airborne hop added velocity: {vy} -> {}",
            s.velocity.y
        );
    }

    #[test]
    fn wall_stops_penetration() {
        let w = walled_world();
        let p = VehicleParams::default();
        let mut s = with_speed(&w, &p, 30.0);
        let input = DriveInput {
            throttle: 1.0,
            ..Default::default()
        };
        for _ in 0..120 {
            step(&mut s, &p, input, &w, DT);
            let gap = 50.0 - s.position.z;
            assert!(gap >= p.body_radius - 0.05, "center {gap} m from wall");
        }
    }

    #[test]
    fn deterministic() {
        let w = flat_world();
        let p = VehicleParams::default();
        let run = || {
            let mut s = VehicleState::spawn(Vec3::new(0.0, 1.0, 0.0), 0.3);
            for i in 0..600 {
                let t = i as f32 * DT;
                let input = DriveInput {
                    throttle: 1.0,
                    steer: (t * 1.3).sin(),
                    drift: (i / 90) % 2 == 1,
                    hop: i % 90 == 0,
                    ..Default::default()
                };
                step(&mut s, &p, input, &w, DT);
            }
            s
        };
        let (a, b) = (run(), run());
        let bits = |s: &VehicleState| {
            let mut v: Vec<u32> = Vec::new();
            v.extend(s.position.to_array().map(f32::to_bits));
            v.extend(s.velocity.to_array().map(f32::to_bits));
            v.extend(s.rotation.to_array().map(f32::to_bits));
            v.push(s.boost_timer.to_bits());
            v
        };
        assert_eq!(bits(&a), bits(&b));
        let _ = Quat::IDENTITY;
    }
}
