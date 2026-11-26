//! Turns race state and events into mixer commands.

use glam::Vec3;
use rivet_audio::{Audio, Command, Sfx};

use crate::combat::{CombatEvent, ItemKind};
use crate::race::{Phase, Race, RaceEvent};

/// Engines fade out beyond this distance (m).
const HEARING: f32 = 140.0;

/// Gain and stereo pan of a sound at `pos` for a listener at `ear` facing with `right`.
pub fn spatial(ear: Vec3, right: Vec3, pos: Vec3, range: f32) -> (f32, f32) {
    let rel = pos - ear;
    let d = rel.length();
    let gain = (1.0 - d / range).clamp(0.0, 1.0).powi(2);
    let pan = if d > 1e-3 {
        (rel / d).dot(right).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    (gain, pan * 0.85)
}

/// Where the player hears from: the camera position and its right vector.
#[derive(Debug, Clone, Copy)]
pub struct Listener {
    pub ear: Vec3,
    pub right: Vec3,
}

/// Remembers what was playing to detect starts (boosts, countdown steps, new nails).
#[derive(Default)]
pub struct SoundDirector {
    boosting: Vec<bool>,
    countdown_step: i32,
    nails: usize,
    silent: bool,
}

impl SoundDirector {
    /// Call once per rendered frame. `active` is false in menus and pause (loops stop).
    pub fn update(
        &mut self,
        audio: &Audio,
        race: &Race,
        events: &[RaceEvent],
        listener: Listener,
        focus: usize,
        active: bool,
    ) {
        let Listener { ear, right } = listener;
        if !active {
            if !self.silent {
                audio.send(Command::StopLoops);
                self.silent = true;
            }
            return;
        }
        self.silent = false;
        self.boosting.resize(race.racers.len(), false);

        for (i, r) in race.racers.iter().enumerate() {
            let (gain, pan) = spatial(ear, right, r.state.position, HEARING);
            let max = r.params.max_speed + r.params.boost_tiers[2].speed;
            let rpm = (r.state.speed().abs() / max).clamp(0.0, 1.2)
                + if r.state.boost_timer > 0.0 { 0.12 } else { 0.0 };
            let me = if i == focus { 1.0 } else { 0.7 };
            audio.send(Command::Engine {
                id: i as u8,
                rpm,
                gain: gain * me,
                pan,
            });
            let skid = if r.state.drift.is_some() && r.state.grounded {
                0.35 + 0.15 * r.state.drift.map_or(0.0, |d| d.charge.min(2.0))
            } else {
                0.0
            };
            audio.send(Command::Skid {
                id: i as u8,
                gain: skid * gain,
                pan,
            });
            let boosting = r.state.boost_timer > 0.0;
            if boosting && !self.boosting[i] && gain > 0.05 {
                audio.send(Command::Play {
                    sfx: Sfx::Whoosh,
                    gain: gain * 0.8,
                    pan,
                });
            }
            self.boosting[i] = boosting;
        }

        // Countdown: 3, 2, 1 beeps then GO.
        let step = match race.phase {
            Phase::Countdown(left) => left.ceil() as i32,
            Phase::Racing => 0,
        };
        if step != self.countdown_step {
            if step > 0 {
                audio.send(Command::Play {
                    sfx: Sfx::Beep,
                    gain: 0.9,
                    pan: 0.0,
                });
            } else if self.countdown_step > 0 {
                audio.send(Command::Play {
                    sfx: Sfx::BeepGo,
                    gain: 1.0,
                    pan: 0.0,
                });
            }
            self.countdown_step = step;
        }

        // One nail-shot per nail fired.
        let nails = race.combat.nails.len();
        if nails > self.nails {
            audio.send(Command::Play {
                sfx: Sfx::NailShot,
                gain: 0.5,
                pan: 0.0,
            });
        }
        self.nails = nails;

        let orb_gain = race
            .combat
            .orbs
            .iter()
            .map(|o| spatial(ear, right, o.pos, 120.0).0)
            .fold(0.0f32, f32::max);
        audio.send(Command::OrbHum { gain: orb_gain });

        for e in events {
            let RaceEvent::Combat(c) = e else { continue };
            let at = |i: usize| race.racers[i].state.position;
            let (sfx, pos, loud) = match *c {
                CombatEvent::Pickup { racer, .. } => (
                    Sfx::Pickup,
                    at(racer),
                    if racer == focus { 1.0 } else { 0.3 },
                ),
                CombatEvent::Fire {
                    racer,
                    item: ItemKind::Harpoon,
                } => (Sfx::Harpoon, at(racer), 1.0),
                CombatEvent::Fire { .. } => continue,
                CombatEvent::Hit { victim, .. } => (Sfx::Hit, at(victim), 1.0),
                CombatEvent::Zap { victim } => (Sfx::Zap, at(victim), 1.0),
                CombatEvent::Explode { pos, .. } => (Sfx::Explosion, pos, 1.0),
                CombatEvent::Slingshot { racer } => (Sfx::Whoosh, at(racer), 1.0),
            };
            let (gain, pan) = spatial(ear, right, pos, 220.0);
            if gain * loud > 0.01 {
                audio.send(Command::Play {
                    sfx,
                    gain: (gain * loud).sqrt(),
                    pan,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spatial_falls_off_and_pans() {
        let (near, _) = spatial(Vec3::ZERO, Vec3::X, Vec3::new(0.0, 0.0, 5.0), 100.0);
        let (far, _) = spatial(Vec3::ZERO, Vec3::X, Vec3::new(0.0, 0.0, 90.0), 100.0);
        assert!(near > far * 10.0);
        assert_eq!(
            spatial(Vec3::ZERO, Vec3::X, Vec3::new(0.0, 0.0, 200.0), 100.0).0,
            0.0
        );
        let (_, right) = spatial(Vec3::ZERO, Vec3::X, Vec3::new(10.0, 0.0, 0.0), 100.0);
        let (_, left) = spatial(Vec3::ZERO, Vec3::X, Vec3::new(-10.0, 0.0, 0.0), 100.0);
        assert!(right > 0.5 && left < -0.5);
    }
}
