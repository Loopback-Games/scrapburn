//! The right-hand side of the Crucible menus: what the highlighted row is about. Each
//! preview draws in a 570x430 design-pixel area at (660, 110), as in the mockup.

use std::borrow::Cow;

use glam::Vec3;
use rivet_render::{Face, Overlay};
use winit::keyboard::KeyCode;

use super::{
    Extra, MenuView, PauseInfo, ResultsInfo, line, outline, polyline, px, ring, tracked,
    tracked_width, wrap,
};
use crate::bindings::{BindRow, PadBind};
use crate::hud::{Minimap, fmt_time, ordinal};
use crate::menu::{Row, RowId, RowKind, Screen, Tab};
use crate::style::crucible::{ASH, EMBER, GLOW, GOLD, HOT, fade};
use crate::track::TRACKS;
use rivet_platform::PadButtons;

/// Where the preview area starts, in design pixels.
const AREA_X: f32 = 660.0;
const AREA_Y: f32 = 110.0;
const AREA_W: f32 = 570.0;

/// The preview area: design coordinates inside it to screen pixels.
#[derive(Clone, Copy)]
struct Area {
    x: f32,
    y: f32,
    s: f32,
}

impl Area {
    fn new(s: f32) -> Self {
        Self {
            x: AREA_X * s,
            y: AREA_Y * s,
            s,
        }
    }

    fn p(&self, x: f32, y: f32) -> [f32; 2] {
        [self.x + x * self.s, self.y + y * self.s]
    }

    fn d(&self, v: f32) -> f32 {
        v * self.s
    }
}

/// The text styles the mockup's previews use.
#[derive(Clone, Copy)]
enum Style {
    /// Small ash caption (`f-t`).
    Caption,
    /// Small gold caption (`f-ta`).
    Accent,
    /// Stencil display text in white heat (`f-tb`).
    Display,
}

impl Style {
    fn face(self) -> Face {
        match self {
            Style::Display => Face::Display,
            _ => Face::Row,
        }
    }

    fn color(self) -> [f32; 4] {
        match self {
            Style::Caption => ASH,
            Style::Accent => GOLD,
            Style::Display => HOT,
        }
    }

    fn tracking(self) -> f32 {
        match self {
            Style::Display => 0.0,
            _ => 0.14,
        }
    }

    /// Cap height as a fraction of the font size, to place text by its baseline.
    fn cap(self) -> f32 {
        match self {
            Style::Display => 0.8,
            _ => 0.7,
        }
    }
}

/// Text with its baseline at design (`x`, `y`); returns its width in design pixels.
fn text(o: &mut Overlay, a: Area, x: f32, y: f32, size: f32, style: Style, s: &str) -> f32 {
    text_colored(o, a, x, y, size, style, style.color(), s)
}

#[allow(clippy::too_many_arguments)]
fn text_colored(
    o: &mut Overlay,
    a: Area,
    x: f32,
    y: f32,
    size: f32,
    style: Style,
    color: [f32; 4],
    s: &str,
) -> f32 {
    // Every style is set in capitals (the stencil face has nothing else).
    let [sx, sy] = a.p(x, y - 0.5 * style.cap() * size);
    tracked(
        o,
        style.face(),
        sx,
        sy,
        a.d(size),
        style.tracking(),
        color,
        s,
    ) / a.s
}

/// Width of `text` in design pixels.
fn text_width(a: Area, size: f32, style: Style, s: &str) -> f32 {
    tracked_width(style.face(), a.d(size), style.tracking(), s) / a.s
}

/// Text centered on design `cx`.
fn text_centered(o: &mut Overlay, a: Area, cx: f32, y: f32, size: f32, style: Style, s: &str) {
    let w = text_width(a, size, style, s);
    text(o, a, cx - 0.5 * w, y, size, style, s);
}

/// Display text shrunk to fit `max_w` design pixels.
fn display_fit(o: &mut Overlay, a: Area, x: f32, y: f32, size: f32, max_w: f32, s: &str) {
    // The stencil face has capitals, digits and ASCII punctuation only.
    let s = &s.replace('×', "X").replace('°', "");
    let w = text_width(a, size, Style::Display, s);
    let size = if w > max_w { size * max_w / w } else { size };
    text(o, a, x, y, size, Style::Display, s);
}

/// The preview's small gold caption at its top left.
fn frame_label(o: &mut Overlay, a: Area, label: &str) {
    let [x, y] = a.p(0.0, 0.0);
    tracked(o, Face::Row, x, y, a.d(12.0), 0.3, GOLD, label);
}

/// A glowing ember stroke (`f-acc`).
fn glow_line(o: &mut Overlay, a: Area, p: [f32; 2], q: [f32; 2], t: f32) {
    line(o, p, q, a.d(t * 3.0), fade(GLOW, 0.25));
    line(o, p, q, a.d(t), EMBER);
}

fn glow_polyline(o: &mut Overlay, a: Area, points: &[[f32; 2]], t: f32, closed: bool) {
    polyline(o, points, a.d(t * 3.2), fade(GLOW, 0.22), closed);
    polyline(o, points, a.d(t), EMBER, closed);
}

pub fn draw(o: &mut Overlay, w: f32, h: f32, s: f32, v: &MenuView) {
    let _ = (w, h);
    let a = Area::new(s);
    let track = v.settings.track % TRACKS.len();
    let selected = v.rows.get(v.menu.selected);
    match v.menu.screen {
        Screen::Title => match selected.map(|r| r.id) {
            Some(RowId::SoloRace) => track_preview(o, a, v, track, None),
            Some(RowId::Multiplayer) => network(o, a, v),
            Some(RowId::Garage) => garage(o, a, v),
            Some(RowId::Settings) => graphics(o, a, v),
            Some(RowId::Update) => farewell(
                o,
                a,
                "Fresh parts",
                "The new version installs, then the game restarts.",
            ),
            _ => farewell(o, a, "See you on the grid.", ""),
        },
        Screen::RaceSetup => {
            let circuit = v.menu.solo_event() == crate::circuit::Event::Circuit;
            track_preview(
                o,
                a,
                v,
                track,
                circuit.then_some("Circuit: every track in order"),
            )
        }
        Screen::Circuits => {
            let i = if v.menu.selected < TRACKS.len() {
                v.menu.selected
            } else {
                track
            };
            track_preview(o, a, v, i, None)
        }
        Screen::Garage => garage(o, a, v),
        Screen::Online | Screen::Tailscale => network(o, a, v),
        Screen::DriverName => name_plate(o, a, v),
        Screen::Lobby => {
            if let Extra::Lobby {
                lines,
                names,
                slots,
            } = &v.extra
            {
                grid(o, a, lines, names, *slots);
            }
        }
        Screen::Paused => {
            if let Extra::Pause(p) = &v.extra {
                pause(o, a, p, v.routes.get(p.track).map_or(&[], Vec::as_slice));
            }
        }
        Screen::Results => {
            if let Extra::Results(r) = &v.extra {
                results(o, a, r);
            }
        }
        Screen::Settings(Tab::Graphics) => graphics(o, a, v),
        Screen::Settings(Tab::Audio) => audio(o, a, v),
        Screen::Settings(Tab::Controls) => controls(o, a, v, selected),
        Screen::Settings(Tab::Camera) => camera(o, a, v),
        Screen::Settings(Tab::Gameplay) => track_preview(o, a, v, track, None),
        Screen::Playing => {}
    }
}

