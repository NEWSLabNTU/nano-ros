#!/usr/bin/env bash
# RFC-0141 D1/D2 / phase-582 W1d: the FreeRTOS port's wall-clock resolution,
# on the host. Builds the REAL FreeRTOS kernel (POSIX port) and the REAL
# `nros-platform-freertos` through its own CMakeLists (the cmake road, so
# `nros_clock_selection_apply` applies the selection from the configure's
# environment, as it does from an image's), with a stub board hook
# (`tests/freertos-clock-board`). Each case is its own build and process: the
# port resolves once per process, at its boot point
# (`nros_platform_freertos_clock_resolve_point`), which the program calls from
# a task as a board entry does after network bring-up. Every list case first
# checks that a read before the point reads 0, not synced, and asks the hook
# nothing.
#
# Cases:
#   board-late  source = board, fallback_settle_ms = 1000, a hook that answers
#               only after 200 ms: the point waits for it (>= ~200 ms, under
#               the window), reads the board's instant, {synced, board}, the
#               verdict line "nros: wall clock from board" goes through the
#               board's log writer once, and 100 later reads ask nothing; a read
#               before the scheduler runs resolves nothing and reads 0;
#   board-net   window 300 ms, a hook that answers only once the test sets its
#               "network up" flag, set just before the point: installed,
#               {synced, board}, the verdict once;
#   board-early the same hook, a read early, then MORE than the window passes
#               before the network is up and the point runs: the early read
#               spent no window, so the board's answer is still installed;
#   board-never source = board, window 300 ms, a hook that never answers: the
#               point returns after the window, reads 0, {unsynced, board},
#               and says so once;
#   board-halt  the same with epoch_failure = halt: the port's panic names
#               board and the knob, and nothing runs past it (FreeRTOS's panic
#               halts in place, so the process is ended by the deadline);
#   host-platform  freertos-posix: source = board,platform, the hook never
#               answers: `platform` answers the host's CLOCK_REALTIME;
#   host-default   freertos-posix, no list: CLOCK_REALTIME, {synced, platform};
#   bare-platform  the bare-metal arm (NROS_FREERTOS_POSIX_PORT=0):
#               board,platform with an unanswered hook reaches epoch_failure,
#               because a bare-metal `platform` has no wall clock;
#   bare-default   the bare-metal arm, no list: 0, unsynced (unchanged).
# Every case that resolves a list also prints "nros: clock resolution took <n> ms"
# once, as Zephyr does; board-late and board-never hold its number to a lower
# bound (the hook's delay, the window).
# Negative controls, each against a mutated port that must FAIL:
#   - a point that resolves nothing fails board-late;
#   - resolving lazily on the first read (the shape this replaced, which ran
#     before the network was up) fails board-net and board-early.
#
# Needs the FreeRTOS kernel (`nros locate freertos-kernel`); without it this is
# a REPORTED skip (nros_check_unverified), as the C-port smoke is.
set -uo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/lib/grep-q.sh
. "$root/scripts/lib/grep-q.sh"
# shellcheck source=scripts/lib/deadline.sh
. "$root/scripts/lib/deadline.sh"
fail() { echo "clock-board-freertos: $*" >&2; exit 1; }

freertos_dir="${FREERTOS_DIR:-}"
if [ -z "$freertos_dir" ]; then
    # shellcheck source=scripts/build/cargo.sh
    . "$root/scripts/build/cargo.sh"
    freertos_dir="$(nros_locate_source freertos-kernel)" || freertos_dir=""
fi
if [ -z "$freertos_dir" ] || [ ! -f "$freertos_dir/tasks.c" ]; then
    # shellcheck source=scripts/build/check-skip.sh
    . "$root/scripts/build/check-skip.sh"
    nros_check_unverified clock-board-freertos \
        "the FreeRTOS kernel is not provisioned (nros locate freertos-kernel) — nros setup --source freertos-kernel" \
        || exit 1
    exit 0
fi
command -v cmake >/dev/null || fail "no cmake"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
harness="$root/tests/freertos-clock-board"

