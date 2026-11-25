# M0 + M1: Tooling and Drivable Gray-Box Loop — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Rust workspace with mise/just/CI, and a window where a kart drives around a gray-box loop track on the Intel UHD 730 with an on-screen CPU/GPU frame graph and a repeatable benchmark mode.

**Architecture:** Cargo workspace of small engine crates (`rivet-core`, `rivet-platform`, `rivet-render`, `rivet-physics`) plus the game binary `nailrunners`. The sim runs at a fixed 60 Hz tick; rendering interpolates between the last two sim states. Rendering is wgpu (Vulkan on Linux, DX12 on Windows) with a simple forward pipeline; the frame graph is a screen-space quad overlay with a built-in bitmap font.

**Tech Stack:** Rust 1.100.0 (mise), wgpu 30.0.2, winit 0.30.13 (latest stable; not the 0.31 beta), glam 0.34.3, bytemuck 1.25.2, pollster 1.0.3, log 0.4 + env_logger 0.11, anyhow 1.0. `cargo-audit` for `just security`.

**Spec:** `docs/superpowers/specs/2026-11-25-rivet-nailrunners-design.md`

## Global Constraints

- License `MIT OR Apache-2.0` on every crate (`license = "MIT OR Apache-2.0"` in workspace package; `LICENSE-MIT`, `LICENSE-APACHE` at root).
- No Quake 3 (or any id Software) code is copied or translated. Ideas only.
- Engine crates (`crates/rivet-*`) must not depend on `game/` or on each other except: `rivet-platform` may depend on `rivet-core`, and `rivet-render` on `rivet-core` + `rivet-platform`. `rivet-physics` depends only on glam.
- Minimum spec: Intel UHD 730 (this PC also has an RTX 3060; the iGPU must be selectable).
- M1 perf gate on UHD 730, 1600×900, `--present immediate`: average ≥ 60 fps **and** p99 frame time ≤ 16.7 ms over a 30 s bench. Numbers are only claimed from a captured run.
- Sim tick: fixed 60 Hz (`TICK_DT = 1.0 / 60.0`), at most 5 sim steps per rendered frame.
- New dependencies beyond the Tech Stack list go through the dependency-vetting rules (1000+ stars, commit within 6 months, not archived, compatible license).
- GitHub Actions pinned by SHA: `actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1`, `jdx/mise-action@924e7c586e33d7d4bae9c14e26c5d22e2beb6e78 # v5.3.0`.
- Gamepads are **out of M1**: `gilrs` needs libudev headers, which this Bluefin host lacks. Keyboard only; gamepad is a follow-up decision (see end).
- Conventional Commits; American English; commit locally only, never push.

## Review Focus

1. Window minimized / resized to 0×0 → no surface reconfigure with zero size, no panic; rendering resumes on restore.
2. `SurfaceError::Lost`/`Outdated` (alt-tab, monitor change, PRIME) → reconfigure and skip the frame, never panic.
3. A long hitch (debugger pause, window drag, 2 s stall) → sim does at most 5 steps and drops the rest; no spiral of death, the car doesn't teleport through walls.
4. Car leaves the track (flies over wall, falls) → when `y < -30` or the player presses R, respawn at the last passed checkpoint facing track direction with zero velocity.
5. `--adapter intel` matches nothing → exit with an error that lists every adapter's name and type, not a panic or silent fallback.

---

## File Structure

```
rivet/
  Cargo.toml                 workspace, [workspace.package], [workspace.dependencies]
  mise.toml  justfile  rustfmt.toml  .gitignore  LICENSE-MIT  LICENSE-APACHE  README.md
  .github/workflows/ci.yml
  crates/rivet-core/src/{lib.rs, timestep.rs, frame_stats.rs}
  crates/rivet-platform/src/{lib.rs, adapter.rs, input.rs}
  crates/rivet-render/src/{lib.rs, context.rs, gpu_timer.rs, mesh.rs, mesh_pipeline.rs,
                           mesh.wgsl, overlay.rs, overlay.wgsl, font.rs, frame_graph.rs}
  crates/rivet-physics/src/{lib.rs, trimesh.rs, vehicle.rs}
  game/nailrunners/src/{main.rs, app.rs, args.rs, track.rs, laps.rs, camera.rs,
                        controls.rs, bench.rs, autopilot.rs}
  docs/perf/
```

