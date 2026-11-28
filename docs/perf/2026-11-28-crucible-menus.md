# Crucible menus benchmark, November 28, 2026

The Crucible menu redesign (branch `feat/crucible-menus`, based on `55dc92e`) was compared with `main` at `55dc92e`. Both release builds rendered offscreen at 1600×900 on an Intel UHD Graphics 730 using Vulkan and Mesa 26.2.7, with the Low preset and the integrated adapter. Each run used a three-second warmup and a 20-second measurement window, and each case ran twice. The title screen (attract mode) and a race on Dustbowl Loop with the autopilot driving were measured.

The machine's GPU is a UHD 730, which is faster than the UHD 620 that Low targets. Treat the numbers as a relative comparison, not as the minimum-spec result.

| Case | Build | Frames | Average frame time (ms) | 99th percentile frame time (ms) | Average GPU time (ms) |
|---|---|---:|---:|---:|---:|
| Title screen | `main` | 3,737 / 3,730 | 5.35 / 5.36 | 5.72 / 5.75 | 5.02 / 5.03 |
| Title screen | Crucible menus | 3,158 / 3,154 | 6.33 / 6.34 | 6.69 / 6.73 | 5.17 / 5.18 |
| Race | `main` | 3,667 / 3,668 | 5.45 / 5.45 | 6.07 / 6.09 | 5.30 / 5.29 |
| Race | Crucible menus | 3,664 / 3,663 | 5.46 / 5.46 | 6.09 / 6.15 | 5.30 / 5.30 |

Each cell shows run one, then run two.

- The race is unchanged. The menu code does not run during a race, and frame and GPU times match `main` within 0.01 ms.
- The title screen is about 0.15 ms slower on the GPU (5.17 ms against 5.02 ms) and about 1.0 ms slower per frame (6.33 ms against 5.35 ms, or 158 fps against 187 fps). The extra GPU time is the frosted backdrop and the larger menu overlay. The rest of the gap is CPU work that the benchmark does not break out, most likely building the new menu geometry each frame. Both builds stay well inside a 60 fps budget.

Run from the repository root in a graphical session. Each run prints a summary and writes per-frame measurements to `target/bench/`.

```sh
just run --bench 20 --attract --size 1600x900 --preset low --adapter integrated --offscreen
just run --bench 20 --size 1600x900 --preset low --adapter integrated --offscreen
```

To measure `main`, run the same commands from a worktree of that commit.
