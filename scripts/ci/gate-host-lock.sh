#!/usr/bin/env bash
# One `just ci gate` per host (issue 2602). "One gate per host" was agent
# convention only, and on 2026-11-15 two concurrent gates turned each other red
# on two hosts (a `cli-tests` timeout on newslab-243; both gates red at load ~50
# on newslab-175, then a push on partial reruns).
#
# Usage: gate-host-lock.sh [--housekeeping <shell-command>] <command...>
#   runs <command> (the VERDICT) holding the lock, then the housekeeping
#
# Issue 2733 — the verdict region and the housekeeping region, defined HERE:
#   * The VERDICT is <command>. It alone runs under the wall limit and the hang
#     detector below, and its status alone is the gate's exit code.
#   * The HOUSEKEEPING (`--housekeeping`, one shell command) runs after the
#     verdict has ended, however it ended, OUTSIDE those limits. It gets its own
#     limit (15 min), it reports, and it never changes the exit code: a failure
#     or a timeout is a warning line. Anything that cannot change the verdict
#     (cache trims, reminders) belongs here, not at the tail of <command>.
#     On 2026-11-19 a gate whose 11 steps all passed was killed red at the wall
#     limit during the cache trim at the end of its body (newslab-175).
#   * The housekeeping still HOLDS the lock: the trim deletes target dirs under
#     load the next gate would contend with, and the lock's promise is one gate's
#     work per host at a time. The waiter cap covers it (wall + housekeeping +
#     30 min), so a waiter still outlasts any live holder.
#
# The design (user-approved, 2026-11-15):
#   * `flock -o -w <cap> <lock> <command>`: the `flock` process holds the lock,
#     and `-o` closes the descriptor in the command, so an orphaned cargo or QEMU
#     never holds it after `flock` is gone. The kernel releases a flock on any
#     death (SIGKILL, OOM, reboot), so there is no pidfile, no "file exists"
#     test, and nothing to clean up by hand.
#   * ONE lock per host, on a local filesystem: `$XDG_RUNTIME_DIR/nros-gate.lock`,
#     else `~/.cache/nros/gate.lock`. Never per worktree (every worktree shares
#     the host), never on a network filesystem (its locks are not this host's).
#   * The holder writes `pid`, `worktree`, `branch` and `started` into the lock
#     file. A waiter prints that line when it starts waiting and every few
#     minutes after, with `kill -0` saying whether the pid is alive.
#   * The holder's gate has a WALL limit that scales with the host: 2.5 h x
#     max(1, 16 / nproc), so 2.5 h at 16+ cores and 10 h at 4 (user ruling,
#     2026-11-15: a normal gate on a 4-core host measured ~2.5 h). On expiry,
#     `timeout` signals the gate's whole process group, and the gate exits red
#     "gate timed out", so a hung holder frees the lock by itself.
#   * A HANG detector (same ruling): a gate that makes no progress for 45 min is
#     killed and exits red "gate hung: no progress for 45 min". Progress is
#     EITHER new gate output OR growth in the CPU time (utime+stime, children's
#     included) of the gate's process tree, so a long quiet compile lives and a
#     deadlock at 0 % CPU (a nested cargo once held a host 3 h) does not.
#   * The waiter has a cap, the holder's wall limit + its housekeeping limit
#     + 30 min (issue 2733), so it outlasts any
#     live holder. On expiry it exits red naming the holder, and it NEVER runs
#     its command without the lock. Both limits are printed when a gate starts.
#   * No knob skips the lock, and nothing anywhere says to delete it: either
#     would become the bypass. The limits are NOT user knobs. The tests shorten
#     them through `_NROS_GATE_LOCK_TEST_*` variables (WALL_S, WAIT_S, HANG_S,
#     TICK_S, REPORT_S, NPROC, HOUSEKEEPING_S), and none of them can make a gate run unlocked: a
#     zero wait is an immediate refusal.
set -uo pipefail

