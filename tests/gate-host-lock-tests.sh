#!/usr/bin/env bash
# `scripts/ci/gate-host-lock.sh` (issue 2602): one `just ci gate` per host.
# Each case runs short stand-in gates (`sleep`, a marker file) against a
# throwaway lock dir (`XDG_RUNTIME_DIR` under a temp dir), and each has a
# negative control showing the case can fail: without the property, the same
# measurement comes out the other way.
#
# Issue 2763: no case asserts a wall-clock upper bound that host load can
# break. A case orders events the lock itself orders (a waiter's start against
# the holder's command END, recorded inside the lock), waits for a state the
# scripts print (the holder's `pid=` line, a waiter's "lock is held"), holds
# its holder until the waiter is seen waiting, or bounds by a margin many
# times the nominal time. Load can make a case slower, never wrong.
set -uo pipefail
cd "$(dirname "$0")/.."
here="$PWD"
# shellcheck source=scripts/lib/grep-q.sh
. scripts/lib/grep-q.sh
lock_sh="$here/scripts/ci/gate-host-lock.sh"
tmp="$(mktemp -d)"
pids=()
cleanup() {
    for p in "${pids[@]}"; do kill -9 "$p" 2>/dev/null; done
    pkill -9 -f "nros-gate-lock-test-$$" 2>/dev/null
    rm -rf "$tmp"
}
trap cleanup EXIT
fail=0
ok() { echo "  ok   $1"; }
bad() { echo "  FAIL $1"; fail=1; }
gt() { python3 -c "import sys; sys.exit(0 if float(sys.argv[1]) > float(sys.argv[2]) else 1)" "$1" "$2"; }

run_dir() { # <dir> -> a fresh XDG_RUNTIME_DIR
    mkdir -p "$1" && chmod 700 "$1"
}
# A stand-in gate: records start/end times, then sleeps.
gate() { # <xdg> <tag> <seconds> [extra env...]
    local xdg="$1" tag="$2" secs="$3"
    shift 3
    env XDG_RUNTIME_DIR="$xdg" _NROS_GATE_LOCK_TEST_REPORT_S=1 _NROS_GATE_LOCK_TEST_TICK_S=1 "$@" bash "$lock_sh" \
        bash -c "echo \$(date +%s.%N) > '$tmp/$tag.start'; (exec -a nros-gate-lock-test-$$ sleep $secs); echo \$(date +%s.%N) > '$tmp/$tag.cmdend'" \
        >"$tmp/$tag.out" 2>&1
    echo $? >"$tmp/$tag.rc"
    date +%s.%N >"$tmp/$tag.end"
}
wait_for() { # <file> <seconds>
    local i=0
    while [ ! -s "$1" ] && [ "$i" -lt $(( $2 * 20 )) ]; do sleep 0.05; i=$((i + 1)); done
    [ -s "$1" ]
}
wait_held() { # <xdg> <seconds>: the holder has written its `pid=` line
    local i=0
    while ! { [ -s "$1/nros-gate.lock" ] && nros_grep_q "^pid=" "$1/nros-gate.lock"; } \
        && [ "$i" -lt $(( $2 * 20 )) ]; do
        sleep 0.05; i=$((i + 1))
    done
    [ -s "$1/nros-gate.lock" ] && nros_grep_q "^pid=" "$1/nros-gate.lock"
}
wait_waiting() { # <out> <seconds>: the gate found the lock held and waits
    local i=0
    while ! { [ -s "$1" ] && nros_grep_q "gate lock is held: pid=" "$1"; } \
        && [ "$i" -lt $(( $2 * 20 )) ]; do
        sleep 0.05; i=$((i + 1))
    done
    [ -s "$1" ] && nros_grep_q "gate lock is held: pid=" "$1"
}
# End every stand-in gate's sleeper (the holders the cases release by hand).
# Anchored on the sleeper's argv0: the stand-in wrappers' own command lines
# carry the same text, and a waiting gate must survive its holder's release.
sleeper_re="^nros-gate-lock-test-$$(-orphan)? "
release() { pkill -9 -f "$sleeper_re" 2>/dev/null; }

