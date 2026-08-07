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

# 1. The CLI's source stamp. FIRST because everything downstream keys on it, and
#    because ANY tree refresh — pull, rebase, stash, rsync — re-arms it. That
#    re-arming is what makes the contract once-per-refresh rather than
#    once-per-clone, and it is the single most repeated stop.
probe "in-tree nros CLI is stale" \
    "just setup-cli    (any pull/rebase/stash re-arms this)" \
    bash scripts/check-cli-fresh.sh
cli_stale=$failed   # non-zero ⇒ everything that EXECS the CLI below would only echo this

# 2. Leaves must be sync'd or cargo cannot even PARSE them (#463) — the error
#    surfaces as "failed to parse manifest", four frames deep, never naming sync.
probe "leaf .cargo/config.toml includes an unwritten sync target" \
    "nros sync   in the named leaf   (bypass: NROS_SKIP_LEAF_INCLUDE_CHECK=1)" \
    python3 scripts/build/leaf-config-includes.py

# 3. The build stage needs the UNION of vendored sources, not the per-board
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

# 4. Fixtures, for the LANE this run will test. Coverage is per-lane (#393), so
#    "some fixtures exist" is not the question.
probe "test fixtures missing or stale for this lane" \
    "just build-test-fixtures lane=${NROS_FIXTURE_LANE:-all}   (bypass: NROS_SKIP_FIXTURE_CHECK=1)" \
    just _require-fixtures

# 5. The pinned make. `nros_pool_run` needs make 4.4's FIFO jobserver: the
#    system make on Ubuntu 22.04 is 4.3, whose pipe-FD jobserver a grandchild
#    (cargo, or cmake's sub-make) cannot join. Without it every jobserver
#    fan-out in the tree — example checks, fixture builds, the compile-check
#    sweep — silently walks SERIALLY. `just install-make` builds it to
#    `third-party/make/` and `.envrc` puts it first on PATH.
#
#    Deliberately NOT a `[system.make]` entry in nros-sdk-index.toml: the apt
#    package is 4.3 on the LTS we target, so provisioning it would satisfy a
#    `check = { cmd = "make" }` probe while still not giving a usable jobserver.
#    The index has no version predicate (cmd / sharedlib / pkg_config only), so
#    an entry there would assert something false.
#
#    WARN, not fail: a serial walk is correct, only slow.
if [ ! -x "third-party/make/make" ] ||
    ! third-party/make/make --version 2>/dev/null | head -1 | grep -q "4\.4"; then
    echo "check-tier-preconditions: WARNING — pinned GNU make 4.4 absent;" >&2
    echo "  every jobserver fan-out degrades to a SERIAL walk (the system make" >&2
    echo "  is 4.3 on Ubuntu LTS, and its pipe-FD jobserver cannot be joined by" >&2
    echo "  cargo or by cmake's sub-make)." >&2
    echo "  Remedy: just install-make" >&2
fi

# 5. A lane that silently DEGRADES is worse than one that fails: without GNU
#    parallel the example check walks ~99 leaves serially and reads as a hung
#    tier, not a missing package. Warn — do not fail — since the lane is correct,
#    only slow.
if ! command -v parallel >/dev/null 2>&1; then
    echo "check-tier-preconditions: WARNING — GNU parallel absent; the example" >&2
    echo "  lane degrades to a serial walk (minutes, and it looks like a hang)." >&2
    echo "  Remedy: nros setup --system     (just doctor lists it)" >&2
fi

if [ "$failed" -eq 0 ]; then
    echo "check-tier-preconditions: OK (CLI, leaf includes, build sources, fixtures)"
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
    echo "  Order matters: rebuild the CLI BEFORE fixtures — fixtures key on its"
    echo "  source stamp, so doing it the other way round re-stales them all."
    echo
    echo "  Bypass everything: NROS_SKIP_TIER_PRECONDITIONS=1"
} >&2

exit 1
