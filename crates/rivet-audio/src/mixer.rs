//! Procedural sound: engines, skids, one-shot effects. Pure, sample-accurate, testable.

use std::f32::consts::TAU;

/// One-shot effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sfx {
    Pickup,
    NailShot,
    Harpoon,
    Hit,
    Zap,
    Explosion,
    Whoosh,
    Beep,
    BeepGo,
}

impl Sfx {
    fn duration(self) -> f32 {
        match self {
            Sfx::Pickup => 0.3,
            Sfx::NailShot => 0.09,
            Sfx::Harpoon => 0.5,
            Sfx::Hit => 0.35,
            Sfx::Zap => 0.35,
            Sfx::Explosion => 1.8,
            Sfx::Whoosh => 0.7,
            Sfx::Beep => 0.2,
            Sfx::BeepGo => 0.6,
        }
    }
}

/// Messages from the game to the mixer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    /// Continuous engine for kart `id`: `rpm` 0..1, `gain` 0..1, `pan` -1 (left)..1 (right).
    Engine {
        id: u8,
        rpm: f32,
        gain: f32,
        pan: f32,
    },
    /// Tire squeal for kart `id` while drifting (gain 0 stops it).
    Skid {
        id: u8,
        gain: f32,
        pan: f32,
    },
    Play {
        sfx: Sfx,
        gain: f32,
        pan: f32,
    },
    /// The Plasma Orb's hum (gain 0 when none is flying).
    OrbHum {
        gain: f32,
    },
    /// Silences every continuous voice (menus, pause).
    StopLoops,
}

const VOICES: usize = 16;
const MAX_ONE_SHOTS: usize = 48;

#[derive(Debug, Clone, Copy, Default)]
struct Loop {
    rpm: f32,
    gain: f32,
    pan: f32,
    target_rpm: f32,
    target_gain: f32,
    target_pan: f32,
    phase: f32,
    sub_phase: f32,
    lp: f32,
}

#[derive(Debug, Clone, Copy)]
struct OneShot {
    sfx: Sfx,
    t: f32,
    gain: f32,
    pan: f32,
    phase: f32,
    lp: f32,
}

pub struct Mixer {
    rate: f32,
    engines: [Loop; VOICES],
    skids: [Loop; VOICES],
    orb: Loop,
    shots: Vec<OneShot>,
    noise: u32,
}

/// Equal-power pan gains (left, right).
fn pan_gains(pan: f32) -> (f32, f32) {
    let a = (pan.clamp(-1.0, 1.0) + 1.0) * 0.25 * std::f32::consts::PI;
    (a.cos(), a.sin())
}

impl Mixer {
    pub fn new(rate: f32) -> Self {
        Self {
            rate,
            engines: [Loop::default(); VOICES],
            skids: [Loop::default(); VOICES],
            orb: Loop::default(),
            shots: Vec::new(),
            noise: 0x1234_5678,
        }
    }

    pub fn active_one_shots(&self) -> usize {
        self.shots.len()
    }

    pub fn apply(&mut self, cmd: Command) {
        // A NaN would stick in the filters and silence the mix for good: drop such commands.
        let finite = match cmd {
            Command::Engine { rpm, gain, pan, .. } => {
                rpm.is_finite() && gain.is_finite() && pan.is_finite()
            }
            Command::Skid { gain, pan, .. } | Command::Play { gain, pan, .. } => {
                gain.is_finite() && pan.is_finite()
            }
            Command::OrbHum { gain } => gain.is_finite(),
            Command::StopLoops => true,
        };
        if !finite {
            return;
        }
        match cmd {
            Command::Engine { id, rpm, gain, pan } => {
                let e = &mut self.engines[id as usize % VOICES];
                if e.target_gain == 0.0 && e.gain < 1e-4 {
                    e.rpm = rpm; // start at pitch instead of sliding up from zero
                    e.pan = pan;
                }
                (e.target_rpm, e.target_gain, e.target_pan) =
                    (rpm.clamp(0.0, 1.2), gain.clamp(0.0, 1.0), pan);
            }
            Command::Skid { id, gain, pan } => {
                let k = &mut self.skids[id as usize % VOICES];
                (k.target_gain, k.target_pan) = (gain.clamp(0.0, 1.0), pan);
            }
            Command::Play { sfx, gain, pan } => {
                if self.shots.len() >= MAX_ONE_SHOTS {
                    self.shots.remove(0);
                }
                self.shots.push(OneShot {
                    sfx,
                    t: 0.0,
                    gain: gain.clamp(0.0, 1.0),
                    pan,
                    phase: 0.0,
                    lp: 0.0,
                });
            }
            Command::OrbHum { gain } => self.orb.target_gain = gain.clamp(0.0, 1.0),
            Command::StopLoops => {
                for v in self.engines.iter_mut().chain(self.skids.iter_mut()) {
                    v.target_gain = 0.0;
                }
                self.orb.target_gain = 0.0;
            }
        }
    }

