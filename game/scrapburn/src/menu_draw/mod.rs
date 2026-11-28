//! Crucible menus: the glass column with its molten edge, the header, tabs, rows and footer,
//! the embers, and the preview on the right. Geometry is the design mockup's at 1280x720,
//! scaled by `menu_scale`.

mod previews;

use std::ops::Range;

use glam::Vec3;
use rivet_render::{Face, Overlay, label_width_face};

use crate::circuit::Circuit;
use crate::embers::Embers;
use crate::hud::PadGlyph;
use crate::menu::{MenuState, Row, RowKind, Screen, Settings, Tab};
use crate::style::crucible::{ASH, ASH_DIM, CHAR, EMBER, GLOW, GOLD, HOT, MOLTEN, fade, molten};

pub use previews::{garage_swatch_at, swatch_rects};

/// The design size every menu measurement refers to.
const DESIGN_W: f32 = 1280.0;
const DESIGN_H: f32 = 720.0;

/// Design pixels to screen pixels: 1 at 1280x720, larger for bigger windows.
pub fn menu_scale(w: f32, h: f32) -> f32 {
    (w / DESIGN_W).min(h / DESIGN_H).max(0.5)
}

/// Label scale (as `Overlay::label`) for text `px` design pixels tall at scale `s`.
fn px(px: f32, s: f32) -> f32 {
    px * s / 12.0
}

// --- Column ---
/// Glass width at the top; the bottom edge ends at `CUT` of it.
const COLUMN_W: f32 = 620.0;
const CUT: f32 = 0.86;
/// How much wider the molten edge is than the glass.
const EDGE: f32 = 8.0;

// --- Header ---
const HEADER_X: f32 = 56.0;
const KICKER_CY: f32 = 55.0;
const TITLE_PX: f32 = 112.0;
const SUB_TITLE_PX: f32 = 86.0;
const TITLE_CY: f32 = 116.0;
const SUB_TITLE_CY: f32 = 106.0;
const HELP_CY: f32 = 187.0;
const SUB_HELP_CY: f32 = 164.0;
const HELP_PX: f32 = 16.0;
const HELP_LINE: f32 = 24.0;
const HELP_WIDTH: f32 = 430.0;
const TITLE_MAX_W: f32 = 520.0;

// --- Tabs ---
const TABS_CY: f32 = 231.0;
const TAB_PX: f32 = 12.0;
const TAB_GAP: f32 = 18.0;

// --- Rows ---
const ROWS_X: f32 = 40.0;
const ROWS_W: f32 = 470.0;
const ROW_PAD: f32 = 16.0;
/// The selection's bottom-right corner sits at this fraction of the row width.
const SLANT: f32 = 0.97;
const BAR_W: f32 = 130.0;

// --- Footer ---
const FOOTER_BOTTOM: f32 = 44.0;
const FOOTER_PX: f32 = 13.0;

/// How a screen's rows are set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Density {
    /// The title screen: rows start lower.
    Title,
    Normal,
    /// More than `NORMAL_MAX` rows: shorter rows, smaller text, from higher up.
    Dense,
}

const NORMAL_MAX: usize = 7;
const DENSE_MAX: usize = 10;

impl Density {
    pub fn of(screen: Screen, rows: usize) -> Self {
        if rows > NORMAL_MAX {
            Density::Dense
        } else if screen == Screen::Title {
            Density::Title
        } else {
            Density::Normal
        }
    }

    fn top(self) -> f32 {
        match self {
            Density::Title => 330.0,
            Density::Normal => 300.0,
            Density::Dense => 262.0,
        }
    }

    fn height(self) -> f32 {
        if self == Density::Dense { 34.0 } else { 48.0 }
    }

    fn pitch(self) -> f32 {
        if self == Density::Dense { 34.0 } else { 52.0 }
    }

    fn text_px(self) -> f32 {
        if self == Density::Dense { 15.0 } else { 20.0 }
    }

    fn max_visible(self) -> usize {
        if self == Density::Dense {
            DENSE_MAX
        } else {
            NORMAL_MAX
        }
    }
}

/// Where the rows of a screen sit, in screen pixels, and which of them are shown.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowLayout {
    pub x: f32,
    pub top: f32,
    pub width: f32,
    /// Height of one row's box.
    pub height: f32,
    /// Distance from one row's top to the next.
    pub pitch: f32,
    /// Index of the first row shown.
    pub first: usize,
    pub visible: usize,
}

impl RowLayout {
    pub fn new(s: f32, density: Density, rows: usize, selected: usize) -> Self {
        let visible = rows.min(density.max_visible());
        let first = (selected + 1)
            .saturating_sub(visible)
            .min(rows.saturating_sub(visible));
        Self {
            x: ROWS_X * s,
            top: density.top() * s,
            width: ROWS_W * s,
            height: density.height() * s,
            pitch: density.pitch() * s,
            first,
            visible,
        }
    }

    /// Top of the `j`th shown row.
    fn row_top(&self, j: usize) -> f32 {
        self.top + j as f32 * self.pitch
    }

