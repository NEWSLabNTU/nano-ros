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
# Cases a-f state `clock.fallback_settle_ms = 0` (ask once). The settle window
# (manager ruling A: `board` behaves like `sntp`):
#   g. source = board, window 1000 ms, a hook that says "not synced" for its
#      first 200 ms and then answers: installed, {synced, board}, the first
#      read took at least 200 ms and less than the window, and the hook was
#      asked more than once (polled) -- and NOT AGAIN on 100 later reads;
#   h. source = board, window 300 ms, a hook that never answers, default
#      `unstamped`: reads 0, {unsynced, board}, the first read returned after
#      the window and well before forever, and later reads ask nothing;
#   i. the same with `halt`: the process stops (SIGABRT) after the window.
# The re-ask (`clock.board_resync_interval_ms`), with a hook that answers the
# instant on its first call and the instant + 1 h on every later one (a board
# clock its own time service stepped):
#   j. interval 100 ms, stack 4096 B (a floor): 600 ms on, the reading has
#      followed the step, the hook was re-asked, the state is {synced, board};
#   k. no interval: the hook is asked once and the step never reaches a read.
# Negative controls: case j against a port that never starts the re-ask must
# FAIL; the same case a against a port whose lazy resolution
# call is deleted must FAIL, so a reading that is merely nonzero cannot pass;
# case g against a selection TU that ignores the window (asks once) must
# FAIL; case g against a port that resolves on EVERY read must FAIL (the
# per-read hook call the ruling rejects).
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
# One selection TU for every port (`nros_zephyr_build::clock_selection::board_tu`).
tu_src="$root/packages/platform/nros-platform-api/c/$tu.c"

