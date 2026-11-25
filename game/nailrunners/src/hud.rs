//! HUD layout helpers (pure) and drawing.

use rivet_physics::{BoostTier, VehicleParams, VehicleState};
use rivet_render::Overlay;

use crate::style::tier_color;

/// Lit segments of an `n`-segment speed bar (full at `max`).
pub fn speed_segments(speed: f32, max: f32, n: usize) -> usize {
    ((speed.abs() / max).clamp(0.0, 1.0) * n as f32).round() as usize
}

/// Current tier (0 = none yet) and progress toward the next tier (1.0 at the top).
pub fn drift_progress(charge: f32, tiers: &[BoostTier]) -> (usize, f32) {
    let tier = tiers.iter().filter(|t| charge >= t.charge).count();
    if tier >= tiers.len() {
        return (tier, 1.0);
    }
    let from = if tier == 0 {
        0.0
    } else {
        tiers[tier - 1].charge
    };
    let to = tiers[tier].charge;
    (tier, ((charge - from) / (to - from)).clamp(0.0, 1.0))
}

pub fn fmt_time(t: f64) -> String {
    let m = (t / 60.0).floor();
    format!("{}:{:06.3}", m as u32, t - m * 60.0)
}

const SHADOW: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const DIM: [f32; 4] = [1.0, 1.0, 1.0, 0.65];
const PANEL: [f32; 4] = [0.05, 0.04, 0.03, 0.45];
const ACCENT: [f32; 4] = [1.0, 0.55, 0.12, 1.0];

fn text(o: &mut Overlay, x: f32, y: f32, scale: f32, color: [f32; 4], s: &str) {
    o.text(x + scale * 0.6, y + scale * 0.6, scale, SHADOW, s);
    o.text(x, y, scale, color, s);
}

fn width(s: &str, scale: f32) -> f32 {
    rivet_render::font::text_width(s, scale)
}

pub struct HudInfo {
    pub lap: u32,
    pub lap_time: f64,
    pub last: Option<f64>,
    pub best: Option<f64>,
    pub ghost: bool,
}

