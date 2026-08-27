#!/usr/bin/env bash
# Issue 0762 — a killed build launcher must take its whole subtree with it.
#
# The bug this guards against is silent by construction: the launcher exits, the
# terminal comes back, and the build keeps running underneath against sources
# that have since moved. Nothing reports it. So the test drives real process
# trees and asserts on `ps`, not on the guard's own log lines.
#
# Three paths, because they have three different correct answers:
#
#   trap    SIGTERM to the launcher -> the whole subtree dies
#   refuse  a live launcher -> a second build refuses rather than racing it
#   reap    a dead launcher with a live subtree -> the orphans are collected
#
# The last two are one decision keyed on whether the LAUNCHER is alive, and
# getting that discriminator backwards is the interesting failure: keying on the
# payload's leader instead makes the guard report "already running" forever
# after a SIGKILL, refusing to start while the orphans it should have reaped
# keep burning the machine. That case is asserted explicitly below.

set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
guard="$repo_root/scripts/build/subtree-guard.sh"

[ -f "$guard" ] || {
    echo "FAIL: $guard not found" >&2
    exit 1
}

work="$(mktemp -d)"
trap 'rm -rf "$work"; pkill -f "nros-guard-test-payload" 2>/dev/null || true' EXIT

export NROS_GUARD_LOCK_DIR="$work/guards"

# A payload with real depth: the orphans that motivated this were four levels
# below the launcher, and a one-level test would pass against a guard that only
# killed its direct child.
cat > "$work/payload.sh" <<'PAYLOAD'
#!/usr/bin/env bash
exec -a nros-guard-test-payload bash -c 'bash -c "sleep 240 & sleep 240 & wait" & wait'
PAYLOAD
chmod +x "$work/payload.sh"

cat > "$work/launcher.sh" <<LAUNCHER
#!/usr/bin/env bash
set -eu
source "$guard"
nros_guard_exec guardtest bash "$work/payload.sh"
LAUNCHER
chmod +x "$work/launcher.sh"

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

payload_pgid() {
    awk '{print $2}' "$NROS_GUARD_LOCK_DIR/guardtest.pgid" 2>/dev/null
}

launcher_pid() {
    awk '{print $1}' "$NROS_GUARD_LOCK_DIR/guardtest.pgid" 2>/dev/null
}

# The PIDs currently in $1, so a later check can ask about THOSE processes
# rather than about a number.
#
# A ZOMBIE IS NOT A MEMBER — see the same rule (and the same reason) in
# scripts/build/subtree-guard.sh. Under a PID 1 that never reaps, a killed
# subtree stays visible to `ps` at its original pgid forever, and a state-blind
# count reads a successful kill as the orphan bug. That is issue 0853: this test
# failed on every GitHub push, where the container's PID 1 is `tail -f
# /dev/null`, and passed under every `docker run` whose PID 1 happened to reap.
group_members() {
    local pgid="$1"
    [ -n "$pgid" ] || return 0
    ps -eo pid=,pgid=,stat= 2>/dev/null |
        awk -v g="$pgid" '$2 == g && $3 !~ /^Z/ { print $1 }'
}
export -f group_members

group_size() {
    local pgid="$1"
    [ -n "$pgid" ] || { echo 0; return; }
    group_members "$pgid" | wc -l | tr -d ' '
}
export -f group_size

# How many of the recorded PIDs ($1, space-separated) are still alive AND still
# in pgid $2.
#
# A bare `pgid == N` count is unsound on a busy host: PGIDs are RECYCLED, so an
# unrelated process can land in a group numerically equal to one that already
# drained, and the caller reports survivors that were never its subtree. Not
# hypothetical — `check-subtree-guard` failed on every GitHub push while passing
# locally and in a quiet container, with the survivor count varying between runs
# (1, then 2), which is reuse rather than a slow drain. The hosted runner has 4
# vCPUs and `check-fast` runs its gates 32-way parallel, so PID churn is
# enormous there and negligible here.
#
# Requiring the SAME pid AND the same pgid is what makes it sound. The repo
# already carries this lesson as `group_ledger::start_time()` — "a pid is not a
# pid once recycled"; this is the same rule one level over.
members_still_in_group() {
    local pids="$1" pgid="$2" alive=0 p cur state
    for p in $pids; do
        [ -d "/proc/$p" ] || continue
        # `stat=` too: a zombie keeps both its pid and its pgid, so identity is
        # necessary but not sufficient — it must also still be a process.
        read -r cur state < <(ps -o pgid=,stat= -p "$p" 2>/dev/null)
        case "$state" in Z*) continue ;; esac
        [ "$cur" = "$pgid" ] && alive=$((alive + 1))
    done
    echo "$alive"
}

