# Rivet engine + Nailrunners — design (brainstorm 2026-11-25)

Working names (rename freely): **Rivet** = the engine, **Nailrunners** = the game.

## Context / intent
The user wants to push a Quake 3–spirited engine as far as it can go, written in Rust, and ship a
combat kart racer on it under the Loopback Games GitHub org: "Mario Kart meets Mad Max meets Quake."
It must look as close to AAA as the engine allows and still run smoothly at high frame rates on Intel
integrated graphics, on Linux and Windows, with multiplayer and AI bots. Engine pieces should be
reusable in future Loopback projects.

Key decision: **not a port.** Quake 3's source is GPLv2; a Rust translation would be a derivative
work and force GPL on every reusable module. Instead Rivet is a **clean, original engine inspired by
the designs** of Quake 3 and others (Doom 3/2016, Source, Trackmania/F-Zero). We study ideas, we do
not translate code. License: **MIT OR Apache-2.0**.

## Goals
- Original Rust engine, cargo workspace of small separately reusable crates.
- **Minimum spec: the user's ThinkPad — Intel Core i7-8550U (4C/8T, 15 W) with Intel UHD
  Graphics 620, 16 GB RAM** (updated 2026-11-25; it is weaker than the desktop UHD 730, so per the
  rule below it became the floor). Target ≥60 fps on the Low preset at the laptop's native
  resolution with render-resolution scaling (to be confirmed by measurement). The desktop UHD 730
  is the everyday measuring stick; RTX 3060 is high-end.
- Render-resolution scaling (3D at a fraction of output resolution, HUD at native) is an early
  engine feature, not a late polish item. CPU budget matters too: 4 cores at 15 W, thermal
  throttling. Scalable presets Low → Ultra; 120+ fps on stronger GPUs.
- "AAA as possible" through art direction: stylized-premium look (Mario Kart 8 / Overwatch-ish
  clarity), not photorealism.
- Linux + Windows builds.
- Multiplayer: friends online, player-hosted or headless dedicated server (Linux/Windows), 8–12
  racers, bots fill slots, direct IP / invite code, **Tailscale support** (tailnet peer discovery).
- v1 modes: **Combat race**, **Time trial + ghosts**, **Elimination**.
- At least 3 tracks at launch; AI bot racers that use weapons.

## Non-goals (v1)
- Translating or copying any id Software code; using any original game's assets, names, logos.
- Public matchmaking, server browser service, anti-cheat, accounts.
- Arena deathmatch (later), split-screen (later), music (deferred), mod VM (later — keep the seam).
- Bevy, Fyrox, or any full engine as the foundation.

