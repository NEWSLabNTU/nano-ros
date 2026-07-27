#!/usr/bin/env bash
# Issue-id integrity guard.
#
# `docs/issues/NNNN-slug.md` is an id namespace shared by every parallel agent
# session, and the ids collided repeatedly: two sessions filing at the same
# time both picked "next free", and neither noticed. By 2026-07-27 six ids were
# duplicated across thirteen files, so `0051-*` matched three unrelated
# documents and every `See 0051-*` pointer was ambiguous.
#
# Two invariants, both cheap:
#   1. no id is used by more than one file (in `docs/issues/` or `archived/`);
#   2. a file's `id:` frontmatter matches its filename prefix.
#
# (2) matters because a renumber that renames the file but forgets the
# frontmatter leaves the document lying about which issue it is.
set -euo pipefail

cd "$(dirname "$0")/../.."
issues_dir="docs/issues"
status=0

# --- 1. duplicate ids ------------------------------------------------------
dups="$(
    find "$issues_dir" -maxdepth 2 -name '[0-9][0-9][0-9][0-9]-*.md' -print0 |
        xargs -0 -n1 basename |
        grep -oE '^[0-9]{4}' |
        sort | uniq -d
)"
if [ -n "$dups" ]; then
    status=1
    echo "[FAIL] duplicate issue ids:" >&2
    while read -r id; do
        [ -z "$id" ] && continue
        echo "  id $id:" >&2
        find "$issues_dir" -maxdepth 2 -name "$id-*.md" -printf '    %p\n' >&2
    done <<<"$dups"
    echo >&2
    echo "  Pick the next free id (highest existing + 1) for the LATER filing," >&2
    echo "  rename the file, update its \`id:\` frontmatter, and fix references." >&2
fi

# --- 2. frontmatter id matches the filename --------------------------------
while IFS= read -r -d '' f; do
    base="$(basename "$f")"
    fname_id="$(echo "$base" | grep -oE '^[0-9]{4}')"
    front_id="$(grep -m1 '^id:' "$f" 2>/dev/null | awk '{print $2}' || true)"
    [ -z "$front_id" ] && continue
    # Compare numerically so `id: 48` and `0048-` agree.
    if [ "$((10#$fname_id))" -ne "$((10#$front_id))" ] 2>/dev/null; then
        status=1
        echo "[FAIL] $base: filename id $fname_id != frontmatter id $front_id" >&2
    fi
done < <(find "$issues_dir" -maxdepth 2 -name '[0-9][0-9][0-9][0-9]-*.md' -print0)

if [ "$status" -eq 0 ]; then
    count="$(find "$issues_dir" -maxdepth 2 -name '[0-9][0-9][0-9][0-9]-*.md' | wc -l)"
    echo "[OK] issue ids unique and self-consistent ($count issues)"
fi
exit "$status"
