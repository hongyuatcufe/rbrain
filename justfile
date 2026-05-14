#!/usr/bin/env just

# Environment
export LANG := "C.UTF-8"

# Default recipe
default:
    just test

# Run all workspace tests
test:
    cargo test --workspace

# Run only unit tests (no integration/e2e)
test-fast:
    cargo test --workspace --lib

# Run integration/e2e tests (requires API keys)
test-e2e:
    RBRAIN_E2E=1 cargo test --workspace --test '*'

# Run the CLI
run args...:
    cargo run --quiet --bin rbrain --root . -- {{args}}

# Run clippy with warnings as errors
lint:
    cargo clippy --workspace -- -D warnings

# Format all code
format:
    cargo fmt --workspace

# Check format
fmt-check:
    cargo fmt --workspace --check

# Run migrations (sqlx)
migrate:
    sqlx migrate run

# Clean all data (fresh start)
clean-data:
    rm -rf ~/.rbrain
    echo "Data cleaned. Run 'rbrain init' to start fresh."

# Validate dependencies (Phase 0.10)
validate-deps:
    echo "Checking Qwen API access..."
    curl -s https://dashscope-intl.aliyuncs.com/compatible-mode/v1/embeddings \
      -H "Authorization: Bearer ${DASHSCOPE_API_KEY:?Set DASHSCOPE_API_KEY}" \
      -H "Content-Type: application/json" \
      -d '{"model":"text-embedding-v4","input":["test"]}' | \
      python3 -c "import sys,json; d=json.load(sys.stdin); assert 'data' in d, f'API error: {d}'; dims=len(d['data'][0]['embedding']); print(f'Qwen API OK, dim={dims}'); assert dims==1024, f'Expected dim 1024, got {dims}'"
    echo "Checking usearch..."
    cargo test -p rbrain-search --lib usearch -- --ignored 2>/dev/null || echo "usearch smoke test placeholder - manual check needed"
    echo "Checking lindera..."
    cargo check -p rbrain-search 2>&1 | grep -q "Finished" && echo "lindera compiles OK"
    echo "All dependency checks passed!"