/// The rows of a settings tab: the frame's own rows on that tab, else built for it.
fn tab_rows<'a>(v: &MenuView<'a>, tab: Tab) -> Cow<'a, [Row]> {
    if v.menu.screen == Screen::Settings(tab) {
        Cow::Borrowed(v.rows)
    } else {
        Cow::Owned(v.menu.tab_rows(tab, v.settings))
    }
}

fn row_value(rows: &[Row], id: RowId) -> String {
    rows.iter()
        .find(|r| r.id == id)
        .and_then(|r| match &r.kind {
            RowKind::Choice { value }
            | RowKind::Slider {
                value_text: value, ..
            }
            | RowKind::Bind { text: value }
            | RowKind::Text { value } => Some(value.clone()),
            RowKind::Go => None,
        })
        .unwrap_or_default()
}

/// A plan view of `route` fitted in a design-pixel box, as screen points.
fn route_points(a: Area, route: &[Vec3], (x, y, w, h): (f32, f32, f32, f32)) -> Vec<[f32; 2]> {
    let [sx, sy] = a.p(x, y);
    let map = Minimap::fit(route, (sx, sy, a.d(w), a.d(h)));
    route
        .iter()
        .step_by(2)
        .map(|p| {
            let (px, py) = map.project(*p);
            [px, py]
        })
        .collect()
}

/// A glowing route with a dim shadow and a marker running round it.
fn route(o: &mut Overlay, a: Area, v_time: f32, route: &[Vec3], rect: (f32, f32, f32, f32)) {
    let points = route_points(a, route, rect);
    let shadow: Vec<[f32; 2]> = points
        .iter()
        .map(|p| [p[0] + a.d(3.0), p[1] + a.d(3.0)])
        .collect();
    polyline(o, &shadow, a.d(1.0), fade(ASH, 0.3), true);
    glow_polyline(o, a, &points, 2.5, true);
    if let Some(&start) = points.first() {
        o.circle(start[0], start[1], a.d(5.0), EMBER);
    }
    if !points.is_empty() {
        let p = points[(v_time * 14.0) as usize % points.len()];
        o.circle(p[0], p[1], a.d(7.0), fade(GLOW, 0.4));
        o.circle(p[0], p[1], a.d(3.5), HOT);
    }
}

/// A panel of frosted glass with a faint warm rim.
fn glass_panel(o: &mut Overlay, a: Area, (x, y, w, h): (f32, f32, f32, f32)) {
    let [px, py] = a.p(x, y);
    let (pw, ph) = (a.d(w), a.d(h));
    o.glow_rect(px, py, pw, ph, a.d(30.0), [0.0, 0.0, 0.0, 0.35]);
    o.backdrop_quad(
        [[px, py], [px + pw, py], [px + pw, py + ph], [px, py + ph]],
        [0.42, 0.36, 0.32, 1.0],
    );
    o.rect(
        px,
        py,
        pw,
        ph,
        [16.0 / 255.0, 10.0 / 255.0, 8.0 / 255.0, 0.35],
    );
    outline(o, px, py, pw, ph, a.d(1.0), [1.0, 0.75, 0.51, 0.25]);
}

/// Track name, route, round and its song.
fn track_preview(o: &mut Overlay, a: Area, v: &MenuView, i: usize, note: Option<&str>) {
    let def = &TRACKS[i];
    frame_label(o, a, "The route");
    let panel = (0.0, 26.0, 360.0, 204.0);
    glass_panel(o, a, panel);
    if let Some(r) = v.routes.get(i) {
        route(o, a, v.t(), r, (24.0, 44.0, 312.0, 168.0));
    }
    for (k, l) in wrap(Face::Row, def.tagline, px(13.0, a.s), a.d(180.0))
        .iter()
        .take(6)
        .enumerate()
    {
        text(o, a, 384.0, 62.0 + k as f32 * 22.0, 13.0, Style::Caption, l);
    }
    display_fit(o, a, 0.0, 290.0, 46.0, AREA_W, def.name);
    let round = note.map_or_else(
        || format!("Track {} of {}", i + 1, TRACKS.len()),
        str::to_string,
    );
    text(o, a, 2.0, 320.0, 13.0, Style::Accent, &round);
    let song = crate::sound::theme_for(def.id).info();
    text(
        o,
        a,
        2.0,
        344.0,
        13.0,
        Style::Caption,
        &format!("{}, {}, {:.0} BPM", song.style, song.key, song.bpm),
    );
}

/// The garage's name plate sits under the turning kart: its left edge and the name's
/// baseline, in design pixels.
const GARAGE_X: f32 = 40.0;
const GARAGE_Y: f32 = 488.0;

/// The finish swatches under the garage's kart, in screen pixels.
pub fn swatch_rects(w: f32, h: f32) -> Vec<(f32, f32, f32, f32)> {
    swatches_in(Area::new(super::menu_scale(w, h)))
}

fn swatches_in(a: Area) -> Vec<(f32, f32, f32, f32)> {
    (0..crate::kart::FINISHES.len())
        .map(|i| {
            let [x, y] = a.p(GARAGE_X + i as f32 * 34.0, GARAGE_Y + 40.0);
            (x, y, a.d(26.0), a.d(26.0))
        })
        .collect()
}

/// The finish swatch under the pointer, if any.
pub fn garage_swatch_at(w: f32, h: f32, x: f32, y: f32) -> Option<usize> {
    swatch_rects(w, h)
        .iter()
        .position(|&(sx, sy, sw, sh)| (sx..sx + sw).contains(&x) && (sy..sy + sh).contains(&y))
}

