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

# issue 0726 — both arms below turn a `grep -qE` STATUS into a claim about an
# assigner file, and they fail in OPPOSITE directions: a grep that never ran
# reports the shared range as unmet on the first arm, and silently drops the
# modulo-232 check on the second. `nros_grep_q` exits 2 rather than either.
# HERESTRINGS, not pipes — a pipeline segment is a subshell and would swallow
# that exit.
# shellcheck source=scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

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

# The three assigners must keep agreeing, on the SAME ceiling. One scheme in
# three languages is only worth having if it stays one scheme — and issue 0703
# is what the ceiling itself is for.
#
# Cyclone derives its RTPS ports from the domain (`7400 + 250*D` multicast,
# `+10 + 2*participantIndex` unicast). Linux hands out ephemeral ports from
# 32768, so `7400 + 250*102 = 32900` is a port the OS may already have given
# away: from domain 102 up the bind fails outright and the session never opens.
# That is a test failing for a reason having nothing to do with what it tests,
# at a rate set by how busy the box is — 0703 was ~2-in-5 inside `just check`,
# 0-in-4 solo, on a different test each time.
#
# 101 is the last safe value with margin (`7400 + 250*101 + 11 + 2*9 = 32679`)
# and is the range ROS 2 documents as safe on Linux. Measured with 32768-34000
# held: D=100 ok, D=101 ok, D=102 bind failure, D=103 bind failure.
#
# This checks the CEILING, not merely that the three files agree: agreeing on
# 232 is exactly the bug.
NROS_TEST_DOMAIN_MAX_EXPECTED=101
for f in packages/testing/nros-tests/src/lib.rs \
         packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/ros2_e2e_common.sh \
         packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/nros_test_domain.h; do
    [ -f "$f" ] || {
        echo "ERROR: $f missing — the assigner set is stale" >&2
        fail=1
        continue
    }
    # Comments in these files legitimately DISCUSS the old range — that record
    # is why the ceiling exists, so it must survive. Strip comments before
    # reading the code, or the gate flags its own documentation (it did).
    # `#` is a comment in the shell file and a PREPROCESSOR directive in the
    # header, so it is only stripped when what follows is not a directive.
    code="$(sed -E \
        -e 's@^[[:space:]]*//.*$@@' \
        -e 's@^[[:space:]]*\*.*$@@' \
        -e 's@^[[:space:]]*#([[:space:]].*|$)@@' \
        -e 's@^[[:space:]]*#[^dei].*$@@' "$f")"
    if ! nros_grep_q -E \
        "(TEST_DOMAIN_MAX.*[^0-9]|% *)${NROS_TEST_DOMAIN_MAX_EXPECTED}([^0-9]|\$)" <<<"$code"; then
        echo "ERROR: $f does not fold into the shared 1..=${NROS_TEST_DOMAIN_MAX_EXPECTED} range" >&2
        echo "  A domain above ${NROS_TEST_DOMAIN_MAX_EXPECTED} puts Cyclone's RTPS ports inside Linux's" >&2
        echo "  ephemeral range (32768+), so the bind can fail and the session never" >&2
        echo "  opens — a red that tracks machine load, not code (issue 0703)." >&2
        fail=1
    fi
    if nros_grep_q -E '% *232([^0-9]|$)' <<<"$code"; then
        echo "ERROR: $f folds modulo 232 — that is the issue-0703 range" >&2
        fail=1
    fi
done

if [ "$fail" -ne 0 ]; then
    exit 1
fi

echo "test domain assignment: OK (no literal domains; 3 assigners agree on 1..=${NROS_TEST_DOMAIN_MAX_EXPECTED})"
