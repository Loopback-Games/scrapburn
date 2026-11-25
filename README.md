# Rivet

An original Rust game engine inspired by the designs of Quake 3 and friends, and
**Nailrunners**, the combat kart racer built on it. Rivet contains no id Software code.

## Quick start

```sh
just setup   # installs the pinned toolchain via mise
just run     # drive the gray-box loop
just bench   # 30 s automated benchmark run
```

Useful flags: `--adapter integrated|discrete|<name>` (or `RIVET_ADAPTER`),
`--size 1600x900`, `--present vsync|immediate`.

Linux builds need libudev headers (`libudev-dev` / `systemd-devel`, or Homebrew `systemd`).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
