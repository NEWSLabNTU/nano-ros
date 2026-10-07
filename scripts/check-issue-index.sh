#!/usr/bin/env bash
#
# The "Open issues" list in docs/issues/README.md must name exactly the files in
# docs/issues/ — no more, no fewer.
#
# THE RULE ALREADY EXISTS. docs/issues/README.md states it under Conventions:
#
#   3. **"Open issues" below lists exactly the files in `docs/issues/`** — one row
#      per open issue, no more. A resolved issue keeps a row only under "Recently
#      resolved" … Verify with: `ls docs/issues/[0-9]*.md` versus the `**#NNN**` rows.
#
# It just had nothing enforcing it, and it drifts in ONE direction: an issue gets
# resolved, its file is moved to `archived/`, and its README row is left in the
# OPEN format. The index then advertises an open issue whose file is gone.
#
# That happened twice in two consecutive pulls (2026-08-07): #0465 (archived by
# `4b30c29cb`/`8151819b7`) and #0474 (archived by `2a89b5040`) each kept an
# open-format row. It is not carelessness — the archive step is a `git mv` plus
# a prose edit in a different part of a 600-line file, and nothing failed when
# the second half was skipped. Exactly the shape CLAUDE.md means by fixing the
# CLASS: the same rule was already written down, and being written down was not
# enough.
#
# WHAT COUNTS AS A ROW
#
# A line whose FIRST characters are `**#NNN**`. That is the open-row spelling,
# and it is what the convention's own "versus the `**#NNN**` rows" refers to.
# Resolved rows read `Recently resolved (DATE): **#NNN** — …`, so they do not
# start with the marker and are correctly ignored here.
#
# One consequence worth knowing: a wrapped line that HAPPENS to begin with
# `**#NNN**` reads as a row. That is not a false positive to paper over — it is
# the same ambiguity a human scanning the list hits, and it has already produced
# one (a #422 summary wrapped so a continuation line began `**#448**`, making a
# resolved issue look open). Reflow the paragraph; do not special-case it.
#
# TWO MORE CHECKS, and both close holes this gate had (2026-08-22)
#
# 1. A file in `docs/issues/` whose frontmatter says `status: resolved`.
#    The set comparison above cannot see it: the file exists, its open row
#    exists, the two match, green. But the issue is DONE and its row still
#    advertises it as open. That is the same drift the header describes,
#    stopped one step earlier — the `git mv` was skipped rather than the prose
#    edit. Found on #0745 and #0749 in one afternoon; #0749 was caught only
#    because it happened to have no row at all.
#
# 2. The same id under two `Recently resolved` rows. Two sessions resolving one
#    issue each write one, neither sees the other, and the index carries both —
#    observed on #0749 within an hour. A duplicate is not merely untidy: the two
#    rows say different things, and a reader has no way to know which is current.
#
# Buildless: two `ls`-equivalents and a `grep`. Fast tier.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
# shellcheck source=scripts/lib/population.sh
. scripts/lib/population.sh

# The ids of every `Recently resolved (DATE): **#NNN**` digest in a file, one
# per line, zero-padded. Shared by the gate and its negative control so the
# control exercises the matcher the gate runs.
resolved_digest_ids() {
    grep -oE 'Recently resolved \([^)]*\): \*\*#[0-9]+\*\*' "$1" \
        | grep -oE '#[0-9]+' | tr -d '#' \
        | awk '{printf "%04d\n", $1}'
}
dup_resolved_ids() {
    resolved_digest_ids "$1" | sort | uniq -d
}
# Negative control, run on every invocation: a duplicated digest IS reported,
# and a clean pair is not.
dup_resolved_selftest() {
    local t rc=0
    t="$(mktemp)"
    printf '%s\n' \
        'Recently resolved (2026-01-01): **#0007** — a.' \
        'Recently resolved (2026-01-02): **#7** — b, the same id unpadded.' \
        'Recently resolved (2026-01-03): **#0008** — c.' > "$t"
    [ "$(dup_resolved_ids "$t")" = "0007" ] || rc=1
    printf '%s\n' 'Recently resolved (2026-01-01): **#0007** — a.' > "$t"
    [ -z "$(dup_resolved_ids "$t")" ] || rc=1
    rm -f "$t"
    return "$rc"
}

