#!/usr/bin/env bash
# Build a nano-ros workspace that lives in NEITHER the nano-ros checkout NOR
# the west workspace — RFC-0085 D1's actual shape, end to end.
#
# WHY A SCRIPT AND NOT A TEST
#
# Compiling at test time is banned (AGENTS.md Testing): a test consumes a
# prebuilt fixture. This compiles, needs a Zephyr, and takes minutes. The
# plan-level half — which is where every location decision is made — IS tested,
# in `packages/cli/nros-cli-core/tests/build_verb_pipeline.rs`:
#
#   the_zephyr_workspace_can_live_in_a_third_tree
#   a_plan_is_answerable_with_no_zephyr_on_the_machine
#   a_missing_framework_names_how_to_supply_it
#
# What only a real build can show is the part those cannot reach: `nros sync`
# rewriting absolute paths into a MOVED tree, and the compile actually
# succeeding from there.
#
# WHAT THIS PROVES, AND WHAT IT DOES NOT
#
# Proves: the three trees can be three trees. The workspace is copied out of the
# checkout, the Zephyr is named explicitly, the framework is named explicitly,
# and the image builds and runs.
#
# Does NOT prove: dependency resolution from a clean install. The example
# workspaces path-dep the nano-ros crates, so a copied tree still points back at
# this checkout. That is legitimate — a user path-deps or registry-deps the same
# way — but it means this measures LOCATIONS, not a from-scratch resolve. Do not
# let a green run here be read as the stronger claim.
#
# Usage:
#   scripts/dev/two-tree-check.sh [--workspace examples/workspaces/rust]
#                                 [--image demo_bringup:zephyr]
#                                 [--keep]
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SRC_WS="$REPO/examples/workspaces/rust"
IMAGE="demo_bringup:zephyr"
KEEP=0

while [ $# -gt 0 ]; do
    case "$1" in
        --workspace) SRC_WS="$2"; shift 2 ;;
        --image)     IMAGE="$2"; shift 2 ;;
        --keep)      KEEP=1; shift ;;
        -h|--help)   sed -n '2,32p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) echo "two-tree-check: unknown argument $1" >&2; exit 2 ;;
    esac
done

# The Zephyr, resolved the same way `nros build` resolves it, so this script
# cannot disagree with the thing it is checking.
ZEPHYR_WS="${NROS_ZEPHYR_WORKSPACE:-$REPO/zephyr-workspace}"
if [ ! -d "$ZEPHYR_WS/zephyr" ]; then
    echo "two-tree-check: no Zephyr at $ZEPHYR_WS/zephyr" >&2
    echo "  Set NROS_ZEPHYR_WORKSPACE, or run: just zephyr setup" >&2
    exit 1
fi

command -v nros >/dev/null 2>&1 || {
    echo "two-tree-check: no \`nros\` on PATH — source ./activate.sh" >&2
    exit 1
}

# OUTSIDE the checkout. `mktemp -d` lands in $TMPDIR, which is what makes this a
# different tree rather than a subdirectory pretending to be one.
OUT="$(mktemp -d "${TMPDIR:-/tmp}/nros-two-tree-XXXXXX")"
cleanup() { [ "$KEEP" -eq 1 ] || rm -rf "$OUT"; }
trap cleanup EXIT

echo "two-tree-check: three trees"
echo "  framework : $REPO"
echo "  zephyr    : $ZEPHYR_WS"
echo "  workspace : $OUT/ws   (copied from $SRC_WS)"
echo

# Copy the SOURCES only. `build/`, `target*/` and `generated/` are outputs of
# the tree they were made in; carrying them over would let the copy pass on
# artifacts that were never rebuilt in their new location — which is precisely
# the thing under test.
mkdir -p "$OUT/ws"
tar -C "$SRC_WS" \
    --exclude='build' --exclude='target' --exclude='target-*' \
    --exclude='generated' --exclude='.cargo/nros-managed-patch.toml' \
    -cf - . | tar -C "$OUT/ws" -xf -

cd "$OUT/ws"

echo "==> nros sync   (rewrites absolute paths for THIS tree)"
NROS_REPO_DIR="$REPO" nros sync

echo
echo "==> nros build $IMAGE"
NROS_REPO_DIR="$REPO" nros build "$IMAGE" --zephyr-workspace "$ZEPHYR_WS"

# Assert the artifact is in the COPY, not back in the checkout. A build that
# silently produced its output in the source tree would look identical up to
# here, and would mean none of this proved anything.
ELF="$OUT/ws/build/zephyr/zephyr.elf"
if [ ! -f "$ELF" ]; then
    echo "two-tree-check: FAIL — no artifact at $ELF" >&2
    exit 1
fi
if [ -e "$SRC_WS/build/zephyr/zephyr.elf" ] \
   && [ "$SRC_WS/build/zephyr/zephyr.elf" -nt "$ELF" ]; then
    echo "two-tree-check: FAIL — the source tree's artifact is newer;" >&2
    echo "  the build wrote back into $SRC_WS instead of the copy." >&2
    exit 1
fi

echo
echo "two-tree-check: OK"
echo "  artifact: $ELF"
[ "$KEEP" -eq 1 ] && echo "  tree kept at $OUT (--keep)"
exit 0