---

### Task 1: Workspace, tooling and CI (M0)

**Files:** Create everything at the root above, plus empty-lib crates for the four `rivet-*` crates and a `nailrunners` binary that prints `nailrunners <version>`.

**Interfaces:** Produces the `just` recipes every later task uses: `setup fmt lint test security build run clean ci`.

- [ ] **Step 1:** `mise.toml`: `[tools] rust = { version = "1.100.0", components = "rustfmt,clippy" }`, `"cargo:cargo-audit" = "latest"` (pin the resolved version after first install), `just = "1.59.0"`.
- [ ] **Step 2:** `justfile` recipes:
  - `setup`: `mise install`
  - `fmt`: `cargo fmt --all`
  - `lint`: `cargo fmt --all -- --check` then `cargo clippy --workspace --all-targets -- -D warnings`
  - `test`: `cargo test --workspace`
  - `security`: `cargo audit`
  - `build`: `cargo build --workspace --release`
  - `run *ARGS`: `cargo run -p nailrunners --release -- {{ARGS}}`
  - `bench *ARGS`: `cargo run -p nailrunners --release -- --bench 30 {{ARGS}}`
  - `clean`: `cargo clean`
  - `ci`: `lint test security build`
- [ ] **Step 3:** `Cargo.toml`: `resolver = "3"`, `edition = "2024"`, `[profile.dev.package."*"] opt-level = 2` (keeps debug builds playable), `[profile.release] debug = "line-tables-only"`.
- [ ] **Step 4:** `ci.yml`: job id **`ci`**, matrix `ubuntu-latest`, `windows-latest`; steps: checkout (pinned), mise-action (pinned), `just ci`. `permissions: contents: read`. Cache `~/.cargo/registry` and `target` with the cache built into mise-action if available, otherwise none (no unvetted third-party cache action).
- [ ] **Step 5:** Run `just setup && just ci`. Expected: fmt/clippy clean, 0 tests run, `cargo audit` reports no vulnerabilities, release build succeeds.
- [ ] **Step 6:** Commit `chore: scaffold Rivet workspace with mise, just and CI`.

---

### Task 2: `rivet-core` — fixed timestep and frame stats

**Files:** `crates/rivet-core/src/{timestep.rs, frame_stats.rs, lib.rs}` (tests inline `#[cfg(test)]`).

**Interfaces — Produces:**
- `pub struct FixedTimestep { pub dt: f64, pub max_steps: u32, .. }`; `FixedTimestep::new(dt: f64, max_steps: u32) -> Self`; `fn advance(&mut self, frame_dt: f64) -> u32` (returns steps to run; drops accumulated time beyond `max_steps`); `fn alpha(&self) -> f32` (interpolation factor in `[0,1)`).
- `pub struct FrameStats` ring buffer of 240 samples: `new(capacity: usize)`, `push(&mut self, cpu_ms: f32, gpu_ms: Option<f32>)`, `samples(&self) -> impl Iterator<Item = FrameSample>`, `summary(&self) -> StatsSummary { avg_ms, p50_ms, p99_ms, max_ms, fps }`, with `FrameSample { cpu_ms: f32, gpu_ms: Option<f32> }`. Percentiles use nearest-rank on frame (CPU wall) time.

