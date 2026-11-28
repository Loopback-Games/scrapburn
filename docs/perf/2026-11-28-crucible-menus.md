# Crucible menus benchmark, November 28, 2026

The Crucible menu redesign (branch `feat/crucible-menus`, based on `55dc92e`) was compared with `main` at `55dc92e`. Both release builds rendered offscreen at 1600×900 on an Intel UHD Graphics 730 using Vulkan and Mesa 26.2.7, with the Low preset and the integrated adapter. Each run used a three-second warmup and a 20-second measurement window, and each case ran twice. The title screen (attract mode) and a race on Dustbowl Loop with the autopilot driving were measured.

The machine's GPU is a UHD 730, which is faster than the UHD 620 that Low targets. Treat the numbers as a relative comparison, not as the minimum-spec result.

## What GPU time measures

GPU time runs from the first scene pass (shadows) to the end of the overlay pass that draws the HUD and the menus. Before commit `58a3893`, the timer stopped at the composite or FXAA pass, so the overlay pass was not timed at all. An earlier version of this note was measured that way and understated the menus' GPU cost.

For a like-for-like comparison, `main` was measured from a temporary worktree with the same timer change applied and not committed. Frame time is the wall-clock interval between frames. These offscreen runs are GPU-bound, so frame time follows GPU time closely.

## Results

| Case | Build | Frames | Average frame time (ms) | 99th percentile frame time (ms) | Average GPU time (ms) |
|---|---|---:|---:|---:|---:|
| Title screen | `main` | 3,735 / 3,730 | 5.36 / 5.36 | 5.69 / 5.76 | 5.26 / 5.27 |
| Title screen | Crucible menus before tuning (`58a3893`) | 3,149 / 3,152 | 6.35 / 6.34 | 6.70 / 6.72 | 6.19 / 6.18 |
| Title screen | Crucible menus (`f8b6f30`) | 3,460 / 3,455 | 5.78 / 5.79 | 6.16 / 6.17 | 5.61 / 5.61 |
| Race | `main` | 3,665 / 3,664 | 5.46 / 5.46 | 6.13 / 6.13 | 5.36 / 5.36 |
| Race | Crucible menus before tuning (`58a3893`) | 3,661 / 3,655 | 5.46 / 5.47 | 6.11 / 6.10 | 5.37 / 5.38 |
| Race | Crucible menus (`f8b6f30`) | 3,653 / 3,665 | 5.48 / 5.46 | 6.13 / 6.09 | 5.36 / 5.34 |

Each cell shows run one, then run two.

- The race is unchanged. The menu code does not run during a race, and frame and GPU times match `main` within 0.02 ms.
- The title screen costs 0.35 ms more GPU time than on `main` (5.61 ms against 5.26 ms). That is within the plan's budget of 0.5 ms more GPU time for menus on Low.
- Before tuning, the title screen cost 0.92 ms more. Nearly all of it was fill in the overlay pass. Short 8-second runs with parts of the menu switched off attributed about 0.72 ms to the menu overlay and about 0.15 ms to the frosted backdrop. That 0.15 ms covers the blur passes and the glass sampling them. The overlay's 0.72 ms broke down as follows:
  - About 0.48 ms for the column: smoke over the scene, the frosted glass and its tint, and the forge light, which together blended about 2.2 screens of pixels.
  - About 0.11 ms for the previews.
  - Under 0.1 ms each for text and embers.
- Tuning removed most of that fill. The smoke beside the column is now applied in the scene's composite pass, which already touches every pixel. The glass, its smoke and the forge light now fill the column once. The preview panels glaze their tint and skip the shadow under themselves. Menu text and embers were also made cheaper to build on the CPU, which does not show in these GPU-bound numbers.

## How to reproduce

Run from the repository root in a graphical session. Each run prints a summary and writes per-frame measurements to `target/bench/`.

```sh
just run --bench 20 --attract --size 1600x900 --preset low --adapter integrated --offscreen
just run --bench 20 --size 1600x900 --preset low --adapter integrated --offscreen
```

To measure `main`, run the same commands from a worktree of that commit. To compare GPU times fairly, also apply the timer change from `58a3893`: the begin timestamp goes on the first scene pass and the end timestamp on the HUD pass.
