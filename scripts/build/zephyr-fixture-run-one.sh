#!/usr/bin/env bash
# Run one Zephyr fixture leaf from a structured TSV record.
#
# This script is intentionally Zephyr-owned: it only runs `west build` for
# configure/build or `ninja -C` for an already-current Zephyr build directory.
set -euo pipefail

usage() {
    cat >&2 <<'EOF'
usage: scripts/build/zephyr-fixture-run-one.sh RECORD.tsv
       scripts/build/zephyr-fixture-run-one.sh -

Runs one Zephyr fixture record. The record must contain these tab-separated
fields:
  kind id target board lang lang_tag role rmw src src_dir build_name build_dir
  log xrce_agent_port zenoh_locator cyclone_domain conf_files extra_cmake_defs
  sig sig_file best_effort eff_pristine ws_dir nros_image

`ws_dir` and `nros_image` are EMPTY on an unmigrated leaf and the build runs
`west build` directly. When both are set (phase-383 W9.b) the build runs
`nros build <bringup>:<image>` from the workspace instead, which resolves the
application and the image's overlays and execs the same `west build` — the
build dir, the pristine mode and the row's cmake defines ride across as native
args.

Required environment prepared by the caller:
  NROS_ZEPHYR_WORKSPACE

Optional environment:
  NROS_ZEPHYR_TOOL_PATH
  NROS_ZEPHYR_MAKE_BIN
  NROS_ZEPHYR_NINJA_JOBS
  NROS_ZEPHYR_SCCACHE_DISABLE
  NROS_ZEPHYR_CCACHE_DIR
  NROS_ZEPHYR_CCACHE_TEMPDIR
  NROS_ZEPHYR_JOBSERVER
EOF
}

die() {
    echo "zephyr-fixture-run-one: $*" >&2
    exit 2
}

unescape_field() {
    local value="$1"
    value="${value//\\t/$'\t'}"
    value="${value//\\n/$'\n'}"
    printf '%s' "$value"
}

record_path="${1:-}"
if [ -z "$record_path" ] || [ "$record_path" = "-h" ] || [ "$record_path" = "--help" ]; then
    usage
    if [ -z "$record_path" ]; then
        exit 2
    fi
    exit 0
fi
[ "$#" -eq 1 ] || { usage; exit 2; }

if [ "$record_path" = "-" ]; then
    IFS= read -r record || die "no record on stdin"
else
    [ -f "$record_path" ] || die "record file not found: $record_path"
    IFS= read -r record < "$record_path" || die "empty record file: $record_path"
fi

record_us="${record//$'\t'/$'\x1f'}"
IFS=$'\x1f' read -r \
    kind id target board lang lang_tag role rmw src src_dir build_name build_dir \
    log xrce_agent_port zenoh_locator cyclone_domain conf_files extra_cmake_defs \
    sig sig_file best_effort eff_pristine ws_dir nros_image extra_field <<< "$record_us"

[ -z "${extra_field:-}" ] || die "record has extra fields: $id"

for field_name in kind id target board lang lang_tag role rmw src src_dir build_name build_dir \
    log xrce_agent_port zenoh_locator cyclone_domain conf_files extra_cmake_defs \
    sig sig_file best_effort eff_pristine ws_dir nros_image; do
    printf -v "$field_name" '%s' "$(unescape_field "${!field_name}")"
done

[ "$kind" = "fixture" ] || die "unsupported record kind '$kind' for $id"
[ -n "$id" ] || die "record id is empty"
[ -n "$board" ] || die "record board is empty: $id"
[ -n "$src_dir" ] || die "record src_dir is empty: $id"
[ -n "$build_dir" ] || die "record build_dir is empty: $id"
[ -n "$log" ] || die "record log is empty: $id"
[ -n "$sig_file" ] || die "record sig_file is empty: $id"
case "$best_effort" in
    0|1) ;;
    *) die "invalid best_effort=$best_effort for $id" ;;
esac
case "$eff_pristine" in
    auto|always|never) ;;
    *) die "invalid eff_pristine=$eff_pristine for $id" ;;
esac

nros_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
workspace="${NROS_ZEPHYR_WORKSPACE:-}"
[ -n "$workspace" ] || die "NROS_ZEPHYR_WORKSPACE is required"
[ -d "$workspace" ] || die "NROS_ZEPHYR_WORKSPACE does not exist: $workspace"

