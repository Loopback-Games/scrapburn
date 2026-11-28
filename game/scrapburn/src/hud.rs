//! HUD layout helpers (pure) and drawing.

use rivet_physics::{BoostTier, VehicleParams, VehicleState};
use rivet_render::Overlay;

use crate::prefs::{Hud as HudPrefs, Units};
use crate::style::tier_color;

/// The race HUD's size factor for a `h`-pixel-tall window and a HUD scale of `percent`
/// (the 70-130 setting; out-of-range values clamp).
pub fn ui_scale(h: f32, percent: u8) -> f32 {
    (h / 900.0).clamp(0.6, 2.0) * f32::from(percent.clamp(70, 130)) / 100.0
}

/// A speed in m/s as the number and unit label the speedometer shows.
pub fn speed_readout(mps: f32, units: Units) -> (i32, &'static str) {
    match units {
        Units::Kmh => ((mps * 3.6).round() as i32, "KM/H"),
        Units::Mph => ((mps * 2.23694).round() as i32, "MPH"),
    }
}

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
    opts: &HudPrefs,
) {
    let ui = ui_scale(h, opts.scale);

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
    if opts.speedometer {
        draw_speedometer(o, w, h, ui, kart, params, opts.units);
    }
    draw_drift(o, w, h, ui, kart, params);
}

fn draw_speedometer(
    o: &mut Overlay,
    w: f32,
    h: f32,
    ui: f32,
    kart: &VehicleState,
    params: &VehicleParams,
    units: Units,
) {
    let (shown, label) = speed_readout(kart.speed().abs(), units);
    let num = format!("{shown}");
    let ns = 6.0 * ui;
    let right = w - 28.0 * ui;
    let base = h - 34.0 * ui;
    let unit_w = width(label, 2.0 * ui);
    text(o, right - unit_w, base - 24.0 * ui, 2.0 * ui, DIM, label);
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
}

fn draw_drift(
    o: &mut Overlay,
    w: f32,
    h: f32,
    ui: f32,
    kart: &VehicleState,
    params: &VehicleParams,
) {
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

/// Circuit progress for the race HUD.
pub struct CircuitHud {
    /// 1-based round.
    pub round: usize,
    pub rounds: usize,
}

/// Race-level HUD data.
pub struct RaceHud<'a> {
    pub position: usize,
    pub racers: usize,
    /// Seconds before GO (negative after), while the label should show.
    pub countdown: Option<f32>,
    /// Round, when the race is part of a circuit.
    pub circuit: Option<CircuitHud>,
    pub track: &'a [glam::Vec3],
    /// (position, tint, is the player)
    pub karts: Vec<(glam::Vec3, [f32; 3], bool)>,
}

/// Position, minimap, and countdown.
pub fn draw_race(o: &mut Overlay, w: f32, h: f32, race: &RaceHud, opts: &HudPrefs) {
    let ui = ui_scale(h, opts.scale);
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
    if opts.minimap {
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
}

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
    (row, position): (usize, usize),
    seat: usize,
    kart: &crate::race::Racer,
    me: bool,
    scale: u8,
) {
    let u = ui_scale(h, scale);
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

/// A gamepad control, drawn as an Xbox-style button glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadGlyph {
    A,
    B,
    X,
    Y,
    LeftBumper,
    RightBumper,
    LeftTrigger,
    RightTrigger,
    LeftStick,
    RightStick,
    /// The small left center button (two overlapping windows).
    View,
    /// The small right center button (three lines).
    Menu,
    DPad,
}

/// Face button fills: A green, B red, X blue, Y yellow.
const PAD_A: [f32; 4] = [0.365, 0.761, 0.118, 1.];
const PAD_B: [f32; 4] = [0.910, 0.067, 0.137, 1.];
/// A touch deeper than Xbox's #0078D7 so a white letter keeps 4.5:1 contrast.
const PAD_X: [f32; 4] = [0., 0.447, 0.808, 1.];
const PAD_Y: [f32; 4] = [1., 0.725, 0., 1.];
/// Letters on the light face buttons (A, Y).
const PAD_DARK_LETTER: [f32; 4] = [0.05, 0.06, 0.07, 1.];
/// Bumpers, triggers, sticks, the center buttons and the D-pad.
const PAD_BODY: [f32; 4] = [0.19, 0.22, 0.24, 1.];
const PAD_RIM: [f32; 4] = [0.46, 0.52, 0.55, 1.];

