set windows-shell := ["pwsh", "-NoLogo", "-Command"]

default:
    @just --list

setup:
    mise install

fmt:
    cargo fmt --all

lint:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    actionlint
    zizmor --quiet .github/workflows

test:
    cargo test --workspace
    cd tools/tailcat && mise exec -- go test ./...

# Clippy findings and test coverage for SonarQube Cloud, written to target/sonar.
[unix]
sonar-reports:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p target/sonar
    cargo clippy --workspace --all-targets --message-format=json > target/sonar/clippy.json
    cargo llvm-cov --workspace --lcov --output-path target/sonar/lcov.info
    cd tools/tailcat
    mise exec -- go test -coverprofile=../../target/sonar/go-coverage.out ./...

security:
    cargo audit
    cargo deny check licenses bans sources

build:
    cargo build --workspace --release

run *ARGS:
    cargo run -p scrapburn --release -- {{ARGS}}

# Ship the game, private-race helper and assets together, as a .zip.
[linux]
package: build-portable
    python3 tools/package.py target/portable/release

# Ship the game, private-race helper and assets together.
[macos]
package: build
    python3 tools/package.py

# Ship the game, private-race helper and assets together.
[windows]
package: build
    python tools/package.py

# Release build against glibc 2.28 (AlmaLinux 8) in rootless podman, so the binary
# runs on older distributions than the build host.
[linux]
build-portable:
    #!/usr/bin/env bash
    set -euo pipefail
    rust="$(mise current rust)"
    image="localhost/scrapburn-linux-build:${rust}"
    podman build -q --build-arg RUST="${rust}" -t "${image}" tools/linux-build >/dev/null
    podman volume exists scrapburn-cargo-registry || podman volume create scrapburn-cargo-registry >/dev/null
    podman run --rm -v "$PWD":/src:z -v scrapburn-cargo-registry:/opt/cargo/registry \
        "${image}" cargo build -p scrapburn --release --target-dir target/portable

# Cross-compile a Windows x86_64 .zip from Linux, using MinGW-w64 in rootless podman.
[linux]
package-windows:
    #!/usr/bin/env bash
    set -euo pipefail
    rust="$(mise current rust)"
    image="localhost/scrapburn-windows-build:${rust}"
    podman build -q --build-arg RUST="${rust}" -t "${image}" tools/windows-build >/dev/null
    podman volume exists scrapburn-cargo-registry || podman volume create scrapburn-cargo-registry >/dev/null
    podman run --rm -v "$PWD":/src:z -v scrapburn-cargo-registry:/opt/cargo/registry \
        -e CARGO_PROFILE_RELEASE_STRIP=symbols \
        "${image}" cargo build -p scrapburn --release --target x86_64-pc-windows-gnu \
        --target-dir target/windows
    python3 tools/package.py target/windows/x86_64-pc-windows-gnu/release --windows

# Open a private lobby and print an invite.
[working-directory: 'tools/tailcat']
tailcat-host: build
    mise exec -- go run . host ../../target/release/scrapburn

# Paste a private invite and enter the host's lobby.
[working-directory: 'tools/tailcat']
tailcat-join: build
    mise exec -- go run . join ../../target/release/scrapburn

bench *ARGS:
    cargo run -p scrapburn --release -- --bench 30 {{ARGS}}

clean:
    cargo clean

ci: lint test security build
