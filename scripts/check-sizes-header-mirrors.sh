#!/usr/bin/env bash
#
# Issue 0268 — drift gate for the per-build sizes headers (the "stale mirror" class).
#
# `nros-c` / `nros-cpp` build.rs emit `nros_config_generated.h` and
# `nros_cpp_config_generated.h` (executor/entity storage sizes) into the corrosion
# build dir; CMake mirrors them into a sibling `include/nros/` that sits on every
# consumer's include path. The two copies MUST be identical: the C `_opaque`
# buffers are sized from the mirror, while the Rust objects placement-constructed
# into them are sized by the crate that wrote the source. A stale mirror is silent
# memory corruption — issue 0268 (freertos C, 336 bytes short: `register_subscription`
# returned -1 and the zenoh session self-closed), issue 0245 (zephyr C++, 32 bytes),
# and the 0088/0114/0122/0123 lineage before them.
#
# The structural fix is the real Ninja input edge on the mirror commands (see the
# 0268 comment blocks in `packages/core/nros-{c,cpp}/CMakeLists.txt`). This gate is
# the cheap backstop: it re-proves the invariant on whatever build trees exist
# locally, so a NEW build path that reintroduces an order-only mirror is caught by a
# fast check instead of a week-long bisect.
#
# Vacuously green when no build trees exist (a fresh clone / CI runner), so the
# scanned-tree count is always printed — a "0 trees" pass is not evidence of health
# (the issue-0196 rule: a probe that watches nothing must say so).

#
# `--fix` re-copies source→mirror for every drifted pair. That is exactly what the
# build system's own mirror step does; applying it out of band heals trees that went
# stale under the old rule without a full rebuild of each family. The refreshed mtime
# makes every consuming TU recompile on the next build (the OBJECT_DEPENDS edge from
# issues 0090/0114), so the binaries catch up too.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

FIX=0
if [ "${1:-}" = "--fix" ]; then
    FIX=1
fi

# Mirror dirs live at a fixed depth under each cmake build dir. Globs (not `find`)
# keep this at ~30 ms warm — a full walk of the artifact tree costs minutes.
mirror_dirs=(
    examples/*/*/*/build*/nano_ros/packages/core/nros-*/include/nros/
    examples/workspaces/*/build*/nano_ros/packages/core/nros-*/include/nros/
    build/*/nano_ros/packages/core/nros-*/include/nros/
)

scanned=0
compared=0
drift=0
fixed=0

for dir in "${mirror_dirs[@]}"; do
    [ -d "$dir" ] || continue
    scanned=$((scanned + 1))
    # <…>/nros-c/include/nros/ → <…>/nros-c/ (where build.rs's copy lands)
    src_dir="$(dirname "$(dirname "${dir%/}")")"
    for mirror in "${dir}"nros_config_generated.h "${dir}"nros_cpp_config_generated.h; do
        [ -f "$mirror" ] || continue
        src="${src_dir}/$(basename "$mirror")"
        # No source copy = this crate's cargo build never ran in this tree (e.g. a
        # C-only build dir that still carries the C++ subproject). Not drift.
        [ -f "$src" ] || continue
        compared=$((compared + 1))
        if ! cmp -s "$src" "$mirror"; then
            if [ "$FIX" -eq 1 ]; then
                cp "$src" "$mirror"
                fixed=$((fixed + 1))
                continue
            fi
            drift=$((drift + 1))
            echo "STALE MIRROR: $mirror"
            echo "         vs: $src"
            diff <(grep -E '^#define .*(_SIZE|_OPAQUE_U64S) ' "$src" || true) \
                 <(grep -E '^#define .*(_SIZE|_OPAQUE_U64S) ' "$mirror" || true) \
                 | sed 's/^/             /' || true
        fi
    done
done

if [ "$drift" -gt 0 ]; then
    cat <<EOF

$drift stale sizes-header mirror(s) — every consumer TU in those trees compiles
against the wrong storage sizes (silent \`_opaque\` overflow at runtime).

Fix: \`bash scripts/check-sizes-header-mirrors.sh --fix\` re-mirrors in place (and
forces the dependent TUs to recompile), or rebuild the family from a CLEAN build
dir. Then confirm the mirror command has a REAL input edge (not order-only):
  ninja -C <build-dir> -t query <…>/include/nros/nros_config_generated.h
An input list of only \`||\` entries is the issue-0268 defect.
EOF
    exit 1
fi

if [ "$FIX" -eq 1 ] && [ "$fixed" -gt 0 ]; then
    echo "re-mirrored $fixed stale pair(s) from their build-dir source"
fi
echo "sizes-header mirrors OK — $compared mirror/source pair(s) across $scanned build tree(s)"
if [ "$scanned" -eq 0 ]; then
    echo "  (no local build trees — this gate proved NOTHING here; it needs built fixtures)"
fi