# issue 0884 — the generated list moved OUT of README.md into `open.md`, so the
# authored prose and the machine-written rows are separate files. `merge=union`
# rode on that split until 2026-09-03, when it was retired: the driver does not
# run in GitHub's server-side merge or rebase, which is the only place the
# conflict mattered. The variable keeps its name; only the file it names changed.
readme="docs/issues/open.md"

# REGENERATE before reading — issue 0883.
#
# This gate used to compare the COMMITTED `open.md` against the issue files, so
# a PR that filed an issue and did not also regenerate the list failed. That
# made committing this generated file mandatory on every issue-filing PR, which
# made it the one path nearly every concurrent PR touches, which is why it
# conflicts on roughly every merge. The gate was the forcing function behind the
# churn it was meant to police.
#
# Regenerating first is not a weakened check, it is the same check one step
# earlier: `open.md` is a build artifact (gitignored per `.gitignore`), and the
# rows-match-files property is true BY CONSTRUCTION once it is generated from
# those files. What this gate still asserts for real is below — that the
# authored README has not grown a second, hand-maintained copy of the list, and
# that resolved issues appear only under the `Recently resolved` spelling.
#
# Regenerating also covers the fresh-clone and CI cases, where the artifact
# legitimately does not exist yet.
python3 scripts/gen-issue-index.py >/dev/null

# ...and it must not be TRACKED. Regenerating (above) removed the REQUIREMENT to
# commit this artifact; this removes the ABILITY, which is the half that had no
# enforcement and is why the untrack did not stick. `ba9879da8` removed the file
# from the index on 2026-09-03 and three later commits — `1069dd578`,
# `61489d5c4`, `9e2a417ac` — added it straight back. Nobody did anything
# unusual: they regenerated the artifact and staged it. `.gitignore` is silent
# on a path that is already tracked, so a single `git add` re-tracks it for
# every clone. A decision with no gate is a decision that lasts until the next
# person types the obvious command.
if git ls-files --error-unmatch "$readme" >/dev/null 2>&1; then
    echo "check-issue-index: $readme is TRACKED, and must not be." >&2
    echo "  It is a generated build artifact (issues 0883/0884): every PR" >&2
    echo "  regenerates it, so tracking it makes it the one path every" >&2
    echo "  concurrent PR conflicts on. \`merge=union\` cannot help — the" >&2
    echo "  driver does not run in GitHub's server-side merge or rebase." >&2
    echo "  Fix:  git rm --cached $readme" >&2
    echo "  (\`.gitignore\` already lists it; gitignore does not apply to a" >&2
    echo "  path that is already tracked, which is why this keeps coming back.)" >&2
    exit 1
fi

# ...and the authored README must NOT grow a second copy of it. One sat there
# after issue 0884 moved the list out, regenerated by nothing, drifted to 46
# rows against 59, and stayed a per-PR conflict site because agents kept
# editing it. A generated block inside an authored file is a conflict site no
# mitigation reaches, so the rule is that it does not exist.
# shellcheck source=scripts/lib/grep-q.sh
. scripts/lib/grep-q.sh
nros_grep_q "BEGIN GENERATED open-issue list" docs/issues/README.md
if [ $? -eq 0 ]; then
    echo "check-issue-index: docs/issues/README.md contains a generated" >&2
    echo "  open-issue block. The list lives in docs/issues/open.md (issue" >&2
    echo "  0884) — that file is generated end to end and is gitignored;" >&2
    echo "  README.md is authored and cannot. Delete the block and link instead." >&2
    exit 1
fi

