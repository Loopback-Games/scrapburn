# M3: Complete the Game — Plan

> User goal (2026-11-26): "complete the game. Make sure it's polished and looking like a AAA
> title." Executed inline; design calls delegated. Merge each reviewed chunk back into `main`.

## Remaining scope vs the spec
1. **Polish quick wins** — nails stop at barriers; cap stacked nail slowdown; Escape/Start on
   title and results; prop shadows not frustum-popping; HUD font upgrade (TTF via a vetted crate
   and an OFL font) so the HUD reads AAA.
2. **Elimination mode** — last place is eliminated at the end of each lap after lap 1; title menu
   entry; results show elimination order.
3. **Track system + tracks 2 and 3** — generalize `build_graybox_loop` into `TrackDef` (control
   points, banking, jumps, pads, box rows, theme: terrain shape, materials, props, lighting, sky).
   - Castle Grindstone (Wolfenstein homage, original): stone fortress walls, towers, gate tunnel,
     courtyard jump, banners with original emblems (no real-world symbols), dusk lighting.
   - Skyway Zero (F-Zero / Mario Kart / Excite Truck homage): floating neon circuit over a city at
     night, emissive edge rails, boost strips, banked loops, starry sky.
   - Track select menu; per-track ghosts.
4. **Online multiplayer** — `rivet-net`: UDP, server-authoritative race (host or `--server`),
   clients send inputs at tick rate, server sends snapshots at 30 Hz, clients interpolate remote
   karts; join by IP:port (works over Tailscale); lobby in the menu (Host / Join IP); headless
   loopback tests.
5. **Windows** — `cargo check --target x86_64-pc-windows-gnu` (or msvc) clean; CI matrix builds.
6. **Final pass** — perf on Low/Medium, screenshots, README, review, merge.

## Rules
- TDD for logic; screenshots (`--bench N --offscreen --screenshot`) for visuals.
- Lint before every commit. Merge reviewed chunks to local `main`. Never push.
- The user playtests on this machine: avoid opening windows / heavy benches while they play.
