set shell := ["sh", "-cu"]

# Format the Python sources.
fmt:
    uvx ruff format

# Check formatting as CI does.
fmt-check:
    uvx ruff format --check

# Run the CI linter.
lint:
    uvx ruff check
    uvx --with pytest ty check

# Run the test suite.
test:
    uv run --no-project --with pytest pytest
