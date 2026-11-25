//! Window, main loop, and frame rendering.

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Context as _;
use glam::{Mat4, Vec3};
use nailrunners::args::Args;
use nailrunners::autopilot::autopilot;
use nailrunners::bench::{Bench, BenchPhase};
use nailrunners::camera::ChaseCamera;
use nailrunners::controls::{HopLatch, SteerRamp, actions, drive_input};
use nailrunners::laps::{LapEvent, LapTracker};
use nailrunners::track::{Track, build_graybox_loop};
use nailrunners::{MAX_STEPS, TICK_DT};
use rivet_core::{FixedTimestep, FrameStats};
use rivet_physics::{DriveInput, VehicleParams, VehicleState, step};
use rivet_platform::{Gamepads, KeyInput};
use rivet_render::{
    CameraUniform, DEPTH_FORMAT, GpuContext, GpuMesh, GpuTimer, MeshData, MeshPipeline, Overlay,
    box_mesh, draw_frame_graph,
};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowId};

/// Below this height the kart is considered lost and respawns.
const FALL_LIMIT_Y: f32 = -30.0;
const SPAWN_LIFT: f32 = 0.6;

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

struct Game {
    window: Arc<Window>,
    ctx: GpuContext,
    meshes: MeshPipeline,
    overlay: Overlay,
    timer: Option<GpuTimer>,
    track_mesh: GpuMesh,
    kart_mesh: GpuMesh,
    track: Track,
    params: VehicleParams,
    prev: VehicleState,
    cur: VehicleState,
    clock: FixedTimestep,
    sim_time: f64,
    laps: LapTracker,
    last_lap: Option<f64>,
    best_lap: Option<f64>,
    camera: ChaseCamera,
    keys: KeyInput,
    pads: Option<Gamepads>,
    stats: FrameStats,
    show_graph: bool,
    last_frame: Instant,
    minimized: bool,
    bench: Option<Bench>,
    hop: HopLatch,
    steer: SteerRamp,
    screenshot: Option<std::path::PathBuf>,
}

