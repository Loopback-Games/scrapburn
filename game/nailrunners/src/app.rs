//! Window, screens (title, settings, race, pause, results), main loop, and frame rendering.

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Context as _;
use glam::{Mat4, Vec3};
use nailrunners::args::{Args, Mode};
use nailrunners::bench::{Bench, BenchPhase};
use nailrunners::camera::ChaseCamera;
use nailrunners::controls::{SteerRamp, actions, drive_input, menu_input};
use nailrunners::effects::Effects;
use nailrunners::ghost::{BestGhost, GhostRecorder};
use nailrunners::hud::{self, HudInfo, RaceHud};
use nailrunners::materials::{find_assets_dir, load_layers, shading};
use nailrunners::menu::{MenuAction, MenuState, Screen, Settings};
use nailrunners::race::{Phase, Race, RaceEvent};
use nailrunners::track::{Track, build_graybox_loop};
use nailrunners::{MAX_STEPS, TICK_DT};
use rivet_assets::Rgba8Image;
use rivet_core::{FixedTimestep, FrameStats};
use rivet_physics::{DriveInput, VehicleState};
use rivet_platform::{Gamepads, KeyInput, PadButtons};
use rivet_render::{
    Draw, FrameContent, GpuContext, GpuMesh, GpuTimer, MeshData, Overlay, ParticleInstance,
    SceneRenderer, View, box_mesh, draw_frame_graph,
};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowId};

/// Identifies this track's saved data (ghosts).
const TRACK_ID: &str = "dustbowl-loop";
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

struct Game {
    window: Arc<Window>,
    ctx: GpuContext,
    scene: SceneRenderer,
    layers: Vec<Vec<Rgba8Image>>,
    overlay: Overlay,
    timer: Option<GpuTimer>,
    track: Track,
    track_meshes: [GpuMesh; 3],
    kart_mesh: GpuMesh,
    ghost_mesh: GpuMesh,
    race: Race,
    session: Session,
    menu: MenuState,
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
            .with_title("Nailrunners")
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
        let (kart_data, kart_tex) = match nailrunners::kart::load(&assets, "scrapper") {
            Ok((mesh, tex)) => (mesh, tex),
            Err(e) => {
                log::warn!("kart model unavailable ({e}); using the placeholder kart");
                (placeholder_kart(), None)
            }
        };
        let size = (layers[0][0].width, layers[0][0].height);
        let kart_layer = kart_tex.map_or_else(
            || layers[0][0].clone(),
            |t| rivet_assets::resized(&t, size.0, size.1),
        );
        layers.push(rivet_assets::mip_chain(&kart_layer));
        let settings = Settings {
            preset: args.preset,
            ..Settings::default()
        };
        let scene = SceneRenderer::new(
            &ctx.device,
            &ctx.queue,
            ctx.format(),
            ctx.size(),
            settings.preset,
            &layers,
            &shading(),
        );
        let overlay = Overlay::new(&ctx.device, ctx.format(), None);
        let timer = GpuTimer::new(&ctx.device, &ctx.queue);
        let track = build_graybox_loop();
        let track_meshes =
            [&track.road, &track.terrain, &track.props].map(|m| GpuMesh::new(&ctx.device, m));
        let kart_mesh = GpuMesh::new(&ctx.device, &kart_data);
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
        let ghost_file =
            nailrunners::ghost::data_dir().map(|d| nailrunners::ghost::ghost_path(&d, TRACK_ID));
        let best_ghost =
            BestGhost::from_ghost(ghost_file.as_deref().and_then(nailrunners::ghost::load));

