#!/bin/bash
# Enumerate the ROS interface definitions this codegen cannot handle.
#
# The enumeration itself lives in the TESTS now (issue 1176): `parity_test`'s
# walks hold their result to the committed ledger
# `tests/parity-expected-failures.txt`, and the failing assertion prints every
# unlisted failure in that file's own format. This script is a driver over the
# per-package tests plus the bundled-corpus one — it no longer scrapes numbers
# out of an `eprintln!`, because that `eprintln!` is gone.
#
# Issue 0693 — it resolved `/opt/ros/jazzy` while the project installs humble,
# so every package reported "No msg directory" and it checked nothing while
# exiting 0.
#
# Issue 1176 — and after that was fixed it STILL checked nothing, for a second
# reason nobody had looked for: it gated each package on
# `grep -q "0 filtered out"`, but `cargo test --test parity_test <one-name>`
# filters the other 21 tests OUT, so the output reads `21 filtered out` and
# every package took the `⊘ No test` arm. It then divided a failure count of 0
# by a message count it had gathered from the filesystem and announced
# "Success rate: 100%". Same class as the defect in the tests it drives: a
# report of success over work that never ran. A package with no test is now
# COUNTED AND NAMED as unchecked instead of disappearing into that percentage.

set -e

cd "$(dirname "$0")/.."

echo "Enumerating parser/codegen failures..."
echo "======================================================"
echo

# Only these three have a `test_parse_all_*` against the installed distro. The
# other packages the old list named (nav_msgs, action_msgs, diagnostic_msgs,
# builtin_interfaces, example_interfaces, lifecycle_msgs, rosgraph_msgs) never
# had one, so naming them only ever produced a "⊘" line — all but nav_msgs are
# covered by the bundled walk at the bottom, which needs no ROS at all.
PACKAGES=(
    "std_msgs"
    "geometry_msgs"
    "sensor_msgs"
)

TOTAL_MESSAGES=0
CHECKED_PACKAGES=0
FAILED_PACKAGES=0

ROS_SHARE=""
if [ -n "${ROS_DISTRO:-}" ] && [ -d "/opt/ros/${ROS_DISTRO}/share" ]; then
    ROS_SHARE="/opt/ros/${ROS_DISTRO}/share"
else
    _found=(/opt/ros/*/share)
    if [ ${#_found[@]} -eq 1 ] && [ -d "${_found[0]}" ]; then
        ROS_SHARE="${_found[0]}"
    fi
fi

if [ -z "$ROS_SHARE" ]; then
    echo "no ROS 2 install found (set ROS_DISTRO or install one under /opt/ros)"
    echo "the per-distro walks below cannot run; going straight to the bundled corpus"
    echo
else
    echo "using ROS share: $ROS_SHARE"
    echo

    for pkg in "${PACKAGES[@]}"; do
        MSG_DIR="${ROS_SHARE}/${pkg}/msg"

        if [ ! -d "$MSG_DIR" ]; then
            echo "⊘ ${pkg}: not installed on this host"
            continue
        fi

        MSG_COUNT=$(find "$MSG_DIR" -name "*.msg" | wc -l)
        if [ "$MSG_COUNT" -eq 0 ]; then
            echo "⊘ ${pkg}: installed but holds no .msg files"
            continue
        fi

        echo -n "Testing ${pkg} (${MSG_COUNT} messages)... "

        OUTPUT=$(cargo test --test parity_test "test_parse_all_${pkg}" -- --nocapture 2>&1 || true)

        # Did the test RUN? libtest prints one `test <name> ... <verdict>` line
        # per executed test, and prints nothing at all for a filtered one.
        if ! echo "$OUTPUT" | grep -q "^test test_parse_all_${pkg} "; then
            echo "⊘ no such test — nothing checked"
            continue
        fi

        TOTAL_MESSAGES=$((TOTAL_MESSAGES + MSG_COUNT))
        CHECKED_PACKAGES=$((CHECKED_PACKAGES + 1))

        if echo "$OUTPUT" | grep -q "^test test_parse_all_${pkg} \.\.\. ok"; then
            echo "✓ every definition matched the ledger"
        else
            FAILED_PACKAGES=$((FAILED_PACKAGES + 1))
            echo "✗ the ledger and the walk disagree:"
            echo "$OUTPUT" | sed -n '/definition(s) walked/,/^note:/p' | sed 's/^/    /'
        fi
    done
    echo
fi

# The bundled corpus — `packages/cli/interfaces/`, the vendored ROS 2 Humble
# sources. Runs on ANY host, which is the point: `check-cli-tests` has no ROS,
# so without this the ratchet above fires on nobody's lane.
echo -n "Testing bundled interface sources (no ROS needed)... "
OUTPUT=$(cargo test --test parity_test bundled_interfaces_have_no_parity_failures \
    -- --nocapture 2>&1 || true)
if echo "$OUTPUT" | grep -q "^test bundled_interfaces_have_no_parity_failures \.\.\. ok"; then
    echo "✓ every definition matched the ledger"
else
    FAILED_PACKAGES=$((FAILED_PACKAGES + 1))
    echo "✗:"
    echo "$OUTPUT" | sed -n '/definition(s) walked/,/^note:/p' | sed 's/^/    /'
fi

echo
echo "======================================================"
echo "Summary:"
echo "  Distro packages checked:  ${CHECKED_PACKAGES} of ${#PACKAGES[@]} (${TOTAL_MESSAGES} messages)"
echo "  Walks disagreeing with the ledger: ${FAILED_PACKAGES}"
echo "  Ledger: $(pwd)/tests/parity-expected-failures.txt"
echo

# A package that could not be checked is NOT a pass. Reporting one as such is
# what this script did for its whole life (issues 0693, 1176).
if [ "$FAILED_PACKAGES" -gt 0 ]; then
    exit 1
fi
