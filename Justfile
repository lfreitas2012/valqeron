# Show available commands
default:
    @just --list

# Run all tests
test:
    cargo install cargo-llvm-cov --locked
    cargo llvm-cov

# Run all loom tests
test-loom:
    RUSTFLAGS="--cfg loom" cargo test -p valqeron-infrastructure --lib loom_tests

# Check Rust code formatting without modifying files
format-check:
    cargo fmt --all -- --check

# Format all Rust code
format:
    cargo fmt --all

# Run Clippy lints
lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Assert async containment: core and infrastructure must stay tokio/tonic-free.
deps-check:
    #!/usr/bin/env bash
    set -euo pipefail
    for crate in valqeron-core valqeron-infrastructure; do
        for dep in tokio tonic; do
            if cargo tree -p "$crate" -e normal | grep -q " ${dep} v"; then
                echo "FAIL: ${crate} depends on ${dep}" >&2
                exit 1
            fi
        done
        echo "OK: ${crate} is async-free"
    done

# Run every fuzz target for the requested bounded time.
fuzz-all:
    just --justfile crates/identifiers/Justfile fuzz-all
