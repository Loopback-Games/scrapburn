//! Window, screens (title, settings, race, pause, results), main loop, and frame rendering.

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Context as _;
use glam::{Mat4, Vec3};
use rivet_assets::Rgba8Image;
use rivet_core::{FixedTimestep, FrameStats};
use rivet_physics::{DriveInput, VehicleState};
use rivet_platform::{Gamepads, KeyInput, PadButtons};
use rivet_render::{
    Draw, FrameContent, GpuContext, GpuMesh, GpuTimer, MeshData, Overlay, ParticleInstance,
    SceneRenderer, View, box_mesh, draw_frame_graph,
};
use scrapburn::args::{Args, Mode};
use scrapburn::bench::{Bench, BenchPhase};
use scrapburn::camera::ChaseCamera;
use scrapburn::circuit::{Circuit, Event};
use scrapburn::combat::CombatEvent;
use scrapburn::controls::{SteerRamp, actions, drive_input, menu_input, menu_input_typing};
use scrapburn::effects::Effects;
use scrapburn::fx_meshes;
use scrapburn::ghost::{BestGhost, GhostRecorder};
use scrapburn::hud::{self, CircuitHud, HudInfo, RaceHud};
use scrapburn::materials::{find_assets_dir, load_layers, shading};
use scrapburn::menu::{FpsDisplay, MenuAction, MenuState, Screen, Settings};
use scrapburn::netplay::{
    self, ClientNote, ClientView, HostNote, Lobby, NetClient, NetHost, RaceSetup, Snapshot,
};
use scrapburn::particles::{Blend, Particle, ParticleSystem};
use scrapburn::race::{Phase, Race, RaceEvent};
use scrapburn::scenery;
use scrapburn::sound::SoundDirector;
use scrapburn::track::{TRACKS, Track, build_track};
use scrapburn::{MAX_STEPS, TICK_DT};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowId};

/// Seconds after the player finishes before the results screen appears.
const RESULTS_DELAY: f64 = 2.5;

pub fn run(args: Args) -> anyhow::Result<ExitCode> {
    let event_loop = EventLoop::new().context("creating the event loop")?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        args,
        game: None,
        exit: ExitCode::SUCCESS,
        launched: Instant::now(),
    };
    event_loop
        .run_app(&mut app)
        .context("running the event loop")?;
    Ok(app.exit)
}

struct App {
    args: Args,
    game: Option<Game>,
    exit: ExitCode,
    launched: Instant,
}

/// What the race on screen is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Session {
    /// Bots racing behind the title screen.
    Attract,
    Play(Mode),
}

/// One kart design on the GPU: body, four wheels around their hubs, tire radius.
struct KartGpu {
    /// Index into `kart::KARTS`.
    design: usize,
    body: GpuMesh,
    wheels: [GpuMesh; 4],
    hubs: [Vec3; 4],
    radii: [f32; 4],
}

impl KartGpu {
    fn new(
        device: &wgpu::Device,
        design: usize,
        data: &MeshData,
        tex: Option<&Rgba8Image>,
    ) -> Self {
        let k = &scrapburn::kart::KARTS[design];
        let mut parts = scrapburn::kart::split_wheels_textured(
            data,
            scrapburn::kart::KART_LENGTH * k.wheel_ratio,
            tex,
        );
        scrapburn::kart::add_marker_lamps(&mut parts.body);
        scrapburn::kart::fit_buggy_frame(&mut parts.body, design);
        Self {
            design,
            body: GpuMesh::new(device, &parts.body),
            wheels: parts.wheels.each_ref().map(|w| GpuMesh::new(device, w)),
            hubs: parts.hubs,
            radii: parts.radii,
        }
    }
}

/// Online role of this machine.
enum Net {
    Off,
    Host(Box<NetHost>),
    Client {
        link: Box<NetClient>,
        /// Set once a race snapshot arrives.
        view: Option<ClientView>,
        lobby: Option<Lobby>,
        /// A race this player left early: its snapshots are ignored until the next one.
        sat_out: Option<u32>,
    },
}

/// What to do online right after launch (`--host` / `--join`).
enum LaunchNet {
    Host { loopback: bool },
    Join(String),
}

/// Tiny deterministic RNG for cosmetic effects.
struct Lcg(u32);

impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// A burst of additive sparks.
#[allow(clippy::too_many_arguments)]
fn fx_burst(
    fx: &mut ParticleSystem,
    rng: &mut Lcg,
    at: Vec3,
    n: u32,
    speed: f32,
    color: [f32; 4],
    size: f32,
    life: f32,
) {
    for _ in 0..n {
        let dir =
            Vec3::new(rng.next() - 0.5, rng.next() * 0.8, rng.next() - 0.5).normalize_or(Vec3::Y);
        let vel = dir * speed * (0.4 + rng.next() * 0.6);
        let life = life * (0.6 + rng.next() * 0.4);
        fx.spawn(Particle {
            pos: at,
            vel,
            age: 0.0,
            life,
            size: (size, size * 0.2),
            color: (color, [color[0] * 0.3, color[1] * 0.2, color[2] * 0.2, 0.0]),
            gravity: 6.0,
            drag: 2.5,
            blend: Blend::Additive,
        });
    }
}

/// GPU meshes for items and weapons.
struct FxMeshes {
    shell: GpuMesh,
    core: GpuMesh,
    nail: GpuMesh,
    chain: GpuMesh,
    hook: GpuMesh,
    orb: GpuMesh,
    orb_glow: GpuMesh,
}

struct Game {
    window: Arc<Window>,
    ctx: GpuContext,
    scene: SceneRenderer,
    layers: Vec<Vec<Rgba8Image>>,
    shading_table: Vec<rivet_render::MaterialShading>,
    prop_gpu: Vec<Option<GpuMesh>>,
    placements: Vec<scenery::Placement>,
    overlay: Overlay,
    timer: Option<GpuTimer>,
    track: Track,
    track_meshes: [GpuMesh; 3],
    service_bay: GpuMesh,
    route_previews: Vec<Vec<Vec3>>,
    /// Loaded kart designs (index into `kart::KARTS` order; missing ones are skipped).
    karts: Vec<KartGpu>,
    /// Design each racer drives (index into `karts`).
    kart_of: Vec<usize>,
    wheel_spin: Vec<[f32; 4]>,
    ghost_mesh: GpuMesh,
    fx_meshes: FxMeshes,
    combat_fx: ParticleSystem,
    race: Race,
    session: Session,
    menu: MenuState,
    menu_rows: Option<(Screen, hud::MenuRows)>,
    pointer: Option<(f32, f32)>,
    clicked: Option<(Screen, usize, bool)>,
    /// The circuit being raced, if any.
    circuit: Option<Circuit>,
    settings: Settings,
    clock: FixedTimestep,
    camera: ChaseCamera,
    keys: KeyInput,
    pads: Option<Gamepads>,
    stats: FrameStats,
    last_frame: Instant,
    minimized: bool,
    bench: Option<Bench>,
    screenshot: Option<std::path::PathBuf>,
    steer: SteerRamp,
    effects: Vec<Effects>,
    leans: Vec<f32>,
    ghost_file: Option<std::path::PathBuf>,
    best_ghost: BestGhost,
    recorder: GhostRecorder,
    /// Wall time for the attract camera.
    attract_t: f32,
    audio: Option<rivet_audio::Audio>,
    sound: SoundDirector,
    frame_events: Vec<RaceEvent>,
    /// The latest elimination knockout: racer and race time.
    knockout: Option<(usize, f64)>,
    net: Net,
    tunnel: Option<crate::tailcat::Tunnel>,
    /// The private invite this host shares.
    invite: Option<String>,
    /// Kept open: on X11 copied text lasts only while its owner lives.
    clipboard: Option<arboard::Clipboard>,
    launch_net: Option<LaunchNet>,
    /// Name shown to other players online.
    player_name: String,
    /// A short notice and when it appeared (wall seconds).
    toast: Option<(String, f32)>,
    /// Tailnet peers being looked up for the Online screen.
    peers_rx: Option<std::sync::mpsc::Receiver<Vec<netplay::TailnetPeer>>>,
    /// When the peer list was last fetched (wall seconds).
    peers_at: f32,
    /// A hostname being looked up off the frame thread.
    resolving: Option<(
        String,
        std::sync::mpsc::Receiver<Option<std::net::SocketAddr>>,
    )>,
    net_ticks: u32,
    /// Picks the graphics preset from measured frame times (when not set by hand).
    tuner: scrapburn::autotune::AutoTuner,
    /// Live fire: torch and brazier flames.
    flame_fx: ParticleSystem,
    flame_points: Vec<Vec3>,
    flame_acc: f32,
}

/// Use the equipped chassis for the local driver and weight-based designs for bots.
fn kart_assignment(race: &Race, karts: &[KartGpu], design: usize) -> Vec<usize> {
    race.racers
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let human = race.player == Some(i) || r.remote;
            let want = if race.player == Some(i) {
                design
            } else {
                scrapburn::kart::model_for(r.mass / scrapburn::race::BASE_MASS, human)
            };
            karts.iter().position(|k| k.design == want).unwrap_or(0)
        })
        .collect()
}

/// Where flames burn: the track's torches and the top of every brazier.
fn flame_points(track: &Track, placements: &[scenery::Placement]) -> Vec<Vec3> {
    let mut v = track.flames.clone();
    v.extend(
        placements
            .iter()
            .filter(|p| scenery::PROPS[p.kind].name == "brazier")
            .map(|p| p.pos + Vec3::Y * p.size * 0.9),
    );
    v
}

fn placeholder_kart() -> MeshData {
    let mut m = MeshData::default();
    let mut part = |half: Vec3, offset: Vec3, color: [f32; 4]| {
        let mut b = box_mesh(half, color);
        for v in &mut b.vertices {
            v.pos = (Vec3::from_array(v.pos) + offset).to_array();
        }
        m.append(&b);
    };
    part(
        Vec3::new(0.7, 0.3, 1.0),
        Vec3::ZERO,
        [0.15, 0.35, 0.85, 1.0],
    );
    part(
        Vec3::new(0.45, 0.2, 0.45),
        Vec3::new(0.0, 0.45, -0.25),
        [0.06, 0.12, 0.30, 1.0],
    );
    for (x, z) in [(0.75, 0.75), (-0.75, 0.75), (0.75, -0.75), (-0.75, -0.75)] {
        part(
            Vec3::new(0.12, 0.25, 0.25),
            Vec3::new(x, -0.2, z),
            [0.08, 0.08, 0.08, 1.0],
        );
    }
    m
}

