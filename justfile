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

# Ship the game, private-race helper and assets together.
package: build
    {{ if os() == "windows" { "python" } else { "python3" } }} tools/package.py

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
