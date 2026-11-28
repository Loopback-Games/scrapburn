# Crucible Menus and Full Settings — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current menus with the **Crucible** design and ship every setting shown in the mockup: Graphics, Audio, Controls (with rebinding), Camera & view, and Gameplay, all saved between sessions.

**Design reference:** `docs/superpowers/specs/2026-11-27-crucible-menus-mockup.html`. Open it in a browser (it uses `docs/media/*.jpg` as backdrops) and press 2 for Crucible. The private artifact it came from is https://claude.ai/artifact/KMuMXCej25K5RiaTtCGwjK. Rows marked NEW in the mockup are the settings this plan adds.

**Look in one paragraph:** the left side is a tall frosted-glass column, cut on a diagonal, with a sliver of molten metal along the cut. The title is in a stencil face (Big Shoulders Stencil Display, Black) filled with a molten gradient. Rows use Chakra Petch. The selected row fills with a molten gradient with dark text and a slanted right end. The right side previews whatever is highlighted. The scene behind is graded warm and dark, with embers rising.

**Scope decision (user, 2026-11-27):** everything in the mockup, including rebinding, display mode, V-sync, frame cap, HUD toggles, and vibration.
**Fullscreen (user, 2026-11-27):** "True full screen did work at some point." Ship real exclusive fullscreen.

## Global Constraints
- Branch `feat/crucible-menus` off `main`. Commit per task (Conventional Commits, American English). Never push; ask before any PR.
- `just lint` and `just test` are green before every commit. `just ci` runs before the branch is called done.
- TDD for logic: menu navigation, settings parsing, bindings, limiter, embers, camera tuning, volume math. Visuals are judged from offscreen screenshots compared against the mockup. Nothing that looks wrong is committed.
- Minimum spec stays Intel UHD 620 on the Low preset. Menu frames on Low must stay within **+0.5 ms GPU** of today's `--attract` bench, which means the blur pass has to be cheap. Measure before and after; numbers are only claimed from a captured run.
- No new crates are expected. Rumble uses `gilrs::ff`, already in the tree. If one becomes necessary, it goes through the dependency-vetting rules first.
- Fonts are OFL from github.com/google/fonts. Add each with `OFL.txt` coverage and a line in `assets/fonts/PROVENANCE.md`.
- Rajdhani stays as the race HUD font. Crucible faces are used in menus only.

## Facts the plan relies on (from the 2026-11-27 code survey)
- **Overlay** (`crates/rivet-render/src/overlay.rs`):
  - Vertex is `{pos, uv, color}`. A negative uv means solid color, and color is per vertex, so gradients only need different vertex colors.
  - It has one bind group: uniform, an R8 atlas, and a linear sampler. It runs one draw with `ALPHA_BLENDING`.
  - It draws in a separate "hud" pass on the swapchain, **after** tonemap, bloom, and FXAA (`app.rs:2169-2195`). The stale `//!` comment at `overlay.rs:1` should be fixed while there.
- **Font atlas** (`font_atlas.rs`):
  - It is a single face: 1024×512 R8, `RASTER_PX = 64`, ASCII 32..126, coverage-only (not SDF), no kerning.
  - `GLOBAL_METRICS` is a single `OnceLock`. `label`, `label_centered`, and `label_width` take no face.
  - Drawing 80–150px titles from a 64px raster would blur.
- **Post** (`scene.rs`):
  - Targets are private: `scene_color` (HDR, render-scale), bloom mips (threshold-filtered, so not a scene blur), and `ldr` (only when FXAA is on).
  - **No blurred copy of the scene exists**, and the overlay cannot sample the swapchain.
  - `Lighting { exposure, bloom_intensity, vignette, saturation }` is public and already swapped per frame for the garage (`app.rs:2165-2178`).
- **Quality** (`preset.rs`, `scene.rs:347`):
  - `SceneRenderer::new(preset)`. Any quality change is a full `rebuild_renderer()` (`app.rs:1213`), which keeps in-RAM layers and lighting.
  - `render_scale`, `bloom_levels`, and `fxaa` only touch `make_targets`. `shadow_size` and `anisotropy` are baked into `new`. `texture_skip_mips` re-uploads the materials.
