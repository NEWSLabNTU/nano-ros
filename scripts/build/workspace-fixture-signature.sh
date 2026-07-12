#!/usr/bin/env bash
# Print the build-input signature for one workspace fixture manifest record.
set -euo pipefail

record="${1:?usage: workspace-fixture-signature.sh <manifest-record>}"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"

IFS=$'\x1f' read -r id _lang dir _bringup _entry _build_subdir _target_dir _codegen_out _defs <<< "$record"
[ -n "$id" ] && [ -n "$dir" ] || {
    echo "workspace fixture record is missing id/dir" >&2
    exit 2
}

workspace="$repo_root/$dir"
[ -d "$workspace" ] || {
    echo "workspace fixture '$id' dir does not exist: $dir" >&2
    exit 2
}

{
    printf 'nros-workspace-fixture-v2\0%s\0' "$record"
    # #182 — the fixture is a function of the CODEGEN TOOL, not just the
    # workspace sources: `nros codegen entry` emits the entry TU, `nros ws
    # sync`/`generate-*` shape the msg crates. A signature blind to the tool
    # let a fixture built with a pre-fd32a0f75 emitter verify as "fresh"
    # (realtime tier lanes ran museum TUs with correct-looking sources).
    # Hash the CLI binary's content into the signature; absent binary hashes
    # as the literal marker (the build script builds it before stamping).
    nros_bin="$repo_root/packages/cli/target/release/nros"
    if [ -x "$nros_bin" ]; then
        printf 'tool:nros\0'
        sha256sum "$nros_bin" | awk '{printf "%s", $1}'
        printf '\0'
    else
        printf 'tool:nros-absent\0'
    fi
    find "$workspace" \
        \( -name target -o -name 'target-*' -o -name build -o -name 'build-*' -o -name generated \) \
        -prune -o -type f -print0 \
        | sort -z \
        | while IFS= read -r -d '' file; do
            rel="${file#$repo_root/}"
            case "$rel" in
                *.c|*.cc|*.cpp|*.h|*.hpp|*.rs|*.toml|*.xml|*.lock|*/CMakeLists.txt|*/package.xml|*.cmake)
                    printf 'path:%s\0' "$rel"
                    cat "$file"
                    printf '\0'
                    ;;
            esac
        done
} | sha256sum | awk '{print $1}'