# HOOK_ANSWERS: 1 always, 0 never, 2 only from HOOK_AFTER_MS after its first
# call (a board whose clock syncs late), 3 the instant first and the instant
# + 1 h after (a board clock stepped after boot).
cat > "$tmp/hook.c" <<'EOF'
#include <stdbool.h>
#include <stdint.h>
#include <time.h>
#define BOARD_NS 1700000000123456789ULL /* 2023-11-14 */
#ifndef HOOK_AFTER_MS
#define HOOK_AFTER_MS 200
#endif
int hook_calls;
__attribute__((unused)) static uint64_t mono_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}
bool nros_board_wall_clock_ns(uint64_t* out_ns) {
    hook_calls++;
#if HOOK_ANSWERS == 1
    *out_ns = BOARD_NS;
    return true;
#elif HOOK_ANSWERS == 2
    static uint64_t first_ns;
    if (first_ns == 0ULL) {
        first_ns = mono_ns();
    }
    if (mono_ns() - first_ns < (uint64_t)HOOK_AFTER_MS * 1000000ULL) {
        return false;
    }
    *out_ns = BOARD_NS;
    return true;
#elif HOOK_ANSWERS == 3
    *out_ns = hook_calls == 1 ? BOARD_NS : BOARD_NS + 3600000000000ULL;
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
#elif defined(CASE_G)
    uint64_t m0 = nros_platform_clock_ns();
    uint64_t t = nros_platform_time_now_ns();
    uint64_t m1 = nros_platform_clock_ns();
    printf("  g first read took %llu ms, hook asked %d times\n",
           (unsigned long long)((m1 - m0) / 1000000ULL), hook_calls);
    CHECK("g the first read waited for the board (>= 200 ms)", m1 - m0 >= 200000000ULL);
    CHECK("g ...and less than the 1000 ms window", m1 - m0 < 1000000000ULL);
    CHECK("g reading is the board's instant", t + 1000ULL >= BOARD_NS && t <= BOARD_NS + 50000000ULL);
    nros_wall_clock_state_t st = nros_platform_wall_clock_state();
    CHECK("g state: synced", st.synced);
    CHECK("g state: source board", st.source == NROS_WALL_CLOCK_SOURCE_BOARD);
    CHECK("g the hook was polled, not asked once", hook_calls >= 2);
    int calls = hook_calls;
    for (int i = 0; i < 100; i++) {
        (void)nros_platform_time_now_ns();
        (void)nros_platform_wall_clock_state();
    }
    CHECK("g later reads never ask the hook again", hook_calls == calls);
#elif defined(CASE_H)
    uint64_t m0 = nros_platform_clock_ns();
    uint64_t t = nros_platform_time_now_ns();
    uint64_t m1 = nros_platform_clock_ns();
    printf("  h first read took %llu ms, hook asked %d times\n",
           (unsigned long long)((m1 - m0) / 1000000ULL), hook_calls);
    CHECK("h the first read waited out the 300 ms window", m1 - m0 >= 300000000ULL);
    CHECK("h ...and then returned (under 2 s)", m1 - m0 < 2000000000ULL);
    CHECK("h reading is 0 (unstamped)", t == 0ULL);
    nros_wall_clock_state_t st = nros_platform_wall_clock_state();
    CHECK("h state: not synced", !st.synced);
    CHECK("h state: names board", st.source == NROS_WALL_CLOCK_SOURCE_BOARD);
    int calls = hook_calls;
    for (int i = 0; i < 100; i++) {
        (void)nros_platform_time_now_ns();
    }
    CHECK("h later reads never ask the hook again", hook_calls == calls);
#elif defined(CASE_J) || defined(CASE_K)
    uint64_t t0 = nros_platform_time_now_ns();
    CHECK("jk the first reading is the first answer",
          t0 + 1000ULL >= BOARD_NS && t0 < BOARD_NS + 1000000000ULL);
    struct timespec d = {0, 600 * 1000 * 1000};
    nanosleep(&d, NULL);
    uint64_t t = nros_platform_time_now_ns();
    nros_wall_clock_state_t st = nros_platform_wall_clock_state();
    printf("  jk read 600 ms on: %llu, hook asked %d times\n", (unsigned long long)t, hook_calls);
    CHECK("jk state: synced, board", st.synced && st.source == NROS_WALL_CLOCK_SOURCE_BOARD);
#if defined(CASE_J)
    CHECK("j the board was re-asked", hook_calls >= 3);
    CHECK("j the reading followed the board's step (+1 h)",
          t >= BOARD_NS + 3600000000000ULL && t < BOARD_NS + 3600000000000ULL + 5000000000ULL);
#else
    CHECK("k the board was asked once", hook_calls == 1);
    CHECK("k the reading never followed the step", t < BOARD_NS + 5000000000ULL);
#endif
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
        "$cc" "${flags[@]}" -c "${PORT_TU_SRC:-$tu_src}" -o "$d/$tu.o" 2>>"$d.err" || return 1
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
        "$cc" -std=c11 -Wall -Wextra -Werror -D_POSIX_C_SOURCE=200809L -DHOOK_ANSWERS="$hook" -c "$tmp/hook.c" \
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

board=('-DNROS_CLOCK_SOURCE_CSV="board"' -DNROS_CLOCK_HAS_BOARD=1 -DNROS_CLOCK_FALLBACK_SETTLE_MS=0u)
need_build "the board port" "$tmp/board.err" port board "$src/platform.c" "${board[@]}"
need_build "the board,platform port" "$tmp/fallback.err" port fallback "$src/platform.c" \
    '-DNROS_CLOCK_SOURCE_CSV="board,platform"' -DNROS_CLOCK_HAS_BOARD=1 -DNROS_CLOCK_FALLBACK_SETTLE_MS=0u
need_build "the halt port" "$tmp/halt.err" port halt "$src/platform.c" "${board[@]}" \
    '-DNROS_CLOCK_EPOCH_FAILURE="halt"'
need_build "the default port" "$tmp/default.err" port default "$src/platform.c"
win1000=('-DNROS_CLOCK_SOURCE_CSV="board"' -DNROS_CLOCK_HAS_BOARD=1 -DNROS_CLOCK_FALLBACK_SETTLE_MS=1000u)
win300=('-DNROS_CLOCK_SOURCE_CSV="board"' -DNROS_CLOCK_HAS_BOARD=1 -DNROS_CLOCK_FALLBACK_SETTLE_MS=300u)
need_build "the 1000 ms window port" "$tmp/win1000.err" port win1000 "$src/platform.c" "${win1000[@]}"
need_build "the 300 ms window port" "$tmp/win300.err" port win300 "$src/platform.c" "${win300[@]}"
need_build "the 300 ms halt port" "$tmp/halt300.err" port halt300 "$src/platform.c" "${win300[@]}" \
    '-DNROS_CLOCK_EPOCH_FAILURE="halt"'

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
# g/h/i: the settle window.
need_build "case g" "$tmp/g.err" prog "$tmp/g" win1000 CASE_G 2
run_case g "$tmp/g"
need_build "case h" "$tmp/h.err" prog "$tmp/h" win300 CASE_H 0
run_case h "$tmp/h"
need_build "case i" "$tmp/i.err" prog "$tmp/i" halt300 CASE_H 0
# Captured, so bash's own "Aborted" job notice stays out of the log.
rc=0; s0=$(date +%s%N); iout="$("${NROS_DEADLINE[@]}" 20 "$tmp/i" 2>"$tmp/i.stderr")" || rc=$?
printf '%s\n' "$iout" >"$tmp/i.stdout"
el_ms=$(( ($(date +%s%N) - s0) / 1000000 ))
[ "$rc" -eq 134 ] || { cat "$tmp/i.stdout" "$tmp/i.stderr" >&2; fail "case i: halt after the window did not abort (exit $rc)"; }
[ "$el_ms" -ge 300 ] || fail "case i: halted after ${el_ms} ms, before the 300 ms window ran out"
nros_grep_q -F 'no wall-clock source answered (board); halting' "$tmp/i.stderr"; m=$?
[ "$m" -lt 2 ] || fail "grep failed reading case i's stderr"
[ "$m" -eq 0 ] || { cat "$tmp/i.stderr" >&2; fail "case i: the halt message does not name board"; }
echo "  ok   i halt after the 300 ms window (${el_ms} ms), naming board"

# Negative control: a selection TU that ignores the window (asks once) fails g.
sed 's/const uint64_t budget_ns = (uint64_t)settle_ms \* 1000000ULL;/const uint64_t budget_ns = 0ULL * settle_ms;/' \
    "$tu_src" >"$tmp/tu_once.c"
cmp -s "$tu_src" "$tmp/tu_once.c" && fail "negative control: the settle budget was not found to mutate"
PORT_TU_SRC="$tmp/tu_once.c" need_build "the ask-once mutant" "$tmp/once.err" port once "$src/platform.c" "${win1000[@]}"
need_build "the ask-once mutant's case g" "$tmp/ng.err" prog "$tmp/ng" once CASE_G 2
rc=0; out="$("${NROS_DEADLINE[@]}" 20 "$tmp/ng" 2>&1)" || rc=$?
[ "$rc" -ne 0 ] || { printf '%s\n' "$out" >&2; fail "negative control: a TU that ignores the window passed case g"; }
nros_grep_q -F 'FAIL g the first read waited for the board' <<<"$out"; m=$?
[ "$m" -eq 0 ] || { printf '%s\n' "$out" >&2; fail "negative control: the ask-once mutant failed, but not on the wait"; }

# Negative control: a port that resolves on every read asks the hook per read.
python3 - "$src/platform.c" "$tmp/platform_perread.c" <<'PY'
import sys
s = open(sys.argv[1]).read()
needle = "uint64_t nros_platform_time_now_ns(void) {\n    pthread_once(&s_wall_clock_once, nros_posix_clock_resolve_once);"
if needle not in s:
    sys.exit("the time_now resolution call was not found")
s = s.replace(needle, "uint64_t nros_platform_time_now_ns(void) {\n    nros_posix_clock_resolve_once();")
open(sys.argv[2], "w").write(s)
PY
[ -s "$tmp/platform_perread.c" ] || fail "negative control: could not build the per-read mutant"
need_build "the per-read mutant" "$tmp/perread.err" port perread "$tmp/platform_perread.c" "${win1000[@]}" \
    -Wno-unused-function -Wno-unused-variable
need_build "the per-read mutant's case g" "$tmp/npr.err" prog "$tmp/npr" perread CASE_G 2
rc=0; out="$("${NROS_DEADLINE[@]}" 20 "$tmp/npr" 2>&1)" || rc=$?
[ "$rc" -ne 0 ] || { printf '%s\n' "$out" >&2; fail "negative control: a per-read resolving port passed case g"; }
nros_grep_q -F 'FAIL g later reads never ask the hook again' <<<"$out"; m=$?
[ "$m" -eq 0 ] || { printf '%s\n' "$out" >&2; fail "negative control: the per-read mutant failed, but not on the later reads"; }
# j/k: the re-ask.
resync=('-DNROS_CLOCK_SOURCE_CSV="board"' -DNROS_CLOCK_HAS_BOARD=1 -DNROS_CLOCK_FALLBACK_SETTLE_MS=0u \
    -DNROS_CLOCK_BOARD_RESYNC_INTERVAL_MS=100u -DNROS_CLOCK_BOARD_RESYNC_STACK_BYTES=4096u)
need_build "the re-ask port" "$tmp/resync.err" port resync "$src/platform.c" "${resync[@]}"
need_build "case j" "$tmp/j.err" prog "$tmp/j" resync CASE_J 3
run_case j "$tmp/j"
need_build "case k" "$tmp/k.err" prog "$tmp/k" board CASE_K 3
run_case k "$tmp/k"

# Negative control: a port that never starts the re-ask fails j.
python3 - "$src/platform.c" "$tmp/platform_noreask.c" <<'PY'
import sys
s = open(sys.argv[1]).read()
n = "    nros_clock_board_resync_start__set_by__knobs_clock_source();\n"
if s.count(n) != 1:
    sys.exit("the re-ask start was not found")
open(sys.argv[2], "w").write(s.replace(n, ""))
PY
[ -s "$tmp/platform_noreask.c" ] || fail "negative control: could not build the no-re-ask mutant"
need_build "the no-re-ask mutant" "$tmp/noreask.err" port noreask "$tmp/platform_noreask.c" "${resync[@]}"
need_build "the no-re-ask mutant's case j" "$tmp/nj.err" prog "$tmp/nj" noreask CASE_J 3
rc=0; out="$("${NROS_DEADLINE[@]}" 20 "$tmp/nj" 2>&1)" || rc=$?
[ "$rc" -ne 0 ] || { printf '%s\n' "$out" >&2; fail "negative control: a port that never re-asks passed case j"; }
nros_grep_q -F 'FAIL j the board was re-asked' <<<"$out"; m=$?
[ "$m" -eq 0 ] || { printf '%s\n' "$out" >&2; fail "negative control: the no-re-ask mutant failed, but not on the re-ask"; }
echo "clock-board-posix: OK (negative controls: no lazy resolution fails a; ask-once and per-read resolution fail g; no re-ask fails j)"