# 1. A second gate waits, then runs. Negative control: two lock dirs (two
#    "hosts") overlap, so the measurement can see an overlap.
#    The holder runs until the waiter is seen waiting, so the case cannot pass
#    by the waiter arriving late.
x="$tmp/x1"; run_dir "$x"
gate "$x" a1 300 & pids+=($!)
wait_for "$tmp/a1.start" 60
gate "$x" b1 0.1 & pids+=($!)
wait_waiting "$tmp/b1.out" 60
release
wait
if [ -s "$tmp/b1.start" ] && [ -s "$tmp/a1.cmdend" ] && gt "$(cat "$tmp/b1.start")" "$(cat "$tmp/a1.cmdend")"; then
    ok "a second gate waits for the first, then runs"
else bad "a second gate waits for the first, then runs"; fi
nros_grep_q "gate lock is held: pid=" "$tmp/b1.out" && ok "the waiter names the holder" \
    || bad "the waiter names the holder"
run_dir "$tmp/x1a"; run_dir "$tmp/x1b"
gate "$tmp/x1a" c1 300 & pids+=($!)
wait_for "$tmp/c1.start" 60
gate "$tmp/x1b" d1 300 & pids+=($!)
wait_for "$tmp/d1.start" 60
if [ -s "$tmp/d1.start" ] && [ ! -e "$tmp/c1.cmdend" ]; then
    ok "negative control: two separate locks overlap"
else bad "negative control: two separate locks overlap"; fi
release
wait

# 2. kill -9 of the holder lets the waiter in while the holder's command is
#    still running: the kernel releases the lock with the holder, not when the
#    command ends. Negative control: with the holder alive, the waiter is seen
#    waiting and has not started.
x="$tmp/x2"; run_dir "$x"
gate "$x" a2 300 & pids+=($!)
wait_for "$tmp/a2.start" 60
gate "$x" b2 0.1 & pids+=($!)
wait_waiting "$tmp/b2.out" 60
[ ! -e "$tmp/b2.start" ] && ok "negative control: the waiter waits while the holder lives" \
    || bad "negative control: the waiter waits while the holder lives"
holder="$(sed -n 's/^pid=\([0-9]*\).*/\1/p' "$x/nros-gate.lock")"
kill -9 "$holder" 2>/dev/null
wait_for "$tmp/b2.start" 120
if [ -s "$tmp/b2.start" ] && [ ! -e "$tmp/a2.cmdend" ]; then
    ok "kill -9 of the holder lets the waiter in before the holder's command ends"
else bad "kill -9 of the holder lets the waiter in before the holder's command ends"; fi
release
wait

# 3. An orphaned child does not hold the lock, and the gate does not wait for
#    it. The gate backgrounds a long sleeper, which inherits the gate's output
#    pipe, and exits. Measured while the orphan still runs: the gate has
#    returned, the lock is free, and the next gate gets in without waiting.
#    Negative control: a bare `flock` WITHOUT `-o` passes the descriptor to the
#    orphan, which then holds the lock after its gate has exited.
x="$tmp/x3"; run_dir "$x"
env XDG_RUNTIME_DIR="$x" bash "$lock_sh" \
    bash -c "(exec -a nros-gate-lock-test-$$-orphan sleep 300) & disown; exit 0" >/dev/null 2>&1
pgrep -f "^nros-gate-lock-test-$$-orphan " >/dev/null \
    && ok "the gate returns while its orphan still runs" \
    || bad "the gate returns while its orphan still runs"
flock -n "$x/nros-gate.lock" true \
    && ok "an orphaned child does not hold the lock" \
    || bad "an orphaned child does not hold the lock"
