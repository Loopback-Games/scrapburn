//! Command-line arguments.

pub use rivet_platform::AdapterPreference;
pub use rivet_render::{PresentChoice, Preset};

#[derive(Debug, Clone, PartialEq)]
pub struct Args {
    pub adapter: AdapterPreference,
    pub size: (u32, u32),
    pub present: PresentChoice,
    /// Seconds of measured benchmark after warmup; the autopilot drives.
    pub bench: Option<u32>,
    /// Render to an offscreen image instead of the window (bench only). For GPUs that can
    /// render but not present to this display, e.g. an iGPU when the monitor is on a dGPU.
    pub offscreen: bool,
    /// Write the last offscreen bench frame here as PNG.
    pub screenshot: Option<std::path::PathBuf>,
    /// `None`: pick automatically (benchmarks use Medium).
    pub preset: Option<Preset>,
    pub mode: Mode,
    /// Benchmark the title screen (attract mode) instead of a race.
    pub attract: bool,
    /// Open directly to the garage.
    pub garage: bool,
    /// Index into `track::TRACKS`.
    pub track: usize,
    /// Host an online game at launch.
    pub host: bool,
    /// Use a private ephemeral UDP socket for the Tailcat launcher.
    pub host_loopback: bool,
    /// Join the online game at this address at launch.
    pub join: Option<String>,
    /// Player name shown to others online.
    pub name: Option<String>,
    /// Open this menu screen at launch (for offscreen screenshots of the menus).
    pub menu: Option<MenuShot>,
    /// Reduced motion: the menu embers and the molten flow stand still.
    pub no_motion: bool,
}

/// A menu screen `--menu` opens. Names match the design mockup's screens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuShot {
    Title,
    RaceSetup,
    Tracks,
    Garage,
    Multiplayer,
    /// A stand-in host lobby (no network).
    Lobby,
    /// A race paused at the start.
    Pause,
    /// The results of a race that has not finished.
    Results,
    Graphics,
    Audio,
    Controls,
    Camera,
    Gameplay,
}

impl MenuShot {
    pub const ALL: [(MenuShot, &'static str); 13] = [
        (MenuShot::Title, "title"),
        (MenuShot::RaceSetup, "race-setup"),
        (MenuShot::Tracks, "tracks"),
        (MenuShot::Garage, "garage"),
        (MenuShot::Multiplayer, "multiplayer"),
        (MenuShot::Lobby, "lobby"),
        (MenuShot::Pause, "pause"),
        (MenuShot::Results, "results"),
        (MenuShot::Graphics, "graphics"),
        (MenuShot::Audio, "audio"),
        (MenuShot::Controls, "controls"),
        (MenuShot::Camera, "camera"),
        (MenuShot::Gameplay, "gameplay"),
    ];

    /// Pause and Results sit on top of a race; the rest open from the title screen.
    pub fn in_race(self) -> bool {
        matches!(self, MenuShot::Pause | MenuShot::Results)
    }
}

impl std::str::FromStr for MenuShot {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.to_ascii_lowercase();
        Self::ALL
            .iter()
            .find(|(_, name)| *name == s)
            .map(|(m, _)| *m)
            .ok_or_else(|| {
                let names: Vec<&str> = Self::ALL.iter().map(|(_, n)| *n).collect();
                format!("unknown menu '{s}' (use {})", names.join(", "))
            })
    }
}

/// What to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Combat race against seven rivals.
    #[default]
    Race,
    /// Solo laps against your best-lap ghost.
    TimeTrial,
    /// Last place is knocked out every lap until one racer survives.
    Elimination,
}

impl std::str::FromStr for Mode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "race" => Ok(Self::Race),
            "timetrial" | "time-trial" | "tt" => Ok(Self::TimeTrial),
            "elimination" | "elim" => Ok(Self::Elimination),
            other => Err(format!(
                "unknown mode '{other}' (use race, timetrial or elimination)"
            )),
        }
    }
}

pub const USAGE: &str = "usage: scrapburn [--adapter auto|integrated|discrete|<name>] [--size WxH] \
[--present vsync|immediate] [--mode race|timetrial|elimination] [--track dustbowl|castle|skyway|orchard|helios] [--garage] [--host | --join ADDR] [--name NAME] [--preset low|medium|high] [--no-motion] \
[--menu title|race-setup|tracks|garage|multiplayer|lobby|pause|results|graphics|audio|controls|camera|gameplay] \
[--bench SECS [--offscreen [--screenshot FILE.png]]]

  --no-motion  reduced motion: the menu embers and the molten flow stand still
  --menu NAME  open that menu screen at launch (with --bench --offscreen, for screenshots)";

