---
id: 2935
title: "clock-board-freertos's board-late case holds a tick-measured duration to a lower bound, which fails under host load"
status: open
type: bug
severity: low
area: [testing, freertos, clock]
related: [phase-582, issue-2866]
found: 2026-11-24
---

## What happens

`tests/clock-board-freertos-tests.sh`, case `board-late`: the stub hook
answers only 200 ms after its first call, and the case checks the port's own
line `nros: clock resolution took <n> ms` with `took board-late 180`, i.e.
`n >= 180`. That `n` is measured on the image's monotonic clock, which on the
FreeRTOS POSIX kernel port is the TICK count. Ticks come from a host timer
signal, and under host load the kernel delivers fewer of them than real time
passed. So the tick-measured duration comes out SHORT, and a lower bound on
it fails even though the wait was correct.

Measured on host 139 on 2026-11-24 at a load average of 64 on 32 cores: two
runs reported 132 ms and 170 ms. In the same runs the case's own host-clock
check, "the point waited for the board (>= 200 ms)", passed. The code under
test waited correctly, and only the tick-measured report was short. The case
does not reach issue 2866's code, which was being tested at the time.

## Why it matters

A lower bound on a duration counted in simulated ticks is a known flake
shape: under load the guest clock lags, so the bound fails on correct code.
The same case already checks the wait on the HOST clock, which is the
measurement that means "it waited".

## Fix options

1. Drop the minimum on the `took` line for `board-late` and keep the
   once-only check (`took board-late`). The host-clock `>= 200 ms` check
   already says the point waited.
2. Keep a lower bound only where ticks cannot run behind, which on this port
   is nowhere.

Option 1 is proposed. `board-never` holds its `took` line to 280 ms, so it
has the same shape and needs the same decision.

## Acceptance

`just check clock-board-freertos` passes at high host load, and the
host-clock wait checks still fail the inert-point negative control.
