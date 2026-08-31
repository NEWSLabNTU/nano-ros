#!/usr/bin/env bash
# Issue 0466 — report EVERY unmet tier precondition at once, with its remedy.
#
# `just ci` is the instruction every task ends with, and reaching it has an
# ordered setup contract that is written nowhere as a sequence: the CLI must be
# rebuilt before fixtures (fixtures key on its stamp), fixtures must be built
# for the LANE you will test (#393), leaves need `nros sync` before cargo can
# even parse them (#463), the build stage needs the UNION of vendored sources
# (#390). Every step is documented somewhere; the ORDER is documented nowhere.
#
# The cost is not any single item — it is that the list is serial. One session
# hit EIGHT consecutive stops, each invisible until the previous cleared, ~40
# minutes apart. Four were source reds already on main that nobody could reach.
# A gauntlet that long is also a strong incentive to skip the tier, which is the
# exact dynamic RFC-0061's ladder exists to prevent.
#
# So this runs the probes that already exist, does not stop at the first, and
# prints one listing. One failure naming four things beats four failures naming
# one thing each.
#
# It deliberately owns NO check logic of its own: each item shells out to the
# recipe or script that is already the authority, so there is no second copy to
# drift (the CLAUDE.md "one shared helper, never a second spelling" rule). What
# this adds is only: keep going, and collect.
#
# Bypass wholesale with NROS_SKIP_TIER_PRECONDITIONS=1. Each underlying probe
# keeps its own bypass too (they are named in the remedies below).
set -uo pipefail

if [ "${NROS_SKIP_TIER_PRECONDITIONS:-0}" != "0" ]; then
    exit 0
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

failed=0
declare -a REPORT=()

# Run a probe, capture its output, record a remedy on failure. Never aborts.
probe() {
    local label="$1" remedy="$2"
    shift 2
    local out rc
    out="$("$@" 2>&1)"
    rc=$?
    if [ "$rc" -ne 0 ]; then
        failed=$((failed + 1))
        REPORT+=("$label|$remedy|$out")
    fi
}

# 1. Submodule pointers, BEFORE the CLI stamp — the remedy for this one
#    (`git submodule update`) rewrites source mtimes, so it re-arms both the CLI
#    stamp and every fixture. Clearing it after them would invalidate the work.
#    Issue 0550: a stale cyclonedds checkout took the fixture sweep down 17
#    leaves in, as a missing-file cmake error naming neither the submodule nor
#    the pull that moved the pointer.
probe "a submodule is not at the commit this superproject records" \
    "git submodule update <path>   (bypass: NROS_SKIP_SUBMODULE_DRIFT_CHECK=1)" \
    bash scripts/check-submodule-drift.sh

# 2. The CLI's source stamp. Before fixtures because everything downstream keys on it, and
#    because ANY tree refresh — pull, rebase, stash, rsync — re-arms it. That
#    re-arming is what makes the contract once-per-refresh rather than
#    once-per-clone, and it is the single most repeated stop.
probe "in-tree nros CLI is stale" \
    "just setup-cli    (any pull/rebase/stash re-arms this)" \
    bash scripts/check-cli-fresh.sh
cli_stale=$failed   # non-zero ⇒ everything that EXECS the CLI below would only echo this

# 3. Leaves must be sync'd or cargo cannot even PARSE them (#463) — the error
#    surfaces as "failed to parse manifest", four frames deep, never naming sync.
probe "leaf .cargo/config.toml includes an unwritten sync target" \
    "nros sync   in the named leaf   (bypass: NROS_SKIP_LEAF_INCLUDE_CHECK=1)" \
    python3 scripts/build/leaf-config-includes.py

# 4. The build stage needs the UNION of vendored sources, not the per-board
#    slice `nros setup <board>` provisions (#390).
# Skipped when the CLI is stale: this probe RUNS the CLI, so it would fail with
# the stale-stamp text again and the listing would report one root cause twice.
# A listing whose items are echoes of each other is the thing this gate exists
# to replace.
if [ "${NROS_SKIP_BUILD_SOURCE_CHECK:-0}" = "0" ] && [ "$cli_stale" -eq 0 ]; then
    # shellcheck source=scripts/build/cargo.sh
    source scripts/build/cargo.sh 2>/dev/null || true
    if cli="$(nros_cli_bin 2>/dev/null)" && [ -x "$cli" ]; then
        probe "vendored build sources missing" \
            "nros setup --source <name>   (bypass: NROS_SKIP_BUILD_SOURCE_CHECK=1)" \
            "$cli" setup --build-sources --check
    fi
    # A missing/unbuilt CLI is item 1's problem, not this one's — do not
    # double-report it.
