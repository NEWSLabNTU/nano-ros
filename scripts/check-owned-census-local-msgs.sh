#!/usr/bin/env bash
# Issue 2805 -- the census of an OWNED entry is taken when the entry depends on
# one of its workspace's own message packages.
#
# `packages/testing/nros-tests/bins/owned-census-local-msgs` is a workspace
# with a stock message package (`src/owned_census_msgs`), an owned entry on a
# cross board (`src/owned_census_talker`, threadx-linux) whose one publisher
# carries that package's `Tally`, and a bringup with no `[census]` table, so a
# census that cannot be taken refuses. The check runs the take the build's
# pre-pass runs (`nros dev image entity-census take --image`) and asserts that
# it succeeds and that the census it writes records the `/tally` publisher of
# type `owned_census_msgs/msg/Tally`.
#
# The census is a HOST build of the entry's package alone. Before issue 2805
# that build put nothing of the workspace on its interface search path, so
# `owned_census_msgs.h` was never generated and the take refused.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ws="$repo/packages/testing/nros-tests/bins/owned-census-local-msgs"
image="owned_census_bringup:threadx-linux"
census="$ws/build/nros/census/owned/owned_census_bringup/threadx-linux.census.json"
log="$ws/build/census-take.log"

# shellcheck source=scripts/lib/grep-q.sh
source "$repo/scripts/lib/grep-q.sh"

fail() {
    echo "check-owned-census-local-msgs: FAILED -- $*" >&2
    exit 1
}

# verify <census.json>
# Prints the reason and returns 1 when the census misses the entry's publisher.
verify() {
    local census="$1" rc
    [ -f "$census" ] || { echo "the take wrote no census at $census"; return 1; }
    rc=0
    nros_grep_q '"/tally' "$census" || rc=$?
    case "$rc" in
        0) ;;
        1) echo "the census records no \`/tally\` publisher: $census"; return 1 ;;
        *) echo "cannot read $census (grep exited $rc)"; return 1 ;;
    esac
    rc=0
    nros_grep_q '"package": "owned_census_msgs"' "$census" || rc=$?
    case "$rc" in
        0) ;;
        1) echo "the census records no \`owned_census_msgs\` interface: $census"; return 1 ;;
        *) echo "cannot read $census (grep exited $rc)"; return 1 ;;
    esac
}

self_test() {
    local t rc=0 out
    t="$(mktemp -d)"
    # shellcheck disable=SC2064
    trap "rm -rf '$t'" RETURN
    printf '{"id": "/tally#1", "interface": {"package": "owned_census_msgs"}}\n' >"$t/ok.json"
    printf '{"id": "/tally#1", "interface": {"package": "std_msgs"}}\n' >"$t/other.json"
    printf '{"nodes": []}\n' >"$t/empty.json"
    out="$(verify "$t/ok.json")" || { echo "self-test: a complete census was refused: $out" >&2; rc=1; }
    if out="$(verify "$t/other.json")"; then
        echo "self-test: a census without the workspace message was accepted" >&2
        rc=1
    fi
    if out="$(verify "$t/empty.json")"; then
        echo "self-test: a census without the publisher was accepted" >&2
        rc=1
    fi
    if out="$(verify "$t/missing.json")"; then
        echo "self-test: a missing census was accepted" >&2
        rc=1
    fi
    [ "$rc" -eq 0 ] && echo "check-owned-census-local-msgs: self-test OK (complete accepted; foreign type, no publisher and missing rejected)"
    return "$rc"
}

case "${1:-}" in
    --self-test) self_test; exit $? ;;
    "") ;;
    *) echo "usage: $0 [--self-test]" >&2; exit 2 ;;
esac
self_test || exit 1

# shellcheck source=scripts/build/cargo.sh
source "$repo/scripts/build/cargo.sh"
nros=""
if ! nros="$(nros_cli_bin --checkout-only)"; then
    fail "this checkout has no built nros CLI (run \`just setup-cli\`)"
fi

# A fresh take every run: a census left by an earlier run would answer the
# take as current and prove nothing about this tree.
rm -rf "$ws/build"
mkdir -p "$ws/build"
echo "check-owned-census-local-msgs: taking the census of $image"
if ! (
    export PATH="$(dirname "$nros"):$PATH"
    "$nros" dev image entity-census take --image "$image" --workspace "$ws"
) >"$log" 2>&1; then
    grep -m20 -E 'error|Error|refus' "$log" >&2 || tail -40 "$log" >&2
    fail "the take refused (log: $log)"
fi
verdict=""
if ! verdict="$(verify "$census")"; then
    fail "$verdict (log: $log)"
fi
echo "check-owned-census-local-msgs: OK (census of $image taken: $census)"