tool_path="${NROS_ZEPHYR_TOOL_PATH:-$PATH}"
make_bin="${NROS_ZEPHYR_MAKE_BIN:-}"
ninja_jobs="${NROS_ZEPHYR_NINJA_JOBS:-1}"
sccache_disable="${NROS_ZEPHYR_SCCACHE_DISABLE:-0}"
ccache_dir="${NROS_ZEPHYR_CCACHE_DIR:-$nros_root/build/zephyr-ccache}"
ccache_tmpdir="${NROS_ZEPHYR_CCACHE_TEMPDIR:-$nros_root/build/zephyr-ccache-tmp}"
jobserver="${NROS_ZEPHYR_JOBSERVER:-0}"

case "$ninja_jobs" in
    ''|*[!0-9]*) die "invalid NROS_ZEPHYR_NINJA_JOBS=$ninja_jobs" ;;
    *) [ "$ninja_jobs" -ge 1 ] || die "invalid NROS_ZEPHYR_NINJA_JOBS=$ninja_jobs" ;;
esac
case "$sccache_disable" in
    0|1) ;;
    *) die "invalid NROS_ZEPHYR_SCCACHE_DISABLE=$sccache_disable" ;;
esac
case "$jobserver" in
    0|1) ;;
    *) die "invalid NROS_ZEPHYR_JOBSERVER=$jobserver" ;;
esac

if [ -n "$make_bin" ] && [ -f "$build_dir/CMakeCache.txt" ]; then
    cache_make="MAKE:FILEPATH=$make_bin"
else
    cache_make=""
fi

extra_args=()
if [ -n "$extra_cmake_defs" ]; then
    # The generator emits CMake -D tokens without spaces. Split them into argv
    # elements, then normalize the fields whose values are also carried in the
    # structured record so quoted display strings are not required for execution.
    read -r -a extra_args <<< "$extra_cmake_defs"
fi

replace_or_append_arg() {
    local key="$1"
    local value="$2"
    local arg="${key}=${value}"
    local i
    for i in "${!extra_args[@]}"; do
        case "${extra_args[$i]}" in
            "$key="*)
                extra_args[$i]="$arg"
                return
                ;;
        esac
    done
    extra_args+=("$arg")
}

if [ -n "$make_bin" ]; then
    replace_or_append_arg "-DMAKE" "$make_bin"
fi
if [ -n "$xrce_agent_port" ]; then
    replace_or_append_arg "-DCONFIG_NROS_XRCE_AGENT_PORT" "$xrce_agent_port"
fi
if [ -n "$zenoh_locator" ]; then
    replace_or_append_arg "-DCONFIG_NROS_ZENOH_LOCATOR" "\"$zenoh_locator\""
fi
if [ -n "$cyclone_domain" ]; then
    replace_or_append_arg "-DCONFIG_NROS_DOMAIN_ID" "$cyclone_domain"
fi
if [ -n "$conf_files" ]; then
    replace_or_append_arg "-DCONF_FILE" "$conf_files"
fi

west_extra=()
if [ "${#extra_args[@]}" -gt 0 ]; then
    west_extra=(-- "${extra_args[@]}")
fi

needs_west=0
if [ "$eff_pristine" = "always" ]; then
    needs_west=1
fi
if [ ! -f "$build_dir/build.ninja" ]; then
    needs_west=1
fi
if [ -n "$cache_make" ] && ! grep -qxF "$cache_make" "$build_dir/CMakeCache.txt"; then
    needs_west=1
fi
if [ ! -f "$sig_file" ] || [ "$(cat "$sig_file")" != "$sig" ]; then
    needs_west=1
fi

actual_pristine="$eff_pristine"
case "$build_dir" in
    *cyclonedds*)
        if [ -f "$build_dir/zephyr/zephyr.exe" ] && [ -n "$(find \
                "$nros_root/packages/rmw/cyclonedds/nros-rmw-cyclonedds/src" \
                \( -name '*.cpp' -o -name '*.hpp' \) \
                -newer "$build_dir/zephyr/zephyr.exe" -print -quit 2>/dev/null)" ]; then
            needs_west=1
            actual_pristine=always
        fi
        ;;
esac

cmake_build_env=()
if [ "$jobserver" = "1" ]; then
    unset CMAKE_BUILD_PARALLEL_LEVEL
