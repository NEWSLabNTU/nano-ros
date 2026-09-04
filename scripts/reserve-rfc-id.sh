#!/usr/bin/env bash
#
# Reserve the next free `docs/design/NNNN-` RFC number, ATOMICALLY, across every
# parallel session.
#
# WHY THIS EXISTS
#
# Two RFC-0087s existed on 2026-09-04: `package-identity-and-provider-format`
# (10:47 UTC, landed on main) and `ros2-api-adoption-and-the-compile-or-conform-
# rule` (14:59 UTC, on a branch). Neither session could see the other — both
# read "highest existing + 1" from a tree that did not yet contain the other's
# file. The later filing renumbered to 0089, which cost a sweep across 111 files
# in six extensions, four of which a filtered grep could not see.
#
# That is the same check-then-act race `reserve-issue-id.sh` and
# `reserve-phase-id.sh` were written for. RFCs were the last of the three
# numbered series still numbered by reading the directory, which is why this is
# the third copy of one idea rather than the first.
#
# WHY IT MATTERS MORE HERE THAN FOR PHASES
#
# An RFC id is UNIQUE PER DOCUMENT — `scripts/ci/issue-ids-check.sh` enforces
# it, so a collision is a hard red rather than a readability problem. A phase
# number is deliberately not unique (26 of 342 carry several docs), so the phase
# tool exists only for work that needs its own number. There is no such
# exemption for an RFC: every new one reserves.
#
# The collision is also expensive to unwind in a way an issue collision is not.
# An issue id appears in its own filename, its frontmatter and a handful of
# cross-references. An RFC number is cited in PROSE across the whole tree — 104
# files at the time of the 0087 collision — and a stale citation does not fail:
# it silently resolves to whichever RFC now holds the number. Renumbering is a
# sweep BY SENSE, not by string, because both RFCs are real and both are cited.
#
# THE ATOMIC PRIMITIVE
#
# Identical to the issue and phase tools: build a commit object nobody else can
# reproduce, push it to `refs/rfc-ids/NNNN`, and treat push success as
# ownership. Git rejects creating a ref that already exists, which is a
# compare-and-swap over the one piece of shared state every session already has.
#
# DEGRADATION
#
# If the reservation push fails for any reason OTHER than "already taken" — no
# network, no push permission, a read-only mirror — this says so and falls back
# to the local maximum. That fallback IS the old racy behaviour, so it is
# announced rather than hidden.

set -euo pipefail
cd "$(dirname "$0")/.."

# `grep -q` cannot tell a NON-MATCH from a TOOL ERROR, and the one conditional
# below decides whether a failed reservation push means "someone else took this
# number" (retry) or "something is wrong" (warn and degrade). Getting that
# backwards under load would silently pick the racy fallback. Issue 0726.
# shellcheck source=lib/grep-q.sh
. "$(dirname "$0")/lib/grep-q.sh"

REF_NS="refs/rfc-ids"
REMOTE="${NROS_RFC_REMOTE:-origin}"
MAX_ATTEMPTS=25

slug="${1:-}"

# Highest number already used by a FILE. `archived/` is scanned even though no
# archived RFC is numbered today: an archived RFC would still own its number,
# and the uniqueness gate only looks at `docs/design` maxdepth 1, so a number
# reused from the archive would collide silently rather than fail.
highest_file_id() {
    local max=0 id
    while IFS= read -r base; do
        id="$(printf '%s' "$base" | grep -oE '^[0-9]{4}' || true)"
        [ -z "$id" ] && continue
        id=$((10#$id))
        [ "$id" -gt "$max" ] && max="$id"
    done < <(git ls-files 'docs/design/*.md' 'docs/design/archived/*.md' | xargs -r -n1 basename)
    printf '%s' "$max"
}

# Highest number already RESERVED by any session, including one whose RFC is not
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
    printf 'reserve rfc %s\n\nhost: %s\npid: %s\nwhen: %s\nslug: %s\n' \
        "$id" "$(hostname 2>/dev/null || echo unknown)" "$$" \
        "$(date +%s%N 2>/dev/null || date +%s)" "${slug:-<unspecified>}" |
        git commit-tree "$empty_tree"
}

file_max="$(highest_file_id)"
reserved_max="$(highest_reserved_id)"
candidate=$(( (file_max > reserved_max ? file_max : reserved_max) + 1 ))

attempt=0
while [ "$attempt" -lt "$MAX_ATTEMPTS" ]; do
    padded="$(printf '%04d' "$candidate")"
    obj="$(unique_object "$padded")"

    if err="$(git push "$REMOTE" "$obj:$REF_NS/$padded" 2>&1)"; then
        printf '%s\n' "$padded"
        echo "reserved RFC $padded on $REMOTE ($REF_NS/$padded)" >&2
        if [ -n "$slug" ]; then
            echo "  file it as: docs/design/$padded-$slug.md" >&2
        fi
        echo "  it needs a \`# RFC-$padded — <title>\` heading, a \`**Status:**\` line," >&2
        echo "  and a row in docs/design/README.md (both gated)" >&2
        exit 0
    fi

    # Taken by another session between our read and our write — exactly the race
    # this exists for. Step on and retry; not an error.
    if nros_grep_q -iE "already exists|non-fast-forward|fetch first|rejected" <<<"$err"; then
        candidate=$((candidate + 1))
        attempt=$((attempt + 1))
        continue
    fi

    echo "[WARN] could not reserve an RFC number on '$REMOTE':" >&2
    printf '%s\n' "$err" | sed 's/^/       /' >&2
    echo "" >&2
    echo "       Falling back to the local maximum + 1. This is the ORIGINAL" >&2
    echo "       RACY behaviour: another session may pick the same number, and" >&2
    echo "       the collision only surfaces once both have pushed — as a sweep" >&2
    echo "       across every file citing the number, not a one-line fix." >&2
    printf '%04d\n' "$((file_max + 1))"
    exit 0
done

echo "[FAIL] $MAX_ATTEMPTS consecutive numbers were already reserved." >&2
echo "       That is not contention any more — check \`git ls-remote $REMOTE '$REF_NS/*'\`" >&2
echo "       for stale reservations left by sessions that never filed." >&2
exit 1