ncpu="${_NROS_GATE_LOCK_TEST_NPROC:-$(nproc)}"
[ "$ncpu" -ge 1 ] 2>/dev/null || ncpu=1
# 2.5 h x max(1, 16 / nproc), in whole seconds.
if [ "$ncpu" -ge 16 ]; then wall_default=9000; else wall_default=$((9000 * 16 / ncpu)); fi
gate_s="${_NROS_GATE_LOCK_TEST_WALL_S:-$wall_default}"
housekeeping_s="${_NROS_GATE_LOCK_TEST_HOUSEKEEPING_S:-900}"
wait_s="${_NROS_GATE_LOCK_TEST_WAIT_S:-$((gate_s + housekeeping_s + 1800))}"
hang_s="${_NROS_GATE_LOCK_TEST_HANG_S:-2700}"
tick_s="${_NROS_GATE_LOCK_TEST_TICK_S:-60}"
report_s="${_NROS_GATE_LOCK_TEST_REPORT_S:-300}"
limits_line() {
    echo "wall limit ${gate_s} s ($((gate_s / 60)) min, ${ncpu} cores), hang limit $((hang_s / 60)) min without output or CPU, housekeeping limit ${housekeeping_s} s outside both, waiter cap ${wait_s} s"
}

# CPU seconds x 100 (clock ticks) used by <pid> and every descendant, live
# children's utime+stime plus what reaped children left in cutime+cstime. A
# descendant in its own process group (nextest's per-test groups) still counts.
tree_ticks() { # <pid>
    python3 - "$1" <<'PY' 2>/dev/null || echo 0
import os, sys
root = int(sys.argv[1]); kids = {}; ticks = {}
for d in os.listdir("/proc"):
    if not d.isdigit():
        continue
    try:
        st = open(f"/proc/{d}/stat").read()
    except OSError:
        continue
    f = st[st.rindex(")") + 2:].split()
    pid = int(d); ppid = int(f[1])
    kids.setdefault(ppid, []).append(pid)
    ticks[pid] = sum(int(x) for x in f[11:15])
total, todo = 0, [root]
while todo:
    p = todo.pop()
    total += ticks.get(p, 0)
    todo += kids.get(p, [])
print(total)
PY
}

kill_tree() { # <signal> <pid>
    local sig="$1" p
    for p in $(pgrep -P "$2" 2>/dev/null); do kill_tree "$sig" "$p"; done
    kill "-$sig" "$2" 2>/dev/null
}
# The exit `flock` gives when its wait expires, distinct from any gate status.
conflict=75

holder_line() { # <lock>
    local line pid
    line="$(head -n 1 "$1" 2>/dev/null)"
    if [ -z "$line" ]; then
        echo "(the holder has not written its line yet)"
        return
    fi
    pid="$(sed -n 's/^pid=\([0-9]*\).*/\1/p' <<<"$line")"
    if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
        echo "$line (alive)"
    else
        echo "$line (pid not running: the kernel releases its lock as it exits)"
    fi
}

if [ "${1:-}" = "--held" ]; then
    # Run by `flock` once the lock is taken: argv is
    # `--held <lock> <housekeeping or ""> <command...>`.
    lock="$2"
    housekeeping="$3"
    shift 3
    if [ -n "${NROS_GATE_LOCK_REPORTER:-}" ]; then
        kill "$NROS_GATE_LOCK_REPORTER" 2>/dev/null || true
    fi
    printf 'pid=%s worktree=%s branch=%s started=%s\n' "$$" "$PWD" \
        "$(git branch --show-current 2>/dev/null || echo '?')" \
        "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >"$lock"
    echo "ci gate: holding this host's gate lock: $(limits_line) (issue 2602)."
    rc=0
    # The repo's one deadline spelling (TERM, then KILL after the grace). Not
    # `--foreground`: GNU timeout then signals the command's whole process group
    # on expiry, so the gate's cargo, nextest and QEMU children go with it.
    # shellcheck source=scripts/lib/deadline.sh
    . "$(dirname "$0")/../lib/deadline.sh"
    # The gate's output goes through a FIFO into `tee`, so the hang detector can
    # see it grow without re-reading the gate.
    run_dir="$(mktemp -d)"
    mkfifo "$run_dir/out"
    : >"$run_dir/log"
    tee -a "$run_dir/log" <"$run_dir/out" &
    tee_pid=$!
    "${NROS_DEADLINE[@]}" "$gate_s" "$@" >"$run_dir/out" 2>&1 &
    gate_pid=$!
    hung=0 idle=0 last_size=0 last_ticks=0
    while kill -0 "$gate_pid" 2>/dev/null; do
        # Wake on the tick OR the gate's exit, whichever is first, so a gate
        # that ends is never held for the rest of a tick.
        sleep "$tick_s" &
        sleep_pid=$!
        wait -n "$gate_pid" "$sleep_pid" 2>/dev/null
        kill "$sleep_pid" 2>/dev/null
        kill -0 "$gate_pid" 2>/dev/null || break
        size="$(stat -c %s "$run_dir/log" 2>/dev/null || echo 0)"
        ticks="$(tree_ticks "$gate_pid")"
        if [ "$size" != "$last_size" ] || [ "$ticks" != "$last_ticks" ]; then
            idle=0
        else
            idle=$((idle + tick_s))
        fi
        last_size="$size" last_ticks="$ticks"
        if [ "$idle" -ge "$hang_s" ]; then
            hung=1
            kill_tree TERM "$gate_pid"
            sleep 3
            kill_tree KILL "$gate_pid"
            break
        fi
    done
    wait "$gate_pid" || rc=$?
    wait "$tee_pid" 2>/dev/null
    rm -rf "$run_dir"
    # Issue 2733 — the verdict is decided (rc, hung). Housekeeping runs now,
    # under its own deadline and outside the wall limit and the hang detector,
    # still holding the lock. Its status is reported, never returned.
    if [ -n "$housekeeping" ]; then
        hk_rc=0
        "${NROS_DEADLINE[@]}" "$housekeeping_s" bash -c "$housekeeping" || hk_rc=$?
        if [ "$hk_rc" -eq 124 ] || [ "$hk_rc" -eq 137 ]; then
            echo "ci gate: WARNING: housekeeping timed out after ${housekeeping_s} s and was killed; the gate's verdict is unaffected (issue 2733)." >&2
        elif [ "$hk_rc" -ne 0 ]; then
            echo "ci gate: WARNING: housekeeping FAILED (exit ${hk_rc}, see above); the gate's verdict is unaffected (issue 2733)." >&2
        fi
    fi
    : >"$lock"
    if [ "$hung" = 1 ]; then
        echo "ci gate: gate hung: no progress for $((hang_s / 60)) min (no new output, no CPU time in its process tree); it was killed, and the host's gate lock is free (issue 2602)." >&2
        exit 1
    fi
    if [ "$rc" -eq 124 ] || [ "$rc" -eq 137 ]; then
        echo "ci gate: gate timed out after ${gate_s} s; its process group was killed, and the host's gate lock is free (issue 2602)." >&2
        exit 1
    fi
    # Never let a gate's own status read as the waiter's expiry.
    [ "$rc" -eq 75 ] && rc=1
    exit "$rc"
