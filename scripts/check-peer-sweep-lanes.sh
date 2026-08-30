#!/usr/bin/env bash
# issue 0923 — every lane that RUNS the peer-spawning suites must sweep first.
#
# Issue 0659 built the reaper and wired it into `test-all`. `just test` runs the
# same nextest suites, spawns the same peers, and did not sweep — so a SIGKILLed
# run left them alive until somebody happened to run the other lane. 67 orphans,
# oldest 3 h, measured 2026-08-30.
#
# The sweep's own unit tests call `sweep_in` directly, so they can never see
# whether a LANE calls it. That is what this gate is for, and it is the
# issue-0196 rule: when a mechanism exists, check the sites are covered.
set -o pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root" || exit 1

# ONE extraction, used by both the selftest and the real check — a control that
# exercises a copy of the logic proves nothing about the logic that ships.
extract_body() {
    awk -v want="^$1( |:)" '
        $0 ~ want { inbody = 1; next }
        inbody && /^[^[:space:]#]/ { exit }
        inbody { print }
    ' "$2"
}

# The gate's own negative control, on the NORMAL path: a checker that cannot
# fail is a comment. Runs against synthetic recipe text rather than the real
# justfile, so it proves the DETECTION, not the current state of the tree.
selftest() {
    local body_ok body_bad
    body_ok='    cargo run -q -p nros-tests --bin nros-peer-sweep 2>/dev/null || true
    cargo nextest run'
    body_bad='    cargo nextest run'

    if ! grep -q 'nros-peer-sweep' <<<"$body_ok"; then
        echo "check-peer-sweep-lanes selftest: FAILED to accept a sweeping lane" >&2
        return 1
    fi
    if grep -q 'nros-peer-sweep' <<<"$body_bad"; then
        echo "check-peer-sweep-lanes selftest: FAILED to reject a non-sweeping lane" >&2
        return 1
    fi

    # The extraction has to stop at the next recipe, or a LATER lane's sweep
    # would satisfy an earlier lane that has none — the failure mode that makes
    # a per-site gate pass vacuously.
    local tf extracted
    tf="$(mktemp)"
    cat > "$tf" <<'SELFTEST_EOF'
test verbose="":
    cargo nextest run
test-all verbose="":
    cargo run -q -p nros-tests --bin nros-peer-sweep
SELFTEST_EOF
    extracted="$(extract_body test "$tf")"
    rm -f "$tf"
    # Non-empty is asserted FIRST: an extraction that returns nothing makes the
    # leak check below pass without examining anything, which is the vacuous
    # pass this control exists to catch. It happened while writing this.
    if [ -z "$extracted" ]; then
        echo "check-peer-sweep-lanes selftest: body extraction returned NOTHING" >&2
        return 1
    fi
    if grep -q 'nros-peer-sweep' <<<"$extracted"; then
        echo "check-peer-sweep-lanes selftest: body extraction LEAKED into the next recipe" >&2
        return 1
    fi
    return 0
}

selftest || {
    echo "check-peer-sweep-lanes: selftest failed — the checker is not trustworthy." >&2
    exit 1
}

recipes="$(mktemp)"
trap 'rm -f "$recipes"' EXIT

# The lanes that invoke nextest over the peer-spawning suites. Named rather than
# detected: `just --summary` cannot tell which recipe spawns a peer, and a
# name-based guess would silently stop covering a renamed lane.
LANES=(test test-all)

fail=0
for lane in "${LANES[@]}"; do
    # The recipe body runs from its header to the next unindented line.
    body="$(extract_body "$lane" justfile)"
    if [ -z "$body" ]; then
        echo "check-peer-sweep-lanes: recipe '$lane' not found in justfile."
        echo "  If it was renamed, update LANES in $0 — a lane that vanished from"
        echo "  this list stops being checked WITHOUT failing, which is the shape"
        echo "  this gate exists to prevent."
        fail=1
        continue
    fi
    if ! grep -q 'nros-peer-sweep' <<<"$body"; then
        echo "check-peer-sweep-lanes: '$lane' runs the peer suites but never sweeps."
        echo "  Add, before nextest starts:"
        echo "      cargo run -q -p nros-tests --bin nros-peer-sweep 2>/dev/null || true"
        echo "  Peers outlive a SIGKILLed run (PR_SET_PDEATHSIG reaches bash only;"
        echo "  timeout/ros2/the node reparent to init) and hold DDS discovery ports"
        echo "  until something reaps them (issues 0659, 0923)."
        fail=1
    fi
done

if [ "$fail" -ne 0 ]; then
    exit 1
fi
echo "check-peer-sweep-lanes: OK (selftest ok; ${#LANES[@]} lane(s) sweep before nextest)"
