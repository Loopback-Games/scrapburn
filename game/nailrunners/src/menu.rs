//! Menus: title, settings, pause, results. Pure state and navigation; drawing lives in hud.rs.

use rivet_render::Preset;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Title,
    Settings,
    Playing,
    Paused,
    Results,
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
}

/// What the app should do in response to a menu input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    None,
    StartRace,
    StartTimeTrial,
    Resume,
    Restart,
    ToTitle,
    Quit,
    /// The graphics preset changed; rebuild the renderer.
    PresetChanged,
    /// A different track was picked on the title screen.
    TrackChanged,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub preset: Preset,
    pub laps: u32,
    pub rivals: usize,
    pub fps: FpsDisplay,
    /// Index into `track::TRACKS`.
    pub track: usize,
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

impl Default for Settings {
    fn default() -> Self {
        Self {
            preset: Preset::Medium,
            laps: 3,
            rivals: 7,
            fps: FpsDisplay::Off,
            track: 0,
        }
    }
}

/// Title screen rows.
pub const TITLE_TRACK: usize = 2;

pub const SETTING_PRESET: usize = 0;
pub const SETTING_LAPS: usize = 1;
pub const SETTING_RIVALS: usize = 2;
pub const SETTING_FPS: usize = 3;
const LAP_CHOICES: [u32; 3] = [1, 3, 5];
const RIVAL_CHOICES: [usize; 3] = [3, 5, 7];

fn cycle<T: PartialEq + Copy>(choices: &[T], current: T, dir: i32) -> T {
    let i = choices.iter().position(|c| *c == current).unwrap_or(0) as i32;
    choices[(i + dir).rem_euclid(choices.len() as i32) as usize]
}

/// Current screen and highlighted row. `return_to` is where Settings' Back goes.
#[derive(Debug, Clone)]
pub struct MenuState {
    pub screen: Screen,
    pub selected: usize,
    return_to: Screen,
}

impl MenuState {
    pub fn new(screen: Screen) -> Self {
        Self {
            screen,
            selected: 0,
            return_to: Screen::Title,
        }
    }

    pub fn go(&mut self, screen: Screen) {
        if screen == Screen::Settings {
            self.return_to = self.screen;
        }
        self.screen = screen;
        self.selected = 0;
    }

    /// Rows of the current screen.
    pub fn items(&self, s: &Settings) -> Vec<String> {
        let preset = match s.preset {
            Preset::Low => "LOW",
            Preset::Medium => "MEDIUM",
            Preset::High => "HIGH",
        };
        match self.screen {
            Screen::Title => vec![
                "RACE".into(),
                "TIME TRIAL".into(),
                format!(
                    "TRACK  < {} >",
                    crate::track::TRACKS[s.track % crate::track::TRACKS.len()].name
                ),
                "SETTINGS".into(),
                "QUIT".into(),
            ],
            Screen::Settings => vec![
                format!("GRAPHICS  < {preset} >"),
                format!("LAPS  < {} >", s.laps),
                format!("RIVALS  < {} >", s.rivals),
                format!("SHOW FPS  < {} >", s.fps.label()),
                "BACK".into(),
            ],
            Screen::Paused => vec![
                "RESUME".into(),
                "RESTART".into(),
                "SETTINGS".into(),
                "MAIN MENU".into(),
                "QUIT".into(),
            ],
            Screen::Results => vec!["CONTINUE".into(), "RACE AGAIN".into()],
            Screen::Playing => vec![],
        }
    }

