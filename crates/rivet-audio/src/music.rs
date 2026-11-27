//! The soundtrack: original, fully synthesized music, one piece for the menus and one per
//! circuit (the songs themselves are data in `songs.rs`).
//!
//! A 16th-note sequencer drives a small band: two double-tracked distorted rhythm guitars
//! (palm mutes, chugs and open power chords), a driving bass, a lead with glide and vibrato
//! through a ping-pong delay, a pulse arpeggio, and a drum kit (kick, snare, hats, toms,
//! crash, a noise riser) with a little room reverb. Each song sets the band up its own way
//! (tempo, guitar gain and tone, a guitar-like or synth lead). Like the rest of the mixer it
//! is pure and sample-accurate: no samples, no files, nothing to load.

use std::f32::consts::{PI, TAU};

use crate::songs::{BassPart, FILL, STEPS, Song, Theme, chord_letter};

/// The slowest tempo the delay line is sized for.
const MIN_BPM: f32 = 90.0;

/// What a rhythm-guitar step does.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Strike {
    /// A ringing power chord.
    Open,
    /// A half-muted chug: shorter, a little darker.
    Half,
    /// A tight palm mute.
    Mute,
}

impl Strike {
    /// Seconds for the string to decay by 1/e.
    fn decay(self) -> f32 {
        match self {
            Strike::Open => 1.8,
            Strike::Half => 0.16,
            Strike::Mute => 0.065,
        }
    }

    /// Pre-distortion tone (Hz): palm mutes are dark.
    fn tone(self) -> f32 {
        match self {
            Strike::Open => 5200.0,
            Strike::Half => 2600.0,
            Strike::Mute => 950.0,
        }
    }
}

fn midi_hz(n: f32) -> f32 {
    440.0 * ((n - 69.0) / 12.0).exp2()
}

/// Per-sample multiplier for an exponential decay with time constant `tau` seconds.
fn decay_coef(tau: f32, rate: f32) -> f32 {
    (-1.0 / (tau * rate)).exp()
}

/// Polynomial band-limited step correction (keeps the saws from aliasing).
fn blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let t = t / dt;
        t + t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Osc {
    phase: f32,
}

impl Osc {
    /// Band-limited sawtooth; `inc` is frequency / sample rate.
    fn saw(&mut self, inc: f32) -> f32 {
        let inc = inc.min(0.45);
        let t = self.phase;
        self.phase += inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        2.0 * t - 1.0 - blep(t, inc)
    }

    /// Band-limited pulse of duty `width`.
    fn pulse(&mut self, inc: f32, width: f32) -> f32 {
        let inc = inc.min(0.45);
        let t = self.phase;
        let t2 = (t + 1.0 - width).fract();
        let a = 2.0 * t - 1.0 - blep(t, inc);
        let b = 2.0 * t2 - 1.0 - blep(t2, inc);
        self.phase += inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        a - b
    }

    fn sine(&mut self, inc: f32) -> f32 {
        self.phase = (self.phase + inc).fract();
        (self.phase * TAU).sin()
    }
}

/// Topology-preserving state-variable filter (stable under fast modulation).
#[derive(Debug, Clone, Copy, Default)]
struct Svf {
    ic1: f32,
    ic2: f32,
}

/// Filter coefficient for cutoff `hz`.
fn svf_g(hz: f32, rate: f32) -> f32 {
    (PI * hz.clamp(10.0, rate * 0.45) / rate).tan()
}

impl Svf {
    /// (low-pass, band-pass, high-pass) for input `x`; `k` = 1/Q.
    fn run(&mut self, x: f32, g: f32, k: f32) -> (f32, f32, f32) {
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, v1, x - k * v1 - v2)
    }

    fn lp(&mut self, x: f32, g: f32, k: f32) -> f32 {
        self.run(x, g, k).0
    }

    fn hp(&mut self, x: f32, g: f32, k: f32) -> f32 {
        self.run(x, g, k).2
    }
}

/// One rhythm-guitar track: a power chord (root, fifth, octave), each string two
/// detuned saws, into a high-gain stage and a speaker-cabinet filter.
#[derive(Debug, Clone)]
struct Guitar {
    osc: [Osc; 6],
    /// Detune between each string's two saws, in semitones.
    spread: f32,
    root: f32,
    /// String energy, set on a strike and decaying.
    energy: f32,
    decay: f32,
    /// Smoothed amplitude (no clicks on re-strikes).
    amp: f32,
    tone_g: f32,
    tight: Svf,
    tone: Svf,
    cab: Svf,
    cab2: Svf,
    scoop: Svf,
    sweep: Svf,
    /// A strike waiting `n` samples (the second track plays a hair late, for width).
    pending: Option<(u32, f32, Strike, f32)>,
    lag: u32,
}

impl Guitar {
    fn new(spread: f32, lag: u32, rate: f32) -> Self {
        Self {
            osc: [Osc::default(); 6],
            spread,
            root: 40.0,
            energy: 0.0,
            decay: decay_coef(Strike::Open.decay(), rate),
            amp: 0.0,
            tone_g: svf_g(Strike::Open.tone(), rate),
            tight: Svf::default(),
            tone: Svf::default(),
            cab: Svf::default(),
            cab2: Svf::default(),
            scoop: Svf::default(),
            sweep: Svf::default(),
            pending: None,
            lag,
        }
    }

    fn strike(&mut self, root: u8, kind: Strike, accent: f32) {
        self.pending = Some((self.lag, root as f32, kind, accent));
    }

    fn choke(&mut self, rate: f32) {
        self.pending = None;
        self.decay = decay_coef(0.02, rate);
    }

