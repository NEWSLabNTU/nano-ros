---
id: 133
title: "Several ros2-interop tests soft-pass when 0 messages are received — violates the tests-must-fail-on-unmet-preconditions rule"
status: open
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
