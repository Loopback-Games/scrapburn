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
    o.label(x + scale * 0.45, y + scale * 0.45, scale, SHADOW, s);
    o.label(x, y, scale, color, s);
}

fn width(s: &str, scale: f32) -> f32 {
    rivet_render::label_width(s, scale)
}

pub struct HudInfo {
    pub lap: u32,
    /// Shown as "LAP n/total" in a race.
    pub total_laps: Option<u32>,
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
    let lap = match info.total_laps {
        Some(t) => format!("LAP {}/{t}", info.lap.min(t)),
        None => format!("LAP {}", info.lap),
    };
    let time = fmt_time(info.lap_time);
    let ts = 4.2 * ui;
    let tw = width(&time, ts);
    o.rect(
        w * 0.5 - tw * 0.5 - 18.0 * ui,
        14.0 * ui,
        tw + 36.0 * ui,
        112.0 * ui,
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
        w * 0.5 - width(&sub, 1.5 * ui) * 0.5,
        100.0 * ui,
        1.5 * ui,
        DIM,
        &sub,
    );

    // Speed, bottom right: big number plus a segmented bar.
    let kmh = (kart.speed().abs() * 3.6).round() as i32;
    let num = format!("{kmh}");
    let ns = 6.0 * ui;
    let right = w - 28.0 * ui;
    let base = h - 34.0 * ui;
    let unit_w = width("KM/H", 2.0 * ui);
    text(o, right - unit_w, base - 24.0 * ui, 2.0 * ui, DIM, "KM/H");
    text(
        o,
        right - unit_w - 10.0 * ui - width(&num, ns),
        base - 66.0 * ui,
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
            base - 80.0 * ui - seg_h,
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

/// One results line: name, kart tint, finish time.
pub type ResultRow<'a> = (&'a str, [f32; 3], Option<f64>);

/// One circuit standings line: name, kart tint, points this round, total points.
pub type PointsRow<'a> = (&'a str, [f32; 3], u8, u8);

/// Circuit progress for the race HUD.
pub struct CircuitHud<'a> {
    /// 1-based round.
    pub round: usize,
    pub rounds: usize,
    /// Standings after this round, leader first.
    pub table: Vec<PointsRow<'a>>,
}

