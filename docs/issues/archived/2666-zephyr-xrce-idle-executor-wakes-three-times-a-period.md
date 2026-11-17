---
id: 2666
title: "Zephyr mps2/an385 XRCE idle executor is switched in about 3 times a timer period, against a bound of 2"
status: resolved
type: bug
severity: medium
area: [zephyr, xrce, executor, testing]
related: [phase-521, 2252, 2608]
found: 2026-11-17
resolved_in: "fix-2666-zephyr-xrce-idle-wakes: the witness counts wakes, not switch-ins"
---

**Home: [phase-521](../../roadmap/archived/phase-521-rt-unified-wait.md)** (RFC-0113 D2).

## What

`zephyr_cortex_m_qemu::zephyr_cortex_m_idle_xrce_image_wakes_only_for_its_deadlines`
fails every run:

```
idle-wake: switch_ins=15 fires=5 ...
the executor's thread woke 15 times over 5 timer periods
```

The assertion is `switch_ins <= 2 * fires + 2`, which is 12 for 5 fires. So the
executor thread is switched in about 3 times a period, not the 1 a deadline
needs. Reported by another session, which measured:

- The same count at `1037efabd0`, before its own change. The red pre-dates that change.
- 16 with `NROS_EXECUTOR_MAX_CBS=4`. Callback-table sizing is not the cause.
- The zenoh twin, `…_idle_zenoh_image_…`, passes on the same board.

The cell was last green in the issue 2252 verification (`max_late_us=0`; the
switch-in count was not recorded there). Since `3eefb44f93` (2026-11-10)
the witness has been a workspace row that no lane built, so nothing ran it.

## What the count counts

The witness (`packages/testing/nros-tests/bins/idle-wake-zephyr-mps2/…/Witness.c`)
increments `switch_ins` in Zephyr's `sys_trace_thread_switched_in_user` hook
whenever the executor thread is the one switched in. That counts every
switch-in: a wake from a blocked wait, and also a resumption after the
thread was PREEMPTED while it was runnable. RFC-0113 D2.3 bounds the first.
The second is not a wake. So the 15 is the sum of two different things until
it is split.

## Where an XRCE wake comes from on Zephyr

XRCE has no thread of its own. On Zephyr its readiness is the zsock UDP
descriptor (`nros_platform_udp_readiness_handle`, WAITABLE under
`CONFIG_ZVFS_EVENTFD`). The executor parks in
`nros_platform_wake_park_handles_us` (`nros-platform-zephyr/src/platform.c`):
a `zsock_poll` over the wake object's eventfd plus that descriptor, with the next
deadline as the timeout. It then drains the session non-blockingly. The zenoh twin
waits on a semaphore, and zenoh's own read task does the socket work.
That is the structural difference between the passing and failing cells.

Candidates, none measured yet:

1. **Agent traffic.** A frame from the Agent (a reliable-stream HEARTBEAT, an
   ACKNACK) makes the socket readable. That is an RX wake that carries no data
   the sched task owns. D2.3 allows a wake only for owned data or a deadline.
2. **The drain's own send.** The non-blocking drain runs the uxr session, which
   can send (an ACKNACK, a heartbeat for the reliable output stream). A
   `zsock_sendto` hands the frame to the higher-priority net TX thread, which
   preempts the executor. That is one extra switch-in each time, and not a wake.
3. **A loop inside the park.** `zsock_poll` returning with only the eventfd ready
   and the semaphore already taken (the "stale" path) loops again. So does
   Zephyr's own `zvfs_poll` re-polling after a `k_poll` return that found no
   ready descriptor. Each pass is a switch-in.

## Measured (2026-11-17, host 139, at `1037efabd0` plus instrumentation)

Instrumentation, never committed for merge, lives on the local branch
`wip-2666-instrument`:

- The witness's `switched_out` hook records the thread state of the executor
  thread as it leaves. PENDING/SUSPENDED/SLEEPING means it blocked; QUEUED
  means it was still runnable.
- For a runnable switch-out, the hook names the next thread switched in, if
  any.
- `nros_platform_wake_park_handles_us` counts which source ended each park.

Two runs gave `switch_ins=14` and `switch_ins=15`, each over `fires=5`.

