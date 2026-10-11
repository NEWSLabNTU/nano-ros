#!/usr/bin/env bash
# The cache key for the CI CLI-binary cache (phase-489 W3).
#
# Prints one sha256. `gate.yml`'s `check-*` siblings RESTORE
# `nros` + `nros-launch-resolve` (+ its content stamp and Python half) under
# `nros-cli-bin-<os>-<this>`, and `warm-cache` SAVES under the same key.
#
# A SUPERSET of `source_stamp.rs`'s watch set, by construction rather than by
# mirroring its predicate: every git index entry under the stamp's closure —
# `packages/cli` plus each directory `packages/cli/cli-source-dirs.txt` lists
# (the closure `source_stamp.rs` reads from the same file). That covers every
# `.rs` / `.jinja` / `Cargo.*` / `askama.toml` the stamp folds, the list file
# itself, and the submodule GITLINKS — so the `play_launch` pin, which is a
# stamp input and also all of `nros-launch-resolve`'s layer-2 sources, is in
# the key. Plus `rust-toolchain.toml`, which builds both binaries.
#
# A superset can only cost a spurious MISS (an edit to a file the stamp
# ignores, e.g. a CLI test fixture). It cannot produce a hit for a tree whose
# stamp moved, which is the gap the previous key (`hashFiles` over `*.rs`,
# `Cargo.toml`, `Cargo.lock`) had. Even so the key is not the authority:
# `nros source-stamp` in the consuming step is, and it refuses a binary built
# from other sources.
#
# Index entries, not file contents: a CI checkout is clean, so the index sha IS
# the content, and `git ls-files -s` costs milliseconds where hashing the tree
# would not. On a dirty developer tree the key ignores uncommitted edits, which
# is why nothing outside CI should use it.
#
# Usage: scripts/ci/cli-bin-cache-key.sh [repo-root]
set -euo pipefail

root="${1:-$(git rev-parse --show-toplevel)}"
list="$root/packages/cli/cli-source-dirs.txt"
if [ ! -f "$list" ]; then
    echo "cli-bin-cache-key: $list is missing — source_stamp.rs would refuse to" >&2
    echo "  stamp without it, so there is no watch set to key on" >&2
    exit 1
fi

dirs=(packages/cli)
while IFS= read -r line; do
    line="${line%%#*}"
    line="${line//[[:space:]]/}"
    [ -n "$line" ] && dirs+=("$line")
done < "$list"

entries="$(git -C "$root" ls-files -s -- "${dirs[@]}" rust-toolchain.toml)"
if [ -z "$entries" ]; then
    echo "cli-bin-cache-key: no index entries under ${dirs[*]} — not a checkout?" >&2
    exit 1
fi
printf '%s\n' "$entries" | sha256sum | cut -d' ' -f1