gate "$x" b3 0.1 _NROS_GATE_LOCK_TEST_WAIT_S=60
if [ "$(cat "$tmp/b3.rc")" = 0 ] && [ -s "$tmp/b3.start" ] && ! nros_grep_q "gate lock is held" "$tmp/b3.out"; then
    ok "the next gate gets in without waiting for the orphan"
else bad "the next gate gets in without waiting for the orphan"; fi
release
nl="$tmp/x3/bare.lock"
flock "$nl" bash -c "(exec -a nros-gate-lock-test-$$ sleep 30) & disown; exit 0"
if ! flock -n "$nl" true; then
    ok "negative control: without -o, the orphan holds the lock"
else bad "negative control: without -o, the orphan holds the lock"; fi
pkill -9 -f "nros-gate-lock-test-$$" 2>/dev/null

# 4. A hung holder times out, exits red "gate timed out", and the waiter gets
#    in. Negative control: with a long timeout the waiter is still out.
x="$tmp/x4"; run_dir "$x"
gate "$x" a4 300 _NROS_GATE_LOCK_TEST_WALL_S=2 & pids+=($!)
wait_for "$tmp/a4.start" 60
gate "$x" b4 0.1 & pids+=($!)
wait
# A LOWER bound, which load can only widen: the waiter started no earlier
# than the holder's wall limit.
[ -s "$tmp/b4.start" ] && gt "$(cat "$tmp/b4.start")" "$(python3 -c "print($(cat "$tmp/a4.start") + 2)")" \
    && ok "negative control: the waiter is out before the holder's timeout" \
    || bad "negative control: the waiter is out before the holder's timeout"
if [ "$(cat "$tmp/a4.rc")" = 1 ] && nros_grep_q "gate timed out" "$tmp/a4.out" \
    && [ -s "$tmp/b4.start" ] && [ "$(cat "$tmp/b4.rc")" = 0 ]; then
    ok "a hung holder times out red, and the waiter gets in"
else bad "a hung holder times out red, and the waiter gets in"; fi
pgrep -f "nros-gate-lock-test-$$" >/dev/null \
    && bad "the timed-out gate's process group is gone" \
    || ok "the timed-out gate's process group is gone"

# 5. The waiter's cap expires red, naming the holder, and its gate never runs.
#    Negative control: a cap longer than the holder lets it run.
x="$tmp/x5"; run_dir "$x"
gate "$x" a5 300 & pids+=($!)
wait_for "$tmp/a5.start" 60
gate "$x" b5 0.1 _NROS_GATE_LOCK_TEST_WAIT_S=1
if [ "$(cat "$tmp/b5.rc")" = 1 ] && [ ! -e "$tmp/b5.start" ] \
    && nros_grep_q "NOT RUN — waited 1 s for this host's gate lock, still held by: pid=" "$tmp/b5.out" \
    && nros_grep_q "ask the user" "$tmp/b5.out"; then
    ok "the waiter's cap expires red, naming the holder, and never runs"
else bad "the waiter's cap expires red, naming the holder, and never runs"; fi
gate "$x" c5 0.1 _NROS_GATE_LOCK_TEST_WAIT_S=120 & pids+=($!)
wait_waiting "$tmp/c5.out" 60
release
wait_for "$tmp/c5.rc" 120
[ "$(cat "$tmp/c5.rc")" = 0 ] && [ -s "$tmp/c5.start" ] \
    && ok "negative control: a longer cap lets the waiter run" \
    || bad "negative control: a longer cap lets the waiter run"
wait

# 6. A zero wait is a refusal, never an unlocked run (no knob skips the lock).
x="$tmp/x6"; run_dir "$x"
gate "$x" a6 300 & pids+=($!)
wait_for "$tmp/a6.start" 60
gate "$x" b6 0.1 _NROS_GATE_LOCK_TEST_WAIT_S=0
[ "$(cat "$tmp/b6.rc")" = 1 ] && [ ! -e "$tmp/b6.start" ] \
    && ok "a zero wait refuses rather than running unlocked" \
    || bad "a zero wait refuses rather than running unlocked"
release
wait

