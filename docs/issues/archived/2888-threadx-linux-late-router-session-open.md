---
id: 2888
title: "ThreadX-Linux: `test_rtos_session_open_outlasts_a_late_router` fails in the nightly"
status: wontfix
type: bug
area: [threadx, testing, rmw-zenoh]
severity: medium
found: 2026-11-23
resolved: 2026-11-23
resolved_in: "duplicate of issue 2777, whose fixes landed after both red nightlies"
related: [phase-590, 2777, 2896]
---

## What fails

The nightly's `ThreadxLinux` job (run 44988007873, 2026-11-22, main `98b5d8e0c7`):

```text
FAIL [  84.837s] (11/12) nros-tests::rtos_e2e test_rtos_session_open_outlasts_a_late_router::platform_1_Platform__ThreadxLinux
Summary [ 167.248s] 12 tests run: 11 passed (1 slow), 1 failed, 28 skipped
```

The failed-step log ends at nextest's summary; the test's own output is in the job's full log,
not yet read.

## Next

Read the full job log. Check whether the same test failed on the earlier nightlies
(2026-11-21 `7660227bcf` was also red) or only this one. Per CLAUDE.md "No retries", a start race is
fixed at its readiness edge, never re-rolled.

## Acceptance

Green in a scheduled nightly. A release blocker:
[phase-590](../../roadmap/phase-590-nros12-release-plan.md) W1.

## Resolved (2026-11-23): a duplicate of issue 2777

Both red nightlies (2026-11-21 `7660227bcf`, run 44869354157, and 2026-11-22
`98b5d8e0c7`, run 44988007873) fail with issue 2777's signature, read from the
full job log:

```text
parameter store refused: needs 283392 bytes in one allocation (32 slots x 8856)
Application error: Capability { name: "param_services", reason: "Transport::BadAlloc" }
```

All four of issue 2777's fixes (`2ca3d02bd0`, `50c4326515`, `2e417c5253`,
`54034ad1aa`) were committed at 2026-11-22 09:47Z, after the 11-22 nightly
started at 05:07Z, so neither red run had them. The first nightly with them,
2026-11-23 `a1cbbb6b04` (run 45115997343), PASSES the test:

```text
PASS [  72.528s] (10/12) nros-tests::rtos_e2e test_rtos_session_open_outlasts_a_late_router::platform_1_Platform__ThreadxLinux
```

That job is still red, on a different step (`test-c-port`): the ThreadX fork's
Linux port hung on a below-floor timer-thread stack. That is
[issue 2896](2896-threadx-linux-port-below-floor-stack-hangs.md),
fixed in the same pull request.
