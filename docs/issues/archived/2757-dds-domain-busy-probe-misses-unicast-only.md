---
id: 2757
title: "The test domain assigner read a domain held by a unicast-only DDS workload as free, and a Cyclone test then found no free participant index"
status: resolved
type: flaky-test
severity: medium
area: [testing, cyclonedds, dds]
related: [issue-0707, issue-1762, issue-1009, issue-0196]
found: 2026-11-20
resolved: 2026-11-20
resolved_in: "the change that archives this issue"
---

## What happened

On 2026-11-20 a full `just ci gate` went red at step 8 (`check::build`). The
failing gate was `check::rmw-cyclonedds`, test
`nros_rmw_cyclonedds_ros2_pubsub_e2e`:

    ros2: Failed to find a free participant index for domain 4
    [rmw_cyclonedds_cpp]: rmw_create_node: failed to create domain

Run solo, the suite passed 39/39.

## Why

Another workload on the host held domain 4: a user's `play_launch` scenario
run with five DDS processes. It bound the unicast participant ports
8410-8429 (`7400 + 250*4 + 10 + 2*index`) and NOT the multicast discovery
port 8400.

All three spellings of the "is somebody on domain d" probe looked at 8400
only:
- `nros_tests::domain_discovery_port_busy`;
- `nros_domain_busy` in `ros2_e2e_common.sh`;
- `nros_test_domain_busy` in `nros_test_domain.h`.

A unicast-only participant (`AllowMulticast=false`, which is also what this
repo's own pinned profile sets) never binds that port. So the domain read as
free, the test claimed it, and Cyclone found every participant index taken.
The probe's reach was narrower than its rule (the 0196 shape).

Measured on the live squatter:
- the old shell probe answered "domain 4 free";
- the fixed one answers "busy".

## Resolution

All three probes also treat participant index 0's unicast metatraffic port
(`7400 + 250*d + 10`) as busy. Every participant binds its index's unicast
ports, and index 0 is the first taken. So any user of the domain, multicast
or not, is seen.

- Test `a_unicast_only_participant_reads_as_busy` binds only that port on an
  idle domain and asserts busy. Mutation: dropping the unicast port turns
  it red.
- `just check rmw-cyclonedds` passes 39/39 with the squatter still on
  domain 4.
