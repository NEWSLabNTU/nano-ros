---
id: 3026
title: "`xrce_guard_wake` held a correct executor to an absolute 3 ms on one sample, which a loaded runner exceeds"
status: resolved
type: test-flake
severity: low
area: [xrce, testing, executor]
related: [phase-521, rfc-0113]
found: 2026-11-28
resolved: 2026-11-28
resolved_in: "the change that adds this line"
---

## What happened

`nros-tests::xrce_guard_wake guard_trigger_wakes_an_xrce_executor_parked_on_its_socket`
went red on 241's tier-1 cells in scheduled run 46115819794 (job
140908682517) while the runner host was loaded:

```text
trigger to dispatch took 4.54927ms (bound 3ms) — the executor waited inside the backend's recv (sliced or whole) instead of parking on the session's socket
```

The test had not changed since 11-02. It measured ONE trigger-to-dispatch
latency and compared it with an absolute 3 ms. A correct executor costs
~90 µs on an idle host. What load inflates is the OS's own wake latency,
which a correct executor rides on, so one slow wake failed it.

## Reproduced

On host 139 (32 cores, load ~18 from other sessions plus 48 busy loops of
mine, ~35 at the peak), 10 runs each, interleaved:

- the old test failed 1 of 10 (`3.317585ms (bound 3ms)`), every other run
  between 66 and 148 µs;
- the new test passed 10 of 10, with medians 61–89 µs against an allowance
  of ~1.02 ms.

## The claim, kept

The test exists to show that the executor PARKS on the XRCE session's
socket plus its own wake object, so a trigger ends the park at once. The two
waits it rules out both put the trigger behind a receive timeout:

- **A whole receive-blocking wait** dispatches at the end of the spin's
  budget: ~975 ms after a trigger 25 ms into a 1000 ms spin.
- **The retired 10 ms guard slice** dispatches at the next slice boundary,
  uniform over 0–10 ms, ~5 ms at the median.

## Fix

The test now measures nine trials. Each is paired with a control: a thread
parked on a futex, woken by a thread that slept the same delay. That is the
OS wake path a correct executor takes, measured under the same load at the
same moment.

- **The whole-recv wait** is ruled out by a separation, on every trial. A
  dispatch past 250 ms is two orders past any wake and four times short of
  what that wait would give.
- **The slice** is ruled out by a ratio of medians. The executor's median
  wake may cost 4× the median control wake plus 1 ms for its own dispatch
  (the ready-set scan and the guard callback). Idle, that allows ~1.02 ms
  against the slice's ~5 ms. Under load the allowance grows with the control
  instead of failing a correct executor.
- **The lower bound stays** on every trial: the spin waited for the trigger,
  so no upper bound passes vacuously.

## Negative control

A temporary 5 ms sleep after a signalled park in `node_wake.rs`
(`park_handles_us`, never committed) stands in for a slice boundary. The new
test failed twice out of two runs:

```text
the median trigger-to-dispatch was 5.127882ms, against a median OS wake of 5.209µs measured beside it (allowed 4 x that + 1ms = 1.020836ms)
```

Reverted, it passes again (median 80 µs).
