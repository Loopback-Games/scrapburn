# M1 gray-box benchmark — 2026-11-25

Commit `ee35737`. Scene: gray-box loop (~1.1 km road, walls, ground plane, one kart), autopilot
driving, 3 s warmup then 30 s measured, 1600×900. Two runs per GPU.

**Gate (UHD 730): average ≥ 60 fps and p99 ≤ 16.7 ms — PASS.**

| GPU | Present | Frames | Avg ms | p50 ms | p99 ms | Max ms | FPS | GPU avg ms |
|---|---|---|---|---|---|---|---|---|
| Intel UHD Graphics 730 (ADL-S GT1), Mesa 26.2.6 | Offscreen | 85,436 | 0.35 | 0.35 | 0.38 | 0.50 | 2848 | 0.27 |
| Intel UHD Graphics 730 (ADL-S GT1), Mesa 26.2.6 | Offscreen | 85,415 | 0.35 | 0.35 | 0.38 | 1.48 | 2847 | 0.27 |
| NVIDIA GeForce RTX 3060, driver 615.78.08 | Mailbox (window) | 232,855 | 0.13 | 0.13 | 0.16 | 14.39 | 7762 | 0.02 |
| NVIDIA GeForce RTX 3060, driver 615.78.08 | Mailbox (window) | 227,866 | 0.13 | 0.13 | 0.17 | 14.60 | 7596 | 0.02 |

## Caveats

- **The UHD 730 ran offscreen.** On this desktop the monitor is attached to the RTX 3060, so the
  iGPU cannot present (X11 swapchain creation fails; GNOME Wayland rejects its buffers). The
  offscreen path renders the identical frame into a hidden texture with at most two frames in
  flight, so GPU and CPU cost are real; only presentation/compositing is missing. A run on a
  machine whose display hangs off the iGPU (the ThinkPad) will confirm windowed numbers.
- The gray-box scene is trivially light. The gate becomes meaningful once real art and the
  renderer push (M5) land; this run proves the frame loop, timing, and measurement path.
- The RTX 3060's ~14 ms max is a single startup-adjacent hitch per run (window compositor),
  not sustained; p99 stays at 0.16 ms.

Reproduce:

```sh
just bench --adapter integrated --offscreen
just bench --adapter discrete
```
