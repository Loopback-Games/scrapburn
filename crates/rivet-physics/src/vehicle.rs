//! Arcade kart: raycast suspension, kart-style grip, drift-to-boost and hop.
//!
//! Conventions: +Y up, yaw 0 faces +Z, the kart's right is local -X, `steer > 0` turns right.

use crate::trimesh::{Contact, Surface, TriMesh};
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
    /// Drift yaw rate as a multiple of the normal steer rate: full counter-steer gives `min`
    /// (negative: the kart rotates back against the drift), steering into it gives `max`.
    pub drift_arc_min: f32,
    pub drift_arc_max: f32,
    /// Extra charge rate at full tightness (1.0 + this).
    pub drift_tight_charge_bonus: f32,
    pub drift_min_speed: f32,
    pub hop_speed: f32,
    pub body_radius: f32,
    /// Acceleration toward the boosted top speed while a boost runs.
    pub boost_accel: f32,
    /// Drift boost tiers, ascending by charge.
    pub boost_tiers: [BoostTier; 3],
    pub boost_pad: BoostTier,
    /// Top speed multiplier when fully on offroad surfaces.
    pub offroad_speed_scale: f32,
    /// Deceleration (m/s²) above the offroad top speed.
    pub offroad_drag: f32,
    /// Share of the tangential speed lost per unit of head-on wall impact.
    pub wall_scrub: f32,
}

