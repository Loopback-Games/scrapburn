# Circuit scenery benchmark, November 27, 2026

At commit `3b12caa`, each circuit ran once with eight autopilot karts, scenery enabled, and the Medium preset at 1600×900. The release build rendered offscreen on an Intel UHD Graphics 730 using Vulkan and Mesa 26.2.7. Each run used a three-second warmup and a 30-second measurement window.

| Circuit | Frames | Average FPS | Average frame time (ms) | 99th percentile frame time (ms) | Average GPU time (ms) |
|---|---:|---:|---:|---:|---:|
| Dustbowl Loop | 3,929 | 131.0 | 7.64 | 8.98 | 7.48 |
| Castle Grindstone | 4,172 | 139.1 | 7.19 | 8.08 | 7.03 |
| Skyway Zero | 5,252 | 175.1 | 5.71 | 6.34 | 5.56 |

Run from the repository root in a graphical session. Each run prints a summary and writes per-frame measurements to `target/bench/`.

```sh
for track in dustbowl castle skyway; do
    just bench --track "$track" --mode race --size 1600x900 --preset medium --adapter integrated --offscreen
done
```
