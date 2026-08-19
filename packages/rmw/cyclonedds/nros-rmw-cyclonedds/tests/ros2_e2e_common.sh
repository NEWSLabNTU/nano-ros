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

nros_unique_ros_domain_id() {
    if [ -n "${NEXTEST_TEST_GLOBAL_SLOT:-}" ]; then
        echo $(( (NEXTEST_TEST_GLOBAL_SLOT % NROS_TEST_DOMAIN_MAX) + 1 ))
    else
        echo $(( ($$ % NROS_TEST_DOMAIN_MAX) + 1 ))
    fi
}