/// The kart turns in the service bay above; this names it and offers its paint.
fn garage(o: &mut Overlay, a: Area, v: &MenuView) {
    let loadout = v.settings.loadout;
    let kart = &crate::kart::KARTS[loadout.design];
    glass_panel(o, a, (GARAGE_X - 18.0, GARAGE_Y - 70.0, 520.0, 152.0));
    text(
        o,
        a,
        GARAGE_X,
        GARAGE_Y - 48.0,
        12.0,
        Style::Accent,
        "Your machine",
    );
    display_fit(o, a, GARAGE_X, GARAGE_Y, 40.0, 480.0, &kart.label());
    let finish = crate::kart::FINISHES[loadout.finish];
    text(
        o,
        a,
        GARAGE_X,
        GARAGE_Y + 24.0,
        13.0,
        Style::Caption,
        &format!(
            "{} class  /  wheel ratio {:.2}  /  {} finish",
            kart.kind.label(),
            kart.wheel_ratio,
            finish
        ),
    );
    for (i, (x, y, sw, sh)) in swatches_in(a).into_iter().enumerate() {
        let [r, g, b] = crate::race::TINTS[i];
        let c = [
            (r * 0.8).min(1.0),
            (g * 0.8).min(1.0),
            (b * 0.8).min(1.0),
            1.0,
        ];
        if i == loadout.finish {
            o.glow_rect(x, y, sw, sh, a.d(10.0), fade(GLOW, 0.8));
        }
        o.rect(x, y, sw, sh, c);
        let (t, rim) = if i == loadout.finish {
            (a.d(2.5), EMBER)
        } else {
            (a.d(1.0), fade(ASH, 0.3))
        };
        outline(o, x, y, sw, sh, t, rim);
    }
}

/// Tailnet peers around "you", and the online status lines.
fn network(o: &mut Overlay, a: Area, v: &MenuView) {
    frame_label(o, a, "Tailnet");
    let center = (270.0, 160.0);
    let spots = [
        (90.0, 60.0),
        (450.0, 70.0),
        (470.0, 240.0),
        (80.0, 250.0),
        (280.0, 30.0),
        (290.0, 290.0),
    ];
    let peers = &v.menu.peers;
    for ((name, _), &(x, y)) in peers.iter().zip(&spots) {
        line(
            o,
            a.p(center.0, center.1),
            a.p(x, y),
            a.d(1.0),
            fade(ASH, 0.3),
        );
        let [px, py] = a.p(x, y);
        ring(o, px, py, a.d(16.0), a.d(1.4), ASH);
        o.circle(px, py, a.d(8.0), fade(GLOW, 0.35));
        o.circle(px, py, a.d(5.0), EMBER);
        text_centered(o, a, x, y + 38.0, 13.0, Style::Caption, name);
    }
    let [cx, cy] = a.p(center.0, center.1);
    o.circle(cx, cy, a.d(44.0), [1.0, 0.78, 0.59, 0.05]);
    ring(o, cx, cy, a.d(44.0), a.d(4.0), fade(GLOW, 0.25));
    ring(o, cx, cy, a.d(44.0), a.d(1.4), GOLD);
    text_centered(o, a, center.0, center.1 + 12.0, 30.0, Style::Display, "YOU");
    let status = if peers.is_empty() {
        "No Tailscale peers found yet. The list refreshes every few seconds.".to_string()
    } else {
        format!(
            "{} devices online. The list refreshes every few seconds.",
            peers.len()
        )
    };
    text(o, a, 0.0, 350.0, 13.0, Style::Accent, &status);
    if let Extra::Online(lines) = &v.extra {
        for (k, l) in lines.iter().take(3).enumerate() {
            text(o, a, 0.0, 374.0 + k as f32 * 22.0, 13.0, Style::Caption, l);
        }
    }
}

/// The name being typed, as others will see it.
fn name_plate(o: &mut Overlay, a: Area, v: &MenuView) {
    frame_label(o, a, "Your tag");
    let name = if v.menu.name_entry.is_empty() {
        v.player_name
    } else {
        v.menu.name_entry.as_str()
    };
    glass_panel(o, a, (0.0, 40.0, 520.0, 150.0));
    display_fit(o, a, 28.0, 140.0, 72.0, 470.0, name);
    text(
        o,
        a,
        2.0,
        230.0,
        13.0,
        Style::Accent,
        "Shown over your kart and on the grid",
    );
}

/// The eight grid slots of the lobby.
fn grid(o: &mut Overlay, a: Area, lines: &[String], names: &[String], slots: usize) {
    frame_label(o, a, "Starting grid");
    for i in 0..slots.min(8) {
        let x = if i % 2 == 1 { 290.0 } else { 40.0 };
        let y = 30.0 + (i / 2) as f32 * 82.0 + if i % 2 == 1 { 40.0 } else { 0.0 };
        let name = names.get(i);
        let c = if name.is_some() { ASH } else { fade(ASH, 0.3) };
        polyline(
            o,
            &[
                a.p(x + 24.0, y),
                a.p(x, y),
                a.p(x, y + 54.0),
                a.p(x + 210.0, y + 54.0),
            ],
            a.d(1.4),
            c,
            false,
        );
        let tag = text(
            o,
            a,
            x + 12.0,
            y + 22.0,
            13.0,
            Style::Accent,
            &format!("P{}", i + 1),
        );
        if i == 0 && name.is_some() {
            text(o, a, x + 24.0 + tag, y + 22.0, 11.0, Style::Caption, "Host");
        }
        match name {
            Some(n) => display_fit(o, a, x + 12.0, y + 46.0, 24.0, 190.0, n),
            None => {
                text_colored(
                    o,
                    a,
                    x + 12.0,
                    y + 44.0,
                    13.0,
                    Style::Caption,
                    fade(ASH, 0.6),
                    "Open slot",
                );
            }
        }
    }
    let head = format!("{} of {} drivers", names.len(), slots);
    text(o, a, 40.0, 382.0, 13.0, Style::Accent, &head);
    for (k, l) in lines.iter().take(2).enumerate() {
        text(o, a, 40.0, 404.0 + k as f32 * 20.0, 13.0, Style::Caption, l);
    }
}

/// Position, lap, best lap and the route, under the pause menu.
fn pause(o: &mut Overlay, a: Area, p: &PauseInfo, route: &[Vec3]) {
    frame_label(o, a, "Race status");
    glass_panel(o, a, (0.0, 24.0, 570.0, 306.0));
    text(o, a, 30.0, 60.0, 13.0, Style::Accent, "Position");
    let pos = ordinal(p.position);
    let w = text(o, a, 30.0, 150.0, 104.0, Style::Display, &pos);
    text(
        o,
        a,
        30.0 + w + 16.0,
        150.0,
        13.0,
        Style::Caption,
        &format!("of {}", p.racers),
    );
    text(o, a, 30.0, 210.0, 13.0, Style::Accent, "Lap");
    let lap = match p.laps {
        Some(n) => format!("{} / {}", p.lap.min(n), n),
        None => p.lap.to_string(),
    };
    text(o, a, 30.0, 256.0, 42.0, Style::Display, &lap);
    text(o, a, 260.0, 210.0, 13.0, Style::Accent, "Best lap");
    let best = p.best.map_or_else(|| "None yet".to_string(), fmt_time);
    text(o, a, 260.0, 256.0, 42.0, Style::Display, &best);
    if !route.is_empty() {
        let points = route_points(a, route, (340.0, 48.0, 200.0, 120.0));
        glow_polyline(o, a, &points, 2.5, true);
    }
    let mut foot = p.track_name.to_string();
    if let Some((round, rounds)) = p.circuit {
        foot.push_str(&format!("   Circuit round {round} of {rounds}"));
    }
    text(o, a, 30.0, 310.0, 13.0, Style::Caption, &foot);
}

