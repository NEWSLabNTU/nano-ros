#!/usr/bin/env bash
# tests/zephyr-pthread-pool-defaults-tests.sh -- zenoh's POSIX pools are derived.
#
# WHAT THIS MEASURES
#
# zenoh-pico takes one pthread mutex and one condition variable per declared
# subscriber and queryable, from Zephyr's static pools. The module's
# pre-Kconfig hook writes the pools' Kconfig DEFAULTS from the counts
# `nros configure` resolved (`nros_zpico_pthread_pool_kconfig`,
# zephyr/cmake/nros_zpico_pthread_pools.cmake), so no conf hand-sets a count
# the next entity outgrows.
#
#   A  a small image (2 subscribers, 9 queryables) gets the cond floor
#      (2 + 9 + 4 fixed + 4 headroom = 19) and the module's 64 mutexes, which
#      is above its mutex floor
#   B  a large one (11 subscribers, 31 queryables, 11 subscriber slots) gets
#      both floors: 50 condvars, 70 mutexes
#   C  a table the image STATES as a knob (Kconfig.from-system-toml) above the
#      derived count sizes the pools
#   D  a derived 0 is sized as the zenoh table's C-array floor of 1
#   E  no resolved fragment: the file is removed (a guest keeps the default)
#   F  an unchanged re-render leaves the file untouched (it is a Kconfig source)
#   G  REACH: the build's refusal takes its floor from the same function, and
#      zephyr/Kconfig sources the file before the
#      module's own zenoh defaults
#
# A cmake -P probe, so seconds; no Zephyr workspace. PRECONDITIONS ARE HARD
# FAILURES (issue 1758): no cmake, no verdict, red.
#
# Usage: ./tests/zephyr-pthread-pool-defaults-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=../scripts/lib/grep-q.sh
source "$PROJECT_ROOT/scripts/lib/grep-q.sh"
HELPER="$PROJECT_ROOT/zephyr/cmake/nros_zpico_pthread_pools.cmake"

FAILURES=0
fail() {
    echo "FAIL: $*" >&2
    FAILURES=$((FAILURES + 1))
}

command -v cmake >/dev/null 2>&1 || { echo "FAIL: cmake not found" >&2; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# render <resolved-or-empty> [<knob-defaults>] -- run the helper, out to $WORK/out
render() {
    printf '%s\n' \
        "include(\"$HELPER\")" \
        "nros_zpico_pthread_pool_kconfig(\"$1\" \"$WORK/out\" \"${2:-}\")" \
        > "$WORK/probe.cmake"
    cmake -P "$WORK/probe.cmake" >/dev/null 2>&1
}

# pool <SYM> -- the default the rendered file states for MAX_PTHREAD_<SYM>_COUNT
pool() {
    sed -n "/^configdefault MAX_PTHREAD_$1_COUNT\$/{n;s/^    default //p}" "$WORK/out"
}

resolved() { # subs qrys [slots]
    {
        echo "set(NROS_DERIVED_MAX_SUBSCRIBERS $1)"
        echo "set(NROS_DERIVED_MAX_QUERYABLES $2)"
        [ -n "${3:-}" ] && echo "set(NROS_DERIVED_RMW_SUBSCRIBER_SLOTS $3)"
    } > "$WORK/resolved.cmake"
}

resolved 2 9
render "$WORK/resolved.cmake"
[ "$(pool COND)" = 19 ] || fail "A: cond default $(pool COND), want 19 (2 + 9 + 4 + 4)"
[ "$(pool MUTEX)" = 64 ] || fail "A: mutex default $(pool MUTEX), want the module's 64"
nros_grep_q '^if NROS_RMW_ZENOH$' "$WORK/out" || fail "A: the defaults are not under if NROS_RMW_ZENOH"

resolved 11 31 11
render "$WORK/resolved.cmake"
[ "$(pool COND)" = 50 ] || fail "B: cond default $(pool COND), want 50 (11 + 31 + 4 + 4)"
[ "$(pool MUTEX)" = 70 ] || fail "B: mutex default $(pool MUTEX), want 70 (11 + 31 + 24 + 4)"

resolved 2 9
printf '%s\n' '# knobs' '' 'config NROS_MAX_QUERYABLES' '    int' \
    '    depends on NROS_RMW_ZENOH' '    default 40' '' > "$WORK/kgen"
render "$WORK/resolved.cmake" "$WORK/kgen"
[ "$(pool COND)" = 50 ] || fail "C: cond default $(pool COND), want 50 (2 + stated 40 + 8)"

resolved 0 0
render "$WORK/resolved.cmake"
[ "$(pool COND)" = 16 ] || fail "D: cond default $(pool COND), want the module's 16 over floors of 1"
nros_grep_q 'for 1 subscriber(s) and 1' "$WORK/out" || fail "D: a derived 0 was not floored at 1"

render ""
[ ! -e "$WORK/out" ] || fail "E: with no resolved fragment the file is still there"

resolved 3 4
render "$WORK/resolved.cmake"
touch -d '2000-01-01' "$WORK/out"
render "$WORK/resolved.cmake"
[ "$(stat -c %Y "$WORK/out")" = "$(date -d '2000-01-01' +%s)" ] ||
    fail "F: an unchanged re-render rewrote the file (re-runs Kconfig for nothing)"

nros_grep_q 'nros_zpico_pthread_floors(' "$PROJECT_ROOT/zephyr/cmake/nros_cargo_build.cmake" ||
    fail "G: the build's refusal does not take its floor from nros_zpico_pthread_floors"
order="$(grep -n -e 'Kconfig.pthread-pools"$' -e '^configdefault MAX_PTHREAD_COND_COUNT$' \
             "$PROJECT_ROOT/zephyr/Kconfig" | cut -d: -f2 | head -2 | tr '\n' ' ')"
case "$order" in
    'osource "$(KCONFIG_BINARY_DIR)/../nros/Kconfig.pthread-pools" configdefault MAX_PTHREAD_COND_COUNT ') ;;
    *) fail "G: zephyr/Kconfig does not source Kconfig.pthread-pools before the module's own pool defaults: [$order]" ;;
esac

if [ "$FAILURES" -ne 0 ]; then
    echo "zephyr-pthread-pool-defaults: $FAILURES failure(s)" >&2
    exit 1
fi
echo "zephyr-pthread-pool-defaults: OK (zenoh's POSIX pools default to the image's derived floor)"
