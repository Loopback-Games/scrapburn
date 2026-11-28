//! Saved settings: every option in one `Prefs`, stored as `key value` lines in `<data>/settings`.
//!
//! Reading never fails: a missing file gives the defaults, unknown keys are ignored, a bad
//! value falls back to the default for that key only, and numbers are clamped to their range.
//! Defaults equal the game's long-standing constants, so a player who never opens the
//! settings sees no change.

use std::path::Path;

use rivet_render::{Preset, QualitySettings};

use crate::bindings::{Action, Bindings};
use crate::menu::FpsDisplay;

/// Lap and rival counts the menu offers; the file accepts nothing else.
pub const LAP_CHOICES: [u32; 3] = [1, 3, 5];
pub const RIVAL_CHOICES: [usize; 3] = [3, 5, 7];

pub const FILE: &str = "settings";

/// Graphics preset, or `Custom` once individual quality options are changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetChoice {
    Low,
    Medium,
    High,
    Custom,
}

impl PresetChoice {
    /// The preset to start from; `Custom` has none, so it stands in as Medium.
    pub fn preset(self) -> Preset {
        match self {
            Self::Low => Preset::Low,
            Self::Medium | Self::Custom => Preset::Medium,
            Self::High => Preset::High,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Custom => "custom",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            "custom" => Self::Custom,
            _ => return None,
        })
    }
}