impl PadGlyph {
    /// The letter and colors of a face button, or `None` for the neutral controls.
    fn face(self) -> Option<(&'static str, [f32; 4], [f32; 4])> {
        match self {
            Self::A => Some(("A", PAD_A, PAD_DARK_LETTER)),
            Self::B => Some(("B", PAD_B, WHITE)),
            Self::X => Some(("X", PAD_X, WHITE)),
            Self::Y => Some(("Y", PAD_Y, PAD_DARK_LETTER)),
            _ => None,
        }
    }

    /// The name written beside a center button, which carries an icon rather than text.
    fn caption(self) -> Option<&'static str> {
        match self {
            Self::View => Some("View"),
            Self::Menu => Some("Menu"),
            _ => None,
        }
    }

    /// Glyph width for a button `s` pixels across (the face button diameter).
    pub fn width(self, s: f32) -> f32 {
        match self {
            Self::LeftBumper | Self::RightBumper => 1.7 * s,
            Self::LeftTrigger | Self::RightTrigger => 1.35 * s,
            _ => match self.caption() {
                Some(c) => s * 1.3 + width(c, s * 0.08),
                None => s,
            },
        }
    }

    /// Draws the glyph from left edge `x`, vertically centered on `cy`; `s` as in `width`.
    pub fn draw(self, o: &mut Overlay, x: f32, cy: f32, s: f32) {
        let r = 0.5 * s;
        let rim = (0.07 * s).max(1.);
        let centered = |o: &mut Overlay, cx: f32, scale: f32, color, t: &str| {
            o.label_centered(cx - 0.5 * width(t, scale), cy, scale, color, t);
        };
        // A neutral body: a light rim with a dark fill inset.
        let body = |o: &mut Overlay, w: f32, h: f32, radius: f32| {
            let y = cy - 0.5 * h;
            o.rounded_rect(x, y, w, h, radius, PAD_RIM);
            o.rounded_rect(
                x + rim,
                y + rim,
                w - 2. * rim,
                h - 2. * rim,
                radius - rim,
                PAD_BODY,
            );
        };
        if let Some((letter, fill, ink)) = self.face() {
            o.circle(x + r, cy, r, fill);
            centered(o, x + r, s * 0.068, ink, letter);
            return;
        }
        match self {
            Self::LeftBumper | Self::RightBumper => {
                let w = self.width(s);
                body(o, w, 0.72 * s, 0.24 * s);
                let t = if self == Self::LeftBumper { "LB" } else { "RB" };
                centered(o, x + 0.5 * w, s * 0.056, WHITE, t);
            }
            Self::LeftTrigger | Self::RightTrigger => {
                let w = self.width(s);
                body(o, w, s, 0.42 * s);
                let t = if self == Self::LeftTrigger {
                    "LT"
                } else {
                    "RT"
                };
                centered(o, x + 0.5 * w, s * 0.056, WHITE, t);
            }
            Self::LeftStick | Self::RightStick => {
                // A raised cap inside the stick's well.
                o.circle(x + r, cy, r, PAD_RIM);
                o.circle(x + r, cy, r - rim, PAD_BODY);
                o.circle(x + r, cy, 0.72 * r, [0.27, 0.31, 0.33, 1.]);
                let t = if self == Self::LeftStick { "LS" } else { "RS" };
                centered(o, x + r, s * 0.05, WHITE, t);
            }
            Self::View | Self::Menu => {
                o.circle(x + r, cy, r, PAD_RIM);
                o.circle(x + r, cy, r - rim, PAD_BODY);
                let t = (0.06 * s).max(1.);
                if self == Self::View {
                    // Two overlapping windows; the front one hides the back one's corner.
                    let side = 0.34 * s;
                    let back = (x + r - 0.36 * side, cy - 0.64 * side);
                    let front = (x + r - 0.64 * side, cy - 0.36 * side);
                    outline(o, back.0, back.1, side, side, t, WHITE);
                    o.rect(front.0, front.1, side, side, PAD_BODY);
                    outline(o, front.0, front.1, side, side, t, WHITE);
                } else {
                    let lw = 0.4 * s;
                    for i in -1..=1 {
                        let ly = cy + i as f32 * 0.13 * s - 0.5 * t;
                        o.rect(x + r - 0.5 * lw, ly, lw, t, WHITE);
                    }
                }
                if let Some(c) = self.caption() {
                    o.label_centered(x + 1.3 * s, cy, s * 0.08, MINT, c);
                }
            }
            Self::DPad => {
                let arm = 0.36 * s;
                o.rounded_rect(x, cy - 0.5 * arm, s, arm, 0.1 * s, PAD_RIM);
                o.rounded_rect(x + r - 0.5 * arm, cy - r, arm, s, 0.1 * s, PAD_RIM);
                let a = arm - 2. * rim;
                o.rect(x + rim, cy - 0.5 * a, s - 2. * rim, a, PAD_BODY);
                o.rect(x + r - 0.5 * a, cy - r + rim, a, s - 2. * rim, PAD_BODY);
            }
            // Drawn above from `face`.
            Self::A | Self::B | Self::X | Self::Y => {}
        }
    }
}

