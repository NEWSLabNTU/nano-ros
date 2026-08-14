#!/usr/bin/env bash
# Issue 0580 — a test must ASSIGN its ROS domain, never name one.
#
# WHAT THIS CATCHES
#
# A DDS domain is a shared bus. A test that names one with a literal shares that
# bus with every concurrent run of itself, and the collision does not present as
# a collision — it presents as WRONG DATA. That is how 0580 read: a subscriber
# waiting for `hello-from-ros2` captured `hello-from-nros`, which is the payload
# the OTHER copy's publisher emits, while solo runs passed 17/17 twice.
#
# The literals were spread across three languages: `${ROS_DOMAIN_ID:-117}` and
# `:-118` in shell, `create_session(..., 99, ...)` (six sites), 88 and 42 in
# C++, and a bare `.env("ROS_DOMAIN_ID", "0")` in Rust. Fixing one language left
# the failure alive in the next, one file over.
#
# THE RULE
#
# Under the paths below, a domain comes from one of the three assigners — all of
# which resolve `ROS_DOMAIN_ID` first (pinning is how you reproduce by hand) and
# otherwise derive a per-process value:
#
#   Rust   nros_tests::unique_ros_domain_id()
#   shell  nros_unique_ros_domain_id      (tests/ros2_e2e_common.sh)
#   C++    nros_test_domain()             (tests/nros_test_domain.h)
#
# NOT a ban on the string `ROS_DOMAIN_ID`: passing an assigned value through the
# environment is the point. What is banned is a LITERAL as the value.
set -euo pipefail
cd "$(dirname "$0")/.."

fail=0

# `git grep`, not a filesystem walk (check-no-tracked-file-find).
#
# Allowlisted, with the reason in the file at the site:
#   px4_xrce_e2e.rs — must match PX4's own uxrce_dds_client default, which is
#     configured inside PX4, not from our side. The XRCE path is keyed by the
#     agent's UDP port, which IS unique per run.
#   ros_env.rs — unit tests asserting on a generated script's text; the literal
#     is the expected OUTPUT, not a domain anything joins.
# Two patterns, two scopes.
#
# 1. A literal passed as ROS_DOMAIN_ID, anywhere a test spawns a process.
# 2. A literal as `create_session`'s THIRD argument (the domain) — scoped to the
#    cyclone tests, because that argument only names a DDS bus there. uORB has
#    no domain concept, and XRCE keys its session by the agent locator, so a 0
#    in those smoke tests is not a shared bus. Matching the third argument
#    specifically also keeps the SECOND one (a length, always 0) out of it — the
#    first draft of this gate flagged every already-fixed site for that reason.
env_literals="$(git grep -nE '\.env\("ROS_DOMAIN_ID", *"[0-9]+"\)|ROS_DOMAIN_ID="?\$\{ROS_DOMAIN_ID:-[0-9]+\}' \
    -- packages/testing packages/rmw \
    | grep -v 'px4_xrce_e2e.rs' \
    | grep -v 'src/ros_env.rs' \
    || true)"
session_literals="$(git grep -nE 'create_session\([^,]+, *[^,]+, *[0-9]+ *,' \
    -- packages/rmw/cyclonedds \
    || true)"
literals="$(printf '%s\n%s' "$env_literals" "$session_literals" | grep -v '^$' || true)"

if [ -n "$literals" ]; then
    echo "ERROR: a test names its ROS domain with a literal:" >&2
    echo "$literals" | sed 's/^/  /' >&2
    echo "" >&2
    echo "  A named domain is a SHARED BUS: two concurrent runs join it and the" >&2
    echo "  collision surfaces as wrong data, not as a collision (issue 0580)." >&2
    echo "  Assign one instead:" >&2
    echo "    Rust   nros_tests::unique_ros_domain_id()" >&2
    echo "    shell  nros_unique_ros_domain_id   (tests/ros2_e2e_common.sh)" >&2
    echo "    C++    nros_test_domain()          (tests/nros_test_domain.h)" >&2
    fail=1
fi

# The three assigners must keep agreeing. One scheme in three languages is only
# worth having if it stays one scheme.
for f in packages/testing/nros-tests/src/lib.rs \
         packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/ros2_e2e_common.sh \
         packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/nros_test_domain.h; do
    [ -f "$f" ] || {
        echo "ERROR: $f missing — the assigner set is stale" >&2
        fail=1
        continue
    }
    grep -q '232' "$f" || {
        echo "ERROR: $f no longer folds into the shared 1..=232 range" >&2
        fail=1
    }
done

if [ "$fail" -ne 0 ]; then
    exit 1
fi

echo "test domain assignment: OK (no literal domains; 3 assigners agree)"