    /// The row under the pointer.
    pub fn row_at(&self, x: f32, y: f32) -> Option<usize> {
        let j = ((y - self.top) / self.pitch).floor();
        let inside =
            (self.x..self.x + self.width).contains(&x) && j >= 0.0 && (j as usize) < self.visible;
        inside.then(|| self.first + j as usize)
    }
}

/// A click in the left 45% of a value area steps the value down; the rest steps it up.
pub fn decrease_zone(left: f32, right: f32) -> Range<f32> {
    left..left + 0.45 * (right - left)
}

/// What the pointer can hit on the menu drawn this frame.
#[derive(Debug, Clone, Default)]
pub struct MenuHits {
    rows: Option<RowLayout>,
    /// Per shown row: the x range that steps its value down.
    decrease: Vec<Option<Range<f32>>>,
    /// Settings tabs: x range, y range.
    tabs: Vec<(Range<f32>, Range<f32>, Tab)>,
}

impl MenuHits {
    pub fn row_at(&self, x: f32, y: f32) -> Option<usize> {
        self.rows.and_then(|r| r.row_at(x, y))
    }

    /// Whether a click at `x` on `row` steps its value down rather than selecting it.
    pub fn decrease_at(&self, row: usize, x: f32) -> bool {
        let first = self.rows.map_or(0, |r| r.first);
        self.decrease
            .get(row.wrapping_sub(first))
            .and_then(Option::as_ref)
            .is_some_and(|r| r.contains(&x))
    }

    pub fn tab_at(&self, x: f32, y: f32) -> Option<Tab> {
        self.tabs
            .iter()
            .find(|(xs, ys, _)| xs.contains(&x) && ys.contains(&y))
            .map(|(_, _, t)| *t)
    }
}

/// Race status for the pause preview.
pub struct PauseInfo {
    pub position: usize,
    pub racers: usize,
    /// The lap being driven (1-based) and the race length, if it has one.
    pub lap: u32,
    pub laps: Option<u32>,
    pub best: Option<f64>,
    pub track_name: String,
    pub route: Vec<Vec3>,
    /// Round and rounds, in a circuit.
    pub circuit: Option<(usize, usize)>,
}

/// One line of the results table.
#[derive(Debug, Clone, PartialEq)]
pub struct ResultRow {
    pub name: String,
    pub time: Option<f64>,
    /// Place in this race, 1-based.
    pub place: usize,
    /// Points this round and the circuit total.
    pub points: Option<(u8, u8)>,
    pub me: bool,
}

/// The results table: race order for a single race; circuit order (most points first, ties
/// to the better place in this race) in a circuit. `racer(seat, place)` gives a seat's name
/// and finishing time, `place` being 0-based in this race.
pub fn results_rows(
    standings: &[usize],
    circuit: Option<&Circuit>,
    focus: usize,
    mut racer: impl FnMut(usize, usize) -> (String, Option<f64>),
) -> Vec<ResultRow> {
    let place = |seat: usize| standings.iter().position(|&s| s == seat);
    let totals = circuit.map(|c| (c.totals(standings), c));
    let mut order = match &totals {
        Some((t, _)) => crate::circuit::table(t, standings, standings.len()),
        None => Vec::with_capacity(standings.len()),
    };
    // Anyone the points table has no seat for follows in race order.
    for &seat in standings {
        if !order.contains(&seat) {
            order.push(seat);
        }
    }
    order
        .into_iter()
        .filter_map(|seat| {
            let k = place(seat)?;
            let (name, time) = racer(seat, k);
            Some(ResultRow {
                name,
                time,
                place: k + 1,
                points: totals.as_ref().and_then(|(t, c)| {
                    let (total, before) = (*t.get(seat)?, *c.points.get(seat)?);
                    Some((total - before, total))
                }),
                me: seat == focus,
            })
        })
        .collect()
}

pub struct ResultsInfo {
    pub rows: Vec<ResultRow>,
    /// The player's finishing place.
    pub finished: Option<u32>,
    pub circuit: Option<(usize, usize)>,
    /// The circuit leader, once the last round is run.
    pub champion: Option<String>,
}

/// Screen data only the app has.
pub enum Extra {
    None,
    /// Online screen status lines.
    Online(Vec<String>),
    /// Lobby status lines and the drivers on the grid.
    Lobby {
        lines: Vec<String>,
        names: Vec<String>,
        slots: usize,
    },
    Pause(PauseInfo),
    Results(ResultsInfo),
}

/// Everything one menu frame draws from.
pub struct MenuView<'a> {
    pub menu: &'a MenuState,
    pub rows: &'a [Row],
    pub settings: &'a Settings,
    /// Wall seconds, for animation.
    pub time: f32,
    /// False with reduced motion: nothing animates.
    pub motion: bool,
    /// The last input came from a gamepad: prompts show its buttons.
    pub pad: bool,
    pub embers: &'a Embers,
    /// Plan views of each track (`track::route_preview`).
    pub routes: &'a [Vec<Vec3>],
    pub player_name: &'a str,
    pub extra: Extra,
}

impl MenuView<'_> {
    /// Animation time: frozen at zero with reduced motion.
    fn t(&self) -> f32 {
        if self.motion { self.time } else { 0.0 }
    }

    fn selected_row(&self) -> Option<&Row> {
        self.rows.get(self.menu.selected)
    }
}

