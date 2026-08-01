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

# Issue 0380 follow-up (2026-08-02) — `docs/design/` needs the same guard.
#
# RFC ids collided exactly as issue ids did, and for the same reason: parallel
# sessions each read "highest existing + 1". `0062` was used TWICE
# (`unified-dependency-ssot` 01:24, `board-support-organization` 09:59, both
# committed the same day) and nothing noticed, because this gate only ever
# looked at `docs/issues/`. A gate narrower than the rule it enforces is
# issue 0196's class — and it went unnoticed here for as long as it did
# precisely because the check LOOKED like it covered the numbered-doc series.
design_dups="$(
    find docs/design -maxdepth 1 -name '[0-9][0-9][0-9][0-9]-*.md' -print0 2>/dev/null |
        xargs -0 -r -n1 basename |
        grep -oE '^[0-9]{4}' |
        sort | uniq -d
)"
if [ -n "$design_dups" ]; then
    status=1
    echo "[FAIL] duplicate RFC ids in docs/design/:" >&2
    while read -r id; do
        [ -z "$id" ] && continue
        echo "  id $id:" >&2
        find docs/design -maxdepth 1 -name "$id-*.md" -printf '    %p\n' >&2
    done <<<"$design_dups"
    echo >&2
    echo "  Renumber the LATER filing to the next free id, update its \`# RFC-NNNN\`" >&2
    echo "  heading, self-references, the docs/design/README.md row, and any doc" >&2
    echo "  pointing at it. Reserve ids with \`just issue-new\` where possible." >&2
fi

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
