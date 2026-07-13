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

# ZEPHYR_BASE: discover the provisioned west workspace the same way the
# `just zephyr` recipes resolve ZEPHYR_WORKSPACE (just/zephyr.just) — an explicit
# `NROS_ZEPHYR_WORKSPACE`, then the in-repo `zephyr-workspace/`, then the sibling
# `../nano-ros-workspace[-4.4]/` checkouts a `just zephyr setup` lands. Without
# this the fixture only saw the in-repo path and skipped whenever the workspace
# lived in the sibling (the common `just zephyr setup` layout).
if [ -z "${ZEPHYR_BASE:-}" ]; then
    for _ws in \
        "${NROS_ZEPHYR_WORKSPACE:-}" \
        "$repo_root/zephyr-workspace" \
        "$repo_root/../nano-ros-workspace" \
        "$repo_root/../nano-ros-workspace-4.4"; do
        if [ -n "$_ws" ] && [ -d "$_ws/zephyr" ]; then
            export ZEPHYR_BASE="$_ws/zephyr"
            break
        fi
    done
fi

if ! command -v west >/dev/null 2>&1; then
    echo "west-fixtures: west unavailable — skipping" >&2
    exit 0
fi
if [ -z "${ZEPHYR_BASE:-}" ] || [ ! -d "$ZEPHYR_BASE" ]; then
    echo "west-fixtures: ZEPHYR_BASE unset/invalid — skipping" >&2
    exit 0
fi

# #185 (the #182 guard, west edition) — every west fixture's bake runs the
# `nros` CLI (nros_system_generate / nros_generate_interfaces), so the fixture
# is a function of the CODEGEN TOOL. Stamp the CLI's content hash next to the
# date; `require_west_fixture` compares it and fails loud on a stale-tool
# fixture instead of soft-passing a museum bake (the #185 half-bake red
# herring). A date-only legacy stamp reads as stale — one rebuild refreshes.
west_fixture_stamp() {
    local bld="$1"
    local nros_bin="$repo_root/packages/cli/target/release/nros"
    {
        date -u +%Y-%m-%dT%H:%M:%SZ
        if [ -x "$nros_bin" ]; then
            printf 'tool:nros=%s\n' "$(sha256sum "$nros_bin" | awk '{print $1}')"
        else
            printf 'tool:nros=absent\n'
        fi
    } > "$bld/.compile-ok"
}

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
    # issue #87 — native_sim builds with host gcc (no Zephyr SDK); board-keyed,
    # so the FVP board_import entry (empty board → board.cmake) stays SDK-gated.
    tc_env=()
    case "$board" in
        native_sim*) [ -z "${ZEPHYR_TOOLCHAIN_VARIANT:-}" ] && tc_env=(ZEPHYR_TOOLCHAIN_VARIANT=host) ;;
    esac
    if env "${tc_env[@]}" west "${args[@]}"; then
        west_fixture_stamp "$bld"
        echo "   built $bld"
        n=$((n + 1))
    else
        echo "   west build failed for $id (no stamp; test will report)" >&2
    fi
done
echo "west fixtures built ($n/${#WEST_FIXTURES[@]})."

# §212.M-F.3 self-pkg bringup (issue 0041 — promoted from zephyr_self_pkg.rs's
# in-test `fs::write`). These exercise the `nros_system_generate` shim's L.7
# self-pkg resolver; the contract is "the configure-time BAKE
# (nros-system/system_{config.h,main.c}) fires", NOT a full ELF link (the link
# needs the rest of the runtime — out of scope, same as the original test). So
# the stamp gate is BAKE-EXISTS, not west's exit code: `west build` configures
# (baking) then attempts the doomed link, and we stamp iff the bake landed.
#   id : src-rel : app-subdir
SELF_PKG_FIXTURES=(
    "zephyr_self_pkg_rust:packages/testing/nros-tests/fixtures/zephyr_self_pkg/self:alpha_pkg"
    "zephyr_self_pkg_sibling:packages/testing/nros-tests/fixtures/zephyr_self_pkg/sibling:caller"
)
sp_n=0
for entry in "${SELF_PKG_FIXTURES[@]}"; do
    IFS=':' read -r id src subdir <<< "$entry"
    [ -d "$repo_root/$src/$subdir" ] || { echo "west-fixtures: src missing: $src/$subdir" >&2; continue; }
    bld="$out_root/$id"
    echo "== west-fixture: $id (self-pkg bake-only) =="
    rm -rf "$bld"
    # The link failure is expected → don't let it abort the script (set -u only,
    # no -e, but be explicit). Inspect the bake afterward.
    # issue #87 — native_sim → host gcc toolchain (no Zephyr SDK download).
    sp_tc_env=()
    [ -z "${ZEPHYR_TOOLCHAIN_VARIANT:-}" ] && sp_tc_env=(ZEPHYR_TOOLCHAIN_VARIANT=host)
    env "${sp_tc_env[@]}" west build -b native_sim/native/64 -d "$bld" "$repo_root/$src/$subdir" \
        -- -DCONF_FILE=prj.conf || true
    # Issue 0154 — post-258 bake contract: config header + config cmake
    # (system_main.c retired with the install seam).
    if [ -f "$bld/nros-system/system_config.h" ] && [ -f "$bld/nros-system/system_config.cmake" ]; then
        west_fixture_stamp "$bld"
        echo "   baked $bld/nros-system (link out-of-scope)"
        sp_n=$((sp_n + 1))
    else
        echo "   self-pkg bake MISSING for $id (shim regressed?; test will report)" >&2
    fi
done
echo "self-pkg fixtures baked ($sp_n/${#SELF_PKG_FIXTURES[@]})."