- [ ] **Step 1: Failing tests**
  - `advance_accumulates`: dt=1/60; `advance(0.010)` → 0, `advance(0.010)` → 1, `alpha()` ≈ 0.2 (±1e-6 relative to (0.020−1/60)/(1/60)).
  - `advance_clamps_hitch`: `advance(2.0)` → 5 and afterwards `alpha() < 1.0`; next `advance(0.0)` → 0.
  - `stats_percentiles`: push 1.0..=100.0 ms (100 samples) → p50 = 50.0, p99 = 99.0, max = 100.0, avg = 50.5, fps ≈ 1000/50.5.
  - `stats_ring_overwrites`: capacity 4, push 5 values → `samples().count() == 4`, oldest dropped.
- [ ] **Step 2:** `cargo test -p rivet-core` → FAIL (missing items).
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test -p rivet-core` → PASS; `just lint` clean.
- [ ] **Step 5:** Commit `feat(core): add fixed timestep and frame stats`.

---

### Task 3: `rivet-platform` — adapter selection and key input

**Files:** `crates/rivet-platform/src/{adapter.rs, input.rs, lib.rs}`. Depends on `winit` (for `KeyCode`) and `wgpu` (for `DeviceType` and adapter enumeration only).

**Interfaces — Produces:**
- `pub enum AdapterPreference { Auto, Integrated, Discrete, NameContains(String) }`; `impl FromStr` parses `auto|integrated|discrete|<substring>` (case-insensitive). Env var `RIVET_ADAPTER` is the default when no CLI flag is given.
- `pub struct AdapterDesc { pub name: String, pub device_type: wgpu::DeviceType, pub backend: wgpu::Backend }`.
- `pub fn select_adapter(list: &[AdapterDesc], pref: &AdapterPreference) -> Result<usize, AdapterError>`. `Auto` = integrated first (min-spec-first project), then discrete, then anything; `NameContains` is case-insensitive. `AdapterError::NoMatch { wanted: String, available: Vec<AdapterDesc> }` with a `Display` that lists every adapter `name (type, backend)` one per line.
- `pub struct KeyInput`: `on_key(&mut self, code: KeyCode, pressed: bool)`, `held(&self, code) -> bool`, `just_pressed(&self, code) -> bool`, `end_frame(&mut self)` (clears just-pressed). Key repeat events must not re-trigger `just_pressed`.

- [ ] **Step 1: Failing tests**
  - `auto_prefers_integrated`: list [3060 Discrete, "Intel(R) UHD Graphics 730" Integrated, llvmpipe Cpu] → 1.
  - `name_contains_case_insensitive`: `"uhd".parse()` → index 1.
  - `no_match_lists_all`: `NameContains("radeon")` → `Err`, and `to_string()` contains both "RTX 3060" and "UHD Graphics 730".
  - `discrete_falls_back_error`: list with only Integrated and `Discrete` → `Err(NoMatch)` (an explicit request never silently falls back).
  - `just_pressed_once`: press KeyR, `just_pressed` true; `end_frame`; press KeyR again (repeat, still held) → `just_pressed` false; release + press → true.
- [ ] **Step 2:** `cargo test -p rivet-platform` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** PASS, lint clean.
- [ ] **Step 5:** Commit `feat(platform): add adapter selection and key input`.

---

### Task 4: `rivet-physics` — triangle mesh raycasts

**Files:** `crates/rivet-physics/src/{trimesh.rs, lib.rs}`.

**Interfaces — Produces:**
- `pub struct TriMesh` built by `TriMesh::new(positions: Vec<Vec3>, indices: Vec<[u32; 3]>, cell_size: f32) -> Self` with a uniform 2D grid over XZ (each cell lists overlapping triangles).
- `pub struct RayHit { pub t: f32, pub point: Vec3, pub normal: Vec3, pub tri: u32 }`; `fn raycast(&self, origin: Vec3, dir: Vec3 /*normalized*/, max_t: f32) -> Option<RayHit>` → nearest hit, normal faces against `dir`. Walks the grid cells the ray's XZ projection crosses (2D DDA), then Möller–Trumbore on candidates.
- `fn sphere_contacts(&self, center: Vec3, radius: f32, out: &mut Vec<Contact>)` with `Contact { normal: Vec3, depth: f32 }` (used for wall collisions of the car body).

- [ ] **Step 1: Failing tests**
  - `ray_hits_flat_ground`: 100 m square at y=0 (2 tris), ray from (3,5,7) down → t=5, normal (0,1,0).
  - `ray_misses_outside`: ray from (500,5,0) down → None.
  - `nearest_of_two_layers`: planes at y=0 and y=2, ray from y=10 down → t=8.
  - `long_diagonal_ray`: a 1 km plane with cell_size 8, a ray from (−400,1,−400) toward (400,0,400) hits at the analytic point (±1e-3).
  - `sphere_against_wall`: vertical wall at x=0, sphere at x=0.3 radius 0.5 → one contact, normal (±1,0,0) pointing toward the sphere, depth 0.2 (±1e-4).
- [ ] **Step 2:** FAIL. **Step 3:** Implement. **Step 4:** PASS + lint.
- [ ] **Step 5:** Commit `feat(physics): add grid-accelerated triangle mesh queries`.

---

### Task 5: `rivet-physics` — arcade kart vehicle

**Files:** `crates/rivet-physics/src/vehicle.rs`.

**Interfaces — Produces:**
- `pub struct DriveInput { pub throttle: f32 /*0..1*/, pub brake: f32 /*0..1, also reverse*/, pub steer: f32 /*-1..1, +right*/, pub drift: bool, pub hop: bool }` (`Default`, `Copy`).
- `pub struct VehicleParams` with `Default` = medium kart: `mass 300.0`, `max_speed 32.0` m/s, `accel 12.0` m/s², `brake_decel 30.0`, `reverse_max 8.0`, `gravity 25.0`, `ride_height 0.45`, `suspension_rest 0.6`, `spring 60.0` (per unit mass), `damping 8.0`, `grip 10.0` (lateral velocity damping /s), `drift_grip 2.5`, `steer_rate 2.2` rad/s at low speed tapering to `1.1` at max speed, `hop_speed 4.5` m/s, `body_radius 0.8`, boost tiers `[(0.6 s, 4.0 m/s for 0.5 s), (1.4 s, 7.0 m/s for 1.0 s)]`.
- `pub struct VehicleState { pub position: Vec3, pub rotation: Quat, pub velocity: Vec3, pub angular_yaw: f32, pub grounded: bool, pub drift: Option<DriftState>, pub boost_timer: f32, pub boost_speed: f32 }`, `DriftState { dir: f32 /*-1|1*/, charge: f32 }`; `VehicleState::spawn(position: Vec3, yaw: f32) -> Self`; `fn lerp(a: &Self, b: &Self, alpha: f32) -> (Vec3, Quat)` for render interpolation.
- `pub fn step(state: &mut VehicleState, params: &VehicleParams, input: DriveInput, world: &TriMesh, dt: f32)`.

Approach (the parts tests don't pin): four suspension rays down the body's local −Y from corner points (±0.6, 0, ±0.9); spring force on compression; body aligns its up vector toward the average ground normal when grounded (slerp, rate 10/s) and slowly self-rights in air; forward drive along the ground-projected forward; lateral velocity is damped by `grip` (or `drift_grip` while drifting); drift starts when `drift` is held with |steer| > 0.3 and speed > 10 m/s while grounded, locking `dir` to the steer sign and adding 0.5 rad/s of yaw bias; charge accumulates while drifting; releasing grants the highest tier reached; `hop` gives an upward impulse only when grounded; walls via `sphere_contacts` remove the velocity component into the wall and push out by depth (weight: ramming lighter cars comes later). All arithmetic is f32 in a fixed order — no HashMap iteration, no threads.

- [ ] **Step 1: Failing tests** (flat 400 m plane world, dt = 1/60)
  - `settles_at_ride_height`: spawn at y=2, 180 steps no input → `grounded`, |y − ride_height| < 0.05, speed < 0.05.
  - `accelerates_to_max_speed`: full throttle 600 steps → forward speed within 0.5 of `max_speed`; at step 60 speed between 8 and 14.
  - `steers_right`: at 20 m/s, steer +1 for 60 steps → yaw changed clockwise (seen from +Y) by 0.6–2.0 rad, speed > 15.
  - `drift_tier2_boost`: at 25 m/s, drift + steer +1 for 100 steps (1.67 s), then release for 1 step → `boost_speed == 7.0`, `boost_timer ≈ 1.0`; forward speed 10 steps later exceeds `max_speed`.
  - `short_drift_no_boost`: drift for 20 steps (0.33 s) then release → `boost_speed == 0.0`.
  - `hop_only_when_grounded`: grounded hop → upward velocity ≈ `hop_speed`; hop pressed again while airborne → no added velocity.
  - `wall_stops_penetration`: drive at 30 m/s into a wall → body center never closer than `body_radius − 0.05` to the wall plane over 120 steps.
  - `deterministic`: run the same 600-step scripted input twice from the same spawn → states bit-identical (`to_bits` compare on all floats).
- [ ] **Step 2:** FAIL. **Step 3:** Implement. **Step 4:** PASS + lint.
- [ ] **Step 5:** Commit `feat(physics): add arcade kart with drift boost and hop`.

---

### Task 6: `rivet-render` — GPU context, timer and mesh pipeline

**Files:** `crates/rivet-render/src/{context.rs, gpu_timer.rs, mesh.rs, mesh_pipeline.rs, mesh.wgsl, lib.rs}`.

**Interfaces — Produces:**
- `pub enum PresentChoice { Vsync, Immediate }` (`FromStr`: `fifo|vsync`, `immediate`); Immediate falls back to Mailbox, then Fifo, logging which mode it got.
- `pub struct GpuContext { pub device, pub queue, pub surface_config, .. }`: `GpuContext::new(window: Arc<winit::window::Window>, pref: &AdapterPreference, present: PresentChoice) -> anyhow::Result<Self>` (uses `rivet_platform::select_adapter` over `instance.enumerate_adapters(Backends::PRIMARY)`; requests `TIMESTAMP_QUERY | TIMESTAMP_QUERY_INSIDE_ENCODERS` only if the adapter supports them); `resize(&mut self, w: u32, h: u32)` (ignores 0); `begin_frame(&mut self) -> Option<Frame>` (returns `None` and reconfigures on `Lost`/`Outdated`, `None` on `Timeout`); `adapter_info(&self) -> &wgpu::AdapterInfo`; `depth_view(&self)`.
- `pub struct GpuTimer`: `new(device, queue) -> Option<Self>` (None without timestamp support); `begin(&mut self, encoder)`, `end(&mut self, encoder)`, `resolve(&mut self, encoder)`, `poll_ms(&mut self, device) -> Option<f32>` — a 3-frame ring of query sets + map-read buffers, never blocks the frame.
- `pub struct Vertex { pos: [f32;3], normal: [f32;3], color: [f32;4] }` (Pod); `pub struct MeshData { pub vertices: Vec<Vertex>, pub indices: Vec<u32> }`; `pub struct GpuMesh` uploaded by `GpuMesh::new(device, &MeshData)`; `pub fn box_mesh(half: Vec3, color: [f32;4]) -> MeshData`.
- `pub struct MeshPipeline`: `new(ctx: &GpuContext)`; `draw(&self, pass, camera: &CameraUniform, items: &[(&GpuMesh, Mat4)])`. `CameraUniform { view_proj: Mat4, eye: Vec3, sun_dir: Vec3, sun_color: Vec3, ambient_sky: Vec3, ambient_ground: Vec3, fog_color: Vec3, fog_density: f32 }`. Shading: Lambert sun + hemispheric ambient + exponential fog, sRGB surface; per-draw model matrix through a dynamic-offset uniform buffer. No MSAA in M1.

- [ ] **Step 1: Failing tests**
  - `box_mesh_counts`: 24 vertices, 36 indices, all normals unit length.
  - `present_choice_parse`: `"immediate"` → Immediate, `"vsync"` → Vsync, `"bogus"` → Err.
  - `offscreen_smoke` (`#[ignore]` unless `RIVET_GPU_TESTS=1`): headless device renders one box to a 64×64 texture; the center pixel is not the clear color.