# A stand-in gate running an arbitrary command (no lock contention needed).
cmd_gate() { # <xdg> <tag> <command> [extra env...]
    local xdg="$1" tag="$2" cmd="$3"
    shift 3
    local t0; t0="$(date +%s)"
    env XDG_RUNTIME_DIR="$xdg" _NROS_GATE_LOCK_TEST_TICK_S=1 "$@" bash "$lock_sh" \
        bash -c "exec -a nros-gate-lock-test-$$ bash -c '$cmd'" >"$tmp/$tag.out" 2>&1
    echo $? >"$tmp/$tag.rc"
    echo $(( $(date +%s) - t0 )) >"$tmp/$tag.secs"
}

# 7. The hang detector spares a gate that is silent but burning CPU.
#    Negative control (case 8): the same silence with no CPU is killed.
x="$tmp/x7"; run_dir "$x"
cmd_gate "$x" a7 'end=$((SECONDS + 5)); while [ $SECONDS -lt $end ]; do :; done' _NROS_GATE_LOCK_TEST_HANG_S=2
[ "$(cat "$tmp/a7.rc")" = 0 ] && ! nros_grep_q "gate hung" "$tmp/a7.out" \
    && ok "a silent gate that burns CPU outlives the hang limit" \
    || bad "a silent gate that burns CPU outlives the hang limit"

# 8. A gate silent with NO CPU is killed at the hang limit, exits red "gate
#    hung", and frees the lock. Negative control: a gate that prints while it
#    idles is alive.
x="$tmp/x8"; run_dir "$x"
cmd_gate "$x" a8 'sleep 600' _NROS_GATE_LOCK_TEST_HANG_S=2
if [ "$(cat "$tmp/a8.rc")" = 1 ] && nros_grep_q "gate hung: no progress" "$tmp/a8.out" \
    && [ "$(cat "$tmp/a8.secs")" -lt 300 ]; then
    ok "a silent gate with no CPU is killed as hung, red"
else bad "a silent gate with no CPU is killed as hung, red"; fi
gate "$x" b8 0.1 _NROS_GATE_LOCK_TEST_WAIT_S=60
[ "$(cat "$tmp/b8.rc")" = 0 ] && ok "a hung gate's kill frees the lock" || bad "a hung gate's kill frees the lock"
pgrep -f "nros-gate-lock-test-$$" >/dev/null \
    && bad "the hung gate's process tree is gone" || ok "the hung gate's process tree is gone"
cmd_gate "$x" c8 'for i in 1 2 3 4 5 6 7 8 9 10; do echo tick; sleep 0.5; done' _NROS_GATE_LOCK_TEST_HANG_S=2
[ "$(cat "$tmp/c8.rc")" = 0 ] && ok "negative control: a gate that prints while idle is alive" \
    || bad "negative control: a gate that prints while idle is alive"

# 9. The wall limit scales with the host: 2.5 h x max(1, 16 / nproc), and the
#    waiter cap is that + 30 min. Both are printed when the gate starts.
x="$tmp/x9"; run_dir "$x"
for spec in 4:36000 8:18000 16:9000 48:9000; do
    n="${spec%%:*}" want="${spec#*:}"
    cmd_gate "$x" "n$n" 'true' "_NROS_GATE_LOCK_TEST_NPROC=$n"
    # Issue 2733: the cap also covers the 900 s housekeeping limit.
    cap=$((want + 900 + 1800))
    if nros_grep_q "wall limit $want s ($((want / 60)) min, $n cores), hang limit 45 min without output or CPU, housekeeping limit 900 s outside both, waiter cap $cap s" "$tmp/n$n.out"; then
        ok "on $n cores the wall limit is $want s and the waiter cap $cap s"
    else bad "on $n cores the wall limit is $want s and the waiter cap $cap s ($(head -1 "$tmp/n$n.out"))"; fi
done