fi

# 5. Fixtures, for the LANE this run will test. Coverage is per-lane (#393), so
#    "some fixtures exist" is not the question.
#
#    The remedy names the BUILD lane, not the run's lane (#482). They differ
#    whenever a lane does not narrow its test run: `ci-matrix` gates freshness
#    over the tier-2 cover but EXECUTES everything, so `lane=tier2` is the wrong
#    advice — it builds only the coordinates the gate checks and the run then
#    fails on the rest. `_require-fixtures` derives the same mapping, so this
#    line and the error it prints cannot disagree.
# shellcheck source=scripts/build/fixture-lane.sh
source scripts/build/fixture-lane.sh
_fixture_build_lane="$(nros_lane_build_lane "${NROS_FIXTURE_LANE:-all}" 2>/dev/null || echo "${NROS_FIXTURE_LANE:-all}")"

# 1b. An UNINITIALIZED submodule a lane's fixtures need. Distinct from 1: that
#     probe catches a checkout BEHIND the pin (issue 0550); this catches one that
#     was never checked out at all, which is legitimate for most of them and
#     fatal for a few.
#
#     Measured 2026-08-21. `px4_bridge_ffi` is a compile-check fixture, built
#     module-level, and its builder skips cleanly when PX4-Autopilot is absent:
#
#         px4: PX4-Autopilot submodule absent (third-party/px4/PX4-Autopilot)
#              — skipping (recorded in the summary)
#
#     The RUN does not skip. `px4_bridge_compile` treats the coordinate as
#     in-lane and fails "MISSING for an in-lane coordinate … a broken promise,
#     not an environment skip" — deliberately, per its own docstring, because
#     issue 0738 was about that path being silently unexercised. Both halves are
#     right; what was missing is that nothing said so BEFORE the sweep. Tier 2
#     surfaced it ~90 minutes in; tier 1 never does, because its scope filter
#     excludes the test.
#
#     Named rather than derived: there is no lane -> submodule mapping in the
#     tree, and inventing one would flag every optional SDK. One known
#     prerequisite for one lane is the honest size of this fact.
case "${NROS_FIXTURE_LANE:-all}" in
    tier2 | tier2-nightly | all)
        probe "the PX4-Autopilot submodule is not checked out (tier 2+ runs \`px4_bridge_compile\`)" \
            "git submodule update --init --depth 1 third-party/px4/PX4-Autopilot   (~406 MB)" \
            test -f third-party/px4/PX4-Autopilot/msg/DebugKeyValue.msg
        ;;
esac
probe "test fixtures not ready for this lane (missing, or stale under the stamp)" \
    "just build-test-fixtures lane=${_fixture_build_lane}   (bypass: NROS_SKIP_FIXTURE_CHECK=1)" \
    just _require-fixtures-ready

# 5b. `_require-fixtures-ready` is ONE gate over two questions (issue 0681
#     direction 2): the STAMP ("was a build run covering this lane?") and
#     per-fixture FRESHNESS ("is each `.inputsig` newer than its inputs?").
#
#     They were separate, and this batch probed only the first — so a stamp that
#     covered the lane while fixtures had gone stale underneath it reported OK
#     here and killed `just ci` minutes later, after `check`, `check-fast` and
#     `rust-rtos-link-check`. Probing both fixed the symptom; collapsing them
#     removes the seam, which is what produced 0443 and 0681 in the first place.
#     One remedy answers both, so one line is also the honest report.
#
#     CAVEAT, because it breaks a property probes 1-4 hold: this one is NOT
#     buildless. The freshness half self-heals the C/C++ cmake cells it finds
#     stale ("54 cell(s) … have now been rebuilt"), so this batch can do work
#     rather than only report. Same work `just ci` would do minutes later, moved
#     earlier — deliberate, not an oversight.

