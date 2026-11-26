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
    /// Continuous engine for kart `id`: `rpm` 0..1, `load` (throttle) 0..1, `gain` 0..1,
    /// `pan` -1 (left)..1 (right).
    Engine {
        id: u8,
        rpm: f32,
        load: f32,
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
    load: f32,
    target_load: f32,
    /// Which cylinder fires next (uneven firing gives the burble).
    cyl: usize,
    /// DC-blocking filter state.
    hp: f32,
    /// Seconds of exhaust crackle left (after lifting off at high revs).
    crackle: f32,
    /// Envelope of the current backfire pop.
    pop: f32,
}

/// Relative strength of each firing in a cross-plane V8's cycle.
const FIRING: [f32; 8] = [1.0, 0.72, 0.9, 0.66, 0.95, 0.7, 0.85, 0.62];

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
            Command::Engine {
                rpm,
                load,
                gain,
                pan,
                ..
            } => rpm.is_finite() && load.is_finite() && gain.is_finite() && pan.is_finite(),
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
            Command::Engine {
                id,
                rpm,
                load,
                gain,
                pan,
            } => {
                let e = &mut self.engines[id as usize % VOICES];
                if e.target_gain == 0.0 && e.gain < 1e-4 {
                    e.rpm = rpm; // start at pitch instead of sliding up from zero
                    e.pan = pan;
                }
                let load = load.clamp(0.0, 1.0);
                // Lifting off hard at high revs: the exhaust crackles and pops.
                if e.target_load > 0.6 && load < 0.2 && e.rpm > 0.55 {
                    e.crackle = 0.7;
                }
                (e.target_rpm, e.target_gain, e.target_pan, e.target_load) =
                    (rpm.clamp(0.0, 1.2), gain.clamp(0.0, 1.0), pan, load);
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

            // Engines: a V8-style train of combustion pulses (uneven firing for the burble),
            // combustion noise riding on each pulse, a load-dependent tone filter, DC blocking and
            // soft saturation; plus exhaust crackle when lifting off at high revs.
            for i in 0..VOICES {
                if self.engines[i].gain < 1e-4 && self.engines[i].target_gain == 0.0 {
                    continue;
                }
                let (n1, n2) = (self.white(), self.white());
                let e = &mut self.engines[i];
                e.rpm += (e.target_rpm - e.rpm) * smooth * 0.3;
                e.gain += (e.target_gain - e.gain) * smooth;
                e.pan += (e.target_pan - e.pan) * smooth;
                e.load += (e.target_load - e.load) * smooth * 0.5;
                let f = 30.0 + e.rpm * 150.0;
                e.phase += f * dt;
                if e.phase >= 1.0 {
                    e.phase -= 1.0;
                    e.cyl = (e.cyl + 1) % FIRING.len();
                }
                e.sub_phase = (e.sub_phase + f * 0.5 * dt).fract();
                let pulse = (-e.phase * 7.0).exp() * FIRING[e.cyl];
                let body = (e.sub_phase * TAU).sin() * 0.22;
                let raw = pulse + n1 * pulse * 0.3 + body;
                let cutoff = 0.04 + 0.22 * e.rpm.min(1.0) + 0.22 * e.load;
                e.lp += (raw - e.lp) * cutoff;
                e.hp += (e.lp - e.hp) * 0.004;
                let x = e.lp - e.hp;
                let drive = 1.6 + 2.4 * e.load;
                let mut y = (x * drive).tanh();
                if e.crackle > 0.0 {
                    e.crackle -= dt;
                    if n2 > 0.9997 {
                        e.pop = 1.0;
                    }
                }
                if e.pop > 1e-3 {
                    y += n1 * e.pop * 2.4;
                    e.pop *= 0.993;
                }
                let s = y * e.gain * (0.18 + 0.1 * e.load);
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

    /// Fundamental frequency (Hz) of the left channel by autocorrelation: the shortest lag
    /// whose correlation is close to the strongest (sub-harmonics would otherwise win).
    fn pitch(buf: &[f32]) -> f32 {
        let x: Vec<f32> = buf.chunks(2).map(|f| f[0]).collect();
        let (lo, hi) = ((RATE / 400.0) as usize, (RATE / 20.0) as usize);
        let corr: Vec<f32> = (lo..hi)
            .map(|lag| x.iter().zip(&x[lag..]).map(|(a, b)| a * b).sum::<f32>())
            .collect();
        let best = corr.iter().cloned().fold(f32::MIN, f32::max);
        let lag = corr.iter().position(|c| *c >= best * 0.8).unwrap_or(0) + lo;
        RATE / lag as f32
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
            load: 0.5,
            gain: 0.8,
            pan: 0.0,
        });
        render(&mut slow, 0.5);
        let low = pitch(&render(&mut slow, 0.5));
        let mut fast = Mixer::new(RATE);
        fast.apply(Command::Engine {
            id: 0,
            rpm: 0.95,
            load: 0.5,
            gain: 0.8,
            pan: 0.0,
        });
        render(&mut fast, 0.5);
        let high = pitch(&render(&mut fast, 0.5));
        assert!(high > low * 2.0, "low {low} Hz, high {high} Hz");
    }

    fn engine(m: &mut Mixer, rpm: f32, load: f32) {
        m.apply(Command::Engine {
            id: 0,
            rpm,
            load,
            gain: 0.8,
            pan: 0.0,
        });
    }

    fn rms(buf: &[f32]) -> f32 {
        (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt()
    }

    #[test]
    fn the_engine_is_louder_under_load() {
        let mut on = Mixer::new(RATE);
        engine(&mut on, 0.6, 1.0);
        render(&mut on, 0.5);
        let mut off = Mixer::new(RATE);
        engine(&mut off, 0.6, 0.0);
        render(&mut off, 0.5);
        let (a, b) = (rms(&render(&mut on, 0.5)), rms(&render(&mut off, 0.5)));
        assert!(a > b * 1.3, "on throttle {a}, coasting {b}");
    }

    #[test]
    fn lifting_off_at_high_revs_crackles() {
        let peaks = |lift: bool| {
            let mut m = Mixer::new(RATE);
            engine(&mut m, 0.9, if lift { 1.0 } else { 0.0 });
            render(&mut m, 0.5);
            engine(&mut m, 0.9, 0.0);
            render(&mut m, 0.05);
            let buf = render(&mut m, 0.6);
            let base = rms(&buf);
            buf.iter().filter(|s| s.abs() > base * 4.0).count()
        };
        assert!(
            peaks(true) > peaks(false) + 20,
            "lift-off {} vs steady {}",
            peaks(true),
            peaks(false)
        );
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
                load: 1.0,
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
            load: 0.5,
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
            load: 0.5,
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
            load: 0.5,
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
