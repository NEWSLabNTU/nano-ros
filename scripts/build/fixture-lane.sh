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
# issue 0523 — the newest prebuilt `lane-coords`, or empty when none is usable.
#
# Empty means "compile it": either nothing is built, or what is built predates a
# source of `nros-tests` and would answer for a tree that no longer exists.
# NEWEST rather than a preferred profile — preferring `nros-fast-release` picked
# an eleven-day-old artifact on this host while `debug` was current.
_nros_lane_coords_bin() {
    local best="" best_t=0 p b t
    for p in nros-fast-release debug release; do
        b="target/$p/lane-coords"
        [ -x "$b" ] || continue
        t="$(stat -c %Y "$b" 2>/dev/null || echo 0)"
        if [ "$t" -gt "$best_t" ]; then
            best_t="$t"
            best="$b"
        fi
    done
    [ -n "$best" ] || return 0
    # Any source newer than the binary ⇒ it is stale; say nothing and let the
    # caller rebuild. `-quit` stops at the first hit, so this stays cheap.
    if [ -n "$(find packages/testing/nros-tests/src -type f -newer "$best" -print -quit 2>/dev/null)" ]; then
        return 0
    fi
    printf '%s' "$best"
}

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
    # issue 0494 — write to a TEMP file and rename. `> "$out"` truncates the
    # file the instant the redirection is set up, and `cargo run` then COMPILES
    # for seconds-to-minutes before writing a byte. Every reader in that window
    # sees an empty file, and because the narrowing fails CLOSED on empty
    # coordinates (correctly), one truncated file fails every narrowed test at
    # once. Measured on one tree at one commit: 223 real failures, 203 of them
    # `no coordinates`; the immediate re-run with the file already populated
    # gave 20. Same tree — the gate was non-deterministic.
    #
    # rename(2) within one directory is atomic, so a concurrent reader now sees
    # either the previous content or the new content, never a truncated one.
    local tmp="${out}.tmp.$$"
    # issue 0523 part B — RUN the selector; compile it only if nobody already
    # has. `cargo run` here is correct in a BUILD recipe and wrong on the
    # preflight/test path: `lane-coords` is a bin of `nros-tests`, so any edit to
    # that crate invalidates it and the next call recompiles the whole package
    # before writing a byte (the comment above says "seconds-to-minutes"). Three
    # `lane_build_covers_run` cases reach this through
    # `nros_fixtures_stamp_require` and blew nextest's 60 s per-test timeout —
    # and only inside a SWEEP, where concurrent cargos hold the package-cache
    # and build-directory locks. Solo the same call is instant, which is exactly
    # how it read as green after a partial fix.
    #
    # A prebuilt binary is used ONLY when no source of its crate is newer than
    # it. A stale selector answers for a tree that no longer exists: one dated
    # eleven days back reported 12 tier-2 coordinates where the sources said 13,
    # and the mismatch surfaced as a coordinate-drift error blaming the guard.
    local bin
    bin="$(_nros_lane_coords_bin)"
    if [ -n "$bin" ]; then
        if ! "$bin" "$lane" > "$tmp"; then
            echo "fixture-lane: $bin failed for '$lane'" >&2
            rm -f "$tmp"
            return 1
        fi
    elif ! cargo run -q -p nros-tests --bin lane-coords -- "$lane" > "$tmp"; then
        echo "fixture-lane: lane-coords failed for '$lane'" >&2
        rm -f "$tmp"
        return 1
    fi
    mv -f "$tmp" "$out"
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
        # phase-395 W19 — tier 1 maps to ITSELF now, like tier 2.
        #
        # It used to map to the `native` MODULE, and the reason was sound while
        # it lasted: `just ci` narrowed its run by NAME
        # (`NROS_TEST_SCOPE=native`), which selects every host test BINARY — a
        # broader set than `coords(Tier1)` — so a coordinate-scoped build would
        # have left the rest absent.
        #
        # That premise is gone. `just ci` now narrows by COORDINATE, so the run
        # and the build ask the same question again and the answer is the lane
        # itself. The old mapping had also become actively wrong: the `native`
        # module does not build zephyr native_sim or threadx-linux fixtures, and
        # tier 1 covers both — `lane-coords tier1 --modules` returns
        # `native threadx_linux zephyr`.
        tier1) echo "$lane" ;;
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
#   built_at=2026-08-01T09:12:33Z      (upper bound — stamped on success)
#   started_at=2026-08-01T08:41:07Z    (lower bound — issue 0499)
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
    # issue 0499 — record when the build STARTED, beside the (now absent) stamp.
    #
    # `built_at` is stamped on SUCCESS, so it is an UPPER bound on the build's
    # own output: every artifact the run produced is older than it. A consumer
    # asking "which artifacts belong to this build?" needs a LOWER bound, and
    # filtering on `built_at` marks the whole current build as history —
    # measured, and it made check-artifact-identity-budget skip permanently.
    #
    # Written HERE because this function already owns "a build is starting",
    # and it survives the shell boundary between the clearing recipe and the
    # writing one, which a variable would not.
    printf '%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "${NROS_FIXTURE_STAMP}.started" 2>/dev/null || true
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
        # issue 0499 — the LOWER bound. Absent on a stamp written without a
        # preceding `stamp_clear` (a legacy or hand-made stamp), and a consumer
        # must treat its absence as "cannot filter", never as "nothing is new".
        if [ -r "${NROS_FIXTURE_STAMP}.started" ]; then
            echo "started_at=$(cat "${NROS_FIXTURE_STAMP}.started")"
        fi
        echo "lane=$lane"
        if [ -n "$coords" ]; then sed 's/^/coord=/' "$coords"; fi
    } > "$NROS_FIXTURE_STAMP"
    rm -f "${NROS_FIXTURE_STAMP}.started"
    echo "fixture stamp: $NROS_FIXTURE_STAMP (lane=$lane)"
}

