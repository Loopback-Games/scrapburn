# Rivet

An original Rust game engine inspired by the designs of Quake 3 and friends, and
**Nailrunners**, the combat kart racer built on it. Rivet contains no id Software code.

## Quick start

```sh
just setup   # installs the pinned toolchain via mise
just run     # drive the gray-box loop
just bench   # 30 s automated benchmark run
```

Useful flags: `--adapter auto|integrated|discrete|<name>` (or `RIVET_ADAPTER`),
`--size 1600x900`, `--present vsync|immediate`.

`auto` (the default) uses the GPU that drives your display. A GPU that can render but has no
monitor attached (for example the iGPU on a desktop whose monitor is on the dGPU) can still be
benchmarked offscreen:

```sh
just bench --adapter integrated --offscreen --screenshot shot.png
```

Controls: W/S or triggers to drive and brake, A/D or left stick to steer, Space/Shift or a
bumper to hop and drift (hold through a turn, release for a boost), R or B to respawn,
F3 or Select for the frame graph, Esc to quit.

Linux builds need libudev headers (`libudev-dev` / `systemd-devel`, or Homebrew `systemd`).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