/// Draws the race HUD for a `w`×`h` window.
pub fn draw(
    o: &mut Overlay,
    w: f32,
    h: f32,
    kart: &VehicleState,
    params: &VehicleParams,
    info: &HudInfo,
) {
    let ui = (h / 900.0).clamp(0.6, 2.0);

    // Lap and time, top center.
    let lap = format!("LAP {}", info.lap);
    let time = fmt_time(info.lap_time);
    let ts = 5.0 * ui;
    let tw = width(&time, ts);
    o.rect(
        w * 0.5 - tw * 0.5 - 18.0 * ui,
        14.0 * ui,
        tw + 36.0 * ui,
        96.0 * ui,
        PANEL,
    );
    o.rect(
        w * 0.5 - tw * 0.5 - 18.0 * ui,
        14.0 * ui,
        tw + 36.0 * ui,
        3.0 * ui,
        ACCENT,
    );
    text(
        o,
        w * 0.5 - width(&lap, 2.5 * ui) * 0.5,
        24.0 * ui,
        2.5 * ui,
        ACCENT,
        &lap,
    );
    text(o, w * 0.5 - tw * 0.5, 46.0 * ui, ts, WHITE, &time);
    let sub = format!(
        "LAST {}   BEST {}{}",
        info.last.map_or_else(|| "--".into(), fmt_time),
        info.best.map_or_else(|| "--".into(), fmt_time),
        if info.ghost { "   GHOST ON" } else { "" }
    );
    text(
        o,
        w * 0.5 - width(&sub, 1.6 * ui) * 0.5,
        88.0 * ui,
        1.6 * ui,
        DIM,
        &sub,
    );

    // Speed, bottom right: big number plus a segmented bar.
    let kmh = (kart.speed().abs() * 3.6).round() as i32;
    let num = format!("{kmh}");
    let ns = 8.0 * ui;
    let right = w - 28.0 * ui;
    let base = h - 34.0 * ui;
    let unit_w = width("KM/H", 2.0 * ui);
    text(o, right - unit_w, base - 14.0 * ui, 2.0 * ui, DIM, "KM/H");
    text(
        o,
        right - unit_w - 10.0 * ui - width(&num, ns),
        base - 56.0 * ui,
        ns,
        WHITE,
        &num,
    );
    let n = 24;
    let lit = speed_segments(
        kart.speed(),
        params.max_speed + params.boost_tiers[2].speed,
        n,
    );
    let seg_w = 9.0 * ui;
    let bar_x = right - n as f32 * (seg_w + 3.0 * ui);
    for i in 0..n {
        let t = i as f32 / n as f32;
        let on = i < lit;
        let mut c = if t < 0.6 {
            [0.95, 0.95, 0.9, 1.0]
        } else if t < 0.78 {
            ACCENT
        } else {
            [1.0, 0.25, 0.15, 1.0]
        };
        if !on {
            c = [0.2, 0.18, 0.16, 0.55];
        } else if kart.boost_timer > 0.0 {
            c = [0.5, 0.85, 1.0, 1.0];
        }
        let seg_h = (6.0 + 10.0 * t) * ui;
        o.rect(
            bar_x + i as f32 * (seg_w + 3.0 * ui),
            base - 74.0 * ui - seg_h,
            seg_w,
            seg_h,
            c,
        );
    }

    // Drift meter, bottom center: three pips filling in tier colors.
    if let Some(d) = kart.drift {
        let (tier, frac) = drift_progress(d.charge, &params.boost_tiers);
        let pip_w = 54.0 * ui;
        let gap = 10.0 * ui;
        let x0 = w * 0.5 - (3.0 * pip_w + 2.0 * gap) * 0.5;
        let y = h - 70.0 * ui;
        for k in 0..3 {
            let x = x0 + k as f32 * (pip_w + gap);
            o.rect(
                x - 2.0 * ui,
                y - 2.0 * ui,
                pip_w + 4.0 * ui,
                16.0 * ui,
                PANEL,
            );
            let fill = if k < tier {
                1.0
            } else if k == tier {
                frac
            } else {
                0.0
            };
            o.rect(x, y, pip_w * fill, 12.0 * ui, tier_color(k + 1));
        }
        let label = "DRIFT";
        text(
            o,
            w * 0.5 - width(label, 2.2 * ui) * 0.5,
            y - 30.0 * ui,
            2.2 * ui,
            tier_color(tier),
            label,
        );
    } else if kart.boost_timer > 0.0 {
        let label = "BOOST!";
        text(
            o,
            w * 0.5 - width(label, 4.0 * ui) * 0.5,
            h - 96.0 * ui,
            4.0 * ui,
            [0.55, 0.9, 1.0, 1.0],
            label,
        );
    }
}

/// "1ST", "2ND", "3RD", "4TH"… (11–13 are "TH").
pub fn ordinal(n: usize) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "TH",
        (1, _) => "ST",
        (2, _) => "ND",
        (3, _) => "RD",
        _ => "TH",
    };
    format!("{n}{suffix}")
}

/// What the countdown shows: `remaining` seconds before GO (negative = after GO).
pub fn countdown_label(remaining: f32) -> Option<&'static str> {
    match remaining {
        r if r > 2.0 => Some("3"),
        r if r > 1.0 => Some("2"),
        r if r > 0.0 => Some("1"),
        r if r > -1.0 => Some("GO!"),
        _ => None,
    }
}

/// Top-down projection of world XZ into a screen rectangle, aspect preserved.
#[derive(Debug, Clone, Copy)]
pub struct Minimap {
    origin: (f32, f32),
    lo: (f32, f32),
    scale: f32,
}

