#!/usr/bin/env bash
#
# Reserve the next free `docs/issues/NNNN-` id, ATOMICALLY, across every
# parallel session.
#
# WHY A TOOL AND NOT AN INSTRUCTION
#
# CLAUDE.md has said "fetch and check origin's highest issue id (incl.
# `archived/`) before filing" for a long time, and the ids collided anyway —
# six times by 2026-08-01, plus the batch `issue-ids-check.sh` was written for
# ("six ids duplicated across thirteen files"). The instruction is not the
# problem; the RACE is. Two sessions both read "highest = 371", both write
# `0372-*.md`, and neither can see the other until one pushes. No amount of
# looking-before-filing fixes a check-then-act race — the check and the act
# have to be one atomic step against shared state.
#
# THE ATOMIC PRIMITIVE
#
# `git push` creating a ref that already exists on the remote is REJECTED.
# That is a compare-and-swap, and it is the only shared state every session
# already has access to. So: build an object nobody else can produce, push it
# to `refs/issue-ids/NNNN`, and treat push success as ownership of the id.
#
# The object must be UNIQUE per attempt, or the CAS is unsound: pushing a ref
# that already exists pointing at the SAME object is a silent no-op success, so
# two sessions pushing an identical object would both believe they won. A
# commit object carrying a timestamp (nanoseconds), pid and hostname is unique
# in practice, and its content makes the reservation self-documenting.
#
# The refs live in `refs/issue-ids/`, NOT `refs/tags/`, so they never clutter
# `git tag -l` and are not pulled by a default fetch. Nobody has to look at
# them.
#
# DEGRADATION
#
# If the reservation push fails for any reason OTHER than "already taken" — no
# network, no push permission, a read-only mirror — this reports that clearly
# and falls back to the local maximum. That fallback is exactly the old racy
# behaviour, so it says so rather than pretending the id is safe.

set -euo pipefail
cd "$(dirname "$0")/.."

REF_NS="refs/issue-ids"
REMOTE="${NROS_ISSUE_REMOTE:-origin}"
MAX_ATTEMPTS=25

slug="${1:-}"

# --------------------------------------------------------------------------
# Before reserving: show what already exists on this subject.
# --------------------------------------------------------------------------
#
# This script has always guarded duplicate IDs — the numbering race, where two
# sessions pick the same number. It said nothing about duplicate SUBJECTS, and
# that is the failure that actually costs: an issue filed for work already done
# or in flight. Eleven of those happened in one session on 2026-09-04/05, every
# one surfacing later through a merge conflict, a failing gate, or reading the
# code — never at the moment of filing, which is where it is cheapest.
#
# `scripts/issues.py` already searches title and body, and `--all` reaches
# `archived/` — which is where an ALREADY-FIXED issue lives, and therefore
# exactly where a duplicate hides. Nothing prompted anyone to run it.
#
# Advisory, never fatal: a false match must not block filing, and the search
# failing must not either. `NROS_ISSUE_SKIP_SEARCH=1` silences it for scripts.
#
# The terms are the slug's own words. `issues.py` ANDs them, so a six-word slug
# matches nothing — hence the narrowing: three words, then two, then one. The
# query that produced the hits is printed, because a reader has to know how
# wide a net caught them.
prior_art_search() {
    [ -n "$slug" ] || return 0
    [ -z "${NROS_ISSUE_SKIP_SEARCH:-}" ] || return 0
    command -v python3 >/dev/null 2>&1 || return 0
    [ -f scripts/issues.py ] || return 0

    local words
    words="$(printf '%s\n' "$slug" | tr '-' '\n' \
        | awk 'length($0) >= 4' \
        | grep -vxE 'with|when|that|this|from|into|does|only|then|than|were|been|have|they|what|make|made|used|uses|using|about|after|before' || true)"
    [ -n "$words" ] || return 0

    # Rank by SELECTIVITY, not by length. Length is the wrong proxy: in the case
    # this was built for, the longest word (`nonconforming`) was the filer's own
    # coinage and matched NOTHING, while `stdbool` — the shared vocabulary —
    # matched the three issues that mattered. `nuttx` matched 256 and is equally
    # useless in the other direction.
    #
    # So a term earns its place by matching FEW issues but more than zero. The
    # ledger query is ~20 ms, so scoring every word costs a fraction of a second.
    local best_term="" best_count=0 w c
    while IFS= read -r w; do
        [ -n "$w" ] || continue
        c="$(python3 scripts/issues.py --all "$w" 2>/dev/null | wc -l || echo 0)"
        [ "$c" -gt 0 ] || continue          # a word nobody else used says nothing
        [ "$c" -le 12 ] || continue         # a word everybody used says nothing either
        if [ -z "$best_term" ] || [ "$c" -lt "$best_count" ]; then
            best_term="$w"
            best_count="$c"
        fi
    done <<EOF
$words
EOF

    [ -n "$best_term" ] || return 0

    echo "" >&2
    echo "Existing issues matching '$best_term' (open AND archived):" >&2
    python3 scripts/issues.py --all "$best_term" 2>/dev/null | head -8 | sed 's/^/  /' >&2
    echo "" >&2
    echo "  If one of these is your subject, ADD to it rather than filing a" >&2
    echo "  second row — an archived match means the work may already be DONE." >&2
    echo "  Search wider yourself with: just issues --all <terms>" >&2
    echo "" >&2
    return 0
}

