#!/usr/bin/env bash
#
# Reserve the next free `docs/roadmap/phase-NNN-` number, ATOMICALLY, across
# every parallel session.
#
# WHY THIS EXISTS
#
# Two sessions opened `phase-350` on 2026-08-13 — `west-fixtures-join-the-
# manifest` at 11:44 and `platform-clock-ns` at 13:58 — for entirely unrelated
# work. (The second was renumbered to `phase-352` once this tool existed; the
# first kept 350.) Neither could see the other: both read "highest
# existing + 1" from a tree that did not yet contain the other's file. That is
# the same check-then-act race `reserve-issue-id.sh` was written for, in the
# third numbered series CLAUDE.md names.
#
# WHEN **NOT** TO USE THIS
#
# A phase number is NOT required to be unique per file, and this tool must not
# be read as making it so. 26 of 342 numbers already carry more than one
# document (`phase-126` has five), because one EFFORT is often split across
# several docs — `phase-275-example-fixture-gap-fill` and
# `phase-275-276-branch-notes` are one piece of work, not two.
#
#   adding a doc to an effort that already has a number  ->  reuse that number,
#                                                            do not reserve
#   starting NEW work that needs its own number          ->  reserve here
#
# There is deliberately NO uniqueness gate for phases, for the same reason:
# a gate forbidding what 26 existing numbers already do would be wrong, and
# would fail the tree on legitimate history. `scripts/ci/issue-ids-check.sh`
# guards issues and RFCs — series where one id really does mean one document.
#
# THE ATOMIC PRIMITIVE
#
# Identical to the issue tool: build a commit object nobody else can reproduce,
# push it to `refs/phase-ids/NNN`, and treat push success as ownership. Git
# rejects creating a ref that already exists, which is a compare-and-swap over
# the one piece of shared state every session already has.
#
# DEGRADATION
#
# If the reservation push fails for any reason OTHER than "already taken" — no
# network, no push permission, a read-only mirror — this says so and falls back
# to the local maximum. That fallback IS the old racy behaviour, so it is
# announced rather than hidden.

set -euo pipefail
cd "$(dirname "$0")/.."

REF_NS="refs/phase-ids"
REMOTE="${NROS_PHASE_REMOTE:-origin}"
MAX_ATTEMPTS=25

slug="${1:-}"

# Highest number already used by a FILE, active or archived. `archived/` counts:
# a phase that has been filed away still owns its number.
highest_file_id() {
    local max=0 id
    while IFS= read -r base; do
        id="$(printf '%s' "$base" | grep -oE '^phase-[0-9]+' | grep -oE '[0-9]+' || true)"
        [ -z "$id" ] && continue
        id=$((10#$id))
        [ "$id" -gt "$max" ] && max="$id"
    done < <(git ls-files 'docs/roadmap/*.md' 'docs/roadmap/archived/*.md' | xargs -r -n1 basename)
    printf '%s' "$max"
}

# Highest number already RESERVED by any session, including one whose doc is not
# committed yet. This is what a file scan cannot see, and the whole point.
highest_reserved_id() {
    local max=0 id
    while IFS= read -r ref; do
        id="$(printf '%s' "${ref##*/}" | grep -oE '^[0-9]+' || true)"
        [ -z "$id" ] && continue
        id=$((10#$id))
        [ "$id" -gt "$max" ] && max="$id"
    done < <(git ls-remote "$REMOTE" "$REF_NS/*" 2>/dev/null | awk '{print $2}')
    printf '%s' "$max"
}

# A commit object no other session can reproduce.
unique_object() {
    local id="$1" empty_tree
    empty_tree="$(git hash-object -t tree /dev/null)"
    printf 'reserve phase %s\n\nhost: %s\npid: %s\nwhen: %s\nslug: %s\n' \
        "$id" "$(hostname 2>/dev/null || echo unknown)" "$$" \
        "$(date +%s%N 2>/dev/null || date +%s)" "${slug:-<unspecified>}" |
        git commit-tree "$empty_tree"
}

file_max="$(highest_file_id)"
reserved_max="$(highest_reserved_id)"
candidate=$(( (file_max > reserved_max ? file_max : reserved_max) + 1 ))

attempt=0
while [ "$attempt" -lt "$MAX_ATTEMPTS" ]; do
    obj="$(unique_object "$candidate")"

    if err="$(git push "$REMOTE" "$obj:$REF_NS/$candidate" 2>&1)"; then
        printf '%s\n' "$candidate"
        echo "reserved phase $candidate on $REMOTE ($REF_NS/$candidate)" >&2
        if [ -n "$slug" ]; then
            echo "  file it as: docs/roadmap/phase-$candidate-$slug.md" >&2
            echo "  it needs a \`**Status (YYYY-MM-DD). …**\` line (check-roadmap-status)" >&2
        fi
        exit 0
    fi

    # Taken by another session between our read and our write — exactly the race
    # this exists for. Step on and retry; not an error.
    if printf '%s' "$err" | grep -qiE "already exists|non-fast-forward|fetch first|rejected"; then
        candidate=$((candidate + 1))
        attempt=$((attempt + 1))
        continue
    fi

    echo "[WARN] could not reserve a phase number on '$REMOTE':" >&2
    printf '%s\n' "$err" | sed 's/^/       /' >&2
    echo "" >&2
    echo "       Falling back to the local maximum + 1. This is the ORIGINAL" >&2
    echo "       RACY behaviour: another session may pick the same number, and" >&2
    echo "       the collision only surfaces once both have pushed." >&2
    printf '%s\n' "$((file_max + 1))"
    exit 0
done

echo "[FAIL] $MAX_ATTEMPTS consecutive numbers were already reserved." >&2
echo "       That is not contention any more — check \`git ls-remote $REMOTE '$REF_NS/*'\`" >&2
echo "       for stale reservations left by sessions that never filed." >&2
exit 1