/// Parses CLI args (without the program name). `env_adapter` is `RIVET_ADAPTER`.
pub fn parse(
    mut args: impl Iterator<Item = String>,
    env_adapter: Option<String>,
) -> Result<Args, String> {
    let mut adapter =
        env_adapter.map(|s| s.parse().expect("AdapterPreference parsing is infallible"));
    let mut size = (1600, 900);
    let mut present = None;
    let mut bench = None;
    let mut offscreen = false;
    let mut screenshot = None;
    let mut preset = None;
    let mut mode = Mode::Race;
    let mut attract = false;
    let mut garage = false;
    let mut track = 0;
    let mut host = false;
    let mut host_loopback = false;
    let mut join = None;
    let mut name = None;
    let mut menu: Option<MenuShot> = None;
    let mut no_motion = false;
    while let Some(flag) = args.next() {
        let mut value = || {
            args.next()
                .ok_or_else(|| format!("{flag} needs a value\n{USAGE}"))
        };
        match flag.as_str() {
            "--adapter" => adapter = Some(value()?.parse().expect("infallible")),
            "--size" => {
                let v = value()?;
                let (w, h) = v
                    .split_once(['x', 'X'])
                    .ok_or_else(|| format!("--size expects WxH, got '{v}'"))?;
                let parse_dim = |s: &str| {
                    s.parse::<u32>()
                        .ok()
                        .filter(|d| (64..=16384).contains(d))
                        .ok_or_else(|| format!("--size expects WxH (64..16384), got '{v}'"))
                };
                size = (parse_dim(w)?, parse_dim(h)?);
            }
            "--present" => present = Some(value()?.parse::<PresentChoice>()?),
            "--bench" => {
                let v = value()?;
                bench = Some(
                    v.parse::<u32>()
                        .ok()
                        .filter(|s| *s > 0)
                        .ok_or_else(|| format!("--bench expects seconds > 0, got '{v}'"))?,
                );
            }
            "--offscreen" => offscreen = true,
            "--preset" => preset = Some(value()?.parse()?),
            "--mode" => mode = value()?.parse()?,
            "--garage" => garage = true,
            "--attract" => attract = true,
            "--host-loopback" => {
                host = true;
                host_loopback = true;
            }
            "--host" => host = true,
            "--join" => join = Some(value()?),
            "--name" => name = Some(value()?),
            "--menu" => menu = Some(value()?.parse()?),
            "--no-motion" => no_motion = true,
            "--track" => {
                let v = value()?.to_ascii_lowercase();
                track = crate::track::TRACKS
                    .iter()
                    .position(|t| t.name.to_ascii_lowercase().contains(&v))
                    .ok_or_else(|| {
                        format!("unknown track '{v}' (dustbowl, castle, skyway, orchard, helios)")
                    })?;
            }
            "--screenshot" => screenshot = Some(std::path::PathBuf::from(value()?)),
            "-h" | "--help" => return Err(USAGE.to_string()),
            other => return Err(format!("unknown argument '{other}'\n{USAGE}")),
        }
    }
    if offscreen && bench.is_none() {
        return Err(format!("--offscreen only works with --bench\n{USAGE}"));
    }
    if screenshot.is_some() && !offscreen {
        return Err(format!(
            "--screenshot needs --bench and --offscreen\n{USAGE}"
        ));
    }
    // The title-screen menus run over the attract race.
    if menu.is_some_and(|m| !m.in_race()) {
        attract = true;
    }
    let present = present.unwrap_or(if bench.is_some() {
        PresentChoice::Immediate
    } else {
        PresentChoice::Vsync
    });
    Ok(Args {
        adapter: adapter.unwrap_or_default(),
        size,
        present,
        bench,
        offscreen,
        screenshot,
        preset,
        mode,
        attract,
        garage,
        track,
        host,
        host_loopback,
        join,
        name,
        menu,
        no_motion,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(a: &[&str], env: Option<&str>) -> Result<Args, String> {
        parse(a.iter().map(|s| s.to_string()), env.map(str::to_string))
    }

    #[test]
    fn args_defaults() {
        let a = parse_strs(&[], None).unwrap();
        assert_eq!(a.size, (1600, 900));
        assert_eq!(a.present, PresentChoice::Vsync);
        assert_eq!(a.bench, None);
        assert_eq!(a.adapter, AdapterPreference::Auto);
    }

    #[test]
    fn args_env_adapter_and_flag_override() {
        assert_eq!(
            parse_strs(&[], Some("discrete")).unwrap().adapter,
            AdapterPreference::Discrete
        );
        assert_eq!(
            parse_strs(&["--adapter", "integrated"], Some("discrete"))
                .unwrap()
                .adapter,
            AdapterPreference::Integrated
        );
    }

    #[test]
    fn args_bench_forces_immediate() {
        let a = parse_strs(&["--bench", "30"], None).unwrap();
        assert_eq!(a.bench, Some(30));
        assert_eq!(a.present, PresentChoice::Immediate);
        let a = parse_strs(&["--bench", "30", "--present", "vsync"], None).unwrap();
        assert_eq!(a.present, PresentChoice::Vsync);
    }

    #[test]
    fn args_offscreen() {
        assert!(!parse_strs(&[], None).unwrap().offscreen);
        assert!(
            parse_strs(&["--bench", "30", "--offscreen"], None)
                .unwrap()
                .offscreen
        );
        let e = parse_strs(&["--offscreen"], None).unwrap_err();
        assert!(e.contains("--bench"), "{e}");
    }

    #[test]
    fn args_screenshot() {
        let a = parse_strs(
            &["--bench", "5", "--offscreen", "--screenshot", "shot.png"],
            None,
        )
        .unwrap();
        assert_eq!(
            a.screenshot.as_deref(),
            Some(std::path::Path::new("shot.png"))
        );
        assert!(
            parse_strs(&["--bench", "5", "--screenshot", "s.png"], None)
                .unwrap_err()
                .contains("--offscreen")
        );
    }

    #[test]
    fn args_preset() {
        assert_eq!(
            parse_strs(&[], None).unwrap().preset,
            None,
            "auto by default"
        );
        assert_eq!(
            parse_strs(&["--preset", "low"], None).unwrap().preset,
            Some(Preset::Low)
        );
        assert!(parse_strs(&["--preset", "potato"], None).is_err());
    }

    #[test]
    fn args_mode() {
        assert_eq!(parse_strs(&[], None).unwrap().mode, Mode::Race);
        assert_eq!(
            parse_strs(&["--mode", "timetrial"], None).unwrap().mode,
            Mode::TimeTrial
        );
        assert!(parse_strs(&["--mode", "derby"], None).is_err());
    }

    #[test]
    fn args_elimination_mode() {
        assert_eq!(
            parse_strs(&["--mode", "elimination"], None).unwrap().mode,
            Mode::Elimination
        );
    }

    #[test]
    fn args_online() {
        let a = parse_strs(&["--join", "100.64.0.2", "--name", "Rusty"], None).unwrap();
        assert_eq!(a.join.as_deref(), Some("100.64.0.2"));
        assert_eq!(a.name.as_deref(), Some("Rusty"));
        assert!(!a.host);
        assert!(parse_strs(&["--host"], None).unwrap().host);
        assert!(parse_strs(&["--join"], None).is_err());
    }

    #[test]
    fn args_track() {
        assert_eq!(parse_strs(&[], None).unwrap().track, 0);
        assert_eq!(parse_strs(&["--track", "skyway"], None).unwrap().track, 2);
        assert_eq!(parse_strs(&["--track", "castle"], None).unwrap().track, 1);
        assert!(parse_strs(&["--track", "moon"], None).is_err());
    }

    #[test]
    fn args_menu() {
        assert_eq!(parse_strs(&[], None).unwrap().menu, None);
        let a = parse_strs(&["--menu", "race-setup"], None).unwrap();
        assert_eq!(a.menu, Some(MenuShot::RaceSetup));
        assert!(a.attract, "title-screen menus run over the attract race");
        let a = parse_strs(&["--menu", "pause"], None).unwrap();
        assert_eq!(a.menu, Some(MenuShot::Pause));
        assert!(!a.attract, "pause sits on a race");
        for (m, name) in MenuShot::ALL {
            assert_eq!(name.parse::<MenuShot>(), Ok(m));
        }
        assert!(parse_strs(&["--menu", "options"], None).is_err());
        assert!(parse_strs(&["--menu"], None).is_err());
    }

    #[test]
    fn args_no_motion() {
        assert!(!parse_strs(&[], None).unwrap().no_motion);
        assert!(parse_strs(&["--no-motion"], None).unwrap().no_motion);
        assert!(USAGE.contains("--no-motion"));
    }

    #[test]
    fn args_errors() {
        let e = parse_strs(&["--size", "1600"], None).unwrap_err();
        assert!(e.contains("WxH"), "{e}");
        assert!(parse_strs(&["--bogus"], None).is_err());
        assert!(parse_strs(&["--bench"], None).is_err());
    }
}