# issue 0883's class, one file over — `docs/issues/README.md`'s "Recently
# resolved" digests are a per-PR CONFLICT SITE.
#
# Every entry is prepended at the same offset in one shared authored file, so
# two agents resolving issues on the same day conflict by construction. That is
# what `open.md` was until it was untracked, and this file cannot take the same
# fix: `open.md` is generated, so a clone can rebuild it; these digests are
# prose nothing can regenerate.
#
# So the block is FROZEN at its current size and the count is a ratchet. The
# digest for a newly resolved issue belongs in `archived/<id>-*.md`, which is
# where the fuller record already lives, and the list is read with
# `just issues --all --status resolved` — generated from the files, so it cannot
# drift.
#
# Ratchet, not zero: deleting the 327 existing digests would destroy history for
# a conflict that only future appends cause.
#
# A FROZEN count is forced DOWN too (phase-472 W9, `scripts/lib/ratchet.py`):
# a digest deleted without lowering `frozen_digests` leaves a free slot, and the
# next append regrows into it with this gate green. The 2026-10-07 gate-reach
# re-audit measured exactly that (delete one digest: rc 0). So the verdict is
# the class helper's `judge`, and a fall fails naming the edit to make.
frozen_digests=327
have_digests="$(grep -c '^Recently resolved' docs/issues/README.md || true)"
digest_ratchet() {  # <have> <frozen> -> prints rose|fell|ok
    python3 -c 'import sys; sys.path.insert(0, "scripts/lib"); import ratchet
r, f = ratchet.judge({"digests": int(sys.argv[1])}, {"digests": int(sys.argv[2])})
print("rose" if r else "fell" if f else "ok")' "$1" "$2"
}
# Normal-path negative controls: both directions must be seen by the verdict.
if [ "$(digest_ratchet 328 327)" != rose ] || [ "$(digest_ratchet 326 327)" != fell ] \
        || [ "$(digest_ratchet 327 327)" != ok ]; then
    echo "check-issue-index: SELF-TEST FAILED — the digest ratchet no longer sees a rise and a fall" >&2
    exit 1
fi
digest_verdict="$(digest_ratchet "${have_digests:-0}" "$frozen_digests")"
if [ "$digest_verdict" = fell ]; then
    echo "check-issue-index: docs/issues/README.md has $have_digests 'Recently" >&2
    echo "  resolved' digests, BELOW the frozen $frozen_digests. A ratchet is forced" >&2
    echo "  down: set frozen_digests=$have_digests in $0, or the freed slot is room" >&2
    echo "  for the next append to land green (phase-472 W9)." >&2
    exit 1
fi
if [ "$digest_verdict" = rose ]; then
    echo "check-issue-index: docs/issues/README.md has $have_digests 'Recently" >&2
    echo "  resolved' digests, above the frozen $frozen_digests." >&2
    echo "  That block is a per-PR conflict site: every entry is prepended at the" >&2
    echo "  same place in one shared file, so two agents resolving issues on the" >&2
    echo "  same day conflict by construction (issue 0883's class)." >&2
    echo "  Put the digest in \`docs/issues/archived/<id>-*.md\` instead — the" >&2
    echo "  fuller record is already there — and read the list with:" >&2
    echo "      just issues --all --status resolved" >&2
    exit 1
fi