- **Window and present** (`context.rs`, `app.rs:326`):
  - winit 0.30.13 and wgpu 30.0.2. The window is windowed only, with no fullscreen code anywhere.
  - Present is `Vsync` (Fifo) or `Immediate`, chosen once in `GpuContext::new`, with no runtime setter.
  - There's no frame limiter: `ControlFlow::Poll` plus `request_redraw` every iteration.
- **Input** (`controls.rs`, `rivet-platform/src/gamepad.rs`):
  - Every binding is a hard-coded `KeyCode` or `PadButtons` in `drive_input`, `menu_input`, `menu_input_typing`, and `actions`. Start-to-pause is at `app.rs:1263`.
  - Deadzones are consts (stick 0.15, trigger 0.05). No rumble, and `GamepadId` is private.
- **Audio** (`rivet-audio/src/mixer.rs`):
  - The `Command` enum has no volume or bus control. `MUSIC_LEVEL = 0.42` is fixed, and there are no UI sounds.
  - `Theme` has no public name or info. A song can be previewed by sending `Command::Music{theme}`. There's no readback.
- **Camera** (`camera.rs`):
  - Everything is a const: BACK 4.15, HEIGHT 1.64, FOV_MIN 61 / FOV_MAX 72, boost +9, SHAKE_MAX 0.35. The FOV already widens with speed.
- **Menus** (`menu.rs`, `hud.rs`):
  - Rows are strings in a `"LABEL  < value >"` format, parsed by `hud::split_row`. Help text is a per-screen table inside `draw_menu`. Title row indices are hard-coded in `handle()`.
  - Mouse hover, click, and the wheel go through `MenuRows`. The race HUD scale is `ui = (h/900).clamp(0.6, 2.0)`, and the speedometer is km/h only.
- **Persistence:**
  - Settings are **not saved today**. Only `<data>/garage` (`Loadout::save`) and `<data>/racer-name` are, both written with a temp file and rename.
  - The data dir is `ghost::data_dir()`.
- **Screenshots:** `--bench N --offscreen --screenshot FILE.png`, plus `--attract` for the title and `--garage` for the garage. No menu screen can be opened directly yet.

## Review Focus
1. **The player can't lock themselves out.** Menu navigation (arrows/WASD/Enter/Esc, D-pad/A/B) and Esc/Start-to-pause are fixed and never rebindable. Rebinding a key that another action already uses swaps the two, so no action is left unbound.
2. **A corrupt or old settings file never stops the game from starting.** Unknown keys are ignored, bad values fall back to defaults per key, and the file is written to a temp file and then renamed.
3. **CLI flags beat saved settings for that run and are not written back:** `--preset`, `--present`, `--size`, and bench runs. Benches never read or write the settings file, so they stay repeatable.
4. **Holding left or right on a graphics slider doesn't rebuild the renderer every frame.** Rebuilds are debounced (≈350 ms after the last change, or immediately on leaving the tab).
5. **Resize, minimize, rebuild, and present-mode changes leave the blur and overlay bind groups valid.** Use a generation counter. A zero-size window never configures.
6. **Exclusive fullscreen is real.** The user reports true fullscreen has worked on this machine before, though git history holds no fullscreen code. Implement `Fullscreen::Exclusive` with the monitor's best video mode. Fall back to borderless only when `video_modes()` is empty, and log that at info level. Verify on this machine.
7. **Rumble failure is silent.** No force-feedback support, or no write permission on `/dev/input/event*`, disables vibration without errors or panics.
8. **The blur costs nothing outside menus.** It only runs when a menu screen is drawn, and never during a race unless paused.

---