impl VehicleParams {
    /// The best boost earned by `charge` seconds of drifting, if any.
    pub fn boost_for_charge(&self, charge: f32) -> Option<BoostTier> {
        self.boost_tiers
            .iter()
            .rev()
            .find(|t| charge >= t.charge)
            .copied()
    }
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
            drift_arc_min: -0.35,
            drift_arc_max: 1.45,
            drift_tight_charge_bonus: 0.6,
            drift_min_speed: 10.0,
            hop_speed: 4.5,
            body_radius: 0.8,
            boost_accel: 40.0,
            boost_tiers: [
                BoostTier {
                    charge: 0.55,
                    speed: 5.0,
                    duration: 0.6,
                },
                BoostTier {
                    charge: 1.2,
                    speed: 7.0,
                    duration: 1.1,
                },
                BoostTier {
                    charge: 2.0,
                    speed: 9.0,
                    duration: 1.7,
                },
            ],
            boost_pad: BoostTier {
                charge: 0.0,
                speed: 8.0,
                duration: 1.2,
            },
            offroad_speed_scale: 0.55,
            offroad_drag: 10.0,
            wall_scrub: 0.25,
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
    /// Share of grounded wheels on offroad surfaces last tick (0..1), for effects.
    pub offroad: f32,
    /// Seconds of spin-out left (hit by a weapon): no control while > 0.
    pub spin: f32,
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
            offroad: 0.0,
            spin: 0.0,
        }
    }

    /// Knocks the kart into a spin for `secs`, keeping `keep` of its speed. Cancels drift and
    /// boost.
    pub fn spin_out(&mut self, secs: f32, keep: f32) {
        self.spin = self.spin.max(secs);
        self.velocity *= keep.clamp(0.0, 1.0);
        self.drift = None;
        self.boost_timer = 0.0;
        self.boost_speed = 0.0;
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
/// Yaw rate while spun out (rad/s).
const SPIN_YAW_RATE: f32 = 9.0;
/// Bounciness of kart-to-kart hits.
const KART_RESTITUTION: f32 = 0.3;

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
    // Spinning out: the player has no control until it ends.
    let spinning = state.spin > 0.0;
    let input = if spinning {
        DriveInput::default()
    } else {
        input
    };
    state.spin = (state.spin - dt).max(0.0);
    let throttle = input.throttle.clamp(0.0, 1.0);
    let brake = input.brake.clamp(0.0, 1.0);
    let steer = input.steer.clamp(-1.0, 1.0);

    let up = state.rotation * Vec3::Y;

    // --- Suspension ---
    let mut normal_sum = Vec3::ZERO;
    let mut hits = 0;
    let mut offroad_hits = 0;
    let mut on_pad = false;
    let mut accel = Vec3::new(0.0, -p.gravity, 0.0);
    for corner in CORNERS {
        let origin = state.position + state.rotation * corner;
        if let Some(hit) = world.raycast(origin, -up, p.suspension_rest) {
            hits += 1;
            normal_sum += hit.normal;
            match hit.surface {
                Surface::Offroad => offroad_hits += 1,
                Surface::BoostPad => on_pad = true,
                Surface::Road | Surface::Wall => {}
            }
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
    let offroad = if hits > 0 {
        offroad_hits as f32 / hits as f32
    } else {
        0.0
    };
    state.offroad = offroad;
    let top_speed = p.max_speed * (1.0 + (p.offroad_speed_scale - 1.0) * offroad);

    // --- Boost timer, drift state ---
    if state.boost_timer > 0.0 {
        state.boost_timer = (state.boost_timer - dt).max(0.0);
        if state.boost_timer == 0.0 {
            state.boost_speed = 0.0;
        }
    }

    if on_pad {
        state.boost_speed = state.boost_speed.max(p.boost_pad.speed);
        state.boost_timer = state.boost_timer.max(p.boost_pad.duration);
    }

    let fwd = state.rotation * Vec3::Z;
    let fwd_g = (fwd - ground_n * fwd.dot(ground_n)).normalize_or(fwd);
    let right_g = ground_n.cross(fwd_g).normalize_or(Vec3::NEG_X) * -1.0;
    let mut speed = state.velocity.dot(fwd_g);

    match state.drift {
        Some(ref mut d) => {
            if !input.drift || speed < p.drift_min_speed * 0.5 {
                let earned = p.boost_for_charge(d.charge);
                state.drift = None;
                if let Some(t) = earned {
                    state.boost_speed = state.boost_speed.max(t.speed);
                    state.boost_timer = state.boost_timer.max(t.duration);
                }
            } else if state.grounded && offroad < 0.5 {
                let tightness = (steer * d.dir + 1.0) * 0.5;
                d.charge += dt * (1.0 + p.drift_tight_charge_bonus * tightness);
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
        } else if speed > top_speed {
            // Faster than this surface allows (left a boost, ran onto grass).
            let drag = if offroad > 0.0 {
                p.offroad_drag
            } else {
                p.coast_decel * 4.0
            };
            speed = approach(speed, top_speed, drag * dt);
        } else if throttle > 0.0 && speed >= -0.5 {
            let r = (speed / top_speed).clamp(0.0, 1.0);
            speed += p.accel * throttle * (1.0 - r * r) * dt;
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
    let mut yaw_rate = match state.drift {
        // Drifting: the turn direction is locked; steering only tightens or widens the arc.
        Some(d) => {
            let tightness = (steer * d.dir + 1.0) * 0.5;
            let arc = p.drift_arc_min + (p.drift_arc_max - p.drift_arc_min) * tightness;
            -d.dir * rate * arc * low_speed_scale
        }
        None => -steer * rate * low_speed_scale * speed.signum(),
    };
    if !state.grounded {
        yaw_rate *= 0.3;
    }
    if spinning {
        yaw_rate = SPIN_YAW_RATE;
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
    // Only wall-like faces push the body; floors (and their edges) belong to the suspension.
    for c in contacts
        .iter()
        .filter(|c| c.face_normal.y.abs() < GROUND_NORMAL_Y)
    {
        state.position += c.normal * c.depth;
        let into = state.velocity.dot(c.normal);
        if into < 0.0 {
            let speed_before = state.velocity.length().max(1e-3);
            state.velocity -= c.normal * into;
            // Scrub speed in proportion to how head-on the hit was.
            state.velocity *= 1.0 - p.wall_scrub * (-into / speed_before).min(1.0);
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

/// Resolves overlap and impact between two karts modeled as spheres of `radius`. Masses decide
/// who gets shoved: momentum is conserved and the lighter kart moves more.
pub fn collide_karts(
    a: &mut VehicleState,
    mass_a: f32,
    b: &mut VehicleState,
    mass_b: f32,
    radius: f32,
) {
    let d = b.position - a.position;
    let dist = d.length();
    if dist >= 2.0 * radius || dist < 1e-6 {
        return;
    }
    let n = d / dist;
    let total = mass_a + mass_b;
    let overlap = 2.0 * radius - dist;
    a.position -= n * overlap * (mass_b / total);
    b.position += n * overlap * (mass_a / total);
    let closing = (b.velocity - a.velocity).dot(n);
    if closing < 0.0 {
        let j = -(1.0 + KART_RESTITUTION) * closing / (1.0 / mass_a + 1.0 / mass_b);
        a.velocity -= n * (j / mass_a);
        b.velocity += n * (j / mass_b);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::trimesh::Surface;
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

    /// Ground built from 2 m tiles, like real track meshes.
    fn tiled_world() -> TriMesh {
        let (mut p, mut idx) = (Vec::new(), Vec::new());
        for zi in -10..200 {
            for xi in -10..10 {
                let (x, z) = (xi as f32 * 2.0, zi as f32 * 2.0);
                let b = p.len() as u32;
                p.extend([
                    Vec3::new(x, 0.0, z),
                    Vec3::new(x + 2.0, 0.0, z),
                    Vec3::new(x + 2.0, 0.0, z + 2.0),
                    Vec3::new(x, 0.0, z + 2.0),
                ]);
                idx.extend([[b, b + 2, b + 1], [b, b + 3, b + 2]]);
            }
        }
        TriMesh::new(p, idx, 8.0)
    }

    #[test]
    fn tile_seams_do_not_snag() {
        let w = tiled_world();
        let p = VehicleParams::default();
        let mut s = VehicleState::spawn(Vec3::new(0.3, 1.0, 0.0), 0.0);
        let input = DriveInput {
            throttle: 1.0,
            ..Default::default()
        };
        for _ in 0..300 {
            step(&mut s, &p, input, &w, DT);
        }
        assert!(
            forward_speed(&s) > 25.0,
            "speed after 5 s on tiles: {}",
            forward_speed(&s)
        );
        assert!(
            s.position.x.abs() < 1.0,
            "pushed sideways to x = {}",
            s.position.x
        );
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

    fn flat_world_with(surface: Surface) -> TriMesh {
        let h = 400.0;
        let p = vec![
            Vec3::new(-h, 0.0, -h),
            Vec3::new(h, 0.0, -h),
            Vec3::new(h, 0.0, h),
            Vec3::new(-h, 0.0, h),
        ];
        TriMesh::with_surfaces(p, vec![[0, 2, 1], [0, 3, 2]], vec![surface; 2], 8.0)
    }

    #[test]
    fn boost_tiers_by_charge() {
        let p = VehicleParams::default();
        assert_eq!(p.boost_for_charge(0.5), None);
        assert_eq!(p.boost_for_charge(0.6).map(|t| t.speed), Some(5.0));
        assert_eq!(p.boost_for_charge(1.3).map(|t| t.speed), Some(7.0));
        assert_eq!(
            p.boost_for_charge(2.1).map(|t| (t.speed, t.duration)),
            Some((9.0, 1.7))
        );
    }

    #[test]
    fn tight_drift_charges_faster_and_turns_more() {
        let w = flat_world();
        let p = VehicleParams::default();
        let run = |steer_after_start: f32| {
            let mut s = with_speed(&w, &p, 25.0);
            let h0 = heading(&s);
            step(
                &mut s,
                &p,
                DriveInput {
                    throttle: 1.0,
                    steer: 1.0,
                    drift: true,
                    hop: true,
                    ..Default::default()
                },
                &w,
                DT,
            );
            for _ in 0..40 {
                step(
                    &mut s,
                    &p,
                    DriveInput {
                        throttle: 1.0,
                        steer: steer_after_start,
                        drift: true,
                        ..Default::default()
                    },
                    &w,
                    DT,
                );
            }
            (s.drift.expect("drifting").charge, h0 - heading(&s))
        };
        let (tight_charge, tight_turn) = run(1.0);
        let (wide_charge, wide_turn) = run(-1.0);
        assert!(
            tight_charge > wide_charge * 1.3,
            "tight {tight_charge} vs wide {wide_charge}"
        );
        assert!(
            tight_turn > wide_turn * 1.5,
            "tight {tight_turn} vs wide {wide_turn}"
        );
        assert!(
            wide_turn < 0.0,
            "full counter-steer turns against the drift (tight {tight_turn}, counter {wide_turn})"
        );
    }

    #[test]
    fn offroad_caps_speed() {
        let w = flat_world_with(Surface::Offroad);
        let p = VehicleParams::default();
        let mut s = settled(&w, &p);
        for _ in 0..600 {
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
        }
        let v = forward_speed(&s);
        assert!(
            (v - p.max_speed * p.offroad_speed_scale).abs() < 0.6,
            "offroad speed {v}"
        );
        assert_eq!(s.offroad, 1.0);
    }

    #[test]
    fn boost_pad_triggers_boost() {
        let w = flat_world_with(Surface::BoostPad);
        let p = VehicleParams::default();
        let s = settled(&w, &p);
        assert_eq!(s.boost_speed, p.boost_pad.speed);
        assert!(s.boost_timer > p.boost_pad.duration - 0.05);
    }

    #[test]
    fn fast_karts_do_not_tunnel_through_walls() {
        let w = walled_world();
        let p = VehicleParams::default();
        let mut s = settled(&w, &p);
        s.position.z = 46.0;
        s.velocity = Vec3::new(0.0, 0.0, 80.0); // 1.33 m per tick, more than the body radius
        for _ in 0..30 {
            step(&mut s, &p, DriveInput::default(), &w, DT);
            assert!(
                s.position.z < 50.0,
                "tunneled through the wall to z = {}",
                s.position.z
            );
        }
    }

    #[test]
    fn glancing_wall_hit_keeps_most_speed() {
        let w = walled_world();
        let p = VehicleParams::default();
        let mut s = settled(&w, &p);
        // Head 30 degrees off the wall's tangent, toward the z = 50 wall.
        s.rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_3);
        s.position.z = 45.0;
        s.velocity = s.rotation * Vec3::Z * 30.0;
        for _ in 0..60 {
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
        }
        let v = s.velocity.length();
        assert!(v > 18.0 && v < 31.0, "speed after glancing hit {v}");
    }

    #[test]
    fn spin_out_takes_control_and_scrubs_speed() {
        let w = flat_world();
        let p = VehicleParams::default();
        let mut s = with_speed(&w, &p, 30.0);
        s.spin_out(1.0, 0.7);
        assert!(
            s.velocity.length() < 21.5,
            "hit scrubs speed: {}",
            s.velocity.length()
        );
        let h0 = heading(&s);
        for _ in 0..30 {
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
        }
        assert!((heading(&s) - h0).abs() > 1.0, "the kart spins");
        assert!(s.spin > 0.0);
        for _ in 0..40 {
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
        }
        assert_eq!(s.spin, 0.0, "control returns");
    }

    #[test]
    fn karts_collide_by_mass() {
        let mut a = VehicleState::spawn(Vec3::new(0.0, 0.5, 0.0), 0.0);
        let mut b = VehicleState::spawn(Vec3::new(1.2, 0.5, 0.0), 0.0);
        a.velocity = Vec3::new(10.0, 0.0, 0.0);
        let momentum = a.velocity * 400.0 + b.velocity * 250.0;
        collide_karts(&mut a, 400.0, &mut b, 250.0, 1.0);
        assert!(a.position.distance(b.position) >= 2.0 - 1e-4, "separated");
        let after = a.velocity * 400.0 + b.velocity * 250.0;
        assert!(
            (after - momentum).length() < 1e-2,
            "momentum kept: {momentum} vs {after}"
        );
        assert!(
            b.velocity.x > a.velocity.x * 0.9,
            "heavy kart shoves the light one forward"
        );
        assert!(
            b.position.x - 1.2 > 0.0 && a.position.x < 0.0 + 1e-6,
            "light kart moved more"
        );
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
    fn drift_full_charge_gives_top_tier() {
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
        assert_eq!(s.boost_speed, 9.0, "a long tight drift earns the top tier");
        assert!(
            (s.boost_timer - 1.7).abs() < 0.02,
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
        for _ in 0..15 {
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