/// The kicker (small line above the title) and the title of a screen.
fn header(v: &MenuView) -> (String, String) {
    let (kicker, title): (String, &str) = match v.menu.screen {
        Screen::Title => (
            concat!("Main menu  /  v", env!("CARGO_PKG_VERSION")).into(),
            "Scrapburn",
        ),
        Screen::RaceSetup => ("Solo race".into(), "Race setup"),
        Screen::Circuits => ("Track select".into(), "Tracks"),
        Screen::Garage => ("Motor pool".into(), "Garage"),
        Screen::Online => ("Race network".into(), "Multiplayer"),
        Screen::Tailscale => ("Multiplayer / tailnet".into(), "Tailscale"),
        Screen::DriverName => ("Multiplayer / profile".into(), "Racer name"),
        Screen::Lobby => ("Multiplayer".into(), "Race lobby"),
        Screen::Settings(tab) => ("Settings".into(), tab.name()),
        Screen::Paused => ("Race menu".into(), "Paused"),
        Screen::Results => {
            let kicker = match &v.extra {
                Extra::Results(r) => match (r.circuit, r.finished) {
                    (Some(_), _) if r.champion.is_some() => {
                        format!("Circuit champion: {}", r.champion.as_deref().unwrap_or(""))
                    }
                    (Some((round, rounds)), _) => format!("Circuit round {round} of {rounds}"),
                    (None, Some(p)) => format!("You finished {}", crate::hud::ordinal(p as usize)),
                    (None, None) => "Race over".into(),
                },
                _ => "Race over".into(),
            };
            (kicker, "Results")
        }
        Screen::Playing => (String::new(), ""),
    };
    (kicker, title.to_string())
}

/// Draws the whole menu and returns what the pointer can hit.
pub fn draw(o: &mut Overlay, w: f32, h: f32, v: &MenuView) -> MenuHits {
    let s = menu_scale(w, h);
    let t = v.t();
    draw_column(o, w, h, s, t);
    draw_embers(o, w, h, v.embers);
    let mut hits = MenuHits::default();
    let screen = v.menu.screen;
    let title_screen = screen == Screen::Title;
    let (kicker, title) = header(v);
    tracked(
        o,
        Face::Row,
        HEADER_X * s,
        KICKER_CY * s,
        13.0 * s,
        0.3,
        GOLD,
        &kicker.to_uppercase(),
    );
    let (title_px, title_cy, help_cy) = if title_screen {
        (TITLE_PX, TITLE_CY, HELP_CY)
    } else {
        (SUB_TITLE_PX, SUB_TITLE_CY, SUB_HELP_CY)
    };
    let title = title.to_uppercase();
    let mut tscale = px(title_px, s);
    let tw = label_width_face(Face::Display, &title, tscale);
    if tw > TITLE_MAX_W * s {
        tscale *= TITLE_MAX_W * s / tw;
    }
    // A soft ember glow behind the title, then the stencil letters from gold to ember.
    let glow_w = label_width_face(Face::Display, &title, tscale);
    o.glow_rect(
        HEADER_X * s + 0.1 * glow_w,
        title_cy * s - 0.15 * title_px * s,
        0.8 * glow_w,
        0.3 * title_px * s,
        26.0 * s,
        fade(GLOW, 0.22),
    );
    molten_title(
        o,
        HEADER_X * s,
        title_cy * s,
        tscale,
        TITLE_MAX_W * s,
        &title,
    );
    let help = if v.menu.capturing.is_some() {
        if v.menu.gamepad_guide {
            "Press a button. Start cancels."
        } else {
            "Press a key. Esc cancels."
        }
    } else {
        v.selected_row().map_or("", |r| r.help.as_str())
    };
    for (i, line) in wrap(Face::Row, help, px(HELP_PX, s), HELP_WIDTH * s)
        .iter()
        .take(2)
        .enumerate()
    {
        o.label_face_centered(
            Face::Row,
            HEADER_X * s,
            (help_cy + i as f32 * HELP_LINE) * s,
            px(HELP_PX, s),
            ASH,
            line,
        );
    }
    if let Screen::Settings(tab) = screen {
        hits.tabs = draw_tabs(o, s, tab);
    }
    let density = Density::of(screen, v.rows.len());
    let layout = RowLayout::new(s, density, v.rows.len(), v.menu.selected);
    hits.decrease = draw_rows(o, s, &layout, density, v);
    hits.rows = Some(layout);
    draw_more_hint(o, s, &layout, v.rows.len());
    draw_footer(o, h, s, v);
    previews::draw(o, w, h, s, v);
    hits
}

/// Stencil letters in the molten gradient, which runs left to right across `span` pixels
/// (so short titles stay red and orange, as in the mockup), each letter a touch lighter at
/// the top.
fn molten_title(o: &mut Overlay, x: f32, cy: f32, scale: f32, span: f32, title: &str) {
    let mut buf = [0u8; 4];
    let mut cx = x;
    for c in title.chars() {
        let g = c.encode_utf8(&mut buf);
        let gw = label_width_face(Face::Display, g, scale);
        let t = (cx + 0.5 * gw - x) / span;
        let bottom = molten(t);
        let top = std::array::from_fn(|i| bottom[i] + (HOT[i] - bottom[i]) * 0.3);
        o.label_gradient_centered(Face::Display, cx, cy, scale, top, bottom, g);
        cx += gw;
    }
}