/// The finishing order on the right of the results screen.
fn results(o: &mut Overlay, a: Area, r: &ResultsInfo) {
    let circuit = r.circuit.is_some();
    let width = 560.0;
    let n = r.rows.len().max(1) as f32;
    let row_h = ((430.0 - 40.0) / n).min(38.0);
    glass_panel(o, a, (-14.0, -10.0, width + 28.0, 38.0 + n * row_h + 10.0));
    let cols: &[(&str, f32, bool)] = if circuit {
        // Pos is the place in the circuit; Race is the place in the race just run.
        &[
            ("Pos", 6.0, false),
            ("Driver", 58.0, false),
            ("Race", 290.0, true),
            ("Time", 410.0, true),
            ("+Pts", 482.0, true),
            ("Total", 552.0, true),
        ]
    } else {
        &[
            ("Pos", 6.0, false),
            ("Driver", 58.0, false),
            ("Time", 552.0, true),
        ]
    };
    for &(name, x, right) in cols {
        let tw = text_width(a, 11.0, Style::Accent, name);
        let x = if right { x - tw } else { x };
        text(o, a, x, 18.0, 11.0, Style::Accent, name);
    }
    let rule = [1.0, 0.75, 0.51, 0.14];
    let [lx, ly] = a.p(0.0, 28.0);
    o.rect(lx, ly, a.d(width), a.d(1.0), rule);
    for (k, row) in r.rows.iter().enumerate() {
        let top = 28.0 + k as f32 * row_h;
        let base = top + row_h * 0.5 + 6.0;
        if row.me {
            let [x, y] = a.p(0.0, top + 1.0);
            o.quad(
                [
                    [x, y],
                    [x + a.d(width), y],
                    [x + a.d(width), y + a.d(row_h - 1.0)],
                    [x, y + a.d(row_h - 1.0)],
                ],
                [
                    fade(EMBER, 0.28),
                    fade(EMBER, 0.04),
                    fade(EMBER, 0.04),
                    fade(EMBER, 0.28),
                ],
            );
        }
        let c = if row.me { HOT } else { ASH };
        let size = (row_h * 0.47).min(18.0);
        let cell = |o: &mut Overlay, x: f32, right: bool, s: &str| {
            let x = if right {
                x - text_width(a, size, Style::Caption, s)
            } else {
                x
            };
            text_colored(o, a, x, base, size, Style::Caption, c, s);
        };
        cell(o, 6.0, false, &(k + 1).to_string());
        cell(o, 58.0, false, &row.name);
        let time = row.time.map_or_else(|| "--".to_string(), fmt_time);
        if let Some((gained, total)) = row.points.filter(|_| circuit) {
            cell(o, 290.0, true, &crate::hud::ordinal(row.place));
            cell(o, 410.0, true, &time);
            cell(o, 482.0, true, &format!("+{gained}"));
            cell(o, 552.0, true, &total.to_string());
        } else {
            cell(o, 552.0, true, &time);
        }
        let [lx, ly] = a.p(0.0, top + row_h);
        o.rect(lx, ly, a.d(width), a.d(1.0), rule);
    }
}

/// The graphics settings that matter most, in large type; the scene behind is the preview.
fn graphics(o: &mut Overlay, a: Area, v: &MenuView) {
    let rows = tab_rows(v, Tab::Graphics);
    let preset = row_value(&rows, RowId::Preset);
    frame_label(o, a, &format!("Preview / {preset}"));
    let specs = [
        ("Render scale", RowId::RenderScale),
        ("Shadow map", RowId::Shadows),
        ("Bloom", RowId::Bloom),
        ("Anisotropy", RowId::Anisotropy),
        ("Textures", RowId::Textures),
        ("Frame cap", RowId::FrameCap),
    ];
    for (k, (label, id)) in specs.iter().enumerate() {
        let x = if k % 2 == 0 { 0.0 } else { 280.0 };
        let y = 52.0 + (k / 2) as f32 * 78.0;
        text(o, a, x, y, 11.0, Style::Caption, label);
        display_fit(o, a, x, y + 40.0, 34.0, 250.0, &row_value(&rows, *id));
    }
    let [x, y] = a.p(0.0, 262.0);
    o.rect(x, y, a.d(520.0), a.d(1.0), [1.0, 0.75, 0.51, 0.18]);
    text(
        o,
        a,
        0.0,
        292.0,
        13.0,
        Style::Accent,
        "The scene behind reacts to your changes",
    );
    text(
        o,
        a,
        0.0,
        318.0,
        13.0,
        Style::Caption,
        "Low: 75% scale, 1024 shadows. Runs on Intel UHD 620.",
    );
}

/// Five faders and the song the Listen to row plays.
fn audio(o: &mut Overlay, a: Area, v: &MenuView) {
    frame_label(o, a, "Mixer");
    let au = &v.settings.prefs.audio;
    let channels = [
        ("MST", au.master),
        ("MUS", au.music),
        ("FX", au.effects),
        ("ENG", au.engines),
        ("UI", au.menus),
    ];
    let (top, height) = (40.0, 180.0);
    for (i, (label, value)) in channels.iter().enumerate() {
        let x = 40.0 + i as f32 * 66.0;
        let y = top + height * (1.0 - f32::from(*value) / 100.0);
        line(
            o,
            a.p(x, top),
            a.p(x, top + height),
            a.d(1.0),
            fade(ASH, 0.3),
        );
        glow_line(o, a, a.p(x, y), a.p(x, top + height), 2.5);
        let [kx, ky] = a.p(x - 14.0, y - 7.0);
        o.rect(kx, ky, a.d(28.0), a.d(14.0), [0.08, 0.05, 0.04, 0.9]);
        outline(o, kx, ky, a.d(28.0), a.d(14.0), a.d(1.4), GOLD);
        text_centered(o, a, x, top + height + 24.0, 13.0, Style::Caption, label);
        text_centered(o, a, x, 28.0, 13.0, Style::Accent, &value.to_string());
    }
    let song = v.menu.listen.info();
    text(o, a, 370.0, 60.0, 13.0, Style::Accent, "Now playing");
    display_fit(o, a, 370.0, 96.0, 28.0, 200.0, song.title);
    text(o, a, 370.0, 122.0, 13.0, Style::Caption, song.style);
    text(
        o,
        a,
        370.0,
        142.0,
        13.0,
        Style::Caption,
        &format!("{}, {:.0} BPM", song.key, song.bpm),
    );
    if au.soundtrack {
        let t = v.t();
        for i in 0..14 {
            let f = i as f32;
            let level = if v.motion {
                0.3 + 0.7 * ((t * (3.0 + (f * 53.0) % 5.0) + f * 1.37).sin() * 0.5 + 0.5)
            } else {
                0.35 + 0.6 * ((f * 2.3).sin() * 0.5 + 0.5)
            };
            let bh = 44.0 * level;
            let [x, y] = a.p(370.0 + f * 11.0, 214.0 - bh);
            o.glow_rect(x, y, a.d(7.0), a.d(bh), a.d(4.0), fade(GLOW, 0.3));
            o.rect(x, y, a.d(7.0), a.d(bh), EMBER);
        }
    } else {
        text_colored(
            o,
            a,
            370.0,
            200.0,
            13.0,
            Style::Caption,
            fade(ASH, 0.7),
            "Soundtrack off",
        );
    }
}

