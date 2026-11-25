//! Time-trial ghosts: record the kart's transform each lap, keep the best, replay it.

use std::path::{Path, PathBuf};

use glam::{Quat, Vec3};

const SAMPLE_DT: f32 = 1.0 / 30.0;
const MAGIC: &[u8; 4] = b"NRGH";
const VERSION: u32 = 1;
/// Bytes per sample: t, pos (3), rot (4).
const SAMPLE_BYTES: usize = 8 * 4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GhostSample {
    /// Seconds since the lap started.
    pub t: f32,
    pub pos: [f32; 3],
    pub rot: [f32; 4],
}

/// One recorded lap.
#[derive(Debug, Clone, PartialEq)]
pub struct Ghost {
    pub lap_time: f64,
    pub samples: Vec<GhostSample>,
}

impl Ghost {
    /// Interpolated transform `t` seconds into the lap, or `None` outside it.
    pub fn sample(&self, t: f32) -> Option<(Vec3, Quat)> {
        let s = &self.samples;
        if s.is_empty() || t < s[0].t || t > s[s.len() - 1].t {
            return None;
        }
        let i = s.partition_point(|x| x.t <= t).clamp(1, s.len() - 1);
        let (a, b) = (s[i - 1], s[i]);
        let k = ((t - a.t) / (b.t - a.t).max(1e-6)).clamp(0.0, 1.0);
        let pos = Vec3::from_array(a.pos).lerp(Vec3::from_array(b.pos), k);
        let rot = Quat::from_array(a.rot).slerp(Quat::from_array(b.rot), k);
        Some((pos, rot))
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(20 + self.samples.len() * SAMPLE_BYTES);
        out.extend(MAGIC);
        out.extend(VERSION.to_le_bytes());
        out.extend(self.lap_time.to_le_bytes());
        out.extend((self.samples.len() as u32).to_le_bytes());
        for s in &self.samples {
            out.extend(s.t.to_le_bytes());
            for v in s.pos.iter().chain(&s.rot) {
                out.extend(v.to_le_bytes());
            }
        }
        out
    }

    pub fn decode(b: &[u8]) -> Option<Self> {
        if b.get(0..4)? != MAGIC || u32::from_le_bytes(b.get(4..8)?.try_into().ok()?) != VERSION {
            return None;
        }
        let lap_time = f64::from_le_bytes(b.get(8..16)?.try_into().ok()?);
        let n = u32::from_le_bytes(b.get(16..20)?.try_into().ok()?) as usize;
        let body = b.get(20..20 + n.checked_mul(SAMPLE_BYTES)?)?;
        let f =
            |i: usize| f32::from_le_bytes(body[i * 4..i * 4 + 4].try_into().expect("4-byte slice"));
        let samples = (0..n)
            .map(|k| {
                let o = k * 8;
                GhostSample {
                    t: f(o),
                    pos: [f(o + 1), f(o + 2), f(o + 3)],
                    rot: [f(o + 4), f(o + 5), f(o + 6), f(o + 7)],
                }
            })
            .collect();
        (lap_time.is_finite() && lap_time > 0.0).then_some(Ghost { lap_time, samples })
    }
}

/// Samples the kart at 30 Hz during a lap.
#[derive(Debug, Default)]
pub struct GhostRecorder {
    samples: Vec<GhostSample>,
}

impl GhostRecorder {
    /// Records if at least one sample period passed since the last sample.
    pub fn record(&mut self, t: f32, pos: Vec3, rot: Quat) {
        if self
            .samples
            .last()
            .is_none_or(|s| t - s.t >= SAMPLE_DT - 1e-4)
        {
            self.samples.push(GhostSample {
                t,
                pos: pos.to_array(),
                rot: rot.to_array(),
            });
        }
    }

    /// Ends the lap and starts a fresh recording.
    pub fn finish(&mut self, lap_time: f64) -> Ghost {
        Ghost {
            lap_time,
            samples: std::mem::take(&mut self.samples),
        }
    }
}

/// The fastest lap so far.
#[derive(Debug, Default)]
pub struct BestGhost {
    best: Option<Ghost>,
}

impl BestGhost {
    pub fn from_ghost(g: Option<Ghost>) -> Self {
        Self { best: g }
    }