# The CONTENT is frozen, not just the count (issue 1739's audit follow-up).
#
# A count says nothing about WHICH digests are there: delete one and append a
# new one and the count is still 327, so the ratchet above reads `ok` — that
# swap was measured passing (rc 0) after W9 called the hole closed. The block
# below the FROZEN-DIGESTS-BELOW marker is history nothing maintains, so its
# bytes are the invariant: any edit, deletion or addition changes the hash. The
# count arm stays because its message names the remedy for the common case.
frozen_block_sha256=a769ca89c5a3af06f64a5e6655bbb59b01c1ead1e5b424d0ef8e6186c19ee807
frozen_block_digest() {  # <file> -> sha256 of every line after the ONE marker; "" if none/many
    local body
    body="$(awk '/^<!-- FROZEN-DIGESTS-BELOW/ { n++; next } n == 1 { print }
                 END { if (n != 1) exit 3 }' "$1")" || return 0
    printf '%s\n' "$body" | sha256sum | cut -d' ' -f1
}
# Normal-path control: the SWAP (net count unchanged) and the DELETE must both
# move the digest; prose above the marker must not; a missing marker yields "".
frozen_block_selftest() {
    local d base rc=0
    d="$(mktemp -d)"
    printf '%s\n' 'prose' '<!-- FROZEN-DIGESTS-BELOW: x -->' \
        'Recently resolved (2026-01-01): **#0007** — a.' \
        'Recently resolved (2026-01-02): **#0008** — b.' > "$d/base"
    printf '%s\n' 'other prose' '<!-- FROZEN-DIGESTS-BELOW: x -->' \
        'Recently resolved (2026-01-01): **#0007** — a.' \
        'Recently resolved (2026-01-02): **#0008** — b.' > "$d/same"
    printf '%s\n' '<!-- FROZEN-DIGESTS-BELOW: x -->' \
        'Recently resolved (2026-01-01): **#0007** — a.' \
        'Recently resolved (2026-01-03): **#0009** — swapped in.' > "$d/swap"
    printf '%s\n' '<!-- FROZEN-DIGESTS-BELOW: x -->' \
        'Recently resolved (2026-01-01): **#0007** — a.' > "$d/del"
    printf '%s\n' 'Recently resolved (2026-01-01): **#0007** — a.' > "$d/nomark"
    base="$(frozen_block_digest "$d/base")"
    [ -n "$base" ] || rc=1
    [ "$(frozen_block_digest "$d/same")" = "$base" ] || rc=1
    [ "$(frozen_block_digest "$d/swap")" != "$base" ] || rc=1
    [ "$(frozen_block_digest "$d/del")" != "$base" ] || rc=1
    [ -z "$(frozen_block_digest "$d/nomark")" ] || rc=1
    rm -rf "$d"
    return "$rc"
}
if ! frozen_block_selftest; then
    echo "check-issue-index: SELF-TEST FAILED — the frozen-block digest no longer sees a swap or a delete" >&2
    exit 1
fi
have_block_sha256="$(frozen_block_digest docs/issues/README.md)"
if [ "$have_block_sha256" != "$frozen_block_sha256" ]; then
    echo "check-issue-index: the frozen 'Recently resolved' block in docs/issues/README.md" >&2
    echo "  CHANGED (sha256 ${have_block_sha256:-<not exactly one FROZEN-DIGESTS-BELOW marker>};" >&2
    echo "  frozen $frozen_block_sha256)." >&2
    echo "  The CONTENT is frozen, not just the count: deleting one digest and" >&2
    echo "  appending another keeps the count and is still the per-PR conflict" >&2
    echo "  site (issue 0883's class). Restore everything below the marker from" >&2
    echo "  origin/main; a new digest belongs in docs/issues/archived/<id>-*.md." >&2
    echo "  If the edit is deliberate (a correction), record it in THIS change:" >&2
    echo "  set frozen_block_sha256=${have_block_sha256:-?} (and frozen_digests) in $0." >&2
    exit 1
fi

# phase-395 W1 — the open list is GENERATED (scripts/gen-issue-index.py) and its
# rows are list items, `- **#NNNN** (area) — title`. Accept the optional bullet
# so this convention check keeps working against the generated block; the
# row<->file correspondence it asserts is now true by construction, but the
# "resolved rows only under the Recently resolved spelling" half still is not.
rows="$(grep -oE '^(- )?\*\*#[0-9]+\*\*' "$readme" \
        | grep -oE '[0-9]+' \
        | awk '{printf "%04d\n", $1}' \
        | sort -u)"

# `--cached --others --exclude-standard`, not bare `git ls-files`: a just-filed
# issue is UNTRACKED until it is staged, and that is exactly when this runs.
# Tracked-only enumeration made the checker disagree with
# `scripts/gen-issue-index.py` — the generator lists the new issue, the checker
# says the row names no file, and the two are unresolvable by editing either
# one. Both sides must enumerate the same set. → phase-395
files="$(git ls-files --cached --others --exclude-standard 'docs/issues/[0-9]*.md' \
         | sed -E 's#.*/([0-9]+)-.*#\1#' \
         | sort -u)"

missing_file="$(comm -23 <(printf '%s\n' "$rows") <(printf '%s\n' "$files"))"
missing_row="$(comm -13 <(printf '%s\n' "$rows") <(printf '%s\n' "$files"))"