/// The molten edge, the smoke glass over the blurred scene, and the ember glow low down.
fn draw_column(o: &mut Overlay, w: f32, h: f32, s: f32, t: f32) {
    let top = COLUMN_W * s;
    // The scene on the right is darkened toward the column and the edges (the mockup's tint
    // and vignette), so previews read over bright tracks.
    let smoke = |a: f32| [14.0 / 255.0, 8.0 / 255.0, 6.0 / 255.0, a];
    o.quad(
        [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]],
        [smoke(0.62), smoke(0.32), smoke(0.32), smoke(0.62)],
    );
    let band = 0.28 * h;
    o.quad(
        [[0.0, 0.0], [w, 0.0], [w, band], [0.0, band]],
        [smoke(0.45), smoke(0.45), smoke(0.0), smoke(0.0)],
    );
    o.quad(
        [[0.0, h - band], [w, h - band], [w, h], [0.0, h]],
        [smoke(0.0), smoke(0.0), smoke(0.55), smoke(0.55)],
    );
    let side = 0.18 * w;
    o.quad(
        [[w - side, 0.0], [w, 0.0], [w, h], [w - side, h]],
        [smoke(0.0), smoke(0.4), smoke(0.4), smoke(0.0)],
    );
    let edge_top = (COLUMN_W + EDGE) * s;
    let x_at = |top_w: f32, y: f32| top_w - (1.0 - CUT) * top_w * (y / h);
    // A wide soft glow off the molten edge into the scene.
    let glow = 26.0 * s;
    o.quad(
        [
            [x_at(edge_top, 0.0), 0.0],
            [x_at(edge_top, 0.0) + glow, 0.0],
            [x_at(edge_top, h) + glow, h],
            [x_at(edge_top, h), h],
        ],
        [
            fade(GLOW, 0.5),
            fade(GLOW, 0.0),
            fade(GLOW, 0.0),
            fade(GLOW, 0.9),
        ],
    );
    // The edge: white heat at the top through gold and orange to red at the bottom. A
    // slow pulse runs down it unless motion is off.
    let stops = [
        (0.0, MOLTEN[3].0),
        (0.25, MOLTEN[2].0),
        (0.6, MOLTEN[1].0),
        (1.0, MOLTEN[0].0),
    ];
    for pair in stops.windows(2) {
        let ((ya, ca), (yb, cb)) = (pair[0], pair[1]);
        let (y0, y1) = (ya * h, yb * h);
        o.quad(
            [
                [0.0, y0],
                [x_at(edge_top, y0), y0],
                [x_at(edge_top, y1), y1],
                [0.0, y1],
            ],
            [ca, ca, cb, cb],
        );
    }
    if t > 0.0 {
        let band = 90.0 * s;
        let y = (t * 0.25).fract() * (h + band) - band;
        let c = fade(HOT, 0.55);
        let (y0, y1, y2) = (y, y + band * 0.5, y + band);
        let x = |y: f32| x_at(edge_top, y.clamp(0.0, h));
        let x_in = |y: f32| x_at(top, y.clamp(0.0, h)) - 2.0 * s;
        o.quad(
            [[x_in(y0), y0], [x(y0), y0], [x(y1), y1], [x_in(y1), y1]],
            [fade(c, 0.0), fade(c, 0.0), c, c],
        );
        o.quad(
            [[x_in(y1), y1], [x(y1), y1], [x(y2), y2], [x_in(y2), y2]],
            [c, c, fade(c, 0.0), fade(c, 0.0)],
        );
    }
    // The glass: the frosted scene, darkened and warmed.
    o.backdrop_quad(
        [[0.0, 0.0], [top, 0.0], [x_at(top, h), h], [0.0, h]],
        [0.55, 0.48, 0.42, 1.0],
    );
    let glass_top = [16.0 / 255.0, 10.0 / 255.0, 8.0 / 255.0, 0.62];
    let glass_bottom = [24.0 / 255.0, 12.0 / 255.0, 8.0 / 255.0, 0.5];
    o.quad(
        [[0.0, 0.0], [top, 0.0], [x_at(top, h), h], [0.0, h]],
        [glass_top, glass_top, glass_bottom, glass_bottom],
    );
    // Forge light rising from the lower left.
    let warm = [1.0, 0.35, 0.08, 0.0];
    let mid = 0.45 * h;
    o.quad(
        [
            [0.0, mid],
            [x_at(top, mid), mid],
            [x_at(top, h), h],
            [0.0, h],
        ],
        [
            warm,
            warm,
            fade([1.0, 0.35, 0.08, 1.0], 0.16),
            fade([1.0, 0.35, 0.08, 1.0], 0.34),
        ],
    );
}

