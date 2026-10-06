export CRACKER_DATA_DIR := ".data"

dev:
    cargo run --locked

build:
    cargo build

release:
    cargo build --release

test:
    cargo test

lint:
    cargo clippy --all-targets --all-features -- -D warnings

fmt:
    cargo fmt

check:
    cargo fmt --check
    cargo check --locked
    cargo clippy --locked --all-targets --all-features -- -D warnings
    cargo test --locked --all-features
