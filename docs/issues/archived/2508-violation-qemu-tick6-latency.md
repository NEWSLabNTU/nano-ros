---
id: 2508
title: "The violation-channel QEMU cell stored a second latency verdict: tick 6's dispatch measured 189 ms against the 180 ms callback row on /t4/answer that no dispatch was meant to reach"
status: resolved
type: bug
severity: low
area: [testing, zephyr, qemu, monitors]
related: [issue-2273, issue-2389, phase-544, phase-474]
found: 2026-11-13
resolved_in: "p2508-violation-margin branch: tick 6's overrun 150 -> 110 ms, so both sides of the 180 ms react budget keep ~30 ms"
---

**Home: [phase-544](../../roadmap/archived/phase-544-harness-run-plan-followups.md)** (the QEMU leg, issue 2273).

## What

`violation_channel_e2e::zephyr_cortex_m_cpp_violation_channel_reports_one_overrun_after_arming`
(fixture `workspace-zephyr-cortex-m-cpp-violation`, `examples/workspaces/violation-cpp`
on mps2/an385 under `-icount shift=auto`) failed deterministically, 3/3 solo, in the
tier-2 catch-up run on main 75f2d3857d. It expects exactly one `max-latency-runtime`
verdict after arming and got two:

```
contract violation #2: max-latency-runtime /handler/answer measured=189 declared=180
contract violation #3: max-latency-runtime /handler/state measured=189 declared=50
```

The second line is the expected one. The first is the CALLBACK-level 180 ms row the
contract installs on `/t4/answer` beside the `react` route row. The contract's own
comment says no single dispatch reaches that row. Tick 6's dispatch is a 150 ms busy-wait
plus three publishes and a printf. On 2026-11-09 (bf9fccba83) it measured 164..177 ms,
so the row kept 3..16 ms of headroom. On 2026-11-13 it measured 189 ms.

## Why: a margin, not a regression

The overhead around the busy-wait is ~37..39 ms of guest time on mps2/an385 under icount
(the three zenoh publishes over SLIRP and the console). A 150 ms overrun therefore left
the dispatch inside 180 ms only while that overhead stayed under 30 ms. Measured,
all 3/3 deterministic per build:

| build | zenoh-pico | tick-6 dispatch |
| --- | --- | --- |
| bf9fccba83 (passed 2026-11-09 at 164..177) | cf228437 | 189, 215, 189 |
| af1a14d8b5^ | 6827fe1f | 189 (an earlier 3/3 PASS here did not reproduce) |
| af1a14d8b5 | 15f3b012 (bounded TCP write) | 189 |
| af1a14d8b5, Zephyr TCP write reverted to blocking `send()` | 15f3b012 minus the write | 187 |
| 121a6a99c6 + overrun 120 ms | fb5f5cdc | 157 |
| 121a6a99c6 + overrun 110 ms | fb5f5cdc | 147 |

So the bisect boundary first reported here (the zenoh-pico bump in af1a14d8b5) was
not real. Reverting the bounded write moves the dispatch by 2 ms, and the parent
commit measures the same 189 ms once rebuilt. Ruled out on the way: the QEMU command
line (the board's `[board.run.qemu]` was identical at both ends) and
`CONFIG_NROS_CONSOLE_WHOLE_LINES` (SMP boards only). No single commit owns the drift.
The row had too little headroom for any of them.

## Fix

Move the workload, not the contract (the 180 ms stays). Tick 6's overrun is 110 ms:

- the callback row on `/t4/answer`: mps2/an385 147 ms against 180 (33 ms headroom);
- tick 6's route, which must still exceed 180: native_sim 210 ms (30 ms over; the test's
  floor is 181);
- tick 10's route: native_sim 250 ms (unchanged);
- the `/t4/state` 50 ms verdict still fires (147 ms), and the test's `measured >=`
  floor moves to 110.

Both legs 3/3 (QEMU under tier-2 scope, native_sim unscoped).

## Other rows near their bound (the sweep)

`violation-cpp` is the only violation workspace, and `zephyr_mps2` its only QEMU image.
Its other rows:

- `max-latency-runtime /handler/state` (50 ms) on an ON-TIME tick: not printed (a row
  that does not fire reports no value). Derived from tick 6: dispatch minus busy-wait is
  ~37 ms on mps2/an385, so an ordinary tick sits ~13 ms under its 50 ms bound. Its
  workload is the three publishes and one printf the fixture exists to do, so there is
  nothing to shorten without changing what the fixture tests. LEFT AS IS, listed here.
  A dispatch past 50 ms on an on-time tick would be a second stored verdict before the
  window end, which is exactly this failure's signature.
- tick 10's timer path: already known (60 ms against 50 in 1 of 6 runs, 2026-11-09),
  and the test excludes it by ending its window at `[handler] took avail 9`.
- `route-latency-runtime /handler/answer` (180 ms): fires by design on ticks 6 and 10.
  An on-time route is one period (~100 ms), 80 ms under the bound.
