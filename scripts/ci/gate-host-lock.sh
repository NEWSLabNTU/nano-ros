#!/usr/bin/env bash
# One `just ci gate` per host (issue 2602). "One gate per host" was agent
# convention only, and on 2026-11-15 two concurrent gates turned each other red
# on two hosts (a `cli-tests` timeout on newslab-243; both gates red at load ~50
# on newslab-175, then a push on partial reruns).
#
# Usage: gate-host-lock.sh <command...>     runs <command> holding the lock
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
#   * The holder's gate has an overall timeout. On expiry, `timeout` signals the
#     gate's whole process group, and the gate exits red "gate timed out", so a
#     hung holder frees the lock by itself.
#   * The waiter has a cap. On expiry it exits red naming the holder, and it
#     NEVER runs its command without the lock.
#   * No knob skips the lock, and nothing anywhere says to delete it: either
#     would become the bypass. The durations below can be shortened (the tests
#     do), and none of them can make a gate run unlocked: a zero wait is an
#     immediate refusal.
#
# Durations (seconds): NROS_GATE_TIMEOUT_S (default 9000, 2.5 h),
# NROS_GATE_LOCK_WAIT_S (default 10800, 3 h), NROS_GATE_LOCK_REPORT_S (default
# 300).
set -uo pipefail

gate_s="${NROS_GATE_TIMEOUT_S:-9000}"
wait_s="${NROS_GATE_LOCK_WAIT_S:-10800}"
report_s="${NROS_GATE_LOCK_REPORT_S:-300}"
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
    # Run by `flock` once the lock is taken: argv is `--held <lock> <command...>`.
    lock="$2"
    shift 2
    if [ -n "${NROS_GATE_LOCK_REPORTER:-}" ]; then
        kill "$NROS_GATE_LOCK_REPORTER" 2>/dev/null || true
    fi
    printf 'pid=%s worktree=%s branch=%s started=%s\n' "$$" "$PWD" \
        "$(git branch --show-current 2>/dev/null || echo '?')" \
        "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >"$lock"
    rc=0
    # The repo's one deadline spelling (TERM, then KILL after the grace). Not
    # `--foreground`: GNU timeout then signals the command's whole process group
    # on expiry, so the gate's cargo, nextest and QEMU children go with it.
    # shellcheck source=scripts/lib/deadline.sh
    . "$(dirname "$0")/../lib/deadline.sh"
    "${NROS_DEADLINE[@]}" "$gate_s" "$@" || rc=$?
    : >"$lock"
    if [ "$rc" -eq 124 ] || [ "$rc" -eq 137 ]; then
        echo "ci gate: gate timed out after ${gate_s} s; its process group was killed, and the host's gate lock is free (issue 2602)." >&2
        exit 1
    fi
    # Never let a gate's own status read as the waiter's expiry.
    [ "$rc" -eq 75 ] && rc=1
    exit "$rc"
fi

if [ "$#" -eq 0 ]; then
    echo "usage: gate-host-lock.sh <command...>" >&2
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
    echo "  waiting for it, up to ${wait_s} s (issue 2602: one gate per host)."
    (
        while sleep "$report_s"; do
            echo "ci gate: still waiting for the gate lock: $(holder_line "$lock")"
        done
    ) &
    reporter=$!
fi

rc=0
NROS_GATE_LOCK_REPORTER="$reporter" flock -o -E "$conflict" -w "$wait_s" "$lock" \
    bash "$0" --held "$lock" "$@" || rc=$?
[ -n "$reporter" ] && kill "$reporter" 2>/dev/null
if [ "$rc" -eq "$conflict" ]; then
    echo "ci gate: NOT RUN — waited ${wait_s} s for this host's gate lock, still held by: $(holder_line "$lock")" >&2
    echo "  Wait for that gate, or ask the user (issue 2602)." >&2
    exit 1
fi
exit "$rc"
