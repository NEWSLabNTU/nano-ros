#!/usr/bin/env bash
# RFC-0141 D2 / phase-582 W1d: the `board` wall-clock source on the POSIX
# port. Compiles the REAL port (`nros-platform-posix/src/platform.c`) and the
# selection-named TU (`nros_clock_board__set_by__knobs_clock_source.c`) with
# the defines `nros_zephyr_build::clock_selection` passes, into an archive
# shaped like the one `nros-platform-cffi`'s `posix-c-port` builds, and links
# a small program against it with a stub board hook.
#
# Cases (each its own process, since the port resolves once per process):
#   a. source = board, the hook answers a fixed instant: the wall clock reads
#      that instant plus the elapsed monotonic time (bounded on BOTH sides),
#      ADVANCES across a 20 ms sleep, the state is {synced, board, 0 steps},
#      and the hook was asked exactly once. The hook sits in an archive
#      linked AFTER the port's, the harder form of the link rule;
#   b. source = board,platform, the hook says "not synced": the fallback
#      latches `platform`, the reading sits between two CLOCK_REALTIME reads,
#      and the state is {synced, platform};
#   c. source = board, hook not synced, epoch_failure default (unstamped):
#      the reading is 0, the state {unsynced, board}, and the resolver said
#      so once on stderr;
#   d. source = board, epoch_failure = halt, hook not synced: the process
#      stops (SIGABRT, through nros_platform_panic) naming the source and the
#      knob;
#   e. the link rule: board selected and NO hook defined FAILS TO LINK, and
#      the error names `nros_board_wall_clock_ns` and an object whose name
#      says `set_by__knobs_clock_source`;
#   f. board not selected (no define): the port's objects neither define nor
#      reference the hook or the selection TU's function.
# Negative control: the same case a against a port whose lazy resolution
# call is deleted must FAIL, so a reading that is merely nonzero cannot pass.
set -uo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/lib/grep-q.sh
. "$root/scripts/lib/grep-q.sh"
# shellcheck source=scripts/lib/deadline.sh
. "$root/scripts/lib/deadline.sh"
cc="${CC:-cc}"
ar="${AR:-ar}"
command -v "$cc" >/dev/null || { echo "clock-board-posix: no C compiler ($cc)" >&2; exit 1; }
command -v "$ar" >/dev/null || { echo "clock-board-posix: no archiver ($ar)" >&2; exit 1; }
command -v nm >/dev/null || { echo "clock-board-posix: no nm" >&2; exit 1; }
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
inc="$root/packages/platform/nros-platform-api/include"
src="$root/packages/platform/nros-platform-posix/src"
tu=nros_clock_board__set_by__knobs_clock_source

cat > "$tmp/hook.c" <<'EOF'
#include <stdbool.h>
#include <stdint.h>
#define BOARD_NS 1700000000123456789ULL /* 2023-11-14 */
int hook_calls;
bool nros_board_wall_clock_ns(uint64_t* out_ns) {
    hook_calls++;
#if HOOK_ANSWERS
    *out_ns = BOARD_NS;
    return true;
#else
    (void)out_ns;
    return false;
#endif
}
EOF

cat > "$tmp/main.c" <<'EOF'
#include <nros/platform.h>
#include <stdio.h>
#include <time.h>
#define BOARD_NS 1700000000123456789ULL
extern int hook_calls;
static int bad;
#define CHECK(name, cond) do { if (cond) printf("  ok   %s\n", name); \
    else { printf("  FAIL %s\n", name); bad = 1; } } while (0)
__attribute__((unused)) static uint64_t realtime_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_REALTIME, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}
int main(void) {
#if defined(CASE_A)
    uint64_t m0 = nros_platform_clock_ns();
    uint64_t t1 = nros_platform_time_now_ns();
    uint64_t m1 = nros_platform_clock_ns();
    /* The offset is taken in microseconds, so a reading may sit up to 1 us
     * below the instant the hook answered. */
    CHECK("a reading >= the board's instant", t1 + 1000ULL >= BOARD_NS);
    CHECK("a reading <= the board's instant + elapsed", t1 <= BOARD_NS + (m1 - m0) + 1000ULL);
    struct timespec d = {0, 20 * 1000 * 1000};
    nanosleep(&d, NULL);
    uint64_t t2 = nros_platform_time_now_ns();
    CHECK("a advances: at least the 20 ms slept", t2 + 1000ULL >= t1 + 20000000ULL);
    CHECK("a advances: under 5 s", t2 < t1 + 5000000000ULL);
    nros_wall_clock_state_t st = nros_platform_wall_clock_state();
    CHECK("a state: synced", st.synced);
    CHECK("a state: source board", st.source == NROS_WALL_CLOCK_SOURCE_BOARD);
    CHECK("a state: no steps", st.steps == 0u);
    CHECK("a the hook was asked exactly once", hook_calls == 1);
#elif defined(CASE_B)
    uint64_t r0 = realtime_ns();
    uint64_t t = nros_platform_time_now_ns();
    uint64_t r1 = realtime_ns();
    CHECK("b reading >= CLOCK_REALTIME before", t >= r0);
    CHECK("b reading <= CLOCK_REALTIME after", t <= r1);
    nros_wall_clock_state_t st = nros_platform_wall_clock_state();
    CHECK("b state: synced", st.synced);
    CHECK("b state: source platform", st.source == NROS_WALL_CLOCK_SOURCE_PLATFORM);
    CHECK("b the board was tried first, once", hook_calls == 1);
#elif defined(CASE_C) || defined(CASE_D)
    uint64_t t = nros_platform_time_now_ns();
    CHECK("c reading is 0 (unstamped)", t == 0ULL);
    nros_wall_clock_state_t st = nros_platform_wall_clock_state();
    CHECK("c state: not synced", !st.synced);
    CHECK("c state: names board", st.source == NROS_WALL_CLOCK_SOURCE_BOARD);
    CHECK("c the hook was asked exactly once", hook_calls == 1);
#endif
    return bad;
}
EOF

