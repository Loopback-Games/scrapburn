//! Menus: title, settings, pause, results. Pure state and navigation; drawing lives in hud.rs.

use rivet_render::Preset;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Title,
    RaceSetup,
    Circuits,
    Controls,
    DriverName,
    Garage,
    Settings,
    Playing,
    Paused,
    Results,
    /// Host or join an online game.
    Online,
    /// Waiting for the host to start.
    Lobby,
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
    SaveGarage,
    SaveName,
    StartTimeTrial,
    Resume,
    Restart,
    ToTitle,
    Quit,
    /// Start an elimination race.
    StartElimination,
    /// The graphics preset changed; rebuild the renderer.
    PresetChanged,
    /// A different track was picked on the title screen.
    TrackChanged,
    /// Background scenery was switched on or off.
    SceneryChanged,
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
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub loadout: crate::kart::Loadout,
    pub preset: Preset,
    pub laps: u32,
    pub rivals: usize,
    pub fps: FpsDisplay,
    /// Index into `track::TRACKS`.
    pub track: usize,
    /// Background scenery (props and rocks around the circuits).
    pub scenery: bool,
    /// The preset is being picked automatically.
    pub auto_preset: bool,
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
            loadout: Default::default(),
            preset: Preset::Medium,
            laps: 3,
            rivals: 7,
            fps: FpsDisplay::Off,
            track: 0,
            scenery: true,
            auto_preset: false,
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
/// Online screen: the typed address row.
pub const ONLINE_ADDRESS: usize = 4;
/// Longest typed address.
const MAX_ADDRESS: usize = 64;

pub const SETTING_PRESET: usize = 0;
pub const SETTING_LAPS: usize = 1;
pub const SETTING_RIVALS: usize = 2;
pub const SETTING_FPS: usize = 3;
pub const SETTING_SCENERY: usize = 4;
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
    pub mode: RaceMode,
    pub gamepad_guide: bool,
    pub name_entry: String,
    /// The address typed on the Online screen.
    pub join_addr: String,
    /// Online tailnet peers (name, address) offered on the Online screen.
    pub peers: Vec<(String, String)>,
    /// This machine hosts the lobby.
    pub lobby_host: bool,
    /// The host picked Elimination for the online race.
    pub lobby_elimination: bool,
    /// The address the last JoinGame action refers to.
    join_choice: Option<String>,
}

