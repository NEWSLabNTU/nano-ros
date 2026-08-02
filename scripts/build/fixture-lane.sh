#!/usr/bin/env bash
# Fixture LANE resolution + the build stamp — issue 0393.
#
# Sourced, not executed. This is the one place a CI lane (RFC-0061 / phase-318)
# turns into the three things every consumer needs:
#
#   nros_lane_coords_file <lane>   the `platform,lang,rmw` set the lane needs
#   nros_lane_modules <lane>       the `just <module>` set that builds them
#   nros_fixtures_stamp_*          what a build actually produced, and whether
#                                  that covers the lane a run is about to use
#
# # Why this file exists
#
# `ci_lane.rs` already promised that "a lane's build, its staleness gate and its
# test run cannot disagree about what the lane covers" — but only the gate and
# the run read the selection. The build fanned out over every platform
# unconditionally, so tier 1 had to build all 337 manifest rows to prove the 180
# native ones it runs (issue 0393).
#
# The stamp had the matching hole: three separate copies of
# `date -u > target/nextest/.fixtures-built` (justfile build-all, justfile
# build-test-fixtures, build-all.mk) wrote a bare timestamp, so
# `_require-fixtures` could ask "did a build finish?" and never "does what was
# built cover what I am about to run?". Issue 0351 fixed the sticky half of that
# (a failed run used to leave the stamp behind); this is the scope half.
#
# Three spellings of a two-line stamp is exactly the pattern CLAUDE.md's
# fix-the-class rule names, so the stamp writer is a function here and the three
# sites call it.

# The stamp path. Overridable for tests; every caller reads it from here rather
# than re-spelling the literal.
NROS_FIXTURE_STAMP="${NROS_FIXTURE_STAMP:-target/nextest/.fixtures-built}"

# Every lane token this repo knows.
#
#   all            not a `CiLane` — the absence of one (build/require
#                  everything). The default, and the pre-0393 behavior.
#   native         every row of the `native` module, no coordinate filter. This
#                  is what tier 1 needs: `just ci` scopes its RUN with
#                  `NROS_TEST_SCOPE=native`, which selects every native test
#                  binary — a BROADER set than `coords(Tier1)` (10 of 47
#                  coordinates). Building only the tier-1 coordinates would
#                  leave the other native binaries absent and the run would
#                  mass-fail "Binary not found", which is the failure this
#                  preflight exists to prevent.
#   tier1/tier2/   the computed `CiLane` covers. Correct for a build ONLY when
#   tier2-nightly  the run is scoped to match; `ci-matrix` deliberately runs the
#                  full suite, so it still builds `all` (see the recipe).
_NROS_LANES="all native tier1 tier2 tier2-nightly"

# Normalize a lane argument as it arrives from `just`.
#
# `just <recipe> lane=tier2` passes the string `lane=tier2` VERBATIM as the
# positional value — just's `name=value` CLI form sets variables, not recipe
# parameters. The repo's existing recipes each strip their own prefix by hand
# (`doctor` and `setup` both do `${x#tier=}`); this is that idiom in one place
# so a third copy does not appear here. Bare `tier2` works too, since both
# spellings reach users through `just --list` and the docs.
nros_lane_arg() {
    local lane="${1:-all}"
    lane="${lane#lane=}"
    echo "${lane:-all}"
}

nros_lane_validate() {
    local lane="$1"
    local known
    for known in $_NROS_LANES; do
        if [ "$lane" = "$known" ]; then return 0; fi
    done
    echo "fixture-lane: unknown lane '$lane' — expected one of: $_NROS_LANES" >&2
    return 2
}

# Resolve a lane to a coordinate file and echo its path. Echoes NOTHING for
# `all` (no filtering) — callers test for an empty string.
#
# The coordinates come from `lane-coords`, the same binary `_lane-gate` uses, so
# the build cannot compute a different selection from the gate.
nros_lane_coords_file() {
    local lane="$1"
    nros_lane_validate "$lane" || return 2
    # `all` and `native` are MODULE-level selections, not coordinate ones: they
    # build every row of what they select, so there is nothing to filter by.
    # An explicit `if`, not `[ … ] || [ … ] && return` — under `set -e` a failing
    # final test in an AND-OR list aborts the caller.
    if [ "$lane" = "all" ] || [ "$lane" = "native" ]; then
        return 0
    fi
    local out="target/nextest/lane-coords-${lane}.txt"
    mkdir -p target/nextest
    if ! cargo run -q -p nros-tests --bin lane-coords -- "$lane" > "$out"; then
        echo "fixture-lane: lane-coords failed for '$lane'" >&2
        rm -f "$out"
        return 1
    fi
    # An empty selection would make the caller build (or require) nothing and
    # look instant rather than broken — the same refusal `_coords_for` makes in
    # fixtures-manifest.py.
    if [ ! -s "$out" ]; then
        echo "fixture-lane: lane '$lane' selected zero coordinates — refusing" >&2
        rm -f "$out"
        return 1
    fi
    echo "$out"
}