/// The embers, scaled from the design space to the window.
fn draw_embers(o: &mut Overlay, w: f32, h: f32, embers: &Embers) {
    let (sx, sy) = (w / crate::embers::WIDTH, h / crate::embers::HEIGHT);
    let r_scale = sx.min(sy);
    for e in embers.sparks() {
        let a = e.alpha();
        let c = [
            1.0,
            (140.0 + e.heat * 80.0) / 255.0,
            (40.0 + e.heat * 40.0) / 255.0,
            a,
        ];
        let (x, y) = (e.x * sx, e.y * sy);
        let r = e.radius * r_scale;
        o.circle(x, y, r * 3.2, [1.0, 0.43, 0.12, a * 0.16]);
        o.circle(x, y, r, c);
    }
}

/// The five Settings tabs; returns their hit boxes.
fn draw_tabs(o: &mut Overlay, s: f32, current: Tab) -> Vec<(Range<f32>, Range<f32>, Tab)> {
    let mut x = HEADER_X * s;
    let cy = TABS_CY * s;
    let mut hits = Vec::new();
    for tab in Tab::ALL {
        let name = tab.name().to_uppercase();
        let on = tab == current;
        let width = tracked(
            o,
            Face::Row,
            x,
            cy,
            TAB_PX * s,
            0.16,
            if on { HOT } else { ASH_DIM },
            &name,
        );
        if on {
            o.glow_rect(x, cy + 12.0 * s, width, 2.0 * s, 6.0 * s, fade(GLOW, 0.6));
            o.rect(x, cy + 12.0 * s, width, 2.0 * s, EMBER);
        }
        let pad = 0.5 * TAB_GAP * s;
        hits.push((x - pad..x + width + pad, cy - 14.0 * s..cy + 18.0 * s, tab));
        x += width + TAB_GAP * s;
    }
    hits
}

/// The value text of a row, without arrows.
fn value_text(row: &Row) -> Option<&str> {
    match &row.kind {
        RowKind::Go => None,
        RowKind::Choice { value }
        | RowKind::Slider {
            value_text: value, ..
        }
        | RowKind::Bind { text: value }
        | RowKind::Text { value } => (!value.is_empty()).then_some(value.as_str()),
    }
}

/// Draws the shown rows; returns each one's step-down zone.
fn draw_rows(
    o: &mut Overlay,
    s: f32,
    l: &RowLayout,
    density: Density,
    v: &MenuView,
) -> Vec<Option<Range<f32>>> {
    let text = px(density.text_px(), s);
    let value_scale = px(
        density.text_px() - if density == Density::Dense { 0.0 } else { 1.0 },
        s,
    );
    let mut zones = Vec::with_capacity(l.visible);
    for j in 0..l.visible {
        let i = l.first + j;
        let row = &v.rows[i];
        let sel = i == v.menu.selected;
        let (x, y, rw, rh) = (l.x, l.row_top(j), l.width, l.height);
        let cy = y + rh * 0.5;
        if sel {
            o.glow_rect(x, y, rw * 0.985, rh, 20.0 * s, fade(GLOW, 0.3));
            molten_bar(o, x, y, rw, rh);
        }
        let fg = if sel { CHAR } else { ASH };
        let label = row.label.to_uppercase();
        let label_w = tracked(
            o,
            Face::Row,
            x + ROW_PAD * s,
            cy,
            text * 12.0,
            0.1,
            fg,
            &label,
        );
        let right = x + rw - ROW_PAD * s - if sel { (1.0 - SLANT) * rw } else { 0.0 };
        let capturing = sel && v.menu.capturing.is_some();
        let mut zone = None;
        match &row.kind {
            RowKind::Go => {
                if sel {
                    chevron(o, right - 6.0 * s, cy, 6.0 * s, 2.2 * s, true, CHAR);
                }
            }
            RowKind::Slider { fraction, .. } if !capturing => {
                let value = value_text(row).unwrap_or("").to_uppercase();
                let vw = tracked_width(Face::Row, value_scale * 12.0, 0.06, &value);
                let arrows = if sel { 22.0 * s } else { 0.0 };
                let vx = right - arrows - vw;
                tracked(o, Face::Row, vx, cy, value_scale * 12.0, 0.06, fg, &value);
                if sel {
                    chevron(o, vx - 12.0 * s, cy, 5.0 * s, 2.0 * s, false, CHAR);
                    chevron(o, right - 5.0 * s, cy, 5.0 * s, 2.0 * s, true, CHAR);
                }
                let bar_w = BAR_W * s;
                let bar_right = vx - arrows - 14.0 * s;
                let bar_x = bar_right - bar_w;
                if bar_x > x + ROW_PAD * s + label_w + 12.0 * s {
                    slider_bar(o, bar_x, cy, bar_w, s, *fraction, sel);
                }
                zone = Some(decrease_zone(bar_x.min(vx - arrows) - 8.0 * s, x + rw));
            }
            _ => {
                let value = if capturing {
                    "PRESS...".to_string()
                } else {
                    value_text(row).unwrap_or("").to_uppercase()
                };
                let choice = matches!(row.kind, RowKind::Choice { .. }) && !capturing;
                if !value.is_empty() {
                    let arrows = if sel && choice { 22.0 * s } else { 0.0 };
                    // Long values shrink to fit beside the label.
                    let room = right - 2.0 * arrows - (x + ROW_PAD * s + label_w + 20.0 * s);
                    let mut vs = value_scale;
                    let full = tracked_width(Face::Row, vs * 12.0, 0.06, &value);
                    if full > room && room > 0.0 {
                        vs *= room / full;
                    }
                    let vw = tracked_width(Face::Row, vs * 12.0, 0.06, &value);
                    let vx = right - arrows - vw;
                    let color = if capturing { EMBER } else { fg };
                    tracked(o, Face::Row, vx, cy, vs * 12.0, 0.06, color, &value);
                    if sel && choice {
                        chevron(o, vx - 12.0 * s, cy, 5.0 * s, 2.0 * s, false, CHAR);
                        chevron(o, right - 5.0 * s, cy, 5.0 * s, 2.0 * s, true, CHAR);
                    }
                    if choice {
                        zone = Some(decrease_zone(vx - arrows - 8.0 * s, x + rw));
                    }
                }
            }
        }
        zones.push(zone);
    }
    zones
}