/// Race-level HUD data.
pub struct RaceHud<'a> {
    pub position: usize,
    pub racers: usize,
    /// Seconds before GO (negative after), while the label should show.
    pub countdown: Option<f32>,
    pub finished: Option<u32>,
    /// (name, tint, finish time) in final order, once the player has finished.
    pub results: Option<Vec<ResultRow<'a>>>,
    /// Round and points, when the race is part of a circuit.
    pub circuit: Option<CircuitHud<'a>>,
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

    if let Some(c) = &race.circuit {
        text(
            o,
            26.0 * ui,
            118.0 * ui,
            2.2 * ui,
            ACCENT,
            &format!("CIRCUIT  ROUND {} / {}", c.round, c.rounds),
        );
    }

    // Results.
    if let Some(results) = &race.results {
        let gap = 20.0 * ui;
        let pw = if race.circuit.is_some() {
            (520.0 * ui).min((w - 3.0 * gap) * 0.5)
        } else {
            520.0 * ui
        };
        let ph = (90.0 + 40.0 * results.len() as f32) * ui;
        let x = if race.circuit.is_some() {
            w * 0.5 - pw - gap * 0.5
        } else {
            w * 0.5 - pw * 0.5
        };
        let y = h * 0.5 - ph * 0.5;
        if let Some(c) = &race.circuit {
            draw_points(o, ui, (w * 0.5 + gap * 0.5, y), pw, c);
        }
        o.rect(x, y, pw, ph, [0.04, 0.03, 0.02, 0.78]);
        o.rect(x, y, pw, 4.0 * ui, ACCENT);
        let title = race.finished.map_or_else(
            || "RESULTS".to_string(),
            |p| format!("YOU FINISHED {}", ordinal(p as usize)),
        );
        text(
            o,
            x + pw * 0.5 - width(&title, 4.0 * ui) * 0.5,
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

/// The circuit standings beside the race results.
fn draw_points(o: &mut Overlay, ui: f32, (x, y): (f32, f32), pw: f32, c: &CircuitHud) {
    let ph = (90.0 + 40.0 * c.table.len() as f32) * ui;
    o.rect(x, y, pw, ph, [0.04, 0.03, 0.02, 0.78]);
    o.rect(x, y, pw, 4.0 * ui, MINT);
    let title = if c.round >= c.rounds {
        c.table
            .first()
            .map_or_else(|| "CIRCUIT".into(), |l| format!("CHAMPION  {}", l.0))
    } else {
        format!("CIRCUIT  AFTER ROUND {} / {}", c.round, c.rounds)
    };
    let ts = (4.0 * ui).min((pw - 40.0 * ui) / width(&title, 1.0).max(1.0));
    text(
        o,
        x + pw * 0.5 - width(&title, ts) * 0.5,
        y + 22.0 * ui,
        ts,
        MINT,
        &title,
    );
    for (k, (name, tint, gained, total)) in c.table.iter().enumerate() {
        let ry = y + (80.0 + 40.0 * k as f32) * ui;
        let col = [tint[0].min(1.0), tint[1].min(1.0), tint[2].min(1.0), 1.0];
        o.rect(x + 30.0 * ui, ry, 14.0 * ui, 22.0 * ui, col);
        text(
            o,
            x + 60.0 * ui,
            ry,
            3.0 * ui,
            WHITE,
            &format!("{}  {name}", ordinal(k + 1)),
        );
        let pts = format!("+{gained}  {total} PTS");
        text(
            o,
            x + pw - 30.0 * ui - width(&pts, 3.0 * ui),
            ry,
            3.0 * ui,
            if k == 0 { ACCENT } else { DIM },
            &pts,
        );
    }
}

/// A centered menu panel with an optional big title (the game logo on the title screen).
/// Elimination: racers left, a pulsing warning in last place, and the knockout callout.
pub fn draw_elimination(
    o: &mut Overlay,
    w: f32,
    h: f32,
    left: usize,
    danger: bool,
    knockout: Option<(&str, f32, bool)>,
    t: f32,
) {
    let ui = (h / 900.0).clamp(0.6, 2.0);
    let tag = format!("ELIMINATION  {left} LEFT");
    let ts = 1.6 * ui;
    let tw = width(&tag, ts);
    o.rect(
        w * 0.5 - tw * 0.5 - 14.0 * ui,
        118.0 * ui,
        tw + 28.0 * ui,
        30.0 * ui,
        PANEL,
    );
    o.label(w * 0.5 - tw * 0.5, 123.0 * ui, ts, ACCENT, &tag);
    if danger {
        let pulse = 0.55 + 0.45 * (t * 7.0).sin().abs();
        let msg = "LAST PLACE - YOU'RE NEXT";
        let ms = 2.4 * ui;
        let mw = width(msg, ms);
        o.rect(
            w * 0.5 - mw * 0.5 - 18.0 * ui,
            h * 0.22,
            mw + 36.0 * ui,
            44.0 * ui,
            [0.55, 0.03, 0.02, 0.6 * pulse],
        );
        o.label(
            w * 0.5 - mw * 0.5,
            h * 0.22 + 6.0 * ui,
            ms,
            [1.0, 0.85, 0.8, pulse],
            msg,
        );
    }
    if let Some((name, age, is_you)) = knockout
        && age < 2.8
    {
        let fade = (1.0 - (age - 2.2).max(0.0) / 0.6).clamp(0.0, 1.0);
        let pop = 1.0 + 0.25 * (1.0 - (age / 0.25).min(1.0));
        let msg = if is_you {
            "YOU'RE OUT!".to_string()
        } else {
            format!("{name} IS OUT!")
        };
        let ms = 4.0 * ui * pop;
        let mw = width(&msg, ms);
        let y = h * 0.3;
        o.label(
            w * 0.5 - mw * 0.5 + 4.0 * ui,
            y + 4.0 * ui,
            ms,
            [0.0, 0.0, 0.0, 0.6 * fade],
            &msg,
        );
        o.label(w * 0.5 - mw * 0.5, y, ms, [1.0, 0.3, 0.12, fade], &msg);
    }
}

/// A pulsing WRONG WAY warning, upper middle of the screen.
pub fn draw_wrong_way(o: &mut Overlay, w: f32, h: f32, t: f32) {
    let ui = (h / 900.0).clamp(0.6, 2.0);
    let pulse = 0.6 + 0.4 * (t * 6.0).sin().abs();
    let msg = "WRONG WAY";
    let s = 4.4 * ui;
    let mw = width(msg, s);
    let y = h * 0.2;
    o.rect(
        w * 0.5 - mw * 0.5 - 30.0 * ui,
        y - 12.0 * ui,
        mw + 60.0 * ui,
        96.0 * ui,
        [0.6, 0.02, 0.02, 0.55 * pulse],
    );
    o.rect(
        w * 0.5 - mw * 0.5 - 30.0 * ui,
        y - 12.0 * ui,
        mw + 60.0 * ui,
        5.0 * ui,
        [1.0, 0.3, 0.2, pulse],
    );
    o.label(w * 0.5 - mw * 0.5, y, s, [1.0, 0.92, 0.88, pulse], msg);
    let sub = "TURN AROUND";
    let ss = 1.6 * ui;
    o.label(
        w * 0.5 - width(sub, ss) * 0.5,
        y + 58.0 * ui,
        ss,
        [1.0, 0.8, 0.75, pulse],
        sub,
    );
}

const INK: [f32; 4] = [0.015, 0.029, 0.042, 0.95];
const MINT: [f32; 4] = [0.40, 0.95, 0.80, 1.0];

fn menu_scale(w: f32, h: f32) -> f32 {
    (w / 1270.0).min(h / 800.0).min(2.5)
}

fn fit_label(
    o: &mut Overlay,
    x: f32,
    cy: f32,
    size: f32,
    available: f32,
    color: [f32; 4],
    value: &str,
) {
    let size = size.min(available / width(value, 1.0).max(1.0));
    o.label_centered(x, cy, size, color, value);
}

/// Network details and the eight starting positions share the right-hand side of the paddock.
pub fn draw_lobby(
    o: &mut Overlay,
    w: f32,
    h: f32,
    heading: &str,
    lines: &[String],
    names: &[String],
) {
    let u = menu_scale(w, h);
    let grid = heading != "ONLINE";
    let pw = 530. * u;
    let x = w - pw - 44. * u;
    let y = h * 0.29;
    let slots = crate::netplay::MAX_PEERS + 1;
    let height = (64. + lines.len() as f32 * 30. + if grid { 232. } else { 8. }) * u;
    o.rect(x, y, pw, height, INK);
    o.rect(x, y, pw, 3. * u, MINT);
    o.label_centered(x + 20. * u, y + 28. * u, 2.4 * u, WHITE, heading);
    if grid {
        o.label_centered(
            x + pw - 145. * u,
            y + 28. * u,
            1.8 * u,
            MINT,
            &format!("{} / {} DRIVERS", names.len(), slots),
        );
    }
    let mut cy = y + 65. * u;
    for line in lines {
        fit_label(o, x + 20. * u, cy, 1.85 * u, pw - 40. * u, DIM, line);
        cy += 30. * u;
    }
    if grid {
        for i in 0..slots {
            let sx = x + (16. + (i % 2) as f32 * 250.) * u;
            let sy = cy + (i / 2) as f32 * 52. * u;
            let active = names.get(i);
            o.rect(sx, sy, 240. * u, 43. * u, [0.09, 0.14, 0.16, 0.95]);
            o.rect(
                sx,
                sy,
                3. * u,
                43. * u,
                if active.is_some() { MINT } else { DIM },
            );
            o.label_centered(
                sx + 10. * u,
                sy + 22. * u,
                1.6 * u,
                MINT,
                &format!("{:02}", i + 1),
            );
            let name = active.map(String::as_str).unwrap_or("OPEN SEAT");
            fit_label(
                o,
                sx + 40. * u,
                sy + 22. * u,
                1.85 * u,
                if i == 0 { 138. } else { 187. } * u,
                if active.is_some() { WHITE } else { DIM },
                name,
            );
            if i == 0 && active.is_some() {
                o.label_centered(sx + 184. * u, sy + 22. * u, 1.3 * u, ACCENT, "HOST");
            }
        }
    }
}

/// Seat numbers and colors stay with the driver as the running order changes.
fn seat_color(seat: usize) -> [f32; 4] {
    let [r, g, b] = crate::race::TINTS[seat % crate::race::TINTS.len()];
    [r * 0.76, g * 0.76, b * 0.76, 1.]
}

pub fn driver_tag(o: &mut Overlay, (w, h): (f32, f32), clip: glam::Vec4, seat: usize, name: &str) {
    if clip.w <= 0. || clip.z < 0. || clip.z > clip.w || clip.w > 100. {
        return;
    }
    let p = clip.truncate() / clip.w;
    if p.x.abs() > 1. || p.y.abs() > 1. {
        return;
    }
    let u = menu_scale(w, h) * (1.2 - clip.w / 200.).clamp(0.75, 1.);
    let x = (p.x + 1.) * 0.5 * w;
    let y = (1. - p.y) * 0.5 * h;
    let color = seat_color(seat);
    let name_size = 2.05 * u;
    let badge = 33. * u;
    let tw = width(name, name_size) + badge + 18. * u;
    let left = x - tw / 2.;
    o.rect(left, y - 43. * u, tw, 31. * u, INK);
    o.rect(left, y - 43. * u, badge, 31. * u, color);
    o.rect(left + badge, y - 43. * u, tw - badge, 2. * u, color);
    o.label_centered(
        left + 5. * u,
        y - 27. * u,
        2.0 * u,
        INK,
        &format!("{:02}", seat + 1),
    );
    o.label_centered(left + badge + 8. * u, y - 27. * u, name_size, WHITE, name);
    o.triangle(
        [[x - 9. * u, y - 12. * u], [x + 9. * u, y - 12. * u], [x, y]],
        color,
    );
}

pub fn human_row(
    o: &mut Overlay,
    h: f32,
    row: usize,
    position: usize,
    seat: usize,
    kart: &crate::race::Racer,
    me: bool,
) {
    let u = (h / 900.).clamp(0.6, 2.);
    let x = 22. * u;
    let y = (154. + row as f32 * 30.) * u;
    if row == 0 {
        o.rect(x, y - 28. * u, 210. * u, 28. * u, INK);
        o.label_centered(x + 8. * u, y - 14. * u, 1.5 * u, MINT, "HUMAN DRIVERS");
    }
    o.rect(x, y, 210. * u, 28. * u, INK);
    o.rect(x, y, 3. * u, 28. * u, seat_color(seat));
    o.label_centered(
        x + 8. * u,
        y + 14. * u,
        1.6 * u,
        WHITE,
        &format!("P{position}"),
    );
    let label = format!("{:02}  {}", seat + 1, kart.name);
    fit_label(
        o,
        x + 42. * u,
        y + 14. * u,
        1.65 * u,
        123. * u,
        seat_color(seat),
        &label,
    );
    let tag = if kart.out {
        "OUT"
    } else if kart.finish.is_some() {
        "FIN"
    } else if me {
        "YOU"
    } else {
        ""
    };
    o.label_centered(x + 173. * u, y + 14. * u, 1.3 * u, WHITE, tag);
}

pub fn controls_card(o: &mut Overlay, w: f32, h: f32, pad: bool) {
    let u = menu_scale(w, h);
    let pw = 520. * u;
    let x = w - pw - 44. * u;
    let y = h * 0.28;
    o.rect(x, y, pw, 386. * u, INK);
    o.rect(x, y, pw, 3. * u, MINT);
    o.label_centered(x + 20. * u, y + 34. * u, 2.8 * u, WHITE, "DRIVING CONTROLS");
    for (i, (action, key, button)) in [
        ("Accelerate", "W / Up", "RT / A"),
        ("Brake / reverse", "S / Down", "LT"),
        ("Steer", "A D / Left Right", "Left stick"),
        ("Hop / drift", "Space / Shift", "LB / RB"),
        ("Fire item", "E / Left Ctrl / J", "X"),
        ("Recover kart", "R", "B"),
        ("Pause", "Esc", "Start"),
        ("FPS display", "F3", "Select"),
    ]
    .iter()
    .enumerate()
    {
        let cy = y + (80. + i as f32 * 40.) * u;
        o.rect(
            x + 20. * u,
            cy + 18. * u,
            pw - 40. * u,
            u,
            [0.4, 0.65, 0.65, 0.28],
        );
        o.label_centered(x + 20. * u, cy, 2.15 * u, WHITE, action);
        let val = if pad { button } else { key };
        o.label_centered(
            x + pw - 20. * u - width(val, 2.1 * u),
            cy,
            2.1 * u,
            MINT,
            val,
        );
    }
}

fn swatch_rect(w: f32, h: f32, index: usize) -> (f32, f32, f32, f32) {
    let u = menu_scale(w, h);
    (
        w - (413. - 44. * index as f32) * u,
        h - 101. * u,
        37. * u,
        25. * u,
    )
}

pub fn garage_swatch_at(w: f32, h: f32, x: f32, y: f32) -> Option<usize> {
    (0..crate::kart::FINISHES.len()).find(|&i| {
        let (sx, sy, sw, sh) = swatch_rect(w, h, i);
        (sx..sx + sw).contains(&x) && (sy..sy + sh).contains(&y)
    })
}

pub fn garage_card(o: &mut Overlay, w: f32, h: f32, loadout: crate::kart::Loadout) {
    let u = menu_scale(w, h);
    let pw = 385. * u;
    let x = w - pw - 44. * u;
    let y = h - 153. * u;
    o.rect(x, y, pw, 92. * u, INK);
    let name = crate::kart::KARTS[loadout.design].name.to_ascii_uppercase();
    o.label_centered(x + 16. * u, y + 27. * u, 2.8 * u, WHITE, &name);
    let finish = crate::kart::FINISHES[loadout.finish];
    o.label_centered(
        x + pw - 16. * u - width(finish, 1.8 * u),
        y + 27. * u,
        1.8 * u,
        MINT,
        finish,
    );
    for (i, &[r, g, b]) in crate::race::TINTS.iter().enumerate() {
        let (sx, sy, sw, _) = swatch_rect(w, h, i);
        o.rect(sx, sy, sw, 17. * u, [r * 0.8, g * 0.8, b * 0.8, 1.]);
        if i == loadout.finish {
            o.rect(sx, y + 73. * u, 37. * u, 4. * u, WHITE);
        }
    }
}

/// A short network notice (joined, left, connection lost), fading out after a few seconds.
pub fn draw_toast(o: &mut Overlay, w: f32, h: f32, msg: &str, age: f32) {
    if age > 4.0 {
        return;
    }
    let ui = (h / 900.0).clamp(0.6, 2.0);
    let a = (1.0 - (age - 3.4).max(0.0) / 0.6).clamp(0.0, 1.0);
    let s = 1.8 * ui;
    let tw = width(msg, s);
    let (x, y) = (w * 0.5 - tw * 0.5, h - 110.0 * ui);
    o.rect(
        x - 18.0 * ui,
        y - 8.0 * ui,
        tw + 36.0 * ui,
        40.0 * ui,
        [0.05, 0.04, 0.03, 0.7 * a],
    );
    o.rect(
        x - 18.0 * ui,
        y - 8.0 * ui,
        4.0 * ui,
        40.0 * ui,
        [ACCENT[0], ACCENT[1], ACCENT[2], a],
    );
    o.label(x, y, s, [1.0, 1.0, 1.0, a], msg);
}

/// A live plan view of the highlighted circuit.
pub fn draw_track_card(
    o: &mut Overlay,
    w: f32,
    h: f32,
    index: usize,
    def: &crate::track::TrackDef,
    route: &[glam::Vec3],
    time: f32,
) {
    let (name, tagline) = (def.name, def.tagline);
    let count = crate::track::TRACKS.len();
    let ui = menu_scale(w, h);
    let ts = 1.7 * ui;
    let cw = 520. * ui;
    let (x, y) = (w - cw - 44. * ui, h * 0.30);
    o.rect(x, y, cw, 318. * ui, INK);
    o.rect(x, y, cw, 3. * ui, MINT);
    for k in 1..10 {
        o.rect(
            x + cw * k as f32 / 10.,
            y + 123. * ui,
            ui,
            174. * ui,
            [0.4, 0.65, 0.65, 0.12],
        );
    }
    for k in 0..7 {
        o.rect(
            x + 18. * ui,
            y + (123. + 28. * k as f32) * ui,
            cw - 36. * ui,
            ui,
            [0.4, 0.65, 0.65, 0.12],
        );
    }
    let map = Minimap::fit(
        route,
        (x + 24.0 * ui, y + 128.0 * ui, cw - 48.0 * ui, 161.0 * ui),
    );
    for point in route {
        let (px, py) = map.project(*point);
        o.rect(px, py, 3.0 * ui, 3.0 * ui, WHITE);
    }
    if let Some(start) = route.get((time * 22.) as usize % route.len().max(1)) {
        let (px, py) = map.project(*start);
        o.rect(px - 2.0 * ui, py - 2.0 * ui, 7.0 * ui, 7.0 * ui, ACCENT);
    }
    let head = format!("TRACK {} / {}", index + 1, count);
    o.label(x + 24.0 * ui, y + 14.0 * ui, 1.5 * ui, ACCENT, &head);
    o.label(x + 24.0 * ui, y + 38.0 * ui, 3.4 * ui, WHITE, name);
    fit_label(
        o,
        x + 24. * ui,
        y + 99. * ui,
        ts,
        cw - 48. * ui,
        DIM,
        tagline,
    );
}

/// Splits a menu row like `"TRACK  < CASTLE GRINDSTONE >"` into its label, value, and
/// whether the value is a cycling option (shown between arrows).
fn split_row(item: &str) -> (&str, Option<&str>, bool) {
    match item.split_once("  ") {
        Some((label, value)) => {
            let value = value.trim();
            let option = value.starts_with('<') && value.ends_with('>');
            let v = value.trim_start_matches('<').trim_end_matches('>').trim();
            (label.trim(), Some(v), option)
        }
        None => (item.trim(), None, false),
    }
}

pub struct MenuRows {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    first: usize,
    previous: Vec<Option<std::ops::Range<f32>>>,
}

impl MenuRows {
    pub fn previous_at(&self, row: usize, x: f32) -> bool {
        self.previous
            .get(row.wrapping_sub(self.first))
            .and_then(Option::as_ref)
            .is_some_and(|range| range.contains(&x))
    }
    pub fn hit(&self, x: f32, y: f32) -> Option<usize> {
        let index = ((y - self.y) / self.height).floor();
        (x >= self.x
            && x < self.x + self.width
            && index >= 0.0
            && index < self.previous.len() as f32)
            .then_some(self.first + index.max(0.) as usize)
    }
}

/// The navigation rail shares its row window with keyboard focus and pointer hit testing.
pub fn draw_menu(
    o: &mut Overlay,
    w: f32,
    h: f32,
    menu: &crate::menu::MenuState,
    items: &[String],
    time: f32,
) -> MenuRows {
    use crate::menu::Screen;
    let u = menu_scale(w, h);
    let (heading, section, help) = match menu.screen {
        Screen::Title => (
            "SCRAPBURN",
            "MAIN MENU",
            [
                "Pick a track or run a circuit.",
                "Host a race or join friends.",
                "Choose your chassis and paint.",
                "Keyboard and gamepad bindings.",
                "Graphics and race preferences.",
                "Exit to desktop.",
            ][menu.selected.min(5)],
        ),
        Screen::RaceSetup => (
            "Race setup",
            "SOLO RACE",
            if menu.selected == 2 {
                match menu.solo_event() {
                    crate::circuit::Event::Single => "One race on the track of your choice.",
                    crate::circuit::Event::Circuit => {
                        "Race every track in order. Points decide the champion."
                    }
                }
            } else {
                menu.mode.description()
            },
        ),
        Screen::Circuits => (
            "Tracks",
            "TRACK SELECT",
            "Browse the routes. Select a track to equip it.",
        ),
        Screen::Tailscale => (
            "Tailscale",
            "MULTIPLAYER / TAILNET",
            if menu.peers.is_empty() {
                "No Tailscale peers online yet. The list refreshes every few seconds."
            } else {
                "Pick a device on your tailnet to join its race."
            },
        ),
        Screen::Garage => ("Garage", "MOTOR POOL", "Choose a chassis and paint finish."),
        Screen::Online => (
            "Multiplayer",
            "RACE NETWORK",
            "Create a private invite or join a host.",
        ),
        Screen::DriverName => (
            "Racer name",
            "MULTIPLAYER / PROFILE",
            "Type your new name, then press Enter to save.",
        ),
        Screen::Lobby => (
            "Race lobby",
            "MULTIPLAYER",
            "Gather your drivers on the starting grid.",
        ),
        Screen::Controls => (
            "Controls",
            "DRIVER GUIDE",
            "Switch devices to see the racing controls.",
        ),
        Screen::Settings => (
            "Settings",
            "PREFERENCES",
            "Use the arrows to adjust an option.",
        ),
        Screen::Paused => ("Paused", "RACE MENU", ""),
        Screen::Results | Screen::Playing => ("", "", ""),
    };
    let results = menu.screen == Screen::Results;
    let x = 38. * u;
    let pw = 535. * u;
    let row = 53. * u;
    let top = if results { h - 166. * u } else { h * 0.32 };
    let visible = (((h - 65. * u - top) / row).floor() as usize)
        .max(1)
        .min(items.len());
    let first = (menu.selected + 1)
        .saturating_sub(visible)
        .min(items.len().saturating_sub(visible));
    if !results {
        let rail = pw + 76. * u;
        o.rect(0., 0., rail, h, [0.016, 0.03, 0.044, 0.90]);
        o.triangle(
            [[rail, 0.], [rail + 62. * u, 0.], [rail, h]],
            [0.016, 0.03, 0.044, 0.90],
        );
        let cy = top - 184. * u;
        for i in 0..3 {
            let sx = x + i as f32 * 12. * u;
            o.triangle(
                [
                    [sx, cy + 6. * u],
                    [sx + 9. * u, cy - 7. * u],
                    [sx + 15. * u, cy - 7. * u],
                ],
                ACCENT,
            );
        }
        o.label_centered(x + 52. * u, cy, 1.85 * u, MINT, section);
        fit_label(o, x, top - 109. * u, 8.5 * u, pw, WHITE, heading);
        fit_label(o, x, top - 36. * u, 1.95 * u, pw, DIM, help);
    }
    let mut previous = vec![None; visible];
    for (j, item) in items.iter().skip(first).take(visible).enumerate() {
        let (label, value, option) = split_row(item);
        let sel = j + first == menu.selected;
        let y = top + j as f32 * row;
        let cy = y + row * 0.47;
        let active = [1., 0.63, 0.18, 1.];
        let fg = if sel {
            [0.035, 0.043, 0.048, 1.]
        } else {
            WHITE
        };
        o.rect(
            x,
            y,
            pw,
            row - 4. * u,
            if sel {
                active
            } else {
                [0.09, 0.135, 0.155, 0.96]
            },
        );
        if sel {
            let offset = (time * 85.).rem_euclid(95.);
            for i in 0..3 {
                let px = x + pw - (offset + i as f32 * 29.) * u;
                o.triangle(
                    [[px, y], [px + 10. * u, y], [px - 16. * u, y + row - 4. * u]],
                    [0.85, 0.35, 0.07, 0.21],
                );
            }
        }
        let label_space = if value.is_some() {
            pw * 0.46
        } else {
            pw - 64. * u
        };
        fit_label(o, x + 18. * u, cy, 2.7 * u, label_space, fg, label);
        if let Some(value) = value {
            let left = x + pw * 0.51;
            let right = x + pw - 14. * u;
            let arrow = if option { 30. * u } else { 0. };
            let val_s = (2.35 * u).min((right - left - 2. * arrow) / width(value, 1.).max(1.));
            let vx = right - arrow - width(value, val_s);
            o.label_centered(vx, cy, val_s, fg, value);
            if option {
                previous[j] = Some(vx - arrow..vx);
                o.label_centered(vx - arrow + 4. * u, cy, 2.4 * u, fg, "<");
                o.label_centered(right - 16. * u, cy, 2.4 * u, fg, ">");
            }
        } else if sel {
            o.label_centered(x + pw - 34. * u, cy, 2.5 * u, fg, ">");
        }
    }
    o.rect(0., h - 43. * u, w, 43. * u, INK);
    o.label_centered(
        x,
        h - 22. * u,
        1.65 * u,
        MINT,
        "ENTER / A   SELECT      ESC / B   BACK",
    );
    if items.len() > visible {
        let page = format!("{} / {}", menu.selected + 1, items.len());
        o.label_centered(
            x + pw - width(&page, 1.65 * u),
            h - 22. * u,
            1.65 * u,
            WHITE,
            &page,
        );
    }
    MenuRows {
        x,
        y: top,
        width: pw,
        height: row,
        first,
        previous,
    }
}

pub fn dim(o: &mut Overlay, w: f32, h: f32) {
    o.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.45]);
}

