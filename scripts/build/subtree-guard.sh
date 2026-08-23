#!/usr/bin/env bash
# Kill the whole build subtree when a launcher dies — issue 0762.
#
# `just build-test-fixtures` is a shell script that runs `make`, which runs a
# per-platform `just`, which runs another `make`, which runs `cmake`, which runs
# `cargo`. Killing the top of that chain kills ONE process. Everything below it
# is reparented to init and keeps building: on 2026-08-23 a `kill` of the
# launcher left a `workspace-fixtures-make` and its cmake/cargo subtree running
# for ten more minutes, against sources that had already moved. Starting a new
# build then races the survivors in the same trees.
#
# The signal that kills a process group reaches every descendant, because a
# process group is INHERITED — make, just, cmake and cargo are all already in
# the launcher's group. So the fix is not a trap per launcher (there are ~10 of
# them, and the next one added would silently not have it); it is one guard at
# the OUTERMOST launcher, which every descendant is already inside.
#
# ## Nesting is the part that is easy to get wrong
#
# If each level made its own process group, killing the top group would stop
# reaching the levels below — the guard would defeat itself, and it would do so
# invisibly, which is worse than not having it. So only the outermost caller
# creates a group; `NROS_SUBTREE_GUARD` announces it to descendants, and inner
# calls run the command inline. The guard is safe to add to any launcher, and
# adding it to all of them is correct.
#
# ## What this cannot do, and what covers that
#
# SIGKILL is not trappable, so a `kill -9` of the launcher still orphans the
# tree. Nothing in a shell can prevent that. What makes the promise honest is
# the second half: the group id is recorded in a lock file, so the NEXT build
# finds the survivors and reaps them before starting (`nros_guard_reap`),
# rather than silently racing them. Trap for the ordinary case, lock for the
# violent one.
#
# Usage:
#   source scripts/build/subtree-guard.sh
#   nros_guard_exec fixtures make -j "$n" -f "$makefile"

# Where the lock lives. One per NAME, so `fixtures` and a platform lane do not
# evict each other.
_nros_guard_lock_path() {
    local name="$1" root
    root="${NROS_GUARD_LOCK_DIR:-${TMPDIR:-/tmp}/nros-build-guards}"
    mkdir -p "$root" 2>/dev/null || true
    printf '%s/%s.pgid\n' "$root" "$name"
}

# Every live PID in process group `pgid`, excluding this shell's own tree.
_nros_guard_group_members() {
    local pgid="$1"
    ps -eo pid=,pgid= 2>/dev/null | awk -v g="$pgid" '$2 == g { print $1 }'
}

# Reap a process group left behind by a launcher that could not run its trap.
#
# Announced, never silent: a build that quietly kills processes it did not start
# is indistinguishable from one that hangs, and the thing being reaped is
# usually somebody's interrupted work.
nros_guard_reap() {
    local name="$1" lock pgid members leader
    lock="$(_nros_guard_lock_path "$name")"
    [ -f "$lock" ] || return 0
    local launcher
    launcher="$(awk '{print $1}' "$lock" 2>/dev/null || true)"
    pgid="$(awk '{print $2}' "$lock" 2>/dev/null || true)"
    case "$pgid$launcher" in
        ''|*[!0-9]*) rm -f "$lock"; return 0 ;;
    esac
    members="$(_nros_guard_group_members "$pgid")"
    if [ -z "$members" ]; then
        rm -f "$lock"
        return 0
    fi
    # The discriminator is the LAUNCHER, not the payload.
    #
    # A live launcher means a build is genuinely running, and refusing is right:
    # two fixture builds in one tree corrupt each other's artifacts, and "it was
    # already broken" is how that gets misdiagnosed for hours.
    #
    # Keying on the payload leader instead would be wrong in exactly the case
    # this function exists for — after a SIGKILL of the launcher, the payload
    # leader is still very much alive, so the guard would report "already
    # running" and refuse, forever, while the orphans it should have reaped kept
    # burning the machine.
    if kill -0 "$launcher" 2>/dev/null && [ "${NROS_GUARD_FORCE:-}" != "1" ]; then
        echo "subtree-guard: a '$name' build is already running (pgid $pgid, $(printf '%s\n' "$members" | wc -l | tr -d ' ') process(es))." >&2
        echo "  Two builds in one tree corrupt each other's artifacts, so this one refuses to start." >&2
        echo "  Wait for it, or stop it with:  kill -TERM -$pgid" >&2
        echo "  Override (only if you know that group is not really building):  NROS_GUARD_FORCE=1" >&2
        return 1
    fi
    echo "subtree-guard: reaping $(printf '%s\n' "$members" | wc -l | tr -d ' ') orphan(s) from a previous '$name' build (pgid $pgid)." >&2
    echo "  Its launcher (pid $launcher) is gone but the subtree outlived it — a SIGKILL, or a crash." >&2
    kill -TERM -"$pgid" 2>/dev/null || true
    local waited=0
    while [ "$waited" -lt 10 ]; do
        [ -z "$(_nros_guard_group_members "$pgid")" ] && break
        sleep 1
        waited=$((waited + 1))
    done
    if [ -n "$(_nros_guard_group_members "$pgid")" ]; then
        echo "subtree-guard: pgid $pgid ignored SIGTERM after ${waited}s — sending SIGKILL." >&2
        kill -KILL -"$pgid" 2>/dev/null || true
    fi
    rm -f "$lock"
    return 0
}

