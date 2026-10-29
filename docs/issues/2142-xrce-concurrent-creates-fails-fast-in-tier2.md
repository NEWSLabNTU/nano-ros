---
id: 2142
title: "`xrce_concurrent_entities` failed in 0.169 s in the 2026-10-29 tier-2 run after passing in 5.4 s the night before; the lane suppresses failure output, so the panic text was never recorded"
status: open
type: bug
area: [rmw-xrce, ci]
severity: medium
found: 2026-10-29
related: [1734, 2106, 2120, 2135, 2108]
---

## What

Scheduled `run-matrix` run 40828659447 (2026-10-29 06:20 UTC, main at
`2441c437a`), job 123297497925 `tier 2 (1-wise matrix)`, ended with exactly one
real failure:

> FAIL [   0.169s] (1/4) nros-tests::xrce_concurrent_entities concurrent_creates_on_one_xrce_session_succeed_on_distinct_slots
>
> Real failures: 1 / 1 total failures

The feature lane that runs it (`.config/feature-lanes.toml`, `features =
["xrce-wake-test"]`) printed `==> feature lane FAILED (rc=1)`. The other three
tests in the same binary set passed against the same Agent:
`xrce_guard_wake`, `xrce_entity_wake` and `xrce_deadline`.

The previous night's tier-2 run 40643270547 (main at `e0f81b4ac`) passed the same
test:

> PASS [   5.432s] (3/3) nros-tests::xrce_concurrent_entities concurrent_creates_on_one_xrce_session_succeed_on_distinct_slots

## Why the cause is not in this issue

The lane runs nextest with `--failure-output never`, so the log has no panic
message. The run uploads no junit for this lane: `post-submit-junit` holds the
main nextest run only. This sweep host has no XRCE Agent, so I could not
reproduce it locally. The failure is therefore observed but not diagnosed.

What the shape suggests, without claiming it: 0.169 s is far below the 5.4 s
of a passing run, so the test probably failed early in setup (session or node
creation, or the first create) rather than in the concurrent phase. An Agent
that was missing would have failed all four tests, not one, so the Agent was
present.

## Candidate commits

These XRCE commits landed between the green night and the red one
(`e0f81b4ac..2441c437a`, scoped to `packages/rmw/xrce`, the test, and the
fixtures):

- `2fc7f2c4fc` feat(#2106): XRCE reads node names from ros_discovery_info through the Agent
- `8d6493df05` feat(#2108): XRCE emulates both DEADLINE events client-side
- `3fe682f534` fix(issue 2120): create XRCE requesters/repliers from XML so their QoS applies
- `94e6fde050` feat(issue 2135): report received_timestamp on zenoh and XRCE takes
- `f326a1eba5` fix(issue 2132): lease the port before spawning the never-binds stand-in

#2106 and #2120 are the likeliest by shape. #2106 makes `graph.c` create a
`ros_discovery_info` reader on first use, taking an endpoint key from the
issue-1292 count. #2120 changes how endpoints are created on the Agent. Either
could change what an early create on a fresh session sees. This is a lead, not
a finding.

## What it is NOT

- **Not issue 1734 coming back as described.** 1734 was a race between threads
  (slot claims). A 0.17 s failure does not fit a race that needs four threads
  to reach the barrier.
- **Not an environment gap.** `require_xrce_agent` uses `unmet!`, which counts
  as a real failure, but the sibling tests passed against the same Agent.

## What would close it

1. Run the lane with failure output on, either locally with the Agent
   (`just xrce setup`, then `just _nextest-tolerant -p nros-tests --features
   xrce-wake-test --test xrce_concurrent_entities`) or by setting
   `--failure-output final` for this lane, and record the panic here.
2. Bisect the candidate commits above if the message does not name the cause.
3. Close when the test passes on a tier-2 run again, with the fix commit named.

A smaller, separate fix: a feature lane that suppresses failure output turns
every red into a no-verdict. The lane should print failure output, or upload
its junit, so the next red carries its own diagnosis.