        // Benchmarks skip the menus and race bots; otherwise start on the title screen.
        let (session, screen) = if args.bench.is_some() && !args.attract {
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
            overlay,
            timer,
            track,
            track_meshes,
            kart_mesh,
            ghost_mesh,
            race,
            session,
            menu: MenuState::new(screen),
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
        })
    }

    fn make_race(track: &Track, session: Session, s: &Settings, bench: bool) -> Race {
        match session {
            Session::Attract => Race::new(track, 8, false, u32::MAX),
            Session::Play(Mode::Race) => Race::new(track, s.rivals + 1, !bench, s.laps),
            Session::Play(Mode::TimeTrial) => Race::new(track, 1, !bench, u32::MAX),
        }
    }

    fn start(&mut self, session: Session) {
        self.session = session;
        self.race = Self::make_race(&self.track, session, &self.settings, self.bench.is_some());
        let n = self.race.racers.len();
        self.effects = (0..n).map(|_| Effects::new()).collect();
        self.leans = vec![0.0; n];
        self.recorder = GhostRecorder::default();
        self.camera.reset();
        self.menu = MenuState::new(if matches!(session, Session::Attract) {
            Screen::Title
        } else {
            Screen::Playing
        });
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

    fn rebuild_renderer(&mut self) {
        self.scene = SceneRenderer::new(
            &self.ctx.device,
            &self.ctx.queue,
            self.ctx.format(),
            self.ctx.size(),
            self.settings.preset,
            &self.layers,
            &shading(),
        );
    }

    /// Returns `Some(exit code)` when the app should quit.
    fn frame(&mut self, since_launch: f64) -> Option<ExitCode> {
        let now = Instant::now();
        let frame_dt = now.duration_since(self.last_frame).as_secs_f64();
        self.last_frame = now;
        if let Some(p) = self.pads.as_mut() {
            p.poll();
        }
        let pad = self.pads.as_ref().and_then(Gamepads::active).copied();
        let pad = pad.as_ref();
        let act = actions(&self.keys, pad);
        if act.toggle_graph {
            self.settings.show_fps = !self.settings.show_fps;
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
                if act.respawn
                    && let Some(p) = self.race.player
                {
                    self.race.respawn_racer(p, &self.track);
                }
            }
            Screen::Title | Screen::Settings | Screen::Paused | Screen::Results => {
                let paused_game =
                    matches!(self.session, Session::Play(_)) && self.menu.screen != Screen::Results;
                simulate = !paused_game;
                let input = menu_input(&self.keys, pad);
                match self.menu.handle(input, &mut self.settings) {
                    MenuAction::None => {}
                    MenuAction::StartRace => self.start(Session::Play(Mode::Race)),
                    MenuAction::StartTimeTrial => self.start(Session::Play(Mode::TimeTrial)),
                    MenuAction::Resume => self.menu.go(Screen::Playing),
                    MenuAction::Restart => self.start(self.session),
                    MenuAction::ToTitle => self.start(Session::Attract),
                    MenuAction::Quit => return Some(ExitCode::SUCCESS),
                    MenuAction::PresetChanged => self.rebuild_renderer(),
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
        let steps = self.clock.advance(frame_dt);
        if simulate {
            for _ in 0..steps {
                let events = self.race.step(&self.track, input, TICK_DT as f32);
                self.after_step(&events);
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
                        match std::fs::write(path, nailrunners::png::encode_rgba(w, h, &rgba)) {
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
                    match nailrunners::ghost::save(path, g) {
                        Ok(()) => {
                            log::info!("new best lap {time:.3} s saved to {}", path.display())
                        }
                        Err(e) => log::warn!("saving the ghost failed: {e}"),
                    }
                }
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
            models.push((
                pos,
                rot,
                Mat4::from_rotation_translation(rot, pos) * Mat4::from_rotation_z(self.leans[i]),
            ));
        }
        let (fpos, frot, _) = models[focus];
        let fstate = &self.race.racers[focus].state;
        self.camera.set_boost(fstate.boost_timer > 0.0);
        self.camera.update(fpos, frot, fstate.speed(), frame_dt);
        let (w, h) = self.ctx.size();
        let (view_m, eye) = if matches!(self.session, Session::Attract) {
            // Cinematic orbit around the featured kart.
            let a = self.attract_t * 0.22;
            let eye = fpos + Vec3::new(a.cos() * 11.0, 3.2 + (a * 0.7).sin() * 1.5, a.sin() * 11.0);
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
        self.draw_hud(w as f32, h as f32, focus);
        self.overlay
            .prepare(&self.ctx.device, &self.ctx.queue, w, h);

        let Some(frame) = self.ctx.begin_frame() else {
            return;
        };
        let mut enc = self.ctx.device.create_command_encoder(&Default::default());
        let mut draws: Vec<Draw> = self
            .track_meshes
            .iter()
            .map(|m| Draw::new(m, Mat4::IDENTITY))
            .collect();
        for (i, (_, _, model)) in models.iter().enumerate() {
            draws.push(Draw {
                mesh: &self.kart_mesh,
                model: *model,
                casts_shadow: true,
                tint: self.race.racers[i].tint,
            });
        }
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
        let mut additive: Vec<ParticleInstance> = Vec::new();
        let mut alpha_p: Vec<ParticleInstance> = Vec::new();
        for fx in &mut self.effects {
            let (a, b) = fx.particles.instances();
            additive.extend_from_slice(a);
            alpha_p.extend_from_slice(b);
        }
        let content = FrameContent {
            draws: &draws,
            additive: &additive,
            alpha: &alpha_p,
        };
        self.scene.render(
            &self.ctx.device,
            &self.ctx.queue,
            &mut enc,
            &frame.view,
            &view,
            &content,
            self.timer.as_mut(),
        );
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

    fn draw_hud(&mut self, w: f32, h: f32, focus: usize) {
        let race = &self.race;
        let r = &race.racers[focus];
        let o = &mut self.overlay;
        if matches!(self.session, Session::Attract) {
            let items = self.menu.items(&self.settings);
            let on_title = self.menu.screen == Screen::Title;
            let title = on_title.then_some("NAILRUNNERS");
            let sub = if on_title {
                "COMBAT KART RACING IN THE WASTES"
            } else {
                "SETTINGS"
            };
            hud::draw_menu(o, w, h, title, sub, &items, self.menu.selected);
        } else {
            let in_race = matches!(self.session, Session::Play(Mode::Race));
            let info = HudInfo {
                lap: r.laps.laps() + 1,
                total_laps: in_race.then_some(race.laps_to_win),
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
                            race.racers[i].name,
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
                track: &self.track.centerline,
                karts: race
                    .racers
                    .iter()
                    .enumerate()
                    .map(|(i, k)| (k.state.position, k.tint, i == focus))
                    .collect(),
            };
            hud::draw_race(o, w, h, &race_hud);
            let items = self.menu.items(&self.settings);
            match self.menu.screen {
                Screen::Paused => {
                    hud::dim(o, w, h);
                    hud::draw_menu(o, w, h, None, "PAUSED", &items, self.menu.selected);
                }
                Screen::Settings => {
                    hud::dim(o, w, h);
                    hud::draw_menu(o, w, h, None, "SETTINGS", &items, self.menu.selected);
                }
                Screen::Results => {
                    hud::draw_menu(o, w, h * 1.6, None, "", &items, self.menu.selected)
                }
                Screen::Title | Screen::Playing => {}
            }
        }
        if self.settings.show_fps || self.bench.is_some() {
            draw_frame_graph(o, &self.stats, &self.ctx.adapter_info.name, w - 270.0, 80.0);
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
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    game.keys.on_key(code, event.state.is_pressed());
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
