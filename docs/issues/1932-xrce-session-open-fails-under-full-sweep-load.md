---
id: 1932
title: "Two XRCE clients failed to open their Agent session in one tier-1 sweep on newslab-server-241, and pass 20 of 20 solo"
status: open
type: bug
area: [rmw-xrce, testing, ci]
severity: low
found: 2026-10-17
related: [0741, 1365]
---

## Seen

run-matrix **38818350050**, job **116843072656** `tier 1 (cells)` on
`nano-ros-runner-newslab-server-241`, 2026-10-16 ~09:44Z. The junit rewrite
counted 2 real failures, both XRCE clients that never got a session from the
Agent the test started for them:

- `c_xrce_api::test_c_xrce_listener_starts` (14.3 s):
  `rclc_support_init(&app.support, argc, argv, &allocator) -> -1`, before
  `Support initialized`.
- `bridge_mixed_rmw::test_zenoh_to_xrce_bridge_e2e` (16.9 s): the zenoh half
  opened; the XRCE half panicked
  `egress Node (XRCE session open): Transport(Backend("rmw_ret error"))`.

Both runs used the SDK-pinned Agent
(`~/.nros/sdk/xrce-agent/2.4.3-nros1/bin/MicroXRCEAgent`). In the same sweep
39 other XRCE tests passed, including `test_c_xrce_talker_starts`, the
listener's twin, which ran beside it.

## Not reproduced

- The next tier-1 run, **38861323075** on `newslab-118-server`, passed both.
  Its only real failure was `rmw_coordinate_truth`, fixed by #2317.
- On newslab-118, on fixtures rebuilt at `c43a903140`: both tests 10 times
  each, run solo, 20 of 20 passed.

So the failure needs the full sweep, at the lane's `-P32`, or that host. That
is the shape CLAUDE.md warns about for QEMU lanes ("flake under load, retest
SOLO"), here on the host XRCE path.

## Not established

- Whether the Agent was not yet accepting when each client sent its
  CREATE_CLIENT. The `XrceAgent` fixture waits for the UDP bind; the
  `bridge_mixed_rmw` comment measured a further settle the Agent needs.
- Whether the client's session-open retry budget is shorter than an Agent's
  start-up on a loaded host.
- Whether it is specific to 241. That runner had just been brought back after
  issue 1365, and this was its first tier-1 sweep.

## Next

Watch tier-1 runs for a repeat. If one appears, keep the Agent's own log for
that test (the fixture captures it), and compare the client's session-open
timeline with when the Agent printed its bind line.