## File Structure
```
crates/rivet-render/src/
  font_atlas.rs      multi-face atlas (Face enum, per-face raster px, 2048×1024)
  overlay.rs         faces, gradient quads/text, glow, backdrop sampling, set_backdrop()
  overlay.wgsl       backdrop texture binding; uv sentinel selects atlas vs backdrop vs solid
  scene.rs           SceneRenderer::with_settings(QualitySettings), backdrop blur pass + accessor
  post.wgsl          fs_backdrop_down (tonemap + 4× downsample), fs_kawase
  context.rs         GpuContext::set_present(PresentChoice)
crates/rivet-platform/src/gamepad.rs   runtime deadzone, rumble(), Y / Menu already mapped
crates/rivet-audio/src/{mixer.rs, songs.rs}   Bus volumes, UI sfx, Theme::info()
game/scrapburn/src/
  prefs.rs           NEW: Prefs (all settings) + load/save of <data>/settings
  bindings.rs        NEW: Action, Bindings, capture/swap logic, key-name table
  limiter.rs         NEW: frame-cap deadline math
  menu.rs            typed rows, Screen::Settings(Tab), new title/pause, capture state
  menu_draw.rs       NEW: all Crucible drawing (column, rows, tabs, previews, embers)
  embers.rs          NEW: ember particle sim (pure)
  hud.rs             race HUD honors scale/minimap/speedo/units/names; menu code moves out
  controls.rs        reads Bindings + sensitivity; look-behind action
  camera.rs          CameraTuning; look_behind
  sound.rs           bus volumes, UI sounds, song preview
  style.rs           Crucible palette
  args.rs            --menu <screen> for screenshots
  app.rs             wiring: prefs, debounce rebuild, display/vsync/cap, capture routing, rumble
assets/fonts/        BigShouldersStencilDisplay-Black.ttf, ChakraPetch-SemiBold.ttf (+ PROVENANCE)
```

---

### Task 1: Saved settings (`prefs.rs`)
**Goal:** one `Prefs` struct holds every setting and round-trips through `<data>/settings`.

- [ ] **Step 1:** Write failing tests:
  - Defaults when the file is missing.
  - A round trip of every field.
  - Unknown keys are ignored.
  - A bad value falls back to the default for that key only.
  - Values are clamped to range.
- [ ] **Step 2:** Define the `Prefs` sections:
  - `graphics`: `preset: PresetChoice{Low,Medium,High,Custom}`, `auto`, `quality: QualitySettings`, `display: DisplayMode{Windowed,Borderless,Fullscreen}`, `vsync`, `frame_cap: Option<u32>` (30/60/120/144/None), `fps: FpsDisplay`.
  - `audio`: `master, music, effects, engines, menus` (0–100), `soundtrack: bool`.
  - `controls`: `bindings: Bindings` (Task 10, stubbed with defaults here), `sensitivity` (0–100, 50 is neutral), `deadzone` (0–30%, default 15), `vibration`.
  - `camera`: `distance` (3.0–7.0 m, default 4.15), `height` (1.0–3.0 m, default 1.64), `fov` (55–90°, default 61), `widen_at_speed` (default on), `shake` (0–100%, default 100).
  - `hud`: `scale` (70–130%), `minimap`, `speedometer`, `units: Kmh|Mph`, `rival_names`.
  - `gameplay`: `laps`, `rivals`.
  - The mockup's camera numbers (6.5 m, 75°) were illustrative. **Defaults must equal today's constants**, so nothing changes for a player who never opens the menu.
- [ ] **Step 3:** Use a plain `key value` text format, one per line, matching the existing `garage` file style. No serde, because no new dependency is needed. Example lines: `graphics.render_scale 0.75`, `controls.bind.use_item key:KeyE key:ControlLeft pad:West`.
- [ ] **Step 4:** Factor the temp-file-then-rename write used by `Loadout::save` and the racer name into one `ghost::write_atomic(path, bytes)` and use it here.
- [ ] **Step 5:** Wire into `app.rs:443-458`: load the prefs, then apply CLI overrides on top for that run only. Bench runs skip load and save.
- [ ] **Step 6:** `menu::Settings` becomes a view over `Prefs`: keep `loadout` and `track` and embed `prefs`. Update every reader the survey listed (`app.rs` lines for laps, rivals, fps, music, preset). `music: bool` becomes `prefs.audio.soundtrack`.
- [ ] **Step 7:** Save on leaving any Settings tab, and on Quit.

### Task 2: Typed menu rows and the new menu structure (`menu.rs`)
**Goal:** the menus carry structured rows and the new screen map. The old renderer still draws them during this task, through an adapter.

- [ ] **Step 1:** Replace `items() -> Vec<String>` with `rows() -> Vec<Row>`:
  - `Row { label, kind, help, new: bool }`.
  - `kind` is one of `Go`, `Choice{value}`, `Slider{value_text, fraction}`, `Bind{text}`, `Text{value}`.
  - `help` moves out of `hud.rs`'s per-screen table and into each row.
  - Keep a temporary `Row::legacy_string()` so today's `draw_menu` keeps working until Task 5.