impl From<Preset> for PresetChoice {
    fn from(p: Preset) -> Self {
        match p {
            Preset::Low => Self::Low,
            Preset::Medium => Self::Medium,
            Preset::High => Self::High,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisplayMode {
    #[default]
    Windowed,
    Borderless,
    Fullscreen,
}

impl DisplayMode {
    fn key(self) -> &'static str {
        match self {
            Self::Windowed => "windowed",
            Self::Borderless => "borderless",
            Self::Fullscreen => "fullscreen",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "windowed" => Self::Windowed,
            "borderless" => Self::Borderless,
            "fullscreen" => Self::Fullscreen,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Units {
    #[default]
    Kmh,
    Mph,
}

impl Units {
    fn key(self) -> &'static str {
        match self {
            Self::Kmh => "kmh",
            Self::Mph => "mph",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "kmh" => Self::Kmh,
            "mph" => Self::Mph,
            _ => return None,
        })
    }
}

/// The file's values that command-line flags replaced for this run. `persisted` writes these
/// instead, so a flag lasts one run. A menu edit of the same setting drops its entry.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CliOverrides {
    /// Preset, quality and auto flag, replaced by `--preset`.
    pub graphics: Option<Graphics>,
    /// V-sync, replaced by `--present`.
    pub vsync: Option<bool>,
}

/// Frame caps the menu offers; `None` is uncapped.
pub const FRAME_CAPS: [Option<u32>; 5] = [Some(30), Some(60), Some(120), Some(144), None];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Graphics {
    pub preset: PresetChoice,
    /// The preset is picked automatically (start high, step down on a slow GPU).
    pub auto: bool,
    pub quality: QualitySettings,
    pub display: DisplayMode,
    pub vsync: bool,
    pub frame_cap: Option<u32>,
    pub fps: FpsDisplay,
}

impl Graphics {
    /// Pick a named preset; its quality values replace the individual ones.
    pub fn set_preset(&mut self, p: Preset) {
        self.preset = p.into();
        self.quality = p.settings();
    }
}

impl Default for Graphics {
    fn default() -> Self {
        Self {
            preset: PresetChoice::Medium,
            // Start on High and settle down, as the game always has without `--preset`.
            auto: true,
            quality: Preset::Medium.settings(),
            display: DisplayMode::Windowed,
            vsync: true,
            frame_cap: None,
            fps: FpsDisplay::Off,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Audio {
    pub master: u8,
    pub music: u8,
    pub effects: u8,
    pub engines: u8,
    pub menus: u8,
    /// The race soundtrack.
    pub soundtrack: bool,
}

impl Default for Audio {
    fn default() -> Self {
        Self {
            master: 100,
            music: 100,
            effects: 100,
            engines: 100,
            menus: 100,
            soundtrack: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Controls {
    pub bindings: Bindings,
    /// 0-100; 50 is neutral.
    pub sensitivity: u8,
    /// Stick deadzone, 0-30 percent.
    pub deadzone: u8,
    pub vibration: bool,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            bindings: Bindings::default(),
            sensitivity: 50,
            deadzone: 15,
            vibration: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// Metres behind the kart, 3.0-7.0.
    pub distance: f32,
    /// Metres above the kart, 1.0-3.0.
    pub height: f32,
    /// Degrees, 55-90.
    pub fov: f32,
    pub widen_at_speed: bool,
    /// Percent, 0-100.
    pub shake: u8,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            distance: 4.15,
            height: 1.64,
            fov: 61.0,
            widen_at_speed: true,
            shake: 100,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hud {
    /// Percent, 70-130.
    pub scale: u8,
    pub minimap: bool,
    pub speedometer: bool,
    pub units: Units,
    pub rival_names: bool,
}

impl Default for Hud {
    fn default() -> Self {
        Self {
            scale: 100,
            minimap: true,
            speedometer: true,
            units: Units::Kmh,
            rival_names: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gameplay {
    pub laps: u32,
    pub rivals: usize,
}

impl Default for Gameplay {
    fn default() -> Self {
        Self { laps: 3, rivals: 7 }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Prefs {
    pub graphics: Graphics,
    pub audio: Audio,
    pub controls: Controls,
    pub camera: Camera,
    pub hud: Hud,
    pub gameplay: Gameplay,
}

fn flag(v: &str) -> Option<bool> {
    match v {
        "true" | "on" | "yes" | "1" => Some(true),
        "false" | "off" | "no" | "0" => Some(false),
        _ => None,
    }
}

fn float(v: &str, lo: f32, hi: f32) -> Option<f32> {
    v.parse::<f32>()
        .ok()
        .filter(|x| x.is_finite())
        .map(|x| x.clamp(lo, hi))
}

fn int(v: &str, lo: u32, hi: u32) -> Option<u32> {
    // Parse wide so that an out-of-range number clamps rather than being rejected.
    v.parse::<i64>()
        .ok()
        .map(|x| x.clamp(i64::from(lo), i64::from(hi)) as u32)
}

fn percent(v: &str, lo: u8, hi: u8) -> Option<u8> {
    int(v, lo.into(), hi.into()).map(|x| x as u8)
}

impl Prefs {
    /// Parse settings text. Never fails; see the module docs.
    pub fn parse(text: &str) -> Self {
        let mut p = Self::default();
        for line in text.lines() {
            let line = line.trim();
            let Some((key, value)) = line.split_once(char::is_whitespace) else {
                continue;
            };
            p.set(key, value.trim());
        }
        p
    }

    /// Apply one `key value` pair; an unknown key or bad value changes nothing.
    fn set(&mut self, key: &str, v: &str) {
        let g = &mut self.graphics;
        match key {
            "graphics.preset" => set(&mut g.preset, PresetChoice::parse(v)),
            "graphics.auto" => set(&mut g.auto, flag(v)),
            "graphics.render_scale" => set(&mut g.quality.render_scale, float(v, 0.5, 1.0)),
            "graphics.shadow_size" => set(&mut g.quality.shadow_size, int(v, 512, 4096)),
            "graphics.bloom_levels" => set(&mut g.quality.bloom_levels, int(v, 0, 6)),
            "graphics.fxaa" => set(&mut g.quality.fxaa, flag(v)),
            "graphics.texture_skip_mips" => set(&mut g.quality.texture_skip_mips, int(v, 0, 2)),
            "graphics.anisotropy" => {
                set(&mut g.quality.anisotropy, int(v, 1, 16).map(|x| x as u16));
            }
            "graphics.display" => set(&mut g.display, DisplayMode::parse(v)),
            "graphics.vsync" => set(&mut g.vsync, flag(v)),
            "graphics.frame_cap" => {
                if v == "off" {
                    g.frame_cap = None;
                } else if let Some(c) = v
                    .parse::<u32>()
                    .ok()
                    .filter(|c| FRAME_CAPS.contains(&Some(*c)))
                {
                    g.frame_cap = Some(c);
                }
            }
            "graphics.fps" => set(
                &mut g.fps,
                match v {
                    "off" => Some(FpsDisplay::Off),
                    "counter" => Some(FpsDisplay::Counter),
                    "graph" => Some(FpsDisplay::Graph),
                    _ => None,
                },
            ),
            "audio.master" => set(&mut self.audio.master, percent(v, 0, 100)),
            "audio.music" => set(&mut self.audio.music, percent(v, 0, 100)),
            "audio.effects" => set(&mut self.audio.effects, percent(v, 0, 100)),
            "audio.engines" => set(&mut self.audio.engines, percent(v, 0, 100)),
            "audio.menus" => set(&mut self.audio.menus, percent(v, 0, 100)),
            "audio.soundtrack" => set(&mut self.audio.soundtrack, flag(v)),
            "controls.sensitivity" => set(&mut self.controls.sensitivity, percent(v, 0, 100)),
            "controls.deadzone" => set(&mut self.controls.deadzone, percent(v, 0, 30)),
            "controls.vibration" => set(&mut self.controls.vibration, flag(v)),
            "camera.distance" => set(&mut self.camera.distance, float(v, 3.0, 7.0)),
            "camera.height" => set(&mut self.camera.height, float(v, 1.0, 3.0)),
            "camera.fov" => set(&mut self.camera.fov, float(v, 55.0, 90.0)),
            "camera.widen_at_speed" => set(&mut self.camera.widen_at_speed, flag(v)),
            "camera.shake" => set(&mut self.camera.shake, percent(v, 0, 100)),
            "hud.scale" => set(&mut self.hud.scale, percent(v, 70, 130)),
            "hud.minimap" => set(&mut self.hud.minimap, flag(v)),
            "hud.speedometer" => set(&mut self.hud.speedometer, flag(v)),
            "hud.units" => set(&mut self.hud.units, Units::parse(v)),
            "hud.rival_names" => set(&mut self.hud.rival_names, flag(v)),
            "gameplay.laps" => set(
                &mut self.gameplay.laps,
                v.parse().ok().filter(|n| LAP_CHOICES.contains(n)),
            ),
            "gameplay.rivals" => set(
                &mut self.gameplay.rivals,
                v.parse().ok().filter(|n| RIVAL_CHOICES.contains(n)),
            ),
            _ => {
                if let Some(action) = key.strip_prefix("controls.bind.") {
                    self.controls.bindings.set_line(action, v);
                }
            }
        }
    }

    /// The settings for this run: `self` (as loaded) with the command line on top (`preset`
    /// from `--preset`, `vsync` from `--present`). Benchmarks of races stay on Medium. Also
    /// returns the file's values the command line overrode, so `persisted` can keep the
    /// overrides out of the saved file.
    pub fn with_cli(
        mut self,
        preset: Option<Preset>,
        vsync: Option<bool>,
        bench: bool,
        attract: bool,
    ) -> (Self, CliOverrides) {
        let loaded = self.graphics;
        let vsync_restore = vsync.map(|v| std::mem::replace(&mut self.graphics.vsync, v));
        let g = &mut self.graphics;
        g.auto = g.auto && preset.is_none() && (!bench || attract);
        let restore = match preset {
            Some(p) => {
                g.set_preset(p);
                Some(loaded)
            }
            None => {
                // Auto graphics starts on High and settles down.
                if g.auto {
                    g.set_preset(Preset::High);
                } else if bench {
                    g.set_preset(Preset::Medium);
                }
                None
            }
        };
        (
            self,
            CliOverrides {
                graphics: restore,
                vsync: vsync_restore,
            },
        )
    }

    /// What to write: `self`, except the values in `restore` while a command-line override
    /// of them is in force.
    pub fn persisted(&self, restore: &CliOverrides) -> Self {
        let mut p = *self;
        if let Some(r) = &restore.graphics {
            p.graphics.preset = r.preset;
            p.graphics.auto = r.auto;
            p.graphics.quality = r.quality;
        }
        if let Some(v) = restore.vsync {
            p.graphics.vsync = v;
        }
        p
    }

    /// The file text, one `key value` per line.
    pub fn to_text(&self) -> String {
        let g = &self.graphics;
        let q = &g.quality;
        let on = |b: bool| if b { "on" } else { "off" };
        let fps = match g.fps {
            FpsDisplay::Off => "off",
            FpsDisplay::Counter => "counter",
            FpsDisplay::Graph => "graph",
        };
        let cap = g.frame_cap.map_or("off".to_string(), |c| c.to_string());
        let (a, c, cam, h, gp) = (
            &self.audio,
            &self.controls,
            &self.camera,
            &self.hud,
            &self.gameplay,
        );
        let lines: Vec<(&str, String)> = vec![
            ("graphics.preset", g.preset.key().into()),
            ("graphics.auto", on(g.auto).into()),
            ("graphics.render_scale", q.render_scale.to_string()),
            ("graphics.shadow_size", q.shadow_size.to_string()),
            ("graphics.bloom_levels", q.bloom_levels.to_string()),
            ("graphics.fxaa", on(q.fxaa).into()),
            (
                "graphics.texture_skip_mips",
                q.texture_skip_mips.to_string(),
            ),
            ("graphics.anisotropy", q.anisotropy.to_string()),
            ("graphics.display", g.display.key().into()),
            ("graphics.vsync", on(g.vsync).into()),
            ("graphics.frame_cap", cap),
            ("graphics.fps", fps.into()),
            ("audio.master", a.master.to_string()),
            ("audio.music", a.music.to_string()),
            ("audio.effects", a.effects.to_string()),
            ("audio.engines", a.engines.to_string()),
            ("audio.menus", a.menus.to_string()),
            ("audio.soundtrack", on(a.soundtrack).into()),
            ("controls.sensitivity", c.sensitivity.to_string()),
            ("controls.deadzone", c.deadzone.to_string()),
            ("controls.vibration", on(c.vibration).into()),
            ("camera.distance", cam.distance.to_string()),
            ("camera.height", cam.height.to_string()),
            ("camera.fov", cam.fov.to_string()),
            ("camera.widen_at_speed", on(cam.widen_at_speed).into()),
            ("camera.shake", cam.shake.to_string()),
            ("hud.scale", h.scale.to_string()),
            ("hud.minimap", on(h.minimap).into()),
            ("hud.speedometer", on(h.speedometer).into()),
            ("hud.units", h.units.key().into()),
            ("hud.rival_names", on(h.rival_names).into()),
            ("gameplay.laps", gp.laps.to_string()),
            ("gameplay.rivals", gp.rivals.to_string()),
        ];
        let binds = Action::ALL
            .iter()
            .map(|a| format!("controls.bind.{} {}\n", a.key(), c.bindings.line(*a)));
        lines
            .iter()
            .map(|(k, v)| format!("{k} {v}\n"))
            .chain(binds)
            .collect()
    }

    /// Read `<dir>/settings`; defaults when it is missing or unreadable.
    pub fn load(dir: &Path) -> Self {
        std::fs::read_to_string(dir.join(FILE))
            .map_or_else(|_| Self::default(), |t| Self::parse(&t))
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        crate::ghost::write_atomic(&dir.join(FILE), self.to_text().as_bytes())
    }
}

fn set<T>(slot: &mut T, value: Option<T>) {
    if let Some(v) = value {
        *slot = v;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bindings::Press;
    use rivet_platform::PadButtons;
    use winit::keyboard::KeyCode;

    fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("scrapburn-prefs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn defaults_when_file_missing() {
        let dir = scratch("missing");
        assert_eq!(Prefs::load(&dir), Prefs::default());
    }

    #[test]
    fn defaults_equal_todays_constants() {
        let p = Prefs::default();
        assert_eq!(
            (p.camera.distance, p.camera.height, p.camera.fov),
            (4.15, 1.64, 61.0)
        );
        assert_eq!((p.camera.shake, p.controls.deadzone), (100, 15));
        assert_eq!((p.gameplay.laps, p.gameplay.rivals), (3, 7));
        assert!(p.audio.soundtrack);
        assert_eq!(p.graphics.preset, PresetChoice::Medium);
        assert_eq!(p.graphics.fps, FpsDisplay::Off);
        assert_eq!(p.graphics.quality, Preset::Medium.settings());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn round_trips_every_field() {
        let mut p = Prefs::default();
        p.graphics = Graphics {
            preset: PresetChoice::Custom,
            auto: false,
            quality: QualitySettings {
                render_scale: 0.75,
                shadow_size: 1024,
                bloom_levels: 2,
                fxaa: false,
                texture_skip_mips: 1,
                anisotropy: 4,
            },
            display: DisplayMode::Borderless,
            vsync: false,
            frame_cap: Some(144),
            fps: FpsDisplay::Graph,
        };
        p.audio = Audio {
            master: 80,
            music: 70,
            effects: 60,
            engines: 50,
            menus: 40,
            soundtrack: false,
        };
        let mut bindings = Bindings::default();
        bindings.capture(Action::UseItem, Press::Key(KeyCode::KeyZ));
        bindings.capture(Action::Accelerate, Press::Key(KeyCode::KeyE));
        bindings.capture(Action::Brake, Press::Pad(PadButtons::RIGHT_TRIGGER));
        p.controls = Controls {
            bindings,
            sensitivity: 65,
            deadzone: 25,
            vibration: false,
        };
        p.camera = Camera {
            distance: 5.5,
            height: 2.25,
            fov: 72.5,
            widen_at_speed: false,
            shake: 30,
        };
        p.hud = Hud {
            scale: 120,
            minimap: false,
            speedometer: false,
            units: Units::Mph,
            rival_names: false,
        };
        p.gameplay = Gameplay { laps: 5, rivals: 3 };
        assert_eq!(Prefs::parse(&p.to_text()), p);

        let dir = scratch("roundtrip");
        p.save(&dir).unwrap();
        assert_eq!(Prefs::load(&dir), p);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn uncapped_frame_cap_round_trips() {
        let mut p = Prefs::default();
        p.graphics.frame_cap = Some(60);
        assert_eq!(Prefs::parse(&p.to_text()).graphics.frame_cap, Some(60));
        p.graphics.frame_cap = None;
        assert_eq!(Prefs::parse(&p.to_text()).graphics.frame_cap, None);
    }

    #[test]
    fn unknown_keys_and_junk_lines_are_ignored() {
        let p = Prefs::parse(
            "future.thing 9\ncontrols.bind.fly key:KeyE pad:West\n\nlonely\ngameplay.laps 5\n\
             controls.bind.use_item key:KeyZ key:Nope pad:West\n",
        );
        assert_eq!(
            p,
            Prefs {
                gameplay: Gameplay { laps: 5, rivals: 7 },
                ..Prefs::default()
            }
        );
    }

    #[test]
    fn bindings_are_saved_one_line_per_action() {
        let text = Prefs::default().to_text();
        assert!(
            text.contains("controls.bind.use_item key:KeyE key:ControlLeft key:KeyJ pad:West\n")
        );
        assert!(text.contains("controls.bind.steer_left key:KeyA key:ArrowLeft pad:Stick\n"));
        let p = Prefs::parse("controls.bind.respawn key:KeyT pad:North\n");
        let r = p.controls.bindings.keys_for(Action::Respawn);
        assert_eq!(r, [Some(KeyCode::KeyT), None, None]);
    }

    #[test]
    fn bad_value_falls_back_for_that_key_only() {
        let p = Prefs::parse(
            "gameplay.laps many\ngameplay.rivals 5\ncamera.fov nan\ngraphics.frame_cap 75\n\
             hud.units furlongs\naudio.music loud\naudio.master 40\n",
        );
        assert_eq!(p.gameplay.laps, 3);
        assert_eq!(p.gameplay.rivals, 5);
        assert_eq!(p.camera.fov, 61.0);
        assert_eq!(p.graphics.frame_cap, None);
        assert_eq!(p.hud.units, Units::Kmh);
        assert_eq!(p.audio.music, 100);
        assert_eq!(p.audio.master, 40);
    }

    #[test]
    fn values_are_clamped_to_range() {
        let p = Prefs::parse(
            "camera.distance 99\ncamera.height -4\ncamera.fov 200\ncamera.shake 900\n\
             controls.deadzone 80\ncontrols.sensitivity -5\nhud.scale 10\naudio.master 1000\n\
             gameplay.laps 5\ngraphics.render_scale 9\ngraphics.anisotropy 64\n",
        );
        assert_eq!(p.camera.distance, 7.0);
        assert_eq!(p.camera.height, 1.0);
        assert_eq!(p.camera.fov, 90.0);
        assert_eq!(p.camera.shake, 100);
        assert_eq!(p.controls.deadzone, 30);
        assert_eq!(p.controls.sensitivity, 0);
        assert_eq!(p.hud.scale, 70);
        assert_eq!(p.audio.master, 100);
        assert_eq!(p.gameplay.laps, 5);
        assert_eq!(p.graphics.quality.render_scale, 1.0);
        assert_eq!(p.graphics.quality.anisotropy, 16);
    }

    #[test]
    fn laps_and_rivals_snap_to_menu_choices() {
        let p = Prefs::parse("gameplay.laps 4\ngameplay.rivals 30\n");
        assert_eq!((p.gameplay.laps, p.gameplay.rivals), (3, 7));
        let p = Prefs::parse("gameplay.laps 5\ngameplay.rivals 3\n");
        assert_eq!((p.gameplay.laps, p.gameplay.rivals), (5, 3));
    }

    #[test]
    fn cli_preset_is_not_persisted() {
        let mut file = Prefs::default();
        file.graphics.set_preset(Preset::High);
        file.graphics.auto = true;
        let (run, restore) = file.with_cli(Some(Preset::Low), None, false, false);
        assert_eq!(run.graphics.preset, PresetChoice::Low);
        assert!(!run.graphics.auto);
        assert_eq!(run.graphics.quality, Preset::Low.settings());
        let saved = run.persisted(&restore);
        assert_eq!(saved.graphics, file.graphics);
        // Without an override the run's graphics are saved as they are.
        let (run, restore) = file.with_cli(None, None, false, false);
        assert_eq!(restore, CliOverrides::default());
        assert_eq!(run.persisted(&restore), run);
    }

    #[test]
    fn cli_present_is_not_persisted() {
        let mut file = Prefs::default();
        file.graphics.vsync = true;
        let (run, restore) = file.with_cli(None, Some(false), false, false);
        assert!(!run.graphics.vsync);
        assert!(run.persisted(&restore).graphics.vsync);
        // Other edits made during the run are still saved.
        let mut edited = run;
        edited.graphics.frame_cap = Some(60);
        assert_eq!(edited.persisted(&restore).graphics.frame_cap, Some(60));
        // Once the menu changes V-sync, the override is dropped and the edit is saved.
        let dropped = CliOverrides {
            vsync: None,
            ..restore
        };
        assert!(!run.persisted(&dropped).graphics.vsync);
    }

    #[test]
    fn cli_preset_and_present_restore_separately() {
        let file = Prefs::default();
        let (mut run, mut restore) = file.with_cli(Some(Preset::Low), Some(false), false, false);
        // The menu picks a preset (dropping that override) but leaves V-sync alone.
        restore.graphics = None;
        run.graphics.set_preset(Preset::High);
        let saved = run.persisted(&restore);
        assert_eq!(saved.graphics.preset, PresetChoice::High);
        assert!(saved.graphics.vsync);
    }

    #[test]
    fn bench_ignores_auto_unless_attract() {
        let (race, _) = Prefs::default().with_cli(None, None, true, false);
        assert!(!race.graphics.auto);
        assert_eq!(race.graphics.preset, PresetChoice::Medium);
        let (title, _) = Prefs::default().with_cli(None, None, true, true);
        assert!(title.graphics.auto);
        assert_eq!(title.graphics.preset, PresetChoice::High);
    }

    #[test]
    fn set_preset_replaces_quality() {
        let mut g = Graphics::default();
        g.quality.bloom_levels = 1;
        g.set_preset(Preset::High);
        assert_eq!(g.preset, PresetChoice::High);
        assert_eq!(g.quality, Preset::High.settings());
    }
}