    fn sample(&mut self, rate: f32, filter: &Filters, sweep_g: Option<f32>) -> f32 {
        if let Some((n, root, kind, accent)) = self.pending {
            if n == 0 {
                self.pending = None;
                // Phases keep running so a re-strike never clicks; seeding them apart on a
                // chord change keeps the beating between saws from lining up identically.
                if root != self.root {
                    for (i, o) in self.osc.iter_mut().enumerate() {
                        o.phase = (o.phase + 0.37 * i as f32).fract();
                    }
                }
                self.root = root;
                self.energy = accent;
                self.decay = decay_coef(kind.decay(), rate);
                self.tone_g = svf_g(kind.tone(), rate);
            } else {
                self.pending = Some((n - 1, root, kind, accent));
            }
        }
        if self.energy < 1e-4 && self.amp < 1e-4 {
            return 0.0;
        }
        self.energy *= self.decay;
        self.amp += (self.energy - self.amp) * filter.attack;
        let inv = 1.0 / rate;
        let mut x = 0.0;
        for (s, interval) in [0.0f32, 7.0, 12.0].iter().enumerate() {
            let f = midi_hz(self.root + interval);
            let d = self.spread * 0.5;
            x += self.osc[2 * s].saw(f * (1.0 + d * 0.0578) * inv);
            x += self.osc[2 * s + 1].saw(f * (1.0 - d * 0.0578) * inv);
        }
        let x = x * 0.17 * self.amp;
        // Tight low end into the amp, then the pick tone.
        let x = self.tight.hp(x, filter.g_tight, 1.0);
        let x = self.tone.lp(x, self.tone_g, 1.2);
        // High gain with a touch of asymmetry for even harmonics.
        let y = (x * filter.drive + 0.15).tanh() - 0.15f32.tanh();
        // Gate the hiss of the gain stage as the note dies.
        let y = y * (self.amp * 10.0).min(1.0);
        // Cabinet: two low-passes and a mid scoop.
        let y = self.cab.lp(y, filter.g_cab, 0.9);
        let y = self.cab2.lp(y, filter.g_cab2, 1.4);
        let (_, mid, _) = self.scoop.run(y, filter.g_scoop, 1.2);
        let y = y - mid * 0.45;
        match sweep_g {
            Some(g) => self.sweep.lp(y, g, 0.6),
            None => y,
        }
    }
}

/// Fixed filter coefficients shared by the voices.
#[derive(Debug, Clone, Copy)]
struct Filters {
    attack: f32,
    /// Guitar gain (the song's).
    drive: f32,
    g_tight: f32,
    g_cab: f32,
    g_cab2: f32,
    g_scoop: f32,
    g_hat: f32,
    g_crash: f32,
    g_snare: f32,
    g_lead: f32,
}