# Kill our own group's survivors. Called from the trap.
_nros_guard_cleanup() {
    local pgid="$1" name="$2" sig="${3:-}"
    if [ -n "$sig" ]; then
        echo "" >&2
        echo "subtree-guard: caught $sig — stopping the whole '$name' build subtree (pgid $pgid)." >&2
    fi
    kill -TERM -"$pgid" 2>/dev/null || true
    local waited=0
    while [ "$waited" -lt 10 ]; do
        [ -z "$(_nros_guard_group_members "$pgid")" ] && break
        sleep 1
        waited=$((waited + 1))
    done
    if [ -n "$(_nros_guard_group_members "$pgid")" ]; then
        kill -KILL -"$pgid" 2>/dev/null || true
    fi
    rm -f "$(_nros_guard_lock_path "$name")"
}

# Run a command such that killing THIS shell kills the command's whole subtree.
#
#   nros_guard_exec <name> <command> [args...]
#
# Returns the command's exit status. Inside an already-guarded tree it is a
# transparent passthrough — see the nesting note above.
nros_guard_exec() {
    local name="$1"
    shift
    if [ -n "${NROS_SUBTREE_GUARD:-}" ]; then
        # Already inside a guarded group. Making another one here would break
        # the outer guard's reach, which is the failure this whole file is
        # about — so do nothing but run the command.
        "$@"
        return $?
    fi
    if [ "${NROS_GUARD_DISABLE:-}" = "1" ]; then
        "$@"
        return $?
    fi
    nros_guard_reap "$name" || return 1

    # Job control gives the backgrounded command its own process group whose id
    # equals its pid, which is what makes `kill -- -PID` reach the subtree. In a
    # non-interactive shell this is the only way to get a new group without
    # exec'ing away the trap that is the point of the exercise.
    set -m
    NROS_SUBTREE_GUARD=1 "$@" &
    local pid=$!
    set +m

    # `<launcher-pid> <payload-pgid>` — the reaper needs both, and which one it
    # keys on is the whole correctness of the refuse-vs-reap decision.
    printf '%s %s\n' "$$" "$pid" > "$(_nros_guard_lock_path "$name")"
    # shellcheck disable=SC2064  # expand pid/name NOW, not at trap time
    trap "_nros_guard_cleanup $pid $name INT; exit 130" INT
    trap "_nros_guard_cleanup $pid $name TERM; exit 143" TERM
    trap "_nros_guard_cleanup $pid $name HUP; exit 129" HUP

    local rc=0
    wait "$pid" || rc=$?
    trap - INT TERM HUP
    # The command is reaped, so its group id can be RECYCLED. Killing it now
    # would eventually hit an unrelated process — so on the normal path the lock
    # is simply removed and nothing is signalled.
    rm -f "$(_nros_guard_lock_path "$name")"
    return "$rc"
}
