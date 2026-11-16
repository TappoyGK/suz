#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
fail() { printf '%s\n' "$1" >&2; exit 2; }
usage='Usage: build-c.sh INPUT.c -o OUTPUT [-O0|-O2] [--env ENV.c]'
[[ $# -ge 1 ]] || fail "$usage"
source_file="$1"
shift
[[ "$source_file" != -* ]] || fail "$usage"
output_file=''
optimization='-O0'
optimization_set=false
environment_file="$repo_dir/runtime/suz_env.c"
environment_set=false
while [[ $# -gt 0 ]]; do
    case "$1" in
        -o)
            [[ $# -ge 2 && -z "$output_file" ]] || fail "$usage"
            output_file="$2"
            shift 2
            ;;
        -O0|-O2)
            [[ "$optimization_set" == false ]] || fail 'Duplicate optimization option.'
            optimization="$1"
            optimization_set=true
            shift
            ;;
        --env)
            [[ $# -ge 2 && "$environment_set" == false ]] || fail "$usage"
            [[ "$2" != -* ]] || fail "$usage"
            environment_file="$2"
            environment_set=true
            shift 2
            ;;
        *) fail "Unsupported C build option: $1 [tool.build-settings]" ;;
    esac
done
[[ -n "$output_file" ]] || fail "$usage"
[[ ! -e "$output_file" && ! -L "$output_file" ]] || fail 'Output path already exists.'
for variable in CFLAGS CPPFLAGS LDFLAGS; do
    [[ -z "${!variable:-}" ]] || fail "$variable is not accepted; use SUZ_CFLAGS and -O0 or -O2 [tool.build-settings]"
done
# シェルの展開は行わない。改行も区切りとし、各項目を独立した引数として保持する。
extra_flags=()
while IFS= read -r line; do
    IFS=$' \t\r\v\f' read -r -a words <<< "$line"
    for flag in "${words[@]+"${words[@]}"}"; do
        [[ -n "$flag" ]] || continue
        allowed=false
        while IFS= read -r entry; do
            [[ -n "$entry" && "$entry" != \#* ]] || continue
            if [[ "$entry" == *= ]]; then
                [[ "$flag" == "$entry"* && "$flag" != "$entry" ]] || continue
                if [[ "$entry" == '-fsanitize=' ]]; then
                    value="${flag#*=}"
                    [[ "$value" =~ ^(address|undefined)(,(address|undefined))*$ ]] || continue
                fi
                allowed=true
            elif [[ "$flag" == "$entry" ]]; then
                allowed=true
            fi
        done < "$repo_dir/scripts/cflags.txt"
        [[ "$allowed" == true ]] || fail "Unsupported SUZ_CFLAGS flag: $flag [tool.extra-flags]"
        extra_flags+=("$flag")
    done
done <<< "${SUZ_CFLAGS:-}"
# 相対ファイル名が「@」で始まっても、パスとして渡す。
[[ "$source_file" == /* ]] || source_file="./$source_file"
[[ "$environment_file" == /* ]] || environment_file="./$environment_file"
[[ "$output_file" == /* ]] || output_file="./$output_file"
runtime="$repo_dir/runtime"
exec "${CC:-clang}" "${extra_flags[@]+"${extra_flags[@]}"}" -std=c23 -Wall -Wextra -Werror "$optimization" \
    -fhonor-nans -fhonor-infinities -ffp-contract=off \
    -fdenormal-fp-math=ieee \
    -I "$runtime" -I "$runtime/third_party/ryu" \
    "$source_file" "$runtime/suz_runtime.c" "$runtime/suz_text.c" "$runtime/suz_parse.c" "$runtime/suz_parse_float.c" "$runtime/suz_parse_struct.c" "$environment_file" "$runtime/suz_float.c" "$runtime/suz_format_float.c" \
    "$runtime/third_party/ryu/ryu/f2s.c" "$runtime/third_party/ryu/ryu/d2s.c" \
    -lm -o "$output_file"