# build <name> <case> <hook> <baremetal ON|OFF> [VAR=value ...]: configure with
# the knobs in the environment, as an image's configure sees them.
build() {
    local name="$1" case="$2" hook="$3" bare="$4"; shift 4
    local d="$tmp/$name"
    # nros-cmake-prefix-exempt: pure-C FreeRTOS port harness, no Rust.
    if ! env -u NROS_CLOCK_SOURCE -u NROS_CLOCK_UPGRADE -u NROS_CLOCK_EPOCH_FAILURE \
        -u NROS_CLOCK_FALLBACK_SETTLE_MS "$@" \
        cmake -S "$harness" -B "$d" -DFREERTOS_DIR="$freertos_dir" \
        -DCLOCK_TEST_CASE="$case" -DCLOCK_TEST_HOOK="$hook" -DCLOCK_TEST_BAREMETAL="$bare" \
        >"$d.log" 2>&1; then
        cat "$d.log" >&2
        fail "$name: configure failed"
    fi
    cmake --build "$d" -j4 >>"$d.log" 2>&1 || { cat "$d.log" >&2; fail "$name: build failed"; }
}
# run <name> [want-rc]: run the case under a deadline; prints its output.
run() {
    local name="$1" want="${2:-0}" rc=0 out
    out="$("${NROS_DEADLINE[@]}" "${3:-20}" "$tmp/$name/clock_board" 2>&1)" || rc=$?
    printf '%s\n' "$out" >"$tmp/$name.out"
    printf '%s\n' "$out"
    [ "$rc" -eq "$want" ] || fail "case $name: exit $rc, want $want"
}
# once <name> <line>: the verdict line appears exactly once.
once() {
    local n
    n="$(grep -cF "$2" "$tmp/$1.out")" || true
    [ "$n" = 1 ] || { cat "$tmp/$1.out" >&2; fail "case $1: '$2' printed $n times, not once"; }
    echo "  ok   the verdict \"$2\" went through the log writer once"
}

# took <name>: the resolution-time line appears once, as on Zephyr. No bound
# on the time it reports: that figure is counted in the port's ticks, which
# a loaded host delivers late, so a lower bound fails on code that waited
# correctly (measured 132 ms for a 200 ms wait, issue 2935). That the point
# WAITED is checked on the host clock, in main.c's own CHECKs.
took() {
    once "$1" "nros: clock resolution took "
}

echo "clock-board-freertos: against the FreeRTOS port (POSIX kernel port)"
echo "== board-late"
build board-late CASE_BOARD_LATE 2 OFF NROS_CLOCK_SOURCE=board NROS_CLOCK_FALLBACK_SETTLE_MS=1000
nros_grep_q -F 'resolves the wall clock from `board`' "$tmp/board-late.log"; m=$?
[ "$m" -eq 0 ] || fail "board-late: the configure did not apply the selection"
run board-late
once board-late "nros: wall clock from board"
took board-late

echo "== board-net"
build board-net CASE_BOARD_NET 3 OFF NROS_CLOCK_SOURCE=board NROS_CLOCK_FALLBACK_SETTLE_MS=300
run board-net
once board-net "nros: wall clock from board"

echo "== board-early"
build board-early CASE_BOARD_EARLY 3 OFF NROS_CLOCK_SOURCE=board NROS_CLOCK_FALLBACK_SETTLE_MS=300
run board-early
once board-early "nros: wall clock from board"

echo "== board-never"
build board-never CASE_BOARD_NEVER 0 OFF NROS_CLOCK_SOURCE=board NROS_CLOCK_FALLBACK_SETTLE_MS=300
run board-never
once board-never "nros: no wall-clock source answered (board); stamps are unset"
took board-never

echo "== board-halt"
build board-halt CASE_BOARD_NEVER 0 OFF NROS_CLOCK_SOURCE=board NROS_CLOCK_FALLBACK_SETTLE_MS=300 \
    NROS_CLOCK_EPOCH_FAILURE=halt
# The port's panic disables interrupts and halts in place, so the deadline
# ends it: on the POSIX kernel port the halted thread has the host's signals
# masked too, so it is the deadline's KILL (137), or its TERM (124).
rc=0; out="$("${NROS_DEADLINE[@]}" 3 "$tmp/board-halt/clock_board" 2>&1)" || rc=$?
printf '%s\n' "$out" >"$tmp/board-halt.out"
printf '%s\n' "$out"
[ "$rc" -eq 124 ] || [ "$rc" -eq 137 ] || fail "case board-halt: exit $rc, want the deadline's 124/137 (halted)"
nros_grep_q -F 'no wall-clock source answered (board); halting, as clock.epoch_failure = halt asks' \
    "$tmp/board-halt.out"; m=$?
[ "$m" -eq 0 ] || fail "board-halt: the halt line does not name board and the knob"
# Nothing from the task may follow the halt.
nros_grep_q -F 'read after the point' "$tmp/board-halt.out"; m=$?
[ "$m" -eq 1 ] || fail "board-halt: the task ran past the halt"
echo "  ok   halt stops the image after the window, naming board and the knob"

echo "== host-platform (freertos-posix)"
build host-platform CASE_PLATFORM_HOST 0 OFF NROS_CLOCK_SOURCE=board,platform \
    NROS_CLOCK_FALLBACK_SETTLE_MS=0
run host-platform
once host-platform "nros: wall clock from platform"
took host-platform

echo "== host-default (freertos-posix)"
build host-default CASE_DEFAULT_HOST 1 OFF
run host-default
nros_grep_q -F 'nros: ' "$tmp/host-default.out"; m=$?
[ "$m" -eq 1 ] || fail "host-default: the default build reported a resolution"