## Anti-goals (failure even if it "works")
- Stutters / frame-time spikes on the UHD 730. Frame time is a gate on every milestone.
- Cheap-looking models or scenery (user's known top wince — see TREELINE). Quality gates mandatory.
- Homages that are copies: names, characters, logos, music, level layouts must be original.
- Crates so entangled with the game they can't be reused.

## Approaches considered
- **A. Own engine on low-level crates (chosen).** wgpu/winit/glam/gilrs/cpal; our own renderer,
  deterministic arcade physics, netcode, AI. Max control over iGPU frame time, determinism for
  ghosts/prediction, truly portable modules. Slowest to first pixels; we build our own tooling.
- B. Bevy — fastest start, but someone else's engine, ~quarterly API breaks, general renderer hard
  to tune for UHD 730, scheduler fights determinism.
- C. Hybrid (bevy_ecs, rapier) — modest savings; rapier is a poor fit for arcade kart handling.
- Rejected earlier: faithful Q3 port (GPL lock-in; its GL1-era renderer and corridor BSP/PVS would
  be replaced anyway).

## Decided "how"
### Repo & tooling
- Local repo `/var/home/napalm/git/loopback-games/rivet` (one workspace to start: `crates/` engine,
  `game/` Nailrunners, `tools/`). Split into separate repos later if wanted. GitHub repo under
  loopback-games **only after asking**; never push without asking.
- `mise.toml` pins Rust toolchain + tools; `justfile` with `setup fmt lint test security build run
  clean ci`; CI on GitHub Actions (Linux + Windows runners, actions pinned by SHA) calls `just ci`.
- Every new dependency goes through the dependency-vetting rules (1000+ stars, active, license OK).

### Engine crates (each usable alone)
| Crate | Owns | Inspired by |
|---|---|---|
| `rivet-core` | fixed-tick main loop (sim at fixed Hz, render interpolated), time, cvars/console, logging | Q3 frame loop, cvars, console |
| `rivet-platform` | winit window, input, gilrs gamepads, adapter selection (force iGPU on hybrid systems) | — |
| `rivet-render` | wgpu renderer: clustered forward+, PBR with shading tiers, lightmaps + baked probes, few dynamic lights, decals, GPU particles, post (bloom, tonemap, TAA/FXAA tiers), material scripts, GPU timestamp profiling | Doom 2016 clustered forward + decals; Q3 shader scripts |
| `rivet-assets` | glTF import, offline baker (lightmaps, probes, collision, nav, visibility), packed asset format, hot reload | Q3 q3map compiler, .pk3 |
| `rivet-physics` | deterministic fixed-point-friendly arcade vehicle sim (raycast suspension), collision queries | Trackmania determinism |
| `rivet-net` | UDP client/server, delta-compressed snapshots, client prediction + reconciliation, lag-compensated hits, Tailscale peer discovery | Q3 snapshots, Source lag compensation |
| `rivet-ai` | racing-line following, overtaking, item use, rubber-banding difficulty | Q3 AAS idea → racing-line graph |
| `rivet-audio` | cpal mixer, 3D positional, procedural engine/weapon synthesis | — |
| `rivet-ui` | HUD, menus, debug overlay (frame graph always available) | — |
Mod VM / scripting: leave a clean seam in `game/`, not v1.

### Map authoring
Tracks built in **Blender via blender-mcp**, exported as glTF with custom empties/properties for
checkpoints, spawn grid, item boxes, jump pads, teleporters, shootable switches, bot racing lines.
`rivet-assets` bakes lighting, probes, collision, and bot data (the q3map role).

### Game design (Nailrunners)
- **Handling:** kart arcade controls (drift-to-boost, hop, air tricks) with Mad Max heft — ramming
  shoves lighter cars, landings thud.
- **Vehicles:** light / medium / heavy classes (speed vs armor vs ram); armor bar; wrecked → quick
  respawn behind; visible damage (panels fly off).
- **Pickups** from item boxes, position-weighted odds: Nailer (nail stream, tire puncture slow),
  Nail Bomb (spike ring hazard), Railspike (become a giant auto-piloted nail, hammer shockwave exit —
  Bullet Bill homage), Doom Orb (slow plasma orb seeks leader, arcs lightning on passing cars — BFG +
  blue shell), Rivet Rail (instant beam skill shot), Scrap Rocket (homing, LOS-break), Harpoon
  (hook and slingshot past / yank into wall), Overdrive (4× weapon damage — Quad homage), Scrap
  Plating (armor).
- **Quake track features:** jump pads, teleporter gates, secret routes, shootable switches.
- **Modes:** Combat race; Time trial + ghosts (deterministic replay); Elimination (last place out
  each lap).
- **Tracks v1:** Castle Grindstone (Wolfenstein homage — stone fortress, push-wall secret route;
  no real-world extremist symbols, original emblems only), Refinery Gate (Doom/Quake — moon base
  that a teleporter rips into a lava hellscape with jump pads), Skyway Zero (F-Zero/Mario Kart/
  Excite Truck — floating neon circuit, boost strips, loops, sky ribbon).
  Later: Duke-style city rooftops/neon strip, Unreal-style floating sky islands, Excite-style dirt
  ridge course, arena maps.

### Assets & tools Claude can't do alone
- **Codex CLI 0.167 is installed with `image_generation` enabled** → drive via `codex exec` for
  textures (albedo; derive normal/roughness offline), decals, skyboxes, UI art, concept art, logos.
  Verify output licensing/terms and tile-ability per asset.
- **3D: Tripo** (within the user's standing credit-cap rule) for vehicles and props → Blender cleanup
  via blender-mcp (retopo, LODs, UVs, collision proxies).
- **Audio:** procedural SFX synthesized in `rivet-audio` + CC0 libraries; music deferred.

### Performance discipline
- Frame-time budget on UHD 730 tracked from milestone 1 (on-screen CPU/GPU frame graph, Tracy).
- Perf numbers are measured on the real iGPU (force adapter selection on this hybrid PC); never
  claim a target without a capture.

### Connectivity (user decision)
- v1: **Tailscale + direct IP.** Tailnet peer discovery (local Tailscale client's peer list, probe
  game port); direct `ip:port` for others (host forwards a UDP port). "Invite code" = encoded address.
- Friends outside the tailnet: recommend **Tailscale node sharing** (they install Tailscale, accept the
  shared host; UDP works, nothing to host).
- **Tailscale Funnel** is TCP/HTTPS only (no UDP), so it can't carry the real-time game traffic
  without TCP head-of-line stutter. Optional later use: expose a lobby/invite HTTP endpoint through it.
  (Re-verify Funnel's UDP status against current Tailscale docs when implementing.)
- Keep a transport seam in `rivet-net` so **Steam networking** can slot in later.

### Release
GitHub Releases on the loopback-games repo (Linux tarball, Windows zip). No store work in v1.

### Scope: vertical slice first (user decision)
Quality over speed, but get something drivable fast — aim for a gray-box drivable build in the first
session or two, a playable slice within a few days. **Slice =** one track (Refinery Gate or a gray-box
version of it), Combat race, 3 weapons (Nailer, Nail Bomb, Doom Orb), bots, online play over
Tailscale/IP, polished feel. Then grow to 3 tracks, all pickups, Time trial + ghosts, Elimination.

### Asset budget
Tripo cap **800 credits** for this project, spend tracked in a ledger, no per-call approval.

### Calls made by Claude (check these)
- **Ghosts store sampled transforms, not inputs**, so they're shareable across machines/OSes/GPUs
  without cross-platform bit-exact physics. Physics uses normal f32, deterministic only within one
  build/machine (enough for client prediction replay). Avoids fixed-point math everywhere.
- **Quality gates:** art milestones stop for the user's screenshot sign-off; frame-time gates are
  checked automatically from captures on the UHD 730.
- **ThinkPad:** i7-8550U / UHD 620 — weaker than the UHD 730, so it is now the floor and the Low
  preset is tuned for it.

## Left to the builder
Exact crate choices beyond the named foundation (after vetting), ECS vs plain data layout, snapshot
format, tick rate, shading model details, asset pack format, preset contents, CI details, art
style bible.

## Suggested milestone order (refined in the plan)
M0 repo + tooling + CI skeleton → M1 car drives on a gray-box loop on UHD 730 with frame graph →
M2 deterministic physics + time trial + ghosts → M3 client/server netcode, prediction, dedicated
server, Tailscale discovery → M4 weapons + combat race + bots → M5 renderer push (lightmaps, probes,
clustered lights, post) → M6 three tracks with art pipeline (Codex/Tripo/Blender) + Elimination →
M7 Windows polish, presets, ThinkPad tuning.

## Next steps after approval
1. Create local repo `rivet/`, commit this design as
   `docs/superpowers/specs/2026-11-25-rivet-nailrunners-design.md`.
2. Write the implementation plan for the vertical slice (superpowers:writing-plans), starting with
   M0 tooling and M1 "drive a gray-box loop on UHD 730 with frame graph".
3. Save a project memory for Rivet/Nailrunners decisions.

## Verification (per milestone)
`just ci` green on Linux + Windows CI; frame-time capture on UHD 730 attached to the milestone;
2-client + dedicated-server playtest over Tailscale for net milestones; screenshot-based quality
gate review of art.