# The port's archive: <name> <platform.c source> [defines...]. Members are the
# port's platform.o and termination.o, plus the selection TU when a define
# names board, as the cargo road compiles them.
port() {
    local name="$1" pc="$2"; shift 2
    local d="$tmp/$name"
    mkdir -p "$d"
    local flags=(-std=c11 -Wall -Wextra -Werror -Wpedantic -D_POSIX_C_SOURCE=200809L -I"$inc" "$@")
    "$cc" "${flags[@]}" -c "$pc" -o "$d/platform.o" 2>>"$d.err" || return 1
    "$cc" "${flags[@]}" -c "$src/termination.c" -o "$d/termination.o" 2>>"$d.err" || return 1
    local objs=("$d/platform.o" "$d/termination.o")
    case " $* " in
    *NROS_CLOCK_HAS_BOARD*)
        "$cc" "${flags[@]}" -c "$src/$tu.c" -o "$d/$tu.o" 2>>"$d.err" || return 1
        objs+=("$d/$tu.o")
        ;;
    esac
    "$ar" rcs "$d/libport.a" "${objs[@]}" 2>>"$d.err"
}
# A program: <out> <port name> <case> <hook answers: 1|0|none> [hook-as-archive]
prog() {
    local out="$1" name="$2" case="$3" hook="$4" hook_ar="${5:-}"
    "$cc" -std=c11 -Wall -Wextra -Werror -D_POSIX_C_SOURCE=200809L -I"$inc" -D"$case" \
        -c "$tmp/main.c" -o "$out.main.o" 2>"$out.err" || return 1
    local link=("$out.main.o")
    if [ "$hook" != none ]; then
        "$cc" -std=c11 -Wall -Wextra -Werror -DHOOK_ANSWERS="$hook" -c "$tmp/hook.c" \
            -o "$out.hook.o" 2>>"$out.err" || return 1
        if [ -n "$hook_ar" ]; then
            "$ar" rcs "$out.libhook.a" "$out.hook.o" 2>>"$out.err"
            link+=("$tmp/$name/libport.a" "$out.libhook.a")
        else
            link+=("$out.hook.o" "$tmp/$name/libport.a")
        fi
    else
        link+=("$tmp/$name/libport.a")
    fi
    "$cc" "${link[@]}" -o "$out" -lpthread 2>>"$out.err"
}
fail() { echo "clock-board-posix: $*" >&2; exit 1; }
need_build() { # <what> <err file> <command...>
    local what="$1" err="$2"; shift 2
    "$@" || { echo "clock-board-posix: $what did not build:" >&2; cat "$err" >&2; exit 1; }
}

board=('-DNROS_CLOCK_SOURCE_CSV="board"' -DNROS_CLOCK_HAS_BOARD=1)
need_build "the board port" "$tmp/board.err" port board "$src/platform.c" "${board[@]}"
need_build "the board,platform port" "$tmp/fallback.err" port fallback "$src/platform.c" \
    '-DNROS_CLOCK_SOURCE_CSV="board,platform"' -DNROS_CLOCK_HAS_BOARD=1
need_build "the halt port" "$tmp/halt.err" port halt "$src/platform.c" "${board[@]}" \
    '-DNROS_CLOCK_EPOCH_FAILURE="halt"'
need_build "the default port" "$tmp/default.err" port default "$src/platform.c"

run_case() { # <label> <binary>
    local rc=0 out
    out="$("${NROS_DEADLINE[@]}" 20 "$2" 2>&1)" || rc=$?
    printf '%s\n' "$out"
    [ "$rc" -eq 0 ] || fail "case $1 FAILED (exit $rc)"
}