# `wait_until` evaluates its predicate in a `bash -c` subshell, which does not
# inherit shell functions unless they are exported.
export -f members_still_in_group

wait_until() {
    # $1 = seconds, rest = predicate
    local deadline=$(( SECONDS + $1 )); shift
    while [ "$SECONDS" -lt "$deadline" ]; do
        if "$@"; then return 0; fi
        sleep 1
    done
    return 1
}

start_launcher() {
    "$work/launcher.sh" > "$work/launcher.log" 2>&1 &
    wait_until 15 test -s "$NROS_GUARD_LOCK_DIR/guardtest.pgid" \
        || fail "launcher never wrote its lock file"
    # The lock is written before the tree is fully up; wait for real depth.
    wait_until 15 bash -c '[ "$(group_size "$(awk "{print \$2}" "$0")")" -ge 3 ]' \
        "$NROS_GUARD_LOCK_DIR/guardtest.pgid" || true
}

# --- path 1: trap -----------------------------------------------------------
start_launcher
pgid="$(payload_pgid)"
lpid="$(launcher_pid)"
[ -n "$pgid" ] || fail "no payload pgid recorded"
[ "$(group_size "$pgid")" -ge 3 ] || fail "payload tree never reached depth (got $(group_size "$pgid"))"

members="$(group_members "$pgid" | tr '\n' ' ')"
kill -TERM "$lpid" 2>/dev/null || fail "could not signal launcher $lpid"
wait_until 20 bash -c '[ "$(members_still_in_group "'"$members"'" "'"$pgid"'")" -eq 0 ]' \
    || fail "subtree survived SIGTERM to its launcher — $(members_still_in_group "$members" "$pgid") of ITS OWN process(es) still in pgid $pgid. This is the orphan bug."
# The guard removes the lock just AFTER the group drains, so wait for it rather
# than sampling the instant the last process exits.
wait_until 10 bash -c '! [ -f "$NROS_GUARD_LOCK_DIR/guardtest.pgid" ]' \
    || fail "lock file outlived a clean shutdown"
echo "  ok: SIGTERM to the launcher killed the whole subtree"

# --- path 2: refuse while a launcher is alive -------------------------------
start_launcher
pgid="$(payload_pgid)"
lpid="$(launcher_pid)"
out="$("$work/launcher.sh" 2>&1)" && fail "a second build started while the first was running"
grep -q "already running" <<<"$out" \
    || fail "expected a refusal naming the running build, got: $out"
grep -q "kill -TERM -$pgid" <<<"$out" \
    || fail "the refusal must tell the operator how to stop the build it found"
echo "  ok: a second build refuses while a launcher is alive"

# --- path 3: reap after an untrappable kill ---------------------------------
# SIGKILL is the case a trap cannot cover, so the lock is what makes the promise
# honest. The orphans must SURVIVE this kill (proving the situation is real)
# and then be collected by the next run.
kill -KILL "$lpid" 2>/dev/null || fail "could not SIGKILL launcher $lpid"
wait_until 10 bash -c "! kill -0 $lpid 2>/dev/null" || fail "launcher survived SIGKILL"
[ "$(group_size "$pgid")" -ge 3 ] \
    || fail "the payload did NOT outlive a SIGKILLed launcher, so this test is no longer exercising the orphan case"

# A FRESH log: reading the previous run's file would let path 2's output
# satisfy the announcement assertion below, which is the kind of pass that
# looks like coverage and is not.
"$work/launcher.sh" > "$work/reap.log" 2>&1 &
wait_until 30 bash -c '[ "$(group_size "'"$pgid"'")" -eq 0 ]' \
    || fail "orphans from a SIGKILLed launcher were NOT reaped — $(group_size "$pgid") still alive in pgid $pgid. A guard that refuses instead of reaping here leaves them running forever (the discriminator must be the launcher, not the payload leader)."
grep -q "reaping" "$work/reap.log" \
    || fail "orphans were collected but not ANNOUNCED; a build that silently kills processes it did not start is indistinguishable from one that hangs. Log was: $(cat "$work/reap.log")"
echo "  ok: orphans from a SIGKILLed launcher are reaped and announced"

pkill -f nros-guard-test-payload 2>/dev/null || true
echo "subtree-guard: all 3 paths OK (trap, refuse, reap)"
