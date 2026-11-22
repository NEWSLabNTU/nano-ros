#!/usr/bin/env bash
# RFC-0141 D1/D2 / phase-582 W1d: the FreeRTOS port's wall-clock resolution,
# on the host. Builds the REAL FreeRTOS kernel (POSIX port) and the REAL
# `nros-platform-freertos` through its own CMakeLists (the cmake road, so
# `nros_clock_selection_apply` applies the selection from the configure's
# environment, as it does from an image's), with a stub board hook
# (`tests/freertos-clock-board`). Each case is its own build and process: the
# port resolves once per process, on the first wall-clock read from a task.
#
# Cases:
#   board-late  source = board, fallback_settle_ms = 1000, a hook that answers
#               only after 200 ms: the first read waits for it (>= ~200 ms, under
#               the window), reads the board's instant, {synced, board}, the
#               verdict line "nros: wall clock from board" goes through the
#               board's log writer once, and 100 later reads ask nothing; a read
#               before the scheduler runs resolves nothing and reads 0;
#   board-never source = board, window 300 ms, a hook that never answers: the
#               first read returns after the window, reads 0, {unsynced, board},
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
# Negative control: board-late against a port whose resolution call is
# deleted from time_now must FAIL.
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

echo "clock-board-freertos: against the FreeRTOS port (POSIX kernel port)"
echo "== board-late"
build board-late CASE_BOARD_LATE 2 OFF NROS_CLOCK_SOURCE=board NROS_CLOCK_FALLBACK_SETTLE_MS=1000
nros_grep_q -F 'resolves the wall clock from `board`' "$tmp/board-late.log"; m=$?
[ "$m" -eq 0 ] || fail "board-late: the configure did not apply the selection"
run board-late
once board-late "nros: wall clock from board"

echo "== board-never"
build board-never CASE_BOARD_NEVER 0 OFF NROS_CLOCK_SOURCE=board NROS_CLOCK_FALLBACK_SETTLE_MS=300
run board-never
once board-never "nros: no wall-clock source answered (board); stamps are unset"

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
nros_grep_q -F 'first read took' "$tmp/board-halt.out"; m=$?
[ "$m" -eq 1 ] || fail "board-halt: the task ran past the halt"
echo "  ok   halt stops the image after the window, naming board and the knob"

echo "== host-platform (freertos-posix)"
build host-platform CASE_PLATFORM_HOST 0 OFF NROS_CLOCK_SOURCE=board,platform \
    NROS_CLOCK_FALLBACK_SETTLE_MS=0
run host-platform
once host-platform "nros: wall clock from platform"

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

echo "== bare-default (bare-metal arm)"
build bare-default CASE_DEFAULT_NONE 1 ON
run bare-default

# Negative control: time_now that never resolves fails board-late.
mutant="$tmp/mutant-src"
mkdir -p "$mutant"
cp -r "$root/packages/platform/nros-platform-freertos/." "$mutant/"
python3 - "$mutant/src/platform.c" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
needle = "uint64_t nros_platform_time_now_ns(void) {\n    const bool resolved = nros_freertos_clock_resolved();"
if needle not in s:
    sys.exit("the time_now resolution call was not found")
open(p, "w").write(s.replace(needle, "uint64_t nros_platform_time_now_ns(void) {\n    const bool resolved = false;"))
PY
[ $? -eq 0 ] || fail "negative control: could not build the mutant"
# The mutant tree sits outside the repo, so its CMakeLists cannot reach
# ../../../cmake; point it back.
sed -i "s#\${CMAKE_CURRENT_LIST_DIR}/../../../cmake/NanoRosClockSelection.cmake#$root/cmake/NanoRosClockSelection.cmake#" \
    "$mutant/CMakeLists.txt"
sed "s#\${REPO_ROOT}/packages/platform/nros-platform-freertos\"#$mutant\"#" \
    "$harness/CMakeLists.txt" >"$tmp/harness-CMakeLists.txt"
mkdir -p "$tmp/mutant-harness"
cp "$harness/main.c" "$harness/hook.c" "$tmp/mutant-harness/"
sed "s#set(REPO_ROOT \"\${CMAKE_CURRENT_LIST_DIR}/../..\")#set(REPO_ROOT \"$root\")#" \
    "$tmp/harness-CMakeLists.txt" >"$tmp/mutant-harness/CMakeLists.txt"
harness="$tmp/mutant-harness" build mutant CASE_BOARD_LATE 2 OFF NROS_CLOCK_SOURCE=board \
    NROS_CLOCK_FALLBACK_SETTLE_MS=1000
rc=0; out="$("${NROS_DEADLINE[@]}" 20 "$tmp/mutant/clock_board" 2>&1)" || rc=$?
[ "$rc" -ne 0 ] || { printf '%s\n' "$out" >&2; fail "negative control: a port that never resolves passed board-late"; }
nros_grep_q -F 'FAIL the first read waited for the board' <<<"$out"; m=$?
[ "$m" -eq 0 ] || { printf '%s\n' "$out" >&2; fail "negative control: it failed, but not on the wait"; }
echo "clock-board-freertos: OK (negative control: a time_now that never resolves fails board-late)"