impl Filters {
    fn new(rate: f32, song: &Song) -> Self {
        Self {
            attack: 1.0 - decay_coef(0.0015, rate),
            drive: song.tone.drive,
            g_tight: svf_g(110.0, rate),
            g_cab: svf_g(song.tone.cab, rate),
            g_cab2: svf_g(6800.0, rate),
            g_scoop: svf_g(520.0, rate),
            g_hat: svf_g(7800.0, rate),
            g_crash: svf_g(4500.0, rate),
            g_snare: svf_g(1700.0, rate),
            g_lead: svf_g(if song.tone.synth { 5200.0 } else { 3800.0 }, rate),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Bass {
    saw: Osc,
    sub: Osc,
    note: f32,
    /// Filter envelope (the pluck).
    pluck: f32,
    energy: f32,
    amp: f32,
    lp: Svf,
}

impl Bass {
    /// Picks `root` an octave down (`octave` up from there).
    fn pick(&mut self, root: u8, octave: bool, accent: f32) {
        self.note = root as f32 - if octave { 0.0 } else { 12.0 };
        self.pluck = 1.0;
        self.energy = accent;
    }

    fn mute(&mut self) {
        self.energy = 0.0;
    }

    fn sample(&mut self, rate: f32, attack: f32, k: &Coefs) -> f32 {
        if self.energy < 1e-4 && self.amp < 1e-4 {
            return 0.0;
        }
        self.energy *= k.bass_decay;
        self.pluck *= k.pluck_decay;
        self.amp += (self.energy - self.amp) * attack;
        let f = midi_hz(self.note) / rate;
        let x = self.saw.saw(f) * 0.7 + self.sub.sine(f) * 0.4;
        let cutoff = 140.0 + 2200.0 * self.pluck * self.pluck;
        let y = self.lp.lp(x, svf_g(cutoff, rate), 0.8);
        (y * 2.6).tanh() * self.amp
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Lead {
    osc: [Osc; 3],
    sub: Osc,
    /// Current pitch (MIDI, gliding) and target.
    pitch: f32,
    target: f32,
    on: bool,
    amp: f32,
    /// Pick brightness, decaying after each note.
    pick: f32,
    /// Seconds since the note began (vibrato fades in).
    age: f32,
    vib: f32,
    lp: Svf,
}

impl Lead {
    fn note_on(&mut self, note: u8, shift: f32) {
        let n = note as f32 + shift;
        if !self.on && self.amp < 1e-3 {
            self.pitch = n;
        }
        self.target = n;
        self.on = true;
        self.pick = 1.0;
        self.age = 0.0;
    }

    fn sample(&mut self, rate: f32, k: &Coefs, filter: &Filters, double: f32, synth: bool) -> f32 {
        let want = if self.on { 1.0 } else { 0.0 };
        let rate_amp = if self.on {
            k.lead_attack
        } else {
            k.lead_release
        };
        self.amp += (want - self.amp) * rate_amp;
        if self.amp < 1e-4 && !self.on {
            return 0.0;
        }
        let dt = 1.0 / rate;
        self.age += dt;
        self.pitch += (self.target - self.pitch) * k.glide;
        self.pick *= k.pick_decay;
        self.vib = (self.vib + 5.7 * dt).fract();
        let depth = ((self.age - 0.16) * 4.0).clamp(0.0, 1.0) * 0.28;
        let p = self.pitch + (self.vib * TAU).sin() * depth;
        let f = midi_hz(p) * dt;
        let y = if synth {
            // Clean: two detuned saws and a narrow pulse an octave down, softly rounded.
            let mut x = (self.osc[0].saw(f) + self.osc[1].saw(f * 1.006)) * 0.5;
            x += self.osc[2].pulse(f * 0.5, 0.3) * 0.3;
            if double > 0.0 {
                x += self.sub.saw(f * 0.5) * double * 0.6;
            }
            (x * (1.0 + 0.8 * self.pick)).tanh() * 0.9
        } else {
            // Overdriven, brighter on each pick: a lead guitar.
            let mut x = self.osc[0].saw(f) + self.osc[1].saw(f * 1.0047) * 0.8;
            x += self.osc[2].pulse(f * 0.5, 0.5) * 0.35;
            if double > 0.0 {
                x += self.sub.saw(f * 0.5) * double;
            }
            let drive = 3.0 + 5.0 * self.pick;
            (x * 0.4 * drive).tanh()
        };
        self.lp.lp(y, filter.g_lead, 1.1) * self.amp
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Arp {
    osc: Osc,
    note: f32,
    energy: f32,
    lp: Svf,
    lfo: f32,
}

/// Coefficients that depend only on the sample rate.
#[derive(Debug, Clone, Copy)]
struct Coefs {
    bass_decay: f32,
    pluck_decay: f32,
    lead_attack: f32,
    lead_release: f32,
    glide: f32,
    pick_decay: f32,
    arp_decay: f32,
    duck_decay: f32,
    smooth: f32,
}

impl Coefs {
    fn new(rate: f32) -> Self {
        Self {
            bass_decay: decay_coef(0.3, rate),
            pluck_decay: decay_coef(0.07, rate),
            lead_attack: 1.0 - decay_coef(0.004, rate),
            lead_release: 1.0 - decay_coef(0.05, rate),
            glide: 1.0 - decay_coef(0.025, rate),
            pick_decay: decay_coef(0.05, rate),
            arp_decay: decay_coef(0.085, rate),
            duck_decay: decay_coef(0.11, rate),
            smooth: 1.0 - decay_coef(0.25, rate),
        }
    }
}

/// A drum hit: time since it fired and how hard.
#[derive(Debug, Clone, Copy, Default)]
struct Hit {
    t: f32,
    vel: f32,
    phase: f32,
    phase2: f32,
    /// Tom pitch, or 1 for the open hat.
    tune: f32,
    on: bool,
    f1: Svf,
    f2: Svf,
}

impl Hit {
    fn fire(&mut self, vel: f32, tune: f32) {
        self.t = 0.0;
        self.vel = vel;
        self.tune = tune;
        self.on = true;
    }
}

/// A feedback comb with damping (Freeverb's building block).
#[derive(Debug, Clone)]
struct Comb {
    buf: Vec<f32>,
    pos: usize,
    store: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            pos: 0,
            store: 0.0,
        }
    }

    fn clear(&mut self) {
        self.buf.fill(0.0);
        self.store = 0.0;
    }

    fn run(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let y = self.buf[self.pos];
        self.store = y * (1.0 - damp) + self.store * damp;
        self.buf[self.pos] = x + self.store * feedback;
        self.pos = (self.pos + 1) % self.buf.len();
        y
    }
}

#[derive(Debug, Clone)]
struct Allpass {
    buf: Vec<f32>,
    pos: usize,
}

impl Allpass {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            pos: 0,
        }
    }

    fn clear(&mut self) {
        self.buf.fill(0.0);
    }

    fn run(&mut self, x: f32) -> f32 {
        let b = self.buf[self.pos];
        self.buf[self.pos] = x + b * 0.5;
        self.pos = (self.pos + 1) % self.buf.len();
        b - x
    }
}

/// A small, bright room.
#[derive(Debug, Clone)]
struct Reverb {
    combs: [Vec<Comb>; 2],
    allpass: [Vec<Allpass>; 2],
}

impl Reverb {
    fn new(rate: f32) -> Self {
        let s = rate / 44_100.0;
        let side = |spread: usize| {
            let combs = [1116, 1188, 1277, 1356]
                .iter()
                .map(|&n| Comb::new(((n + spread) as f32 * s * 0.8) as usize))
                .collect();
            let ap = [556, 441]
                .iter()
                .map(|&n| Allpass::new(((n + spread) as f32 * s) as usize))
                .collect();
            (combs, ap)
        };
        let (cl, al) = side(0);
        let (cr, ar) = side(23);
        Self {
            combs: [cl, cr],
            allpass: [al, ar],
        }
    }

    fn clear(&mut self) {
        for c in self.combs.iter_mut().flatten() {
            c.clear();
        }
        for a in self.allpass.iter_mut().flatten() {
            a.clear();
        }
    }

    fn run(&mut self, x: f32) -> (f32, f32) {
        let mut out = [0.0f32; 2];
        for (side, o) in out.iter_mut().enumerate() {
            let mut y = 0.0;
            for c in &mut self.combs[side] {
                y += c.run(x, 0.76, 0.35);
            }
            for a in &mut self.allpass[side] {
                y = a.run(y);
            }
            *o = y * 0.25;
        }
        (out[0], out[1])
    }
}

/// Ping-pong delay a dotted 8th long, darkening on each repeat.
#[derive(Debug, Clone)]
struct Delay {
    l: Vec<f32>,
    r: Vec<f32>,
    /// Samples in use (the buffers are sized for the slowest song).
    len: usize,
    pos: usize,
    tone_l: f32,
    tone_r: f32,
}

impl Delay {
    fn new(capacity: usize) -> Self {
        Self {
            l: vec![0.0; capacity.max(1)],
            r: vec![0.0; capacity.max(1)],
            len: capacity.max(1),
            pos: 0,
            tone_l: 0.0,
            tone_r: 0.0,
        }
    }

    /// Silences it and sets the length (clamped to the buffers).
    fn reset(&mut self, len: usize) {
        self.l.fill(0.0);
        self.r.fill(0.0);
        self.len = len.clamp(1, self.l.len());
        self.pos = 0;
        self.tone_l = 0.0;
        self.tone_r = 0.0;
    }

    fn run(&mut self, x: f32) -> (f32, f32) {
        let (ol, or) = (self.l[self.pos], self.r[self.pos]);
        self.tone_l += (or - self.tone_l) * 0.35;
        self.tone_r += (ol - self.tone_r) * 0.35;
        self.l[self.pos] = x + self.tone_l * 0.42;
        self.r[self.pos] = self.tone_r * 0.42;
        self.pos = (self.pos + 1) % self.len;
        (ol, or)
    }
}

/// The soundtrack player. The mixer owns one and adds it to the mix.
#[derive(Debug, Clone)]
pub struct Music {
    rate: f32,
    /// The piece playing, and the one asked for (a change fades out, then starts it).
    theme: Theme,
    want: Theme,
    song: &'static Song,
    filters: Filters,
    k: Coefs,
    step_len: f32,
    clock: f32,
    section: usize,
    bar: usize,
    step: usize,
    playing: bool,
    gain: f32,
    target_gain: f32,
    muffle: f32,
    target_muffle: f32,
    intensity: f32,
    target_intensity: f32,
    /// The rhythm guitars' current chord (the bass follows it).
    sounding: u8,
    /// Last bar ended with a push into this one's chord: don't strike it again.
    pushed: bool,
    guitars: [Guitar; 2],
    bass: Bass,
    lead: Lead,
    /// 16ths left in the lead's current note.
    lead_left: u8,
    arp: Arp,
    kick: Hit,
    snare: Hit,
    hat: Hit,
    tom: Hit,
    crash: Hit,
    /// Seconds into the breakdown's riser, and its length (0 when none).
    riser_t: f32,
    riser_len: f32,
    riser_f: Svf,
    /// Kick sidechain: the band ducks a little under each kick.
    duck: f32,
    delay: Delay,
    reverb: Reverb,
    muffle_f: [Svf; 2],
    noise: u32,
}

impl Music {
    pub fn new(rate: f32) -> Self {
        let song = Theme::default().song();
        let step_len = rate * 60.0 / song.bpm / 4.0;
        let lag = (rate * 0.009) as u32;
        Self {
            rate,
            theme: Theme::default(),
            want: Theme::default(),
            song,
            filters: Filters::new(rate, song),
            k: Coefs::new(rate),
            step_len,
            clock: 0.0,
            section: 0,
            bar: 0,
            step: 0,
            playing: false,
            gain: 0.0,
            target_gain: 0.0,
            muffle: 0.0,
            target_muffle: 0.0,
            intensity: 0.0,
            target_intensity: 0.0,
            sounding: 40,
            pushed: false,
            guitars: [Guitar::new(0.11, 0, rate), Guitar::new(0.08, lag, rate)],
            bass: Bass::default(),
            lead: Lead::default(),
            lead_left: 0,
            arp: Arp::default(),
            kick: Hit::default(),
            snare: Hit::default(),
            hat: Hit::default(),
            tom: Hit::default(),
            crash: Hit::default(),
            riser_t: 0.0,
            riser_len: 0.0,
            riser_f: Svf::default(),
            duck: 0.0,
            delay: Delay::new((rate * 60.0 / MIN_BPM / 4.0 * 3.0) as usize),
            reverb: Reverb::new(rate),
            muffle_f: [Svf::default(); 2],
            noise: 0x9E37_79B9,
        }
    }

    /// The piece (a different one fades the current one out first, then starts from the
    /// top), level 0..1 (0 fades out, and the next start is from the top), `muffle` 0..1
    /// (muffled, as heard from the pause menu) and `intensity` 0..1 (extra layers).
    pub fn set(&mut self, theme: Theme, gain: f32, muffle: f32, intensity: f32) {
        self.want = theme;
        self.target_gain = gain.clamp(0.0, 1.0);
        self.target_muffle = muffle.clamp(0.0, 1.0);
        self.target_intensity = intensity.clamp(0.0, 1.0);
        if self.target_gain > 0.0 && !self.playing {
            self.start();
        }
    }

    /// The piece playing (or last played).
    pub fn theme(&self) -> Theme {
        self.theme
    }

    /// Whether the song is running (it stops once faded out).
    pub fn playing(&self) -> bool {
        self.playing
    }

    /// Current position: (section index, bar within it, 16th step).
    pub fn position(&self) -> (usize, usize, usize) {
        (self.section, self.bar, self.step)
    }

    /// Starts the requested piece from the top. Runs on the audio thread, so it resets in
    /// place rather than allocating.
    fn start(&mut self) {
        let rate = self.rate;
        self.theme = self.want;
        self.song = self.theme.song();
        self.filters = Filters::new(rate, self.song);
        self.step_len = rate * 60.0 / self.song.bpm / 4.0;
        self.clock = 0.0;
        (self.section, self.bar, self.step) = (0, 0, 0);
        self.sounding = self.song.sections[0].chords[0];
        self.pushed = false;
        let lag = (rate * 0.009) as u32;
        self.guitars = [Guitar::new(0.11, 0, rate), Guitar::new(0.08, lag, rate)];
        self.bass = Bass::default();
        self.lead = Lead::default();
        self.lead_left = 0;
        self.arp = Arp::default();
        for h in [
            &mut self.kick,
            &mut self.snare,
            &mut self.hat,
            &mut self.tom,
            &mut self.crash,
        ] {
            *h = Hit::default();
        }
        (self.riser_t, self.riser_len) = (0.0, 0.0);
        self.riser_f = Svf::default();
        self.duck = 0.0;
        self.delay.reset((self.step_len * 3.0) as usize);
        self.reverb.clear();
        self.muffle_f = [Svf::default(); 2];
        self.noise = 0x9E37_79B9;
        self.playing = true;
        // Hit the first beat at full level: the song starts on GO, not fading in.
        self.gain = self.target_gain;
        self.muffle = self.target_muffle;
        self.intensity = self.target_intensity;
        self.trigger();
    }

    fn white(&mut self) -> f32 {
        self.noise ^= self.noise << 13;
        self.noise ^= self.noise >> 17;
        self.noise ^= self.noise << 5;
        (self.noise as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    fn chord(&self, section: usize, bar: usize) -> u8 {
        let s = &self.song.sections[section];
        s.chords[bar % s.chords.len()]
    }

    /// The chord of the bar after this one (for pushes).
    fn next_chord(&self) -> u8 {
        let sections = self.song.sections;
        if self.bar + 1 < sections[self.section].bars {
            self.chord(self.section, self.bar + 1)
        } else {
            let next = if self.section + 1 < sections.len() {
                self.section + 1
            } else {
                self.song.loop_to
            };
            self.chord(next, 0)
        }
    }

    fn advance(&mut self) {
        self.step += 1;
        if self.step == STEPS {
            self.step = 0;
            self.bar += 1;
            let sections = self.song.sections;
            if self.bar == sections[self.section].bars {
                self.bar = 0;
                self.section = if self.section + 1 < sections.len() {
                    self.section + 1
                } else {
                    self.song.loop_to
                };
            }
        }
        self.trigger();
    }

    /// Fires everything that starts on the current 16th.
    fn trigger(&mut self) {
        let sec = self.song.sections[self.section];
        let (bar, step) = (self.bar, self.step);
        let last_bar = bar + 1 == sec.bars;
        let downbeat = step % 4 == 0;

        // Rhythm guitars, and the bass following them.
        let (pat, notes) = sec.guitar[bar % sec.guitar.len()];
        let bar_chord = self.chord(self.section, bar);
        let root = notes
            .as_bytes()
            .get(step)
            .and_then(|&c| chord_letter(c))
            .unwrap_or(bar_chord);
        let accent = if downbeat { 1.0 } else { 0.85 };
        let mut c = pat.as_bytes()[step];
        if step == 0 && self.pushed {
            c = b'-';
        }
        if step == 0 {
            self.pushed = false;
        }
        let struck = match c {
            b'O' => Some((root, Strike::Open)),
            b'H' => Some((root, Strike::Half)),
            b'M' => Some((root, Strike::Mute)),
            b'>' => {
                self.pushed = true;
                Some((self.next_chord(), Strike::Open))
            }
            b'.' => {
                for g in &mut self.guitars {
                    g.choke(self.rate);
                }
                None
            }
            _ => None,
        };
        if let Some((r, kind)) = struck {
            for g in &mut self.guitars {
                g.strike(r, kind, accent);
            }
            self.sounding = r;
        }
        match sec.bass {
            BassPart::Off => self.bass.mute(),
            BassPart::Follow => match (struck, c) {
                (Some((r, kind)), _) => {
                    let a = if kind == Strike::Mute { 0.8 } else { 1.0 };
                    self.bass.pick(r, false, a * accent);
                }
                (None, b'.') => self.bass.mute(),
                (None, _) if step % 2 == 0 && self.bass.energy > 0.02 => {
                    self.bass.pick(self.sounding, false, 0.9 * accent);
                }
                _ => {}
            },
            BassPart::Pattern(p) => match p[bar % p.len()].as_bytes()[step] {
                b'x' => self.bass.pick(bar_chord, false, accent),
                b'o' => self.bass.pick(bar_chord, true, 0.85 * accent),
                b'.' => self.bass.mute(),
                _ => {}
            },
        }

        // Drums.
        let g = if sec.fill && last_bar {
            FILL
        } else {
            sec.drums[bar % sec.drums.len()]
        };
        let at = |s: &str| s.as_bytes().get(step).copied().unwrap_or(b'.');
        if at(g.kick) == b'x' {
            let vel = match step % 4 {
                0 => 1.0,
                2 => 0.9,
                _ => 0.72, // double-kick 16ths
            };
            self.kick.fire(vel, 1.0);
            self.duck = 1.0;
        }
        match at(g.snare) {
            b'x' => self.snare.fire(0.85, 1.0),
            b'X' => self.snare.fire(1.0, 1.0),
            b'g' => self.snare.fire(0.3, 1.0),
            b'r' => self.snare.fire(0.25 + 0.6 * step as f32 / 15.0, 1.0),
            _ => {}
        }
        match at(g.hat) {
            b'x' => self.hat.fire(if downbeat { 0.6 } else { 0.45 }, 0.0),
            b'-' => self.hat.fire(0.25, 0.0),
            b'o' => self.hat.fire(0.5, 1.0),
            _ => {}
        }
        let tom = match at(g.tom) {
            b'h' => 200.0,
            b'm' => 150.0,
            b'l' => 112.0,
            b'f' => 84.0,
            _ => 0.0,
        };
        if tom > 0.0 {
            self.tom.fire(0.9, tom);
        }
        let crash_bar = bar == 0 || (sec.crash_every > 0 && bar % sec.crash_every == 0);
        // No crash on the intro's first beat: it starts with just the riff.
        if step == 0 && crash_bar && !(self.section == 0 && bar == 0) {
            self.crash.fire(1.0, 1.0);
        }
        if sec.riser && bar + 2 == sec.bars && step == 0 {
            self.riser_t = 0.0;
            self.riser_len = 2.0 * STEPS as f32 * self.step_len / self.rate;
        }

        // Lead melody.
        if let Some(phrases) = sec.lead {
            let phrase = phrases[bar % phrases.len()];
            let mut at = 0u8;
            let mut started = false;
            for &(note, len) in phrase {
                if at as usize == step {
                    if note == 0 {
                        self.lead.on = false;
                    } else {
                        self.lead.note_on(note, self.song.tone.lead_shift);
                    }
                    self.lead_left = len;
                    started = true;
                    break;
                }
                at += len;
            }
            if !started && self.lead_left == 0 {
                self.lead.on = false;
            }
        } else {
            self.lead.on = false;
        }
        self.lead_left = self.lead_left.saturating_sub(1);

        // Arpeggio over the sounding chord: root, octave, fifth, octave, tenth, octave,
        // fifth, twelfth.
        let r = self.sounding;
        let pat = [0, 12, 7, 12, 12 + self.song.third(r), 12, 7, 19];
        self.arp.note = (r + 24 + pat[step % pat.len()]) as f32;
        self.arp.energy = if step % 4 == 0 { 1.0 } else { 0.75 };
    }

    /// Renders one stereo frame (already at the soundtrack's level).
    pub fn frame(&mut self) -> (f32, f32) {
        if !self.playing {
            return (0.0, 0.0);
        }
        let k = self.k;
        // A different piece was asked for: fade this one out first.
        let switching = self.want != self.theme;
        let target = if switching { 0.0 } else { self.target_gain };
        // Fade in gently (resuming from pause), out quickly (gone before the next GO).
        let fade = if target < self.gain { 1.6 } else { 0.6 };
        self.gain += (target - self.gain) * k.smooth * fade;
        self.muffle += (self.target_muffle - self.muffle) * k.smooth;
        self.intensity += (self.target_intensity - self.intensity) * k.smooth * 0.3;
        if target == 0.0 && self.gain < 1e-3 {
            if switching && self.target_gain > 0.0 {
                self.start();
            } else {
                self.playing = false;
                return (0.0, 0.0);
            }
        }
        self.clock += 1.0;
        if self.clock >= self.step_len {
            self.clock -= self.step_len;
            self.advance();
        }

        let rate = self.rate;
        let dt = 1.0 / rate;
        let f = self.filters;
        let sec = self.song.sections[self.section];
        let tone = self.song.tone;

        let sweep = sec.sweep.then(|| {
            let total = (sec.bars * STEPS) as f32 * self.step_len;
            let at = ((self.bar * STEPS + self.step) as f32 * self.step_len + self.clock) / total;
            svf_g(350.0 * (at * 5.8).exp2(), rate)
        });
        self.duck *= k.duck_decay;
        let pump = 1.0 - 0.22 * self.duck;
        let gl = self.guitars[0].sample(rate, &f, sweep);
        let gr = self.guitars[1].sample(rate, &f, sweep);
        let bass = self.bass.sample(rate, f.attack, &k) * pump;

        let double = if sec.lead_double { 0.7 } else { 0.0 } + self.intensity * 0.3;
        let lead = self.lead.sample(rate, &k, &f, double.min(0.8), tone.synth);
        let arp_level = if sec.arp { 1.0 } else { self.intensity };
        let arp = if arp_level > 0.01 {
            let a = &mut self.arp;
            a.energy *= k.arp_decay;
            a.lfo = (a.lfo + dt * self.song.bpm / 60.0 / 8.0).fract();
            let width = 0.3 + 0.18 * (a.lfo * TAU * 2.0).sin();
            let x = a.osc.pulse(midi_hz(a.note) * dt, width);
            let cutoff = 900.0 + 3200.0 * (0.5 - 0.5 * (a.lfo * TAU).cos());
            a.lp.lp(x, svf_g(cutoff, rate), 0.7) * a.energy * arp_level
        } else {
            0.0
        };

        // Drums.
        let (n1, n2, n3) = (self.white(), self.white(), self.white());
        let kick = {
            let h = &mut self.kick;
            if h.on {
                let fq = 46.0 + 125.0 * (-h.t / 0.032).exp();
                let body = h.f1.lp(n1, f.g_hat, 1.0) * (-h.t / 0.004).exp() * 0.5;
                let s = Osc { phase: h.phase }.sine(fq * dt);
                h.phase = (h.phase + fq * dt).fract();
                h.t += dt;
                h.on = h.t < 0.7;
                ((s * (-h.t / 0.28).exp() + body) * 1.9).tanh() * h.vel
            } else {
                0.0
            }
        };
        let snare = {
            let h = &mut self.snare;
            if h.on {
                let drop = 1.0 + 0.4 * (-h.t / 0.015).exp();
                h.phase = (h.phase + 185.0 * drop * dt).fract();
                h.phase2 = (h.phase2 + 330.0 * drop * dt).fract();
                let tone = ((h.phase * TAU).sin() * 0.6 + (h.phase2 * TAU).sin() * 0.3)
                    * (-h.t / 0.05).exp();
                let (_, bp, hp) = h.f1.run(n2, f.g_snare, 0.9);
                let wires = (hp * 0.8 + bp * 0.5) * (-h.t / 0.16).exp();
                h.t += dt;
                h.on = h.t < 0.6;
                ((tone * 0.9 + wires * 1.1) * 1.4).tanh() * h.vel
            } else {
                0.0
            }
        };
        let hat = {
            let h = &mut self.hat;
            if h.on {
                let tau = if h.tune > 0.5 { 0.2 } else { 0.032 };
                let x = h.f1.hp(n3, f.g_hat, 1.0);
                let x = h.f2.hp(x, f.g_hat, 1.0);
                h.t += dt;
                h.on = h.t < tau * 7.0;
                x * (-h.t / tau).exp() * h.vel
            } else {
                0.0
            }
        };
        let tom = {
            let h = &mut self.tom;
            if h.on {
                let fq = h.tune * (1.0 + 0.6 * (-h.t / 0.04).exp());
                h.phase = (h.phase + fq * dt).fract();
                let s =
                    (h.phase * TAU).sin() * (-h.t / 0.32).exp() + n2 * (-h.t / 0.01).exp() * 0.25;
                h.t += dt;
                h.on = h.t < 1.2;
                (s * 1.5).tanh() * h.vel
            } else {
                0.0
            }
        };
        let (crash_l, crash_r) = {
            let h = &mut self.crash;
            if h.on {
                // Six detuned square partials (a metallic core) plus bright noise.
                let mut metal = 0.0;
                for (i, fq) in [302.0f32, 427.0, 539.0, 811.0, 1043.0, 1377.0]
                    .iter()
                    .enumerate()
                {
                    let ph = (h.t * fq * (1.0 + i as f32 * 0.011)).fract();
                    metal += if ph < 0.5 { 1.0 } else { -1.0 };
                }
                let env = (-h.t / 1.4).exp() * (1.0 - (-h.t / 0.002).exp());
                let a = h.f1.hp(n1 * 0.8 + metal * 0.08, f.g_crash, 0.8);
                let b = h.f2.hp(n3 * 0.8 + metal * 0.08, f.g_crash, 0.8);
                h.t += dt;
                h.on = h.t < 4.0;
                (a * env * h.vel, b * env * h.vel)
            } else {
                (0.0, 0.0)
            }
        };
        let riser = if self.riser_len > 0.0 {
            let p = (self.riser_t / self.riser_len).min(1.0);
            self.riser_t += dt;
            if self.riser_t >= self.riser_len {
                self.riser_len = 0.0;
            }
            let (_, bp, _) = self
                .riser_f
                .run(n1, svf_g(300.0 * (p * 5.0).exp2(), rate), 0.5);
            bp * p * p * 0.9
        } else {
            0.0
        };

        // Sends.
        let (dl, dr) = self.delay.run(lead * tone.delay + arp * 0.35);
        let (rl, rr) = self
            .reverb
            .run(snare * 0.35 + tom * 0.25 + lead * 0.12 + riser * 0.2);

        // Mix: guitars wide, bass and kick center, hats a little right, toms across.
        let guitars = (gl * tone.guitar * pump, gr * tone.guitar * pump);
        let center = kick * 0.62 + snare * 0.42 + bass * 0.36 + lead * tone.lead;
        let mut l = center
            + guitars.0
            + guitars.1 * 0.15
            + hat * 0.12
            + tom * 0.36
            + crash_l * 0.2
            + arp * tone.arp * 0.8
            + riser * 0.8
            + dl * 0.6
            + rl * 0.8;
        let mut r = center
            + guitars.1
            + guitars.0 * 0.15
            + hat * 0.18
            + tom * 0.3
            + crash_r * 0.2
            + arp * tone.arp * 1.2
            + riser * 0.8
            + dr * 0.6
            + rr * 0.8;

        // Bus glue, then the pause-menu muffle.
        l = (l * 0.9).tanh();
        r = (r * 0.9).tanh();
        if self.muffle > 1e-3 {
            let g = svf_g(18_000.0 * (-self.muffle * 5.0).exp2(), rate);
            l = self.muffle_f[0].lp(l, g, 0.9);
            r = self.muffle_f[1].lp(r, g, 0.9);
        }
        (l * self.gain, r * self.gain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::songs::Section;

    const RATE: f32 = 48_000.0;

    fn render(m: &mut Music, secs: f32) -> Vec<(f32, f32)> {
        (0..(RATE * secs) as usize).map(|_| m.frame()).collect()
    }

    fn rms(buf: &[(f32, f32)]) -> f32 {
        (buf.iter().map(|(l, r)| l * l + r * r).sum::<f32>() / (2 * buf.len()) as f32).sqrt()
    }

    fn bar_secs(theme: Theme) -> f32 {
        STEPS as f32 * 60.0 / theme.song().bpm / 4.0
    }

    fn playing(theme: Theme, intensity: f32) -> Music {
        let mut m = Music::new(RATE);
        m.set(theme, 1.0, 0.0, intensity);
        m
    }

    fn bars(sections: &[Section]) -> usize {
        sections.iter().map(|s| s.bars).sum()
    }

    #[test]
    fn every_song_is_well_formed() {
        for theme in Theme::ALL {
            let song = theme.song();
            assert!(
                song.bpm >= MIN_BPM,
                "{theme:?} is slower than the delay allows"
            );
            assert!(song.loop_to < song.sections.len());
            for (i, s) in song.sections.iter().enumerate() {
                let at = format!("{theme:?} section {i}");
                assert!(s.bars > 0 && !s.chords.is_empty(), "{at}");
                assert!(!s.guitar.is_empty() && !s.drums.is_empty(), "{at}");
                for (pat, notes) in s.guitar {
                    assert_eq!(pat.len(), STEPS, "{at} guitar {pat}");
                    assert!(notes.is_empty() || notes.len() == STEPS, "{at} {notes}");
                    assert!(notes.bytes().all(|c| chord_letter(c).is_some()), "{at}");
                }
                for g in s.drums.iter().chain([&FILL]) {
                    for p in [g.kick, g.snare, g.hat, g.tom] {
                        assert!(p.is_empty() || p.len() == STEPS, "{at} drums {p}");
                    }
                }
                if let BassPart::Pattern(p) = s.bass {
                    assert!(p.iter().all(|b| b.len() == STEPS), "{at} bass");
                }
                if let Some(phrases) = s.lead {
                    for (b, p) in phrases.iter().enumerate() {
                        let len: u32 = p.iter().map(|n| n.1 as u32).sum();
                        assert_eq!(len, STEPS as u32, "{at} lead bar {b}");
                    }
                }
            }
        }
    }

    #[test]
    fn silent_until_asked() {
        let mut m = Music::new(RATE);
        assert!(
            render(&mut m, 0.2)
                .iter()
                .all(|&(l, r)| l == 0.0 && r == 0.0)
        );
        assert!(!m.playing());
    }

    #[test]
    fn every_song_plays_through_and_loops_bounded() {
        for theme in Theme::ALL {
            let mut m = playing(theme, 0.0);
            let mut peak = 0.0f32;
            let mut quiet = 0;
            let secs = bars(theme.song().sections) as f32 * bar_secs(theme) + 4.0;
            for _ in 0..(secs as usize) {
                let second = render(&mut m, 1.0);
                if rms(&second) < 0.01 {
                    quiet += 1;
                }
                for (l, r) in second {
                    assert!(l.is_finite() && r.is_finite(), "{theme:?}");
                    peak = peak.max(l.abs()).max(r.abs());
                }
            }
            assert!(peak <= 1.0 && peak > 0.3, "{theme:?} peak {peak}");
            assert!(quiet <= 1, "{theme:?} has {quiet} near-silent seconds");
            assert_eq!(
                m.position().0,
                theme.song().loop_to,
                "{theme:?} looped back"
            );
        }
    }

    #[test]
    fn the_race_songs_start_loud_on_go() {
        for theme in [Theme::Dust, Theme::Castle] {
            let mut m = playing(theme, 0.0);
            let first = rms(&render(&mut m, 0.2));
            assert!(first > 0.03, "{theme:?} audible at once: {first}");
        }
    }

    #[test]
    fn the_chorus_is_bigger_than_the_intro() {
        // Castle opens on war drums: a strong intro, so a smaller step up.
        for (theme, lift) in [(Theme::Dust, 1.5), (Theme::Castle, 1.25), (Theme::Sky, 1.5)] {
            let mut m = playing(theme, 0.0);
            let bar = bar_secs(theme);
            let intro = rms(&render(&mut m, 2.0 * bar));
            let to_chorus = bars(&theme.song().sections[..3]);
            render(&mut m, (to_chorus - 2) as f32 * bar + 0.05);
            assert_eq!(m.position().0, 3, "{theme:?}");
            let chorus = rms(&render(&mut m, 2.0 * bar));
            assert!(
                chorus > intro * lift,
                "{theme:?} intro {intro}, chorus {chorus}"
            );
        }
    }

    #[test]
    fn songs_sound_different() {
        // Same length of each song's verse: no two alike (a cheap guard against a song
        // accidentally pointing at another's data).
        let verse = |theme: Theme| {
            let mut m = playing(theme, 0.0);
            render(&mut m, 4.0 * bar_secs(theme) + 0.05);
            render(&mut m, 2.0)
        };
        let takes: Vec<_> = Theme::ALL.iter().map(|&t| verse(t)).collect();
        for i in 0..takes.len() {
            for j in i + 1..takes.len() {
                let diff: Vec<_> = takes[i]
                    .iter()
                    .zip(&takes[j])
                    .map(|(a, b)| (a.0 - b.0, a.1 - b.1))
                    .collect();
                assert!(
                    rms(&diff) > 0.05,
                    "{:?} vs {:?}",
                    Theme::ALL[i],
                    Theme::ALL[j]
                );
            }
        }
    }

    #[test]
    fn fades_out_and_restarts_from_the_top() {
        let mut m = playing(Theme::Dust, 0.0);
        render(&mut m, 6.0);
        assert!(m.position() != (0, 0, 0));
        m.set(Theme::Dust, 0.0, 0.0, 0.0);
        render(&mut m, 3.0);
        assert!(!m.playing());
        m.set(Theme::Dust, 1.0, 0.0, 0.0);
        assert_eq!(m.position(), (0, 0, 0));
    }

    #[test]
    fn a_new_theme_fades_the_old_one_out_then_starts() {
        let mut m = playing(Theme::Menu, 0.0);
        render(&mut m, 5.0);
        m.set(Theme::Sky, 1.0, 0.0, 0.0);
        assert_eq!(m.theme(), Theme::Menu, "fades out first");
        render(&mut m, 2.0);
        assert_eq!(m.theme(), Theme::Sky);
        assert!(m.playing());
        assert_eq!(m.position().0, 0, "from the top");
    }

    #[test]
    fn muffling_takes_the_top_off() {
        // High-frequency energy: the second difference of the signal.
        let bright = |muffle: f32| {
            let mut m = Music::new(RATE);
            m.set(Theme::Dust, 1.0, muffle, 0.0);
            let buf = render(&mut m, 3.0);
            buf.windows(3)
                .map(|w| (w[2].0 - 2.0 * w[1].0 + w[0].0).abs())
                .sum::<f32>()
        };
        let (open, muffled) = (bright(0.0), bright(1.0));
        assert!(open > muffled * 4.0, "open {open}, muffled {muffled}");
    }

    #[test]
    fn intensity_adds_the_arpeggio_to_the_verse() {
        // The synthesis is deterministic, so the difference is the added layer.
        let verse = |intensity: f32| {
            let mut m = playing(Theme::Dust, intensity);
            render(&mut m, 4.0 * bar_secs(Theme::Dust) + 0.1);
            assert_eq!(m.position().0, 1, "in the verse");
            render(&mut m, 2.0 * bar_secs(Theme::Dust))
        };
        let (calm, intense) = (verse(0.0), verse(1.0));
        let diff: Vec<_> = calm
            .iter()
            .zip(&intense)
            .map(|(a, b)| (b.0 - a.0, b.1 - a.1))
            .collect();
        assert!(rms(&diff) > 0.01, "added layer {}", rms(&diff));
    }
}