/// The selected row: molten from red to white heat, its right end slanted.
fn molten_bar(o: &mut Overlay, x: f32, y: f32, w: f32, h: f32) {
    for pair in MOLTEN.windows(2) {
        let ((ca, ta), (cb, tb)) = (pair[0], pair[1]);
        o.quad(
            [
                [x + ta * w, y],
                [x + tb * w, y],
                [x + tb * SLANT * w, y + h],
                [x + ta * SLANT * w, y + h],
            ],
            [ca, cb, cb, ca],
        );
    }
}

/// A slider: the track, the molten fill, and the knob (all dark on the selected row).
fn slider_bar(o: &mut Overlay, x: f32, cy: f32, w: f32, s: f32, fraction: f32, sel: bool) {
    let f = fraction.clamp(0.0, 1.0);
    let t = 2.0 * s;
    let y = cy - 0.5 * t;
    if sel {
        o.rect(x, y, w, t, fade(CHAR, 0.3));
        o.rect(x, y, w * f, t, CHAR);
        o.circle(x + w * f, cy, 5.0 * s, CHAR);
    } else {
        o.rect(x, y, w, t, fade(ASH, 0.25));
        o.quad(
            [[x, y], [x + w * f, y], [x + w * f, y + t], [x, y + t]],
            [MOLTEN[0].0, molten(f), molten(f), MOLTEN[0].0],
        );
        o.circle(x + w * f, cy, 9.0 * s, fade(GLOW, 0.35));
        o.circle(x + w * f, cy, 5.0 * s, HOT);
    }
}

/// A "›" (or "‹") made of two strokes, its point at `x`.
fn chevron(o: &mut Overlay, x: f32, cy: f32, size: f32, t: f32, right: bool, c: [f32; 4]) {
    let d = if right { 1.0 } else { -1.0 };
    let back = x - d * size * 0.8;
    line(o, [back, cy - size], [x, cy], t, c);
    line(o, [x, cy], [back, cy + size], t, c);
}

/// "▲ N more / ▼ N more" under a list that scrolls.
fn draw_more_hint(o: &mut Overlay, s: f32, l: &RowLayout, rows: usize) {
    let above = l.first;
    let below = rows - l.first - l.visible;
    if above == 0 && below == 0 {
        return;
    }
    let cy = l.row_top(l.visible) + 14.0 * s;
    let mut x = HEADER_X * s;
    let tri = 4.5 * s;
    for (n, up) in [(above, true), (below, false)] {
        if n == 0 {
            continue;
        }
        let (a, b) = if up {
            (cy + tri, cy - tri)
        } else {
            (cy - tri, cy + tri)
        };
        o.triangle([[x, a], [x + 2.0 * tri, a], [x + tri, b]], GOLD);
        x += 2.0 * tri + 8.0 * s;
        x += tracked(
            o,
            Face::Row,
            x,
            cy,
            12.0 * s,
            0.2,
            GOLD,
            &format!("{n} MORE"),
        ) + 22.0 * s;
    }
}

/// Select / Back / Tab prompts in the last-used device's keys or buttons.
fn draw_footer(o: &mut Overlay, h: f32, s: f32, v: &MenuView) {
    let cy = h - FOOTER_BOTTOM * s;
    let mut x = HEADER_X * s;
    let fpx = FOOTER_PX * s;
    let mut items: Vec<(Prompt, &str)> = if v.menu.capturing.is_some() {
        vec![(Prompt::new("ESC", &[PadGlyph::Menu]), "Cancel")]
    } else {
        vec![
            (Prompt::new("ENTER", &[PadGlyph::A]), "Select"),
            (Prompt::new("ESC", &[PadGlyph::B]), "Back"),
        ]
    };
    if matches!(v.menu.screen, Screen::Settings(_)) && v.menu.capturing.is_none() {
        items.push((
            Prompt::new("Q E", &[PadGlyph::LeftBumper, PadGlyph::RightBumper]),
            "Tab",
        ));
    }
    for (prompt, word) in items {
        if v.pad {
            let size = 20.0 * s;
            for g in prompt.pad {
                g.draw(o, x, cy, size);
                x += g.width(size) + 4.0 * s;
            }
            x += 4.0 * s;
        } else {
            x += tracked(o, Face::Row, x, cy, fpx, 0.2, HOT, prompt.keys) + 8.0 * s;
        }
        x += tracked(o, Face::Row, x, cy, fpx, 0.2, ASH_DIM, &word.to_uppercase()) + 22.0 * s;
    }
}