prior_art_search

# Highest id already used by a FILE, in either docs/issues/ or archived/.
# `archived/` counts — forgetting it is how ids get reused for a second time
# after the first document is filed away.
highest_file_id() {
    local max=0 id
    while IFS= read -r base; do
        id="$(printf '%s' "$base" | grep -oE '^[0-9]{4}' || true)"
        [ -z "$id" ] && continue
        id=$((10#$id))
        [ "$id" -gt "$max" ] && max="$id"
    done < <(git ls-files 'docs/issues/*.md' 'docs/issues/archived/*.md' | xargs -r -n1 basename)
    printf '%s' "$max"
}

# Highest id already RESERVED by any session, including ones whose file has not
# been committed yet. This is the part a file scan cannot see.
highest_reserved_id() {
    local max=0 id
    while IFS= read -r ref; do
        id="$(printf '%s' "${ref##*/}" | grep -oE '^[0-9]{4}' || true)"
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
    printf 'reserve issue %04d\n\nhost: %s\npid: %s\nwhen: %s\nslug: %s\n' \
        "$id" "$(hostname 2>/dev/null || echo unknown)" "$$" \
        "$(date +%s%N 2>/dev/null || date +%s)" "${slug:-<unspecified>}" |
        git commit-tree "$empty_tree"
}

file_max="$(highest_file_id)"
reserved_max="$(highest_reserved_id)"
candidate=$(( (file_max > reserved_max ? file_max : reserved_max) + 1 ))

attempt=0
while [ "$attempt" -lt "$MAX_ATTEMPTS" ]; do
    id="$(printf '%04d' "$candidate")"
    obj="$(unique_object "$candidate")"

    if err="$(git push "$REMOTE" "$obj:$REF_NS/$id" 2>&1)"; then
        printf '%s\n' "$id"
        echo "reserved issue id $id on $REMOTE ($REF_NS/$id)" >&2
        if [ -n "$slug" ]; then
            echo "  file it as: docs/issues/$id-$slug.md  (frontmatter \`id: $((10#$id))\`)" >&2
        fi
        exit 0
    fi

    # Taken by another session between our read and our write — exactly the
    # race this exists for. Step to the next id and retry; do NOT treat it as
    # an error.
    if printf '%s' "$err" | grep -qiE "already exists|non-fast-forward|fetch first|rejected"; then
        candidate=$((candidate + 1))
        attempt=$((attempt + 1))
        continue
    fi

    # Anything else is an environment problem, not contention.
    echo "[WARN] could not reserve an id on '$REMOTE':" >&2
    printf '%s\n' "$err" | sed 's/^/       /' >&2
    echo "" >&2
    echo "       Falling back to the local maximum + 1. This is the ORIGINAL" >&2
    echo "       RACY behaviour: another session may pick the same id, and the" >&2
    echo "       collision will only surface once both have pushed." >&2
    echo "       The pre-push hook still refuses to let a duplicate land." >&2
    printf '%04d\n' "$((file_max + 1))"
    exit 0
done

echo "[FAIL] $MAX_ATTEMPTS consecutive ids were already reserved." >&2
echo "       That is not contention any more — check \`git ls-remote $REMOTE '$REF_NS/*'\`" >&2
echo "       for stale reservations left by sessions that never filed." >&2
exit 1