- [ ] **Step 2:** Screen changes:
  - `Settings` and `Controls` merge into `Settings(Tab)`, where `Tab` is `Graphics | Audio | Controls | Camera | Gameplay`.
  - Title rows: Solo race, Multiplayer, Garage, Settings, the optional Update row, then Quit.
  - Pause rows: Resume, Restart, Settings, Main menu, Quit.
  - Gameplay holds Laps, Rivals, and Racer name (which opens `DriverName`).
  - Race setup gains Laps and Rivals rows, as in the mockup.
  - Replace the hard-coded title indices in `handle()` with matching on a row id enum.
- [ ] **Step 3:** `MenuInput` gains `tab_prev` and `tab_next`: Q/E on keyboard, LB/RB on pad (both mappings in `controls::menu_input`). Wrap across the five tabs. Back from any tab returns to `return_to` with the Settings row reselected.
- [ ] **Step 4:** Add `MenuAction`s:
  - `GraphicsChanged`, debounced in app.
  - `DisplayChanged`, `VsyncChanged`, `FrameCapChanged`.
  - `VolumeChanged`, `PreviewSong(Theme)`.
  - `BindingCaptured`.
  - `PresetChanged` stays and applies the whole preset to `quality`.
  - Editing any single quality row sets the preset to `Custom`.
- [ ] **Step 5:** Update every existing menu test to the new indices and rows. Keep the behavior each one pins (wrap, update-above-quit, garage reselect, track picker return, pause return row, music default on, settings cycle). Add new tests:
  - Tab cycling.
  - Back from a tab.
  - Custom preset on a single edit.
  - Slider clamping and steps.
  - Debounce: an action is only emitted on a value change.

### Task 3: Overlay — faces, gradients, glow (`rivet-render`)
**Goal:** the overlay can draw crisp large stencil text, gradient quads and text, and soft glows, with one atlas texture and one draw call.

- [ ] **Step 1:** Add fonts:
  - `BigShouldersStencilDisplay-Black.ttf` and `ChakraPetch-SemiBold.ttf` from google/fonts, with PROVENANCE lines.
  - Big Shoulders Stencil ships as a variable font. If fontdue renders its default instance rather than Black, make a static Black instance with `uvx fonttools varLib.instancer` and record the command in PROVENANCE. **Check this first.**
- [ ] **Step 2:** `FontAtlas::new(&[(bytes, raster_px)])` packs N faces into one 2048×1024 R8 texture:
  - Text at 64 px, Rows (Chakra Petch) at 64 px, Display (stencil) at **176 px** with uppercase, digits, and punctuation only.
  - Failing test first: all three faces fit, and the atlas-full error still fires when overfilled.
  - `GLOBAL_METRICS` becomes per face.
- [ ] **Step 3:** API:
  - `Face { Hud, Row, Display }`, plus `label_face`, `label_face_centered`, `label_width_face`.
  - The existing `label*` keep meaning `Face::Hud`, so the race HUD is untouched.
- [ ] **Step 4:**
  - `quad(points: [[f32;2];4], colors: [[f32;4];4])`. Gradient rects and slanted row ends are built from it.
  - `label_gradient(face, x, y, scale, top, bottom)` gives each glyph quad its vertex colors by the vertex's y within the cap height.
  - `glow_rect(x, y, w, h, spread, color)` is a 9-slice of quads whose outer vertices have alpha 0.
- [ ] **Step 5:** Unit-test the vertex math on the CPU (no GPU): the gradient endpoints, the quad vertex count, and that glow outer vertices have alpha 0.

### Task 4: Frosted backdrop and menu grade (`scene.rs`, `post.wgsl`, overlay)
**Goal:** the overlay can sample a blurred, tonemapped copy of the scene, and the scene can be graded for menus.

- [ ] **Step 1:** `SceneRenderer::set_backdrop(enabled)`. When enabled, after the HDR scene:
  - `fs_backdrop_down` samples `scene_color`, applies the same exposure/ACES/saturation as `fs_composite` (factor out a WGSL function), and writes **¼ output resolution** `Rgba8UnormSrgb`.
  - Two dual-Kawase passes run at ¼ resolution.
  - Expose `backdrop_view() -> Option<&TextureView>` and `backdrop_generation() -> u64`, which bumps on resize and rebuild.
