#!/usr/bin/env bash
# phase-314 W5 — the feature-set SSoT gate.
#
# Every failure this phase fixed was SILENT: a consumer hook that applied on one
# path and not another (issue 0304), an edition hardcoded so a non-humble build
# compiled as humble and failed on the wire, capabilities that reached a pure
# C/C++ workspace but not a mixed one. None of them produced an error at the
# time; they produced a link failure or a wire mismatch a build away from the
# cause.
#
# So the gate is not "does it build" — it is "is there still exactly one source
# of truth". Drift here is invisible by construction, which is why it needs a
# check rather than a convention.
set -uo pipefail
cd "$(dirname "$0")/.."

fail=0
note() { printf '  %s\n' "$1"; }

# 1. The ROS edition is chosen in exactly one place. A literal anywhere else is
#    the defect that made a non-humble build a wire mismatch rather than an
#    error (RFC-0056: the edition drives the keyexpr format that must match the
#    codegen-baked type_hash).
# Comment lines are excluded: the conversion sites legitimately REFERENCE the
# old hardcode when explaining why they no longer do it.
edition_hits=$(git grep -n 'ros-humble' -- 'cmake/*.cmake' 'packages/core/nros-c/CMakeLists.txt' \
    'packages/core/nros-cpp/CMakeLists.txt' 2>/dev/null \
    | grep -v 'cmake/NanoRosFeatureSet.cmake' \
    | grep -v ':[0-9]*:[[:space:]]*#' | cut -d: -f1 | sort -u || true)
if [ -n "$edition_hits" ]; then
    echo "FAIL: a ROS edition literal outside cmake/NanoRosFeatureSet.cmake:"
    echo "$edition_hits" | while read -r f; do note "$f"; done
    note "The edition must come from NANO_ROS_ROS_EDITION via nros_feature_set()."
    fail=1
fi

# 2. Exactly one platform→cargo-feature mapping.
#
#    Matched on the ASSIGNMENT, not on a feature name: `platform-freertos` also
#    appears as a directory (`packages/core/nros-platform-freertos`) in the
#    board/platform wiring files, and `NROS_PLATFORM_LINK_FEATURES` is a
#    transport axis (tcp/udp) with nothing to do with cargo. An earlier version
#    of this check matched the bare substring and reported six files that were
#    not duplication at all — which is how a gate loses its credibility.
plat_hits=$(git grep -n 'set(_platform_features\|set(_rmw_features' -- 'cmake/*.cmake' \
    'cmake/*/*.cmake' 'packages/core/nros-c/CMakeLists.txt' \
    'packages/core/nros-cpp/CMakeLists.txt' 2>/dev/null \
    | grep -v ':[0-9]*:[[:space:]]*#' | cut -d: -f1 | sort -u || true)
if [ -n "$plat_hits" ]; then
    echo "FAIL: a platform/rmw→cargo-feature assembly outside cmake/NanoRosFeatureSet.cmake:"
    echo "$plat_hits" | while read -r f; do note "$f"; done
    note "Call nros_feature_set() instead of re-deriving the list."
    fail=1
fi

# 3. No Rust NODE package names a ROS edition. Cargo features are additive and
#    ros-{humble,iron,jazzy} are compile_error!-exclusive, so a leaf naming one
#    is not an overridable default — it makes every other edition unbuildable in
#    that workspace. Entries own the choice.
leaf_hits=""
while read -r f; do
    [ -n "$f" ] || continue
    # An IMAGE owns the edition. That is an entry package, and also a
    # self-contained standalone example — which has no `[nros.entry]` table but
    # IS the binary (src/main.rs / [[bin]]). Only a lib-only node package,
    # which is linked into someone else\'s image, must stay silent.
    grep -q '\[package.metadata.nros.entry\]' "$f" && continue
    grep -q '^\[\[bin\]\]' "$f" && continue
    [ -f "$(dirname "$f")/src/main.rs" ] && continue
    leaf_hits="${leaf_hits}${f}"$'\n'
done < <(git grep -ln '"ros-\(humble\|iron\|jazzy\)"' -- 'examples/**/Cargo.toml' 2>/dev/null || true)
if [ -n "${leaf_hits//[$'\n']/}" ]; then
    echo "FAIL: node package(s) name a ROS edition — the edition is image-level:"
    echo "$leaf_hits" | while read -r f; do [ -n "$f" ] && note "$f"; done
    note "Drop the ros-* feature; the entry supplies it and unification carries it."
    fail=1
fi

if [ "$fail" -eq 0 ]; then
    echo "feature-set SSoT OK — one edition source, one platform mapping, no node-level editions."
fi
exit "$fail"
