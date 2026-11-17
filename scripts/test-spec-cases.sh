#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_dir"

# 例集全体、またはSUZ_CASESで選んだIDを実行する。
exec cargo test "$@" --test spec_cases execute_spec_cases -- --nocapture
