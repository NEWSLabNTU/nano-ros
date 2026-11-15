#!/usr/bin/env bash
# `scripts/ci/gate-host-lock.sh` (issue 2602): one `just ci gate` per host.
# Each case runs short stand-in gates (`sleep`, a marker file) against a
# throwaway lock dir (`XDG_RUNTIME_DIR` under a temp dir), and each has a
# negative control showing the case can fail: without the property, the same
# measurement comes out the other way.
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
now() { date +%s.%N; }
gt() { python3 -c "import sys; sys.exit(0 if float(sys.argv[1]) > float(sys.argv[2]) else 1)" "$1" "$2"; }

run_dir() { # <dir> -> a fresh XDG_RUNTIME_DIR
    mkdir -p "$1" && chmod 700 "$1"
}
# A stand-in gate: records start/end times, then sleeps.
gate() { # <xdg> <tag> <seconds> [extra env...]
    local xdg="$1" tag="$2" secs="$3"
    shift 3
    env XDG_RUNTIME_DIR="$xdg" _NROS_GATE_LOCK_TEST_REPORT_S=1 _NROS_GATE_LOCK_TEST_TICK_S=1 "$@" bash "$lock_sh" \
        bash -c "echo \$(date +%s.%N) > '$tmp/$tag.start'; exec -a nros-gate-lock-test-$$ sleep $secs; " \
        >"$tmp/$tag.out" 2>&1
    echo $? >"$tmp/$tag.rc"
    date +%s.%N >"$tmp/$tag.end"
}
wait_for() { # <file> <seconds>
    local i=0
    while [ ! -s "$1" ] && [ "$i" -lt $(( $2 * 20 )) ]; do sleep 0.05; i=$((i + 1)); done
    [ -s "$1" ]
}

# 1. A second gate waits, then runs. Negative control: two lock dirs (two
#    "hosts") overlap, so the measurement can see an overlap.
x="$tmp/x1"; run_dir "$x"
gate "$x" a1 2 & pids+=($!)
wait_for "$tmp/a1.start" 5
gate "$x" b1 0.1 & pids+=($!)
wait
if [ -s "$tmp/b1.start" ] && gt "$(cat "$tmp/b1.start")" "$(cat "$tmp/a1.end")"; then
    ok "a second gate waits for the first, then runs"
else bad "a second gate waits for the first, then runs"; fi
nros_grep_q "gate lock is held: pid=" "$tmp/b1.out" && ok "the waiter names the holder" \
    || bad "the waiter names the holder"
run_dir "$tmp/x1a"; run_dir "$tmp/x1b"
gate "$tmp/x1a" c1 2 & pids+=($!)
wait_for "$tmp/c1.start" 5
gate "$tmp/x1b" d1 0.1 & pids+=($!)
wait
if ! gt "$(cat "$tmp/d1.start")" "$(cat "$tmp/c1.end")"; then
    ok "negative control: two separate locks overlap"
else bad "negative control: two separate locks overlap"; fi

# 2. kill -9 of the holder lets the waiter in within ~1 s. Negative control:
#    the same waiter with the holder left alone is still waiting at that time.
x="$tmp/x2"; run_dir "$x"
gate "$x" a2 30 & pids+=($!)
wait_for "$tmp/a2.start" 5
gate "$x" b2 0.1 & pids+=($!)
sleep 0.5
[ ! -e "$tmp/b2.start" ] && ok "negative control: the waiter waits while the holder lives" \
    || bad "negative control: the waiter waits while the holder lives"
holder="$(sed -n 's/^pid=\([0-9]*\).*/\1/p' "$x/nros-gate.lock")"
killed="$(now)"
kill -9 "$holder" 2>/dev/null
wait_for "$tmp/b2.start" 3
if [ -s "$tmp/b2.start" ] && ! gt "$(cat "$tmp/b2.start")" "$(python3 -c "print($killed + 1.5)")"; then
    ok "kill -9 of the holder lets the waiter in within ~1 s"
else bad "kill -9 of the holder lets the waiter in within ~1 s"; fi
pkill -9 -f "nros-gate-lock-test-$$" 2>/dev/null
wait

# 3. An orphaned child does not hold the lock. The gate backgrounds a long
#    sleeper and exits; the next gate gets in at once. Negative control: a bare
#    `flock` WITHOUT `-o` passes the descriptor to the orphan, which then holds
#    the lock after its gate has exited.
x="$tmp/x3"; run_dir "$x"
env XDG_RUNTIME_DIR="$x" bash "$lock_sh" \
    bash -c "(exec -a nros-gate-lock-test-$$ sleep 30) & disown; exit 0" >/dev/null 2>&1
