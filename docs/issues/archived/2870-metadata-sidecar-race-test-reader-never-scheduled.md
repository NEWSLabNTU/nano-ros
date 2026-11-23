---
id: 2870
title: "`two_writers_staging_one_component_leave_a_whole_stamped_sidecar` fails under load: its reader thread can be scheduled after both writers finish"
status: resolved
type: bug
severity: low
area: [cli, testing]
related: [issue-2827]
found: 2026-11-23
---

## What happens

The nros-cli-core unit test
`orchestration::metadata_refresh::tests::two_writers_staging_one_component_leave_a_whole_stamped_sidecar`
(issue 2827's regression test) spawns one reader thread and two writer
threads. Each writer writes and stamps 200 times, then the test stops the
reader and asserts it read at least once (`"the reader ran"`).

Nothing orders the reader before the writers. Under load the reader thread
can first be scheduled after both writers have finished and `stop` is set, so
it never enters its loop and the test fails:

```
panicked at nros-cli-core/src/orchestration/metadata_refresh.rs:3585:9:
the reader ran
```

Measured 2026-11-23 in a full `just ci gate` of PR #4628 at 83cd7fa096
(2841 passed, 1 failed); the same suite passed in the `just check cli-tests`
run right after it. The PR does not touch this file. The race is in the test,
not in the code under test: a reader that never ran checked nothing.

## Fix

A `Barrier` of three. The reader reads once, then waits at the barrier; the
writers wait at it before their first write. So the reader has read at least
once before any writer starts, and it keeps reading until `stop`. This is a
start-order fix, not a retry.

## Resolution

Fixed in the test (the barrier above).