/// The action a Controls row binds, if it is a binding row.
fn bind_row(row: Option<&Row>) -> Option<BindRow> {
    match row?.id {
        RowId::Bind(b) => Some(b),
        _ => None,
    }
}

/// A key cap on the keyboard diagram: code, label, and width in key units.
type Cap = (Option<KeyCode>, &'static str, f32);

/// A compact keyboard, row by row. `None` codes are gaps.
const KEYBOARD: [&[Cap]; 6] = {
    use KeyCode::*;
    [
        &[
            (Some(Escape), "Esc", 1.0),
            (None, "", 0.5),
            (Some(F1), "F1", 1.0),
            (Some(F2), "F2", 1.0),
            (Some(F3), "F3", 1.0),
            (Some(F4), "F4", 1.0),
            (None, "", 0.5),
            (Some(F5), "F5", 1.0),
            (Some(F6), "F6", 1.0),
            (Some(F7), "F7", 1.0),
            (Some(F8), "F8", 1.0),
            (None, "", 0.5),
            (Some(F9), "F9", 1.0),
            (Some(F10), "F10", 1.0),
            (Some(F11), "F11", 1.0),
            (Some(F12), "F12", 1.0),
        ],
        &[
            (Some(Backquote), "`", 1.0),
            (Some(Digit1), "1", 1.0),
            (Some(Digit2), "2", 1.0),
            (Some(Digit3), "3", 1.0),
            (Some(Digit4), "4", 1.0),
            (Some(Digit5), "5", 1.0),
            (Some(Digit6), "6", 1.0),
            (Some(Digit7), "7", 1.0),
            (Some(Digit8), "8", 1.0),
            (Some(Digit9), "9", 1.0),
            (Some(Digit0), "0", 1.0),
            (Some(Minus), "-", 1.0),
            (Some(Equal), "=", 1.0),
            (Some(Backspace), "", 2.0),
        ],
        &[
            (Some(Tab), "Tab", 1.5),
            (Some(KeyQ), "Q", 1.0),
            (Some(KeyW), "W", 1.0),
            (Some(KeyE), "E", 1.0),
            (Some(KeyR), "R", 1.0),
            (Some(KeyT), "T", 1.0),
            (Some(KeyY), "Y", 1.0),
            (Some(KeyU), "U", 1.0),
            (Some(KeyI), "I", 1.0),
            (Some(KeyO), "O", 1.0),
            (Some(KeyP), "P", 1.0),
            (Some(BracketLeft), "[", 1.0),
            (Some(BracketRight), "]", 1.0),
            (Some(Backslash), "\\", 1.5),
        ],
        &[
            (Some(CapsLock), "", 1.75),
            (Some(KeyA), "A", 1.0),
            (Some(KeyS), "S", 1.0),
            (Some(KeyD), "D", 1.0),
            (Some(KeyF), "F", 1.0),
            (Some(KeyG), "G", 1.0),
            (Some(KeyH), "H", 1.0),
            (Some(KeyJ), "J", 1.0),
            (Some(KeyK), "K", 1.0),
            (Some(KeyL), "L", 1.0),
            (Some(Semicolon), ";", 1.0),
            (Some(Quote), "'", 1.0),
            (Some(Enter), "Enter", 2.25),
        ],
        &[
            (Some(ShiftLeft), "Shift", 2.25),
            (Some(KeyZ), "Z", 1.0),
            (Some(KeyX), "X", 1.0),
            (Some(KeyC), "C", 1.0),
            (Some(KeyV), "V", 1.0),
            (Some(KeyB), "B", 1.0),
            (Some(KeyN), "N", 1.0),
            (Some(KeyM), "M", 1.0),
            (Some(Comma), ",", 1.0),
            (Some(Period), ".", 1.0),
            (Some(Slash), "/", 1.0),
            (Some(ShiftRight), "Shift", 2.75),
            (None, "", 1.4),
            (Some(ArrowUp), "", 1.0),
        ],
        &[
            (Some(ControlLeft), "Ctrl", 1.5),
            (Some(SuperLeft), "", 1.25),
            (Some(AltLeft), "Alt", 1.25),
            (Some(Space), "Space", 6.75),
            (Some(AltRight), "", 1.25),
            (Some(ContextMenu), "", 1.5),
            (Some(ControlRight), "", 1.5),
            (None, "", 0.4),
            (Some(ArrowLeft), "", 1.0),
            (Some(ArrowDown), "", 1.0),
            (Some(ArrowRight), "", 1.0),
        ],
    ]
};

/// Key and gamepad diagrams that light up the selected binding.
fn controls(o: &mut Overlay, a: Area, v: &MenuView, row: Option<&Row>) {
    let b = &v.settings.prefs.controls.bindings;
    let bind = bind_row(row);
    let capturing = v.menu.capturing.is_some();
    let pad = v.menu.gamepad_guide;
    let (caption, hint) = match (bind, row) {
        (Some(_), Some(r)) => {
            let value = match &r.kind {
                RowKind::Bind { text } => text.clone(),
                _ => String::new(),
            };
            (
                format!("{}:  {}", r.label, value),
                if capturing {
                    if pad {
                        "Press a button. Start cancels."
                    } else {
                        "Press a key. Esc cancels."
                    }
                } else if bind == Some(BindRow::Pause) {
                    "Pause is fixed so you can always reach the menu."
                } else {
                    match (v.pad, pad) {
                        (true, true) => "Press A, then a new button to rebind",
                        (true, false) => "Press A, then a new key to rebind",
                        (false, true) => "Press Enter, then a new button to rebind",
                        (false, false) => "Press Enter, then a new key to rebind",
                    }
                },
            )
        }
        _ => (
            if pad {
                "Select a binding to see its buttons".to_string()
            } else {
                "Select a binding to see its keys".to_string()
            },
            "",
        ),
    };
    if pad {
        gamepad(o, a, b, bind);
        text(o, a, 40.0, 360.0, 13.0, Style::Caption, &caption);
        text(o, a, 40.0, 384.0, 13.0, Style::Accent, hint);
    } else {
        keyboard(o, a, b, bind);
        text(o, a, 0.0, 300.0, 13.0, Style::Caption, &caption);
        text(o, a, 0.0, 324.0, 13.0, Style::Accent, hint);
    }
}

fn keyboard(o: &mut Overlay, a: Area, b: &crate::bindings::Bindings, bind: Option<BindRow>) {
    frame_label(o, a, "Keyboard");
    let hot: Vec<KeyCode> = match bind {
        Some(BindRow::Action(act)) => {
            let mut keys: Vec<KeyCode> = b.keys_for(act).into_iter().flatten().collect();
            // Either Shift answers a Shift binding.
            if keys.contains(&KeyCode::ShiftLeft) {
                keys.push(KeyCode::ShiftRight);
            }
            keys
        }
        Some(BindRow::Pause) => vec![KeyCode::Escape],
        None => Vec::new(),
    };
    let unit = 30.0;
    let gap = 4.0;
    glass_panel(o, a, (-12.0, 12.0, 576.0, 232.0));
    for (r, caps) in KEYBOARD.iter().enumerate() {
        let y = 24.0 + r as f32 * 34.0 + if r > 0 { 8.0 } else { 0.0 };
        let mut x = 0.0;
        for &(code, label, width) in caps.iter() {
            let kw = width * unit - gap;
            if let Some(code) = code {
                let on = hot.contains(&code);
                let [kx, ky] = a.p(x, y);
                let (kw_s, kh_s) = (a.d(kw), a.d(28.0));
                if on {
                    o.glow_rect(kx, ky, kw_s, kh_s, a.d(8.0), fade(GLOW, 0.6));
                    o.rect(kx, ky, kw_s, kh_s, [0.25, 0.09, 0.03, 1.0]);
                    outline(o, kx, ky, kw_s, kh_s, a.d(1.6), EMBER);
                } else {
                    outline(o, kx, ky, kw_s, kh_s, a.d(1.0), fade(ASH, 0.45));
                }
                let arrow = match code {
                    KeyCode::ArrowUp => Some(-std::f32::consts::FRAC_PI_2),
                    KeyCode::ArrowDown => Some(std::f32::consts::FRAC_PI_2),
                    KeyCode::ArrowLeft => Some(std::f32::consts::PI),
                    KeyCode::ArrowRight => Some(0.0),
                    _ => None,
                };
                if let Some(angle) = arrow {
                    let [cx, cy] = a.p(x + 0.5 * kw, y + 14.0);
                    let r = a.d(6.0);
                    let pt = |k: f32| {
                        let t = angle + k * std::f32::consts::TAU / 3.0;
                        [cx + r * t.cos(), cy + r * t.sin()]
                    };
                    o.triangle(
                        [pt(0.0), pt(1.0), pt(2.0)],
                        if on { GOLD } else { fade(ASH, 0.8) },
                    );
                }
                if !label.is_empty() {
                    let size = if label.len() > 3 { 9.0 } else { 11.0 };
                    let style = if on { Style::Accent } else { Style::Caption };
                    let c = if on { GOLD } else { fade(ASH, 0.8) };
                    let tw = text_width(a, size, style, label);
                    text_colored(o, a, x + 0.5 * (kw - tw), y + 18.0, size, style, c, label);
                }
            }
            x += width * unit;
        }
    }
}

/// Gamepad parts the diagram can light.
#[derive(Clone, Copy, PartialEq)]
enum PadPart {
    Button(PadButtons),
    Lt,
    Rt,
    Ls,
    DPad,
}

fn pad_part(bind: PadBind) -> PadPart {
    match bind {
        PadBind::Button(b)
            if [
                PadButtons::DPAD_UP,
                PadButtons::DPAD_DOWN,
                PadButtons::DPAD_LEFT,
                PadButtons::DPAD_RIGHT,
            ]
            .contains(&b) =>
        {
            PadPart::DPad
        }
        PadBind::Button(b) => PadPart::Button(b),
        PadBind::LeftTrigger => PadPart::Lt,
        PadBind::RightTrigger => PadPart::Rt,
        PadBind::Stick => PadPart::Ls,
    }
}

/// A cubic Bezier from `p0`, sampled into `out` (without its first point).
fn cubic(out: &mut Vec<[f32; 2]>, p0: [f32; 2], c: [[f32; 2]; 3]) {
    for i in 1..=10 {
        let t = i as f32 / 10.0;
        let u = 1.0 - t;
        let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
        let pts = [p0, c[0], c[1], c[2]];
        out.push([
            (0..4).map(|k| w[k] * pts[k][0]).sum(),
            (0..4).map(|k| w[k] * pts[k][1]).sum(),
        ]);
    }
}

/// The controller outline from the mockup, in its own 540-wide coordinates (built once).
fn pad_outline() -> &'static [[f32; 2]] {
    static OUTLINE: std::sync::OnceLock<Vec<[f32; 2]>> = std::sync::OnceLock::new();
    OUTLINE.get_or_init(build_pad_outline)
}

