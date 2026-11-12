---
id: 2479
title: "`a_termination_signal_ends_a_long_park` takes its start time after spawning the thread that sends SIGTERM, so a correct 100 ms wake can measure 99.99 ms and fail"
status: resolved
type: bug
area: [platform, testing]
severity: low
found: 2026-11-13
related: [issue-0499]
---

## Measured

`just ci gate`, step `test-unit`, on the phase-535 W8 branch (an unrelated
change), 2026-11-13:

    FAIL nros-platform::termination_wake a_termination_signal_ends_a_long_park
    a SIGTERM at 100 ms ended a 5 s park after 99.991147ms

## Cause

`packages/platform/nros-platform/tests/termination_wake.rs` spawned the killer
thread (sleep 100 ms, then `kill(SIGTERM)`) and only then took
`let start = Instant::now()`. The killer's 100 ms starts at its own spawn, so
`start` can be later than the killer's clock, and the park's measured
`elapsed` falls short of the `>= 100 ms` lower bound by the spawn-to-`start`
gap. The wake itself was correct. This is a start race in the test, not a
platform defect, and a retry would only hide it.

## Fix

Take `start` before spawning the killer. The lower bound then holds by
construction: the signal cannot be sent until 100 ms after `start`.