struct Prompt {
    keys: &'static str,
    pad: &'static [PadGlyph],
}

impl Prompt {
    fn new(keys: &'static str, pad: &'static [PadGlyph]) -> Self {
        Self { keys, pad }
    }
}

// --- Drawing helpers shared with the previews ---

/// Width of `s` at `size_px` with `track_em` extra space between letters.
fn tracked_width(face: Face, size_px: f32, track_em: f32, s: &str) -> f32 {
    let scale = size_px / 12.0;
    let mut buf = [0u8; 4];
    let n = s.chars().count();
    s.chars()
        .map(|c| label_width_face(face, c.encode_utf8(&mut buf), scale))
        .sum::<f32>()
        + track_em * size_px * n.saturating_sub(1) as f32
}

/// Letter-spaced text from `x`, capitals centered on `cy`; returns its width.
#[allow(clippy::too_many_arguments)]
fn tracked(
    o: &mut Overlay,
    face: Face,
    x: f32,
    cy: f32,
    size_px: f32,
    track_em: f32,
    color: [f32; 4],
    s: &str,
) -> f32 {
    let scale = size_px / 12.0;
    if track_em == 0.0 {
        o.label_face_centered(face, x, cy, scale, color, s);
        return label_width_face(face, s, scale);
    }
    let mut buf = [0u8; 4];
    let mut cx = x;
    for c in s.chars() {
        let g = c.encode_utf8(&mut buf);
        o.label_face_centered(face, cx, cy, scale, color, g);
        cx += label_width_face(face, g, scale) + track_em * size_px;
    }
    (cx - x - track_em * size_px).max(0.0)
}

/// Greedy word wrap to `width` pixels at label `scale`.
fn wrap(face: Face, text: &str, scale: f32, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if !line.is_empty() && label_width_face(face, &candidate, scale) > width {
            lines.push(std::mem::replace(&mut line, word.to_string()));
        } else {
            line = candidate;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// A straight stroke `t` pixels thick.
fn line(o: &mut Overlay, a: [f32; 2], b: [f32; 2], t: f32, c: [f32; 4]) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = (dx * dx + dy * dy).sqrt();
    if len <= f32::EPSILON {
        return;
    }
    let (nx, ny) = (-dy / len * t * 0.5, dx / len * t * 0.5);
    o.quad(
        [
            [a[0] + nx, a[1] + ny],
            [b[0] + nx, b[1] + ny],
            [b[0] - nx, b[1] - ny],
            [a[0] - nx, a[1] - ny],
        ],
        [c; 4],
    );
}

/// Strokes through `points`, closing the loop when `closed`.
fn polyline(o: &mut Overlay, points: &[[f32; 2]], t: f32, c: [f32; 4], closed: bool) {
    for pair in points.windows(2) {
        line(o, pair[0], pair[1], t, c);
    }
    if closed && let (Some(&a), Some(&b)) = (points.last(), points.first()) {
        line(o, a, b, t, c);
    }
}

/// A circle outline.
fn ring(o: &mut Overlay, cx: f32, cy: f32, r: f32, t: f32, c: [f32; 4]) {
    let n = ((r * 0.8) as usize).clamp(16, 64);
    let points: Vec<[f32; 2]> = (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            [cx + r * a.cos(), cy + r * a.sin()]
        })
        .collect();
    polyline(o, &points, t, c, true);
}

