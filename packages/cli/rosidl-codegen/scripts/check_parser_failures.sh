#!/bin/bash
# Script to identify all ROS messages that fail to parse

set -e

cd "$(dirname "$0")/.."

echo "Checking parser failures across common ROS packages..."
echo "======================================================"
echo

PACKAGES=(
    "std_msgs"
    "geometry_msgs"
    "sensor_msgs"
    "nav_msgs"
    "action_msgs"
    "diagnostic_msgs"
    "builtin_interfaces"
    "example_interfaces"
    "lifecycle_msgs"
    "rosgraph_msgs"
)

TOTAL_MESSAGES=0
TOTAL_FAILURES=0
FAILED_MESSAGES=()

# Issue 0693 — resolve the INSTALLED distro instead of naming one. This said
# `jazzy` while the project installs humble, so every package reported
# "No msg directory" and the script checked nothing while exiting 0.
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
    echo "no ROS 2 install found (set ROS_DISTRO or install one under /opt/ros)" >&2
    echo "this script checks message parsing against a real distro; nothing to check" >&2
    exit 1
fi
echo "using ROS share: $ROS_SHARE"

for pkg in "${PACKAGES[@]}"; do
    MSG_DIR="${ROS_SHARE}/${pkg}/msg"

    if [ ! -d "$MSG_DIR" ]; then
        echo "⊘ ${pkg}: No msg directory"
        continue
    fi

    MSG_COUNT=$(find "$MSG_DIR" -name "*.msg" | wc -l)
    if [ "$MSG_COUNT" -eq 0 ]; then
        echo "⊘ ${pkg}: No .msg files"
        continue
    fi

    TOTAL_MESSAGES=$((TOTAL_MESSAGES + MSG_COUNT))

    echo -n "Testing ${pkg} (${MSG_COUNT} messages)... "

    # Run test and capture output
    OUTPUT=$(cargo test --test parity_test "test_parse_all_${pkg}" -- --nocapture 2>&1 || true)

    # Check if test exists and ran
    if echo "$OUTPUT" | grep -q "0 filtered out"; then
        # Extract failure info
        if echo "$OUTPUT" | grep -q "Failed to process"; then
            FAILURES=$(echo "$OUTPUT" | grep "Failed to process" | sed 's/.*Failed to process \([0-9]*\) out of.*/\1/')
            SUCCESS_RATE=$(echo "$OUTPUT" | grep "success rate" | sed 's/.*(\([0-9]*\)% success rate).*/\1/')
            echo "✗ ${FAILURES} failures (${SUCCESS_RATE}% success)"

            TOTAL_FAILURES=$((TOTAL_FAILURES + FAILURES))

            # Extract failed messages
            while IFS= read -r line; do
                if echo "$line" | grep -q "\.msg:"; then
                    MSG_FILE=$(echo "$line" | sed 's|.*/\([^/]*\.msg\):.*|\1|')
                    ERROR=$(echo "$line" | sed 's/.*Failed to parse[^:]*: \(.*\)/\1/')
                    FAILED_MESSAGES+=("${pkg}/${MSG_FILE}: ${ERROR}")
                fi
            done < <(echo "$OUTPUT" | grep "\.msg:")
        else
            echo "✓ All passed"
        fi
    else
        echo "⊘ No test for this package"
    fi
done

echo
echo "======================================================"
echo "Summary:"
echo "  Total messages tested: ${TOTAL_MESSAGES}"
echo "  Total failures: ${TOTAL_FAILURES}"
echo "  Success rate: $(( (TOTAL_MESSAGES - TOTAL_FAILURES) * 100 / TOTAL_MESSAGES ))%"
echo

if [ ${#FAILED_MESSAGES[@]} -gt 0 ]; then
    echo "Failed messages:"
    for msg in "${FAILED_MESSAGES[@]}"; do
        echo "  - ${msg}"
    done
fi
