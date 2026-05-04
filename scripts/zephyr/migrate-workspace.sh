#!/bin/bash
# Migrate legacy sibling Zephyr workspace into the in-tree default.
#
# Old layout (pre-Phase-113):
#   $parent/nano-ros/                       <- repo
#   $parent/nano-ros-workspace/             <- workspace (sibling)
#   $parent/nano-ros/zephyr-workspace ---> ../nano-ros-workspace  (symlink)
#
# New layout:
#   $parent/nano-ros/                       <- repo
#   $parent/nano-ros/zephyr-workspace/      <- workspace (gitignored)
#
# This script replaces the sibling-symlink-pair with a real directory at
# $repo/zephyr-workspace/. The actual workspace contents are MOVED, not
# copied — `mv` keeps west's internal absolute paths working as long as
# they didn't bake the old path into a config file (they don't, per the
# Zephyr 3.7 layout we use).
#
# Usage:
#   ./scripts/zephyr/migrate-workspace.sh [--dry-run]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NANO_ROS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
NANO_ROS_PARENT="$(dirname "$NANO_ROS_ROOT")"
NANO_ROS_NAME="$(basename "$NANO_ROS_ROOT")"
LEGACY_SIBLING="$NANO_ROS_PARENT/${NANO_ROS_NAME}-workspace"
IN_TREE="$NANO_ROS_ROOT/zephyr-workspace"

DRY_RUN=false
[ "${1:-}" = "--dry-run" ] && DRY_RUN=true

run() {
    echo "+ $*"
    [ "$DRY_RUN" = true ] || "$@"
}

if [ -d "$IN_TREE" ] && [ ! -L "$IN_TREE" ]; then
    echo "Already migrated: $IN_TREE is a real directory."
    exit 0
fi

if [ ! -d "$LEGACY_SIBLING/.west" ]; then
    echo "No legacy sibling workspace found at $LEGACY_SIBLING"
    echo "Run \`just zephyr setup\` to create the in-tree workspace."
    exit 0
fi

echo "Migrating workspace:"
echo "  from: $LEGACY_SIBLING"
echo "  to:   $IN_TREE"
echo ""

# 1. Drop any pre-existing in-tree symlink pointing at the sibling.
if [ -L "$IN_TREE" ]; then
    run rm "$IN_TREE"
fi

# 2. Move the sibling into the repo.
run mv "$LEGACY_SIBLING" "$IN_TREE"

# 3. Fix the inner $name -> $repo symlink: previously pointed at an
#    absolute path that may now resolve through the new location, but
#    re-creating it is cheap and removes ambiguity.
INNER_LINK="$IN_TREE/$NANO_ROS_NAME"
if [ -L "$INNER_LINK" ]; then
    run rm "$INNER_LINK"
    run ln -s "$NANO_ROS_ROOT" "$INNER_LINK"
fi

echo ""
echo "Migration complete."
echo "Verify: \`just zephyr doctor\`"