fn build_pad_outline() -> Vec<[f32; 2]> {
    let segs: [[[f32; 2]; 3]; 8] = [
        [[200.0, 78.0], [340.0, 78.0], [390.0, 90.0]],
        [[440.0, 100.0], [470.0, 190.0], [482.0, 250.0]],
        [[492.0, 300.0], [452.0, 320.0], [426.0, 292.0]],
        [[426.0, 292.0], [386.0, 248.0], [386.0, 248.0]],
        [[360.0, 236.0], [180.0, 236.0], [154.0, 248.0]],
        [[154.0, 248.0], [114.0, 292.0], [114.0, 292.0]],
        [[88.0, 320.0], [48.0, 300.0], [58.0, 250.0]],
        [[70.0, 190.0], [100.0, 100.0], [150.0, 90.0]],
    ];
    let mut out = vec![[150.0, 90.0]];
    for seg in segs {
        let p0 = *out.last().unwrap_or(&[150.0, 90.0]);
        cubic(&mut out, p0, seg);
    }
    out
}

fn gamepad(o: &mut Overlay, a: Area, b: &crate::bindings::Bindings, bind: Option<BindRow>) {
    frame_label(o, a, "Gamepad");
    glass_panel(o, a, (0.0, 16.0, 570.0, 318.0));
    let hot: Vec<PadPart> = match bind {
        Some(BindRow::Action(act)) if act.stick_only() => vec![PadPart::Ls],
        Some(BindRow::Action(act)) => b.pad_for(act).into_iter().flatten().map(pad_part).collect(),
        Some(BindRow::Pause) => vec![PadPart::Button(PadButtons::START)],
        None => Vec::new(),
    };
    // The mockup draws the pad 540 wide at x 0; center it in the area.
    let ox = 15.0;
    let at = |x: f32, y: f32| a.p(x + ox, y);
    let body: Vec<[f32; 2]> = pad_outline().iter().map(|p| at(p[0], p[1])).collect();
    polyline(o, &body, a.d(4.0), fade(GLOW, 0.18), true);
    polyline(o, &body, a.d(1.4), GOLD, true);
    let draw_part =
        |o: &mut Overlay, part: PadPart, shape: &dyn Fn(&mut Overlay, f32, [f32; 4])| {
            if hot.contains(&part) {
                shape(o, 2.5, EMBER);
            } else {
                shape(o, 1.4, ASH);
            }
        };
    let shoulder = |o: &mut Overlay, part, x: f32, y: f32, w: f32, label: &str| {
        let on = hot.contains(&part);
        let [px, py] = at(x, y);
        if on {
            o.glow_rect(px, py, a.d(w), a.d(22.0), a.d(8.0), fade(GLOW, 0.6));
            o.rect(px, py, a.d(w), a.d(22.0), [0.25, 0.09, 0.03, 1.0]);
        }
        draw_part(o, part, &|o, t, c| {
            outline(o, px, py, a.d(w), a.d(22.0), a.d(t), c)
        });
        let style = if on { Style::Accent } else { Style::Caption };
        text_centered(o, a, x + ox + 0.5 * w, y + 16.0, 13.0, style, label);
    };
    shoulder(o, PadPart::Lt, 130.0, 30.0, 70.0, "LT");
    shoulder(o, PadPart::Rt, 340.0, 30.0, 70.0, "RT");
    shoulder(
        o,
        PadPart::Button(PadButtons::LEFT_BUMPER),
        140.0,
        58.0,
        60.0,
        "LB",
    );
    shoulder(
        o,
        PadPart::Button(PadButtons::RIGHT_BUMPER),
        340.0,
        58.0,
        60.0,
        "RB",
    );
    let button = |o: &mut Overlay, part, x: f32, y: f32, r: f32, label: &str| {
        let on = hot.contains(&part);
        let [px, py] = at(x, y);
        if on {
            o.circle(px, py, a.d(r + 8.0), fade(GLOW, 0.35));
            o.circle(px, py, a.d(r - 2.0), [1.0, 0.42, 0.1, 0.3]);
        }
        draw_part(o, part, &|o, t, c| ring(o, px, py, a.d(r), a.d(t), c));
        if !label.is_empty() {
            let style = if on { Style::Accent } else { Style::Caption };
            text_centered(o, a, x + ox, y + 5.0, 13.0, style, label);
        }
    };
    button(o, PadPart::Ls, 170.0, 150.0, 26.0, "LS");
    button(
        o,
        PadPart::Button(PadButtons::NORTH),
        400.0,
        120.0,
        15.0,
        "Y",
    );
    button(
        o,
        PadPart::Button(PadButtons::WEST),
        372.0,
        148.0,
        15.0,
        "X",
    );
    button(
        o,
        PadPart::Button(PadButtons::EAST),
        428.0,
        148.0,
        15.0,
        "B",
    );
    button(
        o,
        PadPart::Button(PadButtons::SOUTH),
        400.0,
        176.0,
        15.0,
        "A",
    );
    button(
        o,
        PadPart::Button(PadButtons::SELECT),
        240.0,
        140.0,
        9.0,
        "",
    );
    button(o, PadPart::Button(PadButtons::START), 300.0, 140.0, 9.0, "");
    // The right stick is never bound.
    let [rx, ry] = at(330.0, 210.0);
    ring(o, rx, ry, a.d(22.0), a.d(1.4), ASH);
    text_centered(o, a, 330.0 + ox, 215.0, 13.0, Style::Caption, "RS");
    text_centered(o, a, 240.0 + ox, 170.0, 10.0, Style::Caption, "View");
    text_centered(o, a, 300.0 + ox, 170.0, 10.0, Style::Caption, "Menu");
    // The D-pad cross.
    let cross: Vec<[f32; 2]> = [
        (200.0, 198.0),
        (216.0, 198.0),
        (216.0, 182.0),
        (232.0, 182.0),
        (232.0, 198.0),
        (248.0, 198.0),
        (248.0, 214.0),
        (232.0, 214.0),
        (232.0, 230.0),
        (216.0, 230.0),
        (216.0, 214.0),
        (200.0, 214.0),
    ]
    .iter()
    .map(|&(x, y)| at(x, y))
    .collect();
    draw_part(o, PadPart::DPad, &|o, t, c| {
        polyline(o, &cross, a.d(t), c, true)
    });
}

