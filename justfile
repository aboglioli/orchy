default:
    @just --list

fmt:
    cargo fmt --all

lint:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace --no-fail-fast

build:
    cargo build --workspace

check: fmt lint test

orchy *args:
    cargo run -p orchy-cli -- {{args}}

t pattern:
    cargo test --workspace {{pattern}} -- --nocapture
