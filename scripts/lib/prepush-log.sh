#!/usr/bin/env bash
# Keep the pre-push hook's fast-gate output after the push — issue 1964.
#
# WHY
#
# On 2026-10-18 `just check fast`, run by `.githooks/pre-push`, refused a push
# from an agent worktree and passed minutes later with nothing changed. The hook
# printed the full output to the terminal; the caller kept only the last three
# lines, and the worktree was removed afterwards. So the one red run of a gate
# that has not been reproduced since (six rebase cycles, loaded and unloaded)
# left no record of WHICH gate it was.
#
# The fix is that the record outlives both the terminal and the worktree:
#
#   ${NROS_PREPUSH_LOG_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/nros-pre-push}/
#       <UTC stamp>-<pid>.log         the full combined output, with a header
#       <UTC stamp>-<pid>.gates.tsv   per-gate `name rc wall_ms cpu_ms`
#                                     (the runner's NROS_GATE_TIMINGS_FILE)
#
# WHERE, and why not under the repository
#
# Never in the checkout or its git dir: `check-hook-repo-side-effects` compares
# the repositories a hook can reach byte for byte, and a linked worktree's git
# dir is deleted with the worktree — which is how the first red was lost. A
# per-user state directory is outside every checkout, shared by all of them, and
# survives `git worktree remove`. Same root as `runner-sweep.sh`'s state.
#
# Bounded: the newest NROS_PREPUSH_LOG_KEEP runs (default 50) are kept.
#
# FAILS OPEN on the LOG, never on the VERDICT: if the directory cannot be made,
# the command runs unlogged and its exit status is returned unchanged. A hook
# that refuses a push because it could not write a diagnostic gets bypassed.

# nros_prepush_log_dir — print the directory logs go to.
nros_prepush_log_dir() {
    printf '%s' "${NROS_PREPUSH_LOG_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/nros-pre-push}"
}

# nros_prepush_run_logged <cmd…>
#
# Runs <cmd>, streaming its combined stdout+stderr to stderr AND to a new log
# file, and returns <cmd>'s exit status. On the way out it prints the log path
# to stderr, so a caller that keeps only the tail still has the path.
# NROS_GATE_TIMINGS_FILE is pointed beside the log unless the caller set it.
nros_prepush_run_logged() {
    local dir log rc keep stamp
    dir="$(nros_prepush_log_dir)"
    stamp="$(date -u +%Y%m%dT%H%M%SZ)-$$"
    log=""
    if mkdir -p "$dir" 2>/dev/null && : >"$dir/$stamp.log" 2>/dev/null; then
        log="$dir/$stamp.log"
    fi
    if [ -z "$log" ]; then
        echo "pre-push: could not create a log in $dir — output NOT kept." >&2
        rc=0
        "$@" || rc=$?
        return "$rc"
    fi

    {
        echo "# pre-push fast-gate log (issue 1964)"
        echo "# when:      $(date -u +%Y-%m-%dT%H:%M:%SZ)"
        echo "# cwd:       $PWD"
        echo "# HEAD:      $(git rev-parse HEAD 2>/dev/null || echo '?')"
        echo "# origin/main: $(git rev-parse --verify --quiet refs/remotes/origin/main 2>/dev/null || echo '?')"
        echo "# load:      $(cut -d' ' -f1-3 /proc/loadavg 2>/dev/null || echo '?')"
        echo "# command:   $*"
        echo
    } >>"$log"

    # PIPESTATUS, not `set -o pipefail`: this is sourced into callers with and
    # without it, and the status wanted is the command's, never tee's.
    NROS_GATE_TIMINGS_FILE="${NROS_GATE_TIMINGS_FILE:-$dir/$stamp.gates.tsv}" \
        "$@" 2>&1 | tee -a "$log" >&2
    rc="${PIPESTATUS[0]}"
    echo "# exit: $rc" >>"$log"
    echo "pre-push: full output kept in $log" >&2

    keep="${NROS_PREPUSH_LOG_KEEP:-50}"
    case "$keep" in ''|*[!0-9]*) keep=50 ;; esac
    # Newest first by the stamp in the name (UTC, fixed width), so ordering
    # does not depend on mtimes; drop everything past `keep` runs.
    local f n=0
    while IFS= read -r f; do
        n=$((n + 1))
        if [ "$n" -gt "$keep" ]; then
            rm -f "$dir/$f" "$dir/${f%.log}.gates.tsv"
        fi
    done < <(ls -1 "$dir" 2>/dev/null | grep -E '^[0-9]{8}T[0-9]{6}Z-[0-9]+\.log$' | sort -r)
    return "$rc"
}
