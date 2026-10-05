set shell := ["sh", "-cu"]

# Format the Rust source.
fmt:
    cargo fmt

# Check formatting as CI does.
fmt-check:
    cargo fmt --check

# Run the CI linter.
lint:
    cargo clippy --all-targets -- -D warnings

# Run the test suite.
test:
    cargo test