echo "clock-board-posix: against the POSIX port"
need_build "case a" "$tmp/a.err" prog "$tmp/a" board CASE_A 1 archive
run_case a "$tmp/a"
need_build "case b" "$tmp/b.err" prog "$tmp/b" fallback CASE_B 0
run_case b "$tmp/b"
need_build "case c" "$tmp/c.err" prog "$tmp/c" board CASE_C 0
rc=0; out="$("${NROS_DEADLINE[@]}" 20 "$tmp/c" 2>"$tmp/c.stderr")" || rc=$?
printf '%s\n' "$out"
[ "$rc" -eq 0 ] || fail "case c FAILED (exit $rc)"
nros_grep_q -F 'no wall-clock source answered (board); stamps are unset' "$tmp/c.stderr"; m=$?
[ "$m" -lt 2 ] || fail "grep failed reading case c's stderr"
[ "$m" -eq 0 ] || { cat "$tmp/c.stderr" >&2; fail "case c: the resolver did not report the unstamped verdict"; }
n="$(grep -c 'nros: ' "$tmp/c.stderr")" || true
[ "$n" = 1 ] || { cat "$tmp/c.stderr" >&2; fail "case c: the verdict was reported $n times, not once"; }
echo "  ok   c reported once on stderr"

need_build "case d" "$tmp/d.err" prog "$tmp/d" halt CASE_D 0
# Captured, so bash's own "Aborted" job notice stays out of the log.
rc=0; dout="$("${NROS_DEADLINE[@]}" 20 "$tmp/d" 2>"$tmp/d.stderr")" || rc=$?
printf '%s\n' "$dout" >"$tmp/d.stdout"
[ "$rc" -eq 134 ] || { cat "$tmp/d.stdout" "$tmp/d.stderr" >&2; fail "case d: halt did not abort (exit $rc, want 134 = SIGABRT)"; }
nros_grep_q -F 'nros: PANIC nros: no wall-clock source answered (board); halting, as clock.epoch_failure = halt asks' "$tmp/d.stderr"; m=$?
[ "$m" -lt 2 ] || fail "grep failed reading case d's stderr"
[ "$m" -eq 0 ] || { cat "$tmp/d.stderr" >&2; fail "case d: the halt message does not name the source and the knob"; }
nros_grep_q -F 'ok   c reading' "$tmp/d.stdout"; m=$?
[ "$m" -eq 1 ] || fail "case d: the image ran past resolution under halt"
echo "  ok   d halt stops the image (SIGABRT), naming board and the knob"

# e. the link rule.
if prog "$tmp/e" board CASE_C none; then
    fail "case e: board selected with no hook LINKED; it must fail to link"
fi
nros_grep_q -F 'nros_board_wall_clock_ns' "$tmp/e.err"; m1=$?
nros_grep_q -F "$tu" "$tmp/e.err"; m2=$?
{ [ "$m1" -lt 2 ] && [ "$m2" -lt 2 ]; } || fail "grep failed reading case e's link output"
if [ "$m1" -ne 0 ] || [ "$m2" -ne 0 ]; then
    cat "$tmp/e.err" >&2
    fail "case e: the link error must name nros_board_wall_clock_ns and the $tu object"
fi
echo "  ok   e no hook: the link fails naming nros_board_wall_clock_ns in $tu.o"

# f. not selected: unchanged, the hook is neither defined nor referenced.
syms="$(nm "$tmp/default/libport.a")" || fail "nm failed on the default port"
nros_grep_q -E 'nros_board_wall_clock_ns|set_by__knobs_clock_source' <<<"$syms"; m=$?
[ "$m" -lt 2 ] || fail "grep failed reading nm output"
[ "$m" -eq 1 ] || fail "case f: the default port names the board hook or the selection TU"
nros_grep_q -F 'nros_board_wall_clock_ns' <<<"$(nm "$tmp/board/libport.a")"; m=$?
[ "$m" -eq 0 ] || fail "case f's control: the board port does not reference the hook, so f proves nothing"
echo "  ok   f not selected: no hook symbol in the port's objects"

# Negative control: a port that never runs the lazy resolution must fail a.
sed '/pthread_once(&s_wall_clock_once, nros_posix_clock_resolve_once);/d' "$src/platform.c" \
    > "$tmp/platform_mutant.c"
if cmp -s "$src/platform.c" "$tmp/platform_mutant.c"; then
    fail "negative control: the resolution call was not found to delete"
fi
need_build "the negative control" "$tmp/mutant.err" port mutant "$tmp/platform_mutant.c" "${board[@]}" \
    -Wno-unused-function -Wno-unused-variable
need_build "the negative control's case a" "$tmp/na.err" prog "$tmp/na" mutant CASE_A 1 archive
rc=0; out="$("${NROS_DEADLINE[@]}" 20 "$tmp/na" 2>&1)" || rc=$?
[ "$rc" -ne 0 ] || { printf '%s\n' "$out" >&2; fail "negative control: a port that never resolves passed case a"; }
nros_grep_q -F 'FAIL a reading >= the board' <<<"$out"; m=$?
[ "$m" -lt 2 ] || fail "grep failed reading the negative control's output"
[ "$m" -eq 0 ] || { printf '%s\n' "$out" >&2; fail "negative control: it failed, but not on the board instant"; }
echo "clock-board-posix: OK (negative control: no lazy resolution fails case a)"