impl Game {
    fn new(event_loop: &ActiveEventLoop, args: &Args) -> anyhow::Result<Self> {
        let attrs = Window::default_attributes()
            .with_title("Scrapburn")
            .with_inner_size(PhysicalSize::new(args.size.0, args.size.1));
        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .context("creating the window")?,
        );
        let ctx = GpuContext::new(window.clone(), &args.adapter, args.present, args.offscreen)?;
        let assets =
            find_assets_dir().context("could not find the assets directory (set RIVET_ASSETS)")?;
        let mut layers = load_layers(&assets).context("loading material textures")?;
        let kart_designs = &scrapburn::kart::KARTS;
        let (kart_data, kart_tex) = match scrapburn::kart::load(
            &assets,
            &kart_designs[0],
            scrapburn::materials::KART_LAYER,
        ) {
            Ok((mesh, tex)) => (mesh, tex),
            Err(e) => {
                log::warn!("kart model unavailable ({e}); using the placeholder kart");
                (placeholder_kart(), None)
            }
        };
        let size = (layers[0][0].width, layers[0][0].height);
        let kart_layer = kart_tex.as_ref().map_or_else(
            || layers[0][0].clone(),
            |t| rivet_assets::resized(t, size.0, size.1),
        );
        layers.push(rivet_assets::mip_chain(&kart_layer));
        // Props: one texture layer each, after the kart.
        let mut shading_table: Vec<rivet_render::MaterialShading> = shading().to_vec();
        let mut prop_meshes = Vec::new();
        for kind in scenery::PROPS.iter() {
            let layer = layers.len() as u32;
            match scenery::load_prop(&assets, kind, layer) {
                Ok((mesh, tex)) => {
                    let img = tex.map_or_else(
                        || layers[0][0].clone(),
                        |t| rivet_assets::resized(&t, size.0, size.1),
                    );
                    layers.push(rivet_assets::mip_chain(&img));
                    shading_table.push(rivet_render::MaterialShading {
                        specular: kind.specular,
                        gloss: 24.0,
                        triplanar_scale: 0.2,
                    });
                    prop_meshes.push(Some(mesh));
                }
                Err(e) => {
                    log::warn!("prop {} unavailable ({e})", kind.name);
                    prop_meshes.push(None);
                }
            }
        }
        // The other kart designs, each with its own texture layer after the props.
        let mut extra_karts: Vec<(usize, MeshData, Option<Rgba8Image>)> = Vec::new();
        for (d, design) in kart_designs.iter().enumerate().skip(1) {
            let layer = layers.len() as u32;
            match scrapburn::kart::load(&assets, design, layer) {
                Ok((mesh, tex)) => {
                    let img = tex.as_ref().map_or_else(
                        || layers[0][0].clone(),
                        |t| rivet_assets::resized(t, size.0, size.1),
                    );
                    layers.push(rivet_assets::mip_chain(&img));
                    shading_table.push(rivet_render::MaterialShading {
                        specular: 0.3,
                        gloss: 32.0,
                        triplanar_scale: 0.25,
                    });
                    extra_karts.push((d, mesh, tex));
                }
                Err(e) => log::info!("kart {} not available ({e})", design.name),
            }
        }
        let settings = Settings {
            loadout: scrapburn::ghost::data_dir()
                .map(|d| scrapburn::kart::Loadout::read(&d))
                .unwrap_or_default(),
            // Explicit preset, Medium for benchmarks, otherwise start High and auto-tune.
            preset: args
                .preset
                .unwrap_or(if args.bench.is_some() && !args.attract {
                    rivet_render::Preset::Medium
                } else {
                    rivet_render::Preset::High
                }),
            auto_preset: args.preset.is_none() && (args.bench.is_none() || args.attract),
            track: args.track,
            ..Settings::default()
        };
        let scene = SceneRenderer::new(
            &ctx.device,
            &ctx.queue,
            ctx.format(),
            ctx.size(),
            settings.preset,
            &layers,
            &shading_table,
        );
        let font = std::fs::read(assets.join("fonts").join("Rajdhani-Bold.ttf"))
            .map_err(|e| log::warn!("HUD font missing ({e}); using the bitmap font"))
            .ok();
        let mut scene = scene;
        scene.lighting = scrapburn::track::lighting(TRACKS[settings.track].id);
        let overlay = Overlay::new(&ctx.device, &ctx.queue, ctx.format(), None, font.as_deref());
        let timer = GpuTimer::new(&ctx.device, &ctx.queue);
        let track = build_track(&TRACKS[settings.track]);
        let track_meshes =
            [&track.road, &track.terrain, &track.props].map(|m| GpuMesh::new(&ctx.device, m));
        let parts = scrapburn::kart::split_wheels(
            &kart_data,
            scrapburn::kart::KART_LENGTH * scrapburn::kart::WHEEL_RADIUS_RATIO,
        );
        log::info!(
            "kart wheels: {} triangles split off the body",
            parts
                .wheels
                .iter()
                .map(|w| w.indices.len() / 3)
                .sum::<usize>()
        );
        let mut karts = vec![KartGpu::new(&ctx.device, 0, &kart_data, kart_tex.as_ref())];
        karts.extend(
            extra_karts
                .iter()
                .map(|(d, m, t)| KartGpu::new(&ctx.device, *d, m, t.as_ref())),
        );
        let mut ghost_data = kart_data.clone();
        for v in &mut ghost_data.vertices {
            v.color = [0.2, 0.5, 0.6, 0.0];
            v.material = rivet_render::material(
                0,
                0,
                rivet_render::UNTEXTURED | rivet_render::EMISSIVE | rivet_render::DITHER,
            );
        }
        let ghost_mesh = GpuMesh::new(&ctx.device, &ghost_data);
        let prop_gpu: Vec<Option<GpuMesh>> = prop_meshes
            .iter()
            .map(|m| m.as_ref().map(|m| GpuMesh::new(&ctx.device, m)))
            .collect();
        let placements = if settings.scenery {
            scenery::placements(&track)
        } else {
            scenery::track_features(&track)
        };
        let g = |m: MeshData| GpuMesh::new(&ctx.device, &m);
        let service_bay = g(crate::garage::service_bay());
        let fx_meshes = FxMeshes {
            shell: g(fx_meshes::item_shell()),
            core: g(fx_meshes::item_core()),
            nail: g(fx_meshes::nail()),
            chain: g(fx_meshes::chain()),
            hook: g(fx_meshes::hook_head()),
            orb: g(fx_meshes::sphere(
                2,
                [0.35, 1.0, 0.4, 0.0],
                rivet_render::EMISSIVE,
            )),
            orb_glow: g(fx_meshes::plasma_halo()),
        };
        let ghost_file =
            scrapburn::ghost::data_dir().map(|d| scrapburn::ghost::ghost_path(&d, track.id.key()));
        let best_ghost =
            BestGhost::from_ghost(ghost_file.as_deref().and_then(scrapburn::ghost::load));

