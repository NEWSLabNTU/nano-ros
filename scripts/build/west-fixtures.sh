#!/usr/bin/env bash
# Build-stage zephyr west fixtures (issue 0041 — No compilation inside tests).
# `west build` a zephyr bringup fixture into build/west-fixtures/<id>/, stamping
# `.compile-ok`. Tests inspect the build dir (baked artifacts / CMakeCache /
# zephyr.exe) instead of running west at run time.
#
# Gated: skips cleanly (no stamp → test skips/deselects per tier) when west or a
# provisioned Zephyr workspace is unavailable.
set -u

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
cd "$repo_root"

out_root="$repo_root/build/west-fixtures"
mkdir -p "$out_root"

# ZEPHYR_BASE: prefer the provisioned workspace under the repo.
if [ -z "${ZEPHYR_BASE:-}" ] && [ -d "$repo_root/zephyr-workspace/zephyr" ]; then
    export ZEPHYR_BASE="$repo_root/zephyr-workspace/zephyr"
fi

if ! command -v west >/dev/null 2>&1; then
    echo "west-fixtures: west unavailable — skipping" >&2
    exit 0
fi
if [ -z "${ZEPHYR_BASE:-}" ] || [ ! -d "$ZEPHYR_BASE" ]; then
    echo "west-fixtures: ZEPHYR_BASE unset/invalid — skipping" >&2
    exit 0
fi

# id : src-rel : app-subdir ('.' = src root) : board ('' = from board.cmake) : extra-cmake-args
WEST_FIXTURES=(
    "west_bringup_zephyr:packages/testing/nros-tests/fixtures/multi_pkg_workspace_zephyr:zephyr_app:native_sim/native/64:-DCONF_FILE=prj.conf;prj-zenoh.conf"
    "west_board_import:packages/testing/nros-tests/fixtures/board_import_fvp:.::"
)

n=0
for entry in "${WEST_FIXTURES[@]}"; do
    IFS=':' read -r id src subdir board extra <<< "$entry"
    [ -d "$repo_root/$src" ] || { echo "west-fixtures: src missing: $src" >&2; continue; }
    bld="$out_root/$id"
    echo "== west-fixture: $id (board=${board:-board.cmake}) =="
    rm -rf "$bld"
    args=(build -d "$bld")
    [ -n "$board" ] && args+=(-b "$board")
    args+=("$repo_root/$src/$subdir")
    [ -n "$extra" ] && args+=(-- "$extra")
    if west "${args[@]}"; then
        date -u +%Y-%m-%dT%H:%M:%SZ > "$bld/.compile-ok"
        echo "   built $bld"
        n=$((n + 1))
    else
        echo "   west build failed for $id (no stamp; test will report)" >&2
    fi
done
echo "west fixtures built ($n/${#WEST_FIXTURES[@]})."