    fn white(&mut self) -> f32 {
        self.noise ^= self.noise << 13;
        self.noise ^= self.noise >> 17;
        self.noise ^= self.noise << 5;
        (self.noise as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// Fills interleaved `out` (`channels` per frame); stereo panning on the first two.
    pub fn render(&mut self, out: &mut [f32], channels: usize) {
        let dt = 1.0 / self.rate;
        let smooth = 1.0 - (-dt * 20.0).exp();
        for frame in out.chunks_mut(channels.max(1)) {
            let (mut l, mut r) = (0.0f32, 0.0f32);

            // Engines: sawtooth plus a sub-octave pulse and a little grit, pitched by rpm.
            for e in self.engines.iter_mut() {
                e.rpm += (e.target_rpm - e.rpm) * smooth * 0.3;
                e.gain += (e.target_gain - e.gain) * smooth;
                e.pan += (e.target_pan - e.pan) * smooth;
                if e.gain < 1e-4 && e.target_gain == 0.0 {
                    continue;
                }
                let f = 42.0 + e.rpm * 170.0;
                e.phase = (e.phase + f * dt).fract();
                e.sub_phase = (e.sub_phase + f * 0.5 * dt).fract();
                let saw = e.phase * 2.0 - 1.0;
                let sub = if e.sub_phase < 0.5 { 1.0 } else { -1.0 };
                let raw = saw * 0.55 + sub * 0.3;
                e.lp += (raw - e.lp) * (0.08 + e.rpm * 0.25);
                let s = e.lp * e.gain * 0.22;
                let (gl, gr) = pan_gains(e.pan);
                l += s * gl;
                r += s * gr;
            }

            // Skids: band-limited noise.
            for i in 0..VOICES {
                let n = self.white();
                let k = &mut self.skids[i];
                k.gain += (k.target_gain - k.gain) * smooth;
                k.pan += (k.target_pan - k.pan) * smooth;
                if k.gain < 1e-4 && k.target_gain == 0.0 {
                    continue;
                }
                k.lp += (n - k.lp) * 0.35;
                let s = (n - k.lp) * k.gain * 0.12;
                let (gl, gr) = pan_gains(k.pan);
                l += s * gl;
                r += s * gr;
            }

            // Plasma Orb hum: a low throb.
            {
                let o = &mut self.orb;
                o.gain += (o.target_gain - o.gain) * smooth;
                if o.gain > 1e-4 || o.target_gain > 0.0 {
                    o.phase = (o.phase + 55.0 * dt).fract();
                    o.sub_phase = (o.sub_phase + 3.0 * dt).fract();
                    let s = (o.phase * TAU).sin()
                        * (0.6 + 0.4 * (o.sub_phase * TAU).sin())
                        * o.gain
                        * 0.25;
                    l += s;
                    r += s;
                }
            }

            // One-shots.
            let mut k = 0;
            while k < self.shots.len() {
                let n = self.white();
                let sh = &mut self.shots[k];
                let t = sh.t;
                let d = sh.sfx.duration();
                let env = (1.0 - t / d).clamp(0.0, 1.0);
                let s = match sh.sfx {
                    Sfx::Pickup => {
                        let f = if t < 0.12 { 880.0 } else { 1320.0 };
                        sh.phase = (sh.phase + f * dt).fract();
                        (sh.phase * TAU).sin() * env * 0.35
                    }
                    Sfx::NailShot => {
                        sh.phase = (sh.phase + 1900.0 * dt).fract();
                        ((sh.phase * TAU).sin() * 0.5 + n * 0.6) * env * env * 0.4
                    }
                    Sfx::Harpoon => {
                        sh.phase += dt;
                        let ring = (sh.phase * 310.0 * TAU).sin()
                            + (sh.phase * 745.0 * TAU).sin() * 0.6
                            + (sh.phase * 1180.0 * TAU).sin() * 0.4;
                        (ring * 0.25 + n * 0.25 * env) * env * 0.35
                    }
                    Sfx::Hit => {
                        sh.phase = (sh.phase + (120.0 - 60.0 * t / d) * dt).fract();
                        sh.lp += (n - sh.lp) * 0.2;
                        ((sh.phase * TAU).sin() * 0.7 + sh.lp * 0.8) * env * env * 0.5
                    }
                    Sfx::Zap => {
                        sh.phase = (sh.phase + 95.0 * dt).fract();
                        let buzz = if sh.phase < 0.5 { 1.0 } else { -1.0 };
                        (buzz * 0.4 + n * 0.5) * env * 0.3
                    }
                    Sfx::Explosion => {
                        sh.phase = (sh.phase + (60.0 - 25.0 * t / d) * dt).fract();
                        sh.lp += (n - sh.lp) * (0.05 + 0.3 * env);
                        ((sh.phase * TAU).sin() * env * 0.9 + sh.lp * 1.4) * env * env * 0.6
                    }
                    Sfx::Whoosh => {
                        let cut = 0.02 + 0.3 * (t / d);
                        sh.lp += (n - sh.lp) * cut;
                        sh.lp * (env * (1.0 - env) * 4.0) * 0.5
                    }
                    Sfx::Beep | Sfx::BeepGo => {
                        let f = if sh.sfx == Sfx::Beep { 520.0 } else { 1040.0 };
                        sh.phase = (sh.phase + f * dt).fract();
                        (sh.phase * TAU).sin() * env.sqrt() * 0.3
                    }
                };
                let s = s * sh.gain;
                let (gl, gr) = pan_gains(sh.pan);
                l += s * gl;
                r += s * gr;
                sh.t += dt;
                if sh.t >= d {
                    self.shots.swap_remove(k);
                } else {
                    k += 1;
                }
            }

            // Soft clip so stacked effects never exceed full scale.
            let (l, r) = (l.tanh(), r.tanh());
            for (c, sample) in frame.iter_mut().enumerate() {
                *sample = match c {
                    0 => l,
                    1 => r,
                    _ => 0.5 * (l + r),
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn render(m: &mut Mixer, secs: f32) -> Vec<f32> {
        let mut buf = vec![0.0; (RATE * secs) as usize * 2];
        m.render(&mut buf, 2);
        buf
    }

    fn zero_crossings(buf: &[f32]) -> usize {
        buf.chunks(2)
            .map(|f| f[0])
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
            .count()
    }

    #[test]
    fn silent_by_default() {
        let mut m = Mixer::new(RATE);
        assert!(render(&mut m, 0.1).iter().all(|s| *s == 0.0));
    }

    #[test]
    fn engine_pitch_rises_with_rpm() {
        let mut slow = Mixer::new(RATE);
        slow.apply(Command::Engine {
            id: 0,
            rpm: 0.1,
            gain: 0.8,
            pan: 0.0,
        });
        render(&mut slow, 0.5);
        let low = zero_crossings(&render(&mut slow, 0.5));
        let mut fast = Mixer::new(RATE);
        fast.apply(Command::Engine {
            id: 0,
            rpm: 0.95,
            gain: 0.8,
            pan: 0.0,
        });
        render(&mut fast, 0.5);
        let high = zero_crossings(&render(&mut fast, 0.5));
        assert!(high > low * 2, "low {low}, high {high}");
    }

    #[test]
    fn one_shots_finish() {
        let mut m = Mixer::new(RATE);
        m.apply(Command::Play {
            sfx: Sfx::Pickup,
            gain: 1.0,
            pan: 0.0,
        });
        let first = render(&mut m, 0.1);
        assert!(first.iter().any(|s| s.abs() > 0.01), "audible");
        render(&mut m, 3.0);
        assert!(
            render(&mut m, 0.1).iter().all(|s| s.abs() < 1e-4),
            "and then silent"
        );
        assert_eq!(m.active_one_shots(), 0);
    }

    #[test]
    fn output_stays_bounded() {
        let mut m = Mixer::new(RATE);
        for _ in 0..40 {
            m.apply(Command::Play {
                sfx: Sfx::Explosion,
                gain: 1.0,
                pan: 0.0,
            });
        }
        for id in 0..8 {
            m.apply(Command::Engine {
                id,
                rpm: 1.0,
                gain: 1.0,
                pan: 0.0,
            });
        }
        assert!(render(&mut m, 0.5).iter().all(|s| s.abs() <= 1.0));
    }

    #[test]
    fn non_finite_commands_cannot_poison_the_mix() {
        let mut m = Mixer::new(RATE);
        m.apply(Command::Engine {
            id: 0,
            rpm: f32::NAN,
            gain: 0.8,
            pan: f32::INFINITY,
        });
        m.apply(Command::Play {
            sfx: Sfx::Hit,
            gain: f32::NAN,
            pan: 0.0,
        });
        m.apply(Command::Engine {
            id: 1,
            rpm: 0.5,
            gain: 0.5,
            pan: 0.0,
        });
        assert!(render(&mut m, 0.2).iter().all(|s| s.is_finite()));
    }

    #[test]
    fn panning_moves_sound() {
        let mut m = Mixer::new(RATE);
        m.apply(Command::Engine {
            id: 0,
            rpm: 0.5,
            gain: 1.0,
            pan: -1.0,
        });
        render(&mut m, 0.3);
        let buf = render(&mut m, 0.2);
        let (l, r) = buf.chunks(2).fold((0.0f32, 0.0f32), |(l, r), f| {
            (l + f[0].abs(), r + f[1].abs())
        });
        assert!(l > r * 4.0, "left {l} right {r}");
    }
}