| Switch-ins in the window | Count | Cause |
|---|---|---|
| after a BLOCKED switch-out (`state=0x02`, pending in `zsock_poll`) | **5** | one per deadline, at t ≈ 999/1999/2999/3999/4999 ms |
| after a RUNNABLE switch-out, with another thread run in between | ≤ 2 | Zephyr's net RX traffic-class thread (`rx_classes`, cooperative, prio −1) handling the Agent's frames; 4 LAN9118 IRQs in the window |
| after a RUNNABLE switch-out, with no other thread switched in | the rest (7–8) | a reschedule pass where the hooks fire and the same thread continues |

Park outcomes over the same window: 8 parks, 5 ended by the timeout (the
deadlines) and 3 by the socket. No socket-ended park produced a blocked
switch-in, so in each of those 3 the frame was already readable when the
park began, and the thread never slept. The eventfd, its stale-loop path and
EINTR fired 0 times. The semaphore park was never called.

All non-blocked switch-ins fall in the same millisecond as a deadline
dispatch: the executor is running when they happen.

**So RFC-0113 D2.3 holds on this image.** The executor wakes from a blocked
wait exactly once per deadline, and never for the Agent's session traffic.
Candidates 1 and 3 above are ruled out. Candidate 2 is real but small (≤ 2). The
red is the witness: it counts every switch-in, so it counts each preemption,
and each reschedule pass where no other thread ran, as a wake. The zenoh twin
passes because fewer of those land in its window, not because it wakes less.

## Fix

Fix the witness, not the bound. Count WAKES, meaning switch-ins whose
preceding switch-out left the thread blocked. That is D2's quantity. Assert
`wakes <= fires + 2` and `wakes >= fires`, which is TIGHTER than the old
`2 * fires + 2`. Print the total switch-ins and the preemptions beside it for
the record. A sliced wait (a 10 ms slice) still reads ~100 wakes a second,
because each slice blocks, so the cell keeps the defect it exists to catch.

## Verified (2026-11-17, host 139)

On issue 2608's witness split (`40fb495e38`), which moves the measurement
behind `witness_board.h`. The wake classification lives in
`witness_board_zephyr.c` (`witness_board_wakes()`), and the host probe's
`witness_board_host.c` answers 0. The build ran under the metadata probe's
refuse rule, so the host twin compiles.
`NROS_ZEPHYR_FIXTURE_FILTER='^build-cortex-m-c-idle-wake-(xrce|zenoh)$' just zephyr build-fixtures`,
then `cargo nextest run -p nros-tests --test zephyr_cortex_m_qemu -- idle_`:

| cell | wakes | switch-ins | fires | result |
|---|---|---|---|---|
| XRCE | 5 | 15 | 5 | pass |
| zenoh | 5 | 10 | 5 | pass |

Negative control: the park was capped at 10 ms in
`nros_platform_wake_park_handles_us`, which is the sliced wait this cell exists
to catch. The XRCE cell then read `wakes=458` (920 switch-ins) and failed with
"woke 458 times over 5 timer periods". The cap was reverted and the leaf
rebuilt. The same three results held at `1037efabd0`, before the split
(5 / 14, 5 / 10, 457).

## Plan (as filed, before the measurement)

Measure before choosing. Instrument the witness, without changing the
executor, to record per switch-in during the window:

- the uptime;
- whether the previous switch-out left the thread BLOCKED or still READY
  (pending vs preempted, from the `switched_out` hook);
- the LAN9118 RX interrupt times, which the witness already stamps.

For each park, also record which source ended it (handle, eventfd, timeout).
Then:

- If the extra switch-ins are preemptions (2): the count is not a wake count.
  Fix the witness to count blocked→running transitions only, and state the
  measured split in RFC-0113's evidence table. Leave the bound alone.
- If they are wakes on Agent traffic that carries no owned data (1): this is a
  D2.3 violation in the XRCE wait. Fix it in the backend or the park, or the
  RFC must state XRCE's session protocol as a measured, named exception.
- If they are park loops (3): fix the park.

The leaf needs issue 2608's witness split (PR #4123) to build on main, so
this work is based on it.
