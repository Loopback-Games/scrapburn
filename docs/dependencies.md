# Dependencies

Every third-party crate is vetted against the project rules: 1000+ GitHub stars, a commit
within 6 months, not archived or deprecated, license compatible with MIT OR Apache-2.0.

| Crate | Why | Notes |
|---|---|---|
| wgpu | Graphics on Vulkan/DX12/Metal | gfx-rs, MIT/Apache |
| winit | Windows and input | rust-windowing, Apache-2.0 |
| glam | Math | MIT/Apache |
| bytemuck | GPU buffer casting | Zlib/MIT/Apache |
| pollster | Blocking on wgpu setup futures | MIT/Apache |
| log, env_logger | Logging | rust-lang/rust-cli, MIT/Apache |
| anyhow | App-level errors | MIT/Apache |
| thiserror | Library error types | dtolnay, MIT/Apache |
| cargo-deny (tool) | License/ban/source checks in `just security` | EmbarkStudios, MIT/Apache |
| image | PNG/JPEG decoding (png + jpeg features only) | image-rs, 5.9k stars, MIT/Apache |
| serde_json | glTF (GLB) JSON chunk parsing | serde-rs, MIT/Apache |
| fontdue | TrueType HUD text rasterization | mooman219, 1.7k stars, MIT/Apache |
| cpal | Audio output | RustAudio, ~3k stars, Apache-2.0; Linux needs ALSA headers (Homebrew alsa-lib locally, libasound2-dev in CI) |
| cargo-audit (tool) | `just security` | RustSec (Rust working group), first-party carve-out |

## Exceptions

### gilrs (gamepads) — approved by the project owner 2026-11-25

Fails the 1000-star rule only because it is hosted on GitLab (gitlab.com/gilrs-project/gilrs,
~90 stars). Otherwise it clears the bar: ~10.5M downloads, latest release 0.11.2 (2026-05-30),
repository activity 2026-11-23, MIT/Apache, and it is the gamepad backend used by Bevy.

Linux builds need libudev headers. On Bluefin they come from the keg-only Homebrew `systemd`
formula (wired into `PKG_CONFIG_PATH` in `mise.toml`); CI installs `libudev-dev`. The binary
links the system `libudev.so.1` at runtime.
