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

bench *ARGS:
    cargo run -p scrapburn --release -- --bench 30 {{ARGS}}

clean:
    cargo clean

ci: lint test security build
