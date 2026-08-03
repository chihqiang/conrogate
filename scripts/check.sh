#!/usr/bin/env bash
set -euo pipefail

# Conrogate 本地 CI 检查脚本
# 等价于 GitHub Actions 中的质量门禁，可在本地预跑

echo "=== cargo fmt ==="
cargo fmt --all -- --check

echo "=== cargo clippy ==="
cargo clippy --all-targets --all-features -- -D warnings

echo "=== cargo deny (advisories + licenses + bans + sources) ==="
if command -v cargo-deny &>/dev/null; then
    cargo deny check
else
    echo "  [SKIP] cargo-deny not installed. Install with: cargo install cargo-deny"
fi

echo "=== cargo build (all targets) ==="
cargo build --workspace --all-targets --all-features

echo "=== cargo test (unit + integration, all features) ==="
cargo test --workspace --all-features

echo "=== cargo doc ==="
if RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace --all-features 2>&1; then
    echo "  doc build OK"
else
    echo "  [WARN] cargo doc produced warnings (non-fatal in local CI)"
fi

echo "=== cargo machete (unused dependency detection) ==="
if command -v cargo-machete &>/dev/null; then
    cargo machete --workspace
else
    echo "  [SKIP] cargo-machete not installed. Install with: cargo install cargo-machete"
fi

echo "=== all checks passed ==="