/// A rectangle border `t` pixels thick.
fn outline(o: &mut Overlay, x: f32, y: f32, w: f32, h: f32, t: f32, color: [f32; 4]) {
    o.rect(x, y, w, t, color);
    o.rect(x, y + h - t, w, t, color);
    o.rect(x, y + t, t, h - 2. * t, color);
    o.rect(x + w - t, y + t, t, h - 2. * t, color);
}

/// A short network notice (joined, left, connection lost), fading out after a few seconds.
pub fn draw_toast(o: &mut Overlay, w: f32, h: f32, msg: &str, age: f32, scale: u8) {
    if age > 4.0 {
        return;
    }
    let ui = ui_scale(h, scale);
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

/// The held-item slot, top right.
pub fn draw_item_slot(
    o: &mut Overlay,
    w: f32,
    h: f32,
    item: Option<crate::combat::ItemKind>,
    time: f32,
    hint: &str,
    scale: u8,
) {
    use crate::combat::ItemKind;
    let ui = ui_scale(h, scale);
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
pub fn draw_fps_counter(o: &mut Overlay, w: f32, h: f32, fps: f32, scale: u8) {
    let ui = ui_scale(h, scale);
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
mod tests {
    use super::*;
    use rivet_physics::VehicleParams;

    #[test]
    fn ui_scale_applies_percent_and_clamps() {
        assert!((ui_scale(900.0, 100) - 1.0).abs() < 1e-6);
        assert!((ui_scale(900.0, 130) - 1.3).abs() < 1e-6);
        assert!((ui_scale(900.0, 70) - 0.7).abs() < 1e-6);
        // The window-height clamp still applies underneath.
        assert!((ui_scale(100.0, 100) - 0.6).abs() < 1e-6);
        assert!((ui_scale(5000.0, 100) - 2.0).abs() < 1e-6);
        // Out-of-range percents clamp to 70-130.
        assert!((ui_scale(900.0, 10) - 0.7).abs() < 1e-6);
        assert!((ui_scale(900.0, 250) - 1.3).abs() < 1e-6);
    }

    #[test]
    fn speed_readout_converts_and_labels() {
        assert_eq!(speed_readout(10.0, Units::Kmh), (36, "KM/H"));
        assert_eq!(speed_readout(10.0, Units::Mph), (22, "MPH"));
        assert_eq!(speed_readout(0.0, Units::Mph), (0, "MPH"));
        assert_eq!(speed_readout(44.704, Units::Mph).0, 100);
    }

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

    /// WCAG contrast ratio between two sRGB colors.
    fn contrast(a: [f32; 4], b: [f32; 4]) -> f32 {
        let lum = |c: [f32; 4]| {
            let lin = |v: f32| {
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
        };
        let (hi, lo) = (lum(a).max(lum(b)), lum(a).min(lum(b)));
        (hi + 0.05) / (lo + 0.05)
    }

    #[test]
    fn face_button_letters_are_legible() {
        for g in [PadGlyph::A, PadGlyph::B, PadGlyph::X, PadGlyph::Y] {
            let (letter, fill, ink) = g.face().expect("a face button");
            let ratio = contrast(fill, ink);
            assert!(ratio >= 4.5, "{letter}: {ratio:.2}:1");
        }
        assert!(PadGlyph::LeftBumper.face().is_none());
    }

    #[test]
    fn time_format() {
        assert_eq!(fmt_time(0.0), "0:00.000");
        assert_eq!(fmt_time(75.25), "1:15.250");
    }
}