# Issue 2733: housekeeping runs after the verdict, outside the wall limit and
# the hang detector, under its own limit, and never decides the exit code.
# A stand-in gate with a verdict command and a housekeeping command.
hk_gate() { # <xdg> <tag> <verdict> <housekeeping> [extra env...]
    local xdg="$1" tag="$2" verdict="$3" hk="$4"
    shift 4
    local t0; t0="$(date +%s)"
    env XDG_RUNTIME_DIR="$xdg" _NROS_GATE_LOCK_TEST_TICK_S=1 "$@" bash "$lock_sh" \
        --housekeeping "exec -a nros-gate-lock-test-$$ bash -c '$hk'" \
        bash -c "exec -a nros-gate-lock-test-$$ bash -c '$verdict'" >"$tmp/$tag.out" 2>&1
    echo $? >"$tmp/$tag.rc"
    echo $(( $(date +%s) - t0 )) >"$tmp/$tag.secs"
}

# 10. Slow housekeeping (silent, no CPU, longer than both the wall limit and
#     the hang limit) leaves rc 0 when the verdict passed: the 2026-11-19 run.
#     Negative control: the same sleep at the TAIL of the verdict command, the
#     old layout, is killed red at the wall limit.
x="$tmp/x10"; run_dir "$x"
hk_gate "$x" a10 'echo verdict ok' 'sleep 4; echo hk done' \
    _NROS_GATE_LOCK_TEST_WALL_S=2 _NROS_GATE_LOCK_TEST_HANG_S=2 _NROS_GATE_LOCK_TEST_HOUSEKEEPING_S=20
if [ "$(cat "$tmp/a10.rc")" = 0 ] && nros_grep_q "hk done" "$tmp/a10.out" \
    && ! nros_grep_q "WARNING" "$tmp/a10.out" && ! nros_grep_q "timed out" "$tmp/a10.out"; then
    ok "slow housekeeping past the wall and hang limits leaves a passed gate at rc 0"
else bad "slow housekeeping past the wall and hang limits leaves a passed gate at rc 0 (rc $(cat "$tmp/a10.rc"))"; fi
cmd_gate "$x" b10 'echo verdict ok; sleep 4; echo hk done' _NROS_GATE_LOCK_TEST_WALL_S=2
[ "$(cat "$tmp/b10.rc")" = 1 ] && nros_grep_q "gate timed out" "$tmp/b10.out" \
    && ok "negative control: housekeeping inside the verdict is killed red at the wall" \
    || bad "negative control: housekeeping inside the verdict is killed red at the wall"

# 11. A failing verdict is still non-zero, with its own status, after
#     housekeeping that passes. Negative control: the same verdict passing is 0.
x="$tmp/x11"; run_dir "$x"
hk_gate "$x" a11 'exit 3' 'echo hk done'
[ "$(cat "$tmp/a11.rc")" = 3 ] && nros_grep_q "hk done" "$tmp/a11.out" \
    && ok "a failing verdict keeps its status after housekeeping" \
    || bad "a failing verdict keeps its status after housekeeping (rc $(cat "$tmp/a11.rc"))"
hk_gate "$x" b11 'exit 0' 'echo hk done'
[ "$(cat "$tmp/b11.rc")" = 0 ] && ok "negative control: the passing verdict is 0" \
    || bad "negative control: the passing verdict is 0"

# 12. A housekeeping failure is rc 0 with a warning; a housekeeping that
#     outlives its own limit is killed and is rc 0 with a warning, and it does
#     not run on. Negative control: the passing housekeeping (case 10) warns
#     nothing.
x="$tmp/x12"; run_dir "$x"
hk_gate "$x" a12 'true' 'exit 7'
[ "$(cat "$tmp/a12.rc")" = 0 ] && nros_grep_q "WARNING: housekeeping FAILED (exit 7" "$tmp/a12.out" \
    && ok "a housekeeping failure warns and leaves rc 0" \
    || bad "a housekeeping failure warns and leaves rc 0 (rc $(cat "$tmp/a12.rc"))"