- [ ] **Step 2:** Overlay gets binding 3, a backdrop `texture_2d` (sampled as color), plus a 1×1 dummy when there's no backdrop.
  - The vertex `uv.x` sentinel selects the source: `< -1.5` samples the backdrop at `pos / screen` and multiplies by the vertex color; `-1.5..0` is solid; `>= 0` is the atlas.
  - Add `Overlay::set_backdrop(device, Option<&TextureView>, generation)`, which rebuilds the bind group only when the generation changes.
  - Add the `backdrop_quad(points, tint)` helper.
- [ ] **Step 3:** Menu grade reuses `Lighting`: while a menu is shown (but not in the garage), swap in a warm, dark `Lighting` (lower exposure, saturation ≈0.8, stronger vignette), the same way the garage does at `app.rs:2165`. No shader change is needed.
- [ ] **Step 4:** Bench `--attract --preset low --adapter integrated` before and after. Record the GPU ms in the commit message. The gate is +0.5 ms.

### Task 5: Crucible drawing (`menu_draw.rs`, `embers.rs`, `style.rs`)
**Goal:** every menu screen renders as in the mockup, at 1280×720 through 4K, using `menu_scale`.

- [ ] **Step 1:** Add an `--menu <name>` arg (args.rs and app.rs) that opens a screen directly for offscreen screenshots.
  - Names match the mockup's hash keys: `title`, `race-setup`, `tracks`, `garage`, `multiplayer`, `lobby`, `pause`, `results`, `graphics`, `audio`, `controls`, `camera`, `gameplay`.
  - It follows the `--garage` precedent.
