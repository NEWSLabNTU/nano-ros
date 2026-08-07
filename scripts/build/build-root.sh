#!/usr/bin/env bash
# RFC-0070 R1 — the ONE build-cache root.
#
# phase-334 W2.b step 1: the DERIVATION, whose output is today's paths. No
# directory moves here. Callers migrate to these helpers first; only once a
# family reads its path from this file does the path itself change (step 3).
# That ordering is not stylistic — there are 236 hardcoded cache-path literals
# across 17 files, and moving paths before the readers agree is how the build,
# the staleness gate and the test resolver end up looking in three places.
#
# phase-334 W2.b step 2 (CALLERS) migrated three families, each in one commit
# with its build + staleness probe + test resolver together:
#
#   compile-check   scripts/build/compile-check-fixtures.sh (build)
#   cmake-fixtures  scripts/test/compile-check-stale.sh     (probe)
#                   nros_tests::fixtures::require_compile_check{,_bin} /
#                   require_cmake_fixture                   (resolver)
#   idf-fixtures    scripts/build/idf-fixtures.sh  + require_idf_fixture
#   west-fixtures   scripts/build/west-fixtures.sh + require_west_fixture
#   fixtures-cargo  fixtures-target-dir.sh (step 1) + fixture_shared_target_dir
#
# The Rust half of a family cannot source this file, so `nros_tests::build_root`
# / `nros_tests::build_dir` mirror these two functions — ONE mirror, pinned to
# the same literals from both sides by `build_root_derivation.sh` and the
# `nros_tests` unit tests. Do not add a third spelling.
#
# `NROS_BUILD_ROOT` generalizes what `NROS_ZEPHYR_BUILD_ROOT` already does for
# one family: let the whole cache tree move to a faster or larger volume. The
# default is `<repo>/build`, so an unset environment behaves exactly as before.

# This file's own repo root, resolved at SOURCE time. `${BASH_SOURCE[0]}` is
# whatever path the caller sourced, often relative — and callers legitimately
# `cd` afterwards (fixtures-build.sh cds into each example), at which point
# resolving it inside the function yields `/build`. Capture it once, here.
#
# EXPORTED because `fixtures-build.sh` ships `nros_build_root` to its make leaves
# with `export -f`: a leaf gets the function but never sources this file, so the
# value has to travel in the environment with it.
export _NROS_BUILD_ROOT_REPO
_NROS_BUILD_ROOT_REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# nros_build_root
# The root every build cache lives under. Absolute.
#
# Rooted at NROS_REPO_ROOT when set (the fixture builders cd into example dirs
# before invoking cargo, so a $PWD-relative root would land inside the example —
# the same trap `nros_scoped_target_dir` documents for issue 0400).
#
# Last resort is THIS FILE's own repo root (`_NROS_BUILD_ROOT_REPO`), not `$PWD`.
# Step 2 migrates callers that already computed `repo_root` from their own
# `BASH_SOURCE`; falling back to `$PWD` would have made the emitted path depend
# on the caller's cwd, which is the very failure the paragraph above describes.
# `scripts/build/` is two levels below the repo root by construction, so this
# fallback is always the checkout the script was read from — including in a git
# worktree, where an inherited `NROS_REPO_DIR` may still name the main checkout.
nros_build_root() {
    if [ -n "${NROS_BUILD_ROOT:-}" ]; then
        printf '%s' "${NROS_BUILD_ROOT%/}"
        return 0
    fi
    printf '%s/build' "${NROS_REPO_ROOT:-${NROS_REPO_DIR:-$_NROS_BUILD_ROOT_REPO}}"
}

# nros_build_dir <kind> [<coordinate>...]
# RFC-0070 R2 — `<root>/<kind>/<coordinate>`, the ONE naming shape.
#
#   nros_build_dir cargo linux-zenoh   -> <root>/cargo/linux-zenoh
#   nros_build_dir tools zenohd        -> <root>/tools/zenohd
#
# The coordinate comes from the fixture-manifest vocabulary (platform, lang,
# rmw, feature-sig). A new ad-hoc suffix is a bug, not a naming choice — the
# suffix zoo this replaces grew precisely by inventing one per need.
nros_build_dir() {
    local kind="$1"
    shift || true
    [ -n "$kind" ] || {
        echo "nros_build_dir: kind is required" >&2
        return 2
    }
    local out
    out="$(nros_build_root)/$kind"
    local part
    for part in "$@"; do
        [ -n "$part" ] || continue
        out="$out/$part"
    done
    printf '%s' "$out"
}