impl Minimap {
    /// Fits `points` into `rect` = (x, y, w, h).
    pub fn fit(points: &[glam::Vec3], rect: (f32, f32, f32, f32)) -> Self {
        let (mut lo, mut hi) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
        for p in points {
            lo = (lo.0.min(p.x), lo.1.min(p.z));
            hi = (hi.0.max(p.x), hi.1.max(p.z));
        }
        let scale = (rect.2 / (hi.0 - lo.0).max(1.0)).min(rect.3 / (hi.1 - lo.1).max(1.0));
        Self {
            origin: (rect.0, rect.1),
            lo,
            scale,
        }
    }

    pub fn project(&self, p: glam::Vec3) -> (f32, f32) {
        (
            self.origin.0 + (p.x - self.lo.0) * self.scale,
            self.origin.1 + (p.z - self.lo.1) * self.scale,
        )
    }
}

/// Race-level HUD data.
pub struct RaceHud<'a> {
    pub position: usize,
    pub racers: usize,
    /// Seconds before GO (negative after), while the label should show.
    pub countdown: Option<f32>,
    pub finished: Option<u32>,
    /// (name, tint, finish time) in final order, once the player has finished.
    pub results: Option<Vec<(&'a str, [f32; 3], Option<f64>)>>,
    pub track: &'a [glam::Vec3],
    /// (position, tint, is the player)
    pub karts: Vec<(glam::Vec3, [f32; 3], bool)>,
}

