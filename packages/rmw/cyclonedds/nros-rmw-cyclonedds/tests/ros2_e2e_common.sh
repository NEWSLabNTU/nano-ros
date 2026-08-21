#!/usr/bin/env bash
# Shared helpers for the ROS 2 interop e2e scripts in this directory.
#
# Sourced, never executed. `add_test` runs these scripts from
# `CMAKE_CURRENT_SOURCE_DIR`, so a sibling file is always present.

# issue 0580 — pick a ROS domain no CONCURRENT copy of this test will pick.
#
# These scripts used to hardcode a fallback (`${ROS_DOMAIN_ID:-117}` for pubsub,
# 118 for services). A fixed domain is a shared bus: two overlapping runs
# discover each other's writers, and the failure reads as a delivery bug rather
# than a collision. Observed under a tier-2 sweep — the subscriber captured
# `hello-from-nros`, which is the payload the OTHER copy's case-A.1 publisher
# emits, while solo runs of the same suite passed 17/17 twice.
#
# Mirrors `nros_tests::unique_ros_domain_id` (packages/testing/nros-tests/src/lib.rs)
# rather than inventing a second scheme: prefer nextest's global slot when the
# runner provides one, else the pid, folded into 1..=232 (0 is the default
# everyone else lands on, and the ROS 2 range tops out well below 255).
#
# An explicit `ROS_DOMAIN_ID` in the environment still wins — pinning one is how
# you reproduce a failure by hand.
# issue 0703 — the modulus is 101, shared with the Rust and C++ assigners.
# Cyclone derives its RTPS ports from the domain (`7400 + 250*D`), and Linux
# hands out ephemeral ports from 32768, so from domain 102 up
# (`7400 + 250*102 = 32900`) the port a participant must have is one the OS may
# already have given away — the bind fails and the session never opens. See
# `nros_test_domain.h` for the measurement.
NROS_TEST_DOMAIN_MAX=101

# issue 0747 — probe before taking the bus, the third of the three assigners.
#
# Issue 0707 taught the Rust one to read /proc/net/udp and step past an occupied
# domain, and the header above has claimed "one scheme" throughout — but the fix
# landed in Rust only, so C++ and this file kept picking blind. Measured cost on
# 2026-08-21: `check-rmw-cyclonedds` 3 red in ~6 in-sweep runs, 0 red in 3 solo,
# a different test each time, once printing `failed to bind to ANY:8650: address
# in use` (domain 5). With 101 buckets and a 32-way fan-out over hundreds of
# short-lived processes on sequential PIDs, a collision inside one sweep is the
# expected case.
#
# `false` when /proc is unreadable: unknown must not read as busy, or every
# domain looks taken and the stepping degrades to its fallback for nothing.
nros_domain_busy() {
    local domain="$1"
    local want
    want=$(printf '%04X' $(( 7400 + 250 * domain )))
    local table
    for table in /proc/net/udp /proc/net/udp6; do
        [ -r "$table" ] || continue
        # Column 2 is `HEXADDR:HEXPORT`. Compared as UPPERCASE HEX TEXT rather
        # than converted to a number, because `strtonum` is a gawk extension and
        # Ubuntu's default awk is mawk — there it is a fatal error, awk exits
        # non-zero, and this function would answer "not busy" for every domain
        # on the very hosts CI runs. /proc prints the port as fixed 4-digit
        # uppercase hex, which `printf '%04X'` matches exactly.
        if awk -v want="$want" 'NR > 1 {
                split($2, a, ":")
                if (a[2] == want) { found = 1; exit }
            }
            END { exit(found ? 0 : 1) }' "$table"; then
            return 0
        fi
    done
    return 1
}

nros_unique_ros_domain_id() {
    local first
    if [ -n "${NEXTEST_TEST_GLOBAL_SLOT:-}" ]; then
        first=$(( (NEXTEST_TEST_GLOBAL_SLOT % NROS_TEST_DOMAIN_MAX) + 1 ))
    else
        first=$(( ($$ % NROS_TEST_DOMAIN_MAX) + 1 ))
    fi
    if ! nros_domain_busy "$first"; then
        echo "$first"
        return 0
    fi
    # Bounded step, then give up and return the first candidate: a box where
    # every domain looks busy is not something this function can fix, and
    # returning nothing would break every caller (issue 0707's contract).
    local step candidate
    for (( step = 1; step <= NROS_TEST_DOMAIN_MAX; step++ )); do
        candidate=$(( ((first - 1 + step) % NROS_TEST_DOMAIN_MAX) + 1 ))
        if ! nros_domain_busy "$candidate"; then
            echo "$candidate"
            return 0
        fi
    done
    echo "$first"
}