impl MenuState {
    pub fn new(screen: Screen) -> Self {
        Self {
            screen,
            selected: 0,
            return_to: Screen::Title,
            mode: RaceMode::Race,
            gamepad_guide: false,
            name_entry: String::new(),
            join_addr: String::new(),
            peers: Vec::new(),
            lobby_host: false,
            lobby_elimination: false,
            join_choice: None,
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

    /// Where to connect after a JoinGame action.
    pub fn join_target(&self) -> Option<String> {
        self.join_choice.clone()
    }

    pub fn go(&mut self, screen: Screen) {
        if matches!(
            screen,
            Screen::Settings | Screen::Controls | Screen::Circuits
        ) {
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
            Screen::Title => [
                "SOLO RACE",
                "MULTIPLAYER",
                "GARAGE",
                "CONTROLS",
                "SETTINGS",
                "QUIT",
            ]
            .map(String::from)
            .to_vec(),
            Screen::RaceSetup => vec![
                "START RACE".into(),
                format!("MODE  < {} >", self.mode.name()),
                format!("CIRCUIT  {}", crate::track::TRACKS[s.track].name),
                "BACK".into(),
            ],
            Screen::Circuits => crate::track::TRACKS
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    if i == s.track {
                        format!("{}  EQUIPPED", t.name)
                    } else {
                        t.name.into()
                    }
                })
                .chain(std::iter::once("DONE".into()))
                .collect(),
            Screen::Controls => vec![
                format!(
                    "DEVICE  < {} >",
                    if self.gamepad_guide {
                        "GAMEPAD"
                    } else {
                        "KEYBOARD"
                    }
                ),
                "BACK".into(),
            ],
            Screen::DriverName => vec![
                format!("NAME  {}_", self.name_entry),
                "SAVE NAME".into(),
                "CANCEL".into(),
            ],
            Screen::Garage => vec![
                format!(
                    "CHASSIS  < {} >",
                    crate::kart::KARTS[s.loadout.design]
                        .name
                        .to_ascii_uppercase()
                ),
                format!("FINISH  < {} >", crate::kart::FINISHES[s.loadout.finish]),
                "DONE".into(),
            ],
            Screen::Settings => vec![
                if s.auto_preset {
                    format!("GRAPHICS  < {preset} (AUTO) >")
                } else {
                    format!("GRAPHICS  < {preset} >")
                },
                format!("LAPS  < {} >", s.laps),
                format!("RIVALS  < {} >", s.rivals),
                format!("SHOW FPS  < {} >", s.fps.label()),
                format!("SCENERY  < {} >", if s.scenery { "ON" } else { "OFF" }),
                "BACK".into(),
            ],
            Screen::Paused => vec![
                "RESUME".into(),
                "RESTART".into(),
                "SETTINGS".into(),
                "CONTROLS".into(),
                "MAIN MENU".into(),
                "QUIT".into(),
            ],
            Screen::Results => vec!["CONTINUE".into(), "RACE AGAIN".into()],
            Screen::Online => {
                let mut v = vec![
                    "HOST PRIVATE RACE".into(),
                    "OPEN RACE INVITE".into(),
                    "CHANGE RACER NAME".into(),
                    "HOST GAME".into(),
                    if self.join_addr.is_empty() {
                        "JOIN  TYPE AN ADDRESS_".to_string()
                    } else {
                        format!("JOIN  {}_", self.join_addr.to_ascii_uppercase())
                    },
                ];
                v.extend(self.peers.iter().map(|(n, _)| format!("JOIN  {n}")));
                v.push("BACK".into());
                v
            }
            Screen::Lobby if self.lobby_host => vec![
                "START RACE".into(),
                format!(
                    "MODE  < {} >",
                    if self.lobby_elimination {
                        "ELIMINATION"
                    } else {
                        "RACE"
                    }
                ),
                format!(
                    "CIRCUIT  {}",
                    crate::track::TRACKS[s.track % crate::track::TRACKS.len()].name
                ),
                "LEAVE".into(),
            ],
            Screen::Lobby => vec!["WAITING FOR THE HOST".into(), "LEAVE".into()],
            Screen::Playing => vec![],
        }
    }