/// The held-item slot, top right.
pub fn draw_item_slot(
    o: &mut Overlay,
    w: f32,
    h: f32,
    item: Option<crate::combat::ItemKind>,
    time: f32,
) {
    use crate::combat::ItemKind;
    let ui = (h / 900.0).clamp(0.6, 2.0);
    let size = 92.0 * ui;
    let (x, y) = (w - size - 26.0 * ui, 22.0 * ui);
    o.rect(
        x - 6.0 * ui,
        y - 6.0 * ui,
        size + 12.0 * ui,
        size + 12.0 * ui,
        PANEL,
    );
    o.rect(
        x - 6.0 * ui,
        y - 6.0 * ui,
        size + 12.0 * ui,
        3.0 * ui,
        ACCENT,
    );
    let Some(item) = item else {
        text(
            o,
            x + size * 0.5 - width("--", 3.0 * ui) * 0.5,
            y + size * 0.4,
            3.0 * ui,
            DIM,
            "--",
        );
        return;
    };
    let color = match item {
        ItemKind::Nailer => [1.0, 0.65, 0.25, 1.0],
        ItemKind::Harpoon => [0.25, 0.86, 1.0, 1.0],
        ItemKind::PlasmaOrb => [0.50, 1.0, 0.35, 1.0],
    };
    let pulse = 0.85 + 0.15 * (time * 6.0).sin();
    o.rect(
        x + 6.0 * ui,
        y + 6.0 * ui,
        size - 12.0 * ui,
        size - 12.0 * ui,
        [color[0] * 0.25, color[1] * 0.25, color[2] * 0.25, 0.8],
    );
    let c = [color[0] * pulse, color[1] * pulse, color[2] * pulse, 1.0];
    let mut shard = |points: [[f32; 2]; 3], color| {
        o.triangle(points.map(|[u, v]| [x + u * size, y + v * size]), color);
    };
    match item {
        ItemKind::Nailer => {
            for shift in [0.0, 0.13, 0.26] {
                shard(
                    [
                        [0.16 + shift, 0.77],
                        [0.29 + shift, 0.46],
                        [0.50 + shift, 0.22],
                    ],
                    c,
                );
                shard(
                    [
                        [0.29 + shift, 0.46],
                        [0.36 + shift, 0.51],
                        [0.50 + shift, 0.22],
                    ],
                    WHITE,
                );
            }
        }
        ItemKind::Harpoon => {
            shard([[0.2, 0.79], [0.28, 0.80], [0.74, 0.25]], c);
            shard([[0.34, 0.43], [0.77, 0.22], [0.67, 0.65]], c);
            shard([[0.46, 0.46], [0.77, 0.22], [0.58, 0.49]], WHITE);
        }
        ItemKind::PlasmaOrb => {
            shard([[0.18, 0.50], [0.50, 0.18], [0.80, 0.50]], c);
            shard([[0.18, 0.50], [0.80, 0.50], [0.50, 0.82]], c);
            shard([[0.35, 0.50], [0.52, 0.31], [0.68, 0.50]], WHITE);
        }
    }
    let name = item.name();
    text(
        o,
        x + size * 0.5 - width(name, 1.6 * ui) * 0.5,
        y + size + 14.0 * ui,
        1.6 * ui,
        color,
        name,
    );
    let hint = "E / X";
    text(
        o,
        x + size * 0.5 - width(hint, 1.4 * ui) * 0.5,
        y + size + 34.0 * ui,
        1.4 * ui,
        DIM,
        hint,
    );
}

