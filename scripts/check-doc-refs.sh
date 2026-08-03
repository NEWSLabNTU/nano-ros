#!/usr/bin/env bash
# Every `docs/design/NNNN-*.md` and `docs/issues/NNNN-*.md` path written
# anywhere in the tree must resolve to a file that exists.
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
done < <(git grep -onE "docs/(design|issues)/[0-9]{4}-[a-z0-9-]+\.md" -- . 2>/dev/null)

if [ "$broken" -ne 0 ]; then
    {
        echo
        echo "  Either the document was renumbered (check its header for a"
        echo "  \"Renumbered NNNN -> NNNN\" note) or archived under a path this"
        echo "  reference does not name. Point the reference at the current file."
    } >&2
    exit 1
fi

echo "check-doc-refs: OK (every referenced design/issue document exists)"