- [ ] **Step 2:** Add the Crucible palette to `style.rs`: char, smoke glass, ember, gold, white heat, plus the molten gradient stops (#ff3a0a, #ff8a1f, #ffc35c, #fff1d6).
- [ ] **Step 3:** The column has three parts:
  - Molten edge: two triangles, 8 px wider than the glass, with a vertical gradient.
  - Glass: a `backdrop_quad` with a dark warm tint.
  - The diagonal cut at 86% of the width at the bottom, as in the mockup.
- [ ] **Step 4:** Header, rows, and footer:
  - Kicker and help use `Face::Row`. The title uses `Face::Display` with a gradient from gold to ember.
  - Tabs.
  - Row variants: Go, Choice with ‹ › on the selected row, and Slider (track, molten fill, and knob; dark on the selected row). `NEW` badges are not drawn in the game.
  - Selected row: a molten gradient quad with a slanted right end plus `glow_rect`.
  - Footer prompts follow the last-used device (key names, or pad glyphs via the existing `PadGlyph`).
  - Port `MenuRows` hit testing, including tab hit zones and slider click left/right.
- [ ] **Step 5:** `embers.rs`, as a pure sim: spawn, rise, sway, fade, and respawn, with a fixed count (70) and seeded RNG.
  - Tests: particles stay in bounds, the count is stable, and reduced motion freezes them. Add `--no-motion`, or reuse an existing accessibility flag if there is one.
  - Draw them as small overlay circles.
- [ ] **Step 6:** Right-side previews. Reuse existing drawing where it exists and restyle it:
  - Track card from `draw_track_card`.
  - Lobby grid from `draw_lobby`.
  - Garage: the right side stays mostly clear because the 3D kart already rotates in the service bay. Show the name, class, and finish swatches; swatch picking reuses `garage_swatch_at`.
  - Pause status: position, lap, best lap, and the minimap.
  - Results table on the right.
  - Graphics spec list (render scale, shadow map size, bloom, anisotropy, textures, frame cap). The live scene behind is the preview.
  - Audio mixer faders, plus a now-playing card from `Theme::info()` (Task 8).
  - Keyboard and gamepad diagrams that highlight the selected binding's keys or buttons, drawn from `Bindings`.
  - Camera rig side view plus a mini HUD preview (Task 9 and Task 10 values).
- [ ] **Step 7:** Restyle the Online, Tailscale, DriverName, and Lobby screens with the same components. They're not in the mockup but share it.
- [ ] **Step 8:** Delete the old `draw_menu`, `controls_card`, `garage_card`, and the `legacy_string` adapter. Move the `split_row` tests or drop them with the code.
- [ ] **Step 9:** Take screenshots of every `--menu` screen at 1280×720 and 1920×1080 on `--adapter integrated --preset low`. Compare each against the mockup and fix the differences before committing.

### Task 6: Graphics settings apply (`scene.rs`, `app.rs`)
- [ ] **Step 1:** Add `SceneRenderer::with_settings(QualitySettings)`. `new(preset)` calls it with `preset.settings()`.
- [ ] **Step 2:** `rebuild_renderer` uses `prefs.graphics.quality`. `GraphicsChanged` schedules a rebuild at now + 350 ms. Leaving the Graphics tab flushes any pending rebuild.
- [ ] **Step 3:** Auto preset:
  - The "Pick automatically" toggle drives `tuner`. When the tuner steps, it writes `preset` and `quality`.
  - Turning auto off stops the tuner.
  - This fixes today's stale "(AUTO)" label.
- [ ] **Step 4:** Test the debounce scheduling as a pure function. Then screenshot Low, Medium, High, and a Custom mix to confirm each knob changes the image.

### Task 7: Display mode, V-sync, frame cap (`context.rs`, `limiter.rs`, `app.rs`)
- [ ] **Step 1:** `GpuContext::set_present(PresentChoice)` updates `surface_config.present_mode` (keeping the Immediate → Mailbox → Fifo fallback) and reconfigures. V-sync on means Fifo; off means Immediate.
- [ ] **Step 2:** Display mode:
  - Windowed: `set_fullscreen(None)`.
  - Borderless: `Fullscreen::Borderless(None)`.
  - Fullscreen: `Fullscreen::Exclusive(mode)`, picking the current monitor's mode closest to the window size at the highest refresh rate. If `video_modes()` is empty, fall back to Borderless and log it.
  - Remember the windowed size to restore it.
- [ ] **Step 3:** `limiter.rs`: `next_deadline(last: Instant, cap_hz) -> Instant` and `wait_mode(now, deadline)`. Use `ControlFlow::WaitUntil`, then a short spin for the last ~1 ms.
  - TDD the deadline math, including a missed deadline not accumulating debt.
  - In `about_to_wait`, only request a redraw once the deadline has passed. No cap keeps today's `Poll`.
- [ ] **Step 4:** Verify by hand on this machine: toggle each mode, alt-tab, resize, and minimize. Measure the frame cap with the bench `--present immediate` at caps of 60 and 144: average frame time within ±3%.

### Task 8: Audio buses, UI sounds, song preview (`rivet-audio`, `sound.rs`)
- [ ] **Step 1:** Add `Command::Volume { bus: Bus, gain: f32 }`, where `Bus` is `Master | Music | Effects | Engines | Ui`.
  - The mixer multiplies per bus: Music applies on top of `MUSIC_LEVEL`, Engine and Skid go to Engines, `Play` goes to Effects, and the new UI sfx go to Ui. Master applies before the soft clip.
  - Tests: zero gain on a bus silences only that bus, and master scales everything.
- [ ] **Step 2:** Add `Sfx::UiMove`, `UiSelect`, `UiBack`: short synthesized clicks in the existing sfx synth. Send them on menu move, select, and back.
- [ ] **Step 3:** `Theme::info() -> ThemeInfo { title, style, key, bpm }`, taken from the songs.rs doc comments and `bpm` fields. Glass is 142 BPM and Solar is 160; read the key and style from the source rather than guessing.
- [ ] **Step 4:** While the Audio tab's "Listen to" row is set, the music cue uses that theme. Leaving the tab restores the normal cue. A percent volume maps to gain with `(v/100)^2` for a perceptual curve; test that 0 is silent and 100 is unity.

### Task 9: Camera tuning and look-behind (`camera.rs`)
- [ ] **Step 1:** `CameraTuning { distance, height, fov, widen_at_speed, shake }` with `Default` equal to today's constants. `ChaseCamera` holds one, and `set_tuning()` is called when prefs change.
- [ ] **Step 2:** FOV goal becomes `fov + (widen ? speed term + boost : 0)`, with `FOV_MAX - FOV_MIN` kept as the speed range. Shake impulses are multiplied by `shake/100`.
- [ ] **Step 3:** `look_behind(bool)`: while held, the offset mirrors to the front and looks back, snapping without spring lag.
- [ ] **Step 4:** Update the existing camera tests to read from `CameraTuning::default()`, and add tests for distance and height settling, shake 0 meaning no movement, and widen-off keeping FOV constant.

### Task 10: Bindings, rebinding, sensitivity, dead zone (`bindings.rs`, `controls.rs`, platform)
- [ ] **Step 1:** `Action`: Accelerate, Brake, SteerLeft, SteerRight, DriftHop, UseItem, Respawn, LookBehind.
  - Pause and menu navigation are not actions (Review Focus 1).
  - `Bindings { keys: [[Option<KeyCode>; 2]; N], pad: [PadBind; N] }`, where `PadBind` is `Button(PadButtons) | LeftTrigger | RightTrigger | Stick`.
  - Pad steering is always the left stick.
  - Defaults equal today's hard-coded set. Look behind defaults to Q and Y.
- [ ] **Step 2:** The key-name table maps a bindable `KeyCode` to and from a stable string for prefs, covering letters, digits, arrows, Space, Shift, Ctrl, Alt, Tab, Enter, and F1–F12. Esc can't be bound.
  - Test the round trip for every entry, and that an unknown name falls back to the default binding.
- [ ] **Step 3:** Capture flow:
  - Enter on a Bind row sets `MenuState.capturing = Some(action)`. The row shows "Press a key… Esc cancels".
  - app.rs routes the next key or pad press to `menu.capture(...)`.
  - A conflict swaps the two actions.
  - TDD: capture sets the binding, Esc cancels, swap on conflict, and nothing is ever left unbound.
- [ ] **Step 4:** `drive_input` and `actions` take `&Bindings`. `hud.rs` hint strings (the item hint at `hud.rs:1471`) and the controls diagram read from bindings. Update the existing `controls.rs` tests to use the default bindings.
- [ ] **Step 5:** Sensitivity: analog steer becomes `sign(s)·|s|^e`, with `e = lerp(1.6, 0.6, sens/100)`, so 50 maps to about 1.0. Keyboard: `STEER_RAMP_IN` is scaled by `lerp(0.6, 1.4, sens/100)`. Tests at 0, 50, and 100.
- [ ] **Step 6:** `Gamepads::set_stick_deadzone(f32)` replaces the const in the poll loop. Test `apply_deadzone` at the new values.

### Task 11: Vibration (`rivet-platform/src/gamepad.rs`, `app.rs`)
- [ ] **Step 1:** `Gamepads::rumble(strong, weak, ms)` uses `gilrs::ff::EffectBuilder` on the active pad. Any error disables rumble for the session, logged once at debug level.
- [ ] **Step 2:** Trigger points: wall hit, landing, boost start, item hit (the same places that call `camera.kick`), scaled by impact. Gate on `prefs.controls.vibration`.
- [ ] **Step 3:** Check by hand with a pad on this machine. If `/dev/input` permissions block force feedback, document the udev or uaccess note in README rather than asking for sudo.

### Task 12: HUD options (`hud.rs`)
- [ ] **Step 1:** Multiply the race HUD `ui` by `hud.scale/100` in every function that computes it (draw, draw_race, human_row, draw_toast, draw_item_slot, draw_fps_counter). Put the formula in one helper.
- [ ] **Step 2:** Minimap and speedometer toggles. Units convert with mph = m/s × 2.23694 and a matching label. Rival names gate `driver_tag`, which today is only used online. Offline name tags over the karts ahead are out of scope unless they're cheap; note the decision in the commit.
- [ ] **Step 3:** Screenshot a race with every toggle off, mph, and 130% scale.

### Task 13: Final pass
- [ ] **Step 1:** Screenshot every `--menu` screen at both resolutions, plus a race HUD. Review them side by side with the mockup.
- [ ] **Step 2:** Run the perf bench on Low with the integrated GPU, in menus and in a race. Compare against `main` and write the result in `docs/perf/`.
- [ ] **Step 3:** Update the README controls and settings sections and the settings-file location. Replace `docs/media/title.jpg` with the new title screen.
- [ ] **Step 4:** Run `just ci`, then use requesting-code-review against the Review Focus list. Ask the user before merging or opening a PR.

## Out of scope
- Kerning and SDF text. Rasterizing the display face at 176 px is enough for 4K at `menu_scale` 2.5; revisit only if screenshots at 4K look soft.
- Partial renderer rebuilds (targets-only for render scale, bloom, and FXAA). The debounced full rebuild is enough. Only optimize if a rebuild takes over 250 ms on UHD 620.
- Remapping menu navigation, or binding the right stick.
