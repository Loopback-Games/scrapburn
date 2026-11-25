# M1b feel and beauty benchmark — 2026-11-25

Commit `a1a34d6`. Scene: the dressed desert loop (terrain ~25k triangles, road/barriers, 110
rocks, 24k-triangle kart), autopilot driving with drifts, particles and boosts; 3 s warmup then 30 s
measured.

| GPU | Preset | Size | Present | Avg ms | p99 ms | FPS | GPU avg ms |
|---|---|---|---|---|---|---|---|
| Intel UHD 730, Mesa 26.2.6 | Low (75% scale) | 1920×1080 | Offscreen | 3.96 | 4.24 | 252 | 3.82 |
| Intel UHD 730, Mesa 26.2.6 | Medium | 1600×900 | Offscreen | 4.89 | 5.36 | 204 | 4.76 |
| Intel UHD 730, Mesa 26.2.6 | Medium | 1920×1080 | Offscreen | 6.55 | 7.11 | 153 | 6.42 |
| RTX 3060, driver 615.78.08 | High | 1920×1080 | Mailbox | 0.87 | 0.98 | 1153 | 0.79 |

Plan gate (UHD 730, Medium 1600×900: GPU ≤ 6.0 ms, p99 ≤ 8 ms): **PASS** (4.76 / 5.36).

**UHD 620 (ThinkPad, minimum spec) — estimate, not measured:** it is roughly half a UHD 730, so
Low at 1080p should land near 8 ms GPU (~120 fps) and Medium at 1080p near 13 ms (~75 fps). A real
windowed run on the ThinkPad will replace this estimate.

Caveats: the UHD 730 runs offscreen (its desktop's monitor is on the RTX 3060); the 3060's ~15 ms
max is a single compositor hitch.

Screenshots (High, 1920×1080): `docs/media/m1b-6.jpg`, `m1b-14.jpg`, `m1b-24.jpg`.