else
    cmake_build_env=(CMAKE_BUILD_PARALLEL_LEVEL="$ninja_jobs")
fi

# issues #87 + 0698 — which toolchain this board builds with. native_sim uses
# host gcc and needs no SDK; every other board names `zephyr` rather than
# leaving the variant unset, because unset is what CMake 4 rejects. One rule for
# all three callers, with the reasoning, in scripts/build/zephyr-toolchain.sh.
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/zephyr-toolchain.sh"
toolchain_env=(ZEPHYR_TOOLCHAIN_VARIANT="$(nros_zephyr_toolchain_variant "$board")")

# issue 0698 follow-up — the Zephyr venv is this lane's, not the session's.
source "$nros_root/scripts/build/zephyr-python.sh"
nros_zephyr_activate
use_west=0
if [ "$needs_west" = "0" ]; then
    if [ "$jobserver" = "1" ]; then
        build_argv=(ninja -C "$build_dir")
    else
        build_argv=(ninja -C "$build_dir" -j "$ninja_jobs")
    fi
else
    use_west=1
    if [ -n "$nros_image" ]; then
        # phase-383 W9.b — a RETARGETED row. `nros build` resolves the
        # application from `[image.<id>] entry` and applies the image's `conf`
        # overlays (RFC-0085 D4/D5), then execs the same `west build`. It is
        # addressed from the WORKSPACE and never named the application, which is
        # the whole point: the row stops spelling a fact the image declares.
        #
        # The three harness fields stay ROW facts and ride across as native
        # args, because they are this fixture's ISOLATION and not properties of
        # the program: `-d <build_dir>` (a private build dir) and `-p` route to
        # west's own zone, the `-D` defines — the locator among them — to
        # cmake's. `nros build`'s `route_native_arg` does that split; `-b` is
        # deliberately NOT passed, because the image declares the board and a
        # second spelling is refused.
        #
        # `--offline`: this lane must not reach the network mid-sweep, matching
        # `workspace-fixtures-build.sh`'s migrated arms.
        [ -n "$ws_dir" ] || die "record has nros_image but no ws_dir: $id"
        [ -d "$ws_dir" ] || die "record ws_dir does not exist: $ws_dir ($id)"
        # THE nros CLI this lane already chose — the emitter resolved it once and
        # published it as the codegen tool. Re-resolving here (PATH, or a second
        # search) is how a lane ends up building with a different binary than the
        # one its signature names.
        nros_bin=""
        for _arg in "${extra_args[@]}"; do
            case "$_arg" in
                -D_NANO_ROS_CODEGEN_TOOL=*) nros_bin="${_arg#-D_NANO_ROS_CODEGEN_TOOL=}" ;;
            esac
        done
        [ -n "$nros_bin" ] && [ -x "$nros_bin" ] \
            || die "no usable nros CLI in the record's -D_NANO_ROS_CODEGEN_TOOL: $id"
        build_argv=("$nros_bin" build "$nros_image" --workspace "$ws_dir" --offline \
            -- -d "$build_dir" -p "$actual_pristine" "${extra_args[@]}")
    else
        build_argv=(west build -b "$board" -d "$build_dir" -p "$actual_pristine" "$src_dir" "${west_extra[@]}")
    fi
fi

mkdir -p "$(dirname "$log")" "$(dirname "$sig_file")" "$ccache_dir" "$ccache_tmpdir"

set +e
(
    cd "$workspace"
    env PATH="$tool_path" SCCACHE_DISABLE="$sccache_disable" \
        CCACHE_DIR="$ccache_dir" CCACHE_TEMPDIR="$ccache_tmpdir" \
        "${toolchain_env[@]}" \
        "${cmake_build_env[@]}" \
        "${build_argv[@]}"
    rc=$?
    if [ "$rc" -eq 0 ] && [ "$use_west" = "1" ]; then
        printf '%s\n' "$sig" > "$sig_file"
    fi
    exit "$rc"
) > "$log" 2>&1
rc=$?
set -e

if [ "$rc" -eq 0 ]; then
    exit 0
fi

if [ "$best_effort" = "1" ]; then
    echo "zephyr-fixture-run-one: best-effort failed: $id (log: $log)" >&2
    tail -80 "$log" >&2 || true
    exit 0
fi

echo "zephyr-fixture-run-one: failed: $id (log: $log)" >&2
tail -80 "$log" >&2 || true
exit "$rc"