/// A rectangle outline.
fn outline(o: &mut Overlay, x: f32, y: f32, w: f32, h: f32, t: f32, c: [f32; 4]) {
    o.rect(x, y, w, t, c);
    o.rect(x, y + h - t, w, t, c);
    o.rect(x, y + t, t, h - 2.0 * t, c);
    o.rect(x + w - t, y + t, t, h - 2.0 * t, c);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_is_one_at_the_design_size() {
        assert_eq!(menu_scale(1280.0, 720.0), 1.0);
        assert_eq!(menu_scale(1920.0, 1080.0), 1.5);
        assert_eq!(menu_scale(3840.0, 2160.0), 3.0);
        // A tall window scales by width so the column never runs off the side.
        assert_eq!(menu_scale(1280.0, 1024.0), 1.0);
    }

    #[test]
    fn density_follows_the_mockup() {
        assert_eq!(Density::of(Screen::Title, 5), Density::Title);
        assert_eq!(Density::of(Screen::RaceSetup, 7), Density::Normal);
        assert_eq!(Density::of(Screen::Settings(Tab::Audio), 8), Density::Dense);
    }

    #[test]
    fn rows_match_the_mockup_geometry() {
        let l = RowLayout::new(1.0, Density::Normal, 4, 0);
        assert_eq!(
            (l.x, l.top, l.width, l.height, l.pitch),
            (40.0, 300.0, 470.0, 48.0, 52.0)
        );
        let d = RowLayout::new(2.0, Density::Dense, 14, 0);
        assert_eq!((d.top, d.height, d.visible), (524.0, 68.0, 10));
    }

    #[test]
    fn the_window_scrolls_to_keep_the_selection_shown() {
        let n = 14;
        for selected in 0..n {
            let l = RowLayout::new(1.0, Density::Dense, n, selected);
            assert!((l.first..l.first + l.visible).contains(&selected));
            assert!(l.first + l.visible <= n);
        }
        assert_eq!(RowLayout::new(1.0, Density::Dense, n, 13).first, 4);
        assert_eq!(RowLayout::new(1.0, Density::Dense, n, 3).first, 0);
    }

    #[test]
    fn row_hit_testing() {
        let l = RowLayout::new(1.0, Density::Dense, 14, 12);
        assert_eq!(l.first, 3);
        assert_eq!(l.row_at(100.0, 262.0 + 5.0), Some(3));
        assert_eq!(l.row_at(100.0, 262.0 + 34.0 * 9.0 + 1.0), Some(12));
        assert_eq!(
            l.row_at(100.0, 262.0 + 34.0 * 10.0 + 1.0),
            None,
            "below the window"
        );
        assert_eq!(l.row_at(100.0, 250.0), None, "above the rows");
        assert_eq!(l.row_at(30.0, 270.0), None, "left of the rows");
        assert_eq!(l.row_at(520.0, 270.0), None, "right of the rows");
    }

    #[test]
    fn value_clicks_split_at_45_percent() {
        let z = decrease_zone(300.0, 500.0);
        assert!(z.contains(&300.0) && z.contains(&389.0));
        assert!(!z.contains(&391.0) && !z.contains(&299.0));
        let hits = MenuHits {
            rows: Some(RowLayout::new(1.0, Density::Normal, 3, 0)),
            decrease: vec![None, Some(z), None],
            tabs: vec![(56.0..120.0, 220.0..245.0, Tab::Audio)],
        };
        assert!(hits.decrease_at(1, 350.0));
        assert!(!hits.decrease_at(1, 450.0));
        assert!(!hits.decrease_at(0, 350.0));
        assert!(!hits.decrease_at(7, 350.0));
        assert_eq!(hits.tab_at(80.0, 230.0), Some(Tab::Audio));
        assert_eq!(hits.tab_at(80.0, 300.0), None);
    }

    fn seat_names(seat: usize, _place: usize) -> (String, Option<f64>) {
        (format!("S{seat}"), Some(seat as f64))
    }

    #[test]
    fn single_race_results_follow_the_race() {
        let rows = results_rows(&[2, 0, 1], None, 0, seat_names);
        let names: Vec<_> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["S2", "S0", "S1"]);
        assert_eq!(rows.iter().map(|r| r.place).collect::<Vec<_>>(), [1, 2, 3]);
        assert!(rows[1].me && !rows[0].me);
        assert!(rows.iter().all(|r| r.points.is_none()));
    }

    #[test]
    fn circuit_results_follow_the_points_table() {
        // Seat 1 leads on points from earlier rounds but finishes last in this race.
        let mut c = Circuit {
            round: 2,
            ..Circuit::default()
        };
        c.points[..3].copy_from_slice(&[4, 30, 10]);
        let rows = results_rows(&[0, 2, 1], Some(&c), 2, seat_names);
        let names: Vec<_> = rows.iter().map(|r| r.name.as_str()).collect();
        // Totals: seat 0 = 14, seat 1 = 36, seat 2 = 18.
        assert_eq!(names, ["S1", "S2", "S0"]);
        assert_eq!(rows.iter().map(|r| r.place).collect::<Vec<_>>(), [3, 2, 1]);
        assert_eq!(
            rows.iter().map(|r| r.points).collect::<Vec<_>>(),
            [Some((6, 36)), Some((8, 18)), Some((10, 14))]
        );
        let totals: Vec<u8> = rows.iter().filter_map(|r| r.points.map(|p| p.1)).collect();
        assert!(totals.windows(2).all(|w| w[0] >= w[1]), "totals never rise");
        assert!(rows[1].me);
    }

    #[test]
    fn the_racer_callback_gets_the_race_place() {
        let mut seen = Vec::new();
        let c = Circuit::default();
        results_rows(&[1, 0], Some(&c), 0, |seat, place| {
            seen.push((seat, place));
            seat_names(seat, place)
        });
        seen.sort_unstable();
        assert_eq!(seen, [(0, 1), (1, 0)]);
    }

    #[test]
    fn wrap_breaks_between_words() {
        let lines = wrap(Face::Row, "one two three four", 1.0, 1.0);
        assert_eq!(lines, ["one", "two", "three", "four"]);
        assert_eq!(wrap(Face::Row, "one two", 1.0, 1.0e6), ["one two"]);
        assert!(wrap(Face::Row, "", 1.0, 100.0).is_empty());
    }
}
