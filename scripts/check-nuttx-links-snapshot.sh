#!/usr/bin/env bash
#
# phase-339 W3 — no NuttX consumer may link the SHARED live kernel tree.
#
# # What this protects
#
# `third-party/nuttx/nuttx` is one configured tree and NuttX builds in-tree, so
# `staging/` belongs to whichever architecture built last. A consumer that links
# it is linking a directory the other arch rewrites: arm entries went stale the
# moment riscv built, their cells stopped running, and two consecutive green
# builds could not converge (issue 0433).
#
# W1/W2 moved consumers onto per-arch export snapshots
# (`nros-nuttx-export-<arch>/`), which is NuttX's own build-once-link-many
# mechanism. This gate is what keeps them there. The failure it prevents is
# SILENT — a fixture that reaches back into the live tree still builds and still
# runs; it only breaks the OTHER architecture, later, as a staleness report that
# looks like a flake.
#
# # Why source-level and not build-artifact-level
#
# Checking emitted `.d` files would be stronger but needs a completed NuttX
# build, which no fast gate can assume. This greps the SOURCE for the live-tree
# spelling instead: it is the thing a future edit would reintroduce, it runs in
# milliseconds, and it cannot report a false green on a machine that has never
# built NuttX. Buildless.

set -euo pipefail
cd "$(dirname "$0")/.."

# Consumers whose link inputs must come from the snapshot. The build script that
# PRODUCES the tree is excluded by construction — it is the one thing that must
# name `staging/`.
CONSUMERS=(
    "packages/boards/nros-board-common/src/nuttx_ffi_build.rs"
    "packages/boards/nros-board-common/src/nuttx_image_link.rs"
)

fail=0
for f in "${CONSUMERS[@]}"; do
    [ -f "$f" ] || continue
    # `join("staging")` is the live-tree spelling. The compatibility fallback in
    # `nuttx_export.rs` is the ONE sanctioned use, and it lives in that file.
    hits="$(grep -nE '\.join\("staging"\)' "$f" || true)"
    if [ -n "$hits" ]; then
        echo "[FAIL] $f links the SHARED live NuttX tree:" >&2
        printf '  %s\n' "$hits" >&2
        fail=1
    fi
done

# The fallback is allowed exactly once, in the resolver that owns the policy.
resolver="packages/boards/nros-board-common/src/nuttx_export.rs"
if [ -f "$resolver" ]; then
    n="$(grep -cE '\.join\("staging"\)' "$resolver" || true)"
    if [ "$n" -gt 1 ]; then
        echo "[FAIL] $resolver names the live tree $n times — expected exactly one" >&2
        echo "       (the documented pre-phase-339 compatibility fallback)." >&2
        fail=1
    fi
fi

if [ "$fail" != 0 ]; then
    echo "" >&2
    echo "  Resolve kernel inputs through \`nros_board_common::nuttx_export\`:" >&2
    echo "  it returns the per-arch export snapshot and falls back to the live" >&2
    echo "  tree in ONE place. Linking \`staging/\` directly reintroduces issue" >&2
    echo "  0433 — the other architecture's build silently stales this one." >&2
    exit 1
fi

echo "nuttx-links-snapshot OK — consumers resolve the per-arch export, not the shared tree."
