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

# Regenerate docs/cli.md from the CLI definition
cli-doc:
    ORCHY_WRITE_CLI_REFERENCE=1 cargo test -p orchy-cli the_command_reference_matches_the_cli

orchy *args:
    cargo run -p orchy-cli -- {{args}}

t pattern:
    cargo test --workspace {{pattern}} -- --nocapture
