#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_dir"

# A release check always covers the whole catalog.
unset SUZ_CASES
python3 scripts/test-implementation.py
python3 scripts/check-implementation.py
cargo fmt --all -- --check
cargo clippy "$@" --all-targets -- -D warnings
cargo test "$@" --release -- --include-ignored --nocapture
