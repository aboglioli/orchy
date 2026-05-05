export DOCKER_HOST := env_var_or_default("DOCKER_HOST", "unix:///run/user/" + env_var("UID") + "/podman/podman.sock")
export TESTCONTAINERS_RYUK_DISABLED := "true"

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

server *args:
    cargo run -p orchy-server -- {{args}}

cli *args:
    cargo run -p orchy-cli -- {{args}}

it-pg:
    cargo test -p orchy-store-pg --features integration-tests -- --test-threads=1

it-conformance:
    cargo test -p orchy-store-conformance --features integration-tests -- --test-threads=1

it-sqs:
    cargo test -p orchy-events-sqs --features integration-tests -- --test-threads=1

it-kafka:
    cargo test -p orchy-events-kafka --features integration-tests -- --test-threads=1

it: it-pg it-conformance it-sqs it-kafka

db-up:
    podman compose up -d postgres

db-down:
    podman compose down

t pattern:
    cargo test --workspace {{pattern}} -- --nocapture
