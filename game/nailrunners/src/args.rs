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
    pub preset: Preset,
    pub mode: Mode,
    /// Benchmark the title screen (attract mode) instead of a race.
    pub attract: bool,
    /// Index into `track::TRACKS`.
    pub track: usize,
}

/// What to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Combat race against seven rivals.
    #[default]
    Race,
    /// Solo laps against your best-lap ghost.
    TimeTrial,
}

impl std::str::FromStr for Mode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "race" => Ok(Self::Race),
            "timetrial" | "time-trial" | "tt" => Ok(Self::TimeTrial),
            other => Err(format!("unknown mode '{other}' (use race or timetrial)")),
        }
    }
}

pub const USAGE: &str = "usage: nailrunners [--adapter auto|integrated|discrete|<name>] [--size WxH] \
[--present vsync|immediate] [--mode race|timetrial] [--track dustbowl|castle|skyway] [--preset low|medium|high] [--bench SECS [--offscreen [--screenshot FILE.png]]]";

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
    let mut preset = Preset::Medium;
    let mut mode = Mode::Race;
    let mut attract = false;
    let mut track = 0;
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
            "--preset" => preset = value()?.parse()?,
            "--mode" => mode = value()?.parse()?,
            "--attract" => attract = true,
            "--track" => {
                let v = value()?.to_ascii_lowercase();
                track = crate::track::TRACKS
                    .iter()
                    .position(|t| t.name.to_ascii_lowercase().contains(&v))
                    .ok_or_else(|| format!("unknown track '{v}' (dustbowl, castle, skyway)"))?;
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
        track,
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
        assert_eq!(parse_strs(&[], None).unwrap().preset, Preset::Medium);
        assert_eq!(
            parse_strs(&["--preset", "low"], None).unwrap().preset,
            Preset::Low
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
    fn args_track() {
        assert_eq!(parse_strs(&[], None).unwrap().track, 0);
        assert_eq!(parse_strs(&["--track", "skyway"], None).unwrap().track, 2);
        assert_eq!(parse_strs(&["--track", "castle"], None).unwrap().track, 1);
        assert!(parse_strs(&["--track", "moon"], None).is_err());
    }

    #[test]
    fn args_errors() {
        let e = parse_strs(&["--size", "1600"], None).unwrap_err();
        assert!(e.contains("WxH"), "{e}");
        assert!(parse_strs(&["--bogus"], None).is_err());
        assert!(parse_strs(&["--bench"], None).is_err());
    }
}
