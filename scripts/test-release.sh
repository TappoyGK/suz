#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_dir"

# リリース検査では、常に例集全体を検証する。
unset SUZ_CASES
cargo fmt --all -- --check
cargo clippy "$@" --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc "$@" --no-deps --document-private-items
cargo test "$@" --release -- --include-ignored --nocapture
