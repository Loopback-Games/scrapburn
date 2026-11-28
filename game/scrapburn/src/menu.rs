//! Menus: title, settings, pause, results. Pure state and navigation; drawing lives in hud.rs.

use rivet_audio::Theme;
use rivet_render::{Preset, QualitySettings};

use crate::bindings::{Action, BindRow, Press};
use crate::prefs::{
    DisplayMode, FRAME_CAPS, Graphics, LAP_CHOICES, PresetChoice, RIVAL_CHOICES, Units,
};

/// A page of the Settings screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Graphics,
    Audio,
    Controls,
    Camera,
    Gameplay,
}

impl Tab {
    pub const ALL: [Tab; 5] = [
        Tab::Graphics,
        Tab::Audio,
        Tab::Controls,
        Tab::Camera,
        Tab::Gameplay,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Tab::Graphics => "Graphics",
            Tab::Audio => "Audio",
            Tab::Controls => "Controls",
            Tab::Camera => "Camera & view",
            Tab::Gameplay => "Gameplay",
        }
    }

    /// The tab `dir` steps away, wrapping across all five.
    pub fn cycled(self, dir: i32) -> Tab {
        cycle(&Self::ALL, self, dir)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Title,
    RaceSetup,
    Circuits,
    DriverName,
    Garage,
    /// Settings, one tab at a time.
    Settings(Tab),
    Playing,
    Paused,
    Results,
    /// Host or join an online game.
    Online,
    /// Waiting for the host to start.
    Lobby,
    /// Online Tailscale peers to join.
    Tailscale,
}

/// Edge-triggered menu input for one frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MenuInput {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub select: bool,
    pub back: bool,
    /// Previous and next settings tab: Q / E, or LB / RB.
    pub tab_prev: bool,
    pub tab_next: bool,
}

/// What the app should do in response to a menu input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    None,
    StartRace,
    SaveGarage,
    SaveName,
    StartTimeTrial,
    Resume,
    Restart,
    ToTitle,
    Quit,
    /// Start an elimination race.
    StartElimination,
    /// A named graphics preset was picked; its quality values are already in `quality`.
    PresetChanged,
    /// One graphics option changed (the preset is now `Custom`); the app rebuilds the
    /// renderer, debounced.
    GraphicsChanged,
    /// Windowed, borderless, or fullscreen changed.
    DisplayChanged,
    VsyncChanged,
    FrameCapChanged,
    /// A volume slider moved.
    VolumeChanged,
    /// The Audio tab's "Listen to" row picked a song to preview.
    PreviewSong(Theme),
    /// A Controls tab binding was captured; it is already in `prefs.controls.bindings`.
    BindingCaptured,
    /// A different track was picked on the title screen.
    TrackChanged,
    /// Host an online game.
    HostGame,
    PrivateHost,
    PrivateJoin,
    /// Join the game at `MenuState::join_target`.
    JoinGame,
    /// The host starts the online race.
    LobbyStart,
    /// Leave the online game (back to the title).
    LeaveOnline,
    /// Run the next round of the circuit.
    NextRace,
    /// Copy the private-race invite to the clipboard again.
    CopyInvite,
    /// Download and install the release in `MenuState::update`, then restart.
    Update,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Settings {
    pub loadout: crate::kart::Loadout,
    /// Every saved option; see `prefs`.
    pub prefs: crate::prefs::Prefs,
    /// Index into `track::TRACKS`.
    pub track: usize,
}

impl Settings {
    /// The graphics preset in effect (`Custom` counts as Medium).
    pub fn preset(&self) -> Preset {
        self.prefs.graphics.preset.preset()
    }
}

/// How frame rate is shown in game (F3 / Select cycles it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FpsDisplay {
    #[default]
    Off,
    /// A small FPS readout.
    Counter,
    /// The full CPU/GPU frame-time graph.
    Graph,
}

impl FpsDisplay {
    pub fn next(self) -> Self {
        match self {
            FpsDisplay::Off => FpsDisplay::Counter,
            FpsDisplay::Counter => FpsDisplay::Graph,
            FpsDisplay::Graph => FpsDisplay::Off,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            FpsDisplay::Off => "OFF",
            FpsDisplay::Counter => "COUNTER",
            FpsDisplay::Graph => "GRAPH",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RaceMode {
    #[default]
    Race,
    TimeTrial,
    Elimination,
}
impl RaceMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Race => "GRAND PRIX",
            Self::TimeTrial => "TIME TRIAL",
            Self::Elimination => "ELIMINATION",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Race => "Battle the pack to the finish line.",
            Self::TimeTrial => "Chase your own best lap.",
            Self::Elimination => "Stay out of last place to survive.",
        }
    }
}

/// What a menu row is, independent of where it sits on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowId {
    /// Back, Done, Cancel, or Leave: closes the screen.
    Back,
    Quit,
    /// Opens Settings.
    Settings,
    StartRace,
    Mode,
    Event,
    Track,
    Laps,
    Rivals,
    /// Opens the racer-name editor.
    RacerName,
    SoloRace,
    Multiplayer,
    Garage,
    Update,
    Resume,
    Restart,
    MainMenu,
    NextRace,
    QuitCircuit,
    Continue,
    RaceAgain,
    /// A track in the browser; index into `track::TRACKS`.
    PickTrack(usize),
    KartType,
    Chassis,
    Finish,
    NameField,
    SaveName,
    HostPrivate,
    PasteInvite,
    HostGame,
    JoinAddress,
    Tailscale,
    /// A tailnet peer; index into `MenuState::peers`.
    Peer(usize),
    CopyInvite,
    Waiting,
    Preset,
    AutoPreset,
    RenderScale,
    Shadows,
    Bloom,
    Fxaa,
    Textures,
    Anisotropy,
    DisplayMode,
    Vsync,
    FrameCap,
    ShowFps,
    MasterVolume,
    MusicVolume,
    EffectsVolume,
    EnginesVolume,
    MenusVolume,
    Soundtrack,
    ListenTo,
    Device,
    Bind(BindRow),
    Sensitivity,
    Deadzone,
    Vibration,
    CameraDistance,
    CameraHeight,
    Fov,
    WidenAtSpeed,
    Shake,
    HudScale,
    Minimap,
    Speedometer,
    SpeedUnits,
    RivalNames,
}

/// How a row shows its value.
#[derive(Debug, Clone, PartialEq)]
pub enum RowKind {
    /// Opens a screen or does something.
    Go,
    /// Left and right cycle through options.
    Choice { value: String },
    /// Left and right step a number; `fraction` (0-1) is how full the bar is.
    Slider { value_text: String, fraction: f32 },
    /// The keys or buttons bound to a control.
    Bind { text: String },
    /// A value shown beside the label (a typed field, a count, the equipped track).
    Text { value: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: RowId,
    pub label: String,
    pub kind: RowKind,
    /// One line describing the row, shown while it is selected.
    pub help: String,
    /// Added by the menu redesign (the mockup's NEW badge); not drawn in game.
    pub new: bool,
}

impl Row {
    fn new(id: RowId, label: impl Into<String>, kind: RowKind, help: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            kind,
            help: help.into(),
            new: false,
        }
    }

    fn go(id: RowId, label: impl Into<String>, help: impl Into<String>) -> Self {
        Self::new(id, label, RowKind::Go, help)
    }

    fn choice(id: RowId, label: &str, value: impl Into<String>, help: impl Into<String>) -> Self {
        let value = value.into();
        Self::new(id, label, RowKind::Choice { value }, help)
    }

    fn toggle(id: RowId, label: &str, on: bool, help: &str) -> Self {
        Self::choice(id, label, if on { "On" } else { "Off" }, help)
    }

    fn slider(id: RowId, label: &str, value_text: String, fraction: f32, help: &str) -> Self {
        Self::new(
            id,
            label,
            RowKind::Slider {
                value_text,
                fraction,
            },
            help,
        )
    }

    fn text(id: RowId, label: impl Into<String>, value: impl Into<String>, help: &str) -> Self {
        let value = value.into();
        Self::new(id, label, RowKind::Text { value }, help)
    }

    /// Marks a row the menu redesign added.
    fn fresh(mut self) -> Self {
        self.new = true;
        self
    }

    /// The old `"LABEL  < VALUE >"` string `hud::draw_menu` parses. Temporary: Task 5 draws
    /// rows directly and deletes this.
    pub fn legacy_string(&self) -> String {
        let label = self.label.to_ascii_uppercase();
        match &self.kind {
            RowKind::Go => label,
            RowKind::Choice { value }
            | RowKind::Slider {
                value_text: value, ..
            } => format!("{label}  < {} >", value.to_ascii_uppercase()),
            RowKind::Bind { text: value } | RowKind::Text { value } if !value.is_empty() => {
                format!("{label}  {}", value.to_ascii_uppercase())
            }
            RowKind::Bind { .. } | RowKind::Text { .. } => label,
        }
    }
}

/// Help for the steering rows on the gamepad device, where Enter changes nothing.
const STEER_PAD_HELP: &str = "On a gamepad, steering is always the left stick.";

/// Online screen: the typed address row.
pub const ONLINE_ADDRESS: usize = 4;
/// Online screen: the row that opens the Tailscale peer list.
pub const ONLINE_TAILSCALE: usize = 5;
/// Longest typed address.
const MAX_ADDRESS: usize = 64;

/// Songs the Audio tab can preview.
pub const SONGS: [(Theme, &str); 6] = [
    (Theme::Menu, "Menu"),
    (Theme::Dust, "Dustbowl"),
    (Theme::Castle, "Grindstone"),
    (Theme::Sky, "Skyway"),
    (Theme::Glass, "Glass Orchard"),
    (Theme::Solar, "Helios"),
];

const PRESETS: [(PresetChoice, &str); 4] = [
    (PresetChoice::Low, "Low"),
    (PresetChoice::Medium, "Medium"),
    (PresetChoice::High, "High"),
    (PresetChoice::Custom, "Custom"),
];
const SHADOWS: [(u32, &str); 3] = [(1024, "1024"), (2048, "2048"), (4096, "4096")];
const BLOOM: [(u32, &str); 4] = [(0, "Off"), (4, "4 mips"), (5, "5 mips"), (6, "6 mips")];
const TEXTURES: [(u32, &str); 2] = [(1, "Half"), (0, "Full")];
const ANISOTROPY: [(u16, &str); 5] = [(1, "Off"), (2, "2x"), (4, "4x"), (8, "8x"), (16, "16x")];
const DISPLAYS: [(DisplayMode, &str); 3] = [
    (DisplayMode::Windowed, "Windowed"),
    (DisplayMode::Borderless, "Borderless"),
    (DisplayMode::Fullscreen, "Fullscreen"),
];
const FPS: [(FpsDisplay, &str); 3] = [
    (FpsDisplay::Off, "Off"),
    (FpsDisplay::Counter, "Counter"),
    (FpsDisplay::Graph, "Graph"),
];
const UNITS: [(Units, &str); 2] = [(Units::Kmh, "km/h"), (Units::Mph, "mph")];