    pub fn handle(&mut self, input: MenuInput, s: &mut Settings) -> MenuAction {
        let n = self.items(s).len();
        if n == 0 {
            return MenuAction::None;
        }
        if input.up {
            self.selected = (self.selected + n - 1) % n;
        }
        if input.down {
            self.selected = (self.selected + 1) % n;
        }
        let dir = input.right as i32 - input.left as i32;
        match self.screen {
            Screen::Title => {
                let tracks = crate::track::TRACKS.len();
                if self.selected == TITLE_TRACK && (dir != 0 || input.select) {
                    let step = if dir != 0 { dir } else { 1 };
                    s.track = (s.track as i32 + step).rem_euclid(tracks as i32) as usize;
                    return MenuAction::TrackChanged;
                }
                match (input.select, self.selected) {
                    (true, 0) => MenuAction::StartRace,
                    (true, 1) => MenuAction::StartTimeTrial,
                    (true, 3) => {
                        self.go(Screen::Settings);
                        MenuAction::None
                    }
                    (true, _) => MenuAction::Quit,
                    _ => MenuAction::None,
                }
            }
            Screen::Settings => {
                if input.back || (input.select && self.selected == n - 1) {
                    let back = self.return_to;
                    self.go(back);
                    return MenuAction::None;
                }
                let step = if dir != 0 {
                    dir
                } else if input.select {
                    1
                } else {
                    0
                };
                if step == 0 {
                    return MenuAction::None;
                }
                match self.selected {
                    SETTING_PRESET => {
                        s.preset =
                            cycle(&[Preset::Low, Preset::Medium, Preset::High], s.preset, step);
                        return MenuAction::PresetChanged;
                    }
                    SETTING_LAPS => s.laps = cycle(&LAP_CHOICES, s.laps, step),
                    SETTING_RIVALS => s.rivals = cycle(&RIVAL_CHOICES, s.rivals, step),
                    SETTING_FPS => {
                        s.fps = if step > 0 {
                            s.fps.next()
                        } else {
                            s.fps.next().next()
                        };
                    }
                    _ => {}
                }
                MenuAction::None
            }
            Screen::Paused => {
                if input.back {
                    return MenuAction::Resume;
                }
                match (input.select, self.selected) {
                    (true, 0) => MenuAction::Resume,
                    (true, 1) => MenuAction::Restart,
                    (true, 2) => {
                        self.go(Screen::Settings);
                        MenuAction::None
                    }
                    (true, 3) => MenuAction::ToTitle,
                    (true, _) => MenuAction::Quit,
                    _ => MenuAction::None,
                }
            }
            Screen::Results if input.back => MenuAction::ToTitle,
            Screen::Results => match (input.select, self.selected) {
                (true, 0) => MenuAction::ToTitle,
                (true, _) => MenuAction::Restart,
                _ => MenuAction::None,
            },
            Screen::Playing => MenuAction::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(f: impl Fn(&mut MenuInput)) -> MenuInput {
        let mut i = MenuInput::default();
        f(&mut i);
        i
    }

    #[test]
    fn navigation_wraps() {
        let mut m = MenuState::new(Screen::Title);
        let n = m.items(&Settings::default()).len();
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
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::StartRace
        );
        m.selected = 1;
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::StartTimeTrial
        );
        m.selected = 3;
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::None
        );
        assert_eq!(m.screen, Screen::Settings);
        assert_eq!(m.handle(press(|i| i.back = true), &mut s), MenuAction::None);
        assert_eq!(m.screen, Screen::Title, "back returns to the title");
    }

    #[test]
    fn title_track_row_cycles_tracks() {
        let mut s = Settings::default();
        let mut m = MenuState::new(Screen::Title);
        m.selected = TITLE_TRACK;
        assert_eq!(
            m.handle(press(|i| i.right = true), &mut s),
            MenuAction::TrackChanged
        );
        assert_eq!(s.track, 1);
        m.handle(press(|i| i.left = true), &mut s);
        m.handle(press(|i| i.left = true), &mut s);
        assert_eq!(s.track, crate::track::TRACKS.len() - 1, "wraps");
        assert!(m.items(&s)[TITLE_TRACK].contains(crate::track::TRACKS[s.track].name));
    }

    #[test]
    fn settings_cycle() {
        let mut s = Settings::default();
        let mut m = MenuState::new(Screen::Settings);
        m.selected = SETTING_LAPS;
        assert_eq!(s.laps, 3);
        m.handle(press(|i| i.right = true), &mut s);
        assert_eq!(s.laps, 5);
        m.handle(press(|i| i.right = true), &mut s);
        assert_eq!(s.laps, 1, "wraps");
        m.selected = SETTING_PRESET;
        let before = s.preset;
        assert_eq!(
            m.handle(press(|i| i.right = true), &mut s),
            MenuAction::PresetChanged
        );
        assert_ne!(s.preset, before);
    }

    #[test]
    fn fps_display_cycles() {
        let mut s = Settings::default();
        let mut m = MenuState::new(Screen::Settings);
        m.selected = SETTING_FPS;
        m.handle(press(|i| i.right = true), &mut s);
        assert_eq!(s.fps, FpsDisplay::Counter);
        m.handle(press(|i| i.right = true), &mut s);
        assert_eq!(s.fps, FpsDisplay::Graph);
        m.handle(press(|i| i.left = true), &mut s);
        assert_eq!(s.fps, FpsDisplay::Counter);
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
    }
}