/// Position, minimap, countdown and results.
pub fn draw_race(o: &mut Overlay, w: f32, h: f32, race: &RaceHud) {
    let ui = (h / 900.0).clamp(0.6, 2.0);
    // Position, top left.
    let pos = ordinal(race.position);
    let total = format!("/{}", race.racers);
    text(o, 26.0 * ui, 20.0 * ui, 9.0 * ui, WHITE, &pos);
    text(
        o,
        26.0 * ui + width(&pos, 9.0 * ui) + 6.0 * ui,
        50.0 * ui,
        3.0 * ui,
        DIM,
        &total,
    );

    // Minimap, bottom left.
    let size = 190.0 * ui;
    let rect = (22.0 * ui, h - size - 22.0 * ui, size, size);
    o.rect(
        rect.0 - 8.0 * ui,
        rect.1 - 8.0 * ui,
        size + 16.0 * ui,
        size + 16.0 * ui,
        PANEL,
    );
    let mm = Minimap::fit(race.track, rect);
    for p in race.track.iter().step_by(3) {
        let (x, y) = mm.project(*p);
        o.rect(
            x - 1.5 * ui,
            y - 1.5 * ui,
            3.0 * ui,
            3.0 * ui,
            [0.85, 0.82, 0.78, 0.8],
        );
    }
    for (p, tint, me) in &race.karts {
        let (x, y) = mm.project(*p);
        let s = if *me { 9.0 } else { 6.0 } * ui;
        if *me {
            o.rect(
                x - s * 0.5 - 2.0 * ui,
                y - s * 0.5 - 2.0 * ui,
                s + 4.0 * ui,
                s + 4.0 * ui,
                WHITE,
            );
        }
        let c = [
            (tint[0] * 0.7).min(1.0),
            (tint[1] * 0.7).min(1.0),
            (tint[2] * 0.7).min(1.0),
            1.0,
        ];
        o.rect(x - s * 0.5, y - s * 0.5, s, s, if *me { ACCENT } else { c });
    }

    // Countdown, center.
    if let Some(label) = race.countdown.and_then(countdown_label) {
        let s = 26.0 * ui;
        let color = if label == "GO!" {
            [0.4, 1.0, 0.5, 1.0]
        } else {
            ACCENT
        };
        text(
            o,
            w * 0.5 - width(label, s) * 0.5,
            h * 0.32,
            s,
            color,
            label,
        );
    }

    // Results.
    if let Some(results) = &race.results {
        let pw = 520.0 * ui;
        let ph = (90.0 + 40.0 * results.len() as f32) * ui;
        let (x, y) = (w * 0.5 - pw * 0.5, h * 0.5 - ph * 0.5);
        o.rect(x, y, pw, ph, [0.04, 0.03, 0.02, 0.78]);
        o.rect(x, y, pw, 4.0 * ui, ACCENT);
        let title = race.finished.map_or_else(
            || "RESULTS".to_string(),
            |p| format!("YOU FINISHED {}", ordinal(p as usize)),
        );
        text(
            o,
            w * 0.5 - width(&title, 4.0 * ui) * 0.5,
            y + 22.0 * ui,
            4.0 * ui,
            ACCENT,
            &title,
        );
        for (k, (name, tint, time)) in results.iter().enumerate() {
            let ry = y + (80.0 + 40.0 * k as f32) * ui;
            let c = [tint[0].min(1.0), tint[1].min(1.0), tint[2].min(1.0), 1.0];
            o.rect(x + 30.0 * ui, ry, 14.0 * ui, 22.0 * ui, c);
            text(
                o,
                x + 60.0 * ui,
                ry,
                3.0 * ui,
                WHITE,
                &format!("{}  {name}", ordinal(k + 1)),
            );
            let t = time.map_or_else(|| "--".into(), fmt_time);
            text(
                o,
                x + pw - 30.0 * ui - width(&t, 3.0 * ui),
                ry,
                3.0 * ui,
                DIM,
                &t,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rivet_physics::VehicleParams;

    #[test]
    fn speed_segments_scale_and_cap() {
        assert_eq!(speed_segments(0.0, 32.0, 20), 0);
        assert_eq!(speed_segments(16.0, 32.0, 20), 10);
        assert_eq!(speed_segments(60.0, 32.0, 20), 20);
    }

    #[test]
    fn drift_progress_tiers() {
        let p = VehicleParams::default();
        assert_eq!(drift_progress(0.0, &p.boost_tiers), (0, 0.0));
        let (tier, frac) = drift_progress(0.275, &p.boost_tiers);
        assert_eq!(tier, 0);
        assert!((frac - 0.5).abs() < 1e-3);
        assert_eq!(drift_progress(9.0, &p.boost_tiers), (3, 1.0));
    }

    #[test]
    fn ordinals() {
        assert_eq!(ordinal(1), "1ST");
        assert_eq!(ordinal(2), "2ND");
        assert_eq!(ordinal(3), "3RD");
        assert_eq!(ordinal(8), "8TH");
        assert_eq!(ordinal(11), "11TH");
        assert_eq!(ordinal(22), "22ND");
    }

    #[test]
    fn minimap_maps_bounds_into_rect_keeping_aspect() {
        let pts = [
            glam::Vec3::new(0.0, 0.0, 0.0),
            glam::Vec3::new(200.0, 0.0, 100.0),
        ];
        let m = Minimap::fit(&pts, (10.0, 20.0, 100.0, 100.0));
        let a = m.project(pts[0]);
        let b = m.project(pts[1]);
        assert!((b.0 - a.0 - 100.0).abs() < 1e-3, "width fills the rect");
        assert!((b.1 - a.1).abs() - 50.0 < 1e-3, "aspect kept");
        assert!(a.0 >= 10.0 && b.0 <= 110.0 && a.1 >= 20.0 && a.1 <= 120.0);
    }

    #[test]
    fn countdown_labels() {
        assert_eq!(countdown_label(2.4), Some("3"));
        assert_eq!(countdown_label(1.5), Some("2"));
        assert_eq!(countdown_label(0.2), Some("1"));
        assert_eq!(countdown_label(-0.5), Some("GO!"));
        assert_eq!(countdown_label(-1.5), None);
    }

    #[test]
    fn time_format() {
        assert_eq!(fmt_time(0.0), "0:00.000");
        assert_eq!(fmt_time(75.25), "1:15.250");
    }
}