#    NOT added alongside it: `check-artifact-identity-budget`, which issue 0466
#    lists in the same finding (stop #3, "1.9 GB of Aug-7 rlibs"). Its own
#    `started_at` filter (issues 0499/0513) now answers that case with
#    "[SKIP] … this tree is history, not that build" — and this batch runs
#    BEFORE fixtures are built, which is exactly when that filter reports SKIP.
#    Adding it here would contribute a line that can never fire, which is the
#    gate-narrower-than-its-rule shape this tree keeps paying for. It stays in
#    `check-fast`, where the tree it measures is the one the run produced.
#
# 6. Residue a long-lived checkout accumulates. Issue 0466's finding (a): the
#    batch checked the CONTRACT but not the TREE'S HISTORY, so a stray `target/`
#    beside workspace source was stop #2 of five on 2026-08-11 — cheap to detect
#    up front, discovered last. The gate already runs in `check-fast`; what it
#    lacked was a seat in the ONE listing, so it arrived as its own 40-minute
#    round trip after this batch had already reported green.
#
#    Buildless and source-free, like every probe above it.
probe "build output beside workspace source (long-lived-tree residue)" \
    "rm -rf the named dir(s) — build output belongs under \$NROS_BUILD_ROOT (RFC-0070 R1)" \
    bash scripts/check-workspace-build-output.sh

# 6b. Corrosion's pin, because a stale one builds SUCCESSFULLY and wrong.
#
#     Every other probe here catches something that fails loudly later. This one
#     catches something that does not: Corrosion < 0.6.0 shares ONE cargo
#     target-dir across workspace roots, so two roots duplicate every nros crate
#     and the consequence lands hours away as duplicate `#[no_mangle]` symbols
#     at link (issues 0493/0500/0616). The store ACCUMULATES, so a stale prefix
#     shadows a pin that was correctly installed.
#
#     The only existing signal is a cmake WARNING printed per configure, which
#     is per-leaf noise in a build that prints thousands of lines — on
#     2026-08-19 it scrolled past 32 configures and was read only after the
#     failure it predicted had been misdiagnosed twice.
#
#     `nros setup --check --tool` already answers exactly this and NAMES what
#     the store holds instead (issue 0466 finding (b)); it had no caller ahead
#     of a build. Buildless, ~20 ms.
#
#     FAIL, not warn: the remedy is one command, and a warning is precisely what
#     was already being missed.
probe "corrosion in the SDK store is not at the pinned version" \
    "nros setup --tool corrosion   (or: just workspace install-corrosion)" \
    nros setup --check --tool corrosion

# 7. The CLI and the launch resolver are built by SEPARATE recipes and must
#    agree on an argument list (#0363 C), and `setup-cli` deliberately only
#    WARNS when it leaves the resolver behind — its job is to produce the CLI,
#    and the resolver has its own skip conditions. A warning printed at the tail
#    of one recipe is not something the next run re-states, so the skew reaches
#    a fixture build and surfaces there instead. Report it with everything else.
#
#    WARN, not fail: a resolver older than the CLI is only WRONG if the argument
#    list moved, which this cannot know. Failing on it would block the
#    legitimate CLI-only setup that `setup-cli` is careful to allow.
# issue 0599 — a lane that cannot run is a precondition, and this recipe exists
# to report every one of them BEFORE a run is committed to (issue 0466). The
# Zephyr lane skips when its workspace is absent; four west-owned compile-check
# fixtures are built by that lane and by no other, and they are unattributable
# to a coordinate so every run scope requires them. Left unsaid here, the
# operator learned it from `_lane-gate` twenty minutes later as four missing
# `.inputsig` files. WARN rather than fail: an unprovisioned host is legitimate
# for tier 1, and only the wider tiers actually need the lane.
_zephyr_ws="${NROS_ZEPHYR_WORKSPACE:-}"
if [ -z "$_zephyr_ws" ]; then
    for _cand in zephyr-workspace ../nano-ros-workspace; do
        [ -d "$_cand/zephyr" ] && _zephyr_ws="$_cand" && break
    done
fi
if [ -z "$_zephyr_ws" ] || [ ! -d "$_zephyr_ws/zephyr" ]; then
    echo "check-tier-preconditions: WARNING — no Zephyr workspace, so the zephyr" >&2
    echo "  fixture lane will SKIP. Tier 1 does not need it; tier 2+ does — the" >&2
    echo "  west-built compile-check fixtures (west_bringup_zephyr," >&2
    echo "  west_board_import, zephyr_self_pkg_{rust,sibling}) are built by that" >&2
    echo "  lane alone and are required by every run scope (issue 0599)." >&2
    echo "  Remedy: just zephyr setup" >&2
fi

