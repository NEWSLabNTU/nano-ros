#!/usr/bin/env bash
# A leaf `.cargo/config.toml` is tracked if and only if it carries content
# `nros sync` cannot reproduce.
#
# Sync writes these files (RFC-0048 W9): an `include = ["…/nros-patch.toml"]`
# line pointing at a gitignored, host-specific central file, and a
# `[patch.crates-io]` block of `# nros-managed` path entries. A config holding
# only that is a pure artifact — recreated by `nros sync` in any checkout — and
# committing it means a file that churns on every sync as the patch set moves.
#
# Many configs also carry hand-authored content: `[build] target` for a
# cross-compiled leaf, a QEMU `runner`, linker `rustflags`. Sync refreshes the
# patch block INSIDE those, but nothing can regenerate the rest. They must stay
# tracked, or a fresh clone loses the target selection and the leaf builds for
# the host, or not at all.
#
# `**/.gitignore` cannot tell the two apart — they share a filename and sit in
# the same directories — so the ignore rule is blanket and this gate supplies
# the discrimination it cannot. Without it, a new embedded example's
# hand-authored config would be silently ignored: fine on the machine that
# wrote it, missing for everyone else.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# The canonical build-tree prune list (phase-300 W3). Without it the walk
# descends into `build-zenoh/_deps/corrosion-src/test/…`, where vendored
# corrosion ships its own hand-authored `.cargo/config.toml` fixtures — none of
# which are ours to track.
# shellcheck source=scripts/build/prune-dirs.sh
source scripts/build/prune-dirs.sh

# Content beyond what sync writes: anything that is not blank, a comment, the
# include line, the patch table header, or an `# nros-managed` entry.
has_authored_content() {
    grep -qvE '^\s*$|^\s*#|^include = |^\[patch\.crates-io\]|# nros-managed\s*$' "$1"
}

untracked_authored=0
tracked_pure=0

while IFS= read -r -d '' cfg; do
    cfg="${cfg#./}"
    tracked=0
    git ls-files --error-unmatch "$cfg" >/dev/null 2>&1 && tracked=1

    if has_authored_content "$cfg"; then
        if [ "$tracked" -eq 0 ]; then
            if [ "$untracked_authored" -eq 0 ]; then
                echo "check-cargo-config-tracked: hand-authored cargo config NOT tracked:" >&2
            fi
            echo "  $cfg" >&2
            untracked_authored=$((untracked_authored + 1))
        fi
    elif [ "$tracked" -eq 1 ]; then
        if [ "$tracked_pure" -eq 0 ]; then
            echo "check-cargo-config-tracked: pure sync-output cargo config IS tracked:" >&2
        fi
        echo "  $cfg" >&2
        tracked_pure=$((tracked_pure + 1))
    fi
done < <(find examples packages "${NROS_FIND_PRUNE[@]}" -o -path '*/.cargo/config.toml' -print0 2>/dev/null)

rc=0
if [ "$untracked_authored" -ne 0 ]; then
    {
        echo
        echo "  These hold content `nros sync` cannot regenerate — a \`[build] target\`,"
        echo "  a runner, or link flags. \`**/.cargo/config.toml\` is gitignored because"
        echo "  most of these files are pure sync output, so committing one takes:"
        echo "      git add -f <path>"
    } >&2
    rc=1
fi
if [ "$tracked_pure" -ne 0 ]; then
    {
        echo
        echo "  These hold nothing but sync's own include + [patch.crates-io] block, so"
        echo "  they are recreated by \`nros sync\` and only churn in git. Untrack with:"
        echo "      git rm --cached <path>"
    } >&2
    rc=1
fi

[ "$rc" -eq 0 ] && echo "check-cargo-config-tracked: OK (tracked <=> hand-authored content)"
exit "$rc"
