# phase-300 W3 — the ONE canonical build-tree prune list (sourced, not run).
#
# Directory BASENAMES that hold build products and must never be walked by
# `find`, read by `grep -r`, or hashed into a fixture signature. Before this
# file the list was copy-pasted inline at 8+ sites, each a different
# incomplete subset (audit F1/F3/F4/F6) — every new build-dir name (e.g.
# `target-embedded`, `_deps`) had to be re-discovered per site.
#
# PREFER NOT WALKING AT ALL: for tracked files use `git ls-files <glob>`
# (the enumeration SSoT — O(index), structurally immune to build trees).
# Source this file only where a walk over UNTRACKED artifacts is genuinely
# required (clean recipes, payload probes).
#
# Usage:
#   source scripts/build/prune-dirs.sh
#   find <root> "${NROS_FIND_PRUNE[@]}" -o -type f -print
#   grep -rn "${NROS_GREP_EXCLUDES[@]}" <pattern> <root>

# shellcheck disable=SC2034  # consumed by sourcing scripts
NROS_PRUNE_DIRS=(
    target 'target-*'
    build 'build-*'
    _deps generated cargo-target install log
    node_modules .git
)

# Ready-made `find` prune group: \( -name a -o -name b ... \) -prune
NROS_FIND_PRUNE=('(')
for _d in "${NROS_PRUNE_DIRS[@]}"; do
    NROS_FIND_PRUNE+=(-name "$_d" -o)
done
unset 'NROS_FIND_PRUNE[-1]'
NROS_FIND_PRUNE+=(')' -prune)
unset _d

# Ready-made grep exclusions (basename globs are honored by --exclude-dir).
NROS_GREP_EXCLUDES=()
for _d in "${NROS_PRUNE_DIRS[@]}"; do
    NROS_GREP_EXCLUDES+=("--exclude-dir=$_d")
done
unset _d
