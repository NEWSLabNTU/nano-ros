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
#   tier1/tier2/   the computed `CiLane` covers — i.e. which fixtures the lane
#   tier2-nightly  keeps FRESH. Correct for a BUILD only when the run is scoped
#                  to match, which `nros_lane_build_lane` is the arbiter of:
#                  tier 1 maps to `native` (its run is filtered by NAME, which
#                  selects a broader set than its cover), while tier 2 and
#                  nightly map to THEMSELVES since phase-340 W3 — their run is
#                  narrowed at fixture-resolution time to the same coordinates.
#                  Passing one of these to `build-test-fixtures` builds the
#                  cover; passing it to `_require-fixtures` requires whatever
#                  that lane's RUN resolves. The two are different questions
#                  (issue 0482 — they used to be the same lookup, so a
#                  `lane=tier2` build satisfied a `ci-matrix` that then failed on
#                  34 unbuilt coordinates); they now have the same ANSWER for
#                  tier 2, which is the point of W3 rather than a reason to
#                  collapse them again.
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

# The fixture BUILD lane a RUN of `<lane>` needs — issue 0482.
#
# `nros_lane_coords_file` answers "which fixtures must be FRESH": the lane's own
# cell selection, and legitimately narrow — that narrowing IS the tier-2 saving.
# This answers the other question, "which fixtures must EXIST", which is a
# property of the RUN and not of the cell cover:
#
#   just ci             filters the run to host binaries -> a `native` build covers it
#   just ci-matrix      runs the WHOLE suite             -> only an `all` build covers it
#
# Both were being answered from the one lane name. `_require-fixtures` was
# handed `NROS_FIXTURE_LANE=tier2`, asked "does the stamp cover tier 2?", and a
# `just build-test-fixtures lane=tier2` is a perfectly good answer to THAT
# question — so the preflight passed and the run then discovered 34 of 47
# coordinates missing, one test at a time (~231 failures after a build that
# reported success). The justfile already said tier 2's build had to be `all`;
# a comment is not a gate.
#
# The DECLARATION of which lanes narrow their run lives in `CiLane::run_scope`,
# next to the cell selection it has to stay consistent with, and is emitted by
# `lane-coords <lane> --build-lane`. This is the runtime implementation of the
# same mapping — deliberately pure bash and NOT a `cargo run`, because this runs
# inside a preflight whose whole job is to fail in seconds, and because
# `packages/testing/nros-tests/tests/lane_build_covers_run.rs` has to be able to
# exercise it without compiling anything (CLAUDE.md: no compilation inside
# tests). That test is what binds the two: it runs this function for every lane
# and asserts the answer equals `CiLane::build_lane()`, so the second spelling
# cannot drift the way `matches_filters` drifted from `matrix_fixture_coverage`.
nros_lane_build_lane() {
    local lane="$1"
    nros_lane_validate "$lane" || return 2
    case "$lane" in
        # Module-level lanes are their own build lane.
        all | native) echo "$lane" ;;
        # `just ci` narrows its run to host binaries by NAME, which a `native`
        # build covers exactly. Deliberately NOT `tier1`: `NROS_TEST_SCOPE=
        # native` selects every host test BINARY, a broader set than
        # `coords(Tier1)` (10 of 47), so a coordinate-scoped build would leave
        # the rest absent.
        tier1) echo native ;;
        # phase-340 W3 — these narrow their run at fixture-RESOLUTION time
        # (`RunScope::LaneCoords`; `NROS_TEST_COORDS` -> `fixtures::lane`), to
        # exactly the coordinates `--coords-from` told the build to produce. So
        # a lane's own build now covers its own run, and this maps each to
        # itself. Before phase-340 W3 both mapped to `all`, because the run was
        # unnarrowed and every coordinate had to exist — which is what made the
        # middle rung cost the top rung's build (issue 0482).
        tier2 | tier2-nightly) echo "$lane" ;;
        *)
            # Unreachable while `_NROS_LANES` and this case agree; a new lane
            # that lands in neither arm must fail LOUDLY, because the silent
            # readings ("" or `all`) are respectively a hang and a laundered
            # requirement.
            echo "fixture-lane: no build lane declared for '$lane' — add it to" >&2
            echo "              nros_lane_build_lane AND CiLane::run_scope" >&2
            return 2
            ;;
    esac
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

