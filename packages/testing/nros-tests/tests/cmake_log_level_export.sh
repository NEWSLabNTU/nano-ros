#!/usr/bin/env bash
#
# issue 2952 -- the cmake road exports the image's resolved `log.max_level` to
# C/C++ as `NROS_LOG_MAX_LEVEL` (a CACHE INTERNAL plus a compile definition on
# the umbrella), and a changed level reaches an INCREMENTAL build.
#
# THE GAP
#
# On the cmake road the image's knobs reached cargo only (a `--config` env
# file, issue 1712), so a C/C++ consumer had no value to derive a compile-time
# log floor from. `nros_log_max_level_export` (cmake/NanoRosLogLevel.cmake)
# asks the ladder resolver (`nros config explain --get log.max_level`) with the
# inputs cargo gets, and is a configure-time emitter, so its inputs must be
# configure dependencies (issue 1018).
#
# WHAT THIS PINS (a real configure of a CMake project with no languages)
#
#   T1  the board's `[knobs.log] max_level` is exported: cache AND definition,
#       and the resolver is asked for the board facts' platform and board.
#   T2  editing that board descriptor re-runs the configure on `cmake --build`,
#       and the new number is exported (the issue-1018 edge).
#   T3  the image's row (the image-env file) reaches the resolver as its env.
#   T4  an exported NROS_LOG_MAX_LEVEL outranks the image row.
#   T5  no platform resolved: a stated number is taken, the builtin is 0.
#   T6  no platform resolved and a level NAME: refused, not guessed.
#
# The resolver is a stub (`$NROS_CLI`): what it RESOLVES is
# `config::tests::a_board_log_level_resolves_through_the_ladder`'s question;
# this script pins what the configure hands it and does with the answer.
# Board facts are pre-seeded in their per-configure memo for the same reason.
# Buildless: no compiler, no cargo, no fixtures.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
mkdir -p "$ROOT/tmp"
WORK="$(mktemp -d "$ROOT/tmp/cmake-log-level-export.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

if ! command -v cmake >/dev/null; then
    echo "cmake_log_level_export: FAIL -- no cmake on PATH" >&2
    exit 1
fi

# The stub resolver: the env rung, else the board descriptor's level.
STUB="$WORK/bin/nros"
mkdir -p "$WORK/bin"
cat >"$STUB" <<'EOF'
#!/usr/bin/env bash
echo "args: $* | env: ${NROS_LOG_MAX_LEVEL:-<unset>}" >>"$NROS_STUB_LOG"
board=""
while [ $# -gt 0 ]; do
    [ "$1" = --board-toml ] && board="$2"
    shift
done
level="${NROS_LOG_MAX_LEVEL:-}"
if [ -z "$level" ] && [ -n "$board" ]; then
    level="$(sed -n 's/^max_level *= *"\(.*\)"/\1/p' "$board")"
fi
case "${level:-trace}" in
    trace|0) echo 0 ;; debug|1) echo 1 ;; info|2) echo 2 ;; warn|3) echo 3 ;;
    error|4) echo 4 ;; fatal|5) echo 5 ;; off|6) echo 6 ;;
    *) echo "stub: not a level: $level" >&2; exit 1 ;;
esac
EOF
chmod +x "$STUB"

fail=0
expect() {
    local label="$1" got="$2" want="$3"
    if [[ "$got" == "$want" ]]; then
        echo "  ok   $label"
    else
        echo "  FAIL $label" >&2
        echo "       want: $want" >&2
        echo "       got:  $got" >&2
        fail=1
    fi
}

# project <dir> <board-facts env list> [image-env file]
project() {
    local dir="$1" facts="$2" image_env="${3:-}"
    mkdir -p "$dir"
    {
        printf 'cmake_minimum_required(VERSION 3.22)\nproject(p NONE)\n'
        # The memo `nros_resolve_board_facts` reads for (board "", deploy "").
        printf 'set_property(GLOBAL PROPERTY NROS_BOARD_FACTS_ENV______VALUE "%s")\n' "$facts"
        if [ -n "$image_env" ]; then
            printf 'set_property(GLOBAL PROPERTY NROS_IMAGE_ENV_CONFIG "%s")\n' "$image_env"
        fi
        printf 'include("%s/cmake/NanoRosLogLevel.cmake")\n' "$ROOT"
        printf 'add_library(NanoRos INTERFACE)\n'
        printf 'nros_log_max_level_export(NanoRos)\n'
        printf 'get_target_property(_d NanoRos INTERFACE_COMPILE_DEFINITIONS)\n'
        printf 'file(WRITE "${CMAKE_BINARY_DIR}/defs.txt" "${_d}")\n'
    } >"$dir/CMakeLists.txt"
}

cache_level() { sed -n 's/^NROS_LOG_MAX_LEVEL:INTERNAL=//p' "$1/CMakeCache.txt"; }

# configure <src> <build> [env assignments...]
configure() {
    local src="$1" build="$2"
    shift 2
    env -u NROS_LOG_MAX_LEVEL NROS_CLI="$STUB" NROS_STUB_LOG="$WORK/calls.log" "$@" \
        cmake -S "$src" -B "$build" >"$build.log" 2>&1
}

BOARD="$WORK/board/nros-board.toml"
mkdir -p "$WORK/board"
printf '[knobs.log]\nmax_level = "warn"\n' >"$BOARD"
FACTS="NROS_PLATFORM_NAME=posix;NROS_BOARD_TOML=$BOARD"

echo "T1 the board's level is exported"
project "$WORK/t1" "$FACTS"
configure "$WORK/t1" "$WORK/t1/build"
expect "T1 cache" "$(cache_level "$WORK/t1/build")" "3"
expect "T1 definition" "$(cat "$WORK/t1/build/defs.txt")" "NROS_LOG_MAX_LEVEL=3"
expect "T1 asked for the facts' platform and board" \
    "$(grep -c -- "--platform posix --get log.max_level --workspace $WORK/t1 --board-toml $BOARD" "$WORK/calls.log")" "1"

echo "T2 an edited board descriptor re-exports on an incremental build"
sleep 1  # a coarse-mtime filesystem must see the edit as newer
printf '[knobs.log]\nmax_level = "error"\n' >"$BOARD"
env -u NROS_LOG_MAX_LEVEL NROS_CLI="$STUB" NROS_STUB_LOG="$WORK/calls.log" \
    cmake --build "$WORK/t1/build" >"$WORK/t2.log" 2>&1
expect "T2 cache after build" "$(cache_level "$WORK/t1/build")" "4"
expect "T2 definition after build" "$(cat "$WORK/t1/build/defs.txt")" "NROS_LOG_MAX_LEVEL=4"
printf '[knobs.log]\nmax_level = "warn"\n' >"$BOARD"

echo "T3 the image row reaches the resolver"
IMAGE_ENV="$WORK/nros-image-env.toml"
printf '[env]\nNROS_LOG_MAX_LEVEL = "1"\n' >"$IMAGE_ENV"
project "$WORK/t3" "$FACTS" "$IMAGE_ENV"
: >"$WORK/calls.log"
configure "$WORK/t3" "$WORK/t3/build"
expect "T3 cache" "$(cache_level "$WORK/t3/build")" "1"
expect "T3 resolver env" "$(sed -n 's/.*| env: //p' "$WORK/calls.log")" "1"

echo "T4 an exported variable outranks the image row"
project "$WORK/t4" "$FACTS" "$IMAGE_ENV"
configure "$WORK/t4" "$WORK/t4/build" NROS_LOG_MAX_LEVEL=fatal
expect "T4 cache" "$(cache_level "$WORK/t4/build")" "5"

echo "T5 no platform: a stated number, else the builtin"
project "$WORK/t5" "" "$IMAGE_ENV"
configure "$WORK/t5" "$WORK/t5/build"
expect "T5 stated" "$(cache_level "$WORK/t5/build")" "1"
project "$WORK/t5b" ""
configure "$WORK/t5b" "$WORK/t5b/build"
expect "T5 builtin" "$(cache_level "$WORK/t5b/build")" "0"

echo "T6 no platform and a level name: refused"
printf '[env]\nNROS_LOG_MAX_LEVEL = "info"\n' >"$WORK/named.toml"
project "$WORK/t6" "" "$WORK/named.toml"
if configure "$WORK/t6" "$WORK/t6/build"; then
    expect "T6 refused" "configured" "refused"
else
    expect "T6 refusal names the issue" \
        "$(grep -c 'Write the level as a number' "$WORK/t6/build.log")" "1"
fi

if [ "$fail" -ne 0 ]; then
    echo "cmake_log_level_export: FAIL" >&2
    exit 1
fi
echo "cmake_log_level_export: OK (6 cases)"