start="$(now)"
gate "$x" b3 0.1 _NROS_GATE_LOCK_TEST_WAIT_S=3
if [ "$(cat "$tmp/b3.rc")" = 0 ] && ! gt "$(cat "$tmp/b3.start")" "$(python3 -c "print($start + 1.5)")"; then
    ok "an orphaned child does not hold the lock"
else bad "an orphaned child does not hold the lock"; fi
pkill -9 -f "nros-gate-lock-test-$$" 2>/dev/null
nl="$tmp/x3/bare.lock"
flock "$nl" bash -c "(exec -a nros-gate-lock-test-$$ sleep 30) & disown; exit 0"
if ! flock -n "$nl" true; then
    ok "negative control: without -o, the orphan holds the lock"
else bad "negative control: without -o, the orphan holds the lock"; fi
pkill -9 -f "nros-gate-lock-test-$$" 2>/dev/null

# 4. A hung holder times out, exits red "gate timed out", and the waiter gets
#    in. Negative control: with a long timeout the waiter is still out.
x="$tmp/x4"; run_dir "$x"
gate "$x" a4 60 _NROS_GATE_LOCK_TEST_WALL_S=2 & pids+=($!)
wait_for "$tmp/a4.start" 5
gate "$x" b4 0.1 & pids+=($!)
sleep 1
[ ! -e "$tmp/b4.start" ] && ok "negative control: the waiter is out before the holder's timeout" \
    || bad "negative control: the waiter is out before the holder's timeout"
wait
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
gate "$x" a5 4 & pids+=($!)
wait_for "$tmp/a5.start" 5
gate "$x" b5 0.1 _NROS_GATE_LOCK_TEST_WAIT_S=1
if [ "$(cat "$tmp/b5.rc")" = 1 ] && [ ! -e "$tmp/b5.start" ] \
    && nros_grep_q "NOT RUN — waited 1 s for this host's gate lock, still held by: pid=" "$tmp/b5.out" \
    && nros_grep_q "ask the user" "$tmp/b5.out"; then
    ok "the waiter's cap expires red, naming the holder, and never runs"
else bad "the waiter's cap expires red, naming the holder, and never runs"; fi
gate "$x" c5 0.1 _NROS_GATE_LOCK_TEST_WAIT_S=10
[ "$(cat "$tmp/c5.rc")" = 0 ] && [ -s "$tmp/c5.start" ] \
    && ok "negative control: a longer cap lets the waiter run" \
    || bad "negative control: a longer cap lets the waiter run"
wait

# 6. A zero wait is a refusal, never an unlocked run (no knob skips the lock).
x="$tmp/x6"; run_dir "$x"
gate "$x" a6 3 & pids+=($!)
wait_for "$tmp/a6.start" 5
gate "$x" b6 0.1 _NROS_GATE_LOCK_TEST_WAIT_S=0
[ "$(cat "$tmp/b6.rc")" = 1 ] && [ ! -e "$tmp/b6.start" ] \
    && ok "a zero wait refuses rather than running unlocked" \
    || bad "a zero wait refuses rather than running unlocked"
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
cmd_gate "$x" a8 'sleep 30' _NROS_GATE_LOCK_TEST_HANG_S=2
if [ "$(cat "$tmp/a8.rc")" = 1 ] && nros_grep_q "gate hung: no progress" "$tmp/a8.out" \
    && [ "$(cat "$tmp/a8.secs")" -lt 15 ]; then
    ok "a silent gate with no CPU is killed as hung, red"
else bad "a silent gate with no CPU is killed as hung, red"; fi
gate "$x" b8 0.1 _NROS_GATE_LOCK_TEST_WAIT_S=3
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
    if nros_grep_q "wall limit $want s ($((want / 60)) min, $n cores), hang limit 45 min without output or CPU, waiter cap $((want + 1800)) s" "$tmp/n$n.out"; then
        ok "on $n cores the wall limit is $want s and the waiter cap $((want + 1800)) s"
    else bad "on $n cores the wall limit is $want s and the waiter cap $((want + 1800)) s ($(head -1 "$tmp/n$n.out"))"; fi
done

if [ "$fail" -ne 0 ]; then
    echo "gate-host-lock tests: FAILED" >&2
    exit 1
fi
echo "gate-host-lock tests: OK"