fn kart_mesh() -> MeshData {
    let mut m = MeshData::default();
    let mut part = |half: Vec3, offset: Vec3, color: [f32; 4]| {
        let mut b = box_mesh(half, color);
        for v in &mut b.vertices {
            v.pos = (Vec3::from_array(v.pos) + offset).to_array();
        }
        m.append(&b);
    };
    // Body 1.4 × 0.6 × 2.0 m, cockpit, and four wheels.
    part(
        Vec3::new(0.7, 0.3, 1.0),
        Vec3::new(0.0, 0.0, 0.0),
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

fn fmt_time(t: f64) -> String {
    let m = (t / 60.0).floor();
    format!("{}:{:06.3}", m as u32, t - m * 60.0)
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
        let meshes = MeshPipeline::new(&ctx.device, ctx.format(), DEPTH_FORMAT);
        let overlay = Overlay::new(&ctx.device, ctx.format(), DEPTH_FORMAT);
        let timer = GpuTimer::new(&ctx.device, &ctx.queue);
        let track = build_graybox_loop();
        let track_mesh = GpuMesh::new(&ctx.device, &track.render);
        let kart_mesh = GpuMesh::new(&ctx.device, &kart_mesh());
        let spawn = VehicleState::spawn(track.spawn.0 + Vec3::Y * SPAWN_LIFT, track.spawn.1);
        let laps = LapTracker::new(track.checkpoints.len());
        Ok(Self {
            window,
            ctx,
            meshes,
            overlay,
            timer,
            track_mesh,
            kart_mesh,
            track,
            params: VehicleParams::default(),
            prev: spawn.clone(),
            cur: spawn,
            clock: FixedTimestep::new(TICK_DT, MAX_STEPS),
            sim_time: 0.0,
            laps,
            last_lap: None,
            best_lap: None,
            camera: ChaseCamera::default(),
            keys: KeyInput::default(),
            pads: Gamepads::new(),
            stats: FrameStats::new(240),
            show_graph: true,
            last_frame: Instant::now(),
            minimized: false,
            bench: args.bench.map(Bench::new),
            hop: HopLatch::default(),
            steer: SteerRamp::default(),
            screenshot: args.screenshot.clone(),
        })
    }

    fn respawn(&mut self) {
        let cp = self.track.checkpoints[self.laps.last_checkpoint()];
        let yaw = cp.forward.x.atan2(cp.forward.z);
        self.cur = VehicleState::spawn(cp.center + Vec3::Y * SPAWN_LIFT, yaw);
        self.prev = self.cur.clone();
        self.camera.reset();
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
        if act.quit {
            return Some(ExitCode::SUCCESS);
        }
        if act.toggle_graph {
            self.show_graph = !self.show_graph;
        }
        if act.respawn {
            self.respawn();
        }
        let input = if self.bench.is_some() {
            autopilot(&self.cur, &self.track.centerline)
        } else {
            drive_input(&self.keys, pad)
        };
        self.keys.end_frame();
        self.hop.press(input.hop);
        let digital = self.bench.is_none() && pad.is_none_or(|p| p.left_x.abs() < 0.01);
        let input = DriveInput {
            steer: self.steer.apply(input.steer, digital, frame_dt as f32),
            ..input
        };

        // --- Simulation ---
        let steps = self.clock.advance(frame_dt);
        for _ in 0..steps {
            self.prev = self.cur.clone();
            let tick_input = DriveInput {
                hop: self.hop.take(),
                ..input
            };
            step(
                &mut self.cur,
                &self.params,
                tick_input,
                &self.track.collision,
                TICK_DT as f32,
            );
            self.sim_time += TICK_DT;
            if let Some(LapEvent::Lap { time, .. }) = self.laps.update(
                self.prev.position,
                self.cur.position,
                &self.track.checkpoints,
                self.sim_time,
            ) {
                self.last_lap = Some(time);
                self.best_lap = Some(self.best_lap.map_or(time, |b| b.min(time)));
            }
            if self.cur.position.y < FALL_LIMIT_Y {
                self.respawn();
            }
        }

        // --- Stats ---
        let cpu_ms = (frame_dt * 1000.0) as f32;
        let gpu_ms = self
            .timer
            .as_mut()
            .and_then(|t| t.poll_ms(&self.ctx.device));
        self.stats.push(cpu_ms, gpu_ms);
        if let Some(b) = self.bench.as_mut() {
            match b.record(since_launch, cpu_ms, gpu_ms) {
                BenchPhase::Done => {
                    let info = &self.ctx.adapter_info;
                    let line = b.line(
                        &info.name,
                        &format!("{:?}", info.backend),
                        self.ctx.size(),
                        &self.ctx.present_label(),
                    );
                    println!("{line}");
                    if let Some(path) = &self.screenshot {
                        match self.ctx.capture_offscreen() {
                            Some((w, h, rgba)) => match std::fs::write(
                                path,
                                nailrunners::png::encode_rgba(w, h, &rgba),
                            ) {
                                Ok(()) => println!("BENCH screenshot={}", path.display()),
                                Err(e) => eprintln!("writing screenshot failed: {e}"),
                            },
                            None => eprintln!("screenshot capture failed"),
                        }
                    }
                    match b.write_csv(std::path::Path::new("target/bench")) {
                        Ok(p) => println!("BENCH csv={}", p.display()),
                        Err(e) => eprintln!("writing bench CSV failed: {e}"),
                    }
                    return Some(ExitCode::SUCCESS);
                }
                BenchPhase::Warmup | BenchPhase::Running => {}
            }
        }

        if self.minimized {
            return None;
        }
        self.render(frame_dt as f32);
        None
    }

    fn render(&mut self, frame_dt: f32) {
        let (pos, rot) = VehicleState::lerp(&self.prev, &self.cur, self.clock.alpha());
        self.camera.update(pos, rot, self.cur.speed(), frame_dt);
        let (w, h) = self.ctx.size();
        let vp = self.camera.view_proj(w as f32 / h as f32);
        let cam = CameraUniform::new(vp, self.camera.position());
        let kart =
            Mat4::from_rotation_translation(rot, pos) * Mat4::from_translation(Vec3::Y * 0.05);
        self.meshes.prepare(
            &self.ctx.device,
            &self.ctx.queue,
            &cam,
            &[Mat4::IDENTITY, kart],
        );
        self.hud(w as f32, h as f32);
        self.overlay
            .prepare(&self.ctx.device, &self.ctx.queue, w, h);

        let Some(frame) = self.ctx.begin_frame() else {
            return;
        };
        let mut enc = self.ctx.device.create_command_encoder(&Default::default());
        {
            let fog = cam.fog;
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &frame.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: fog.x as f64,
                            g: fog.y as f64,
                            b: fog.z as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: self.ctx.depth_view(),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: self.timer.as_mut().and_then(GpuTimer::pass_writes),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.meshes
                .draw(&mut pass, &[&self.track_mesh, &self.kart_mesh]);
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

    fn hud(&mut self, w: f32, h: f32) {
        let o = &mut self.overlay;
        if self.show_graph {
            draw_frame_graph(o, &self.stats, &self.ctx.adapter_info.name, 16.0, 16.0);
        }
        let s = 3.0;
        let white = [1.0; 4];
        let kmh = self.cur.speed().abs() * 3.6;
        let y = h - 110.0;
        o.rect(10.0, y - 10.0, 330.0, 104.0, [0.0, 0.0, 0.0, 0.45]);
        o.text(20.0, y, s, white, &format!("{kmh:3.0} KM/H"));
        let lap_time = self.sim_time - self.laps.lap_start();
        o.text(
            20.0,
            y + 30.0,
            s,
            white,
            &format!("LAP {} {}", self.laps.laps() + 1, fmt_time(lap_time)),
        );
        let best = self.best_lap.map_or_else(|| "--".to_string(), fmt_time);
        let last = self.last_lap.map_or_else(|| "--".to_string(), fmt_time);
        o.text(
            20.0,
            y + 60.0,
            2.0,
            white,
            &format!("LAST {last}  BEST {best}"),
        );
        if let Some(d) = self.cur.drift {
            let tier = self
                .params
                .boost_tiers
                .iter()
                .filter(|t| d.charge >= t.charge)
                .count();
            let color = nailrunners::style::tier_color(tier);
            o.text(
                w * 0.5 - 60.0,
                h - 80.0,
                4.0,
                color,
                &format!("DRIFT {tier}"),
            );
        } else if self.cur.boost_timer > 0.0 {
            o.text(
                w * 0.5 - 60.0,
                h - 80.0,
                4.0,
                [1.0, 0.85, 0.2, 1.0],
                "BOOST",
            );
        }
        if self.bench.is_some() {
            o.text(w - 200.0, 16.0, 3.0, [1.0, 0.4, 0.4, 1.0], "BENCH");
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
            }
            WindowEvent::Focused(false) => game.keys.clear(),
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