        // Benchmarks skip the menus and race bots; otherwise start on the title screen.
        let (session, screen) = if args.garage {
            (Session::Attract, Screen::Garage)
        } else if args.bench.is_some() && !args.attract {
            (Session::Play(args.mode), Screen::Playing)
        } else {
            (Session::Attract, Screen::Title)
        };
        let race = Self::make_race(&track, session, &settings, args.bench.is_some());
        let n = race.racers.len();
        Ok(Self {
            window,
            ctx,
            scene,
            layers,
            shading_table,
            prop_gpu,
            flame_points: flame_points(&track, &placements),
            route_previews: TRACKS.iter().map(scrapburn::track::route_preview).collect(),
            placements,
            overlay,
            timer,
            track,
            track_meshes,
            kart_of: kart_assignment(&race, &karts, settings.loadout.design),
            karts,
            wheel_spin: vec![[0.0; 4]; n],
            ghost_mesh,
            service_bay,
            fx_meshes,
            combat_fx: ParticleSystem::new(2500),
            race,
            session,
            menu: MenuState::new(screen),
            menu_rows: None,
            pointer: None,
            clicked: None,
            circuit: None,
            settings,
            clock: FixedTimestep::new(TICK_DT, MAX_STEPS),
            camera: ChaseCamera::default(),
            keys: KeyInput::default(),
            pads: Gamepads::new(),
            stats: FrameStats::new(240),
            last_frame: Instant::now(),
            minimized: false,
            bench: args.bench.map(Bench::new),
            screenshot: args.screenshot.clone(),
            steer: SteerRamp::default(),
            effects: (0..n).map(|_| Effects::new()).collect(),
            leans: vec![0.0; n],
            ghost_file,
            best_ghost,
            recorder: GhostRecorder::default(),
            attract_t: 0.0,
            audio: if args.bench.is_some() {
                None
            } else {
                rivet_audio::Audio::new()
            },
            sound: SoundDirector::default(),
            frame_events: Vec::new(),
            knockout: None,
            net: Net::Off,
            tunnel: None,
            invite: None,
            clipboard: None,
            launch_net: if args.host {
                Some(LaunchNet::Host {
                    loopback: args.host_loopback,
                })
            } else {
                args.join.clone().map(LaunchNet::Join)
            },
            // A generated name unless the player picks one (the OS account name stays private).
            player_name: rivet_net::clean_name(
                &args
                    .name
                    .clone()
                    .or_else(|| {
                        std::fs::read_to_string(scrapburn::ghost::data_dir()?.join("racer-name"))
                            .ok()
                    })
                    .unwrap_or_else(|| {
                        let t = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(0, |d| d.subsec_nanos());
                        format!("RACER {:02X}", (t >> 8) & 0xff)
                    }),
            )
            .to_ascii_uppercase(),
            toast: None,
            peers_rx: None,
            peers_at: -100.0,
            resolving: None,
            net_ticks: 0,
            tuner: {
                let mut t = scrapburn::autotune::AutoTuner::new();
                // Benchmarks of races use a fixed preset; a title-screen benchmark may tune.
                if args.preset.is_some() || (args.bench.is_some() && !args.attract) {
                    t.stop();
                }
                t
            },
            flame_fx: ParticleSystem::new(1500),
            flame_acc: 0.0,
        })
    }

    fn make_race(track: &Track, session: Session, s: &Settings, bench: bool) -> Race {
        match session {
            Session::Attract => Race::new(track, 8, false, u32::MAX),
            Session::Play(Mode::Race) => Race::new(track, s.rivals + 1, !bench, s.laps),
            Session::Play(Mode::TimeTrial) => Race::new(track, 1, !bench, u32::MAX),
            Session::Play(Mode::Elimination) => {
                Race::new_elimination(track, s.rivals.max(1) + 1, !bench)
            }
        }
    }

    fn start(&mut self, session: Session) {
        if matches!(session, Session::Attract) {
            self.circuit = None;
        }
        self.session = session;
        self.race = Self::make_race(&self.track, session, &self.settings, self.bench.is_some());
        self.fit_race_buffers();
        self.recorder = GhostRecorder::default();
        self.knockout = None;
        self.camera.reset();
        let mut menu = MenuState::new(if matches!(session, Session::Attract) {
            Screen::Title
        } else {
            Screen::Playing
        });
        // Online choices survive a restart.
        menu.mode = self.menu.mode;
        menu.gamepad_guide = self.menu.gamepad_guide;
        menu.join_addr = std::mem::take(&mut self.menu.join_addr);
        menu.peers = std::mem::take(&mut self.menu.peers);
        menu.lobby_host = self.menu.lobby_host;
        menu.lobby_elimination = self.menu.lobby_elimination;
        menu.event = self.menu.event;
        menu.circuit_next = self.circuit.is_some_and(|c| !c.is_final());
        self.menu = menu;
        // Hosting: everyone connected gets a seat; bots fill the rest.
        if let (Net::Host(h), Session::Play(mode)) = (&mut self.net, session) {
            h.setup = RaceSetup {
                track: self.settings.track as u8,
                elimination: mode == Mode::Elimination,
                laps: self.settings.laps as u8,
                rivals: self.settings.rivals as u8,
                circuit: self.circuit,
            };
            let count = h.racer_count();
            self.race = if mode == Mode::Elimination {
                Race::new_elimination(&self.track, count, true)
            } else {
                Race::new(&self.track, count, true, self.settings.laps)
            };
            h.start(&mut self.race);
            self.fit_race_buffers();
        }
    }

    /// Starts a race in `mode`: one race on the equipped track, or round one of a circuit.
    fn begin(&mut self, mode: Mode, event: Event) {
        self.circuit = (event == Event::Circuit && mode != Mode::TimeTrial).then(Circuit::default);
        self.play_round(mode);
    }

    /// Starts the current circuit round (loading its track), or a plain race.
    fn play_round(&mut self, mode: Mode) {
        match self.circuit {
            Some(c) if self.settings.track != c.track() => {
                self.settings.track = c.track();
                // load_track restarts the current session, which is now the race.
                self.session = Session::Play(mode);
                self.load_track();
            }
            _ => self.start(Session::Play(mode)),
        }
    }

    /// Scores the round just run and starts the next one.
    fn next_round(&mut self) {
        let Session::Play(mode) = self.session else {
            return;
        };
        let standings = self.race.standings(&self.track);
        if let Some(next) = self.circuit.and_then(|c| c.next(&standings)) {
            self.circuit = Some(next);
            self.play_round(mode);
        }
    }

    /// Props around the circuit: everything, or just the track's own features.
    fn scenery_for(&self, track: &Track) -> Vec<scenery::Placement> {
        if self.settings.scenery {
            scenery::placements(track)
        } else {
            scenery::track_features(track)
        }
    }

    /// Per-kart effect buffers sized to the current race.
    fn fit_race_buffers(&mut self) {
        self.kart_of = kart_assignment(&self.race, &self.karts, self.settings.loadout.design);
        let n = self.race.racers.len();
        self.effects = (0..n).map(|_| Effects::new()).collect();
        self.leans = vec![0.0; n];
        self.wheel_spin = vec![[0.0; 4]; n];
    }

    fn show_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), self.attract_t));
    }

    /// Starts hosting and opens the lobby.
    fn host_game(&mut self, loopback: bool) {
        let setup = RaceSetup {
            track: self.settings.track as u8,
            elimination: self.menu.lobby_elimination,
            laps: self.settings.laps as u8,
            rivals: self.settings.rivals as u8,
            circuit: None,
        };
        let addr = if loopback {
            ([127, 0, 0, 1], 0)
        } else {
            ([0, 0, 0, 0], netplay::DEFAULT_PORT)
        };
        match NetHost::bind_addr(addr.into(), setup, &self.player_name) {
            Ok(h) => {
                if loopback {
                    println!("SCRAPBURN_HOST_PORT={}", h.port());
                }
                log::info!("hosting on UDP port {}", h.port());
                self.net = Net::Host(Box::new(h));
                self.menu.lobby_host = true;
                self.menu.go(Screen::Lobby);
            }
            Err(e) => {
                if loopback {
                    println!("SCRAPBURN_HOST_CLOSED");
                }
                self.show_toast(format!("COULDN'T HOST: {e}").to_ascii_uppercase());
            }
        }
    }

    fn private_game(&mut self, host: bool) {
        self.resolving = None;
        let invite = if host {
            None
        } else {
            match self.paste_invite() {
                Some(invite) => Some(invite),
                None => {
                    self.show_toast("COPY YOUR HOST'S INVITE, THEN TRY AGAIN");
                    return;
                }
            }
        };
        let port = if host {
            self.host_game(true);
            let Net::Host(h) = &self.net else { return };
            Some(h.port())
        } else {
            None
        };
        match crate::tailcat::Tunnel::start(port, invite.as_deref()) {
            Ok(tunnel) => self.tunnel = Some(tunnel),
            Err(_) => self.close_private("BUNDLED TAILCAT HELPER COULD NOT START"),
        }
    }

    /// The clipboard, opened on first use.
    fn clipboard(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.clipboard.is_none() {
            self.clipboard = arboard::Clipboard::new()
                .inspect_err(|e| log::warn!("clipboard unavailable: {e}"))
                .ok();
        }
        self.clipboard.as_mut()
    }

    /// A Tailcat invite from the clipboard, if it holds one.
    fn paste_invite(&mut self) -> Option<String> {
        let text = self.clipboard()?.get_text().ok()?;
        let invite = text.trim();
        let plausible = invite.len() <= 8192
            && invite.starts_with("tc")
            && invite
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        plausible.then(|| invite.to_string())
    }

    fn copy_invite(&mut self) {
        let Some(invite) = self.invite.clone() else {
            return;
        };
        let copied = self
            .clipboard()
            .is_some_and(|c| c.set_text(invite.as_str()).is_ok());
        if copied {
            self.show_toast("INVITE COPIED. PASTE IT TO YOUR FRIENDS");
        } else {
            // Still shareable: the host can copy it from the terminal.
            println!("Scrapburn private race invite: {invite}");
            self.show_toast("COULDN'T COPY. THE INVITE IS IN THE TERMINAL");
        }
    }

    fn close_private(&mut self, reason: &str) {
        self.leave_online();
        self.menu.go(Screen::Online);
        if !reason.is_empty() {
            self.show_toast(reason);
        }
    }

    fn poll_private(&mut self) {
        if self.tunnel.is_none() {
            self.invite = None;
        }
        self.menu.invite_ready = self.invite.is_some() && matches!(self.net, Net::Host(_));
        let Some(tunnel) = self.tunnel.as_mut() else {
            return;
        };
        if !tunnel.ready && self.menu.screen == Screen::Title {
            self.tunnel = None;
            return;
        }
        match tunnel.poll() {
            Some(crate::tailcat::Event::Invite(invite)) => self.invite = Some(invite),
            Some(crate::tailcat::Event::Hosted) => {
                tunnel.ready = true;
                self.copy_invite();
            }
            Some(crate::tailcat::Event::Join(port)) => {
                tunnel.ready = true;
                self.connect_to(([127, 0, 0, 1], port).into());
                if !matches!(self.net, Net::Client { .. }) {
                    self.tunnel = None;
                }
            }
            Some(crate::tailcat::Event::Closed(reason)) => self.close_private(&reason),
            None => {}
        }
    }

    /// Joins `addr`: IP addresses connect at once, names are looked up on a thread first.
    fn join_game(&mut self, addr: &str) {
        let literal = addr.parse::<std::net::SocketAddr>().is_ok()
            || addr.parse::<std::net::IpAddr>().is_ok();
        if literal {
            if let Some(server) = netplay::resolve(addr) {
                self.connect_to(server);
            }
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let name = addr.to_string();
        std::thread::spawn(move || {
            let _ = tx.send(netplay::resolve(&name));
        });
        self.resolving = Some((addr.to_string(), rx));
        self.show_toast(format!("LOOKING UP {}", addr.to_ascii_uppercase()));
    }

    /// Finishes a pending name lookup.
    fn poll_resolve(&mut self) {
        let Some((name, rx)) = &self.resolving else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => None,
        };
        let name = name.clone();
        self.resolving = None;
        match result {
            Some(server) => self.connect_to(server),
            None => self.show_toast(format!("CAN'T FIND {}", name.to_ascii_uppercase())),
        }
    }

    /// Connects to `server` and waits in the lobby.
    fn connect_to(&mut self, server: std::net::SocketAddr) {
        match NetClient::connect(server, &self.player_name) {
            Ok(link) => {
                self.net = Net::Client {
                    link: Box::new(link),
                    view: None,
                    lobby: None,
                    sat_out: None,
                };
                self.menu.lobby_host = false;
                self.menu.go(Screen::Lobby);
                self.show_toast(format!("CONNECTING TO {server}"));
            }
            Err(e) => self.show_toast(format!("COULDN'T CONNECT: {e}").to_ascii_uppercase()),
        }
    }

    /// Hosting, on the results screen, with a remote player still on track.
    fn remote_still_racing(&self) -> bool {
        matches!(self.net, Net::Host(_))
            && self.menu.screen == Screen::Results
            && self
                .race
                .racers
                .iter()
                .any(|r| r.remote && r.finish.is_none() && !r.out)
    }

    /// Leaves any online game and returns to the title.
    fn leave_online(&mut self) {
        self.tunnel = None;
        if let Net::Host(h) = &self.net
            && h.is_loopback()
        {
            println!("SCRAPBURN_HOST_CLOSED");
        }
        self.net = Net::Off;
        self.start(Session::Attract);
    }

    /// Back to the lobby after an online race (still connected).
    fn back_to_lobby(&mut self) {
        if let Net::Host(h) = &mut self.net {
            h.end_race();
        }
        if let Net::Client { view, sat_out, .. } = &mut self.net {
            // Sit out the rest of this race instead of being pulled back in.
            *sat_out = view.take().map(|v| v.race_id);
        }
        self.start(Session::Attract);
        self.menu.go(Screen::Lobby);
    }

    /// A client builds its copy of the host's race from the first snapshot.
    fn start_client_race(&mut self, snap: &Snapshot) {
        self.circuit = snap.setup.circuit;
        if snap.setup.track as usize != self.settings.track {
            self.settings.track = snap.setup.track as usize;
            self.load_track();
        }
        let mode = if snap.setup.elimination {
            Mode::Elimination
        } else {
            Mode::Race
        };
        self.start(Session::Play(mode));
        let n = snap.racers.len();
        let you = snap.you as usize;
        let mut race = if snap.setup.elimination {
            Race::new_elimination(&self.track, n, true)
        } else {
            Race::new(&self.track, n, true, snap.setup.laps as u32)
        };
        race.player = Some(you);
        for r in &mut race.racers {
            r.bot = None;
        }
        race.racers[you].state = snap.racers[you].state.clone();
        race.racers[you].prev = race.racers[you].state.clone();
        self.race = race;
        self.fit_race_buffers();
        if let Net::Client { view, .. } = &mut self.net {
            *view = Some(ClientView::new(you, snap.race_id, n));
        }
    }

    /// Network input for this frame: joins, leaves, lobby updates and race snapshots.
    fn net_poll(&mut self, now: Instant) {
        match &mut self.net {
            Net::Off => {}
            Net::Host(h) => {
                let racing = matches!(self.session, Session::Play(_)) && h.in_race();
                let notes = h.poll(now, racing.then_some(&mut self.race));
                if !h.in_race() {
                    // Clients see the planned event in the lobby; a circuit opens on its first track.
                    h.setup.circuit = (self.menu.event == Event::Circuit).then(Circuit::default);
                    h.setup.track =
                        h.setup.circuit.map_or(self.settings.track, |c| c.track()) as u8;
                    h.setup.elimination = self.menu.lobby_elimination;
                    self.net_ticks += 1;
                    if self.net_ticks.is_multiple_of(6) {
                        h.send(None);
                    }
                }
                // Benchmarks (automated two-machine runs) start as soon as someone joins.
                let autostart = self.bench.is_some() && !h.in_race() && h.player_count() > 1;
                for n in notes {
                    match n {
                        HostNote::Joined(name) => self.show_toast(format!("{name} JOINED")),
                        HostNote::Left(_) => self.show_toast("A PLAYER LEFT"),
                    }
                }
                if autostart {
                    self.begin(Mode::Race, Event::Single);
                }
            }
            Net::Client { link, .. } => {
                let notes = link.poll(now);
                for n in notes {
                    self.client_note(n);
                }
            }
        }
    }

    fn client_note(&mut self, note: ClientNote) {
        match note {
            ClientNote::Connected => self.show_toast("CONNECTED"),
            ClientNote::Rejected(reason) => {
                self.tunnel = None;
                self.net = Net::Off;
                self.start(Session::Attract);
                self.menu.go(Screen::Online);
                self.show_toast(format!("REJECTED: {}", reason.to_ascii_uppercase()));
            }
            ClientNote::Lost => {
                self.leave_online();
                self.show_toast("CONNECTION LOST");
            }
            ClientNote::Lobby(l) => {
                let racing = matches!(self.session, Session::Play(_));
                if let Net::Client { lobby, .. } = &mut self.net {
                    *lobby = Some(l);
                }
                // The host ended the race: back to the lobby.
                if racing {
                    self.back_to_lobby();
                }
            }
            ClientNote::Snapshot(snap) => {
                let fresh = match &self.net {
                    Net::Client {
                        sat_out: Some(id), ..
                    } if *id == snap.race_id => return,
                    Net::Client { view: Some(v), .. } => v.race_id != snap.race_id,
                    Net::Client { view: None, .. } => true,
                    _ => return,
                };
                if fresh {
                    self.start_client_race(&snap);
                }
                let events = match &mut self.net {
                    Net::Client { view: Some(v), .. } => v.apply(&mut self.race, &snap),
                    _ => return,
                };
                self.after_step(&events);
                self.combat_effects(&events);
                self.frame_events.extend(events);
            }
        }
    }

    /// Fetches tailnet peers in the background while the Online screen is open.
    fn poll_peers(&mut self) {
        if !matches!(self.menu.screen, Screen::Online | Screen::Tailscale) {
            return;
        }
        if let Some(rx) = &self.peers_rx {
            if let Ok(peers) = rx.try_recv() {
                self.menu.peers = peers
                    .into_iter()
                    .map(|p| (p.name, p.ip.to_string()))
                    .collect();
                self.peers_rx = None;
                self.peers_at = self.attract_t;
            }
            return;
        }
        // Refresh every few seconds while the screen is open.
        if self.attract_t - self.peers_at < 5.0 {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(netplay::tailscale_peers());
        });
        self.peers_rx = Some(rx);
    }

    /// The racer the camera and HUD follow.
    fn focus(&self) -> usize {
        match self.session {
            Session::Attract => {
                // The attract camera features a different kart every 8 seconds.
                (self.attract_t / 8.0) as usize % self.race.racers.len()
            }
            Session::Play(_) => self.race.player.unwrap_or(0),
        }
    }

    /// Switches to `settings.track`: geometry, scenery, lighting, ghosts, and a fresh race.
    fn load_track(&mut self) {
        self.track = build_track(&TRACKS[self.settings.track % TRACKS.len()]);
        self.track_meshes = [&self.track.road, &self.track.terrain, &self.track.props]
            .map(|m| GpuMesh::new(&self.ctx.device, m));
        self.placements = self.scenery_for(&self.track);
        self.flame_points = flame_points(&self.track, &self.placements);
        self.flame_fx = ParticleSystem::new(1500);
        self.scene.lighting = scrapburn::track::lighting(self.track.id);
        self.ghost_file = scrapburn::ghost::data_dir()
            .map(|d| scrapburn::ghost::ghost_path(&d, self.track.id.key()));
        self.best_ghost =
            BestGhost::from_ghost(self.ghost_file.as_deref().and_then(scrapburn::ghost::load));
        // Rebuilding the attract race must preserve the browser and its return destination.
        let browser = matches!(self.session, Session::Attract).then(|| self.menu.clone());
        self.start(self.session);
        if let Some(menu) = browser {
            self.menu = menu;
        }
    }

    fn rebuild_renderer(&mut self) {
        let lighting = self.scene.lighting;
        self.scene = SceneRenderer::new(
            &self.ctx.device,
            &self.ctx.queue,
            self.ctx.format(),
            self.ctx.size(),
            self.settings.preset,
            &self.layers,
            &self.shading_table,
        );
        self.scene.lighting = lighting;
    }

    /// Returns `Some(exit code)` when the app should quit.
    fn frame(&mut self, since_launch: f64) -> Option<ExitCode> {
        match self.launch_net.take() {
            Some(LaunchNet::Host { loopback }) => self.host_game(loopback),
            Some(LaunchNet::Join(addr)) => self.join_game(&addr),
            None => {}
        }
        self.poll_private();
        self.poll_peers();
        self.poll_resolve();
        let now = Instant::now();
        let frame_dt = now.duration_since(self.last_frame).as_secs_f64();
        self.last_frame = now;
        if let Some(p) = self.pads.as_mut() {
            p.poll();
        }
        // Benchmarks ignore gamepads (stray events from macro pads would drive the menus).
        let pad = if self.bench.is_some() {
            None
        } else {
            self.pads.as_ref().and_then(Gamepads::active).copied()
        };
        let pad = pad.as_ref();
        let act = actions(&self.keys, pad);
        if act.toggle_graph {
            self.settings.fps = self.settings.fps.next();
        }

        // --- Menus ---
        let mut simulate = true;
        match self.menu.screen {
            Screen::Playing => {
                let pause =
                    act.quit || pad.is_some_and(|p| p.just_pressed.contains(PadButtons::START));
                if pause && self.bench.is_none() {
                    self.menu.go(Screen::Paused);
                }
                if let Some(p) = self.race.player {
                    if act.respawn {
                        self.race.respawn_racer(p, &self.track);
                    }
                    if act.fire {
                        match &mut self.net {
                            // The host fires for us.
                            Net::Client { link, .. } => link.fires = link.fires.wrapping_add(1),
                            _ => self.race.fire(p),
                        }
                    }
                }
            }
            Screen::Title
            | Screen::RaceSetup
            | Screen::Circuits
            | Screen::Controls
            | Screen::DriverName
            | Screen::Garage
            | Screen::Settings
            | Screen::Paused
            | Screen::Results
            | Screen::Online
            | Screen::Tailscale
            | Screen::Lobby => {
                // Online races can't be paused.
                let online = !matches!(self.net, Net::Off);
                let paused_game = matches!(self.session, Session::Play(_))
                    && self.menu.screen != Screen::Results
                    && !online;
                simulate = !paused_game && self.menu.screen != Screen::Garage;
                let typing = (self.menu.screen == Screen::DriverName && self.menu.selected == 0)
                    || self.menu.screen == Screen::Online
                        && self.menu.selected == scrapburn::menu::ONLINE_ADDRESS;
                let mut input = if typing {
                    menu_input_typing(&self.keys, pad)
                } else {
                    menu_input(&self.keys, pad)
                };
                if let Some((screen, row, previous)) = self.clicked.take()
                    && screen == self.menu.screen
                    && row < self.menu.items(&self.settings).len()
                {
                    self.menu.selected = row;
                    // Clicking the name field focuses it; Enter or SAVE NAME submits.
                    let text_field = screen == Screen::DriverName && row == 0;
                    input = scrapburn::menu::MenuInput {
                        left: previous && !text_field,
                        select: !previous && !text_field,
                        ..Default::default()
                    };
                }
                let pending = self.tunnel.as_ref().is_some_and(|t| !t.ready);
                if pending && input.back {
                    self.close_private("");
                    input = Default::default();
                } else if pending {
                    input = Default::default();
                }
                match self.menu.handle(input, &mut self.settings) {
                    MenuAction::None => {}
                    MenuAction::StartRace => self.begin(Mode::Race, self.menu.solo_event()),
                    MenuAction::StartTimeTrial => self.begin(Mode::TimeTrial, Event::Single),
                    MenuAction::StartElimination => {
                        self.begin(Mode::Elimination, self.menu.solo_event())
                    }
                    MenuAction::Resume => self.menu.go(Screen::Playing),
                    MenuAction::Restart => match (&self.net, self.session) {
                        // Only the host decides when to race again.
                        (Net::Client { .. }, _) => {}
                        // Racing again after the final round starts a fresh circuit.
                        (_, Session::Play(mode))
                            if self.circuit.is_some() && self.menu.screen == Screen::Results =>
                        {
                            self.begin(mode, Event::Circuit)
                        }
                        _ => self.start(self.session),
                    },
                    MenuAction::NextRace if matches!(self.net, Net::Client { .. }) => {
                        self.show_toast("WAITING FOR THE HOST")
                    }
                    MenuAction::NextRace if online && self.remote_still_racing() => {
                        self.show_toast("WAITING FOR EVERYONE TO FINISH")
                    }
                    MenuAction::NextRace => self.next_round(),
                    MenuAction::CopyInvite => self.copy_invite(),
                    MenuAction::ToTitle if online && self.remote_still_racing() => {
                        self.show_toast("WAITING FOR EVERYONE TO FINISH")
                    }
                    MenuAction::ToTitle if online => self.back_to_lobby(),
                    MenuAction::ToTitle => self.start(Session::Attract),
                    MenuAction::Quit => return Some(ExitCode::SUCCESS),
                    MenuAction::PresetChanged => {
                        self.tuner.stop();
                        self.rebuild_renderer();
                    }
                    MenuAction::SaveName => {
                        self.player_name =
                            rivet_net::clean_name(&self.menu.name_entry).to_ascii_uppercase();
                        if let Some(dir) = scrapburn::ghost::data_dir() {
                            let tmp = dir.join(format!("racer-name.{}.tmp", std::process::id()));
                            if std::fs::create_dir_all(&dir)
                                .and_then(|_| std::fs::write(&tmp, &self.player_name))
                                .and_then(|_| std::fs::rename(&tmp, dir.join("racer-name")))
                                .is_err()
                            {
                                self.show_toast("NAME SET FOR THIS SESSION");
                            }
                        }
                    }
                    MenuAction::SaveGarage => {
                        if let Some(dir) = scrapburn::ghost::data_dir()
                            && let Err(e) = self.settings.loadout.save(&dir)
                        {
                            log::warn!("saving garage: {e}");
                            self.show_toast("GARAGE EQUIPPED FOR THIS SESSION");
                        }
                    }
                    MenuAction::TrackChanged => self.load_track(),
                    MenuAction::SceneryChanged => {
                        self.placements = self.scenery_for(&self.track);
                        self.flame_points = flame_points(&self.track, &self.placements);
                    }
                    MenuAction::PrivateHost => self.private_game(true),
                    MenuAction::PrivateJoin => self.private_game(false),
                    MenuAction::HostGame => self.host_game(false),
                    MenuAction::JoinGame => {
                        if let Some(addr) = self.menu.join_target() {
                            self.join_game(&addr);
                        }
                    }
                    MenuAction::LobbyStart => {
                        let mode = if self.menu.lobby_elimination {
                            Mode::Elimination
                        } else {
                            Mode::Race
                        };
                        self.begin(mode, self.menu.event);
                    }
                    MenuAction::LeaveOnline => self.leave_online(),
                }
            }
        }
        let playing = self.menu.screen == Screen::Playing;
        let input = if playing {
            drive_input(&self.keys, pad)
        } else {
            DriveInput::default()
        };
        self.keys.end_frame();
        let digital = pad.is_none_or(|p| p.left_x.abs() < 0.01);
        let input = DriveInput {
            steer: self.steer.apply(input.steer, digital, frame_dt as f32),
            ..input
        };

        // --- Simulation ---
        // Paused: don't advance the clock, or interpolation keeps cycling and karts jitter.
        let steps = if simulate {
            self.clock.advance(frame_dt)
        } else {
            0
        };
        self.frame_events.clear();
        self.net_poll(now);
        if simulate {
            for _ in 0..steps {
                if let Net::Client {
                    link,
                    view: Some(view),
                    ..
                } = &mut self.net
                    && matches!(self.session, Session::Play(_))
                {
                    // A client drives its own kart; everything else comes from the host.
                    let you = view.you;
                    // Benchmarks drive the client's kart with the autopilot.
                    let input = if self.bench.is_some() {
                        scrapburn::autopilot::autopilot(
                            &self.race.racers[you].state,
                            &self.track.centerline,
                        )
                    } else {
                        input
                    };
                    self.race.step_own(&self.track, you, input, TICK_DT as f32);
                    view.tick_others(&mut self.race, TICK_DT as f32);
                    link.send_kart(&self.race.racers[you].state, view, now);
                    continue;
                }
                let events = self.race.step(&self.track, input, TICK_DT as f32);
                if let Net::Host(h) = &mut self.net
                    && h.in_race()
                {
                    h.record(&events);
                    self.net_ticks += 1;
                    if self.net_ticks.is_multiple_of(2) {
                        h.send(Some(&self.race));
                    }
                }
                self.after_step(&events);
                self.combat_effects(&events);
                self.frame_events.extend(events);
            }
        }
        self.attract_t += frame_dt as f32;
        if let (Session::Play(_), Some(p)) = (self.session, self.race.player)
            && self.menu.screen == Screen::Playing
            && let Some((_, t)) = self.race.racers[p].finish
            && self.race.time - t > RESULTS_DELAY
        {
            self.menu.go(Screen::Results);
        }

        // --- Stats and bench ---
        let cpu_ms = (frame_dt * 1000.0) as f32;
        let gpu_ms = self
            .timer
            .as_mut()
            .and_then(|t| t.poll_ms(&self.ctx.device));
        self.stats.push(cpu_ms, gpu_ms);
        // Auto graphics: settle the preset on the title screen. Without a GPU timer there is
        // nothing reliable to measure, so play safe on Medium.
        if self.tuner.active() && self.timer.is_none() {
            self.tuner.stop();
            self.settings.preset = rivet_render::Preset::Medium;
            self.rebuild_renderer();
        }
        if self.tuner.active() && matches!(self.session, Session::Attract) {
            // GPU time only: CPU frame time includes vsync waits.
            if let Some(p) = gpu_ms.and_then(|g| self.tuner.feed(g)) {
                log::info!("auto graphics: stepping down to {p:?}");
                self.settings.preset = p;
                self.rebuild_renderer();
            }
            if !self.tuner.active() {
                log::info!("auto graphics: settled on {:?}", self.settings.preset);
            }
        }
        if let Some(b) = self.bench.as_mut()
            && b.record(since_launch, cpu_ms, gpu_ms) == BenchPhase::Done
        {
            let info = &self.ctx.adapter_info;
            println!(
                "{}",
                b.line(
                    &info.name,
                    &format!("{:?}", info.backend),
                    self.ctx.size(),
                    &self.ctx.present_label()
                )
            );
            if let Some(path) = &self.screenshot {
                match self.ctx.capture_offscreen() {
                    Some((w, h, rgba)) => {
                        match std::fs::write(path, scrapburn::png::encode_rgba(w, h, &rgba)) {
                            Ok(()) => println!("BENCH screenshot={}", path.display()),
                            Err(e) => eprintln!("writing screenshot failed: {e}"),
                        }
                    }
                    None => eprintln!("screenshot capture failed"),
                }
            }
            match b.write_csv(std::path::Path::new("target/bench")) {
                Ok(p) => println!("BENCH csv={}", p.display()),
                Err(e) => eprintln!("writing bench CSV failed: {e}"),
            }
            return Some(ExitCode::SUCCESS);
        }
        if let Some(audio) = &self.audio {
            let active = matches!(self.session, Session::Play(_))
                && matches!(self.menu.screen, Screen::Playing | Screen::Results);
            let view = self.camera.view();
            let right = view.row(0).truncate().normalize_or(Vec3::X);
            let listener = scrapburn::sound::Listener {
                ear: self.camera.eye(),
                right,
            };
            self.sound.update(
                audio,
                &self.race,
                &self.frame_events,
                listener,
                self.focus(),
                active,
            );
        }
        if !self.minimized {
            self.render(frame_dt as f32, simulate);
        }
        None
    }

    /// Ghost recording and saving in time trial.
    fn after_step(&mut self, events: &[RaceEvent]) {
        let (Session::Play(Mode::TimeTrial), Some(p)) = (self.session, self.race.player) else {
            return;
        };
        let r = &self.race.racers[p];
        let lap_t = (self.race.time - r.laps.lap_start()) as f32;
        if matches!(self.race.phase, Phase::Racing) {
            self.recorder
                .record(lap_t, r.state.position, r.state.rotation);
        }
        for e in events {
            if let RaceEvent::Lap { racer, time, .. } = *e
                && racer == p
            {
                let lap = self.recorder.finish(time);
                if self.best_ghost.offer(lap)
                    && let (Some(path), Some(g)) = (&self.ghost_file, self.best_ghost.ghost())
                {
                    match scrapburn::ghost::save(path, g) {
                        Ok(()) => {
                            log::info!("new best lap {time:.3} s saved to {}", path.display())
                        }
                        Err(e) => log::warn!("saving the ghost failed: {e}"),
                    }
                }
            }
        }
    }

    /// Sparks, lightning and blasts for weapon events.
    fn combat_effects(&mut self, events: &[RaceEvent]) {
        let focus_pos = self.race.racers[self.focus()].state.position;
        let mut rng = Lcg((self.race.time * 1000.0) as u32 | 1);
        for e in events {
            if let RaceEvent::Eliminated { racer, .. } = *e {
                // Knocked out: the kart goes up in a fireball.
                let p = self.race.racers[racer].state.position + Vec3::Y * 0.8;
                fx_burst(
                    &mut self.combat_fx,
                    &mut rng,
                    p,
                    140,
                    18.0,
                    [9.0, 4.0, 1.2, 1.0],
                    1.0,
                    1.0,
                );
                fx_burst(
                    &mut self.combat_fx,
                    &mut rng,
                    p,
                    60,
                    7.0,
                    [1.5, 1.2, 1.0, 1.0],
                    1.6,
                    1.4,
                );
                self.camera
                    .kick((1.0 - p.distance(focus_pos) / 60.0).max(0.0));
                self.knockout = Some((racer, self.race.time));
                continue;
            }
            let RaceEvent::Combat(c) = e else { continue };
            match *c {
                CombatEvent::Hit { victim, .. } => {
                    let p = self.race.racers[victim].state.position + Vec3::Y * 0.6;
                    fx_burst(
                        &mut self.combat_fx,
                        &mut rng,
                        p,
                        30,
                        9.0,
                        [6.0, 4.0, 1.5, 1.0],
                        0.3,
                        0.5,
                    );
                }
                CombatEvent::Zap { victim } => {
                    let to = self.race.racers[victim].state.position + Vec3::Y * 0.6;
                    if let Some(orb) = self
                        .race
                        .combat
                        .orbs
                        .iter()
                        .min_by(|a, b| a.pos.distance(to).total_cmp(&b.pos.distance(to)))
                    {
                        // A jagged bolt from the orb to the victim.
                        let from = orb.pos;
                        for k in 0..24 {
                            let t = k as f32 / 23.0;
                            let jitter =
                                Vec3::new(rng.next() - 0.5, rng.next() - 0.5, rng.next() - 0.5)
                                    * 1.2
                                    * (1.0 - (2.0 * t - 1.0).abs());
                            self.combat_fx.spawn(Particle {
                                pos: from.lerp(to, t) + jitter,
                                vel: Vec3::ZERO,
                                age: 0.0,
                                life: 0.25,
                                size: (0.45, 0.1),
                                color: ([3.0, 8.0, 4.0, 1.0], [0.5, 2.0, 1.0, 0.0]),
                                gravity: 0.0,
                                drag: 0.0,
                                blend: Blend::Additive,
                            });
                        }
                    }
                    fx_burst(
                        &mut self.combat_fx,
                        &mut rng,
                        to,
                        20,
                        6.0,
                        [2.0, 7.0, 3.0, 1.0],
                        0.3,
                        0.4,
                    );
                }
                CombatEvent::Explode { pos, .. } => {
                    fx_burst(
                        &mut self.combat_fx,
                        &mut rng,
                        pos,
                        160,
                        24.0,
                        [3.0, 9.0, 3.5, 1.0],
                        1.1,
                        1.0,
                    );
                    fx_burst(
                        &mut self.combat_fx,
                        &mut rng,
                        pos,
                        90,
                        14.0,
                        [9.0, 5.0, 1.5, 1.0],
                        0.9,
                        0.8,
                    );
                    let near = (1.0 - pos.distance(focus_pos) / 60.0).max(0.0);
                    self.camera.kick(near);
                }
                CombatEvent::Slingshot { racer } => {
                    let p = self.race.racers[racer].state.position;
                    fx_burst(
                        &mut self.combat_fx,
                        &mut rng,
                        p,
                        40,
                        10.0,
                        [2.0, 4.0, 8.0, 1.0],
                        0.4,
                        0.5,
                    );
                }
                CombatEvent::Pickup { racer, .. } => {
                    let p = self.race.racers[racer].state.position + Vec3::Y;
                    fx_burst(
                        &mut self.combat_fx,
                        &mut rng,
                        p,
                        24,
                        6.0,
                        [2.0, 5.0, 8.0, 1.0],
                        0.25,
                        0.4,
                    );
                }
                CombatEvent::Fire { .. } => {}
            }
        }
    }

    fn render(&mut self, frame_dt: f32, simulating: bool) {
        let alpha = self.clock.alpha();
        let focus = self.focus();
        let fx_dt = if simulating { frame_dt } else { 0.0 };
        let mut models = Vec::with_capacity(self.race.racers.len());
        for (i, r) in self.race.racers.iter().enumerate() {
            let (pos, rot) = VehicleState::lerp(&r.prev, &r.state, alpha);
            self.effects[i].dusty_road = self.track.id == scrapburn::track::TrackId::Dustbowl;
            let shake = self.effects[i].update(&r.state, &r.params, fx_dt);
            if i == focus {
                self.camera.kick(shake);
            }
            let drift_roll = r.state.drift.map_or(0.0, |d| d.dir * 0.10);
            let turn_roll = (r.state.angular_yaw * r.state.speed() * 0.006).clamp(-0.18, 0.18);
            let target = if r.state.grounded {
                turn_roll + drift_roll
            } else {
                0.0
            };
            self.leans[i] += (target - self.leans[i]) * (1.0 - (-8.0 * frame_dt).exp());
            // Each tire rolls at its own rate, so big and small ones both keep pace with the road.
            let radii = self.karts[self.kart_of[i]].radii;
            for ((spin, radius), hub) in self.wheel_spin[i]
                .iter_mut()
                .zip(radii)
                .zip(self.karts[self.kart_of[i]].hubs)
            {
                let tire_speed = r.state.speed() - r.state.angular_yaw * hub.x;
                *spin = (*spin + tire_speed * fx_dt / radius) % std::f32::consts::TAU;
            }
            models.push((
                pos,
                rot,
                Mat4::from_rotation_translation(rot, pos) * Mat4::from_rotation_z(self.leans[i]),
            ));
        }
        let garage = self.menu.screen == Screen::Garage;
        let (mut fpos, frot, _) = models[focus];
        if garage {
            fpos = self.track.spawn.0 + Vec3::Y * scrapburn::kart::GROUND_OFFSET;
            let rotation = glam::Quat::from_rotation_y(self.track.spawn.1 + self.attract_t * 0.45);
            models[focus] = (
                fpos,
                rotation,
                Mat4::from_rotation_translation(rotation, fpos),
            );
        }
        let fstate = &self.race.racers[focus].state;
        self.camera.set_boost(fstate.boost_timer > 0.0);
        self.camera.update(fpos, frot, fstate.speed(), frame_dt);
        let collision = &self.track.collision;
        self.camera.keep_clear(
            |o, d, max| collision.raycast(o, d, max).map(|h| h.t),
            frame_dt,
        );
        let (w, h) = self.ctx.size();
        let (view_m, eye) = if garage {
            let offset = glam::Quat::from_rotation_y(self.track.spawn.1);
            let eye = fpos + offset * Vec3::new(2.2, 1.15, 3.5);
            let aim = fpos + Vec3::Y * 0.3 + (fpos - eye).cross(Vec3::Y).normalize() * -1.0;
            (glam::camera::rh::view::look_at_mat4(eye, aim, Vec3::Y), eye)
        } else if matches!(self.session, Session::Attract) {
            // Cinematic orbit around the featured kart.
            let a = self.attract_t * 0.22;
            let mut eye =
                fpos + Vec3::new(a.cos() * 11.0, 3.2 + (a * 0.7).sin() * 1.5, a.sin() * 11.0);
            // Never orbit into a dune or a wall: stay above the ground and the barrier tops.
            let ground = scrapburn::terrain::height_for(
                self.track.terrain_kind,
                &self.track.centerline,
                eye.x,
                eye.z,
            );
            let wall = TRACKS
                .iter()
                .find(|d| d.id == self.track.id)
                .map_or(1.5, |d| d.style.barrier_height);
            eye.y = eye.y.max(ground + 2.5).max(fpos.y + wall + 1.8);
            (
                glam::camera::rh::view::look_at_mat4(eye, fpos + Vec3::Y * 0.8, Vec3::Y),
                eye,
            )
        } else {
            (self.camera.view(), self.camera.eye())
        };
        let view = View {
            view: view_m,
            proj: self.camera.proj(w as f32 / h as f32),
            eye,
            focus: fpos,
            time: self.attract_t,
        };
        self.draw_hud(w as f32, h as f32, focus, view.proj * view.view, &models);
        self.overlay
            .prepare(&self.ctx.device, &self.ctx.queue, w, h);

        let Some(frame) = self.ctx.begin_frame() else {
            return;
        };
        let mut enc = self.ctx.device.create_command_encoder(&Default::default());
        // Road, terrain, and (with scenery on) the rocks.
        let shown = if self.settings.scenery { 3 } else { 2 };
        let mut draws: Vec<Draw> = self
            .track_meshes
            .iter()
            .take(if garage { 0 } else { shown })
            .map(|m| Draw::new(m, Mat4::IDENTITY))
            .collect();
        if garage {
            draws.push(Draw::new(
                &self.service_bay,
                Mat4::from_rotation_translation(
                    glam::Quat::from_rotation_y(self.track.spawn.1),
                    self.track.spawn.0,
                ),
            ));
        }
        for (i, (pos, rot, model)) in models.iter().enumerate() {
            // Knocked-out rivals leave the track (the player's own wreck stays in view).
            if (garage || self.race.racers[i].out) && i != focus {
                continue;
            }
            let tint = if garage || self.race.player == Some(i) {
                scrapburn::race::TINTS[self.settings.loadout.finish]
            } else {
                self.race.racers[i].tint
            };
            let kart = if garage {
                self.karts
                    .iter()
                    .find(|k| k.design == self.settings.loadout.design)
                    .unwrap_or(&self.karts[0])
            } else {
                &self.karts[self.kart_of[i]]
            };
            draws.push(Draw {
                mesh: &kart.body,
                model: *model,
                casts_shadow: true,
                tint,
            });
            // Wheels roll with speed; the front pair steers with the turn rate.
            let steer = (self.race.racers[i].state.angular_yaw * 0.3).clamp(-0.45, 0.45);
            for (k, mesh) in kart.wheels.iter().enumerate() {
                let roll = if garage {
                    Mat4::IDENTITY
                } else {
                    Mat4::from_rotation_x(self.wheel_spin[i][k])
                };
                let yaw = if !garage && k < 2 {
                    Mat4::from_rotation_y(steer)
                } else {
                    Mat4::IDENTITY
                };
                let m = Mat4::from_rotation_translation(*rot, *pos)
                    * Mat4::from_translation(kart.hubs[k])
                    * yaw
                    * roll;
                draws.push(Draw {
                    mesh,
                    model: m,
                    casts_shadow: true,
                    tint: [1.0; 3],
                });
            }
        }
        let mut additive: Vec<ParticleInstance> = Vec::new();
        let mut alpha_p: Vec<ParticleInstance> = Vec::new();
        if !garage {
            if let (Session::Play(Mode::TimeTrial), Some(p)) = (self.session, self.race.player) {
                let lap_t = (self.race.time - self.race.racers[p].laps.lap_start()) as f32;
                if let Some((gp, gr)) = self.best_ghost.ghost().and_then(|g| g.sample(lap_t)) {
                    draws.push(Draw {
                        mesh: &self.ghost_mesh,
                        model: Mat4::from_rotation_translation(gr, gp),
                        casts_shadow: false,
                        tint: [1.0; 3],
                    });
                }
            }
            // Scenery: frustum-culled; small props dropped far away; shadows only near the focus.
            let frustum = rivet_render::Frustum::from_view_proj(&(view.proj * view.view));
            for p in &self.placements {
                let Some(mesh) = self.prop_gpu[p.kind].as_ref() else {
                    continue;
                };
                let d = p.pos.distance(fpos);
                if (p.size < 7.0 && d > 300.0)
                    || !frustum.sphere_visible(p.pos + Vec3::Y * p.size * 0.4, p.size * 0.7)
                {
                    continue;
                }
                draws.push(Draw {
                    mesh,
                    model: p.model(),
                    casts_shadow: d < 110.0 + p.size,
                    tint: [1.0; 3],
                });
            }

            // Items and weapons.
            let spin = Mat4::from_rotation_y(self.attract_t * 1.6)
                * Mat4::from_rotation_x(self.attract_t * 0.9);
            for b in self
                .race
                .combat
                .boxes
                .iter()
                .filter(|b| b.active() && self.race.combat_on)
            {
                let bob = Vec3::Y * (self.attract_t * 2.0 + b.pos.x).sin() * 0.15;
                let at = Mat4::from_translation(b.pos + bob);
                draws.push(Draw {
                    mesh: &self.fx_meshes.core,
                    model: at * spin,
                    casts_shadow: false,
                    tint: [1.0; 3],
                });
                draws.push(Draw {
                    mesh: &self.fx_meshes.shell,
                    model: at * Mat4::from_rotation_y(-self.attract_t),
                    casts_shadow: true,
                    tint: [1.0; 3],
                });
            }
            for n in &self.race.combat.nails {
                let dir = n.vel.normalize_or(Vec3::Z);
                let m = Mat4::from_rotation_translation(
                    glam::Quat::from_rotation_arc(Vec3::Z, dir),
                    n.pos,
                );
                draws.push(Draw {
                    mesh: &self.fx_meshes.nail,
                    model: m,
                    casts_shadow: false,
                    tint: [1.0; 3],
                });
            }
            for (from, to) in self.race.combat.chains(&self.race.racers) {
                // The cable arcs over the track in short links instead of one straight bar.
                let pts = scrapburn::combat::cable_points(from, to, 10);
                for w in pts.windows(2) {
                    let d = w[1] - w[0];
                    let len = d.length().max(0.01);
                    let rot = glam::Quat::from_rotation_arc(Vec3::Z, d / len);
                    draws.push(Draw {
                        mesh: &self.fx_meshes.chain,
                        model: Mat4::from_scale_rotation_translation(
                            Vec3::new(1.0, 1.0, len),
                            rot,
                            w[0] + d * 0.5,
                        ),
                        casts_shadow: false,
                        tint: [1.0; 3],
                    });
                }
                let n = pts.len();
                let head = (pts[n - 1] - pts[n - 2]).normalize_or(Vec3::Z);
                draws.push(Draw {
                    mesh: &self.fx_meshes.hook,
                    model: Mat4::from_rotation_translation(
                        glam::Quat::from_rotation_arc(Vec3::Z, head),
                        to,
                    ),
                    casts_shadow: true,
                    tint: [1.0; 3],
                });
            }
            for orb in &self.race.combat.orbs {
                let pulse = 1.0 + (self.attract_t * 9.0).sin() * 0.08;
                draws.push(Draw {
                    mesh: &self.fx_meshes.orb,
                    model: Mat4::from_scale_rotation_translation(
                        Vec3::splat(0.9 * pulse),
                        glam::Quat::IDENTITY,
                        orb.pos,
                    ),
                    casts_shadow: false,
                    tint: [1.0; 3],
                });
                draws.push(Draw {
                    mesh: &self.fx_meshes.orb_glow,
                    model: Mat4::from_scale_rotation_translation(
                        Vec3::splat(1.5 / pulse),
                        glam::Quat::from_rotation_y(self.attract_t * 3.0),
                        orb.pos,
                    ),
                    casts_shadow: false,
                    tint: [1.0; 3],
                });
                // A trail of green motes behind the orb (only while the race runs).
                if fx_dt > 0.0 {
                    self.combat_fx.spawn(Particle {
                        pos: orb.pos,
                        vel: Vec3::Y * 1.5,
                        age: 0.0,
                        life: 0.6,
                        size: (0.9, 0.1),
                        color: ([1.5, 5.0, 2.0, 1.0], [0.2, 1.0, 0.4, 0.0]),
                        gravity: 0.0,
                        drag: 1.0,
                        blend: Blend::Additive,
                    });
                }
            }
            self.combat_fx.update(fx_dt);
            // Live fire on torches and braziers near the camera.
            self.flame_acc += fx_dt * 28.0;
            let per = self.flame_acc.floor() as u32;
            self.flame_acc -= per as f32;
            if per > 0 {
                let mut rng = Lcg((self.attract_t * 977.0) as u32 | 1);
                for &f in &self.flame_points {
                    if f.distance_squared(eye) > 90.0 * 90.0 {
                        continue;
                    }
                    for _ in 0..per {
                        let jitter = Vec3::new(rng.next() - 0.5, 0.0, rng.next() - 0.5) * 0.22;
                        self.flame_fx.spawn(Particle {
                            pos: f + jitter,
                            vel: Vec3::new(
                                (rng.next() - 0.5) * 0.5,
                                1.3 + rng.next() * 1.1,
                                (rng.next() - 0.5) * 0.5,
                            ),
                            age: 0.0,
                            life: 0.3 + rng.next() * 0.3,
                            size: (0.45, 0.05),
                            color: ([5.0, 1.8, 0.4, 1.0], [1.2, 0.2, 0.04, 0.0]),
                            gravity: -1.0,
                            drag: 0.6,
                            blend: Blend::Additive,
                        });
                    }
                }
            }
            self.flame_fx.update(fx_dt);
            for sys in [&mut self.combat_fx, &mut self.flame_fx] {
                let (a, b) = sys.instances();
                additive.extend_from_slice(a);
                alpha_p.extend_from_slice(b);
            }
            for fx in &mut self.effects {
                let (a, b) = fx.particles.instances();
                additive.extend_from_slice(a);
                alpha_p.extend_from_slice(b);
            }
        }
        let content = FrameContent {
            draws: &draws,
            additive: &additive,
            alpha: &alpha_p,
        };
        let track_lighting = self.scene.lighting;
        if garage {
            self.scene.lighting = crate::garage::lighting();
        }
        self.scene.render(
            &self.ctx.device,
            &self.ctx.queue,
            &mut enc,
            &frame.view,
            &view,
            &content,
            self.timer.as_mut(),
        );
        self.scene.lighting = track_lighting;
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("hud"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &frame.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.overlay.draw(&mut pass);
        }
        if let Some(t) = self.timer.as_mut() {
            t.resolve(&mut enc);
        }
        self.ctx.submit(enc.finish());
        if let Some(t) = self.timer.as_mut() {
            t.after_submit();
        }
        self.ctx.present(frame);
    }

    fn draw_hud(
        &mut self,
        w: f32,
        h: f32,
        focus: usize,
        view_proj: Mat4,
        models: &[(Vec3, glam::Quat, Mat4)],
    ) {
        let race = &self.race;
        let r = &race.racers[focus];
        let o = &mut self.overlay;
        if matches!(self.session, Session::Attract) {
            let items = self.menu.items(&self.settings);
            self.menu_rows = Some((
                self.menu.screen,
                hud::draw_menu(o, w, h, &self.menu, &items, self.attract_t),
            ));
            match self.menu.screen {
                Screen::Garage => hud::garage_card(o, w, h, self.settings.loadout),
                Screen::Controls => hud::controls_card(o, w, h, self.menu.gamepad_guide),
                _ => {}
            }
            let n_tracks = scrapburn::track::TRACKS.len();
            match (&self.net, self.menu.screen) {
                (_, Screen::Online) => {
                    let lines = [
                        format!("YOUR NAME  {}", self.player_name),
                        if self.tunnel.is_some() {
                            "JOINING WITH YOUR INVITE. ESC TO CANCEL"
                        } else {
                            "PRIVATE RACE INVITES WORK ACROSS NETWORKS"
                        }
                        .to_string(),
                        format!("UDP PORT {}", netplay::DEFAULT_PORT),
                        if self.menu.peers.is_empty() {
                            "NO TAILSCALE PEERS FOUND".to_string()
                        } else {
                            format!("{} TAILSCALE PEERS ONLINE", self.menu.peers.len())
                        },
                    ];
                    hud::draw_lobby(o, w, h, "ONLINE", &lines, &[]);
                }
                (Net::Host(host), Screen::Lobby) => {
                    let def = &scrapburn::track::TRACKS[self.settings.track % n_tracks];
                    let lines = [
                        if let Some(tunnel) = &self.tunnel {
                            if tunnel.ready {
                                "PASTE YOUR COPIED INVITE TO FRIENDS"
                            } else {
                                "CREATING YOUR PRIVATE INVITE. ESC TO CANCEL"
                            }
                            .into()
                        } else if host.is_loopback() {
                            "PRIVATE INVITE IN THE HOST TERMINAL".into()
                        } else {
                            format!("HOSTING ON PORT {}", host.port())
                        },
                        if self.menu.event == Event::Circuit {
                            format!("CIRCUIT  ALL {} TRACKS", scrapburn::circuit::ROUNDS)
                        } else {
                            def.name.to_string()
                        },
                        format!(
                            "{} RACERS ON THE GRID    {} BOTS",
                            host.racer_count(),
                            host.racer_count() - host.player_count()
                        ),
                    ];
                    hud::draw_lobby(o, w, h, "STARTING GRID", &lines, &host.names());
                }
                (Net::Client { link, lobby, .. }, Screen::Lobby) => {
                    let (lines, names) = match lobby {
                        Some(l) => (
                            vec![
                                format!("JOINED {}", link.server()),
                                if l.setup.circuit.is_some() {
                                    format!("CIRCUIT  ALL {} TRACKS", scrapburn::circuit::ROUNDS)
                                } else {
                                    scrapburn::track::TRACKS[l.setup.track as usize % n_tracks]
                                        .name
                                        .to_string()
                                },
                                if l.setup.elimination {
                                    "ELIMINATION"
                                } else {
                                    "RACE"
                                }
                                .to_string(),
                            ],
                            l.names.clone(),
                        ),
                        None => (vec![format!("CONNECTING TO {}", link.server())], Vec::new()),
                    };
                    hud::draw_lobby(o, w, h, "STARTING GRID", &lines, &names);
                }
                _ => {}
            }
            if matches!(self.menu.screen, Screen::Circuits | Screen::RaceSetup) {
                let preview =
                    if self.menu.screen == Screen::Circuits && self.menu.selected < TRACKS.len() {
                        self.menu.selected
                    } else {
                        self.settings.track
                    };
                hud::draw_track_card(
                    o,
                    w,
                    h,
                    preview,
                    &TRACKS[preview],
                    &self.route_previews[preview],
                    self.attract_t,
                );
            }
        } else {
            let in_race = matches!(self.session, Session::Play(Mode::Race | Mode::Elimination));
            let info = HudInfo {
                lap: r.laps.laps() + 1,
                total_laps: (in_race && !race.elimination).then_some(race.laps_to_win),
                lap_time: race.time - r.laps.lap_start(),
                last: r.last_lap,
                best: r.best_lap.or(self
                    .best_ghost
                    .ghost()
                    .map(|g| g.lap_time)
                    .filter(|_| !in_race)),
                ghost: !in_race && self.best_ghost.ghost().is_some(),
            };
            hud::draw(o, w, h, &r.state, &r.params, &info);
            let standings = race.standings(&self.track);
            let countdown = match race.phase {
                Phase::Countdown(left) => Some(left),
                Phase::Racing if race.time < 1.0 => Some(-(race.time as f32)),
                Phase::Racing => None,
            };
            let results = (self.menu.screen == Screen::Results).then(|| {
                standings
                    .iter()
                    .map(|&i| {
                        (
                            race.racers[i].name.as_ref(),
                            race.racers[i].tint,
                            race.racers[i].finish.map(|f| f.1),
                        )
                    })
                    .collect()
            });
            let race_hud = RaceHud {
                position: standings
                    .iter()
                    .position(|&i| i == focus)
                    .map_or(1, |p| p + 1),
                racers: race.racers.len(),
                countdown,
                finished: r.finish.map(|(p, _)| p),
                results,
                circuit: self.circuit.map(|c| {
                    let totals = c.totals(&standings);
                    CircuitHud {
                        round: c.round as usize + 1,
                        rounds: scrapburn::circuit::ROUNDS,
                        table: scrapburn::circuit::table(&totals, &standings, race.racers.len())
                            .into_iter()
                            .map(|s| {
                                (
                                    race.racers[s].name.as_ref(),
                                    race.racers[s].tint,
                                    totals[s] - c.points[s],
                                    totals[s],
                                )
                            })
                            .collect(),
                    }
                }),
                track: &self.track.centerline,
                karts: race
                    .racers
                    .iter()
                    .enumerate()
                    .filter(|(_, k)| !k.out)
                    .map(|(i, k)| (k.state.position, k.tint, i == focus))
                    .collect(),
            };
            hud::draw_race(o, w, h, &race_hud);
            if self.menu.screen == Screen::Playing && !matches!(self.net, Net::Off) {
                let humans = standings
                    .iter()
                    .enumerate()
                    .filter_map(|(position, &seat)| {
                        let kart = &race.racers[seat];
                        let human = match &self.net {
                            Net::Client {
                                view: Some(view), ..
                            } => view.human_seats & (1 << seat) != 0,
                            _ => seat == focus || kart.remote,
                        };
                        human.then_some((position + 1, seat, kart))
                    });
                for (row, (position, seat, kart)) in humans.enumerate() {
                    hud::human_row(o, h, row, position, seat, kart, seat == focus);
                    if seat != focus && !kart.out {
                        let clip = view_proj * (models[seat].0 + Vec3::Y * 1.22).extend(1.0);
                        hud::driver_tag(o, (w, h), clip, seat, &kart.name);
                    }
                }
            }
            if race.phase == Phase::Racing && r.finish.is_none() && race.wrong_way(focus) {
                hud::draw_wrong_way(o, w, h, self.attract_t);
            }
            if race.elimination && race.phase == Phase::Racing {
                let left = race.racers.iter().filter(|k| !k.out).count();
                let last_active = standings
                    .iter()
                    .rev()
                    .find(|&&i| !race.racers[i].out)
                    .copied();
                let danger = last_active == Some(focus) && r.finish.is_none() && left > 1;
                let knockout = self.knockout.map(|(k, t)| {
                    (
                        race.racers[k].name.as_ref(),
                        (race.time - t) as f32,
                        k == focus,
                    )
                });
                hud::draw_elimination(o, w, h, left, danger, knockout, self.attract_t);
            }
            if race.combat_on {
                hud::draw_item_slot(o, w, h, race.combat.held[focus], self.attract_t);
            }
            let items = self.menu.items(&self.settings);
            match self.menu.screen {
                Screen::Paused | Screen::Settings | Screen::Controls | Screen::Results => {
                    if self.menu.screen != Screen::Results {
                        hud::dim(o, w, h);
                    }
                    self.menu_rows = Some((
                        self.menu.screen,
                        hud::draw_menu(o, w, h, &self.menu, &items, self.attract_t),
                    ));
                    if self.menu.screen == Screen::Controls {
                        hud::controls_card(o, w, h, self.menu.gamepad_guide);
                    }
                }
                Screen::Garage
                | Screen::RaceSetup
                | Screen::Circuits
                | Screen::DriverName
                | Screen::Title
                | Screen::Playing
                | Screen::Online
                | Screen::Tailscale
                | Screen::Lobby => {}
            }
        }
        match (self.settings.fps, self.bench.is_some()) {
            (FpsDisplay::Graph, _) | (_, true) => {
                draw_frame_graph(
                    o,
                    &self.stats,
                    &self.ctx.adapter_info.name,
                    w - 270.0,
                    if self.menu.screen == Screen::Playing {
                        200.0
                    } else {
                        14.0
                    },
                );
            }
            (FpsDisplay::Counter, false) => {
                hud::draw_fps_counter(o, w, h, self.stats.summary().fps)
            }
            (FpsDisplay::Off, false) => {}
        }
        if let Some((msg, at)) = &self.toast {
            hud::draw_toast(o, w, h, msg, self.attract_t - at);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.game.is_some() {
            return;
        }
        match Game::new(event_loop, &self.args) {
            Ok(g) => self.game = Some(g),
            Err(e) => {
                eprintln!("error: {e:#}");
                self.exit = ExitCode::from(2);
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                game.minimized = size.width == 0 || size.height == 0;
                game.ctx.resize(size.width, size.height);
                game.scene.resize(&game.ctx.device, game.ctx.size());
            }
            WindowEvent::Focused(false) => {
                game.keys.clear();
                if game.menu.screen == Screen::Playing && game.bench.is_none() {
                    game.menu.go(Screen::Paused);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (position.x as f32, position.y as f32);
                game.pointer = Some((x, y));
                if let Some((screen, rows)) = &game.menu_rows
                    && *screen == game.menu.screen
                    && let Some(row) = rows.hit(x, y)
                {
                    game.menu.selected = row;
                }
            }
            WindowEvent::MouseWheel { delta, .. } if game.menu.screen != Screen::Playing => {
                let dy = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, v) => v,
                    winit::event::MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
                };
                let last = game.menu.items(&game.settings).len().saturating_sub(1);
                if dy < 0. {
                    game.menu.selected = (game.menu.selected + 1).min(last);
                }
                if dy > 0. {
                    game.menu.selected = game.menu.selected.saturating_sub(1);
                }
            }
            WindowEvent::CursorLeft { .. } => game.pointer = None,
            WindowEvent::MouseInput {
                state,
                button: winit::event::MouseButton::Left,
                ..
            } if state.is_pressed() => {
                if game.menu.screen == Screen::Garage
                    && let Some((x, y)) = game.pointer
                {
                    let (w, h) = game.ctx.size();
                    if let Some(finish) = hud::garage_swatch_at(w as f32, h as f32, x, y) {
                        game.settings.loadout.finish = finish;
                        game.menu.selected = 1;
                    }
                }
                if let Some((screen, rows)) = &game.menu_rows
                    && *screen == game.menu.screen
                    && let Some((x, y)) = game.pointer
                    && let Some(row) = rows.hit(x, y)
                {
                    game.clicked = Some((*screen, row, rows.previous_at(row, x)));
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    game.keys.on_key(code, event.state.is_pressed());
                    // Route printable keys to the active text field.
                    if event.state.is_pressed() {
                        if code == winit::keyboard::KeyCode::Backspace {
                            game.menu.backspace();
                        } else if let Some(text) = event.text.as_ref() {
                            game.menu.type_text(text);
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(code) = game.frame(self.launched.elapsed().as_secs_f64()) {
                    self.exit = code;
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(game) = self.game.as_ref() {
            game.window.request_redraw();
        }
    }
}
