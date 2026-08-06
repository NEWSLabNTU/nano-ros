#!/usr/bin/env bash
# Every numbered-series document path written anywhere in the tree must resolve
# to a file that exists: `docs/design/NNNN-*.md`, `docs/issues/NNNN-*.md`, and
# `docs/roadmap/phase-NNN-*.md`.
#
# The numbered series are referenced by PATH from prose, from issue
# frontmatter, and — the case that motivated this — from user-facing build
# output. `cmake/platform/nano-ros-freertos.cmake` told anyone integrating
# nano-ros board-less to "See <the board-support RFC under its OLD id 0062>",
# a file that had not existed since that RFC was renumbered 0062 → 0064 on an
# id collision. The renumber updated the document and every prose link; the
# cmake string was the one reference nothing grepped, so the error message a
# user reaches at exactly their most confused moment pointed at a 404.
#
# (That example is written out rather than quoted verbatim on purpose: a
# literal stale path in this file is a reference like any other, and this gate
# would flag itself. It did, once — because the first run happened while this
# script was still untracked, and `git grep` cannot see what git does not
# track. Verify a grep-based gate AFTER staging it.)
#
# Renumbering is rare but not exceptional — parallel sessions collide on ids
# often enough that `just issue-new` exists to reserve them — and archiving
# MOVES a resolved issue under `archived/`. So a reference resolves if the file
# is at the named path OR in that series' `archived/` directory, which is where
# the same basename lands.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

broken=0

# `git grep` over tracked files only: the enumeration SSoT, and structurally
# immune to build trees (an unfiltered walk here takes minutes and finds
# vendored copies).
#
# THIS FILE IS EXCLUDED from its own scan. The comment above names the dead
# `0062-` path as the worked example of what this gate catches, so scanning
# itself makes the gate fail on its own documentation — which is what it did
# the moment it landed. A gate that cannot describe the bug it prevents is
# worse than one that quietly skips one file.
while IFS= read -r hit; do
    src="${hit%%:*}"
    rest="${hit#*:}"
    line="${rest%%:*}"
    ref="${hit##*:}"
    [ -f "$ref" ] && continue
    # Resolved issues / superseded RFCs move to <series>/archived/<same-name>.
    series="$(dirname "$ref")"
    [ -f "$series/archived/$(basename "$ref")" ] && continue
    if [ "$broken" -eq 0 ]; then
        echo "check-doc-refs: references to documents that do not exist:" >&2
    fi
    echo "  $src:$line -> $ref" >&2
    broken=$((broken + 1))
done < <(git grep -onE "docs/(design|issues|roadmap)/(phase-)?[0-9]{3,4}-[a-z0-9.-]+\.md" -- . \
    ':!scripts/check-doc-refs.sh' 2>/dev/null)

# Phase docs link each other by BARE FILENAME from inside `docs/roadmap/`
# (`[phase-NNN](phase-NNN-slug.md)`), which the absolute-path scan above cannot
# see. That is the form that broke: completing a phase MOVES it to `archived/`,
# and five were archived in one pass while the gate reported OK — it was
# checking two of the three series, and only the `docs/`-prefixed spelling of
# those. A guard narrower than the rule it enforces (issue 0196), found by
# having to repair the links by hand right after it said OK.
#
# Resolve relative to the LINKING file's directory, then apply the same
# archived/ fallback as above.
while IFS= read -r hit; do
    src="${hit%%:*}"
    rest="${hit#*:}"
    line="${rest%%:*}"
    ref="${hit##*(}"
    ref="${ref%)}"
    # Absolute-from-repo-root spellings are already covered by the scan above.
    case "$ref" in docs/*) continue ;; esac
    resolved="$(dirname "$src")/$ref"
    [ -f "$resolved" ] && continue
    # A bare-filename sibling link is written when both docs sit in the same
    # directory, and stays behind when EITHER of them is archived. So the two
    # dirs are one series for resolution purposes and both directions count:
    # active -> archived (the doc completed) and archived -> active (this doc
    # completed, the one it cites has not).
    [ -f "$(dirname "$resolved")/archived/$(basename "$resolved")" ] && continue
    case "$(dirname "$resolved")" in
        */archived) [ -f "$(dirname "$(dirname "$resolved")")/$(basename "$resolved")" ] && continue ;;
    esac
    # The SOURCE moved. A `../design/NNNN-*.md` written from `docs/issues/` is
    # correct until that issue is archived, at which point `../` lands in
    # `docs/issues/` instead of `docs/` and every cross-series link in the file
    # is one level short. 147 of the 150 hits the first version of this arm
    # reported were exactly that — links that were right when written and that a
    # reader still follows without trouble. Re-resolve as if the source were
    # still un-archived, and only then call it broken.
    case "$(dirname "$src")" in
        */archived)
            unarchived="$(dirname "$(dirname "$src")")/$ref"
            [ -f "$unarchived" ] && continue
            [ -f "$(dirname "$unarchived")/archived/$(basename "$unarchived")" ] && continue
            ;;
    esac
    if [ "$broken" -eq 0 ]; then
        echo "check-doc-refs: references to documents that do not exist:" >&2
    fi
    echo "  $src:$line -> $ref (relative to $(dirname "$src"))" >&2
    broken=$((broken + 1))
done < <(git grep -onE "\]\(((\.\./)*[a-z][a-z-]*/)*(phase-[0-9]{3}[a-z0-9.-]*|[0-9]{4}-[a-z0-9-]+)\.md\)" -- 'docs/**/*.md' \
    ':!scripts/check-doc-refs.sh' 2>/dev/null)

if [ "$broken" -ne 0 ]; then
    {
        echo
        echo "  Either the document was renumbered (check its header for a"
        echo "  \"Renumbered NNNN -> NNNN\" note) or archived under a path this"
        echo "  reference does not name. Point the reference at the current file."
    } >&2
    exit 1
fi

echo "check-doc-refs: OK (every referenced design/issue/roadmap document exists)"