- [ ] **Step 2:** FAIL. **Step 3:** Implement (`offscreen_smoke` uses a texture target instead of a surface; factor `MeshPipeline::new` to take a `TextureFormat` so both paths share it).
- [ ] **Step 4:** `cargo test -p rivet-render` PASS; `RIVET_GPU_TESTS=1 RIVET_ADAPTER=integrated cargo test -p rivet-render -- --include-ignored` PASS on this PC.
- [ ] **Step 5:** Commit `feat(render): add wgpu context, GPU timer and mesh pipeline`.

---

### Task 7: `rivet-render` — overlay, bitmap font and frame graph

**Files:** `crates/rivet-render/src/{overlay.rs, overlay.wgsl, font.rs, frame_graph.rs}`.

**Interfaces — Produces:**
- `pub struct Overlay`: `new(ctx)`; `rect(&mut self, x, y, w, h: f32, color: [f32;4])` in pixels with origin top-left; `text(&mut self, x, y, scale: f32, color, s: &str)`; `draw(&mut self, ctx, pass)` (uploads and clears the batch; alpha blended, no depth).
- `font.rs`: `const GLYPHS: [(char, [u8; 7]); N]` 5×7 bitmaps for `0-9 A-Z a-z . : / - % ( ) space` (lowercase may reuse uppercase shapes); `pub fn layout(s: &str, x: f32, y: f32, scale: f32) -> Vec<[f32; 4]>` returns one pixel-rect per lit cell; unknown chars render as a filled box; advance = 6 × scale.
- `frame_graph.rs`: `pub fn bar_heights(samples: &[FrameSample], max_ms: f32, height_px: f32) -> Vec<(f32, Option<f32>)>` (CPU and GPU bar heights, clamped to `height_px`); `pub fn draw_frame_graph(overlay: &mut Overlay, stats: &FrameStats, adapter_name: &str, x, y: f32)`: 240×80 px panel, reference lines at 16.7 ms (amber) and 8.3 ms (green), bars colored green ≤ 8.3, amber ≤ 16.7, red above; text lines `"<adapter name>"` and `"CPU 6.1 GPU 4.3 FPS 162 P99 7.9"` (one decimal, GPU shows `--` if unavailable). Toggle with F3 (the game owns the key).