# Echo the `just <module>` names a lane needs, one per line (deduped by
# lane-coords: `nuttx` owns both arm and riscv, `zephyr` both native_sim and
# fvp). For `all`, echoes nothing — callers treat that as "no filter".
nros_lane_modules() {
    local lane="$1"
    nros_lane_validate "$lane" || return 2
    if [ "$lane" = "all" ]; then
        return 0
    fi
    if [ "$lane" = "native" ]; then
        echo native
        return 0
    fi
    cargo run -q -p nros-tests --bin lane-coords -- "$lane" --modules
}

# --- the stamp -------------------------------------------------------------
#
# Format (line-oriented, greppable):
#
#   # nano-ros fixture build stamp
#   built_at=2026-08-01T09:12:33Z
#   lane=tier1
#   coord=native,rust,zenoh
#   coord=native,c,zenoh
#
# `lane=all` carries no `coord=` lines and means "everything". A file with no
# `lane=` key at all is a PRE-0393 stamp (bare timestamp); it is read as
# `lane=all`, which is what it meant, so an existing tree does not start failing
# its preflight after this change.

nros_fixtures_stamp_clear() {
    rm -f "$NROS_FIXTURE_STAMP"
}

# nros_fixtures_stamp_write <lane>
#
# Called only after a build succeeds. Records the lane AND the coordinate set,
# so the reader can answer coverage rather than recency.
nros_fixtures_stamp_write() {
    local lane="${1:-all}"
    nros_lane_validate "$lane" || return 2
    local coords=""
    if [ "$lane" != "all" ]; then
        coords="$(nros_lane_coords_file "$lane")" || return 1
    fi
    mkdir -p "$(dirname "$NROS_FIXTURE_STAMP")"
    {
        echo "# nano-ros fixture build stamp (issue 0393) — lane + coordinates built"
        echo "built_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        echo "lane=$lane"
        if [ -n "$coords" ]; then sed 's/^/coord=/' "$coords"; fi
    } > "$NROS_FIXTURE_STAMP"
    echo "fixture stamp: $NROS_FIXTURE_STAMP (lane=$lane)"
}

# Echo the stamp's lane, or `all` for a legacy timestamp-only stamp.
nros_fixtures_stamp_lane() {
    [ -f "$NROS_FIXTURE_STAMP" ] || return 1
    local lane
    lane="$(sed -n 's/^lane=//p' "$NROS_FIXTURE_STAMP" | head -1)"
    echo "${lane:-all}"
}

# nros_fixtures_stamp_require <lane>
#
# The `_require-fixtures` preflight. Fails when no build has run, or when the
# build that did run does not COVER the lane about to be tested — naming the
# missing coordinates instead of telling a tier-1 user to build 157 cross
# fixtures they will never run.
nros_fixtures_stamp_require() {
    local want="${1:-all}"
    nros_lane_validate "$want" || return 2

    if [ ! -f "$NROS_FIXTURE_STAMP" ]; then
        echo "ERROR: test fixtures not built — 'just test-all' would mass-fail with 'Binary not found'." >&2
        echo "" >&2
        if [ "$want" = "all" ]; then
            echo "  Run:  just build-test-fixtures" >&2
        else
            echo "  Run:  just build-test-fixtures lane=$want" >&2
        fi
        echo "" >&2
        echo "  (built them another way? bypass with  NROS_SKIP_FIXTURE_CHECK=1 just test-all )" >&2
        return 1
    fi

    local have
    have="$(nros_fixtures_stamp_lane)"
    # A build of everything covers every lane; nothing else to check.
    if [ "$have" = "all" ]; then
        return 0
    fi

    # Requiring everything from a scoped build cannot be satisfied — say so in
    # those terms rather than listing 150 missing coordinates.
    if [ "$want" = "all" ]; then
        echo "ERROR: fixtures were built for lane '$have', but this run needs ALL of them." >&2
        echo "" >&2
        echo "  Run:  just build-test-fixtures" >&2
        echo "" >&2
        echo "  (bypass with  NROS_SKIP_FIXTURE_CHECK=1 )" >&2
        return 1
    fi

    if [ "$have" = "$want" ]; then
        return 0
    fi

    # Different lanes: the built set must be a superset of the wanted one.
    local want_file missing
    want_file="$(nros_lane_coords_file "$want")" || return 1

    if [ "$have" = "native" ]; then
        # A `native` build covers every row of the native module, so it covers
        # any lane whose coordinates are ALL on native — which is true of tier 1
        # by construction. Checked rather than assumed: if the tier-1 selection
        # ever grows a cross-platform cell this reports it instead of waving a
        # run through on fixtures that were never built.
        if [ -z "$want_file" ]; then
            # want=native handled by the equality above; nothing else is
            # module-level except `all`, handled earlier.
            missing=""
        else
            missing="$(grep -v '^native,' "$want_file" || true)"
        fi
    else
        missing="$(comm -23 \
            <(sort -u "$want_file") \
            <(sed -n 's/^coord=//p' "$NROS_FIXTURE_STAMP" | sort -u))"
    fi
    if [ -z "$missing" ]; then
        return 0
    fi

    echo "ERROR: fixtures were built for lane '$have', which does not cover '$want'." >&2
    echo "       Missing coordinate(s):" >&2
    printf '         %s\n' $missing >&2
    echo "" >&2
    echo "  Run:  just build-test-fixtures lane=$want" >&2
    echo "" >&2
    echo "  (bypass with  NROS_SKIP_FIXTURE_CHECK=1 )" >&2
    return 1
}