/// The display name of `v` in an option table; "Custom" for a value the menu doesn't offer
/// (only a hand-edited settings file has those).
fn name_of<T: PartialEq + Copy>(table: &[(T, &'static str)], v: T) -> &'static str {
    table
        .iter()
        .find(|(k, _)| *k == v)
        .map_or("Custom", |(_, name)| name)
}

/// The option `dir` steps from `v` in an option table, wrapping.
fn cycle_in<T: PartialEq + Copy>(table: &[(T, &str)], v: T, dir: i32) -> T {
    let keys: Vec<T> = table.iter().map(|(k, _)| *k).collect();
    cycle(&keys, v, dir)
}

fn cycle<T: PartialEq + Copy>(choices: &[T], current: T, dir: i32) -> T {
    let i = choices.iter().position(|c| *c == current).unwrap_or(0) as i32;
    choices[(i + dir).rem_euclid(choices.len() as i32) as usize]
}

/// A slider's range and step.
#[derive(Debug, Clone, Copy)]
struct Range {
    min: f32,
    max: f32,
    step: f32,
}

const PERCENT: Range = Range {
    min: 0.,
    max: 100.,
    step: 5.,
};
const RENDER_SCALE: Range = Range {
    min: 0.5,
    max: 1.0,
    step: 0.05,
};
const DEADZONE: Range = Range {
    min: 0.,
    max: 30.,
    step: 1.,
};
const CAMERA_DISTANCE: Range = Range {
    min: 3.,
    max: 7.,
    step: 0.5,
};
const CAMERA_HEIGHT: Range = Range {
    min: 1.,
    max: 3.,
    step: 0.2,
};
const FOV: Range = Range {
    min: 55.,
    max: 90.,
    step: 1.,
};
const SHAKE: Range = Range {
    min: 0.,
    max: 100.,
    step: 10.,
};
const HUD_SCALE: Range = Range {
    min: 70.,
    max: 130.,
    step: 5.,
};

impl Range {
    fn fraction(self, v: f32) -> f32 {
        ((v - self.min) / (self.max - self.min)).clamp(0., 1.)
    }

    /// The next point on the step grid from `v` toward `dir`, clamped to the range. A value
    /// between grid points (an old default such as 4.15 m) moves to its neighbor.
    fn stepped(self, v: f32, dir: i32) -> f32 {
        let k = (v - self.min) / self.step;
        let k = match dir.signum() {
            1 => (k + 1e-3).floor() + 1.,
            -1 => (k - 1e-3).ceil() - 1.,
            _ => return v,
        };
        // Round off float noise so the settings file reads 1.8, not 1.8000001.
        (((self.min + k * self.step) * 100.).round() / 100.).clamp(self.min, self.max)
    }

    fn stepped_u8(self, v: u8, dir: i32) -> u8 {
        self.stepped(f32::from(v), dir).round() as u8
    }
}

fn percent_row(id: RowId, label: &str, v: u8, range: Range, help: &str) -> Row {
    Row::slider(
        id,
        label,
        format!("{v}%"),
        range.fraction(f32::from(v)),
        help,
    )
}

/// Write `v` to `slot`, returning `action` only when the value changed.
fn set<T: PartialEq>(slot: &mut T, v: T, action: MenuAction) -> MenuAction {
    if *slot == v {
        MenuAction::None
    } else {
        *slot = v;
        action
    }
}

/// Turn a toggle over; it always changes.
fn flip(slot: &mut bool, action: MenuAction) -> MenuAction {
    *slot = !*slot;
    action
}

/// Change one quality option. A real change makes the preset `Custom` and turns off auto.
fn set_quality(g: &mut Graphics, edit: impl FnOnce(&mut QualitySettings)) -> MenuAction {
    let mut q = g.quality;
    edit(&mut q);
    if q == g.quality {
        return MenuAction::None;
    }
    g.quality = q;
    g.preset = PresetChoice::Custom;
    g.auto = false;
    MenuAction::GraphicsChanged
}

/// The track row of race setup: the equipped track, or the circuit's route.
fn track_row(event: crate::circuit::Event, track: usize, help: &str) -> Row {
    let value = match event {
        crate::circuit::Event::Single => crate::track::TRACKS[track % crate::track::TRACKS.len()]
            .name
            .to_string(),
        crate::circuit::Event::Circuit => format!("All {} in order", crate::circuit::ROUNDS),
    };
    let label = match event {
        crate::circuit::Event::Single => "Track",
        crate::circuit::Event::Circuit => "Tracks",
    };
    Row::text(RowId::Track, label, value, help)
}

fn laps_row(s: &Settings) -> Row {
    Row::choice(
        RowId::Laps,
        "Laps",
        s.prefs.gameplay.laps.to_string(),
        "Laps per race.",
    )
}

fn rivals_row(s: &Settings) -> Row {
    Row::choice(
        RowId::Rivals,
        "Rivals",
        s.prefs.gameplay.rivals.to_string(),
        "How many computer drivers line up.",
    )
}

/// Current screen and highlighted row. `return_to` is where Settings' and the track
/// browser's Back goes; `name_return` is where the racer-name editor goes back to.
#[derive(Debug, Clone)]
pub struct MenuState {
    pub screen: Screen,
    pub selected: usize,
    return_to: Screen,
    name_return: Screen,
    pub mode: RaceMode,
    /// The Controls tab's Device choice: show gamepad buttons rather than keys.
    pub gamepad_guide: bool,
    /// The action waiting for a key or button press (see `capture`); menu input is ignored
    /// until it arrives or is cancelled.
    pub capturing: Option<Action>,
    /// The Audio tab's "Listen to" song.
    pub listen: Theme,
    pub name_entry: String,
    /// The address typed on the Online screen.
    pub join_addr: String,
    /// Online tailnet peers (name, address) offered on the Online screen.
    pub peers: Vec<(String, String)>,
    /// This machine hosts the lobby.
    pub lobby_host: bool,
    /// The host picked Elimination for the online race.
    pub lobby_elimination: bool,
    /// Single track or a full circuit.
    pub event: crate::circuit::Event,
    /// A circuit is under way and has another round after this one.
    pub circuit_next: bool,
    /// This host has a private invite to copy.
    pub invite_ready: bool,
    /// The address the last JoinGame action refers to.
    join_choice: Option<String>,
    /// A newer release the title screen offers to install, e.g. "v0.2.0".
    pub update: Option<String>,
}

impl MenuState {
    pub fn new(screen: Screen) -> Self {
        Self {
            screen,
            selected: 0,
            return_to: Screen::Title,
            name_return: Screen::Online,
            mode: RaceMode::Race,
            gamepad_guide: false,
            capturing: None,
            listen: Theme::Menu,
            name_entry: String::new(),
            join_addr: String::new(),
            peers: Vec::new(),
            lobby_host: false,
            lobby_elimination: false,
            event: Default::default(),
            circuit_next: false,
            invite_ready: false,
            join_choice: None,
            update: None,
        }
    }

    /// Text typed while the address row is selected (address characters only).
    pub fn type_text(&mut self, text: &str) {
        if self.screen == Screen::DriverName && self.selected == 0 {
            for c in text.chars().filter(|c| c.is_ascii_graphic() || *c == ' ') {
                if self.name_entry.len() < 16 {
                    self.name_entry.push(c);
                }
            }
            return;
        }
        if self.screen != Screen::Online || self.selected != ONLINE_ADDRESS {
            return;
        }
        for c in text.chars() {
            let ok = c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '-' | '[' | ']');
            if ok && self.join_addr.len() < MAX_ADDRESS {
                self.join_addr.push(c.to_ascii_lowercase());
            }
        }
    }

    pub fn backspace(&mut self) {
        if self.screen == Screen::DriverName && self.selected == 0 {
            self.name_entry.pop();
        }
        if self.screen == Screen::Online && self.selected == ONLINE_ADDRESS {
            self.join_addr.pop();
        }
    }

    /// The event a solo race runs: Time Trial is always a single track.
    pub fn solo_event(&self) -> crate::circuit::Event {
        if self.mode == RaceMode::TimeTrial {
            crate::circuit::Event::Single
        } else {
            self.event
        }
    }

    /// Where to connect after a JoinGame action.
    pub fn join_target(&self) -> Option<String> {
        self.join_choice.clone()
    }

    pub fn go(&mut self, screen: Screen) {
        let remember = match screen {
            // Coming back from the name editor keeps where Settings was opened from.
            Screen::Settings(_) => !matches!(self.screen, Screen::Settings(_) | Screen::DriverName),
            Screen::Circuits => true,
            _ => false,
        };
        if remember {
            self.return_to = self.screen;
        }
        if screen == Screen::DriverName {
            self.name_return = self.screen;
        }
        self.screen = screen;
        self.selected = 0;
        self.capturing = None;
    }

    /// Highlight the row `id` on the current screen, if it has one.
    fn select_row(&mut self, id: RowId, s: &Settings) {
        if let Some(i) = self.rows(s).iter().position(|r| r.id == id) {
            self.selected = i;
        }
    }

    /// Rows of the current screen.
    pub fn rows(&self, s: &Settings) -> Vec<Row> {
        match self.screen {
            Screen::Title => {
                let mut v = vec![
                    Row::go(
                        RowId::SoloRace,
                        "Solo race",
                        "Pick a track or run a circuit.",
                    ),
                    Row::go(
                        RowId::Multiplayer,
                        "Multiplayer",
                        "Host a race or join friends.",
                    ),
                    Row::go(RowId::Garage, "Garage", "Choose your chassis and paint."),
                    Row::go(
                        RowId::Settings,
                        "Settings",
                        "Graphics, audio, controls, and camera.",
                    ),
                ];
                if let Some(version) = &self.update {
                    v.push(Row::go(
                        RowId::Update,
                        format!("Update to {version}"),
                        "Download the new version and restart.",
                    ));
                }
                v.push(Row::go(RowId::Quit, "Quit", "Exit to desktop."));
                v
            }
            Screen::RaceSetup => {
                let event_help = match self.solo_event() {
                    crate::circuit::Event::Single => "One race on the track of your choice.",
                    crate::circuit::Event::Circuit => {
                        "Race every track in order. Points decide the champion."
                    }
                };
                vec![
                    Row::go(RowId::StartRace, "Start race", self.mode.description()),
                    Row::choice(
                        RowId::Mode,
                        "Mode",
                        self.mode.name(),
                        self.mode.description(),
                    ),
                    Row::choice(RowId::Event, "Event", self.solo_event().name(), event_help),
                    track_row(
                        self.solo_event(),
                        s.track,
                        "Browse the routes and pick one.",
                    ),
                    laps_row(s),
                    rivals_row(s),
                    Row::go(RowId::Back, "Back", "Back to the main menu."),
                ]
            }
            Screen::Circuits => crate::track::TRACKS
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    Row::text(
                        RowId::PickTrack(i),
                        t.name,
                        if i == s.track { "Equipped" } else { "" },
                        "Select to equip this track.",
                    )
                })
                .chain(std::iter::once(Row::go(
                    RowId::Back,
                    "Done",
                    if self.return_to == Screen::Lobby {
                        "Back to the lobby."
                    } else {
                        "Back to race setup."
                    },
                )))
                .collect(),
            Screen::DriverName => vec![
                Row::text(
                    RowId::NameField,
                    "Name",
                    format!("{}_", self.name_entry),
                    "Type your new name, then press Enter to save.",
                ),
                Row::go(RowId::SaveName, "Save name", "Save the name and go back."),
                Row::go(RowId::Back, "Cancel", "Keep the old name."),
            ],
            Screen::Garage => {
                let kart = &crate::kart::KARTS[s.loadout.design];
                vec![
                    Row::choice(
                        RowId::KartType,
                        "Type",
                        kart.kind.label(),
                        "Cars race trucks. ATVs race dirt bikes.",
                    ),
                    Row::choice(
                        RowId::Chassis,
                        "Chassis",
                        kart.label(),
                        "Every chassis in a class handles a little differently.",
                    ),
                    Row::choice(
                        RowId::Finish,
                        "Finish",
                        crate::kart::FINISHES[s.loadout.finish],
                        "Paint is cosmetic. Everyone sees it online.",
                    ),
                    Row::go(RowId::Back, "Done", "Save and go back."),
                ]
            }
            Screen::Settings(tab) => {
                let mut v = self.settings_rows(tab, s);
                v.push(Row::go(RowId::Back, "Back", "Save and go back."));
                v
            }
            Screen::Paused => vec![
                Row::go(RowId::Resume, "Resume", "Back to the race."),
                Row::go(
                    RowId::Restart,
                    "Restart",
                    "Start this race over from the grid.",
                ),
                Row::go(
                    RowId::Settings,
                    "Settings",
                    "Change settings without leaving the race.",
                ),
                Row::go(RowId::MainMenu, "Main menu", "Leave the race."),
                Row::go(RowId::Quit, "Quit", "Exit to desktop."),
            ],
            Screen::Results if self.circuit_next => vec![
                Row::go(RowId::NextRace, "Next race", "Start the next round."),
                Row::go(
                    RowId::QuitCircuit,
                    "Quit circuit",
                    "Leave the circuit. Points are lost.",
                ),
            ],
            Screen::Results => vec![
                Row::go(RowId::Continue, "Continue", "Back to the main menu."),
                Row::go(RowId::RaceAgain, "Race again", "Run this race again."),
            ],
            Screen::Online => vec![
                Row::go(
                    RowId::HostPrivate,
                    "Host private race",
                    "Makes an invite you can paste to friends.",
                ),
                Row::go(
                    RowId::PasteInvite,
                    "Paste invite & join",
                    "Join with an invite a friend sent you.",
                ),
                Row::go(
                    RowId::RacerName,
                    "Racer name",
                    "The name other players see.",
                ),
                Row::go(RowId::HostGame, "Host game", "Host on your local network."),
                Row::text(
                    RowId::JoinAddress,
                    "Join",
                    if self.join_addr.is_empty() {
                        "Type an address_".to_string()
                    } else {
                        format!("{}_", self.join_addr)
                    },
                    "Type an IP address or host name.",
                ),
                Row::text(
                    RowId::Tailscale,
                    "Tailscale",
                    format!("{} online", self.peers.len()),
                    "Pick a device on your tailnet to join its race.",
                ),
                Row::go(RowId::Back, "Back", "Back to the main menu."),
            ],
            Screen::Tailscale => self
                .peers
                .iter()
                .enumerate()
                .map(|(i, (n, _))| {
                    Row::text(
                        RowId::Peer(i),
                        "Join",
                        n.as_str(),
                        "Pick a device on your tailnet to join its race.",
                    )
                })
                .chain(std::iter::once(Row::go(
                    RowId::Back,
                    "Back",
                    if self.peers.is_empty() {
                        "No Tailscale peers online yet. The list refreshes every few seconds."
                    } else {
                        "Back to multiplayer."
                    },
                )))
                .collect(),
            Screen::Lobby if self.lobby_host => {
                let mut v = vec![
                    Row::go(
                        RowId::StartRace,
                        "Start race",
                        "Everyone on the grid races now.",
                    ),
                    Row::choice(
                        RowId::Mode,
                        "Mode",
                        if self.lobby_elimination {
                            "Elimination"
                        } else {
                            "Race"
                        },
                        "Pick the race mode for everyone.",
                    ),
                    Row::choice(
                        RowId::Event,
                        "Event",
                        self.event.name(),
                        "One race, or every track for points.",
                    ),
                    track_row(self.event, s.track, "Pick the track."),
                ];
                if self.invite_ready {
                    v.push(Row::go(
                        RowId::CopyInvite,
                        "Copy invite",
                        "Copies the invite again so you can share it.",
                    ));
                }
                v.push(Row::go(RowId::Back, "Leave", "Close the lobby."));
                v
            }
            Screen::Lobby => vec![
                Row::go(
                    RowId::Waiting,
                    "Waiting for the host",
                    "Gather your drivers on the starting grid.",
                ),
                Row::go(RowId::Back, "Leave", "Leave the lobby."),
            ],
            Screen::Playing => vec![],
        }
    }

    /// The rows of one Settings tab, without the closing Back row.
    fn settings_rows(&self, tab: Tab, s: &Settings) -> Vec<Row> {
        let p = &s.prefs;
        let g = &p.graphics;
        let q = &g.quality;
        match tab {
            Tab::Graphics => vec![
                Row::choice(
                    RowId::Preset,
                    "Quality preset",
                    name_of(&PRESETS, g.preset),
                    "Low targets Intel UHD 620 laptops. High doubles the shadow map.",
                ),
                Row::toggle(
                    RowId::AutoPreset,
                    "Pick automatically",
                    g.auto,
                    "Measures frame time on the first race and picks a preset.",
                ),
                Row::slider(
                    RowId::RenderScale,
                    "Render scale",
                    format!("{}%", (q.render_scale * 100.).round()),
                    RENDER_SCALE.fraction(q.render_scale),
                    "3D resolution relative to the window. The menus and HUD stay sharp.",
                )
                .fresh(),
                Row::choice(
                    RowId::Shadows,
                    "Shadows",
                    name_of(&SHADOWS, q.shadow_size),
                    "Shadow map size. Bigger is crisper and costs GPU time.",
                )
                .fresh(),
                Row::choice(
                    RowId::Bloom,
                    "Bloom",
                    name_of(&BLOOM, q.bloom_levels),
                    "Glow around bright lights, boost flames, and crystals.",
                )
                .fresh(),
                Row::toggle(
                    RowId::Fxaa,
                    "Anti-aliasing (FXAA)",
                    q.fxaa,
                    "Smooths jagged edges for almost no cost.",
                )
                .fresh(),
                Row::choice(
                    RowId::Textures,
                    "Texture detail",
                    name_of(&TEXTURES, q.texture_skip_mips),
                    "Half skips the top mip level to save video memory.",
                )
                .fresh(),
                Row::choice(
                    RowId::Anisotropy,
                    "Anisotropic filtering",
                    name_of(&ANISOTROPY, q.anisotropy),
                    "Keeps the road texture sharp at a distance.",
                )
                .fresh(),
                Row::choice(
                    RowId::DisplayMode,
                    "Display mode",
                    name_of(&DISPLAYS, g.display),
                    "Borderless switches apps fastest.",
                )
                .fresh(),
                Row::toggle(
                    RowId::Vsync,
                    "V-sync",
                    g.vsync,
                    "Stops tearing. Adds a little input delay.",
                )
                .fresh(),
                Row::choice(
                    RowId::FrameCap,
                    "Frame cap",
                    g.frame_cap.map_or("None".to_string(), |c| c.to_string()),
                    "Caps frames per second to save power on laptops.",
                )
                .fresh(),
                Row::choice(
                    RowId::ShowFps,
                    "Show FPS",
                    name_of(&FPS, g.fps),
                    "F3 or View cycles this during a race.",
                ),
            ],
            Tab::Audio => {
                let a = &p.audio;
                vec![
                    percent_row(
                        RowId::MasterVolume,
                        "Master",
                        a.master,
                        PERCENT,
                        "Everything Scrapburn plays.",
                    )
                    .fresh(),
                    percent_row(
                        RowId::MusicVolume,
                        "Music",
                        a.music,
                        PERCENT,
                        "The soundtrack. Each track has its own song.",
                    )
                    .fresh(),
                    percent_row(
                        RowId::EffectsVolume,
                        "Effects",
                        a.effects,
                        PERCENT,
                        "Items, impacts, boosts, and the countdown.",
                    )
                    .fresh(),
                    percent_row(
                        RowId::EnginesVolume,
                        "Engines",
                        a.engines,
                        PERCENT,
                        "Your engine and the pack around you.",
                    )
                    .fresh(),
                    percent_row(
                        RowId::MenusVolume,
                        "Menus",
                        a.menus,
                        PERCENT,
                        "Menu clicks and confirmations.",
                    )
                    .fresh(),
                    Row::toggle(
                        RowId::Soundtrack,
                        "Soundtrack",
                        a.soundtrack,
                        "Turn the race music on or off.",
                    ),
                    Row::choice(
                        RowId::ListenTo,
                        "Listen to",
                        name_of(&SONGS, self.listen),
                        "Preview any song from the soundtrack.",
                    )
                    .fresh(),
                ]
            }
            Tab::Controls => {
                let c = &p.controls;
                let pad = self.gamepad_guide;
                let mut v = vec![Row::choice(
                    RowId::Device,
                    "Device",
                    if pad { "Gamepad" } else { "Keyboard" },
                    "Bindings for the selected device.",
                )];
                for b in BindRow::ALL {
                    let (label, help) = match b {
                        BindRow::Action(Action::Accelerate) => (
                            "Accelerate",
                            "Hold to drive. On a gamepad the trigger is analog.",
                        ),
                        BindRow::Action(Action::Brake) => {
                            ("Brake & reverse", "Hold to brake, keep holding to reverse.")
                        }
                        BindRow::Action(Action::SteerLeft) if pad => ("Steer left", STEER_PAD_HELP),
                        BindRow::Action(Action::SteerRight) if pad => {
                            ("Steer right", STEER_PAD_HELP)
                        }
                        BindRow::Action(Action::SteerLeft) => (
                            "Steer left",
                            "Keyboard steering eases in so taps don't snap the kart.",
                        ),
                        BindRow::Action(Action::SteerRight) => (
                            "Steer right",
                            "Keyboard steering eases in so taps don't snap the kart.",
                        ),
                        BindRow::Action(Action::DriftHop) => (
                            "Drift & hop",
                            "Tap to hop. Hold through a corner to charge a drift boost.",
                        ),
                        BindRow::Action(Action::UseItem) => {
                            ("Use item", "Fire or drop the item you're holding.")
                        }
                        BindRow::Action(Action::Respawn) => {
                            ("Respawn", "Back on the track at the last checkpoint.")
                        }
                        BindRow::Action(Action::LookBehind) => {
                            ("Look behind", "Hold to watch the pack behind you.")
                        }
                        BindRow::Pause => ("Pause", "Opens the race menu. Can't be changed."),
                    };
                    let text = match (b, self.capturing) {
                        (BindRow::Action(a), Some(c)) if a == c && pad => {
                            "Press a button... Start cancels".to_string()
                        }
                        (BindRow::Action(a), Some(c)) if a == c => {
                            "Press a key... Esc cancels".to_string()
                        }
                        _ => c.bindings.text(b, pad),
                    };
                    let row = Row::new(RowId::Bind(b), label, RowKind::Bind { text }, help);
                    v.push(if b == BindRow::Action(Action::LookBehind) {
                        row.fresh()
                    } else {
                        row
                    });
                }
                v.extend([
                    percent_row(
                        RowId::Sensitivity,
                        "Steering sensitivity",
                        c.sensitivity,
                        PERCENT,
                        "How far the wheels turn for a given stick angle.",
                    )
                    .fresh(),
                    percent_row(
                        RowId::Deadzone,
                        "Stick dead zone",
                        c.deadzone,
                        DEADZONE,
                        "Ignore small stick movement from a worn controller.",
                    )
                    .fresh(),
                    Row::toggle(
                        RowId::Vibration,
                        "Vibration",
                        c.vibration,
                        "Rumble on impacts, boosts, and landings.",
                    )
                    .fresh(),
                ]);
                v
            }
            Tab::Camera => {
                let cam = &p.camera;
                let h = &p.hud;
                vec![
                    Row::slider(
                        RowId::CameraDistance,
                        "Camera distance",
                        format!("{:.1} m", cam.distance),
                        CAMERA_DISTANCE.fraction(cam.distance),
                        "How far the chase camera trails the kart.",
                    )
                    .fresh(),
                    Row::slider(
                        RowId::CameraHeight,
                        "Camera height",
                        format!("{:.1} m", cam.height),
                        CAMERA_HEIGHT.fraction(cam.height),
                        "Height of the camera above the road.",
                    )
                    .fresh(),
                    Row::slider(
                        RowId::Fov,
                        "Field of view",
                        format!("{:.0} deg", cam.fov),
                        FOV.fraction(cam.fov),
                        "Wider shows more track and feels faster.",
                    )
                    .fresh(),
                    Row::toggle(
                        RowId::WidenAtSpeed,
                        "Widen at speed",
                        cam.widen_at_speed,
                        "The view opens up a little while boosting.",
                    )
                    .fresh(),
                    percent_row(
                        RowId::Shake,
                        "Camera shake",
                        cam.shake,
                        SHAKE,
                        "Jolts from landings and hits.",
                    )
                    .fresh(),
                    percent_row(
                        RowId::HudScale,
                        "HUD scale",
                        h.scale,
                        HUD_SCALE,
                        "Size of the race display.",
                    )
                    .fresh(),
                    Row::toggle(
                        RowId::Minimap,
                        "Minimap",
                        h.minimap,
                        "Track map in the lower-left corner.",
                    )
                    .fresh(),
                    Row::toggle(
                        RowId::Speedometer,
                        "Speedometer",
                        h.speedometer,
                        "Speed readout in the lower-right corner.",
                    )
                    .fresh(),
                    Row::choice(
                        RowId::SpeedUnits,
                        "Speed units",
                        name_of(&UNITS, h.units),
                        "Units for the speedometer.",
                    )
                    .fresh(),
                    Row::toggle(
                        RowId::RivalNames,
                        "Rival names",
                        h.rival_names,
                        "Name tags over the karts ahead.",
                    )
                    .fresh(),
                ]
            }
            Tab::Gameplay => vec![
                laps_row(s),
                rivals_row(s),
                Row::go(
                    RowId::RacerName,
                    "Racer name",
                    "The name other players see online.",
                ),
            ],
        }
    }

    /// Apply left/right (`dir`) or select to a settings row. Select steps forward, like
    /// right, so a mouse click (select, or left on the "<") can move any row both ways.
    /// Returns an action only when the value actually changed.
    fn edit_setting(&mut self, id: RowId, dir: i32, select: bool, s: &mut Settings) -> MenuAction {
        use MenuAction as A;
        let step = if dir != 0 { dir } else { i32::from(select) };
        if step == 0 {
            return A::None;
        }
        // Sliders step the same way choices do; they clamp rather than wrap.
        let dir = step;
        let p = &mut s.prefs;
        let g = &mut p.graphics;
        match id {
            RowId::AutoPreset => flip(&mut g.auto, A::GraphicsChanged),
            RowId::RenderScale => set_quality(g, |q| {
                q.render_scale = RENDER_SCALE.stepped(q.render_scale, dir);
            }),
            RowId::Shadows => set_quality(g, |q| {
                q.shadow_size = cycle_in(&SHADOWS, q.shadow_size, step);
            }),
            RowId::Bloom => set_quality(g, |q| {
                q.bloom_levels = cycle_in(&BLOOM, q.bloom_levels, step);
            }),
            RowId::Fxaa => set_quality(g, |q| q.fxaa = !q.fxaa),
            RowId::Textures => set_quality(g, |q| {
                q.texture_skip_mips = cycle_in(&TEXTURES, q.texture_skip_mips, step);
            }),
            RowId::Anisotropy => set_quality(g, |q| {
                q.anisotropy = cycle_in(&ANISOTROPY, q.anisotropy, step);
            }),
            RowId::DisplayMode => {
                let next = cycle_in(&DISPLAYS, g.display, step);
                set(&mut g.display, next, A::DisplayChanged)
            }
            RowId::Vsync => flip(&mut g.vsync, A::VsyncChanged),
            RowId::FrameCap => {
                let next = cycle(&FRAME_CAPS, g.frame_cap, step);
                set(&mut g.frame_cap, next, A::FrameCapChanged)
            }
            RowId::MasterVolume
            | RowId::MusicVolume
            | RowId::EffectsVolume
            | RowId::EnginesVolume
            | RowId::MenusVolume => {
                let a = &mut p.audio;
                let slot = match id {
                    RowId::MasterVolume => &mut a.master,
                    RowId::MusicVolume => &mut a.music,
                    RowId::EffectsVolume => &mut a.effects,
                    RowId::EnginesVolume => &mut a.engines,
                    _ => &mut a.menus,
                };
                let next = PERCENT.stepped_u8(*slot, dir);
                set(slot, next, A::VolumeChanged)
            }
            RowId::ListenTo => {
                self.listen = cycle_in(&SONGS, self.listen, step);
                A::PreviewSong(self.listen)
            }
            RowId::Sensitivity => {
                let c = &mut p.controls;
                c.sensitivity = PERCENT.stepped_u8(c.sensitivity, dir);
                A::None
            }
            RowId::Deadzone => {
                let c = &mut p.controls;
                c.deadzone = DEADZONE.stepped_u8(c.deadzone, dir);
                A::None
            }
            RowId::Vibration => flip(&mut p.controls.vibration, A::None),
            RowId::CameraDistance => {
                p.camera.distance = CAMERA_DISTANCE.stepped(p.camera.distance, dir);
                A::None
            }
            RowId::CameraHeight => {
                p.camera.height = CAMERA_HEIGHT.stepped(p.camera.height, dir);
                A::None
            }
            RowId::Fov => {
                p.camera.fov = FOV.stepped(p.camera.fov, dir);
                A::None
            }
            RowId::WidenAtSpeed => flip(&mut p.camera.widen_at_speed, A::None),
            RowId::Shake => {
                p.camera.shake = SHAKE.stepped_u8(p.camera.shake, dir);
                A::None
            }
            RowId::HudScale => {
                p.hud.scale = HUD_SCALE.stepped_u8(p.hud.scale, dir);
                A::None
            }
            RowId::Minimap => flip(&mut p.hud.minimap, A::None),
            RowId::Speedometer => flip(&mut p.hud.speedometer, A::None),
            RowId::SpeedUnits => {
                p.hud.units = cycle_in(&UNITS, p.hud.units, step);
                A::None
            }
            RowId::RivalNames => flip(&mut p.hud.rival_names, A::None),
            RowId::Preset => {
                let next = cycle(
                    &[Preset::Low, Preset::Medium, Preset::High],
                    p.graphics.preset.preset(),
                    step,
                );
                p.graphics.set_preset(next);
                p.graphics.auto = false;
                A::PresetChanged
            }
            RowId::Laps => {
                p.gameplay.laps = cycle(&LAP_CHOICES, p.gameplay.laps, step);
                A::None
            }
            RowId::Rivals => {
                p.gameplay.rivals = cycle(&RIVAL_CHOICES, p.gameplay.rivals, step);
                A::None
            }
            RowId::ShowFps => {
                p.graphics.fps = cycle_in(&FPS, p.graphics.fps, step);
                A::None
            }
            RowId::Soundtrack => flip(&mut p.audio.soundtrack, A::None),
            RowId::Device => {
                self.gamepad_guide = !self.gamepad_guide;
                A::None
            }
            // Enter waits for the new key or button; gamepad steering is always the stick.
            RowId::Bind(BindRow::Action(a))
                if select && !(self.gamepad_guide && a.stick_only()) =>
            {
                self.capturing = Some(a);
                A::None
            }
            _ => A::None,
        }
    }

    /// Offer one key or button press to the binding being captured. Esc or Start cancels.
    /// A press from the other device, or one that can't be bound, is ignored and the capture
    /// keeps waiting.
    pub fn capture(&mut self, press: Press, s: &mut Settings) -> MenuAction {
        let Some(a) = self.capturing else {
            return MenuAction::None;
        };
        let cancel = matches!(press, Press::Key(winit::keyboard::KeyCode::Escape))
            || press == Press::Pad(rivet_platform::PadButtons::START);
        if cancel {
            self.capturing = None;
            return MenuAction::None;
        }
        if matches!(press, Press::Pad(_)) != self.gamepad_guide
            || !s.prefs.controls.bindings.capture(a, press)
        {
            return MenuAction::None;
        }
        self.capturing = None;
        MenuAction::BindingCaptured
    }

    pub fn handle(&mut self, input: MenuInput, s: &mut Settings) -> MenuAction {
        if self.capturing.is_some() {
            return MenuAction::None;
        }
        if let Screen::Settings(tab) = self.screen {
            let d = i32::from(input.tab_next) - i32::from(input.tab_prev);
            if d != 0 {
                // A tab switch stays inside Settings, so `return_to` is kept.
                self.screen = Screen::Settings(tab.cycled(d));
                self.selected = 0;
                return MenuAction::None;
            }
        }
        let rows = self.rows(s);
        let n = rows.len();
        if n == 0 {
            return MenuAction::None;
        }
        self.selected = self.selected.min(n - 1);
        if input.up {
            self.selected = (self.selected + n - 1) % n;
        }
        if input.down {
            self.selected = (self.selected + 1) % n;
        }
        let id = rows[self.selected].id;
        let dir = input.right as i32 - input.left as i32;
        let select = input.select;
        // Choices step forward on select as well as with left and right.
        let step = if dir != 0 { dir } else { i32::from(select) };
        let closing = input.back || (select && id == RowId::Back);
        match self.screen {
            Screen::Title => {
                if select {
                    match id {
                        RowId::SoloRace => self.go(Screen::RaceSetup),
                        RowId::Multiplayer => self.go(Screen::Online),
                        RowId::Garage => self.go(Screen::Garage),
                        RowId::Settings => self.go(Screen::Settings(Tab::Graphics)),
                        RowId::Update => return MenuAction::Update,
                        RowId::Quit => return MenuAction::Quit,
                        _ => {}
                    }
                }
                MenuAction::None
            }
            Screen::RaceSetup => {
                if closing {
                    self.go(Screen::Title);
                    return MenuAction::None;
                }
                match id {
                    RowId::StartRace if select => {
                        return match self.mode {
                            RaceMode::Race => MenuAction::StartRace,
                            RaceMode::TimeTrial => MenuAction::StartTimeTrial,
                            RaceMode::Elimination => MenuAction::StartElimination,
                        };
                    }
                    RowId::Mode if step != 0 => {
                        self.mode = cycle(
                            &[RaceMode::Race, RaceMode::TimeTrial, RaceMode::Elimination],
                            self.mode,
                            step,
                        );
                    }
                    RowId::Event if step != 0 && self.mode != RaceMode::TimeTrial => {
                        self.event = self.event.toggled();
                    }
                    RowId::Track
                        if select && self.solo_event() == crate::circuit::Event::Single =>
                    {
                        self.go(Screen::Circuits);
                        self.selected = s.track;
                    }
                    RowId::Laps | RowId::Rivals => {
                        return self.edit_setting(id, dir, select, s);
                    }
                    _ => {}
                }
                MenuAction::None
            }
            Screen::Circuits => {
                if closing {
                    self.go(self.return_to);
                    self.select_row(RowId::Track, s);
                } else if let RowId::PickTrack(i) = id
                    && select
                    && s.track != i
                {
                    s.track = i;
                    return MenuAction::TrackChanged;
                }
                MenuAction::None
            }
            Screen::DriverName => {
                let save = select && !self.name_entry.trim().is_empty();
                if closing || save {
                    self.go(self.name_return);
                    self.select_row(RowId::RacerName, s);
                    if !closing {
                        return MenuAction::SaveName;
                    }
                }
                MenuAction::None
            }
            Screen::Garage => {
                if closing {
                    self.go(Screen::Title);
                    self.select_row(RowId::Garage, s);
                    return MenuAction::SaveGarage;
                }
                match id {
                    RowId::KartType => s.loadout.cycle_type(step),
                    RowId::Chassis => s.loadout.cycle_chassis(step),
                    RowId::Finish => {
                        s.loadout.finish = (s.loadout.finish as i32 + step)
                            .rem_euclid(crate::kart::FINISHES.len() as i32)
                            as usize
                    }
                    _ => {}
                }
                MenuAction::None
            }
            Screen::Settings(_) => {
                if closing {
                    self.go(self.return_to);
                    self.select_row(RowId::Settings, s);
                    return MenuAction::None;
                }
                if id == RowId::RacerName {
                    if select {
                        self.name_entry.clear();
                        self.go(Screen::DriverName);
                    }
                    return MenuAction::None;
                }
                self.edit_setting(id, dir, select, s)
            }
            Screen::Paused => {
                if input.back {
                    return MenuAction::Resume;
                }
                if !select {
                    return MenuAction::None;
                }
                match id {
                    RowId::Resume => MenuAction::Resume,
                    RowId::Restart => MenuAction::Restart,
                    RowId::Settings => {
                        self.go(Screen::Settings(Tab::Graphics));
                        MenuAction::None
                    }
                    RowId::MainMenu => MenuAction::ToTitle,
                    RowId::Quit => MenuAction::Quit,
                    _ => MenuAction::None,
                }
            }
            Screen::Online => {
                if closing {
                    self.go(Screen::Title);
                    self.select_row(RowId::Multiplayer, s);
                    return MenuAction::None;
                }
                if !select {
                    return MenuAction::None;
                }
                match id {
                    RowId::Tailscale => {
                        self.go(Screen::Tailscale);
                        MenuAction::None
                    }
                    RowId::HostGame => MenuAction::HostGame,
                    RowId::HostPrivate => MenuAction::PrivateHost,
                    RowId::PasteInvite => MenuAction::PrivateJoin,
                    RowId::RacerName => {
                        self.name_entry.clear();
                        self.go(Screen::DriverName);
                        MenuAction::None
                    }
                    RowId::JoinAddress if self.join_addr.is_empty() => MenuAction::None,
                    RowId::JoinAddress => {
                        self.join_choice = Some(self.join_addr.clone());
                        MenuAction::JoinGame
                    }
                    _ => MenuAction::None,
                }
            }
            Screen::Tailscale => {
                if closing {
                    self.go(Screen::Online);
                    self.selected = ONLINE_TAILSCALE;
                    return MenuAction::None;
                }
                let RowId::Peer(i) = id else {
                    return MenuAction::None;
                };
                if !select {
                    return MenuAction::None;
                }
                self.join_choice = self.peers.get(i).map(|p| p.1.clone());
                if self.join_choice.is_some() {
                    MenuAction::JoinGame
                } else {
                    MenuAction::None
                }
            }
            Screen::Lobby => {
                if closing {
                    return MenuAction::LeaveOnline;
                }
                if !self.lobby_host {
                    return MenuAction::None;
                }
                let single = self.event == crate::circuit::Event::Single;
                match id {
                    RowId::CopyInvite if select => MenuAction::CopyInvite,
                    RowId::Mode if step != 0 => {
                        self.lobby_elimination = !self.lobby_elimination;
                        MenuAction::None
                    }
                    RowId::Event if step != 0 => {
                        self.event = self.event.toggled();
                        MenuAction::None
                    }
                    RowId::Track if select && single => {
                        self.go(Screen::Circuits);
                        self.selected = s.track;
                        MenuAction::None
                    }
                    RowId::Track if dir != 0 && single => {
                        let tracks = crate::track::TRACKS.len() as i32;
                        s.track = (s.track as i32 + dir).rem_euclid(tracks) as usize;
                        MenuAction::TrackChanged
                    }
                    RowId::StartRace if select => MenuAction::LobbyStart,
                    _ => MenuAction::None,
                }
            }
            Screen::Results if input.back => MenuAction::ToTitle,
            Screen::Results => match (select, id) {
                (true, RowId::NextRace) => MenuAction::NextRace,
                (true, RowId::QuitCircuit | RowId::Continue) => MenuAction::ToTitle,
                (true, RowId::RaceAgain) => MenuAction::Restart,
                _ => MenuAction::None,
            },
            Screen::Playing => MenuAction::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bindings::{Action, Bindings, Press};
    use rivet_platform::PadButtons;
    use winit::keyboard::KeyCode;

    fn press(f: impl Fn(&mut MenuInput)) -> MenuInput {
        let mut i = MenuInput::default();
        f(&mut i);
        i
    }

    /// The rows as the old renderer sees them.
    fn texts(m: &MenuState, s: &Settings) -> Vec<String> {
        m.rows(s).iter().map(Row::legacy_string).collect()
    }

    /// Select the row `id` on the current screen.
    fn pick(m: &mut MenuState, s: &Settings, id: RowId) {
        m.selected = m
            .rows(s)
            .iter()
            .position(|r| r.id == id)
            .unwrap_or_else(|| panic!("{id:?} is not on {:?}", m.screen));
    }

    fn open_tab(tab: Tab) -> MenuState {
        let mut m = MenuState::new(Screen::Title);
        m.go(Screen::Settings(tab));
        m
    }

    #[test]
    fn navigation_wraps() {
        let mut m = MenuState::new(Screen::Title);
        let n = m.rows(&Settings::default()).len();
        m.handle(press(|i| i.up = true), &mut Settings::default());
        assert_eq!(m.selected, n - 1);
        m.handle(press(|i| i.down = true), &mut Settings::default());
        assert_eq!(m.selected, 0);
    }

    #[test]
    fn title_actions() {
        let mut s = Settings::default();
        let mut m = MenuState::new(Screen::Title);
        assert_eq!(
            texts(&m, &s),
            ["SOLO RACE", "MULTIPLAYER", "GARAGE", "SETTINGS", "QUIT"]
        );
        m.handle(press(|i| i.select = true), &mut s);
        assert_eq!(m.screen, Screen::RaceSetup);
        for action in [
            MenuAction::StartRace,
            MenuAction::StartTimeTrial,
            MenuAction::StartElimination,
        ] {
            m.selected = 0;
            assert_eq!(m.handle(press(|i| i.select = true), &mut s), action);
            m.selected = 1;
            m.handle(press(|i| i.right = true), &mut s);
        }
        m.go(Screen::Title);
        m.selected = 1;
        m.handle(press(|i| i.select = true), &mut s);
        assert_eq!(m.screen, Screen::Online);
        m.go(Screen::Title);
        m.selected = 3;
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::None
        );
        assert_eq!(m.screen, Screen::Settings(Tab::Graphics));
        assert_eq!(m.handle(press(|i| i.back = true), &mut s), MenuAction::None);
        assert_eq!(
            (m.screen, m.selected),
            (Screen::Title, 3),
            "back returns to the title on the Settings row"
        );
    }

    #[test]
    fn title_offers_an_available_update_above_quit() {
        let mut s = Settings::default();
        let mut m = MenuState::new(Screen::Title);
        let plain = texts(&m, &s);
        assert_eq!(plain.last().map(String::as_str), Some("QUIT"));
        m.selected = plain.len() - 1;
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::Quit
        );
        m.update = Some("v0.2.0".into());
        let items = texts(&m, &s);
        assert_eq!(items.len(), plain.len() + 1);
        assert_eq!(items[4], "UPDATE TO V0.2.0");
        m.selected = 4;
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::Update
        );
        m.selected = 5;
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::Quit
        );
    }

    #[test]
    fn garage_equips_chassis_and_finish() {
        let mut menu = MenuState::new(Screen::Title);
        let mut settings = Settings::default();
        menu.selected = texts(&menu, &settings)
            .iter()
            .position(|label| label == "GARAGE")
            .unwrap();
        menu.handle(press(|i| i.select = true), &mut settings);
        assert_eq!(menu.screen, Screen::Garage);
        menu.handle(press(|i| i.down = true), &mut settings);
        menu.handle(press(|i| i.left = true), &mut settings);
        assert!(texts(&menu, &settings)[1].contains("SPEEDSTER"));
        menu.handle(press(|i| i.down = true), &mut settings);
        menu.handle(press(|i| i.right = true), &mut settings);
        assert!(texts(&menu, &settings)[2].contains("GLACIER"));
        assert_eq!(
            menu.handle(press(|i| i.back = true), &mut settings),
            MenuAction::SaveGarage
        );
        assert_eq!(texts(&menu, &settings)[menu.selected], "GARAGE");
        assert_eq!(settings.loadout.design, 2);
    }

    #[test]
    fn garage_type_filters_chassis_and_wraps_both_ways() {
        let mut menu = MenuState::new(Screen::Garage);
        let mut settings = Settings::default();
        menu.handle(press(|i| i.left = true), &mut settings);
        assert!(texts(&menu, &settings)[0].contains("DIRT BIKE"));
        menu.handle(press(|i| i.down = true), &mut settings);
        menu.handle(press(|i| i.left = true), &mut settings);
        assert!(texts(&menu, &settings)[1].contains("RIDGEBACK 450"));
        menu.handle(press(|i| i.right = true), &mut settings);
        assert!(texts(&menu, &settings)[1].contains("REDLINE 250"));
        menu.handle(press(|i| i.up = true), &mut settings);
        menu.handle(press(|i| i.left = true), &mut settings);
        assert!(texts(&menu, &settings)[1].contains("NORTH STAR 570"));
    }

    #[test]
    fn track_picker_applies_choice_and_returns_to_setup() {
        let mut s = Settings::default();
        let mut m = MenuState::new(Screen::RaceSetup);
        m.selected = 3;
        m.handle(press(|i| i.select = true), &mut s);
        assert_eq!(m.screen, Screen::Circuits);
        m.handle(press(|i| i.down = true), &mut s);
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::TrackChanged
        );
        assert_eq!(s.track, 1);
        m.handle(press(|i| i.back = true), &mut s);
        assert_eq!((m.screen, m.selected), (Screen::RaceSetup, 3));
    }

    #[test]
    fn race_setup_sets_laps_and_rivals() {
        let mut s = Settings::default();
        let mut m = MenuState::new(Screen::RaceSetup);
        let rows = texts(&m, &s);
        assert_eq!(rows[4], "LAPS  < 3 >");
        assert_eq!(rows[5], "RIVALS  < 7 >");
        assert_eq!(rows.last().map(String::as_str), Some("BACK"));
        m.selected = 4;
        m.handle(press(|i| i.right = true), &mut s);
        assert_eq!(s.prefs.gameplay.laps, 5);
        m.selected = 5;
        m.handle(press(|i| i.select = true), &mut s);
        assert_eq!(s.prefs.gameplay.rivals, 3, "wraps");
        m.selected = rows.len() - 1;
        m.handle(press(|i| i.select = true), &mut s);
        assert_eq!(m.screen, Screen::Title);
    }

    #[test]
    fn circuit_event_runs_every_track_and_offers_the_next_race() {
        let mut s = Settings::default();
        let mut m = MenuState::new(Screen::RaceSetup);
        m.selected = 2;
        m.handle(press(|i| i.right = true), &mut s);
        assert_eq!(m.solo_event(), crate::circuit::Event::Circuit);
        assert!(texts(&m, &s)[3].contains("ALL"));
        // The track browser stays closed: a circuit runs every track.
        m.selected = 3;
        m.handle(press(|i| i.select = true), &mut s);
        assert_eq!(m.screen, Screen::RaceSetup);
        // Time Trial is always a single track.
        m.mode = RaceMode::TimeTrial;
        assert_eq!(m.solo_event(), crate::circuit::Event::Single);
        m.selected = 2;
        m.handle(press(|i| i.right = true), &mut s);
        assert_eq!(m.event, crate::circuit::Event::Circuit, "unchanged");

        let mut r = MenuState::new(Screen::Results);
        r.circuit_next = true;
        assert_eq!(texts(&r, &s)[0], "NEXT RACE");
        assert_eq!(
            r.handle(press(|i| i.select = true), &mut s),
            MenuAction::NextRace
        );
        r.selected = 1;
        assert_eq!(
            r.handle(press(|i| i.select = true), &mut s),
            MenuAction::ToTitle
        );
    }

    #[test]
    fn online_menu_hosts_and_joins_by_typed_address_or_tailnet_peer() {
        let mut s = Settings::default();
        let mut m = MenuState::new(Screen::Online);
        m.selected = 3;
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::HostGame
        );
        // Typing only lands in the address row.
        m.type_text("9");
        assert_eq!(m.join_addr, "");
        m.selected = ONLINE_ADDRESS;
        m.type_text("100.64.0.2:27960 ;x");
        m.backspace();
        assert_eq!(m.join_addr, "100.64.0.2:27960");
        assert!(texts(&m, &s)[ONLINE_ADDRESS].contains("100.64.0.2:27960"));
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::JoinGame
        );
        assert_eq!(m.join_target().as_deref(), Some("100.64.0.2:27960"));
        // Tailnet peers live on their own screen, however many there are.
        m.peers = (0..30)
            .map(|i| (format!("PEER {i}"), format!("100.64.0.{i}")))
            .collect();
        m.peers[0] = ("THINKPAD".into(), "100.64.0.9".into());
        assert_eq!(
            m.rows(&s).len(),
            7,
            "the peer list stays off the Online screen"
        );
        m.selected = ONLINE_TAILSCALE;
        assert!(texts(&m, &s)[m.selected].contains("30 ONLINE"));
        m.handle(press(|i| i.select = true), &mut s);
        assert_eq!(m.screen, Screen::Tailscale);
        assert!(texts(&m, &s)[0].contains("THINKPAD"));
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::JoinGame
        );
        assert_eq!(m.join_target().as_deref(), Some("100.64.0.9"));
        m.handle(press(|i| i.back = true), &mut s);
        assert_eq!((m.screen, m.selected), (Screen::Online, ONLINE_TAILSCALE));
        // An empty address does nothing; Back returns to the title.
        m.join_addr.clear();
        m.selected = ONLINE_ADDRESS;
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::None
        );
        m.handle(press(|i| i.back = true), &mut s);
        assert_eq!((m.screen, m.selected), (Screen::Title, 1));
    }

    #[test]
    fn name_editor_saves_only_a_nonempty_name_and_escape_cancels() {
        let mut m = MenuState::new(Screen::Online);
        let mut s = Settings::default();
        m.selected = 2;
        m.handle(press(|i| i.select = true), &mut s);
        assert_eq!(m.screen, Screen::DriverName);
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::None
        );
        m.type_text("DUST RIDER");
        m.backspace();
        m.type_text("R");
        assert_eq!(m.name_entry, "DUST RIDER");
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::SaveName
        );
        assert_eq!((m.screen, m.selected), (Screen::Online, 2));
        m.handle(press(|i| i.select = true), &mut s);
        m.type_text("NEW NAME");
        assert_eq!(m.handle(press(|i| i.back = true), &mut s), MenuAction::None);
        assert_eq!(m.screen, Screen::Online);
    }

    #[test]
    fn gameplay_racer_name_returns_to_its_tab() {
        let mut s = Settings::default();
        let mut m = MenuState::new(Screen::Paused);
        m.selected = 2;
        m.handle(press(|i| i.select = true), &mut s);
        m.handle(press(|i| i.tab_prev = true), &mut s);
        assert_eq!(m.screen, Screen::Settings(Tab::Gameplay));
        assert_eq!(
            texts(&m, &s)[..3],
            ["LAPS  < 3 >", "RIVALS  < 7 >", "RACER NAME"]
        );
        pick(&mut m, &s, RowId::RacerName);
        m.handle(press(|i| i.select = true), &mut s);
        assert_eq!(m.screen, Screen::DriverName);
        m.type_text("ACE");
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::SaveName
        );
        assert_eq!((m.screen, m.selected), (Screen::Settings(Tab::Gameplay), 2));
        // Settings still goes back to where it was opened.
        m.handle(press(|i| i.back = true), &mut s);
        assert_eq!((m.screen, m.selected), (Screen::Paused, 2));
    }

    #[test]
    fn pause_settings_return_to_the_same_row() {
        let mut m = MenuState::new(Screen::Paused);
        let mut s = Settings::default();
        m.selected = 2;
        m.handle(press(|i| i.select = true), &mut s);
        m.handle(press(|i| i.tab_next = true), &mut s);
        m.handle(press(|i| i.tab_next = true), &mut s);
        assert_eq!(m.screen, Screen::Settings(Tab::Controls));
        m.handle(press(|i| i.right = true), &mut s);
        assert!(m.gamepad_guide);
        m.handle(press(|i| i.back = true), &mut s);
        assert_eq!((m.screen, m.selected), (Screen::Paused, 2));
    }

    #[test]
    fn controls_tab_shows_bindings_for_the_chosen_device() {
        let mut s = Settings::default();
        let mut m = open_tab(Tab::Controls);
        let rows = texts(&m, &s);
        assert_eq!(rows[0], "DEVICE  < KEYBOARD >");
        assert_eq!(rows[1], "ACCELERATE  W / UP");
        m.handle(press(|i| i.select = true), &mut s);
        let rows = texts(&m, &s);
        assert_eq!(rows[0], "DEVICE  < GAMEPAD >");
        assert_eq!(rows[1], "ACCELERATE  RT OR A");
        pick(&mut m, &s, RowId::Bind(BindRow::Action(Action::SteerLeft)));
        assert_eq!(texts(&m, &s)[m.selected], "STEER LEFT  LEFT STICK");
        assert_eq!(
            m.rows(&s)[m.selected].help,
            "On a gamepad, steering is always the left stick."
        );
        // Gamepad steering is always the stick: nothing to capture.
        m.handle(press(|i| i.select = true), &mut s);
        assert_eq!(m.capturing, None);
    }

    fn capture_on(device_pad: bool, action: Action) -> (MenuState, Settings) {
        let s = Settings::default();
        let mut m = open_tab(Tab::Controls);
        m.gamepad_guide = device_pad;
        pick(&mut m, &s, RowId::Bind(BindRow::Action(action)));
        (m, s)
    }

    #[test]
    fn enter_on_a_binding_waits_for_a_key() {
        let (mut m, mut s) = capture_on(false, Action::UseItem);
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::None
        );
        assert_eq!(m.capturing, Some(Action::UseItem));
        assert_eq!(
            texts(&m, &s)[m.selected],
            "USE ITEM  PRESS A KEY... ESC CANCELS"
        );
        // Menu input is ignored while capturing.
        let at = m.selected;
        m.handle(press(|i| i.down = true), &mut s);
        m.handle(press(|i| i.back = true), &mut s);
        assert_eq!(
            (m.screen, m.selected),
            (Screen::Settings(Tab::Controls), at)
        );
        // Unbindable keys and the wrong device are ignored.
        assert_eq!(
            m.capture(Press::Key(KeyCode::Backspace), &mut s),
            MenuAction::None
        );
        assert_eq!(
            m.capture(Press::Pad(PadButtons::WEST), &mut s),
            MenuAction::None
        );
        assert_eq!(m.capturing, Some(Action::UseItem));
        assert_eq!(
            m.capture(Press::Key(KeyCode::KeyF), &mut s),
            MenuAction::BindingCaptured
        );
        assert_eq!(m.capturing, None);
        assert_eq!(texts(&m, &s)[m.selected], "USE ITEM  F / CTRL / J");
    }

    #[test]
    fn esc_or_start_cancels_a_capture() {
        let (mut m, mut s) = capture_on(false, Action::Respawn);
        m.handle(press(|i| i.select = true), &mut s);
        assert_eq!(
            m.capture(Press::Key(KeyCode::Escape), &mut s),
            MenuAction::None
        );
        assert_eq!(m.capturing, None);
        assert_eq!(s.prefs.controls.bindings, Bindings::default());
        let (mut m, mut s) = capture_on(true, Action::Respawn);
        m.handle(press(|i| i.select = true), &mut s);
        assert_eq!(
            texts(&m, &s)[m.selected],
            "RESPAWN  PRESS A BUTTON... START CANCELS"
        );
        m.capture(Press::Pad(PadButtons::START), &mut s);
        assert_eq!(m.capturing, None);
        assert_eq!(s.prefs.controls.bindings, Bindings::default());
        // Leaving the screen any other way drops the capture too.
        m.handle(press(|i| i.select = true), &mut s);
        m.go(Screen::Playing);
        assert_eq!(m.capturing, None);
    }

    #[test]
    fn gamepad_capture_binds_a_button_and_swaps() {
        let (mut m, mut s) = capture_on(true, Action::Respawn);
        m.handle(press(|i| i.select = true), &mut s);
        // Keys are ignored while capturing for the gamepad.
        m.capture(Press::Key(KeyCode::KeyZ), &mut s);
        assert_eq!(m.capturing, Some(Action::Respawn));
        // X uses the item; it moves to respawn and use item takes B.
        assert_eq!(
            m.capture(Press::Pad(PadButtons::WEST), &mut s),
            MenuAction::BindingCaptured
        );
        let b = &s.prefs.controls.bindings;
        assert_eq!(b.text(BindRow::Action(Action::Respawn), true), "X");
        assert_eq!(b.text(BindRow::Action(Action::UseItem), true), "B");
    }

    #[test]
    fn private_race_actions_stay_available_with_peers() {
        let mut menu = MenuState::new(Screen::Online);
        let mut settings = Settings::default();
        menu.peers.push(("Friend".into(), "100.64.0.1".into()));
        let rows = texts(&menu, &settings);
        for (label, action) in [
            ("HOST PRIVATE RACE", MenuAction::PrivateHost),
            ("PASTE INVITE & JOIN", MenuAction::PrivateJoin),
        ] {
            menu.selected = rows.iter().position(|row| row == label).unwrap();
            assert_eq!(
                menu.handle(press(|i| i.select = true), &mut settings),
                action
            );
        }
    }

    #[test]
    fn host_lobby_offers_copy_invite_only_with_an_invite() {
        let mut s = Settings::default();
        let mut m = MenuState::new(Screen::Lobby);
        m.lobby_host = true;
        assert!(!texts(&m, &s).iter().any(|r| r == "COPY INVITE"));
        m.invite_ready = true;
        let rows = texts(&m, &s);
        m.selected = rows.iter().position(|r| r == "COPY INVITE").unwrap();
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::CopyInvite
        );
        m.selected = rows.len() - 1;
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::LeaveOnline
        );
    }

    #[test]
    fn lobby_host_starts_and_picks_the_mode_guests_wait() {
        let mut s = Settings::default();
        let mut m = MenuState::new(Screen::Lobby);
        m.lobby_host = true;
        assert!(texts(&m, &s)[1].contains("RACE"));
        assert_eq!(
            m.handle(
                press(|i| {
                    i.down = true;
                }),
                &mut s
            ),
            MenuAction::None
        );
        assert_eq!(
            m.handle(press(|i| i.right = true), &mut s),
            MenuAction::None
        );
        assert!(m.lobby_elimination);
        assert!(texts(&m, &s)[1].contains("ELIMINATION"));
        m.selected = 3;
        assert!(texts(&m, &s)[3].contains(crate::track::TRACKS[0].name));
        assert_eq!(
            m.handle(press(|i| i.right = true), &mut s),
            MenuAction::TrackChanged
        );
        assert_eq!(s.track, 1);
        m.handle(press(|i| i.select = true), &mut s);
        assert_eq!(m.screen, Screen::Circuits);
        m.handle(press(|i| i.back = true), &mut s);
        assert_eq!(m.screen, Screen::Lobby);
        m.selected = 0;
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::LobbyStart
        );
        assert_eq!(
            m.handle(press(|i| i.back = true), &mut s),
            MenuAction::LeaveOnline
        );

        let mut g = MenuState::new(Screen::Lobby);
        g.lobby_host = false;
        assert_eq!(
            g.handle(press(|i| i.select = true), &mut s),
            MenuAction::None,
            "guests can't start"
        );
        g.selected = 1;
        assert_eq!(
            g.handle(press(|i| i.select = true), &mut s),
            MenuAction::LeaveOnline
        );
    }

    #[test]
    fn music_toggles_and_defaults_on() {
        let mut s = Settings::default();
        assert!(s.prefs.audio.soundtrack);
        let mut m = open_tab(Tab::Audio);
        pick(&mut m, &s, RowId::Soundtrack);
        assert_eq!(texts(&m, &s)[m.selected], "SOUNDTRACK  < ON >");
        m.handle(press(|i| i.select = true), &mut s);
        assert!(!s.prefs.audio.soundtrack);
        assert!(texts(&m, &s)[m.selected].contains("OFF"));
        m.handle(press(|i| i.left = true), &mut s);
        assert!(s.prefs.audio.soundtrack);
    }

    #[test]
    fn settings_cycle() {
        let mut s = Settings::default();
        let mut m = open_tab(Tab::Gameplay);
        pick(&mut m, &s, RowId::Laps);
        assert_eq!(s.prefs.gameplay.laps, 3);
        m.handle(press(|i| i.right = true), &mut s);
        assert_eq!(s.prefs.gameplay.laps, 5);
        m.handle(press(|i| i.right = true), &mut s);
        assert_eq!(s.prefs.gameplay.laps, 1, "wraps");
        let mut m = open_tab(Tab::Graphics);
        pick(&mut m, &s, RowId::Preset);
        let before = s.preset();
        assert_eq!(
            m.handle(press(|i| i.right = true), &mut s),
            MenuAction::PresetChanged
        );
        assert_ne!(s.preset(), before);
    }

    #[test]
    fn fps_display_cycles() {
        let mut s = Settings::default();
        let mut m = open_tab(Tab::Graphics);
        pick(&mut m, &s, RowId::ShowFps);
        m.handle(press(|i| i.right = true), &mut s);
        assert_eq!(s.prefs.graphics.fps, FpsDisplay::Counter);
        m.handle(press(|i| i.right = true), &mut s);
        assert_eq!(s.prefs.graphics.fps, FpsDisplay::Graph);
        m.handle(press(|i| i.left = true), &mut s);
        assert_eq!(s.prefs.graphics.fps, FpsDisplay::Counter);
    }

    #[test]
    fn tabs_cycle_and_wrap() {
        let mut s = Settings::default();
        let mut m = open_tab(Tab::Graphics);
        m.selected = 3;
        let mut seen = vec![];
        for _ in 0..5 {
            m.handle(press(|i| i.tab_next = true), &mut s);
            let Screen::Settings(tab) = m.screen else {
                panic!("left Settings on a tab switch");
            };
            assert_eq!(m.selected, 0, "a new tab starts at its first row");
            seen.push(tab);
        }
        assert_eq!(
            seen,
            [
                Tab::Audio,
                Tab::Controls,
                Tab::Camera,
                Tab::Gameplay,
                Tab::Graphics
            ]
        );
        m.handle(press(|i| i.tab_prev = true), &mut s);
        assert_eq!(m.screen, Screen::Settings(Tab::Gameplay), "wraps backward");
        // Tab keys do nothing outside Settings.
        let mut t = MenuState::new(Screen::Title);
        t.handle(press(|i| i.tab_next = true), &mut s);
        assert_eq!(t.screen, Screen::Title);
    }

    #[test]
    fn back_from_any_tab_returns_to_the_settings_row() {
        for tab in Tab::ALL {
            let mut s = Settings::default();
            let mut m = MenuState::new(Screen::Title);
            pick(&mut m, &s, RowId::Settings);
            m.handle(press(|i| i.select = true), &mut s);
            for _ in 0..Tab::ALL.len() {
                if m.screen == Screen::Settings(tab) {
                    break;
                }
                m.handle(press(|i| i.tab_next = true), &mut s);
            }
            assert_eq!(m.screen, Screen::Settings(tab), "Q/E reach every tab");
            m.handle(press(|i| i.back = true), &mut s);
            assert_eq!((m.screen, m.selected), (Screen::Title, 3), "{tab:?}");
            // The Back row at the end of the tab does the same.
            m.go(Screen::Settings(tab));
            m.selected = m.rows(&s).len() - 1;
            assert_eq!(texts(&m, &s)[m.selected], "BACK");
            m.handle(press(|i| i.select = true), &mut s);
            assert_eq!((m.screen, m.selected), (Screen::Title, 3), "{tab:?}");
        }
    }

    #[test]
    fn a_single_quality_edit_makes_the_preset_custom() {
        let mut s = Settings::default();
        s.prefs.graphics.set_preset(Preset::High);
        let mut m = open_tab(Tab::Graphics);
        pick(&mut m, &s, RowId::Shadows);
        assert_eq!(texts(&m, &s)[m.selected], "SHADOWS  < 4096 >");
        assert_eq!(
            m.handle(press(|i| i.left = true), &mut s),
            MenuAction::GraphicsChanged
        );
        let g = s.prefs.graphics;
        assert_eq!(g.preset, PresetChoice::Custom);
        assert!(!g.auto, "a hand edit stops auto picking");
        assert_eq!(g.quality.shadow_size, 2048);
        assert_eq!(
            g.quality.bloom_levels,
            Preset::High.settings().bloom_levels,
            "only that option moved"
        );
        assert_eq!(texts(&m, &s)[0], "QUALITY PRESET  < CUSTOM >");
        // Picking a preset again applies all of it.
        pick(&mut m, &s, RowId::Preset);
        assert_eq!(
            m.handle(press(|i| i.right = true), &mut s),
            MenuAction::PresetChanged
        );
        assert_eq!(s.prefs.graphics.preset, PresetChoice::High);
        assert_eq!(s.prefs.graphics.quality, Preset::High.settings());
        for (id, action) in [
            (RowId::Bloom, MenuAction::GraphicsChanged),
            (RowId::Fxaa, MenuAction::GraphicsChanged),
            (RowId::Textures, MenuAction::GraphicsChanged),
            (RowId::Anisotropy, MenuAction::GraphicsChanged),
            (RowId::RenderScale, MenuAction::GraphicsChanged),
        ] {
            s.prefs.graphics.set_preset(Preset::High);
            pick(&mut m, &s, id);
            assert_eq!(m.handle(press(|i| i.left = true), &mut s), action, "{id:?}");
            assert_eq!(s.prefs.graphics.preset, PresetChoice::Custom, "{id:?}");
        }
    }

    #[test]
    fn display_audio_and_song_rows_emit_their_actions() {
        let mut s = Settings::default();
        let mut m = open_tab(Tab::Graphics);
        for (id, action) in [
            (RowId::DisplayMode, MenuAction::DisplayChanged),
            (RowId::Vsync, MenuAction::VsyncChanged),
            (RowId::FrameCap, MenuAction::FrameCapChanged),
        ] {
            pick(&mut m, &s, id);
            assert_eq!(m.handle(press(|i| i.right = true), &mut s), action);
        }
        assert_eq!(s.prefs.graphics.display, DisplayMode::Borderless);
        assert!(!s.prefs.graphics.vsync);
        assert_eq!(s.prefs.graphics.frame_cap, Some(30), "None wraps to 30");
        let mut m = open_tab(Tab::Audio);
        pick(&mut m, &s, RowId::MusicVolume);
        assert_eq!(
            m.handle(press(|i| i.left = true), &mut s),
            MenuAction::VolumeChanged
        );
        assert_eq!(s.prefs.audio.music, 95);
        pick(&mut m, &s, RowId::ListenTo);
        assert_eq!(
            m.handle(press(|i| i.right = true), &mut s),
            MenuAction::PreviewSong(Theme::Dust)
        );
        assert_eq!(texts(&m, &s)[m.selected], "LISTEN TO  < DUSTBOWL >");
        assert_eq!(
            m.handle(press(|i| i.left = true), &mut s),
            MenuAction::PreviewSong(Theme::Menu)
        );
    }

    #[test]
    fn sliders_step_on_their_grid_and_clamp() {
        let mut s = Settings::default();
        let mut m = open_tab(Tab::Audio);
        pick(&mut m, &s, RowId::MasterVolume);
        assert_eq!(texts(&m, &s)[m.selected], "MASTER  < 100% >");
        m.handle(press(|i| i.left = true), &mut s);
        assert_eq!(s.prefs.audio.master, 95);
        assert_eq!(texts(&m, &s)[m.selected], "MASTER  < 95% >");
        for _ in 0..30 {
            m.handle(press(|i| i.left = true), &mut s);
        }
        assert_eq!(s.prefs.audio.master, 0, "clamps at the bottom");

        let mut m = open_tab(Tab::Controls);
        pick(&mut m, &s, RowId::Deadzone);
        m.handle(press(|i| i.right = true), &mut s);
        assert_eq!(s.prefs.controls.deadzone, 16, "dead zone steps by 1");
        for _ in 0..30 {
            m.handle(press(|i| i.right = true), &mut s);
        }
        assert_eq!(s.prefs.controls.deadzone, 30);

        let mut m = open_tab(Tab::Camera);
        pick(&mut m, &s, RowId::CameraDistance);
        assert_eq!(texts(&m, &s)[m.selected], "CAMERA DISTANCE  < 4.2 M >");
        m.handle(press(|i| i.right = true), &mut s);
        assert_eq!(s.prefs.camera.distance, 4.5, "off-grid default steps up");
        s.prefs.camera.distance = 4.15;
        m.handle(press(|i| i.left = true), &mut s);
        assert_eq!(s.prefs.camera.distance, 4.0, "and down to its neighbor");
        for _ in 0..20 {
            m.handle(press(|i| i.right = true), &mut s);
        }
        assert_eq!(s.prefs.camera.distance, 7.0);
        pick(&mut m, &s, RowId::CameraHeight);
        m.handle(press(|i| i.right = true), &mut s);
        assert_eq!(s.prefs.camera.height, 1.8);
        pick(&mut m, &s, RowId::Fov);
        m.handle(press(|i| i.right = true), &mut s);
        assert_eq!(s.prefs.camera.fov, 62.0);
        pick(&mut m, &s, RowId::HudScale);
        for _ in 0..20 {
            m.handle(press(|i| i.left = true), &mut s);
        }
        assert_eq!(s.prefs.hud.scale, 70);

        let mut m = open_tab(Tab::Graphics);
        pick(&mut m, &s, RowId::RenderScale);
        assert_eq!(texts(&m, &s)[m.selected], "RENDER SCALE  < 100% >");
        m.handle(press(|i| i.left = true), &mut s);
        assert_eq!(s.prefs.graphics.quality.render_scale, 0.95);
        for _ in 0..20 {
            m.handle(press(|i| i.left = true), &mut s);
        }
        assert_eq!(s.prefs.graphics.quality.render_scale, 0.5);
        let RowKind::Slider { fraction, .. } = m.rows(&s)[m.selected].kind else {
            panic!("render scale is a slider");
        };
        assert_eq!(fraction, 0.0);
    }

    #[test]
    fn actions_fire_only_when_a_value_changes() {
        let mut s = Settings::default();
        let mut m = open_tab(Tab::Graphics);
        pick(&mut m, &s, RowId::RenderScale);
        let before = s.prefs.graphics;
        assert_eq!(
            m.handle(press(|i| i.right = true), &mut s),
            MenuAction::None,
            "render scale is already at its top"
        );
        assert_eq!(s.prefs.graphics, before, "and the preset stays named");
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::None,
            "select (a mouse click) can't raise a slider past its top"
        );
        assert_eq!(s.prefs.graphics, before);
        let mut m = open_tab(Tab::Audio);
        pick(&mut m, &s, RowId::EffectsVolume);
        assert_eq!(
            m.handle(press(|i| i.right = true), &mut s),
            MenuAction::None
        );
        // A click lowers on "<" and raises anywhere else, so a mouse can undo either way.
        m.handle(press(|i| i.left = true), &mut s);
        assert_eq!(s.prefs.audio.effects, 95);
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::VolumeChanged
        );
        assert_eq!(s.prefs.audio.effects, 100);
        // Moving between rows changes nothing.
        assert_eq!(m.handle(press(|i| i.down = true), &mut s), MenuAction::None);
        assert_eq!(s.prefs, Settings::default().prefs);
    }

    #[test]
    fn every_row_has_help_and_settings_rows_match_the_prefs() {
        let s = Settings::default();
        let mut screens = vec![
            Screen::Title,
            Screen::RaceSetup,
            Screen::Circuits,
            Screen::DriverName,
            Screen::Garage,
            Screen::Paused,
            Screen::Results,
            Screen::Online,
            Screen::Lobby,
            Screen::Tailscale,
        ];
        screens.extend(Tab::ALL.map(Screen::Settings));
        for screen in screens {
            let m = MenuState::new(screen);
            for row in m.rows(&s) {
                assert!(!row.help.is_empty(), "{screen:?} {:?}", row.id);
            }
        }
        let count = |tab| open_tab(tab).rows(&s).len() - 1;
        assert_eq!(count(Tab::Graphics), 12);
        assert_eq!(count(Tab::Audio), 7);
        // Steering is two rows, left and right, so each direction can be rebound.
        assert_eq!(count(Tab::Controls), 13);
        assert_eq!(count(Tab::Camera), 10);
        assert_eq!(count(Tab::Gameplay), 3);
        let fresh = open_tab(Tab::Graphics).rows(&s);
        assert!(
            !fresh[0].new && fresh[2].new,
            "preset is old, render scale new"
        );
    }

    #[test]
    fn legacy_strings_for_each_kind() {
        let s = Settings::default();
        let rows = open_tab(Tab::Graphics).rows(&s);
        assert_eq!(rows[2].legacy_string(), "RENDER SCALE  < 100% >");
        let mut m = MenuState::new(Screen::Circuits);
        m.go(Screen::Circuits);
        let rows = texts(&m, &s);
        assert!(rows[0].ends_with("  EQUIPPED"));
        assert!(!rows[1].contains("  "), "an empty value draws no gap");
    }

    #[test]
    fn results_back_returns_to_title() {
        let mut s = Settings::default();
        let mut m = MenuState::new(Screen::Results);
        assert_eq!(
            m.handle(press(|i| i.back = true), &mut s),
            MenuAction::ToTitle
        );
    }

    #[test]
    fn pause_menu() {
        let mut s = Settings::default();
        let mut m = MenuState::new(Screen::Paused);
        assert_eq!(
            texts(&m, &s),
            ["RESUME", "RESTART", "SETTINGS", "MAIN MENU", "QUIT"]
        );
        assert_eq!(
            m.handle(press(|i| i.back = true), &mut s),
            MenuAction::Resume
        );
        m.selected = 1;
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::Restart
        );
        m.selected = 3;
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::ToTitle
        );
        m.selected = 4;
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::Quit
        );
    }
}
