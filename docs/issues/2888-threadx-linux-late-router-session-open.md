---
id: 2888
title: "ThreadX-Linux: `test_rtos_session_open_outlasts_a_late_router` fails in the nightly"
status: open
type: bug
area: [threadx, testing, rmw-zenoh]
severity: medium
found: 2026-11-23
related: [phase-590]
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
[phase-590](../roadmap/phase-590-nros12-release-plan.md) W1.
