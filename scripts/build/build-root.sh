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
#   cargo-fixtures  fixtures-target-dir.sh (step 1) + fixture_shared_target_dir
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

# RFC-0070 R5 — the build-cache KIND vocabulary, one definition each.
#
# A kind used to be a bare word at every call site, which is why renaming one
# was a search over an overloaded token rather than an edit: phase-350 W5 tried
# to rename `compile-check` and found the word also names the compile-check
# LANE, the `list-compile-checks` subcommand and three scripts, so a global
# replace rewrote 43 files and produced `list-compile-check-fixturess`. Reverted;
# these constants are the prerequisite that was missing.
#
# EXPORTED for the same reason `_NROS_BUILD_ROOT_REPO` is: `fixtures-build.sh`
# ships `nros_build_root` to its make leaves with `export -f`, and a leaf that
# gets the function but not the vocabulary would be back to bare words.
#
# The Rust half is `nros_tests::kind`; `build_root_derivation.sh` pins the two
# lists to each other and keeps LITERALS on its expected side deliberately — a
# check that a constant equals itself checks nothing.

# Fixture trees — `<family>-fixtures` per R5.
export NROS_KIND_CARGO_FIXTURES="cargo-fixtures"
export NROS_KIND_CMAKE_FIXTURES="cmake-fixtures"
export NROS_KIND_IDF_FIXTURES="idf-fixtures"
export NROS_KIND_WEST_FIXTURES="west-fixtures"

# The compile-check lane's trees. Renamed from `compile-check` to carry the
# `-fixtures` suffix R5 requires (2026-08-13) — two edits, this and the Rust
# twin. The three scripts sharing the `compile-check` prefix are NOT this kind
# and keep their names.
export NROS_KIND_COMPILE_CHECK="compile-check-fixtures"

# issue 0805 — the SHARED Corrosion cargo target dirs. A C/C++ example leaf is
# a standalone cmake project, so Corrosion roots its `--target-dir` at that
# leaf's `CMAKE_BINARY_DIR` and every leaf rebuilds the same staticlib (21
# fresh `libnros_c.a` in one threadx_riscv64 run, ~1.2 GB per leaf). The
# coordinate is the platform; cmake appends a hash of the leaf's resolved
# feature inputs, so two leaves share only when their cargo inputs are equal.
export NROS_KIND_CORROSION_CARGO="corrosion-cargo"

# Everything else — bare `<family>`, named for what it holds.
export NROS_KIND_BORROWED_E2E="borrowed-e2e"
export NROS_KIND_CARGO="cargo"
# issue 0624 — `check-examples` lint caches. ONE SUBDIRECTORY PER LEAF, not one
# shared dir: every example leaf is its own workspace root (RFC-0026 standalone
# copy-out projects, each with its own `[workspace]`), and issue 0616's rule is
# that a `--target-dir` serves exactly ONE root — two roots sharing one get two
# units of every shared crate, differing only in the `path` fingerprint field.
# The lane links nothing, so 0616's duplicate-lang-item failure cannot fire
# here, but the duplicate UNITS would still be built and cached, which is the
# cost this kind exists to avoid.
export NROS_KIND_EXAMPLE_LINT="example-lint"
# issue 0635 — the sibling for a walk that BUILDS example leaves to prove they
# still build (`build-example-extras`), rather than to produce a fixture. Its
# own kind, not the lint one beside it: those artifacts are a different
# compilation (different flags, and a link) and sharing one dir would give each
# lane the other's fingerprints to invalidate. Used only when the leaf's
# platform has no shared group to join — when it has one, the walk builds INTO
# that group and reuses the fixture build instead of making a second copy.
export NROS_KIND_EXAMPLE_BUILD="example-build"
# issue 0650 — where a lane records the steps it skipped, so its terminal recipe
# can refuse to claim it built fixtures. One file per lane; each step is its own
# `just` invocation, so no shell state survives between them.
export NROS_KIND_LANE_SKIPS="lane-skips"
# issue 0650 — the same for CHECKS, which keep their exit code and instead
# qualify the lane's closing sentence with what did not run.
export NROS_KIND_CHECK_SKIPS="check-skips"
export NROS_KIND_FIXTURE_MAKE_DRIVER="fixture-make-driver"
export NROS_KIND_LINK_DETERMINISM="link-determinism"
export NROS_KIND_PX4_MSGS_CODEGEN="px4-msgs-codegen"
export NROS_KIND_QEMU_ZENOH_PICO="qemu-zenoh-pico"
export NROS_KIND_ROS_EDITIONS="ros-editions"
export NROS_KIND_SIZES_PROBE="sizes-probe"
export NROS_KIND_STACK_ANALYSIS="stack-analysis"
export NROS_KIND_TOOLS="tools"
export NROS_KIND_XRCE_AGENT="xrce-agent"
export NROS_KIND_ZENOHD="zenohd"
export NROS_KIND_ZEPHYR_FIXTURE_BUILD="zephyr-fixture-build"
export NROS_KIND_ZEPHYR_FIXTURE_MAKE_DRIVER="zephyr-fixture-make-driver"
# issue 0535 — the last two fixtures whose path was a literal on BOTH sides.
export NROS_KIND_ESP32_QEMU="esp32-qemu"
# The RISC-V zenoh-pico archive for ESP32 — DISTINCT from `qemu-zenoh-pico`
# (the ARM one). Folding the two together in `just esp32 clean` would have
# deleted the wrong tree; caught before landing.
export NROS_KIND_ESP32_ZENOH_PICO="esp32-zenoh-pico"
export NROS_KIND_ZENOH_FIXTURE_POSIX="zenoh-fixture-posix"
# Issue 0787 — host build dirs for the two C backends that had no lane.
export NROS_KIND_XRCE_CHECK="xrce-check"
export NROS_KIND_UORB_CHECK="uorb-check"

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
