# M2: Combat Race Slice — Implementation Plan

> Executed inline. The user asked to keep going and be impressed; design calls are delegated.

**Goal:** A full combat race on the desert loop: the player against 7 AI rivals over 3 laps, with
a countdown, live positions, kart-to-kart bumping, item boxes and three weapons (Nailer, Nail Bomb,
Doom Orb), procedural sound, a minimap and a results screen.

**Spec:** `docs/superpowers/specs/2026-11-25-rivet-nailrunners-design.md` (vertical slice: one
track, combat race, 3 weapons, bots).

## Global constraints
- Perf: UHD 730 offscreen, Medium 1600×900, 8 karts + effects: GPU ≤ 6.5 ms avg, p99 ≤ 9 ms.
- Determinism: race simulation steps in a fixed order (racers by index), no HashMap iteration.
- New deps vetted: `cpal` (RustAudio, audio output). Linux needs ALSA headers (Homebrew
  `alsa-lib` locally, `libasound2-dev` in CI).
- TDD for logic: race standings, countdown, kart collisions, bot steering/skill/rubber band, item
  odds, projectiles and hits, audio synthesis (sample-level tests). Screenshots for visuals.

## Design calls
- **Racers:** 8 karts; player starts 8th on a 2×4 grid behind the line. Bots are tinted variants
  of the scrap kart (8 palette tints).
- **Kart collisions:** sphere vs sphere (radius 1.0), resolved by mass (medium 300 kg; bots vary
  ±15%) with 0.3 restitution; heavier karts shove lighter ones.
- **Standings:** progress = laps × N + checkpoints passed + fraction toward the next checkpoint.
- **Race flow:** 3 s countdown (karts frozen; holding throttle on "GO" gives a start boost),
  3 laps, finish order, results panel; bench mode = all-bot race with the camera on racer 0.
- **Bots:** follow the centerline with a personal lane offset, look-ahead scaled by speed, drift
  through corners (autopilot logic), skill 0.86–0.98 of top speed, catch-up rubber band (±6% by
  distance to the player), steer around a kart ahead, fire items when a target is in front or
  drop bombs when someone is close behind.
- **Items:** boxes in rows of 4 at 3 places; respawn after 3 s; position-weighted odds (leaders
  get the Nailer, the middle the Harpoon, the back of the pack gets the Doom Orb more often).
  - Nailer: 12 nails over 0.6 s forward; each hit = small spin-out (0.6 s, speed ×0.75).
  - Harpoon (replaces the Nail Bomb, which was too close to the Nailer — user call): fires a
    chained hook at the nearest kart ahead within 70 m in a forward cone; on hit it reels the
    shooter in (boost toward the target) for up to 1.2 s, then releases with a slingshot boost
    while yanking the target (speed ×0.6). Misses retract after 0.5 s.
  - Doom Orb: slow green orb homes on the leader along the track; arcs lightning at karts within
    8 m it passes (small slow); detonates on the leader (big spin-out 1.6 s, launch).
- **Spin-out:** physics state; steering/throttle ignored, yaw spin, speed scrub.
- **Sound:** `rivet-audio` crate, cpal output, a lock-free command queue to a mixer thread;
  procedural voices: engine (sawtooth + noise, pitch by speed, per kart, distance-attenuated),
  drift squeal, boost whoosh, item pickup chime, nail shots, bomb, orb hum and blast, countdown
  beeps. No audio device → silent, game keeps running.