    pub fn handle(&mut self, input: MenuInput, s: &mut Settings) -> MenuAction {
        let n = self.items(s).len();
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
        let dir = input.right as i32 - input.left as i32;
        match self.screen {
            Screen::Title => {
                if input.select {
                    let dest = match self.selected {
                        0 => Screen::RaceSetup,
                        1 => Screen::Online,
                        2 => Screen::Garage,
                        3 => Screen::Controls,
                        4 => Screen::Settings,
                        _ => return MenuAction::Quit,
                    };
                    self.go(dest);
                }
                MenuAction::None
            }
            Screen::RaceSetup => {
                if input.back || (input.select && self.selected == 3) {
                    self.go(Screen::Title);
                } else if self.selected == 0 && input.select {
                    return match self.mode {
                        RaceMode::Race => MenuAction::StartRace,
                        RaceMode::TimeTrial => MenuAction::StartTimeTrial,
                        RaceMode::Elimination => MenuAction::StartElimination,
                    };
                } else if self.selected == 1 && (dir != 0 || input.select) {
                    self.mode = cycle(
                        &[RaceMode::Race, RaceMode::TimeTrial, RaceMode::Elimination],
                        self.mode,
                        if dir == 0 { 1 } else { dir },
                    );
                } else if self.selected == 2 && input.select {
                    self.go(Screen::Circuits);
                    self.selected = s.track;
                }
                MenuAction::None
            }
            Screen::Circuits => {
                if input.back || (input.select && self.selected == n - 1) {
                    self.go(self.return_to);
                    self.selected = 2;
                } else if input.select && s.track != self.selected {
                    s.track = self.selected;
                    return MenuAction::TrackChanged;
                }
                MenuAction::None
            }
            Screen::Controls => {
                if input.back || (input.select && self.selected == 1) {
                    self.go(self.return_to);
                    self.selected = 3;
                } else if self.selected == 0 && (dir != 0 || input.select) {
                    self.gamepad_guide = !self.gamepad_guide;
                }
                MenuAction::None
            }
            Screen::DriverName => {
                if input.back || (input.select && self.selected == 2) {
                    self.go(Screen::Online);
                } else if input.select && !self.name_entry.trim().is_empty() {
                    self.go(Screen::Online);
                    return MenuAction::SaveName;
                }
                MenuAction::None
            }
            Screen::Garage => {
                if input.back || (input.select && self.selected == 2) {
                    self.go(Screen::Title);
                    self.selected = 2;
                    return MenuAction::SaveGarage;
                }
                let step = if dir != 0 {
                    dir
                } else {
                    i32::from(input.select)
                };
                let (index, count) = match self.selected {
                    0 => (&mut s.loadout.design, crate::kart::KARTS.len()),
                    1 => (&mut s.loadout.finish, crate::kart::FINISHES.len()),
                    _ => return MenuAction::None,
                };
                *index = (*index as i32 + step).rem_euclid(count as i32) as usize;
                MenuAction::None
            }
            Screen::Settings => {
                if input.back || (input.select && self.selected == n - 1) {
                    let back = self.return_to;
                    self.go(back);
                    self.selected = if back == Screen::Paused { 2 } else { 4 };
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
                        s.auto_preset = false;
                        return MenuAction::PresetChanged;
                    }
                    SETTING_LAPS => s.laps = cycle(&LAP_CHOICES, s.laps, step),
                    SETTING_RIVALS => s.rivals = cycle(&RIVAL_CHOICES, s.rivals, step),
                    SETTING_SCENERY => {
                        s.scenery = !s.scenery;
                        return MenuAction::SceneryChanged;
                    }
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
                    (true, 3) => {
                        self.go(Screen::Controls);
                        MenuAction::None
                    }
                    (true, 4) => MenuAction::ToTitle,
                    (true, _) => MenuAction::Quit,
                    _ => MenuAction::None,
                }
            }
            Screen::Online => {
                if input.back || (input.select && self.selected == n - 1) {
                    self.go(Screen::Title);
                    self.selected = 1;
                    return MenuAction::None;
                }
                if !input.select {
                    return MenuAction::None;
                }
                match self.selected {
                    3 => MenuAction::HostGame,
                    0 => MenuAction::PrivateHost,
                    1 => MenuAction::PrivateJoin,
                    2 => {
                        self.name_entry.clear();
                        self.go(Screen::DriverName);
                        MenuAction::None
                    }
                    ONLINE_ADDRESS if self.join_addr.is_empty() => MenuAction::None,
                    ONLINE_ADDRESS => {
                        self.join_choice = Some(self.join_addr.clone());
                        MenuAction::JoinGame
                    }
                    k => {
                        self.join_choice =
                            self.peers.get(k - ONLINE_ADDRESS - 1).map(|p| p.1.clone());
                        if self.join_choice.is_some() {
                            MenuAction::JoinGame
                        } else {
                            MenuAction::None
                        }
                    }
                }
            }
            Screen::Lobby => {
                if input.back || (input.select && self.selected == n - 1) {
                    return MenuAction::LeaveOnline;
                }
                if self.lobby_host {
                    if self.selected == 1 && (dir != 0 || input.select) {
                        self.lobby_elimination = !self.lobby_elimination;
                    }
                    if self.selected == 2 && input.select {
                        self.go(Screen::Circuits);
                        self.selected = s.track;
                        return MenuAction::None;
                    }
                    if self.selected == 2 && dir != 0 {
                        let tracks = crate::track::TRACKS.len() as i32;
                        s.track = (s.track as i32 + dir).rem_euclid(tracks) as usize;
                        return MenuAction::TrackChanged;
                    }
                    if input.select && self.selected == 0 {
                        return MenuAction::LobbyStart;
                    }
                }
                MenuAction::None
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
        m.selected = 4;
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::None
        );
        assert_eq!(m.screen, Screen::Settings);
        assert_eq!(m.handle(press(|i| i.back = true), &mut s), MenuAction::None);
        assert_eq!(m.screen, Screen::Title, "back returns to the title");
    }

    #[test]
    fn garage_equips_chassis_and_finish() {
        let mut menu = MenuState::new(Screen::Title);
        let mut settings = Settings::default();
        menu.selected = menu
            .items(&settings)
            .iter()
            .position(|label| label == "GARAGE")
            .unwrap();
        menu.handle(press(|i| i.select = true), &mut settings);
        assert_eq!(menu.screen, Screen::Garage);
        menu.handle(press(|i| i.left = true), &mut settings);
        assert!(menu.items(&settings)[0].contains("SPEEDSTER"));
        menu.handle(press(|i| i.down = true), &mut settings);
        menu.handle(press(|i| i.right = true), &mut settings);
        assert!(menu.items(&settings)[1].contains("GLACIER"));
        assert_eq!(
            menu.handle(press(|i| i.back = true), &mut settings),
            MenuAction::SaveGarage
        );
        assert_eq!(menu.items(&settings)[menu.selected], "GARAGE");
        assert_eq!(settings.loadout.design, 2);
    }