hk_gate "$x" b12 'true' 'sleep 600' _NROS_GATE_LOCK_TEST_HOUSEKEEPING_S=2
if [ "$(cat "$tmp/b12.rc")" = 0 ] && nros_grep_q "WARNING: housekeeping timed out after 2 s" "$tmp/b12.out" \
    && [ "$(cat "$tmp/b12.secs")" -lt 300 ]; then
    ok "a housekeeping timeout is killed at its own limit, warns, and leaves rc 0"
else bad "a housekeeping timeout is killed at its own limit, warns, and leaves rc 0 (rc $(cat "$tmp/b12.rc"))"; fi
pgrep -f "nros-gate-lock-test-$$" >/dev/null \
    && bad "the timed-out housekeeping is gone" || ok "the timed-out housekeeping is gone"

# 13. The wall limit and the hang detector still bind the verdict when
#     housekeeping is configured, and the housekeeping still runs after them.
x="$tmp/x13"; run_dir "$x"
hk_gate "$x" a13 'sleep 60' 'echo hk done' _NROS_GATE_LOCK_TEST_WALL_S=2
[ "$(cat "$tmp/a13.rc")" = 1 ] && nros_grep_q "gate timed out" "$tmp/a13.out" && nros_grep_q "hk done" "$tmp/a13.out" \
    && ok "the wall limit still kills a slow verdict red, then housekeeping runs" \
    || bad "the wall limit still kills a slow verdict red, then housekeeping runs"
hk_gate "$x" b13 'sleep 60' 'echo hk done' _NROS_GATE_LOCK_TEST_HANG_S=2
[ "$(cat "$tmp/b13.rc")" = 1 ] && nros_grep_q "gate hung: no progress" "$tmp/b13.out" && nros_grep_q "hk done" "$tmp/b13.out" \
    && ok "the hang detector still kills an idle verdict red, then housekeeping runs" \
    || bad "the hang detector still kills an idle verdict red, then housekeeping runs"

# 14. Housekeeping holds the lock: a second gate waits for it. Negative
#     control (case 1's): two lock dirs overlap.
x="$tmp/x14"; run_dir "$x"
hk_gate "$x" a14 'true' "sleep 2; date +%s.%N > $tmp/a14.hkend" & pids+=($!)
wait_held "$x" 60
gate "$x" b14 0.1
wait
if [ -s "$tmp/b14.start" ] && [ -s "$tmp/a14.hkend" ] && gt "$(cat "$tmp/b14.start")" "$(cat "$tmp/a14.hkend")"; then
    ok "housekeeping holds the lock; the next gate waits for it"
else bad "housekeeping holds the lock; the next gate waits for it"; fi

# 15. The one place: `just ci gate` names its housekeeping through the lock,
#     and the verdict recipe runs none of it. Read from just/ci.just itself.
ci_just="$here/just/ci.just"
body() { # <recipe>: its body lines
    awk -v r="$1:" '$0 == r { on = 1; next } on && /^[^ \t]/ { exit } on' "$ci_just"
}
body gate >"$tmp/gate.body"
body _gate-inner >"$tmp/inner.body"
body _gate-housekeeping >"$tmp/hk.body"
if nros_grep_q -F -- "--housekeeping 'just ci::_gate-housekeeping' just ci::_gate-inner" "$tmp/gate.body" \
    && nros_grep_q -F "trim-caches.py" "$tmp/hk.body" && nros_grep_q -F "worktree-prune.py" "$tmp/hk.body" \
    && nros_grep_q -F "run-lane-steps.sh" "$tmp/inner.body" \
    && ! nros_grep_q -E "trim-caches|worktree-prune" "$tmp/inner.body"; then
    ok "\`just ci gate\` puts its housekeeping outside the verdict"
else bad "\`just ci gate\` puts its housekeeping outside the verdict"; fi

if [ "$fail" -ne 0 ]; then
    echo "gate-host-lock tests: FAILED" >&2
    exit 1
fi
echo "gate-host-lock tests: OK"