/// A compact frame-rate readout, bottom center (clear of the race HUD).
pub fn draw_fps_counter(o: &mut Overlay, w: f32, h: f32, fps: f32) {
    let ui = (h / 900.0).clamp(0.6, 2.0);
    let label = format!("{fps:.0} FPS");
    let color = if fps >= 59.0 {
        [0.45, 1.0, 0.5, 1.0]
    } else if fps >= 30.0 {
        ACCENT
    } else {
        [1.0, 0.3, 0.2, 1.0]
    };
    let s = 2.0 * ui;
    let x = w * 0.5 - width(&label, s) * 0.5;
    o.rect(
        x - 8.0 * ui,
        6.0 * ui + 112.0 * ui,
        width(&label, s) + 16.0 * ui,
        26.0 * ui,
        PANEL,
    );
    text(o, x, 12.0 * ui + 112.0 * ui, s, color, &label);
}

#[cfg(test)]
mod menu_row_tests {
    use super::split_row;

    #[test]
    fn rows_split_into_label_value_and_option() {
        assert_eq!(
            split_row("TRACK  < CASTLE GRINDSTONE >"),
            ("TRACK", Some("CASTLE GRINDSTONE"), true)
        );
        assert_eq!(
            split_row("JOIN  100.64.0.2_"),
            ("JOIN", Some("100.64.0.2_"), false)
        );
        assert_eq!(split_row("RACE"), ("RACE", None, false));
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