status=0

if [ -n "$missing_file" ]; then
    status=1
    echo "check-issue-index: listed as OPEN but no file in docs/issues/:" >&2
    for id in $missing_file; do
        if git ls-files --error-unmatch "docs/issues/archived/${id}-"*.md >/dev/null 2>&1 \
           || compgen -G "docs/issues/archived/${id}-"*.md >/dev/null 2>&1; then
            echo "  #${id} — the file is in archived/, so the row is stale." >&2
            echo "        Convert it to the resolved spelling, which this gate ignores:" >&2
            echo "        Recently resolved (YYYY-MM-DD): **#${id}** — … See \`archived/${id}-*\`." >&2
        else
            echo "  #${id} — no file anywhere. Either the row is a typo or the issue was never filed." >&2
        fi
    done
fi

if [ -n "$missing_row" ]; then
    status=1
    echo "check-issue-index: open issue file with no row in the index:" >&2
    for id in $missing_row; do
        echo "  #${id} — add a row: **#${id}** — <one-line hook>. See \`${id}-*\`. (DATE)" >&2
    done
fi

# A file living in docs/issues/ must still be OPEN. See header note 1.
resolved_in_open=""
for f in $(git ls-files --cached --others --exclude-standard 'docs/issues/[0-9]*.md' | sort -u); do
    st="$(sed -n 's/^status:[[:space:]]*//p' "$f" | head -1 | tr -d '[:space:]')"
    case "$st" in
        open|"") ;;
        *) resolved_in_open="${resolved_in_open}${f}|${st} " ;;
    esac
done
if [ -n "$resolved_in_open" ]; then
    status=1
    echo "check-issue-index: file in docs/issues/ is not open:" >&2
    for entry in $resolved_in_open; do
        f="${entry%%|*}"; st="${entry##*|}"
        id="$(printf '%s' "$f" | sed -E 's#.*/([0-9]+)-.*#\1#')"
        echo "  ${f} — frontmatter says status: ${st}" >&2
        echo "        git mv it to docs/issues/archived/ and convert its row:" >&2
        echo "        Recently resolved (YYYY-MM-DD): **#${id}** — … See \`archived/${id}-*\`." >&2
    done
fi

# One `Recently resolved` row per id. See header note 2.
#
# phase-472 W4 — this arm read "$readme", i.e. the GENERATED `open.md`, from the
# day issue 0884 moved the open list out of README.md. `open.md` carries no
# `Recently resolved` digests at all — they stayed in the AUTHORED README.md —
# so the arm examined zero rows and could never fire (the 2026-09-28 audit
# confirmed a duplicated digest passed). It reads the file the digests are in,
# prints how many it examined, and fails on zero.
digests="docs/issues/README.md"
dup_resolved_selftest || {
    echo "check-issue-index: the duplicate-digest arm's own negative control failed;" >&2
    echo "  its verdict cannot be trusted." >&2
    exit 1
}
nros_require_population_self_test || exit 1
n_digests="$(resolved_digest_ids "$digests" | grep -c . || true)"
nros_require_population "${n_digests:-0}" "'Recently resolved' digest row(s) in $digests" \
    check-issue-index || exit 1
dup_resolved="$(dup_resolved_ids "$digests")"
if [ -n "$dup_resolved" ]; then
    status=1
    echo "check-issue-index: more than one 'Recently resolved' row for:" >&2
    for id in $dup_resolved; do
        echo "  #${id} — two sessions each wrote one. Merge them into a single row;" >&2
        echo "        they say different things and a reader cannot tell which is current." >&2
    done
fi

if [ "$status" -ne 0 ]; then
    echo "" >&2
    echo "  docs/issues/README.md Conventions #3: the Open issues list names" >&2
    echo "  EXACTLY the files in docs/issues/. Resolved issues keep a row only" >&2
    echo "  under the 'Recently resolved' spelling." >&2
    exit 1
fi

echo "check-issue-index: OK ($(printf '%s\n' "$rows" | grep -c . ) open row(s) = $(printf '%s\n' "$files" | grep -c . ) file(s))"
