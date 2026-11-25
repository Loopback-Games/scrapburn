# M1b: Feel and Beauty — Implementation Plan

> **For agentic workers:** executed inline (superpowers:executing-plans). The user is AFK and
> delegated all judgment calls ("make your best judgement calls on handling… make it beautiful").

**Goal:** Kart handling that feels like a polished arcade kart racer, and a gray-box loop turned into
a good-looking track — within the UHD 620 budget.

**Spec:** `docs/superpowers/specs/2026-11-25-rivet-nailrunners-design.md` (min spec now UHD 620).

## Global Constraints
- Perf: UHD 730 offscreen at 1600×900, Medium preset: GPU avg ≤ 6.0 ms, p99 frame ≤ 8 ms. (UHD 620 is
  roughly half a UHD 730; Low preset = render scale 0.75 must give it 60 fps at 1080p.)
- Every visual change is judged from an offscreen screenshot (`--bench N --offscreen --screenshot`)
  before it is committed. Bad-looking results are not committed (user's top wince).
- Original art only. Codex CLI (`codex exec`, image_generation) for 2D textures; Tripo (project cap
  800 credits, ledger `assets/tripo-spend.tsv`) for 3D; Blender (flatpak org.blender.Blender) for
  cleanup. Generated sources and prompts are kept in `assets/` with a `PROVENANCE.md`.
- New crates pass dependency vetting (1000+ stars, active, license). Expected: `image` (png+jpeg
  only), `serde_json`.
- TDD for logic (physics, mesh generation, UVs, mipmaps, GLB parsing, particles, ghosts); screenshots
  + bench for rendering.
- Commit per task, merge back to local `main` when the branch is reviewed. Never push.

## Handling design (research-backed, original numbers)
Sources: Disney Speedstorm drift dev diary (progressive glow/sparks, ~1.5 s charge), ModNation
Racers (better drift = more boost), MK8 mechanics write-ups (hop starts drifts, three tiers
blue→orange→purple, boosts ~0.5–3 s, steering modulates arc), Game Developer "implementing racing
games" (arcade = subtract simulation; simple colliders).
- Hop starts the drift; holding drift on landing with steer > 0.3 locks direction.
- Drift arc: yaw rate = dir × base × lerp(0.55, 1.45, tightness), tightness = (steer·dir + 1)/2.
- Charge rate = 1.0 + 0.6 × tightness (tight lines charge faster).
- Tiers: blue 0.55 s → +5 m/s 0.6 s; orange 1.2 s → +7 m/s 1.1 s; purple 2.0 s → +9 m/s 1.7 s.
- Surfaces per triangle: Road, Offroad (max speed ×0.55, extra drag, no drift charge), Wall,
  BoostPad (+8 m/s for 1.2 s). Wall hits scrub 25% of the speed component into the wall.
- Digital steering ramps (6/s in, 10/s out); analog passes through.
- Visual lean: body rolls into turns/drifts (render only), landing squash + camera shake.

## Tasks
1. **Handling v2** (`rivet-physics`): surfaces on `TriMesh` (`TriMesh::with_surfaces`,
   `RayHit::surface`), drift arc control, 3 tiers, charge-by-tightness, boost pads, wall scrub,
   offroad. Tests: tier thresholds, tight-vs-wide charge, offroad cap, boost pad, wall scrub,
   determinism, existing tests stay green.
2. **Post pipeline + sky + presets** (`rivet-render`): HDR (Rgba16Float) scene target at render
   scale, procedural sky (gradient, sun disk, soft clouds), bloom (mip chain), ACES tonemap, vignette,
   FXAA, `Preset {Low, Medium, High}`. Tests: preset values, render-size math, bloom mip sizes.
3. **Shadows**: one directional shadow map fitted around the kart, 3×3 PCF; size per preset.
   Tests: light-space fit contains the focus sphere.
4. **Textures + assets crate** (`rivet-assets`): PNG/JPEG decode (`image`), CPU mipmaps, texture
   array of materials; mesh vertices gain UV + material index; triplanar for terrain. Codex
   textures: asphalt, curb, grass, dirt, concrete barrier, metal. Tests: mip chain sizes/averaging,
   UV continuity along the road.
5. **World dressing**: hills terrain heightfield carved around the road, red/white curbs, start/finish
   gantry, boost pads, rocks and pines (instanced). Tests: terrain never above road near the track,
   instance placement avoids the road.
6. **Particles + juice**: drift sparks by tier, boost flames, offroad dust, landing puffs, camera
   shake, boost FOV kick + radial speed lines. Tests: particle lifetime/pool, spark color by tier.
7. **Kart model**: minimal GLB loader (`serde_json`), Tripo-generated kart cleaned in Blender, wheels
   spin/steer if separable, body lean. Tests: GLB parse of a tiny embedded file.
8. **Time trial + ghosts**: record transforms at 30 Hz, save best per track (XDG data dir / APPDATA),
   translucent ghost kart. Tests: record/playback interpolation, save/load round trip.
9. **HUD polish**: speed dial, lap/time panel, boost tier meter.
10. **Perf + screenshots + review**: bench doc, screenshots in `docs/media/`, final review, merge.
