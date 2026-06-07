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
    printf 'nros-workspace-fixture-v1\0%s\0' "$record"
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
