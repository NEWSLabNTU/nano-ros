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
# `NROS_BUILD_ROOT` generalizes what `NROS_ZEPHYR_BUILD_ROOT` already does for
# one family: let the whole cache tree move to a faster or larger volume. The
# default is `<repo>/build`, so an unset environment behaves exactly as before.

# nros_build_root
# The root every build cache lives under. Absolute.
#
# Rooted at NROS_REPO_ROOT when set (the fixture builders cd into example dirs
# before invoking cargo, so a $PWD-relative root would land inside the example —
# the same trap `nros_scoped_target_dir` documents for issue 0400).
nros_build_root() {
    if [ -n "${NROS_BUILD_ROOT:-}" ]; then
        printf '%s' "${NROS_BUILD_ROOT%/}"
        return 0
    fi
    printf '%s/build' "${NROS_REPO_ROOT:-${NROS_REPO_DIR:-$PWD}}"
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
