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
# issue 1616 (W7): the consumer list was AUTHORED (two files), so a third
# build script — `nros-board-nuttx-qemu/build.rs` joining `staging` itself —
# passed. The population is now every tracked Rust source (comments stripped):
# the live-tree spelling may appear exactly ONCE, in the resolver's documented
# compatibility fallback, and nowhere else.
fail=0
if ! python3 - <<'PY'
import re, sys
sys.path.insert(0, "scripts/lib")
import comments, file_kinds, population

LIVE = re.compile(r'\.join\(\s*"staging"\s*\)')
RESOLVER = "packages/boards/nros-board-common/src/nuttx_export.rs"

def count(text):
    return len(LIVE.findall(comments.strip_comments(text, "rust")))

# Normal-path selftest: a comment is not a use; a real join is.
assert count('// p.join("staging")\nfn f() {}\n') == 0
assert count('fn f(p: &P) -> Q { p.join("staging") }\n') == 1

files = file_kinds.files_of_kind("rust")
if not population.require_population(files, "Rust source(s)", gate="nuttx-links-snapshot"):
    sys.exit(1)
bad = []
for rel in files:
    try:
        n = count(open(rel, errors="replace").read())
    except OSError:
        continue
    want = 1 if rel == RESOLVER else 0
    if n > want:
        bad.append(f"{rel}: {n} live-tree join(s), {want} allowed")
    if rel == RESOLVER and n == 0:
        bad.append(f"{rel}: the documented fallback is gone — drop this exemption")
for b in bad:
    print(f"[FAIL] {b}", file=sys.stderr)
sys.exit(1 if bad else 0)
PY
then
    fail=1
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
