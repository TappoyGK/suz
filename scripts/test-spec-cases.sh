#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_dir"

# Run the full catalog, or the IDs selected through SUZ_CASES.
exec cargo test "$@" --test spec_cases execute_spec_cases -- --nocapture