echo "== bare-platform (bare-metal arm)"
build bare-platform CASE_PLATFORM_NONE 0 ON NROS_CLOCK_SOURCE=board,platform \
    NROS_CLOCK_FALLBACK_SETTLE_MS=0
run bare-platform
once bare-platform "nros: no wall-clock source answered (board, platform); stamps are unset"
took bare-platform

echo "== bare-default (bare-metal arm)"
build bare-default CASE_DEFAULT_NONE 1 ON
run bare-default

# Negative controls. mutant <name> <python-replacement-script>: a copy of the
# port with one edit, wired into a copy of the harness.
mutant() {
    local name="$1" edit="$2" src="$tmp/$1-src" h="$tmp/$1-harness"
    mkdir -p "$src" "$h"
    cp -r "$root/packages/platform/nros-platform-freertos/." "$src/"
    python3 -c "$edit" "$src/src/platform.c" || fail "negative control $name: could not build the mutant"
    # The mutant tree sits outside the repo, so its CMakeLists cannot reach
    # ../../../cmake; point it back.
    sed -i "s#\${CMAKE_CURRENT_LIST_DIR}/../../../cmake/NanoRosClockSelection.cmake#$root/cmake/NanoRosClockSelection.cmake#" \
        "$src/CMakeLists.txt"
    cp "$root/tests/freertos-clock-board/main.c" "$root/tests/freertos-clock-board/hook.c" "$h/"
    sed -e "s#\${REPO_ROOT}/packages/platform/nros-platform-freertos\"#$src\"#" \
        -e "s#set(REPO_ROOT \"\${CMAKE_CURRENT_LIST_DIR}/../..\")#set(REPO_ROOT \"$root\")#" \
        "$root/tests/freertos-clock-board/CMakeLists.txt" >"$h/CMakeLists.txt"
}
# must_fail <build> <want-line>: the mutant run fails, on the named check.
must_fail() {
    local rc=0 out
    out="$("${NROS_DEADLINE[@]}" 20 "$tmp/$1/clock_board" 2>&1)" || rc=$?
    [ "$rc" -ne 0 ] || { printf '%s\n' "$out" >&2; fail "negative control $1: the mutant passed"; }
    nros_grep_q -F "$2" <<<"$out"; m=$?
    [ "$m" -eq 0 ] || { printf '%s\n' "$out" >&2; fail "negative control $1: it failed, but not on '$2'"; }
    echo "  ok   negative control $1 fails on '$2'"
}

# 1. A point that resolves nothing fails board-late.
mutant inert '
import sys
p = sys.argv[1]
s = open(p).read()
needle = "void nros_platform_freertos_clock_resolve_point(void) {\n    if (nros_freertos_clock_resolved()) {"
if needle not in s:
    sys.exit("the resolve point was not found")
open(p, "w").write(s.replace(needle, "void nros_platform_freertos_clock_resolve_point(void) {\n    return;\n    if (nros_freertos_clock_resolved()) {"))
'
harness="$tmp/inert-harness" build inert CASE_BOARD_LATE 2 OFF NROS_CLOCK_SOURCE=board \
    NROS_CLOCK_FALLBACK_SETTLE_MS=1000
must_fail inert 'FAIL the point waited for the board'

# 2. Lazy on the first read (resolve from time_now / wall_clock_state) fails
#    board-net and board-early: the first read resolves while the network is
#    down, spends the window, and latches "unanswered".
mutant lazy '
import sys
p = sys.argv[1]
s = open(p).read()
needles = [
    "uint64_t nros_platform_time_now_ns(void) {\n    if (!nros_freertos_clock_resolved()) {",
    "nros_wall_clock_state_t nros_platform_wall_clock_state(void) {\n    if (!nros_freertos_clock_resolved()) {",
]
for n in needles:
    if n not in s:
        sys.exit("a read was not found: " + n)
    head, rest = n.split("\n", 1)
    s = s.replace(n, head + "\n    nros_platform_freertos_clock_resolve_point();\n" + rest)
open(p, "w").write(s)
'
harness="$tmp/lazy-harness" build lazy-net CASE_BOARD_NET 3 OFF NROS_CLOCK_SOURCE=board \
    NROS_CLOCK_FALLBACK_SETTLE_MS=300
must_fail lazy-net 'FAIL ...and never asks the hook'
harness="$tmp/lazy-harness" build lazy-early CASE_BOARD_EARLY 3 OFF NROS_CLOCK_SOURCE=board \
    NROS_CLOCK_FALLBACK_SETTLE_MS=300
must_fail lazy-early "FAIL reading is the board's instant"
echo "clock-board-freertos: OK (negative controls: an inert point fails board-late; lazy-on-first-read fails board-net and board-early)"