    /// Keeps `g` if it is faster; returns true when it became the best.
    pub fn offer(&mut self, g: Ghost) -> bool {
        if self.best.as_ref().is_none_or(|b| g.lap_time < b.lap_time) && g.samples.len() >= 2 {
            self.best = Some(g);
            return true;
        }
        false
    }

    pub fn ghost(&self) -> Option<&Ghost> {
        self.best.as_ref()
    }
}

/// `$XDG_DATA_HOME/nailrunners`, `$HOME/.local/share/nailrunners`, or `%APPDATA%/nailrunners`.
pub fn data_dir_from(
    xdg: Option<PathBuf>,
    home: Option<PathBuf>,
    appdata: Option<PathBuf>,
) -> Option<PathBuf> {
    xdg.filter(|p| p.is_absolute() || p.to_string_lossy().starts_with('/'))
        .or_else(|| appdata.clone().filter(|_| cfg!(windows)))
        .or_else(|| home.map(|h| h.join(".local").join("share")))
        .or(appdata)
        .map(|d| d.join("nailrunners"))
}

pub fn data_dir() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    data_dir_from(env("XDG_DATA_HOME"), env("HOME"), env("APPDATA"))
}

pub fn ghost_path(dir: &Path, track: &str) -> PathBuf {
    dir.join("ghosts").join(format!("{track}.ghost"))
}

pub fn load(path: &Path) -> Option<Ghost> {
    Ghost::decode(&std::fs::read(path).ok()?)
}

pub fn save(path: &Path, g: &Ghost) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Write then rename, so a crash never leaves a half-written best lap.
    let tmp = path.with_extension("ghost.tmp");
    std::fs::write(&tmp, g.encode())?;
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Quat, Vec3};

    fn lap(time_scale: f32) -> Ghost {
        let mut r = GhostRecorder::default();
        for i in 0..=60 {
            let t = i as f32 / 30.0 * time_scale;
            r.record(
                t,
                Vec3::new(i as f32, 0.0, 0.0),
                Quat::from_rotation_y(i as f32 * 0.01),
            );
        }
        r.finish(2.0 * time_scale as f64)
    }

    #[test]
    fn records_at_30hz_and_interpolates() {
        let mut r = GhostRecorder::default();
        for i in 0..120 {
            r.record(
                i as f32 / 120.0,
                Vec3::new(i as f32, 0.0, 0.0),
                Quat::IDENTITY,
            );
        }
        let g = r.finish(1.0);
        assert!(
            (29..=31).contains(&g.samples.len()),
            "{} samples for 1 s",
            g.samples.len()
        );
        let (p, _) = g.sample(0.5).unwrap();
        assert!((p.x - 60.0).abs() < 2.5, "halfway position {p}");
        assert!(g.sample(5.0).is_none(), "past the end the ghost is gone");
    }

    #[test]
    fn keeps_only_better_laps() {
        let mut best = BestGhost::default();
        assert!(best.offer(lap(1.0)));
        assert!(!best.offer(lap(1.5)), "slower lap ignored");
        assert!(best.offer(lap(0.8)), "faster lap replaces");
        assert_eq!(best.ghost().unwrap().lap_time, 2.0 * 0.8f32 as f64);
    }

    #[test]
    fn encode_decode_round_trip() {
        let g = lap(1.0);
        let back = Ghost::decode(&g.encode()).unwrap();
        assert_eq!(back.lap_time, g.lap_time);
        assert_eq!(back.samples.len(), g.samples.len());
        assert_eq!(back.samples[7].pos, g.samples[7].pos);
        assert!(Ghost::decode(b"garbage").is_none());
    }

    #[test]
    fn data_dir_prefers_xdg() {
        let d = data_dir_from(Some("/x/data".into()), Some("/home/u".into()), None).unwrap();
        assert_eq!(d, std::path::PathBuf::from("/x/data/nailrunners"));
        let d = data_dir_from(None, Some("/home/u".into()), None).unwrap();
        assert_eq!(
            d,
            std::path::PathBuf::from("/home/u/.local/share/nailrunners")
        );
        let d = data_dir_from(None, None, Some("C:/Users/u/AppData/Roaming".into())).unwrap();
        assert_eq!(
            d,
            std::path::PathBuf::from("C:/Users/u/AppData/Roaming/nailrunners")
        );
    }
}