- [ ] **Step 1: Failing tests**
  - `layout_digit_one`: `layout("1", 0, 0, 1.0)` has exactly the lit cells of the "1" bitmap, all inside x∈[0,5), y∈[0,7).
  - `layout_advances`: `layout("11", 0, 0, 2.0)` second glyph's rects start at x ≥ 12.
  - `unknown_char_box`: `layout("~", ..)` → 35 rects.
  - `bar_heights_scale_and_clamp`: max_ms 33.3, height 80 → 16.65 ms → 40.0 (±0.1); 100 ms → 80.0; GPU None stays None.
- [ ] **Step 2:** FAIL. **Step 3:** Implement. **Step 4:** PASS + lint.
- [ ] **Step 5:** Commit `feat(render): add overlay, bitmap font and frame graph`.

---

### Task 8: Gray-box loop track and lap logic

**Files:** `game/nailrunners/src/{track.rs, laps.rs}`.

**Interfaces — Produces:**
- `pub struct Track { pub render: MeshData, pub collision: TriMesh, pub centerline: Vec<Vec3>, pub checkpoints: Vec<Checkpoint>, pub spawn: (Vec3, f32 /*yaw*/) }`, `Checkpoint { pub center: Vec3, pub forward: Vec3, pub half_width: f32 }`.
- `pub fn build_graybox_loop() -> Track`: a closed Catmull-Rom loop through these XZ control points (meters): `(0,0) (220,0) (300,80) (300,200) (220,280) (60,280) (-40,200) (-60,100)`. Sampled every 2 m; road 14 m wide with 1.5 m walls on both sides; the turn at `(300,80)→(300,200)` banked 15°; a 6 m-high crest/jump on the straight between `(220,280)` and `(60,280)` (smooth sine bump, 60 m long); checkpoints every ~40 m of arc (first at spawn); spawn on the first straight facing +X. Gray-box colors: road mid-gray with lighter stripes every 10 m (for speed readability), walls orange-red, ground plane dark olive at y = −0.5 extended 200 m beyond the loop.
- `pub struct LapTracker`: `new(checkpoint_count)`; `update(&mut self, prev_pos: Vec3, pos: Vec3, checkpoints: &[Checkpoint], now: f64) -> Option<LapEvent>`; `LapEvent::Lap { number: u32, time: f64 }`; a checkpoint is passed when the segment `prev→pos` crosses its plane within `half_width` in the forward direction, only in order; `last_checkpoint(&self) -> usize` (for respawn).