/// A side view of the chase camera and a small HUD at the chosen scale.
fn camera(o: &mut Overlay, a: Area, v: &MenuView) {
    frame_label(o, a, "Camera rig, side view");
    let cam = &v.settings.prefs.camera;
    let k = 34.0;
    let (gx, gy) = (430.0, 200.0);
    let (cx, cy) = (gx - cam.distance * k, gy - cam.height * k - 10.0);
    let (tx, ty) = (gx + 40.0, gy - 24.0);
    let dim = fade(ASH, 0.3);
    line(o, a.p(0.0, gy), a.p(560.0, gy), a.d(1.0), dim);
    let ang = (ty - cy).atan2(tx - cx);
    let half = (cam.fov * 0.5).to_radians() * 0.55;
    let reach = 330.0;
    for side in [-1.0, 1.0] {
        let e = ang + side * half;
        let end = [cx + e.cos() * reach, (cy + e.sin() * reach).min(gy + 40.0)];
        line(o, a.p(cx, cy), a.p(end[0], end[1]), a.d(1.0), dim);
    }
    // Dashed sight line from the camera to the kart.
    let steps = 14;
    for i in (0..steps).step_by(2) {
        let f0 = i as f32 / steps as f32;
        let f1 = (i + 1) as f32 / steps as f32;
        let p0 = a.p(cx + (tx - cx) * f0, cy + (ty - cy) * f0);
        let p1 = a.p(cx + (tx - cx) * f1, cy + (ty - cy) * f1);
        glow_line(o, a, p0, p1, 2.0);
    }
    // The kart, from the mockup's buggy profile at a third scale.
    let kart = |x: f32, y: f32| a.p(gx - 30.0 + x * 0.32, gy - 62.0 + y * 0.32);
    let body: Vec<[f32; 2]> = [
        (70.0, 150.0),
        (96.0, 118.0),
        (170.0, 108.0),
        (208.0, 78.0),
        (282.0, 76.0),
        (322.0, 110.0),
        (362.0, 118.0),
        (370.0, 150.0),
    ]
    .iter()
    .map(|&(x, y)| kart(x, y))
    .collect();
    polyline(o, &body, a.d(1.4), HOT, true);
    for wx in [118.0, 320.0] {
        let [wx, wy] = kart(wx, 160.0);
        ring(o, wx, wy, a.d(36.0 * 0.32), a.d(1.4), GOLD);
    }
    // The camera body and lens, with shake marks.
    let [bx, by] = a.p(cx - 16.0, cy - 11.0);
    o.rect(bx, by, a.d(26.0), a.d(22.0), [1.0, 0.78, 0.59, 0.08]);
    outline(o, bx, by, a.d(26.0), a.d(22.0), a.d(1.4), GOLD);
    polyline(
        o,
        &[
            a.p(cx + 10.0, cy - 7.0),
            a.p(cx + 22.0, cy - 13.0),
            a.p(cx + 22.0, cy + 13.0),
            a.p(cx + 10.0, cy + 7.0),
        ],
        a.d(1.4),
        GOLD,
        true,
    );
    let shake = f32::from(cam.shake) / 100.0 * 6.0;
    if shake > 0.0 {
        for (off, spread) in [(24.0, 16.0), (30.0, 20.0)] {
            polyline(
                o,
                &[
                    a.p(cx - off, cy - spread * 0.5 - shake),
                    a.p(cx - off - 5.0, cy),
                    a.p(cx - off, cy + spread * 0.5 + shake),
                ],
                a.d(1.2),
                fade(ASH, 0.6),
                false,
            );
        }
    }
    // Distance and height dimensions.
    line(o, a.p(cx, gy + 16.0), a.p(gx, gy + 16.0), a.d(1.2), ASH);
    for x in [cx, gx] {
        line(o, a.p(x, gy + 10.0), a.p(x, gy + 22.0), a.d(1.2), ASH);
    }
    text_centered(
        o,
        a,
        0.5 * (cx + gx),
        gy + 40.0,
        13.0,
        Style::Caption,
        &format!("{:.1} m", cam.distance),
    );
    line(o, a.p(cx - 26.0, cy), a.p(cx - 26.0, gy), a.d(1.2), ASH);
    let hl = format!("{:.1} m", cam.height);
    let hw = text_width(a, 13.0, Style::Caption, &hl);
    text(
        o,
        a,
        cx - 34.0 - hw,
        0.5 * (cy + gy) + 4.0,
        13.0,
        Style::Caption,
        &hl,
    );
    text(
        o,
        a,
        cx + 30.0,
        cy - 18.0,
        13.0,
        Style::Accent,
        &format!("FOV {:.0}°", cam.fov),
    );
    // The HUD preview.
    let hud = &v.settings.prefs.hud;
    let hs = f32::from(hud.scale) / 100.0;
    text(
        o,
        a,
        24.0,
        246.0,
        13.0,
        Style::Accent,
        &format!("HUD preview {}%", hud.scale),
    );
    let (hx, hy, hw, hh) = (24.0, 256.0, 224.0, 126.0);
    let [bx, by] = a.p(hx, hy);
    o.rect(bx, by, a.d(hw), a.d(hh), [0.06, 0.04, 0.03, 0.35]);
    outline(o, bx, by, a.d(hw), a.d(hh), a.d(1.2), ASH);
    text(
        o,
        a,
        hx + 8.0,
        hy + 6.0 + 20.0 * hs,
        20.0 * hs,
        Style::Display,
        "3RD",
    );
    if hud.minimap
        && let Some(r) = v.routes.get(v.settings.track % TRACKS.len())
    {
        let size = 34.0 * hs;
        let points = route_points(a, r, (hx + 6.0, hy + hh - 6.0 - size, size * 1.4, size));
        polyline(o, &points, a.d(1.6), EMBER, true);
    }
    if hud.speedometer {
        let units = match hud.units {
            crate::prefs::Units::Kmh => ("KM/H", "136"),
            crate::prefs::Units::Mph => ("MPH", "85"),
        };
        let uw = text_width(a, 9.0 * hs, Style::Caption, units.0);
        let ux = hx + hw - 6.0 - uw;
        text(o, a, ux, hy + hh - 8.0, 9.0 * hs, Style::Caption, units.0);
        let sw = text_width(a, 22.0 * hs, Style::Display, units.1);
        text(
            o,
            a,
            ux - 4.0 - sw,
            hy + hh - 8.0,
            22.0 * hs,
            Style::Display,
            units.1,
        );
    }
    if hud.rival_names {
        text_centered(
            o,
            a,
            hx + 112.0,
            hy + 52.0,
            10.0 * hs,
            Style::Accent,
            "Volt",
        );
        polyline(
            o,
            &[
                a.p(hx + 104.0, hy + 58.0),
                a.p(hx + 112.0, hy + 64.0),
                a.p(hx + 120.0, hy + 58.0),
            ],
            a.d(1.2),
            ASH,
            false,
        );
    }
}

