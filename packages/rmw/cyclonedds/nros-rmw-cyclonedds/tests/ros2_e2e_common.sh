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
nros_unique_ros_domain_id() {
    if [ -n "${NEXTEST_TEST_GLOBAL_SLOT:-}" ]; then
        echo $(( (NEXTEST_TEST_GLOBAL_SLOT % 232) + 1 ))
    else
        echo $(( ($$ % 232) + 1 ))
    fi
}