# Does the RUN IN PROGRESS need fixtures for `<platform-token>`?
#
# Issue #405 / phase-337 W3.f. `lane-coords --modules` schedules `just <module>
# build-fixtures`, and a module may own SEVERAL fixture platforms (`nuttx` owns
# `nuttx` and `nuttx-riscv`; `esp32` owns `esp32` and `qemu-esp32-baremetal`).
# Such a module has to build the ones its lane asked for and skip the ones it
# did not — a riscv NuttX build costs an arm↔rv-virt kernel reconfigure, so
# "always build both" is not free, and "never build the second" is issue #405.
#
# `NROS_FIXTURE_COORDS` is exported by `build-test-fixtures-leaves` for the tier
# lanes and UNSET for `lane=all` / a bare `just <module> build-fixtures`. Unset
# therefore means "no narrowing" — i.e. yes, build it — which is the same
# reading `fixtures-build.sh` gives the variable.
#
# One helper rather than a `grep` open-coded per module: the second spelling is
# how this class of bug comes back (CLAUDE.md, fix-the-class).
nros_lane_wants_platform() {
    local platform="${1:?usage: nros_lane_wants_platform <fixtures.toml platform token>}"
    [ -n "${NROS_FIXTURE_COORDS:-}" ] || return 0
    [ -s "${NROS_FIXTURE_COORDS}" ] || {
        echo "fixture-lane: NROS_FIXTURE_COORDS=${NROS_FIXTURE_COORDS} is empty or absent" >&2
        return 2
    }
    grep -q "^${platform}," "$NROS_FIXTURE_COORDS"
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
# build that did run does not COVER the RUN the named lane is about to perform —
# naming the missing coordinates instead of telling a tier-1 user to build 157
# cross fixtures they will never run.
#
# Issue 0482 — note the argument is the lane the RUN is scoped by, and the
# requirement is `nros_lane_build_lane "$lane"`, NOT the lane itself. Those are
# the same thing only for lanes whose run is narrowed to match their cover
# (`native`), and asking the wrong one is the whole defect: a `lane=tier2` build
# satisfied a `ci-matrix` that then executed every test binary in the tree.
nros_fixtures_stamp_require() {
    local lane="${1:-all}"
    nros_lane_validate "$lane" || return 2

    # What this RUN needs to EXIST. `$lane` is kept for the diagnostics, which
    # have to be able to say "you asked for lane X, whose run needs build Y" —
    # collapsing them would reproduce the confusion in the error message.
    local want
    want="$(nros_lane_build_lane "$lane")" || return 1

    if [ ! -f "$NROS_FIXTURE_STAMP" ]; then
        echo "ERROR: test fixtures not built — 'just test-all' would mass-fail with 'Binary not found'." >&2
        echo "" >&2
        if [ "$want" = "all" ]; then
            echo "  Run:  just build-test-fixtures" >&2
        else
            echo "  Run:  just build-test-fixtures lane=$want" >&2
        fi
        if [ "$want" != "$lane" ]; then
            echo "" >&2
            echo "  (lane '$lane' does not narrow its test RUN, so every fixture must exist" >&2
            echo "   — 'lane=$lane' would build only the coordinates it keeps FRESH.)" >&2
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
        if [ "$lane" != "all" ]; then
            echo "" >&2
            echo "       '$lane' narrows which fixtures must be FRESH (its cell cover)," >&2
            echo "       not which must EXIST: the recipe runs the whole test suite, so" >&2
            echo "       every coordinate's fixtures are resolved. Issue 0482 — a" >&2
            echo "       'lane=$lane' build used to satisfy this check and the run then" >&2
            echo "       failed STALE on ~34 coordinates it had never built." >&2
        fi
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
    #
    # `want` is what a COVERING BUILD would have been given, so it is either
    # module-level (`native`; `all` returned above) or — once some lane narrows
    # its run and `run_scope` says so — coordinate-scoped. Both shapes are
    # handled: the coordinate diff below needs a coordinate file, and a
    # module-level requirement has none.
    local want_file missing
    want_file="$(nros_lane_coords_file "$want")" || return 1

    if [ -z "$want_file" ]; then
        # want=native — a MODULE-level requirement: every row of the host
        # module, which is a strict SUPERSET of any coordinate cover of the host
        # (tier 1 selects 10 of the 47 coordinates). A coordinate-scoped build
        # therefore cannot satisfy it and there is no diff to show. Without this
        # arm the `comm` below would be handed an empty filename and `sort` would
        # read stdin — the preflight would HANG rather than fail.
        echo "ERROR: fixtures were built for lane '$have', which is coordinate-scoped and" >&2
        echo "       cannot cover the module-level lane '$want' (a coordinate cover is a" >&2
        echo "       strict subset of a module's rows, so the run would fail 'Binary not" >&2
        echo "       found' on the remainder)." >&2
        echo "" >&2
        echo "  Run:  just build-test-fixtures lane=$want" >&2
        echo "" >&2
        echo "  (bypass with  NROS_SKIP_FIXTURE_CHECK=1 )" >&2
        return 1
    fi

    if [ "$have" = "native" ]; then
        # NOTE the two spellings here, which are NOT a typo (phase-337 W8.c).
        # `native` is the LANE name — `lane=native`, `_NROS_LANES`, `just native
        # …` — and it deliberately keeps that name. `linux` is the fixture TOKEN
        # the coordinates are built from (`lane-coords` → `fixture_tokens()`),
        # and W8.c moved it to match `PlatformId::Linux` and
        # `nros-board-linux`. They are different vocabularies that used to share
        # a spelling, so this line is where the seam is visible.
        #
        # A `native`-lane build covers every row of the host module, so it covers
        # any lane whose coordinates are ALL on the host — which is true of tier 1
        # by construction. Checked rather than assumed: if the tier-1 selection
        # ever grows a cross-platform cell this reports it instead of waving a
        # run through on fixtures that were never built.
        # An empty `want_file` (a module-level requirement) returned above.
        missing="$(grep -v '^linux,' "$want_file" || true)"
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