# issue 0596 — ask about SOURCES, not binary mtimes. The old test was
# `cli -nt resolver`, and `setup-launch-resolve` is a cargo no-op when the
# resolver's sources have not changed, so it never relinked and the warning
# could not be cleared by the remedy it printed. Source staleness is the real
# invariant behind 0363 C (the two drift only when one is built from stale
# sources) and it IS clearable.
. "$(dirname "$0")/build/launch-resolve-stale.sh"
if nros_launch_resolve_stale "."; then
    echo "check-tier-preconditions: WARNING — nros-launch-resolve is older than its" >&2
    echo "  own SOURCES. It and the in-tree CLI must agree on an argument list" >&2
    echo "  (issue 0363 C); a skew surfaces deep in a fixture build, not here." >&2
    echo "  Remedy: just setup-launch-resolve" >&2
fi

# 8. The pinned make. `nros_pool_run` needs make 4.4's FIFO jobserver: the
#    system make on Ubuntu 22.04 is 4.3, whose pipe-FD jobserver a grandchild
#    (cargo, or cmake's sub-make) cannot join. Without it every jobserver
#    fan-out in the tree — example checks, fixture builds, the compile-check
#    sweep — silently walks SERIALLY.
#
#    It is now an ORDINARY store tool: `nros setup --tool make` builds 4.4.1
#    from the release tarball, and `scripts/sdk-path-tools.txt` puts it on PATH.
#    The path is resolved with `nros sdk-path` — CONSTRUCTED from the index pin,
#    never searched (issue 0625) — so this cannot drift from what provisioning
#    installed.
#
#    This block used to read `third-party/make/make`, filled by a bespoke
#    `just workspace install-make`, and its comment justified that with "the
#    index has no version predicate (cmd / sharedlib / pkg_config only), so an
#    entry there would assert something false". That was true when written and
#    is not any more: `check = { cmd = …, version = { min = … } }` exists (see
#    `[prereq.openocd]`), which is exactly the predicate that was missing. So
#    `[prereq.make]` now asserts `min = "4.4"` truthfully, and the apt-installs-4.3
#    objection is answered by the version floor rather than by staying out of
#    the index.
#
#    WARN, not fail: a serial walk is correct, only slow.
_pre_make=""
if command -v nros >/dev/null 2>&1; then
    _pre_make="$(nros sdk-path make 2>/dev/null)/bin/make"
fi
if [ -z "$_pre_make" ] || [ ! -x "$_pre_make" ] ||
    ! "$_pre_make" --version 2>/dev/null | head -1 | grep -q "4\.4"; then
    echo "check-tier-preconditions: WARNING — pinned GNU make 4.4 absent;" >&2
    echo "  every jobserver fan-out degrades to a SERIAL walk (the system make" >&2
    echo "  is 4.3 on Ubuntu LTS, and its pipe-FD jobserver cannot be joined by" >&2
    echo "  cargo or by cmake's sub-make)." >&2
    echo "  Remedy: nros setup --tool make" >&2
fi
unset _pre_make

# 9. A lane that silently DEGRADES is worse than one that fails: without GNU
#    parallel the example check walks ~99 leaves serially and reads as a hung
#    tier, not a missing package. Warn — do not fail — since the lane is correct,
#    only slow.
if ! command -v parallel >/dev/null 2>&1; then
    echo "check-tier-preconditions: WARNING — GNU parallel absent; the example" >&2
    echo "  lane degrades to a serial walk (minutes, and it looks like a hang)." >&2
    echo "  Remedy: nros setup --system     (just doctor lists it)" >&2
fi

if [ "$failed" -eq 0 ]; then
    echo "check-tier-preconditions: OK (submodules, CLI, leaf includes, build sources, fixtures, build-output residue)"
    exit 0
fi

{
    echo
    echo "======================================================================"
    echo " $failed tier precondition(s) unmet — ALL of them, not just the first"
    echo "======================================================================"
    for entry in "${REPORT[@]}"; do
        label="${entry%%|*}"
        rest="${entry#*|}"
        remedy="${rest%%|*}"
        detail="${rest#*|}"
        echo
        echo "  [x] $label"
        echo "      remedy: $remedy"
        if [ -n "$detail" ]; then
            printf '      %s\n' "$(printf '%s' "$detail" | head -6 | sed 's/^/  /')"
        fi
    done
    echo
    echo "  Order matters, and it is the order above: submodules first (updating"
    echo "  one rewrites source mtimes, re-arming everything below it), then the"
    echo "  CLI, then fixtures — fixtures key on the CLI's source stamp, so doing"
    echo "  those two the other way round re-stales them all."
    echo
    echo "  Bypass everything: NROS_SKIP_TIER_PRECONDITIONS=1"
} >&2

exit 1