fi

housekeeping=""
if [ "${1:-}" = "--housekeeping" ]; then
    if [ "$#" -lt 2 ] || [ -z "$2" ]; then
        echo "usage: gate-host-lock.sh [--housekeeping <shell-command>] <command...>" >&2
        exit 2
    fi
    housekeeping="$2"
    shift 2
fi
if [ "$#" -eq 0 ]; then
    echo "usage: gate-host-lock.sh [--housekeeping <shell-command>] <command...>" >&2
    exit 2
fi

if [ -n "${XDG_RUNTIME_DIR:-}" ] && [ -d "$XDG_RUNTIME_DIR" ] && [ -w "$XDG_RUNTIME_DIR" ]; then
    lock="$XDG_RUNTIME_DIR/nros-gate.lock"
else
    lock="$HOME/.cache/nros/gate.lock"
    mkdir -p "$(dirname "$lock")"
fi
fs="$(stat -f -c %T "$(dirname "$lock")" 2>/dev/null || echo unknown)"
case "$fs" in
    nfs*|cifs|smb*|fuse.sshfs|ceph|9p)
        echo "ci gate: NOT RUN — the host's gate lock would live on a network filesystem ($fs: $lock), whose locks are not this host's (issue 2602). Set XDG_RUNTIME_DIR to a local runtime dir." >&2
        exit 1 ;;
esac
: >>"$lock"

reporter=""
if ! flock -n "$lock" true; then
    echo "ci gate: this host's gate lock is held: $(holder_line "$lock")"
    echo "  waiting for it (issue 2602: one gate per host): $(limits_line)."
    (
        while sleep "$report_s"; do
            echo "ci gate: still waiting for the gate lock: $(holder_line "$lock")"
        done
    ) &
    reporter=$!
fi

rc=0
NROS_GATE_LOCK_REPORTER="$reporter" flock -o -E "$conflict" -w "$wait_s" "$lock" \
    bash "$0" --held "$lock" "$housekeeping" "$@" || rc=$?
[ -n "$reporter" ] && kill "$reporter" 2>/dev/null
if [ "$rc" -eq "$conflict" ]; then
    echo "ci gate: NOT RUN — waited ${wait_s} s for this host's gate lock, still held by: $(holder_line "$lock")" >&2
    echo "  Wait for that gate, or ask the user (issue 2602)." >&2
    exit 1
fi
exit "$rc"
