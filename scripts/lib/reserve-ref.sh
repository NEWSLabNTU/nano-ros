# shellcheck shell=bash
# reserve-ref.sh — the one claim loop behind every `just <kind>-new`.
#
# Issue, phase and RFC numbers (and diagnostic codes) are namespaces shared by
# every parallel session. A number is taken by pushing a commit no other
# session can reproduce to `<ref-ns>/<number>` on the remote: the push either
# creates the ref (the number is ours) or is rejected because the ref exists
# (another session got there first, so step to the next number). Push success
# IS the claim; reading "highest + 1" and writing it later is the race that
# collided ids repeatedly.
#
# A push that fails for any OTHER reason (no network, no credentials, a
# read-only mirror) claims nothing. Earlier copies of this loop then fell back
# to "local maximum + 1" with a warning, which is that same race; this one
# prints no number and fails, so a caller cannot file under a number nobody
# claimed. Retry once the push works.
#
# Usage (source it, then):
#   number="$(nros_reserve_claim KIND REF_NS REMOTE MAX_ATTEMPTS CANDIDATE WIDTH OBJECT_FN)" || exit 1
#     KIND       words for messages ("an issue id", "a phase number", ...)
#     WIDTH      zero-pad width (0 for none)
#     OBJECT_FN  function called with the formatted number; prints a commit id
#                unique to this attempt (the reservation object)
# Prints the claimed, formatted number on stdout; diagnostics go to stderr.

# `grep -q` cannot tell a non-match from a tool error; the one test below
# decides retry (contention) from fail (anything else).
# shellcheck source=grep-q.sh
. "$(dirname "${BASH_SOURCE[0]}")/grep-q.sh"

nros_reserve_claim() {
    local kind="$1" ns="$2" remote="$3" max="$4" cand="$5" width="$6" objfn="$7"
    local attempt=0 num obj err
    while [ "$attempt" -lt "$max" ]; do
        if [ "$width" -gt 0 ]; then
            num="$(printf "%0${width}d" "$cand")"
        else
            num="$cand"
        fi
        obj="$("$objfn" "$num")"

        if err="$(git push "$remote" "$obj:$ns/$num" 2>&1)"; then
            printf '%s\n' "$num"
            return 0
        fi

        # Taken by another session between our read and our write: the race
        # this exists for. Step on and retry; not an error.
        if nros_grep_q -iE "already exists|non-fast-forward|fetch first|rejected" <<<"$err"; then
            cand=$((cand + 1))
            attempt=$((attempt + 1))
            continue
        fi

        echo "[FAIL] could not reserve $kind on '$remote':" >&2
        printf '%s\n' "$err" | sed 's/^/       /' >&2
        echo "" >&2
        echo "       No number is printed: one not claimed on '$remote' could be" >&2
        echo "       taken by another session too, and numbers are never reused." >&2
        echo "       Fix the push (network, credentials, a read-only remote) and" >&2
        echo "       run the same command again." >&2
        return 1
    done

    echo "[FAIL] $max consecutive numbers were already reserved." >&2
    echo "       That is not contention any more — check \`git ls-remote $remote '$ns/*'\`" >&2
    echo "       for stale reservations left by sessions that never filed." >&2
    return 1
}