- [ ] **Step 1: Failing tests**
  - `loop_is_closed`: centerline first and last points < 2.5 m apart; length between 900 and 1100 m.
  - `road_is_drivable`: a raycast down at every centerline point hits the road at the expected height (±0.05) and normal.y > 0.9.
  - `walls_present`: a horizontal ray from the centerline toward the left side hits a wall at 7 m (±0.3).
  - `jump_height`: max centerline y on the jump straight is 6.0 (±0.1).
  - `laps_count_in_order`: feed positions along the centerline for 2.0 laps → exactly 2 `Lap` events, numbers 1 and 2.
  - `laps_reject_shortcut`: jump from checkpoint 0 straight to the last one → no lap event.
  - `backwards_does_not_count`: drive the centerline in reverse → no events.
- [ ] **Step 2:** FAIL. **Step 3:** Implement. **Step 4:** PASS + lint.
- [ ] **Step 5:** Commit `feat(game): add gray-box loop track and lap tracking`.

---

### Task 9: The game app — drive it

**Files:** `game/nailrunners/src/{main.rs, app.rs, args.rs, camera.rs, controls.rs, autopilot.rs, bench.rs}`.

**Interfaces — Consumes:** everything above. **Produces:** the `nailrunners` binary.

- `args.rs`: `pub struct Args { adapter: AdapterPreference /*--adapter, else RIVET_ADAPTER, else Auto*/, size: (u32,u32) /*--size 1600x900 default*/, present: PresentChoice /*--present, default Vsync*/, bench: Option<u32> /*--bench SECS: autopilot drive, forces Immediate unless --present given*/ }`; `fn parse(iter: impl Iterator<Item=String>) -> Result<Args, String>` (hand-rolled, no clap in M1).
- `controls.rs`: `fn drive_input(keys: &KeyInput) -> DriveInput` — W/Up throttle, S/Down brake/reverse, A/D or Left/Right steer, Space or Shift drift (held), hop on drift `just_pressed` (kart convention: hop starts drift), R respawn, F3 toggle graph, Esc quit.
- `camera.rs`: `ChaseCamera { update(&mut self, target_pos: Vec3, target_rot: Quat, speed: f32, dt: f32), view_proj(&self, aspect: f32) -> Mat4 }`: 7 m behind, 2.6 m up, look 3 m ahead; position spring-damped (critically damped, ω=8); FOV 70° rising to 82° at max speed + boost.
- `autopilot.rs`: `fn autopilot(state: &VehicleState, centerline: &[Vec3]) -> DriveInput` — steer toward the centerline point 15 m ahead, full throttle, brake above 0.9 rad heading error. (Seed for M4 bots.)
- `bench.rs`: after a 3 s warmup, collect `FrameStats` (capacity = all frames) for N seconds, then print one line `BENCH adapter="<name>" backend=<b> size=1600x900 present=<mode> frames=<n> avg_ms=<x> p50_ms=<x> p99_ms=<x> max_ms=<x> fps=<x> gpu_avg_ms=<x|na>` and write per-frame CSV `cpu_ms,gpu_ms` to `target/bench/<unix_time>.csv`; then exit 0.
- `app.rs`: winit 0.30 `ApplicationHandler`; window created in `resumed`; per frame: measure CPU frame time (wall clock between `RedrawRequested` events), `FixedTimestep::advance`, for each step `vehicle::step` (keep previous state for interpolation), lap tracker, respawn rule (Review Focus #4), interpolate for render, chase camera, draw track + car (box 1.4×0.6×2.0 m, body blue, a darker cockpit box), HUD text (speed km/h, lap n, current/best lap time, boost tier while drifting), frame graph. Skip render while minimized (Review Focus #1). On `--adapter` failure print the error to stderr and exit 2 (Review Focus #5).

- [ ] **Step 1: Failing tests**
  - `args_defaults`: no args → size 1600×900, present Vsync, bench None, adapter Auto (with `RIVET_ADAPTER` unset).
  - `args_bench_forces_immediate`: `--bench 30` → present Immediate; `--bench 30 --present vsync` → Vsync.
  - `args_errors`: `--size 1600` → Err mentioning `WxH`.
  - `controls_map`: W + D held → throttle 1, steer +1; Space just-pressed → drift && hop.
  - `autopilot_completes_lap`: headless (no GPU) sim of autopilot + `vehicle::step` on `build_graybox_loop()` for 90 s of ticks → at least 1 `Lap` event, never respawned.
  - `camera_settles_behind`: static car facing +Z, 120 updates → camera at (0, 2.6, −7) ±0.2.
- [ ] **Step 2:** FAIL. **Step 3:** Implement. **Step 4:** `just test` PASS, `just lint` clean.
- [ ] **Step 5: Manual run:** `just run --adapter integrated` → window shows track and car on the "Intel(R) UHD Graphics 730" adapter (named in the frame graph); drive a lap with the keyboard; drift-boost works; jump lands; R respawns; resize, minimize and restore don't crash. Then `just run --adapter discrete` works too.
- [ ] **Step 6:** Commit `feat(game): drive the gray-box loop with chase camera, HUD and bench mode`.

---

### Task 10: M1 perf gate and playtest handoff

**Files:** Create `docs/perf/2026-11-m1-graybox.md`.

- [ ] **Step 1:** Make sure the user isn't playing on this machine (perf taken while they play is discarded). Run `just bench --adapter integrated` and `just bench --adapter discrete`, each twice.
- [ ] **Step 2:** Record both `BENCH` lines per adapter, the date, commit hash, and Mesa/NVIDIA driver versions (`glxinfo -B` or wgpu `AdapterInfo.driver_info`) in the perf doc.
- [ ] **Step 3:** Gate: UHD 730 avg ≥ 60 fps **and** p99 ≤ 16.7 ms. If it fails, profile (GPU timer vs CPU time tells which side), fix, rerun; do not lower the gate without asking the user.
- [ ] **Step 4:** Commit `docs(perf): record M1 gray-box benchmark`.
- [ ] **Step 5:** Hand the build to the user to playtest the driving feel (`just run`); collect feedback on handling numbers before M2.

---

## Open follow-ups (not M1 tasks)

- **Gamepad:** choose between installing libudev headers somewhere for `gilrs` (Linux) vs. a pure-Rust evdev route — needs a dependency decision with the user.
- Windows manual test on real hardware (CI only proves it builds and tests pass).
- Real profiler integration (Tracy) once the frame graph isn't enough.
