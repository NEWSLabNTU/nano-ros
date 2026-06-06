#!/usr/bin/env bash
# Build workspace fixtures from examples/fixtures.toml.
#
# Workspace fixtures differ from single-node fixtures: each row is a complete
# workspace with Node packages, a Bringup package, and an Entry package. The
# build must follow the documented user workflow:
#
#   nros ws sync
#   nros codegen-system --bringup <bringup> --out <codegen_out>
#   cargo build -p <entry> ... OR cmake --build ... --target <entry>
#
# Usage, from anywhere in the repo checkout:
#   scripts/build/workspace-fixtures-build.sh <platform> [lang]
set -euo pipefail

platform="${1:?usage: workspace-fixtures-build.sh <platform> [lang]}"
lang_filter="${2:-}"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"

# shellcheck source=scripts/build/cargo.sh
source "$repo_root/scripts/build/cargo.sh"
# shellcheck source=scripts/build/cmake-incremental.sh
source "$repo_root/scripts/build/cmake-incremental.sh"

cd "$repo_root"

nros_cli="$(nros_cli_bin)"
nros_require_ws_sync "$nros_cli"

export NROS_REPO_DIR="${NROS_REPO_DIR:-$repo_root}"
export NROS_REPO_ROOT="${NROS_REPO_ROOT:-$repo_root}"
export NROS_CLI="$nros_cli"
export NROS_CLI_BIN="$nros_cli"

if [ "${NROS_JOBSERVER:-}" = "1" ]; then
    unset CMAKE_BUILD_PARALLEL_LEVEL
else
    export CMAKE_BUILD_PARALLEL_LEVEL="${CMAKE_BUILD_PARALLEL_LEVEL:-${NROS_BUILD_JOBS:-8}}"
fi

manifest() {
    python3 "$repo_root/scripts/build/fixtures-manifest.py" list-workspaces \
        --platform "$platform" ${lang_filter:+--lang "$lang_filter"}
}

profile_dir="$(nros_cargo_target_profile_dir)"
mapfile -t cargo_profile_args < <(nros_cargo_profile_args)

build_workspace() {
    local record="$1"
    local id lang dir bringup entry build_subdir target_dir codegen_out defs
    IFS=$'\x1f' read -r id lang dir bringup entry build_subdir target_dir codegen_out defs <<< "$record"

    [ -n "$id" ] || return 0
    [ -n "$dir" ] && [ -n "$bringup" ] && [ -n "$entry" ] || {
        echo "workspace fixture '$id' is missing dir/bringup/entry" >&2
        return 2
    }
    [ -d "$repo_root/$dir" ] || {
        echo "workspace fixture '$id' dir does not exist: $dir" >&2
        return 2
    }
    [ -n "$codegen_out" ] || {
        echo "workspace fixture '$id' is missing codegen_out" >&2
        return 2
    }

    echo "  -> $id ($lang) $dir"
    (
        cd "$repo_root/$dir"
        if [ "$lang" != "rust" ] && [ -n "$build_subdir" ] && \
           [ -f "$build_subdir/CMakeCache.txt" ] && \
           [ ! -f "$build_subdir/build.ninja" ] && \
           [ ! -f "$build_subdir/Makefile" ]; then
            case "$build_subdir" in
                ""|"."|"/")
                    echo "refusing to clean unsafe CMake build dir: '$build_subdir'" >&2
                    return 2
                    ;;
            esac
            echo "     removing incomplete CMake build dir: $build_subdir"
            rm -rf "$build_subdir"
        fi
        mkdir -p "$(dirname "$codegen_out")"

        echo "     nros ws sync"
        "$nros_cli" ws sync >/dev/null

        echo "     nros codegen-system --bringup $bringup --out $codegen_out"
        "$nros_cli" codegen-system --bringup "$bringup" --out "$codegen_out" >/dev/null

        if [ "$lang" = "rust" ]; then
            local cargo_args=(build "${cargo_profile_args[@]}" -p "$entry")
            if [ -n "$target_dir" ]; then
                cargo_args+=(--target-dir "$target_dir")
            fi
            echo "     cargo ${cargo_args[*]}"
            cargo "${cargo_args[@]}"

            local out_root="${target_dir:-target}"
            echo "     built: $dir/$out_root/$profile_dir/$entry"
        else
            [ -n "$build_subdir" ] || {
                echo "workspace fixture '$id' is missing build_subdir for CMake build" >&2
                return 2
            }
            local cmake_args=()
            if [ -n "$defs" ]; then
                read -r -a cmake_args <<< "$defs"
            fi
            cmake_args+=(
                "-DNROS_CLI_BIN=$nros_cli"
                "-D_NANO_ROS_CODEGEN_TOOL=$nros_cli"
            )

            echo "     cmake -S . -B $build_subdir ${cmake_args[*]}"
            nros_cmake_configure_if_needed . "$build_subdir" "${cmake_args[@]}"

            echo "     cmake --build $build_subdir --target $entry"
            cmake --build "$build_subdir" --target "$entry"
            local built_path
            built_path="$(find "$build_subdir" -type f -name "$entry" -perm -111 | sort | head -n 1 || true)"
            if [ -n "$built_path" ]; then
                echo "     built: $dir/$built_path"
            else
                echo "     built target: $entry under $dir/$build_subdir"
            fi
        fi
    )
}

found=0
while IFS= read -r record; do
    found=1
    build_workspace "$record"
done < <(manifest)

if [ "$found" = "0" ]; then
    echo "No workspace fixtures matched platform=$platform${lang_filter:+ lang=$lang_filter}."
fi