    #[test]
    fn circuit_picker_applies_choice_and_returns_to_setup() {
        let mut s = Settings::default();
        let mut m = MenuState::new(Screen::RaceSetup);
        m.selected = 2;
        m.handle(press(|i| i.select = true), &mut s);
        assert_eq!(m.screen, Screen::Circuits);
        m.handle(press(|i| i.down = true), &mut s);
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::TrackChanged
        );
        assert_eq!(s.track, 1);
        m.handle(press(|i| i.back = true), &mut s);
        assert_eq!((m.screen, m.selected), (Screen::RaceSetup, 2));
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
        assert!(m.items(&s)[ONLINE_ADDRESS].contains("100.64.0.2:27960"));
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::JoinGame
        );
        assert_eq!(m.join_target().as_deref(), Some("100.64.0.2:27960"));
        // Tailnet peers appear as their own rows.
        m.peers = vec![("THINKPAD".into(), "100.64.0.9".into())];
        m.selected = ONLINE_ADDRESS + 1;
        assert!(m.items(&s)[m.selected].contains("THINKPAD"));
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::JoinGame
        );
        assert_eq!(m.join_target().as_deref(), Some("100.64.0.9"));
        // An empty address does nothing; Back returns to the title.
        m.join_addr.clear();
        m.selected = ONLINE_ADDRESS;
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::None
        );
        m.handle(press(|i| i.back = true), &mut s);
        assert_eq!(m.screen, Screen::Title);
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
        m.selected = 2;
        m.handle(press(|i| i.select = true), &mut s);
        m.type_text("NEW NAME");
        assert_eq!(m.handle(press(|i| i.back = true), &mut s), MenuAction::None);
        assert_eq!(m.screen, Screen::Online);
    }

    #[test]
    fn pause_controls_return_to_the_same_row() {
        let mut m = MenuState::new(Screen::Paused);
        let mut s = Settings::default();
        m.selected = 3;
        m.handle(press(|i| i.select = true), &mut s);
        m.handle(press(|i| i.right = true), &mut s);
        assert!(m.gamepad_guide);
        m.handle(press(|i| i.back = true), &mut s);
        assert_eq!((m.screen, m.selected), (Screen::Paused, 3));
    }

    #[test]
    fn private_race_actions_stay_available_with_peers() {
        let mut menu = MenuState::new(Screen::Online);
        let mut settings = Settings::default();
        menu.peers.push(("Friend".into(), "100.64.0.1".into()));
        let rows = menu.items(&settings);
        for (label, action) in [
            ("HOST PRIVATE RACE", MenuAction::PrivateHost),
            ("OPEN RACE INVITE", MenuAction::PrivateJoin),
        ] {
            menu.selected = rows.iter().position(|row| row == label).unwrap();
            assert_eq!(
                menu.handle(press(|i| i.select = true), &mut settings),
                action
            );
        }
    }

    #[test]
    fn lobby_host_starts_and_picks_the_mode_guests_wait() {
        let mut s = Settings::default();
        let mut m = MenuState::new(Screen::Lobby);
        m.lobby_host = true;
        assert!(m.items(&s)[1].contains("RACE"));
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
        assert!(m.items(&s)[1].contains("ELIMINATION"));
        m.selected = 2;
        assert!(m.items(&s)[2].contains(crate::track::TRACKS[0].name));
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
    fn scenery_toggles_and_defaults_on() {
        let mut s = Settings::default();
        assert!(s.scenery);
        let mut m = MenuState::new(Screen::Settings);
        m.selected = SETTING_SCENERY;
        assert!(m.items(&s)[SETTING_SCENERY].contains("ON"));
        assert_eq!(
            m.handle(press(|i| i.right = true), &mut s),
            MenuAction::SceneryChanged
        );
        assert!(!s.scenery);
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
        m.selected = 4;
        assert_eq!(
            m.handle(press(|i| i.select = true), &mut s),
            MenuAction::ToTitle
        );
    }
}
