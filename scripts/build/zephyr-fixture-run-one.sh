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
  sig sig_file best_effort eff_pristine

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
    sig sig_file best_effort eff_pristine extra_field <<< "$record_us"

[ -z "${extra_field:-}" ] || die "record has extra fields: $id"

for field_name in kind id target board lang lang_tag role rmw src src_dir build_name build_dir \
    log xrce_agent_port zenoh_locator cyclone_domain conf_files extra_cmake_defs \
    sig sig_file best_effort eff_pristine; do
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
                "$nros_root/packages/dds/nros-rmw-cyclonedds/src" \
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

# issue #87 — native_sim builds with the host gcc toolchain (no Zephyr SDK).
# Board-keyed, not version-keyed: native_sim on ANY line (3.7 + 4.4) uses
# ZEPHYR_TOOLCHAIN_VARIANT=host, so an SDK-free host can build the native_sim
# fixture subset. Real embedded boards (FVP cortex-a/r, cyclonedds targets)
# leave the variant unset → Zephyr locates the downloaded SDK as before.
# Respect an externally-set variant (caller override wins).
toolchain_env=()
if [ -z "${ZEPHYR_TOOLCHAIN_VARIANT:-}" ]; then
    case "$board" in
        native_sim|native_sim/*) toolchain_env=(ZEPHYR_TOOLCHAIN_VARIANT=host) ;;
    esac
fi

use_west=0
if [ "$needs_west" = "0" ]; then
    if [ "$jobserver" = "1" ]; then
        build_argv=(ninja -C "$build_dir")
    else
        build_argv=(ninja -C "$build_dir" -j "$ninja_jobs")
    fi
else
    use_west=1
    build_argv=(west build -b "$board" -d "$build_dir" -p "$actual_pristine" "$src_dir" "${west_extra[@]}")
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