/// A closing line in big type.
fn farewell(o: &mut Overlay, a: Area, big: &str, small: &str) {
    frame_label(o, a, "Exit");
    display_fit(o, a, 20.0, 200.0, 56.0, 540.0, big);
    if !small.is_empty() {
        text(o, a, 22.0, 236.0, 13.0, Style::Caption, small);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swatches_hit_where_they_draw() {
        let (w, h) = (1920.0, 1080.0);
        let rects = swatch_rects(w, h);
        assert_eq!(rects.len(), crate::kart::FINISHES.len());
        for (i, (x, y, sw, sh)) in rects.iter().enumerate() {
            assert_eq!(garage_swatch_at(w, h, x + sw * 0.5, y + sh * 0.5), Some(i));
        }
        let (x, y, _, _) = rects[0];
        assert_eq!(garage_swatch_at(w, h, x - 2.0, y + 2.0), None);
        // The swatches sit right of the menu column.
        assert!(rects[0].0 > 620.0 * 1.5);
    }

    #[test]
    fn pad_outline_is_closed() {
        let p = pad_outline();
        let (first, last) = (p[0], p[p.len() - 1]);
        assert!((first[0] - last[0]).abs() < 1e-3 && (first[1] - last[1]).abs() < 1e-3);
    }

    #[test]
    fn every_bindable_key_is_on_the_keyboard() {
        let on_board = |k: KeyCode| KEYBOARD.iter().any(|r| r.iter().any(|c| c.0 == Some(k)));
        for (code, _, _) in crate::bindings::KEY_NAMES {
            assert!(on_board(*code), "{code:?} missing from the diagram");
        }
    }
}
