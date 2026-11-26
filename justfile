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

test:
    cargo test --workspace

security:
    cargo audit
    cargo deny check licenses bans sources

build:
    cargo build --workspace --release

run *ARGS:
    cargo run -p scrapburn --release -- {{ARGS}}

# Release build plus the committed assets, as a tarball to hand to someone:
# target/dist/scrapburn-<version>-linux-x86_64.tar.gz
[unix]
package:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build -p scrapburn --release
    version="$(git describe --tags --always --dirty)"
    name="scrapburn-${version}-$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)"
    stage="target/dist/${name}"
    rm -rf "${stage}" "${stage}.tar.gz"
    mkdir -p "${stage}"
    cp target/release/scrapburn "${stage}/"
    strip "${stage}/scrapburn"
    # Only committed assets: the game never reads the raw Tripo downloads.
    git ls-files -z assets ':!assets/tripo-spend.tsv' | xargs -0 cp --parents -t "${stage}"
    cp LICENSE-APACHE LICENSE-MIT "${stage}/"
    tar -C target/dist -czf "${stage}.tar.gz" "${name}"
    rm -rf "${stage}"
    echo "${stage}.tar.gz"

bench *ARGS:
    cargo run -p scrapburn --release -- --bench 30 {{ARGS}}

clean:
    cargo clean

ci: lint test security build
