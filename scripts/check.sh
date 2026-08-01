#!/usr/bin/env bash
set -euo pipefail

echo "=== cargo fmt ==="
cargo fmt --all -- --check

echo "=== cargo clippy ==="
cargo clippy --all-targets --all-features -- -D warnings

echo "=== cargo build ==="
cargo build --workspace --all-targets

echo "=== cargo test ==="
cargo test --workspace

echo "=== all checks passed ==="
