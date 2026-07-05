---
id: 133
title: "Several ros2-interop tests soft-pass when 0 messages are received — violates the tests-must-fail-on-unmet-preconditions rule"
status: resolved
type: tech-debt
area: testing
related: [phase-277]
---

## Summary

Several tests in `packages/testing/nros-tests/tests/` around ROS 2 / bridge
interop treat "0 messages received" as a pass (log-and-return instead of
`assert!`/`nros_tests::skip!`). CLAUDE.md's testing contract: tests must fail
(or explicitly skip) on unmet preconditions — a bare early-return reports PASS
and hides broken interop.

Found during phase-277 W4 while retargeting chatter literals
(`cyclonedds_ros2_interop.rs`, `rmw_interop.rs` and siblings — see
`tmp/sdd-277/task-9-report.md` for the specific soft-pass sites). Pre-existing
pattern, not introduced by the wave; the wave only fixed the literals.

## Next steps

1. Grep those files for early-return-on-empty patterns; convert each to
   `nros_tests::skip!` (when the missing piece is environment, e.g. no ros2)
   or a hard assert (when the SUT should have delivered).
2. Re-run the interop lane where ros2 + rmw_zenoh are provisioned to confirm
   which now legitimately skip vs fail.

## Resolution (2026-07-06)

All soft-pass sites were in `tests/rmw_interop.rs` (the named siblings —
`cyclonedds_ros2_interop.rs`, `xrce_ros2_interop.rs`, `demo_nodes_cpp_interop.rs`,
the `bridge_*` — already used hard `assert!(received >= N, …)`). Twelve sites
converted:

- pub/sub (`test_nano_to_ros2`, `test_ros2_to_nano`) and the matrix's
  `_inner` helpers: `if received > 0 {PASS} else {INFO "may be timing"}` →
  `assert!(received > 0, …)`; ros2-process launch `Err` in the helpers →
  `nros_tests::skip!` (environment gap, not a delivery failure), so a `false`
  from an inner can only mean 0-delivered.
- data-integrity check (`Hello World: 42`) promoted from a soft `if contains`
  to a hard `assert!`.
- service (nano-server↔ros2-client, ros2-server↔nano-client), action
  (ros2-server↔nano-client): soft `else {INFO}` → `assert!`.
- QoS matrix: `if received == should_work {PASS} else {INFO}` →
  `assert_eq!`; the RELIABLE arm's bare `[SKIP]` log → `nros_tests::skip!`.
- `test_qos_compatibility`, and the latency / throughput benchmarks: 0-received
  "[INFO] inconclusive" / silent-`0.0 msg/sec` → `assert!` (all gated behind
  `require_ros2()`, so 0 is a real SUT failure, not environment).

Every converted site sits AFTER a `require_ros2()` skip guard, so provisioning
gaps still skip cleanly; only genuine delivery failures now fail loud, per the
CLAUDE.md testing contract.

The conversion immediately did its job: it surfaced a real, previously-hidden
**ROS 2 → nros** delivery gap (`ros2 topic pub` → native nros listener receives
0 while nano → ROS 2 works), reproduced by `test_ros2_to_nano` +
`test_communication_matrix::case_3`. Filed as **#146**. The `test_qos_matrix`
BEST_EFFORT→RELIABLE cell was NOT hard-asserted: that encodes DDS request-vs-
offered incompatibility, but zenoh's reliability model is looser (a BEST_EFFORT
publisher may still reach a RELIABLE subscriber), so it only asserts the
delivery-expected cells; the RELIABLE-publisher cells `skip!` (nros talker has
no RELIABLE support yet).
