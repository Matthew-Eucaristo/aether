#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --release --workspace --all-targets -- -D warnings
cargo test --release --workspace
node scripts/viewer-check.mjs target/release/aether-server
