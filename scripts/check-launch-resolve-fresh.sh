#!/usr/bin/env bash
#
# Issue 1487 — fail FAST when `nros-launch-resolve` no longer matches the
# sources it compiled, instead of failing deep inside a lane that uses it.
#
# The sibling of `check-cli-fresh.sh`, and for the same reason that file gives:
# what it contributes is POSITION. The predicate already existed
# (`nros_launch_resolve_stale`, issue 0596's consolidation) and
# `check-tier-preconditions` already runs it — but only as a WARNING, and
# `just ci gate` does not run `tier-preconditions` at all. So on the push lane
# nothing asked, and a stale resolver surfaced eleven minutes in, inside
# `check-template-copy-out`, as:
#
#     Error: sync: `.../nros-launch-resolve` was built from play_launch
#            07f0461e but this `nros` was built from 9a610488.
#
# That is issue 0409's guard, which is correct and load-bearing: a resolver from
# a different layer-2 checkout does NOT fail, it writes models that are missing
# data silently. It just cannot say which recipe to run or which step should
# have caught it.
#
# The asymmetry this closes: `nros` and `nros-launch-resolve` are built by
# SEPARATE recipes from overlapping inputs — both compile the `play_launch`
# submodule — so a pin move stales both, and only one of them was guarded.
#
# IMPLEMENTS NOTHING. It sources the one predicate, exactly as `check-cli-fresh`
# defers to `nros source-stamp`. A second implementation of "is the resolver
# stale" is what 0596 removed.

set -euo pipefail
cd "$(dirname "$0")/.."

CRATE="packages/cli/nros-launch-resolve"
# Honour CARGO_TARGET_DIR the way the recipe does (issue 0400): cargo writes
# there, so a check that looks elsewhere reports on a binary nobody will run.
BIN="${CARGO_TARGET_DIR:-$CRATE/target}/release/nros-launch-resolve"

# No resolver yet is a NORMAL state, not a failure — a fresh clone before
# `just setup-launch-resolve`, and any CI job that builds no resolver. Gating on
# it would make the lane unrunnable on a fresh clone, which is the position
# `cli-fresh` already takes for the same reason.
if [ ! -x "$BIN" ]; then
    echo "launch-resolve-fresh: SKIP — no resolver binary yet (run \`just setup-launch-resolve\`)."
    exit 0
fi

# The play_launch submodule not being checked out is likewise not this check's
# failure to report: `setup-launch-resolve` fails loudly on it with the
# `git submodule update --init` remedy, and the predicate cannot answer without
# the tree.
if [ ! -d "packages/cli/third-party/play_launch/src/ros-launch-resolve" ]; then
    echo "launch-resolve-fresh: SKIP — play_launch submodule not initialised."
    exit 0
fi

# shellcheck source=scripts/build/launch-resolve-stale.sh
. scripts/build/launch-resolve-stale.sh

# NEGATIVE CONTROL, on the normal path (issue 1167 — a guard that exists is not
# a guard that fires; a self-test nobody runs decays into a comment).
#
# Drives BOTH verdicts over a throwaway `CARGO_TARGET_DIR`, which is the one
# input that redirects where the predicate looks for the binary and its stamp.
# The repo itself is only READ (the stamp hashes tracked sources), so this
# cannot disturb the real resolver, its stamp, or any git state.
_selftest() {
    local td stamp rc
    td="$(mktemp -d "${TMPDIR:-/tmp}/nros-lrf-selftest.XXXXXX")" || return 1
    mkdir -p "$td/release"
    : > "$td/release/nros-launch-resolve"
    chmod +x "$td/release/nros-launch-resolve"

    # (a) a stamp that matches the tree must read FRESH.
    stamp="$(nros_launch_resolve_stamp ".")"
    printf '%s\n' "$stamp" > "$td/release/nros-launch-resolve.nros-source-stamp"
    rc=0
    ( CARGO_TARGET_DIR="$td" nros_launch_resolve_stale "." ) || rc=$?
    if [ "$rc" -eq 0 ]; then
        echo "[ERROR] selftest: a stamp matching the tree was reported STALE." >&2
        rm -rf "$td"
        return 1
    fi

    # (b) a stamp that does not must read STALE. This is the arm that fails if
    #     the predicate is ever made to answer "fresh" unconditionally.
    printf 'not-the-stamp\n' > "$td/release/nros-launch-resolve.nros-source-stamp"
    rc=0
    ( CARGO_TARGET_DIR="$td" nros_launch_resolve_stale "." ) || rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "[ERROR] selftest: a stamp that does not match the tree was reported FRESH." >&2
        echo "        That is issue 1487's failure with the check in place." >&2
        rm -rf "$td"
        return 1
    fi

    rm -rf "$td"
    return 0
}

_selftest || exit 1

if nros_launch_resolve_stale "."; then
    pin="$("$BIN" --version 2>/dev/null \
        | sed -n 's/.*play_launch \([0-9a-f]*\).*/\1/p' | head -1)"
    head="$(git -C packages/cli/third-party/play_launch rev-parse HEAD 2>/dev/null || echo unknown)"
    echo "[ERROR] nros-launch-resolve is STALE — built from sources this tree no longer has." >&2
    # Name the PIN only when it is the thing that moved. Printing two identical
    # shas for a sources-only change reads as a contradiction ("built from X,
    # tree at X, therefore stale?") and sends the reader after the submodule
    # instead of after their own edit.
    if [ -n "$pin" ] && [ "$pin" != "$head" ]; then
        echo "        binary built from play_launch ${pin}" >&2
        echo "        tree now checked out at       ${head}" >&2
        echo "        (a pin move — \`git submodule update\` advanced play_launch)" >&2
    else
        echo "        play_launch ${head} — the PIN is unchanged, the resolver SOURCES differ" >&2
        echo "        (an edit under play_launch/src/ros-launch-resolve, or in this crate)" >&2
    fi
    echo "" >&2
    echo "        It and the in-tree \`nros\` must agree (issue 0409): a resolver from a" >&2
    echo "        different layer-2 checkout does not fail, it writes models that are" >&2
    echo "        MISSING DATA silently, and \`nros sync\` refuses rather than allow that." >&2
    echo "" >&2
    echo "        Rebuild it:  just setup-launch-resolve" >&2
    echo "" >&2
    echo "        A \`git submodule update\` on play_launch stales BOTH binaries." >&2
    echo "        \`just setup-cli\` alone is not enough (issue 1487)." >&2
    exit 1
fi

echo "launch-resolve-fresh: OK ($BIN matches its sources)"