# Echo the stamp's lane, or `all` for a legacy timestamp-only stamp.
nros_fixtures_stamp_lane() {
    [ -f "$NROS_FIXTURE_STAMP" ] || return 1
    local lane
    lane="$(sed -n 's/^lane=//p' "$NROS_FIXTURE_STAMP" | head -1)"
    echo "${lane:-all}"
}

# nros_lane_run_is_narrowed <build-lane>
#
# For a MODULE-level build lane (`all`, `native`) there is nothing to check —
# those cover any run. For a COORDINATE-scoped one, the run must be narrowed to
# the same coordinates or the lane is back to issue 0482: a build that produced
# 152 of 333 rows, accepted for a run that resolves all 333.
#
# The narrowing is `NROS_TEST_COORDS`, read by `nros_tests::fixtures::lane`. It
# must be SET, non-empty, and hold exactly the lane's coordinates — an
# arbitrary file would let a caller widen the build's acceptance while narrowing
# the run somewhere else entirely, which is the two-spellings-of-one-fact defect
# (issue 0443) with a new pair of names.
nros_lane_run_is_narrowed() {
    local want="$1"
    case "$want" in
        all | native) return 0 ;;
    esac

    local want_file
    want_file="$(nros_lane_coords_file "$want")" || return 1
    # `all`/`native` returned above, so a coordinate lane always has a file;
    # an empty name here would feed `comm` an empty filename and hang.
    [ -n "$want_file" ] || {
        echo "fixture-lane: no coordinate file for '$want' — refusing rather than" >&2
        echo "              comparing against nothing" >&2
        return 1
    }

    if [ -z "${NROS_TEST_COORDS:-}" ] || [ ! -s "${NROS_TEST_COORDS}" ]; then
        echo "ERROR: lane '$want' is coordinate-scoped, but NROS_TEST_COORDS is unset" >&2
        echo "       or empty — so the RUN would resolve every coordinate while this" >&2
        echo "       preflight accepts a build of only ${want}'s." >&2
        echo "" >&2
        echo "       That is issue 0482: preflight green, then ~231 failures on" >&2
        echo "       fixtures the lane never built. The tier recipes export it;" >&2
        echo "       a hand-run 'NROS_FIXTURE_LANE=$want just test-all' must too." >&2
        echo "" >&2
        echo "  Run:  just ci-matrix        (or ci-matrix-nightly)" >&2
        echo "  Or:   NROS_TEST_COORDS=\"\$(bash -c 'source scripts/build/fixture-lane.sh; nros_lane_coords_file $want')\"" >&2
        echo "" >&2
        echo "  (bypass with  NROS_SKIP_FIXTURE_CHECK=1 )" >&2
        return 1
    fi

    local differing
    differing="$(comm -3 \
        <(grep -v '^[[:space:]]*\(#.*\)\?$' "$want_file" | sort -u) \
        <(grep -v '^[[:space:]]*\(#.*\)\?$' "$NROS_TEST_COORDS" | sort -u))"
    if [ -n "$differing" ]; then
        echo "ERROR: NROS_TEST_COORDS does not hold lane '$want''s coordinates." >&2
        echo "       The RUN would then be narrowed to a different set from the one" >&2
        echo "       this preflight is accepting a build for — two spellings of one" >&2
        echo "       fact (issue 0443), which is how they drift." >&2
        echo "       Differing coordinate(s):" >&2
        printf '         %s\n' $differing >&2
        echo "" >&2
        echo "  file: $NROS_TEST_COORDS" >&2
        echo "  lane: $want_file" >&2
        return 1
    fi
    return 0
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
    # A build of everything covers every lane; nothing else to check. Note this
    # returns BEFORE the narrowing check below on purpose: with every fixture
    # present, an unnarrowed run is fine, and `NROS_FIXTURE_LANE=tier2` on top
    # of a full build is a legitimate combination (scope the FRESHNESS gate,
    # keep the run wide).
    if [ "$have" = "all" ]; then
        return 0
    fi

    # phase-340 W3 — from here on the caller is relying on a build NARROWER than
    # everything, which is only a correct answer while the RUN is narrowed too.
    # What narrows it is `NROS_TEST_COORDS` reaching the test processes: the
    # tier recipes export it and `recipes_run_the_scope_their_lane_declares`
    # gates that, but a HAND-RUN `NROS_FIXTURE_LANE=tier2 just test-all`
    # bypasses both and reproduces issue 0482 exactly — narrow stamp accepted,
    # whole suite resolved, ~231 failures. Checked HERE, where the acceptance is
    # actually granted, not only where it is configured; gating one of the two
    # would be a gate narrower than the rule it enforces (issue 0196).
    nros_lane_run_is_narrowed "$want" || return 1

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
