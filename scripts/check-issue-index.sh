#!/usr/bin/env bash
#
# The "Open issues" list in docs/issues/README.md must name exactly the files in
# docs/issues/ — no more, no fewer.
#
# THE RULE ALREADY EXISTS. docs/issues/README.md states it under Conventions:
#
#   3. **"Open issues" below lists exactly the files in `docs/issues/`** — one row
#      per open issue, no more. A resolved issue keeps a row only under "Recently
#      resolved" … Verify with: `ls docs/issues/0*.md` versus the `**#NNN**` rows.
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

readme="docs/issues/README.md"

rows="$(grep -oE '^\*\*#[0-9]+\*\*' "$readme" \
        | grep -oE '[0-9]+' \
        | awk '{printf "%04d\n", $1}' \
        | sort -u)"

files="$(git ls-files 'docs/issues/0*.md' \
         | sed -E 's#.*/([0-9]{4}).*#\1#' \
         | sort -u)"

missing_file="$(comm -23 <(printf '%s\n' "$rows") <(printf '%s\n' "$files"))"
missing_row="$(comm -13 <(printf '%s\n' "$rows") <(printf '%s\n' "$files"))"

status=0

if [ -n "$missing_file" ]; then
    status=1
    echo "check-issue-index: listed as OPEN but no file in docs/issues/:" >&2
    for id in $missing_file; do
        if git ls-files --error-unmatch "docs/issues/archived/${id}-"*.md >/dev/null 2>&1; then
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
for f in $(git ls-files 'docs/issues/0*.md'); do
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
        id="$(printf '%s' "$f" | sed -E 's#.*/([0-9]{4}).*#\1#')"
        echo "  ${f} — frontmatter says status: ${st}" >&2
        echo "        git mv it to docs/issues/archived/ and convert its row:" >&2
        echo "        Recently resolved (YYYY-MM-DD): **#${id}** — … See \`archived/${id}-*\`." >&2
    done
fi

# One `Recently resolved` row per id. See header note 2.
dup_resolved="$(grep -oE 'Recently resolved \([^)]*\): \*\*#[0-9]+\*\*' "$readme" \
                | grep -oE '#[0-9]+' | tr -d '#' \
                | awk '{printf "%04d\n", $1}' \
                | sort | uniq -d)"
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
