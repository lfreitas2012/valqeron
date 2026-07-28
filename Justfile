# Show available commands
default:
    @just --list

# Run all tests
test:
    cargo install cargo-llvm-cov --locked
    cargo llvm-cov

# Check Rust code formatting without modifying files
format-check:
    cargo fmt --all -- --check

# Format all Rust code
format:
    cargo fmt --all

# Run Clippy lints
lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings