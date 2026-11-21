#!/usr/bin/env bash
# check-knob-env-refusal: a knob's environment rung that moved after a build
# dir was configured REFUSES the build instead of being ignored (issue 2810).
#
# The configure reads a knob's env rung once, through `_nros_knob_env`
# (cmake/NanoRosKnobEnv.cmake), and bakes the answer into the build. cmake has
# no dependency edge on an environment variable, so a value exported, changed
# or unset afterwards used to be dropped silently. The ruling: record every
# read, and refuse at build time when the live environment disagrees.
#
#   1. Every knob the Zephyr ladder resolves (the `_nros_resolve(_derivable)?_knob(`
#      inventory, the same harvest the shared cargo dir key uses) is read ONLY
#      through `_nros_knob_env`: a bare `$ENV{<knob>}` anywhere under cmake/ or
#      zephyr/ is refused, because it bypasses the record.
#   2. The Zephyr module installs the check (`nros_knob_env_check_target`).
#   3. Behaviour, on a small cmake + ninja project over the REAL helper (this
#      gate's selftest, run on every invocation):
#        - configured with the knob set, a build with it changed or unset refuses,
#          naming the knob, the cached value and source, the live value and
#          `cmake "<build-dir>"`, and the refused build regenerates nothing;
#        - a knob unset at configure and exported afterwards refuses;
#        - an unrelated variable never refuses;
#        - after a re-configure the same build passes;
#        - a second build with nothing changed re-runs only the check.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
mkdir -p "$repo/tmp"
work="$(mktemp -d "$repo/tmp/knob-env-refusal.XXXXXX")"
trap 'rm -rf "$work"' EXIT
fail=0
say() { echo "check-knob-env-refusal: $*"; }
bad() { say "FAIL: $*"; fail=1; }
# shellcheck source=../lib/grep-q.sh
. "$repo/scripts/lib/grep-q.sh"

# 1. One reader.
rc=0
inventory="$(grep -oE '_nros_resolve(_derivable)?_knob\([A-Z0-9_]+' \
    "$repo/zephyr/cmake/nros_cargo_build.cmake")" || rc=$?
if [ "$rc" -ne 0 ] || [ -z "$inventory" ]; then
    bad "the knob inventory harvest found no names (grep rc=$rc)"; exit 1
fi
inventory="$(sed -E 's/.*\(//' <<<"$inventory" | sort -u)"
alt="$(paste -sd'|' <<<"$inventory")"
nros_git_grep raw -C "$repo" -n -E "\\\$ENV\\{($alt)\\}" -- \
    'cmake/*.cmake' 'cmake/**/*.cmake' 'zephyr/*.txt' 'zephyr/*.cmake' 'zephyr/**/*.cmake'
hits="$(grep -v -E '^cmake/NanoRosKnobEnv(Check)?\.cmake:|^[^:]+:[0-9]+:[[:space:]]*#' <<<"$raw" || true)"
if [ -n "$hits" ]; then
    bad "a resolved knob's environment is read outside _nros_knob_env (so the build-time check cannot see it):"
    sed 's/^/    /' <<<"$hits"
fi

# 2. Installed on every Zephyr configure.
rc=0; nros_grep_q -E '^[[:space:]]*nros_knob_env_check_target\(' "$repo/zephyr/CMakeLists.txt" || rc=$?
case $rc in
    0) ;;
    1) bad "zephyr/CMakeLists.txt no longer calls nros_knob_env_check_target()" ;;
    *) exit 2 ;;
esac

# 3. Behaviour: the selftest, run on every invocation.
selftest() {
command -v cmake >/dev/null || { bad "cmake not on PATH"; exit 1; }
command -v ninja >/dev/null || { bad "ninja not on PATH"; exit 1; }
k=NROS_ZEPHYR_HEAP_SIZE
mkdir -p "$work/src"
cat > "$work/src/CMakeLists.txt" <<'EOF'
cmake_minimum_required(VERSION 3.20)
project(knob_env_refusal NONE)
include("${NROS_REPO}/cmake/NanoRosKnobEnv.cmake")
_nros_knob_env(NROS_ZEPHYR_HEAP_SIZE _v)
if(_v STREQUAL "")
    set(_v 114688)
    set(NROS_KNOB_SOURCE_NROS_ZEPHYR_HEAP_SIZE kconfig CACHE INTERNAL "")
else()
    set(NROS_KNOB_SOURCE_NROS_ZEPHYR_HEAP_SIZE environment CACHE INTERNAL "")
endif()
set(NROS_RESOLVED_NROS_ZEPHYR_HEAP_SIZE "${_v}" CACHE INTERNAL "")
# What the Zephyr module does: publish the answer into its own environment,
# then read the knob again (the shared cargo dir key). The record must keep
# the FIRST read, the operator's.
set(ENV{NROS_ZEPHYR_HEAP_SIZE} "${_v}")
_nros_knob_env(NROS_ZEPHYR_HEAP_SIZE _again)
nros_knob_env_check_target(_chk)
add_custom_command(OUTPUT out.txt COMMAND sh -c "echo ${_v} > out.txt" VERBATIM)
add_custom_target(app ALL DEPENDS out.txt)
add_dependencies(app ${_chk})
EOF
b="$work/b"
# The project includes this checkout's helper by path (NROS_REPO); it never
# looks for an installed nano_ros.
configure() {
    local cfg=(cmake -S "$work/src" -B "$b" -G Ninja "-DNROS_REPO=$repo")  # copy-out-root-ok: synthetic project, names NROS_REPO itself
    env "$@" "${cfg[@]}" >"$work/cfg.log" 2>&1 \
        || { bad "configure failed: $(tail -3 "$work/cfg.log")"; exit 1; }
}
build() { rc=0; env "$@" ninja -C "$b" >"$work/build.log" 2>&1 || rc=$?; }
expect_pass() { # <case> <env...>
    local c=$1; shift; build "$@"
    [ "$rc" -eq 0 ] || bad "$c: refused, expected to pass: $(grep -m1 "$k" "$work/build.log" || tail -2 "$work/build.log")"
}
need_line() { # <case> <fixed text> <what>
    local r=0; nros_grep_q -F -- "$2" "$work/build.log" || r=$?
    case $r in 0) ;; 1) bad "$1: refusal does not name $3" ;; *) exit 2 ;; esac
}
expect_refusal() { # <case> <cached-text> <live-text> <env...>
    local c=$1 cached=$2 live=$3; shift 3; build "$@"
    if [ "$rc" -eq 0 ]; then bad "$c: built, expected a refusal"; return; fi
    need_line "$c" "$k: this build dir was configured with $cached" "$k and its cached value $cached"
    need_line "$c" "the environment now says $live" "the live value $live"
    need_line "$c" "cmake \"$b\"" "the re-configure command"
}

configure "$k=1000"
expect_pass "configured with the knob, same env" "$k=1000"
[ "$(cat "$b/out.txt")" = 1000 ] || bad "the configured value did not reach the build"
before="$(stat -c %Y.%y "$b/out.txt")"
build "$k=1000"
explain="$(env "$k=1000" ninja -C "$b" -n -d explain 2>&1 | grep -E '^\[[0-9]+/[0-9]+\]' || true)"
[ "$(stat -c %Y.%y "$b/out.txt")" = "$before" ] || bad "a no-change build regenerated an output"
case "$explain" in
    *$'\n'*|'') bad "a no-change build runs more than the check: $explain" ;;
    *"knob environment agrees"*) ;;
    *) bad "a no-change build runs something other than the check: $explain" ;;
esac
expect_refusal "knob changed after configure" "1000 (from environment)" 2000 "$k=2000"
[ "$(stat -c %Y.%y "$b/out.txt")" = "$before" ] || bad "a refused build regenerated an output"
expect_refusal "knob unset after configure" "1000 (from environment)" unset -u "$k"
expect_pass "an unrelated variable" "$k=1000" NROS_KNOB_ENV_UNRELATED=1 NROS_VERBOSE=1
configure "$k=2000"
expect_pass "after a re-configure" "$k=2000"
[ "$(cat "$b/out.txt")" = 2000 ] || bad "the re-configured value did not reach the build"
rm -rf "$b"
configure -u "$k"
expect_pass "configured without the knob" -u "$k"
expect_refusal "knob exported after configure" "114688 (from kconfig)" 327680 "$k=327680"
}
selftest

if [ "$fail" -ne 0 ]; then
    exit 1
fi
say "OK ($(wc -l <<<"$inventory") resolved knobs read through one helper; the refusal behaves)"
